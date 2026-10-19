//! Authenticated native Task creation and owned reads using the Lance SDK.
#![doc = include_str!("../../../docs/specs/native_task_store.md")]
mod catalogue;
mod embeddings;
pub use embeddings::*;
mod comments;
mod get;
mod get_model;
pub(crate) mod git;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod lexical_index;
mod lexical_model;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod lexical_search;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod lifecycle;
mod model;
mod native;
mod observation;
mod public_search;
mod query;
mod query_model;
mod read;
mod references;
mod rows;
mod session_get;
mod session_get_model;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod specification;
#[cfg(all(test, any(target_os = "linux", target_os = "macos", windows)))]
#[path = "../tests/task_store/fixture.rs"]
mod test_fixture;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod write;
use crate::task_spec::TaskSpecError;
pub use get_model::{
    TaskCommentsPage, TaskDetailCursor, TaskDetailPageInput, TaskGetInput, TaskGetRead,
    TaskHistoryPage,
};
use graphit_auth::AuthError;
use graphit_core::{
    artifact_store::{ArtifactError, ArtifactLimits},
    native_document_file::NativeDocumentFileLimits,
};
use graphit_data::{
    lance_branch_store::{BranchError, DeltaCommitFailure},
    lance_module_store::StoreError,
};
pub use lexical_model::{
    TaskLexicalHit, TaskLexicalMatchLocation, TaskLexicalQuery, TaskLexicalSearchRead,
    TaskSearchCursor, TaskSearchFilter, TaskSearchMode,
};
pub use model::*;
pub use query_model::*;
pub use references::{
    TaskReference, TaskReferenceBacklinksCursor, TaskReferenceBacklinksPage,
    TaskReferenceBacklinksQuery, TaskReferenceBacklinksRead, TaskReferenceEdge, TaskReferenceKind,
    TaskReferencePage, TaskReferenceProvenance, TaskReferenceRecord, TaskReferenceResolution,
    TaskReferenceScope, TaskReferenceTarget, TaskReferenceUpdate, TaskReferencesCoverage,
    TaskReferencesCursor, TaskReferencesInputLimits, TaskReferencesQuery, TaskReferencesRead,
    TaskReferencesWarning,
};
pub use session_get_model::{
    SelectedSessionRead, SessionDetailCursor, SessionDetailPageInput, SessionGetInput,
    SessionHistoryPage,
};
use std::{error::Error, fmt, io, num::NonZeroU32};

#[derive(Clone, Copy)]
pub struct TaskStoreReadLimits {
    pub native: NativeDocumentFileLimits,
    pub batch_rows: NonZeroU32,
}
#[derive(Clone, Copy)]
pub struct TaskStoreWriteLimits {
    pub read: TaskStoreReadLimits,
    pub artifacts: ArtifactLimits,
    pub references: TaskReferencesInputLimits,
}
/// Stateless service. Every operation requires a fresh original admission.
pub struct NativeTaskStore;
#[derive(Debug)]
pub enum TaskStoreError {
    Auth(AuthError),
    Artifact(ArtifactError),
    Request(io::Error),
    Arrow(arrow_schema::ArrowError),
    Json(serde_json::Error),
    Specification(TaskSpecError),
    Reference(crate::reference_projection::ReferenceProjectionError),
    Gate(crate::gates::GateError),
    Store(StoreError),
    Branch(BranchError),
    Commit(DeltaCommitFailure),
    WriterAcquisition(tokio::task::JoinError),
    Invalid(&'static str),
    DifferentRequest,
    WrongAdmission,
    IncompleteCorpus,
    IdempotencyConflict,
    MissingSession,
    MissingTask,
    InvalidCursor,
    AlreadyClaimed,
    LeaseExpired,
    FenceMismatch,
    RevisionConflict,
    CountsUnavailable,
    Random(getrandom::Error),
    VectorDistance(graphit_data::vector_distance::VectorDistanceError),
    Confirmed {
        version: u64,
        cause: Box<TaskStoreError>,
    },
    LexicalIndexConfirmed {
        publication: Box<TaskLexicalIndexPublication>,
        cause: Box<TaskStoreError>,
    },
}
impl fmt::Display for TaskStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Task store: {self:?}")
    }
}
impl Error for TaskStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Random(e) => Some(e),
            Self::VectorDistance(e) => Some(e),
            Self::Auth(e) => Some(e),
            Self::Artifact(e) => Some(e),
            Self::Request(e) => Some(e),
            Self::Arrow(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Specification(e) => Some(e),
            Self::Reference(e) => Some(e),
            Self::Gate(e) => Some(e),
            Self::Store(e) => Some(e),
            Self::Branch(e) => Some(e),
            Self::Commit(e) => Some(e),
            Self::WriterAcquisition(e) => Some(e),
            Self::Confirmed { cause, .. } => Some(cause.as_ref()),
            Self::LexicalIndexConfirmed { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
macro_rules! error_from {
    ($variant:ident, $type:ty) => {
        impl From<$type> for TaskStoreError {
            fn from(e: $type) -> Self {
                Self::$variant(e)
            }
        }
    };
}
error_from!(Auth, AuthError);
error_from!(Artifact, ArtifactError);
error_from!(Request, io::Error);
error_from!(Arrow, arrow_schema::ArrowError);
error_from!(Json, serde_json::Error);
error_from!(Specification, TaskSpecError);
error_from!(
    Reference,
    crate::reference_projection::ReferenceProjectionError
);
error_from!(Gate, crate::gates::GateError);
error_from!(Store, StoreError);
error_from!(Branch, BranchError);
error_from!(Commit, DeltaCommitFailure);
pub(crate) fn canonical(text: &str) -> Result<(), TaskStoreError> {
    if text.is_empty() || text.trim() != text {
        Err(TaskStoreError::Invalid("blank or noncanonical text"))
    } else {
        Ok(())
    }
}

error_from!(Random, getrandom::Error);

error_from!(
    VectorDistance,
    graphit_data::vector_distance::VectorDistanceError
);
