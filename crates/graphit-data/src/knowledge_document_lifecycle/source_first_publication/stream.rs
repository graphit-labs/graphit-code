//! Validate ordinary batches under the same writer and finish the real SDK Create.
use super::*;
use crate::lance_module_store::{
    DataFusionError, RecordBatchStreamAdapter, SendableRecordBatchStream,
};
use arrow_array::{StringArray, UInt8Array};
use futures::{StreamExt, stream};
use std::{collections::BTreeSet, future::Future, time::Duration};
use tokio::sync::mpsc;

/// Consume a complete canonical18 selection before committing an exclusive Create.
/// Batches grant no authority and do not prove their relation to the captured Source.
/// The caller keeps that selection proof. A late failure publishes no prefix;
/// staged SDK files may remain. An initiated commit is always awaited.
pub async fn publish_native_knowledge_source_stream(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
    input: SendableRecordBatchStream,
    limits: KnowledgeIndexWriteLimits,
) -> Result<(KnowledgeFirstPublication, RecoveryKey), KnowledgeSourceFirstPublicationError> {
    publish_stream(original, read, write, source, input, None, limits).await
}

/// First publication of one byte-exact captured FILE, available before corpus EOF.
pub async fn publish_native_knowledge_source_file(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
    batch: arrow_array::RecordBatch,
    limits: KnowledgeIndexWriteLimits,
) -> Result<(KnowledgeFirstPublication, RecoveryKey), KnowledgeSourceFirstPublicationError> {
    let input = Box::pin(RecordBatchStreamAdapter::new(
        index::physical_schema(),
        stream::iter([Ok(batch.clone())]),
    ));
    publish_stream(original, read, write, source, input, Some(&batch), limits).await
}

async fn publish_stream(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
    mut input: SendableRecordBatchStream,
    file: Option<&arrow_array::RecordBatch>,
    limits: KnowledgeIndexWriteLimits,
) -> Result<(KnowledgeFirstPublication, RecoveryKey), KnowledgeSourceFirstPublicationError> {
    use KnowledgeSourcePublicationEffect as Effect;
    use KnowledgeSourcePublicationPhase as Phase;
    let fail = |phase, cause| failure(phase, Effect::BeforeCommit, None, None, cause);
    check_authority(original, read, write, source).map_err(|e| fail(Phase::Admission, e))?;
    if input.schema() != index::physical_schema() {
        return Err(fail(
            Phase::Image,
            KnowledgeIndexError::InvalidField("schema"),
        ));
    }
    let target = acquire_target(original, read, write, source, limits, &|_| {}).await?;
    let before = || check_target(original, read, write, source, &target);
    check_physical(original, read, write, source, &target, file)
        .map_err(|e| fail(Phase::SourceBeforePrepare, e))?;
    let project = write
        .project()
        .map_err(KnowledgeIndexError::from)
        .map_err(|e| fail(Phase::Admission, e))?;
    let graphit_core::artifact_store::StoreOwner::Project { project_id } =
        project.scope.live.owner()
    else {
        return Err(fail(Phase::Admission, KnowledgeIndexError::WrongAdmission));
    };
    // Admissions cannot be detached into the SDK's 'static ordinary stream.
    // A joined producer holds them and the real writer; only owned batches cross.
    let (sender, receiver) = mpsc::channel(1);
    let sdk_input = Box::pin(RecordBatchStreamAdapter::new(
        index::physical_schema(),
        stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|item| (item, receiver))
        }),
    ));
    let validate = async {
        let mut namespace = index::NamespaceValidation::bound(Some(*project_id));
        let mut slugs = BTreeSet::new();
        let mut origins = BTreeSet::new();
        let mut seen_batch = false;
        let result: Result<(), KnowledgeIndexError> = async {
            loop {
                let frame = guarded(input.next(), original, &before).await?;
                let Some(frame) = frame else { break };
                let batch = match frame {
                    Ok(batch) => batch,
                    Err(cause) => {
                        // Keep the original domain/I/O object locally; SDK wrapping
                        // is not assumed to preserve its concrete source type.
                        return Err(KnowledgeIndexError::InputStream(Box::new(cause)));
                    }
                };
                index::validate_full_batch(&batch, limits.content, &mut namespace, &before)?;
                let names: &StringArray = index::array(&batch, 0)?;
                let paths: &StringArray = index::array(&batch, 7)?;
                let states: &UInt8Array = index::array(&batch, 16)?;
                for row in 0..batch.num_rows() {
                    before()?;
                    if states.value(row)
                        != crate::knowledge_references::KnowledgeDocumentState::Active as u8
                    {
                        return Err(KnowledgeIndexError::InvalidField("initial_document_state"));
                    }
                    if !slugs.insert(names.value(row).to_owned()) {
                        return Err(KnowledgeIndexError::DuplicateSlug);
                    }
                    if !origins.insert(paths.value(row).to_owned()) {
                        return Err(KnowledgeIndexError::InvalidField("duplicate_source_path"));
                    }
                }
                seen_batch = true;
                if guarded(sender.send(Ok(batch)), original, &before)
                    .await?
                    .is_err()
                {
                    // The consumer's actual SDK failure owns this outcome.
                    return Ok(());
                }
            }
            if !seen_batch {
                return Err(KnowledgeIndexError::InvalidField("stream_empty"));
            }
            check_physical(original, read, write, source, &target, file)
        }
        .await;
        let local_cause = result.err();
        if local_cause.is_some() {
            // A control signal aborts SDK preparation. The typed local cause is
            // returned below, never reported as a fabricated SDK failure.
            let _ = sender
                .send(Err(DataFusionError::Execution(
                    "Knowledge stream rejected by its admitted validator".into(),
                )))
                .await;
        }
        drop(sender);
        (local_cause, slugs)
    };
    let prepare =
        LanceModuleStore::prepare_create_stream(&target.uri, index::physical_schema(), sdk_input);
    let ((local_cause, slugs), prepared) = tokio::join!(validate, prepare);
    if let Some(cause) = local_cause {
        return Err(fail(Phase::Image, cause));
    }
    let mut prepared = prepared.map_err(|cause| fail(Phase::Prepare, cause.into()))?;
    let changes = crate::knowledge_sync_history::KnowledgeSyncChanges {
        added: slugs.into_iter().collect(),
        ..Default::default()
    };
    let properties = changes
        .into_properties()
        .map_err(|cause| fail(Phase::Image, cause))?;
    prepared.set_transaction_properties(properties);
    finish_prepared(
        original,
        read,
        write,
        source,
        target,
        prepared,
        file,
        &|_| {},
    )
    .await
}

// Waiting for an ordinary producer or backpressure must still observe the same
// request and admission revocation. This timer checks liveness; it never retries
// I/O, renews a grant, or supplies a replacement request/deadline.
async fn guarded<T>(
    future: impl Future<Output = T>,
    original: &SnapshotContext,
    before: &impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<T, KnowledgeIndexError> {
    before()?;
    tokio::pin!(future);
    // The pre-check already covers entry. Start periodic revocation checks
    // after an actual wait instead of duplicating it with an immediate tick.
    let period = Duration::from_millis(25);
    let mut checks = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            value = &mut future => { before()?; return Ok(value); }
            _ = original.cancellation().cancelled() => { before()?; }
            _ = checks.tick() => { before()?; }
        }
    }
}
