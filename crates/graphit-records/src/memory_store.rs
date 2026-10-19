//! Authenticated PROJECT Memory creation, history and complete metadata reads.
#![doc = include_str!("../../../docs/specs/native_memory_store.md")]
pub(crate) mod automatic;
pub(crate) mod catalogue;
pub(crate) mod embeddings;
mod fenced;
pub(crate) mod git;
mod history;
pub(crate) mod hybrid;
mod lexical_index;
pub(crate) mod lexical_search;
pub(crate) mod model;
mod native;
pub(crate) mod read;
pub(crate) mod references;
pub(crate) mod revision;
pub(crate) mod rows;
pub(crate) mod write;

use crate::reference_projection::ReferenceProjectionError;
pub use embeddings::{
    MEMORY_DOCUMENT_RECIPE, MemoryEmbeddingAttachOutcome, MemoryEmbeddingAttachment,
    MemoryEmbeddingCorpus, MemoryEmbeddingCorpusRead, MemoryEmbeddingDocument,
    MemoryEmbeddingIdentity, MemoryEmbeddingPublication, MemoryEmbeddingUnavailable,
    MemorySemanticHit, MemorySemanticSearchOptions, MemorySemanticSearchRead,
    encode_memory_embedding_row, prepare_memory_embedding_document,
};
pub use fenced::{FencedMemoryMutation, FencedUpdateMemory};
use graphit_auth::AuthError;
use graphit_core::{
    artifact_store::{ArtifactError, ArtifactLimits},
    native_document_file::NativeDocumentFileLimits,
};
use graphit_data::{
    lance_branch_store::{BranchError, DeltaCommitFailure},
    lance_module_store::StoreError,
};
pub use hybrid::{
    MemoryHybridHit, MemoryHybridLexicalMatch, MemoryHybridQuery, MemoryHybridSearchOptions,
    MemoryHybridSearchRead, MemoryHybridSemanticMatch,
};
pub use model::*;
pub use references::{
    MemoryReferenceEdge, MemoryReferencePage, MemoryReferenceProvenance, MemoryReferenceResolution,
    MemoryReferenceTarget, MemoryReferencesCoverage, MemoryReferencesCursor, MemoryReferencesQuery,
    MemoryReferencesRead, MemoryReferencesWarning,
};
use std::{
    error::Error,
    fmt, io,
    num::{NonZeroU32, NonZeroUsize},
};

#[derive(Clone, Copy)]
pub struct MemoryStoreReadLimits {
    /// Maximum bytes in one encoded lexical query; never a corpus bound.
    pub max_query_bytes: NonZeroUsize,
    pub batch_rows: NonZeroU32,
    pub native: NativeDocumentFileLimits,
}
#[derive(Clone, Copy)]
pub struct MemoryHeadReadLimits {
    pub batch_rows: NonZeroU32,
    pub native: NativeDocumentFileLimits,
}
#[derive(Clone, Copy)]
pub struct MemoryStoreWriteLimits {
    pub read: MemoryStoreReadLimits,
    pub artifacts: ArtifactLimits,
    pub max_input_bytes: NonZeroUsize,
    pub max_row_bytes: NonZeroUsize,
}
/// Stateless service; admissions and the original request are required on every call.
pub struct NativeMemoryStore;

#[derive(Debug)]
pub enum MemoryStoreError {
    Auth(AuthError),
    Artifact(ArtifactError),
    Request(io::Error),
    Arrow(arrow_schema::ArrowError),
    Json(serde_json::Error),
    References(ReferenceProjectionError),
    Store(StoreError),
    Branch(BranchError),
    Commit(DeltaCommitFailure),
    WriterAcquisition(tokio::task::JoinError),
    VectorDistance(graphit_data::vector_distance::VectorDistanceError),
    EmbeddingConfirmed {
        publication: Box<MemoryEmbeddingPublication>,
        cause: Box<MemoryStoreError>,
    },
    WrongAdmission,
    DifferentRequest,
    Invalid(&'static str),
    Limit(&'static str),
    IncompleteCorpus,
    LexicalIndexUnavailable(graphit_data::lance_branch_store::JsonFtsIndexState),
    IdempotencyConflict,
    NotFound,
    RevisionConflict {
        expected: u64,
        actual: u64,
    },
    NoChange,
    IndexConfirmed {
        publication: Box<MemoryLexicalIndexPublication>,
        cause: Box<MemoryStoreError>,
    },
    Confirmed {
        version: u64,
        cause: Box<MemoryStoreError>,
    },
    UserConfirmed {
        publication: Box<crate::user_memory_store::UserMemoryPublication>,
        cause: Box<MemoryStoreError>,
    },
    UserIndexConfirmed {
        publication: Box<crate::user_memory_store::UserMemoryLexicalIndexPublication>,
        cause: Box<MemoryStoreError>,
    },
    UserEmbeddingConfirmed {
        publication: Box<crate::user_memory_store::UserMemoryEmbeddingPublication>,
        cause: Box<MemoryStoreError>,
    },
}
impl fmt::Display for MemoryStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Memory store: {self:?}")
    }
}
impl Error for MemoryStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(e) => Some(e),
            Self::Artifact(e) => Some(e),
            Self::Request(e) => Some(e),
            Self::Arrow(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::References(e) => Some(e),
            Self::Store(e) => Some(e),
            Self::Branch(e) => Some(e),
            Self::Commit(e) => Some(e),
            Self::WriterAcquisition(e) => Some(e),
            Self::VectorDistance(e) => Some(e),
            Self::Confirmed { cause, .. }
            | Self::IndexConfirmed { cause, .. }
            | Self::EmbeddingConfirmed { cause, .. }
            | Self::UserConfirmed { cause, .. }
            | Self::UserIndexConfirmed { cause, .. }
            | Self::UserEmbeddingConfirmed { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
macro_rules! error_from {
    ($variant:ident, $ty:ty) => {
        impl From<$ty> for MemoryStoreError {
            fn from(error: $ty) -> Self {
                Self::$variant(error)
            }
        }
    };
}
error_from!(Auth, AuthError);
error_from!(Artifact, ArtifactError);
error_from!(Request, io::Error);
error_from!(Arrow, arrow_schema::ArrowError);
error_from!(Json, serde_json::Error);
error_from!(References, ReferenceProjectionError);
error_from!(Store, StoreError);
error_from!(Branch, BranchError);
error_from!(Commit, DeltaCommitFailure);

pub(crate) fn charge(
    total: &mut usize,
    amount: usize,
    maximum: usize,
    label: &'static str,
) -> Result<(), MemoryStoreError> {
    *total = total
        .checked_add(amount)
        .filter(|n| *n <= maximum)
        .ok_or(MemoryStoreError::Limit(label))?;
    Ok(())
}
pub(crate) fn nonblank(text: &str) -> Result<(), MemoryStoreError> {
    if text.trim().is_empty() {
        Err(MemoryStoreError::Invalid("blank authored text"))
    } else {
        Ok(())
    }
}
pub(crate) fn valid_actor(text: &str) -> Result<(), MemoryStoreError> {
    if text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(MemoryStoreError::Invalid("actor encoding"))
    }
}

error_from!(
    VectorDistance,
    graphit_data::vector_distance::VectorDistanceError
);
