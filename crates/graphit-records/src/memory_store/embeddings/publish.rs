use super::super::{
    MemoryGeneration, MemoryId, MemoryStoreError, MemoryStoreWriteLimits, NativeMemoryStore,
    charge, history, native, rows, write,
};
use super::{
    MemoryEmbeddingCorpus, SourceOwner, capture, encode_memory_embedding_row, metadata,
    prepare_memory_embedding_document, source_at, validate_source,
};
use crate::memory_store::lexical_search::HistoryView;
use arrow_array::{Float32Array, ListArray, RecordBatch};
use futures::TryStreamExt;
use graphit_auth::ProjectWriteAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::{BranchError, DeltaRecoveryKey, DeltaResolution, PreparedDelta},
    lance_module_store::StoreError,
};
use std::{
    collections::{BTreeMap, HashSet},
    num::NonZeroU64,
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEmbeddingAttachment {
    pub memory_id: MemoryId,
    pub revision: NonZeroU64,
    pub input_fingerprint: [u8; 32],
    pub vector: Vec<f32>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEmbeddingPublication {
    pub generation: MemoryGeneration,
    pub recovery_key: DeltaRecoveryKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryEmbeddingAttachOutcome {
    Existing { generation: MemoryGeneration },
    Published(MemoryEmbeddingPublication),
}

pub(super) async fn commit(
    prepared: PreparedDelta,
    project_id: graphit_core::project_identity::ProjectId,
    project_version: &str,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<MemoryEmbeddingAttachOutcome, MemoryStoreError> {
    let recovery_key = prepared.recovery_key().clone();
    before()?;
    let pin = prepared.commit().await?;
    let publication = MemoryEmbeddingPublication {
        generation: native::generation(&pin, project_id, project_version),
        recovery_key,
    };
    match before() {
        Ok(()) => Ok(MemoryEmbeddingAttachOutcome::Published(publication)),
        Err(cause) => Err(MemoryStoreError::EmbeddingConfirmed {
            publication: Box::new(publication),
            cause: Box::new(cause),
        }),
    }
}

impl NativeMemoryStore {
    /// Attach only documents actually emitted by this immutable capture, under a current writer pin.
    pub async fn attach_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        capture: &MemoryEmbeddingCorpus,
        batch: Vec<MemoryEmbeddingAttachment>,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryEmbeddingAttachOutcome, MemoryStoreError> {
        native::check_write(original, admission)?;
        if !Arc::ptr_eq(original, &capture.state.original) {
            return Err(MemoryStoreError::DifferentRequest);
        }
        capture.state.read.guard.revalidate(original)?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        if project_id != capture.state.generation.project_id
            || project.version.version() != capture.state.generation.project_version
        {
            return Err(MemoryStoreError::Invalid(
                "Memory embedding capture binding",
            ));
        }
        if batch.is_empty() {
            return Err(MemoryStoreError::Invalid(
                "empty Memory embedding attachment",
            ));
        }
        let mut input_bytes = 0;
        let mut grouped = BTreeMap::new();
        let mut unique = HashSet::new();
        for attachment in batch {
            native::check_write(original, admission)?;
            let key = (attachment.memory_id, attachment.revision.get());
            if !unique.insert(key) {
                return Err(MemoryStoreError::Invalid(
                    "duplicate Memory embedding attachment",
                ));
            }
            if capture.state.emitted.get(&key) != Some(&attachment.input_fingerprint) {
                return Err(MemoryStoreError::Invalid(
                    "unemitted Memory embedding attachment",
                ));
            }
            if attachment.vector.len() != capture.identity().dimensions().get() as usize
                || attachment.vector.iter().any(|v| !v.is_finite())
            {
                return Err(MemoryStoreError::Invalid("Memory embedding vector"));
            }
            charge(
                &mut input_bytes,
                attachment
                    .vector
                    .len()
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(56))
                    .ok_or(MemoryStoreError::Limit("embedding input bytes"))?,
                limits.max_input_bytes.get(),
                "embedding input bytes",
            )?;
            grouped
                .entry(attachment.memory_id)
                .or_insert_with(Vec::new)
                .push(attachment);
        }
        let writer = write::acquire(original, admission, limits, false)
            .await?
            .ok_or(MemoryStoreError::IncompleteCorpus)?;
        if writer.guard.corpus_key(writer.pin.corpus_uri())
            != capture
                .state
                .read
                .guard
                .corpus_key(capture.state.read.pin.corpus_uri())
        {
            return Err(MemoryStoreError::Invalid(
                "Memory embedding storage changed during capture",
            ));
        }
        let before = || {
            native::check_write(original, admission)?;
            capture.state.read.guard.revalidate(original)?;
            writer.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        before()?;
        // Validate physical rows to EOF before preparing any publication.
        capture::discover_ids(
            &writer.pin,
            original,
            SourceOwner::Project(project_id),
            limits.read,
            &before,
        )
        .await?;
        let mut updates = Vec::new();
        for (id, attachments) in grouped {
            let chain = history::chain(&writer.pin, original, id, project_id, limits.read, &before)
                .await?
                .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
            let existing = existing_rows(
                &super::query::EmbeddingScan {
                    pin: &writer.pin,
                    original,
                    owner: SourceOwner::Project(project_id),
                    limits: limits.read,
                },
                HistoryView::Project(&chain),
                &attachments,
                capture.identity(),
                &before,
            )
            .await?;
            for attachment in attachments {
                before()?;
                let source = source_at(HistoryView::Project(&chain), attachment.revision.get())
                    .ok_or(MemoryStoreError::Invalid("orphan Memory embedding"))?;
                let document = prepare_memory_embedding_document(source, capture.identity())?;
                if document.fingerprint() != &attachment.input_fingerprint {
                    return Err(MemoryStoreError::Invalid(
                        "Memory embedding source fingerprint",
                    ));
                }
                if !existing.contains(&attachment.revision.get()) {
                    updates.push(encode_memory_embedding_row(
                        project_id,
                        &document,
                        &attachment.vector,
                        limits,
                    )?);
                }
            }
        }
        before()?;
        if updates.is_empty() {
            return Ok(MemoryEmbeddingAttachOutcome::Existing {
                generation: native::generation(&writer.pin, project_id, project.version.version()),
            });
        }
        let pin = writer.pin.materialize().await?;
        before()?;
        let prepared = pin.prepare_merge(updates, vec!["row_key".into()]).await?;
        commit(prepared, project_id, project.version.version(), before).await
    }

    /// Observe the actual transaction without creating, materializing, replaying or repairing it.
    pub async fn resolve_embeddings(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        key: &DeltaRecoveryKey,
        limits: MemoryStoreWriteLimits,
    ) -> Result<DeltaResolution, MemoryStoreError> {
        native::check_write(original, admission)?;
        let project = admission.project()?;
        let branch = project.version.lance_branch();
        let physical = (!branch.is_empty()).then_some(branch.as_str());
        let selected = native::Selection::resolve(
            admission.record_storage().await?,
            project.scope.live,
            project.version.version(),
            limits.read.native,
            original,
        )?;
        if key.corpus_uri() != selected.uri() || key.physical_branch() != physical {
            return Err(BranchError::RecoveryBinding.into());
        }
        let Some(writer) = write::acquire_selected(original, admission, limits, selected).await?
        else {
            native::check_write(original, admission)?;
            return Ok(DeltaResolution::Unresolved);
        };
        let before = || {
            native::check_write(original, admission)?;
            writer.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        before()?;
        if writer.pin.inherited_target().is_some() {
            return Ok(DeltaResolution::Unresolved);
        }
        let observed = writer.pin.resolve(key).await?;
        before()?;
        Ok(observed)
    }
}

// One complete SDK scan, including ignored logical rows; vectors remain borrowed.
pub(crate) async fn existing_rows(
    scan: &super::query::EmbeddingScan<'_>,
    chain: HistoryView<'_>,
    attachments: &[MemoryEmbeddingAttachment],
    identity: &super::MemoryEmbeddingIdentity,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<HashSet<u64>, MemoryStoreError> {
    before()?;
    let mut scanner = scan.pin.scan();
    scanner
        .filter(&format!(
            "record_id = {}",
            native::literal(&chain.current().id.to_string())
        ))
        .map_err(StoreError::from)?
        .batch_size(scan.limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(scan.original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut keys = HashSet::new();
    let mut existing = HashSet::new();
    loop {
        before()?;
        let batch = crate::storage::read(scan.original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = batch else { break };
        rows::validate_schema(&batch, scan.owner.schema().as_ref())?;
        for row in 0..batch.num_rows() {
            before()?;

            if !keys.insert(rows::text(&batch, 0, row)?.to_owned()) {
                return Err(MemoryStoreError::Invalid("duplicate Memory row key"));
            }
            if scan.owner.decode(&batch, row, scan.original)?
                || rows::text(&batch, 1, row)? != "embedding"
            {
                continue;
            }
            let meta = metadata(&batch, row, scan.owner.layout())?;
            validate_source(&meta, chain)?;
            if let Some(input) = attachments
                .iter()
                .find(|a| a.revision.get() == meta.revision)
                && meta.identity == *identity
                && meta.fingerprint == input.input_fingerprint
                && vector(&batch, row, scan.owner)? == input.vector.as_slice()
            {
                existing.insert(meta.revision);
            }
        }
    }
    before()?;
    Ok(existing)
}
fn vector<'a>(
    batch: &'a RecordBatch,
    row: usize,
    owner: SourceOwner<'_>,
) -> Result<&'a [f32], MemoryStoreError> {
    let lists = batch
        .column(owner.column(23))
        .as_any()
        .downcast_ref::<ListArray>()
        .ok_or(MemoryStoreError::Invalid("Memory embedding list"))?;
    let values = lists
        .values()
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or(MemoryStoreError::Invalid("Memory embedding values"))?;
    let offsets = lists.value_offsets();
    Ok(&values.values()[offsets[row] as usize..offsets[row + 1] as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_data::lance_branch_store::BranchSnapshot;
    use std::error::Error;
    async fn pin() -> (tempfile::TempDir, BranchSnapshot) {
        let temp = tempfile::tempdir().unwrap();
        let uri = temp
            .path()
            .join("index.lance")
            .to_string_lossy()
            .into_owned();
        let schema = rows::schema();
        graphit_data::lance_module_store::LanceModuleStore::create(
            &uri,
            Arc::clone(&schema),
            vec![RecordBatch::new_empty(Arc::clone(&schema))],
        )
        .await
        .unwrap();
        let store = graphit_data::lance_branch_store::LanceBranchStore::open(&uri, schema, None)
            .await
            .unwrap();
        (temp, store.capture_target(None).await.unwrap())
    }
    fn row() -> RecordBatch {
        // Synthetic derived row: this fixture isolates the actual commit/result boundary.
        let memory = super::super::super::Memory {
            id: "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap(),
            revision: 1,
            title: "literal title".into(),
            body: "literal body".into(),
            tags: vec![],
            references: vec![],
            reference_generation: 1,
            created_by: ulid::Ulid::from(10u128).to_string(),
            updated_by: ulid::Ulid::from(10u128).to_string(),
            created_at: 1,
            updated_at: 1,
            attributes: crate::memory_attributes::MemoryAttributes {
                classification: crate::memory_attributes::MemoryClass::Fact,
                mandatory: false,
                important: false,
            },
        };
        let identity = super::super::MemoryEmbeddingIdentity::new(
            [1; 32],
            std::num::NonZeroU32::new(2).unwrap(),
            super::super::MEMORY_DOCUMENT_RECIPE.into(),
        )
        .unwrap();
        let document = prepare_memory_embedding_document(&memory, &identity).unwrap();
        let native = graphit_core::native_document_file::NativeDocumentFileLimits::new(
            1048576, 65536, 256, 8388608,
        )
        .unwrap();
        let read = super::super::super::MemoryStoreReadLimits {
            max_query_bytes: std::num::NonZeroUsize::new(4194304).unwrap(),
            batch_rows: std::num::NonZeroU32::new(16).unwrap(),
            native,
        };
        let copy =
            || graphit_core::artifact_store::CopyLimits::new(1048576, 128, 65536, 16).unwrap();
        let limits = MemoryStoreWriteLimits {
            read,
            artifacts: graphit_core::artifact_store::ArtifactLimits::new(1048576, 65536, copy())
                .unwrap(),
            max_input_bytes: std::num::NonZeroUsize::new(1048576).unwrap(),
            max_row_bytes: std::num::NonZeroUsize::new(2097152).unwrap(),
        };
        encode_memory_embedding_row(
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            &document,
            &[1., 0.],
            limits,
        )
        .unwrap()
    }
    #[tokio::test]
    async fn memory_embedding_actual_strict_conflict_preserves_both_keys_and_causes() {
        let (_temp, pin) = pin().await;
        let a = pin
            .prepare_merge(vec![row()], vec!["row_key".into()])
            .await
            .unwrap();
        let ak = a.recovery_key().clone();
        let b = pin
            .prepare_merge(vec![row()], vec!["row_key".into()])
            .await
            .unwrap();
        let bk = b.recovery_key().clone();
        let project = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        let MemoryEmbeddingAttachOutcome::Published(winner) =
            commit(a, project, "main", || Ok(())).await.unwrap()
        else {
            panic!("actual publication")
        };
        let loser = commit(b, project, "main", || Ok(())).await.unwrap_err();
        let MemoryStoreError::Commit(cause) = &loser else {
            panic!("actual SDK commit error")
        };
        assert_eq!(cause.recovery_key(), &bk);
        assert!(cause.source().is_some());
        assert!(loser.source().is_some());
        assert_eq!(winner.recovery_key, ak);
        assert_eq!(
            pin.resolve(&ak).await.unwrap(),
            DeltaResolution::Applied {
                version: winner.generation.sdk_version
            }
        );
        assert_eq!(
            pin.resolve(&bk).await.unwrap(),
            DeltaResolution::Conflicted {
                version: winner.generation.sdk_version
            }
        );
    }
    #[tokio::test]
    async fn memory_embedding_known_commit_postcheck_failure_preserves_publication_and_cause() {
        let (_temp, pin) = pin().await;
        let prepared = pin
            .prepare_merge(vec![row()], vec!["row_key".into()])
            .await
            .unwrap();
        let key = prepared.recovery_key().clone();
        let calls = std::cell::Cell::new(0);
        let error = commit(
            prepared,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            "main",
            || {
                let n = calls.get();
                calls.set(n + 1);
                if n == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "postpublication request canceled",
                    )
                    .into())
                }
            },
        )
        .await
        .unwrap_err();
        let MemoryStoreError::EmbeddingConfirmed { publication, cause } = &error else {
            panic!("known confirmed boundary")
        };
        assert_eq!(publication.recovery_key, key);
        assert!(
            matches!(cause.as_ref(),MemoryStoreError::Request(e) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        assert!(error.source().is_some());
        assert!(cause.source().is_some());
        assert_eq!(
            pin.resolve(&key).await.unwrap(),
            DeltaResolution::Applied {
                version: publication.generation.sdk_version
            }
        );
        assert!(publication.generation.sdk_version > pin.version());
    }
}
