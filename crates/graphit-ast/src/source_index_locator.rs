//! Read-only access to a configured local Source corpus.
//! Authorization remains with the caller; the SDK snapshot supplies the data pin.

use crate::indexed_source::{
    BranchSourceObservation, IndexedSourceError, IndexedSourceSnapshot, read_branch_source,
    source_schema,
};
use graphit_auth::ProjectSelection;
use graphit_core::artifact_store::PublicationScope;
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, project_identity::ProjectId,
};
use graphit_data::lance_branch_store::{BranchError, BranchSnapshot};
use std::{error::Error, fmt, fs, io, sync::Arc};

pub enum SourceIndexPresence<'a> {
    Absent,
    Available(Box<LocalSourceIndex<'a>>),
}

/// Retains the SDK snapshot and original request. Authorization remains with the caller.
pub struct LocalSourceIndex<'a> {
    installation: &'a PublicationScope,
    project: ProjectSelection<'a>,
    snapshot: BranchSnapshot,
    original: Arc<SnapshotContext>,
}

#[derive(Debug)]
pub enum SourceIndexError {
    InvalidScope,
    RequestMismatch,
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
            Self::UnexpectedEntry => f.write_str("Source index entry is not a directory"),
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
    installation: &PublicationScope,
    project: ProjectSelection<'_>,
    original: &SnapshotContext,
) -> Result<ProjectId, SourceIndexError> {
    original.check()?;
    if !matches!(installation.owner(), StoreOwner::Installation { .. })
        || installation.brand() != project.scope.brand()
        || project.version.scope() != project.scope.owner_root()
    {
        return Err(SourceIndexError::InvalidScope);
    }
    let project_id = match project.scope.owner() {
        StoreOwner::Project { project_id } => *project_id,
        StoreOwner::Installation { .. } | StoreOwner::User { .. } => {
            return Err(SourceIndexError::InvalidScope);
        }
    };
    Ok(project_id)
}

impl LocalSourceIndex<'_> {
    /// Borrow the actual selected branch for packaging the same Raw pin.
    pub fn snapshot(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&BranchSnapshot, SourceIndexError> {
        self.check_request(original)?;
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

    /// Check the original request and logical project selection before delivery.
    pub fn check_request(&self, original: &Arc<SnapshotContext>) -> Result<(), SourceIndexError> {
        if !Arc::ptr_eq(&self.original, original) {
            return Err(SourceIndexError::RequestMismatch);
        }
        validate_scopes(self.installation, self.project, original)?;
        original.check()?;
        Ok(())
    }

    pub async fn read(
        &self,
        path: &str,
        original: &Arc<SnapshotContext>,
    ) -> Result<IndexedSourceSnapshot, SourceIndexError> {
        self.check_request(original)?;
        let result = read_branch_source(&self.snapshot, path, original).await;
        self.check_request(original)?;
        result.map_err(SourceIndexError::Source)
    }

    /// Enumerate every literal path in this same captured generation.
    ///
    /// Keys with unstored source remain present. The result contains no bodies,
    /// capture or authority; keep this owner alive for subsequent reads and checks.
    pub async fn paths(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<Vec<String>, SourceIndexError> {
        self.check_request(original)?;
        let result = crate::indexed_source::branch_source_paths(&self.snapshot, original).await;
        self.check_request(original)?;
        result.map_err(SourceIndexError::Source)
    }
}

/// Acquire only the fixed Installation/ProjectId corpus and resolved SDK branch.
/// An observed miss is read-only. Once presence is observed, every failure stays
/// an error. No directory, dataset or branch is created or repaired.
pub async fn open_local_source_index<'a>(
    installation: &'a PublicationScope,
    project: ProjectSelection<'a>,
    original: &Arc<SnapshotContext>,
) -> Result<SourceIndexPresence<'a>, SourceIndexError> {
    validate_scopes(installation, project, original)?;
    let path = project.scope.runtime_root().join("ast/source.lance");
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(SourceIndexError::UnexpectedEntry),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SourceIndexPresence::Absent);
        }
        Err(error) => return Err(error.into()),
    }
    let encoded = project.version.lance_branch();
    let branch = if encoded.is_empty() {
        None
    } else {
        Some(encoded.as_str())
    };
    let captured = BranchSnapshot::capture_ast_corpus(&path, branch, original).await;
    validate_scopes(installation, project, original)?;
    let snapshot = captured.map_err(SourceIndexError::Operation)?;
    if snapshot.schema() != source_schema().as_ref() {
        return Err(SourceIndexError::Storage(Box::new(
            BranchError::SchemaMismatch,
        )));
    }
    let result = LocalSourceIndex {
        installation,
        project,
        snapshot,
        original: Arc::clone(original),
    };
    result.check_request(original)?;
    Ok(SourceIndexPresence::Available(Box::new(result)))
}
