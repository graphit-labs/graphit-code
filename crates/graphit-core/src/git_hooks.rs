//! Reconcile brand-owned local hook registrations through Git 2.54 or newer.
//!
//! This registry stores trusted Git commands; it does not execute checks or pick
//! product defaults. Each Git config write is atomic, the whole reconciliation
//! is not. Global/worktree overrides retain Git's native precedence.

use crate::brand::Brand;
use crate::file_lock::{Cancellation, FileLock};
use crate::git_checkout::git_command;
use crate::local_files::{canonical_directory, directory_exists, regular_metadata};
use crate::process::{Budget, OUTPUT_LIMIT};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::str::FromStr;
use std::time::{Duration, Instant};

macro_rules! events {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
        pub enum Event { $($variant),+ }
        impl Event {
            pub const ALL: [Self; 28] = [$(Self::$variant),+];
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }
        }
        impl FromStr for Event {
            type Err = InvalidEvent;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value { $($name => Ok(Self::$variant)),+, _ => Err(InvalidEvent) }
            }
        }
    };
}
events! {
    ApplypatchMsg => "applypatch-msg", CommitMsg => "commit-msg",
    FsmonitorWatchman => "fsmonitor-watchman", P4Changelist => "p4-changelist",
    P4PostChangelist => "p4-post-changelist", P4PreSubmit => "p4-pre-submit",
    P4PrepareChangelist => "p4-prepare-changelist", PostApplypatch => "post-applypatch",
    PostCheckout => "post-checkout", PostCommit => "post-commit",
    PostIndexChange => "post-index-change", PostMerge => "post-merge",
    PostReceive => "post-receive", PostRewrite => "post-rewrite",
    PostUpdate => "post-update", PreApplypatch => "pre-applypatch",
    PreAutoGc => "pre-auto-gc", PreCommit => "pre-commit",
    PreMergeCommit => "pre-merge-commit", PrePush => "pre-push", PreRebase => "pre-rebase",
    PreReceive => "pre-receive", PrepareCommitMsg => "prepare-commit-msg",
    ProcReceive => "proc-receive", PushToCheckout => "push-to-checkout",
    ReferenceTransaction => "reference-transaction", SendemailValidate => "sendemail-validate",
    Update => "update",
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidEvent;
impl fmt::Display for InvalidEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Git hook event")
    }
}
impl std::error::Error for InvalidEvent {}

/// Ordered, complete shell commands. This is JSON configuration, never a CSV list.
pub fn commands(
    commands: BTreeMap<String, Vec<String>>,
) -> io::Result<BTreeMap<Event, Vec<String>>> {
    commands
        .into_iter()
        .map(|(name, commands)| {
            let event = name.parse::<Event>().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("githooks: unknown Git event {name:?}"),
                )
            })?;
            if commands
                .iter()
                .any(|command| command.trim().is_empty() || command.contains('\0'))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("githooks.{name}: commands must be nonempty strings without NUL"),
                ));
            }
            if matches!(event, Event::ProcReceive | Event::FsmonitorWatchman) && commands.len() > 1
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "githooks.{name}: use one command to own the interactive/output protocol"
                    ),
                ));
            }
            Ok((event, commands))
        })
        .collect()
}

pub struct Registration<'a> {
    pub event: Event,
    pub command: &'a str,
}

/// Use the POSIX shell supplied by Git, including Git for Windows. This is a
/// finite Git builtin, not execution of the configured hook commands.
pub fn shell_path(root: &Path, timeout: Duration, cancel: &Cancellation) -> io::Result<PathBuf> {
    let budget = Budget::new(timeout, cancel)?;
    version(root, &budget).map_err(io::Error::other)?;
    let output = budget.run(
        Command::new("git")
            .current_dir(root)
            .args(["var", "GIT_SHELL_PATH"]),
    )?;
    if !output.status.success() {
        return Err(io::Error::other("git var GIT_SHELL_PATH failed"));
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "Git shell path must be Unicode")
    })?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    let path = PathBuf::from(text);
    if text.contains('\0') || !path.is_absolute() || !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Git's POSIX shell is unavailable",
        ));
    }
    Ok(path)
}
impl fmt::Debug for Registration<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registration")
            .field("event", &self.event)
            .finish_non_exhaustive()
    }
}

/// Limits must accommodate all 28 possible owned registrations. Filesystem
/// calls and allocator/OS overhead are not hard memory or time guarantees.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub timeout: Duration,
    pub max_command_bytes: usize,
    pub max_snapshot_bytes: usize,
}
impl Options {
    fn validate(self, brand: &Brand) -> Result<(), HookError> {
        let event_bytes = Event::ALL.iter().map(|e| e.as_str().len()).max().unwrap();
        let required = brand
            .key()
            .len()
            .checked_add(event_bytes + 17)
            .and_then(|key| key.checked_mul(3))
            .and_then(|keys| keys.checked_add(self.max_command_bytes))
            .and_then(|row| row.checked_add(event_bytes + 5))
            .and_then(|row| row.checked_mul(Event::ALL.len()));
        if self.timeout.is_zero()
            || Instant::now().checked_add(self.timeout).is_none()
            || self.max_command_bytes == 0
            || self.max_snapshot_bytes > OUTPUT_LIMIT
            || required.is_none_or(|bytes| bytes > self.max_snapshot_bytes)
        {
            return Err(HookError::new(
                Failure::InvalidOptions,
                Stage::Validation,
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Validation,
    Version,
    Resolve,
    Lock,
    Read,
    Write,
    Verify,
    Unlock,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    InvalidOptions,
    InvalidRegistration,
    UnsupportedGit,
    InvalidCheckout,
    InvalidSnapshot,
    Io {
        kind: io::ErrorKind,
        os_code: Option<i32>,
    },
    Git {
        exit_code: Option<i32>,
        event: Option<Event>,
    },
}
/// Contains no commands, raw output, filesystem paths, or OS error messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HookError {
    pub failure: Failure,
    pub stage: Stage,
    /// Conservative: true before the first mutating Git attempt, including a
    /// failed attempt whose child may already have committed a config write.
    pub may_have_changed: bool,
}
impl HookError {
    fn new(failure: Failure, stage: Stage, changed: bool) -> Self {
        Self {
            failure,
            stage,
            may_have_changed: changed,
        }
    }
    fn io(error: io::Error, stage: Stage, changed: bool) -> Self {
        Self::new(
            Failure::Io {
                kind: error.kind(),
                os_code: error.raw_os_error(),
            },
            stage,
            changed,
        )
    }
}
impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.failure == Failure::UnsupportedGit {
            return f.write_str(
                "native Git hooks require Git 2.54 or newer (hook.<name>.event/command/enabled)",
            );
        }
        write!(
            f,
            "Git hook registry failed at {:?}: {:?}; may_have_changed={}",
            self.stage, self.failure, self.may_have_changed
        )
    }
}
impl std::error::Error for HookError {}

#[derive(Debug, Eq, PartialEq)]
pub struct Report {
    pub selected: Vec<Event>,
    pub changed: Vec<Event>,
    pub removed: Vec<Event>,
    pub git_config_writes: usize,
}

#[derive(Clone)]
pub struct Registry {
    root: PathBuf,
    common: PathBuf,
    brand: Brand,
    options: Options,
}
impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("brand", &self.brand)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}
impl Registry {
    /// Capture an exact working-tree root or bare repository, without writes.
    pub fn capture(
        checkout: &Path,
        brand: Brand,
        options: Options,
        cancel: &Cancellation,
    ) -> Result<Self, HookError> {
        options.validate(&brand)?;
        let budget = Budget::new(options.timeout, cancel)
            .map_err(|e| HookError::io(e, Stage::Validation, false))?;
        let root =
            canonical_directory(checkout).map_err(|e| HookError::io(e, Stage::Resolve, false))?;
        version(&root, &budget)?;
        let resolve = |args: &[&str]| -> Result<PathBuf, HookError> {
            let output = budget
                .run(&mut git_command(&root, args))
                .map_err(|e| HookError::io(e, Stage::Resolve, false))?;
            if !output.status.success() {
                return Err(HookError::new(
                    Failure::InvalidCheckout,
                    Stage::Resolve,
                    false,
                ));
            }
            canonical_directory(&native_path(&output.stdout)?)
                .map_err(|e| HookError::io(e, Stage::Resolve, false))
        };
        let bare = budget
            .run(&mut git_command(
                &root,
                &["rev-parse", "--is-bare-repository"],
            ))
            .map_err(|e| HookError::io(e, Stage::Resolve, false))?;
        let location = if bare.status.success() && bare.stdout == b"true\n" {
            resolve(&["rev-parse", "--path-format=absolute", "--git-dir"])?
        } else {
            resolve(&["rev-parse", "--show-toplevel"])?
        };
        if location != root {
            return Err(HookError::new(
                Failure::InvalidCheckout,
                Stage::Resolve,
                false,
            ));
        }
        let common = resolve(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        budget
            .check()
            .map_err(|e| HookError::io(e, Stage::Resolve, false))?;
        Ok(Self {
            root,
            common,
            brand,
            options,
        })
    }

    pub fn reconcile(
        &self,
        desired: &[Registration<'_>],
        cancel: &Cancellation,
    ) -> Result<Report, HookError> {
        self.reconcile_with(&Event::ALL, desired, cancel, |budget, command| {
            budget.run(command)
        })
    }

    /// Reconcile only these events. Registrations owned by the same brand for
    /// other events are preserved, including when `desired` removes this set.
    pub fn reconcile_events(
        &self,
        events: &[Event],
        desired: &[Registration<'_>],
        cancel: &Cancellation,
    ) -> Result<Report, HookError> {
        self.reconcile_with(events, desired, cancel, |budget, command| {
            budget.run(command)
        })
    }

    fn reconcile_with(
        &self,
        events: &[Event],
        desired: &[Registration<'_>],
        cancel: &Cancellation,
        mut execute: impl FnMut(&Budget<'_>, &mut Command) -> io::Result<Output>,
    ) -> Result<Report, HookError> {
        let budget = Budget::new(self.options.timeout, cancel)
            .map_err(|e| HookError::io(e, Stage::Validation, false))?;
        let events_set: BTreeSet<_> = events.iter().copied().collect();
        let mut selected = BTreeMap::new();
        if events.len() != events_set.len() || desired.len() > events.len() {
            return Err(HookError::new(
                Failure::InvalidRegistration,
                Stage::Validation,
                false,
            ));
        }
        for registration in desired {
            if !events_set.contains(&registration.event)
                || registration.command.trim().is_empty()
                || registration.command.contains('\0')
                || registration.command.len() > self.options.max_command_bytes
                || selected
                    .insert(registration.event, registration.command)
                    .is_some()
            {
                return Err(HookError::new(
                    Failure::InvalidRegistration,
                    Stage::Validation,
                    false,
                ));
            }
        }
        version(&self.root, &budget)?;
        let lock_path = self.common.join(".hooks-manager.lock");
        if !directory_exists(&self.common).map_err(|e| HookError::io(e, Stage::Lock, false))? {
            return Err(HookError::io(
                io::ErrorKind::NotFound.into(),
                Stage::Lock,
                false,
            ));
        }
        regular_metadata(&lock_path).map_err(|e| HookError::io(e, Stage::Lock, false))?;
        budget
            .check()
            .map_err(|e| HookError::io(e, Stage::Lock, false))?;
        let lock = FileLock::acquire(lock_path, budget.remaining(), cancel)
            .map_err(|e| HookError::io(e, Stage::Lock, false))?;
        let mut changed = false;
        let result = self.edit(&events_set, &selected, &budget, &mut execute, &mut changed);
        let release = lock
            .release()
            .map_err(|e| HookError::io(e, Stage::Unlock, changed));
        match result {
            Err(primary) => Err(primary),
            Ok(report) => {
                release?;
                budget
                    .check()
                    .map_err(|e| HookError::io(e, Stage::Unlock, changed))?;
                Ok(report)
            }
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = git_command(&self.root, &[]);
        let common = self.common.as_path();
        #[cfg(windows)]
        let common = dunce::simplified(common);
        command.arg("--git-dir").arg(common).args(args);
        command
    }

    fn snapshot(
        &self,
        budget: &Budget<'_>,
        execute: &mut impl FnMut(&Budget<'_>, &mut Command) -> io::Result<Output>,
        stage: Stage,
        changed: bool,
    ) -> Result<Output, HookError> {
        let names = Event::ALL.map(Event::as_str).join("|");
        let pattern = format!(
            "^hook\\.{}--({names})\\.(command|event|enabled)$",
            self.brand.key()
        );
        let output = execute(
            budget,
            &mut self.command(&[
                "config",
                "get",
                "--local",
                "--no-includes",
                "--all",
                "--show-names",
                "--regexp",
                "--null",
                &pattern,
            ]),
        )
        .map_err(|e| HookError::io(e, stage, changed))?;
        if !output.status.success()
            && !(output.status.code() == Some(1)
                && output.stdout.is_empty()
                && output.stderr.is_empty())
        {
            return Err(HookError::new(
                Failure::Git {
                    exit_code: output.status.code(),
                    event: None,
                },
                stage,
                changed,
            ));
        }
        if output.stdout.len() > self.options.max_snapshot_bytes {
            return Err(HookError::new(Failure::InvalidSnapshot, stage, changed));
        }
        Ok(output)
    }

    fn edit(
        &self,
        events: &BTreeSet<Event>,
        desired: &BTreeMap<Event, &str>,
        budget: &Budget<'_>,
        execute: &mut impl FnMut(&Budget<'_>, &mut Command) -> io::Result<Output>,
        changed: &mut bool,
    ) -> Result<Report, HookError> {
        budget
            .check()
            .map_err(|e| HookError::io(e, Stage::Read, *changed))?;
        let output = self.snapshot(budget, execute, Stage::Read, *changed)?;
        let snapshot = parse(&output.stdout, self.brand.key())
            .map_err(|failure| HookError::new(failure, Stage::Read, *changed))?;
        let mut report = Report {
            selected: desired.keys().copied().collect(),
            changed: Vec::new(),
            removed: Vec::new(),
            git_config_writes: 0,
        };
        for &event in events {
            let current = snapshot.get(&event);
            if let Some(command) = desired.get(&event) {
                if current.is_some_and(|state| state.matches(event, command)) {
                    continue;
                }
                for (field, value) in [
                    ("enabled", "false"),
                    ("command", *command),
                    ("event", event.as_str()),
                    ("enabled", "true"),
                ] {
                    let key = format!("hook.{}--{}.{}", self.brand.key(), event.as_str(), field);
                    *changed = true;
                    let output = execute(
                        budget,
                        &mut self
                            .command(&["config", "set", "--local", "--all", "--", &key, value]),
                    )
                    .map_err(|e| HookError::io(e, Stage::Write, *changed))?;
                    check_write(output, event, *changed, false)?;
                    report.git_config_writes += 1;
                }
                report.changed.push(event);
            } else if let Some(state) = current {
                for (field, value) in [
                    ("event", state.event),
                    ("enabled", state.enabled),
                    ("command", state.command),
                ] {
                    if value.count == 0 {
                        continue;
                    }
                    let key = format!("hook.{}--{}.{}", self.brand.key(), event.as_str(), field);
                    *changed = true;
                    let output = execute(
                        budget,
                        &mut self.command(&["config", "unset", "--local", "--all", "--", &key]),
                    )
                    .map_err(|e| HookError::io(e, Stage::Write, *changed))?;
                    let success = output.status.success();
                    check_write(output, event, *changed, true)?;
                    report.git_config_writes += usize::from(success);
                }
                report.removed.push(event);
            }
        }
        if *changed {
            let output = self.snapshot(budget, execute, Stage::Verify, *changed)?;
            let snapshot = parse(&output.stdout, self.brand.key())
                .map_err(|failure| HookError::new(failure, Stage::Verify, *changed))?;
            if snapshot
                .keys()
                .filter(|event| events.contains(*event))
                .count()
                != desired.len()
                || desired.iter().any(|(event, command)| {
                    !snapshot
                        .get(event)
                        .is_some_and(|state| state.matches(*event, command))
                })
            {
                return Err(HookError::new(
                    Failure::InvalidSnapshot,
                    Stage::Verify,
                    *changed,
                ));
            }
        }
        Ok(report)
    }
}

fn check_write(
    output: Output,
    event: Event,
    changed: bool,
    removing: bool,
) -> Result<(), HookError> {
    if output.status.success() || (removing && output.status.code() == Some(5)) {
        Ok(())
    } else {
        Err(HookError::new(
            Failure::Git {
                exit_code: output.status.code(),
                event: Some(event),
            },
            Stage::Write,
            changed,
        ))
    }
}

fn version(root: &Path, budget: &Budget<'_>) -> Result<(), HookError> {
    let output = budget
        .run(&mut git_command(root, &["--version"]))
        .map_err(|e| HookError::io(e, Stage::Version, false))?;
    if output.status.success() && supported_version(&output.stdout) {
        Ok(())
    } else {
        Err(HookError::new(
            Failure::UnsupportedGit,
            Stage::Version,
            false,
        ))
    }
}
fn supported_version(bytes: &[u8]) -> bool {
    let Some(version) = bytes
        .strip_prefix(b"git version ")
        .and_then(|v| std::str::from_utf8(v).ok())
    else {
        return false;
    };
    let mut parts = version.trim().split('.');
    let major = parts.next().and_then(|p| p.parse::<u32>().ok());
    let minor = parts.next().and_then(|p| p.parse::<u32>().ok());
    matches!((major, minor), (Some(major), Some(minor)) if major > 2 || (major == 2 && minor >= 54))
}
fn native_path(bytes: &[u8]) -> Result<PathBuf, HookError> {
    let invalid = || HookError::new(Failure::InvalidCheckout, Stage::Resolve, false);
    let bytes = bytes.strip_suffix(b"\n").ok_or_else(invalid)?;
    #[cfg(windows)]
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(invalid());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::ffi::OsString::from_vec(bytes.to_vec()).into())
    }
    #[cfg(not(unix))]
    {
        Ok(std::str::from_utf8(bytes).map_err(|_| invalid())?.into())
    }
}

#[derive(Clone, Copy, Default)]
struct Field<'a> {
    value: &'a [u8],
    count: usize,
}
impl Field<'_> {
    fn is(&self, value: &[u8]) -> bool {
        self.count == 1 && self.value == value
    }
}
#[derive(Default)]
struct State<'a> {
    command: Field<'a>,
    event: Field<'a>,
    enabled: Field<'a>,
}
impl State<'_> {
    fn matches(&self, event: Event, command: &str) -> bool {
        self.command.is(command.as_bytes())
            && self.event.is(event.as_str().as_bytes())
            && self.enabled.is(b"true")
    }
}
// Decode Git's documented wire format, never a Git config file. Values borrow
// the bounded response; duplicates consume only a counter, not copied strings.
fn parse<'a>(bytes: &'a [u8], brand: &str) -> Result<BTreeMap<Event, State<'a>>, Failure> {
    let mut states = BTreeMap::<Event, State<'a>>::new();
    if bytes.is_empty() {
        return Ok(states);
    }
    if !bytes.ends_with(&[0]) {
        return Err(Failure::InvalidSnapshot);
    }
    let prefix = format!("hook.{brand}--");
    for record in bytes[..bytes.len() - 1].split(|b| *b == 0) {
        let separator = record
            .iter()
            .position(|b| *b == b'\n')
            .ok_or(Failure::InvalidSnapshot)?;
        let key =
            std::str::from_utf8(&record[..separator]).map_err(|_| Failure::InvalidSnapshot)?;
        let (event, field) = key
            .strip_prefix(&prefix)
            .and_then(|s| s.rsplit_once('.'))
            .ok_or(Failure::InvalidSnapshot)?;
        let state = states
            .entry(event.parse().map_err(|_| Failure::InvalidSnapshot)?)
            .or_default();
        let field = match field {
            "command" => &mut state.command,
            "event" => &mut state.event,
            "enabled" => &mut state.enabled,
            _ => return Err(Failure::InvalidSnapshot),
        };
        field.count += 1;
        if field.count == 1 {
            field.value = &record[separator + 1..];
        }
    }
    Ok(states)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Registry) {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            git_command(dir.path(), &["init", "--initial-branch=main"])
                .output()
                .unwrap()
                .status
                .success()
        );
        let options = Options {
            timeout: Duration::from_secs(5),
            max_command_bytes: 256,
            max_snapshot_bytes: 32768,
        };
        let registry = Registry::capture(
            dir.path(),
            Brand::default(),
            options,
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(registry.root, std::fs::canonicalize(dir.path()).unwrap());
        assert_eq!(
            registry.common,
            std::fs::canonicalize(dir.path().join(".git")).unwrap()
        );
        (dir, registry)
    }

    #[test]
    fn temporary_common_directory_snapshot_preserves_canonical_paths() {
        let (_dir, registry) = fixture();
        let root = registry.root.clone();
        let common = registry.common.clone();
        let cancel = Cancellation::default();
        let budget = Budget::new(registry.options.timeout, &cancel).unwrap();
        let output = registry
            .snapshot(
                &budget,
                &mut |budget, command| {
                    let output = budget.run(command)?;
                    // This fresh private repository has no matching hook keys.
                    assert_eq!(
                        output.status.code(),
                        Some(1),
                        "private Git snapshot status={:?}: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    );
                    Ok(output)
                },
                Stage::Read,
                false,
            )
            .unwrap();
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert_eq!(registry.root, root);
        assert_eq!(registry.common, common);
    }

    fn get(registry: &Registry, key: &str) -> Vec<u8> {
        let output = registry
            .command(&["config", "get", "--local", "--no-includes", "--null", key])
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    }

    #[test]
    fn version_threshold_and_wire_format_are_strict_and_borrowed() {
        for bytes in [
            b"git version 2.54.0\n".as_slice(),
            b"git version 2.55.0.windows.1\r\n",
            b"git version 3.0.0 (vendor)\n",
        ] {
            assert!(supported_version(bytes));
        }
        for bytes in [
            b"git version 2.53.9\n".as_slice(),
            b"git version 1.99.0\n",
            b"git version 2.x.0\n",
            b"2.55.0\n",
            b"git version 99999999999.2.0",
        ] {
            assert!(!supported_version(bytes));
        }
        let bytes = b"hook.graphit--pre-commit.command\nline1\nline2\0hook.graphit--pre-commit.command\nsecond\0hook.graphit--pre-commit.event\npre-commit\0hook.graphit--pre-commit.enabled\ntrue\0";
        let parsed = parse(bytes, "graphit").unwrap();
        let state = &parsed[&Event::PreCommit];
        assert_eq!(state.command.count, 2);
        assert_eq!(state.command.value, b"line1\nline2");
        let value_offset = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
        assert_eq!(state.command.value.as_ptr(), bytes[value_offset..].as_ptr());
        assert!(!state.matches(Event::PreCommit, "line1\nline2"));
        for invalid in [
            b"no-terminator".as_slice(),
            b"key\0",
            b"\0",
            b"hook.other-pre-commit.command\nv\0",
            b"hook.graphit--unknown.command\nv\0",
            b"hook.graphit--pre-commit.unknown\nv\0",
            b"\xff\nv\0",
        ] {
            assert!(matches!(
                parse(invalid, "graphit"),
                Err(Failure::InvalidSnapshot)
            ));
        }
    }

    #[test]
    fn failed_update_stays_disabled_and_next_explicit_reconciliation_converges() {
        let (_dir, registry) = fixture();
        let cancel = Cancellation::default();
        registry
            .reconcile(
                &[Registration {
                    event: Event::PreCommit,
                    command: "old",
                }],
                &cancel,
            )
            .unwrap();
        let desired = [Registration {
            event: Event::PreCommit,
            command: "new",
        }];
        let mut writes = 0;
        let error = registry
            .reconcile_with(&Event::ALL, &desired, &cancel, |budget, command| {
                if command.get_args().any(|a| a == "set") {
                    writes += 1;
                    if writes == 2 {
                        return Err(io::Error::other("private raw failure marker"));
                    }
                }
                budget.run(command)
            })
            .unwrap_err();
        assert_eq!(writes, 2);
        assert_eq!(error.stage, Stage::Write);
        assert!(error.may_have_changed);
        assert!(matches!(
            error.failure,
            Failure::Io {
                kind: io::ErrorKind::Other,
                ..
            }
        ));
        assert!(!format!("{error:?}{error}").contains("private raw failure marker"));
        assert_eq!(
            get(&registry, "hook.graphit--pre-commit.enabled"),
            b"false\0"
        );
        assert_eq!(get(&registry, "hook.graphit--pre-commit.command"), b"old\0");
        assert!(!FileLock::is_locked(registry.common.join(".hooks-manager.lock")).unwrap());
        assert_eq!(
            registry
                .reconcile(&desired, &cancel)
                .unwrap()
                .git_config_writes,
            4
        );
        assert_eq!(get(&registry, "hook.graphit--pre-commit.command"), b"new\0");
        assert_eq!(
            get(&registry, "hook.graphit--pre-commit.enabled"),
            b"true\0"
        );
    }

    #[test]
    fn malformed_or_oversized_snapshot_never_writes_and_verification_is_required() {
        let (_dir, registry) = fixture();
        let cancel = Cancellation::default();
        let desired = [Registration {
            event: Event::PreCommit,
            command: "existing",
        }];
        registry.reconcile(&desired, &cancel).unwrap();
        for malformed in [
            b"malformed\0".to_vec(),
            vec![b'x'; registry.options.max_snapshot_bytes + 1],
        ] {
            let mut calls = 0;
            let error = registry
                .reconcile_with(&Event::ALL, &desired, &cancel, |budget, command| {
                    calls += 1;
                    let mut output = budget.run(command)?;
                    output.stdout = malformed.clone();
                    Ok(output)
                })
                .unwrap_err();
            assert_eq!(calls, 1);
            assert_eq!(error.failure, Failure::InvalidSnapshot);
            assert_eq!(error.stage, Stage::Read);
            assert!(!error.may_have_changed);
        }
        let desired = [Registration {
            event: Event::PreCommit,
            command: "updated",
        }];
        let mut reads = 0;
        let error = registry
            .reconcile_with(&Event::ALL, &desired, &cancel, |budget, command| {
                let reading = command.get_args().any(|a| a == "get");
                let mut output = budget.run(command)?;
                if reading {
                    reads += 1;
                    if reads == 2 {
                        output.stdout.clear();
                    }
                }
                Ok(output)
            })
            .unwrap_err();
        assert_eq!(reads, 2);
        assert_eq!(error.stage, Stage::Verify);
        assert!(error.may_have_changed);
        assert_eq!(error.failure, Failure::InvalidSnapshot);
        assert_eq!(
            registry
                .reconcile(&desired, &cancel)
                .unwrap()
                .git_config_writes,
            0
        );
    }

    #[test]
    fn cancellation_after_a_config_write_reports_partial_and_releases_lock() {
        let (_dir, registry) = fixture();
        let cancel = Cancellation::default();
        let error = registry
            .reconcile_with(
                &Event::ALL,
                &[Registration {
                    event: Event::PreCommit,
                    command: "desired",
                }],
                &cancel,
                |budget, command| {
                    let writing = command.get_args().any(|a| a == "set");
                    let output = budget.run(command)?;
                    if writing {
                        cancel.cancel();
                    }
                    Ok(output)
                },
            )
            .unwrap_err();
        assert_eq!(error.stage, Stage::Write);
        assert!(error.may_have_changed);
        assert!(matches!(
            error.failure,
            Failure::Io {
                kind: io::ErrorKind::Interrupted,
                ..
            }
        ));
        assert_eq!(
            get(&registry, "hook.graphit--pre-commit.enabled"),
            b"false\0"
        );
        assert!(!FileLock::is_locked(registry.common.join(".hooks-manager.lock")).unwrap());
    }
}
