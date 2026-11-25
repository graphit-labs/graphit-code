use super::*;
use crate::memory_store::{embeddings as shared, rows::RowOwner, *};
use arrow_array::RecordBatch;
use graphit_data::lance_branch_store::DeltaRecoveryKey;
use serde::Serialize;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod capture;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod publish;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod query;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub use capture::{UserMemoryEmbeddingCorpus, UserMemoryEmbeddingCorpusRead};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserMemoryEmbeddingPublication {
    pub generation: UserMemoryGeneration,
    pub recovery_key: DeltaRecoveryKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserMemoryEmbeddingAttachOutcome {
    Existing { generation: UserMemoryGeneration },
    Published(UserMemoryEmbeddingPublication),
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UserMemorySemanticSearchRead {
    NoCorpus,
    PublishedEmpty {
        generation: UserMemoryGeneration,
    },
    Unavailable {
        generation: UserMemoryGeneration,
        reason: MemoryEmbeddingUnavailable,
    },
    Published {
        generation: UserMemoryGeneration,
        total_hits: usize,
        hits: Vec<MemorySemanticHit>,
    },
}
/// Pure USER25 encoding does not authorize a corpus access or perform inference.
pub fn encode_user_memory_embedding_row(
    owner: &UserMemoryOwner,
    document: &MemoryEmbeddingDocument,
    vector: &[f32],
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    let mut bytes = 0;
    charge(
        &mut bytes,
        vector
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(56))
            .ok_or(MemoryStoreError::Limit("embedding input bytes"))?,
        limits.max_input_bytes.get(),
        "embedding input bytes",
    )?;
    shared::encode_row(RowOwner::User(owner), document, vector, limits)
}
