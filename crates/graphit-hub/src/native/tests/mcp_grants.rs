use super::*;

fn document(binding: NativeBinding, rendered: &RenderedContent) -> Value {
    assert_eq!(binding, NativeBinding::KiroV3);
    let text = std::str::from_utf8(&rendered.bytes).unwrap();
    let front = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap()
        .0;
    serde_saphyr::from_str(front).unwrap()
}

#[test]
fn kiro_mcp_read_grants_preapprove_only_exact_tools() {
    for binding in [NativeBinding::KiroV3] {
        let cancel = Cancellation::default();
        let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut context(&cancel)).unwrap();
        let plan = mcp_config::NativeServerSpec::graphit("live_namespace")
            .plan(binding)
            .unwrap();
        let mut grants = grant(binding);
        for tool in ["read_context", "read_source"] {
            grants.tools.push(NativeToolGrant {
                name: format!("@live_namespace/{tool}"),
                access: ToolAccess::Read,
            });
        }
        let rendered = render_agent_with_native_mcp(
            binding,
            &descriptor,
            None,
            &grants,
            NativeAgentRuntime {
                mcp: &plan,
                kiro_hooks: &[],
            },
            &mut context(&cancel),
        )
        .unwrap();
        let data = document(binding, &rendered);
        assert!(
            data["tools"]
                .as_array()
                .unwrap()
                .contains(&json!("@live_namespace"))
        );
        assert_eq!(data["includeMcpJson"], false);
        assert_eq!(
            data["mcpServers"]["live_namespace"]["args"],
            json!(["mcp", "--stdio"])
        );

        assert_eq!(
            data["permissions"]["rules"],
            json!([
                {"capability":"mcp","match":["live_namespace/read_context"],"effect":"allow"},
                {"capability":"mcp","match":["live_namespace/read_source"],"effect":"allow"},
                {"capability":"fs_write","effect":"deny"},
                {"capability":"shell","effect":"deny"}
            ])
        );
        assert!(data.get("allowedTools").is_none());
        assert_eq!(data["includePowers"], false);
    }
}

#[test]
fn kiro_mcp_grants_reject_missing_or_foreign_plan_and_pattern_selectors() {
    for binding in [NativeBinding::KiroV3] {
        let cancel = Cancellation::default();
        let descriptor = AgentDescriptor::parse(SOURCE.as_bytes(), &mut context(&cancel)).unwrap();
        let plan = mcp_config::NativeServerSpec::graphit("live_namespace")
            .plan(binding)
            .unwrap();
        for name in [
            "@foreign/read_context",
            "@live_namespace",
            "@live_namespace/",
            "@live_namespace/*",
            "@live_namespace/read_*",
            "@live_namespace/read?",
            "@live_namespace/[read]",
            "@live_namespace/read/source",
            "@live_namespace/read source",
            "@live_namespace/read\nsource",
            "@live_namespace/@read",
        ] {
            let grants = ToolGrants {
                binding,
                read_only: true,
                allows_shell: false,
                tools: vec![NativeToolGrant {
                    name: name.into(),
                    access: ToolAccess::Read,
                }],
            };
            assert_eq!(
                render_agent_with_native_mcp(
                    binding,
                    &descriptor,
                    None,
                    &grants,
                    NativeAgentRuntime {
                        mcp: &plan,
                        kiro_hooks: &[]
                    },
                    &mut context(&cancel)
                )
                .unwrap_err(),
                ContentError::InvalidPolicy,
                "{name:?}"
            );
        }
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: false,
            tools: vec![NativeToolGrant {
                name: "@live_namespace/read_context".into(),
                access: ToolAccess::Read,
            }],
        };
        assert_eq!(
            render_agent(binding, &descriptor, None, &grants, &mut context(&cancel)).unwrap_err(),
            ContentError::InvalidPolicy
        );
        for access in [ToolAccess::Write, ToolAccess::Shell] {
            let mut denied = grants.clone();
            denied.tools[0].access = access;
            assert_eq!(
                render_agent_with_native_mcp(
                    binding,
                    &descriptor,
                    None,
                    &denied,
                    NativeAgentRuntime {
                        mcp: &plan,
                        kiro_hooks: &[]
                    },
                    &mut context(&cancel)
                )
                .unwrap_err(),
                ContentError::InvalidPolicy
            );
        }
        let mut duplicate = grants.clone();
        duplicate.tools.push(duplicate.tools[0].clone());
        assert_eq!(
            render_agent_with_native_mcp(
                binding,
                &descriptor,
                None,
                &duplicate,
                NativeAgentRuntime {
                    mcp: &plan,
                    kiro_hooks: &[]
                },
                &mut context(&cancel)
            )
            .unwrap_err(),
            ContentError::InvalidPolicy
        );
        for namespace in ["live space", "live*", "live?", "live[one]", "@live"] {
            let malformed = mcp_config::NativeServerSpec::graphit(namespace)
                .plan(binding)
                .unwrap();
            assert_eq!(
                render_agent_with_native_mcp(
                    binding,
                    &descriptor,
                    None,
                    &grant(binding),
                    NativeAgentRuntime {
                        mcp: &malformed,
                        kiro_hooks: &[]
                    },
                    &mut context(&cancel)
                )
                .unwrap_err(),
                ContentError::InvalidPolicy
            );
        }
    }
}
