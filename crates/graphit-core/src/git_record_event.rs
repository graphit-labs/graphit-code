//! Commit facts for record snapshots, obtained from Git rather than hook input.

use crate::{
    file_lock::Cancellation,
    git_checkout::{GitCheckout, git_command},
    process::Budget,
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

/// Git's post-operation notifications. Values are untrusted until checked
/// against the repository by the receiving product operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GitRecordEvent {
    Commit {},
    Checkout {
        old: String,
        new: String,
        branch: bool,
    },
    Merge {
        squash: bool,
    },
    Rewrite {
        pairs: Vec<GitRewrite>,
    },
    ReferenceTransaction {
        updates: Vec<GitRefUpdate>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitRefUpdate {
    pub old: String,
    pub new: String,
    pub reference: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitRewrite {
    pub old: String,
    pub new: String,
}

/// Validated facts captured from the checkout. No supplied hook arguments can
/// select another repository, override its HEAD, or supply commit parents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRecordFacts {
    pub root: PathBuf,
    pub common_dir: PathBuf,
    pub head: Option<String>,
    pub parents: Vec<String>,
    /// Original commit being replayed when Git exposes CHERRY_PICK_HEAD.
    pub replayed_commit: Option<String>,
    pub branch: Option<String>,
    pub default_branch: String,
}

impl GitRecordFacts {
    /// An unborn repository has no HEAD; a non-Git project returns `None`.
    pub fn inspect(
        directory: impl AsRef<Path>,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<Option<Self>> {
        let budget = Budget::new(timeout, cancellation)?;
        let directory = directory.as_ref();
        let checkout = GitCheckout::inspect(directory, budget.remaining(), cancellation)?;
        let (branch, default_branch) = match checkout {
            GitCheckout::NonGit => return Ok(None),
            GitCheckout::Branch { name, default } => (Some(name), default),
            GitCheckout::Detached { default, .. } => (None, default),
        };
        let root = git_path(directory, "--show-toplevel", &budget)?;
        let common_dir = git_path(&root, "--git-common-dir", &budget)?;
        let head = optional_commit(&root, "HEAD", &budget)?;
        let parents = match &head {
            Some(head) => commit_parents(&root, head, &budget)?,
            None => Vec::new(),
        };
        let replayed_commit = optional_commit(&root, "CHERRY_PICK_HEAD", &budget)?;
        budget.check()?;
        Ok(Some(Self {
            root,
            common_dir,
            head,
            parents,
            replayed_commit,
            branch,
            default_branch,
        }))
    }

    /// Resolve only a full object ID, never arbitrary revision expressions.
    pub fn resolve_commit(
        &self,
        commit: &str,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<String> {
        validate_commit_id(commit)?;
        let budget = Budget::new(timeout, cancellation)?;
        optional_commit(&self.root, commit, &budget)?
            .ok_or_else(|| invalid("Git commit does not exist"))
    }

    /// Inspect parents of a full, actual commit ID without accepting arbitrary
    /// revision expressions supplied by a hook or caller.
    pub fn parents_of(
        &self,
        commit: &str,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<Vec<String>> {
        validate_commit_id(commit)?;
        commit_parents(&self.root, commit, &Budget::new(timeout, cancellation)?)
    }

    /// Git records the previous tip for merge, including fast-forward merges.
    pub fn previous_head(
        &self,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<Option<String>> {
        optional_commit(
            &self.root,
            "ORIG_HEAD",
            &Budget::new(timeout, cancellation)?,
        )
    }

    /// Return the unique merge base. Criss-cross histories with multiple bases
    /// must not silently choose one and lose record changes.
    pub fn merge_base(
        &self,
        left: &str,
        right: &str,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<Option<String>> {
        validate_commit_id(left)?;
        validate_commit_id(right)?;
        let budget = Budget::new(timeout, cancellation)?;
        let output = budget.run(&mut git_command(
            &self.root,
            &["merge-base", "--all", left, right],
        ))?;
        if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
            return Ok(None);
        }
        let text = output_text(output)?;
        let mut bases = text.lines();
        let base = bases
            .next()
            .ok_or_else(|| invalid("Git returned no merge base"))?;
        validate_commit_id(base)?;
        if bases.next().is_some() {
            return Err(invalid(
                "Git record merge requires a unique common ancestor",
            ));
        }
        Ok(Some(base.to_owned()))
    }
}

/// SHA-1 and SHA-256 repositories are supported. Git emits lowercase IDs.
pub fn validate_commit_id(value: &str) -> io::Result<()> {
    if matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a full lowercase Git commit object ID",
        ))
    }
}

fn optional_commit(
    root: &Path,
    reference: &str,
    budget: &Budget<'_>,
) -> io::Result<Option<String>> {
    let expression = format!("{reference}^{{commit}}");
    let output = budget.run(&mut git_command(
        root,
        &["rev-parse", "--verify", "--quiet", &expression],
    ))?;
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(None);
    }
    let value = output_text(output)?;
    validate_commit_id(&value)?;
    Ok(Some(value))
}

fn commit_parents(root: &Path, head: &str, budget: &Budget<'_>) -> io::Result<Vec<String>> {
    let text = output_text(budget.run(&mut git_command(
        root,
        &["rev-list", "--parents", "-n", "1", head],
    ))?)?;
    let mut parts = text.split_whitespace();
    if parts.next() != Some(head) {
        return Err(invalid("Git returned parents for another commit"));
    }
    parts
        .map(|parent| {
            validate_commit_id(parent)?;
            Ok(parent.to_owned())
        })
        .collect()
}

fn git_path(root: &Path, selector: &str, budget: &Budget<'_>) -> io::Result<PathBuf> {
    let value = output_text(budget.run(&mut git_command(
        root,
        &["rev-parse", "--path-format=absolute", selector],
    ))?)?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(invalid("Git returned a non-absolute repository path"));
    }
    std::fs::canonicalize(path)
}

fn output_text(output: Output) -> io::Result<String> {
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(invalid(format!(
            "Git record inspection failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|_| invalid("Git returned a non-Unicode value"))?;
    let text = if cfg!(windows) {
        value
            .strip_suffix("\r\n")
            .or_else(|| value.strip_suffix('\n'))
            .unwrap_or(&value)
    } else {
        value.strip_suffix('\n').unwrap_or(&value)
    };
    Ok(text.to_owned())
}

fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
