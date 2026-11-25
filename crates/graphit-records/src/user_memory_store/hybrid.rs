//! USER lexical and semantic ranks fused over one admitted native read pin.
use super::{NativeUserMemoryStore, UserMemoryGeneration, native, store};
use crate::memory_store::{
    MemoryEmbeddingIdentity, MemoryEmbeddingUnavailable, MemoryHybridHit, MemoryHybridQuery,
    MemoryStoreError, MemoryStoreReadLimits,
    embeddings::{SourceOwner, query::EmbeddingScan},
    hybrid::fuse,
    lexical_search,
};
use graphit_auth::UserReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::JsonFtsIndexState;
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UserMemoryHybridSearchRead {
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
        hits: Vec<MemoryHybridHit>,
    },
}

impl NativeUserMemoryStore {
    pub async fn search_hybrid(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        identity: MemoryEmbeddingIdentity,
        query: MemoryHybridQuery,
        query_vector: &[f32],
        limits: MemoryStoreReadLimits,
    ) -> Result<UserMemoryHybridSearchRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        query.check(limits)?;
        if query_vector.len() != identity.dimensions().get() as usize
            || query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(MemoryStoreError::Invalid("Memory query vector"));
        }
        let super::UserMemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeUserMemoryStore::capture_embeddings(original, admission, identity, limits)
                .await?
        else {
            return Ok(UserMemoryHybridSearchRead::NoCorpus);
        };
        corpus
            .search_hybrid(original, admission, query, query_vector)
            .await
    }
}
impl super::UserMemoryEmbeddingCorpus {
    pub async fn search_hybrid(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        query: MemoryHybridQuery,
        query_vector: &[f32],
    ) -> Result<UserMemoryHybridSearchRead, MemoryStoreError> {
        self.check_read(original, admission)?;
        let limits = self.state.limits;
        let (expanded, options) = query.into_parts(limits)?;
        if query_vector.len() != self.state.identity.dimensions().get() as usize
            || query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(MemoryStoreError::Invalid("Memory query vector"));
        }
        self.renew_read(original, admission).await?;
        let identity = &self.state.identity;
        let generation = self.state.generation.clone();
        if self.is_empty() {
            return Ok(UserMemoryHybridSearchRead::PublishedEmpty { generation });
        }
        let read = &self.state.read;
        let before = || self.check_read(original, admission);
        let state = read.pin.json_fts_index_state("payload").await?;
        before()?;
        if state != JsonFtsIndexState::Ready {
            return Err(MemoryStoreError::LexicalIndexUnavailable(state));
        }
        let candidates =
            lexical_search::candidate_batches(&read.pin, original, expanded, limits, &before)
                .await?;
        let ranked = crate::memory_store::embeddings::query::rank_groups(
            &EmbeddingScan {
                pin: &read.pin,
                original,
                owner: SourceOwner::User(&read.owner),
                limits,
            },
            identity,
            query_vector,
            &options.filter,
            options.include_deleted_history,
            candidates,
            &before,
        )
        .await?;
        let mut semantic = ranked.semantic;
        if let Some(reason) = ranked.unavailable.filter(|_| semantic.is_empty()) {
            return Ok(UserMemoryHybridSearchRead::Unavailable { generation, reason });
        }
        semantic.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.id.cmp(&b.id))
        });
        let fused = fuse(ranked.lexical, semantic, || Ok(original.check()?))?;
        let total_hits = fused.len();
        let mut hits = Vec::new();
        for matched in fused.into_iter().take(options.top_k.get()) {
            before()?;
            let chain = store::chain(
                &read.pin,
                original,
                &read.owner,
                matched.id,
                limits,
                &before,
            )
            .await?
            .ok_or(MemoryStoreError::Invalid("hybrid selection binding"))?;
            hits.push(MemoryHybridHit {
                current: chain.current,
                lexical: matched.lexical,
                semantic: matched.semantic,
                score: matched.score,
            });
        }
        before()?;
        Ok(UserMemoryHybridSearchRead::Published {
            generation,
            total_hits,
            hits,
        })
    }
}
