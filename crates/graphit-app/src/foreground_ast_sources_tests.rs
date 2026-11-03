use super::*;
use graphit_ast::indexed_source::{read_branch_source, source_schema};
use graphit_core::{
    brand::Brand,
    config::{ConfigDocument, ConfigSource, ConfigStore},
    project_identity::{ProjectIdentityStore, ProjectMetadata},
};
use graphit_data::lance_branch_store::LanceBranchStore;
use serde_json::json;

#[path = "foreground_ast_hotreload_tests.rs"]
mod hotreload_tests;

#[path = "foreground_ast_gemma_tests.rs"]
mod gemma;

#[path = "foreground_ast_checkpoint_tests.rs"]
mod checkpoint_tests;

#[path = "foreground_ast_recipe_tests.rs"]
mod recipe_tests;

#[path = "foreground_ast_scheduling_tests.rs"]
mod scheduling;

fn original_error_source<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a (dyn std::error::Error + 'static)> {
    // io::Error exposes its wrapped object through get_ref; source() may skip that object.
    if let Some(error) = error.downcast_ref::<io::Error>() {
        return error
            .get_ref()
            .map(|inner| inner as &(dyn std::error::Error + 'static))
            .or_else(|| std::error::Error::source(error));
    }
    if let Some(error) = error.downcast_ref::<Arc<io::Error>>() {
        return Some(error.as_ref());
    }
    error.source()
}

fn assert_cancelled_raw(
    error: &io::Error,
) -> &crate::foreground_ast_sources::RawPublicationFailure {
    let raw = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<crate::foreground_ast_sources::RawPublicationFailure>())
        .expect("shutdown retains the actual Raw error");
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&raw.cause);
    let mut cancelled = false;
    while let Some(error) = cause {
        cancelled |= error
            .downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == io::ErrorKind::Interrupted)
            || matches!(
                error.downcast_ref::<graphit_auth::AuthError>(),
                Some(graphit_auth::AuthError::Request)
            );
        cause = original_error_source(error);
    }
    assert!(
        cancelled,
        "only real native/request cancellation is expected during stop: {raw:?}"
    );
    raw
}

fn ast_fixture() -> (
    tempfile::TempDir,
    InstallationPaths,
    ProjectPaths,
    ConfigSnapshot,
    HubResourcePolicy,
) {
    let temp = tempfile::tempdir().unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let project = ProjectPaths::at(&root, &installation).unwrap();
    let cancel = Cancellation::default();
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("actual-ast-only").unwrap())
        .unwrap();
    let config = ConfigSnapshot::new(
        installation.brand(),
        [ConfigDocument::new(
            installation.brand(),
            ConfigSource::Global,
            [
                (ConfigKey::new("modules.ast").unwrap(), json!(true)),
                (ConfigKey::new("modules.memory").unwrap(), json!(false)),
                (ConfigKey::new("modules.embedding").unwrap(), json!(false)),
                (ConfigKey::new("modules.dream").unwrap(), json!(false)),
            ],
        )
        .unwrap()],
    )
    .unwrap();
    // Persisted global layer is re-read by the job; retain the real same settings there.
    let global = ConfigStore::global(&installation).unwrap();
    for (key, value) in [
        ("modules.ast", true),
        ("modules.memory", false),
        ("modules.embedding", false),
        ("modules.dream", false),
    ] {
        global
            .set(
                ConfigKey::new(key).unwrap(),
                json!(value),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
    }
    let policy = HubResourcePolicy::resolve(&config).unwrap();
    (temp, installation, project, config, policy)
}
async fn observed_text(
    uri: &std::path::Path,
    path: &str,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> io::Result<Option<String>> {
    observed_branch_text(uri, None, path, original).await
}
async fn observed_branch_text(
    uri: &std::path::Path,
    branch: Option<&str>,
    path: &str,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> io::Result<Option<String>> {
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await
        .map_err(io_error)?;
    let snapshot = store.capture_target(branch).await.map_err(io_error)?;
    let raw = read_branch_source(&snapshot, path, original)
        .await
        .map_err(io_error)?;
    match raw.source(original).map_err(io_error)? {
        graphit_ast::source_inspection::IndexedSource::Available(text) => Ok(Some(text.to_owned())),
        graphit_ast::source_inspection::IndexedSource::Missing => Ok(None),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_ignored_build_churn_publishes_and_selected_change_preserves_previous()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_ast::ignore_policy::AstIgnorePolicy;
    use graphit_core::artifact_store::{
        CapturedSourceReader, ProjectIgnoreCriteria, ProjectSelectionCriteria,
        ProjectSelectionLimits,
    };
    use graphit_hub::policy::ResourceSetting;
    use std::path::Path;

    let (_temp, installation, project, config, policy) = ast_fixture();
    std::fs::create_dir(project.root().join("ui"))?;
    std::fs::write(project.root().join(".gitignore"), "ui/build-output.mjs\n")?;
    std::fs::write(project.root().join("ui/main.rs"), "pub fn original() {}\n")?;
    let cancel = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancel)?;
    let no_docker = tempfile::tempdir()?;
    let mut owner = ForegroundSource::start_and_publish(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        &request,
    )
    .await?;
    // Drive the retained real job serially, without a watcher racing the fixture.
    let selected = owner.supervisors.supervised().next().unwrap().clone();
    if let Err(error) = owner.supervisors.shutdown().await {
        assert_cancelled_raw(&error);
    }
    let job = owner.ast_jobs.as_ref().unwrap();
    job.index(
        &project,
        &selected,
        graphit_daemon::watcher::ChangeBatch {
            rescan: true,
            ..Default::default()
        },
        &cancel,
    )
    .await?;
    let uri = project.project_runtime(selected.id, &["ast", "source.lance"])?;
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
    let initial = store.capture_target(None).await?;
    std::fs::write(project.root().join("ui/main.rs"), "pub fn published() {}\n")?;
    let loader =
        crate::ast_profile_loader::AstProfileLoader::new(policy.clone(), request.snapshot())?;
    let profiles = loader.load(&installation, &project, &config, request.snapshot())?;
    assert!(profiles.allows(Path::new("ui/build-output.mjs"), request.snapshot())?);
    let ignore = AstIgnorePolicy::from_config(&config, request.snapshot())?;
    let defaults: Vec<_> = ignore.defaults().iter().map(String::as_str).collect();
    let selectors: Vec<_> = profiles.selectors.iter().map(String::as_str).collect();
    let capture =
        CapturedPublicationScope::project(&project, selected.id, owner.scope, request.snapshot())?;
    let max = usize::try_from(policy.limit(ResourceSetting::ContentInputBytes).value)?;
    let limits = ProjectSelectionLimits {
        max_depth: usize::try_from(policy.limit(ResourceSetting::SnapshotDepth).value)?,
        max_input_items: usize::try_from(
            policy
                .limit(ResourceSetting::ContentCollectionEntries)
                .value,
        )?,
        max_input_bytes: max,
        max_observed_path_bytes: MAX_PATH_BYTES,
        max_selected_file_bytes: max as u64,
        max_control_file_bytes: max as u64,
    };
    let capture_reader = || {
        CapturedSourceReader::capture_selected_project_metadata(
            &capture,
            ProjectSelectionCriteria {
                subtree: Some(Path::new(".")),
                extras: &[],
                include_readme: false,
                extensions: &selectors,
                ignore: ProjectIgnoreCriteria {
                    protected_defaults: &defaults,
                    custom_control: Some(ignore.filename()),
                    git_control: Some(".gitignore"),
                },
            },
            limits,
            SourceReadLimits::new(max, 64 * 1024, max as u64)?,
            request.snapshot(),
        )
    };
    let reader = capture_reader()?;
    assert_eq!(reader.retained_bytes(), 0);
    // This happens after admission, just as a frontend build runs during Raw.
    // The extension is selected, so only the real ignore policy excludes it.
    std::fs::write(project.root().join("ui/build-output.mjs"), "generated();\n")?;
    let recheck = || {
        request.check()?;
        profiles.revalidate(request.snapshot())?;
        reader.revalidate_content()
    };
    let prepared = crate::foreground_ast_sources::prepare_stream(
        &reader,
        &profiles,
        true,
        request.snapshot(),
        &recheck,
        |stream| initial.prepare_overwrite_stream(source_schema(), stream),
    )
    .await?;
    recheck()?;
    let published = prepared.commit().await?;
    recheck()?;
    assert!(published.version() > initial.version());
    assert_eq!(
        observed_text(&uri, "ui/main.rs", request.snapshot()).await?,
        Some("pub fn published() {}\n".into())
    );
    assert_eq!(
        observed_text(&uri, "ui/build-output.mjs", request.snapshot()).await?,
        None
    );
    job.index_semantic(
        project.clone(),
        selected.clone(),
        None,
        None,
        std::collections::BTreeSet::new(),
        false,
        cancel.clone(),
        cancel.clone(),
    )
    .await?;
    let source = owner.server.as_ref().unwrap();
    let runtime_key = owner.auth.runtime().check(request.snapshot())?;
    let credential = std::str::from_utf8(runtime_key.secret(request.snapshot())?)?;
    let graph = source
        .graph_server()
        .call_mcp(
            rmcp::model::CallToolRequestParams::new("graphit_ast_query").with_arguments(
                json!({
                    "project_dir": project.root(),
                    "project_version": selected.version.version(),
                    "query": "MATCH (n:Function) RETURN n.name, n.path",
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            axum::http::HeaderMap::from_iter([
                (
                    axum::http::header::AUTHORIZATION,
                    axum::http::HeaderValue::from_str(&format!("Bearer {credential}"))?,
                ),
                (
                    axum::http::HeaderName::from_static("x-graphit-auth-origin"),
                    axum::http::HeaderValue::from_static("runtime"),
                ),
            ]),
            CancellationToken::new(),
        )
        .await;
    assert_ne!(graph.is_error, Some(true), "{graph:?}");
    assert!(
        graph.structured_content.as_ref().unwrap()["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row
                == &json!([
                    {"type":"STRING", "value":"published"},
                    {"type":"STRING", "value":"ui/main.rs"},
                ])),
        "the actual parser and graph consumer must see the committed Raw: {graph:?}"
    );

    let reader = capture_reader()?;
    std::fs::write(
        project.root().join("ui/main.rs"),
        "pub fn must_not_publish() {}\n",
    )?;
    let recheck = || {
        request.check()?;
        profiles.revalidate(request.snapshot())?;
        reader.revalidate_content()
    };
    let rejected = crate::foreground_ast_sources::prepare_stream(
        &reader,
        &profiles,
        true,
        request.snapshot(),
        &recheck,
        |stream| published.prepare_overwrite_stream(source_schema(), stream),
    )
    .await;
    assert!(
        rejected.is_err(),
        "a changed selected FILE still fails closed"
    );
    assert_eq!(
        store.capture_target(None).await?.version(),
        published.version()
    );
    assert_eq!(
        observed_text(&uri, "ui/main.rs", request.snapshot()).await?,
        Some("pub fn published() {}\n".into()),
        "a rejected read cannot replace the previous Raw"
    );
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unchanged_inherited_raw_is_materialized_before_first_projection()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, installation, project, config, policy) = ast_fixture();
    std::fs::write(project.root().join("a.rs"), "pub fn alpha() {}")?;
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel)?;
    let no_docker = tempfile::tempdir()?;
    let mut owner = ForegroundSource::start_and_publish(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        &original,
    )
    .await?;
    let mut selected = owner.supervisors.supervised().next().unwrap().clone();
    let uri = project.project_runtime(selected.id, &["ast", "source.lance"])?;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !observed_text(&uri, "a.rs", original.snapshot())
            .await
            .is_ok_and(|text| text.as_deref() == Some("pub fn alpha() {}"))
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    owner.supervisors.shutdown().await?;
    selected.version = graphit_core::version_context::VersionContext::select(
        &project,
        graphit_core::version_context::Checkout::NonGit,
        "feature/source-origin",
        None,
        "",
    )?;
    let branch = selected.version.lance_branch();
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
    assert!(
        store
            .capture_target(Some(&branch))
            .await?
            .inherited_target()
            .is_some()
    );
    let ast = owner.ast_jobs.as_ref().unwrap();
    assert!(
        ast.index(
            &project,
            &selected,
            graphit_daemon::watcher::ChangeBatch {
                rescan: true,
                ..Default::default()
            },
            &cancel
        )
        .await?
        .is_none(),
        "Source bytes did not change"
    );
    let first = store.capture_target(Some(&branch)).await?;
    assert_eq!(first.physical_branch(), Some(branch.as_str()));
    assert!(first.inherited_target().is_none());
    std::fs::write(project.root().join("b.rs"), "pub fn beta() {}")?;
    ast.index(
        &project,
        &selected,
        graphit_daemon::watcher::ChangeBatch {
            paths: std::collections::BTreeSet::from([project.root().join("b.rs")]),
            ..Default::default()
        },
        &cancel,
    )
    .await?;
    let changed = store.capture_target(Some(&branch)).await?;
    assert_eq!(changed.physical_branch(), first.physical_branch());
    assert_eq!(
        observed_branch_text(&uri, Some(&branch), "a.rs", original.snapshot())
            .await?
            .as_deref(),
        Some("pub fn alpha() {}")
    );
    assert_eq!(
        observed_text(&uri, "b.rs", original.snapshot()).await?,
        None,
        "the branch writer preserves Main"
    );
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ast_only_real_foreground_initial_raw_and_source_http()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, installation, project, config, policy) = ast_fixture();
    std::fs::write(project.root().join("a.rs"), "fn original() {}")?;
    std::fs::write(project.root().join("b.rs"), "fn old_b() {}")?;
    std::fs::write(project.root().join("Dockerfile"), "FROM scratch\n")?;
    std::fs::write(project.root().join("unsupported.opaque"), [0xff])?;
    let startup_cancel = Cancellation::default();
    let startup = policy.request_context(Instant::now(), &startup_cancel)?;
    let inputs = [ProjectBootstrapInput {
        project: &project,
        explicit_version: "",
        pin: None,
        detached_base: "",
    }];
    let mut owner = ForegroundSource::start_and_publish(
        &installation,
        &inputs,
        0,
        &config,
        NativeBinding::Codex,
        Vec::new(),
        &startup,
    )
    .await?;
    assert!(owner.memory_jobs.is_none());
    assert!(owner.ast_jobs.is_some());
    assert_eq!(owner.supervisors.supervised().count(), 1);
    let id = ProjectIdentityStore::at(&project).load()?.unwrap().id();
    let uri = project.project_runtime(id, &["ast", "source.lance"])?;
    let request_cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &request_cancel)?;
    let expires = Instant::now() + Duration::from_secs(10);
    loop {
        if observed_text(&uri, "a.rs", original.snapshot())
            .await
            .is_ok_and(|text| text.as_deref() == Some("fn original() {}"))
            && observed_text(&uri, "b.rs", original.snapshot())
                .await
                .is_ok_and(|text| text.as_deref() == Some("fn old_b() {}"))
        {
            break;
        }
        assert!(Instant::now() < expires, "initial Raw callback timed out");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        observed_text(&uri, "Dockerfile", original.snapshot()).await?,
        Some("FROM scratch\n".into())
    );
    assert_eq!(
        observed_text(&uri, "unsupported.opaque", original.snapshot()).await?,
        None
    );
    // Retain an old Main, then change B before the new watcher is registered.
    // Repairing only A after the actual failed full scan must also reconcile B.
    owner.supervisors.shutdown().await?;
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
    let old_version = store.capture_target(None).await?.version();
    assert_eq!(
        observed_text(&uri, "b.rs", original.snapshot())
            .await?
            .as_deref(),
        Some("fn old_b() {}")
    );
    std::fs::write(project.root().join("b.rs"), "fn new_b() {}")?;
    std::fs::write(project.root().join("a.rs"), [0xff])?;
    let failed_full = owner.ast_jobs.as_ref().unwrap().observe_next_raw_error();
    owner
        .attach_jobs(&installation, &config, NativeBinding::Codex, &[], &startup)
        .await?;
    // FILE UTF-8 errors retain their typed cause inside io::Error::other.
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), failed_full).await??,
        io::ErrorKind::Other
    );
    assert_eq!(store.capture_target(None).await?.version(), old_version);
    assert_eq!(
        observed_text(&uri, "a.rs", original.snapshot())
            .await?
            .as_deref(),
        Some("fn original() {}")
    );
    assert_eq!(
        observed_text(&uri, "b.rs", original.snapshot())
            .await?
            .as_deref(),
        Some("fn old_b() {}")
    );
    let repair_batch = owner.ast_jobs.as_ref().unwrap().observe_next_raw_batch();
    // The initial supervisors have joined and the replacement full failed.
    // Only a successful full after this repair can acknowledge this receiver.
    let (full_completed, repair_full) = tokio::sync::oneshot::channel();
    assert!(
        owner
            .ast_full_completed
            .lock()
            .unwrap()
            .replace(full_completed)
            .is_none()
    );
    std::fs::write(project.root().join("a.rs"), "fn original() {}")?;
    let (rescan, paths) = tokio::time::timeout(Duration::from_secs(10), repair_batch).await??;
    assert!(!rescan, "the watcher must deliver an ordinary repair delta");
    assert_eq!(
        paths,
        std::collections::BTreeSet::from([project.root().join("a.rs")])
    );
    let expires = Instant::now() + Duration::from_secs(10);
    loop {
        if observed_text(&uri, "b.rs", original.snapshot())
            .await?
            .as_deref()
            == Some("fn new_b() {}")
        {
            break;
        }
        assert!(
            Instant::now() < expires,
            "pending full reconciliation timed out"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Seeing B's bytes can precede its Raw index maintenance and FILE commit.
    tokio::time::timeout(Duration::from_secs(10), repair_full).await??;
    let reconciled = store.capture_target(None).await?;
    let reconciled_version = reconciled.version();
    assert!(reconciled_version > old_version);
    let pinned = reconciled.at_version(reconciled_version).await?;
    let raw_b = read_branch_source(&pinned, "b.rs", original.snapshot()).await?;
    assert_eq!(raw_b.version(), reconciled_version);
    assert!(matches!(
        raw_b.source(original.snapshot())?,
        graphit_ast::source_inspection::IndexedSource::Available("fn new_b() {}")
    ));
    assert_eq!(
        observed_text(&uri, "a.rs", original.snapshot())
            .await?
            .as_deref(),
        Some("fn original() {}")
    );
    let selected = owner.supervisors.supervised().next().unwrap().clone();
    let source = owner.server.as_ref().unwrap();
    let runtime_key = owner.auth.runtime().check(original.snapshot())?;
    let credential = std::str::from_utf8(runtime_key.secret(original.snapshot())?)?;
    let expires = Instant::now() + Duration::from_secs(10);
    loop {
        let graph = source.graph_server().call_mcp(
            rmcp::model::CallToolRequestParams::new("graphit_ast_query").with_arguments(json!({
                "project_dir":graphit_core::native_path::NativePath::new(project.root().to_owned())?,
                "project_version":selected.version.version(),
                "query":"MATCH (n:AstFileObservation) RETURN n.path, n.raw_version, n.source_sha256",
            }).as_object().unwrap().clone()),
            axum::http::HeaderMap::from_iter([
                (axum::http::header::AUTHORIZATION, axum::http::HeaderValue::from_str(&format!("Bearer {credential}"))?),
                (axum::http::HeaderName::from_static("x-graphit-auth-origin"), axum::http::HeaderValue::from_static("runtime")),
            ]), CancellationToken::new(),
        ).await;
        let expected_hash = {
            use sha2::{Digest, Sha256};
            Sha256::digest(b"fn new_b() {}")
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        if graph.is_error != Some(true)
            && graph
                .structured_content
                .as_ref()
                .and_then(|value| value["rows"].as_array())
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row == &json!([
                            {"type":"STRING", "value":"b.rs"},
                            {"type":"STRING", "value":reconciled_version.to_string()},
                            {"type":"STRING", "value":expected_hash},
                        ])
                    })
                })
        {
            break;
        }
        assert!(
            Instant::now() < expires,
            "Graph did not project Raw {reconciled_version}: {graph:?}; B hash {expected_hash}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let service = Cancellation::default();
    let stop = service.clone();
    let running = tokio::spawn(async move { owner.run(&service).await });
    let client = reqwest::Client::new();
    let mut output = Vec::new();
    crate::source_entry::call_source(
        &installation,
        &client,
        &serde_json::from_value(json!({"path":"a.rs"}))?,
        NativeDocumentFileLimits::new(65536, graphit_core::native_path::MAX_PATH_BYTES, 64, 65536)?,
        &original,
        &mut output,
    )
    .await?;
    assert_eq!(output, b"fn original() {}");
    output.clear();
    crate::source_entry::call_source(
        &installation,
        &client,
        &serde_json::from_value(json!({"path":"b.rs"}))?,
        NativeDocumentFileLimits::new(65536, graphit_core::native_path::MAX_PATH_BYTES, 64, 65536)?,
        &original,
        &mut output,
    )
    .await?;
    assert_eq!(output, b"fn new_b() {}");
    std::fs::write(project.root().join("a.rs"), "fn changed() {}")?;
    let expires = Instant::now() + Duration::from_secs(10);
    loop {
        if observed_text(&uri, "a.rs", original.snapshot())
            .await?
            .as_deref()
            == Some("fn changed() {}")
        {
            break;
        }
        assert!(Instant::now() < expires, "delta callback timed out");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    std::fs::rename(
        project.root().join("a.rs"),
        project.root().join("renamed.rs"),
    )?;
    let expires = Instant::now() + Duration::from_secs(10);
    loop {
        if observed_text(&uri, "a.rs", original.snapshot())
            .await?
            .is_none()
            && observed_text(&uri, "renamed.rs", original.snapshot())
                .await?
                .as_deref()
                == Some("fn changed() {}")
        {
            break;
        }
        if Instant::now() >= expires {
            stop.cancel();
            running.await??;
            return Err("rename callback timed out".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    std::fs::remove_file(project.root().join("renamed.rs"))?;
    let expires = Instant::now() + Duration::from_secs(10);
    loop {
        if observed_text(&uri, "renamed.rs", original.snapshot())
            .await?
            .is_none()
        {
            break;
        }
        assert!(Instant::now() < expires, "delete callback timed out");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    stop.cancel();
    if let Err(mut failure) = running.await? {
        // Cancellation may arrive after the SDK commit while its checks are still running.
        // Preserve that causal failure and observed commit, then finish the retained owner cleanup.
        let raw = assert_cancelled_raw(&failure.error);
        assert!(stop.is_cancelled());
        assert!(matches!(
            raw.effects.keys.last(),
            Some(crate::foreground_ast_sources::RawKey::Delta(_))
        ));
        let committed = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
            .await?
            .capture_target(None)
            .await?;
        assert_eq!(raw.effects.published_version, Some(committed.version()));
        failure
            .owner
            .take()
            .expect("retained foreground owner")
            .shutdown()
            .await?;
    }
    assert!(!installation.root().join("daemon/mcp.key").exists());
    Ok(())
}

#[tokio::test]
async fn raw_named_branch_custom_layers_empty_ignore_and_late_error()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, installation, project, config, policy) = ast_fixture();
    let user_dir = installation.global_path(&["ast", "queries"])?;
    let project_dir = project.brand_directory().join("ast/queries");
    // The installed artifact namespace must retain its real private directory policy.
    drop(graphit_fs::create_private_directory(
        &installation.root().join("ast"),
    )?);
    std::fs::create_dir_all(&user_dir)?;
    std::fs::create_dir_all(&project_dir)?;
    std::fs::write(
        user_dir.join("rust.yaml"),
        "language: rust\ngrammar: tree-sitter-rust\nextensions: [\".user\"]\n",
    )?;
    std::fs::write(
        project_dir.join("rust.yaml"),
        "language: rust\ngrammar: tree-sitter-rust\nextensions: [\".custom\", \"SPECIAL.FILE\"]\n",
    )?;
    std::fs::write(project.root().join("a.custom"), "original custom")?;
    std::fs::write(project.root().join("SPECIAL.FILE"), "")?;
    std::fs::write(project.root().join("ignored.custom"), "ignored")?;
    std::fs::write(project.root().join("stock.rs"), "stock replaced")?;
    std::fs::write(project.root().join("user.user"), "user replaced")?;
    std::fs::write(project.root().join(".astignore"), "ignored.custom\n")?;
    let cancel = Cancellation::default();
    let startup = policy.request_context(Instant::now(), &cancel)?;
    let mut owner = ForegroundSource::start_and_publish(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "feature/raw",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        Vec::new(),
        &startup,
    )
    .await?;
    let selected = owner.supervisors.supervised().next().unwrap().clone();
    let branch = selected.version.lance_branch();
    let uri = project.project_runtime(selected.id, &["ast", "source.lance"])?;
    let expires = Instant::now() + Duration::from_secs(10);
    loop {
        if observed_branch_text(&uri, Some(&branch), "a.custom", startup.snapshot())
            .await
            .is_ok_and(|text| text.as_deref() == Some("original custom"))
        {
            break;
        }
        if Instant::now() >= expires {
            owner.supervisors.shutdown().await?;
            return Err("named Raw initial callback timed out".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if let Err(failure) = owner.supervisors.shutdown().await {
        let raw = assert_cancelled_raw(&failure);
        if let Some(version) = raw.effects.published_version {
            let observed = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
                .await?
                .capture_target(Some(&branch))
                .await?;
            assert_eq!(version, observed.version());
            assert!(!raw.effects.keys.is_empty());
        }
    }
    assert_eq!(
        observed_branch_text(&uri, Some(&branch), "SPECIAL.FILE", startup.snapshot()).await?,
        Some(String::new())
    );
    for path in ["ignored.custom", "stock.rs", "user.user"] {
        assert_eq!(
            observed_branch_text(&uri, Some(&branch), path, startup.snapshot()).await?,
            None,
            "{path}"
        );
    }
    assert_eq!(
        observed_text(&uri, "a.custom", startup.snapshot()).await?,
        None,
        "named publication preserves default image"
    );
    let job = owner.ast_jobs.as_ref().unwrap();
    let rescan = || graphit_daemon::watcher::ChangeBatch {
        rescan: true,
        ..Default::default()
    };
    assert!(
        job.index(&project, &selected, rescan(), &cancel)
            .await?
            .is_none()
    );
    let before = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await?
        .capture_target(Some(&branch))
        .await?
        .version();
    // The first valid row can be streamed, but a later invalid FILE must prevent publication.
    std::fs::write(project.root().join("a.custom"), "must not publish")?;
    std::fs::write(project.root().join("z.custom"), [0xff])?;
    let failure = job
        .index(&project, &selected, rescan(), &cancel)
        .await
        .unwrap_err();
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&failure);
    let mut original_utf8 = false;
    while let Some(error) = cause {
        original_utf8 |= error.downcast_ref::<std::str::Utf8Error>().is_some();
        cause = original_error_source(error);
    }
    assert!(
        original_utf8,
        "the original FILE UTF-8 cause must survive the SDK stream: {failure:?}"
    );
    let after = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await?
        .capture_target(Some(&branch))
        .await?
        .version();
    assert_eq!(before, after);
    assert_eq!(
        observed_branch_text(&uri, Some(&branch), "a.custom", startup.snapshot()).await?,
        Some("original custom".into())
    );
    assert_eq!(
        observed_branch_text(&uri, Some(&branch), "z.custom", startup.snapshot()).await?,
        None
    );
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_direct_existing_job_noop_null_cancel_and_project_switches()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, installation, project, config, policy) = ast_fixture();
    std::fs::write(project.root().join("a.rs"), "fn original() {}")?;
    std::fs::write(project.root().join("b.rs"), [0xff])?;
    let startup_cancel = Cancellation::default();
    let startup = policy.request_context(Instant::now(), &startup_cancel)?;
    let owner = ForegroundSource::start_and_publish(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        Vec::new(),
        &startup,
    )
    .await?;
    let selected = owner.supervisors.supervised().next().unwrap().clone();
    // The same supervisor is serial; stop it before explicitly driving the retained genuine job.
    let mut owner = owner;
    owner.supervisors.shutdown().await?;
    let job = owner.ast_jobs.as_ref().unwrap();
    let cancel = Cancellation::default();
    let rescan = || graphit_daemon::watcher::ChangeBatch {
        rescan: true,
        ..Default::default()
    };
    let uri = project.project_runtime(selected.id, &["ast", "source.lance"])?;
    // An actual failed full scan leaves no published Main. The next event only
    // mentions A, but first publication still needs unchanged B's complete image.
    let failed = job
        .index(&project, &selected, rescan(), &cancel)
        .await
        .unwrap_err();
    let failure = failed
        .get_ref()
        .and_then(|error| {
            error.downcast_ref::<crate::foreground_ast_sources::RawPublicationFailure>()
        })
        .expect("the failed full scan retains its real Raw cause");
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&failure.cause);
    let mut original_utf8 = false;
    while let Some(error) = cause {
        original_utf8 |= error.downcast_ref::<std::str::Utf8Error>().is_some();
        cause = original_error_source(error);
    }
    assert!(
        original_utf8,
        "the initial full scan retains the original FILE UTF-8 cause: {failure:?}"
    );
    assert!(!uri.exists());
    std::fs::write(project.root().join("b.rs"), "fn unchanged() {}")?;
    let delta = graphit_daemon::watcher::ChangeBatch {
        paths: std::collections::BTreeSet::from([project.root().join("a.rs")]),
        ..Default::default()
    };
    let first = job.index(&project, &selected, delta, &cancel).await?;
    assert!(first.is_some());
    let request = policy.request_context(Instant::now(), &cancel)?;
    assert_eq!(
        observed_text(&uri, "b.rs", request.snapshot()).await?,
        Some("fn unchanged() {}".into()),
        "first incremental event must publish the complete selected Main"
    );
    // Graph reconstructs all of that Raw image, even with NN disabled.
    job.index_semantic(
        project.clone(),
        selected.clone(),
        None,
        None,
        std::collections::BTreeSet::new(),
        false,
        cancel.clone(),
        cancel.clone(),
    )
    .await?;
    let source = owner.server.as_ref().unwrap();
    let live = source.live_projects();
    let runtime_key = owner.auth.runtime().check(request.snapshot())?;
    let credential = std::str::from_utf8(runtime_key.secret(request.snapshot())?)?;
    let runtime = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        crate::project_graph::with_exported_graph(
            &source.graph_server(),
            crate::project_graph::ReadInput {
                auth: &owner.auth,
                policy: source.policy(),
                base: &config,
                live: &live,
                selected: (&project, selected.id, &selected.version),
                credential,
                local: false,
                request: &request,
            },
            |graph, _| -> std::io::Result<()> {
                let files = graphit_ast::icebug_query::execute(
                    graph,
                    request.snapshot(),
                    &runtime,
                    "MATCH (n:AstFileObservation) RETURN n.path",
                    &Default::default(),
                    source.policy().graph_query,
                )?;
                assert_eq!(files.rows.len(), 2, "Graph keeps both Raw FILES");
                Ok(())
            },
        )
    })??;
    let second = job.index(&project, &selected, rescan(), &cancel).await?;
    assert!(second.is_none(), "noop must not commit a new Raw version");
    let _ = first;
    let other_root = project
        .root()
        .parent()
        .unwrap()
        .join("unauthorized-project");
    std::fs::create_dir(&other_root)?;
    let other = ProjectPaths::at(&other_root, &installation)?;
    let other_id = ProjectIdentityStore::at(&other)
        .writer(Duration::from_secs(5), &cancel)?
        .ensure(&ProjectMetadata::new("outside-runtime-grants")?)?
        .id();
    std::fs::write(other.root().join("secret.rs"), "must not publish")?;
    let outside = graphit_daemon::supervisor::ProjectContext {
        id: other_id,
        version: graphit_core::version_context::VersionContext::select(
            &other,
            graphit_core::version_context::Checkout::NonGit,
            "",
            None,
            "",
        )?,
    };
    assert!(
        job.index(&other, &outside, rescan(), &cancel)
            .await
            .is_err()
    );
    assert!(!project.project_runtime(other_id, &["ast"])?.exists());
    let cfg = ConfigStore::project(&project)?;
    cfg.set(
        ConfigKey::new("modules.ast")?,
        json!(false),
        Duration::from_secs(5),
        &cancel,
    )?;
    std::fs::write(project.root().join("a.rs"), "fn disabled_change() {}")?;
    assert!(
        job.index(&project, &selected, rescan(), &cancel)
            .await?
            .is_none()
    );
    cfg.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    assert!(
        job.index(&project, &selected, rescan(), &cancel)
            .await?
            .is_some()
    );
    cfg.set(
        ConfigKey::new("ast.index_source")?,
        json!(false),
        Duration::from_secs(5),
        &cancel,
    )?;
    assert!(
        job.index(&project, &selected, rescan(), &cancel)
            .await?
            .is_some()
    );
    let original_cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &original_cancel)?;
    let snapshot = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await?
        .capture_target(None)
        .await?;
    let raw = read_branch_source(&snapshot, "a.rs", original.snapshot()).await?;
    assert_eq!(
        raw.missing_reason(),
        Some(graphit_ast::indexed_source::MissingSourceReason::TextNotStored)
    );
    let aborted = Cancellation::default();
    aborted.cancel();
    assert!(
        job.index(&project, &selected, rescan(), &aborted)
            .await
            .is_err()
    );
    owner.shutdown().await?;
    Ok(())
}

struct AntlrPublication {
    tables: graphit_ast::icebug_query::QueryResult,
    files: graphit_ast::icebug_query::QueryResult,
    hits: Vec<graphit_ast::entity_search::Hit>,
    vectors: std::collections::BTreeMap<(String, String, String), (Vec<f32>, u64)>,
}

fn observed_antlr_publication(
    owner: &ForegroundSource,
    source: &crate::source_server::SourceServer,
    selected: &(ProjectPaths, ProjectId, VersionContext),
    credential: &str,
    resources: &crate::foreground_memory_jobs::MemoryJobResources,
) -> io::Result<AntlrPublication> {
    use arrow_array::{FixedSizeListArray, Float32Array, StringArray, UInt64Array};
    use futures_util::TryStreamExt;
    let cancellation = Cancellation::default();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)
        .map_err(io::Error::other)?;
    // The reused unconfigured fixture's actual startup base is empty;
    // project/global files are still loaded by PreparedRead and the productive jobs.
    let base =
        ConfigSnapshot::new(source.policy().installation.brand(), []).map_err(io::Error::other)?;
    let live = source.live_projects();
    let runtime = tokio::runtime::Handle::current();
    let branch_name = selected.2.lance_branch();
    let raw_branch = (!branch_name.is_empty()).then_some(branch_name.as_str());
    // SDK reads and Cypher stay within the actual ReadAst admission, Original and Copy.
    tokio::task::block_in_place(|| {
        crate::project_graph::with_exported_graph(
            &source.graph_server(),
            crate::project_graph::ReadInput {
                auth: &owner.auth,
                policy: source.policy(),
                base: &base,
                live: &live,
                selected: (&selected.0, selected.1, &selected.2),
                credential,
                local: false,
                request: &original,
            },
            |graph, _| {
                let files = graphit_ast::icebug_query::execute(
                    graph,
                    original.snapshot(),
                    &runtime,
                    "MATCH (n:AstFileObservation) RETURN n.path, n.diagnostics_json",
                    &Default::default(),
                    source.policy().graph_query,
                )?;
                let tables = graphit_ast::icebug_query::execute(
                    graph,
                    original.snapshot(),
                    &runtime,
                    "MATCH (n:`Table`) RETURN n.name, n.path, n.available",
                    &Default::default(),
                    source.policy().graph_query,
                )?;
                let search = runtime
                    .block_on(graphit_ast::entity_search::EntitySearch::open(
                        Arc::clone(graph),
                        Arc::clone(original.snapshot()),
                    ))?
                    .ok_or(io::ErrorKind::NotFound)?;
                let mut hits = Vec::new();
                for term in ["DB2_KEEP", "ORACLE_KEEP", "DB2_NEW", "BROKEN_RECOVERY"] {
                    hits.extend(runtime.block_on(search.query(term, 64))?);
                }
                let model = crate::ast_semantic::expected_native_nn_model(
                    &resources.local.as_ref().unwrap().expected,
                )
                .map_err(io::Error::other)?;
                let nn = runtime
                    .block_on(search.nn(&model))?
                    .ok_or(io::ErrorKind::NotFound)?;
                let mut rows = runtime
                    .block_on(nn.scan().try_into_stream())
                    .map_err(io::Error::other)?;
                let mut vectors = std::collections::BTreeMap::new();
                while let Some(batch) = runtime
                    .block_on(rows.try_next())
                    .map_err(io::Error::other)?
                {
                    graphit_ast::native_nn::validate_native_nn(&batch, &model, original.snapshot())
                        .map_err(io::Error::other)?;
                    let text = |name| {
                        batch
                            .column_by_name(name)
                            .unwrap()
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap()
                    };
                    let versions = batch
                        .column_by_name("raw_version")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<UInt64Array>()
                        .unwrap();
                    let generations = batch
                        .column_by_name("model_generation")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<UInt64Array>()
                        .unwrap();
                    let values = batch
                        .column_by_name("vector")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<FixedSizeListArray>()
                        .unwrap();
                    for row in 0..batch.num_rows() {
                        assert!(versions.value(row) > 0);
                        let path = text("path").value(row);
                        let raw = runtime
                            .block_on(observed_branch_text(
                                &selected
                                    .0
                                    .project_runtime(selected.1, &["ast", "source.lance"])?,
                                raw_branch,
                                path,
                                original.snapshot(),
                            ))?
                            .unwrap();
                        assert!(raw.contains(text("projection").value(row)));
                        let vector = values.value(row);
                        let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
                        assert!(vector.values().iter().all(|value| value.is_finite()));
                        assert!(
                            vectors
                                .insert(
                                    (
                                        path.to_owned(),
                                        text("uid").value(row).to_owned(),
                                        text("data_key").value(row).to_owned()
                                    ),
                                    (vector.values().to_vec(), generations.value(row)),
                                )
                                .is_none()
                        );
                    }
                }
                search.revalidate()?;
                Ok(AntlrPublication {
                    tables,
                    files,
                    hits,
                    vectors,
                })
            },
        )
        .map_err(io::Error::other)?
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_antlr_db2_oracle_graph_search_nn_incremental_and_reuse()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::{
        foreground_ast_sources::{ForegroundAstSourceConfig, ForegroundAstSources},
        foreground_memory_jobs::MemoryJobResources,
    };
    use graphit_daemon::supervisor::ProjectContext;
    let (_storage, mut owner, tasks, credential, _) =
        super::foreground_memory_jobs_tests::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let cancellation = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancellation,
    )?;
    // These shipped SQL profiles are exclusive: opt in through the real grammar mapping.
    // Distinct extensions also avoid the shared .sql selector ambiguity.
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("ast.grammar")?,
        json!(".db2=antlr-db2,.pkb=antlr-plsql"),
        Duration::from_secs(5),
        &cancellation,
    )?;
    const DB2: &str = "CREATE TABLE DB2_KEEP(C_KEEP INTEGER); SELECT C_KEEP FROM DB2_KEEP;";
    const ORACLE: &str = "CREATE TABLE \"ORACLE_KEEP\"(\"ID_ORACLE\" NUMBER DEFAULT 123456789012345678901234567890); SELECT ID_ORACLE FROM ORACLE_KEEP;";
    const BROKEN: &str = "CREATE TABLE BROKEN_RECOVERY(C INTEGER";
    for (path, text) in [
        ("model.db2", DB2),
        ("model.pkb", ORACLE),
        ("recovery.db2", BROKEN),
    ] {
        std::fs::write(selected.0.root().join(path), text)?;
    }
    let resources: Arc<MemoryJobResources> = owner.memory_job_resources.upgrade().unwrap();
    let source = owner.server.as_ref().unwrap().clone();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
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
            environment: Vec::new(),
        },
        original.snapshot(),
    )?);
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let rescan = || graphit_daemon::watcher::ChangeBatch {
        rescan: true,
        ..Default::default()
    };
    let base = ConfigSnapshot::new(source.policy().installation.brand(), [])?;
    let config = crate::ast_profile_loader::project_config(
        &source.policy().installation,
        &selected.0,
        &base,
        original.snapshot(),
    )?;
    let loader = crate::ast_profile_loader::AstProfileLoader::new(
        owner.policy.clone(),
        original.snapshot(),
    )?;
    let profiles = loader.load(
        &source.policy().installation,
        &selected.0,
        &config,
        original.snapshot(),
    )?;
    for (path, language, grammar) in [
        ("model.db2", "db2", "antlr-db2"),
        ("model.pkb", "plsql", "antlr-plsql"),
    ] {
        let graphit_ast::catalog::Selection::Candidates(candidates) =
            profiles.resolve(std::path::Path::new(path), original.snapshot())?
        else {
            panic!("shipped profile does not select {path}");
        };
        assert!(
            candidates
                .iter()
                .any(|profile| profile.language().value() == language
                    && profile.grammar().value() == grammar),
            "exact shipped parser for {path}"
        );
    }
    assert!(
        ast.index(&selected.0, &context, rescan(), &cancellation)
            .await?
            .is_some()
    );
    let raw_uri = selected
        .0
        .project_runtime(selected.1, &["ast", "source.lance"])?;
    let branch_name = selected.2.lance_branch();
    let raw_branch = (!branch_name.is_empty()).then_some(branch_name.as_str());
    for (path, expected) in [
        ("model.db2", DB2),
        ("model.pkb", ORACLE),
        ("recovery.db2", BROKEN),
    ] {
        assert_eq!(
            observed_branch_text(&raw_uri, raw_branch, path, original.snapshot())
                .await?
                .as_deref(),
            Some(expected),
            "selected Raw branch contains the parser input: {path}"
        );
    }
    assert!(
        ast.index_semantic(
            selected.0.clone(),
            context.clone(),
            Some(Arc::clone(&resources)),
            None,
            std::collections::BTreeSet::new(),
            false,
            cancellation.clone(),
            cancellation.clone()
        )
        .await?
        .version
        .is_some()
    );
    let first = observed_antlr_publication(&owner, &source, &selected, &credential, &resources)?;
    for (name, path, language, grammar, body) in [
        ("DB2_KEEP", "model.db2", "db2", "antlr-db2", DB2),
        ("ORACLE_KEEP", "model.pkb", "plsql", "antlr-plsql", ORACLE),
    ] {
        let table = first
            .tables
            .rows
            .iter()
            .find(|row| row[0]["value"] == name)
            .unwrap();
        assert_eq!(table[1]["value"], path);
        assert_eq!(table[2]["value"], true);
        let hit = first
            .hits
            .iter()
            .find(|hit| hit.entity.name == name)
            .unwrap();
        assert_eq!(hit.entity.path, path);
        assert_eq!(hit.entity.language, language);
        assert_eq!(hit.entity.grammar, grammar);
        assert!(hit.score.is_finite());
        let start = usize::try_from(hit.entity.start_byte.unwrap())?;
        let end = usize::try_from(hit.entity.end_byte.unwrap())?;
        assert!(body[start..end].contains(name));
        assert!(
            first
                .vectors
                .keys()
                .any(|(observed, _, _)| observed == path)
        );
    }
    let recovery = first
        .tables
        .rows
        .iter()
        .find(|row| row[0]["value"] == "BROKEN_RECOVERY")
        .unwrap();
    assert_eq!(recovery[2]["value"], false);
    let diagnostic = first
        .files
        .rows
        .iter()
        .find(|row| row[0]["value"] == "recovery.db2")
        .unwrap();
    let diagnostic: serde_json::Value =
        serde_json::from_str(diagnostic[1]["value"].as_str().unwrap())?;
    assert!(
        diagnostic
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["source"] == "NativeSyntax")
    );
    assert!(
        !first
            .vectors
            .keys()
            .any(|(path, _, _)| path == "recovery.db2")
    );
    const CHANGED: &str = "CREATE TABLE DB2_NEW(C_NEW INTEGER); SELECT C_NEW FROM DB2_NEW;";
    std::fs::write(selected.0.root().join("model.db2"), CHANGED)?;
    std::fs::remove_file(selected.0.root().join("model.pkb"))?;
    assert!(
        ast.index(&selected.0, &context, rescan(), &cancellation)
            .await?
            .is_some()
    );
    assert!(
        ast.index_semantic(
            selected.0.clone(),
            context.clone(),
            Some(Arc::clone(&resources)),
            None,
            std::collections::BTreeSet::new(),
            false,
            cancellation.clone(),
            cancellation.clone()
        )
        .await?
        .version
        .is_some()
    );
    let changed = observed_antlr_publication(&owner, &source, &selected, &credential, &resources)?;
    assert!(
        changed
            .tables
            .rows
            .iter()
            .any(|row| row[0]["value"] == "DB2_NEW")
    );
    assert!(
        !changed
            .tables
            .rows
            .iter()
            .any(|row| matches!(row[0]["value"].as_str(), Some("DB2_KEEP" | "ORACLE_KEEP")))
    );
    assert!(changed.hits.iter().any(|hit| hit.entity.name == "DB2_NEW"));
    assert!(
        !changed
            .hits
            .iter()
            .any(|hit| matches!(hit.entity.name.as_str(), "DB2_KEEP" | "ORACLE_KEEP"))
    );
    assert!(
        !changed.vectors.is_empty(),
        "changed DB2 still has genuine NN rows"
    );
    assert!(
        changed
            .vectors
            .keys()
            .all(|(path, _, _)| path == "model.db2")
    );
    assert_eq!(
        observed_branch_text(&raw_uri, raw_branch, "model.db2", original.snapshot())
            .await?
            .as_deref(),
        Some(CHANGED)
    );
    assert!(
        observed_branch_text(&raw_uri, raw_branch, "model.pkb", original.snapshot())
            .await?
            .is_none()
    );
    owner.memory_jobs.as_mut().unwrap().shutdown().await?;
    assert!(
        tokio::net::TcpStream::connect(resources.local.as_ref().unwrap().address)
            .await
            .is_err()
    );
    assert!(
        ast.index(&selected.0, &context, rescan(), &cancellation)
            .await?
            .is_none()
    );
    assert!(
        ast.index_semantic(
            selected.0.clone(),
            context,
            Some(Arc::clone(&resources)),
            None,
            std::collections::BTreeSet::new(),
            false,
            cancellation.clone(),
            cancellation.clone()
        )
        .await?
        .version
        .is_some()
    );
    let reused = observed_antlr_publication(&owner, &source, &selected, &credential, &resources)?;
    assert_eq!(
        reused.vectors, changed.vectors,
        "unchanged projections reuse actual vectors and model generation with the endpoint closed"
    );
    source.shutdown().await?;
    tasks.shutdown().await?;
    drop(resources);
    drop(ast);
    drop(source);
    owner.shutdown().await?;
    Ok(())
}

#[path = "foreground_scip_tests.rs"]
mod scip_tests;

#[path = "foreground_oracle_target_rules_tests.rs"]
mod oracle_target_rules;
