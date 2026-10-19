use super::*;
use graphit_core::file_lock::Cancellation;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

fn limits() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 16,
        max_nodes: 1024,
        max_scalar_bytes: 8192,
        max_collection_entries: 64,
    }
}
fn context(cancellation: &Cancellation) -> RenderContext<'_> {
    RenderContext::new(
        limits(),
        cancellation,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap()
}
fn grant(binding: NativeBinding) -> ToolGrants {
    let names: &[&str] = match binding {
        NativeBinding::Claude | NativeBinding::Kimi => &["Read", "Grep", "Glob"],
        NativeBinding::Qwen => &["read_file", "grep_search", "glob"],
        NativeBinding::KiroV3 => &["read"],
        NativeBinding::OpenCode => &["read", "grep", "glob"],
        NativeBinding::Antigravity => &["view_file", "grep_search"],
        _ => &[],
    };
    ToolGrants {
        binding,
        read_only: true,
        allows_shell: false,
        tools: names
            .iter()
            .map(|s| NativeToolGrant {
                name: (*s).into(),
                access: ToolAccess::Read,
            })
            .collect(),
    }
}
const SOURCE: &str = "---\nname: scout\ndescription: 'Quoted: café \"metadata\"'\nread_only: true\n---\n\nInspect `${var}`.\n```md\n---\n```\n";

#[test]
fn native_agents_roundtrip_with_literal_body_and_profile_identity() {
    let cancellation = Cancellation::default();
    let mut context = context(&cancellation);
    let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut context).unwrap();
    for binding in NativeBinding::ALL
        .into_iter()
        .filter(|b| !matches!(b, NativeBinding::Cursor))
    {
        let output = render_agent(
            binding,
            &descriptor,
            Some("reviewer"),
            &grant(binding),
            &mut context,
        )
        .unwrap();
        assert_eq!(output.provenance.binding, binding);
        assert_eq!(output.provenance.effective_name, "reviewer");
        assert_eq!(
            output.provenance.input_fingerprint,
            <[u8; 32]>::from(Sha256::digest(SOURCE.as_bytes()))
        );
        assert_eq!(
            output.provenance.output_fingerprint,
            <[u8; 32]>::from(Sha256::digest(&output.bytes))
        );
        let text = std::str::from_utf8(&output.bytes).unwrap();
        match binding {
            NativeBinding::Codex => {
                let native = text.parse::<toml_edit::DocumentMut>().unwrap();
                assert_eq!(native["name"].as_str(), Some("reviewer"));
                assert_eq!(
                    native["developer_instructions"].as_str(),
                    Some(descriptor.body())
                );
                assert!(native.get("sandbox_mode").is_none());
                assert_eq!(native["features"]["shell_tool"].as_bool(), Some(false));
            }
            _ => {
                let (header, body) = text
                    .strip_prefix("---\n")
                    .unwrap()
                    .split_once("---\n")
                    .unwrap();
                let native: Value = serde_saphyr::from_str(header).unwrap();
                assert_eq!(native["name"], "reviewer");
                assert_eq!(native["description"], descriptor.description());
                assert_eq!(body, descriptor.body());
                if binding == NativeBinding::KiroV3 {
                    assert_eq!(native["tools"], json!(["read"]));
                    assert_eq!(native["includeMcpJson"], false);
                    assert_eq!(native["includePowers"], false);
                    assert!(native.get("allowedTools").is_none());
                    assert!(native.get("hooks").is_none());
                    assert!(output.relative_entry.ends_with("reviewer.md"));
                }
                if binding == NativeBinding::OpenCode {
                    assert_eq!(
                        native["permissions"][0],
                        json!({"action":"*","resource":"*","effect":"deny"})
                    );
                    assert!(
                        native["permissions"]
                            .as_array()
                            .unwrap()
                            .contains(&json!({"action":"read","resource":"*","effect":"allow"}))
                    );
                }
            }
        }
        assert!(!text.contains("Standing context"));
        assert!(!text.contains("GRAPHIT_SYSTEM_MANDATE"));
    }
    assert_eq!(descriptor.body(), "\nInspect `${var}`.\n```md\n---\n```\n");
}

#[test]
fn cursor_strict_no_shell_is_explicitly_unsupported() {
    let cancellation = Cancellation::default();
    let mut context = context(&cancellation);
    let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut context).unwrap();
    assert_eq!(
        render_agent(
            NativeBinding::Cursor,
            &descriptor,
            None,
            &grant(NativeBinding::Cursor),
            &mut context
        )
        .unwrap_err(),
        ContentError::UnsupportedCapability
    );
    let descriptor = AgentDescriptor::new(
        "runner",
        "Run selected work",
        "Body\n",
        false,
        true,
        &mut context,
    )
    .unwrap();
    let policy = ToolGrants {
        binding: NativeBinding::Cursor,
        read_only: false,
        allows_shell: true,
        tools: vec![],
    };
    let output = render_agent(
        NativeBinding::Cursor,
        &descriptor,
        None,
        &policy,
        &mut context,
    )
    .unwrap();
    let text = std::str::from_utf8(&output.bytes).unwrap();
    let header = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("---\n")
        .unwrap()
        .0;
    let fields: Value = serde_saphyr::from_str(header).unwrap();
    assert_eq!(fields["readonly"], false);
    assert_eq!(fields["model"], "inherit");
}

#[test]
fn skill_alias_preserves_source_metadata_and_asset_references() {
    let source = b"---\nname: original\ndescription: 'Use: quoted metadata'\nlicense: MIT\ncompatibility: Requires git\nmetadata:\n  author: graphit\n---\nRead [guide](references/guide.md).\n";
    let original = source.to_vec();
    let cancellation = Cancellation::default();
    let mut context = context(&cancellation);
    let descriptor = SkillDescriptor::parse(source, &mut context).unwrap();
    for binding in NativeBinding::ALL {
        let output = render_skill(binding, &descriptor, Some("renamed"), &mut context).unwrap();
        assert!(output.relative_entry.ends_with("skills/renamed/SKILL.md"));
        if binding == NativeBinding::Codex {
            assert!(output.relative_entry.starts_with(".agents/"));
        }
        let text = std::str::from_utf8(&output.bytes).unwrap();
        let (header, body) = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("---\n")
            .unwrap();
        let fields: Value = serde_saphyr::from_str(header).unwrap();
        assert_eq!(fields["name"], "renamed");
        assert_eq!(fields["metadata"]["author"], "graphit");
        assert_eq!(body, descriptor.body());
    }
    assert_eq!(source.as_slice(), original);
    assert_eq!(descriptor.name(), "original");
}

#[test]
fn malformed_and_ambiguous_content_never_reaches_renderer() {
    let invalid: &[&[u8]] = &[
        b"name: scout\nBody",
        b"---\nname: scout\ndescription: ok\n---\n ",
        b"---\nname: scout\nname: other\ndescription: ok\n---\nBody",
        b"---\nname: scout\ndescription: ok\nread_only: null\n---\nBody",
        b"---\nname: scout\ndescription: ok\nread_only: yes\n---\nBody",
        b"---\nname: scout\ndescription: &value ok\n---\nBody",
        b"---\nname: scout\ndescription: !custom ok\n---\nBody",
        b"---\nname: scout\ndescription: ok\n...\nname: other\n---\nBody",
        b"---\nname: scout\ndescription: ok\npermissions: {}\n---\nBody",
        b"---\nname: ../escape\ndescription: ok\n---\nBody",
        b"\xff\xfe",
    ];
    let cancellation = Cancellation::default();
    for input in invalid {
        let mut context = context(&cancellation);
        assert!(AgentDescriptor::parse(input, &mut context).is_err());
        assert_eq!(context.output_bytes(), 0);
    }
    let mut context = context(&cancellation);
    let error = AgentDescriptor::parse(
        b"---\nname: scout\ndescription: SECRET_FIXTURE\nunknown: 1\n---\nBody",
        &mut context,
    )
    .unwrap_err();
    assert!(!error.to_string().contains("SECRET_FIXTURE"));
}

#[test]
fn aggregate_quotas_deadlines_and_cancellation_leave_no_result() {
    let cancellation = Cancellation::default();
    let mut policy = limits();
    policy.max_input_bytes = SOURCE.len();
    let mut bounded = RenderContext::new(
        policy,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    AgentDescriptor::parse(SOURCE.as_bytes(), &mut bounded).unwrap();
    assert_eq!(
        AgentDescriptor::parse(SOURCE.as_bytes(), &mut bounded).unwrap_err(),
        ContentError::LimitExceeded
    );
    let mut policy = limits();
    policy.max_output_bytes = 16;
    let mut bounded = RenderContext::new(
        policy,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut bounded).unwrap();
    assert_eq!(
        render_agent(
            NativeBinding::Claude,
            &descriptor,
            None,
            &grant(NativeBinding::Claude),
            &mut bounded
        )
        .unwrap_err(),
        ContentError::LimitExceeded
    );
    let mut bounded = context(&cancellation);
    let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut bounded).unwrap();
    cancellation.cancel();
    assert_eq!(
        render_agent(
            NativeBinding::Claude,
            &descriptor,
            None,
            &grant(NativeBinding::Claude),
            &mut bounded
        )
        .unwrap_err(),
        ContentError::Cancelled
    );
    assert_eq!(bounded.output_bytes(), 0);
    let cancellation = Cancellation::default();
    assert!(matches!(
        RenderContext::new(limits(), &cancellation, Instant::now()),
        Err(ContentError::TimedOut)
    ));
    let mut policy = limits();
    policy.max_nodes = 1;
    let mut bounded = RenderContext::new(
        policy,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(
        AgentDescriptor::parse(SOURCE.as_bytes(), &mut bounded).unwrap_err(),
        ContentError::LimitExceeded
    );
    let mut policy = limits();
    policy.max_nodes = usize::MAX;
    assert!(matches!(
        RenderContext::new(
            policy,
            &cancellation,
            Instant::now() + Duration::from_secs(5)
        ),
        Err(ContentError::InvalidPolicy)
    ));
}

#[test]
fn tool_profiles_cannot_escalate_through_names_or_artifact_flags() {
    let cancellation = Cancellation::default();
    let mut context = context(&cancellation);
    let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut context).unwrap();
    let mut policy = grant(NativeBinding::Claude);
    policy.tools = vec![NativeToolGrant {
        name: "Write".into(),
        access: ToolAccess::Read,
    }];
    assert_eq!(
        render_agent(
            NativeBinding::Claude,
            &descriptor,
            None,
            &policy,
            &mut context
        )
        .unwrap_err(),
        ContentError::InvalidPolicy
    );
    policy.tools[0].name = "mcp_*".into();
    assert!(
        render_agent(
            NativeBinding::Claude,
            &descriptor,
            None,
            &policy,
            &mut context
        )
        .is_err()
    );
    policy = grant(NativeBinding::Claude);
    assert!(
        render_agent(
            NativeBinding::KiroV3,
            &descriptor,
            None,
            &policy,
            &mut context
        )
        .is_err()
    );
    let descriptor =
        AgentDescriptor::new("runner", "Needs shell", "Body", false, true, &mut context).unwrap();
    assert_eq!(
        render_agent(
            NativeBinding::Claude,
            &descriptor,
            None,
            &grant(NativeBinding::Claude),
            &mut context
        )
        .unwrap_err(),
        ContentError::UnsupportedCapability
    );
    let mut policy = grant(NativeBinding::KiroV3);
    policy.tools = vec![NativeToolGrant {
        name: "write".into(),
        access: ToolAccess::Read,
    }];
    let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut context).unwrap();
    assert_eq!(
        render_agent(
            NativeBinding::KiroV3,
            &descriptor,
            None,
            &policy,
            &mut context
        )
        .unwrap_err(),
        ContentError::InvalidPolicy
    );
}

#[test]
fn optional_values_unicode_character_limits_and_crlf_are_semantic() {
    let cancellation = Cancellation::default();
    let mut context = context(&cancellation);
    let source = b"---\r\nname: agent\r\ndescription: ok\r\nread_only: false\r\nallows_shell: false\r\n---\r\nBody\r\n";
    let descriptor = AgentDescriptor::parse(source, &mut context).unwrap();
    assert!(!descriptor.read_only());
    assert!(!descriptor.allows_shell());
    assert_eq!(descriptor.body(), "Body\r\n");
    let descriptor = AgentDescriptor::new(
        "agent",
        &"é".repeat(1024),
        "Body",
        false,
        false,
        &mut context,
    )
    .unwrap();
    assert_eq!(descriptor.description().chars().count(), 1024);
    assert!(
        AgentDescriptor::new(
            "agent",
            &"é".repeat(1025),
            "Body",
            false,
            false,
            &mut context
        )
        .is_err()
    );
    let source = b"---\nname: skill\ndescription: ok\nlicense: null\n---\nBody";
    assert_eq!(
        SkillDescriptor::parse(source, &mut context).unwrap_err(),
        ContentError::InvalidContent
    );
    let source = b"---\nname: skill\ndescription: ok\nlicense: ''\n---\nBody";
    let descriptor = SkillDescriptor::parse(source, &mut context).unwrap();
    let output = render_skill(NativeBinding::KiroV3, &descriptor, None, &mut context).unwrap();
    let text = std::str::from_utf8(&output.bytes).unwrap();
    let header = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("---\n")
        .unwrap()
        .0;
    let fields: Value = serde_saphyr::from_str(header).unwrap();
    assert_eq!(fields["license"], "");
    assert!(fields.get("metadata").is_none());
}

#[test]
fn native_kiro_shell_catalog_never_crosses_profiles_or_access_classes() {
    let cancellation = Cancellation::default();
    let mut context = context(&cancellation);
    let descriptor = AgentDescriptor::new(
        "runner",
        "Use selected tools",
        "Body",
        false,
        true,
        &mut context,
    )
    .unwrap();
    {
        let (binding, write, shell) = (NativeBinding::KiroV3, "write", "shell");
        let grants = ToolGrants {
            binding,
            read_only: false,
            allows_shell: true,
            tools: vec![
                NativeToolGrant {
                    name: write.into(),
                    access: ToolAccess::Write,
                },
                NativeToolGrant {
                    name: shell.into(),
                    access: ToolAccess::Shell,
                },
            ],
        };
        let output = render_agent(binding, &descriptor, None, &grants, &mut context).unwrap();
        assert!(std::str::from_utf8(&output.bytes).unwrap().contains(shell));
        let mut wrong = grants.clone();
        wrong.tools[1].access = ToolAccess::Write;
        assert_eq!(
            render_agent(binding, &descriptor, None, &wrong, &mut context).unwrap_err(),
            ContentError::InvalidPolicy
        );
    }
    let grants = ToolGrants {
        binding: NativeBinding::KiroV3,
        read_only: false,
        allows_shell: false,
        tools: vec![NativeToolGrant {
            name: "shell".into(),
            access: ToolAccess::Write,
        }],
    };
    let descriptor =
        AgentDescriptor::new("reader", "No shell", "Body", false, false, &mut context).unwrap();
    assert_eq!(
        render_agent(
            NativeBinding::KiroV3,
            &descriptor,
            None,
            &grants,
            &mut context
        )
        .unwrap_err(),
        ContentError::InvalidPolicy
    );
}

#[test]
fn every_metadata_quota_is_checked_before_output() {
    let cancellation = Cancellation::default();
    let source = b"---\nname: scout\ndescription: description\n---\nBody";
    for policy in [
        ContentLimits {
            max_scalar_bytes: 4,
            ..limits()
        },
        ContentLimits {
            max_collection_entries: 1,
            ..limits()
        },
        ContentLimits {
            max_depth: 1,
            ..limits()
        },
    ] {
        let mut context = RenderContext::new(
            policy,
            &cancellation,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(
            AgentDescriptor::parse(source, &mut context).unwrap_err(),
            ContentError::LimitExceeded
        );
        assert_eq!(context.output_bytes(), 0);
    }
    let mut context = context(&cancellation);
    let malformed = b"---\nname: scout\ndescription: *missing\n---\nBody";
    assert!(AgentDescriptor::parse(malformed, &mut context).is_err());
    // Failed native parser work consumes the remaining admitted node allowance.
    assert_eq!(
        AgentDescriptor::parse(source, &mut context).unwrap_err(),
        ContentError::LimitExceeded
    );
    for policy in [
        ContentLimits {
            max_input_bytes: 0,
            ..limits()
        },
        ContentLimits {
            max_depth: 81,
            ..limits()
        },
        ContentLimits {
            max_output_bytes: 0,
            ..limits()
        },
    ] {
        assert!(matches!(
            RenderContext::new(
                policy,
                &cancellation,
                Instant::now() + Duration::from_secs(5)
            ),
            Err(ContentError::InvalidPolicy)
        ));
    }
}

#[test]
fn opencode_edit_permission_requires_the_complete_tool_family() {
    let cancellation = Cancellation::default();
    let descriptor = AgentDescriptor::new(
        "editor",
        "Edits files",
        "Edit as requested.",
        false,
        false,
        &mut context(&cancellation),
    )
    .unwrap();
    for names in [
        &["edit"][..],
        &["write"],
        &["patch"],
        &["edit", "write"],
        &["edit", "write", "patch"],
    ] {
        let grants = ToolGrants {
            binding: NativeBinding::OpenCode,
            read_only: false,
            allows_shell: false,
            tools: names
                .iter()
                .map(|name| NativeToolGrant {
                    name: (*name).into(),
                    access: ToolAccess::Write,
                })
                .collect(),
        };
        let mut render_context = context(&cancellation);
        let result = render_agent(
            NativeBinding::OpenCode,
            &descriptor,
            None,
            &grants,
            &mut render_context,
        );
        if names.len() != 3 {
            assert_eq!(result.unwrap_err(), ContentError::UnsupportedCapability);
            assert_eq!(render_context.output_bytes(), 0);
        } else {
            let text = String::from_utf8(result.unwrap().bytes).unwrap();
            let header = text.split("---\n").nth(1).unwrap();
            let value: Value = serde_saphyr::from_str(header).unwrap();
            assert_eq!(
                value["permissions"][0],
                json!({"action":"*","resource":"*","effect":"deny"})
            );
            assert!(
                value["permissions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .skip(1)
                    .all(|r| r == &json!({"action":"edit","resource":"*","effect":"allow"}))
            );
        }
    }
}

#[test]
fn descriptors_are_revalidated_before_copies_under_smaller_render_quotas() {
    let cancellation = Cancellation::default();
    let large = "a".repeat(4096);
    for field in ["license", "metadata"] {
        let input = if field == "license" {
            format!("---\nname: skill\ndescription: Small\nlicense: {large}\n---\nBody\n")
        } else {
            format!(
                "---\nname: skill\ndescription: Small\nmetadata:\n  large: {large}\n---\nBody\n"
            )
        };
        let descriptor =
            SkillDescriptor::parse(input.as_bytes(), &mut context(&cancellation)).unwrap();
        let mut small = limits();
        small.max_scalar_bytes = 64;
        let mut render_context = RenderContext::new(
            small,
            &cancellation,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(
            render_skill(
                NativeBinding::Claude,
                &descriptor,
                None,
                &mut render_context
            )
            .unwrap_err(),
            ContentError::LimitExceeded
        );
        assert_eq!(render_context.output_bytes(), 0);
    }
    let descriptor = AgentDescriptor::new(
        "agent",
        "Small",
        &large,
        true,
        false,
        &mut context(&cancellation),
    )
    .unwrap();
    {
        let binding = NativeBinding::Codex;
        let descriptor = AgentDescriptor::new(
            "scalar-bound",
            "inspect",
            &large,
            false,
            false,
            &mut context(&cancellation),
        )
        .unwrap();
        let grants = ToolGrants::native(binding, false, false);
        let mut small = limits();
        small.max_scalar_bytes = 64;
        let mut render_context = RenderContext::new(
            small,
            &cancellation,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(
            render_agent(binding, &descriptor, None, &grants, &mut render_context).unwrap_err(),
            ContentError::LimitExceeded
        );
        assert_eq!(render_context.output_bytes(), 0);
    }
    // A Markdown body is borrowed text, not a YAML scalar. Its actual document
    // output limit still applies when re-rendering an existing descriptor.
    let mut markdown_limits = limits();
    markdown_limits.max_scalar_bytes = 64;
    let mut markdown_context = RenderContext::new(
        markdown_limits,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let rendered = render_agent(
        NativeBinding::KiroV3,
        &descriptor,
        None,
        &grant(NativeBinding::KiroV3),
        &mut markdown_context,
    )
    .unwrap();
    assert!(
        std::str::from_utf8(&rendered.bytes)
            .unwrap()
            .contains(&large)
    );
    markdown_limits.max_output_bytes = 1024;
    let mut bounded_context = RenderContext::new(
        markdown_limits,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(
        render_agent(
            NativeBinding::KiroV3,
            &descriptor,
            None,
            &grant(NativeBinding::KiroV3),
            &mut bounded_context,
        )
        .unwrap_err(),
        ContentError::LimitExceeded
    );
}

#[test]
fn tracker_readonly_shell_preserves_supported_native_fields() {
    for binding in [NativeBinding::Codex, NativeBinding::Cursor] {
        let cancel = graphit_core::file_lock::Cancellation::default();
        let mut context = RenderContext::new(
            limits(),
            &cancel,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .unwrap();
        let descriptor = AgentDescriptor::new(
            "graphit-tracker",
            "Inspect changes",
            "read only",
            true,
            true,
            &mut context,
        )
        .unwrap();
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: true,
            tools: vec![],
        };
        if binding == NativeBinding::Codex {
            let rendered = render_agent(binding, &descriptor, None, &grants, &mut context).unwrap();
            let native = std::str::from_utf8(&rendered.bytes)
                .unwrap()
                .parse::<toml_edit::DocumentMut>()
                .unwrap();
            assert_eq!(native["developer_instructions"].as_str(), Some("read only"));
            assert_eq!(native["features"]["shell_tool"].as_bool(), Some(true));
            assert!(native.get("sandbox_mode").is_none());
        } else {
            let rendered = render_agent(binding, &descriptor, None, &grants, &mut context).unwrap();
            assert!(
                std::str::from_utf8(&rendered.bytes)
                    .unwrap()
                    .contains("readonly: true")
            );
        }
    }
    let cancel = graphit_core::file_lock::Cancellation::default();
    let mut context = RenderContext::new(
        limits(),
        &cancel,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let descriptor = AgentDescriptor::new(
        "graphit-tracker",
        "Inspect changes",
        "read only",
        true,
        true,
        &mut context,
    )
    .unwrap();
    {
        let binding = NativeBinding::KiroV3;
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: true,
            tools: vec![],
        };
        assert!(matches!(
            render_agent(binding, &descriptor, None, &grants, &mut context),
            Err(ContentError::UnsupportedCapability)
        ));
    }
}

fn role_proxy_plan(binding: NativeBinding, name: &str) -> mcp_config::NativeMcpPlan {
    mcp_config::NativeServerSpec {
        name: name.into(),
        transport: mcp_config::NativeServerTransport::Stdio {
            command: "graphit".into(),
            args: vec!["mcp".into(), "--stdio".into(), "--role".into()],
            env: Default::default(),
            env_vars: Vec::new(),
            cwd: None,
        },
    }
    .plan(binding)
    .unwrap()
}
#[test]
fn role_mcp_profiles_keep_run_proxy_and_reject_runtime_inheritance() {
    {
        let binding = NativeBinding::KiroV3;
        let cancel = Cancellation::default();
        let mut ctx = context(&cancel);
        let descriptor = AgentDescriptor::new(
            "graphit-scout",
            "inspect",
            "Use the scoped tools.",
            true,
            false,
            &mut ctx,
        )
        .unwrap();
        let plan = role_proxy_plan(binding, "graphit");
        let rendered = render_role_agent_with_mcp(
            binding,
            Role::Scout,
            &descriptor,
            None,
            &grant(binding),
            NativeAgentRuntime {
                mcp: &plan,
                kiro_hooks: &[],
            },
            &mut ctx,
        )
        .unwrap();
        let data: Value = {
            let text = std::str::from_utf8(&rendered.bytes).unwrap();
            let front = text
                .strip_prefix("---\n")
                .unwrap()
                .split_once("\n---\n")
                .unwrap()
                .0;
            serde_saphyr::from_str(front).unwrap()
        };
        assert_eq!(data["includeMcpJson"], false);
        assert_eq!(data["mcpServers"]["graphit"]["command"], "graphit");
        assert_eq!(
            data["mcpServers"]["graphit"]["args"],
            json!(["mcp", "--stdio", "--role"])
        );
        assert!(data["mcpServers"]["graphit"].get("env").is_none());
        assert!(
            data["tools"]
                .as_array()
                .unwrap()
                .contains(&json!("@graphit"))
        );
        if binding == NativeBinding::KiroV3 {
            assert_eq!(data["includePowers"], false);
            let rules = data["permissions"]["rules"].as_array().unwrap();
            assert!(rules.contains(&json!({"capability":"fs_write","effect":"deny"})));
            assert!(rules.contains(&json!({"capability":"shell","effect":"deny"})));
            assert!(
                rules.contains(&json!({"capability":"mcp","match":["graphit/*"],"effect":"allow"}))
            );
        }
    }
}
#[test]
fn role_mcp_rejects_persisted_credentials_runtime_proxy_and_foreign_binding() {
    let cancel = Cancellation::default();
    let mut ctx = context(&cancel);
    let descriptor = AgentDescriptor::new(
        "graphit-scout",
        "inspect",
        "Use scoped tools.",
        true,
        false,
        &mut ctx,
    )
    .unwrap();
    let mut server = mcp_config::NativeServerSpec::graphit("graphit");
    let runtime = server.plan(NativeBinding::KiroV3).unwrap();
    assert!(matches!(
        render_role_agent_with_mcp(
            NativeBinding::KiroV3,
            Role::Scout,
            &descriptor,
            None,
            &grant(NativeBinding::KiroV3),
            NativeAgentRuntime {
                mcp: &runtime,
                kiro_hooks: &[]
            },
            &mut ctx
        ),
        Err(ContentError::InvalidPolicy)
    ));
    if let mcp_config::NativeServerTransport::Stdio { args, env, .. } = &mut server.transport {
        args.push("--role".into());
        env.insert(
            "GRAPHIT_ROLE_BEARER".into(),
            "never-persist-this-fixture".into(),
        );
    }
    let secret = server.plan(NativeBinding::KiroV3).unwrap();
    assert!(matches!(
        render_role_agent_with_mcp(
            NativeBinding::KiroV3,
            Role::Scout,
            &descriptor,
            None,
            &grant(NativeBinding::KiroV3),
            NativeAgentRuntime {
                mcp: &secret,
                kiro_hooks: &[]
            },
            &mut ctx
        ),
        Err(ContentError::InvalidPolicy)
    ));
    let foreign = role_proxy_plan(NativeBinding::Codex, "graphit");
    assert!(matches!(
        render_role_agent_with_mcp(
            NativeBinding::KiroV3,
            Role::Scout,
            &descriptor,
            None,
            &grant(NativeBinding::KiroV3),
            NativeAgentRuntime {
                mcp: &foreign,
                kiro_hooks: &[]
            },
            &mut ctx
        ),
        Err(ContentError::UnsupportedCapability)
    ));
}
#[test]
fn role_profiles_admit_only_the_configured_graphit_mcp_server() {
    for binding in [NativeBinding::Claude, NativeBinding::Kimi] {
        let cancel = Cancellation::default();
        let mut ctx = context(&cancel);
        let descriptor = AgentDescriptor::new(
            "graphit-scout",
            "Inspect",
            "Use scoped tools.",
            true,
            false,
            &mut ctx,
        )
        .unwrap();
        let plan = mcp_config::NativeServerSpec::graphit("graphit")
            .plan(binding)
            .unwrap();
        let rendered = render_role_agent_with_mcp(
            binding,
            Role::Scout,
            &descriptor,
            None,
            &grant(binding),
            NativeAgentRuntime {
                mcp: &plan,
                kiro_hooks: &[],
            },
            &mut ctx,
        )
        .unwrap();
        let text = std::str::from_utf8(&rendered.bytes).unwrap();
        let front = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap()
            .0;
        let data: Value = serde_saphyr::from_str(front).unwrap();
        assert_eq!(
            data["tools"],
            json!(["Read", "Grep", "Glob", "mcp__graphit__*"])
        );
        if binding == NativeBinding::Claude {
            assert_eq!(data["mcpServers"], json!(["graphit"]));
        } else {
            assert!(data.get("mcpServers").is_none());
        }
    }
}

#[test]
fn opencode_role_uses_configured_mcp_namespace_and_selectable_profile() {
    let cancel = Cancellation::default();
    let mut ctx = context(&cancel);
    let binding = NativeBinding::OpenCode;
    let descriptor = AgentDescriptor::new(
        "graphit-scout",
        "Inspect",
        "Use scoped tools.",
        true,
        false,
        &mut ctx,
    )
    .unwrap();
    let plan = mcp_config::NativeServerSpec::graphit("graphit-code")
        .plan(binding)
        .unwrap();
    let rendered = render_role_agent_with_mcp(
        binding,
        Role::Scout,
        &descriptor,
        None,
        &grant(binding),
        NativeAgentRuntime {
            mcp: &plan,
            kiro_hooks: &[],
        },
        &mut ctx,
    )
    .unwrap();
    let text = std::str::from_utf8(&rendered.bytes).unwrap();
    let front = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap()
        .0;
    let data: Value = serde_saphyr::from_str(front).unwrap();
    assert_eq!(data["mode"], "all");
    assert!(data.get("mcp").is_none());
    assert!(data.get("mcpServers").is_none());
    assert_eq!(
        data["permissions"],
        json!([
            {"action":"*", "resource":"*", "effect":"deny"},
            {"action":"read", "resource":"*", "effect":"allow"},
            {"action":"grep", "resource":"*", "effect":"allow"},
            {"action":"glob", "resource":"*", "effect":"allow"},
            {"action":"execute", "resource":"*", "effect":"allow"},
            {"action":"graphit-code_*", "resource":"*", "effect":"allow"}
        ])
    );
    let mut server = mcp_config::NativeServerSpec::graphit("graphit-code");
    if let mcp_config::NativeServerTransport::Stdio { env, .. } = &mut server.transport {
        env.insert("GRAPHIT_ROLE_BEARER".into(), "must-not-persist".into());
    }
    let secret = server.plan(binding).unwrap();
    assert!(matches!(
        render_role_agent_with_mcp(
            binding,
            Role::Scout,
            &descriptor,
            None,
            &grant(binding),
            NativeAgentRuntime {
                mcp: &secret,
                kiro_hooks: &[]
            },
            &mut ctx
        ),
        Err(ContentError::InvalidPolicy)
    ));
}

#[test]
fn role_mcp_fingerprint_binds_the_actual_proxy_namespace() {
    let cancel = Cancellation::default();
    let mut descriptor_ctx = context(&cancel);
    let descriptor = AgentDescriptor::new(
        "graphit-scout",
        "inspect",
        "Use scoped tools.",
        true,
        false,
        &mut descriptor_ctx,
    )
    .unwrap();
    let binding = NativeBinding::KiroV3;
    let first = render_role_agent_with_mcp(
        binding,
        Role::Scout,
        &descriptor,
        None,
        &grant(binding),
        NativeAgentRuntime {
            mcp: &role_proxy_plan(binding, "graphit"),
            kiro_hooks: &[],
        },
        &mut context(&cancel),
    )
    .unwrap();
    let changed = render_role_agent_with_mcp(
        binding,
        Role::Scout,
        &descriptor,
        None,
        &grant(binding),
        NativeAgentRuntime {
            mcp: &role_proxy_plan(binding, "selected"),
            kiro_hooks: &[],
        },
        &mut context(&cancel),
    )
    .unwrap();
    assert_ne!(
        first.provenance.input_fingerprint,
        changed.provenance.input_fingerprint
    );
    assert_ne!(
        first.provenance.output_fingerprint,
        changed.provenance.output_fingerprint
    );
    let repeated = render_role_agent_with_mcp(
        binding,
        Role::Scout,
        &descriptor,
        None,
        &grant(binding),
        NativeAgentRuntime {
            mcp: &role_proxy_plan(binding, "graphit"),
            kiro_hooks: &[],
        },
        &mut context(&cancel),
    )
    .unwrap();
    assert_eq!(first.bytes, repeated.bytes);
    assert_eq!(
        first.provenance.input_fingerprint,
        repeated.provenance.input_fingerprint
    );
}

#[test]
fn root_agent_mcp_preserves_catalog_selector_and_host_approval_policy() {
    {
        let binding = NativeBinding::KiroV3;
        let cancel = Cancellation::default();
        let mut ctx = context(&cancel);
        let descriptor = AgentDescriptor::parse(
            b"---\nname: coordinator\ndescription: Root project coordinator\nread_only: true\n---\nUse the current authenticated catalog.\n",
            &mut ctx,
        ).unwrap();
        let plan = mcp_config::NativeServerSpec::graphit("root_namespace")
            .plan(binding)
            .unwrap();
        let rendered = render_agent_with_native_mcp(
            binding,
            &descriptor,
            None,
            &grant(binding),
            NativeAgentRuntime {
                mcp: &plan,
                kiro_hooks: &[],
            },
            &mut ctx,
        )
        .unwrap();
        let data: Value = {
            let text = std::str::from_utf8(&rendered.bytes).unwrap();
            let front = text
                .strip_prefix("---\n")
                .unwrap()
                .split_once("\n---\n")
                .unwrap()
                .0;
            serde_saphyr::from_str(front).unwrap()
        };
        assert_eq!(
            data["mcpServers"]["root_namespace"]["args"],
            json!(["mcp", "--stdio"])
        );
        assert!(
            data["tools"]
                .as_array()
                .unwrap()
                .contains(&json!("@root_namespace"))
        );
        for absent in ["allowedTools", "model", "role"] {
            assert!(data.get(absent).is_none());
        }
        if binding == NativeBinding::KiroV3 {
            assert_eq!(
                data["permissions"]["rules"],
                json!([
                    {"capability":"fs_write","effect":"deny"},
                    {"capability":"shell","effect":"deny"}
                ])
            );
        } else {
            assert!(data.get("permissions").is_none());
        }
        assert_eq!(data["includeMcpJson"], false);
        let other = mcp_config::NativeServerSpec::graphit("other_namespace")
            .plan(binding)
            .unwrap();
        let changed = render_agent_with_native_mcp(
            binding,
            &descriptor,
            None,
            &grant(binding),
            NativeAgentRuntime {
                mcp: &other,
                kiro_hooks: &[],
            },
            &mut context(&cancel),
        )
        .unwrap();
        assert_ne!(
            rendered.provenance.input_fingerprint,
            changed.provenance.input_fingerprint
        );
        assert_ne!(rendered.bytes, changed.bytes);
    }
}

#[test]
fn root_agent_mcp_rejects_role_secret_and_wrong_binding_plans() {
    let cancel = Cancellation::default();
    let mut ctx = context(&cancel);
    let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut ctx).unwrap();
    let binding = NativeBinding::KiroV3;
    let role_plan = role_proxy_plan(binding, "graphit");
    assert_eq!(
        render_agent_with_native_mcp(
            binding,
            &descriptor,
            None,
            &grant(binding),
            NativeAgentRuntime {
                mcp: &role_plan,
                kiro_hooks: &[]
            },
            &mut ctx
        )
        .unwrap_err(),
        ContentError::InvalidPolicy
    );
    let mut server = mcp_config::NativeServerSpec::graphit("graphit");
    if let mcp_config::NativeServerTransport::Stdio { env, .. } = &mut server.transport {
        env.insert("ROOT_TEST_SECRET".into(), "must-not-be-static".into());
    }
    let secret = server.plan(binding).unwrap();
    assert_eq!(
        render_agent_with_native_mcp(
            binding,
            &descriptor,
            None,
            &grant(binding),
            NativeAgentRuntime {
                mcp: &secret,
                kiro_hooks: &[]
            },
            &mut ctx
        )
        .unwrap_err(),
        ContentError::InvalidPolicy
    );
    let plain = mcp_config::NativeServerSpec::graphit("graphit")
        .plan(NativeBinding::Codex)
        .unwrap();
    assert_eq!(
        render_agent_with_native_mcp(
            NativeBinding::KiroV3,
            &descriptor,
            None,
            &grant(NativeBinding::KiroV3),
            NativeAgentRuntime {
                mcp: &plain,
                kiro_hooks: &[]
            },
            &mut ctx
        )
        .unwrap_err(),
        ContentError::UnsupportedCapability
    );
}

#[path = "tests/mcp_grants.rs"]
mod mcp_grants;

#[test]
fn root_native_profiles_link_only_documented_mcp_contracts() {
    for binding in NativeBinding::ALL {
        let cancel = Cancellation::default();
        let mut ctx = context(&cancel);
        let descriptor = AgentDescriptor::new(
            "coordinator",
            "Coordinate work",
            "Use live PROJECT tools.",
            false,
            true,
            &mut ctx,
        )
        .unwrap();
        let mut server = mcp_config::NativeServerSpec::graphit("live_namespace");
        if binding == NativeBinding::Codex
            && let mcp_config::NativeServerTransport::Stdio { env_vars, .. } = &mut server.transport
        {
            env_vars.push("GRAPHIT_GLOBAL_DIR".into());
        }
        let plan = server.plan(binding).unwrap();
        if !matches!(binding, NativeBinding::KiroV3 | NativeBinding::Antigravity) {
            let mut readonly_context = context(&cancel);
            let readonly = AgentDescriptor::new(
                "reader",
                "Read tools",
                "Inspect only.",
                true,
                false,
                &mut readonly_context,
            )
            .unwrap();
            let readonly_grants = ToolGrants::native(binding, true, false);
            assert_eq!(
                render_agent_with_native_mcp(
                    binding,
                    &readonly,
                    None,
                    &readonly_grants,
                    NativeAgentRuntime {
                        mcp: &plan,
                        kiro_hooks: &[]
                    },
                    &mut readonly_context
                )
                .unwrap_err(),
                ContentError::UnsupportedCapability
            );
        }
        let grants = ToolGrants::native(binding, false, true);
        let rendered = render_agent_with_native_mcp(
            binding,
            &descriptor,
            None,
            &grants,
            NativeAgentRuntime {
                mcp: &plan,
                kiro_hooks: &[],
            },
            &mut ctx,
        );
        if binding == NativeBinding::Antigravity {
            assert_eq!(rendered.unwrap_err(), ContentError::UnsupportedCapability);
            continue;
        }
        let rendered = rendered.unwrap();
        let text = std::str::from_utf8(&rendered.bytes).unwrap();
        if binding == NativeBinding::Codex {
            let document = text.parse::<toml_edit::DocumentMut>().unwrap();
            assert!(document.get("sandbox_mode").is_none());
            assert!(document.get("mcpServers").is_none());
            assert!(document.get("tools").is_none());
            assert_eq!(document["features"]["shell_tool"].as_bool(), Some(true));
            continue;
        }
        let front = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap()
            .0;
        let document: Value = serde_saphyr::from_str(front).unwrap();
        match binding {
            NativeBinding::Claude => {
                assert_eq!(document["mcpServers"][0]["live_namespace"]["type"], "stdio");
                assert!(
                    document["tools"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("mcp__live_namespace__*"))
                );
            }
            NativeBinding::Qwen => {
                assert_eq!(
                    document["mcpServers"]["live_namespace"]["args"],
                    json!(["mcp", "--stdio"])
                );
                assert!(
                    document["tools"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("mcp__live_namespace__*"))
                );
            }
            NativeBinding::Kimi => {
                assert!(document.get("mcpServers").is_none());
                let tools = document["tools"].as_array().unwrap();
                for name in ["mcp__live_namespace__*", "Read", "Write", "Edit", "Bash"] {
                    assert!(tools.contains(&json!(name)));
                }
            }
            NativeBinding::Cursor => {
                assert!(document.get("mcpServers").is_none());
                assert!(document.get("tools").is_none());
            }
            NativeBinding::OpenCode => {
                assert_eq!(document["mode"], "all");
                assert!(
                    document["permissions"]
                        .as_array()
                        .unwrap()
                        .contains(&json!({"action":"execute","resource":"*","effect":"ask"}))
                );
                assert!(
                    document["permissions"].as_array().unwrap().contains(
                        &json!({"action":"live_namespace_*","resource":"*","effect":"ask"})
                    )
                );
                assert_eq!(
                    document["permissions"][0],
                    json!({"action":"*","resource":"*","effect":"deny"})
                );
            }
            NativeBinding::KiroV3 => {
                assert_eq!(
                    document["mcpServers"]["live_namespace"]["args"],
                    json!(["mcp", "--stdio"])
                );
            }
            NativeBinding::Codex | NativeBinding::Antigravity => unreachable!(),
        }
        assert!(!text.contains("inheritMcp"));
    }
}
