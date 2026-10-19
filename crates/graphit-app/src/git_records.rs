//! Git notifications use the same authenticated daemon and native record stores.
use graphit_core::{
    config::ConfigSnapshot,
    file_lock::Cancellation,
    git_hooks::{Event, Options, Registration, Registry},
    git_record_event::{GitRecordEvent, GitRefUpdate, GitRewrite, validate_commit_id},
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::policy::{HubResourcePolicy, RequestContext};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    process::ExitCode,
    time::Duration,
};

pub const ROUTE: &str = "/api/git/records";

#[derive(clap::Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}
impl Arguments {
    pub fn ignored_notification(&self) -> bool {
        matches!(&self.command, Command::Hook {event, arguments}
            if event == "reference-transaction" && matches!(arguments.as_slice(), [phase] if phase == "preparing" || phase == "prepared" || phase == "aborted"))
    }
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// Snapshot the current state, then install the native Git notifications.
    Install,
    /// Reconcile configured commands and Task/Memory events without taking a new snapshot.
    Sync,
    /// Remove only this brand's native Git hook registrations.
    Remove,
    /// Bind the current record state to HEAD, or retry a failed commit hook.
    Snapshot,
    /// Retry restoring the record snapshot for the checked-out commit.
    Restore { previous_commit: Option<String> },
    /// Dispatch current configured commands and Task/Memory notifications for a Git event.
    Hook {
        event: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub project_dir: graphit_core::native_path::NativePath,
    /// None captures only the current HEAD baseline; it does not replay old merges.
    pub event: Option<GitRecordEvent>,
    pub restore_previous: Option<String>,
    pub restore: bool,
    #[serde(default)]
    pub initialize: bool,
}

const EVENTS: [Event; 6] = [
    Event::PostCommit,
    Event::PostCheckout,
    Event::PostMerge,
    Event::PostRewrite,
    Event::PostApplypatch,
    Event::ReferenceTransaction,
];

pub fn is_record_event(event: Event) -> bool {
    EVENTS.contains(&event)
}

pub fn requires_source(arguments: &Arguments, config: &ConfigSnapshot) -> io::Result<bool> {
    if !crate::git_hook_commands::records_enabled(config)? {
        return Ok(false);
    }
    Ok(match &arguments.command {
        Command::Sync | Command::Remove => false,
        Command::Hook { event, .. } => {
            !arguments.ignored_notification() && event.parse().is_ok_and(is_record_event)
        }
        _ => true,
    })
}

/// Registration needs a repository, not necessarily a working-tree record snapshot.
pub fn repository_present(root: &std::path::Path) -> io::Result<bool> {
    match gix::discover::upwards(root) {
        Ok(_) => Ok(true),
        Err(error)
            if matches!(
                error.downcast_any_ref::<gix::discover::upwards::Error>(),
                Some(
                    gix::discover::upwards::Error::NoGitRepository { .. }
                        | gix::discover::upwards::Error::NoGitRepositoryWithinFs { .. }
                )
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(io::Error::other(error)),
    }
}

pub fn reconcile_hooks(
    project: &ProjectPaths,
    config: &ConfigSnapshot,
    remove: bool,
    cancellation: &Cancellation,
    timeout: Duration,
) -> io::Result<()> {
    let registry = Registry::capture(
        project.root(),
        project.brand().clone(),
        Options {
            timeout,
            max_command_bytes: 512,
            max_snapshot_bytes: 64 * 1024,
        },
        cancellation,
    )
    .map_err(io::Error::other)?;
    let configured = if remove {
        Default::default()
    } else {
        crate::git_hook_commands::configured(config)?
    };
    let mut events = configured
        .iter()
        .filter(|(_, commands)| !commands.is_empty())
        .map(|(event, _)| *event)
        .collect::<std::collections::BTreeSet<_>>();
    if !remove && crate::git_hook_commands::records_enabled(config)? {
        events.extend(EVENTS);
    }
    let commands = events
        .iter()
        .map(|event| {
            shlex::try_join([
                project.brand().binary_name(),
                "git",
                "hook",
                event.as_str(),
                "--",
            ])
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let desired = if remove {
        Vec::new()
    } else {
        events
            .iter()
            .zip(&commands)
            .map(|(event, command)| Registration {
                event: *event,
                command,
            })
            .collect()
    };
    registry
        .reconcile(&desired, cancellation)
        .map_err(io::Error::other)?;
    Ok(())
}

fn event(
    name: &str,
    arguments: &[String],
    input: impl Read,
    maximum: usize,
) -> io::Result<GitRecordEvent> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Git record hook arguments",
        )
    };
    match (name, arguments) {
        ("post-commit" | "post-applypatch", []) => Ok(GitRecordEvent::Commit {}),
        ("post-checkout", [old, new, flag]) if flag == "0" || flag == "1" => {
            if !matches!(old.len(), 40 | 64) {
                return Err(invalid());
            }
            if !old.bytes().all(|b| b == b'0') {
                validate_commit_id(old)?;
            }
            validate_commit_id(new)?;
            Ok(GitRecordEvent::Checkout {
                old: old.clone(),
                new: new.clone(),
                branch: flag == "1",
            })
        }
        ("post-merge", [squash]) if squash == "0" || squash == "1" => Ok(GitRecordEvent::Merge {
            squash: squash == "1",
        }),
        ("post-rewrite", [kind]) if kind == "amend" || kind == "rebase" => {
            let mut bytes = Vec::new();
            input.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > maximum {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
            let pairs = text
                .lines()
                .map(|line| {
                    let fields = line.split_whitespace().collect::<Vec<_>>();
                    let [old, new] = fields.as_slice() else {
                        return Err(invalid());
                    };
                    validate_commit_id(old)?;
                    validate_commit_id(new)?;
                    Ok(GitRewrite {
                        old: old.to_string(),
                        new: new.to_string(),
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            if pairs.is_empty() {
                return Err(invalid());
            }
            Ok(GitRecordEvent::Rewrite { pairs })
        }
        ("reference-transaction", [phase]) if phase == "committed" => {
            let mut bytes = Vec::new();
            input.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > maximum {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
            let updates = text
                .lines()
                .map(|line| {
                    let fields = line.split_whitespace().collect::<Vec<_>>();
                    let [old, new, reference] = fields.as_slice() else {
                        return Err(invalid());
                    };
                    for oid in [old, new] {
                        if let Some(reference) = oid.strip_prefix("ref:") {
                            if reference.is_empty()
                                || reference.bytes().any(|byte| byte.is_ascii_control())
                            {
                                return Err(invalid());
                            }
                            continue;
                        }
                        if !matches!(oid.len(), 40 | 64) {
                            return Err(invalid());
                        }
                        if !oid.bytes().all(|byte| byte == b'0') {
                            validate_commit_id(oid)?;
                        }
                    }
                    // Git also sends pseudorefs and symbolic targets. The
                    // server matches only the freshly observed HEAD/current
                    // branch; these strings are never filesystem paths.
                    if reference.bytes().any(|byte| byte.is_ascii_control()) {
                        return Err(invalid());
                    }
                    Ok(GitRefUpdate {
                        old: old.to_string(),
                        new: new.to_string(),
                        reference: reference.to_string(),
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            if updates.is_empty() {
                return Err(invalid());
            }
            Ok(GitRecordEvent::ReferenceTransaction { updates })
        }
        _ => Err(invalid()),
    }
}

#[allow(clippy::too_many_arguments)] // Configuration, authenticated request and I/O owners stay separate.
pub async fn run(
    arguments: Arguments,
    project: &ProjectPaths,
    installation: &InstallationPaths,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    config: &ConfigSnapshot,
    hook_input: &mut dyn Read,
    output: &mut dyn Write,
) -> io::Result<ExitCode> {
    if arguments.ignored_notification() {
        return Ok(ExitCode::SUCCESS);
    }
    if [
        "GRAPHIT_ROLE_BEARER",
        "GRAPHIT_ROLE_REFERENCE",
        "GRAPHIT_ROLE_SELECTION",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let timeout = || {
        original
            .snapshot()
            .required_deadline()?
            .checked_duration_since(std::time::Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    };
    original.check()?;
    let repository = repository_present(project.root())?;
    original.check()?;
    if !repository {
        writeln!(output, "{{\"state\":\"non_git\"}}")?;
        return Ok(ExitCode::SUCCESS);
    }
    if matches!(arguments.command, Command::Remove) {
        reconcile_hooks(project, config, true, original.cancellation(), timeout()?)?;
        writeln!(output, "{{\"hooks\":\"removed\"}}")?;
        return Ok(ExitCode::SUCCESS);
    }
    if matches!(arguments.command, Command::Sync)
        || (matches!(arguments.command, Command::Install)
            && !crate::git_hook_commands::records_enabled(config)?)
    {
        reconcile_hooks(project, config, false, original.cancellation(), timeout()?)?;
        writeln!(output, "{{\"hooks\":\"reconciled\"}}")?;
        return Ok(ExitCode::SUCCESS);
    }
    let install = matches!(arguments.command, Command::Install);
    let (event, restore_previous, restore) = match arguments.command {
        Command::Install | Command::Snapshot => (None, None, false),
        Command::Restore { previous_commit } => {
            if let Some(commit) = &previous_commit {
                validate_commit_id(commit)?;
            }
            (None, previous_commit, true)
        }
        Command::Hook {
            event: name,
            arguments,
        } => {
            let value = event(
                &name,
                &arguments,
                hook_input,
                policy.content().max_input_bytes,
            )?;
            // File-only checkouts do not change the selected record snapshot.
            if matches!(value, GitRecordEvent::Checkout { branch: false, .. }) {
                return Ok(ExitCode::SUCCESS);
            }
            (Some(value), None, false)
        }
        Command::Remove | Command::Sync => unreachable!(),
    };
    let limits = NativeDocumentFileLimits::new(
        policy.content().max_input_bytes,
        graphit_core::native_path::MAX_PATH_BYTES,
        policy.content().max_depth,
        policy.content().max_scalar_bytes,
    )?;
    let address = crate::source_entry::address(installation, limits, original).await?;
    let headers = crate::ast_source::runtime_headers(installation, limits, original).await?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(io::Error::other)?;
    let request = Request {
        project_dir: graphit_core::native_path::NativePath::new(project.root().to_owned())?,
        event,
        restore_previous,
        restore,
        initialize: install,
    };
    let body = serde_json::to_vec(&request)?;
    if body.len() > policy.content().max_input_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let operation = async {
        let mut response = client
            .post(format!("http://{address}{ROUTE}"))
            .headers(headers.into_iter().collect())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(io::Error::other)?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
            original.check()?;
            if chunk.len()
                > policy
                    .content()
                    .max_output_bytes
                    .saturating_sub(bytes.len())
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        serde_json::to_writer(&mut *output, &value)?;
        writeln!(output)?;
        if !status.is_success() {
            return Ok(ExitCode::FAILURE);
        }
        if install {
            reconcile_hooks(project, config, false, original.cancellation(), timeout()?)?;
        }
        if value.get("state").and_then(serde_json::Value::as_str) == Some("applied") {
            let id = graphit_core::project_identity::ProjectIdentityStore::at(project)
                .load()?
                .ok_or(io::ErrorKind::NotFound)?
                .id();
            let command = crate::foreground_project_control::ProjectCommand::MaintainCheckout {
                project_id: id,
                project_dir: graphit_core::native_path::NativePath::new(project.root().to_owned())?,
                version: String::new(),
                detached_base: String::new(),
                select: true,
            };
            let (status, result) = crate::foreground_project_control::send(
                installation,
                address,
                &client,
                (limits, policy.content()),
                &command,
                original,
            )
            .await?;
            if !status.is_success() {
                serde_json::to_writer(
                    &mut *output,
                    &serde_json::json!({"state":"records_applied_selection_refresh_failed", "detail":result}),
                )?;
                writeln!(output)?;
                return Ok(ExitCode::FAILURE);
            }
        }
        Ok(ExitCode::SUCCESS)
    };
    tokio::select! { result = operation => result,
    () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
    () = tokio::time::sleep_until(original.snapshot().required_deadline()?.into()) => Err(io::ErrorKind::TimedOut.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hook_arguments_are_closed_and_rewrites_bounded() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        assert!(matches!(
            event(
                "post-checkout",
                &[a.clone(), b.clone(), "1".into()],
                &b""[..],
                1024
            )
            .unwrap(),
            GitRecordEvent::Checkout { branch: true, .. }
        ));
        assert!(event("post-commit", &["extra".into()], &b""[..], 1024).is_err());
        let pair = format!("{a} {b}\n");
        assert!(event("post-rewrite", &["rebase".into()], pair.as_bytes(), 20).is_err());
        assert!(
            matches!(event("post-rewrite", &["amend".into()], pair.as_bytes(), 1024).unwrap(),
            GitRecordEvent::Rewrite { pairs } if pairs.len() == 1)
        );
        let updates = format!("{a} {b} refs/heads/main\n");
        assert!(
            matches!(event("reference-transaction", &["committed".into()], updates.as_bytes(), 1024).unwrap(),
            GitRecordEvent::ReferenceTransaction { updates } if updates.len() == 1)
        );
        let symbolic = format!("{} ref:refs/heads/feature HEAD\n", "0".repeat(40));
        assert!(
            matches!(event("reference-transaction", &["committed".into()], symbolic.as_bytes(), 1024).unwrap(),
            GitRecordEvent::ReferenceTransaction { updates } if updates[0].new == "ref:refs/heads/feature")
        );
        let pseudo = format!("{} {b} CHERRY_PICK_HEAD\n", "0".repeat(40));
        assert!(
            event(
                "reference-transaction",
                &["committed".into()],
                pseudo.as_bytes(),
                1024
            )
            .is_ok()
        );
        for phase in ["preparing", "prepared", "aborted"] {
            assert!(
                Arguments {
                    command: Command::Hook {
                        event: "reference-transaction".into(),
                        arguments: vec![phase.into()]
                    }
                }
                .ignored_notification()
            );
        }
    }
}
