//! Real native-control notifications and their observed SDK Raw publications.
use super::*;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn wait_raw_rows(
    uris: &[PathBuf],
    expected: &[(&str, Option<&str>)],
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_error = None;
    loop {
        let mut matched = true;
        for uri in uris {
            for (path, text) in expected {
                match observed_text(uri, path, original).await {
                    Ok(observed) if observed.as_deref() == *text => {}
                    Ok(_) => matched = false,
                    Err(error) => {
                        matched = false;
                        last_error = Some(error);
                    }
                }
            }
        }
        if matched {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(match last_error {
                Some(error) => io::Error::new(io::ErrorKind::TimedOut, error),
                None => io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("native Raw observation did not reach {expected:?}"),
                ),
            });
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn raw_version(uri: &Path) -> io::Result<u64> {
    Ok(
        LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
            .await
            .map_err(io_error)?
            .capture_target(None)
            .await
            .map_err(io_error)?
            .version(),
    )
}

async fn unchanged_raw_rows(
    uris: &[PathBuf],
    expected: &[(&str, Option<&str>)],
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> io::Result<()> {
    let mut versions = Vec::new();
    for uri in uris {
        versions.push(raw_version(uri).await?);
    }
    // The App's real native debounce is 200ms. Observe more than three intervals
    // without driving any callback or requesting a rescan.
    let deadline = Instant::now() + Duration::from_millis(700);
    loop {
        for (uri, version) in uris.iter().zip(&versions) {
            if raw_version(uri).await? != *version {
                return Err(io::Error::other(
                    "unrelated/disabled edit published a Raw version",
                ));
            }
            for (path, text) in expected {
                if observed_text(uri, path, original).await?.as_deref() != *text {
                    return Err(io::Error::other(format!(
                        "unexpected Raw change for {path}"
                    )));
                }
            }
        }
        if Instant::now() >= deadline {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn stop_foreground(
    stop: &Cancellation,
    running: tokio::task::JoinHandle<Result<(), crate::foreground_source::ForegroundFailure>>,
) -> TestResult {
    stop.cancel();
    if let Err(mut failure) = running.await? {
        // Retain and join the owner even when stop meets a started SDK operation.
        let owner = failure.owner.take().ok_or_else(|| {
            io::Error::other("failed foreground shutdown did not retain its owner")
        })?;
        owner.shutdown().await?;
        assert_cancelled_raw(&failure.error);
    }
    Ok(())
}

fn profile(path: &Path, extension: &str) -> io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(
        path,
        format!("language: rust\ngrammar: tree-sitter-rust\nextensions: [\"{extension}\"]\n"),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_external_runtime_user_and_global_config_reload_two_projects() -> TestResult {
    let (temp, installation, first, config, policy) = ast_fixture();
    // USER queries share the installation's actual protected artifact namespace.
    drop(graphit_fs::create_private_directory(
        &installation.root().join("ast"),
    )?);
    let second_root = temp.path().join("second-project");
    std::fs::create_dir(&second_root)?;
    let second = ProjectPaths::at(&second_root, &installation)?;
    let setup_cancel = Cancellation::default();
    ProjectIdentityStore::at(&second)
        .writer(Duration::from_secs(5), &setup_cancel)?
        .ensure(&ProjectMetadata::new("second-hot-reload-project")?)?;
    for project in [&first, &second] {
        for (path, text) in [
            ("base.rs", "stock initial"),
            ("selected.runtime", "current runtime body"),
            ("selected.user", "user override body"),
            ("selected.global", "global grammar mapping body"),
            ("selected.sibling", "unrelated sibling body"),
        ] {
            std::fs::write(project.root().join(path), text)?;
        }
    }
    let startup = policy.request_context(Instant::now(), &setup_cancel)?;
    let owner = ForegroundSource::start_and_publish(
        &installation,
        &[
            ProjectBootstrapInput {
                project: &first,
                explicit_version: "",
                pin: None,
                detached_base: "",
            },
            ProjectBootstrapInput {
                project: &second,
                explicit_version: "",
                pin: None,
                detached_base: "",
            },
        ],
        0,
        &config,
        NativeBinding::Codex,
        Vec::new(),
        &startup,
    )
    .await?;
    assert_eq!(owner.supervisors.supervised().count(), 2);
    assert!(owner.memory_jobs.is_none());
    let uris = [&first, &second]
        .into_iter()
        .map(|project| {
            let id = ProjectIdentityStore::at(project)
                .load()
                .unwrap()
                .unwrap()
                .id();
            project
                .project_runtime(id, &["ast", "source.lance"])
                .unwrap()
        })
        .collect::<Vec<_>>();
    let service = Cancellation::default();
    let stop = service.clone();
    let running = tokio::spawn(async move { owner.run(&service).await });
    let observe_cancel = Cancellation::default();
    let observe = policy.request_context(Instant::now(), &observe_cancel)?;
    let result: TestResult = async {
        wait_raw_rows(
            &uris,
            &[
                ("base.rs", Some("stock initial")),
                ("selected.runtime", None),
            ],
            observe.snapshot(),
        )
        .await?;
        // Both external profile directories are initially absent. Creating the
        // current runtime path must be observed through its existing ancestor.
        let runtime = installation
            .framework_runtime(env!("GRAPHIT_COMPILED_BUILD_VERSION"))?
            .join("ast/queries/rust.yaml");
        profile(&runtime, ".runtime")?;
        wait_raw_rows(
            &uris,
            &[
                ("selected.runtime", Some("current runtime body")),
                ("base.rs", None),
            ],
            observe.snapshot(),
        )
        .await?;

        let sibling = installation
            .framework_runtime("unrelated-sibling-version")?
            .join("ast/queries/rust.yaml");
        profile(&sibling, ".sibling")?;
        unchanged_raw_rows(
            &uris,
            &[
                ("selected.runtime", Some("current runtime body")),
                ("selected.sibling", None),
            ],
            observe.snapshot(),
        )
        .await?;

        let user = installation.global_path(&["ast", "queries"])?;
        profile(&user.join("rust.yaml"), ".user")?;
        wait_raw_rows(
            &uris,
            &[
                ("selected.user", Some("user override body")),
                ("selected.runtime", None),
            ],
            observe.snapshot(),
        )
        .await?;

        let global = ConfigStore::global(&installation)?;
        global.set(
            ConfigKey::new("ast.grammar")?,
            json!("global=tree-sitter-rust"),
            Duration::from_secs(5),
            &setup_cancel,
        )?;
        wait_raw_rows(
            &uris,
            &[
                ("selected.global", Some("global grammar mapping body")),
                ("selected.user", Some("user override body")),
            ],
            observe.snapshot(),
        )
        .await?;

        global.set(
            ConfigKey::new("modules.ast")?,
            json!(false),
            Duration::from_secs(5),
            &setup_cancel,
        )?;
        for project in [&first, &second] {
            std::fs::write(
                project.root().join("while-disabled.user"),
                "created while disabled",
            )?;
        }
        unchanged_raw_rows(
            &uris,
            &[
                ("while-disabled.user", None),
                ("selected.user", Some("user override body")),
            ],
            observe.snapshot(),
        )
        .await?;

        // No source file is touched after enabling: the external global config
        // notification alone must publish the file previously left unindexed.
        global.set(
            ConfigKey::new("modules.ast")?,
            json!(true),
            Duration::from_secs(5),
            &setup_cancel,
        )?;
        wait_raw_rows(
            &uris,
            &[("while-disabled.user", Some("created while disabled"))],
            observe.snapshot(),
        )
        .await?;
        let mut output = Vec::new();
        crate::source_entry::call_source(
            &installation,
            &source_http_client(),
            &serde_json::from_value(json!({"path":"while-disabled.user"}))?,
            NativeDocumentFileLimits::new(
                65536,
                graphit_core::native_path::MAX_PATH_BYTES,
                64,
                65536,
            )?,
            &observe,
            &mut output,
        )
        .await?;
        if output != b"created while disabled" {
            return Err(io::Error::other("actual Source HTTP did not read re-enabled Raw").into());
        }
        Ok(())
    }
    .await;
    let stopped = stop_foreground(&stop, running).await;
    result?;
    stopped?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_project_queries_dir_retarget_then_native_profile_edit_under_ignore() -> TestResult
{
    let (_temp, installation, project, config, policy) = ast_fixture();
    std::fs::write(project.root().join("base.rs"), "before retarget")?;
    std::fs::write(
        project.root().join("selected.retarget"),
        "first retarget body",
    )?;
    std::fs::write(
        project.root().join("selected.retarget2"),
        "edited retarget body",
    )?;
    std::fs::write(project.root().join(".astignore"), "target/\n")?;
    let profile_path = project.root().join("target/private/queries/rust.yaml");
    profile(&profile_path, ".retarget")?;
    let setup_cancel = Cancellation::default();
    let startup = policy.request_context(Instant::now(), &setup_cancel)?;
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
    let id = ProjectIdentityStore::at(&project).load()?.unwrap().id();
    let uris = [project.project_runtime(id, &["ast", "source.lance"])?];
    let service = Cancellation::default();
    let stop = service.clone();
    let running = tokio::spawn(async move { owner.run(&service).await });
    let observe_cancel = Cancellation::default();
    let observe = policy.request_context(Instant::now(), &observe_cancel)?;
    let result: TestResult = async {
        wait_raw_rows(
            &uris,
            &[
                ("base.rs", Some("before retarget")),
                ("selected.retarget", None),
            ],
            observe.snapshot(),
        )
        .await?;
        ConfigStore::project(&project)?.set(
            ConfigKey::new("ast.queries_dir")?,
            json!("target/private/queries"),
            Duration::from_secs(5),
            &setup_cancel,
        )?;
        wait_raw_rows(
            &uris,
            &[
                ("selected.retarget", Some("first retarget body")),
                ("base.rs", None),
            ],
            observe.snapshot(),
        )
        .await?;
        // This file is under a protected ignore ancestor. Only the newly
        // retargeted control watch can observe this second profile edit.
        profile(&profile_path, ".retarget2")?;
        wait_raw_rows(
            &uris,
            &[
                ("selected.retarget2", Some("edited retarget body")),
                ("selected.retarget", None),
            ],
            observe.snapshot(),
        )
        .await?;
        Ok(())
    }
    .await;
    let stopped = stop_foreground(&stop, running).await;
    result?;
    stopped?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn structural_failure_recovers_on_unselected_watcher_without_a_new_raw_version() -> TestResult
{
    let (_temp, installation, project, config, policy) = ast_fixture();
    std::fs::write(project.root().join("source.json"), "{\"answer\":42}\n")?;
    let cancellation = Cancellation::default();
    let id = ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancellation)?
        .ensure(&ProjectMetadata::new("live-ast-recovery")?)?
        .id();
    let startup = policy.request_context(Instant::now(), &cancellation)?;
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
        Vec::new(),
        &startup,
    )
    .await?;
    let raw = project.project_runtime(id, &["ast", "source.lance"])?;
    wait_raw_rows(
        std::slice::from_ref(&raw),
        &[("source.json", Some("{\"answer\":42}\n"))],
        startup.snapshot(),
    )
    .await?;
    owner.supervisors.shutdown().await?;
    let version = raw_version(&raw).await?;
    // Graph publication now belongs to the progressive Raw producer. Establish
    // its real baseline first, then block its parent without deleting either
    // the Source dataset or the retained graph. A failed retry must not commit
    // another Raw version merely because graph admission is unavailable.
    let parent = project.project_runtime(id, &["ast"])?;
    let blocked = parent.join("live");
    let retained = parent.join("retained-live");
    std::fs::rename(&blocked, &retained)?;
    std::fs::write(&blocked, "not a directory")?;
    let failed = owner.ast_jobs.as_ref().unwrap().observe_next_raw_error();
    owner
        .attach_jobs(&installation, &config, NativeBinding::Codex, &[], &startup)
        .await?;
    tokio::time::timeout(Duration::from_secs(30), failed).await??;
    assert_eq!(raw_version(&raw).await?, version);
    std::fs::remove_file(&blocked)?;
    std::fs::rename(retained, &blocked)?;
    let batch = owner.ast_jobs.as_ref().unwrap().observe_next_raw_batch();
    let note = project.root().join("notes.unselected-extension");
    std::fs::write(&note, "no AST profile")?;
    let (rescan, paths) = tokio::time::timeout(Duration::from_secs(10), batch).await??;
    assert!(
        !rescan,
        "recovery must be driven by the ordinary unsupported-file delta"
    );
    assert_eq!(paths, std::collections::BTreeSet::from([note]));
    let source = owner.server.as_ref().unwrap();
    let context = owner.supervisors.supervised().next().unwrap();
    let key = owner.auth.runtime().check(startup.snapshot())?;
    let credential = std::str::from_utf8(key.secret(startup.snapshot())?)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let result = source.graph_server().call_mcp(
            rmcp::model::CallToolRequestParams::new("graphit_ast_query").with_arguments(json!({
                "project_dir":graphit_core::native_path::NativePath::new(project.root().to_owned())?,
                "project_version":context.version.version(),
                "query":"MATCH(n:AstFileObservation) RETURN n.path,n.raw_version",
            }).as_object().unwrap().clone()),
            axum::http::HeaderMap::from_iter([
                (axum::http::header::AUTHORIZATION,axum::http::HeaderValue::from_str(&format!("Bearer {credential}"))?),
                (axum::http::HeaderName::from_static("x-graphit-auth-origin"),axum::http::HeaderValue::from_static("runtime")),
            ]), CancellationToken::new(),
        ).await;
        if result.is_error != Some(true)
            && result
                .structured_content
                .as_ref()
                .and_then(|value| value["rows"].as_array())
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row == &json!([{"type":"STRING","value":"source.json"},
                    {"type":"STRING","value":version.to_string()}])
                    })
                })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "structural recovery did not expose native Graph after a Raw no-op: {result:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(raw_version(&raw).await?, version);
    owner.shutdown().await?;
    Ok(())
}
