//! Install a validated release using the real installation and daemon producers.
//! Coordination is cooperative; the filesystem producer promises no hostile-mutation CAS.
use crate::{
    daemon_control::{Operation, State},
    daemon_launcher::{self, Inputs as LauncherInputs},
    self_update_release::{self, Preparation, PreparedRelease, Target},
    service_entry::ServiceCommand,
};
use graphit_core::{
    file_io::SnapshotContext,
    file_lock::FileLock,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
};
use graphit_hub::policy::RequestContext;
use graphit_process::ProcessHost;
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;

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
    Restored,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryKind {
    RestorePackage,
    CleanupBackup,
}

#[derive(Debug)]
pub struct Recovery {
    pub kind: RecoveryKind,
    pub directory: PathBuf,
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
    pub recovery: Option<Recovery>,
    pub rollback_error: Option<io::Error>,
    pub cleanup_error: Option<io::Error>,
    pub restart_error: Option<Box<daemon_launcher::Failure>>,
}
impl Failure {
    fn before(cause: Cause) -> Self {
        Self {
            cause,
            package: PackageState::Unchanged,
            daemon: DaemonEffect::Untouched,
            recovery: None,
            rollback_error: None,
            cleanup_error: None,
            restart_error: None,
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
        if let Some(recovery) = &self.recovery {
            write!(
                f,
                "; {:?} at {}",
                recovery.kind,
                recovery.directory.display()
            )?;
        }
        if let Some(error) = &self.rollback_error {
            write!(f, "; rollback: {error}")?;
        }
        if let Some(error) = &self.cleanup_error {
            write!(f, "; backup cleanup: {error}")?;
        }
        if let Some(error) = &self.restart_error {
            write!(f, "; restart: {error}")?;
        }
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
        limits: inputs.limits,
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

struct TargetLock {
    _lock: FileLock,
    _private: File,
    control: CapturedDocumentDirectory,
    parent: CapturedDocumentDirectory,
}
impl TargetLock {
    async fn acquire(
        parent: CapturedDocumentDirectory,
        limits: NativeDocumentFileLimits,
        original: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        parent.revalidate(&original)?;
        let control_path = parent.canonical_path().join(".graphit-update");
        match graphit_fs::create_private_directory(&control_path) {
            Ok(file) => graphit_fs::verify_private_directory(&file)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(&control_path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let control = CapturedDocumentDirectory::capture(&control_path, limits, &original)?;
        let private = control.private_handle(&original)?;
        let lock_path = control.canonical_path().join("update.lock");
        match fs::symlink_metadata(&lock_path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let lock = acquire_lock(lock_path, Arc::clone(&original)).await?;
        parent.revalidate(&original)?;
        control.revalidate(&original)?;
        Ok(Self {
            _lock: lock,
            _private: private,
            control,
            parent,
        })
    }
    fn revalidate(&self, original: &SnapshotContext) -> io::Result<()> {
        self.parent.revalidate(original)?;
        self.control.revalidate(original)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MoveState {
    Original,
    BackedUp,
    Installed,
    Added,
    Removed,
}

struct Backup {
    container: Option<TempDir>,
    directory: PathBuf,
    capture: Option<CapturedDocumentDirectory>,
    private: Option<File>,
    names: Vec<String>,
    incoming_entries: usize,
    moves: Vec<MoveState>,
}
impl Backup {
    fn create(
        target: &TargetLock,
        limits: NativeDocumentFileLimits,
        original: &SnapshotContext,
        prepared: &PreparedRelease,
    ) -> io::Result<Self> {
        target.revalidate(original)?;
        let mut names: Vec<String> = names(prepared.target())
            .into_iter()
            .map(String::from)
            .collect();
        for name in prepared.root_assets() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let incoming_entries = names.len();
        for name in &names[3..] {
            match fs::symlink_metadata(target.parent.canonical_path().join(name)) {
                Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "installed package asset has an invalid type",
                    ));
                }
                Ok(_) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        let container = tempfile::Builder::new()
            .prefix("backup-")
            .tempdir_in(target.control.canonical_path())?;
        let directory = container.path().join("package");
        let private = graphit_fs::create_private_directory(&directory)?;
        let capture = CapturedDocumentDirectory::capture(&directory, limits, original)?;
        capture.private_handle(original)?;
        Ok(Self {
            container: Some(container),
            directory,
            capture: Some(capture),
            private: Some(private),
            moves: vec![MoveState::Original; names.len()],
            names,
            incoming_entries,
        })
    }
    fn check(
        &self,
        target: &TargetLock,
        stage: &CapturedDocumentDirectory,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        target.revalidate(original)?;
        stage.revalidate(original)?;
        self.capture
            .as_ref()
            .expect("backup capture owned")
            .revalidate(original)
    }
    fn apply(
        &mut self,
        target: &TargetLock,
        stage: &CapturedDocumentDirectory,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        for (index, name) in self.names.iter().enumerate() {
            self.check(target, stage, original)?;
            let had_original = match graphit_fs::rename_exclusive(
                &target.parent.canonical_path().join(name),
                &self.directory.join(name),
            ) {
                Ok(()) => {
                    self.moves[index] = MoveState::BackedUp;
                    true
                }
                Err(error) if index >= 3 && error.kind() == io::ErrorKind::NotFound => false,
                Err(error) => return Err(error),
            };
            self.check(target, stage, original)?;
            if name == "graphit-runtime" {
                self.preserve_broker_runtime(stage, original)?;
            }
            match graphit_fs::rename_exclusive(
                &stage.canonical_path().join(name),
                &target.parent.canonical_path().join(name),
            ) {
                Ok(()) => {
                    self.moves[index] = if had_original {
                        MoveState::Installed
                    } else {
                        MoveState::Added
                    }
                }
                Err(error)
                    if index >= self.incoming_entries
                        && had_original
                        && error.kind() == io::ErrorKind::NotFound =>
                {
                    self.moves[index] = MoveState::Removed;
                }
                Err(error) => return Err(error),
            }
        }
        original.check()
    }
    fn preserve_broker_runtime(
        &self,
        stage: &CapturedDocumentDirectory,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        // The updater already requires one filesystem for atomic rename.
        // Hardlinks keep the displaced package complete for rollback, without
        // copying Broker's libraries or treating its namespace as Code assets.
        for prefix in ["broker", "licenses/broker"] {
            let relative = Path::new("graphit-runtime").join(prefix);
            let source = self.directory.join(&relative);
            match fs::symlink_metadata(&source) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => (),
                Ok(_) => return Err(io::Error::other("invalid Broker runtime directory")),
            }
            let mut pending = vec![(source, stage.canonical_path().join(relative))];
            let mut permissions = Vec::new();
            while let Some((source, destination)) = pending.pop() {
                original.check()?;
                fs::create_dir(&destination)?; // A Code release cannot own this namespace.
                permissions.push((destination.clone(), fs::metadata(&source)?.permissions()));
                for entry in fs::read_dir(source)? {
                    original.check()?;
                    let entry = entry?;
                    let kind = entry.file_type()?;
                    let destination = destination.join(entry.file_name());
                    if kind.is_dir() {
                        pending.push((entry.path(), destination));
                    } else if kind.is_file() {
                        fs::hard_link(entry.path(), destination)?;
                    } else {
                        return Err(io::Error::other("invalid Broker runtime entry"));
                    }
                }
            }
            for (directory, mode) in permissions.into_iter().rev() {
                original.check()?;
                fs::set_permissions(directory, mode)?;
            }
        }
        Ok(())
    }
    fn restore(
        &mut self,
        target: &TargetLock,
        stage: &CapturedDocumentDirectory,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        for (index, name) in self.names.iter().enumerate().rev() {
            if self.moves[index] == MoveState::Original {
                continue;
            }
            self.check(target, stage, original)?;
            if matches!(self.moves[index], MoveState::Installed | MoveState::Added) {
                let added = self.moves[index] == MoveState::Added;
                graphit_fs::rename_exclusive(
                    &target.parent.canonical_path().join(name),
                    &stage.canonical_path().join(name),
                )?;
                self.moves[index] = if added {
                    MoveState::Original
                } else {
                    MoveState::BackedUp
                };
                if added {
                    continue;
                }
            }
            self.check(target, stage, original)?;
            graphit_fs::rename_exclusive(
                &self.directory.join(name),
                &target.parent.canonical_path().join(name),
            )?;
            self.moves[index] = MoveState::Original;
        }
        Ok(())
    }
    fn changed(&self) -> bool {
        self.moves.iter().any(|state| *state != MoveState::Original)
    }
    fn retain(&mut self, kind: RecoveryKind) -> Recovery {
        drop(self.private.take());
        drop(self.capture.take());
        let container = self
            .container
            .take()
            .expect("backup container owned")
            .keep();
        Recovery {
            kind,
            directory: container,
        }
    }
    async fn cleanup(&mut self) -> io::Result<()> {
        drop(self.private.take());
        drop(self.capture.take());
        let container = self.container.take().expect("backup container owned");
        tokio::task::spawn_blocking(move || container.close())
            .await
            .map_err(io::Error::other)?
    }
}
impl Drop for Backup {
    fn drop(&mut self) {
        // Unobserved interruption must never recursively delete a displaced old package.
        if self.changed()
            && let Some(container) = self.container.take()
        {
            let _ = container.keep();
        }
    }
}

fn names(target: Target) -> [&'static str; 3] {
    let [main, model] = target.binaries();
    [main, model, "graphit-runtime"]
}

fn installed_layout(parent: &Path, names: [&str; 3]) -> io::Result<()> {
    for (index, name) in names.iter().enumerate() {
        let metadata = fs::symlink_metadata(parent.join(name))?;
        let correct = if index == 2 {
            metadata.is_dir()
        } else {
            metadata.is_file()
        };
        if !correct || metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "installed package entry has an invalid type",
            ));
        }
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
    let parent = CapturedDocumentDirectory::capture(parent_path, inputs.launcher.limits, &snapshot)
        .map_err(|error| Failure::io("capture installed directory", error))?;
    let preparation = self_update_release::prepare(
        Arc::clone(&snapshot),
        inputs.current_version,
        target,
        parent.canonical_path(),
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
    parent: CapturedDocumentDirectory,
    prepared: PreparedRelease,
) -> Result<Outcome, Failure> {
    let snapshot = Arc::clone(original.snapshot());
    let target = TargetLock::acquire(parent, inputs.limits, Arc::clone(&snapshot))
        .await
        .map_err(|error| Failure::io("lock installed package", error))?;
    let entries = names(prepared.target());
    installed_layout(target.parent.canonical_path(), entries)
        .map_err(|error| Failure::io("installed package layout", error))?;
    let stage = CapturedDocumentDirectory::capture(prepared.directory(), inputs.limits, &snapshot)
        .map_err(|error| Failure::io("capture prepared package", error))?;
    // PreparedRelease retains the producer's private envelope; its inner package
    // directories preserve release modes and need not individually be 0700.
    let installation =
        CapturedDocumentDirectory::capture(inputs.installation.root(), inputs.limits, &snapshot)
            .map_err(|error| Failure::io("capture installation control", error))?;
    let _installation_private = installation
        .private_handle(&snapshot)
        .map_err(|error| Failure::io("installation control privacy", error))?;
    let mut backup = Backup::create(&target, inputs.limits, &snapshot, &prepared)
        .map_err(|error| Failure::io("prepare package backup", error))?;
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
    let replacement = installation
        .revalidate(&snapshot)
        .and_then(|()| backup.apply(&target, &stage, &snapshot));
    if let Err(error) = replacement {
        let changed = backup.changed();
        let rollback_error = backup.restore(&target, &stage, &snapshot).err();
        let package = if backup.moves.iter().all(|state| {
            matches!(
                state,
                MoveState::Installed | MoveState::Added | MoveState::Removed
            )
        }) {
            PackageState::Installed
        } else if backup.changed() {
            PackageState::Partial
        } else if changed {
            PackageState::Restored
        } else {
            PackageState::Unchanged
        };
        let mut failure = Failure {
            cause: Cause::Io {
                operation: "replace installed package",
                error,
            },
            package,
            daemon,
            recovery: None,
            rollback_error,
            cleanup_error: None,
            restart_error: None,
        };
        if matches!(package, PackageState::Partial | PackageState::Installed) {
            failure.recovery = Some(backup.retain(RecoveryKind::RestorePackage));
        } else {
            let directory = backup
                .container
                .as_ref()
                .expect("backup owned")
                .path()
                .to_owned();
            if let Err(error) = backup.cleanup().await {
                failure.cleanup_error = Some(error);
                failure.recovery = Some(Recovery {
                    kind: RecoveryKind::CleanupBackup,
                    directory,
                });
            }
        }
        drop(singleton);
        if running && package != PackageState::Partial && original.check().is_ok() {
            failure.daemon = DaemonEffect::RestartRequested;
            match restart(host, inputs, original).await {
                Ok(effect) => failure.daemon = effect,
                Err(error) => failure.restart_error = Some(Box::new(error)),
            }
        }
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
                    recovery: Some(backup.retain(RecoveryKind::RestorePackage)),
                    rollback_error: None,
                    cleanup_error: None,
                    restart_error: None,
                });
            }
        }
    }
    let directory = backup
        .container
        .as_ref()
        .expect("backup owned")
        .path()
        .to_owned();
    if let Err(error) = backup.cleanup().await {
        return Err(Failure {
            cause: Cause::Io {
                operation: "retire package backup",
                error,
            },
            package: PackageState::Installed,
            daemon,
            recovery: Some(Recovery {
                kind: RecoveryKind::CleanupBackup,
                directory,
            }),
            rollback_error: None,
            cleanup_error: None,
            restart_error: None,
        });
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
