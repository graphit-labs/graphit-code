use graphit_core::{brand::Brand, file_io::SnapshotContext, file_lock::Cancellation};
use graphit_runtime::{
    ManagedRole,
    instructions::{
        CapturedInstruction, InstructionErrorKind, InstructionLimits, InstructionOrigin,
    },
    role_instructions::{RoleAliases, render_role, role_templates},
};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::{Duration, Instant},
};
fn limits() -> InstructionLimits {
    InstructionLimits {
        max_templates: NonZeroUsize::new(12).unwrap(),
        max_template_bytes: NonZeroUsize::new(65536).unwrap(),
        max_values: NonZeroUsize::new(16).unwrap(),
        max_variable_bytes: NonZeroUsize::new(4096).unwrap(),
        max_output_bytes: NonZeroUsize::new(65536).unwrap(),
        fuel: NonZeroU64::new(10000).unwrap(),
        recursion: NonZeroUsize::new(32).unwrap(),
    }
}
#[test]
fn raw_precedence_and_defaults_use_effective_aliases_once() {
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let defaults = role_templates(ManagedRole::Tracker);
    for selected in [
        InstructionOrigin::Project,
        InstructionOrigin::Global,
        InstructionOrigin::Hub,
        InstructionOrigin::CompiledDefault,
    ] {
        let mut candidates: Vec<_> = defaults
            .candidates()
            .iter()
            .map(|v| CapturedInstruction {
                name: v.name,
                origin: v.origin,
                markdown: v.markdown,
            })
            .collect();
        for origin in [
            InstructionOrigin::Project,
            InstructionOrigin::Global,
            InstructionOrigin::Hub,
        ] {
            if origin >= selected {
                candidates.push(CapturedInstruction { name: defaults.entry(), origin, markdown: "{% include 'defaults/roles/tracker.md' %}override {{ mcp_tool_module_skill }} {{ skill_names|join(',') }}" });
            }
        }
        let rendered = render_role(
            ManagedRole::Tracker,
            &Brand::default(),
            &candidates,
            RoleAliases {
                module_skill: "effective_module_skill",
                skills: &["actual_ast", "literal_{{x}}"],
            },
            limits(),
            &original,
        )
        .unwrap();
        assert_eq!(rendered.origin(), selected);
        assert!(rendered.body().contains("effective_module_skill"));
        assert!(rendered.body().contains("literal_{{x}}"));
        assert!(!rendered.body().contains("graphit_module_skill"));
        assert!(rendered.body().contains("Do not write records or code"));
    }
}
#[test]
fn invalid_selected_and_duplicate_templates_do_not_fallback() {
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let defaults = role_templates(ManagedRole::Scout);
    let mut candidates: Vec<_> = defaults
        .candidates()
        .iter()
        .map(|v| CapturedInstruction {
            name: v.name,
            origin: v.origin,
            markdown: v.markdown,
        })
        .collect();
    candidates.push(CapturedInstruction {
        name: defaults.entry(),
        origin: InstructionOrigin::Project,
        markdown: "{{ absent }}",
    });
    assert!(matches!(
        render_role(
            ManagedRole::Scout,
            &Brand::default(),
            &candidates,
            RoleAliases {
                module_skill: "effective",
                skills: &[]
            },
            limits(),
            &original
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Template(_)
    ));
    candidates.push(CapturedInstruction {
        name: defaults.entry(),
        origin: InstructionOrigin::Project,
        markdown: "valid",
    });
    assert_eq!(
        render_role(
            ManagedRole::Scout,
            &Brand::default(),
            &candidates,
            RoleAliases {
                module_skill: "effective",
                skills: &[]
            },
            limits(),
            &original
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::DuplicateTemplate
    );
}
#[test]
fn cancelled_original_is_not_replaced() {
    let cancel = Cancellation::default();
    let original = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
    cancel.cancel();
    let defaults = role_templates(ManagedRole::Scribe);
    assert_eq!(
        render_role(
            ManagedRole::Scribe,
            &Brand::default(),
            defaults.candidates(),
            RoleAliases {
                module_skill: "effective",
                skills: &[]
            },
            limits(),
            &original
        )
        .unwrap_err()
        .kind(),
        InstructionErrorKind::Request(std::io::ErrorKind::Interrupted)
    );
}
