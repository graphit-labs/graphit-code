//! NativeNN snapshots selected by logical Installation/PROJECT scope.
//! Filesystem paths follow ordinary platform semantics, including links and mounts.

use arrow_schema::Schema;
use graphit_auth::ProjectSelection;
use graphit_core::artifact_store::PublicationScope;
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, project_identity::ProjectId,
};
use graphit_data::lance_branch_store::{BranchError, BranchSnapshot};
use std::{error::Error, fmt, fs, io, sync::Arc};

pub enum NativeNnIndexPresence<'a> {
    Absent,
    Available(Box<LocalNativeNnIndex<'a>>),
}

/// Keeps the selected SDK snapshot and original operation alive.
pub struct LocalNativeNnIndex<'a> {
    installation: &'a PublicationScope,
    project: ProjectSelection<'a>,
    snapshot: BranchSnapshot,
    original: Arc<SnapshotContext>,
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
            Self::UnexpectedEntry => f.write_str("NativeNN index entry is not a directory"),
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
    installation: &PublicationScope,
    project: ProjectSelection<'_>,
    original: &SnapshotContext,
) -> Result<ProjectId, NativeNnIndexError> {
    original.check()?;
    if !matches!(installation.owner(), StoreOwner::Installation { .. })
        || installation.brand() != project.scope.brand()
        || project.version.scope() != project.scope.owner_root()
    {
        return Err(NativeNnIndexError::InvalidScope);
    }
    let project_id = match project.scope.owner() {
        StoreOwner::Project { project_id } => *project_id,
        StoreOwner::Installation { .. } | StoreOwner::User { .. } => {
            return Err(NativeNnIndexError::InvalidScope);
        }
    };
    Ok(project_id)
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

    /// Check this request without observing a newer SDK snapshot.
    pub fn check_request(&self, original: &Arc<SnapshotContext>) -> Result<(), NativeNnIndexError> {
        if !Arc::ptr_eq(&self.original, original) {
            return Err(NativeNnIndexError::RequestMismatch);
        }
        validate_scopes(self.installation, self.project, original)?;
        original.check()?;
        Ok(())
    }

    /// Lend the actual observed SDK branch while this owner remains borrowed.
    /// Existing W/Q and the caller enforce payload/model/authorization gates.
    pub fn snapshot(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&BranchSnapshot, NativeNnIndexError> {
        self.check_request(original)?;
        Ok(&self.snapshot)
    }
}

/// Resolve this operation's corpus lineage once and observe its actual branch schema.
/// Missing directories or an SDK-confirmed missing dataset cause no effects.
/// Other storage errors, including unreadable published manifests, stay errors.
pub async fn open_local_native_nn_index<'a>(
    installation: &'a PublicationScope,
    project: ProjectSelection<'a>,
    original: &Arc<SnapshotContext>,
) -> Result<NativeNnIndexPresence<'a>, NativeNnIndexError> {
    original.check()?;
    let result = open(installation, project, original).await;
    original.check()?;
    result
}

async fn open<'a>(
    installation: &'a PublicationScope,
    project: ProjectSelection<'a>,
    original: &Arc<SnapshotContext>,
) -> Result<NativeNnIndexPresence<'a>, NativeNnIndexError> {
    validate_scopes(installation, project, original)?;
    let path = project.scope.runtime_root().join("ast/corpus.lance");
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(NativeNnIndexError::UnexpectedEntry),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(NativeNnIndexPresence::Absent);
        }
        Err(error) => return Err(error.into()),
    }
    let encoded = project.version.lance_branch();
    let branch = if encoded.is_empty() {
        None
    } else {
        Some(encoded.as_str())
    };
    validate_scopes(installation, project, original)?;
    path.to_str().ok_or(NativeNnIndexError::InvalidLocator)?;
    let observed = BranchSnapshot::capture_ast_corpus(&path, branch, original).await;
    validate_scopes(installation, project, original)?;
    let snapshot = match observed {
        Ok(snapshot) => snapshot,
        Err(error) => match error.downcast::<BranchError>() {
            Ok(BranchError::Sdk(cause))
                if matches!(cause.as_ref(), lance::Error::DatasetNotFound { .. })
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
        snapshot,
        original: Arc::clone(original),
    };
    index.check_request(original)?;
    Ok(NativeNnIndexPresence::Available(Box::new(index)))
}
