use super::*;
fn field<'a>(plan: &'a NativeMcpPlan, suffix: &[&str]) -> &'a serde_json::Value {
    let mut path = plan.entry.clone();
    path.extend(suffix.iter().map(|v| (*v).to_owned()));
    &plan.fields.iter().find(|f| f.path == path).unwrap().value
}
#[test]
fn every_binding_uses_native_stdio_schema_without_user_defaults() {
    let spec = NativeServerSpec {
        name: "graphit.test".into(),
        transport: NativeServerTransport::Stdio {
            command: "graphit".into(),
            args: vec!["mcp".into(), "--stdio".into(), "ação\n'$".into()],
            env: BTreeMap::from([("GRAPHIT_INSTANCE".into(), "test".into())]),
            env_vars: Vec::new(),
            cwd: None,
        },
    };
    for binding in NativeBinding::ALL {
        let plan = spec.plan(binding).unwrap();
        assert_eq!(plan.binding, binding);
        assert_eq!(plan.entry.last().unwrap(), "graphit.test");
        if binding == NativeBinding::OpenCode {
            assert_eq!(plan.entry, ["mcp", "servers", "graphit.test"]);
        }
        assert!(plan.fields.iter().all(|f| !matches!(
            f.path.last().unwrap().as_str(),
            "timeout" | "disabled" | "enabled"
        )));
        if binding == NativeBinding::OpenCode {
            assert_eq!(field(&plan, &["type"]), "local");
            assert_eq!(
                field(&plan, &["command"]),
                &serde_json::json!(["graphit", "mcp", "--stdio", "ação\n'$"])
            );
            assert_eq!(field(&plan, &["environment", "GRAPHIT_INSTANCE"]), "test");
        } else {
            assert_eq!(field(&plan, &["command"]), "graphit");
            assert_eq!(
                field(&plan, &["args"]),
                &serde_json::json!(["mcp", "--stdio", "ação\n'$"])
            );
            assert_eq!(field(&plan, &["env", "GRAPHIT_INSTANCE"]), "test");
        }
        for generated in &plan.fields {
            assert!(generated.native_value().is_ok());
        }
    }
}
#[test]
fn remote_schema_keeps_explicit_transport_and_native_header_keys() {
    let spec = NativeServerSpec {
        name: "remote".into(),
        transport: NativeServerTransport::Http {
            url: "https://example.test/mcp?project=ação".into(),
            headers: BTreeMap::from([("X-Project".into(), "test".into())]),
            bearer_token_env: None,
        },
    };
    for binding in NativeBinding::ALL {
        let plan = spec.plan(binding).unwrap();
        let url_key = match binding {
            NativeBinding::Qwen => "httpUrl",
            NativeBinding::Antigravity => "serverUrl",
            _ => "url",
        };
        assert_eq!(
            field(&plan, &[url_key]),
            "https://example.test/mcp?project=ação"
        );
        let key = if binding == NativeBinding::Codex {
            "http_headers"
        } else {
            "headers"
        };
        assert_eq!(field(&plan, &[key, "X-Project"]), "test");
    }
    let sse = NativeServerSpec {
        name: "remote".into(),
        transport: NativeServerTransport::Sse {
            url: "https://example.test/events".into(),
            headers: BTreeMap::new(),
        },
    };
    for binding in NativeBinding::ALL {
        assert_eq!(
            sse.plan(binding).is_ok(),
            matches!(
                binding,
                NativeBinding::Claude | NativeBinding::Qwen | NativeBinding::Kimi
            )
        );
    }
}
#[test]
fn unsupported_auth_reference_has_no_fallback() {
    let spec = NativeServerSpec {
        name: "remote".into(),
        transport: NativeServerTransport::Http {
            url: "https://example.test/mcp".into(),
            headers: BTreeMap::new(),
            bearer_token_env: Some("MCP_TOKEN".into()),
        },
    };
    let plan = spec.plan(NativeBinding::Codex).unwrap();
    assert_eq!(field(&plan, &["bearer_token_env_var"]), "MCP_TOKEN");
    for binding in NativeBinding::ALL
        .into_iter()
        .filter(|v| *v != NativeBinding::Codex)
    {
        assert!(matches!(
            spec.plan(binding),
            Err(McpConfigError::UnsupportedOption)
        ));
    }
}
#[test]
fn malformed_or_ambiguous_packets_and_endpoints_are_rejected() {
    assert!(serde_json::from_str::<NativeServerSpec>(r#"{"name":"g","transport":{"type":"stdio","command":"graphit","url":"https://example.test"}}"#).is_err());
    assert!(
        serde_json::from_str::<NativeServerSpec>(
            r#"{"name":"g","transport":{"type":"http","url":null}}"#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<NativeServerSpec>(
        r#"{"name":"g","transport":{"type":"http","url":"https://example.test","unexpected":true}}"#
    )
    .is_err());
    for url in [
        "relative/path",
        "file:///secret",
        "https://user:secret@example.test",
        "https://example.test/#fragment",
        "https://example.test\n",
    ] {
        assert!(endpoint(url).is_err(), "{url}");
    }
    let mut spec = NativeServerSpec::graphit("g");
    spec.name = "bad/name".into();
    assert!(matches!(
        spec.plan(NativeBinding::KiroV3),
        Err(McpConfigError::InvalidName)
    ));
}
#[test]
fn graphit_launch_uses_current_kiro_v3_stdio_schema() {
    let spec = NativeServerSpec::graphit("graphit");
    let plan = spec.plan(NativeBinding::KiroV3).unwrap();
    assert_eq!(plan.entry, ["mcpServers", "graphit"]);
    assert_eq!(field(&plan, &["command"]), "graphit");
    assert_eq!(
        field(&plan, &["args"]),
        &serde_json::json!(["mcp", "--stdio"])
    );
}

#[test]
fn cwd_is_only_emitted_for_native_schemas_that_support_it() {
    let spec = NativeServerSpec {
        name: "g".into(),
        transport: NativeServerTransport::Stdio {
            command: "graphit".into(),
            args: vec![],
            env: BTreeMap::new(),
            env_vars: Vec::new(),
            cwd: Some("/workspace/ação".into()),
        },
    };
    for binding in NativeBinding::ALL {
        let supported = matches!(
            binding,
            NativeBinding::Codex
                | NativeBinding::Qwen
                | NativeBinding::OpenCode
                | NativeBinding::Kimi
        );
        assert_eq!(spec.plan(binding).is_ok(), supported);
    }
}

#[test]
fn codex_inherits_only_named_variables_without_replacing_explicit_env() {
    let spec = NativeServerSpec {
        name: "graphit".into(),
        transport: NativeServerTransport::Stdio {
            command: "graphit".into(),
            args: vec!["mcp".into(), "--stdio".into()],
            env: BTreeMap::from([("EXPLICIT_SETTING".into(), "preserved".into())]),
            env_vars: vec!["GRAPHIT_GLOBAL_DIR".into(), "CUSTOM_HOST_SETTING".into()],
            cwd: None,
        },
    };
    let plan = spec.plan(NativeBinding::Codex).unwrap();
    assert_eq!(
        field(&plan, &["env_vars"]),
        &serde_json::json!(["GRAPHIT_GLOBAL_DIR", "CUSTOM_HOST_SETTING"])
    );
    assert_eq!(field(&plan, &["env", "EXPLICIT_SETTING"]), "preserved");
    for binding in NativeBinding::ALL
        .into_iter()
        .filter(|binding| *binding != NativeBinding::Codex)
    {
        assert!(matches!(
            spec.plan(binding),
            Err(McpConfigError::UnsupportedOption)
        ));
    }
}

#[test]
fn inherited_environment_names_are_validated_without_interpolation() {
    for name in [
        "",
        "GLOBAL=DIR",
        "${HOME}",
        "GRAPHIT\0GLOBAL_DIR",
        "../ROOT",
    ] {
        let spec = NativeServerSpec {
            name: "graphit".into(),
            transport: NativeServerTransport::Stdio {
                command: "graphit".into(),
                args: vec![],
                env: BTreeMap::new(),
                env_vars: vec![name.into()],
                cwd: None,
            },
        };
        assert!(matches!(
            spec.plan(NativeBinding::Codex),
            Err(McpConfigError::InvalidTransport)
        ));
    }
}
