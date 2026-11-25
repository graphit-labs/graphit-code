use super::super::{
    Memory, MemoryGeneration, MemoryListFilter, MemoryMatchLocation, MemoryStoreError,
    MemoryStoreReadLimits, NativeMemoryStore, history, native, rows,
};
use super::{MemoryEmbeddingIdentity, SourceOwner, metadata, source_at, validate_source};
use crate::memory_store::lexical_search::HistoryView;
use arrow_array::ListArray;
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::lance_module_store::StoreError;
use serde::Serialize;
use std::{
    collections::HashSet,
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySemanticSearchOptions {
    pub filter: MemoryListFilter,
    pub include_deleted_history: bool,
    pub top_k: NonZeroUsize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEmbeddingUnavailable {
    MissingCoverage,
    IncompatibleIdentity,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemorySemanticHit {
    pub current: Memory,
    pub matched_revision: NonZeroU64,
    pub location: MemoryMatchLocation,
    pub distance: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MemorySemanticSearchRead {
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
        hits: Vec<MemorySemanticHit>,
    },
}
#[derive(Clone, Copy)]
pub(crate) struct Score {
    pub(crate) id: super::MemoryId,
    pub(crate) revision: NonZeroU64,
    pub(crate) current: bool,
    pub(crate) distance: f32,
}
impl Score {
    fn better_than(&self, other: &Self) -> bool {
        self.distance
            .total_cmp(&other.distance)
            .then_with(|| other.current.cmp(&self.current))
            .then_with(|| other.revision.cmp(&self.revision))
            .is_lt()
    }
}
impl NativeMemoryStore {
    /// Exact SDK cosine across eligible immutable revisions, collapsed before the total cap.
    pub async fn search_semantic(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        identity: MemoryEmbeddingIdentity,
        query_vector: &[f32],
        options: MemorySemanticSearchOptions,
        limits: MemoryStoreReadLimits,
    ) -> Result<MemorySemanticSearchRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if query_vector.len() != identity.dimensions().get() as usize
            || query_vector.iter().any(|v| !v.is_finite())
        {
            return Err(MemoryStoreError::Invalid("Memory query vector"));
        }
        let super::MemoryEmbeddingCorpusRead::Published(mut corpus) =
            NativeMemoryStore::capture_embeddings(original, admission, identity, limits).await?
        else {
            return Ok(MemorySemanticSearchRead::NoCorpus);
        };
        corpus
            .search_semantic(original, admission, query_vector, options)
            .await
    }
}
impl super::MemoryEmbeddingCorpus {
    /// Search the same snapshot captured before query inference.
    pub async fn search_semantic(
        &mut self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        query_vector: &[f32],
        options: MemorySemanticSearchOptions,
    ) -> Result<MemorySemanticSearchRead, MemoryStoreError> {
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
            return Ok(MemorySemanticSearchRead::PublishedEmpty { generation });
        }
        let read = &self.state.read;
        let limits = self.state.limits;
        let project_id = generation.project_id;
        let before = || self.check_read(original, admission);
        let ranked = rank_groups(
            &EmbeddingScan {
                pin: &read.pin,
                original,
                owner: SourceOwner::Project(project_id),
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
            return Ok(MemorySemanticSearchRead::Unavailable { generation, reason });
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
            let chain = history::chain(&read.pin, original, score.id, project_id, limits, &before)
                .await?
                .ok_or(MemoryStoreError::IncompleteCorpus)?;
            if source_at(HistoryView::Project(&chain), score.revision.get()).is_none() {
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
        before()?;
        Ok(MemorySemanticSearchRead::Published {
            generation,
            total_hits,
            hits,
        })
    }
}

pub(crate) struct EmbeddingScan<'a> {
    pub pin: &'a graphit_data::lance_branch_store::BranchSnapshot,
    pub original: &'a Arc<SnapshotContext>,
    pub owner: SourceOwner<'a>,
    pub limits: MemoryStoreReadLimits,
}

pub(crate) struct RankedCorpus {
    pub semantic: Vec<Score>,
    pub lexical: Vec<super::super::lexical_search::RankedLexicalMatch>,
    pub unavailable: Option<MemoryEmbeddingUnavailable>,
}

/// Native ordering keeps one immutable history/embedding group in application
/// memory. Revalidate authority and namespace at SDK boundaries, and keep CPU
/// loops cancellable without reopening directories for each decoded row.
pub(crate) async fn rank_groups(
    scan: &EmbeddingScan<'_>,
    identity: &MemoryEmbeddingIdentity,
    query: &[f32],
    filter: &MemoryListFilter,
    include_deleted_history: bool,
    mut lexical: std::collections::HashMap<super::MemoryId, Vec<arrow_array::RecordBatch>>,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<RankedCorpus, MemoryStoreError> {
    use lance::dataset::scanner::ColumnOrdering;
    let mut scanner = scan.pin.scan();
    scanner
        .order_by(Some(vec![ColumnOrdering::asc_nulls_last(
            "record_id".into(),
        )]))
        .map_err(StoreError::from)?
        .batch_size(scan.limits.batch_rows.get() as usize)
        .fragment_readahead(1)
        .batch_readahead(1);
    before()?;
    let mut stream = crate::storage::read(scan.original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    let mut current = None;
    let mut group = Vec::new();
    let mut semantic = Vec::new();
    let mut lexical_ranks = Vec::new();
    let mut unavailable = None;
    let check_cpu = || Ok(scan.original.check()?);
    let mut consume = |id,
                       batches: Vec<arrow_array::RecordBatch>|
     -> Result<(), MemoryStoreError> {
        let mut keys = HashSet::new();
        let mut score = |chain: HistoryView<'_>| -> Result<(), MemoryStoreError> {
            let eligible =
                filter.matches(chain.current()) && (!chain.deleted() || include_deleted_history);
            let result = score_batches(scan, &batches, chain, identity, query, eligible)?;
            if let Some(reason) = result.1 {
                unavailable.get_or_insert(reason);
            }
            if let Some(best) = result.0 {
                semantic.push(best);
            }
            if let Some(candidates) = lexical.remove(&id)
                && let Some(rank) = super::super::lexical_search::rank_group(
                    candidates,
                    chain,
                    filter,
                    include_deleted_history,
                    check_cpu,
                )?
            {
                lexical_ranks.push(rank);
            }
            Ok(())
        };
        match scan.owner {
            SourceOwner::Project(project_id) => {
                let mut sources = Vec::new();
                for batch in &batches {
                    for row in 0..batch.num_rows() {
                        scan.original.check()?;
                        if !keys.insert(rows::text(batch, 0, row)?.to_owned()) {
                            return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
                        }
                        if let Some(source) = rows::decode(batch, row, scan.original, project_id)? {
                            sources.push(source);
                        }
                    }
                }
                let chain = history::assemble(sources, project_id, check_cpu)?
                    .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
                score(HistoryView::Project(&chain))?;
            }
            SourceOwner::User(owner) => {
                let mut sources = Vec::new();
                for batch in &batches {
                    for row in 0..batch.num_rows() {
                        scan.original.check()?;
                        if !keys.insert(rows::text(batch, 0, row)?.to_owned()) {
                            return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
                        }
                        if let Some(source) = crate::user_memory_store::rows::decode(
                            batch,
                            row,
                            scan.original,
                            owner,
                        )? {
                            sources.push(source);
                        }
                    }
                }
                let chain = crate::user_memory_store::store::assemble(sources, check_cpu)?
                    .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
                score(HistoryView::User(&chain))?;
            }
        }
        check_cpu()
    };
    loop {
        before()?;
        let batch = crate::storage::read(scan.original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else { break };
        rows::validate_schema(&batch, scan.owner.schema().as_ref())?;
        let mut start = 0;
        while start < batch.num_rows() {
            scan.original.check()?;
            let id: super::MemoryId = rows::text(&batch, 2, start)?.parse()?;
            if let Some(prior) = current {
                if id < prior {
                    return Err(MemoryStoreError::Invalid("Memory group ordering"));
                }
                if id != prior {
                    consume(prior, std::mem::take(&mut group))?;
                }
            }
            current = Some(id);
            let mut end = start + 1;
            while end < batch.num_rows()
                && rows::text(&batch, 2, end)? == rows::text(&batch, 2, start)?
            {
                scan.original.check()?;
                end += 1;
            }
            group.push(batch.slice(start, end - start));
            start = end;
        }
    }
    if let Some(id) = current {
        consume(id, group)?;
    }
    if !lexical.is_empty() {
        return Err(MemoryStoreError::Invalid("unknown lexical Memory ID"));
    }
    lexical_ranks.sort_by_key(|(key, _)| *key);
    before()?;
    Ok(RankedCorpus {
        semantic,
        unavailable,
        lexical: lexical_ranks.into_iter().map(|(_, rank)| rank).collect(),
    })
}

fn score_batches(
    scan: &EmbeddingScan<'_>,
    batches: &[arrow_array::RecordBatch],
    chain: HistoryView<'_>,
    identity: &MemoryEmbeddingIdentity,
    query: &[f32],
    eligible: bool,
) -> Result<(Option<Score>, Option<MemoryEmbeddingUnavailable>), MemoryStoreError> {
    let id = chain.current().id;
    let mut covered = HashSet::new();
    let mut best: Option<Score> = None;
    let mut incompatible = false;
    for batch in batches {
        for row in 0..batch.num_rows() {
            scan.original.check()?;
            if rows::text(batch, 1, row)? != "embedding" {
                continue;
            }
            let meta = metadata(batch, row, scan.owner.layout())?;
            validate_source(&meta, chain)?;
            if !eligible {
                continue;
            }
            if meta.identity != *identity {
                incompatible = true;
                continue;
            }
            covered.insert(meta.revision);
            let lists = batch
                .column(scan.owner.column(23))
                .as_any()
                .downcast_ref::<ListArray>()
                .ok_or(MemoryStoreError::Invalid("Memory embedding list"))?;
            let distances =
                graphit_data::vector_distance::cosine_distances(&lists.slice(row, 1), query)?;
            let distance =
                distances[0].ok_or(MemoryStoreError::Invalid("Memory embedding null distance"))?;
            let score = Score {
                id,
                revision: NonZeroU64::new(meta.revision)
                    .ok_or(MemoryStoreError::Invalid("Memory embedding revision"))?,
                current: !chain.deleted() && meta.revision == chain.current().revision,
                distance,
            };
            if best.as_ref().is_none_or(|prior| score.better_than(prior)) {
                best = Some(score);
            }
        }
    }
    let missing = eligible
        && chain
            .sources()
            .any(|memory| !covered.contains(&memory.revision));
    let unavailable = if best.is_some() {
        None
    } else if incompatible {
        Some(MemoryEmbeddingUnavailable::IncompatibleIdentity)
    } else if missing {
        Some(MemoryEmbeddingUnavailable::MissingCoverage)
    } else {
        None
    };
    scan.original.check()?;
    Ok((best, unavailable))
}
