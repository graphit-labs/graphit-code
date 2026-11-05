//! Admitted semantic batches with local vectors, exact comparison and real SDK EOF.
use super::*;
use crate::lance_module_store::SendableRecordBatchStream;
use futures::StreamExt;
use std::{future::Future, time::Duration};

#[derive(Default)]
struct Selection {
    slugs: BTreeSet<String>,
    saw_batch: bool,
}

/// Publish each validated vector batch independently. Model inference never owns
/// a writer; each merge uses current documentary rows and the SDK's native CAS.
/// A changed or removed document drops only its stale computed vector.
pub async fn publish_knowledge_embeddings_stream(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    capture: &KnowledgeEmbeddingCapture,
    identity: &KnowledgeEmbeddingIdentity,
    input: SendableRecordBatchStream,
    mode: KnowledgeEmbeddingWriteMode,
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeEmbeddingPublication, KnowledgeEmbeddingError> {
    capture.write(original, admission)?;
    // The producer advances the documentary cursor while supplying vector
    // batches. Certify its EOF only after the input stream finishes so earlier
    // batches can become searchable while inference is still in progress.
    validate_identity(identity, limits)?;
    if input.schema() != native_knowledge_embedding_schema(identity) {
        return Err(KnowledgeEmbeddingError::InvalidInput("schema"));
    }
    let same_width = capture
        .semantic
        .as_ref()
        .map(|semantic| width(semantic.pin.schema()))
        .transpose()?
        .is_some_and(|width| width == identity.dimensions.get() as i32);
    let identity_changed = capture
        .semantic_identity
        .as_ref()
        .is_some_and(|old| old != identity)
        || capture.semantic.is_some() && !same_width;
    if matches!(mode, KnowledgeEmbeddingWriteMode::Attach) && identity_changed {
        return Err(KnowledgeEmbeddingError::InvalidInput("reindex required"));
    }
    if capture.selected.is_some()
        && matches!(mode, KnowledgeEmbeddingWriteMode::ReplaceComplete)
        && capture.semantic.is_some()
        && !identity_changed
    {
        return Err(KnowledgeEmbeddingError::InvalidInput(
            "selected replacement requires a new identity",
        ));
    }
    let directory = knowledge_directory(&capture.scope, &capture.corpus, original, limits.native)?;
    let before = || {
        capture.write(original, admission)?;
        directory.revalidate(original)?;
        Ok(())
    };
    let mut input = input.fuse();
    let mut selection = Selection::default();
    let mut confirmed = None;
    let mut publication = KnowledgeEmbeddingPublication::Unchanged;
    let result = async {
        while let Some(frame) = guarded(input.next(), original, &before).await? {
            let batch =
                frame.map_err(|cause| KnowledgeEmbeddingError::InputStream(Box::new(cause)))?;
            let batches = select_batch(
                original,
                capture,
                identity,
                batch,
                mode,
                limits,
                same_width && !identity_changed,
                &mut selection,
                &before,
            )
            .await?;
            // Do not hold the writer while asking the producer for its next batch.
            let writer = acquire_writer(original, capture, limits).await?;
            let mut target = BranchSnapshot::capture_observed(
                &capture.corpus.uri,
                capture.branch.as_deref(),
                None,
            )
            .await?;
            before()?;
            if confirmed
                .as_ref()
                .is_some_and(|previous: &KnowledgeGeneration| previous.physical_branch.is_some())
                && target.inherited_target().is_some()
            {
                // A retired explicit branch must not be recreated by an in-flight batch.
                return Err(KnowledgeEmbeddingError::Conflict);
            }
            metadata::guard_target_identity(
                &target,
                original,
                if matches!(publication, KnowledgeEmbeddingPublication::Unchanged) {
                    capture.semantic_identity.as_ref()
                } else {
                    Some(identity)
                },
                identity,
                &before,
            )
            .await?;
            let mut attached = Vec::new();
            for batch in batches {
                let batch = attach_documents(
                    &target,
                    &capture.source.pin,
                    batch,
                    original,
                    identity,
                    mode,
                    limits.content,
                    &before,
                )
                .await?;
                if batch.num_rows() != 0 {
                    attached.push(batch);
                }
            }
            if !attached.is_empty() {
                target = materialize_target(target, &capture.corpus.uri, &mut confirmed, &before)
                    .await?;
                if identity_changed
                    && matches!(publication, KnowledgeEmbeddingPublication::Unchanged)
                    && index::semantic::dimensions(target.schema())?.is_some()
                {
                    target = target.drop_vector("vector", original).await?;
                    confirmed = Some(index::generation(&capture.corpus.uri, &target));
                }
                let canonical = index::semantic::schema(identity.dimensions)?;
                if index::semantic::dimensions(target.schema())?.is_none() {
                    let missing = canonical
                        .fields()
                        .iter()
                        .filter(|field| target.schema().field_with_name(field.name()).is_err())
                        .cloned()
                        .collect::<Vec<_>>();
                    target = target
                        .add_nullable_columns(Arc::new(Schema::new(missing)), original)
                        .await?;
                    confirmed = Some(index::generation(&capture.corpus.uri, &target));
                }
                before()?;
                let pending = target.prepare_merge(attached, vec!["slug".into()]).await?;
                before()?;
                let key = pending.recovery_key().clone();
                // Once started, observe the actual commit even if cancellation arrives.
                target = pending
                    .commit()
                    .await
                    .map_err(|cause| KnowledgeEmbeddingError::Delta(Box::new(cause)))?;
                confirmed = Some(index::generation(&capture.corpus.uri, &target));
                publication = KnowledgeEmbeddingPublication::Committed {
                    generation: generation(&target),
                    recovery: KnowledgeEmbeddingRecoveryKey::Delta(key),
                };
            }
            // Reused batches may repair a previously failed native index. An
            // inherited existing index must not materialize a branch just to probe it.
            if index::semantic::dimensions(target.schema())?.is_some()
                && (!identity_changed
                    || matches!(publication, KnowledgeEmbeddingPublication::Committed { .. }))
            {
                let count = guarded(
                    target
                        .dataset()
                        .count_rows(Some("vector IS NOT NULL".into())),
                    original,
                    &before,
                )
                .await??;
                if count != 0 {
                    let params =
                        lance::index::vector::VectorIndexParams::ivf_flat(1, DistanceType::Cosine);
                    let pending = match target
                        .prepare_vector_index(
                            "vector",
                            "knowledge_document_vector",
                            &params,
                            original,
                        )
                        .await
                    {
                        Ok(pending) => pending,
                        Err(BranchError::InheritedSnapshot) => {
                            target = materialize_target(
                                target,
                                &capture.corpus.uri,
                                &mut confirmed,
                                &before,
                            )
                            .await?;
                            target
                                .prepare_vector_index(
                                    "vector",
                                    "knowledge_document_vector",
                                    &params,
                                    original,
                                )
                                .await?
                        }
                        Err(cause) => {
                            return Err(KnowledgeEmbeddingError::IndexFailed {
                                publication: Box::new(publication),
                                cause: Box::new(cause.into()),
                            });
                        }
                    };
                    before()?;
                    if let Some(pending) = pending {
                        let key = pending.recovery_key().clone();
                        let next = pending.commit().await.map_err(|cause| {
                            KnowledgeEmbeddingError::IndexFailed {
                                publication: Box::new(publication),
                                cause: Box::new(KnowledgeEmbeddingError::Delta(Box::new(cause))),
                            }
                        })?;
                        confirmed = Some(index::generation(&capture.corpus.uri, &next));
                        publication = KnowledgeEmbeddingPublication::Committed {
                            generation: generation(&next),
                            recovery: KnowledgeEmbeddingRecoveryKey::Delta(key),
                        };
                    }
                }
            }
            drop(writer);
            before()?;
        }
        complete(capture, &selection, mode, &before)?;
        finish_publication(publication, before)
    }
    .await;
    result.map_err(|cause| corpus_failure(confirmed, cause))
}

async fn materialize_target(
    target: BranchSnapshot,
    uri: &str,
    confirmed: &mut Option<KnowledgeGeneration>,
    before: &impl Fn() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<BranchSnapshot, KnowledgeEmbeddingError> {
    before()?;
    let inherited = target.inherited_target().is_some();
    let target = target.materialize().await?;
    // materialize returns Ok only for this operation's own successful Clone,
    // or an already physical input pin. Observing another writer is an error.
    if inherited {
        *confirmed = Some(index::generation(uri, &target));
    }
    before()?;
    Ok(target)
}

fn corpus_failure(
    effect: Option<KnowledgeGeneration>,
    cause: KnowledgeEmbeddingError,
) -> KnowledgeEmbeddingError {
    match effect {
        Some(generation) => KnowledgeEmbeddingError::CorpusChanged {
            generation,
            cause: Box::new(cause),
        },
        None => cause,
    }
}

#[allow(clippy::too_many_arguments)]
async fn attach_documents(
    current: &BranchSnapshot,
    captured: &BranchSnapshot,
    batch: RecordBatch,
    original: &SnapshotContext,
    identity: &KnowledgeEmbeddingIdentity,
    mode: KnowledgeEmbeddingWriteMode,
    limits: index::KnowledgeIndexContentLimits,
    before: &impl Fn() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<RecordBatch, KnowledgeEmbeddingError> {
    let slugs = column::<StringArray>(&batch, 0, "slug")?;
    let schema = index::semantic::schema(identity.dimensions)?;
    let mut output = Vec::new();
    for row in 0..batch.num_rows() {
        original.check()?;
        let slug = slugs.value(row);
        let Some(found) = metadata::selected_row(current, slug, &mut || before()).await? else {
            continue;
        };
        let Some(previous) = metadata::selected_row(captured, slug, &mut || before()).await? else {
            return Err(KnowledgeEmbeddingError::InvalidInput(
                "slug outside capture",
            ));
        };
        // These are exactly the model's input-text fields. Metadata, references,
        // dates and other documents may advance without discarding this vector.
        if column::<UInt8Array>(&found, 16, "state")?.value(0)
            != crate::knowledge_references::KnowledgeDocumentState::Active as u8
            || [1, 2, 3, 4, 5]
                .iter()
                .any(|at| found.column(*at) != previous.column(*at))
        {
            continue;
        }
        let vector = batch.slice(row, 1);
        // Nullable vector evolution can retain identity metadata without a vector.
        if found.num_columns() == 23
            && found.column(22).is_valid(0)
            && found.columns()[18..22]
                .iter()
                .zip(&vector.columns()[1..5])
                .all(|(stored, supplied)| stored == supplied)
        {
            if found.column(22) == vector.column(5) {
                continue;
            }
            if matches!(mode, KnowledgeEmbeddingWriteMode::Attach) {
                return Err(KnowledgeEmbeddingError::InvalidInput("already embedded"));
            }
        }
        let mut columns = found.columns()[..18].to_vec();
        columns.extend(vector.columns()[1..].iter().cloned());
        let joined = RecordBatch::try_new(Arc::clone(&schema), columns)?;
        index::semantic::validate(&joined, limits, || {
            before().map_err(|cause| index::KnowledgeIndexError::Request(io::Error::other(cause)))
        })?;
        output.push(joined);
    }
    before()?;
    lance::deps::datafusion::arrow::compute::concat_batches(&schema, &output)
        .map_err(KnowledgeEmbeddingError::from)
}

async fn acquire_writer(
    original: &Arc<SnapshotContext>,
    capture: &KnowledgeEmbeddingCapture,
    limits: KnowledgeIndexWriteLimits,
) -> Result<graphit_core::artifact_store::ArtifactWriter, KnowledgeEmbeddingError> {
    let artifacts = ArtifactStore::new(capture.scope.clone(), limits.artifacts);
    let request = Arc::clone(original);
    Ok(
        tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request))
            .await
            .map_err(index::KnowledgeIndexError::WriterAcquisition)??,
    )
}

fn validate_identity(
    identity: &KnowledgeEmbeddingIdentity,
    limits: KnowledgeIndexWriteLimits,
) -> Result<(), KnowledgeEmbeddingError> {
    if identity.projection_recipe.len() > limits.content.max_identifier_bytes.get() {
        return Err(KnowledgeEmbeddingError::InvalidInput("recipe"));
    }
    Ok(())
}

fn complete(
    capture: &KnowledgeEmbeddingCapture,
    selection: &Selection,
    mode: KnowledgeEmbeddingWriteMode,
    before: &impl Fn() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<(), KnowledgeEmbeddingError> {
    before()?;
    if !selection.saw_batch {
        return Err(KnowledgeEmbeddingError::InvalidInput("stream empty"));
    }
    capture.complete_documents()?;
    if matches!(mode, KnowledgeEmbeddingWriteMode::ReplaceComplete)
        && selection.slugs != capture.source_slugs
    {
        return Err(KnowledgeEmbeddingError::InvalidInput("incomplete inputs"));
    }
    before()
}

#[allow(clippy::too_many_arguments)]
async fn select_batch(
    original: &Arc<SnapshotContext>,
    capture: &KnowledgeEmbeddingCapture,
    identity: &KnowledgeEmbeddingIdentity,
    batch: RecordBatch,
    mode: KnowledgeEmbeddingWriteMode,
    limits: KnowledgeIndexWriteLimits,
    same_width: bool,
    selection: &mut Selection,
    before: &impl Fn() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<Vec<RecordBatch>, KnowledgeEmbeddingError> {
    before()?;
    if batch.schema() != native_knowledge_embedding_schema(identity)
        || batch
            .columns()
            .iter()
            .any(|column| column.null_count() != 0)
    {
        return Err(KnowledgeEmbeddingError::InvalidInput("schema/null"));
    }
    let slugs: &StringArray = column(&batch, 0, "slug")?;
    let fingerprints: &FixedSizeBinaryArray = column(&batch, 1, "fingerprint")?;
    let configs: &FixedSizeBinaryArray = column(&batch, 2, "config")?;
    let recipes: &StringArray = column(&batch, 3, "recipe")?;
    let roles: &StringArray = column(&batch, 4, "role")?;
    let vectors: &FixedSizeListArray = column(&batch, 5, "vector")?;
    // These are local Arrow views. No vector contents are copied into a corpus image.
    let values: Vec<_> = (0..batch.num_rows())
        .map(|row| vectors.value(row))
        .collect();
    let mut rows = Vec::with_capacity(batch.num_rows());
    for (row, value) in values.iter().enumerate() {
        before()?;
        let slug = slugs.value(row);
        if slug.is_empty()
            || slug.len() > limits.content.max_identifier_bytes.get()
            || !capture.source_slugs.contains(slug)
            || !selection.slugs.insert(slug.to_owned())
        {
            return Err(KnowledgeEmbeddingError::InvalidInput("slugs"));
        }
        if configs.value(row) != identity.vector_id
            || recipes.value(row) != identity.projection_recipe
            || roles.value(row) != "Document"
        {
            return Err(KnowledgeEmbeddingError::InvalidInput("identity"));
        }
        let vector = value
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or(KnowledgeEmbeddingError::InvalidInput("vector"))?;
        if vector.null_count() != 0
            || vector.len() != identity.dimensions.get() as usize
            || vector.values().iter().any(|v| !v.is_finite())
        {
            return Err(KnowledgeEmbeddingError::InvalidInput("vector"));
        }
        let fingerprint = fingerprints
            .value(row)
            .try_into()
            .map_err(|_| KnowledgeEmbeddingError::InvalidInput("fingerprint"))?;
        rows.push(KnowledgeEmbeddingRow {
            input: KnowledgeEmbeddingInput {
                slug,
                input_fingerprint: fingerprint,
            },
            vector: vector.values(),
        });
    }
    selection.saw_batch = true;
    let equal = matching_stored_rows(capture, original, identity, &rows, mode, before).await?;
    before()?;
    if !same_width {
        return Ok(vec![batch]);
    }
    // Slices retain only the current input batch. Equal rows never reach SDK merge.
    let changed: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !equal.contains(row.input.slug))
        .map(|(at, _)| batch.slice(at, 1))
        .collect();
    Ok(changed)
}

async fn guarded<T>(
    future: impl Future<Output = T>,
    original: &SnapshotContext,
    before: &impl Fn() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<T, KnowledgeEmbeddingError> {
    before()?;
    tokio::pin!(future);
    let mut timer = tokio::time::interval(Duration::from_millis(25));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            value = &mut future => { before()?; return Ok(value); }
            _ = original.cancellation().cancelled() => { before()?; }
            _ = timer.tick() => { before()?; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn inherited_clone_receipt_survives_cancel_but_never_claims_another_writers_branch() {
        let cancel = Cancellation::default();
        let original = SnapshotContext::service(cancel.clone());
        let temporary = tempfile::tempdir().unwrap();
        let uri = temporary
            .path()
            .join("index.lance")
            .to_str()
            .unwrap()
            .to_owned();
        crate::lance_module_store::LanceModuleStore::create(
            &uri,
            index::physical_schema(),
            vec![index::semantic::tests::document(&original, "body")],
        )
        .await
        .unwrap();
        let inherited = BranchSnapshot::capture_observed(&uri, Some("child"), None)
            .await
            .unwrap();
        assert!(inherited.physical_branch().is_none());
        assert_eq!(inherited.inherited_target(), Some("child"));
        let version = inherited.version();
        let calls = AtomicUsize::new(0);
        let mut confirmed = None;
        let error = materialize_target(inherited, &uri, &mut confirmed, &|| {
            if calls.fetch_add(1, Ordering::SeqCst) == 1 {
                cancel.cancel();
            }
            original
                .check()
                .map_err(index::KnowledgeIndexError::from)
                .map_err(Into::into)
        })
        .await
        .err()
        .expect("post-Clone guard must observe cancellation");
        let error = corpus_failure(confirmed, error);
        let KnowledgeEmbeddingError::CorpusChanged { generation, cause } = error else {
            panic!("confirmed Clone receipt must survive its post-effect guard")
        };
        assert_eq!(generation.version, version);
        assert_eq!(generation.physical_branch.as_deref(), Some("child"));
        assert!(generation.inherited_target.is_none());
        assert!(
            matches!(*cause, KnowledgeEmbeddingError::Index(index::KnowledgeIndexError::Request(error))
            if error.kind() == io::ErrorKind::Interrupted)
        );
        let actual = BranchSnapshot::capture_observed(&uri, Some("child"), None)
            .await
            .unwrap();
        assert_eq!(actual.physical_branch(), Some("child"));
        assert_eq!(index::semantic::dimensions(actual.schema()).unwrap(), None);
        assert_eq!(actual.dataset().count_rows(None).await.unwrap(), 1);

        let never = BranchSnapshot::capture_observed(&uri, Some("never"), None)
            .await
            .unwrap();
        let mut no_effect = None;
        assert!(
            materialize_target(never, &uri, &mut no_effect, &|| original
                .check()
                .map_err(index::KnowledgeIndexError::from)
                .map_err(Into::into))
            .await
            .is_err()
        );
        assert!(no_effect.is_none());
        let never = BranchSnapshot::capture_observed(&uri, Some("never"), None)
            .await
            .unwrap();
        assert_eq!(never.inherited_target(), Some("never"));

        let other = BranchSnapshot::capture_observed(&uri, Some("other"), None)
            .await
            .unwrap();
        other.clone().materialize().await.unwrap(); // A different actual SDK writer.
        let mut not_ours = None;
        assert!(matches!(
            materialize_target(other, &uri, &mut not_ours, &|| Ok(())).await,
            Err(KnowledgeEmbeddingError::Branch(_))
        ));
        assert!(
            not_ours.is_none(),
            "observing a branch is not our publication receipt"
        );
    }
}
