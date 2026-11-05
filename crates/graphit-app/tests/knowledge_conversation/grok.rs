use super::*;

impl Fixture {
    async fn start_grok(&self, broker: &Broker, deadline_ms: Option<u64>) -> ForegroundSource {
        let program = self.temp.path().join("grok");
        agent_fixture::install(&program, json!({"kind":"conversation","protocol":"grok"}));
        self.start_native(
            broker,
            Some(json!(true)),
            deadline_ms,
            NativeBinding::Codex,
            Some(graphit_ai::cli_provider::CliCompletionProvider::Grok),
            vec![
                ("PATH".into(), self.temp.path().as_os_str().to_owned()),
                ("GROK_CAPTURED".into(), "literal-captured".into()),
                ("GROK_JOURNAL".into(), self.launches.as_os_str().to_owned()),
                (
                    "GROK_RELEASE".into(),
                    self.temp.path().join("grok-release").into_os_string(),
                ),
            ],
        )
        .await
    }
}

async fn grok_call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, impl rmcp::ClientHandler>,
    operation: &str,
    id: &str,
    query: &str,
) -> rmcp::model::CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(args(operation, id, query).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
}

#[test]
fn grok_incoming_continuity_cold_reopen_selected_binding_and_user_isolation() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        for restart in 0..4 {
            let owner = f.start_grok(&broker, None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let operations = if restart == 0 {
                vec!["create", "continue"]
            } else {
                vec!["continue"]
            };
            for operation in operations {
                let launches = f.launches().len();
                let query = match restart {
                    2 => "unrelated generation",
                    3 => "selected change",
                    _ => "literal question β",
                };
                let result = grok_call(&client, operation, "grok-A", query).await;
                if restart == 3 {
                    assert_eq!(result.is_error, Some(true));
                    assert_eq!(f.launches().len(), launches);
                    continue;
                }
                assert_eq!(
                    result.is_error,
                    Some(false),
                    "{:?}",
                    result.structured_content
                );
                let payload = result.structured_content.unwrap();
                assert_eq!(payload["persistence"]["status"], "persisted");
                assert_eq!(payload["persistence"]["conversation"]["engine"], "grok");
                assert!(
                    payload["persistence"]["conversation"]
                        .get("native_id")
                        .is_none()
                );
                assert_eq!(payload["receipt"]["completion"]["answer"], "Paris answer");
                let end: Value = serde_json::from_str(
                    payload["receipt"]["native_result_json"].as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(end["sessionId"], "00000000-0000-4000-8000-000000000001");
                assert!(payload["receipt"].get("observed_init").is_none());
            }
            if restart == 1 {
                let other =
                    rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
                assert_eq!(
                    grok_call(&other, "continue", "grok-A", "foreign")
                        .await
                        .is_error,
                    Some(true)
                );
                other.cancel().await.unwrap();
                let launches = f.launches().len();
                broker.state.allowed.store(false, Ordering::SeqCst);
                assert_eq!(
                    grok_call(&client, "continue", "grok-A", "denied")
                        .await
                        .is_error,
                    Some(true)
                );
                assert_eq!(f.launches().len(), launches);
                broker.state.allowed.store(true, Ordering::SeqCst);
            }
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            // The publication fixture owns its own real DaemonAuth singleton.
            // Publish only after the Foreground owner has actually joined.
            match restart {
                1 => f.republish(DocumentChange::Unrelated).await,
                2 => f.republish(DocumentChange::Selected).await,
                _ => {}
            }
        }
        let launches = f.launches();
        assert_eq!(launches.len(), 4);
        let base = json!([
            "--output-format",
            "streaming-json",
            "--model",
            "fixture-model",
            "--verbatim",
            "--deny",
            "*"
        ]);
        assert_eq!(launches[0]["args"], base);
        for launch in &launches[1..] {
            assert_eq!(launch["args"][7], "--resume");
            assert_eq!(launch["args"][8], "00000000-0000-4000-8000-000000000001");
            assert_eq!(launch["captured"], "literal-captured");
            assert_eq!(launch["cwd"], f.project.root().to_str().unwrap());
        }
        broker.shutdown().await;
    });
}

#[test]
fn grok_incoming_native_failures_never_publish_answer_or_create_binding() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start_grok(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        assert_eq!(
            grok_call(&client, "create", "grok-good", "initial")
                .await
                .is_error,
            Some(false)
        );
        for query in [
            "mismatch",
            "missing session",
            "invalid session",
            "native error",
            "malformed",
            "missing end",
            "nonzero",
        ] {
            let (operation, id) = if query == "mismatch" {
                ("continue", "grok-good")
            } else {
                ("create", query)
            };
            let result = grok_call(&client, operation, id, query).await;
            assert_eq!(
                result.is_error,
                Some(true),
                "{query}: {:?}",
                result.structured_content
            );
            let payload = result.structured_content.unwrap();
            let cause = match query {
                "mismatch" => "Sink(ConsumerFailed)",
                "missing session" => "Wire",
                "invalid session" => "Sink(ConsumerFailed)",
                "native error" => "Provider",
                "malformed" => "Wire",
                "missing end" => "MissingResult",
                "nonzero" => "NonzeroExit",
                _ => unreachable!(),
            };
            assert_eq!(payload["execution_error"], cause, "{query}: {payload}");
            assert!(payload["receipt"].get("classification").is_none());
            let known = payload["receipt"]["known_events"].as_u64().unwrap();
            assert!(known >= 1, "{query}: native text progress prefix lost");
            assert!(payload["receipt"]["frames"].as_u64().unwrap() >= 1);
            assert!(payload["receipt"]["frames"].as_u64().unwrap() >= known);
            assert!(
                payload["receipt"]["process"]["observed"]["stdout"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
            if query != "mismatch" {
                assert!(payload["receipt"]["accepted"]["stdout"].as_u64().unwrap() > 0);
            }
            if query == "nonzero" {
                assert_eq!(payload["receipt"]["process"]["exit_code"], 7);
            }

            assert!(
                payload["receipt"]["completion"].get("answer").is_none(),
                "{query}: {payload}"
            );
            assert!(
                payload["receipt"].get("native_result_json").is_none()
                    || payload["receipt"]["native_result_json"].is_null()
            );
            assert!(payload["persistence"].get("conversation").is_none());
        }
        let listed = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(listed.is_error, Some(false));
        let records = listed.structured_content.unwrap();
        assert_eq!(records["conversations"].as_array().unwrap().len(), 1);
        assert_eq!(records["conversations"][0]["conversation_id"], "grok-good");
        let before = f.launches().len();
        let mut changed = args("continue", "grok-good", "model changed");
        changed["model"] = json!("other-model");
        let rejected = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation")
                    .with_arguments(changed.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(rejected.is_error, Some(true));
        let clear = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation")
                    .with_arguments(clear_args("grok-good").as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(clear.is_error, Some(true));
        assert_eq!(f.launches().len(), before);
        // Failed resume retains the prior binding; there is no fallback create.
        assert_eq!(
            grok_call(&client, "continue", "grok-good", "recovered")
                .await
                .is_error,
            Some(false)
        );
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn grok_incoming_progress_precedes_exit_and_actual_cleanup_releases_writer() {
    native_test(async {
        for mode in [
            "release",
            "store-fault",
            "physical-denied",
            "deadline",
            "cancel",
            "shutdown",
        ] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let owner = f
                .start_grok(&broker, (mode == "deadline").then_some(2000))
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let raw = broker.state.token("caller-A", RESOURCE, expiry());
            let mut headers = HashMap::new();
            let mut bearer = header::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
            bearer.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, bearer);
            let progress = ProgressCollector(Arc::new(std::sync::Mutex::new(Vec::new())));
            let client = progress
                .clone()
                .serve_with_lifecycle(
                    StreamableHttpClientTransport::with_client(
                        reqwest::Client::builder().no_proxy().build().unwrap(),
                        StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
                    ),
                    ClientLifecycleMode::Discover {
                        preferred_versions: vec![ProtocolVersion::LATEST],
                    },
                )
                .await
                .unwrap();
            let mut params = CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(
                    args("create", "grok-gated", "wait for release")
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            params.meta = Some(
                serde_json::from_value(json!({"progressToken":"grok-real-progress"})).unwrap(),
            );
            let handle = client
                .peer()
                .send_cancellable_request(
                    rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                        params,
                    )),
                    rmcp::service::PeerRequestOptions::no_options(),
                )
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while !progress
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|message| message.contains("grok native progress"))
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(f.launches().len(), 1);
            let prompt_path = PathBuf::from(f.launches()[0]["prompt_file"].as_str().unwrap());
            assert!(
                prompt_path.exists(),
                "{mode}: private prompt released before process exit"
            );
            let user = kiro_clear_user(&f, &broker);
            let writer = user.runtime_root().join("artifact-store.lock");
            assert!(
                graphit_core::file_lock::FileLock::is_locked(&writer).unwrap(),
                "{mode}: writer released before exit"
            );
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(b"grok-gated")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let target = user
                .user_root()
                .unwrap()
                .join(format!("graphit-ai-conversation-{digest}.json"));
            assert!(!target.exists(), "provisional native identity persisted");
            let moved = f.temp.path().join("grok-moved-project");
            if mode == "physical-denied" {
                std::fs::rename(f.project.root(), &moved).unwrap();
            }
            if mode == "store-fault" {
                // Isolated fixture destination, never the running Graphit store.
                std::fs::write(&target, b"foreign-owned-destination").unwrap();
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            if mode == "cancel" {
                handle
                    .cancel(Some("cancel actual Grok fixture".into()))
                    .await
                    .unwrap();
            } else {
                // Keep the actual request handle alive until the chosen terminal condition.
                if mode == "shutdown" {
                    stop.cancel();
                }
                if mode == "deadline" || mode == "shutdown" {
                    tokio::time::timeout(Duration::from_secs(10), async {
                        while !f.temp.path().join("grok-release.term").exists() {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                    })
                    .await
                    .unwrap();
                    assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
                }
                std::fs::write(f.temp.path().join("grok-release"), b"release").unwrap();
                let response = handle.await_response().await;
                if mode == "release" || mode == "store-fault" || mode == "physical-denied" {
                    let rmcp::model::ServerResult::CallToolResult(result) = response.unwrap()
                    else {
                        panic!("call result")
                    };
                    let payload = result.structured_content.unwrap();
                    assert_eq!(result.is_error, Some(mode != "release"), "{payload}");
                    if mode != "release" {
                        assert_eq!(
                            payload["persistence"]["status"],
                            if mode == "store-fault" {
                                "failed"
                            } else {
                                "denied"
                            }
                        );
                        assert!(payload["receipt"]["completion"].get("answer").is_none());
                        assert!(payload["receipt"]["native_result_json"].is_null());
                    }
                    assert_eq!(payload["receipt"]["process"]["cleanup"], "Complete");
                }
            }
            if mode == "cancel" {
                tokio::time::timeout(Duration::from_secs(10), async {
                    while !f.temp.path().join("grok-release.term").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
                std::fs::write(f.temp.path().join("grok-release"), b"release").unwrap();
            }
            if mode == "physical-denied" {
                std::fs::rename(&moved, f.project.root()).unwrap();
            }
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            assert!(
                !graphit_core::file_lock::FileLock::is_locked(&writer).unwrap(),
                "{mode}: writer retained after Host join"
            );
            if mode == "deadline" || mode == "cancel" || mode == "shutdown" {
                assert!(!target.exists());
            }
            assert!(
                !prompt_path.exists(),
                "{mode}: private prompt retained after Host join"
            );
            broker.shutdown().await;
        }
    });
}
