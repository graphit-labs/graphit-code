use super::*;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

fn original(duration: Duration, cancellation: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + duration,
        cancellation,
    ))
}
fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 32, 65536).unwrap()
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
async fn target(parent: &Path, snapshot: Arc<SnapshotContext>) -> TargetLock {
    let capture = CapturedDocumentDirectory::capture(parent, limits(), &snapshot).unwrap();
    TargetLock::acquire(capture, limits(), snapshot)
        .await
        .unwrap()
}
async fn prepared(parent: &Path, snapshot: Arc<SnapshotContext>) -> PreparedRelease {
    crate::self_update_release::tests::prepared_release(parent, snapshot, None).await
}

#[tokio::test]
async fn real_preparer_fixed_package_swaps_and_restores_without_changing_other_files() {
    let installed = tempfile::tempdir().unwrap();
    let platform = native_target().unwrap();
    old_package(installed.path(), platform);
    let broker_files = [
        "graphit-runtime/broker/lib/libllama.so.0",
        "graphit-runtime/licenses/broker/ggml/LICENSE",
    ];
    for relative in broker_files {
        let file = installed.path().join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, b"independent-broker-runtime").unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(installed.path(), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let snapshot = original(Duration::from_secs(10), Cancellation::default());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let entries = names(platform);
    let expected_main = fs::read(ready.directory().join(entries[0])).unwrap();
    let lock = target(installed.path(), Arc::clone(&snapshot)).await;
    let stage = CapturedDocumentDirectory::capture(ready.directory(), limits(), &snapshot).unwrap();
    let mut backup = Backup::create(&lock, limits(), &snapshot, &ready).unwrap();
    backup.apply(&lock, &stage, &snapshot).unwrap();
    assert_eq!(
        fs::read(installed.path().join(entries[0])).unwrap(),
        expected_main
    );
    assert!(
        installed
            .path()
            .join("graphit-runtime/agents/coordinator/prompt.md")
            .is_file()
    );
    assert!(
        !installed
            .path()
            .join("graphit-runtime/old-runtime")
            .exists()
    );
    for relative in broker_files {
        let installed_file = installed.path().join(relative);
        let backup_file = backup.directory.join(relative);
        assert_eq!(
            fs::read(&installed_file).unwrap(),
            b"independent-broker-runtime"
        );
        assert_eq!(
            fs::read(&backup_file).unwrap(),
            b"independent-broker-runtime"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                fs::metadata(installed_file).unwrap().ino(),
                fs::metadata(backup_file).unwrap().ino()
            );
        }
    }
    sentinels(installed.path());
    backup.restore(&lock, &stage, &snapshot).unwrap();
    for relative in broker_files {
        assert_eq!(
            fs::read(installed.path().join(relative)).unwrap(),
            b"independent-broker-runtime"
        );
    }
    assert!(!backup.changed());
    assert_eq!(
        fs::read(installed.path().join(entries[0])).unwrap(),
        b"old-main"
    );
    assert_eq!(
        fs::read(installed.path().join(entries[1])).unwrap(),
        b"old-model"
    );
    assert_eq!(
        fs::read(installed.path().join(entries[2]).join("old-runtime")).unwrap(),
        b"old-runtime"
    );
    sentinels(installed.path());
    backup.cleanup().await.unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(installed.path()).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}

#[tokio::test]
async fn real_exclusive_conflict_keeps_first_error_and_restores_only_moved_entries() {
    let installed = tempfile::tempdir().unwrap();
    let platform = native_target().unwrap();
    old_package(installed.path(), platform);
    let snapshot = original(Duration::from_secs(10), Cancellation::default());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let lock = target(installed.path(), Arc::clone(&snapshot)).await;
    let stage = CapturedDocumentDirectory::capture(ready.directory(), limits(), &snapshot).unwrap();
    let mut backup = Backup::create(&lock, limits(), &snapshot, &ready).unwrap();
    let entries = names(platform);
    fs::write(backup.directory.join(entries[1]), b"existing-conflict").unwrap();
    let first_error = backup.apply(&lock, &stage, &snapshot).unwrap_err();
    assert_eq!(first_error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        backup.moves,
        [
            MoveState::Installed,
            MoveState::Original,
            MoveState::Original
        ]
    );
    assert_eq!(
        fs::read(installed.path().join(entries[1])).unwrap(),
        b"old-model"
    );
    backup.restore(&lock, &stage, &snapshot).unwrap();
    assert_eq!(first_error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::read(backup.directory.join(entries[1])).unwrap(),
        b"existing-conflict"
    );
    assert_eq!(
        fs::read(installed.path().join(entries[0])).unwrap(),
        b"old-main"
    );
    sentinels(installed.path());
    backup.cleanup().await.unwrap();
}

#[tokio::test]
async fn cancellation_after_real_effect_retains_old_package_when_original_cannot_restore() {
    let installed = tempfile::tempdir().unwrap();
    let platform = native_target().unwrap();
    old_package(installed.path(), platform);
    let cancellation = Cancellation::default();
    let snapshot = original(Duration::from_secs(10), cancellation.clone());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let lock = target(installed.path(), Arc::clone(&snapshot)).await;
    let stage = CapturedDocumentDirectory::capture(ready.directory(), limits(), &snapshot).unwrap();
    let mut backup = Backup::create(&lock, limits(), &snapshot, &ready).unwrap();
    let entries = names(platform);
    fs::write(backup.directory.join(entries[1]), b"existing-conflict").unwrap();
    let first_error = backup.apply(&lock, &stage, &snapshot).unwrap_err();
    cancellation.cancel();
    assert!(backup.restore(&lock, &stage, &snapshot).is_err());
    let recovery = backup.retain(RecoveryKind::RestorePackage);
    drop(backup);
    assert_eq!(first_error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::read(recovery.directory.join("package").join(entries[0])).unwrap(),
        b"old-main"
    );
    assert_eq!(
        fs::read(installed.path().join(entries[1])).unwrap(),
        b"old-model"
    );
    assert!(!ready.directory().join(entries[0]).try_exists().unwrap());
    sentinels(installed.path());
}

#[tokio::test]
async fn inverse_rename_conflict_preserves_backup_and_never_overwrites_the_conflict() {
    let installed = tempfile::tempdir().unwrap();
    let platform = native_target().unwrap();
    old_package(installed.path(), platform);
    let snapshot = original(Duration::from_secs(10), Cancellation::default());
    let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
    let lock = target(installed.path(), Arc::clone(&snapshot)).await;
    let stage = CapturedDocumentDirectory::capture(ready.directory(), limits(), &snapshot).unwrap();
    let mut backup = Backup::create(&lock, limits(), &snapshot, &ready).unwrap();
    let entries = names(platform);
    fs::write(backup.directory.join(entries[1]), b"existing-conflict").unwrap();
    backup.apply(&lock, &stage, &snapshot).unwrap_err();
    fs::write(ready.directory().join(entries[0]), b"inverse-conflict").unwrap();
    assert_eq!(
        backup.restore(&lock, &stage, &snapshot).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    let recovery = backup.retain(RecoveryKind::RestorePackage);
    drop(backup);
    assert_eq!(
        fs::read(recovery.directory.join("package").join(entries[0])).unwrap(),
        b"old-main"
    );
    assert_eq!(
        fs::read(ready.directory().join(entries[0])).unwrap(),
        b"inverse-conflict"
    );
    sentinels(installed.path());
}

#[tokio::test]
async fn real_target_lock_times_out_then_reuses_the_same_permanent_control_entry() {
    let installed = tempfile::tempdir().unwrap();
    let first = original(Duration::from_secs(5), Cancellation::default());
    let lock = target(installed.path(), Arc::clone(&first)).await;
    let control_path = lock.control.canonical_path().to_owned();
    let lock_path = control_path.join("update.lock");
    assert!(lock_path.is_file());
    let second = original(Duration::from_millis(100), Cancellation::default());
    let capture = CapturedDocumentDirectory::capture(installed.path(), limits(), &second).unwrap();
    let error = match TargetLock::acquire(capture, limits(), second).await {
        Ok(_) => panic!("second updater acquired an occupied native lock"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    drop(lock);
    assert!(lock_path.is_file());
    let third = original(Duration::from_secs(5), Cancellation::default());
    let resumed = target(installed.path(), third).await;
    assert_eq!(resumed.control.canonical_path(), control_path);
    assert!(lock_path.is_file());
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
        let parent =
            CapturedDocumentDirectory::capture(installed.path(), limits(), &snapshot).unwrap();
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
            limits: limits(),
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
            assert!(error.recovery.is_none());
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
            assert!(
                fs::read_dir(installed.path().join(".graphit-update"))
                    .unwrap()
                    .all(|entry| !entry.unwrap().file_type().unwrap().is_dir())
            );
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
    let parent = CapturedDocumentDirectory::capture(installed.path(), limits(), &snapshot)?;
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
        limits: limits(),
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
        let recovery = failure
            .recovery
            .expect("restart failure retains package backup");
        assert_eq!(recovery.kind, RecoveryKind::RestorePackage);
        assert!(
            recovery
                .directory
                .join("package")
                .join(platform.binaries()[0])
                .is_file()
        );
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
    if !block_restart {
        assert!(
            fs::read_dir(installed.path().join(".graphit-update"))?
                .all(|entry| entry.unwrap().file_type().is_ok_and(|kind| !kind.is_dir()))
        );
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn broker_namespace_conflict_or_invalid_entry_rolls_back_without_losing_runtime() {
    for conflict in [false, true] {
        let installed = tempfile::tempdir().unwrap();
        let platform = native_target().unwrap();
        old_package(installed.path(), platform);
        let broker = installed.path().join("graphit-runtime/broker/lib");
        fs::create_dir_all(&broker).unwrap();
        fs::write(broker.join("library"), b"keep-broker").unwrap();
        let snapshot = original(Duration::from_secs(10), Cancellation::default());
        let ready = prepared(installed.path(), Arc::clone(&snapshot)).await;
        if conflict {
            fs::create_dir(ready.directory().join("graphit-runtime/broker")).unwrap();
        } else {
            std::os::unix::fs::symlink("library", broker.join("invalid-link")).unwrap();
        }
        let lock = target(installed.path(), Arc::clone(&snapshot)).await;
        let stage =
            CapturedDocumentDirectory::capture(ready.directory(), limits(), &snapshot).unwrap();
        let mut backup = Backup::create(&lock, limits(), &snapshot, &ready).unwrap();
        assert!(backup.apply(&lock, &stage, &snapshot).is_err());
        assert_eq!(
            fs::read(backup.directory.join("graphit-runtime/broker/lib/library")).unwrap(),
            b"keep-broker"
        );
        backup.restore(&lock, &stage, &snapshot).unwrap();
        backup.cleanup().await.unwrap();
        drop(stage);
        drop(ready);
        assert_eq!(fs::read(broker.join("library")).unwrap(), b"keep-broker");
        assert_eq!(
            fs::read(installed.path().join(names(platform)[0])).unwrap(),
            b"old-main"
        );
    }
}
