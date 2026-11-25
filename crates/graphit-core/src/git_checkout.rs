//! Checkout facts read by Git itself, independent of invocation environment.

use crate::file_lock::Cancellation;
use crate::process::Budget;
use crate::project_version::{DEFAULT_NAME, validate};
use crate::version_context::Checkout;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitCheckout {
    NonGit,
    Branch { name: String, default: String },
    Detached { commit: String, default: String },
}

impl GitCheckout {
    /// Reads a checkout without modifying repository or Graphit state.
    /// The shared timeout covers all child commands; filesystem calls use OS I/O.
    pub fn inspect(
        directory: impl AsRef<Path>,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<Self> {
        let budget = Budget::new(timeout, cancellation)?;
        let directory = fs::canonicalize(directory)?;
        if !directory.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "checkout must be a directory",
            ));
        }
        let mut root = None;
        for ancestor in directory.ancestors() {
            budget.check()?;
            match fs::symlink_metadata(ancestor.join(".git")) {
                Ok(_) => {
                    root = Some(ancestor);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let Some(root) = root else {
            return Ok(Self::NonGit);
        };
        let git = |args: &[&str]| budget.run(&mut git_command(root, args));
        // Git may skip an invalid .git directory and discover an ancestor. Confirm
        // this exact boundary without parsing paths or any repository file format.
        let boundary = success(git(&[
            "rev-parse",
            "--is-inside-work-tree",
            "--show-prefix",
        ])?)?;
        if boundary.stdout != b"true\n\n" && boundary.stdout != b"true\r\n\r\n" {
            return Err(invalid("Git did not resolve the nearest checkout boundary"));
        }
        let head = optional(git(&["symbolic-ref", "--quiet", "HEAD"])?)?;
        let mut detached_commit = None;
        let branch = match head {
            Some(head) => {
                let name = branch_name(&head, "refs/heads/")?;
                let commit = git(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?;
                if commit.status.code() == Some(1) && commit.stderr.is_empty() {
                    // Missing branch is unborn; an existing ref with no commit is corrupt.
                    if exists(git(&["show-ref", "--verify", "--quiet", &head])?)? {
                        return Err(invalid("attached HEAD does not name a local commit"));
                    }
                } else {
                    success(commit)?;
                }
                Some(name)
            }
            None => {
                // A detached HEAD must name a real commit, not a malformed/missing ref.
                let output = success(git(&["rev-parse", "--verify", "HEAD^{commit}"])?)?;
                let commit = std::str::from_utf8(&output.stdout)
                    .map_err(|_| invalid("Git commit is not UTF-8"))?
                    .trim_end();
                crate::git_record_event::validate_commit_id(commit)?;
                detached_commit = Some(commit.to_owned());
                None
            }
        };
        let origin = optional(git(&[
            "symbolic-ref",
            "--quiet",
            "refs/remotes/origin/HEAD",
        ])?)?;
        let default = if let Some(origin) = origin {
            branch_name(&origin, "refs/remotes/origin/")?
        } else if exists(git(&[
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/main",
        ])?)? {
            DEFAULT_NAME.to_owned()
        } else if exists(git(&[
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/master",
        ])?)? {
            "master".to_owned()
        } else if let Some(branch) = &branch {
            let refs = success(git(&[
                "for-each-ref",
                "--count=1",
                "--format=%(refname)",
                "refs/heads/",
            ])?)?;
            if refs.stdout.is_empty() {
                branch.clone()
            } else {
                DEFAULT_NAME.to_owned()
            }
        } else {
            DEFAULT_NAME.to_owned()
        };
        Ok(match branch {
            Some(name) => Self::Branch { name, default },
            None => Self::Detached {
                commit: detached_commit.expect("validated detached commit"),
                default,
            },
        })
    }

    pub fn as_checkout(&self) -> Checkout<'_> {
        match self {
            Self::NonGit => Checkout::NonGit,
            Self::Branch { name, default } => Checkout::Branch { name, default },
            Self::Detached { commit, default } => Checkout::Detached { commit, default },
        }
    }
}

pub(crate) fn git_command(directory: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .arg("--no-optional-locks")
        .arg("--no-lazy-fetch")
        .arg("-C")
        .arg(directory)
        .args(args);
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");
    command
}

fn branch_name(value: &str, prefix: &str) -> io::Result<String> {
    let name = value
        .strip_prefix(prefix)
        .ok_or_else(|| invalid("unexpected symbolic reference target"))?;
    validate(name).map_err(|_| invalid("invalid Git branch name"))?;
    if name.is_empty() || name.trim() != name {
        return Err(invalid("empty or padded Git branch name"));
    }
    Ok(name.to_owned())
}

fn optional(output: Output) -> io::Result<Option<String>> {
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(None);
    }
    let output = success(output)?;
    let value =
        std::str::from_utf8(&output.stdout).map_err(|_| invalid("Git reference is not UTF-8"))?;
    let value = value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .unwrap_or(value);
    Ok(Some(value.to_owned()))
}

fn exists(output: Output) -> io::Result<bool> {
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(false);
    }
    success(output).map(|_| true)
}

fn success(output: Output) -> io::Result<Output> {
    if output.status.success() && output.stderr.is_empty() {
        Ok(output)
    } else {
        Err(invalid(format!(
            "Git inspection failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
