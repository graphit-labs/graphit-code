//! Read-only acquisition of the global raw Source corpus for captured scopes.
//!
//! The caller authorizes acquisition and delivery. Scope captures are observations,
//! not grants. Pathname SDK IO is bracketed by native revalidation; this does not
//! promise continuous confinement or a pin against external dataset GC.

use crate::indexed_source::{
    BranchSourceObservation, IndexedSourceError, IndexedSourceSnapshot, read_branch_source,
    source_schema,
};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_core::{
    artifact_store::{ScopeContext, StoreOwner},
    file_io::SnapshotContext,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentDirectoryIdentity, NativeDocumentFileLimits,
    },
    project_identity::ProjectId,
};
use graphit_data::lance_branch_store::{BranchError, BranchSnapshot, LanceBranchStore};
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub enum SourceIndexPresence<'a> {
    Absent,
    Available(Box<LocalSourceIndex<'a>>),
}

/// Retains the captured generation, native directory and original request.
/// Authorization and revalidation before delivery remain the caller's obligation.
pub struct LocalSourceIndex<'a> {
    installation: ScopeCapture<'a>,
    project: ProjectCapture<'a>,
    directory: CapturedDocumentDirectory,
    snapshot: BranchSnapshot,
    original: Arc<SnapshotContext>,
    selected: PathBuf,
}

#[derive(Debug)]
pub enum SourceIndexError {
    InvalidScope,
    RequestMismatch,
    InvalidLocator,
    UnexpectedEntry,
    Operation(io::Error),
    Storage(Box<BranchError>),
    Source(IndexedSourceError),
}

impl fmt::Display for SourceIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScope => f.write_str("invalid Source index scope"),
            Self::RequestMismatch => f.write_str("Source index request mismatch"),
            Self::InvalidLocator => f.write_str("invalid Source index locator"),
            Self::UnexpectedEntry => f.write_str("Source index entry is not a real directory"),
            Self::Operation(error) => write!(f, "Source index operation: {error}"),
            Self::Storage(error) => write!(f, "Source index storage: {error}"),
            Self::Source(error) => write!(f, "Source index read: {error}"),
        }
    }
}

impl Error for SourceIndexError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Storage(error) => Some(error.as_ref()),
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for SourceIndexError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}

fn validate_scopes(
    installation: ScopeCapture<'_>,
    project: ProjectCapture<'_>,
    original: &SnapshotContext,
) -> Result<ProjectId, SourceIndexError> {
    original.check()?;
    if !matches!(installation.live.owner(), StoreOwner::Installation { .. })
        || installation.live.brand() != project.scope.live.brand()
        || project.version.scope() != project.scope.live.owner_root()
    {
        return Err(SourceIndexError::InvalidScope);
    }
    let project_id = match project.scope.live.owner() {
        StoreOwner::Project { project_id } => *project_id,
        StoreOwner::Installation { .. } | StoreOwner::User { .. } => {
            return Err(SourceIndexError::InvalidScope);
        }
    };
    let context = ScopeContext::from_snapshot(original);
    for capture in [installation, project.scope] {
        if !capture
            .record
            .matches_scope_bounded(capture.live, &context)?
        {
            return Err(SourceIndexError::InvalidScope);
        }
        capture.live.revalidate_bounded(&context)?;
        original.check()?;
    }
    Ok(project_id)
}

fn observe_directory(path: &Path) -> Result<(), SourceIndexError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SourceIndexError::UnexpectedEntry);
    }
    Ok(())
}

fn capture_directory(
    path: &Path,
    limits: NativeDocumentFileLimits,
    original: &SnapshotContext,
) -> Result<CapturedDocumentDirectory, SourceIndexError> {
    original.check()?;
    let captured = CapturedDocumentDirectory::capture(path, limits, original)?;
    if captured.canonical_path() != path {
        return Err(SourceIndexError::InvalidLocator);
    }
    original.check()?;
    Ok(captured)
}

impl LocalSourceIndex<'_> {
    /// Historical identity of the actual retained Raw directory; no I/O or grant.
    /// Revalidate this owner around reads. Dataset version/branch remain separate pins;
    /// this identity detects folder replacement, not deliberate edits in place.
    pub fn directory_identity(&self) -> NativeDocumentDirectoryIdentity {
        self.directory.directory_identity()
    }

    /// Borrow the actual selected branch for packaging the same Raw pin.
    pub fn snapshot(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&BranchSnapshot, SourceIndexError> {
        self.revalidate(original)?;
        Ok(&self.snapshot)
    }
    pub fn version(&self) -> u64 {
        self.snapshot.version()
    }

    pub fn branch_observation(&self) -> BranchSourceObservation<'_> {
        BranchSourceObservation {
            physical_branch: self.snapshot.physical_branch(),
            inherited_target: self.snapshot.inherited_target(),
        }
    }

    /// Check the original request, real scopes and captured directory at delivery.
    pub fn revalidate(&self, original: &Arc<SnapshotContext>) -> Result<(), SourceIndexError> {
        if !Arc::ptr_eq(&self.original, original) {
            return Err(SourceIndexError::RequestMismatch);
        }
        validate_scopes(self.installation, self.project, original)?;
        self.directory.revalidate(original)?;
        let mut path = self.installation.live.owner_root().to_path_buf();
        for component in self
            .selected
            .strip_prefix(self.installation.live.owner_root())
            .map_err(|_| SourceIndexError::InvalidLocator)?
            .components()
        {
            original.check()?;
            path.push(component);
            observe_directory(&path)?;
        }
        self.directory.revalidate(original)?;
        validate_scopes(self.installation, self.project, original)?;
        original.check()?;
        Ok(())
    }

    pub async fn read(
        &self,
        path: &str,
        original: &Arc<SnapshotContext>,
    ) -> Result<IndexedSourceSnapshot, SourceIndexError> {
        self.revalidate(original)?;
        let result = read_branch_source(&self.snapshot, path, original).await;
        self.revalidate(original)?;
        result.map_err(SourceIndexError::Source)
    }

    /// Read the physical Raw pin belonging to an already published FILE. Other
    /// FILE commits may have advanced the branch without changing this entity.
    pub async fn read_version(
        &self,
        path: &str,
        version: u64,
        original: &Arc<SnapshotContext>,
    ) -> Result<IndexedSourceSnapshot, SourceIndexError> {
        self.revalidate(original)?;
        if version == 0 || version > self.snapshot.version() {
            return Err(SourceIndexError::InvalidLocator);
        }
        let captured = self
            .snapshot
            .at_version(version)
            .await
            .map_err(|cause| SourceIndexError::Operation(io::Error::other(cause)))?;
        self.revalidate(original)?;
        let source = crate::indexed_source::read_branch_source(&captured, path, original)
            .await
            .map_err(SourceIndexError::Source)?;
        self.revalidate(original)?;
        Ok(source)
    }

    /// Enumerate every literal path in this same captured generation.
    ///
    /// Keys with unstored source remain present. The result contains no bodies,
    /// capture or authority; keep this owner alive for subsequent reads and checks.
    pub async fn paths(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<Vec<String>, SourceIndexError> {
        self.revalidate(original)?;
        let result = crate::indexed_source::branch_source_paths(&self.snapshot, original).await;
        self.revalidate(original)?;
        result.map_err(SourceIndexError::Source)
    }
}

/// Acquire only the fixed Installation/ProjectId corpus and resolved SDK branch.
/// An observed miss is read-only. Once presence is observed, every failure stays
/// an error. No directory, dataset or branch is created or repaired.
pub async fn open_local_source_index<'a>(
    installation: ScopeCapture<'a>,
    project: ProjectCapture<'a>,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
) -> Result<SourceIndexPresence<'a>, SourceIndexError> {
    validate_scopes(installation, project, original)?;
    let relative = graphit_core::local_corpus::resolve(
        project.scope.live,
        project.version.version(),
        "ast",
        "source.lance",
        limits,
        original,
    )?;
    let mapped = relative != Path::new("ast").join("source.lance");
    let selected = project.scope.live.runtime_root().join(relative);
    let components = selected
        .strip_prefix(installation.live.owner_root())
        .map_err(|_| SourceIndexError::InvalidLocator)?;
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
                    return Err(SourceIndexError::UnexpectedEntry);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !mapped => {
                parent.revalidate(original)?;
                validate_scopes(installation, project, original)?;
                original.check()?;
                return Ok(SourceIndexPresence::Absent);
            }
            Err(error) => return Err(error.into()),
        }
        let child = capture_directory(&path, limits, original)?;
        parent.revalidate(original)?;
        validate_scopes(installation, project, original)?;
        parent = child;
    }
    let uri = path.to_str().ok_or(SourceIndexError::InvalidLocator)?;
    parent.revalidate(original)?;
    validate_scopes(installation, project, original)?;
    let opened = LanceBranchStore::open(uri, source_schema(), None).await;
    parent.revalidate(original)?;
    validate_scopes(installation, project, original)?;
    let store = opened.map_err(|error| SourceIndexError::Storage(Box::new(error)))?;
    let encoded = project.version.lance_branch();
    let branch = if encoded.is_empty() {
        None
    } else {
        Some(encoded.as_str())
    };
    let captured = store.capture_target(branch).await;
    parent.revalidate(original)?;
    validate_scopes(installation, project, original)?;
    let snapshot = captured.map_err(|error| SourceIndexError::Storage(Box::new(error)))?;
    let result = LocalSourceIndex {
        installation,
        project,
        directory: parent,
        snapshot,
        original: Arc::clone(original),
        selected,
    };
    result.revalidate(original)?;
    Ok(SourceIndexPresence::Available(Box::new(result)))
}
