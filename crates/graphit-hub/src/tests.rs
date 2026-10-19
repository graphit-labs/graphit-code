use super::*;
use graphit_core::{
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
};
use serde_json::{Value, json};
use std::process::Command;

fn snapshot(values: &[(ConfigSource, Value)]) -> ConfigSnapshot {
    let brand = Brand::default();
    ConfigSnapshot::new(
        &brand,
        values.iter().map(|(source, value)| {
            ConfigDocument::new(
                &brand,
                *source,
                [(ConfigKey::new("agent").unwrap(), value.clone())],
            )
            .unwrap()
        }),
    )
    .unwrap()
}
fn chosen(binding: NativeBinding, origin: SelectionOrigin) -> Selection {
    Selection { binding, origin }
}

#[test]
fn explicit_flag_alias_and_known_outsider_override_all_layers() {
    let config = snapshot(&[
        (ConfigSource::Command, json!(false)),
        (ConfigSource::Project, json!("codex")),
        (ConfigSource::Environment, json!("antigravity")),
    ]);
    let members = AgentMembership::new([NativeBinding::Cursor]);
    assert_eq!(
        select_agent(&config, Some("CLAUDE-CODE"), &members).unwrap(),
        chosen(NativeBinding::Claude, SelectionOrigin::Flag)
    );
    for invalid in ["", " ", " codex", "codex ", "unknown", "Ｃodex"] {
        let error = select_agent(&config, Some(invalid), &members).unwrap_err();
        assert_eq!(error.origin(), SelectionOrigin::Flag);
        assert!(!error.to_string().contains(invalid) || invalid.is_empty() || invalid == " ");
    }
}

#[test]
fn project_agent_precedence_is_specialized_and_generic_core_is_unchanged() {
    let config = snapshot(&[
        (ConfigSource::Project, json!("codex")),
        (ConfigSource::Environment, json!("antigravity")),
    ]);
    let members = AgentMembership::default();
    assert_eq!(
        config
            .get::<&str>(&ConfigKey::new("agent").unwrap())
            .unwrap(),
        Some("antigravity")
    );
    assert_eq!(
        select_agent(&config, None, &members).unwrap(),
        chosen(
            NativeBinding::Codex,
            SelectionOrigin::Configuration(ConfigSource::Project)
        )
    );
    let config = snapshot(&[
        (ConfigSource::Command, json!("qwen")),
        (ConfigSource::Project, json!("codex")),
        (ConfigSource::Environment, json!("antigravity")),
    ]);
    assert_eq!(
        select_agent(&config, None, &members).unwrap(),
        chosen(
            NativeBinding::Qwen,
            SelectionOrigin::Configuration(ConfigSource::Command)
        )
    );
}

#[test]
fn ambient_precedence_and_default_are_provenanced() {
    let members = AgentMembership::default();
    let cases = [
        (
            vec![
                (ConfigSource::Environment, json!("antigravity")),
                (ConfigSource::Global, json!("claude")),
                (ConfigSource::Distribution, json!("kiro-v3")),
            ],
            chosen(
                NativeBinding::Antigravity,
                SelectionOrigin::Configuration(ConfigSource::Environment),
            ),
        ),
        (
            vec![
                (ConfigSource::Global, json!("claude")),
                (ConfigSource::Distribution, json!("kiro-v3")),
            ],
            chosen(
                NativeBinding::Claude,
                SelectionOrigin::Configuration(ConfigSource::Global),
            ),
        ),
        (
            vec![(ConfigSource::Distribution, json!("kiro-v3"))],
            chosen(
                NativeBinding::KiroV3,
                SelectionOrigin::Configuration(ConfigSource::Distribution),
            ),
        ),
        (
            vec![],
            chosen(NativeBinding::OpenCode, SelectionOrigin::Default),
        ),
    ];
    for (layers, expected) in cases {
        assert_eq!(
            select_agent(&snapshot(&layers), None, &members).unwrap(),
            expected
        );
    }
}

#[test]
fn ambient_membership_match_or_first_member_and_explicit_outsider() {
    let members =
        AgentMembership::parse(["CLAUDE-code", "antigravity", "claude", "Antigravity"]).unwrap();
    assert_eq!(
        members.as_slice(),
        &[NativeBinding::Claude, NativeBinding::Antigravity]
    );
    assert_eq!(
        select_agent(
            &snapshot(&[(ConfigSource::Environment, json!("antigravity"))]),
            None,
            &members
        )
        .unwrap(),
        chosen(
            NativeBinding::Antigravity,
            SelectionOrigin::Configuration(ConfigSource::Environment)
        )
    );
    assert_eq!(
        select_agent(
            &snapshot(&[(ConfigSource::Global, json!("qwen"))]),
            None,
            &members
        )
        .unwrap(),
        chosen(NativeBinding::Claude, SelectionOrigin::Membership)
    );
    assert_eq!(
        select_agent(
            &snapshot(&[(ConfigSource::Project, json!("codex"))]),
            None,
            &members
        )
        .unwrap()
        .binding,
        NativeBinding::Codex
    );
    let contains_default = AgentMembership::new([NativeBinding::Claude, NativeBinding::OpenCode]);
    assert_eq!(
        select_agent(&snapshot(&[]), None, &contains_default).unwrap(),
        chosen(NativeBinding::OpenCode, SelectionOrigin::Default)
    );
}

#[test]
fn winning_invalid_layers_fail_and_shadowed_layers_are_not_read() {
    for source in [
        ConfigSource::Command,
        ConfigSource::Project,
        ConfigSource::Environment,
        ConfigSource::Global,
        ConfigSource::Distribution,
    ] {
        for value in [
            json!(""),
            json!("  "),
            json!("unknown"),
            json!(false),
            json!(12),
            json!([]),
            json!({}),
        ] {
            let config = snapshot(&[(source, value)]);
            assert_eq!(
                select_agent(&config, None, &AgentMembership::default())
                    .unwrap_err()
                    .origin(),
                SelectionOrigin::Configuration(source)
            );
            assert_eq!(
                select_agent(&config, Some("codex"), &AgentMembership::default())
                    .unwrap()
                    .binding,
                NativeBinding::Codex
            );
        }
    }
    let config = snapshot(&[
        (ConfigSource::Project, json!("claude")),
        (ConfigSource::Environment, json!(false)),
        (ConfigSource::Global, json!("unknown")),
    ]);
    assert_eq!(
        select_agent(&config, None, &AgentMembership::default())
            .unwrap()
            .binding,
        NativeBinding::Claude
    );
    let config = snapshot(&[
        (ConfigSource::Command, json!("antigravity")),
        (ConfigSource::Project, json!(false)),
    ]);
    assert_eq!(
        select_agent(&config, None, &AgentMembership::default())
            .unwrap()
            .binding,
        NativeBinding::Antigravity
    );
}

#[test]
fn null_is_rejected_before_snapshot_or_agent_resolution() {
    let error = ConfigDocument::new(
        &Brand::default(),
        ConfigSource::Project,
        [(ConfigKey::new("agent").unwrap(), Value::Null)],
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn batch_uses_flag_or_ordered_membership_then_single_fallback() {
    let config = snapshot(&[
        (ConfigSource::Command, json!(false)),
        (ConfigSource::Environment, json!("unknown")),
    ]);
    let members = AgentMembership::parse(["qwen", "claude-code", "qwen", "codex"]).unwrap();
    assert_eq!(
        select_agents(&config, Some("kiro-v3"), &members).unwrap(),
        vec![NativeBinding::KiroV3]
    );
    assert_eq!(
        select_agents(&config, None, &members).unwrap(),
        members.as_slice()
    ); // Config is shadowed by explicit membership for batch.
    assert_eq!(
        select_agents(&config, Some(""), &members)
            .unwrap_err()
            .origin(),
        SelectionOrigin::Flag
    );
    assert!(select_agents(&config, None, &AgentMembership::default()).is_err());
    assert_eq!(
        select_agents(&snapshot(&[]), None, &AgentMembership::default()).unwrap(),
        vec![NativeBinding::OpenCode]
    );
    assert_eq!(
        select_agents(
            &snapshot(&[(ConfigSource::Project, json!("claude"))]),
            None,
            &AgentMembership::default()
        )
        .unwrap(),
        vec![NativeBinding::Claude]
    );
}

#[test]
fn invalid_membership_fails_whole_input_and_valid_storage_is_bounded_by_ten() {
    assert!(AgentMembership::parse(["codex", "unknown", "qwen"]).is_err());
    assert!(AgentMembership::parse(["codex", ""]).is_err());
    let members = AgentMembership::new(NativeBinding::ALL.into_iter().cycle().take(10_000));
    assert_eq!(members.as_slice(), &NativeBinding::ALL);
    let names: Vec<_> = NativeBinding::ALL.iter().map(|id| id.as_str()).collect();
    assert_eq!(AgentMembership::parse(names).unwrap(), members);
}

#[test]
fn eight_binding_layouts_are_fixed_and_distinguish_dynamic_rules_and_native_formats() {
    let expected = [
        (".agents", Some("workflows"), "md"),
        (".claude", Some("commands"), "md"),
        (".codex", Some("commands"), "toml"),
        (".cursor", Some("commands"), "md"),
        (".kiro", Some("hooks"), "md"),
        (".opencode", Some("commands"), "md"),
        (".qwen", Some("commands"), "md"),
        (".kimi-code", None, "md"),
    ];
    assert_eq!(NativeBinding::ALL.len(), expected.len());
    assert_eq!(crate::AdapterId::ALL.len(), 8);
    for (id, (root, commands, extension)) in NativeBinding::ALL.into_iter().zip(expected) {
        let layout = id.adapter().layout();
        assert_eq!(
            (layout.root, layout.commands, id.agent_extension()),
            (root, commands, extension)
        );
        assert_eq!((layout.skills, layout.agents), ("skills", "agents"));
        assert_eq!(layout.rules, RulePlacement::DynamicContext);
    }
}

#[test]
fn canonical_ids_roundtrip_and_active_aliases_never_create_duplicate_records() {
    for id in NativeBinding::ALL {
        assert_eq!(id.as_str().parse::<NativeBinding>().unwrap(), id);
        assert_eq!(
            id.as_str().to_uppercase().parse::<NativeBinding>().unwrap(),
            id
        );
        assert_eq!(serde_json::to_value(id).unwrap(), json!(id.as_str()));
        assert_eq!(
            serde_json::from_value::<NativeBinding>(json!(id.as_str())).unwrap(),
            id
        );
    }
    let id = "CLAUDE-CODE".parse::<NativeBinding>().unwrap();
    assert_eq!(serde_json::to_value(id).unwrap(), json!("claude"));
    for removed in ["gemini", "gemini-code", "GEMINI-CODE"] {
        assert!(removed.parse::<NativeBinding>().is_err());
        assert!(removed.parse::<crate::AdapterId>().is_err());
        assert!(serde_json::from_value::<NativeBinding>(json!(removed)).is_err());
        assert!(serde_json::from_value::<crate::AdapterId>(json!(removed)).is_err());
        assert!(AgentMembership::parse([removed]).is_err());
        assert!(
            select_agent(
                &snapshot(&[(ConfigSource::Command, json!(removed))]),
                None,
                &AgentMembership::default()
            )
            .is_err()
        );
    }
    assert!(serde_json::from_value::<NativeBinding>(json!("unknown")).is_err());
}

#[test]
fn interleaved_captured_brands_layers_and_memberships_remain_independent() {
    let agent_key = ConfigKey::new("agent").unwrap();
    let brand_a = Brand::new("brand-a").unwrap();
    let brand_b = Brand::new("brand-b").unwrap();
    let a = ConfigSnapshot::capture_environment(
        &brand_a,
        std::slice::from_ref(&agent_key),
        [("BRAND_A_AGENT".into(), "antigravity".into())],
    )
    .unwrap();
    let b = ConfigSnapshot::capture_environment(
        &brand_b,
        &[agent_key],
        [("BRAND_B_AGENT".into(), "qwen".into())],
    )
    .unwrap();
    let a = ConfigSnapshot::new(&brand_a, [a]).unwrap();
    let b = ConfigSnapshot::new(&brand_b, [b]).unwrap();
    let members_a = AgentMembership::new([NativeBinding::Claude, NativeBinding::Antigravity]);
    let members_b = AgentMembership::new([NativeBinding::KiroV3, NativeBinding::Codex]);
    for _ in 0..5 {
        assert_eq!(
            select_agent(&a, None, &members_a).unwrap().binding,
            NativeBinding::Antigravity
        );
        assert_eq!(
            select_agent(&b, None, &members_b).unwrap(),
            chosen(NativeBinding::KiroV3, SelectionOrigin::Membership)
        );
    }
}

#[test]
fn captured_selection_is_independent_of_real_child_environment_and_working_directory() {
    const CHILD: &str = "GRAPHIT_HUB_SELECTION_CHILD";
    if std::env::var_os(CHILD).is_some() {
        assert_eq!(std::env::var("GRAPHIT_AGENT").unwrap(), "qwen");
        let config = snapshot(&[(ConfigSource::Environment, json!("antigravity"))]);
        assert_eq!(
            select_agent(&config, None, &AgentMembership::default())
                .unwrap()
                .binding,
            NativeBinding::Antigravity
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::captured_selection_is_independent_of_real_child_environment_and_working_directory", "--nocapture"])
        .env(CHILD, "1").env("GRAPHIT_AGENT", "qwen").current_dir(directory.path()).output().unwrap();
    assert!(
        output.status.success(),
        "child context qualification failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config = snapshot(&[(ConfigSource::Project, json!("codex"))]);
    assert_eq!(
        select_agent(&config, None, &AgentMembership::default())
            .unwrap()
            .binding,
        NativeBinding::Codex
    );
}

#[test]
fn current_kiro_binding_has_no_legacy_aliases_or_preview_membership() {
    let members = AgentMembership::parse(["kiro-v3", "KIRO-V3", "kiro-v3"]).unwrap();
    assert_eq!(members.as_slice(), &[NativeBinding::KiroV3]);
    assert_eq!(
        select_agents(&snapshot(&[]), None, &members).unwrap(),
        members.as_slice()
    );
    assert_eq!(
        select_agent(&snapshot(&[]), Some("kiro-v3"), &members)
            .unwrap()
            .binding,
        NativeBinding::KiroV3
    );
    for legacy in ["kiro", "kiro-v2", "kiro-v3-preview"] {
        assert!(AgentMembership::parse([legacy]).is_err());
        assert!(select_agent(&snapshot(&[]), Some(legacy), &members).is_err());
    }
}
