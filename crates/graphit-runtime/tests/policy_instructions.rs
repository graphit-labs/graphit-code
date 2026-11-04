use graphit_core::module_settings::Module;
use graphit_core::{brand::Brand, file_io::SnapshotContext, file_lock::Cancellation};
use graphit_runtime::{
    instructions::{
        CapturedInstruction, InstructionErrorKind, InstructionLimit, InstructionLimits,
        InstructionOrigin, InstructionValue, InstructionVariable, RenderedInstruction,
        render_instruction,
    },
    policy_instructions::{
        MandateAudience, PolicyPrompt, PolicyTemplateSet, module_mandate, policy_prompt,
    },
};
use serde_json::{Map, Value};
use std::{
    collections::BTreeSet,
    io,
    num::{NonZeroU64, NonZeroUsize},
    time::{Duration, Instant},
};

fn request() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> InstructionLimits {
    InstructionLimits {
        max_templates: NonZeroUsize::new(32).unwrap(),
        max_template_bytes: NonZeroUsize::new(65536).unwrap(),
        max_values: NonZeroUsize::new(128).unwrap(),
        max_variable_bytes: NonZeroUsize::new(16384).unwrap(),
        max_output_bytes: NonZeroUsize::new(65536).unwrap(),
        fuel: NonZeroU64::new(100000).unwrap(),
        recursion: NonZeroUsize::new(32).unwrap(),
    }
}
fn data(brand: &str) -> Map<String, Value> {
    let values: Value =
        serde_json::from_str(include_str!("fixtures/policy-instructions/variables.json")).unwrap();
    Brand::new(brand).unwrap();
    values[brand].as_object().unwrap().clone()
}
fn variables<'a>(
    data: &'a Map<String, Value>,
    modules: &'a [&'a str],
) -> Vec<InstructionVariable<'a>> {
    data.iter()
        .map(|(name, value)| InstructionVariable {
            name,
            value: if name == "module_mandates" {
                InstructionValue::TextList(modules)
            } else {
                InstructionValue::Text(value.as_str().unwrap())
            },
        })
        .collect()
}
fn fixture_modules(data: &Map<String, Value>) -> Vec<&str> {
    data["module_mandates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}
fn render(set: PolicyTemplateSet, data: &Map<String, Value>) -> RenderedInstruction {
    render_with_request(set, data, &request())
}
fn render_with_request(
    set: PolicyTemplateSet,
    data: &Map<String, Value>,
    original: &SnapshotContext,
) -> RenderedInstruction {
    let modules = fixture_modules(data);
    render_instruction(
        set.entry(),
        set.candidates(),
        &variables(data, &modules),
        limits(),
        original,
    )
    .unwrap()
}
fn catalog() -> Vec<PolicyTemplateSet> {
    let mut sets = vec![module_mandate(Module::Task, MandateAudience::Coordinator).unwrap()];
    for module in [
        Module::Task,
        Module::Memory,
        Module::Ast,
        Module::Hub,
        Module::Knowledge,
    ] {
        sets.push(module_mandate(module, MandateAudience::Delegate).unwrap());
    }
    for prompt in [
        PolicyPrompt::RouterPreamble,
        PolicyPrompt::ModuleWrapper,
        PolicyPrompt::Router,
        PolicyPrompt::CoreInvariant,
        PolicyPrompt::CursorCompensation,
        PolicyPrompt::AntigravityCompensation,
        PolicyPrompt::DocumentationConsistencyReminder,
        PolicyPrompt::RoutingFallback,
        PolicyPrompt::BootstrapHeader,
        PolicyPrompt::ProjectVersionNotice,
        PolicyPrompt::BootstrapMemoryFetch,
        PolicyPrompt::BootstrapMemoryEmpty,
        PolicyPrompt::BootstrapMemoryLoadedNotice,
        PolicyPrompt::BootstrapMemoryRecall,
        PolicyPrompt::BootstrapTaskSession,
        PolicyPrompt::BootstrapTaskLifecycle,
        PolicyPrompt::BootstrapTaskRecall,
        PolicyPrompt::BootstrapTaskPlanning,
        PolicyPrompt::WorkerMemoryFetch,
        PolicyPrompt::WorkerMemoryLoadedNotice,
        PolicyPrompt::WorkerMemoryRecall,
        PolicyPrompt::WorkerTaskRecall,
        PolicyPrompt::WorkerDelegatedContract,
        PolicyPrompt::WorkerDeliveryValidation,
        PolicyPrompt::WorkerProtocolHeader,
    ] {
        sets.push(policy_prompt(prompt));
    }
    sets
}

#[test]
fn complete_normative_fixtures_cover_both_brands_and_effective_catalog_aliases() {
    let expected: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/policy-instructions/catalog.json")).unwrap();
    assert_eq!(expected.len(), 37);
    let sets = catalog();
    assert_eq!(sets.len(), 31);
    let names: BTreeSet<_> = expected
        .iter()
        .map(|f| f["entry"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), expected.len(), "fixture names must be unique");
    let required: BTreeSet<_> = sets
        .iter()
        .map(PolicyTemplateSet::entry)
        .chain(
            sets[0]
                .candidates()
                .iter()
                .filter(|s| s.name.starts_with("shared/"))
                .map(|s| s.name),
        )
        .collect();
    assert_eq!(
        names, required,
        "every selection and shared policy needs its full fixture"
    );
    for brand in ["graphit", "orbit"] {
        let data = data(brand);
        let modules = fixture_modules(&data);
        let vars = variables(&data, &modules);
        for fixture in &expected {
            let name = fixture["entry"].as_str().unwrap();
            let candidates = sets
                .iter()
                .find(|s| s.entry() == name)
                .unwrap_or_else(|| {
                    assert!(name.starts_with("shared/"));
                    &sets[0]
                })
                .candidates();
            assert!(
                candidates
                    .iter()
                    .all(|c| c.origin == InstructionOrigin::CompiledDefault)
            );
            let actual = render_instruction(name, candidates, &vars, limits(), &request()).unwrap();
            assert_eq!(
                actual.body(),
                fixture[brand].as_str().unwrap(),
                "{brand}: {name}"
            );
            assert_eq!(actual.origin(), InstructionOrigin::CompiledDefault);
            assert!(
                actual
                    .selections()
                    .iter()
                    .all(|s| s.origin() == InstructionOrigin::CompiledDefault)
            );
        }
        let task = render(sets[0], &data).into_body();
        assert!(task.contains(&format!(
            "catalog_{}__task_get",
            if brand == "graphit" { "g" } else { "o" }
        )));
        assert!(!task.contains(&format!("{brand}_task_get")));
        assert!(task.contains(&format!("installed-{brand}-task")));
    }
}

#[test]
fn project_hook_prompt_is_compiled_unique_and_scope_honest_for_each_state() {
    let set = policy_prompt(PolicyPrompt::ProjectHookContext);
    let names: BTreeSet<_> = set.candidates().iter().map(|c| c.name).collect();
    assert_eq!(names.len(), set.candidates().len());
    for brand in ["graphit", "orbit"] {
        for state in ["disabled", "no_corpus", "empty", "loaded"] {
            let mut values = data(brand);
            values.insert(
                "user_memory_state".into(),
                Value::String("unselected".into()),
            );
            values.insert("user_memory_json".into(), Value::String(String::new()));
            values.insert("project_memory_state".into(), Value::String(state.into()));
            values.insert(
                "project_task_state".into(),
                Value::String("unselected".into()),
            );
            values.insert("project_task_json".into(), Value::String(String::new()));
            values.insert(
                "project_memory_json".into(),
                Value::String("[{\"body\":\"{{ literal }} ação\"}]".into()),
            );
            let body = render(set, &values).into_body();
            assert!(body.contains(&format!("PROJECT Memory state: {state}.")));
            assert!(body.contains("USER Memory was not loaded by this hook."));
            assert!(body.contains("Task sessions and claims were not restored or changed"));
            assert!(body.contains("{{ literal }} ação"));
            assert_eq!(
                body.contains("complete mandatory PROJECT Memory selection"),
                state == "loaded"
            );
            assert_eq!(
                body.contains("no current mandatory records"),
                state == "empty"
            );
            assert_eq!(
                body.contains("No PROJECT Memory corpus is published"),
                state == "no_corpus"
            );
        }
    }
}

#[test]
fn audiences_select_only_task_policy_and_all_seven_missing_modules_are_errors() {
    assert_ne!(
        module_mandate(Module::Task, MandateAudience::Delegate)
            .unwrap()
            .entry(),
        module_mandate(Module::Task, MandateAudience::Coordinator)
            .unwrap()
            .entry()
    );
    for (module, tag) in [
        (Module::Memory, "mem_rule"),
        (Module::Ast, "ast_rule"),
        (Module::Hub, "hub_rule"),
        (Module::Knowledge, "doc_rule"),
    ] {
        let coordinator = module_mandate(module, MandateAudience::Coordinator).unwrap();
        let delegate = module_mandate(module, MandateAudience::Delegate).unwrap();
        assert_eq!(coordinator.entry(), delegate.entry());
        assert_eq!(delegate.module_tag(), Some(tag));
    }
    for module in [
        Module::Daemon,
        Module::Sync,
        Module::Embedding,
        Module::Hooks,
        Module::Agent,
        Module::Dream,
        Module::DaemonUi,
    ] {
        for audience in [MandateAudience::Coordinator, MandateAudience::Delegate] {
            let error = module_mandate(module, audience).err().unwrap();
            assert_eq!(error.module(), module);
            assert_eq!(
                error.to_string(),
                format!("module {} has no compiled policy mandate", module.name())
            );
        }
    }
}

#[test]
fn delegate_claims_and_live_memory_never_leak_from_old_policy() {
    let data = data("graphit");
    let delegate = render(
        module_mandate(Module::Task, MandateAudience::Delegate).unwrap(),
        &data,
    )
    .into_body();
    assert!(delegate.contains("Do not acquire a claim, including your own Task"));
    assert!(delegate.contains("return a draft instead"));
    let protocol = render(policy_prompt(PolicyPrompt::WorkerDelegatedContract), &data).into_body();
    assert_eq!(
        protocol,
        include_str!("../../../templates/agent-instructions/protocol.md")
    );
    assert!(protocol.contains("The coordinator owns acceptance"));
    for set in catalog() {
        for source in set.candidates() {
            assert!(!source.markdown.contains("Claim at most your own"));
            assert!(!source.markdown.contains("claim only their task"));
            assert!(!source.markdown.contains("context.Mandatory"));
            assert!(!source.markdown.contains("{{ mandatory"));
        }
    }
    let set = policy_prompt(PolicyPrompt::RouterPreamble);
    let boundary = render_instruction(
        "shared/persistence-boundary.md",
        set.candidates(),
        &[],
        limits(),
        &request(),
    )
    .unwrap();
    assert!(boundary.body().contains("private user memory"));
    assert!(
        boundary
            .body()
            .contains("Never persist personal or sensitive")
    );
}

#[test]
fn router_consumes_ordered_supplied_modules_and_preserves_literal_content() {
    let mut data = data("orbit");
    let original = request();
    let mut tagged = Vec::new();
    for module in [
        Module::Task,
        Module::Memory,
        Module::Ast,
        Module::Hub,
        Module::Knowledge,
    ] {
        let set = module_mandate(module, MandateAudience::Delegate).unwrap();
        let body = render_with_request(set, &data, &original).into_body();
        data.insert("module_tag".into(), set.module_tag().unwrap().into());
        data.insert("module_content".into(), body.into());
        tagged.push(
            render_with_request(policy_prompt(PolicyPrompt::ModuleWrapper), &data, &original)
                .into_body(),
        );
    }
    let router = policy_prompt(PolicyPrompt::Router);
    for count in [0, 1, 5] {
        let modules: Vec<_> = tagged[..count].iter().map(String::as_str).collect();
        let output = render_instruction(
            router.entry(),
            router.candidates(),
            &variables(&data, &modules),
            limits(),
            &original,
        )
        .unwrap();
        if count == 0 {
            assert_eq!(output.body(), "");
        } else {
            assert!(output.body().starts_with("<ORBIT_SYSTEM_MANDATE>\n"));
            assert!(output.body().ends_with("\n</ORBIT_SYSTEM_MANDATE>"));
            assert!(output.body().contains(&modules.join("\n")));
        }
    }
    data.insert("module_tag".into(), "task_rule".into());
    data.insert(
        "module_content".into(),
        "{{ unknown }} ${var} ação<&>".into(),
    );
    assert_eq!(
        render(policy_prompt(PolicyPrompt::ModuleWrapper), &data).body(),
        "<task_rule>{{ unknown }} ${var} ação<&></task_rule>"
    );
    let header = policy_prompt(PolicyPrompt::WorkerProtocolHeader);
    data.insert("role_name".into(), "".into());
    assert_eq!(render(header, &data).body(), "ORBIT_PROTOCOL_1\n");
    data.remove("role_name");
    let modules = fixture_modules(&data);
    assert_eq!(
        render_instruction(
            header.entry(),
            header.candidates(),
            &variables(&data, &modules),
            limits(),
            &original
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Template(minijinja::ErrorKind::UndefinedError)
    );
}

#[test]
fn overrides_wrap_defaults_and_selected_invalid_inputs_fail_without_fallback() {
    let set = module_mandate(Module::Knowledge, MandateAudience::Coordinator).unwrap();
    let data = data("graphit");
    let modules = fixture_modules(&data);
    let vars = variables(&data, &modules);
    let default = render(set, &data).into_body();
    let wrapper = format!(
        "Project before\n{{% include \"defaults/{}\" %}}\nProject after\n",
        set.entry()
    );
    let mut candidates: Vec<_> = set
        .candidates()
        .iter()
        .map(|s| CapturedInstruction {
            name: s.name,
            origin: s.origin,
            markdown: s.markdown,
        })
        .collect();
    candidates.push(CapturedInstruction {
        name: set.entry(),
        origin: InstructionOrigin::Project,
        markdown: &wrapper,
    });
    let wrapped =
        render_instruction(set.entry(), &candidates, &vars, limits(), &request()).unwrap();
    assert_eq!(
        wrapped.body(),
        format!("Project before\n{default}\nProject after\n")
    );
    assert_eq!(wrapped.origin(), InstructionOrigin::Project);
    candidates.push(CapturedInstruction {
        name: set.entry(),
        origin: InstructionOrigin::Global,
        markdown: "{{",
    });
    assert_eq!(
        render_instruction(set.entry(), &candidates, &vars, limits(), &request())
            .unwrap()
            .body(),
        wrapped.body()
    );
    candidates.last_mut().unwrap().origin = InstructionOrigin::Project;
    assert_eq!(
        render_instruction(set.entry(), &candidates, &vars, limits(), &request())
            .unwrap_err()
            .kind(),
        InstructionErrorKind::DuplicateTemplate
    );
    candidates.pop();
    candidates.last_mut().unwrap().markdown = "{{";
    assert_eq!(
        render_instruction(set.entry(), &candidates, &vars, limits(), &request())
            .unwrap_err()
            .kind(),
        InstructionErrorKind::Template(minijinja::ErrorKind::SyntaxError)
    );
    let mut missing = variables(&data, &modules);
    missing.retain(|v| v.name != "brand_display");
    assert_eq!(
        render_instruction(
            set.entry(),
            set.candidates(),
            &missing,
            limits(),
            &request()
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Template(minijinja::ErrorKind::UndefinedError)
    );
    missing.push(InstructionVariable {
        name: "brand_display",
        value: InstructionValue::Text("Graphit"),
    });
    missing.push(InstructionVariable {
        name: "brand_display",
        value: InstructionValue::Text("Orbit"),
    });
    assert_eq!(
        render_instruction(
            set.entry(),
            set.candidates(),
            &missing,
            limits(),
            &request()
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::DuplicateVariable
    );
}

#[test]
fn original_cancellation_deadline_and_real_utf8_byte_limits_are_enforced() {
    let set = policy_prompt(PolicyPrompt::BootstrapHeader);
    let mut data = data("orbit");
    data.insert("brand_display".into(), "Órbit 🌍".into());
    let modules = fixture_modules(&data);
    let vars = variables(&data, &modules);
    let original = request();
    let full =
        render_instruction(set.entry(), set.candidates(), &vars, limits(), &original).unwrap();
    let mut bounded = limits();
    bounded.max_output_bytes = NonZeroUsize::new(full.body().len()).unwrap();
    assert_eq!(
        render_instruction(set.entry(), set.candidates(), &vars, bounded, &original)
            .unwrap()
            .body(),
        full.body()
    );
    bounded.max_output_bytes = NonZeroUsize::new(full.body().len() - 1).unwrap();
    assert_eq!(
        render_instruction(set.entry(), set.candidates(), &vars, bounded, &original)
            .unwrap_err()
            .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::OutputBytes)
    );
    bounded = limits();
    bounded.max_templates = NonZeroUsize::new(set.candidates().len() - 1).unwrap();
    assert_eq!(
        render_instruction(set.entry(), set.candidates(), &vars, bounded, &original)
            .unwrap_err()
            .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::Templates)
    );
    let cancellation = Cancellation::default();
    let cancel_request = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    cancellation.cancel();
    assert_eq!(
        render_instruction(
            set.entry(),
            set.candidates(),
            &vars,
            limits(),
            &cancel_request
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Request(io::ErrorKind::Interrupted)
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        render_instruction(set.entry(), set.candidates(), &vars, limits(), &expired)
            .unwrap_err()
            .kind(),
        InstructionErrorKind::Request(io::ErrorKind::TimedOut)
    );
}

#[test]
fn actual_kiro_bindings_encode_policy_body_and_explicit_native_grants() {
    use graphit_hub::native::{
        AgentDescriptor, ContentLimits, NativeBinding, NativeToolGrant, RenderContext, ToolAccess,
        ToolGrants, render_agent,
    };
    let body = render(
        module_mandate(Module::Task, MandateAudience::Delegate).unwrap(),
        &data("orbit"),
    )
    .into_body();
    let cancellation = Cancellation::default();
    let mut ctx = RenderContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 16,
            max_nodes: 1024,
            max_scalar_bytes: 8192,
            max_collection_entries: 64,
        },
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let descriptor = AgentDescriptor::new(
        "reviewer",
        "Inspect assigned sources",
        &body,
        true,
        false,
        &mut ctx,
    )
    .unwrap();
    {
        let (binding, tool) = (NativeBinding::KiroV3, "read");
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: false,
            tools: vec![NativeToolGrant {
                name: tool.into(),
                access: ToolAccess::Read,
            }],
        };
        let output = render_agent(binding, &descriptor, None, &grants, &mut ctx).unwrap();
        assert_eq!(output.provenance.binding, binding);
        let text = std::str::from_utf8(&output.bytes).unwrap();

        let (header, native_body) = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("---\n")
            .unwrap();
        assert_eq!(native_body, body);
        assert!(header.contains("includeMcpJson: false"));
        assert!(header.contains("includePowers: false"));
        assert!(output.relative_entry.ends_with(".md"));
    }
}

#[test]
fn explicit_task_context_is_observational_and_disabled_never_claims_loaded() {
    let set = policy_prompt(PolicyPrompt::ProjectHookContext);
    for state in ["disabled", "loaded"] {
        let mut values = data("graphit");
        values.insert(
            "user_memory_state".into(),
            Value::String("unselected".into()),
        );
        values.insert("user_memory_json".into(), Value::String(String::new()));
        values.insert(
            "project_memory_state".into(),
            Value::String("disabled".into()),
        );
        values.insert("project_memory_json".into(), Value::String("[]".into()));
        values.insert("project_task_state".into(), Value::String(state.into()));
        values.insert(
            "project_task_json".into(),
            Value::String(r#"{"session":{"strategy":"{{ literal }} ação"},"tasks":[]}"#.into()),
        );
        let body = render(set, &values).into_body();
        assert!(body.contains(&format!("PROJECT Task state: {state}.")));
        assert!(body.contains("Task recovery below is observational."));
        assert!(
            body.contains("No session or task claim was acquired, renewed, released or changed.")
        );
        assert!(!body.contains("Task sessions and claims were not restored"));
        assert_eq!(
            body.contains("Selected PROJECT Task JSON (data):"),
            state == "loaded"
        );
        assert_eq!(body.contains("{{ literal }} ação"), state == "loaded");
        assert_eq!(body.contains("no Task store was read"), state == "disabled");
    }
}

#[test]
fn source_user_context_states_only_render_admitted_loaded_data() {
    let set = policy_prompt(PolicyPrompt::ProjectHookContext);
    for state in ["disabled", "unavailable", "no_corpus", "empty", "loaded"] {
        let mut values = data("graphit");
        for (name, value) in [
            ("project_memory_state", "disabled"),
            ("project_memory_json", "[]"),
            ("project_task_state", "unselected"),
            ("project_task_json", ""),
            ("user_memory_state", state),
            ("user_memory_json", r#"[{"body":"{{ literal }} privado"}]"#),
        ] {
            values.insert(name.into(), Value::String(value.into()));
        }
        let body = render(set, &values).into_body();
        assert!(body.contains(&format!("USER Memory state: {state}.")));
        assert_eq!(
            body.contains("Mandatory USER Memory JSON (data):"),
            state == "loaded"
        );
        assert_eq!(body.contains("{{ literal }} privado"), state == "loaded");
        assert_eq!(
            body.contains("complete mandatory USER Memory selection"),
            state == "loaded"
        );
        assert_eq!(
            body.contains("USER Memory was not loaded by this hook."),
            matches!(state, "disabled" | "unavailable")
        );
        assert_eq!(
            body.contains("No USER Memory corpus is published"),
            state == "no_corpus"
        );
        assert_eq!(
            body.contains("published USER Memory selection has no current mandatory records"),
            state == "empty"
        );
    }
}

#[test]
fn user_consolidation_prompt_is_compiled_literal_and_independent_of_project_or_role() {
    let set = policy_prompt(PolicyPrompt::UserDreamConsolidation);
    let corpus = r#"[{"id":"synthetic","revision":1,"body":"{{ literal }} ação"}]"#;
    let variables = [
        InstructionVariable {
            name: "dream_session_id",
            value: InstructionValue::Text("synthetic-attempt"),
        },
        InstructionVariable {
            name: "memories_json",
            value: InstructionValue::Text(corpus),
        },
    ];
    let body = render_instruction(
        set.entry(),
        set.candidates(),
        &variables,
        limits(),
        &request(),
    )
    .unwrap()
    .into_body();
    assert!(body.starts_with("# Coordinator — USER Memory consolidation proposal"));
    assert!(body.contains(corpus));
    assert!(!body.contains("\nProject:") && !body.contains("\nVersion:"));
    assert!(body.contains("Do not read PROJECT Memory, other users"));
    assert!(body.contains("The Runtime owns operation keys"));
    let mut bounded = limits();
    bounded.max_variable_bytes = NonZeroUsize::new(corpus.len() - 1).unwrap();
    assert_eq!(
        render_instruction(
            set.entry(),
            set.candidates(),
            &variables,
            bounded,
            &request()
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::VariableBytes),
    );
}
