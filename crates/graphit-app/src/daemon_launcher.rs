//! Per-invocation composition for the native daemon launcher.
//!
//! Native manager observation is independent of a manager command's outcome.

use graphit_core::{file_io::SnapshotContext, project_paths::InstallationPaths};
use graphit_daemon::{
    control::{ServiceFiles, UserService},
    services::{ServiceCommands, ServiceDefinition, ServicePlatform},
};
use graphit_process::ProcessHost;
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Compose a native service against one installation and one invocation's host.
/// The returned owner does not retain a request or infer a detached fallback.
pub fn compose_service<'host>(
    host: &'host ProcessHost,
    installation: &InstallationPaths,
    executable: &Path,
    environment: &[(OsString, OsString)],
    original: &Arc<SnapshotContext>,
) -> io::Result<UserService<'host>> {
    original.check()?;
    original.required_deadline()?;
    let name = service_name(installation);
    let (platform, uid, user) = native_user()?;
    let files = service_files(installation, environment, &name, platform)?;
    let definition = ServiceDefinition {
        name,
        executable: executable.to_owned(),
        global_dir: installation.root().to_owned(),
        user,
        uid,
    };
    definition.validate()?;
    // This directory is also the child CWD; a removed project never owns launch.
    std::fs::create_dir_all(installation.root())?;
    original.check()?;
    let commands = ServiceCommands::resolve(
        host,
        platform,
        uid,
        installation.root().to_owned(),
        environment.to_vec(),
    )?;
    original.check()?;
    Ok(UserService {
        definition,
        files,
        commands,
    })
}

fn service_name(installation: &InstallationPaths) -> String {
    let digest = Sha256::digest(installation.root().as_os_str().as_encoded_bytes());
    let mut name = format!("{}-daemon-", installation.brand().key());
    for byte in digest {
        use std::fmt::Write;
        write!(&mut name, "{byte:02x}").expect("writing a String cannot fail");
    }
    name
}

fn captured_path(environment: &[(OsString, OsString)], name: &str) -> io::Result<Option<PathBuf>> {
    let path = environment
        .iter()
        .find(|(key, _)| key == OsStr::new(name))
        .map(|(_, value)| PathBuf::from(value))
        .filter(|path| !path.as_os_str().is_empty());
    if path.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must be absolute"),
        ));
    }
    Ok(path)
}

fn captured_home(environment: &[(OsString, OsString)]) -> io::Result<PathBuf> {
    captured_path(environment, "HOME")?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "captured home directory is absent"))
}

fn service_files(
    installation: &InstallationPaths,
    environment: &[(OsString, OsString)],
    name: &str,
    platform: ServicePlatform,
) -> io::Result<ServiceFiles> {
    let private = installation.root().join("services");
    let (registration, login_registration) = match platform {
        ServicePlatform::Systemd => {
            let config = match captured_path(environment, "XDG_CONFIG_HOME")? {
                Some(config) => config,
                None => captured_home(environment)?.join(".config"),
            };
            (
                config.join("systemd/user").join(format!("{name}.service")),
                None,
            )
        }
        ServicePlatform::Launchd => (
            private.join(format!("{name}.plist")),
            Some(
                captured_home(environment)?
                    .join("Library/LaunchAgents")
                    .join(format!("{name}.plist")),
            ),
        ),
        ServicePlatform::TaskScheduler => (
            private.join(format!("{name}.xml")),
            Some(private.join(format!("{name}-login.xml"))),
        ),
    };
    Ok(ServiceFiles {
        control_root: installation.root().to_owned(),
        registration,
        login_registration,
    })
}

fn native_user() -> io::Result<(ServicePlatform, u32, String)> {
    #[cfg(target_os = "linux")]
    {
        Ok((
            ServicePlatform::Systemd,
            rustix::process::geteuid().as_raw(),
            whoami::username().map_err(io::Error::other)?,
        ))
    }
    #[cfg(target_os = "macos")]
    {
        Ok((
            ServicePlatform::Launchd,
            rustix::process::geteuid().as_raw(),
            whoami::username().map_err(io::Error::other)?,
        ))
    }
    #[cfg(windows)]
    {
        // POSIX uid is unused by Task Scheduler; the native account selects its user.
        Ok((
            ServicePlatform::TaskScheduler,
            0,
            whoami::account().map_err(io::Error::other)?,
        ))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "per-user daemon service is unsupported on this platform",
        ))
    }
}

/// Whether the current user has a native service manager to receive commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagerAvailability {
    Available,
    Unavailable,
}

/// Observe the Linux user manager on the bus selected by the captured environment.
/// Authentication/protocol errors and unsuccessful manager commands are not absence.
#[cfg(target_os = "linux")]
pub async fn manager_availability(
    environment: &[(std::ffi::OsString, std::ffi::OsString)],
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
) -> std::io::Result<ManagerAvailability> {
    use std::{ffi::OsStr, io, path::PathBuf};
    use zbus::address::{
        Address, Transport,
        transport::{Unix, UnixSocket},
    };

    original.check()?;
    let deadline = original.required_deadline()?;
    let value = |key: &str| {
        environment
            .iter()
            .find(|(name, _)| name == OsStr::new(key))
            .map(|(_, value)| value)
    };
    let address = if let Some(address) = value("DBUS_SESSION_BUS_ADDRESS") {
        let address = address.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "D-Bus address must be Unicode")
        })?;
        address.parse::<Address>().map_err(io::Error::other)?
    } else {
        let runtime = value("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(format!("/run/user/{}", rustix::process::geteuid().as_raw()))
            });
        if runtime.to_str().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "user runtime directory must be valid Unicode",
            ));
        }
        if !runtime.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "user runtime directory must be absolute",
            ));
        }
        Address::new(Transport::Unix(Unix::new(UnixSocket::File(
            runtime.join("bus"),
        ))))
    };
    let observe = async {
        let connection = match zbus::connection::Builder::address(address)
            .map_err(io::Error::other)?
            .build()
            .await
        {
            Ok(connection) => connection,
            Err(zbus::Error::InputOutput(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(ManagerAvailability::Unavailable);
            }
            Err(error) => return Err(io::Error::other(error)),
        };
        let bus = zbus::fdo::DBusProxy::new(&connection)
            .await
            .map_err(io::Error::other)?;
        let name = "org.freedesktop.systemd1"
            .try_into()
            .map_err(io::Error::other)?;
        let available = bus.name_has_owner(name).await.map_err(io::Error::other)?;
        Ok(if available {
            ManagerAvailability::Available
        } else {
            ManagerAvailability::Unavailable
        })
    };
    let result = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
        result = observe => result,
    };
    original.check()?;
    result
}

/// Captured inputs for one launcher invocation. The caller owns the host and client.
/// The client must disable proxies and redirects for the exact loopback authority.
pub struct Inputs<'a> {
    pub installation: &'a InstallationPaths,
    pub executable: &'a Path,
    pub environment: &'a [(OsString, OsString)],
    pub client: &'a reqwest::Client,
    pub limits: graphit_core::native_document_file::NativeDocumentFileLimits,
    pub termination_grace: std::time::Duration,
    pub cleanup: std::time::Duration,
}

#[derive(Debug)]
pub enum Outcome {
    Completed,
    NativeStatus(Box<graphit_process::Outcome>),
    Running(crate::daemon_control::Reply),
    Stopped,
    Absent,
}

/// Errors retain native process outcomes and incomplete startup cleanup owners.
#[derive(Debug)]
pub enum Failure {
    Io(io::Error),
    Manager(Box<graphit_daemon::services::ServiceFailure>),
    AfterStop {
        cause: io::Error,
        acknowledged: Option<crate::daemon_control::Reply>,
    },
    #[cfg(target_os = "linux")]
    Detached(graphit_process::DetachedFailure),
    #[cfg(target_os = "linux")]
    Startup {
        cause: io::Error,
        cleanup: Option<graphit_process::DetachedFailure>,
    },
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "daemon launcher: {error}"),
            Self::Manager(error) => write!(f, "daemon manager: {error}"),
            Self::AfterStop {
                cause,
                acknowledged,
            } => write!(
                f,
                "daemon stop completion failed (acknowledged={}): {cause}",
                acknowledged.is_some()
            ),
            #[cfg(target_os = "linux")]
            Self::Detached(error) => write!(f, "detached daemon: {}", error.cause),
            #[cfg(target_os = "linux")]
            Self::Startup { cause, cleanup } => write!(
                f,
                "daemon readiness failed: {cause}; cleanup complete={}",
                cleanup.is_none()
            ),
        }
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Io(error) => error,
            Self::Manager(error) => error,
            Self::AfterStop { cause, .. } => cause,
            #[cfg(target_os = "linux")]
            Self::Detached(error) => &error.cause,
            #[cfg(target_os = "linux")]
            Self::Startup { cause, .. } => cause,
        })
    }
}
impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<graphit_daemon::services::ServiceFailure> for Failure {
    fn from(error: graphit_daemon::services::ServiceFailure) -> Self {
        Self::Manager(Box::new(error))
    }
}

fn unavailable(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
        || error
            .get_ref()
            .and_then(|source| source.downcast_ref::<reqwest::Error>())
            .is_some_and(reqwest::Error::is_connect)
}

/// Issue an authenticated operation constrained to discovery and, when supplied, an owned child.
pub async fn control(
    inputs: &Inputs<'_>,
    original: &graphit_hub::policy::RequestContext<'_>,
    operation: crate::daemon_control::Operation,
    expected_pid: Option<u32>,
) -> io::Result<Option<crate::daemon_control::Reply>> {
    use graphit_daemon::source_discovery::read_discovery;
    original.check()?;
    let record = read_discovery(inputs.installation, inputs.limits, original.snapshot())
        .map_err(io::Error::other)?;
    let Some(record) = record else {
        return Ok(None);
    };
    if expected_pid.is_some_and(|pid| pid != record.pid()) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "another daemon owns discovery",
        ));
    }
    let target = std::num::NonZeroU32::new(record.pid())
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
    let reply = match crate::daemon_control::request(
        inputs.installation,
        record.address(),
        inputs.client,
        inputs.limits,
        original,
        operation,
        Some(target),
    )
    .await
    {
        Ok(reply) => reply,
        Err(error) if unavailable(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    if reply.pid != record.pid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "authenticated daemon PID differs from discovery",
        ));
    }
    Ok(Some(reply))
}

/// Wait for release of the installation singleton; this does not join a child.
pub async fn wait_stopped(
    inputs: &Inputs<'_>,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> io::Result<()> {
    let path = inputs.installation.global_path(&["daemon.pid"])?;
    let snapshot = Arc::clone(original.snapshot());
    tokio::task::spawn_blocking(move || {
        snapshot.check()?;
        let lock = graphit_core::file_lock::FileLock::acquire_in_existing_parent_until(
            &path,
            Some(snapshot.required_deadline()?),
            snapshot.cancellation(),
        )?;
        snapshot.check()?;
        drop(lock);
        Ok(())
    })
    .await
    .map_err(io::Error::other)?
}

async fn stop(
    inputs: &Inputs<'_>,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> Result<Outcome, Failure> {
    let acknowledged = control(
        inputs,
        original,
        crate::daemon_control::Operation::Stop,
        None,
    )
    .await
    .map_err(|cause| Failure::AfterStop {
        cause,
        acknowledged: None,
    })?;
    wait_stopped(inputs, original)
        .await
        .map_err(|cause| Failure::AfterStop {
            cause,
            acknowledged,
        })?;
    Ok(Outcome::Stopped)
}

/// Authenticate readiness for the expected native child under the supplied Original.
pub async fn readiness(
    inputs: &Inputs<'_>,
    original: &graphit_hub::policy::RequestContext<'_>,
    expected_pid: Option<u32>,
    #[cfg(target_os = "linux")] mut startup: Option<&mut graphit_process::DetachedStartup>,
) -> io::Result<crate::daemon_control::Reply> {
    loop {
        original.check()?;
        #[cfg(target_os = "linux")]
        if let Some(startup) = startup.as_deref_mut()
            && let Some(status) = startup.try_wait()?
        {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("daemon exited before readiness: {status}"),
            ));
        }
        match control(
            inputs,
            original,
            crate::daemon_control::Operation::Status,
            expected_pid,
        )
        .await
        {
            Ok(Some(reply)) => {
                if expected_pid.is_some_and(|pid| pid != reply.pid) {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "another daemon owns authenticated readiness",
                    ));
                }
                if reply.state == crate::daemon_control::State::Running {
                    return Ok(reply);
                }
            }
            Ok(None) => (),
            Err(error) => {
                original.check()?;
                if !matches!(
                    error
                        .get_ref()
                        .and_then(|cause| cause
                            .downcast_ref::<graphit_daemon::source_discovery::DiscoveryError>()),
                    Some(graphit_daemon::source_discovery::DiscoveryError::ObservationChanged(_))
                ) {
                    return Err(error);
                }
                // Discard this invalid observation, then use the existing bounded wait.
            }
        }
        let deadline = original.snapshot().required_deadline()?;
        tokio::select! {
            biased;
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
            () = tokio::time::sleep(std::time::Duration::from_millis(25)) => (),
        }
    }
}

/// Start the explicit Linux detached mode and commit custody after authenticated readiness.
#[cfg(target_os = "linux")]
pub async fn start_detached(
    inputs: &Inputs<'_>,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> Result<Outcome, Failure> {
    if !inputs.executable.is_absolute()
        || inputs.cleanup.is_zero()
        || std::time::Instant::now()
            .checked_add(inputs.cleanup)
            .is_none()
    {
        return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
    }
    original.check()?;
    std::fs::create_dir_all(inputs.installation.root())?;
    // Serialize launchers, not the daemon's lifetime lock. A competing launcher
    // rechecks authenticated readiness after this owner commits or joins startup.
    let path = inputs.installation.global_path(&["daemon-start.lock"])?;
    let snapshot = Arc::clone(original.snapshot());
    let _startup_lock = tokio::task::spawn_blocking(move || {
        snapshot.check()?;
        let lock = graphit_core::file_lock::FileLock::acquire_in_existing_parent_until(
            &path,
            Some(snapshot.required_deadline()?),
            snapshot.cancellation(),
        )?;
        snapshot.check()?;
        Ok::<_, io::Error>(lock)
    })
    .await
    .map_err(io::Error::other)??;
    original.check()?;
    if let Some(reply) = control(
        inputs,
        original,
        crate::daemon_control::Operation::Status,
        None,
    )
    .await?
    {
        if reply.state == crate::daemon_control::State::Running {
            return Ok(Outcome::Running(reply));
        }
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "daemon is already stopping").into());
    }
    original.check()?;
    let request = graphit_process::DetachedRequest {
        program: inputs.executable.to_owned(),
        args: vec![
            "--global-dir".into(),
            inputs.installation.root().as_os_str().to_owned(),
            "daemon".into(),
            "--detached".into(),
        ],
        cwd: inputs.installation.root().to_owned(),
        environment: inputs.environment.to_vec(),
        termination_grace: inputs.termination_grace,
        cleanup: inputs.cleanup,
    };
    let mut startup = graphit_process::spawn_detached(request, original.snapshot())
        .await
        .map_err(Failure::Detached)?;
    let reply = readiness(inputs, original, Some(startup.pid()), Some(&mut startup)).await;
    match reply {
        Ok(reply) => {
            startup.commit().map_err(Failure::Detached)?;
            Ok(Outcome::Running(reply))
        }
        Err(cause) => {
            let deadline = startup.cleanup_deadline()?;
            let cleanup = startup.cancel_join(deadline).await.err();
            Err(Failure::Startup { cause, cleanup })
        }
    }
}

/// Execute using the observed native manager, or the Linux detached capability.
/// An unsuccessful native command never selects another execution path.
pub async fn execute(
    host: &ProcessHost,
    inputs: Inputs<'_>,
    original: &graphit_hub::policy::RequestContext<'_>,
    command: &crate::service_entry::ServiceCommand,
) -> Result<Outcome, Failure> {
    use crate::service_entry::{ServiceCommand, ServiceResult};
    original.check()?;
    if matches!(command, ServiceCommand::Start | ServiceCommand::Status)
        && let Some(reply) = control(
            &inputs,
            original,
            crate::daemon_control::Operation::Status,
            None,
        )
        .await?
    {
        if matches!(command, ServiceCommand::Status)
            || reply.state == crate::daemon_control::State::Running
        {
            return Ok(Outcome::Running(reply));
        }
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "daemon is already stopping").into());
    }
    #[cfg(target_os = "linux")]
    let manager = manager_availability(inputs.environment, original.snapshot()).await?;
    #[cfg(not(target_os = "linux"))]
    let manager = ManagerAvailability::Available;
    if manager == ManagerAvailability::Available {
        let service = compose_service(
            host,
            inputs.installation,
            inputs.executable,
            inputs.environment,
            original.snapshot(),
        )?;
        match command {
            ServiceCommand::Start => {
                if !service.files.registration.try_exists()? {
                    service.install(original.snapshot(), false).await?;
                }
                service.start(original.snapshot()).await?;
                return readiness(
                    &inputs,
                    original,
                    None,
                    #[cfg(target_os = "linux")]
                    None,
                )
                .await
                .map(Outcome::Running)
                .map_err(Failure::Io);
            }
            ServiceCommand::Stop | ServiceCommand::Restart => {
                service.stop(original.snapshot()).await?;
                stop(&inputs, original).await?;
                if matches!(command, ServiceCommand::Stop) {
                    return Ok(Outcome::Stopped);
                }
                service.start(original.snapshot()).await?;
                return readiness(
                    &inputs,
                    original,
                    None,
                    #[cfg(target_os = "linux")]
                    None,
                )
                .await
                .map(Outcome::Running)
                .map_err(Failure::Io);
            }
            _ => {
                return match crate::service_entry::execute(&service, command, original.snapshot())
                    .await?
                {
                    ServiceResult::Completed => {
                        if matches!(command, ServiceCommand::Uninstall) {
                            wait_stopped(&inputs, original).await.map_err(|cause| {
                                Failure::AfterStop {
                                    cause,
                                    acknowledged: None,
                                }
                            })?;
                        }
                        Ok(Outcome::Completed)
                    }
                    ServiceResult::Status(status) => Ok(Outcome::NativeStatus(Box::new(status))),
                };
            }
        }
    }
    match command {
        #[cfg(target_os = "linux")]
        ServiceCommand::Start => start_detached(&inputs, original).await,
        ServiceCommand::Stop => stop(&inputs, original).await,
        #[cfg(target_os = "linux")]
        ServiceCommand::Restart => {
            stop(&inputs, original).await?;
            start_detached(&inputs, original).await
        }
        ServiceCommand::Status => Ok(Outcome::Absent),
        ServiceCommand::Uninstall => {
            let service = compose_service(
                host,
                inputs.installation,
                inputs.executable,
                inputs.environment,
                original.snapshot(),
            )?;
            service.uninstall(original.snapshot()).await?;
            wait_stopped(&inputs, original).await?;
            Ok(Outcome::Completed)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native service manager is unavailable for registration",
        )
        .into()),
    }
}
