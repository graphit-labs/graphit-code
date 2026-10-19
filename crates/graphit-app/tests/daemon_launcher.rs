#![cfg(target_os = "linux")]

use graphit_app::daemon_launcher::{ManagerAvailability, manager_availability};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    ffi::OsString,
    io,
    sync::Arc,
    time::{Duration, Instant},
};

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        Cancellation::default(),
    ))
}

#[tokio::test]
async fn absent_private_bus_is_unavailable() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let environment = vec![("XDG_RUNTIME_DIR".into(), temp.path().as_os_str().to_owned())];
    assert_eq!(
        manager_availability(&environment, &original()).await?,
        ManagerAvailability::Unavailable
    );
    // Connect to a real private broker: it has no systemd manager owner.
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut bus =
        tokio::process::Command::new(which::which("dbus-daemon").map_err(io::Error::other)?)
            .args(["--session", "--nofork", "--print-address=1"])
            .env_clear()
            .env("HOME", temp.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
    let mut output = BufReader::new(
        bus.stdout
            .take()
            .ok_or_else(|| io::Error::other("private broker stdout absent"))?,
    );
    let mut address = String::new();
    let read = tokio::time::timeout(Duration::from_secs(5), output.read_line(&mut address))
        .await
        .map_err(io::Error::other)??;
    assert_ne!(read, 0, "private broker did not publish its address");
    let environment = vec![("DBUS_SESSION_BUS_ADDRESS".into(), address.trim_end().into())];
    assert_eq!(
        manager_availability(&environment, &original()).await?,
        ManagerAvailability::Unavailable
    );
    bus.start_kill()?;
    bus.wait().await?;
    Ok(())
}

#[tokio::test]
async fn malformed_and_cancelled_requests_do_not_poison_the_next_request() -> io::Result<()> {
    let invalid = vec![("DBUS_SESSION_BUS_ADDRESS".into(), "invalid-address".into())];
    assert!(manager_availability(&invalid, &original()).await.is_err());
    let cancelled = original();
    cancelled.cancellation().cancel();
    let environment = vec![("DBUS_SESSION_BUS_ADDRESS".into(), "invalid-address".into())];
    assert_eq!(
        manager_availability(&environment, &cancelled)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let temp = tempfile::tempdir()?;
    let environment = vec![("XDG_RUNTIME_DIR".into(), temp.path().as_os_str().to_owned())];
    assert_eq!(
        manager_availability(&environment, &original()).await?,
        ManagerAvailability::Unavailable
    );
    Ok(())
}

#[tokio::test]
async fn literal_captured_runtime_and_protocol_failure_do_not_authorize_fallback() -> io::Result<()>
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let temp = tempfile::tempdir()?;
    let runtime = temp.path().join("runtime space,% café Ω");
    std::fs::create_dir(&runtime)?;
    let listener = tokio::net::UnixListener::bind(runtime.join("bus"))?;
    let environment = vec![("XDG_RUNTIME_DIR".into(), runtime.into_os_string())];
    let server = async {
        let (mut socket, _) = listener.accept().await?;
        let mut bytes = [0; 128];
        assert_ne!(socket.read(&mut bytes).await?, 0);
        socket
            .write_all(b"ERROR deliberate protocol rejection\r\n")
            .await?;
        socket.shutdown().await
    };
    let context = original();
    let (result, accepted) = tokio::join!(
        manager_availability(&environment, &context),
        tokio::time::timeout(Duration::from_secs(5), server)
    );
    accepted.map_err(io::Error::other)??;
    assert_ne!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    Ok(())
}

#[tokio::test]
async fn service_composition_is_per_installation_and_survives_removed_checkout()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_app::daemon_launcher::compose_service;
    use graphit_core::{brand::Brand, project_paths::InstallationPaths};
    use graphit_process::{HostLimits, HostReadiness, HostShutdown, ProcessHost};
    use zbus::address::{
        Address, Transport,
        transport::{Unix, UnixSocket},
    };

    let temp = tempfile::tempdir()?;
    let checkout = temp.path().join("checkout");
    std::fs::create_dir(&checkout)?;
    let root = temp.path().join("installation space,%测试");
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let installation = InstallationPaths::resolve(brand.clone(), &checkout, None, Some(&root))?;
    let second =
        InstallationPaths::resolve(brand, temp.path(), None, Some(&temp.path().join("other")))?;
    std::fs::remove_dir(&checkout)?;
    let config = temp.path().join("captured-config");
    let runtime = temp.path().join("runtime");
    let address = Address::new(Transport::Unix(Unix::new(UnixSocket::File(
        runtime.join("bus"),
    ))));
    let environment = vec![
        (
            "PATH".into(),
            std::env::var_os("PATH").ok_or("PATH absent")?,
        ),
        ("HOME".into(), temp.path().as_os_str().to_owned()),
        ("XDG_CONFIG_HOME".into(), config.as_os_str().to_owned()),
        ("XDG_RUNTIME_DIR".into(), runtime.into_os_string()),
        (
            "DBUS_SESSION_BUS_ADDRESS".into(),
            address.to_string().into(),
        ),
    ];
    let host = ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1)?)?;
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    {
        let first_request = original();
        let executable = std::path::Path::new(env!("CARGO_BIN_EXE_graphit"));
        let service = compose_service(
            &host,
            &installation,
            executable,
            &environment,
            &first_request,
        )?;
        let other = compose_service(&host, &second, executable, &environment, &first_request)?;
        assert_eq!(service.definition.global_dir, root);
        assert_eq!(service.definition.executable, executable);
        assert_eq!(service.definition.uid, rustix::process::geteuid().as_raw());
        assert_eq!(service.definition.user, whoami::username()?);
        assert_ne!(service.definition.name, other.definition.name);
        assert_ne!(service.files.registration, other.files.registration);
        assert_eq!(
            service.files.registration.parent(),
            Some(config.join("systemd/user").as_path())
        );
        first_request.cancellation().cancel();
        // A real manager utility must run from the installation, even after its
        // selecting checkout disappeared and the composing request was cancelled.
        let status = service.status(&original()).await?;
        assert!(!status.status.success());
    }
    match host.shutdown(Instant::now() + Duration::from_secs(5)).await {
        HostShutdown::Joined(report) => assert!(report.failure().is_none()),
        HostShutdown::Pending(_) => panic!("private process host did not join"),
    }
    Ok(())
}

#[tokio::test]
async fn detached_launcher_authenticates_restarts_stops_and_cleans_failed_startup()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_app::{
        daemon_launcher::{self, Failure, Inputs, Outcome},
        foreground_source::ForegroundSource,
        service_entry::ServiceCommand,
    };
    use graphit_core::{
        brand::Brand,
        config::{ConfigKey, ConfigSnapshot, ConfigStore},
        file_lock::FileLock,
        native_document_file::NativeDocumentFileLimits,
        project_paths::InstallationPaths,
    };
    use graphit_hub::{
        NativeBinding,
        policy::{HubResourcePolicy, ResourceSetting},
    };
    use graphit_process::{HostLimits, HostReadiness, HostShutdown, ProcessHost};
    let temp = tempfile::tempdir()?;
    let checkout = temp.path().join("checkout");
    std::fs::create_dir(&checkout)?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let installation = InstallationPaths::resolve(
        brand.clone(),
        &checkout,
        None,
        Some(&temp.path().join("installation space,%测试")),
    )?;
    let cancellation = Cancellation::default();
    let config = ConfigSnapshot::new(&brand, [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    ForegroundSource::start(
        &installation,
        &[],
        0,
        &config,
        NativeBinding::Codex,
        &original,
    )?
    .shutdown()
    .await?;
    ConfigStore::global(&installation)?.set(
        ConfigKey::new("agent")?,
        serde_json::json!("codex"),
        Duration::from_secs(5),
        &cancellation,
    )?;
    std::fs::remove_dir(&checkout)?;
    let environment = vec![
        (
            "PATH".into(),
            std::env::var_os("PATH").ok_or("PATH absent")?,
        ),
        ("HOME".into(), temp.path().as_os_str().to_owned()),
        (
            "XDG_RUNTIME_DIR".into(),
            temp.path().join("absent-bus").into_os_string(),
        ),
        ("GRAPHIT_MODULES__AST".into(), "false".into()),
    ];
    let number = |setting| usize::try_from(policy.limit(setting).value);
    let limits = NativeDocumentFileLimits::new(
        number(ResourceSetting::ContentInputBytes)?,
        graphit_core::native_path::MAX_PATH_BYTES,
        number(ResourceSetting::SnapshotDepth)?,
        number(ResourceSetting::ContentScalarBytes)?,
    )?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let host = ProcessHost::install(&cancellation, HostLimits::new(2, 2)?)?;
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    let inputs = |executable| Inputs {
        installation: &installation,
        executable,
        environment: &environment,
        client: &client,
        limits,
        termination_grace: Duration::from_secs(1),
        cleanup: Duration::from_secs(3),
    };
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_graphit"));
    let journey = async {
        let request = policy.request_context(Instant::now(), &cancellation)?;
        let Outcome::Running(first) =
            daemon_launcher::execute(&host, inputs(executable), &request, &ServiceCommand::Start)
                .await?
        else {
            return Err(io::Error::other("start did not authenticate readiness").into());
        };
        let argv = std::fs::read(format!("/proc/{}/cmdline", first.pid))?;
        let arguments: Vec<_> = argv
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .collect();
        let expected = [
            executable.as_os_str().as_encoded_bytes(),
            b"--global-dir",
            installation.root().as_os_str().as_encoded_bytes(),
            b"daemon",
            b"--detached",
        ];
        if arguments != expected {
            return Err(io::Error::other(
                "detached argv differs from exact detached installation invocation",
            )
            .into());
        }
        if std::fs::read_link(format!("/proc/{}/cwd", first.pid))? != installation.root() {
            return Err(io::Error::other("detached CWD differs from installation").into());
        }
        let request = policy.request_context(Instant::now(), &cancellation)?;
        let Outcome::Running(status) =
            daemon_launcher::execute(&host, inputs(executable), &request, &ServiceCommand::Status)
                .await?
        else {
            return Err(io::Error::other("status absent after start").into());
        };
        if status.pid != first.pid {
            return Err(io::Error::other("status PID differs from startup").into());
        }
        let request = policy.request_context(Instant::now(), &cancellation)?;
        let Outcome::Running(restarted) = daemon_launcher::execute(
            &host,
            inputs(executable),
            &request,
            &ServiceCommand::Restart,
        )
        .await?
        else {
            return Err(io::Error::other("restart did not authenticate readiness").into());
        };
        if restarted.pid == first.pid {
            return Err(io::Error::other("restart retained previous process").into());
        }
        let request = policy.request_context(Instant::now(), &cancellation)?;
        let occupied = FileLock::try_acquire(installation.root().join("daemon.pid")).unwrap_err();
        if occupied.kind() != io::ErrorKind::WouldBlock {
            return Err(occupied.into());
        }
        let denied = daemon_launcher::execute(
            &host,
            inputs(executable),
            &request,
            &ServiceCommand::Install { login: false },
        )
        .await
        .unwrap_err();
        if !matches!(denied, Failure::Io(ref e) if e.kind()==io::ErrorKind::Unsupported) {
            return Err(Box::new(denied) as Box<dyn std::error::Error>);
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    }
    .await;
    // Always perform an authenticated stop before propagating any journey failure.
    let request = policy.request_context(Instant::now(), &cancellation)?;
    let stopped =
        daemon_launcher::execute(&host, inputs(executable), &request, &ServiceCommand::Stop).await;
    journey?;
    assert!(matches!(stopped?, Outcome::Stopped));
    drop(FileLock::try_acquire(
        installation.root().join("daemon.pid"),
    )?);
    let request = policy.request_context(Instant::now(), &cancellation)?;
    assert!(matches!(
        daemon_launcher::execute(&host, inputs(executable), &request, &ServiceCommand::Status)
            .await?,
        Outcome::Absent
    ));
    let request = policy.request_context(Instant::now(), &cancellation)?;
    let failed = daemon_launcher::execute(
        &host,
        inputs(std::path::Path::new("/usr/bin/false")),
        &request,
        &ServiceCommand::Start,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(failed, Failure::Startup { cleanup: None, .. }),
        "{failed}"
    );
    drop(FileLock::try_acquire(
        installation.root().join("daemon.pid"),
    )?);
    match host.shutdown(Instant::now() + Duration::from_secs(5)).await {
        HostShutdown::Joined(report) => assert!(report.failure().is_none()),
        HostShutdown::Pending(_) => panic!("launcher host cleanup pending"),
    }
    Ok(())
}

#[tokio::test]
async fn non_unicode_runtime_directory_is_rejected_before_connecting() {
    use std::os::unix::ffi::OsStringExt;
    let environment = vec![(
        "XDG_RUNTIME_DIR".into(),
        OsString::from_vec(b"/tmp/runtime-\xff".to_vec()),
    )];
    let error = manager_availability(&environment, &original())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("valid Unicode"));
}
