//! Reconciliation of managed document membership in an existing full Knowledge image.
//! The expected generation and recovery key are observations, never authority.

use super::{KnowledgeSourceIdentityError, parse_managed_knowledge_source_path};
use crate::{
    knowledge_document_update::KnowledgeDocumentUpdateResult,
    knowledge_index::{
        self as index, KnowledgeGeneration, KnowledgeIndexError, KnowledgeIndexReadLimits,
        KnowledgeIndexWriteLimits,
    },
    knowledge_references::{
        self as refs, KnowledgeDocumentState, ReferenceRowInput, WitnessHeader,
    },
    lance_branch_store::{DeltaCommitFailure, DeltaRecoveryKey},
    lance_module_store::StoreError,
};
use arrow_array::builder::{ListBuilder, StringBuilder};
use arrow_array::{
    FixedSizeBinaryArray, ListArray, RecordBatch, StringArray, StructArray, UInt8Array,
};
use arrow_schema::{DataType, Field};
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedSourceReader, StoreOwner},
    file_io::SnapshotContext,
};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt, io,
    sync::Arc,
};

#[cfg(test)]
use crate::knowledge_document_update::{
    KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
};

mod scoped;
pub(super) use scoped::validate_source_file;
mod stream;
pub use scoped::{
    KnowledgeSourceHead, KnowledgeSourceOrigins, knowledge_source_relative_path,
    read_native_knowledge_source_origins, reconcile_native_knowledge_source_file,
    remove_native_knowledge_source_origins,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[derive(Clone, Copy)]
pub struct KnowledgeDocumentLifecycleLimits {
    pub read: KnowledgeIndexReadLimits,
    pub write: KnowledgeIndexWriteLimits,
}
#[derive(Debug)]
pub enum KnowledgeDocumentLifecycleError {
    Data(KnowledgeIndexError),
    InvalidInput(&'static str),
    ScopeMismatch,
    GenerationChanged {
        observed: KnowledgeGeneration,
    },
    NoCorpus,
    ForeignSlugConflict {
        desired: usize,
    },
    Source(io::Error),
    Identity(KnowledgeSourceIdentityError),
    InheritedTarget,
    Commit(DeltaCommitFailure),
    Published {
        generation: KnowledgeGeneration,
        recovery_key: Box<DeltaRecoveryKey>,
        cause: Box<Self>,
    },
}
impl fmt::Display for KnowledgeDocumentLifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge document lifecycle: {self:?}")
    }
}
impl Error for KnowledgeDocumentLifecycleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Data(e) => Some(e),
            Self::Commit(e) => Some(e),
            Self::Source(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Published { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
impl From<KnowledgeIndexError> for KnowledgeDocumentLifecycleError {
    fn from(error: KnowledgeIndexError) -> Self {
        Self::Data(error)
    }
}

fn reserve<T>(items: &mut Vec<T>, count: usize) -> Result<(), KnowledgeIndexError> {
    items
        .try_reserve_exact(count)
        .map_err(|e| KnowledgeIndexError::Request(io::Error::new(io::ErrorKind::OutOfMemory, e)))
}

fn check_authority(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
) -> Result<(), KnowledgeDocumentLifecycleError> {
    index::check_read(original, read)?;
    index::check_write(original, write)?;
    let r = read.project().map_err(KnowledgeIndexError::from)?;
    let w = write.project().map_err(KnowledgeIndexError::from)?;
    if r.scope.live.owner() != w.scope.live.owner()
        || r.scope.live.runtime_root() != w.scope.live.runtime_root()
        || r.version.version() != w.version.version()
        || r.version.lance_branch() != w.version.lance_branch()
    {
        return Err(KnowledgeDocumentLifecycleError::ScopeMismatch);
    }
    Ok(())
}

fn allocation(error: std::collections::TryReserveError) -> KnowledgeIndexError {
    KnowledgeIndexError::Request(io::Error::new(io::ErrorKind::OutOfMemory, error))
}
fn witness_header(
    batch: &RecordBatch,
    row: usize,
) -> Result<WitnessHeader<'_>, KnowledgeIndexError> {
    Ok(WitnessHeader {
        slug: index::array::<StringArray>(batch, 0)?.value(row),
        title: index::array::<StringArray>(batch, 1)?.value(row),
        source_path: index::array::<StringArray>(batch, 7)?.value(row),
        digest: index::array::<FixedSizeBinaryArray>(batch, 9)?
            .value(row)
            .try_into()
            .map_err(|_| KnowledgeIndexError::InvalidField("compilation_digest"))?,
    })
}
