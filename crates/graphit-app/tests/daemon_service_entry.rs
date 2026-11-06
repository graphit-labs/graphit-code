#![cfg(target_os = "linux")]

use crate::runtime_peer;

use graphit_app::service_entry::{ServiceCommand, ServiceResult, execute};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_daemon::{
    control::{ServiceFiles, UserService},
    services::{ServiceCommands, ServiceDefinition, ServiceFailure, ServicePlatform},
};
use graphit_process::{Cleanup, HostLimits, HostReadiness, HostShutdown, ProcessHost};
use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

struct PrivateDaemon {
    child: Child,
}

// The detached/manager process is owned by its private installation, rather
// than by the short-lived CLI child. Keep that installation if cleanup fails.
struct PrivateControl {
    root: Option<tempfile::TempDir>,
    global_dir: std::path::PathBuf,
    environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    registered: bool,
}

impl PrivateControl {
    fn command(&self) -> Command {
        self.command_using(std::path::Path::new(env!("CARGO_BIN_EXE_graphit")))
    }

    fn command_using(&self, executable: &std::path::Path) -> Command {
        let mut command = Command::new(executable);
        command
            .env_clear()
            .envs(self.environment.iter().cloned())
            .current_dir(self.root.as_ref().unwrap().path())
            .arg("--global-dir")
            .arg(&self.global_dir);
        command
    }
}

impl Drop for PrivateControl {
    fn drop(&mut self) {
        let stop = self.command().args(["daemon", "stop"]).output();
        let mut uninstalled = true;
        if self.registered {
            let uninstall = self.command().args(["daemon", "uninstall"]).output();
            if !matches!(uninstall, Ok(ref result) if result.status.success()) {
                uninstalled = false;
                eprintln!("private service uninstall failed: {uninstall:?}");
            }
        }
        let lock = fs::File::open(self.global_dir.join("daemon.pid"));
        let released = match lock {
            Ok(file) => file.try_lock().is_ok(),
            Err(error) => error.kind() == io::ErrorKind::NotFound,
        };
        if !released || !uninstalled {
            let kept = self.root.take().unwrap().keep();
            eprintln!(
                "private daemon cleanup failed; installation retained at {}: {stop:?}",
                kept.display()
            );
        }
    }
}

impl Drop for PrivateDaemon {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

async fn wait_child(child: &mut Child) -> io::Result<std::process::ExitStatus> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= until {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "private daemon did not exit",
            ));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn compiled_foreground_singleton_discovery_sigterm_and_stale_restart()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_app::foreground_source::ForegroundSource;
    use graphit_core::{
        brand::Brand,
        config::{ConfigKey, ConfigSnapshot, ConfigStore},
        file_lock::FileLock,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
    };
    use graphit_daemon::{project_bootstrap::ProjectBootstrapInput, source_discovery};
    use graphit_hub::{
        NativeBinding,
        policy::{HubResourcePolicy, ResourceSetting},
    };

    let temp = tempfile::tempdir()?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let paths = InstallationPaths::resolve(
        brand.clone(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let root = temp.path().join("project");
    fs::create_dir(&root)?;
    let project = ProjectPaths::at(&root, &paths)?;
    let cancellation = Cancellation::default();
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancellation)?
        .ensure(&ProjectMetadata::new("daemon-lifecycle-fixture")?)?;
    let config = ConfigSnapshot::new(&brand, [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let request = policy.request_context(Instant::now(), &cancellation)?;
    let owner = ForegroundSource::start(
        &paths,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        &request,
    )?;
    owner.shutdown().await?;
    ConfigStore::global(&paths)?.set(
        ConfigKey::new("agent")?,
        serde_json::json!("codex"),
        Duration::from_secs(5),
        &cancellation,
    )?;
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(project.root())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", temp.path())
            .env("GRAPHIT_MODULES__AST", "false")
            .env(
                paths.brand().environment_variable("GLOBAL_DIR").unwrap(),
                paths.root(),
            )
            .args(["daemon", "foreground"]);
        command
    };
    let number = |setting| usize::try_from(policy.limit(setting).value);
    let native = graphit_core::native_document_file::NativeDocumentFileLimits::new(
        number(ResourceSetting::ContentInputBytes)?,
        graphit_core::native_path::MAX_PATH_BYTES,
        number(ResourceSetting::SnapshotDepth)?,
        number(ResourceSetting::ContentScalarBytes)?,
    )?;
    let observe =
        || -> Result<Option<source_discovery::DiscoveryRecord>, Box<dyn std::error::Error>> {
            let cancellation = Cancellation::default();
            let request = policy.request_context(Instant::now(), &cancellation)?;
            Ok(source_discovery::read_discovery(
                &paths,
                native,
                request.snapshot(),
            )?)
        };
    let launch = |name: &str| -> io::Result<PrivateDaemon> {
        Ok(PrivateDaemon {
            child: command()
                .stdout(Stdio::null())
                .stderr(fs::File::create(temp.path().join(name))?)
                .spawn()?,
        })
    };
    let mut daemon = launch("first.log")?;
    let until = Instant::now() + Duration::from_secs(30);
    let first = loop {
        if let Some(status) = daemon.child.try_wait()? {
            return Err(io::Error::other(format!(
                "daemon {status}: {}",
                fs::read_to_string(temp.path().join("first.log"))?
            ))
            .into());
        }
        if let Some(record) = observe()? {
            break record;
        }
        if Instant::now() >= until {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "private discovery absent").into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(first.pid(), daemon.child.id());
    let lock_path = paths.root().join("daemon.pid");
    assert_eq!(
        FileLock::try_acquire(&lock_path).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let mut duplicate = launch("duplicate.log")?;
    assert!(!wait_child(&mut duplicate.child).await?.success());
    assert_eq!(observe()?.unwrap().pid(), first.pid());

    // A real killed publisher leaves its SDK-written discovery behind.
    daemon.child.kill()?;
    daemon.child.wait()?;
    assert_eq!(observe()?.unwrap().pid(), first.pid());
    drop(FileLock::try_acquire(&lock_path)?);
    let mut restarted = launch("restarted.log")?;
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = restarted.child.try_wait()? {
            return Err(io::Error::other(format!(
                "restart {status}: {}",
                fs::read_to_string(temp.path().join("restarted.log"))?
            ))
            .into());
        }
        if observe()?.is_some_and(|record| record.pid() == restarted.child.id()) {
            break;
        }
        if Instant::now() >= until {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "stale discovery was not replaced",
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let status = Command::new("kill")
        .args(["-TERM", &restarted.child.id().to_string()])
        .status()?;
    assert!(status.success());
    assert_eq!(wait_child(&mut restarted.child).await?.code(), Some(130));
    assert!(observe()?.is_none());
    assert!(lock_path.exists());
    drop(FileLock::try_acquire(&lock_path)?);
    Ok(())
}

async fn host() -> ProcessHost {
    let host =
        ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1).unwrap()).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    host
}

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}

fn private_service<'a>(host: &'a ProcessHost, root: &std::path::Path) -> UserService<'a> {
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    runtime_peer::install(
        &bin,
        Some(serde_json::json!({"fixture":"service-manager"})),
        &["systemctl"],
    )
    .unwrap();
    let mut environment = vec![("PATH".into(), bin.into_os_string())];
    environment.push(("XDG_RUNTIME_DIR".into(), root.as_os_str().to_owned()));
    environment.push((
        "DBUS_SESSION_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-bus", root.display()).into(),
    ));
    let definition = ServiceDefinition {
        name: format!("graphit-entry-{}", std::process::id()),
        executable: std::env::current_exe().unwrap(),
        global_dir: root.to_owned(),
        user: "graphit-fixture".into(),
        uid: std::fs::metadata(root).unwrap().uid(),
    };
    let commands = ServiceCommands::resolve(
        host,
        ServicePlatform::Systemd,
        definition.uid,
        root.to_owned(),
        environment,
    )
    .expect("private manager fixture resolves through its captured PATH");
    UserService {
        files: ServiceFiles {
            control_root: root.to_owned(),
            registration: root.join(format!("{}.service", definition.name)),
            login_registration: None,
        },
        definition,
        commands,
    }
}

#[tokio::test]
async fn manager_process_failure_retains_status_and_start_outcome() {
    let root = tempfile::tempdir().unwrap();
    let host = host().await;
    let service = private_service(&host, root.path());
    let request = original();
    let observation = execute(&service, &ServiceCommand::Status, &request)
        .await
        .unwrap();
    let ServiceResult::Status(outcome) = observation else {
        panic!("status must preserve the manager outcome");
    };
    assert!(!outcome.status.success());
    assert_eq!(outcome.status.code(), Some(69));
    assert_eq!(outcome.cleanup, Cleanup::Complete);
    assert!(outcome.remaining.is_none());
    assert!(!outcome.transcript.as_ref().unwrap().stderr().is_empty());
    let failure = execute(&service, &ServiceCommand::Start, &request).await;
    let Err(ServiceFailure::Manager(outcome)) = failure else {
        panic!("failed manager start must retain its causal outcome");
    };
    assert!(!outcome.status.success());
    assert_eq!(outcome.status.code(), Some(69));
    assert_eq!(outcome.cleanup, Cleanup::Complete);
    assert!(outcome.remaining.is_none());
    assert!(!service.files.registration.exists());
    drop(service);
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}

#[tokio::test]
async fn cancelled_install_has_no_effect_and_does_not_poison_next_original() {
    let root = tempfile::tempdir().unwrap();
    let host = host().await;
    let service = private_service(&host, root.path());
    let cancelled = original();
    cancelled.cancellation().cancel();
    assert!(
        execute(
            &service,
            &ServiceCommand::Install { login: true },
            &cancelled
        )
        .await
        .is_err()
    );
    assert!(!service.files.registration.exists());
    assert!(!root.path().join("service-install.lock").exists());
    let fresh = original();
    assert!(matches!(
        execute(&service, &ServiceCommand::Status, &fresh)
            .await
            .unwrap(),
        ServiceResult::Status(_)
    ));
    drop(service);
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}

#[tokio::test]
async fn compiled_managed_empty_inventory_outside_project_maintains_and_retires_real_checkout()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_app::foreground_source::ForegroundSource;
    use graphit_core::{
        brand::Brand,
        config::{ConfigKey, ConfigSnapshot, ConfigStore},
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
    };
    use graphit_hub::{NativeBinding, policy::HubResourcePolicy};

    let temp = tempfile::tempdir()?;
    let outside = temp.path().join("outside-project");
    fs::create_dir(&outside)?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let paths = InstallationPaths::resolve(
        brand.clone(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let cancellation = Cancellation::default();
    let config = ConfigSnapshot::new(&brand, [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let request = policy.request_context(Instant::now(), &cancellation)?;
    // Bootstrap only the real installation: no invented PROJECT or trusted root.
    let owner = ForegroundSource::start(&paths, &[], 0, &config, NativeBinding::Codex, &request)?;
    owner.shutdown().await?;
    ConfigStore::global(&paths)?.set(
        ConfigKey::new("agent")?,
        serde_json::json!("codex"),
        Duration::from_secs(5),
        &cancellation,
    )?;
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", temp.path())
            .env("GRAPHIT_MODULES__AST", "false")
            .arg("--global-dir")
            .arg(paths.root());
        command
    };
    let log = temp.path().join("managed.log");
    let mut daemon = PrivateDaemon {
        child: command()
            .current_dir(&outside)
            .args(["daemon", "--managed"])
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log)?)
            .spawn()?,
    };
    let until = Instant::now() + Duration::from_secs(30);
    while !paths.root().join("daemon/mcp.json").exists() {
        if let Some(status) = daemon.child.try_wait()? {
            return Err(io::Error::other(format!(
                "managed startup {status}: {}",
                fs::read_to_string(&log)?
            ))
            .into());
        }
        if Instant::now() >= until {
            return Err(
                io::Error::new(io::ErrorKind::TimedOut, "empty managed discovery absent").into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!outside.join(".graphit").exists());
    // Removing the launching CWD must not remove the global service.
    fs::remove_dir(&outside)?;
    assert!(daemon.child.try_wait()?.is_none());
    let root = temp.path().join("real-checkout");
    fs::create_dir(&root)?;
    let project = ProjectPaths::at(&root, &paths)?;
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancellation)?
        .ensure(&ProjectMetadata::new("managed-maintain-fixture")?)?;
    let identity = ProjectIdentityStore::at(&project)
        .load()?
        .ok_or("project identity absent")?;
    let id = serde_json::to_value(identity.id())?
        .as_str()
        .ok_or("project id is not a string")?
        .to_owned();
    let maintained = command()
        .current_dir(&root)
        .args(["projects", "maintain-checkout", "--project-id"])
        .arg(&id)
        .arg("--project-dir")
        .arg(&root)
        .arg("--select")
        .output()?;
    assert!(
        maintained.status.success(),
        "maintain failed: stdout={} stderr={} daemon={}",
        String::from_utf8_lossy(&maintained.stdout),
        String::from_utf8_lossy(&maintained.stderr),
        fs::read_to_string(&log)?
    );
    let maintained: serde_json::Value = serde_json::from_slice(&maintained.stdout)?;
    assert_eq!(maintained["inventory_registered"], true);
    assert_eq!(maintained["trusted_root"], true);
    let retired = command()
        .current_dir(&root)
        .args(["projects", "retire-checkout", "--project-id"])
        .arg(&id)
        .arg("--project-dir")
        .arg(&root)
        .output()?;
    assert!(
        retired.status.success(),
        "retire failed: stdout={} stderr={} daemon={}",
        String::from_utf8_lossy(&retired.stdout),
        String::from_utf8_lossy(&retired.stderr),
        fs::read_to_string(&log)?
    );
    let retired: serde_json::Value = serde_json::from_slice(&retired.stdout)?;
    assert_eq!(retired["checkout_removed"], true);
    let status = Command::new("kill")
        .args(["-TERM", &daemon.child.id().to_string()])
        .status()?;
    assert!(status.success());
    assert_eq!(wait_child(&mut daemon.child).await?.code(), Some(0));
    assert!(!paths.root().join("daemon/mcp.json").exists());
    Ok(())
}

#[tokio::test]
#[ignore = "requires the real systemd user manager; reads only one private nonexistent unit"]
async fn compiled_status_reports_authenticated_foreground_without_registered_unit()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_app::{daemon_control, daemon_launcher};
    use graphit_core::{
        brand::Brand, config::ConfigSnapshot, native_document_file::NativeDocumentFileLimits,
        project_paths::InstallationPaths,
    };
    use graphit_hub::policy::{HubResourcePolicy, ResourceSetting};

    let temp = tempfile::tempdir()?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let paths = InstallationPaths::resolve(
        brand.clone(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(format!(
                "/run/user/{}",
                fs::metadata(temp.path()).unwrap().uid()
            ))
        });
    let environment = vec![
        ("PATH".into(), std::env::var_os("PATH").unwrap_or_default()),
        ("HOME".into(), temp.path().as_os_str().to_owned()),
        ("XDG_RUNTIME_DIR".into(), runtime.as_os_str().to_owned()),
        ("XDG_CONFIG_HOME".into(), runtime.as_os_str().to_owned()),
        (
            "DBUS_SESSION_BUS_ADDRESS".into(),
            std::env::var_os("DBUS_SESSION_BUS_ADDRESS")
                .unwrap_or_else(|| format!("unix:path={}/bus", runtime.display()).into()),
        ),
    ];
    assert_eq!(
        daemon_launcher::manager_availability(&environment, &original()).await?,
        daemon_launcher::ManagerAvailability::Available
    );
    let config = ConfigSnapshot::new(&brand, [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let number = |setting| usize::try_from(policy.limit(setting).value);
    let limits = NativeDocumentFileLimits::new(
        number(ResourceSetting::ContentInputBytes)?,
        graphit_core::native_path::MAX_PATH_BYTES,
        number(ResourceSetting::SnapshotDepth)?,
        number(ResourceSetting::ContentScalarBytes)?,
    )?;
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_graphit"));

    let command = || {
        let mut command = Command::new(executable);
        command
            .env_clear()
            .envs(environment.iter().cloned())
            .current_dir(temp.path())
            .arg("--global-dir")
            .arg(paths.root());
        command
    };
    let mut daemon = PrivateDaemon {
        child: command()
            .args([
                "-c",
                "agent=\"codex\"",
                "-c",
                "modules.ast=false",
                "-c",
                "modules.task=false",
                "-c",
                "modules.knowledge=false",
                "-c",
                "modules.memory=false",
                "-c",
                "modules.embedding=false",
                "-c",
                "dream.enabled=false",
                "daemon",
                "--managed",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let inputs = daemon_launcher::Inputs {
        installation: &paths,
        executable,
        environment: &environment,
        client: &client,
        limits,
        termination_grace: Duration::from_secs(1),
        cleanup: Duration::from_secs(5),
    };
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            daemon.child.try_wait()?.is_none(),
            "private foreground exited before readiness"
        );
        let cancellation = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancellation)?;
        if let Some(reply) = daemon_launcher::control(
            &inputs,
            &request,
            daemon_control::Operation::Status,
            Some(daemon.child.id()),
        )
        .await?
        {
            assert_eq!(reply.state, daemon_control::State::Running);
            break;
        }
        if Instant::now() >= until {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let verifier = host().await;
    let service =
        daemon_launcher::compose_service(&verifier, &paths, executable, &environment, &original())?;
    assert!(!service.files.registration.exists());
    let native = service.status(&original()).await?;
    let native = String::from_utf8(native.transcript.as_ref().unwrap().stdout().to_vec())?;
    assert!(native.lines().any(|line| line == "LoadState=not-found"));
    assert!(native.lines().any(|line| line == "ActiveState=inactive"));
    assert!(native.lines().any(|line| line == "MainPID=0"));

    let output = command().args(["daemon", "status"]).output()?;
    assert!(
        output.status.success(),
        "compiled status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(status["pid"], daemon.child.id());
    assert_eq!(status["state"], "running");
    assert!(!service.files.registration.exists());
    let signal = Command::new("kill")
        .args(["-TERM", &daemon.child.id().to_string()])
        .status()?;
    assert!(signal.success());
    assert_eq!(wait_child(&mut daemon.child).await?.code(), Some(0));
    assert!(!paths.root().join("daemon/mcp.json").exists());
    assert!(!paths.root().join("daemon/mcp.key").exists());
    drop(service);
    assert!(matches!(
        verifier
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await,
        HostShutdown::Joined(_)
    ));
    Ok(())
}

#[tokio::test]
async fn compiled_direct_commands_without_user_manager() -> Result<(), Box<dyn std::error::Error>> {
    compiled_direct_control_journey(false, false).await
}

#[tokio::test]
#[ignore = "requires the real systemd user manager; installs only one uniquely named runtime unit"]
async fn compiled_direct_commands_with_user_manager() -> Result<(), Box<dyn std::error::Error>> {
    compiled_direct_control_journey(true, false).await
}

#[tokio::test]
#[ignore = "requires the real systemd user manager; replaces only a private uniquely named unit"]
async fn compiled_available_manager_rejection_and_private_binary_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    compiled_direct_control_journey(true, true).await
}

async fn compiled_direct_control_journey(
    available: bool,
    qualify_lifecycle: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use graphit_app::{daemon_launcher, foreground_source::ForegroundSource};
    use graphit_core::{
        brand::Brand,
        config::{ConfigKey, ConfigSnapshot, ConfigStore},
        project_paths::InstallationPaths,
    };
    use graphit_hub::{NativeBinding, policy::HubResourcePolicy};

    let temp = tempfile::tempdir()?;
    let outside = temp.path().join("outside-project");
    fs::create_dir(&outside)?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let paths = InstallationPaths::resolve(
        brand.clone(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let cancellation = Cancellation::default();
    let config = ConfigSnapshot::new(&brand, [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let request = policy.request_context(Instant::now(), &cancellation)?;
    let owner = ForegroundSource::start(&paths, &[], 0, &config, NativeBinding::Codex, &request)?;
    owner.shutdown().await?;
    let store = ConfigStore::global(&paths)?;
    for (key, value) in [
        ("agent", serde_json::json!("codex")),
        ("modules.ast", serde_json::json!(false)),
    ] {
        store.set(
            ConfigKey::new(key)?,
            value,
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    let runtime = if available {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(format!(
                    "/run/user/{}",
                    fs::metadata(temp.path()).unwrap().uid()
                ))
            })
    } else {
        let runtime = temp.path().join("runtime");
        fs::create_dir(&runtime)?;
        runtime
    };
    // The actual manager searches runtime/systemd/user. A random XDG_CONFIG_HOME
    // would create an invisible unit and cannot qualify the positive journey.
    let mut environment = vec![
        ("PATH".into(), std::env::var_os("PATH").unwrap_or_default()),
        ("HOME".into(), temp.path().as_os_str().to_owned()),
        ("XDG_RUNTIME_DIR".into(), runtime.as_os_str().to_owned()),
        ("XDG_CONFIG_HOME".into(), runtime.as_os_str().to_owned()),
        ("GRAPHIT_MODULES__AST".into(), "false".into()),
    ];
    let bus = if available {
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS")
            .unwrap_or_else(|| format!("unix:path={}/bus", runtime.display()).into())
    } else {
        format!("unix:path={}/absent-bus", runtime.display()).into()
    };
    environment.push(("DBUS_SESSION_BUS_ADDRESS".into(), bus));
    let mut control = PrivateControl {
        root: Some(temp),
        global_dir: paths.root().to_owned(),
        environment: environment.clone(),
        registered: available,
    };
    let host = host().await;
    let service = daemon_launcher::compose_service(
        &host,
        &paths,
        std::path::Path::new(env!("CARGO_BIN_EXE_graphit")),
        &environment,
        &original(),
    )?;
    let registration = service.files.registration.clone();
    let name = service.definition.name.clone();
    assert!(!registration.exists());
    if qualify_lifecycle {
        assert_eq!(
            daemon_launcher::manager_availability(&environment, &original()).await?,
            daemon_launcher::ManagerAvailability::Available
        );
        service.install(&original(), false).await?;
        // A real manager policy rejects manual start; do not infer availability
        // from the command's stderr or substitute a fake manager executable.
        let document = fs::read_to_string(&registration)?;
        assert!(document.contains("[Unit]\n"));
        fs::write(
            &registration,
            document.replacen("[Unit]\n", "[Unit]\nRefuseManualStart=yes\n", 1),
        )?;
        service
            .commands
            .execute(&original(), vec!["--user".into(), "daemon-reload".into()])
            .await?;
    }
    drop(service);
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
    let run = |action: &str| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let output = control.command().args(["daemon", action]).output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "daemon {action}: status={} stdout={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
        serde_json::from_slice(&output.stdout).map_err(|error| {
            io::Error::other(format!(
                "daemon {action} returned non-JSON: {error}; stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
            .into()
        })
    };
    if qualify_lifecycle {
        let refused = control.command().args(["daemon", "start"]).output()?;
        assert!(!refused.status.success());
        assert!(
            !refused.stderr.is_empty(),
            "failed CLI must retain its error"
        );
        let verifier = ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1)?)?;
        assert!(matches!(
            verifier
                .ready(Instant::now() + Duration::from_secs(5))
                .await,
            HostReadiness::Ready
        ));
        let service = daemon_launcher::compose_service(
            &verifier,
            &paths,
            std::path::Path::new(env!("CARGO_BIN_EXE_graphit")),
            &control.environment,
            &original(),
        )?;
        let Err(ServiceFailure::Manager(outcome)) = service.start(&original()).await else {
            panic!("available manager rejection must preserve its actual outcome");
        };
        assert!(!outcome.status.success());
        assert_eq!(outcome.cleanup, Cleanup::Complete);
        assert!(outcome.remaining.is_none());
        assert!(
            String::from_utf8_lossy(outcome.transcript.as_ref().unwrap().stderr()).contains(&name)
        );
        drop(service);
        assert!(matches!(
            verifier
                .shutdown(Instant::now() + Duration::from_secs(5))
                .await,
            HostShutdown::Joined(_)
        ));
        assert!(!paths.root().join("daemon/mcp.json").exists());
        let lock = fs::File::open(paths.root().join("daemon.pid"))?;
        lock.try_lock()?;
        lock.unlock()?;
        let repaired = control.command().args(["daemon", "install"]).output()?;
        assert!(
            repaired.status.success(),
            "{}",
            String::from_utf8_lossy(&repaired.stderr)
        );
        assert!(!fs::read_to_string(&registration)?.contains("RefuseManualStart"));
    }
    let started = run("start")?;
    assert_eq!(started["state"], "running");
    let mut pid = started["pid"]
        .as_u64()
        .ok_or("authenticated start PID absent")?;
    let status = run("status")?;
    assert_eq!(status["state"], "running");
    assert_eq!(status["pid"], pid);
    if available {
        // Native manager observation remains independent of authenticated CLI status.
        let verifier = self::host().await;
        let service = daemon_launcher::compose_service(
            &verifier,
            &paths,
            std::path::Path::new(env!("CARGO_BIN_EXE_graphit")),
            &control.environment,
            &original(),
        )?;
        let status = service.status(&original()).await?;
        assert!(status.status.success());
        let status = String::from_utf8(status.transcript.as_ref().unwrap().stdout().to_vec())?;
        let observed = status
            .lines()
            .find_map(|line| line.strip_prefix("MainPID="))
            .ok_or("native manager MainPID absent")?
            .parse::<u64>()?;
        assert_eq!(observed, pid);
        assert!(status.lines().any(|line| line == "ActiveState=active"));
        drop(service);
        assert!(matches!(
            verifier
                .shutdown(Instant::now() + Duration::from_secs(5))
                .await,
            HostShutdown::Joined(_)
        ));
    }
    if !available {
        assert_eq!(fs::read_link(format!("/proc/{pid}/cwd"))?, paths.root());
    }
    assert!(!outside.join(".graphit").exists());
    fs::remove_dir(&outside)?;
    assert_eq!(run("start")?["pid"], pid);
    assert_eq!(registration.exists(), available);
    if qualify_lifecycle {
        use graphit_core::native_document_file::NativeDocumentFileLimits;
        use graphit_hub::policy::ResourceSetting;
        use std::io::{Read, Write};
        let number = |setting| usize::try_from(policy.limit(setting).value);
        let limits = NativeDocumentFileLimits::new(
            number(ResourceSetting::ContentInputBytes)?,
            graphit_core::native_path::MAX_PATH_BYTES,
            number(ResourceSetting::SnapshotDepth)?,
            number(ResourceSetting::ContentScalarBytes)?,
        )?;
        let record = graphit_daemon::source_discovery::read_discovery(&paths, limits, &original())?
            .ok_or("running daemon discovery absent")?;
        let mut connection = std::net::TcpStream::connect(record.address())?;
        connection.set_read_timeout(Some(Duration::from_secs(3)))?;
        connection.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")?;
        let replacement = control
            .root
            .as_ref()
            .unwrap()
            .path()
            .join("replacement-graphit");
        fs::copy(env!("CARGO_BIN_EXE_graphit"), &replacement)?;
        let install = control
            .command_using(&replacement)
            .args(["daemon", "install"])
            .output()?;
        assert!(
            install.status.success(),
            "{}",
            String::from_utf8_lossy(&install.stderr)
        );
        let restarted = control
            .command_using(&replacement)
            .args(["daemon", "restart"])
            .output()?;
        assert!(
            restarted.status.success(),
            "{}",
            String::from_utf8_lossy(&restarted.stderr)
        );
        let restarted: serde_json::Value = serde_json::from_slice(&restarted.stdout)?;
        assert_eq!(restarted["state"], "running");
        // A successful restart command can hide systemd's SIGKILL after a
        // stop timeout. Read this private unit's actual exit observations.
        let journal = Command::new("journalctl")
            .env_clear()
            .envs(control.environment.iter().cloned())
            .args(["--user", "--no-pager", "--output=json"])
            .arg(format!("USER_UNIT={name}.service"))
            .arg("_COMM=systemd")
            .output()?;
        assert!(
            journal.status.success(),
            "private unit exit journal unavailable"
        );
        let exits = String::from_utf8(journal.stdout)?
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| entry.get("EXIT_CODE").is_some())
            .map(|entry| {
                serde_json::json!({
                    "code": entry["EXIT_CODE"], "status": entry["EXIT_STATUS"]
                })
            })
            .collect::<Vec<_>>();
        assert!(!exits.is_empty(), "previous owner exit was not observed");
        assert!(
            exits
                .iter()
                .all(|exit| exit["code"] == "exited" && exit["status"] == "0"),
            "replacement must join the previous owner, without timeout/kill: {exits:?}"
        );
        let successor = restarted["pid"].as_u64().ok_or("replacement PID absent")?;
        assert_ne!(successor, pid);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert_eq!(
            fs::read_link(format!("/proc/{successor}/exe"))?,
            replacement
        );
        let mut byte = [0];
        match connection.read(&mut byte) {
            Ok(0) => (),
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => (),
            result => {
                return Err(io::Error::other(format!(
                    "old listener connection survived replacement: {result:?}"
                ))
                .into());
            }
        }
        pid = successor;
    }
    let before = if available {
        Some((
            fs::read(&registration)?,
            fs::metadata(&registration)?.modified()?,
        ))
    } else {
        None
    };
    assert_eq!(run("stop")?["state"], "stopped");
    assert!(!paths.root().join("daemon/mcp.json").exists());
    let lock = fs::File::open(paths.root().join("daemon.pid"))?;
    lock.try_lock()?;
    lock.unlock()?;
    if let Some((document, modified)) = before {
        let login = control
            .command()
            .args(["daemon", "login", "enable"])
            .output()?;
        assert!(
            login.status.success(),
            "private login enable: {}",
            String::from_utf8_lossy(&login.stderr)
        );
        let enabled = || {
            Command::new("systemctl")
                .env_clear()
                .envs(control.environment.iter().cloned())
                .args(["--user", "is-enabled", &format!("{name}.service")])
                .output()
        };
        let previous = enabled()?;
        assert!(previous.status.success());
        let restarted = run("start")?;
        assert_eq!(restarted["state"], "running");
        assert_ne!(restarted["pid"], pid);
        assert_eq!(fs::read(&registration)?, document);
        assert_eq!(fs::metadata(&registration)?.modified()?, modified);
        assert_eq!(enabled()?.stdout, previous.stdout);
        assert_eq!(run("stop")?["state"], "stopped");
        let uninstall = control.command().args(["daemon", "uninstall"]).output()?;
        assert!(
            uninstall.status.success(),
            "private uninstall: {}",
            String::from_utf8_lossy(&uninstall.stderr)
        );
        assert!(!registration.exists());
        control.registered = false;
    } else {
        let absent = control.command().args(["daemon", "status"]).output()?;
        assert!(!absent.status.success());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&absent.stdout)?["state"],
            "absent"
        );
    }
    Ok(())
}
