use super::*;

#[tokio::test]
async fn root_native_hosts_use_the_shared_codec_and_current_project_admission() {
    let fixture = configured(
        &[graphit_auth::Permission::Read(Module::Memory)],
        json!({"task":false}),
    );
    let (connection, serving) = client(&fixture).await;
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    for engine in Engine::ALL {
        for event in Event::ALL {
            let Ok(true) = event.semantic().accepts_context(engine.binding()) else {
                continue;
            };
            let mut arguments = crate::session_hook_entry::Arguments {
                recipient: crate::session_hook_entry::Recipient::Coordinator,
                engine,
                event,
                project_dir: Some(fixture.projects[0].0.root().to_owned()),
                task_session: None,
                task_ids: Vec::new(),
            };
            let payload = match engine {
                Engine::Cursor => {
                    json!({"hook_event_name":event.semantic().native_name(engine.binding()).unwrap(),"workspace_roots":[fixture.projects[0].0.root()],"conversation_id":"not-a-Graphit-session"})
                }
                Engine::Antigravity => {
                    json!({"workspacePaths":[fixture.projects[0].0.root()],"conversationId":"not-a-Graphit-session","invocationNum":0})
                }
                Engine::OpenCode => json!({"sessionID":"not-a-Graphit-session","messages":[]}),
                _ => {
                    json!({"hook_event_name":event.semantic().native_name(engine.binding()).unwrap(),"cwd":fixture.projects[0].0.root(),"session_id":"not-a-Graphit-session"})
                }
            };
            let input = serde_json::to_vec(&payload).unwrap();
            crate::session_hook_entry::read_host_input(&mut arguments, input.as_slice(), &original)
                .await
                .unwrap();
            let prepared = crate::session_hook_entry::prepare(
                &fixture.paths,
                &arguments,
                Some(fixture.projects[0].2.version()),
                &original,
            )
            .unwrap();
            assert!(
                prepared
                    .arguments
                    .as_ref()
                    .unwrap()
                    .get("task_context")
                    .is_none()
            );
            let result = connection.call_tool(prepared).await.unwrap();
            assert_eq!(result.is_error, Some(false), "{engine:?}/{event:?}");
            let wire = &result.content[0].as_text().unwrap().text;
            let body = match engine {
                Engine::Codex | Engine::Claude | Engine::Qwen => {
                    let value: Value = serde_json::from_str(wire).unwrap();
                    assert_eq!(
                        value["hookSpecificOutput"]["hookEventName"],
                        event.semantic().native_name(engine.binding()).unwrap()
                    );
                    value["hookSpecificOutput"]["additionalContext"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                }
                Engine::Cursor => {
                    serde_json::from_str::<Value>(wire).unwrap()["additional_context"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                }
                Engine::Antigravity => serde_json::from_str::<Value>(wire).unwrap()["injectSteps"]
                    [0]["ephemeralMessage"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                Engine::Kiro | Engine::Kimi | Engine::OpenCode => wire.clone(),
            };
            assert!(body.contains("PROJECT Memory state: no_corpus."));
            assert!(!body.contains("not-a-Graphit-session"));
        }
    }
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn quiet_native_boundaries_do_not_require_record_permissions() {
    let fixture = configured(&[], json!({}));
    let (connection, serving) = client(&fixture).await;
    for (engine, event, expected) in [
        ("kimi", "SessionStart", ""),
        ("kimi", "SubagentStart", ""),
        ("kimi", "PostToolUse", ""),
        ("antigravity", "PostToolUse", "{}"),
    ] {
        let result = connection
            .call_tool(request(json!({"engine":engine,"event":event})))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.content[0].as_text().unwrap().text, expected);
    }
    let denied = connection
        .call_tool(request(json!({"engine":"claude","event":"SessionStart"})))
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    let unsupported = connection
        .call_tool(request(json!({"engine":"gemini","event":"Stop"})))
        .await
        .unwrap();
    assert_eq!(unsupported.is_error, Some(true));
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn stop_and_session_end_dispatch_exact_owned_rescans_with_configure_authority() {
    use graphit_daemon::{
        supervisor::{ProjectContext, ProjectSupervisor},
        watcher::{IgnoreConfig, WatchConfig},
    };
    let mut fixture = crate::source_server::test_fixture::fixture(
        &[graphit_auth::Permission::Configure],
        Brand::default(),
        NativeBinding::KiroV3,
    );
    let config = ConfigSnapshot::new(fixture.paths.brand(), []).unwrap();
    let content = fixture.server.0.policy.native_install.hub.content();
    Arc::get_mut(&mut fixture.server.0).unwrap().policy.content = content;
    fixture.server.attach_role_tools(config).unwrap();
    let context = ProjectContext {
        id: fixture.projects[0].1,
        version: fixture.projects[0].2.clone(),
    };
    let ignore = IgnoreConfig {
        filename: None,
        defaults: vec!["**".into()],
    };
    let (sent, mut batches) = tokio::sync::mpsc::channel(1);
    let owner = ProjectSupervisor::start(
        context.clone(),
        WatchConfig {
            root: context.root().to_owned(),
            ast: ignore.clone(),
            wiki: ignore,
            control_files: Vec::new(),
            control_directories: Vec::new(),
            debounce: Duration::from_millis(20),
        },
        move |_, batch, _| {
            let sent = sent.clone();
            async move { sent.send(batch).await.map_err(io::Error::other) }
        },
    )
    .await
    .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(5), batches.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(first.rescan);
    fixture
        .server
        .attach_memory_rescans(Arc::from([owner.rescan_handle()]))
        .unwrap();
    let (connection, serving) = client(&fixture).await;
    for engine in Engine::ALL {
        for event in [Event::Stop, Event::SessionEnd] {
            if event.semantic().native_name(engine.binding()).is_err() {
                continue;
            }
            let prepared = json!({"engine":engine,"event":event});
            let result = connection.call_tool(request(prepared)).await.unwrap();
            assert_eq!(result.is_error, Some(false), "{engine:?}/{event:?}");
            if matches!(engine, Engine::Antigravity) {
                assert_eq!(
                    serde_json::from_str::<Value>(&result.content[0].as_text().unwrap().text)
                        .unwrap(),
                    json!({"decision":"stop"})
                );
            } else {
                assert!(result.content[0].as_text().unwrap().text.is_empty());
            }
            let batch = tokio::time::timeout(Duration::from_secs(5), batches.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(batch.rescan);
            assert!(batch.paths.is_empty());
            assert!(batch.error.is_none());
        }
    }
    owner.stop().await.unwrap();
    let stopped = connection
        .call_tool(request(json!({"engine":"claude","event":"Stop"})))
        .await
        .unwrap();
    assert_eq!(stopped.is_error, Some(true));
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    let denied = configured(&[], json!({}));
    let (connection, serving) = client(&denied).await;
    let failure = connection
        .call_tool(request(json!({"engine":"claude","event":"Stop"})))
        .await
        .unwrap();
    assert_eq!(failure.is_error, Some(true));
    assert!(
        failure.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("rescan permission denied")
    );
    connection.cancel().await.unwrap();
    denied.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
