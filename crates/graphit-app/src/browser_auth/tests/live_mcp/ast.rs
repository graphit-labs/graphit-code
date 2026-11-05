//! Real selected AST reads and the existing CXX query closure under cancellation.
use super::*;
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority,
    broker::project_access::{BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead},
};
use graphit_core::{
    artifact_store::CapturedPublicationScope, project_identity::ProjectId,
    project_paths::ProjectPaths, version_context::VersionContext,
};
use rmcp::{
    model::{CallToolRequest, ClientRequest, ServerResult},
    service::PeerRequestOptions,
};
use std::io;

#[derive(Clone, Copy, Debug)]
enum End {
    Incoming,
    Eof,
    Original,
    Deadline,
    Natural,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_mcp_ast_reads_keep_independent_grants_and_join_real_cxx_on_every_stop() {
    let (issuer, issuer_service) = Fixture::start_issuer().await;
    let (storage, foreground, tasks, sources, raw, project) = Box::pin(
        crate::foreground_source::foreground_memory_jobs_tests::live_ast_search_fixture(
            &issuer.origin,
        ),
    )
    .await;
    storage.publish_ast_search_package(&foreground, &raw, &project);
    let fixture = Fixture::configured_with_issuer(
        policy(),
        Some((tasks.clone(), raw.clone())),
        None,
        Some((sources.clone(), raw)),
        issuer,
        issuer_service,
    )
    .await;
    fixture.login().await;
    std::fs::write(
        project.0.root().join("notes.unindexed"),
        "first\nneedle\nlast\n",
    )
    .unwrap();
    for end in [
        End::Incoming,
        End::Eof,
        End::Original,
        End::Deadline,
        End::Natural,
    ] {
        Box::pin(ast_case(&fixture, &tasks, &sources, &project, end)).await;
    }
    fixture.finish_tasks().await;
    sources.shutdown().await.unwrap();
    foreground.shutdown().await.unwrap();
}

async fn ast_case(
    fixture: &Fixture,
    tasks: &crate::task_server::TaskServer,
    sources: &crate::source_server::SourceServer,
    project: &(ProjectPaths, ProjectId, VersionContext),
    end: End,
) {
    let (web, entry) = fixture
        .service
        .0
        .sessions
        .lock()
        .await
        .iter()
        .next()
        .map(|(key, entry)| (key.clone(), Arc::clone(entry)))
        .unwrap();
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("AST fixture metadata"));
    let resources = tasks.record_resources(&snapshot);
    let cancellation = Cancellation::default();
    let duration = if matches!(end, End::Deadline) { 10 } else { 90 };
    let mut original = RequestContext::new(
        resources.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(duration),
    )
    .unwrap();
    let rendered = render_mcp_agent(&mut original);
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = fixture
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("AST Web flow"));
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
    let user_read = issuer
        .admit_web_user_knowledge_read(&caller, verifier, &original, provider, capture())
        .unwrap();
    let user_write = issuer
        .admit_web_user_knowledge_write(&caller, verifier, &original, provider, capture())
        .unwrap();
    let access = Access::user(&user_read, Some(&user_write)).unwrap();
    drop(
        ArtifactStore::new(scope.clone(), resources.policy.write.artifacts)
            .writer_with_context(original.snapshot())
            .unwrap(),
    );
    let backend = task::backend(&format!(
        "{}/v1/project/access/authorize",
        fixture.issuer.origin
    ));
    let selected = ArtifactSelection {
        source: "project".into(),
        project_id: Some(project.1.to_string()),
        instance_id: None,
        project_kind: None,
        artifact_type: "ast".into(),
        id: project.1.to_string(),
        version: project.2.version().into(),
    };
    let hub = issuer
        .admit_web_installed_hub_read(
            &caller,
            verifier,
            &original,
            provider,
            resources.auth.installation(),
        )
        .unwrap();
    let version =
        graphit_hub::ConcreteVersion::new("packaged-v1", resources.policy.role_reference).unwrap();
    let registry = sources.policy().registry.load().unwrap();
    let installed_artifact = registry
        .get_version(
            graphit_hub::ArtifactType::Ast,
            "fixture/ast-search",
            &version,
        )
        .unwrap();
    let installed = installed_artifact
        .installations
        .iter()
        .find(|entry| entry.target == graphit_hub::InstallationTarget::GlobalNative)
        .unwrap();
    let imported = crate::live_agent_catalog::selection(&installed.original_source);
    hub.recheck().unwrap();
    let selections = [selected.clone(), imported.clone()];
    let live = history();
    let created = live
        .create(
            &access,
            CreateRequest {
                request_id: format!("ast-{end:?}"),
                project: ProjectSelection {
                    project_id: project.1.to_string(),
                    project_version: project.2.version().into(),
                },
                binding: NativeBinding::KiroV3,
                agent: artifact("agent", "fixture/agent"),
                artifacts: selections.to_vec(),
                title: None,
                prompt: None,
                model: "fixture-model".into(),
            },
        )
        .unwrap();
    let selectors =
        crate::live_agent_ast::select_contexts(&resources, &hub, sources, created.artifacts.iter())
            .unwrap();
    let project_scope = CapturedPublicationScope::project(
        &project.0,
        project.1,
        resources.policy.scope,
        original.snapshot(),
    )
    .unwrap();
    let project_binding = project_scope.binding().unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let ast_read = authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Web(&caller),
            verifier,
            backend: &backend,
            original: &original,
            scope: BrokerProjectScope {
                installation: resources.auth.installation(),
                project: ProjectCapture {
                    scope: ScopeCapture {
                        live: &project_scope,
                        record: &project_binding,
                    },
                    version: &project.2,
                },
                providers: &[],
            },
            provider,
            module: ProjectReadModule::Ast,
        })
        .await
        .unwrap_or_else(|_| panic!("real PROJECT ReadAst"));
    let graph = sources.graph_server();
    let prepared = crate::live_agent_ast::capture_project(
        selectors
            .iter()
            .find(|entry| entry.selection == &selected)
            .unwrap(),
        project,
        &graph,
        sources,
        &ast_read,
        resources.auth.installation(),
        &original,
    )
    .await
    .unwrap();
    let imported_read = issuer
        .admit_web_imported_ast_read(
            &caller,
            verifier,
            &original,
            provider,
            resources.auth.installation(),
        )
        .unwrap();
    let prepared_imported = crate::live_agent_ast::capture_local_imported(
        selectors
            .iter()
            .find(|entry| entry.selection == &imported)
            .unwrap(),
        &graph,
        sources,
        &imported_read,
    )
    .await
    .unwrap();
    let prepared_contexts = selectors
        .iter()
        .map(|entry| {
            if entry.selection == &selected {
                prepared.prepared()
            } else {
                prepared_imported.prepared()
            }
        })
        .collect::<Vec<_>>();
    prepare_mcp_contexts(
        &live,
        &access,
        &created,
        &original,
        &rendered,
        &prepared_contexts,
    );
    let turn = SendRequest {
        turn_id: format!("turn-{end:?}"),
        text: "read selected AST".into(),
    };
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
    let broker = LiveMcpBroker::new();
    let mut route = broker
        .begin_run(Registration {
            sessions: &fixture.service,
            web_session: &web,
            entry: Arc::clone(&entry),
            session: &session,
            read_contexts: &stored.contexts,
            turn: &turn,
            identity,
            original: &original,
        })
        .await
        .unwrap();
    let finish = route.stop_handle();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_stop = CancellationToken::new();
    let http = broker.router(resources.policy.content, server_stop.clone());
    let stopped = server_stop.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, http)
            .with_graceful_shutdown(stopped.cancelled_owned())
            .await
            .unwrap();
    });
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
    let uri = env(&transport, crate::browser_auth::live_mcp::URL_ENV);
    let proxy = LiveMcpProxy::configured(
        zeroize::Zeroizing::new(uri.clone()),
        zeroize::Zeroizing::new(bearer.to_string()),
        zeroize::Zeroizing::new(web.clone()),
        resources.policy.content,
        Duration::from_secs(30),
    )
    .unwrap();
    let (native, wire) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(wire);
    let mut proxy_owner = tokio::spawn(proxy.serve_stream(read, write));
    let (read, write) = tokio::io::split(native);
    {
        let mut context = run_context(
            &resources,
            &backend,
            &original,
            &access,
            &live,
            (&session, &turn),
            &[],
        );
        context.source = Some(sources);
        context.ast = &selectors;
        let mut serving = Box::pin(route.serve(context));
        let phase = async {
            let mut native = ().serve((read, write)).await.unwrap();
            let tools = native.peer().list_all_tools().await.unwrap();
            assert_eq!(tools.len(), 8);
            for tool in tools
                .iter()
                .filter(|tool| live_agent_tools::is_ast_tool(&tool.name))
            {
                assert_eq!(
                    tool.input_schema["properties"]["context"]["enum"],
                    json!(session.artifacts)
                );
            }
            if matches!(end, End::Incoming) {
                for (tool, input) in [
                    ("graphit_ast_schema", json!({"context":selected})),
                    (
                        "graphit_ast_query",
                        json!({"context":selected,"query":"MATCH (n:Function) RETURN n.name LIMIT 10"}),
                    ),
                    (
                        "graphit_ast_source",
                        json!({"context":selected,"path":"browser_search.rs","entity":"browser_search_sum","entity_type":"Function","line_numbers":true}),
                    ),
                    (
                        "graphit_ast_search",
                        json!({"context":selected,"query":"browser_search_sum","mode":"fts","k":8}),
                    ),
                    (
                        "graphit_ast_source",
                        json!({"context":selected,"path":"notes.unindexed","pattern":"needle","before":1,"after":1,"line_numbers":true}),
                    ),
                ] {
                    let result = value(
                        native
                            .peer()
                            .call_tool(call(tool, input.clone()))
                            .await
                            .unwrap(),
                    );
                    assert!(!result.is_null());
                    let content = serde_json::to_string(&result).unwrap();
                    if tool == "graphit_ast_schema" {
                        assert!(content.contains("Function"));
                    } else if input.get("path").and_then(Value::as_str) == Some("notes.unindexed") {
                        assert!(content.contains("needle"));
                    } else {
                        assert!(content.contains("browser_search_sum"));
                    }
                    if tool == "graphit_ast_search" {
                        assert_eq!(result["status"], "available");
                    }
                }
                // A new entity read uses current Source; a snapshot explicitly
                // captured before a commit retains its own admitted bytes.
                {
                    use graphit_ast::{
                        indexed_source::{read_branch_source, source_schema},
                        source_index_locator::{SourceIndexPresence, open_local_source_index},
                    };
                    use graphit_data::lance_branch_store::{col, lit};
                    let SourceIndexPresence::Available(source_index) = open_local_source_index(
                        resources.auth.installation(),
                        ProjectCapture {
                            scope: ScopeCapture {
                                live: &project_scope,
                                record: &project_binding,
                            },
                            version: &project.2,
                        },
                        original.snapshot(),
                        sources.policy().index,
                    )
                    .await
                    .unwrap() else {
                        panic!("productive Source fixture missing")
                    };
                    let retained = source_index.snapshot(original.snapshot()).unwrap().clone();
                    let captured_source = source_index
                        .read("browser_search.rs", original.snapshot())
                        .await
                        .unwrap();
                    let graphit_ast::source_inspection::IndexedSource::Available(text) =
                        captured_source.source(original.snapshot()).unwrap()
                    else {
                        panic!("fixture Source text missing")
                    };
                    let text = text.to_owned();
                    let row = |path: &str, text: Option<&str>| {
                        arrow_array::RecordBatch::try_new(
                            source_schema(),
                            vec![
                                Arc::new(arrow_array::StringArray::from(vec![path])),
                                Arc::new(arrow_array::StringArray::from(vec![text])),
                            ],
                        )
                        .unwrap()
                    };
                    let entity_request = || {
                        call(
                            "graphit_ast_source",
                            json!({
                                "context":selected, "path":"browser_search.rs", "entity":"browser_search_sum", "entity_type":"Function",
                            }),
                        )
                    };
                    let mut current = retained
                        .prepare_merge(
                            vec![row("unrelated.fixture", Some("other FILE"))],
                            vec!["path".into()],
                        )
                        .await
                        .unwrap()
                        .commit()
                        .await
                        .unwrap();
                    assert_ne!(
                        native
                            .peer()
                            .call_tool(entity_request())
                            .await
                            .unwrap()
                            .is_error,
                        Some(true),
                        "an unrelated Source commit cannot invalidate this FILE"
                    );
                    current = current
                        .prepare_merge(
                            vec![row(
                                "browser_search.rs",
                                Some(&format!("{text}\n// changed Source")),
                            )],
                            vec!["path".into()],
                        )
                        .await
                        .unwrap()
                        .commit()
                        .await
                        .unwrap();
                    let stale = native.peer().call_tool(entity_request()).await.unwrap();
                    assert_eq!(
                        stale.is_error,
                        Some(true),
                        "changed Source must reject stale entity extents"
                    );
                    assert_eq!(
                        stale.structured_content.unwrap()["error"]["code"],
                        "native_capture_or_registry_failure"
                    );
                    let entity_options: crate::ast_source::SourceParameters = serde_json::from_value(json!({
                        "path":"browser_search.rs", "entity":"browser_search_sum", "entity_type":"Function",
                    })).unwrap();
                    let mismatch = prepared.source(&entity_options).await.unwrap_err();
                    assert!(matches!(mismatch.get_ref().and_then(|error| error.downcast_ref::<graphit_ast::source_inspection::SourceInspectionError>()),
                        Some(graphit_ast::source_inspection::SourceInspectionError::Association)));
                    current = current
                        .prepare_merge(vec![row("browser_search.rs", None)], vec!["path".into()])
                        .await
                        .unwrap()
                        .commit()
                        .await
                        .unwrap();
                    assert_ne!(
                        native
                            .peer()
                            .call_tool(entity_request())
                            .await
                            .unwrap()
                            .is_error,
                        Some(true),
                        "admitted TextNotStored retains authorized physical fallback and SHA validation"
                    );
                    current = current
                        .prepare_delete_expr(col("path").eq(lit("browser_search.rs")))
                        .await
                        .unwrap()
                        .commit()
                        .await
                        .unwrap();
                    let absent = native.peer().call_tool(entity_request()).await.unwrap();
                    assert_eq!(
                        absent.is_error,
                        Some(true),
                        "FileAbsent cannot use the still-existing physical file"
                    );
                    assert_eq!(
                        absent.structured_content.unwrap()["error"]["code"],
                        "native_capture_or_registry_failure"
                    );
                    let missing = prepared.source(&entity_options).await.unwrap_err();
                    assert!(matches!(missing.get_ref().and_then(|error| error.downcast_ref::<graphit_ast::source_inspection::SourceInspectionError>()),
                        Some(graphit_ast::source_inspection::SourceInspectionError::Association)));
                    drop(
                        current
                            .prepare_merge(
                                vec![row("browser_search.rs", Some(&text))],
                                vec!["path".into()],
                            )
                            .await
                            .unwrap()
                            .commit()
                            .await
                            .unwrap(),
                    );
                    assert_ne!(
                        native
                            .peer()
                            .call_tool(entity_request())
                            .await
                            .unwrap()
                            .is_error,
                        Some(true)
                    );
                    let still_captured =
                        read_branch_source(&retained, "browser_search.rs", original.snapshot())
                            .await
                            .unwrap();
                    assert!(
                        matches!(still_captured.source(original.snapshot()).unwrap(),
                        graphit_ast::source_inspection::IndexedSource::Available(captured) if captured == text)
                    );
                }
                for (tool, input) in [
                    ("graphit_ast_schema", json!({"context":imported})),
                    (
                        "graphit_ast_query",
                        json!({"context":imported,"query":"MATCH (n:Function) RETURN n.name LIMIT 10"}),
                    ),
                    (
                        "graphit_ast_source",
                        json!({"context":imported,"path":"browser_search.rs","entity":"browser_search_sum","entity_type":"Function","line_numbers":true}),
                    ),
                    (
                        "graphit_ast_search",
                        json!({"context":imported,"query":"browser_search_sum","mode":"fts","k":8}),
                    ),
                ] {
                    let result = value(native.peer().call_tool(call(tool, input)).await.unwrap());
                    let encoded = serde_json::to_string(&result).unwrap();
                    if tool == "graphit_ast_schema" {
                        assert!(encoded.contains("Function"));
                    } else {
                        assert!(encoded.contains("browser_search_sum"));
                    }
                }
                assert_eq!(
                    native
                        .peer()
                        .call_tool(call(
                            "graphit_ast_source",
                            json!({
                                "context":imported,"path":"notes.unindexed"
                            })
                        ))
                        .await
                        .unwrap()
                        .is_error,
                    Some(true),
                    "an imported miss cannot read the current PROJECT file"
                );
                for bad in [json!({}), json!({"context":null})] {
                    assert_eq!(
                        native
                            .peer()
                            .call_tool(call("graphit_ast_schema", bad))
                            .await
                            .unwrap()
                            .is_error,
                        Some(true)
                    );
                }
                for wrong in [
                    ArtifactSelection {
                        project_id: Some("different-project".into()),
                        ..selected.clone()
                    },
                    ArtifactSelection {
                        artifact_type: "knowledge".into(),
                        ..selected.clone()
                    },
                    ArtifactSelection {
                        source: "hub".into(),
                        ..selected.clone()
                    },
                ] {
                    assert_eq!(
                        native
                            .peer()
                            .call_tool(call("graphit_ast_schema", json!({"context":wrong})))
                            .await
                            .unwrap()
                            .is_error,
                        Some(true)
                    );
                }
                let mut wrong = selected.clone();
                wrong.version.push_str("-other");
                assert_eq!(
                    native
                        .peer()
                        .call_tool(call("graphit_ast_schema", json!({"context":wrong})))
                        .await
                        .unwrap()
                        .is_error,
                    Some(true)
                );
                fixture
                    .issuer
                    .denied_modules
                    .lock()
                    .unwrap()
                    .push("knowledge".into());
                let separate = client(&uri, &bearer, &web).await;
                assert_eq!(separate.peer().list_all_tools().await.unwrap().len(), 8);
                value(
                    separate
                        .peer()
                        .call_tool(call("graphit_ast_schema", json!({"context":selected})))
                        .await
                        .unwrap(),
                );
                assert_eq!(
                    separate
                        .peer()
                        .call_tool(call(live_agent_tools::HEADS, json!({})))
                        .await
                        .unwrap()
                        .is_error,
                    Some(true)
                );
                separate.cancel().await.unwrap();
                fixture
                    .issuer
                    .denied_modules
                    .lock()
                    .unwrap()
                    .retain(|module| module != "knowledge");
                fixture
                    .issuer
                    .denied_modules
                    .lock()
                    .unwrap()
                    .push("ast".into());
                let separate = client(&uri, &bearer, &web).await;
                assert_eq!(
                    separate
                        .peer()
                        .call_tool(call("graphit_ast_schema", json!({"context":selected})))
                        .await
                        .unwrap()
                        .is_error,
                    Some(true)
                );
                value(
                    separate
                        .peer()
                        .call_tool(call(live_agent_tools::HEADS, json!({})))
                        .await
                        .unwrap(),
                );
                separate.cancel().await.unwrap();
                fixture
                    .issuer
                    .denied_modules
                    .lock()
                    .unwrap()
                    .retain(|module| module != "ast");
            }
            let gate = crate::live_agent_ast::test_query_join(original.snapshot()).unwrap();
            let request = native
                .peer()
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(call(
                        "graphit_ast_query",
                        json!({
                            "context":selected,"query":"MATCH (n:Function) RETURN n.name LIMIT 10"
                        }),
                    ))),
                    PeerRequestOptions::no_options(),
                )
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_secs(5), gate.started)
                    .await
                    .unwrap()
                    .unwrap(),
                "the real CXX query must succeed before the held closure can qualify"
            );
            let mut request = Some(request);
            match end {
                End::Incoming => request
                    .take()
                    .unwrap()
                    .cancel(Some("fixture incoming cancel".into()))
                    .await
                    .unwrap(),
                End::Eof => {
                    native.close().await.unwrap();
                    (&mut proxy_owner).await.unwrap().unwrap();
                }
                End::Original => cancellation.cancel(),
                End::Deadline => {
                    tokio::time::sleep_until(
                        original.snapshot().required_deadline().unwrap().into(),
                    )
                    .await
                }
                End::Natural => finish.stop(),
            }
            if matches!(end, End::Incoming | End::Eof) {
                original.check().unwrap();
                access.read().unwrap();
                let separate = client(&uri, &bearer, &web).await;
                assert!(
                    tokio::time::timeout(
                        Duration::from_millis(200),
                        separate.peer().list_all_tools()
                    )
                    .await
                    .is_err(),
                    "AST queue advanced before its CXX closure joined"
                );
                separate.cancel().await.unwrap();
                finish.stop();
            }
            (native, request, gate.release, gate.finished)
        };
        tokio::pin!(phase);
        let (mut native, request, release, mut finished) = tokio::select! {
            result=&mut phase=>result,
            result=&mut serving=>panic!("serve ended before CXX release ({end:?}): {result:?}"),
        };
        assert!(matches!(
            finished.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut serving)
                .await
                .is_err(),
            "serve dropped its active CXX future on {end:?}"
        );
        release.send(()).unwrap();
        let replies = async {
            finished.await.unwrap();
            if let Some(request) = request {
                match request.await_response().await {
                    Ok(ServerResult::CallToolResult(result)) => {
                        assert_eq!(result.is_error, Some(true));
                        assert!(
                            !serde_json::to_string(&result)
                                .unwrap()
                                .contains("browser_search_sum")
                        );
                        let code =
                            result.structured_content.as_ref().unwrap()["error"]["code"].clone();
                        if matches!(end, End::Original) {
                            assert_eq!(code, "cancelled");
                        }
                        if matches!(end, End::Deadline) {
                            assert_eq!(code, "deadline");
                        }
                    }
                    Err(_) if matches!(end, End::Eof) => {}
                    _ => panic!("cancelled AST returned an unexpected response"),
                }
            }
            native.close().await.unwrap();
        };
        let (served, ()) = tokio::join!(&mut serving, replies);
        match end {
            End::Original => assert_eq!(served.unwrap_err().kind(), io::ErrorKind::Interrupted),
            End::Deadline => assert_eq!(served.unwrap_err().kind(), io::ErrorKind::TimedOut),
            _ => served.unwrap(),
        }
    }
    if !matches!(end, End::Eof) {
        proxy_owner.await.unwrap().unwrap();
    }
    drop(launch);
    drop(route);
    if !matches!(end, End::Original | End::Deadline) {
        original.check().unwrap();
        access.read().unwrap();
        assert_eq!(
            live.observe(&access, &session.id, None)
                .unwrap()
                .replay
                .session
                .id,
            session.id
        );
    }
    server_stop.cancel();
    server.await.unwrap();
}
