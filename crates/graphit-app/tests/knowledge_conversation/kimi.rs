use super::*;
#[path = "../../../graphit-ai/tests/support/kimi_peer.rs"]
mod peer;
#[test]
#[ignore = "native subprocess peer; invoked only by focused Conversation tests"]
fn native_kimi_conversation_peer() {
    peer::run();
}
impl Fixture {
    async fn start_kimi(&self, broker: &Broker, deadline: Option<u64>) -> ForegroundSource {
        let program = self.temp.path().join("kimi");
        let executable = std::env::current_exe().unwrap();
        let quoted = format!("'{}'", executable.to_str().unwrap().replace('\'', "'\\''"));
        std::fs::write(&program, format!("#!/bin/sh\n[ \"$*\" = 'web --no-open --host 127.0.0.1 --port 0 --log-level error' ] || exit 81\nexec {quoted} --ignored --exact kimi::native_kimi_conversation_peer --nocapture --format terse\n")).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        if !self.temp.path().join("kimi-mode").exists() {
            self.kimi_mode("answer");
        }
        let mut environment = vec![
            ("PATH".into(), self.temp.path().as_os_str().to_owned()),
            ("KIMI_PEER".into(), "1".into()),
            ("KIMI_CAPTURED".into(), "literal-captured".into()),
        ];
        for (key, file) in [
            ("KIMI_SESSION_STATE", "kimi-session"),
            ("KIMI_JOURNAL", "kimi-journal"),
            ("KIMI_CONTROL_MODE", "kimi-mode"),
            ("KIMI_PEER_PORT", "kimi-port"),
        ] {
            environment.push((key.into(), self.temp.path().join(file).into_os_string()));
        }
        environment.extend(
            std::env::vars_os().filter(|(key, _)| key == "LD_LIBRARY_PATH" || key == "SYSTEMROOT"),
        );
        self.start_native(
            broker,
            Some(json!(true)),
            deadline,
            NativeBinding::Kimi,
            None,
            environment,
        )
        .await
    }
    fn kimi_mode(&self, mode: &str) {
        std::fs::write(self.temp.path().join("kimi-mode"), mode).unwrap();
    }
    fn kimi_events(&self) -> Vec<Value> {
        match std::fs::read_to_string(self.temp.path().join("kimi-journal")) {
            Ok(text) => text
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => panic!("native journal {:?}", e.kind()),
        }
    }
    fn kimi_launches(&self) -> Vec<Value> {
        self.kimi_events()
            .into_iter()
            .filter(|e| e["stage"] == "spawn")
            .collect()
    }
    fn assert_kimi_joined(&self) {
        for event in self.kimi_launches() {
            let pid = event["pid"].as_u64().unwrap();
            assert_eq!(
                std::fs::metadata(format!("/proc/{pid}"))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }
    async fn release_kimi(&self) -> Result<reqwest::Response, reqwest::Error> {
        let port = std::fs::read_to_string(self.temp.path().join("kimi-port")).unwrap();
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://127.0.0.1:{port}/fixture/release"))
            .bearer_auth(peer::TOKEN)
            .send()
            .await?
            .error_for_status()
    }
}
async fn kimi_call(
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
fn kimi_incoming_continuity_cold_reopen_selected_binding_and_user_isolation() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        for restart in 0..4 {
            let owner = f.start_kimi(&broker, None).await;
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
                let launches = f.kimi_launches().len();
                let query = match restart {
                    2 => "unrelated generation",
                    3 => "selected change",
                    _ => "literal question β",
                };
                let result = kimi_call(&client, operation, "kimi-A", query).await;
                if restart == 3 {
                    assert_eq!(result.is_error, Some(true));
                    assert_eq!(f.kimi_launches().len(), launches);
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
                assert_eq!(payload["persistence"]["conversation"]["engine"], "kimi");
                assert!(
                    payload["persistence"]["conversation"]
                        .get("native_id")
                        .is_none()
                );
                assert_eq!(payload["receipt"]["completion"]["answer"], peer::ANSWER);
                assert!(payload["receipt"].get("session_id").is_none());
            }
            if restart == 1 {
                let other =
                    rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
                assert_eq!(
                    kimi_call(&other, "continue", "kimi-A", "foreign")
                        .await
                        .is_error,
                    Some(true)
                );
                other.cancel().await.unwrap();
                let launches = f.kimi_launches().len();
                broker.state.allowed.store(false, Ordering::SeqCst);
                assert_eq!(
                    kimi_call(&client, "continue", "kimi-A", "denied")
                        .await
                        .is_error,
                    Some(true)
                );
                assert_eq!(f.kimi_launches().len(), launches);
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
        let launches = f.kimi_launches();
        assert_eq!(launches.len(), 4);
        assert_eq!(
            f.kimi_events()
                .iter()
                .filter(|e| e["stage"] == "session")
                .count(),
            1
        );
        let resumes: Vec<_> = f
            .kimi_events()
            .into_iter()
            .filter(|e| e["stage"] == "resume")
            .collect();
        assert_eq!(resumes.len(), 3);
        assert!(resumes.iter().all(|e| e["id"] == peer::SESSION));
        for launch in &launches {
            assert_eq!(launch["captured"], "literal-captured");
            assert_eq!(launch["cwd"], f.project.root().to_str().unwrap());
        }
        f.assert_kimi_joined();
        broker.shutdown().await;
    });
}

#[test]
fn kimi_native_session_and_turn_failures_never_create_replacement_or_publish_answer() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        for mode in [
            "answer",
            "missing_session",
            "wrong_session",
            "wrong_cwd",
            "native_error",
            "completed_error",
            "error",
            "permission",
            "no_terminal",
            "gap",
            "nonzero",
            "answer",
        ] {
            f.kimi_mode(mode);
            let owner = f.start_kimi(&broker, None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let creates = f
                .kimi_events()
                .iter()
                .filter(|e| e["stage"] == "session")
                .count();
            let operation = if creates == 0 { "create" } else { "continue" };
            let result = kimi_call(&client, operation, "kimi-failures", "literal query").await;
            assert_eq!(
                result.is_error,
                Some(mode != "answer"),
                "{mode}: {:?}",
                result.structured_content
            );
            let payload = result.structured_content.unwrap();
            if mode == "answer" {
                assert_eq!(payload["persistence"]["status"], "persisted");
                assert_eq!(payload["receipt"]["completion"]["answer"], peer::ANSWER);
            } else {
                assert!(payload["receipt"]["completion"].get("answer").is_none());
                assert!(payload["receipt"]["native_result_json"].is_null());
            }
            if operation == "continue" {
                assert_eq!(
                    f.kimi_events()
                        .iter()
                        .filter(|e| e["stage"] == "session")
                        .count(),
                    creates
                );
            }
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            f.assert_kimi_joined();
        }
        assert_eq!(
            f.kimi_events()
                .iter()
                .filter(|e| e["stage"] == "session")
                .count(),
            1
        );
        broker.shutdown().await;
    });
}
#[test]
fn kimi_incoming_progress_precedes_exit_and_actual_cleanup_releases_writer() {
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
            f.kimi_mode("gate");
            let owner = f
                .start_kimi(&broker, (mode == "deadline").then_some(2000))
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
                    args("create", "kimi-gated", "wait for release")
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            params.meta = Some(
                serde_json::from_value(json!({"progressToken":"kimi-real-progress"})).unwrap(),
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
                while !progress.0.lock().unwrap().iter().any(|message| {
                    message.contains("assistant.delta") && message.contains("actual β")
                }) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(f.kimi_launches().len(), 1);
            let user = kiro_clear_user(&f, &broker);
            let writer = user.runtime_root().join("artifact-store.lock");
            assert!(
                graphit_core::file_lock::FileLock::is_locked(&writer).unwrap(),
                "{mode}: writer released before exit"
            );
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(b"kimi-gated")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let target = user
                .user_root()
                .unwrap()
                .join(format!("graphit-ai-conversation-{digest}.json"));
            assert!(!target.exists(), "provisional native identity persisted");
            let moved = f.temp.path().join("kimi-moved-project");
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
                    .cancel(Some("cancel actual Kimi fixture".into()))
                    .await
                    .unwrap();
            } else {
                // Keep the actual request handle alive until the chosen terminal condition.
                if mode == "shutdown" {
                    stop.cancel();
                }
                if mode != "deadline" && mode != "shutdown" {
                    match f.release_kimi().await {
                        Ok(response) => assert_eq!(response.status(), reqwest::StatusCode::OK),
                        // A denied project can retire the native child before
                        // its test-only release request reaches the socket.
                        Err(error) if mode == "physical-denied" => {
                            assert!(error.is_connect() || error.is_request(), "{error}");
                        }
                        Err(error) => panic!("Kimi fixture release failed: {error}"),
                    }
                }
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
            f.assert_kimi_joined();
            broker.shutdown().await;
        }
    });
}
