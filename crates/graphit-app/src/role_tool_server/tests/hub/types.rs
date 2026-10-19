use super::super::*;
use graphit_hub::ArtifactType;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hub_type_path_admitted_sdk_layouts_and_closed_input() {
    for binding in [
        NativeBinding::Codex,
        NativeBinding::KiroV3,
        NativeBinding::Kimi,
    ] {
        let (_temp, server, raw) = fixture_with_launch(binding, false);
        let (_session, _run, reference, bearer) = credential(
            &server,
            &raw,
            Role::Scout,
            &[Permission::Read(Module::Hub)],
            Duration::from_secs(90),
        );
        let (_denied_session, _denied_run, denied_reference, denied_bearer) = credential(
            &server,
            &raw,
            Role::Tracker,
            &[Permission::Read(Module::Task)],
            Duration::from_secs(90),
        );
        let snapshot = project_snapshot(&server);
        let root = snapshot.projects[0].0.root();
        // A physical target's parent is deliberately a file. Advice must stay lexical.
        let opaque_parent = root.join(binding.adapter().layout().root).join("agents");
        std::fs::create_dir_all(opaque_parent.parent().unwrap()).unwrap();
        std::fs::write(&opaque_parent, b"unread target parent").unwrap();
        let (uri, serving) = serve(&server).await;
        let client = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        let denied = connect(
            &uri,
            role_credential(&denied_reference, &denied_bearer, server.0.task.content),
        )
        .await;
        let tools = client.list_all_tools().await.unwrap();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "graphit_hub_type_path")
            .unwrap();
        let schema = Value::Object(tool.input_schema.as_ref().clone());
        let input = &schema["properties"]["input"];
        assert_eq!(input["additionalProperties"], false);
        assert_eq!(input["required"], json!(["artifact_type", "name"]));
        assert_eq!(input["properties"].as_object().unwrap().len(), 2);
        assert_eq!(
            input["properties"]["artifact_type"]["enum"],
            json!(ArtifactType::ALL.map(ArtifactType::as_str))
        );
        assert!(
            !denied
                .list_all_tools()
                .await
                .unwrap()
                .iter()
                .any(|tool| tool.name == "graphit_hub_type_path")
        );
        let denial = call(
            &denied,
            "graphit_hub_type_path",
            json!({"input":{"artifact_type":"agent","name":"observer"}}),
            true,
        )
        .await;
        assert!(denial.get("path").is_none());

        let cases: &[(&str, Option<&str>, &str, Option<&str>)] = match binding {
            NativeBinding::Codex => &[
                (
                    "agent",
                    Some(".codex/agents/observer.toml"),
                    "physical",
                    Some("file"),
                ),
                (
                    "skill",
                    Some(".agents/skills/observer"),
                    "physical",
                    Some("directory"),
                ),
                (
                    "command",
                    Some(".codex/commands/observer.md"),
                    "physical",
                    Some("file"),
                ),
                (
                    "workflow",
                    Some(".codex/commands/observer.md"),
                    "physical",
                    Some("file"),
                ),
                ("rule", None, "dynamic_context", None),
                ("mcp", None, "no_physical_path", None),
                ("ast", None, "no_physical_path", None),
                ("knowledge", None, "no_physical_path", None),
                ("language", None, "no_physical_path", None),
                ("power", None, "no_physical_path", None),
            ],
            NativeBinding::KiroV3 => &[
                (
                    "agent",
                    Some(".kiro/agents/observer.md"),
                    "physical",
                    Some("file"),
                ),
                (
                    "skill",
                    Some(".kiro/skills/observer"),
                    "physical",
                    Some("directory"),
                ),
                ("rule", None, "dynamic_context", None),
            ],
            NativeBinding::Kimi => &[(
                "agent",
                Some(".kimi-code/agents/observer.md"),
                "physical",
                Some("file"),
            )],
            _ => unreachable!(),
        };
        for (kind, path, placement, node_kind) in cases {
            let result = call(
                &client,
                "graphit_hub_type_path",
                json!({"input":{"artifact_type":kind,"name":"producer/observer"}}),
                false,
            )
            .await;
            assert_eq!(result["artifact_type"], *kind);
            assert_eq!(result["binding"], json!(binding));
            assert_eq!(result["name"], "observer");
            assert_eq!(result["path"], json!(path));
            assert_eq!(result["placement"], *placement);
            assert_eq!(result["node_kind"], json!(node_kind));
            assert_eq!(
                result["agent_schema"],
                json!((*kind == "agent").then(|| binding.schema()))
            );
            if *kind == "agent" {
                assert_eq!(
                    result["target"],
                    json!({"kind":"native_binding","binding":binding})
                );
            } else if *kind == "skill" {
                assert_eq!(
                    result["target"],
                    json!({"kind":"shared_adapter","adapter":binding.adapter()})
                );
            }
            assert_eq!(result.as_object().unwrap().len(), 8);
            let wire = serde_json::to_string(&result).unwrap();
            assert!(!wire.contains(root.to_str().unwrap()));
            assert!(!wire.contains(server.0.policy.source.installation.root().to_str().unwrap()));
        }
        if matches!(binding, NativeBinding::KiroV3 | NativeBinding::Kimi) {
            for kind in ["command", "workflow"] {
                let error = call(
                    &client,
                    "graphit_hub_type_path",
                    json!({"input":{"artifact_type":kind,"name":"observer"}}),
                    true,
                )
                .await;
                assert_eq!(error, json!({"code":"unsupported_artifact_type"}));
            }
        }
        for name in [
            "".to_owned(),
            "../escape".to_owned(),
            "/absolute".to_owned(),
            "CON".to_owned(),
            "name\\escape".to_owned(),
            "x".repeat(256),
        ] {
            let error = call(
                &client,
                "graphit_hub_type_path",
                json!({"input":{"artifact_type":"skill","name":name}}),
                true,
            )
            .await;
            assert_eq!(error, json!({"code":"invalid_arguments"}));
        }
        for input in [
            json!({"artifact_type":"unknown","name":"observer"}),
            json!({"artifact_type":"agent"}),
            json!({"artifact_type":"agent","name":null}),
            json!({"artifact_type":"agent","name":"observer","binding":"codex"}),
            json!({"artifact_type":"agent","name":"observer","root":"/private"}),
            json!({"artifact_type":"agent","name":"observer","scope":"global_installation"}),
        ] {
            let error = call(
                &client,
                "graphit_hub_type_path",
                json!({"input":input}),
                true,
            )
            .await;
            assert_eq!(error, json!({"code":"invalid_arguments"}));
        }
        assert_eq!(
            std::fs::read(&opaque_parent).unwrap(),
            b"unread target parent"
        );
        assert!(!root.join(".agents/skills/observer").exists());
        client.cancel().await.unwrap();
        denied.cancel().await.unwrap();
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hub_type_path_uses_verified_binding_instead_of_daemon_default() {
    let (_temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, false);
    let permissions = [Permission::Read(Module::Hub)];
    let definition = RoleDefinition::tool_owned(
        Role::Scout,
        project_snapshot(&server).projects[0].0.brand(),
        NativeBinding::Codex,
        &permissions,
        [37; 32],
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
    )
    .unwrap();
    let (_session, _run, reference, bearer) = credential_with_definition_at_binding(
        &server,
        &raw,
        0,
        definition,
        &permissions,
        Duration::from_secs(90),
        NativeBinding::Codex,
    );
    let (uri, serving) = serve(&server).await;
    let client = connect_at_with_binding(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
        None,
        Some(NativeBinding::Codex),
    )
    .await;
    assert_eq!(server.0.task.binding, NativeBinding::KiroV3);
    let agent = call(
        &client,
        "graphit_hub_type_path",
        json!({"input":{"artifact_type":"agent","name":"observer"}}),
        false,
    )
    .await;
    assert_eq!(agent["binding"], "codex");
    assert_eq!(agent["agent_schema"], "codex-agent-toml");
    assert_eq!(agent["path"], ".codex/agents/observer.toml");
    assert_eq!(
        agent["target"],
        json!({"kind":"native_binding","binding":"codex"})
    );
    let skill = call(
        &client,
        "graphit_hub_type_path",
        json!({"input":{"artifact_type":"skill","name":"observer"}}),
        false,
    )
    .await;
    assert_eq!(skill["binding"], "codex");
    assert_eq!(skill["path"], ".agents/skills/observer");
    assert_eq!(
        skill["target"],
        json!({"kind":"shared_adapter","adapter":"codex"})
    );
    // A different header cannot change the binding authorized for the issued credential.
    let wrong_binding = connect_at_with_binding(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
        None,
        Some(NativeBinding::KiroV3),
    )
    .await;
    let denied = call(
        &wrong_binding,
        "graphit_hub_type_path",
        json!({"input":{"artifact_type":"agent","name":"observer"}}),
        true,
    )
    .await;
    assert_eq!(
        denied,
        json!({"code":"authentication_or_permission_denied"})
    );
    client.cancel().await.unwrap();
    wrong_binding.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hub_type_path_withholds_cancelled_expired_revoked_and_oversized_results() {
    let (_temp, mut server, raw) = fixture_with_launch(NativeBinding::Codex, false);
    Arc::get_mut(&mut server.0).unwrap().task.timeout = Duration::from_secs(5);
    let (uri, serving) = serve(&server).await;
    for mode in ["cancelled", "expired", "revoked"] {
        let (session, run, reference, bearer) = credential(
            &server,
            &raw,
            Role::Scout,
            &[Permission::Read(Module::Hub)],
            Duration::from_secs(90),
        );
        let client = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        let (entered, observed) = std::sync::mpsc::channel();
        let (release, waiting) = oneshot::channel();
        *server.0.before_delivery.lock().unwrap() = Some((entered, waiting));
        let result = client.call_tool(
            CallToolRequestParams::new("graphit_hub_type_path").with_arguments(
                json!({"ai_optimized":false,"input":{"artifact_type":"agent","name":"observer"}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        );
        let observed = tokio::task::spawn_blocking(move || {
            observed.recv_timeout(Duration::from_secs(10)).unwrap()
        });
        let result = if mode == "expired" {
            // Keep delivery held until the MCP wrapper itself returns its deadline.
            let (result, observed) =
                tokio::time::timeout(server.0.task.timeout + Duration::from_secs(5), async {
                    tokio::join!(result, observed)
                })
                .await
                .expect("MCP deadline must finish while delivery remains held");
            assert!(observed.unwrap().is_cancelled());
            release.send(()).unwrap();
            result
        } else {
            let invalidate = async {
                let original_cancel = observed.await.unwrap();
                match mode {
                    "cancelled" => original_cancel.cancel(),
                    "revoked" => {
                        let cancel = Cancellation::default();
                        let original = request(&cancel, server.0.task.content);
                        let issuer = server.0.auth.issuer();
                        let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
                        let parent = issuer.admin_parent(&caller, &session).unwrap();
                        issuer.close_run(&parent, &run).unwrap();
                    }
                    _ => unreachable!(),
                }
                release.send(()).unwrap();
            };
            let (result, ()) = tokio::join!(result, invalidate);
            result
        };
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(true));
        let value: Value = typed_value(&result);
        let expected = match mode {
            "cancelled" => "request_cancelled_or_expired",
            "expired" => "deadline",
            "revoked" => "authentication_or_permission_denied",
            _ => unreachable!(),
        };
        assert_eq!(value, json!({"code":expected}));
        client.cancel().await.unwrap();
    }
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();

    let (_temp, mut bounded, raw) = fixture_with_launch(NativeBinding::Codex, false);
    Arc::get_mut(&mut bounded.0)
        .unwrap()
        .task
        .content
        .max_output_bytes = 128;
    let (_session, _run, reference, bearer) = credential(
        &bounded,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Hub)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&bounded).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, bounded.0.task.content),
    )
    .await;
    let error = call(
        &client,
        "graphit_hub_type_path",
        json!({"input":{"artifact_type":"agent","name":"observer"}}),
        true,
    )
    .await;
    assert_eq!(error, json!({"code":"response_limit"}));
    client.cancel().await.unwrap();
    bounded.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
