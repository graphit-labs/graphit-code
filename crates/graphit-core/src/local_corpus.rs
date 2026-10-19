//! Canonical local datasets. Project versions select native Lance branches.
//! Resolution validates the admitted PROJECT path without creating state.

use crate::artifact_store::CapturedPublicationScope;
use crate::file_io::SnapshotContext;
use crate::native_document_file::{CapturedProjectDocumentDirectory, NativeDocumentFileLimits};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Resolve a local dataset once for one operation, using its admitted PROJECT
/// scope and Original. Reuse the returned relative path for URI, read guards and
/// first publication. Select S3 before calling this local-only resolver.
///
/// `module` is one of `ast`, `knowledge`, `memory`, `task`; `dataset` is a single
/// normal filename component. `project_version` must already be normalized and
/// nonempty. All versions use `<module>/<dataset>`; the store selects its native
/// branch or retained snapshot separately. No sidecar or checkout path selects
/// a second dataset.
///
/// Invalid arguments return `InvalidInput`; invalid paths retain their concrete
/// validation errors. Original,
/// physical-directory, document and path-limit errors retain their IO errors.
pub fn resolve(
    scope: &CapturedPublicationScope,
    project_version: &str,
    module: &str,
    dataset: &str,
    limits: NativeDocumentFileLimits,
    request: &Arc<SnapshotContext>,
) -> io::Result<PathBuf> {
    request.check()?;
    scope.require_project()?;
    if !valid_module(module) || !normalized_version(project_version) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local corpus requires a known module and normalized project version",
        ));
    }
    crate::file_io::validate_relative(Path::new(dataset), false)?;
    if dataset.contains(['/', '\\']) || Path::new(dataset).components().count() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local corpus dataset must be one filename component",
        ));
    }
    let relative = Path::new(module).join(dataset);
    validate_selection(scope, &relative, limits, request)?;
    request.check()?;
    Ok(relative)
}

fn valid_module(module: &str) -> bool {
    matches!(module, "ast" | "knowledge" | "memory" | "task")
}

fn normalized_version(version: &str) -> bool {
    !version.is_empty() && crate::project_version::validate(version).is_ok_and(|v| v == version)
}

fn validate_selection(
    scope: &CapturedPublicationScope,
    relative: &Path,
    limits: NativeDocumentFileLimits,
    request: &Arc<SnapshotContext>,
) -> io::Result<()> {
    let selection = scope.runtime_selection(relative)?;
    CapturedProjectDocumentDirectory::validate_runtime_capture_path(
        scope, &selection, limits, request,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
