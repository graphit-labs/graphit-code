//! Complete logical Memory rank fusion over a single authenticated native pin.
use super::{
    Memory, MemoryEmbeddingIdentity, MemoryEmbeddingUnavailable, MemoryGeneration, MemoryId,
    MemoryListFilter, MemoryMatchLocation, MemoryStoreError, MemoryStoreReadLimits,
    NativeMemoryStore,
    embeddings::{
        SourceOwner,
        query::{EmbeddingScan, Score},
    },
    history,
    lexical_search::{self, RankedLexicalMatch},
    native, nonblank,
};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_branch_store::JsonFtsIndexState;
use serde::Serialize;
use std::{
    collections::HashMap,
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryHybridSearchOptions {
    pub filter: MemoryListFilter,
    pub include_deleted_history: bool,
    pub top_k: NonZeroUsize,
}

/// Prepared literal input only; it contains neither authority nor a captured pin.
#[derive(Debug)]
pub struct MemoryHybridQuery {
    expanded: String,
    options: MemoryHybridSearchOptions,
}
impl MemoryHybridQuery {
    pub fn new(
        text: &str,
        options: MemoryHybridSearchOptions,
        limits: MemoryStoreReadLimits,
    ) -> Result<Self, MemoryStoreError> {
        if let Some(tag) = &options.filter.tag {
            nonblank(tag)?;
        }
        Ok(Self {
            expanded: lexical_search::expand(text, limits.max_query_bytes.get())?,
            options,
        })
    }
    pub(crate) fn into_parts(
        self,
        limits: MemoryStoreReadLimits,
    ) -> Result<(String, MemoryHybridSearchOptions), MemoryStoreError> {
        self.check(limits)?;
        Ok((self.expanded, self.options))
    }
    pub(crate) fn check(&self, limits: MemoryStoreReadLimits) -> Result<(), MemoryStoreError> {
        if self.expanded.len() > limits.max_query_bytes.get() {
            return Err(MemoryStoreError::Limit("lexical expansion bytes"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct MemoryHybridLexicalMatch {
    pub matched_revision: NonZeroU64,
    pub location: MemoryMatchLocation,
    pub score: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct MemoryHybridSemanticMatch {
    pub matched_revision: NonZeroU64,
    pub location: MemoryMatchLocation,
    pub distance: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemoryHybridHit {
    pub current: Memory,
    pub lexical: Option<MemoryHybridLexicalMatch>,
    pub semantic: Option<MemoryHybridSemanticMatch>,
    pub score: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MemoryHybridSearchRead {
    NoCorpus,
    PublishedEmpty {
        generation: MemoryGeneration,
    },
    Unavailable {
        generation: MemoryGeneration,
        reason: MemoryEmbeddingUnavailable,
    },
    Published {
        generation: MemoryGeneration,
        total_hits: usize,
        hits: Vec<MemoryHybridHit>,
    },
}
pub(crate) struct Fused {
    pub(crate) id: MemoryId,
    pub(crate) lexical: Option<MemoryHybridLexicalMatch>,
    pub(crate) semantic: Option<MemoryHybridSemanticMatch>,
    pub(crate) score: f64,
}
fn rank_score(index: usize) -> Result<f64, MemoryStoreError> {
    let rank = index
        .checked_add(1)
        .ok_or(MemoryStoreError::Invalid("hybrid rank overflow"))?;
    Ok(1.0 / (60.0 + rank as f64))
}
pub(crate) fn fuse(
    lexical: Vec<RankedLexicalMatch>,
    semantic: Vec<Score>,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Vec<Fused>, MemoryStoreError> {
    let mut union: HashMap<MemoryId, Fused> = HashMap::new();
    for (rank, matched) in lexical.into_iter().enumerate() {
        before()?;
        let entry = union.entry(matched.id).or_insert(Fused {
            id: matched.id,
            lexical: None,
            semantic: None,
            score: 0.0,
        });
        if entry.lexical.is_none() {
            entry.lexical = Some(MemoryHybridLexicalMatch {
                matched_revision: matched.matched_revision,
                location: matched.location,
                score: matched.score,
            });
            entry.score += rank_score(rank)?;
        }
    }
    for (rank, matched) in semantic.into_iter().enumerate() {
        before()?;
        let entry = union.entry(matched.id).or_insert(Fused {
            id: matched.id,
            lexical: None,
            semantic: None,
            score: 0.0,
        });
        if entry.semantic.is_none() {
            entry.semantic = Some(MemoryHybridSemanticMatch {
                matched_revision: matched.revision,
                location: if matched.current {
                    MemoryMatchLocation::Current
                } else {
                    MemoryMatchLocation::Archived {
                        revision: matched.revision,
                    }
                },
                distance: matched.distance,
            });
            entry.score += rank_score(rank)?;
        }
    }
    let mut ranked: Vec<_> = union.into_values().collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    before()?;
    Ok(ranked)
}
impl NativeMemoryStore {
    pub async fn search_hybrid(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        identity: MemoryEmbeddingIdentity,
        query: MemoryHybridQuery,
        query_vector: &[f32],
        limits: MemoryStoreReadLimits,
    ) -> Result<MemoryHybridSearchRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        query.check(limits)?;
        if query_vector.len() != identity.dimensions().get() as usize
            || query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(MemoryStoreError::Invalid("Memory query vector"));
        }
        let super::MemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeMemoryStore::capture_embeddings(original, admission, identity, limits).await?
        else {
            return Ok(MemoryHybridSearchRead::NoCorpus);
        };
        corpus
            .search_hybrid(original, admission, query, query_vector)
            .await
    }
}
impl super::MemoryEmbeddingCorpus {
    pub async fn search_hybrid(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        query: MemoryHybridQuery,
        query_vector: &[f32],
    ) -> Result<MemoryHybridSearchRead, MemoryStoreError> {
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
            return Ok(MemoryHybridSearchRead::PublishedEmpty { generation });
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
        let ranked = super::embeddings::query::rank_groups(
            &EmbeddingScan {
                pin: &read.pin,
                original,
                owner: SourceOwner::Project(generation.project_id),
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
            return Ok(MemoryHybridSearchRead::Unavailable { generation, reason });
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
            let chain = history::chain(
                &read.pin,
                original,
                matched.id,
                generation.project_id,
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
        Ok(MemoryHybridSearchRead::Published {
            generation,
            total_hits,
            hits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU128;
    fn id(n: u128) -> MemoryId {
        MemoryId::new(NonZeroU128::new(n).unwrap())
    }
    #[test]
    fn rrf_one_based_k60_dedup_disjoint_and_stable_tie_keep_raw_channel_values() {
        let lexical = vec![
            RankedLexicalMatch {
                id: id(1),
                matched_revision: NonZeroU64::MIN,
                location: MemoryMatchLocation::Current,
                score: -7.,
            },
            RankedLexicalMatch {
                id: id(2),
                matched_revision: NonZeroU64::MIN,
                location: MemoryMatchLocation::Current,
                score: 42.,
            },
            RankedLexicalMatch {
                id: id(1),
                matched_revision: NonZeroU64::MIN,
                location: MemoryMatchLocation::Current,
                score: 999.,
            },
        ];
        let semantic = vec![
            Score {
                id: id(2),
                revision: NonZeroU64::new(2).unwrap(),
                current: false,
                distance: 0.1,
            },
            Score {
                id: id(3),
                revision: NonZeroU64::MIN,
                current: true,
                distance: 0.2,
            },
        ];
        let ranked = fuse(lexical, semantic, || Ok(())).unwrap();
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].id, id(2));
        assert_eq!(ranked[0].score, 1.0 / 62.0 + 1.0 / 61.0);
        assert_eq!(ranked[1].score, 1.0 / 61.0);
        assert_eq!(ranked[1].lexical.unwrap().score, -7.);
        assert_eq!(ranked[0].semantic.unwrap().distance, 0.1);
        assert_eq!(
            ranked[0].semantic.unwrap().location,
            MemoryMatchLocation::Archived {
                revision: NonZeroU64::new(2).unwrap()
            }
        );
        let ties = fuse(
            vec![RankedLexicalMatch {
                id: id(3),
                matched_revision: NonZeroU64::MIN,
                location: MemoryMatchLocation::Current,
                score: 0.,
            }],
            vec![Score {
                id: id(1),
                revision: NonZeroU64::MIN,
                current: true,
                distance: 1.,
            }],
            || Ok(()),
        )
        .unwrap();
        assert_eq!(
            ties.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![id(1), id(3)]
        );
        assert!(rank_score(usize::MAX).is_err());
    }
}
