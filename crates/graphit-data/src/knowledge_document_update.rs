//! Authenticated replacement of existing Active Knowledge documents in one full image.
//! The expected generation and recovery key are observations, never authority.

use crate::{
    knowledge_document_projection::{MemberRow, PreparedKnowledgeDocumentRows},
    knowledge_index::{
        self as index, KnowledgeGeneration, KnowledgeIndexError, KnowledgeIndexReadLimits,
        KnowledgeIndexWriteLimits,
    },
    knowledge_references::{
        self as refs, KnowledgeAuthoredReference, KnowledgeDocumentState, ReferenceRowInput,
        WitnessHeader,
    },
    lance_branch_store::{DeltaCommitFailure, DeltaRecoveryKey},
    lance_module_store::StoreError,
};
use arrow_array::{
    Array, FixedSizeBinaryArray, ListArray, RecordBatch, StringArray, StructArray, UInt8Array,
};
use futures::TryStreamExt;
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::{
    artifact_store::{ArtifactStore, StoreOwner},
    file_io::SnapshotContext,
};
use std::{collections::BTreeMap, error::Error, fmt, io, sync::Arc};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub enum KnowledgeDocumentAuthoredUpdate<'a> {
    Preserve,
    Replace(&'a [KnowledgeAuthoredReference<'a>]),
}
#[derive(Clone, Copy, Debug)]
pub struct KnowledgeDocumentReferenceUpdate<'a> {
    pub slug: &'a str,
    pub authored: KnowledgeDocumentAuthoredUpdate<'a>,
}
#[derive(Clone, Copy)]
pub struct KnowledgeDocumentUpdateLimits {
    pub read: KnowledgeIndexReadLimits,
    pub write: KnowledgeIndexWriteLimits,
}
#[derive(Debug)]
pub enum KnowledgeDocumentUpdateResult {
    Unchanged {
        generation: KnowledgeGeneration,
    },
    Updated {
        generation: KnowledgeGeneration,
        recovery_key: DeltaRecoveryKey,
    },
}
#[derive(Debug)]
pub enum KnowledgeDocumentUpdateError {
    Data(KnowledgeIndexError),
    InvalidInput(&'static str),
    ScopeMismatch,
    GenerationChanged {
        observed: KnowledgeGeneration,
    },
    NoCorpus,
    UnknownSource,
    InheritedTarget,
    Commit(DeltaCommitFailure),
    Published {
        generation: KnowledgeGeneration,
        recovery_key: Box<DeltaRecoveryKey>,
        cause: Box<Self>,
    },
}
impl fmt::Display for KnowledgeDocumentUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge document update: {self:?}")
    }
}
impl Error for KnowledgeDocumentUpdateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Data(e) => Some(e),
            Self::Commit(e) => Some(e),
            Self::Published { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
impl From<KnowledgeIndexError> for KnowledgeDocumentUpdateError {
    fn from(error: KnowledgeIndexError) -> Self {
        Self::Data(error)
    }
}

fn reserve<T>(items: &mut Vec<T>, count: usize) -> Result<(), KnowledgeIndexError> {
    items
        .try_reserve_exact(count)
        .map_err(|e| KnowledgeIndexError::Request(io::Error::new(io::ErrorKind::OutOfMemory, e)))
}

fn check_authority(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
) -> Result<(), KnowledgeDocumentUpdateError> {
    index::check_read(original, read)?;
    index::check_write(original, write)?;
    let r = read.project().map_err(KnowledgeIndexError::from)?;
    let w = write.project().map_err(KnowledgeIndexError::from)?;
    if r.scope.live.owner() != w.scope.live.owner()
        || r.scope.live.runtime_root() != w.scope.live.runtime_root()
        || r.version.version() != w.version.version()
        || r.version.lance_branch() != w.version.lance_branch()
    {
        return Err(KnowledgeDocumentUpdateError::ScopeMismatch);
    }
    Ok(())
}

/// Replace exactly the requested existing Active rows under both real admissions.
/// Full18 validation reaches EOF before any preparation. Unchanged images create no
/// transaction; changed inherited targets require a separate materialization effect.
/// A confirmed commit followed by failed delivery checks returns `Published` with
/// its actual generation/key. Preparation may leave unpublished SDK fragments.
pub async fn update_existing_native_knowledge_documents(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeDocumentUpdateLimits,
) -> Result<KnowledgeDocumentUpdateResult, KnowledgeDocumentUpdateError> {
    update_documents_inner(
        UpdateObservation {
            original,
            phase: &|_| {},
        },
        read,
        write,
        expected,
        rows,
        references,
        limits,
    )
    .await
}

// Private phase observation makes interruption/confirmed-effect tests deterministic
// on the very same producer. It cannot substitute storage, grants or a commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UpdatePhase {
    BeforeWriter,
    WriterAcquired,
    BatchValidated,
    ImageValidated,
    Prepared,
    BeforeCommit,
    Committed,
    Delivery,
}
struct UpdateObservation<'a> {
    original: &'a Arc<SnapshotContext>,
    phase: &'a (dyn Fn(UpdatePhase) + Sync),
}
async fn update_documents_inner(
    observation: UpdateObservation<'_>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeDocumentUpdateLimits,
) -> Result<KnowledgeDocumentUpdateResult, KnowledgeDocumentUpdateError> {
    let original = observation.original;
    check_authority(original, read, write)?;
    let project = write.project().map_err(KnowledgeIndexError::from)?;
    let scope = project.scope.live;
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission.into());
    };
    let branch = project.version.lance_branch();
    let encoded_branch = (!branch.is_empty()).then_some(branch.as_str());
    let corpus = index::local_corpus(
        scope,
        project.version.version(),
        original,
        limits.write.native,
    )?;
    let uri = corpus.uri.clone();
    let check = || -> Result<(), KnowledgeIndexError> {
        index::check_read(original, read)?;
        index::check_write(original, write)
    };
    index::preflight_document_rows(rows, original, limits.write.content, check)?;
    let mut changes = BTreeMap::new();
    let mut headers = Vec::new();
    for row in rows.rows(original).map_err(KnowledgeIndexError::from)? {
        check()?;
        if let MemberRow::DocumentHeader(header) = row {
            if changes.insert(header.slug, headers.len()).is_some() {
                return Err(KnowledgeIndexError::DuplicateSlug.into());
            }
            reserve(&mut headers, 1)?;
            headers.push(**header);
        }
    }
    if references.len() != headers.len() {
        return Err(KnowledgeDocumentUpdateError::InvalidInput(
            "reference_document_map",
        ));
    }
    let mut reference_map = BTreeMap::new();
    for update in references {
        check()?;
        if !changes.contains_key(update.slug)
            || reference_map.insert(update.slug, update.authored).is_some()
        {
            return Err(KnowledgeDocumentUpdateError::InvalidInput(
                "reference_document_map",
            ));
        }
    }
    (observation.phase)(UpdatePhase::BeforeWriter);
    check()?;
    // Existing cooperative writer; waiting cannot block the SDK holder's executor.
    let artifacts = ArtifactStore::new(scope.clone(), limits.write.artifacts);
    let request = Arc::clone(original);
    let writer = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request))
        .await
        .map_err(KnowledgeIndexError::WriterAcquisition)?
        .map_err(KnowledgeIndexError::from)?;
    (observation.phase)(UpdatePhase::WriterAcquired);
    check_authority(original, read, write)?;
    let Some(guard) = index::capture_index(scope, &corpus, original, limits.read.native, check)?
    else {
        check()?;
        return Err(KnowledgeDocumentUpdateError::NoCorpus);
    };
    let Some(write_guard) =
        index::capture_index(scope, &corpus, original, limits.write.native, check)?
    else {
        guard
            .revalidate(original)
            .map_err(KnowledgeIndexError::from)?;
        return Err(KnowledgeIndexError::InvalidField("corpus_capture_changed").into());
    };
    let before = || -> Result<(), KnowledgeIndexError> {
        check()?;
        guard.revalidate(original)?;
        write_guard.revalidate(original)?;
        Ok(())
    };
    before()?;
    let pin =
        crate::lance_branch_store::BranchSnapshot::capture_observed(&uri, encoded_branch, None)
            .await
            .map_err(KnowledgeIndexError::from)?;
    before()?;
    index::semantic::dimensions(pin.schema())?;
    let generation = index::generation(&uri, &pin);
    if &generation != expected {
        return Err(KnowledgeDocumentUpdateError::GenerationChanged {
            observed: generation,
        });
    }
    let mut scanner = pin.scan();
    scanner.batch_size(limits.read.batch_rows.get() as usize);
    let mut stream = scanner
        .try_into_stream()
        .await
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    before()?;
    let mut counts = index::NamespaceValidation::bound(Some(*project_id));
    let mut batches = Vec::new();
    let mut physical = BTreeMap::new();
    loop {
        before()?;
        let next = stream
            .try_next()
            .await
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
        before()?;
        let Some(batch) = next else {
            break;
        };
        index::validate_full_batch(&batch, limits.read.content, &mut counts, before)?;
        (observation.phase)(UpdatePhase::BatchValidated);
        before()?;
        let slugs: &StringArray = index::array(&batch, 0)?;
        for (row, slug) in slugs.iter().enumerate() {
            before()?;
            let slug = slug.ok_or(KnowledgeIndexError::InvalidField("slug"))?;
            if physical
                .insert(slug.to_owned(), (batches.len(), row))
                .is_some()
            {
                return Err(KnowledgeIndexError::DuplicateSlug.into());
            }
        }
        reserve(&mut batches, 1)?;
        batches.push(batch);
    }
    drop(stream);
    drop(scanner);
    before()?;
    // Canonical16 from H plus one effective Some/None projection per changed row.
    let encoded =
        index::encode_with_references(rows, None, original, limits.write.content, before)?;
    let mut replacements = Vec::new();
    reserve(&mut replacements, headers.len())?;
    let mut sync_changes = crate::knowledge_sync_history::KnowledgeSyncChanges::default();
    for (i, header) in headers.iter().enumerate() {
        before()?;
        let &(batch, row) = physical
            .get(header.slug)
            .ok_or(KnowledgeDocumentUpdateError::UnknownSource)?;
        let old = &batches[batch];
        let states: &UInt8Array = index::array(old, 16)?;
        if states.value(row) != KnowledgeDocumentState::Active as u8 {
            return Err(KnowledgeDocumentUpdateError::UnknownSource);
        }
        let slugs: &StringArray = index::array(old, 0)?;
        let titles: &StringArray = index::array(old, 1)?;
        let paths: &StringArray = index::array(old, 7)?;
        let digests: &FixedSizeBinaryArray = index::array(old, 9)?;
        let wiki: &ListArray = index::array(old, 15)?;
        let wiki_owner = wiki.value(row);
        let wiki = wiki_owner
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
        let projection = refs::update_projection(
            *header,
            ReferenceRowInput {
                projection: index::array::<StructArray>(old, 17)?,
                state: states.value(row),
                row,
                header: WitnessHeader {
                    slug: slugs.value(row),
                    title: titles.value(row),
                    source_path: paths.value(row),
                    digest: digests
                        .value(row)
                        .try_into()
                        .map_err(|_| KnowledgeIndexError::InvalidField("compilation_digest"))?,
                },
                wiki,
                expected_project: Some(*project_id),
            },
            match reference_map[header.slug] {
                KnowledgeDocumentAuthoredUpdate::Preserve => None,
                KnowledgeDocumentAuthoredUpdate::Replace(refs) => Some(refs),
            },
            *project_id,
            limits.write.content,
            before,
        )?;
        let mut columns = encoded.slice(i, 1).columns().to_vec();
        columns[17] = projection;
        let replacement = RecordBatch::try_new(index::physical_schema(), columns)
            .map_err(KnowledgeIndexError::from)?;
        let replacement = index::semantic::replace_document(&replacement, &old.slice(row, 1))?;
        if replacement != old.slice(row, 1) {
            reserve(&mut sync_changes.updated, 1)?;
            sync_changes.updated.push(header.slug.to_owned());
        }
        replacements.push(replacement);
    }
    // Preserve every unaffected raw row, including Removed, without cloning bodies.
    let mut image = Vec::new();
    reserve(&mut image, physical.len().max(1))?;
    for batch in &batches {
        let slugs: &StringArray = index::array(batch, 0)?;
        for row in 0..batch.num_rows() {
            before()?;
            image.push(match changes.get(slugs.value(row)) {
                Some(i) => replacements[*i].clone(),
                None => batch.slice(row, 1),
            });
        }
    }
    if image.is_empty() {
        image.push(RecordBatch::new_empty(Arc::new(pin.schema().clone())));
    }
    let mut write_counts = index::NamespaceValidation::bound(Some(*project_id));
    for batch in &image {
        index::validate_full_batch(batch, limits.write.content, &mut write_counts, before)?;
    }
    (observation.phase)(UpdatePhase::ImageValidated);
    before()?;
    if sync_changes.updated.is_empty() {
        return Ok(KnowledgeDocumentUpdateResult::Unchanged { generation });
    }
    if pin.inherited_target().is_some() {
        return Err(KnowledgeDocumentUpdateError::InheritedTarget);
    }
    before()?;
    let mut prepared = pin
        .prepare_overwrite(image)
        .await
        .map_err(KnowledgeIndexError::from)?;
    prepared.set_transaction_property(
        crate::knowledge_sync_history::SYNC_PROPERTY.into(),
        sync_changes.into_property()?,
    );
    (observation.phase)(UpdatePhase::Prepared);
    before()?;
    let recovery_key = prepared.recovery_key().clone();
    (observation.phase)(UpdatePhase::BeforeCommit);
    before()?;
    let next = prepared
        .commit()
        .await
        .map_err(KnowledgeDocumentUpdateError::Commit)?;
    let generation = index::generation(&uri, &next);
    (observation.phase)(UpdatePhase::Committed);
    before().map_err(|cause| KnowledgeDocumentUpdateError::Published {
        generation: generation.clone(),
        recovery_key: Box::new(recovery_key.clone()),
        cause: Box::new(cause.into()),
    })?;
    (observation.phase)(UpdatePhase::Delivery);
    before().map_err(|cause| KnowledgeDocumentUpdateError::Published {
        generation: generation.clone(),
        recovery_key: Box::new(recovery_key.clone()),
        cause: Box::new(cause.into()),
    })?;
    drop(writer);
    Ok(KnowledgeDocumentUpdateResult::Updated {
        generation,
        recovery_key,
    })
}
