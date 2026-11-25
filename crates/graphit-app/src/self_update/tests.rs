use super::*;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

fn original(duration: Duration, cancellation: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + duration,
        cancellation,
    ))
}
fn old_package(parent: &Path, target: Target) {
    let entries = names(target);
    fs::write(parent.join(entries[0]), b"old-main").unwrap();
    fs::write(parent.join(entries[1]), b"old-model").unwrap();
    fs::create_dir(parent.join(entries[2])).unwrap();
    fs::write(parent.join(entries[2]).join("old-runtime"), b"old-runtime").unwrap();
    fs::write(parent.join("config.json"), b"configuration-sentinel").unwrap();
    fs::write(parent.join("credentials"), b"credential-sentinel").unwrap();
}
fn sentinels(parent: &Path) {
    assert_eq!(
        fs::read(parent.join("config.json")).unwrap(),
        b"configuration-sentinel"
    );
    assert_eq!(
        fs::read(parent.join("credentials")).unwrap(),
        b"credential-sentinel"
    );
}
async fn prepared(parent: &Path, snapshot: Arc<SnapshotContext>) -> PreparedRelease {
    crate::self_update_release::tests::prepared_release(parent, snapshot, None).await
}

#[tokio::test]
async fn installs_verified_files_preserving_other_products_and_user_files() {
    let installed = tempfile::tempdir().unwrap();
    let target = native_target().unwrap();
    old_package(installed.path(), target);
    let broker = installed.path().join("graphit-runtime/broker/lib/engine");
    fs::create_dir_all(broker.parent().unwrap()).unwrap();
    fs::write(&broker, b"independent-broker-runtime").unwrap();
    let snapshot = original(Duration::from_secs(10), Cancellation::default());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let files = package_files(&ready, &snapshot).unwrap();
    let lock = lock_target(installed.path(), Arc::clone(&snapshot))
        .await
        .unwrap();
    publish_package(installed.path(), &ready, &files, &snapshot).unwrap();
    for relative in files {
        assert_eq!(
            fs::read(installed.path().join(&relative)).unwrap(),
            fs::read(ready.directory().join(relative)).unwrap()
        );
    }
    assert_eq!(fs::read(broker).unwrap(), b"independent-broker-runtime");
    sentinels(installed.path());
    drop(lock);
    assert!(!installed.path().join(".graphit-update").exists());
}

#[tokio::test]
async fn partial_error_reports_applied_files_without_rollback_and_allows_explicit_rerun() {
    let installed = tempfile::tempdir().unwrap();
    let target = native_target().unwrap();
    old_package(installed.path(), target);
    let snapshot = original(Duration::from_secs(10), Cancellation::default());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let [main, model] = target.binaries();
    // A directory in the place of the model executable is a real filesystem error.
    fs::remove_file(installed.path().join(model)).unwrap();
    fs::create_dir(installed.path().join(model)).unwrap();
    let files = vec![PathBuf::from(main), PathBuf::from(model)];
    let error = publish_package(installed.path(), &ready, &files, &snapshot).unwrap_err();
    assert_eq!(error.package, PackageState::Partial);
    assert_eq!(
        fs::read(installed.path().join(main)).unwrap(),
        fs::read(ready.directory().join(main)).unwrap()
    );
    assert!(installed.path().join(model).is_dir());
    sentinels(installed.path());
    // User resolves the reported conflict, then explicitly invokes the ordinary path again.
    fs::remove_dir(installed.path().join(model)).unwrap();
    publish_package(installed.path(), &ready, &files, &snapshot).unwrap();
    assert_eq!(
        fs::read(installed.path().join(model)).unwrap(),
        fs::read(ready.directory().join(model)).unwrap()
    );
}

#[tokio::test]
async fn cancelled_publication_does_not_replace_any_file() {
    let installed = tempfile::tempdir().unwrap();
    let target = native_target().unwrap();
    old_package(installed.path(), target);
    let cancellation = Cancellation::default();
    let snapshot = original(Duration::from_secs(10), cancellation.clone());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let files = package_files(&ready, &snapshot).unwrap();
    cancellation.cancel();
    let error = publish_package(installed.path(), &ready, &files, &snapshot).unwrap_err();
    assert_eq!(error.package, PackageState::Unchanged);
    assert!(
        matches!(error.cause, Cause::Io { error, .. } if error.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(
        fs::read(installed.path().join(target.binaries()[0])).unwrap(),
        b"old-main"
    );
    sentinels(installed.path());
}

#[tokio::test]
async fn occupied_target_lock_excludes_another_updater_and_releases_on_drop() {
    let installed = tempfile::tempdir().unwrap();
    let first = lock_target(
        installed.path(),
        original(Duration::from_secs(5), Cancellation::default()),
    )
    .await
    .unwrap();
    let error = lock_target(
        installed.path(),
        original(Duration::from_millis(50), Cancellation::default()),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    drop(first);
    let next = lock_target(
        installed.path(),
        original(Duration::from_secs(5), Cancellation::default()),
    )
    .await
    .unwrap();
    drop(next);
}

#[cfg(unix)]
#[tokio::test]
async fn configured_parent_and_runtime_links_use_conventional_access() {
    use std::os::unix::fs::symlink;
    let installed = tempfile::tempdir().unwrap();
    let target = native_target().unwrap();
    old_package(installed.path(), target);
    let external = tempfile::tempdir().unwrap();
    fs::rename(
        installed.path().join("graphit-runtime"),
        external.path().join("runtime"),
    )
    .unwrap();
    symlink(
        external.path().join("runtime"),
        installed.path().join("graphit-runtime"),
    )
    .unwrap();
    let alias = external.path().join("installation ação");
    symlink(installed.path(), &alias).unwrap();
    let snapshot = original(Duration::from_secs(10), Cancellation::default());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let files = package_files(&ready, &snapshot).unwrap();
    publish_package(&alias, &ready, &files, &snapshot).unwrap();
    assert!(
        fs::symlink_metadata(installed.path().join("graphit-runtime"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(alias.join(target.binaries()[0])).unwrap(),
        fs::read(ready.directory().join(target.binaries()[0])).unwrap()
    );
    sentinels(installed.path());
}

#[tokio::test]
async fn code_release_cannot_claim_broker_namespace() {
    let installed = tempfile::tempdir().unwrap();
    old_package(installed.path(), native_target().unwrap());
    let snapshot = original(Duration::from_secs(10), Cancellation::default());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    fs::create_dir_all(ready.directory().join("graphit-runtime/broker")).unwrap();
    assert_eq!(
        package_files(&ready, &snapshot).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    sentinels(installed.path());
}

#[tokio::test]
async fn inactive_install_and_missing_status_cannot_bypass_the_real_occupied_singleton() {
    use graphit_core::{brand::Brand, config::ConfigSnapshot, project_paths::InstallationPaths};
    use graphit_hub::policy::HubResourcePolicy;
    use graphit_process::{HostLimits, HostReadiness, HostShutdown};

    for occupied in [false, true] {
        let installed = tempfile::tempdir().unwrap();
        let platform = native_target().unwrap();
        old_package(installed.path(), platform);
        let home = tempfile::tempdir().unwrap();
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let installation = InstallationPaths::resolve(
            brand.clone(),
            home.path(),
            None,
            Some(&home.path().join("installation")),
        )
        .unwrap();
        graphit_fs::create_private_directory(installation.root()).unwrap();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancellation = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(if occupied { 1 } else { 10 });
        let original = RequestContext::new(policy.content(), &cancellation, deadline).unwrap();
        let snapshot = Arc::clone(original.snapshot());
        let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
        let entries = names(platform);
        let expected_main = fs::read(ready.directory().join(entries[0])).unwrap();
        let parent = installed.path().to_path_buf();
        let singleton_path = installation.global_path(&["daemon.pid"]).unwrap();
        let singleton = occupied.then(|| {
            FileLock::acquire_in_existing_parent_until(
                &singleton_path,
                Some(deadline),
                &cancellation,
            )
            .unwrap()
        });
        let stop = Cancellation::default();
        let host = ProcessHost::install(&stop, HostLimits::new(1, 1).unwrap()).unwrap();
        assert!(matches!(host.ready(deadline).await, HostReadiness::Ready));
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let executable = installed.path().join(entries[0]);
        let inputs = LauncherInputs {
            installation: &installation,
            executable: &executable,
            environment: &[],
            client: &client,
            termination_grace: Duration::from_secs(1),
            cleanup: Duration::from_secs(3),
        };
        assert!(
            daemon_launcher::control(&inputs, &original, Operation::Status, None)
                .await
                .unwrap()
                .is_none()
        );
        let outcome = install_prepared(&host, &inputs, &original, parent, ready).await;
        stop.cancel();
        assert!(
            matches!(host.shutdown(Instant::now() + Duration::from_secs(3)).await,
            HostShutdown::Joined(report) if report.failure().is_none())
        );
        if occupied {
            let error = outcome.unwrap_err();
            assert!(
                matches!(error.cause, Cause::Io { operation: "daemon singleton", error }
                if error.kind() == io::ErrorKind::TimedOut)
            );
            assert_eq!(error.package, PackageState::Unchanged);
            assert_eq!(error.daemon, DaemonEffect::Untouched);
            assert_eq!(fs::read(&executable).unwrap(), b"old-main");
        } else {
            assert!(matches!(
                outcome.unwrap(),
                Outcome::Installed {
                    daemon: DaemonEffect::Inactive,
                    ..
                }
            ));
            assert_eq!(fs::read(&executable).unwrap(), expected_main);
            let files = fs::read_dir(installation.root())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            assert_eq!(files, [std::ffi::OsString::from("daemon.pid")]);
        }
        sentinels(installed.path());
        drop(singleton);
    }
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires GRAPHIT_TEST_GRAPHIT_BIN pointing to the coherent normal executable"]
async fn real_running_daemon_restarts_from_the_verified_package_without_installing_a_service()
-> Result<(), Box<dyn std::error::Error>> {
    let main = PathBuf::from(
        std::env::var_os("GRAPHIT_TEST_GRAPHIT_BIN")
            .ok_or_else(|| io::Error::other("GRAPHIT_TEST_GRAPHIT_BIN is required"))?,
    );
    for block_restart in [false, true] {
        running_daemon_case(&main, block_restart).await?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
async fn running_daemon_case(
    main: &Path,
    block_restart: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use graphit_core::{
        brand::Brand,
        config::{ConfigKey, ConfigSnapshot, ConfigStore},
        project_paths::InstallationPaths,
    };
    use graphit_hub::policy::HubResourcePolicy;
    use graphit_process::{HostLimits, HostReadiness, HostShutdown};
    use std::{ffi::OsString, process::Stdio};
    let installed = tempfile::tempdir()?;
    let platform = native_target()?;
    old_package(installed.path(), platform);
    let executable = installed.path().join(platform.binaries()[0]);
    fs::copy(main, &executable)?;
    let home = tempfile::tempdir()?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let installation = InstallationPaths::resolve(
        brand.clone(),
        home.path(),
        None,
        Some(&home.path().join("installation")),
    )?;
    let environment = vec![
        (
            OsString::from("PATH"),
            std::env::var_os("PATH").unwrap_or_default(),
        ),
        (OsString::from("HOME"), home.path().as_os_str().to_owned()),
        (
            OsString::from("XDG_RUNTIME_DIR"),
            home.path()
                .join("runtime-without-session-bus")
                .into_os_string(),
        ),
    ];
    let setup = std::process::Command::new(&executable)
        .env_clear()
        .envs(environment.iter().cloned())
        .current_dir(home.path())
        .arg("--global-dir")
        .arg(installation.root())
        .args(["setup", "--agent", "codex"])
        .output()?;
    if !setup.status.success() {
        return Err(io::Error::other("isolated real CLI setup failed").into());
    }
    let config = ConfigSnapshot::new(&brand, [HubResourcePolicy::distribution_defaults(&brand)?])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(120);
    let original = RequestContext::new(policy.content(), &cancellation, deadline)?;
    ConfigStore::global(&installation)?.set(
        ConfigKey::new("modules.ast")?,
        serde_json::json!(false),
        deadline.saturating_duration_since(Instant::now()),
        &cancellation,
    )?;
    let snapshot = Arc::clone(original.snapshot());
    let ready = crate::self_update_release::tests::prepared_release(
        installed.path(),
        Arc::clone(&snapshot),
        Some(main),
    )
    .await;
    let parent = installed.path().to_path_buf();
    let stop = Cancellation::default();
    let host = ProcessHost::install(&stop, HostLimits::new(1, 1)?)?;
    assert!(matches!(host.ready(deadline).await, HostReadiness::Ready));
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let inputs = LauncherInputs {
        installation: &installation,
        executable: &executable,
        environment: &environment,
        client: &client,
        termination_grace: Duration::from_secs(1),
        cleanup: Duration::from_secs(3),
    };
    let log = fs::File::create(home.path().join("runtime.log"))?;
    let mut first = tokio::process::Command::new(&executable)
        .env_clear()
        .envs(environment.iter().cloned())
        .current_dir(home.path())
        .arg("--global-dir")
        .arg(installation.root())
        .args(["daemon", "--managed"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .kill_on_drop(true)
        .spawn()?;
    let mut blocked_port = None;
    let observed: Result<_, Box<dyn std::error::Error>> = async {
        let before = loop {
            original.check()?;
            if let Some(reply) =
                daemon_launcher::control(&inputs, &original, Operation::Status, None).await?
                && matches!(reply.state, State::Running)
            {
                break reply;
            }
            if first.try_wait()?.is_some() {
                return Err(io::Error::other("foreground exited before readiness").into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        if block_restart {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            ConfigStore::global(&installation)?.set(
                ConfigKey::new("mcp.port")?,
                serde_json::json!(listener.local_addr()?.port()),
                deadline.saturating_duration_since(Instant::now()),
                &cancellation,
            )?;
            blocked_port = Some(listener);
        }
        let outcome = install_prepared(&host, &inputs, &original, parent, ready).await;
        if block_restart {
            return Ok((before.pid, None, outcome));
        }
        let outcome = outcome?;
        let after = daemon_launcher::control(&inputs, &original, Operation::Status, None)
            .await?
            .ok_or_else(|| io::Error::other("updated daemon has no authenticated readiness"))?;
        Ok((before.pid, Some(after.pid), Ok(outcome)))
    }
    .await;
    let stopped = daemon_launcher::execute(
        &host,
        launcher_inputs(&inputs),
        &original,
        &ServiceCommand::Stop,
    )
    .await;
    let first_joined = match tokio::time::timeout(Duration::from_secs(5), first.wait()).await {
        Ok(result) => result,
        Err(_) => {
            first.start_kill()?;
            first.wait().await
        }
    };
    stop.cancel();
    let joined = host.shutdown(Instant::now() + Duration::from_secs(3)).await;
    let (before, after, outcome) = observed?;
    if block_restart {
        let failure = outcome.unwrap_err();
        assert_eq!(failure.package, PackageState::Installed);
        assert_eq!(failure.daemon, DaemonEffect::RestartRequested);
        assert!(matches!(failure.cause, Cause::Launcher(error)
            if matches!(*error, daemon_launcher::Failure::Startup { .. })));
        assert_eq!(
            ConfigStore::global(&installation)?
                .load()?
                .get::<u16>(&ConfigKey::new("mcp.port")?)?,
            Some(blocked_port.as_ref().unwrap().local_addr()?.port())
        );
    } else {
        let after = after.expect("authenticated replacement daemon PID");
        assert_ne!(before, after);
        assert!(
            matches!(outcome?, Outcome::Installed { daemon: DaemonEffect::Restarted { pid }, .. } if pid == after)
        );
    }
    assert!(matches!(stopped?, daemon_launcher::Outcome::Stopped));
    assert!(first_joined?.success());
    assert!(matches!(joined, HostShutdown::Joined(report) if report.failure().is_none()));
    sentinels(installed.path());
    assert!(!home.path().join(".config/systemd/user").exists());
    Ok(())
}
