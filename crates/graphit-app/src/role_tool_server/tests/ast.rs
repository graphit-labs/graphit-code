use super::*;
fn ast_source(
    server: &RoleToolServer,
) -> (
    Arc<crate::foreground_ast_sources::ForegroundAstSources>,
    tempfile::TempDir,
) {
    use crate::foreground_ast_sources::{ForegroundAstSourceConfig, ForegroundAstSources};
    use graphit_core::config::{ConfigKey, ConfigStore};

    let cancellation = Cancellation::default();
    let request = request(&cancellation, server.0.task.content);
    let snapshot = project_snapshot(server);
    let (paths, _, _) = &snapshot.projects[0];
    for (key, value) in [("modules.ast", true), ("modules.embedding", false)] {
        ConfigStore::project(paths)
            .unwrap()
            .set(
                ConfigKey::new(key).unwrap(),
                json!(value),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    // The fixture uses the same parser, Source publication and FILE transaction
    // as the daemon, without compiler/provider executables or an EOF bundle.
    let no_docker = tempfile::tempdir().unwrap();
    let source = &server.0.policy.source;
    let ast = Arc::new(
        ForegroundAstSources::new(
            Arc::clone(&server.0.auth),
            ForegroundAstSourceConfig {
                live: server.0.graph.live_ast(),
                installation: source.installation.clone(),
                base: server.0.policy.config.clone(),
                policy: source.native_install.hub.clone(),
                scope: source.scope,
                native: source.index,
                binding: source.binding,
                environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
            },
            request.snapshot(),
        )
        .unwrap(),
    );
    (ast, no_docker)
}

async fn publish(server: &RoleToolServer, name: &str) {
    use graphit_daemon::{supervisor::ProjectContext, watcher::ChangeBatch};
    let snapshot = project_snapshot(server);
    let (paths, id, version) = &snapshot.projects[0];
    std::fs::write(
        paths.root().join("main.rs"),
        format!("pub fn {name}() {{}}\n"),
    )
    .unwrap();
    let (ast, _no_docker) = ast_source(server);
    let cancellation = Cancellation::default();
    let context = ProjectContext {
        id: *id,
        version: version.clone(),
    };
    ast.index(
        paths,
        &context,
        ChangeBatch {
            rescan: true,
            ..Default::default()
        },
        &cancellation,
    )
    .await
    .unwrap();
    ast.index_semantic(
        paths.clone(),
        context,
        None,
        None,
        std::collections::BTreeSet::new(),
        false,
        cancellation.clone(),
        cancellation,
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_ast_schema_query_fts_and_explicit_model_absence_use_real_project() {
    let (_temp, server, raw) = fixture();
    publish(&server, "nebula").await;
    // Observe missing NN independently of model-service availability. The
    // structural fixture deliberately indexed with inference disabled.
    graphit_core::config::ConfigStore::project(&project_snapshot(&server).projects[0].0)
        .unwrap()
        .set(
            graphit_core::config::ConfigKey::new("modules.embedding").unwrap(),
            json!(true),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Ast)],
        Duration::from_secs(90),
    );
    let (_session, _run, no_ast, no_bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Task)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let denied = connect(
        &uri,
        role_credential(&no_ast, &no_bearer, server.0.task.content),
    )
    .await;
    let tools = client.list_all_tools().await.unwrap();
    for name in [
        "graphit_ast_schema",
        "graphit_ast_query",
        "graphit_ast_search",
    ] {
        assert_eq!(tools.iter().filter(|tool| tool.name == name).count(), 1);
        assert!(
            !denied
                .list_all_tools()
                .await
                .unwrap()
                .iter()
                .any(|tool| tool.name == name)
        );
        assert_eq!(
            call(&denied, name, json!({"input":{}}), true).await["code"],
            "authentication_or_permission_denied"
        );
    }
    let schema = call(&client, "graphit_ast_schema", json!({"input":{}}), false).await;
    assert!(
        schema["result"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|table| table["label"] == "Function")
    );
    let query = json!({"input":{"query":"MATCH (n:Function) WHERE n.name = $name RETURN n.name AS name","parameters":{"name":"nebula"}}});
    let value = call(&client, "graphit_ast_query", query.clone(), false).await;
    assert_eq!(value["result"]["rows"][0][0]["value"], "nebula");
    for input in [
        json!({"query":"CREATE (:Function {name:'forbidden'})"}),
        json!({"query":"MATCH (n) RETURN n","unknown":true}),
    ] {
        call(&client, "graphit_ast_query", json!({"input":input}), true).await;
    }
    assert_eq!(
        call(&client, "graphit_ast_query", query, false).await["result"]["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let search = call(
        &client,
        "graphit_ast_search",
        json!({"input":{"query":"nebula","mode":"fts","k":2}}),
        false,
    )
    .await;
    assert_eq!(search["result"]["status"], "available");
    assert_eq!(search["result"]["results"][0]["name"], "nebula");
    assert!(search["result"].get("graph_generation").is_none());
    for (mode, status) in [("semantic", "index_absent"), ("hybrid", "available")] {
        let value = call(
            &client,
            "graphit_ast_search",
            json!({"input":{"query":"nebula","mode":mode}}),
            false,
        )
        .await;
        assert_eq!(value["result"]["status"], status);
        if mode == "hybrid" {
            assert_eq!(value["result"]["vector_status"], "index_absent");
            assert!(value["result"].get("graph_generation").is_none());
        }
    }
    for input in [
        json!({"query":"nebula","mode":"other"}),
        json!({"query":"nebula","mode":"fts","k":0}),
        json!({"query":"nebula","context":"foreign@v1"}),
    ] {
        assert_eq!(
            call(&client, "graphit_ast_search", json!({"input":input}), true).await["code"],
            "invalid_arguments"
        );
    }
    let peer_selection = project_snapshot(&server).projects[1].clone();
    assert_eq!(call(&client, "graphit_ast_schema", json!({"selection":{"project_id":peer_selection.1,"project_version":peer_selection.2.version()},"input":{}}), true).await["code"], "authentication_or_permission_denied");
    publish(&server, "updated").await;
    let value = call(
        &client,
        "graphit_ast_search",
        json!({"input":{"query":"updated","mode":"fts"}}),
        false,
    )
    .await;
    assert_eq!(value["result"]["results"][0]["name"], "updated");
    assert!(value["result"].get("graph_generation").is_none());
    let runtime = server
        .call(
            "graphit_ast_schema".into(),
            serde_json::from_value(json!({"input":{}})).unwrap(),
            Credential {
                role_binding: None,
                bearer: Zeroizing::new(raw),
                origin: Origin::Runtime,
            },
            CancellationToken::new(),
        )
        .await;
    assert_eq!(runtime.is_error, Some(true));
    client.cancel().await.unwrap();
    denied.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_ast_skill_payloads_execute_on_actual_role_transport() {
    let (_temp, server, raw) = fixture();
    publish(&server, "main").await;
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Ast)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let body = call(
        &client,
        "graphit_module_skill",
        json!({"input":{"module":"ast"}}),
        false,
    )
    .await;
    let body = body["source"].as_str().unwrap();
    assert!(body.contains("AST for an authenticated Role"));
    assert!(!body.contains("\"top_k\""));
    assert!(!body.contains("\"ai_optimized\""));
    let payloads: Vec<Value> = body
        .split("```json\n")
        .skip(1)
        .map(|body| serde_json::from_str(body.split_once("```").unwrap().0).unwrap())
        .collect();
    assert_eq!(payloads.len(), 4);
    for (tool, payload) in [
        "graphit_ast_schema",
        "graphit_ast_query",
        "graphit_ast_search",
        "graphit_ast_source",
    ]
    .into_iter()
    .zip(payloads)
    {
        serde_json::from_value::<Arguments>(payload.clone()).unwrap();
        call(&client, tool, payload, false).await;
    }
    let companion = call(
        &client,
        "graphit_module_skill",
        json!({"input":{"module":"ast","reference":"references/impact-assessment.md"}}),
        false,
    )
    .await;
    let payloads: Vec<Value> = companion["source"]
        .as_str()
        .unwrap()
        .split("```json\n")
        .skip(1)
        .map(|body| serde_json::from_str(body.split_once("```").unwrap().0).unwrap())
        .collect();
    assert_eq!(payloads.len(), 2);
    for (tool, payload) in ["graphit_ast_query", "graphit_ast_source"]
        .into_iter()
        .zip(payloads)
    {
        serde_json::from_value::<Arguments>(payload.clone()).unwrap();
        call(&client, tool, payload, false).await;
    }
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_ast_revocation_and_cancellation_before_delivery_preserve_other_run() {
    use rmcp::model::{CallToolRequest, ClientRequest};
    for revoke in [false, true] {
        let (_temp, server, raw) = fixture();
        publish(&server, "nebula").await;
        let (session, run, reference, bearer) = credential(
            &server,
            &raw,
            Role::Scout,
            &[Permission::Read(Module::Ast)],
            Duration::from_secs(90),
        );
        let (_session, _run, sibling_ref, sibling_bearer) = credential(
            &server,
            &raw,
            Role::Tracker,
            &[Permission::Read(Module::Ast)],
            Duration::from_secs(90),
        );
        let (uri, serving) = serve(&server).await;
        let client = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        let sibling = connect(
            &uri,
            role_credential(&sibling_ref, &sibling_bearer, server.0.task.content),
        )
        .await;
        let (started, observed) = std::sync::mpsc::channel();
        let (release, waiting) = oneshot::channel();
        *server.0.before_delivery.lock().unwrap() = Some((started, waiting));
        let params = CallToolRequestParams::new("graphit_ast_search").with_arguments(
            json!({"ai_optimized":false,"input":{"query":"nebula","mode":"fts"}})
                .as_object()
                .unwrap()
                .clone(),
        );
        if revoke {
            let operation = client.call_tool(params);
            let deny = async {
                tokio::task::spawn_blocking(move || {
                    observed.recv_timeout(Duration::from_secs(10)).unwrap()
                })
                .await
                .unwrap();
                let cancellation = Cancellation::default();
                let original = request(&cancellation, server.0.task.content);
                let issuer = server.0.auth.issuer();
                let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
                let parent = issuer.admin_parent(&caller, &session).unwrap();
                issuer.close_run(&parent, &run).unwrap();
                release.send(()).unwrap();
            };
            let (reply, ()) = tokio::join!(operation, deny);
            let reply = reply.unwrap();
            assert_eq!(reply.is_error, Some(true));
            let value: Value = typed_value(&reply);
            assert_eq!(value["code"], "authentication_or_permission_denied");
        } else {
            let pending = client
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(params)),
                    Default::default(),
                )
                .await
                .unwrap();
            let cancellation = tokio::task::spawn_blocking(move || {
                observed.recv_timeout(Duration::from_secs(10)).unwrap()
            })
            .await
            .unwrap();
            pending.cancel(None).await.unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !cancellation.is_cancelled() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            release.send(()).unwrap();
        }
        assert_eq!(
            call(
                &sibling,
                "graphit_ast_search",
                json!({"input":{"query":"nebula","mode":"fts"}}),
                false
            )
            .await["result"]["results"][0]["name"],
            "nebula"
        );
        client.cancel().await.unwrap();
        sibling.cancel().await.unwrap();
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn role_ast_reads_first_file_and_incremental_changes_without_automatic_bundle()
-> Result<(), Box<dyn std::error::Error>> {
    use futures_util::FutureExt;
    use graphit_daemon::{supervisor::ProjectContext, watcher::ChangeBatch};
    use std::collections::BTreeSet;
    let (_fixture, server, raw) = fixture();
    let selected = project_snapshot(&server).projects[0].clone();
    std::fs::write(selected.0.root().join("a.rs"), "pub fn alpha() {}\n")?;
    std::fs::write(selected.0.root().join("b.rs"), "pub fn beta() {}\n")?;
    let (ast, _no_docker) = ast_source(&server);
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Ast)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let cancellation = Cancellation::default();
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    ast.index(
        &selected.0,
        &context,
        ChangeBatch {
            rescan: true,
            ..Default::default()
        },
        &cancellation,
    )
    .await?;
    let (first, release) = ast.hold_after_file_publication("a.rs");
    let producer = Arc::clone(&ast);
    let project = selected.0.clone();
    let selected_context = context.clone();
    let signal = cancellation.clone();
    let job = tokio::spawn(async move {
        producer
            .index_semantic(
                project,
                selected_context,
                None,
                None,
                std::collections::BTreeSet::new(),
                false,
                signal.clone(),
                signal,
            )
            .await
    });
    let result = std::panic::AssertUnwindSafe(async {
        tokio::time::timeout(Duration::from_secs(30), first).await??;
        assert!(!job.is_finished());
        let schema = call(&client, "graphit_ast_schema", json!({"input":{}}), false).await;
        assert!(
            schema["result"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["label"] == "Function")
        );
        let query = call(
            &client,
            "graphit_ast_query",
            json!({"input":{
                "query":"MATCH(n:Function) RETURN n.name ORDER BY n.name"
            }}),
            false,
        )
        .await;
        assert_eq!(query["result"]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(query["result"]["rows"][0][0]["value"], "alpha");
        let found = call(
            &client,
            "graphit_ast_search",
            json!({"input":{"query":"alpha","mode":"fts"}}),
            false,
        )
        .await;
        assert_eq!(found["result"]["status"], "available");
        assert_eq!(found["result"]["results"][0]["name"], "alpha");
        Ok::<_, Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    let _ = release.send(());
    let indexed = job.await?;
    let result = match result {
        Ok(result) => result,
        Err(panic) => {
            let _ = client.cancel().await;
            let _ = server.shutdown().await;
            std::panic::resume_unwind(panic)
        }
    };
    result?;
    indexed?;
    for deleted in [false, true] {
        let path = selected.0.root().join("b.rs");
        if deleted {
            std::fs::remove_file(&path)?;
        } else {
            std::fs::write(&path, "pub fn delta_beta() {}\n")?;
        }
        ast.index(
            &selected.0,
            &context,
            ChangeBatch {
                paths: BTreeSet::from([path]),
                ..Default::default()
            },
            &cancellation,
        )
        .await?;
        ast.index_semantic(
            selected.0.clone(),
            context.clone(),
            None,
            Some(BTreeSet::from(["b.rs".to_owned()])),
            std::collections::BTreeSet::new(),
            false,
            cancellation.clone(),
            cancellation.clone(),
        )
        .await?;
        let query = call(
            &client,
            "graphit_ast_query",
            json!({"input":{
                "query":"MATCH(n:Function) RETURN n.name ORDER BY n.name"
            }}),
            false,
        )
        .await;
        let names = query["result"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row[0]["value"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            if deleted {
                vec!["alpha"]
            } else {
                vec!["alpha", "delta_beta"]
            }
        );
        let found = call(
            &client,
            "graphit_ast_search",
            json!({"input":{"query":"delta_beta","mode":"fts"}}),
            false,
        )
        .await;
        assert_eq!(
            found["result"]["results"].as_array().unwrap().is_empty(),
            deleted
        );
    }
    assert!(
        !selected.0.brand_directory().join("ast-graphs").exists(),
        "local indexing must not create an automatic immutable bundle"
    );
    client.cancel().await?;
    server.shutdown().await?;
    serving.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the configured local embedding artifact and model service binary"]
async fn role_ast_actual_shared_cpu_hybrid_uses_role_service_and_same_slot() {
    let (_storage, foreground, tasks, sources, raw, selected) =
        crate::foreground_source::foreground_memory_jobs_tests::browser_ast_search_fixture().await;
    let resources = sources.semantic_model().unwrap().unwrap();
    let config = ConfigSnapshot::new(sources.policy().installation.brand(), []).unwrap();
    let server = RoleToolServer::new(
        Arc::clone(&resources.auth),
        Arc::clone(&sources.live_projects().snapshot().unwrap().projects),
        0,
        RoleToolPolicy::capture(sources.policy().clone(), config).unwrap(),
        CancellationToken::new(),
    )
    .unwrap()
    .with_live_projects(sources.live_projects())
    .unwrap()
    .with_graph_server(sources.graph_server())
    .unwrap()
    .with_semantic_slot(sources.semantic_slot())
    .unwrap();
    assert!(Arc::ptr_eq(&server.0.semantic, &sources.semantic_slot()));
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Ast)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let value = call(&client, "graphit_ast_search", json!({"selection":{"project_id":selected.1,"project_version":selected.2.version()},"input":{"query":"browser_search_sum","mode":"hybrid","k":8}}), false).await;
    let value = &value["result"];
    assert_eq!(value["status"], "available");
    assert_eq!(value["mode"], "hybrid");
    assert!(value.get("nn_generation").is_none());
    assert!(value.get("query_model_generation").is_none());
    assert!(
        value["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["name"] == "browser_search_sum")
    );
    assert!(sources.shares_semantic_model(&resources));
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    tasks.shutdown().await.unwrap();
    sources.shutdown().await.unwrap();
    drop((sources, resources));
    foreground.shutdown().await.unwrap();
}
