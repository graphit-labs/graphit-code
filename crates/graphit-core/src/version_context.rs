//! Per-call version selection from checkout facts, without shared overrides.
//! Git reference inspection is a separate producer of `Checkout`.

use crate::project_paths::ProjectPaths;
use crate::project_version::{DEFAULT_NAME, lance_branch_name, validate};
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub enum Checkout<'a> {
    NonGit,
    Branch { name: &'a str, default: &'a str },
    Detached { commit: &'a str, default: &'a str },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VersionSource {
    Explicit,
    Pin,
    Checkout,
    DetachedCommit,
    Default,
}

/// A validated pin captured once for the initial project scope.
#[derive(Clone, Debug)]
pub struct ScopedPin {
    scope: ProjectPaths,
    version: String,
}

impl ScopedPin {
    pub fn new(project: &ProjectPaths, version: &str) -> io::Result<Self> {
        let version = required(version)?.to_owned();
        Ok(Self {
            scope: project.clone(),
            version,
        })
    }
}

/// An owned selection passed consistently to every consumer in one call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionContext {
    scope: ProjectPaths,
    version: String,
    default: String,
    source: VersionSource,
    git: bool,
    detached: bool,
}

impl VersionContext {
    pub fn select(
        project: &ProjectPaths,
        checkout: Checkout<'_>,
        explicit: &str,
        pin: Option<&ScopedPin>,
        detached_base: &str,
    ) -> io::Result<Self> {
        let explicit = normalize(explicit)?;
        // Kept as an input of callers that also bootstrap non-Git projects; a
        // detached checkout always derives its identity from the actual commit.
        normalize(detached_base)?;
        let detached_version;
        let (default, branch, git, detached) = match checkout {
            Checkout::NonGit => (DEFAULT_NAME, None, false, false),
            Checkout::Branch { name, default } => {
                (required(default)?, Some(required(name)?), true, false)
            }
            Checkout::Detached { commit, default } => {
                crate::git_record_event::validate_commit_id(commit)?;
                detached_version = format!("git:{commit}");
                (
                    required(default)?,
                    Some(detached_version.as_str()),
                    true,
                    true,
                )
            }
        };
        let scope = project.clone();
        let scoped_pin = pin.filter(|pin| pin.scope.same_scope(&scope));
        let (version, source) = if !explicit.is_empty() {
            (explicit, VersionSource::Explicit)
        } else if let Some(pin) = scoped_pin {
            (pin.version.as_str(), VersionSource::Pin)
        } else if let Some(branch) = branch {
            (
                branch,
                if detached {
                    VersionSource::DetachedCommit
                } else {
                    VersionSource::Checkout
                },
            )
        } else {
            (default, VersionSource::Default)
        };
        Ok(Self {
            scope,
            version: version.to_owned(),
            default: default.to_owned(),
            source,
            git,
            detached,
        })
    }

    pub(crate) fn belongs_to(&self, project: &ProjectPaths) -> bool {
        self.scope.same_scope(project)
    }

    pub fn scope(&self) -> &Path {
        self.scope.root()
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn default_version(&self) -> &str {
        &self.default
    }

    pub fn source(&self) -> VersionSource {
        self.source
    }

    pub fn is_git(&self) -> bool {
        self.git
    }

    pub fn is_detached(&self) -> bool {
        self.detached
    }

    pub fn lance_branch(&self) -> String {
        lance_branch_name(&self.version, &self.default)
    }
}

fn normalize(version: &str) -> io::Result<&str> {
    validate(version).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

fn required(version: &str) -> io::Result<&str> {
    let version = normalize(version)?;
    if version.is_empty() {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "version must not be empty",
        ))
    } else {
        Ok(version)
    }
}
