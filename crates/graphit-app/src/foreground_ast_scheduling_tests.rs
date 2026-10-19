//! Native watcher queue -> actual Source consumer -> progressive FILE publication.
use super::*;
use axum::http::{HeaderMap, HeaderValue, header};
use futures_util::FutureExt;
use graphit_core::native_path::NativePath;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;
use std::{collections::BTreeSet, fs};
use tokio::sync::oneshot;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn read(
    source: &SourceServer,
    selected: &(ProjectPaths, ProjectId, VersionContext),
    credential: &str,
    query: &str,
    lexical: bool,
) -> Value {
    let mut input = json!({
        "project_dir": NativePath::new(selected.0.root().to_owned()).unwrap(),
        "project_version": selected.2.version(),
        "query": query,
    });
    if lexical {
        input["mode"] = json!("fts");
        input["rerank"] = json!(false);
    }
    let params = CallToolRequestParams::new(if lexical {
        "graphit_ast_search"
    } else {
        "graphit_ast_query"
    })
    .with_arguments(input.as_object().unwrap().clone());
    let headers = HeaderMap::from_iter([
        (
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {credential}")).unwrap(),
        ),
        (
            header::HeaderName::from_static("x-graphit-auth-origin"),
            HeaderValue::from_static("runtime"),
        ),
    ]);
    let result = if lexical {
        source
            .graph_server()
            .call_search_mcp(source, params, headers, CancellationToken::new())
            .await
    } else {
        source
            .graph_server()
            .call_mcp(params, headers, CancellationToken::new())
            .await
    };
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.unwrap()
}

fn full_completion(owner: &ForegroundSource) -> oneshot::Receiver<()> {
    let (sender, receiver) = oneshot::channel();
    assert!(
        owner
            .ast_full_completed
            .lock()
            .unwrap()
            .replace(sender)
            .is_none()
    );
    receiver
}

async fn observation(
    owner: &ForegroundSource,
    selected: &(ProjectPaths, ProjectId, VersionContext),
    original: &RequestContext<'_>,
    path: &str,
) -> TestResult<graphit_ast::live_graph::LiveFileObservation> {
    use graphit_auth::{ProjectCapture, ScopeCapture};
    let captured = CapturedPublicationScope::project(
        &selected.0,
        selected.1,
        owner.scope,
        original.snapshot(),
    )?;
    let record = captured.binding()?;
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &captured,
            record: &record,
        },
        version: &selected.2,
    };
    let source = owner.server.as_ref().unwrap();
    let registry = source.graph_server().live_ast();
    let (index, _directory) = registry
        .capture(
            owner.auth.installation(),
            project,
            owner.native,
            original.snapshot(),
            false,
        )
        .await?
        .ok_or("current graph absent")?;

    index
        .graph
        .file_observation(path, original.snapshot())?
        .ok_or_else(|| "current FILE absent".into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_rescan_with_file_delta_publishes_before_full_tail_and_resumes_full() -> TestResult {
    let (_fixture, installation, project, config, policy) = ast_fixture();
    let cancellation = Cancellation::default();
    ConfigStore::project(&project)?.set(
        ConfigKey::new("modules.knowledge")?,
        json!(false),
        Duration::from_secs(5),
        &cancellation,
    )?;
    for (path, body) in [
        ("a.rs", "pub fn old_alpha() {}\n"),
        ("gone/item.lua", "function removed_nested() end\n"),
        ("gone2/item.lua", "function kept_sibling() end\n"),
    ] {
        let path = project.root().join(path);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, body)?;
    }
    let no_docker = tempfile::tempdir()?;
    let environment = vec![("PATH".into(), no_docker.path().as_os_str().to_owned())];
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let inputs = [ProjectBootstrapInput {
        project: &project,
        explicit_version: "",
        pin: None,
        detached_base: "",
    }];
    let mut owner = ForegroundSource::start_with_inventory(
        &installation,
        &inputs,
        0,
        &config,
        NativeBinding::Codex,
        &original,
        None,
    )?;
    let source = owner.server.as_ref().unwrap().clone();
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let context = graphit_daemon::supervisor::ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: installation.clone(),
            base: config.clone(),
            policy: policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::Codex,
            environment: environment.clone(),
        },
        original.snapshot(),
    )?);
    owner.ast_jobs = Some(Arc::clone(&ast));
    let credential = {
        let key = owner.auth.runtime().check(original.snapshot())?;
        std::str::from_utf8(key.secret(original.snapshot())?)?.to_owned()
    };
    let result = std::panic::AssertUnwindSafe(async {
        // A real prior cohort supplies the untouched FILEs and tombstone targets.
        ast.index_productive(&project, &context, graphit_daemon::watcher::ChangeBatch { rescan: true, ..Default::default() },
            None, Some(BTreeSet::new()), false, &cancellation, &cancellation, &cancellation).await?;
        fs::write(project.root().join("a.rs"), "pub fn alpha() {}\n")?;
        fs::write(project.root().join("b.rs"), "pub fn beta_tail() {}\n")?;
        fs::write(project.root().join("c.rs"), "pub fn gamma_tail() {}\n")?;
        let (first, release_first) = ast.hold_after_file_publication("a.rs");
        let yielded = ast.observe_next_raw_error();
        let mut completed = full_completion(&owner);
        owner.attach_jobs(&installation, &config, NativeBinding::Codex, &environment, &original).await?;
        tokio::time::timeout(Duration::from_secs(30), first).await??;
        let alpha_query = "MATCH(n:Function) WHERE n.name='alpha' RETURN n.uid,n.raw_version,n.source_sha256";
        let alpha = read(&source, &selected, &credential, alpha_query, false).await;
        assert_eq!(alpha["rows"].as_array().unwrap().len(), 1);
        let handle = owner.supervisors.rescan_handle(&context.key()).unwrap();
        let (delta, release_delta) = ast.hold_after_file_publication("z_probe.lua");
        let batch = ast.observe_next_raw_batch();
        fs::write(project.root().join("z_probe.lua"), "function delta_probe() end\n")?;
        fs::remove_dir_all(project.root().join("gone"))?;
        assert!(handle.request_rescan());
        // Keep the current FILE held while the real native debouncer delivers
        // its FILE/subtree events and the rescan hint is coalesced in the queue.
        tokio::time::sleep(SOURCE_WATCH_DEBOUNCE * 3).await;
        release_first.send(()).unwrap();
        assert_eq!(tokio::time::timeout(Duration::from_secs(30), yielded).await??, io::ErrorKind::Interrupted);
        let (rescan, paths) = tokio::time::timeout(Duration::from_secs(30), batch).await??;
        assert!(rescan);
        assert!(paths.contains(&project.root().join("z_probe.lua")), "the queued full hint must retain the FILE delta");
        tokio::time::timeout(Duration::from_secs(30), delta).await??;
        let delta_observation = observation(&owner, &selected, &original, "z_probe.lua").await?;
        assert!(matches!(completed.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
        let rows = read(&source, &selected, &credential,
            "MATCH(n:Function) WHERE n.name='beta_tail' OR n.name='gamma_tail' OR n.name='removed_nested' RETURN n.name", false).await;
        assert!(rows["rows"].as_array().unwrap().is_empty(), "delta/tombstone publication precedes the unprocessed full tail");
        let hits = read(&source, &selected, &credential, "delta_probe", true).await;
        assert!(hits["results"].as_array().unwrap().iter().any(|hit| hit["path"] == "z_probe.lua" && hit["name"] == "delta_probe"));
        let removed = read(&source, &selected, &credential, "removed_nested", true).await;
        assert!(!removed["results"].as_array().unwrap().iter().any(|hit| hit["path"] == "gone/item.lua"));
        release_delta.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(30), &mut completed).await??;
        assert_eq!(delta_observation, observation(&owner, &selected, &original, "z_probe.lua").await?,
            "the delta's completed syntax must not be reparsed by its semantic phase or the full resume");
        assert_eq!(alpha["rows"], read(&source, &selected, &credential, alpha_query, false).await["rows"]);
        let tail = read(&source, &selected, &credential,
            "MATCH(n:Function) WHERE n.name='beta_tail' OR n.name='gamma_tail' OR n.name='kept_sibling' RETURN n.name", false).await;
        assert_eq!(tail["rows"].as_array().unwrap().len(), 3);

        // The full obligation is now clear. A NEW topology/rescan request with
        // an explicit delta must create and finish another full obligation.
        let (acknowledged, release_acknowledged) = ast.hold_after_file_publication("m_ack.lua");
        fs::write(project.root().join("m_ack.lua"), "function acknowledged_delta() end\n")?;
        tokio::time::timeout(Duration::from_secs(30), acknowledged).await??;
        let (second, release_second) = ast.hold_after_file_publication("z_second.lua");
        let batch = ast.observe_next_raw_batch();
        let mut completed = full_completion(&owner);
        fs::write(project.root().join("z_second.lua"), "function second_delta() end\n")?;
        fs::remove_dir_all(project.root().join("gone2"))?;
        assert!(handle.request_rescan());
        tokio::time::sleep(SOURCE_WATCH_DEBOUNCE * 3).await;
        release_acknowledged.send(()).unwrap();
        let (rescan, paths) = tokio::time::timeout(Duration::from_secs(30), batch).await??;
        assert!(rescan);
        assert!(paths.contains(&project.root().join("z_second.lua")));
        tokio::time::timeout(Duration::from_secs(30), second).await??;
        assert!(matches!(completed.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
        let hits = read(&source, &selected, &credential, "second_delta", true).await;
        assert!(hits["results"].as_array().unwrap().iter().any(|hit| hit["path"] == "z_second.lua"));
        release_second.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(30), &mut completed).await??;
        let removed = read(&source, &selected, &credential,
            "MATCH(n:Function) WHERE n.name='removed_nested' OR n.name='kept_sibling' RETURN n.name", false).await;
        assert!(removed["rows"].as_array().unwrap().is_empty());
        assert_eq!(alpha["rows"], read(&source, &selected, &credential, alpha_query, false).await["rows"]);
        TestResult::Ok(())
    }).catch_unwind().await;
    cancellation.cancel();
    owner.shutdown().await.map_err(|failure| failure.error)?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn records_notification_preserves_inflight_full_scan_and_remaining_files() -> TestResult {
    let (_fixture, installation, project, config, policy) = ast_fixture();
    let cancellation = Cancellation::default();
    ConfigStore::project(&project)?.set(
        ConfigKey::new("modules.knowledge")?,
        json!(false),
        Duration::from_secs(5),
        &cancellation,
    )?;
    for (path, body) in [
        ("a.lua", "function alpha_records() return 1 end\n"),
        ("b.lua", "function beta_records() return 2 end\n"),
        ("c.lua", "function gamma_records() return 3 end\n"),
    ] {
        fs::write(project.root().join(path), body)?;
    }
    let no_docker = tempfile::tempdir()?;
    let environment = vec![("PATH".into(), no_docker.path().as_os_str().to_owned())];
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let inputs = [ProjectBootstrapInput {
        project: &project,
        explicit_version: "",
        pin: None,
        detached_base: "",
    }];
    let mut owner = ForegroundSource::start_with_inventory(
        &installation,
        &inputs,
        0,
        &config,
        NativeBinding::Codex,
        &original,
        None,
    )?;
    let source = owner.server.as_ref().unwrap().clone();
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let context = graphit_daemon::supervisor::ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: installation.clone(),
            base: config.clone(),
            policy: policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::Codex,
            environment: environment.clone(),
        },
        original.snapshot(),
    )?);
    owner.ast_jobs = Some(Arc::clone(&ast));
    let credential = {
        let key = owner.auth.runtime().check(original.snapshot())?;
        std::str::from_utf8(key.secret(original.snapshot())?)?.to_owned()
    };
    let result = std::panic::AssertUnwindSafe(async {
        let (first, release_first) = ast.hold_after_file_publication("a.lua");
        let background = ast.observe_next_background();
        let mut completed = full_completion(&owner);
        owner
            .attach_jobs(
                &installation,
                &config,
                NativeBinding::Codex,
                &environment,
                &original,
            )
            .await?;
        tokio::time::timeout(Duration::from_secs(30), first).await??;
        let first_observation = observation(&owner, &selected, &original, "a.lua").await?;
        let tail = read(
            &source,
            &selected,
            &credential,
            "MATCH(n:Function) WHERE n.name='beta_records' OR n.name='gamma_records' RETURN n.name",
            false,
        )
        .await;
        assert!(tail["rows"].as_array().unwrap().is_empty());
        let mut another_raw_batch = ast.observe_next_raw_batch();
        let mut raw_error = ast.observe_next_raw_error();
        let handle = owner.supervisors.rescan_handle(&context.key()).unwrap();
        assert!(handle.request_records());
        // The actual supervisor queues this event while FILE A is still held.
        // Its background cancellation must not preempt the retained Raw iterator.
        let background = tokio::time::timeout(Duration::from_secs(30), background).await??;
        tokio::time::timeout(Duration::from_secs(30), background.cancelled()).await?;
        release_first.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(30), &mut completed).await??;
        assert_eq!(
            first_observation,
            observation(&owner, &selected, &original, "a.lua").await?
        );
        for path in ["b.lua", "c.lua"] {
            observation(&owner, &selected, &original, path).await?;
        }
        let tail = read(
            &source,
            &selected,
            &credential,
            "MATCH(n:Function) WHERE n.name='beta_records' OR n.name='gamma_records' RETURN n.name",
            false,
        )
        .await;
        assert_eq!(tail["rows"].as_array().unwrap().len(), 2);
        let hits = read(&source, &selected, &credential, "gamma_records", true).await;
        assert!(
            hits["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|hit| hit["path"] == "c.lua" && hit["name"] == "gamma_records")
        );
        assert!(
            matches!(
                another_raw_batch.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ),
            "Records must not admit another Raw batch to finish the full pass"
        );
        assert!(
            matches!(
                raw_error.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ),
            "the record notification must not yield the full Raw publication"
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    cancellation.cancel();
    let compiler_cleanup = ast.shutdown().await;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
