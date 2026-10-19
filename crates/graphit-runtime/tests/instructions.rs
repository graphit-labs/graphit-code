use graphit_core::{brand::Brand, file_io::SnapshotContext, file_lock::Cancellation};
use graphit_runtime::{ManagedRole, compose_role, instructions::*};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::{Duration, Instant},
};
fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn request() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> InstructionLimits {
    InstructionLimits {
        max_templates: n(32),
        max_template_bytes: n(65536),
        max_values: n(64),
        max_variable_bytes: n(65536),
        max_output_bytes: n(65536),
        fuel: NonZeroU64::new(100_000).unwrap(),
        recursion: n(32),
    }
}
fn template<'a>(
    name: &'a str,
    origin: InstructionOrigin,
    markdown: &'a str,
) -> CapturedInstruction<'a> {
    CapturedInstruction {
        name,
        origin,
        markdown,
    }
}
fn text<'a>(name: &'a str, value: &'a str) -> InstructionVariable<'a> {
    InstructionVariable {
        name,
        value: InstructionValue::Text(value),
    }
}
fn error(result: Result<RenderedInstruction, InstructionError>) -> InstructionError {
    result.expect_err("expected a real rendering failure")
}
#[test]
fn effective_names_artifacts_agents_and_markdown_literals_are_not_hardcoded() {
    for (brand, artifact, agent, tool) in [
        ("graphit", "wiki", "reader", "graphit_knowledge_search"),
        ("acme", "plan", "reviewer", "native_mcp_acme_task_query"),
    ] {
        let source = [template(
            "agent.md",
            InstructionOrigin::CompiledDefault,
            "{{ brand }}/{{ artifact }}/{{ agent }}: {{ mcp_tool_xpto }} <&> {{ literal }} {% raw %}{{filePath}}{% endraw %} ${var}\n{{ enabled }}/{{ revision }}/{{ modules|join(',') }}\n",
        )];
        let modules = ["task", "knowledge"];
        let vars = [
            text("brand", brand),
            text("artifact", artifact),
            text("agent", agent),
            text("mcp_tool_xpto", tool),
            text("literal", "{{ untouched }}"),
            InstructionVariable {
                name: "enabled",
                value: InstructionValue::Bool(true),
            },
            InstructionVariable {
                name: "revision",
                value: InstructionValue::Unsigned(12),
            },
            InstructionVariable {
                name: "modules",
                value: InstructionValue::TextList(&modules),
            },
        ];
        let result = render_instruction("agent.md", &source, &vars, limits(), &request()).unwrap();
        assert_eq!(
            result.body(),
            format!(
                "{brand}/{artifact}/{agent}: {tool} <&> {{{{ untouched }}}} {{{{filePath}}}} ${{var}}\nTrue/12/task,knowledge\n"
            )
        );
        assert_eq!(result.origin(), InstructionOrigin::CompiledDefault);
        assert_eq!(result.selections()[0].name(), "agent.md");
        assert!(!format!("{result:?}").contains(tool));
    }
}
#[test]
fn precedence_is_order_independent_and_default_can_be_wrapped() {
    let source = [
        template("agent.md", InstructionOrigin::Hub, "hub"),
        template(
            "agent.md",
            InstructionOrigin::CompiledDefault,
            "default {{ name }}\n",
        ),
        template("agent.md", InstructionOrigin::Global, "{% broken"),
        template(
            "agent.md",
            InstructionOrigin::Project,
            "before\n{% include 'defaults/agent.md' %}after\n",
        ),
    ];
    let vars = [text("name", "reader")];
    let a = render_instruction("agent.md", &source, &vars, limits(), &request()).unwrap();
    let reverse: Vec<_> = source.into_iter().rev().collect();
    let b = render_instruction("agent.md", &reverse, &vars, limits(), &request()).unwrap();
    assert_eq!(a.body(), "before\ndefault reader\nafter\n");
    assert_eq!(a.body(), b.body());
    assert_eq!(a.origin(), InstructionOrigin::Project);
    for (origin, expected) in [
        (InstructionOrigin::Global, "global"),
        (InstructionOrigin::Hub, "hub"),
    ] {
        let choices = [
            template("agent.md", InstructionOrigin::CompiledDefault, "default"),
            template("agent.md", InstructionOrigin::Hub, "hub"),
            template("agent.md", origin, expected),
        ];
        // Avoid a duplicated Hub origin in that branch.
        let choices = if origin == InstructionOrigin::Hub {
            &choices[..2]
        } else {
            &choices[..]
        };
        assert_eq!(
            render_instruction("agent.md", choices, &[], limits(), &request())
                .unwrap()
                .body(),
            expected
        );
    }
}
#[test]
fn chosen_errors_do_not_fall_back_and_empty_is_explicit() {
    let fallback = template(
        "agent.md",
        InstructionOrigin::CompiledDefault,
        "valid default",
    );
    for bad in ["{% broken", "{{ required }}", "{% include 'missing.md' %}"] {
        let source = [
            template("agent.md", fallback.origin, fallback.markdown),
            template("agent.md", InstructionOrigin::Project, bad),
        ];
        let actual = error(render_instruction(
            "agent.md",
            &source,
            &[],
            limits(),
            &request(),
        ));
        assert!(matches!(actual.kind(), InstructionErrorKind::Template(_)));
        assert!(std::error::Error::source(&actual).is_some());
        assert!(!format!("{actual:?}").contains(bad));
    }
    let source = [
        fallback,
        template("agent.md", InstructionOrigin::Project, ""),
    ];
    assert_eq!(
        render_instruction("agent.md", &source, &[], limits(), &request())
            .unwrap()
            .body(),
        ""
    );
    assert_eq!(
        error(render_instruction(
            "other.md",
            &source,
            &[],
            limits(),
            &request()
        ))
        .kind(),
        InstructionErrorKind::MissingTemplate
    );
}
#[test]
fn optional_sdk_logic_remains_explicit_and_registered_default_syntax_is_checked() {
    let source = [template(
        "agent.md",
        InstructionOrigin::Project,
        "{% if absent is defined %}bad{% endif %}{{ absent|default('optional') }}{% include 'missing.md' ignore missing %}\n",
    )];
    assert_eq!(
        render_instruction("agent.md", &source, &[], limits(), &request())
            .unwrap()
            .body(),
        "optional\n"
    );
    let invalid_default = [
        template("agent.md", InstructionOrigin::Project, "valid"),
        template("agent.md", InstructionOrigin::CompiledDefault, "{% broken"),
    ];
    assert!(matches!(
        error(render_instruction(
            "agent.md",
            &invalid_default,
            &[],
            limits(),
            &request()
        ))
        .kind(),
        InstructionErrorKind::Template(_)
    ));
}
#[test]
fn invalid_names_and_duplicates_fail_without_publishing_partial_output() {
    for bad in [
        "../agent.md",
        "/agent.md",
        "a//agent.md",
        "defaults/agent.md",
        "a\\agent.md",
        "agent.txt",
        "a\n.md",
    ] {
        assert_eq!(
            error(render_instruction(bad, &[], &[], limits(), &request())).kind(),
            InstructionErrorKind::InvalidName
        );
    }
    let source = [
        template("agent.md", InstructionOrigin::Project, "ok"),
        template("agent.md", InstructionOrigin::Project, "second"),
    ];
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &source,
            &[],
            limits(),
            &request()
        ))
        .kind(),
        InstructionErrorKind::DuplicateTemplate
    );
    let source = [template(
        "agent.md",
        InstructionOrigin::Project,
        "{{ value }}",
    )];
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &source,
            &[text("value", "a"), text("value", "b")],
            limits(),
            &request()
        ))
        .kind(),
        InstructionErrorKind::DuplicateVariable
    );
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &source,
            &[text("invalid-name", "a")],
            limits(),
            &request()
        ))
        .kind(),
        InstructionErrorKind::InvalidVariable
    );
}
#[test]
fn admission_uses_utf8_bytes_and_accepts_equality() {
    let source = [template("agent.md", InstructionOrigin::Project, "{{ v }}")];
    let vars = [text("v", "é")];
    let mut exact = limits();
    exact.max_templates = n(1);
    exact.max_template_bytes = n("agent.md".len() * 2 + "{{ v }}".len());
    exact.max_values = n(1);
    exact.max_variable_bytes = n(3);
    exact.max_output_bytes = n(2);
    assert_eq!(
        render_instruction("agent.md", &source, &vars, exact, &request())
            .unwrap()
            .body(),
        "é"
    );
    for (which, expected) in [
        (0, InstructionLimit::TemplateBytes),
        (1, InstructionLimit::VariableBytes),
        (2, InstructionLimit::OutputBytes),
    ] {
        let mut too_small = exact;
        match which {
            0 => too_small.max_template_bytes = n(exact.max_template_bytes.get() - 1),
            1 => too_small.max_variable_bytes = n(2),
            _ => too_small.max_output_bytes = n(1),
        }
        assert_eq!(
            error(render_instruction(
                "agent.md",
                &source,
                &vars,
                too_small,
                &request()
            ))
            .kind(),
            InstructionErrorKind::LimitExceeded(expected)
        );
    }
    let mut entry_cap = limits();
    entry_cap.max_template_bytes = n(8);
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &[],
            &[],
            entry_cap,
            &request()
        ))
        .kind(),
        InstructionErrorKind::MissingTemplate
    );
    entry_cap.max_template_bytes = n(7);
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &[],
            &[],
            entry_cap,
            &request()
        ))
        .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::TemplateBytes)
    );
}
#[test]
fn losers_and_text_list_items_are_admitted_before_sdk_conversion() {
    let source = [
        template("agent.md", InstructionOrigin::Project, "selected"),
        template("agent.md", InstructionOrigin::Global, "ignored"),
    ];
    let mut caps = limits();
    caps.max_templates = n(1);
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &source,
            &[],
            caps,
            &request()
        ))
        .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::Templates)
    );
    caps = limits();
    caps.max_template_bytes = n(8 + 8 + 8);
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &source,
            &[],
            caps,
            &request()
        ))
        .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::TemplateBytes)
    );
    let list = ["a", "é"];
    let vars = [InstructionVariable {
        name: "v",
        value: InstructionValue::TextList(&list),
    }];
    let source = &source[..1];
    caps = limits();
    caps.max_values = n(3);
    caps.max_variable_bytes = n(4);
    assert!(render_instruction("agent.md", source, &vars, caps, &request()).is_ok());
    caps.max_values = n(2);
    assert_eq!(
        error(render_instruction(
            "agent.md",
            source,
            &vars,
            caps,
            &request()
        ))
        .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::Values)
    );
    caps.max_values = n(3);
    caps.max_variable_bytes = n(3);
    assert_eq!(
        error(render_instruction(
            "agent.md",
            source,
            &vars,
            caps,
            &request()
        ))
        .kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::VariableBytes)
    );
}
#[test]
fn sdk_fuel_and_recursion_limits_are_real_errors() {
    let source = [template(
        "loop.md",
        InstructionOrigin::Project,
        "{% for x in range(100000) %}{% if false %}x{% endif %}{% endfor %}",
    )];
    let mut caps = limits();
    caps.fuel = NonZeroU64::new(64).unwrap();
    let e = error(render_instruction(
        "loop.md",
        &source,
        &[],
        caps,
        &request(),
    ));
    assert_eq!(
        e.kind(),
        InstructionErrorKind::Template(minijinja::ErrorKind::OutOfFuel)
    );
    assert!(std::error::Error::source(&e).is_some());
    let recursive = [template(
        "recursive.md",
        InstructionOrigin::Project,
        "{% include 'recursive.md' %}",
    )];
    caps = limits();
    caps.recursion = n(16);
    let e = error(render_instruction(
        "recursive.md",
        &recursive,
        &[],
        caps,
        &request(),
    ));
    let mut cause = std::error::Error::source(&e);
    let mut actual_recursion = false;
    while let Some(error) = cause {
        if let Some(sdk) = error.downcast_ref::<minijinja::Error>() {
            actual_recursion |= sdk.kind() == minijinja::ErrorKind::InvalidOperation
                && sdk
                    .detail()
                    .is_some_and(|detail| detail.contains("recursion"));
        }
        cause = error.source();
    }
    assert!(actual_recursion, "must fail at the SDK recursion boundary");
    caps.recursion = n(501);
    assert_eq!(
        error(render_instruction(
            "recursive.md",
            &recursive,
            &[],
            caps,
            &request()
        ))
        .kind(),
        InstructionErrorKind::InvalidLimits
    );
}
#[test]
fn original_request_cancel_and_expiration_are_not_reset() {
    let cancellation = Cancellation::default();
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    let source = [template("agent.md", InstructionOrigin::Project, "ok")];
    assert!(render_instruction("agent.md", &source, &[], limits(), &original).is_ok());
    cancellation.cancel();
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &source,
            &[],
            limits(),
            &original
        ))
        .kind(),
        InstructionErrorKind::Request(std::io::ErrorKind::Interrupted)
    );
    let deadline = Instant::now() + Duration::from_millis(30);
    let original = SnapshotContext::new(deadline, Cancellation::default());
    assert!(render_instruction("agent.md", &source, &[], limits(), &original).is_ok());
    while Instant::now() <= deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        error(render_instruction(
            "agent.md",
            &source,
            &[],
            limits(),
            &original
        ))
        .kind(),
        InstructionErrorKind::Request(std::io::ErrorKind::TimedOut)
    );
}
#[test]
fn roles_match_the_canonical_policy_snapshots_byte_for_byte() {
    for (role, expected) in [
        (
            ManagedRole::Scout,
            include_str!("fixtures/agent-instructions/scout-default.md"),
        ),
        (
            ManagedRole::Tracker,
            include_str!("fixtures/agent-instructions/tracker-default.md"),
        ),
        (
            ManagedRole::Scribe,
            include_str!("fixtures/agent-instructions/scribe-default.md"),
        ),
    ] {
        assert_eq!(
            compose_role(role, &Brand::default(), &request())
                .unwrap()
                .body(),
            expected
        );
    }
}
#[test]
fn actual_hub_renderers_preserve_expanded_body_in_current_kiro_profile() {
    use graphit_hub::native::{
        AgentDescriptor, ContentLimits, NativeBinding, NativeToolGrant, RenderContext, ToolAccess,
        ToolGrants, render_agent,
    };
    let source = [template(
        "agent.md",
        InstructionOrigin::Project,
        "Use {{ mcp_tool_xpto }} for {{ artifact }}. {% raw %}{{filePath}}{% endraw %}\n",
    )];
    let rendered = render_instruction(
        "agent.md",
        &source,
        &[
            text("mcp_tool_xpto", "acme_task_query"),
            text("artifact", "wiki"),
        ],
        limits(),
        &request(),
    )
    .unwrap();
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
        "Inspect sources",
        rendered.body(),
        true,
        false,
        &mut ctx,
    )
    .unwrap();
    {
        let binding = NativeBinding::KiroV3;
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: false,
            tools: vec![NativeToolGrant {
                name: "read".into(),
                access: ToolAccess::Read,
            }],
        };
        let output = render_agent(binding, &descriptor, None, &grants, &mut ctx).unwrap();
        assert_eq!(output.provenance.binding, binding);
        let text = std::str::from_utf8(&output.bytes).unwrap();

        let (header, body) = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("---\n")
            .unwrap();
        assert_eq!(body, rendered.body());
        assert!(header.contains("includeMcpJson: false"));
        assert!(header.contains("includePowers: false"));
        assert!(output.relative_entry.ends_with(".md"));
    }
}
