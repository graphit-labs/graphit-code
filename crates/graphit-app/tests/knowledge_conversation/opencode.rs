use super::*;

const MODEL: &str = "provider/literal;$(opencode)";
const QUESTION: &str = "  literal β\n  ";
const ANSWER: &str = "  actual β [[Paris]]\n\t";

impl Fixture {
    fn opencode_path(&self, name: &str) -> PathBuf {
        self.temp.path().join(format!("opencode-{name}"))
    }
    async fn start_opencode(&self, broker: &Broker, deadline_ms: Option<u64>) -> ForegroundSource {
        let program = self.temp.path().join("opencode");
        std::fs::copy(env!("CARGO_BIN_EXE_opencode-knowledge-fixture"), &program).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut environment = vec![
            ("PATH".into(), self.temp.path().as_os_str().to_owned()),
            (
                "OPENCODE_CAPTURED".into(),
                "startup literal;$(value)".into(),
            ),
            (
                "OPENCODE_CONTROL_MODE".into(),
                self.opencode_path("mode").into_os_string(),
            ),
            (
                "OPENCODE_SESSION_STORE".into(),
                self.opencode_path("session").into_os_string(),
            ),
            (
                "OPENCODE_JOURNAL".into(),
                self.opencode_path("journal").into_os_string(),
            ),
        ];
        environment.extend(
            std::env::vars_os().filter(|(key, _)| key == "LD_LIBRARY_PATH" || key == "SYSTEMROOT"),
        );
        self.start_native(
            broker,
            Some(json!(true)),
            deadline_ms,
            NativeBinding::Codex,
            Some(graphit_ai::cli_provider::CliCompletionProvider::OpenCode),
            environment,
        )
        .await
    }
    fn opencode_mode(&self, mode: &str) {
        for extension in ["finish", "initialize.finish", "load.finish"] {
            let path = self.opencode_path("journal").with_extension(extension);
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("fixture control {:?}", error.kind()),
            }
        }
        std::fs::write(self.opencode_path("mode"), mode).unwrap();
    }
    fn opencode_events(&self) -> Vec<String> {
        match std::fs::read_to_string(self.opencode_path("journal")) {
            Ok(text) => text.lines().map(str::to_owned).collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("native journal {:?}", error.kind()),
        }
    }
    async fn opencode_wait(&self, from: usize, event: &str) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while !self.opencode_events()[from..].iter().any(|e| e == event) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    fn opencode_release(&self, extension: &str) {
        std::fs::write(
            self.opencode_path("journal").with_extension(extension),
            b"release",
        )
        .unwrap();
    }
}
fn opencode_args(operation: &str, id: &str) -> Value {
    let mut value = args(operation, id, QUESTION);
    value["model"] = json!(MODEL);
    value
}
async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, impl rmcp::ClientHandler>,
    operation: &str,
    id: &str,
) -> rmcp::model::CallToolResult {
    metadata(client, opencode_args(operation, id)).await
}
async fn metadata(
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
fn record(f: &Fixture, broker: &Broker, id: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let key = Sha256::digest(id.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    kiro_clear_user(f, broker)
        .user_root()
        .unwrap()
        .join(format!("graphit-ai-conversation-{key}.json"))
}
fn writer(f: &Fixture, broker: &Broker) -> PathBuf {
    kiro_clear_user(f, broker)
        .runtime_root()
        .join("artifact-store.lock")
}
fn assert_failed(result: &rmcp::model::CallToolResult) {
    assert_eq!(result.is_error, Some(true));
    if let Some(value) = &result.structured_content {
        assert!(value["receipt"]["completion"].get("answer").is_none());
        assert!(value["receipt"].get("native_result_json").is_none());
        assert!(value["persistence"].get("conversation").is_none());
    }
}
fn assert_success(result: rmcp::model::CallToolResult, native_id: &str, loaded: bool) {
    assert_eq!(
        result.is_error,
        Some(false),
        "{:?}",
        result.structured_content
    );
    let value = result.structured_content.unwrap();
    assert_eq!(value["persistence"]["status"], "persisted");
    assert_eq!(value["persistence"]["conversation"]["engine"], "opencode");
    assert_eq!(value["receipt"]["completion"]["answer"], ANSWER);
    assert_eq!(
        value["receipt"]["replayed_notifications"],
        if loaded { 3 } else { 0 }
    );
    assert_eq!(value["receipt"]["accepted_notifications"], 1);
    assert_eq!(value["receipt"]["sdk_driver_completed"], true);
    assert_eq!(value["receipt"]["process"]["joined"], true);
    assert!(!value.to_string().contains(native_id));
    assert!(!value.to_string().contains("private historical"));
}
async fn progress_client(
    uri: &str,
    raw: &str,
    progress: &ProgressCollector,
) -> rmcp::service::RunningService<rmcp::RoleClient, ProgressCollector> {
    let mut headers = HashMap::new();
    let mut bearer = header::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, bearer);
    progress
        .clone()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::builder().no_proxy().build().unwrap(),
                StreamableHttpClientTransportConfig::with_uri(uri.to_owned())
                    .custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap()
}
fn request(operation: &str, id: &str) -> CallToolRequestParams {
    let mut params = CallToolRequestParams::new("graphit_knowledge_conversation")
        .with_arguments(opencode_args(operation, id).as_object().unwrap().clone());
    params.meta =
        Some(serde_json::from_value(json!({"progressToken":"opencode-current-turn"})).unwrap());
    params
}

#[test]
fn opencode_incoming_create_continue_cold_load_and_metadata_keep_native_identity_private() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.opencode_mode("answer");
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        let mut native_id = String::new();
        for restart in 0..3 {
            let owner = f.start_opencode(&broker, None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let progress = ProgressCollector(Arc::new(std::sync::Mutex::new(Vec::new())));
            let client = progress_client(&uri, &raw, &progress).await;
            if restart == 0 {
                let created = client
                    .call_tool(request("create", "opencode-A"))
                    .await
                    .unwrap();
                native_id = std::fs::read_to_string(f.opencode_path("session")).unwrap();
                assert_success(created, &native_id, false);
                assert_success(
                    client
                        .call_tool(request("continue", "opencode-A"))
                        .await
                        .unwrap(),
                    &native_id,
                    true,
                );
                let before = f.opencode_events().len();
                let other =
                    rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
                assert_failed(&call(&other, "continue", "opencode-A").await);
                other.cancel().await.unwrap();
                let mut changed = opencode_args("continue", "opencode-A");
                changed["model"] = json!("provider/changed");
                assert_failed(&metadata(&client, changed).await);
                let mut clear = opencode_args("clear", "opencode-A");
                clear.as_object_mut().unwrap().remove("query");
                clear.as_object_mut().unwrap().remove("selected_slugs");
                assert_failed(&metadata(&client, clear).await);
                assert_eq!(f.opencode_events().len(), before);
            } else if restart == 1 {
                assert_success(
                    client
                        .call_tool(request("continue", "opencode-A"))
                        .await
                        .unwrap(),
                    &native_id,
                    true,
                );
            } else {
                let before = f.opencode_events().len();
                assert_failed(&call(&client, "continue", "opencode-A").await);
                assert_eq!(f.opencode_events().len(), before);
                let listed =
                    metadata(&client, json!({"operation":"list","provider":"broker"})).await;
                assert_eq!(listed.is_error, Some(false));
                assert!(!serde_json::to_string(&listed).unwrap().contains(&native_id));
                let deleted = metadata(&client, json!({"operation":"delete","provider":"broker","conversation_id":"opencode-A"})).await;
                assert_eq!(deleted.is_error, Some(false));
                assert!(f.opencode_path("session").exists());
            }
            let messages = progress.0.lock().unwrap().clone();
            assert!(
                !messages
                    .iter()
                    .any(|m| m.contains("private historical") || m.contains("historical_tokens"))
            );
            if restart < 2 {
                assert!(messages.iter().any(|m| m.contains("actual β")));
            }
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            if restart == 0 {
                f.republish(DocumentChange::Unrelated).await;
            }
            if restart == 1 {
                f.republish(DocumentChange::Selected).await;
            }
        }
        let events = f.opencode_events();
        assert_eq!(events.iter().filter(|e| *e == "new").count(), 1);
        assert_eq!(events.iter().filter(|e| *e == "load_exact").count(), 2);
        assert_eq!(
            std::fs::read_to_string(f.opencode_path("session")).unwrap(),
            native_id
        );
        broker.shutdown().await;
    });
}

#[test]
fn opencode_load_errors_capability_model_and_foreign_replay_never_create_replacement() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.opencode_mode("answer");
        let owner = f.start_opencode(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        assert_eq!(
            call(&client, "create", "opencode-errors").await.is_error,
            Some(false)
        );
        let native_id = std::fs::read(f.opencode_path("session")).unwrap();
        let private = record(&f, &broker, "opencode-errors");
        let committed = std::fs::read(&private).unwrap();
        for mode in [
            "load-missing-capability",
            "load-no-capability",
            "load-native-error",
            "load-drift",
            "load-foreign",
            "missing-native",
        ] {
            f.opencode_mode(if mode == "missing-native" {
                "answer"
            } else {
                mode
            });
            if mode == "missing-native" {
                std::fs::remove_file(f.opencode_path("session")).unwrap();
            }
            let from = f.opencode_events().len();
            let result = call(&client, "continue", "opencode-errors").await;
            assert_failed(&result);
            let events = f.opencode_events();
            assert!(
                !events[from..]
                    .iter()
                    .any(|e| matches!(e.as_str(), "new" | "select" | "prompt_literal"))
            );
            if mode.contains("capability") {
                assert!(!events[from..].iter().any(|e| e == "load"));
            }
            assert_eq!(std::fs::read(&private).unwrap(), committed);
            assert!(
                !serde_json::to_string(&result)
                    .unwrap()
                    .contains("private fixture native session unavailable")
            );
        }
        std::fs::write(f.opencode_path("session"), native_id).unwrap();
        f.opencode_mode("answer");
        assert_eq!(
            call(&client, "continue", "opencode-errors").await.is_error,
            Some(false)
        );
        assert_eq!(
            f.opencode_events().iter().filter(|e| *e == "new").count(),
            1
        );
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn opencode_user_scope_changed_during_initialize_denies_create_and_load_before_effect() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.opencode_mode("answer");
        let owner = f.start_opencode(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        assert_eq!(
            call(&client, "create", "opencode-seed").await.is_error,
            Some(false)
        );
        for operation in ["create", "continue"] {
            f.opencode_mode("initialize_gate");
            let from = f.opencode_events().len();
            let peer = client.peer().clone();
            let pending = tokio::spawn(async move {
                peer.call_tool(request(operation, "opencode-user-guard"))
                    .await
                    .unwrap()
            });
            f.opencode_wait(from, "initialize_waiting").await;
            let user_root = kiro_clear_user(&f, &broker).user_root().unwrap().to_owned();
            let moved = f.temp.path().join("opencode-moved-user");
            std::fs::rename(&user_root, &moved).unwrap();
            assert!(f.project.root().exists());
            f.opencode_release("initialize.finish");
            let result = pending.await.unwrap();
            std::fs::rename(&moved, &user_root).unwrap();
            assert_failed(&result);
            assert!(
                !f.opencode_events()[from..]
                    .iter()
                    .any(|e| matches!(e.as_str(), "new" | "load" | "select" | "prompt_literal"))
            );
            f.opencode_mode("answer");
            assert_eq!(
                call(&client, operation, "opencode-user-guard")
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

#[test]
fn opencode_project_changed_after_replay_load_ack_denies_before_select_or_prompt() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.opencode_mode("answer");
        let owner = f.start_opencode(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        assert_eq!(
            call(&client, "create", "opencode-load-denied")
                .await
                .is_error,
            Some(false)
        );
        let private = record(&f, &broker, "opencode-load-denied");
        let bytes = std::fs::read(&private).unwrap();
        f.opencode_mode("load_gate");
        let from = f.opencode_events().len();
        let peer = client.peer().clone();
        let pending = tokio::spawn(async move {
            peer.call_tool(request("continue", "opencode-load-denied"))
                .await
                .unwrap()
        });
        f.opencode_wait(from, "load_waiting").await;
        assert!(graphit_core::file_lock::FileLock::is_locked(writer(&f, &broker)).unwrap());
        let moved = f.temp.path().join("opencode-load-moved-project");
        std::fs::rename(f.project.root(), &moved).unwrap();
        f.opencode_release("load.finish");
        let result = pending.await.unwrap();
        std::fs::rename(&moved, f.project.root()).unwrap();
        assert_failed(&result);
        assert!(
            !f.opencode_events()[from..]
                .iter()
                .any(|e| matches!(e.as_str(), "select" | "prompt_literal"))
        );
        assert_eq!(std::fs::read(&private).unwrap(), bytes);
        assert!(!graphit_core::file_lock::FileLock::is_locked(writer(&f, &broker)).unwrap());
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn opencode_post_effect_project_denial_keeps_current_prefix_and_suppresses_delivery() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.opencode_mode("gate");
        let owner = f.start_opencode(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let progress = ProgressCollector(Arc::new(std::sync::Mutex::new(Vec::new())));
        let client = progress_client(
            &uri,
            &broker.state.token("caller-A", RESOURCE, expiry()),
            &progress,
        )
        .await;
        let peer = client.peer().clone();
        let pending = tokio::spawn(async move {
            peer.call_tool(request("create", "opencode-prefix-denied"))
                .await
                .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            while !progress
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|m| m.contains("actual β"))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let lock = writer(&f, &broker);
        assert!(graphit_core::file_lock::FileLock::is_locked(&lock).unwrap());
        let moved = f.temp.path().join("opencode-prefix-moved-project");
        std::fs::rename(f.project.root(), &moved).unwrap();
        f.opencode_release("finish");
        let result = pending.await.unwrap();
        std::fs::rename(&moved, f.project.root()).unwrap();
        assert_failed(&result);
        let value = result.structured_content.unwrap();
        assert_eq!(value["receipt"]["accepted_notifications"], 1);
        assert_eq!(value["receipt"]["process"]["joined"], true);
        assert!(!record(&f, &broker, "opencode-prefix-denied").exists());
        assert!(!graphit_core::file_lock::FileLock::is_locked(lock).unwrap());
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn opencode_actual_user_destination_conflict_suppresses_clean_native_answer() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.opencode_mode("gate");
        let owner = f.start_opencode(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let peer = client.peer().clone();
        let pending = tokio::spawn(async move {
            peer.call_tool(request("create", "opencode-conflict"))
                .await
                .unwrap()
        });
        f.opencode_wait(0, "chunk").await;
        let private = record(&f, &broker, "opencode-conflict");
        std::fs::write(&private, b"foreign-owned-destination").unwrap();
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o600)).unwrap();
        f.opencode_release("finish");
        let result = pending.await.unwrap();
        assert_failed(&result);
        let value = result.structured_content.unwrap();
        assert_eq!(value["persistence"]["status"], "failed");
        assert_eq!(value["receipt"]["process"]["joined"], true);
        assert_eq!(
            std::fs::read(private).unwrap(),
            b"foreign-owned-destination"
        );
        assert!(!graphit_core::file_lock::FileLock::is_locked(writer(&f, &broker)).unwrap());
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn opencode_original_deadline_after_native_prompt_never_commits_partial_answer() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        f.opencode_mode("gate");
        let owner = f.start_opencode(&broker, Some(5000)).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let result = call(&client, "create", "opencode-deadline").await;
        assert!(f.opencode_events().iter().any(|e| e == "prompt_literal"));
        assert_failed(&result);
        assert!(serde_json::to_string(&result).unwrap().contains("deadline"));
        let private = record(&f, &broker, "opencode-deadline");
        let lock = writer(&f, &broker);
        assert!(!private.exists());
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        assert!(!private.exists());
        assert!(!graphit_core::file_lock::FileLock::is_locked(lock).unwrap());
        broker.shutdown().await;
    });
}

#[test]
fn opencode_real_cancel_and_dropped_waiter_source_shutdown_retire_tree_before_writer_release() {
    native_test(async {
        for remote_cancel in [true, false] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            f.opencode_mode("shutdown");
            let owner = f.start_opencode(&broker, None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client =
                rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
            let handle = client
                .peer()
                .send_cancellable_request(
                    rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                        request("create", "opencode-retire"),
                    )),
                    rmcp::service::PeerRequestOptions::no_options(),
                )
                .await
                .unwrap();
            f.opencode_wait(0, "descendant_ready").await;
            f.opencode_wait(0, "chunk").await;
            let lock = writer(&f, &broker);
            let private = record(&f, &broker, "opencode-retire");
            assert!(graphit_core::file_lock::FileLock::is_locked(&lock).unwrap());
            let journal = f.opencode_path("journal");
            let pid: u32 = std::fs::read_to_string(journal.with_extension("descendant.pid"))
                .unwrap()
                .parse()
                .unwrap();
            assert!(!journal.with_extension("descendant.term").exists());
            if remote_cancel {
                handle
                    .cancel(Some("controlled native cancellation".into()))
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(10), async {
                    while !journal.with_extension("descendant.term").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                // Actual remote cancellation has reached the tree; TERM-resistant
                // native custody still owns the writer during grace/retirement.
                assert!(graphit_core::file_lock::FileLock::is_locked(&lock).unwrap());
            } else {
                // Dropping the local waiter alone is not evidence of remote cancellation.
                drop(handle);
                assert!(!private.exists());
            }
            cancel.cancel();
            running.await.unwrap().unwrap();
            assert!(!private.exists());
            assert!(!graphit_core::file_lock::FileLock::is_locked(lock).unwrap());
            assert_eq!(
                std::fs::read(journal.with_extension("descendant.term")).unwrap(),
                b"TERM"
            );
            match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(stat) => assert_eq!(stat.rsplit_once(") ").unwrap().1.chars().next(), Some('Z')),
                Err(error) => assert!(
                    error.kind() == std::io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error())
                ),
            }
            // The joined descendant is absent or a zombie above; it cannot execute more writes.
            let _ = client.cancel().await;
            broker.shutdown().await;
        }
    });
}
