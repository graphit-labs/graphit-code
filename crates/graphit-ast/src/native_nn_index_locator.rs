//! Observed NativeNN acquisition for real captured Installation/PROJECT scopes.
//!
//! The snapshot holds the local AST corpus's shared cleanup lease.
//! The caller separately authorizes and revalidates the selected project.

use arrow_schema::Schema;
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_core::{
    artifact_store::{ScopeContext, StoreOwner},
    file_io::SnapshotContext,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    project_identity::ProjectId,
};
use graphit_data::lance_branch_store::{BranchError, BranchSnapshot};
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub enum NativeNnIndexPresence<'a> {
    Absent,
    Available(Box<LocalNativeNnIndex<'a>>),
}

/// Keeps the actual directory, scopes, SDK generation and operation alive.
pub struct LocalNativeNnIndex<'a> {
    installation: ScopeCapture<'a>,
    project: ProjectCapture<'a>,
    directory: CapturedDocumentDirectory,
    snapshot: BranchSnapshot,
    original: Arc<SnapshotContext>,
    selected: PathBuf,
}

#[derive(Debug)]
pub enum NativeNnIndexError {
    InvalidScope,
    RequestMismatch,
    InvalidLocator,
    UnexpectedEntry,
    Operation(io::Error),
    Storage(Box<BranchError>),
}

impl fmt::Display for NativeNnIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScope => f.write_str("invalid NativeNN index scope"),
            Self::RequestMismatch => f.write_str("NativeNN index Original mismatch"),
            Self::InvalidLocator => f.write_str("invalid NativeNN index locator"),
            Self::UnexpectedEntry => f.write_str("NativeNN index entry is not a real directory"),
            Self::Operation(error) => write!(f, "NativeNN index operation: {error}"),
            Self::Storage(error) => write!(f, "NativeNN index storage: {error}"),
        }
    }
}

impl Error for NativeNnIndexError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Storage(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl From<io::Error> for NativeNnIndexError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}

fn validate_scopes(
    installation: ScopeCapture<'_>,
    project: ProjectCapture<'_>,
    original: &SnapshotContext,
) -> Result<ProjectId, NativeNnIndexError> {
    original.check()?;
    if !matches!(installation.live.owner(), StoreOwner::Installation { .. })
        || installation.live.brand() != project.scope.live.brand()
        || project.version.scope() != project.scope.live.owner_root()
    {
        return Err(NativeNnIndexError::InvalidScope);
    }
    let project_id = match project.scope.live.owner() {
        StoreOwner::Project { project_id } => *project_id,
        StoreOwner::Installation { .. } | StoreOwner::User { .. } => {
            return Err(NativeNnIndexError::InvalidScope);
        }
    };
    let context = ScopeContext::from_snapshot(original);
    for capture in [installation, project.scope] {
        if !capture
            .record
            .matches_scope_bounded(capture.live, &context)?
        {
            return Err(NativeNnIndexError::InvalidScope);
        }
        capture.live.revalidate_bounded(&context)?;
        original.check()?;
    }
    Ok(project_id)
}

fn observe_directory(path: &Path) -> Result<(), NativeNnIndexError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(NativeNnIndexError::UnexpectedEntry);
    }
    Ok(())
}

fn capture_directory(
    path: &Path,
    limits: NativeDocumentFileLimits,
    original: &SnapshotContext,
) -> Result<CapturedDocumentDirectory, NativeNnIndexError> {
    original.check()?;
    let directory = CapturedDocumentDirectory::capture(path, limits, original)?;
    if directory.canonical_path() != path {
        return Err(NativeNnIndexError::InvalidLocator);
    }
    original.check()?;
    Ok(directory)
}

impl LocalNativeNnIndex<'_> {
    pub fn version(&self) -> u64 {
        self.snapshot.version()
    }
    pub fn schema(&self) -> &Schema {
        self.snapshot.schema()
    }
    pub fn physical_branch(&self) -> Option<&str> {
        self.snapshot.physical_branch()
    }
    pub fn inherited_target(&self) -> Option<&str> {
        self.snapshot.inherited_target()
    }

    /// Check the retained owner; this does not reopen or observe newer HEAD.
    pub fn revalidate(&self, original: &Arc<SnapshotContext>) -> Result<(), NativeNnIndexError> {
        original.check()?;
        let result = self.revalidate_checked(original);
        original.check()?;
        result
    }
    fn revalidate_checked(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<(), NativeNnIndexError> {
        if !Arc::ptr_eq(&self.original, original) {
            return Err(NativeNnIndexError::RequestMismatch);
        }
        validate_scopes(self.installation, self.project, original)?;
        self.directory.revalidate(original)?;
        let mut path = self.installation.live.owner_root().to_path_buf();
        for component in self
            .selected
            .strip_prefix(self.installation.live.owner_root())
            .map_err(|_| NativeNnIndexError::InvalidLocator)?
            .components()
        {
            original.check()?;
            path.push(component);
            observe_directory(&path)?;
        }
        self.directory.revalidate(original)?;
        validate_scopes(self.installation, self.project, original)?;
        Ok(())
    }

    /// Lend the actual observed SDK branch while this owner remains borrowed.
    /// Existing W/Q and the caller enforce payload/model/authorization gates.
    pub fn snapshot(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&BranchSnapshot, NativeNnIndexError> {
        self.revalidate(original)?;
        Ok(&self.snapshot)
    }
}

/// Resolve this operation's corpus lineage once and observe its actual branch schema.
/// Missing directories or an SDK-confirmed missing dataset cause no effects.
/// Other storage errors, including unreadable published manifests, stay errors.
pub async fn open_local_native_nn_index<'a>(
    installation: ScopeCapture<'a>,
    project: ProjectCapture<'a>,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
) -> Result<NativeNnIndexPresence<'a>, NativeNnIndexError> {
    original.check()?;
    let result = open(installation, project, original, limits).await;
    original.check()?;
    result
}

async fn open<'a>(
    installation: ScopeCapture<'a>,
    project: ProjectCapture<'a>,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
) -> Result<NativeNnIndexPresence<'a>, NativeNnIndexError> {
    validate_scopes(installation, project, original)?;
    let relative = graphit_core::local_corpus::resolve(
        project.scope.live,
        project.version.version(),
        "ast",
        "corpus.lance",
        limits,
        original,
    )?;
    let mapped = relative != Path::new("ast").join("corpus.lance");
    let selected = project.scope.live.runtime_root().join(relative);
    let components = selected
        .strip_prefix(installation.live.owner_root())
        .map_err(|_| NativeNnIndexError::InvalidLocator)?;
    let mut path = installation.live.owner_root().to_path_buf();
    let mut parent = capture_directory(&path, limits, original)?;
    for component in components.components() {
        original.check()?;
        validate_scopes(installation, project, original)?;
        parent.revalidate(original)?;
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(NativeNnIndexError::UnexpectedEntry);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !mapped => {
                parent.revalidate(original)?;
                validate_scopes(installation, project, original)?;
                return Ok(NativeNnIndexPresence::Absent);
            }
            Err(error) => return Err(error.into()),
        }
        let child = capture_directory(&path, limits, original)?;
        parent.revalidate(original)?;
        validate_scopes(installation, project, original)?;
        parent = child;
    }
    let encoded = project.version.lance_branch();
    let branch = if encoded.is_empty() {
        None
    } else {
        Some(encoded.as_str())
    };
    parent.revalidate(original)?;
    validate_scopes(installation, project, original)?;
    path.to_str().ok_or(NativeNnIndexError::InvalidLocator)?;
    let observed = BranchSnapshot::capture_ast_corpus(&path, branch, original).await;
    parent.revalidate(original)?;
    validate_scopes(installation, project, original)?;
    let snapshot = match observed {
        Ok(snapshot) => snapshot,
        Err(error) => match error.downcast::<BranchError>() {
            Ok(BranchError::Sdk(cause))
                if !mapped
                    && matches!(cause.as_ref(), lance::Error::DatasetNotFound { .. })
                    && !cause.is_commit_status_unknown() =>
            {
                return Ok(NativeNnIndexPresence::Absent);
            }
            Ok(error) => return Err(NativeNnIndexError::Storage(Box::new(error))),
            Err(error) => return Err(error.into()),
        },
    };
    crate::corpus::validate_schema(snapshot.schema())?;
    let index = LocalNativeNnIndex {
        installation,
        project,
        directory: parent,
        snapshot,
        original: Arc::clone(original),
        selected,
    };
    index.revalidate(original)?;
    Ok(NativeNnIndexPresence::Available(Box::new(index)))
}
