//! Existing-corpus delta with exact local comparisons and an EOF-gated SDK merge.
use super::*;
use crate::lance_branch_store::{BranchSnapshot, col, lit};
use crate::lance_module_store::{
    DataFusionError, RecordBatchStreamAdapter, SendableRecordBatchStream,
};
use arrow_array::Array;
use futures::{StreamExt, stream};
use std::{future::Future, time::Duration};
use tokio::sync::mpsc;

type Result<T> = std::result::Result<T, KnowledgeDocumentLifecycleError>;

fn copy(value: &str) -> Result<String> {
    let mut result = String::new();
    result.try_reserve_exact(value.len()).map_err(allocation)?;
    result.push_str(value);
    Ok(result)
}

pub(super) async fn guarded<T>(
    future: impl Future<Output = T>,
    original: &SnapshotContext,
    before: &impl Fn() -> Result<()>,
) -> Result<T> {
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

async fn acquire_writer(
    original: &Arc<SnapshotContext>,
    scope: &graphit_core::artifact_store::CapturedPublicationScope,
    limits: KnowledgeIndexWriteLimits,
    before: &impl Fn() -> Result<()>,
) -> Result<graphit_core::artifact_store::ArtifactWriter> {
    before()?;
    let artifacts = ArtifactStore::new(scope.clone(), limits.artifacts);
    let request = Arc::clone(original);
    let acquisition = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request));
    let writer = acquisition
        .await
        .map_err(KnowledgeIndexError::WriterAcquisition)?
        .map_err(KnowledgeIndexError::from)?;
    before()?;
    Ok(writer)
}

pub(super) async fn lookup(
    pin: &BranchSnapshot,
    slug: &str,
    original: &SnapshotContext,
    before: &impl Fn() -> Result<()>,
) -> Result<RecordBatch> {
    let mut scanner = pin.scan();
    scanner.filter_expr(col("slug").eq(lit(slug)));
    let mut input = guarded(scanner.try_into_stream(), original, before)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    let mut found = None;
    while let Some(batch) = guarded(input.next(), original, before).await? {
        let batch = batch
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
        if batch.num_rows() == 0 {
            continue;
        }
        if batch.num_rows() != 1 || found.is_some() {
            return Err(KnowledgeIndexError::DuplicateSlug.into());
        }
        found = Some(batch);
    }
    found.ok_or_else(|| KnowledgeIndexError::InvalidField("captured_slug_missing").into())
}

struct Desired {
    input: SendableRecordBatchStream,
    pending: Option<(RecordBatch, usize)>,
    counts: index::NamespaceValidation,
    origins: HashSet<String>,
    seen: HashSet<String>,
    omitted: std::vec::IntoIter<String>,
    eof: bool,
    ordinal: usize,
    sync_changes: crate::knowledge_sync_history::KnowledgeSyncChanges,
    body_equivalent: index::KnowledgeBodyEquivalence,
}
impl Desired {
    #[allow(clippy::too_many_arguments)]
    async fn next_patch(
        &mut self,
        pin: &BranchSnapshot,
        physical: &HashMap<String, (bool, bool)>,
        selected_origins: &HashMap<&str, String>,
        project: graphit_core::project_identity::ProjectId,
        original: &Arc<SnapshotContext>,
        limits: KnowledgeDocumentLifecycleLimits,
        before: &impl Fn() -> Result<()>,
        data_check: &impl Fn() -> std::result::Result<(), KnowledgeIndexError>,
    ) -> Result<Option<RecordBatch>> {
        loop {
            before()?;
            if let Some((batch, row)) = self.pending.take() {
                let desired = batch.slice(row, 1);
                if row + 1 < batch.num_rows() {
                    self.pending = Some((batch, row + 1));
                }
                let slug = index::array::<StringArray>(&desired, 0)?.value(0);
                let origin = index::array::<StringArray>(&desired, 7)?.value(0);
                if parse_managed_knowledge_source_path(origin)
                    .map_err(KnowledgeDocumentLifecycleError::Identity)?
                    .is_none()
                    || index::array::<UInt8Array>(&desired, 16)?.value(0)
                        != KnowledgeDocumentState::Active as u8
                {
                    return Err(KnowledgeDocumentLifecycleError::InvalidInput(
                        "desired_source_class",
                    ));
                }
                self.seen.try_reserve(1).map_err(allocation)?;
                if !self.seen.insert(copy(slug)?) {
                    return Err(KnowledgeIndexError::DuplicateSlug.into());
                }
                self.origins.try_reserve(1).map_err(allocation)?;
                if !self.origins.insert(copy(origin)?) {
                    return Err(KnowledgeDocumentLifecycleError::InvalidInput(
                        "desired_duplicate_origin",
                    ));
                }
                let ordinal = self.ordinal;
                self.ordinal = self
                    .ordinal
                    .checked_add(1)
                    .ok_or(KnowledgeIndexError::Overflow("pages"))?;
                let Some(&(managed, active)) = physical.get(slug) else {
                    reserve(&mut self.sync_changes.added, 1)?;
                    self.sync_changes.added.push(copy(slug)?);
                    // A metadata ID change keeps the same selected physical source.
                    // Retain certified semantics only through its unique active origin;
                    // the old slug still follows ordinary retirement, never an alias.
                    let next = if let Some(previous) = selected_origins.get(origin) {
                        let old = lookup(pin, previous, original, before).await?;
                        index::semantic::replace_document(
                            &desired,
                            &old,
                            original,
                            self.body_equivalent,
                        )?
                    } else {
                        index::semantic::align(&desired, pin.schema())?
                    };
                    return Ok(Some(next));
                };
                if !managed {
                    return Err(KnowledgeDocumentLifecycleError::ForeignSlugConflict {
                        desired: ordinal,
                    });
                }
                let old = lookup(pin, slug, original, before).await?;
                let mut desired = desired;
                if active && index::array::<StructArray>(&desired, 17)?.is_null(0) {
                    let wiki_owner = index::array::<ListArray>(&desired, 15)?.value(0);
                    let wiki = wiki_owner
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
                    let mut targets = Vec::new();
                    reserve(&mut targets, wiki.len())?;
                    targets.extend((0..wiki.len()).map(|at| wiki.value(at)));
                    let old_wiki_owner = index::array::<ListArray>(&old, 15)?.value(0);
                    let old_wiki = old_wiki_owner
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
                    let projection = refs::preserve_projection(
                        witness_header(&desired, 0)?,
                        &targets,
                        ReferenceRowInput {
                            projection: index::array::<StructArray>(&old, 17)?,
                            state: KnowledgeDocumentState::Active as u8,
                            row: 0,
                            header: witness_header(&old, 0)?,
                            wiki: old_wiki,
                            expected_project: Some(project),
                        },
                        limits.write.content,
                        data_check,
                    )?;
                    let mut columns = desired.columns().to_vec();
                    columns[17] = projection;
                    desired = RecordBatch::try_new(index::physical_schema(), columns)
                        .map_err(KnowledgeIndexError::from)?;
                }
                before()?;
                let desired = index::semantic::replace_document(
                    &desired,
                    &old.slice(0, 1),
                    original,
                    self.body_equivalent,
                )?;
                if desired != old.slice(0, 1) {
                    let slug = index::array::<StringArray>(&desired, 0)?.value(0);
                    let changes = if active {
                        &mut self.sync_changes.updated
                    } else {
                        &mut self.sync_changes.added
                    };
                    reserve(changes, 1)?;
                    changes.push(copy(slug)?);
                    return Ok(Some(desired));
                }
                continue;
            }
            if !self.eof {
                if let Some(frame) = guarded(self.input.next(), original, before).await? {
                    let batch =
                        frame.map_err(|cause| KnowledgeIndexError::InputStream(Box::new(cause)))?;
                    index::validate_full_batch(
                        &batch,
                        limits.write.content,
                        &mut self.counts,
                        data_check,
                    )?;
                    if batch.num_rows() != 0 {
                        self.pending = Some((batch, 0));
                    }
                    continue;
                }
                self.eof = true;
            }
            for slug in self.omitted.by_ref() {
                before()?;
                if self.seen.contains(&slug) {
                    continue;
                }
                let old = lookup(pin, &slug, original, before).await?;
                let projection = refs::encode_removed_projection(
                    witness_header(&old, 0)?,
                    project,
                    limits.write.content,
                    data_check,
                )?;
                let mut columns = old.slice(0, 1).columns().to_vec();
                let mut empty = ListBuilder::new(StringBuilder::new()).with_field(Field::new(
                    "item",
                    DataType::Utf8,
                    false,
                ));
                empty.append(true);
                columns[15] = Arc::new(empty.finish());
                columns[16] = Arc::new(UInt8Array::from(vec![
                    KnowledgeDocumentState::Removed as u8,
                ]));
                columns[17] = projection;
                reserve(&mut self.sync_changes.removed, 1)?;
                self.sync_changes.removed.push(slug);
                return Ok(Some(index::semantic::align(
                    &RecordBatch::try_new(index::physical_schema(), columns[..18].to_vec())
                        .map_err(KnowledgeIndexError::from)?,
                    pin.schema(),
                )?));
            }

            before()?;
            return Ok(None);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn reconcile_stream(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    source: &CapturedSourceReader,
    input: SendableRecordBatchStream,
    selection: (&HashSet<String>, &HashSet<String>),
    file: Option<&RecordBatch>,
    limits: KnowledgeDocumentLifecycleLimits,
    body_equivalent: index::KnowledgeBodyEquivalence,
) -> Result<KnowledgeDocumentUpdateResult> {
    check_authority(original, read, write)?;
    source
        .check_request(original)
        .map_err(KnowledgeDocumentLifecycleError::Source)?;
    if input.schema() != index::physical_schema() {
        return Err(KnowledgeIndexError::InvalidField("schema").into());
    }
    let project = write.project().map_err(KnowledgeIndexError::from)?;
    let scope = project.scope.live;
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission.into());
    };
    let project_id = *project_id;
    let branch = project.version.lance_branch();
    let corpus = index::local_corpus(
        scope,
        project.version.version(),
        original,
        limits.write.native,
    )?;
    let uri = corpus.uri.clone();
    let check = || -> std::result::Result<(), KnowledgeIndexError> {
        index::check_read(original, read)?;
        index::check_write(original, write)?;
        source
            .check_request(original)
            .map_err(KnowledgeIndexError::Request)
    };
    check()?;
    check_authority(original, read, write)?;
    let Some(guard) = index::capture_index(scope, &corpus, original, limits.read.native, check)?
    else {
        check()?;
        return Err(KnowledgeDocumentLifecycleError::NoCorpus);
    };
    let Some(write_guard) =
        index::capture_index(scope, &corpus, original, limits.write.native, check)?
    else {
        return Err(KnowledgeIndexError::InvalidField("corpus_capture_changed").into());
    };
    let data_check = || -> std::result::Result<(), KnowledgeIndexError> {
        check()?;
        guard.revalidate(original)?;
        write_guard.revalidate(original)?;
        Ok(())
    };
    let before = || -> Result<()> {
        data_check()?;
        Ok(())
    };
    let pin = guarded(
        crate::lance_branch_store::BranchSnapshot::capture_observed(
            &uri,
            (!branch.is_empty()).then_some(branch.as_str()),
            None,
        ),
        original,
        &before,
    )
    .await?
    .map_err(KnowledgeIndexError::from)?;
    index::semantic::dimensions(pin.schema())?;
    let generation = index::generation(&uri, &pin);
    if &generation != expected {
        return Err(KnowledgeDocumentLifecycleError::GenerationChanged {
            observed: generation,
        });
    }
    let check_head = || async {
        let current = guarded(
            crate::lance_branch_store::BranchSnapshot::capture_observed(
                &uri,
                (!branch.is_empty()).then_some(branch.as_str()),
                None,
            ),
            original,
            &before,
        )
        .await?
        .map_err(KnowledgeIndexError::from)?;
        let observed = index::generation(&uri, &current);
        if &observed != expected {
            return Err(KnowledgeDocumentLifecycleError::GenerationChanged { observed });
        }
        before()
    };
    let mut scanner = pin.scan();
    scanner.batch_size(limits.read.batch_rows.get() as usize);
    let (origins, slugs) = selection;
    scanner.filter_expr(
        col("source_path")
            .in_list(origins.iter().map(|v| lit(v.as_str())).collect(), false)
            .or(col("slug").in_list(slugs.iter().map(|v| lit(v.as_str())).collect(), false)),
    );
    let mut old = guarded(scanner.try_into_stream(), original, &before)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    let mut counts = index::NamespaceValidation::bound(Some(project_id));
    let mut physical = HashMap::new();
    let mut selected_origins = HashMap::new();
    let mut transferred_origin = false;
    while let Some(frame) = guarded(old.next(), original, &before).await? {
        let batch = frame
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
        index::validate_full_batch(&batch, limits.read.content, &mut counts, data_check)?;

        for row in 0..batch.num_rows() {
            before()?;
            let slug = index::array::<StringArray>(&batch, 0)?.value(row);
            let path = index::array::<StringArray>(&batch, 7)?.value(row);
            let managed = parse_managed_knowledge_source_path(path)
                .map_err(KnowledgeDocumentLifecycleError::Identity)?
                .is_some();
            let active = index::array::<UInt8Array>(&batch, 16)?.value(row)
                == KnowledgeDocumentState::Active as u8;
            let (origins, requested) = selection;
            if requested.contains(slug) && !origins.contains(path) {
                let relative = managed
                    .then(|| knowledge_source_relative_path(path))
                    .transpose()?;
                let transferable = match relative {
                    Some(relative) => {
                        source
                            .covers_selected_path(&relative)
                            .map_err(KnowledgeDocumentLifecycleError::Source)?
                            && !source.nodes().any(|node| {
                                node.kind() == graphit_fs::NodeKind::File
                                    && node.relative() == relative.as_path()
                            })
                    }
                    None => false,
                };
                if !transferable {
                    return Err(KnowledgeDocumentLifecycleError::ForeignSlugConflict {
                        desired: 0,
                    });
                }
                // Only an absence within the actual captured selection permits
                // a rename. An intact FILE outside an incremental capture is
                // not an absence. Revalidate that selection before commit.
                transferred_origin = true;
            }
            if managed
                && active
                && let Some(origin) = origins.get(path)
            {
                selected_origins.try_reserve(1).map_err(allocation)?;
                if selected_origins
                    .insert(origin.as_str(), copy(slug)?)
                    .is_some()
                {
                    return Err(KnowledgeDocumentLifecycleError::InvalidInput(
                        "duplicate_origin",
                    ));
                }
            }
            physical.try_reserve(1).map_err(allocation)?;
            if physical.insert(copy(slug)?, (managed, active)).is_some() {
                return Err(KnowledgeIndexError::DuplicateSlug.into());
            }
        }
    }
    drop(old);
    drop(scanner);
    let mut omitted = Vec::new();
    reserve(&mut omitted, selected_origins.len())?;
    for slug in selected_origins.values() {
        omitted.push(copy(slug)?);
    }
    omitted.sort_unstable();
    let validate_source = || -> Result<()> {
        before()?;
        if let Some(file) = file {
            validate_source_file(original, source, file)?;
            if transferred_origin {
                source
                    .revalidate_bindings()
                    .map_err(KnowledgeDocumentLifecycleError::Source)?;
            }
        } else {
            source
                .revalidate_bindings()
                .map_err(KnowledgeDocumentLifecycleError::Source)?;
        }
        before()
    };
    let mut desired = Desired {
        input,
        pending: None,
        counts: index::NamespaceValidation::bound(Some(project_id)),
        origins: HashSet::new(),
        seen: HashSet::new(),
        omitted: omitted.into_iter(),
        eof: false,
        ordinal: 0,
        sync_changes: Default::default(),
        body_equivalent,
    };
    let first = desired
        .next_patch(
            &pin,
            &physical,
            &selected_origins,
            project_id,
            original,
            limits,
            &before,
            &data_check,
        )
        .await?;
    validate_source()?;
    let Some(first) = first else {
        let writer = acquire_writer(original, scope, limits.write, &before).await?;
        check_head().await?;
        validate_source()?;
        drop(writer);
        return Ok(KnowledgeDocumentUpdateResult::Unchanged { generation });
    };
    if pin.inherited_target().is_some() {
        return Err(KnowledgeDocumentLifecycleError::InheritedTarget);
    }
    let (sender, receiver) = mpsc::channel(1);
    let sdk_input = Box::pin(RecordBatchStreamAdapter::new(
        Arc::new(pin.schema().clone()),
        stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|frame| (frame, receiver))
        }),
    ));
    let validate = async {
        let result: Result<()> = async {
            if guarded(sender.send(Ok(first)), original, &before)
                .await?
                .is_err()
            {
                return Ok(());
            }
            while let Some(batch) = desired
                .next_patch(
                    &pin,
                    &physical,
                    &selected_origins,
                    project_id,
                    original,
                    limits,
                    &before,
                    &data_check,
                )
                .await?
            {
                if guarded(sender.send(Ok(batch)), original, &before)
                    .await?
                    .is_err()
                {
                    return Ok(());
                }
            }
            validate_source()
        }
        .await;
        let cause = result.err();
        if cause.is_some() {
            let _ = sender
                .send(Err(DataFusionError::Execution(
                    "Knowledge reconciliation rejected by its admitted validator".into(),
                )))
                .await;
        }
        drop(sender);
        cause
    };
    let prepare = pin.prepare_merge_stream(sdk_input, vec!["slug".into()], None);
    let (cause, prepared) = tokio::join!(validate, prepare);
    if let Some(cause) = cause {
        return Err(cause);
    }
    let mut prepared = prepared.map_err(KnowledgeIndexError::from)?;
    if !desired.eof || desired.pending.is_some() || desired.omitted.len() != 0 {
        return Err(KnowledgeDocumentLifecycleError::InvalidInput("desired_eof"));
    }
    prepared.set_transaction_property(
        crate::knowledge_sync_history::SYNC_PROPERTY.into(),
        desired.sync_changes.into_property()?,
    );

    let recovery_key = prepared.recovery_key().clone();
    // Full Source byte audits and SDK preparation never retain the PROJECT writer.
    validate_source()?;
    let writer = acquire_writer(original, scope, limits.write, &before).await?;
    check_head().await?;

    // Those exact bytes were audited before acquisition. Recheck their physical
    // bindings, membership and controls after waiting, without hashing under lock.
    validate_source()?;
    let next = prepared
        .commit()
        .await
        .map_err(KnowledgeDocumentLifecycleError::Commit)?;
    let generation = index::generation(&uri, &next);
    drop(writer);

    validate_source().map_err(|cause| KnowledgeDocumentLifecycleError::Published {
        generation: generation.clone(),
        recovery_key: Box::new(recovery_key.clone()),
        cause: Box::new(cause),
    })?;

    validate_source().map_err(|cause| KnowledgeDocumentLifecycleError::Published {
        generation: generation.clone(),
        recovery_key: Box::new(recovery_key.clone()),
        cause: Box::new(cause),
    })?;
    Ok(KnowledgeDocumentUpdateResult::Updated {
        generation,
        recovery_key,
    })
}
