//! Install a validated release using the real installation and daemon producers.
//! Package files are independently replaced atomically. Partial failures are reported without rollback.
use crate::{
    daemon_control::{Operation, State},
    daemon_launcher::{self, Inputs as LauncherInputs},
    self_update_release::{self, Preparation, PreparedRelease, Target},
    service_entry::ServiceCommand,
};
use graphit_core::{
    file_io::SnapshotContext, file_lock::FileLock, local_files::atomic_replace_with,
};
use graphit_hub::policy::RequestContext;
use graphit_process::ProcessHost;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Inputs<'a> {
    pub launcher: LauncherInputs<'a>,
    pub current_version: &'a str,
    pub limits: self_update_release::Limits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonEffect {
    Untouched,
    Inactive,
    StopRequested,
    Stopped,
    RestartRequested,
    Restarted { pid: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageState {
    Unchanged,
    Installed,
    Partial,
}

#[derive(Debug)]
pub enum Outcome {
    Development,
    Current,
    Installed { tag: String, daemon: DaemonEffect },
}

#[derive(Debug)]
pub enum Cause {
    Io {
        operation: &'static str,
        error: io::Error,
    },
    Prepare(self_update_release::PrepareError),
    Launcher(Box<daemon_launcher::Failure>),
    State(&'static str),
}

impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { operation, error } => write!(f, "{operation}: {error}"),
            Self::Prepare(error) => std::fmt::Display::fmt(error, f),
            Self::Launcher(error) => std::fmt::Display::fmt(error, f),
            Self::State(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for Cause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            Self::Prepare(error) => Some(error),
            Self::Launcher(error) => Some(error.as_ref()),
            Self::State(_) => None,
        }
    }
}

#[derive(Debug)]
pub struct Failure {
    pub cause: Cause,
    pub package: PackageState,
    pub daemon: DaemonEffect,
}
impl Failure {
    fn before(cause: Cause) -> Self {
        Self {
            cause,
            package: PackageState::Unchanged,
            daemon: DaemonEffect::Untouched,
        }
    }
    fn io(operation: &'static str, error: io::Error) -> Self {
        Self::before(Cause::Io { operation, error })
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "self-update: {}; package={:?}; daemon={:?}",
            self.cause, self.package, self.daemon
        )?;
        Ok(())
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

pub fn native_target() -> io::Result<Target> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(Target::LinuxAmd64),
        ("macos", "aarch64") => Ok(Target::DarwinArm64),
        ("windows", "x86_64") => Ok(Target::WindowsAmd64),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no maintained release target",
        )),
    }
}

fn launcher_inputs<'a>(inputs: &'a LauncherInputs<'a>) -> LauncherInputs<'a> {
    LauncherInputs {
        installation: inputs.installation,
        executable: inputs.executable,
        environment: inputs.environment,
        client: inputs.client,
        termination_grace: inputs.termination_grace,
        cleanup: inputs.cleanup,
    }
}

async fn acquire_lock(path: PathBuf, original: Arc<SnapshotContext>) -> io::Result<FileLock> {
    tokio::task::spawn_blocking(move || {
        original.check()?;
        let lock = FileLock::acquire_in_existing_parent_until(
            &path,
            Some(original.required_deadline()?),
            original.cancellation(),
        )?;
        original.check()?;
        Ok(lock)
    })
    .await
    .map_err(io::Error::other)?
}

/// Serialize installations in the same prefix, regardless of the global directory in use.
async fn lock_target(parent: &Path, original: Arc<SnapshotContext>) -> io::Result<FileLock> {
    acquire_lock(parent.join(".graphit-update.lock"), original).await
}

fn names(target: Target) -> [&'static str; 3] {
    let [main, model] = target.binaries();
    [main, model, "graphit-runtime"]
}

/// Only incoming, validated package files are replaced. Other products and user files are untouched.
/// Runtime files precede executables so the new executable finds its complete runtime.
fn package_files(
    prepared: &PreparedRelease,
    original: &SnapshotContext,
) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let [main, model, runtime] = names(prepared.target());
    let mut pending = vec![PathBuf::from(runtime)];
    while let Some(relative) = pending.pop() {
        original.check()?;
        if relative.starts_with("graphit-runtime/broker")
            || relative.starts_with("graphit-runtime/licenses/broker")
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Code package claims the Broker namespace",
            ));
        }
        let path = prepared.directory().join(&relative);
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(relative.join(entry?.file_name()));
            }
        } else if metadata.is_file() {
            files.push(relative);
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid package file",
            ));
        }
    }
    files.sort();
    files.extend(prepared.root_assets().iter().map(PathBuf::from));
    files.push(model.into());
    files.push(main.into());
    Ok(files)
}

fn replace_file(source: &Path, destination: &Path, original: &SnapshotContext) -> io::Result<()> {
    original.check()?;
    fs::create_dir_all(destination.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    let mut source = File::open(source)?;
    let permissions = source.metadata()?.permissions();
    atomic_replace_with(
        destination,
        Some(permissions),
        original.cancellation(),
        |output| {
            let mut buffer = [0; 64 * 1024];
            loop {
                original.check()?;
                let count = source.read(&mut buffer)?;
                if count == 0 {
                    return Ok(());
                }
                output.write_all(&buffer[..count])?;
            }
        },
    )
}

fn publish_package(
    parent: &Path,
    prepared: &PreparedRelease,
    files: &[PathBuf],
    original: &SnapshotContext,
) -> Result<(), Failure> {
    let mut applied = false;
    for relative in files {
        if let Err(error) = replace_file(
            &prepared.directory().join(relative),
            &parent.join(relative),
            original,
        ) {
            let mut failure = Failure::io(
                "replace package file",
                io::Error::new(error.kind(), format!("{}: {error}", relative.display())),
            );
            failure.package = if applied {
                PackageState::Partial
            } else {
                PackageState::Unchanged
            };
            return Err(failure);
        }
        applied = true;
    }
    Ok(())
}

async fn restart(
    host: &ProcessHost,
    inputs: &LauncherInputs<'_>,
    original: &RequestContext<'_>,
) -> Result<DaemonEffect, daemon_launcher::Failure> {
    match daemon_launcher::execute(
        host,
        launcher_inputs(inputs),
        original,
        &ServiceCommand::Restart,
    )
    .await?
    {
        daemon_launcher::Outcome::Running(reply) if matches!(reply.state, State::Running) => {
            Ok(DaemonEffect::Restarted { pid: reply.pid })
        }
        _ => Err(daemon_launcher::Failure::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "daemon restart returned without observed running readiness",
        ))),
    }
}

/// The caller retains this invocation and its ProcessHost through completion/cleanup.
/// This function never creates installation identity, credentials or service registrations.
pub async fn execute(
    host: &ProcessHost,
    inputs: Inputs<'_>,
    original: &RequestContext<'_>,
) -> Result<Outcome, Failure> {
    let snapshot = Arc::clone(original.snapshot());
    let target = native_target().map_err(|error| Failure::io("release target", error))?;
    let parent_path = inputs
        .launcher
        .executable
        .parent()
        .ok_or_else(|| Failure::before(Cause::State("current executable has no parent")))?;
    let parent =
        fs::canonicalize(parent_path).map_err(|error| Failure::io("installed directory", error))?;
    let preparation = self_update_release::prepare(
        Arc::clone(&snapshot),
        inputs.current_version,
        target,
        &parent,
        inputs.limits,
    )
    .await
    .map_err(|error| Failure::before(Cause::Prepare(error)))?;
    let prepared = match preparation {
        Preparation::Development => return Ok(Outcome::Development),
        Preparation::Current => return Ok(Outcome::Current),
        Preparation::Ready(prepared) => prepared,
    };
    install_prepared(host, &inputs.launcher, original, parent, prepared).await
}

async fn install_prepared(
    host: &ProcessHost,
    inputs: &LauncherInputs<'_>,
    original: &RequestContext<'_>,
    parent: PathBuf,
    prepared: PreparedRelease,
) -> Result<Outcome, Failure> {
    let snapshot = Arc::clone(original.snapshot());
    // Enumerate the incoming release before stopping anything or taking a publication lock.
    let files = package_files(&prepared, &snapshot)
        .map_err(|error| Failure::io("prepared package layout", error))?;
    let _target_lock = lock_target(&parent, Arc::clone(&snapshot))
        .await
        .map_err(|error| Failure::io("lock installed package", error))?;
    let singleton_path = inputs
        .installation
        .global_path(&["daemon.pid"])
        .map_err(|error| Failure::io("daemon singleton path", error))?;
    let before = daemon_launcher::control(inputs, original, Operation::Status, None)
        .await
        .map_err(|error| Failure::io("daemon status", error))?;
    let running = match before {
        Some(reply) if matches!(reply.state, State::Running) => true,
        Some(_) => return Err(Failure::before(Cause::State("daemon is stopping"))),
        None => false,
    };
    let mut daemon = DaemonEffect::Untouched;
    if running {
        daemon = DaemonEffect::StopRequested;
        match daemon_launcher::execute(
            host,
            launcher_inputs(inputs),
            original,
            &ServiceCommand::Stop,
        )
        .await
        {
            Ok(daemon_launcher::Outcome::Stopped) => daemon = DaemonEffect::Stopped,
            Ok(_) => {
                let mut failure =
                    Failure::before(Cause::State("daemon stop returned an unexpected outcome"));
                failure.daemon = daemon;
                return Err(failure);
            }
            Err(error) => {
                let mut failure = Failure::before(Cause::Launcher(Box::new(error)));
                failure.daemon = daemon;
                return Err(failure);
            }
        }
    }
    let singleton = match acquire_lock(singleton_path, Arc::clone(&snapshot)).await {
        Ok(lock) => lock,
        Err(error) => {
            let mut failure = Failure::io("daemon singleton", error);
            failure.daemon = daemon;
            return Err(failure);
        }
    };
    if !running {
        daemon = DaemonEffect::Inactive;
    }
    if let Err(mut failure) = publish_package(&parent, &prepared, &files, &snapshot) {
        failure.daemon = daemon;
        return Err(failure);
    }
    drop(singleton);
    if running {
        daemon = DaemonEffect::RestartRequested;
        match restart(host, inputs, original).await {
            Ok(effect) => daemon = effect,
            Err(error) => {
                return Err(Failure {
                    cause: Cause::Launcher(Box::new(error)),
                    package: PackageState::Installed,
                    daemon,
                });
            }
        }
    }
    if let Err(error) = original.check() {
        let mut failure = Failure::io("observe completed update", error);
        failure.package = PackageState::Installed;
        failure.daemon = daemon;
        return Err(failure);
    }
    Ok(Outcome::Installed {
        tag: prepared.tag().to_owned(),
        daemon,
    })
}

#[cfg(test)]
mod tests;
