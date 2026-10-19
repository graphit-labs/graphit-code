//! The foreground producer and native MCP readers share the actual FILE stores.
use super::foreground_memory_jobs_tests as support;
use super::*;
use crate::foreground_memory_jobs::{MemoryJobResources, OwnedLocalEmbedding};
use axum::http::{HeaderMap, HeaderValue, header};
use futures_util::{FutureExt, TryStreamExt};
use graphit_core::{config::ConfigStore, native_path::NativePath};
use graphit_daemon::{supervisor::ProjectContext, watcher::ChangeBatch};
use graphit_data::lance_branch_store::BranchSnapshot;
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type Selected = (ProjectPaths, ProjectId, VersionContext);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watcher_admits_file_delta_while_previous_nn_request_is_unanswered() -> TestResult {
    use graphit_daemon::supervisor::ProjectSupervisor;
    let (fixture, owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner.memory_job_resources.upgrade().unwrap();
    let source = owner.server.as_ref().unwrap().clone();
    let lifetime = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &lifetime,
    )?;
    fs::write(selected.0.root().join("a.rs"), "pub fn alpha() {}\n")?;
    fs::write(selected.0.root().join("b.rs"), "pub fn beta() {}\n")?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let (requested, request_seen) = tokio::sync::oneshot::channel();
    let (release, held) = tokio::sync::oneshot::channel();
    let endpoint = tokio::spawn(async move {
        let (socket, _) = listener.accept().await?;
        let _ = requested.send(());
        let _ = held.await;
        drop(socket);
        Ok::<(), io::Error>(())
    });
    let mut held_resources = MemoryJobResources::remote(
        Arc::clone(&resources.auth),
        resources.http.clone(),
        Arc::clone(&resources.brokers),
        resources.policy.clone(),
        resources.scope,
        resources.native,
        resources.binding,
    )?;
    let local = resources.local.as_ref().unwrap();
    held_resources.local = Some(OwnedLocalEmbedding {
        client: Arc::clone(&local.client),
        expected: Arc::clone(&local.expected),
        address,
    });
    let held_resources = Arc::new(held_resources);
    let no_docker = tempfile::tempdir()?;
    let original = owner.policy.request_context(Instant::now(), &lifetime)?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let config = project_watch_config(&selected.0, fixture.config(), &original)?;
    let project = selected.0.clone();
    let mut supervisor = Some(ProjectSupervisor::start_controlled(
        context(&selected), config, Default::default(),
        move |selected, batch, lifetime, _, background, _| {
            let ast = Arc::clone(&ast);
            let project = project.clone();
            let resources = Arc::clone(&held_resources);
            async move {
                let paths = if batch.rescan { None } else {
                    Some(batch.paths.iter().map(|path| {
                        crate::foreground_ast_sources::literal_path(
                            path.strip_prefix(project.root()).map_err(io::Error::other)?
                        )
                    }).collect::<io::Result<BTreeSet<_>>>()?)
                };
                ast.index(&project, &selected, batch, &lifetime).await?;
                match ast.index_semantic(project, selected, Some(resources), paths, std::collections::BTreeSet::new(), false, background, lifetime.clone()).await {
                    Ok(_) => Ok(()),
                    Err(error) if lifetime.is_cancelled()
                        || crate::foreground_ast_sources::semantic::only_inference_failed(&error) => Ok(()),
                    Err(error) => Err(error),
                }
            }
        },
    ).await?);
    let result = std::panic::AssertUnwindSafe(async {
        tokio::time::timeout(Duration::from_secs(30), request_seen).await??;
        let before = query(&source, &selected, &credential,
            "MATCH(n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        assert_eq!(before["rows"].as_array().unwrap().len(), 1);
        fs::write(selected.0.root().join("b.rs"), "pub fn delta_beta() {}\n")?;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let found = lexical(&source, &selected, &credential, "delta_beta").await;
                if found.get("results").and_then(Value::as_array).is_some_and(|rows| !rows.is_empty()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await?;
        assert!(!endpoint.is_finished(), "the NN endpoint has never replied");
        let after = query(&source, &selected, &credential,
            "MATCH(n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        assert_eq!(before["rows"], after["rows"], "queued delta must preserve the intact FILE");
        assert_eq!(count(&query(&source,&selected,&credential,
            "MATCH(n:Function) WHERE n.name='delta_beta' RETURN count(n)").await), "1");
        Ok::<(), Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    supervisor.take().unwrap().stop().await?;
    let _ = release.send(());
    endpoint.await??;
    tasks.shutdown().await?;
    owner.shutdown().await?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

// Exercise the actual binary's package handler; no duplicate import codec.
#[allow(
    dead_code,
    reason = "This fixture calls import; the binary consumes the other CLI operations"
)]
#[path = "package.rs"]
mod package_cli;

fn headers(credential: &str) -> HeaderMap {
    HeaderMap::from_iter([
        (
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {credential}")).unwrap(),
        ),
        (
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            HeaderValue::from_static("runtime"),
        ),
    ])
}
fn context(selected: &Selected) -> ProjectContext {
    ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    }
}
async fn query(
    source: &SourceServer,
    selected: &Selected,
    credential: &str,
    cypher: &str,
) -> Value {
    let result = source
        .graph_server()
        .call_mcp(
            CallToolRequestParams::new("graphit_ast_query").with_arguments(
                json!({
                    "project_dir": NativePath::new(selected.0.root().to_owned()).unwrap(),
                    "project_version": selected.2.version(), "query": cypher,
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            headers(credential),
            CancellationToken::new(),
        )
        .await;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.unwrap()
}
async fn lexical(
    source: &SourceServer,
    selected: &Selected,
    credential: &str,
    term: &str,
) -> Value {
    let result = source.graph_server().call_search_mcp(
        source,
        CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
            "project_dir": NativePath::new(selected.0.root().to_owned()).unwrap(),
            "project_version": selected.2.version(), "query":term,"mode":"fts","rerank":false,
        }).as_object().unwrap().clone()),
        headers(credential), CancellationToken::new(),
    ).await;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.unwrap()
}
fn count(result: &Value) -> &str {
    result["rows"][0][0]["value"].as_str().unwrap()
}

async fn directory_delta(
    ast: &Arc<ForegroundAstSources>,
    selected: &Selected,
    paths: &[&str],
    cancel: &Cancellation,
) -> TestResult {
    ast.index(
        &selected.0,
        &context(selected),
        ChangeBatch {
            paths: paths
                .iter()
                .map(|path| selected.0.root().join(path))
                .collect(),
            ..Default::default()
        },
        cancel,
    )
    .await?;
    ast.index_semantic(
        selected.0.clone(),
        context(selected),
        None,
        Some(paths.iter().map(|path| (*path).to_owned()).collect()),
        std::collections::BTreeSet::new(),
        false,
        cancel.clone(),
        cancel.clone(),
    )
    .await?;
    Ok(())
}

async fn export_and_read_current_package(
    source: &SourceServer,
    auth: &graphit_daemon::auth_startup::DaemonAuth,
    selected: &Selected,
    credential: &str,
    config: &ConfigSnapshot,
    expected: &Value,
    directory: &std::path::Path,
) -> TestResult {
    use graphit_hub::{
        ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
        native_package::PackageKind,
        registry::{Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
    };
    let cancel = Cancellation::default();
    let policy = graphit_hub::policy::HubResourcePolicy::resolve(config)?;
    let original = policy.request_context(Instant::now(), &cancel)?;
    let archive = directory.join("current.zip");
    let arguments = crate::ast_export_entry::Arguments {
        output: archive.clone(),
        project_dir: Some(selected.0.root().to_owned()),
        context: None,
    };
    let prepared =
        crate::ast_export_entry::prepare(&arguments, Some(selected.2.version()), &original)?;
    let result = source
        .graph_server()
        .call_mcp(prepared, headers(credential), CancellationToken::new())
        .await;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    assert_eq!(result.structured_content.unwrap()["status"], "exported");
    let installed = directory.join("installed");
    assert_eq!(
        package_cli::run(
            package_cli::PhysicalCommand::Import {
                input: archive,
                output: installed.clone(),
                kind: Some(PackageKind::Ast)
            },
            config,
            &Cancellation::default()
        ),
        std::process::ExitCode::SUCCESS
    );
    let projection = source.policy().registry.limits().projection();
    let native = SourceRef::local_link(
        NativePath::new(installed)?,
        selected.1,
        ArtifactType::Ast,
        ArtifactId::new("fixture/progressive-ast", projection)?,
        ConcreteVersion::new("after-delta", projection)?,
        projection,
    )?;
    source
        .policy()
        .registry
        .writer(Duration::from_secs(5), &Cancellation::default())?
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Ast,
            id: native.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: native,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })?;
    let context = "fixture/progressive-ast@after-delta";
    let graph=source.graph_server().call_mcp(CallToolRequestParams::new("graphit_ast_query").with_arguments(json!({"context":context,"query":"MATCH(n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256"}).as_object().unwrap().clone()),headers(credential),CancellationToken::new()).await;
    assert_ne!(graph.is_error, Some(true), "{graph:?}");
    assert_eq!(
        graph.structured_content.unwrap()["rows"],
        expected["rows"],
        "the explicit package contains the current graph with intact original FILE metadata"
    );
    for mode in ["fts", "semantic"] {
        let response = source
            .graph_server()
            .call_search_mcp(
                source,
                CallToolRequestParams::new("graphit_ast_search").with_arguments(
                    json!({"context":context,"query":"alpha","mode":mode,"rerank":false})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                headers(credential),
                CancellationToken::new(),
            )
            .await;
        assert_ne!(response.is_error, Some(true), "{mode}: {response:?}");
        let payload = response.structured_content.unwrap();
        assert_eq!(payload["status"], "available", "{payload}");
        assert!(
            payload["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["path"] == "a.rs" && row["name"] == "alpha")
        );
    }
    let selected_source = tokio::task::block_in_place(|| -> TestResult<String> {
        let id = ArtifactId::new("fixture/progressive-ast", projection)?;
        let version = ConcreteVersion::new("after-delta", projection)?;
        let graph = graphit_ast::imported_graph::open_imported_graph_runtime(
            auth.issuer(),
            credential,
            graphit_ast::imported_graph::ImportedGraphInput {
                registry: &source.policy().registry,
                id: &id,
                version: &version,
                installation: auth.installation(),
                file_policy: &source.policy().file,
                read_limits: source.policy().read,
                graph_policy: source.policy().native_install.graph,
                directory_limits: source.policy().index,
                request: &original,
            },
        )?;
        let parameters: crate::ast_source::SourceParameters =
            serde_json::from_value(json!({"path":"a.rs","entity":"alpha"}))?;
        let runtime = tokio::runtime::Handle::current();
        Ok(runtime
            .block_on(graph.entity_source(
                &runtime,
                &parameters.path,
                parameters.entity.as_deref().unwrap(),
                parameters.entity_type.as_deref(),
                &parameters.selection(),
                source.policy().graph_query,
            ))?
            .selection
            .source)
    })?;
    assert!(selected_source.contains("pub fn alpha() -> i32 { 7 }"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn progressive_first_file_and_incremental_mcp_reads_do_not_wait_for_eof_or_models()
-> TestResult {
    let (fixture, mut owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner
        .memory_job_resources
        .upgrade()
        .expect("real CPU model resources");
    owner
        .server
        .as_mut()
        .unwrap()
        .attach_semantic_model(Some(Arc::clone(&resources)))?;
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    fs::write(
        selected.0.root().join("a.rs"),
        "pub fn alpha() -> i32 { 7 }\n",
    )?;
    fs::write(
        selected.0.root().join("b.rs"),
        "pub fn beta() -> i32 { 3 }\n",
    )?;
    let no_docker = tempfile::tempdir()?;
    let package_directory = tempfile::tempdir()?;
    let original = owner.policy.request_context(Instant::now(), &cancel)?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let result = std::panic::AssertUnwindSafe(async {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let unavailable_address = listener.local_addr()?;
        drop(listener);
        let mut unavailable = MemoryJobResources::remote(
            Arc::clone(&resources.auth), resources.http.clone(),
            Arc::clone(&resources.brokers), resources.policy.clone(),
            resources.scope, resources.native, resources.binding,
        )?;
        let local = resources.local.as_ref().expect("fixture uses the actual CPU child");
        unavailable.local = Some(OwnedLocalEmbedding {
            client: Arc::clone(&local.client), expected: Arc::clone(&local.expected),
            address: unavailable_address,
        });
        let unavailable = Arc::new(unavailable);
        ast.index(&selected.0, &context(&selected), ChangeBatch { rescan:true,..Default::default() }, &cancel).await?;
        let (first, release) = ast.hold_after_file_publication("a.rs");
        let producer = Arc::clone(&ast);
        let project = selected.0.clone();
        let selected_context = context(&selected);
        let cancellation = cancel.clone();
        let job = tokio::spawn(async move {
            producer.index_semantic(project, selected_context, None, None, std::collections::BTreeSet::new(), false, cancellation.clone(), cancellation).await
        });
        tokio::time::timeout(Duration::from_secs(30), first).await??;
        assert!(!job.is_finished(), "the corpus must still be held before EOF");
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH (n:Function) WHERE n.name='alpha' RETURN count(n)").await),"1");
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH (n:Function) WHERE n.name='beta' RETURN count(n)").await),"0");
        let found = lexical(&source,&selected,&credential,"alpha").await;
        assert_eq!(found["status"],"available");
        assert!(!found["results"].as_array().unwrap().is_empty());
        // Hybrid must preserve the first FILE's lexical results before NN
        // exists, without consulting the selected unavailable model endpoint.
        let mut configured = source.clone();
        configured.attach_semantic_model(Some(Arc::clone(&unavailable)))?;
        let hybrid = source.graph_server().call_search_mcp(
            &source,
            CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
                "project_dir": NativePath::new(selected.0.root().to_owned())?,
                "project_version": selected.2.version(),
                "query":"alpha","mode":"hybrid","rerank":false,
            }).as_object().unwrap().clone()),
            headers(&credential), CancellationToken::new(),
        ).await;
        configured.attach_semantic_model(Some(Arc::clone(&resources)))?;
        assert_ne!(hybrid.is_error,Some(true),"{hybrid:?}");
        let hybrid = hybrid.structured_content.unwrap();
        assert_eq!(hybrid["status"],"available");
        assert_eq!(hybrid["vector_status"],"index_absent");
        assert!(!hybrid["results"].as_array().unwrap().is_empty());
        let a = query(&source,&selected,&credential,
            "MATCH (n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        release.send(()).unwrap();
        job.await??;
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH (n:Function) RETURN count(n)").await),"2");

        fs::write(selected.0.root().join("b.rs"), "pub fn changed_beta() -> i32 { 5 }\n")?;
        let started = Instant::now();
        ast.index(&selected.0, &context(&selected), ChangeBatch {
            paths:BTreeSet::from([selected.0.root().join("b.rs")]),..Default::default()
        }, &cancel).await?;
        ast.index_semantic(selected.0.clone(),context(&selected),None,
            Some(BTreeSet::from(["b.rs".into()])), std::collections::BTreeSet::new(), false,cancel.clone(), cancel.clone()).await?;
        eprintln!("actual one-FILE Raw/graph/FTS incremental: {:?}", started.elapsed());
        let still_a = query(&source,&selected,&credential,
            "MATCH (n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        assert_eq!(a["rows"],still_a["rows"],"intact FILE identity and historical Raw observation must stay unchanged");
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH (n:Function) WHERE n.name='beta' RETURN count(n)").await),"0");
        assert!(!lexical(&source,&selected,&credential,"changed_beta").await["results"].as_array().unwrap().is_empty());
        let parameters = serde_json::from_value(json!({"path":"a.rs","entity":"alpha"}))?;
        let inspected = source.graph_server().inspect_source_native(
            (&selected.0,selected.1,&selected.2), &parameters,&credential,false,&original,
        ).await?;
        assert!(serde_json::to_string(&inspected)?.contains("pub fn alpha"));

        fs::rename(selected.0.root().join("b.rs"),selected.0.root().join("c.rs"))?;
        ast.index(&selected.0,&context(&selected),ChangeBatch {
            paths:BTreeSet::from([selected.0.root().join("b.rs"),selected.0.root().join("c.rs")]),..Default::default()
        },&cancel).await?;
        ast.index_semantic(selected.0.clone(),context(&selected),None,
            Some(BTreeSet::from(["b.rs".into(),"c.rs".into()])), std::collections::BTreeSet::new(), false,cancel.clone(), cancel.clone()).await?;
        let renamed = query(&source,&selected,&credential,"MATCH (n:Function) WHERE n.name='changed_beta' RETURN n.path").await;
        assert_eq!(renamed["rows"][0][0]["value"],"c.rs");
        fs::remove_file(selected.0.root().join("c.rs"))?;
        ast.index(&selected.0,&context(&selected),ChangeBatch {
            paths:BTreeSet::from([selected.0.root().join("c.rs")]),..Default::default()
        },&cancel).await?;
        ast.index_semantic(selected.0.clone(),context(&selected),None,
            Some(BTreeSet::from(["c.rs".into()])), std::collections::BTreeSet::new(), false,cancel.clone(), cancel.clone()).await?;
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH (n:Function) RETURN count(n)").await),"1");
        assert!(lexical(&source,&selected,&credential,"changed_beta").await["results"].as_array().unwrap().is_empty());
        // Directory events select only their subtrees, including two roots in
        // one event. No full rescan or reparse of the intact A is needed.
        for (path, name) in [("dir_%/one.rs","left_one"),("dir_%other/λ.rs","right_two")] {
            let target=selected.0.root().join(path);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::write(target,format!("pub fn {name}() {{}}\n"))?;
        }
        directory_delta(&ast,&selected,&["dir_%","dir_%other"],&cancel).await?;
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH(n:Function) RETURN count(n)").await),"3");
        let sibling_before=query(&source,&selected,&credential,"MATCH(n:Function) WHERE n.name='right_two' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        fs::rename(selected.0.root().join("dir_%"),selected.0.root().join("moved_%"))?;
        directory_delta(&ast,&selected,&["dir_%","moved_%"],&cancel).await?;
        let moved=query(&source,&selected,&credential,"MATCH(n:Function) WHERE n.name='left_one' RETURN n.path").await;
        assert_eq!(moved["rows"][0][0]["value"],"moved_%/one.rs");
        let sibling_after=query(&source,&selected,&credential,"MATCH(n:Function) WHERE n.name='right_two' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        assert_eq!(sibling_before["rows"],sibling_after["rows"],"literal subtree rename must preserve a similarly prefixed sibling and its Unicode file");
        for (path,name) in [("moved_%/one.rs","left_changed"),("dir_%other/λ.rs","right_changed")] {
            fs::write(selected.0.root().join(path),format!("pub fn {name}() {{}}\n"))?;
        }
        directory_delta(&ast,&selected,&["moved_%","dir_%other"],&cancel).await?;
        for term in ["left_changed","right_changed"] {
            assert!(!lexical(&source,&selected,&credential,term).await["results"].as_array().unwrap().is_empty());
        }
        for path in ["moved_%","dir_%other"] { fs::remove_dir_all(selected.0.root().join(path))?; }
        directory_delta(&ast,&selected,&["moved_%","dir_%other"],&cancel).await?;
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH(n:Function) RETURN count(n)").await),"1");
        let after_directories=query(&source,&selected,&credential,"MATCH (n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        assert_eq!(a["rows"],after_directories["rows"]);
        // A failed NN service must leave the completed graph/FTS publication and
        // durable per-FILE projections intact. Recovery consumes that backlog,
        // without reparsing the unchanged FILE.
        let failure = ast.index_semantic(selected.0.clone(), context(&selected),
            Some(Arc::clone(&unavailable)), Some(BTreeSet::new()), std::collections::BTreeSet::new(), false, cancel.clone(), cancel.clone())
            .await.expect_err("the selected NN endpoint is unavailable");
        assert!(crate::foreground_ast_sources::semantic::only_inference_failed(&failure), "{failure:?}");
        assert_eq!(count(&query(&source,&selected,&credential,"MATCH (n:Function) RETURN count(n)").await), "1");
        assert!(!lexical(&source,&selected,&credential,"alpha").await["results"].as_array().unwrap().is_empty());
        let observed = owner.policy.request_context(Instant::now(), &cancel)?;
        let capture = CapturedPublicationScope::project(&selected.0, selected.1, owner.scope, observed.snapshot())?;
        let record = capture.binding()?;
        let (root, _namespace) = crate::foreground_ast_sources::progressive::location(
            owner.auth.installation(), graphit_auth::ProjectCapture {
                scope: graphit_auth::ScopeCapture { live: &capture, record: &record },
                version: &selected.2,
            },
        )?;
        let pending_uri = root.parent().unwrap().parent().unwrap().join("corpus.lance");
        let pending_rows = async || -> TestResult<usize> {
            let snapshot = BranchSnapshot::capture_observed(pending_uri.to_str().unwrap(), None, None).await?;
            let mut scan = snapshot.scan();
            scan.project(&["path"])?;
            let mut predicate=graphit_data::lance_branch_store::col("projection").is_not_null();
            if snapshot.schema().field_with_name("vector").is_ok(){predicate=predicate.and(graphit_data::lance_branch_store::col("vector").is_null());}
            scan.filter_expr(predicate);
            let mut stream = scan.try_into_stream().await?;
            let mut rows = 0;
            while let Some(batch) = stream.try_next().await? { rows += batch.num_rows(); }
            Ok(rows)
        };
        assert!(pending_rows().await? > 0, "failed inference must remain durable");
        ast.index_semantic(selected.0.clone(), context(&selected), Some(Arc::clone(&resources)),
            Some(BTreeSet::new()), std::collections::BTreeSet::new(), false, cancel.clone(), cancel.clone()).await?;
        assert_eq!(pending_rows().await?, 0, "a completed NN FILE must acknowledge its pending projections");
        let semantic = source.graph_server().call_search_mcp(
            &source, CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
                "project_dir": NativePath::new(selected.0.root().to_owned()).unwrap(),
                "project_version":selected.2.version(), "query":"alpha", "mode":"semantic", "rerank":false,
            }).as_object().unwrap().clone()), headers(&credential), CancellationToken::new(),
        ).await;
        assert_ne!(semantic.is_error, Some(true), "{semantic:?}");
        let semantic = semantic.structured_content.unwrap();
        assert_eq!(semantic["status"], "available");
        assert!(!semantic["results"].as_array().unwrap().is_empty());
        let recovered_a = query(&source,&selected,&credential,
            "MATCH (n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        assert_eq!(a["rows"], recovered_a["rows"], "NN recovery must not reparse or republish an intact FILE");
        export_and_read_current_package(&source,&owner.auth,&selected,&credential,fixture.config(),&a,package_directory.path()).await?;
        // With no pending work and no model refresh, even an unavailable model
        // endpoint must not be consulted.
        ast.index_semantic(selected.0.clone(), context(&selected), Some(unavailable),
            Some(BTreeSet::from(["absent.unsupported".into()])), std::collections::BTreeSet::new(), false, cancel.clone(), cancel.clone()).await?;
        let denied = source.graph_server().call_mcp(
            CallToolRequestParams::new("graphit_ast_schema"), headers("invalid"), CancellationToken::new(),
        ).await;
        assert_eq!(denied.is_error,Some(true));
        Ok::<(),Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn progressive_oracle_targets_reconcile_intact_callers_on_subset_changes() -> TestResult {
    let (_fixture, owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    let config = ConfigStore::project(&selected.0)?;
    config.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    let queries = selected.0.brand_directory().join("ast/queries");
    fs::create_dir_all(&queries)?;
    for (name, stock) in [
        (
            "plsql.yaml",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../graphit-ast/profiles/queries/plsql.yaml"
            )),
        ),
        (
            "sql.yaml",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../graphit-ast/profiles/queries/sql.yaml"
            )),
        ),
    ] {
        fs::write(
            queries.join(name),
            stock.replace("exclusive: true", "exclusive: false"),
        )?;
    }
    const CALLER: &str =
        "CREATE PROCEDURE caller AS v NUMBER; BEGIN SELECT id INTO v FROM target_tab; END; /";
    const TARGET: &str = "CREATE TABLE target_tab(id NUMBER);";
    fs::write(selected.0.root().join("caller.sql"), CALLER)?;
    fs::write(selected.0.root().join("target.sql"), TARGET)?;
    let no_docker = tempfile::tempdir()?;
    let original = owner.policy.request_context(Instant::now(), &cancel)?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let result = std::panic::AssertUnwindSafe(async {
        ast.index(&selected.0,&context(&selected),ChangeBatch{rescan:true,..Default::default()},&cancel).await?;
        ast.index_semantic(selected.0.clone(),context(&selected),None,None, std::collections::BTreeSet::new(), false,cancel.clone(), cancel.clone()).await?;
        let markers = query(&source,&selected,&credential,
            "MATCH (n:AstFileObservation) WHERE n.path='caller.sql' RETURN n.lang,n.grammar,n.diagnostics_json,n.capabilities_json,n.relative_components_json,n.source_stored,n.source_empty,n.model_available ORDER BY n.lang").await;
        assert_eq!(markers["rows"].as_array().unwrap().len(),2,"both genuine non-exclusive parser profiles must be published");
        for row in markers["rows"].as_array().unwrap() {
            assert!(row.as_array().unwrap().iter().all(|value| !value["value"].is_null()),
                "both profiles must retain their diagnostics, capabilities and Source metadata");
        }
        let cypher = "MATCH (a:Procedure)-[:SELECTS]->(b:`Table`) WHERE a.lang='plsql' AND b.lang='plsql' RETURN count(a)";
        assert_eq!(count(&query(&source,&selected,&credential,cypher).await),"1");
        let caller = query(&source,&selected,&credential,
            "MATCH (n:Procedure) WHERE n.path='caller.sql' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256 ORDER BY n.uid").await;
        for (path, content, expected) in [
            ("duplicate.sql",Some(TARGET),"0"),
            ("duplicate.sql",None,"1"),
            ("target.sql",None,"0"),
            ("restored.sql",Some(TARGET),"1"),
        ] {
            let file = selected.0.root().join(path);
            match content { Some(source)=>fs::write(&file,source)?,None=>fs::remove_file(&file)? }
            ast.index(&selected.0,&context(&selected),ChangeBatch{paths:BTreeSet::from([file]),..Default::default()},&cancel).await?;
            ast.index_semantic(selected.0.clone(),context(&selected),None,Some(BTreeSet::from([path.into()])), std::collections::BTreeSet::new(), false,cancel.clone(), cancel.clone()).await?;
            assert_eq!(count(&query(&source,&selected,&credential,cypher).await),expected,"Oracle join after {path}");
            let intact = query(&source,&selected,&credential,
                "MATCH (n:Procedure) WHERE n.path='caller.sql' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256 ORDER BY n.uid").await;
            assert_eq!(caller["rows"],intact["rows"],"Oracle target updates must not reparse the intact caller");
            let intact_markers = query(&source,&selected,&credential,
                "MATCH (n:AstFileObservation) WHERE n.path='caller.sql' RETURN n.lang,n.grammar,n.diagnostics_json,n.capabilities_json,n.relative_components_json,n.source_stored,n.source_empty,n.model_available ORDER BY n.lang").await;
            assert_eq!(markers["rows"],intact_markers["rows"],
                "Oracle re-resolution must leave FILE properties outside the outcomes projection intact");
        }
        assert!(!lexical(&source,&selected,&credential,"caller").await["results"].as_array().unwrap().is_empty());
        Ok::<(),Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn structural_full_yields_between_files_and_prioritizes_a_later_delta_path() -> TestResult {
    let (fixture, owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let source = owner.server.as_ref().unwrap().clone();
    let lifetime = Cancellation::default();
    let config = ConfigStore::project(&selected.0)?;
    for (key, value) in [("modules.ast", true), ("modules.embedding", false)] {
        config.set(
            ConfigKey::new(key)?,
            json!(value),
            Duration::from_secs(5),
            &lifetime,
        )?;
    }
    for (path, name) in [
        ("a.rs", "alpha"),
        ("b.rs", "beta"),
        ("c.rs", "gamma"),
        ("z.rs", "old_zeta"),
    ] {
        fs::write(
            selected.0.root().join(path),
            format!("pub fn {name}() {{}}\n"),
        )?;
    }
    let no_docker = tempfile::tempdir()?;
    let original = owner.policy.request_context(Instant::now(), &lifetime)?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let result = std::panic::AssertUnwindSafe(async {
        ast.index(&selected.0, &context(&selected), ChangeBatch { rescan: true, ..Default::default() }, &lifetime).await?;
        let (first_committed, release_first) = ast.hold_after_file_publication("a.rs");
        let background = Cancellation::default();
        let producer = Arc::clone(&ast);
        let project = selected.0.clone();
        let selected_context = context(&selected);
        let job_lifetime = lifetime.clone();
        let job_background = background.clone();
        let first = tokio::spawn(async move {
            producer.index_semantic(project, selected_context, None, None, BTreeSet::new(), false, job_background, job_lifetime).await
        });
        tokio::time::timeout(Duration::from_secs(30), first_committed).await??;
        assert!(!first.is_finished(), "Source is held between committed FILEs, without any NN request");
        let alpha = query(&source, &selected, &credential,
            "MATCH(n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await;
        assert_eq!(alpha["rows"].as_array().unwrap().len(), 1);
        assert_eq!(count(&query(&source,&selected,&credential,
            "MATCH(n:Function) WHERE n.name='beta' RETURN count(n)").await), "0");
        // This is the same separate token signalled by a queued watcher event.
        // It must not interrupt A's commit, but B/C/Z must not be parsed first.
        background.cancel();
        release_first.send(()).map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        let progress = tokio::time::timeout(Duration::from_secs(30), first).await???;
        for path in ["b.rs", "c.rs", "z.rs"] {
            assert!(progress.pending_files.contains(path), "unprocessed {path} is retained");
        }
        assert_eq!(count(&query(&source,&selected,&credential,
            "MATCH(n:Function) WHERE n.name='beta' RETURN count(n)").await), "0");
        fs::write(selected.0.root().join("z.rs"), "pub fn latest_zeta() {}\n")?;
        ast.index(&selected.0,&context(&selected),ChangeBatch {
            paths:BTreeSet::from([selected.0.root().join("z.rs")]),..Default::default()
        },&lifetime).await?;
        let (delta_committed, release_delta) = ast.hold_after_file_publication("z.rs");
        let producer = Arc::clone(&ast);
        let project = selected.0.clone();
        let selected_context = context(&selected);
        let job_lifetime = lifetime.clone();
        let second = tokio::spawn(async move {
            producer.index_semantic(project, selected_context, None, Some(progress.pending_files),
                BTreeSet::from(["z.rs".to_owned()]), false, Cancellation::default(), job_lifetime).await
        });
        tokio::time::timeout(Duration::from_secs(30), delta_committed).await??;
        assert!(!second.is_finished(), "the full tail has not reached EOF");
        assert_eq!(count(&query(&source,&selected,&credential,
            "MATCH(n:Function) WHERE n.name='latest_zeta' RETURN count(n)").await), "1");
        assert!(!lexical(&source,&selected,&credential,"latest_zeta").await["results"].as_array().unwrap().is_empty());
        assert_eq!(count(&query(&source,&selected,&credential,
            "MATCH(n:Function) WHERE n.name='beta' OR n.name='gamma' RETURN count(n)").await), "0",
            "delta Z is processed before alphabetically earlier full-tail FILEs");
        assert_eq!(alpha["rows"], query(&source,&selected,&credential,
            "MATCH(n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256").await["rows"],
            "the already committed FILE retains its actual historical Raw association");
        release_delta.send(()).map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        let progress = tokio::time::timeout(Duration::from_secs(30), second).await???;
        assert!(progress.pending_files.is_empty());
        assert_eq!(count(&query(&source,&selected,&credential,
            "MATCH(n:Function) WHERE n.name='beta' OR n.name='gamma' RETURN count(n)").await), "2");
        Ok::<(), Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    lifetime.cancel();
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    drop(fixture);
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
