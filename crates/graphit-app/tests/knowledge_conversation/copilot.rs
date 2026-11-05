use super::*;
const ANSWER: &str = "Native Copilot documentary answer 🦀\n";
fn assert_joined(value: &Value) {
    assert_eq!(
        value["receipt"]["process"]["cleanup"], "Complete",
        "{value}"
    );
}
fn assert_success_process(value: &Value) {
    assert_joined(value);
    assert_eq!(value["receipt"]["process"]["status_success"], true);
    assert_eq!(value["receipt"]["process"]["exit_code"], 0);
    assert_eq!(value["receipt"]["process"]["stdout_end"], "Eof");
    assert_eq!(value["receipt"]["process"]["stderr_end"], "Eof");
}
async fn release(f: &Fixture) -> Result<reqwest::Response, reqwest::Error> {
    let path = f.temp.path().join("copilot-gate-port");
    let port = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(port) = text.parse::<u16>()
            {
                break port;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!("http://127.0.0.1:{port}/fixture/release"))
        .send()
        .await
}

impl Fixture {
    async fn start_copilot(&self, broker: &Broker, deadline_ms: Option<u64>) -> ForegroundSource {
        let program = self.temp.path().join("copilot");
        std::fs::copy(env!("CARGO_BIN_EXE_app-copilot-fixture"), program).unwrap();
        let browser = json!({"providers":["broker"],"project_endpoint":format!("{}/v1/project/access/authorize",broker.state.origin),
            "allow_loopback_http":true,"knowledge_conversations":true});
        let mut values = vec![
            (ConfigKey::new("task.browser").unwrap(), browser),
            (ConfigKey::new("ai.cli").unwrap(), json!("copilot")),
        ];
        if let Some(milliseconds) = deadline_ms {
            values.push((
                ConfigKey::new("hub.limits.request.deadline_ms").unwrap(),
                json!(milliseconds),
            ));
        }
        let config = ConfigSnapshot::new(
            self.paths.brand(),
            [ConfigDocument::new(self.paths.brand(), ConfigSource::Global, values).unwrap()],
        )
        .unwrap();
        let mut environment = vec![
            ("PATH".into(), self.temp.path().as_os_str().to_owned()),
            ("COPILOT_MODE".into(), "answer".into()),
            (
                "COPILOT_SESSION_STORE".into(),
                self.temp
                    .path()
                    .join("copilot-native-session")
                    .into_os_string(),
            ),
            (
                "COPILOT_CONVERSATION_JOURNAL".into(),
                self.temp
                    .path()
                    .join("copilot-native-events")
                    .into_os_string(),
            ),
            (
                "COPILOT_CONTROL_MODE".into(),
                self.temp.path().join("copilot-mode").into_os_string(),
            ),
            (
                "COPILOT_GATE_PORT".into(),
                self.temp.path().join("copilot-gate-port").into_os_string(),
            ),
        ];
        environment.extend(
            std::env::vars_os().filter(|(key, _)| key == "LD_LIBRARY_PATH" || key == "SYSTEMROOT"),
        );
        let cancel = Cancellation::default();
        let original = context(&cancel);
        ForegroundSource::start_and_publish(
            &self.paths,
            &[ProjectBootstrapInput {
                project: &self.project,
                explicit_version: "",
                pin: None,
                detached_base: "",
            }],
            0,
            &config,
            NativeBinding::Codex,
            environment,
            &original,
        )
        .await
        .unwrap()
    }
    fn copilot_mode(&self, mode: &str) {
        let _ = std::fs::remove_file(self.temp.path().join("copilot-gate-port"));
        std::fs::write(self.temp.path().join("copilot-mode"), mode).unwrap();
    }
    fn copilot_events(&self) -> Vec<Value> {
        match std::fs::read_to_string(self.temp.path().join("copilot-native-events")) {
            Ok(text) => text
                .split_inclusive('\n')
                .filter(|line| line.ends_with('\n'))
                .map(|line| serde_json::from_str(line).unwrap())
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("native journal {:?}", error.kind()),
        }
    }
}
async fn call(
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
async fn metadata_call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, impl rmcp::ClientHandler>,
    value: Value,
) -> rmcp::model::CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(value.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
}
fn assert_private_success(result: rmcp::model::CallToolResult, native_id: &str) {
    assert_eq!(
        result.is_error,
        Some(false),
        "{:?}",
        result.structured_content
    );
    let value = result.structured_content.unwrap();
    assert_eq!(value["persistence"]["status"], "persisted");
    assert_eq!(value["persistence"]["conversation"]["engine"], "copilot");
    assert_eq!(value["receipt"]["completion"]["answer"], ANSWER);
    assert_success_process(&value);
    assert_eq!(value["receipt"]["native_result"]["type"], "result");
    assert!(value["receipt"]["native_result"].get("sessionId").is_none());
    assert!(!value.to_string().contains(native_id));
    assert!(!value.to_string().contains("tool-canary"));
    assert!(value["receipt"].get("session_id").is_none());
    assert!(
        !value["persistence"]["conversation"]
            .to_string()
            .contains(native_id)
    );
    assert!(
        !value["persistence"]["conversation"]
            .to_string()
            .contains("native_id")
    );
}
#[test]
fn copilot_incoming_create_continue_cold_reopen_and_fresh_binding_are_private() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        let mut native_id = String::new();
        for restart in 0..3 {
            let owner = f.start_copilot(&broker, None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            if restart == 0 {
                let result = call(&client, "create", "copilot-A", "literal first question").await;
                native_id =
                    std::fs::read_to_string(f.temp.path().join("copilot-native-session")).unwrap();
                assert_private_success(result, &native_id);
                assert_private_success(
                    call(&client, "continue", "copilot-A", "literal next question").await,
                    &native_id,
                );
                let other =
                    rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
                let before = f.copilot_events().len();
                assert_eq!(
                    call(&other, "continue", "copilot-A", "foreign user")
                        .await
                        .is_error,
                    Some(true)
                );
                assert_eq!(f.copilot_events().len(), before);
                other.cancel().await.unwrap();
                let denied = metadata_call(&client, clear_args("copilot-A")).await;
                assert_eq!(denied.is_error, Some(true));
            } else if restart == 1 {
                f.copilot_mode("answer");
                assert_private_success(
                    call(
                        &client,
                        "continue",
                        "copilot-A",
                        "cold reopen after unrelated publication",
                    )
                    .await,
                    &native_id,
                );
            } else {
                let before = f.copilot_events().len();
                assert_eq!(
                    call(
                        &client,
                        "continue",
                        "copilot-A",
                        "changed selected document"
                    )
                    .await
                    .is_error,
                    Some(true)
                );
                assert_eq!(f.copilot_events().len(), before);
                let listed =
                    metadata_call(&client, json!({"operation":"list","provider":"broker"})).await;
                assert_eq!(listed.is_error, Some(false));
                let deleted = metadata_call(
                    &client,
                    json!({"operation":"delete","provider":"broker","conversation_id":"copilot-A"}),
                )
                .await;
                assert_eq!(deleted.is_error, Some(false));
                assert!(f.temp.path().join("copilot-native-session").exists());
            }
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            match restart {
                0 => f.republish(DocumentChange::Unrelated).await,
                1 => f.republish(DocumentChange::Selected).await,
                _ => {}
            }
        }
        let events = f.copilot_events();
        let creates: Vec<_> = events.iter().filter(|v| v["op"] == "create").collect();
        let resumes: Vec<_> = events.iter().filter(|v| v["op"] == "resume").collect();
        assert_eq!(creates.len(), 1);
        assert_eq!(resumes.len(), 2);
        for event in creates.into_iter().chain(resumes) {
            assert!(event["session"].as_str() == Some(native_id.as_str()));
            assert_eq!(event["model"], "fixture-model");
            assert_eq!(event["cwd"], f.project.root().to_str().unwrap());
        }
        broker.shutdown().await;
    });
}
#[test]
fn copilot_resume_result_mismatch_missing_session_and_save_failure_never_commit_or_create_replacement()
 {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start_copilot(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        assert_eq!(
            call(&client, "create", "copilot-good", "initial")
                .await
                .is_error,
            Some(false)
        );
        let user = kiro_clear_user(&f, &broker);
        use sha2::{Digest, Sha256};
        let key = Sha256::digest(b"copilot-good")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let record = user
            .user_root()
            .unwrap()
            .join(format!("graphit-ai-conversation-{key}.json"));
        let bytes = std::fs::read(&record).unwrap();
        for mode in [
            "resume_mismatch",
            "nonzero_after_result",
            "resume_error",
            "missing_native",
        ] {
            f.copilot_mode(if mode == "missing_native" {
                "answer"
            } else {
                mode
            });
            if mode == "missing_native" {
                std::fs::remove_file(f.temp.path().join("copilot-native-session")).unwrap();
            }
            let before = f
                .copilot_events()
                .iter()
                .filter(|v| v["op"] == "prompt")
                .count();
            let result = call(&client, "continue", "copilot-good", "failed continuation").await;
            assert_eq!(result.is_error, Some(true));
            let payload = result.structured_content.unwrap();
            assert!(payload["receipt"]["completion"].get("answer").is_none());
            assert!(payload["receipt"].get("native_result").is_none());
            assert_eq!(
                f.copilot_events()
                    .iter()
                    .filter(|v| v["op"] == "prompt")
                    .count(),
                before + usize::from(matches!(mode, "resume_mismatch" | "nonzero_after_result"))
            );
            assert_eq!(std::fs::read(&record).unwrap(), bytes);
            assert!(
                !payload
                    .to_string()
                    .contains("private-fixture-missing-session")
            );
        }
        assert_eq!(
            f.copilot_events()
                .iter()
                .filter(|v| v["op"] == "create")
                .count(),
            1
        );
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn copilot_post_effect_denial_keeps_prefix_and_blocks_user_commit_and_delivery() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.copilot_mode("hold");
        let owner = f.start_copilot(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
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
                args("create", "copilot-denied", "authorized before invalidation")
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        params.meta = Some(
            serde_json::from_value(json!({"progressToken":"copilot-controlled-progress"})).unwrap(),
        );
        let peer = client.peer().clone();
        let call = tokio::spawn(async move { peer.call_tool(params).await.unwrap() });
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if progress
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|m| m.contains("tool.execution_complete"))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        for event in progress.0.lock().unwrap().iter() {
            let value: Value = serde_json::from_str(event).unwrap();
            assert!(!value.to_string().contains("tool-canary"));
            assert!(!value.to_string().contains("sessionId"));
        }
        let user = kiro_clear_user(&f, &broker);
        let writer = user.runtime_root().join("artifact-store.lock");
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        let moved = f.temp.path().join("copilot-moved-project");
        std::fs::rename(f.project.root(), &moved).unwrap();
        let gate_release = release(&f).await;
        let result = call.await.unwrap();
        std::fs::rename(&moved, f.project.root()).unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert!(
            payload["receipt"]["accepted_notifications"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert!(payload["receipt"].get("native_result").is_none());
        assert!(payload["persistence"].get("conversation").is_none());
        assert_joined(&payload);
        assert!(!graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        // Admission can retire the child before the control request arrives.
        match gate_release {
            Ok(response) => assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT),
            Err(error) => assert!(error.is_connect() || error.is_request()),
        }
        f.copilot_mode("answer");
        assert_eq!(
            self::call(&client, "create", "copilot-denied", "fresh admitted retry")
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
fn copilot_original_deadline_during_native_turn_never_persists_partial_answer() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.copilot_mode("hold");
        let owner = f.start_copilot(&broker, Some(5000)).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let result = call(&client, "create", "copilot-deadline", "held turn").await;
        assert!(
            f.copilot_events()
                .iter()
                .any(|event| event["op"] == "prompt")
        );
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.is_none());
        assert!(serde_json::to_string(&result).unwrap().contains("deadline"));
        let user = kiro_clear_user(&f, &broker);
        let writer = user.runtime_root().join("artifact-store.lock");
        use sha2::{Digest, Sha256};
        let key = Sha256::digest(b"copilot-deadline")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let record = user
            .user_root()
            .unwrap()
            .join(format!("graphit-ai-conversation-{key}.json"));
        assert!(!record.exists());
        client.cancel().await.unwrap();
        cancel.cancel();
        // Source shutdown observes retained native custody before returning.
        running.await.unwrap().unwrap();
        assert!(!record.exists());
        assert!(!graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        broker.shutdown().await;
    });
}

#[test]
fn copilot_actual_user_destination_conflict_suppresses_answer_after_clean_native_exit() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.copilot_mode("hold");
        let owner = f.start_copilot(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let peer = client.peer().clone();
        let pending = tokio::spawn(async move {
            peer.call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("create", "copilot-conflict", "held before USER conflict")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            while !f.copilot_events().iter().any(|v| v["op"] == "prompt") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let user = kiro_clear_user(&f, &broker);
        use sha2::{Digest, Sha256};
        let key = Sha256::digest(b"copilot-conflict")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let record = user
            .user_root()
            .unwrap()
            .join(format!("graphit-ai-conversation-{key}.json"));
        std::fs::write(&record, b"foreign-owned-destination").unwrap();
        std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o600)).unwrap();
        let gate_release = release(&f).await;
        if let Ok(response) = gate_release {
            response.error_for_status().unwrap();
        }
        let result = pending.await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["persistence"]["status"], "failed");
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert!(payload["receipt"].get("native_result").is_none());
        assert_success_process(&payload);
        assert_eq!(
            std::fs::read(&record).unwrap(),
            b"foreign-owned-destination"
        );
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                user.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn copilot_project_changed_after_cli_launch_blocks_commit_and_delivery() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.copilot_mode("start_hold");
        let owner = f.start_copilot(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let peer = client.peer().clone();
        let pending = tokio::spawn(async move {
            peer.call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args(
                        "create",
                        "copilot-start-denied",
                        "launched before project moved",
                    )
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
            )
            .await
            .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            while !f.copilot_events().iter().any(|v| v["op"] == "start") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let moved = f.temp.path().join("copilot-start-moved-project");
        std::fs::rename(f.project.root(), &moved).unwrap();
        let gate_release = release(&f).await;
        if let Ok(response) = gate_release {
            response.error_for_status().unwrap();
        }
        let result = pending.await.unwrap();
        std::fs::rename(&moved, f.project.root()).unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert!(payload["persistence"].get("conversation").is_none());
        assert_joined(&payload);
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                kiro_clear_user(&f, &broker)
                    .runtime_root()
                    .join("artifact-store.lock")
            )
            .unwrap()
        );
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn copilot_user_scope_changed_after_cli_launch_blocks_create_and_resume_commit() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start_copilot(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        // Admit the guarded turns against an existing, physically captured USER namespace.
        assert_eq!(
            call(
                &client,
                "create",
                "copilot-scope-seed",
                "legitimate USER namespace birth"
            )
            .await
            .is_error,
            Some(false)
        );
        for operation in ["create", "continue"] {
            f.copilot_mode("start_hold");
            let before = f.copilot_events().len();
            let peer = client.peer().clone();
            let pending = tokio::spawn(async move {
                peer.call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args(
                            operation,
                            "copilot-user-guard",
                            "launched before USER moved",
                        )
                        .as_object()
                        .unwrap()
                        .clone(),
                    ),
                )
                .await
                .unwrap()
            });
            tokio::time::timeout(Duration::from_secs(15), async {
                while !f.copilot_events()[before..]
                    .iter()
                    .any(|v| v["op"] == "start")
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let user = kiro_clear_user(&f, &broker);
            let user_root = user.user_root().unwrap().to_owned();
            let moved = f.temp.path().join("copilot-moved-user");
            std::fs::rename(&user_root, &moved).unwrap();
            assert!(f.project.root().exists());
            let gate_release = release(&f).await;
            if let Ok(response) = gate_release {
                response.error_for_status().unwrap();
            }
            let result = pending.await.unwrap();
            std::fs::rename(&moved, &user_root).unwrap();
            assert_eq!(result.is_error, Some(true));
            // The CLI may consume its piped prompt after launch. Denial must prevent
            // publication and USER persistence, without inventing a pre-prompt ACK.
            let payload = result.structured_content.unwrap();
            assert!(payload["receipt"]["completion"].get("answer").is_none());
            assert!(payload["persistence"].get("conversation").is_none());
            assert_joined(&payload);
            f.copilot_mode("answer");
            assert_eq!(
                call(
                    &client,
                    operation,
                    "copilot-user-guard",
                    "fresh valid USER turn"
                )
                .await
                .is_error,
                Some(false)
            );
        }
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}
