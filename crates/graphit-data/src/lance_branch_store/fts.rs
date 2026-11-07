//! Native lookup and full-text indices on the original captured dataset.
use super::{BranchError, BranchSnapshot, DeltaCommitFailure, DeltaRecoveryKey, StrictCommit};
use arrow_array::{Array, StringArray};
use arrow_schema::DataType;
use futures::TryStreamExt;
use lance::{
    dataset::{
        CommitBuilder,
        builder::DatasetBuilder,
        transaction::{Operation, Transaction, TransactionBuilder},
    },
    index::{
        CreateIndexBuilder, DatasetIndexExt, DatasetIndexInternalExt,
        scalar::{IndexDetails, load_segment_details, load_segment_params},
    },
    session::{CacheSpec, Session},
};
use lance_core::cache::MokaCacheBackend;
use lance_index::{
    IndexParams, IndexType,
    scalar::{
        BuiltinIndexType, InvertedIndexParams, ScalarIndexParams, inverted::DocumentGranularity,
    },
};
use lance_table::{format::IndexMetadata, io::manifest::read_manifest_indexes};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Explicit index publication intent; replacement requires an existing JSON FTS index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonFtsIndexMode {
    Create,
    Replace,
}

impl BranchSnapshot {
    /// Build a real SDK vector index once. Later unindexed fragments retain
    /// Lance's native scan coverage; this does not rebuild on every batch/query.
    /// SDK training errors are returned with their original cause.
    pub async fn prepare_vector_index(
        &self,
        column: &str,
        name: &str,
        params: &lance::index::vector::VectorIndexParams,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
    ) -> Result<Option<PreparedIndex>, BranchError> {
        original
            .check()
            .map_err(|error| lance::Error::io_source(Box::new(error)))?;
        let field_id = self.dataset.schema().field_id(column)?;
        for index in self.dataset.load_indices().await?.iter() {
            if index.name == name {
                if index.fields != [field_id] {
                    return Err(BranchError::SchemaMismatch);
                }
                let observed = self
                    .dataset
                    .open_vector_index(
                        column,
                        &index.uuid,
                        &lance_index::metrics::NoOpMetricsCollector,
                    )
                    .await?;
                if observed.metric_type() != params.metric_type {
                    return Err(BranchError::SchemaMismatch);
                }
                original
                    .check()
                    .map_err(|error| lance::Error::io_source(Box::new(error)))?;
                return Ok(None);
            }
        }
        let mut prepared = self
            .prepare_index_segment(column, name, params.index_type(), params, None, vec![])
            .await?;
        original
            .check()
            .map_err(|error| lance::Error::io_source(Box::new(error)))?;
        let location = self.dataset.branch_location();
        prepared.guard = Arc::new(StrictCommit {
            delegate: Arc::clone(&self.delegate),
            path: location.path,
            branch: location.branch,
            version: self
                .version()
                .checked_add(1)
                .ok_or(BranchError::InvalidVersion)?,
            original: Some(Arc::clone(original)),
        });
        Ok(Some(prepared))
    }

    /// Prepare the initial code-tokenized FTS index on a captured branch.
    /// Subsequent FILE merges retain this index; Lance searches fragments not
    /// covered by it directly until `prepare_code_fts_append` catches them up.
    pub async fn prepare_code_fts_index(
        &self,
        column: &str,
        name: &str,
    ) -> Result<PreparedIndex, BranchError> {
        let params = InvertedIndexParams::code().max_token_length(None);
        self.prepare_index_segment(column, name, IndexType::Inverted, &params, None, vec![])
            .await
    }

    /// Create an absent code FTS index, otherwise build a disjoint segment for
    /// only the uncovered fragments, merging comparable existing posting
    /// segments through Lance. Already indexed rows are never scanned or rebuilt.
    /// A covered, consolidated index is a no-op without a new manifest.
    pub async fn prepare_code_fts_append(
        &self,
        column: &str,
        name: &str,
    ) -> Result<Option<PreparedIndex>, BranchError> {
        let params = InvertedIndexParams::code().max_token_length(None);
        let indices = self
            .native_index_segments(column, name, IndexType::Inverted)
            .await?;
        if indices.is_empty() {
            return self.prepare_code_fts_index(column, name).await.map(Some);
        }
        let details = load_segment_details(&self.dataset, column, &indices).await?;
        let observed = InvertedIndexParams::try_from(&details)?;
        let format = observed.resolved_format_version();
        if observed != params.clone().format_version(format) {
            return Err(BranchError::SchemaMismatch);
        }
        let fragments = self.uncovered_fragments(&indices)?;
        if fragments.is_empty() {
            return self.prepare_index_segments(None, indices).await;
        }
        self.prepare_index_segment(
            column,
            name,
            IndexType::Inverted,
            &params,
            Some(fragments),
            indices,
        )
        .await
        .map(Some)
    }

    /// Prepare a native BTree lookup index for a concrete producer join/filter.
    pub async fn prepare_scalar_index(
        &self,
        column: &str,
        name: &str,
    ) -> Result<PreparedIndex, BranchError> {
        let params = ScalarIndexParams::for_builtin(BuiltinIndexType::BTree);
        self.prepare_index_segment(column, name, IndexType::BTree, &params, None, vec![])
            .await
    }

    /// Create an absent BTree, otherwise index only uncovered fragments and merge
    /// comparable native segments. Captured history keeps its original files;
    /// a covered, consolidated index publishes nothing.
    /// Fully covered inherited snapshots remain read-only no-ops.
    /// An occupied name with a different column or type is an explicit error.
    pub async fn prepare_scalar_append(
        &self,
        column: &str,
        name: &str,
    ) -> Result<Option<PreparedIndex>, BranchError> {
        let indices = self
            .native_index_segments(column, name, IndexType::BTree)
            .await?;
        if indices.is_empty() {
            return self.prepare_scalar_index(column, name).await.map(Some);
        }
        let fragments = self.uncovered_fragments(&indices)?;
        if fragments.is_empty() {
            // A fully covered inherited read does not own physical maintenance.
            // Preserve its no-op without materializing or writing the branch.
            if self.inherited_target.is_some() {
                return Ok(None);
            }
            return self.prepare_index_segments(None, indices).await;
        }
        let params = ScalarIndexParams::for_builtin(BuiltinIndexType::BTree);
        self.prepare_index_segment(
            column,
            name,
            IndexType::BTree,
            &params,
            Some(fragments),
            indices,
        )
        .await
        .map(Some)
    }

    // Validate declarations from the captured manifest. Native code indices
    // persist their analyzer configuration there; opening every old physical
    // segment on every FILE update would add repeated I/O without new input.
    async fn native_index_segments(
        &self,
        column: &str,
        name: &str,
        kind: IndexType,
    ) -> Result<Vec<IndexMetadata>, BranchError> {
        let field_id = self.dataset.schema().field_id(column)?;
        let store = self.dataset.object_store(None).await?;
        let metadata = read_manifest_indexes(
            store.as_ref(),
            self.dataset.manifest_location(),
            self.dataset.manifest(),
        )
        .await?;
        let mut segments = Vec::new();
        for index in metadata {
            if kind == IndexType::Inverted
                && index.name != name
                && index.fields.contains(&field_id)
                && index
                    .index_details
                    .as_ref()
                    .is_some_and(|details| IndexDetails(Arc::clone(details)).supports_fts())
            {
                return Err(BranchError::SchemaMismatch);
            }
            if index.name != name {
                continue;
            }
            index.validate_covering_fields()?;
            let details = IndexDetails(Arc::clone(
                index
                    .index_details
                    .as_ref()
                    .ok_or(BranchError::SchemaMismatch)?,
            ));
            if index.fields != [field_id]
                || index.keyed_fields() != [field_id]
                || match kind {
                    IndexType::Inverted => !details.supports_fts(),
                    _ => !details.get_plugin()?.name().eq_ignore_ascii_case("BTree"),
                }
            {
                return Err(BranchError::SchemaMismatch);
            }
            segments.push(index);
        }
        Ok(segments)
    }

    fn uncovered_fragments(&self, indices: &[IndexMetadata]) -> Result<Vec<u32>, BranchError> {
        let mut covered = indices
            .first()
            .and_then(|index| index.fragment_bitmap.clone())
            .ok_or(BranchError::SchemaMismatch)?;
        for index in indices.iter().skip(1) {
            covered |= index
                .fragment_bitmap
                .as_ref()
                .ok_or(BranchError::SchemaMismatch)?;
        }
        self.dataset
            .get_fragments()
            .iter()
            .map(|fragment| u32::try_from(fragment.id()).map_err(|_| BranchError::SchemaMismatch))
            .collect::<Result<Vec<_>, _>>()
            .map(|ids| {
                ids.into_iter()
                    .filter(|id| !covered.contains(*id))
                    .collect()
            })
    }

    async fn prepare_index_segment(
        &self,
        column: &str,
        name: &str,
        index_type: IndexType,
        params: &dyn IndexParams,
        fragments: Option<Vec<u32>>,
        merge_candidates: Vec<IndexMetadata>,
    ) -> Result<PreparedIndex, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        self.version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        let append = fragments.is_some();
        let mut captured = self.dataset.as_ref().clone();
        let columns = [column];
        let mut builder = CreateIndexBuilder::new(&mut captured, &columns, index_type, params)
            .name(name.to_owned())
            .replace(append);
        if let Some(fragments) = fragments {
            builder = builder.fragments(fragments);
        }
        let metadata = builder.execute_uncommitted().await?;
        if metadata.dataset_version != self.version() {
            return Err(BranchError::InvalidVersion);
        }
        if append && matches!(index_type, IndexType::Inverted | IndexType::BTree) {
            return self
                .prepare_index_segments(Some(metadata), merge_candidates)
                .await?
                .ok_or(BranchError::SchemaMismatch);
        }
        self.prepare_index_publication(vec![metadata], vec![])
    }

    async fn prepare_index_segments(
        &self,
        new_segment: Option<IndexMetadata>,
        existing: Vec<IndexMetadata>,
    ) -> Result<Option<PreparedIndex>, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        if new_segment.is_none() {
            let mut sizes = BTreeSet::new();
            let mut needs_merge = false;
            for index in &existing {
                let tier = index
                    .fragment_bitmap
                    .as_ref()
                    .ok_or(BranchError::SchemaMismatch)?
                    .len()
                    .checked_ilog2()
                    .unwrap_or(0);
                needs_merge |= !sizes.insert(tier);
            }
            if !needs_merge {
                return Ok(None);
            }
        }
        // A dedicated SDK maintenance session must not retain every obsolete
        // intermediate index reader. Query snapshots keep their useful cache.
        let session = Session::with_cache_backends(
            CacheSpec::Backend(Arc::new(MokaCacheBackend::no_cache())),
            CacheSpec::Default,
            self.dataset.session().store_registry(),
        );
        let mut builder = DatasetBuilder::from_uri(self.dataset.uri())
            .with_version(self.version())
            .with_session(Arc::new(session))
            .with_commit_handler(Arc::clone(&self.delegate));
        if let Some(params) = self.dataset.store_params() {
            builder = builder.with_store_params(params.clone());
        }
        let captured = builder.load().await?;
        if captured.version_id() != self.version()
            || captured.manifest().branch.as_deref() != self.physical_branch()
        {
            return Err(BranchError::InvalidVersion);
        }
        let store = captured.object_store(None).await?;
        let mut tiers: BTreeMap<u32, IndexMetadata> = BTreeMap::new();
        let mut candidates = existing.clone();
        if let Some(metadata) = new_segment {
            // Fragment-restricted builders may emit staged segments; finalize only
            // the new segment, without loading all committed segments at once.
            let staged = metadata.uuid;
            let finalized = captured
                .merge_existing_index_segments(vec![metadata])
                .await?;
            // The SDK can finalize a lone staged segment in place. Its returned
            // UUID owns the final files and must not be treated as disposable.
            if finalized.uuid != staged {
                store
                    .remove_dir_all(captured.indices_dir().join(staged.to_string()))
                    .await?;
            }
            candidates.push(finalized);
        }
        for mut segment in candidates {
            loop {
                let size = segment
                    .fragment_bitmap
                    .as_ref()
                    .ok_or(BranchError::SchemaMismatch)?
                    .len();
                let tier = size.checked_ilog2().unwrap_or(0);
                let Some(previous) = tiers.remove(&tier) else {
                    tiers.insert(tier, segment);
                    break;
                };
                // Conventional size-tiered compaction: merge two comparable
                // segments, then carry their union into its resulting tier.
                // Pairwise SDK merges also repair old FILE-per-segment layouts
                // without opening the whole backlog simultaneously.
                let consumed = [previous.uuid, segment.uuid];
                segment = captured
                    .merge_existing_index_segments(vec![previous, segment])
                    .await?;
                // Only our uncommitted intermediate outputs are disposable.
                // Committed inputs remain reachable from historical snapshots;
                // final outputs stay available even if commit acknowledgement
                // is lost. An SDK failure/cancellation still follows SDK GC.
                for uuid in consumed {
                    if uuid != segment.uuid && !existing.iter().any(|index| index.uuid == uuid) {
                        store
                            .remove_dir_all(captured.indices_dir().join(uuid.to_string()))
                            .await?;
                    }
                }
            }
        }
        let retained: Vec<_> = tiers.into_values().collect();
        let removed: Vec<_> = existing
            .iter()
            .filter(|old| !retained.iter().any(|kept| old.uuid == kept.uuid))
            .cloned()
            .collect();
        let added: Vec<_> = retained
            .into_iter()
            .filter(|new| !existing.iter().any(|old| old.uuid == new.uuid))
            .collect();
        if added.is_empty() {
            return Ok(None);
        }
        self.prepare_index_publication(added, removed).map(Some)
    }

    pub(super) fn prepare_index_publication(
        &self,
        new_indices: Vec<IndexMetadata>,
        removed_indices: Vec<IndexMetadata>,
    ) -> Result<PreparedIndex, BranchError> {
        let target_version = self
            .version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        // Publish replacement coverage atomically with the new postings. Old
        // captured manifests retain their original segment files and readers.
        let transaction = TransactionBuilder::new(
            self.version(),
            Operation::CreateIndex {
                new_indices,
                removed_indices,
            },
        )
        .build();
        let key = DeltaRecoveryKey {
            corpus_uri: self.uri.clone(),
            physical_branch: self.physical_branch().map(str::to_owned),
            base_version: self.version(),
            transaction_id: transaction.uuid.clone(),
        };
        let location = self.dataset.branch_location();
        Ok(PreparedIndex {
            snapshot: self.clone(),
            transaction,
            key,
            guard: Arc::new(StrictCommit {
                delegate: Arc::clone(&self.delegate),
                path: location.path,
                branch: location.branch,
                version: target_version,
                original: None,
            }),
        })
    }

    /// Prepare JSON full-text construction without publishing a manifest.
    ///
    /// Every value in `column` must be non-null Utf8 containing one complete
    /// JSON value. Create rejects an occupied name. Replace requires compatible
    /// physical segments of that name on this column in this same captured pin.
    /// Caller authorization and cancellation remain with the service.
    pub async fn prepare_json_fts_index(
        &self,
        column: &str,
        name: &str,
        mode: JsonFtsIndexMode,
    ) -> Result<PreparedIndex, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        let target_version = self
            .version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        self.validate_json_column(column).await?;
        let params = InvertedIndexParams::default()
            .lance_tokenizer("json".into())
            .base_tokenizer("simple".into())
            .max_token_length(None)
            .lower_case(true)
            .stem(false)
            .remove_stop_words(false)
            .ascii_folding(false);
        let removed_indices = match mode {
            JsonFtsIndexMode::Create => vec![],
            JsonFtsIndexMode::Replace => self.replacement_indices(column, name, &params).await?,
        };
        // The SDK builder borrows mutably even for uncommitted construction.
        // Clone its captured handle; publication still uses the original Arc.
        let mut captured = self.dataset.as_ref().clone();
        let metadata =
            CreateIndexBuilder::new(&mut captured, &[column], IndexType::Inverted, &params)
                .name(name.to_owned())
                .replace(mode == JsonFtsIndexMode::Replace)
                .execute_uncommitted()
                .await?;
        if metadata.dataset_version != self.version() {
            return Err(BranchError::InvalidVersion);
        }
        let transaction = TransactionBuilder::new(
            self.version(),
            Operation::CreateIndex {
                new_indices: vec![metadata],
                removed_indices,
            },
        )
        .build();
        let key = DeltaRecoveryKey {
            corpus_uri: self.uri.clone(),
            physical_branch: self.physical_branch().map(str::to_owned),
            base_version: self.version(),
            transaction_id: transaction.uuid.clone(),
        };
        let location = self.dataset.branch_location();
        Ok(PreparedIndex {
            snapshot: self.clone(),
            transaction,
            key,
            guard: Arc::new(StrictCommit {
                delegate: Arc::clone(&self.delegate),
                path: location.path,
                branch: location.branch,
                version: target_version,
                original: None,
            }),
        })
    }

    async fn replacement_indices(
        &self,
        column: &str,
        name: &str,
        expected: &InvertedIndexParams,
    ) -> Result<Vec<IndexMetadata>, BranchError> {
        let field_id = self.dataset.schema().field_id(column)?;
        let store = self.dataset.object_store(None).await?;
        let metadata = read_manifest_indexes(
            store.as_ref(),
            self.dataset.manifest_location(),
            self.dataset.manifest(),
        )
        .await?;
        if !metadata.iter().any(|index| index.name == name) {
            return Err(BranchError::JsonIndexReplacementMissing {
                column: column.to_owned(),
                name: name.to_owned(),
            });
        }
        let incompatible = || BranchError::JsonIndexReplacementIncompatible {
            column: column.to_owned(),
            name: name.to_owned(),
        };
        let mut targets = Vec::new();
        for index in metadata {
            if !index.fields.contains(&field_id) {
                if index.name == name {
                    return Err(incompatible());
                }
                continue;
            }
            index.validate_covering_fields()?;
            let Some(details) = &index.index_details else {
                return Err(incompatible());
            };
            let fts = IndexDetails(Arc::clone(details)).supports_fts();
            if index.name == name {
                if !fts || index.fields != [field_id] || index.keyed_fields() != [field_id] {
                    return Err(incompatible());
                }
                targets.push(index);
            } else if fts {
                // Column-based FTS cannot select among competing logical names.
                return Err(incompatible());
            }
        }
        let details = load_segment_details(&self.dataset, column, &targets).await?;
        if InvertedIndexParams::try_from(&details)?.get_document_granularity()
            != DocumentGranularity::Row
        {
            return Err(incompatible());
        }
        for segment in &targets {
            let physical = load_segment_params(&self.dataset, segment).await?;
            let format = physical.resolved_format_version();
            if physical.format_version(format) != expected.clone().format_version(format) {
                return Err(incompatible());
            }
        }
        Ok(targets)
    }

    async fn validate_json_column(&self, column: &str) -> Result<(), BranchError> {
        if !self
            .schema
            .field_with_name(column)
            .is_ok_and(|field| field.data_type() == &DataType::Utf8)
        {
            return Err(BranchError::JsonIndexColumn(column.to_owned()));
        }
        let mut scanner = self.scan();
        scanner.project(&[column])?;
        let mut stream = scanner.try_into_stream().await?;
        let mut row = 0;
        while let Some(batch) = stream.try_next().await? {
            let values = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or(BranchError::SchemaMismatch)?;
            for i in 0..values.len() {
                if values.is_null(i) {
                    return Err(BranchError::NullJsonIndexValue {
                        column: column.to_owned(),
                        row,
                    });
                }
                // Match the SDK tokenizer's parser, including number bounds,
                // Unicode escapes and recursion. Discard each tree immediately.
                serde_json::from_slice::<serde_json::Value>(values.value(i).as_bytes())
                    .map(drop)
                    .map_err(|cause| BranchError::InvalidJsonIndexValue {
                        column: column.to_owned(),
                        row,
                        cause,
                    })?;
                row += 1;
            }
        }
        Ok(())
    }
}

/// An uncommitted index and its original snapshot, fence and recovery identity.
/// Physical index files may exist before publication or after a failed commit.
pub struct PreparedIndex {
    snapshot: BranchSnapshot,
    transaction: Transaction,
    key: DeltaRecoveryKey,
    guard: Arc<StrictCommit>,
}
impl PreparedIndex {
    pub fn recovery_key(&self) -> &DeltaRecoveryKey {
        &self.key
    }

    /// Publish exactly base + 1 without retrying or adopting another writer.
    /// Revalidate the caller's original write authority before this operation.
    pub async fn commit(self) -> Result<BranchSnapshot, DeltaCommitFailure> {
        let dataset = CommitBuilder::new(Arc::clone(&self.snapshot.dataset))
            .with_max_retries(0)
            .with_skip_auto_cleanup(true)
            .with_commit_handler(self.guard)
            .execute(self.transaction)
            .await
            .map_err(|cause| DeltaCommitFailure {
                key: self.key,
                cause: Box::new(cause),
            })?;
        let mut snapshot = self.snapshot;
        snapshot.dataset = Arc::new(dataset);
        Ok(snapshot)
    }
}
