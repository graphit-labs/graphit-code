use super::*;
use crate::{
    memory_store::{
        embeddings::{SourceOwner, query as shared_query, source_at},
        lexical_search::HistoryView,
    },
    user_memory_store::{native, store},
};
use graphit_auth::UserReadAdmission;
use graphit_core::file_io::SnapshotContext;
use std::sync::Arc;

impl NativeUserMemoryStore {
    pub async fn search_semantic(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        identity: MemoryEmbeddingIdentity,
        query_vector: &[f32],
        options: MemorySemanticSearchOptions,
        limits: MemoryStoreReadLimits,
    ) -> Result<UserMemorySemanticSearchRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if query_vector.len() != identity.dimensions().get() as usize
            || query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(MemoryStoreError::Invalid("Memory query vector"));
        }
        let UserMemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeUserMemoryStore::capture_embeddings(original, admission, identity, limits)
                .await?
        else {
            return Ok(UserMemorySemanticSearchRead::NoCorpus);
        };
        corpus
            .search_semantic(original, admission, query_vector, options)
            .await
    }
}
impl UserMemoryEmbeddingCorpus {
    pub async fn search_semantic(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        query_vector: &[f32],
        options: MemorySemanticSearchOptions,
    ) -> Result<UserMemorySemanticSearchRead, MemoryStoreError> {
        self.check_read(original, admission)?;
        if query_vector.len() != self.state.identity.dimensions().get() as usize
            || query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(MemoryStoreError::Invalid("Memory query vector"));
        }
        self.renew_read(original, admission).await?;
        let identity = &self.state.identity;
        let generation = self.state.generation.clone();
        if self.is_empty() {
            return Ok(UserMemorySemanticSearchRead::PublishedEmpty { generation });
        }
        let read = &self.state.read;
        let limits = self.state.limits;
        let before = || self.check_read(original, admission);
        let ranked = shared_query::rank_groups(
            &shared_query::EmbeddingScan {
                pin: &read.pin,
                original,
                owner: SourceOwner::User(&read.owner),
                limits,
            },
            identity,
            query_vector,
            &options.filter,
            options.include_deleted_history,
            std::collections::HashMap::new(),
            &before,
        )
        .await?;
        let mut scores = ranked.semantic;
        if let Some(reason) = ranked.unavailable.filter(|_| scores.is_empty()) {
            return Ok(UserMemorySemanticSearchRead::Unavailable { generation, reason });
        }
        scores.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.id.cmp(&b.id))
        });
        let total_hits = scores.len();
        let mut hits = Vec::new();
        for score in scores.into_iter().take(options.top_k.get()) {
            before()?;
            let chain = store::chain(&read.pin, original, &read.owner, score.id, limits, &before)
                .await?
                .ok_or(MemoryStoreError::IncompleteCorpus)?;
            if source_at(HistoryView::User(&chain), score.revision.get()).is_none() {
                return Err(MemoryStoreError::IncompleteCorpus);
            }
            hits.push(MemorySemanticHit {
                current: chain.current,
                matched_revision: score.revision,
                location: if score.current {
                    MemoryMatchLocation::Current
                } else {
                    MemoryMatchLocation::Archived {
                        revision: score.revision,
                    }
                },
                distance: score.distance,
            });
        }
        let result = UserMemorySemanticSearchRead::Published {
            generation,
            total_hits,
            hits,
        };
        before()?;
        Ok(result)
    }
}
