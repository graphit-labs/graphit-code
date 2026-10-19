use graphit_core::{
    brand::Brand, file_io::SnapshotContext, file_lock::Cancellation, module_settings::Module,
};
use graphit_runtime::{
    instructions::{
        CapturedInstruction, InstructionErrorKind, InstructionLimit, InstructionLimits,
        InstructionOrigin, InstructionValue, InstructionVariable, RenderedInstruction,
        render_instruction,
    },
    skill_instructions::{SkillTemplateSet, skill_templates},
};
use serde_json::{Map, Value};
use std::{
    collections::BTreeSet,
    io,
    num::{NonZeroU64, NonZeroUsize},
    time::{Duration, Instant},
};

const MODULES: [Module; 5] = [
    Module::Ast,
    Module::Hub,
    Module::Task,
    Module::Memory,
    Module::Knowledge,
];
fn request() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> InstructionLimits {
    InstructionLimits {
        max_templates: NonZeroUsize::new(32).unwrap(),
        max_template_bytes: NonZeroUsize::new(262144).unwrap(),
        max_values: NonZeroUsize::new(256).unwrap(),
        max_variable_bytes: NonZeroUsize::new(65536).unwrap(),
        max_output_bytes: NonZeroUsize::new(65536).unwrap(),
        fuel: NonZeroU64::new(100000).unwrap(),
        recursion: NonZeroUsize::new(32).unwrap(),
    }
}
fn data(brand: &str) -> Map<String, Value> {
    Brand::new(brand).unwrap();
    let values: Value =
        serde_json::from_str(include_str!("fixtures/skill-instructions/variables.json")).unwrap();
    values[brand].as_object().unwrap().clone()
}
fn variables(data: &Map<String, Value>) -> Vec<InstructionVariable<'_>> {
    data.iter()
        .map(|(name, value)| InstructionVariable {
            name,
            value: InstructionValue::Text(value.as_str().unwrap()),
        })
        .collect()
}
fn render(
    entry: &str,
    set: SkillTemplateSet,
    data: &Map<String, Value>,
    original: &SnapshotContext,
) -> RenderedInstruction {
    render_instruction(
        entry,
        set.candidates(),
        &variables(data),
        limits(),
        original,
    )
    .unwrap()
}
fn links(text: &str) -> BTreeSet<&str> {
    text.split("](")
        .skip(1)
        .filter_map(|rest| rest.split_once(')').map(|(path, _)| path))
        .collect()
}

#[test]
fn complete_fixtures_cover_five_bodies_ten_references_and_five_descriptions() {
    let expected: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/skill-instructions/catalog.json")).unwrap();
    let names: BTreeSet<_> = expected
        .iter()
        .map(|e| e["entry"].as_str().unwrap())
        .collect();
    assert_eq!(expected.len(), 20);
    assert_eq!(names.len(), expected.len());
    let required: BTreeSet<_> = MODULES
        .iter()
        .flat_map(|m| skill_templates(*m).unwrap().candidates())
        .filter(|s| s.name.starts_with("skills/"))
        .map(|s| s.name)
        .collect();
    assert_eq!(names, required);
    for brand in ["graphit", "orbit"] {
        let data = data(brand);
        let original = request();
        for fixture in &expected {
            let entry = fixture["entry"].as_str().unwrap();
            let set = MODULES
                .iter()
                .map(|m| skill_templates(*m).unwrap())
                .find(|s| s.candidates().iter().any(|c| c.name == entry))
                .unwrap();
            assert!(
                set.candidates()
                    .iter()
                    .all(|c| c.origin == InstructionOrigin::CompiledDefault)
            );
            let result = render(entry, set, &data, &original);
            assert_eq!(
                result.body(),
                fixture[brand].as_str().unwrap(),
                "{brand} {entry}"
            );
            assert_eq!(result.origin(), InstructionOrigin::CompiledDefault);
            assert!(!result.body().contains("TOOL_"));
            assert!(!result.body().contains("graphit_task_"));
        }
        let knowledge = skill_templates(Module::Knowledge).unwrap();
        let body = render(knowledge.body_entry(), knowledge, &data, &original).into_body();
        assert!(body.contains(if brand == "graphit" {
            "`docs/`"
        } else {
            "`handbook/`"
        }));
        assert!(body.contains(if brand == "graphit" {
            "catalog_s_g__wiki_source"
        } else {
            "catalog_s_o__wiki_source"
        }));
        assert!(!body.contains(&format!("{brand}_wiki_source")));
    }
}

#[test]
fn logical_links_and_metadata_preserve_all_companions_and_current_role_boundaries() {
    let data = data("orbit");
    let original = request();
    let mut count = 0;
    for module in MODULES {
        let set = skill_templates(module).unwrap();
        assert!(!set.description().is_empty());
        assert!(!set.description().ends_with('\n'));
        assert!(set.description().chars().count() <= 1024);
        let description = set
            .candidates()
            .iter()
            .find(|s| s.name.ends_with("/description.md"))
            .unwrap();
        assert_eq!(
            set.description(),
            description.markdown.trim_end_matches('\n')
        );
        assert!(!set.description().contains("{{"));
        let body = render(set.body_entry(), set, &data, &original).into_body();
        assert!(body.contains("Never persist personal or sensitive user/organization data"));
        assert!(body.contains("private user memory"));
        let required: BTreeSet<_> = set.references().iter().map(|r| r.relative_path).collect();
        let actual: BTreeSet<_> = links(&body)
            .into_iter()
            .filter(|p| p.starts_with("references/"))
            .collect();
        assert_eq!(actual, required, "{module:?}");
        for reference in set.references() {
            count += 1;
            assert!(reference.relative_path.starts_with("references/"));
            assert!(reference.relative_path.ends_with(".md"));
            assert!(!reference.relative_path.contains(".."));
            assert_eq!(
                reference.entry,
                format!("skills/{}/{}", module.name(), reference.relative_path)
            );
            let text = render(reference.entry, set, &data, &original).into_body();
            if module == Module::Task {
                for link in links(&text)
                    .into_iter()
                    .filter(|p| p.ends_with(".md") && !p.contains('/'))
                {
                    assert!(
                        required.contains(format!("references/{link}").as_str()),
                        "unresolved companion {link}"
                    );
                }
                assert!(text.contains("The coordinator owns acceptance"));
                assert!(!text.contains("Workers claim only their Task"));
                assert!(!text.contains("each worker has its own Task claim"));
                assert!(!text.contains("claims only U"));
                assert!(!text.contains("A worker releases only its own Task"));
            }
        }
    }
    assert_eq!(count, 10);
    let task = skill_templates(Module::Task).unwrap();
    let body = render(task.body_entry(), task, &data, &original).into_body();
    assert!(body.contains("the coordinator owns Task claims"));
    assert!(!body.contains("workers claim their Tasks"));
    let execution = task
        .references()
        .iter()
        .find(|r| r.relative_path == "references/execution.md")
        .unwrap();
    let body = render(execution.entry, task, &data, &original).into_body();
    assert!(body.contains("catalog_s_o__task_revise"));
    assert!(body.contains("catalog_s_o__task_progress"));
    assert!(body.contains("coordinator claims executable Tasks"));
}

#[test]
fn role_ast_transport_templates_keep_root_default_and_closed_role_payloads() {
    let set = skill_templates(Module::Ast).unwrap();
    for brand in ["graphit", "orbit"] {
        let original = request();
        let root = data(brand);
        let mut omitted = root.clone();
        omitted.remove("tool_transport");
        let mut role = root.clone();
        role.insert("tool_transport".into(), Value::String("role".into()));
        for (entry, count) in [(set.body_entry(), 4), (set.references()[0].entry, 2)] {
            assert_eq!(
                render(entry, set, &root, &original).body(),
                render(entry, set, &omitted, &original).body(),
                "Root remains the default without a transport input"
            );
            let rendered = render(entry, set, &role, &original).into_body();
            assert!(rendered.contains(role["mcp_tool_ast_query"].as_str().unwrap()));
            assert!(!rendered.contains("\"ai_optimized\""));
            assert!(!rendered.contains("\"top_k\""));
            let payloads: Vec<Value> = rendered
                .split("```json\n")
                .skip(1)
                .map(|body| serde_json::from_str(body.split_once("```").unwrap().0).unwrap())
                .collect();
            assert_eq!(payloads.len(), count);
            for payload in payloads {
                assert_eq!(payload.as_object().unwrap().len(), 1);
                assert!(payload["input"].is_object());
            }
        }
    }
}

#[test]
fn role_memory_knowledge_templates_keep_root_default_and_closed_payloads() {
    for brand in ["graphit", "orbit"] {
        for (module, companions) in [
            (Module::Memory, &[("references/durable-context.md", 3)][..]),
            (
                Module::Knowledge,
                &[
                    ("references/documentation-design.md", 3),
                    ("references/worked-domain.md", 2),
                ][..],
            ),
        ] {
            let set = skill_templates(module).unwrap();
            let original = request();
            let root = data(brand);
            let mut omitted = root.clone();
            omitted.remove("tool_transport");
            let mut role = root.clone();
            role.insert("tool_transport".into(), Value::String("role".into()));
            role.insert(
                "mcp_tool_knowledge_references".into(),
                Value::String(format!("catalog_role_{brand}__knowledge_references")),
            );
            let mut entries = vec![(
                set.body_entry(),
                if module == Module::Knowledge { 5 } else { 4 },
            )];
            for (path, count) in companions {
                entries.push((
                    set.references()
                        .iter()
                        .find(|entry| entry.relative_path == *path)
                        .unwrap()
                        .entry,
                    *count,
                ));
            }
            for (entry, expected_count) in entries {
                assert_eq!(
                    render(entry, set, &root, &original).body(),
                    render(entry, set, &omitted, &original).body()
                );
                let rendered = render(entry, set, &role, &original).into_body();
                let payloads: Vec<Value> = rendered
                    .split("```json\n")
                    .skip(1)
                    .map(|body| serde_json::from_str(body.split_once("```").unwrap().0).unwrap())
                    .collect();
                assert_eq!(payloads.len(), expected_count);
                for payload in payloads {
                    assert_eq!(payload.as_object().unwrap().len(), 1);
                    let input = payload["input"].as_object().unwrap();
                    for forbidden in [
                        "project_dir",
                        "project_version",
                        "scope",
                        "context",
                        "wikis",
                        "hub_refs",
                        "ai_optimized",
                    ] {
                        assert!(!input.contains_key(forbidden), "{entry}: {forbidden}");
                    }
                    if module == Module::Memory {
                        assert!(!input.contains_key("query"));
                        assert!(!input.contains_key("exclude_mandatory"));
                    } else {
                        assert!(!input.contains_key("path"));
                    }
                }
            }
        }
    }
}

#[test]
fn seven_other_modules_are_explicit_errors_not_empty_skills() {
    for module in [
        Module::Daemon,
        Module::Sync,
        Module::Embedding,
        Module::Hooks,
        Module::Agent,
        Module::Dream,
        Module::DaemonUi,
    ] {
        let error = skill_templates(module).err().unwrap();
        assert_eq!(error.module(), module);
        assert_eq!(
            error.to_string(),
            format!("module {} has no compiled skill", module.name())
        );
    }
}

#[test]
fn required_docs_root_duplicates_and_selected_reference_syntax_fail_closed() {
    let set = skill_templates(Module::Knowledge).unwrap();
    let mut data = data("graphit");
    data.remove("docs_dir");
    assert_eq!(
        render_instruction(
            set.body_entry(),
            set.candidates(),
            &variables(&data),
            limits(),
            &request()
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Template(minijinja::ErrorKind::UndefinedError)
    );
    data.insert("docs_dir".into(), "handbook".into());
    let mut candidates: Vec<_> = set
        .candidates()
        .iter()
        .map(|c| CapturedInstruction {
            name: c.name,
            origin: c.origin,
            markdown: c.markdown,
        })
        .collect();
    let reference = set.references()[0].entry;
    candidates.push(CapturedInstruction {
        name: reference,
        origin: InstructionOrigin::Project,
        markdown: "{{",
    });
    // All selected registered templates are validated, including a companion not included by the body.
    assert_eq!(
        render_instruction(
            set.body_entry(),
            &candidates,
            &variables(&data),
            limits(),
            &request()
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Template(minijinja::ErrorKind::SyntaxError)
    );
    candidates.push(CapturedInstruction {
        name: reference,
        origin: InstructionOrigin::Project,
        markdown: "valid",
    });
    assert_eq!(
        render_instruction(
            set.body_entry(),
            &candidates,
            &variables(&data),
            limits(),
            &request()
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::DuplicateTemplate
    );
    let mut vars = variables(&data);
    vars.push(InstructionVariable {
        name: "docs_dir",
        value: InstructionValue::Text("docs"),
    });
    assert_eq!(
        render_instruction(
            set.body_entry(),
            set.candidates(),
            &vars,
            limits(),
            &request()
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::DuplicateVariable
    );
    let wrapper = format!(
        "Before\n{{% include \"defaults/{}\" %}}\nAfter\n",
        set.body_entry()
    );
    candidates.truncate(set.candidates().len());
    candidates.push(CapturedInstruction {
        name: set.body_entry(),
        origin: InstructionOrigin::Project,
        markdown: &wrapper,
    });
    let default = render(set.body_entry(), set, &data, &request()).into_body();
    let result = render_instruction(
        set.body_entry(),
        &candidates,
        &variables(&data),
        limits(),
        &request(),
    )
    .unwrap();
    assert_eq!(result.body(), format!("Before\n{default}\nAfter\n"));
    assert_eq!(result.origin(), InstructionOrigin::Project);
}

#[test]
fn original_request_and_unicode_limits_protect_real_body_and_reference_outputs() {
    let set = skill_templates(Module::Knowledge).unwrap();
    let mut data = data("orbit");
    data.insert("brand_display".into(), "Órbit 🌍".into());
    data.insert("docs_dir".into(), "handbüch🌍".into());
    let original = request();
    let vars = variables(&data);
    for entry in [set.body_entry(), set.references()[1].entry] {
        let full = render(entry, set, &data, &original);
        let mut bounded = limits();
        bounded.max_output_bytes = NonZeroUsize::new(full.body().len()).unwrap();
        assert_eq!(
            render_instruction(entry, set.candidates(), &vars, bounded, &original)
                .unwrap()
                .body(),
            full.body()
        );
        bounded.max_output_bytes = NonZeroUsize::new(full.body().len() - 1).unwrap();
        assert_eq!(
            render_instruction(entry, set.candidates(), &vars, bounded, &original)
                .unwrap_err()
                .kind(),
            InstructionErrorKind::LimitExceeded(InstructionLimit::OutputBytes)
        );
    }
    let cancellation = Cancellation::default();
    let same = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    render(set.body_entry(), set, &data, &same);
    cancellation.cancel();
    assert_eq!(
        render_instruction(
            set.references()[0].entry,
            set.candidates(),
            &vars,
            limits(),
            &same
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Request(io::ErrorKind::Interrupted)
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        render_instruction(
            set.body_entry(),
            set.candidates(),
            &vars,
            limits(),
            &expired
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Request(io::ErrorKind::TimedOut)
    );
}

#[test]
fn actual_hub_skill_rendering_encodes_only_the_entrypoint_for_the_current_kiro_profile() {
    use graphit_hub::native::{
        ContentLimits, NativeBinding, RenderContext, SkillDescriptor, render_skill,
    };
    let data = data("orbit");
    let original = request();
    let cancellation = Cancellation::default();
    let mut ctx = RenderContext::new(
        ContentLimits {
            max_input_bytes: 524288,
            max_output_bytes: 524288,
            max_depth: 16,
            max_nodes: 2048,
            max_scalar_bytes: 65536,
            max_collection_entries: 64,
        },
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    for module in MODULES {
        let set = skill_templates(module).unwrap();
        let body = render(set.body_entry(), set, &data, &original).into_body();
        let name = format!("installed-orbit-{}", module.name());
        let input = format!(
            "---\nname: {}\ndescription: {}\n---\n{body}",
            serde_json::to_string(&name).unwrap(),
            serde_json::to_string(set.description()).unwrap()
        );
        let descriptor = SkillDescriptor::parse(input.as_bytes(), &mut ctx).unwrap();
        assert_eq!(descriptor.body(), body);
        let binding = NativeBinding::KiroV3;
        let result = render_skill(
            binding,
            &descriptor,
            Some("effective-project-skill"),
            &mut ctx,
        )
        .unwrap();
        assert_eq!(result.provenance.binding, binding);
        assert!(result.relative_entry.ends_with("/SKILL.md"));
        let text = std::str::from_utf8(&result.bytes).unwrap();
        let (header, native_body) = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("---\n")
            .unwrap();
        let native = SkillDescriptor::parse(&result.bytes, &mut ctx).unwrap();
        assert_eq!(native.name(), "effective-project-skill");
        assert_eq!(native.body(), body);
        assert!(header.contains(set.description()));
        let roundtrip = render_skill(binding, &native, None, &mut ctx).unwrap();
        assert_eq!(roundtrip.bytes, result.bytes);
        assert_eq!(native_body, body);
        assert!(!header.contains("references:"));
    }
}
