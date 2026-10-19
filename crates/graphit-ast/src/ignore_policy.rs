//! AST defaults derived from captured configuration, for the existing Core matcher.
//!
//! This is policy data, without a walker, filesystem capture or authorization grant.

use graphit_core::{
    config::{ConfigKey, ConfigSnapshot},
    file_io::SnapshotContext,
};
use std::{
    error::Error,
    fmt, io,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct AstIgnorePolicy {
    defaults: Vec<String>,
}

#[derive(Debug)]
pub enum AstIgnorePolicyError {
    Operation(io::Error),
    Setting { key: &'static str, cause: io::Error },
    DocsDirectory(io::Error),
}

impl fmt::Display for AstIgnorePolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => write!(formatter, "AST ignore operation: {error}"),
            Self::Setting { key, cause } => write!(formatter, "AST ignore setting {key}: {cause}"),
            Self::DocsDirectory(error) => write!(formatter, "AST docs directory: {error}"),
        }
    }
}

impl Error for AstIgnorePolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Operation(error) | Self::DocsDirectory(error) => error,
            Self::Setting { cause, .. } => cause,
        })
    }
}

impl AstIgnorePolicy {
    pub fn from_config(
        snapshot: &ConfigSnapshot,
        original: &SnapshotContext,
    ) -> Result<Self, AstIgnorePolicyError> {
        check(original)?;
        let result = Self::capture_checked(snapshot, original);
        check(original)?;
        result
    }

    fn capture_checked(
        snapshot: &ConfigSnapshot,
        original: &SnapshotContext,
    ) -> Result<Self, AstIgnorePolicyError> {
        let mut defaults = vec![
            ".git/".to_owned(),
            format!("{}/", snapshot.brand().directory()),
        ];
        let index_key = "ast.index_docs";
        check(original)?;
        let key = ConfigKey::new(index_key).map_err(|cause| AstIgnorePolicyError::Setting {
            key: index_key,
            cause,
        })?;
        let index_docs =
            snapshot
                .get::<bool>(&key)
                .map_err(|cause| AstIgnorePolicyError::Setting {
                    key: index_key,
                    cause,
                })?;
        check(original)?;
        if !index_docs.unwrap_or(false) {
            let docs_key = "knowledge.docs_dir";
            let key = ConfigKey::new(docs_key).map_err(|cause| AstIgnorePolicyError::Setting {
                key: docs_key,
                cause,
            })?;
            check(original)?;
            let directory =
                snapshot
                    .get::<&str>(&key)
                    .map_err(|cause| AstIgnorePolicyError::Setting {
                        key: docs_key,
                        cause,
                    })?;
            check(original)?;
            if let Some(pattern) = docs_pattern(directory.unwrap_or("docs"), original)? {
                defaults.push(pattern);
            }
        }
        check(original)?;
        Ok(Self { defaults })
    }

    pub fn filename(&self) -> &'static str {
        ".astignore"
    }

    pub fn defaults(&self) -> &[String] {
        &self.defaults
    }
}

fn check(original: &SnapshotContext) -> Result<(), AstIgnorePolicyError> {
    original.check().map_err(AstIgnorePolicyError::Operation)
}

fn invalid_directory(message: &'static str) -> AstIgnorePolicyError {
    AstIgnorePolicyError::DocsDirectory(io::Error::new(io::ErrorKind::InvalidInput, message))
}

fn docs_pattern(
    configured: &str,
    original: &SnapshotContext,
) -> Result<Option<String>, AstIgnorePolicyError> {
    check(original)?;
    if configured
        .chars()
        .any(|character| matches!(character, '\0' | '\r' | '\n'))
    {
        return Err(invalid_directory(
            "directory contains an invalid control character",
        ));
    }
    let directory = configured.trim();
    if directory.is_empty() {
        return Err(invalid_directory("directory is empty"));
    }
    let mut resolved = PathBuf::new();
    for component in Path::new(directory).components() {
        check(original)?;
        match component {
            Component::Prefix(_) | Component::RootDir => return Ok(None),
            Component::CurDir => {}
            Component::ParentDir => {
                if !resolved.pop() {
                    return Ok(None);
                }
            }
            Component::Normal(name) => resolved.push(name),
        }
    }
    if resolved.as_os_str().is_empty() {
        return Ok(None);
    }
    let mut names = Vec::new();
    for component in resolved.components() {
        check(original)?;
        if let Component::Normal(name) = component {
            names.push(
                name.to_str()
                    .ok_or_else(|| invalid_directory("directory is not UTF-8"))?,
            );
        }
    }
    let literal = names.join("/");
    // globset escapes glob metacharacters; the Unix Core matcher also treats
    // backslash as a quote. Preserve a literal native backslash before that SDK.
    let quoted = if cfg!(windows) {
        literal
    } else {
        literal.replace('\\', "\\\\")
    };
    let pattern = format!("/{}/", globset::escape(&quoted));
    check(original)?;
    Ok(Some(pattern))
}
