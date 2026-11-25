//! Canonical scope discovery shared by identity consumers and version pins.

use crate::brand::Brand;
use crate::local_files::canonical_directory;
use crate::project_identity::read_identity;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Find the nearest valid project identity or Git marker, without creating state.
/// Corrupt identity markers fail closed; they never select an enclosing project.
/// Persistent consumers must additionally reject their installation directory.
pub fn resolve_project_root(directory: impl AsRef<Path>, brand: &Brand) -> io::Result<PathBuf> {
    let directory = canonical_directory(directory.as_ref())?;
    discover_from_canonical(directory, brand)
}

/// Internal entry point for callers that have already canonicalized and guarded the input.
pub(crate) fn discover_from_canonical(directory: PathBuf, brand: &Brand) -> io::Result<PathBuf> {
    for ancestor in directory.ancestors() {
        if read_identity(ancestor, brand)?.is_some() {
            return Ok(ancestor.to_owned());
        }
        match fs::symlink_metadata(ancestor.join(".git")) {
            Ok(_) => {
                return Ok(ancestor.to_owned());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(directory)
}
