//! MCP forwarding uses the installed CPU model and existing native Knowledge pair.
use super::*;

#[tokio::test]
async fn live_mcp_cpu_search_preserves_project_and_imported_semantic_generations() {
    let (issuer, issuer_service) = Fixture::start_issuer().await;
    let (_storage, foreground, tasks, sources, raw, pair) = Box::pin(
        crate::foreground_source::foreground_memory_jobs_tests::live_imported_knowledge_cpu_search_fixture(&issuer.origin),
    ).await;
    crate::task_server::tests::browser_provider(&tasks, &raw, &issuer.origin, true);
    let mut fixture = Fixture::configured_with_issuer(
        policy(),
        Some((tasks.clone(), raw)),
        None,
        None,
        issuer,
        issuer_service,
    )
    .await;
    let _cookie = fixture.login().await;
    let (web, entry) = fixture
        .service
        .0
        .sessions
        .lock()
        .await
        .iter()
        .next()
        .map(|(key, value)| (key.clone(), Arc::clone(value)))
        .unwrap();
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live CPU MCP metadata fixture"));
    let resources = tasks.record_resources(&snapshot);
    let cancellation = Cancellation::default();
    let mut original = RequestContext::new(
        resources.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let rendered = render_mcp_agent(&mut original);
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = fixture
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("Live MCP Web flow fixture"));
    let (provider, verifier) = &fixture.service.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &bearer,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let identity = (&caller, verifier.as_ref(), provider.as_str());
    let issuer = resources.auth.issuer();
    let scope = crate::foreground_user_memory::capture(
        issuer,
        resources.auth.installation(),
        &caller,
        verifier,
        &original,
        resources.policy,
    )
    .unwrap();
    let binding = scope.binding().unwrap();
    let capture = || ScopeCapture {
        live: &scope,
        record: &binding,
    };
    let read = issuer
        .admit_web_user_knowledge_read(&caller, verifier, &original, provider, capture())
        .unwrap();
    let write = issuer
        .admit_web_user_knowledge_write(&caller, verifier, &original, provider, capture())
        .unwrap();
    let access = Access::user(&read, Some(&write)).unwrap();
    drop(
        ArtifactStore::new(scope.clone(), resources.policy.write.artifacts)
            .writer_with_context(original.snapshot())
            .unwrap(),
    );
    let backend = task::backend(&format!(
        "{}/v1/project/access/authorize",
        fixture.issuer.origin
    ));
    let live = history();
    let mut selected = artifact("knowledge", "vendor/mount-a");
    selected.version = "exact-release".into();
    observe_mcp_knowledge(&resources, &backend, identity, &original, &mut selected).await;
    let source = serde_json::from_value(json!({
        "kind":"imported_knowledge", "artifact":{"id":"vendor/mount-a","version":"exact-release"}
    }))
    .unwrap();
    let selected_project = ArtifactSelection {
        source: "project".into(),
        project_id: Some(resources.projects[0].1.to_string()),
        instance_id: None,
        project_kind: None,
        artifact_type: "knowledge".into(),
        id: resources.projects[0].1.to_string(),
        version: resources.projects[0].2.version().into(),
    };
    let project_source = crate::reference_explorer::SourceKey::ProjectKnowledge;
    let created = live
        .create(
            &access,
            CreateRequest {
                request_id: "live-mcp-cpu".into(),
                project: ProjectSelection {
                    project_id: resources.projects[0].1.to_string(),
                    project_version: resources.projects[0].2.version().into(),
                },
                binding: NativeBinding::KiroV3,
                agent: artifact("agent", "fixture/agent"),
                artifacts: vec![selected.clone(), selected_project.clone()],
                title: None,
                prompt: None,
                model: "fixture-model".into(),
            },
        )
        .unwrap();
    let turn = SendRequest {
        turn_id: "turn one".into(),
        text: "read actual Knowledge".into(),
    };
    prepare_mcp_history(
        &live, &access, &created, &resources, &backend, identity, &original, &rendered,
    )
    .await;
    live.emit_fixture(
        &access,
        &created.id,
        Some(&turn.turn_id),
        EventData::State {
            state: SessionState::Running,
        },
    )
    .unwrap();
    live.emit_fixture(
        &access,
        &created.id,
        Some(&turn.turn_id),
        EventData::Prompt {
            text: turn.text.clone(),
        },
    )
    .unwrap();
    let stored = live
        .read_contexts(&access, &created.id, &created.cursor.generation)
        .unwrap();
    let session = stored.session;
    let read_contexts = stored.contexts;
    let selected_contexts = [
        SelectedContext {
            selection: &selected,
            source: &source,
        },
        SelectedContext {
            selection: &selected_project,
            source: &project_source,
        },
    ];
    let broker = LiveMcpBroker::new();
    let mut route = broker
        .begin_run(Registration {
            sessions: &fixture.service,
            web_session: &web,
            entry: Arc::clone(&entry),
            session: &session,
            read_contexts: &read_contexts,
            turn: &turn,
            identity,
            original: &original,
        })
        .await
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = broker.router(resources.policy.content, fixture.stop.clone());
    fixture.servers.push(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let installation = launch_installation(&resources);
    let launch = route
        .launch(
            identity,
            &original,
            address,
            &installation,
            &installation.root().join("native-bin/graphit"),
        )
        .unwrap();
    let mut transport = process(&original);
    launch
        .apply_to(original.snapshot(), &mut transport)
        .unwrap();
    let uri = env(&transport, "GRAPHIT_LIVE_MCP_URL");
    let model: Arc<crate::foreground_memory_jobs::MemoryJobResources> = tasks
        .memory_model()
        .expect("CPU fixture model slot must be readable")
        .expect("CPU fixture must have its installed model resources");
    let finish = route.stop_handle();
    let closing = tokio::sync::Notify::new();
    let actions = async {
        let connection = client(&uri, &bearer, &web).await;
        for context in [None, Some(&selected), Some(&selected_project)] {
            let imported = context == Some(&selected);
            let mut generations = None;
            for mode in ["semantic", "hybrid"] {
                let mut input = json!({"search":{
                    "query":"Knowledge CPU search", "mode":mode,
                    "top_k":2, "page_size":1, "preview":true
                }});
                if let Some(context) = context {
                    input["context"] = serde_json::to_value(context).unwrap();
                }
                let response = value(
                    connection
                        .peer()
                        .call_tool(call(live_agent_tools::SEARCH, input))
                        .await
                        .unwrap(),
                );
                let native = &response["native"];
                assert_eq!(native["state"], "hits");
                assert_eq!(native["hits"].as_array().unwrap().len(), 1);
                assert!(native["semantic_generation"]["version"].as_u64().is_some());
                let hit = &native["hits"][0];
                assert!(
                    hit["semantic_distance"]
                        .as_f64()
                        .is_some_and(f64::is_finite)
                );
                if mode == "hybrid" {
                    assert!(hit["rrf_score"].as_f64().is_some_and(f64::is_finite));
                } else {
                    assert!(hit.get("rrf_score").is_none());
                }
                if imported {
                    assert_eq!(response["source"], serde_json::to_value(&source).unwrap());
                    assert_eq!(native["total"], 2);
                    assert_eq!(native["generation"]["version"], pair);
                    assert_eq!(native["semantic_generation"]["version"], pair);
                    assert!(response.get("selection").is_none_or(Value::is_null));
                } else {
                    assert_eq!(
                        response["selection"]["project_id"],
                        session.project.project_id
                    );
                    assert_eq!(
                        response["selection"]["project_version"],
                        session.project.project_version
                    );
                }
                let current = (
                    native["generation"].clone(),
                    native["semantic_generation"].clone(),
                );
                if let Some(expected) = &generations {
                    assert_eq!(&current, expected);
                } else {
                    generations = Some(current);
                }
            }
        }
        fixture.issuer.hold_project.store(true, Ordering::Release);
        {
            let response = connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({})));
            tokio::pin!(response);
            tokio::select! {
                () = fixture.issuer.project_started.notified() => {},
                result = &mut response => panic!("natural finish read did not reach held Broker: {}", result.is_ok()),
            }
            finish.stop();
            closing.notify_one();
            assert_eq!(response.await.unwrap().is_error, Some(true));
        }
        assert_eq!(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({})))
                .await
                .unwrap()
                .is_error,
            Some(true)
        );
        assert!(connection.peer().list_all_tools().await.is_err());
        connection.cancel().await.unwrap();
    };
    tokio::pin!(actions);
    {
        let mut context = run_context(
            &resources,
            &backend,
            &original,
            &access,
            &live,
            (&session, &turn),
            &selected_contexts,
        );
        context.model = Some(&model);
        let mut serving = Box::pin(route.serve(context));
        tokio::select! {
            biased;
            () = closing.notified() => {},
            () = &mut actions => panic!("CPU actions ended before natural stop"),
            result = &mut serving => panic!("CPU run bridge ended before calls: {}", result.is_ok()),
        }
        let (early, replies) = tokio::join!(
            tokio::time::timeout(Duration::from_millis(200), &mut serving),
            tokio::time::timeout(Duration::from_secs(2), &mut actions),
        );
        fixture.issuer.hold_project.store(false, Ordering::Release);
        fixture.issuer.release_project.notify_waiters();
        if replies.is_err() {
            actions.await;
        }
        if early.is_err() {
            serving.await.unwrap();
        }
        assert!(
            replies.is_ok(),
            "stopped callers must receive denial before Broker release"
        );
        assert!(
            early.is_err(),
            "natural stop returned before the active Broker read quiesced"
        );
    }
    drop(route);
    original.snapshot().check().unwrap();
    access.read().unwrap();
    assert_eq!(
        live.observe(&access, &session.id, None)
            .unwrap()
            .replay
            .session
            .id,
        session.id
    );
    assert!(!cancellation.is_cancelled());
    fixture.finish_tasks().await;
    sources.shutdown().await.unwrap();
    foreground.shutdown().await.unwrap();
}
