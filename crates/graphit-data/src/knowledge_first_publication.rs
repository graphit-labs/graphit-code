//! Exclusive first publication with explicit sparse authored-reference decisions.

use crate::{
    knowledge_document_projection::{MemberRow, PreparedKnowledgeDocumentRows},
    knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
    },
    knowledge_index::{
        self as index, FirstPublicationPhase, KnowledgeFirstPublication,
        KnowledgeIndexContentLimits, KnowledgeIndexError, KnowledgeIndexWriteLimits,
    },
    knowledge_references as refs,
};
use arrow_array::RecordBatch;
use graphit_auth::ProjectWriteAdmission;
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, project_identity::ProjectId,
};
use std::{collections::BTreeMap, sync::Arc};

/// Publish a new corpus in one canonical18 image. Decisions cover each header
/// exactly once. Preserve is incomplete None; Replace is Some, including [].
/// Only a real Knowledge write admission selects and authorizes the destination.
pub async fn publish_native_authored_with_reference_updates(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError> {
    publish_inner(original, write, rows, references, limits, &|_| {}).await
}

async fn publish_inner(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeIndexWriteLimits,
    phase: &(dyn Fn(FirstPublicationPhase) + Sync),
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError> {
    let before = || index::check_write(original, write);
    before()?;
    let batch = encode_native_authored_reference_updates(
        original, write, rows, references, limits, phase, before,
    )?;
    index::publish_native_authored_batch(original, write, batch, limits, phase).await
}

/// Canonical native Knowledge schema. This ordinary value grants no access.
pub fn native_knowledge_source_schema() -> Arc<arrow_schema::Schema> {
    index::physical_schema()
}

/// Encode one FILE's complete rows and sparse reference decision into owned data.
/// A subsequent stream publisher still validates the complete selection to EOF.
pub fn encode_native_knowledge_source_batch(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeIndexWriteLimits,
) -> Result<RecordBatch, KnowledgeIndexError> {
    encode_native_authored_reference_updates(
        original,
        write,
        rows,
        references,
        limits,
        &|_| {},
        || index::check_write(original, write),
    )
}

/// Encode ordinary FULL18 data with the existing C8 kernel.
/// ProjectId, rows and the returned batch grant no access or publication.
/// The stream publisher must admit the real PROJECT and validate Source/EOF.
pub fn encode_knowledge_source_batch(
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeIndexContentLimits,
) -> Result<RecordBatch, KnowledgeIndexError> {
    encode_source_rows(
        original,
        project_id,
        rows,
        references,
        limits,
        &|_| {},
        || original.check().map_err(KnowledgeIndexError::from),
    )
}

// Authorized producers retain their existing admission and phase callbacks.
pub(crate) fn encode_native_authored_reference_updates(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeIndexWriteLimits,
    phase: &(dyn Fn(FirstPublicationPhase) + Sync),
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<RecordBatch, KnowledgeIndexError> {
    index::check_write(original, write)?;
    before()?;
    let project = write.project()?;
    let StoreOwner::Project { project_id } = project.scope.live.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission);
    };
    encode_source_rows(
        original,
        *project_id,
        rows,
        references,
        limits.content,
        phase,
        before,
    )
}

fn encode_source_rows(
    original: &Arc<SnapshotContext>,
    project_id: ProjectId,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeIndexContentLimits,
    phase: &(dyn Fn(FirstPublicationPhase) + Sync),
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<RecordBatch, KnowledgeIndexError> {
    before()?;
    index::preflight_document_rows(rows, original, limits, &before)?;
    phase(FirstPublicationPhase::RowsValidated);
    before()?;
    let rows_view = rows.rows(original)?;
    let headers = || {
        rows_view.iter().filter_map(|r| match r {
            MemberRow::DocumentHeader(h) => Some(**h),
            _ => None,
        })
    };
    if references.len() != headers().count() {
        return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
    }
    let mut decisions = BTreeMap::new();
    for update in references {
        before()?;
        if decisions.insert(update.slug, update.authored).is_some() {
            return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
        }
    }
    for header in headers() {
        before()?;
        if !decisions.contains_key(header.slug) {
            return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
        }
    }
    let projections = refs::encode_initial_projections(
        headers().map(|h| {
            (
                h,
                match decisions[h.slug] {
                    KnowledgeDocumentAuthoredUpdate::Preserve => None,
                    KnowledgeDocumentAuthoredUpdate::Replace(refs) => Some(refs),
                },
            )
        }),
        project_id,
        limits,
        &before,
    )?;
    phase(FirstPublicationPhase::ReferencesEncoded);
    before()?;
    let encoded = index::encode_with_references(rows, None, original, limits, &before)?;
    let mut columns = encoded.columns().to_vec();
    columns[17] = projections;
    let batch = RecordBatch::try_new(index::physical_schema(), columns)?;
    index::validate_full_batch(
        &batch,
        limits,
        &mut index::NamespaceValidation::bound(Some(project_id)),
        &before,
    )?;
    before()?;
    phase(FirstPublicationPhase::ImageValidated);
    before()?;
    Ok(batch)
}

#[cfg(test)]
mod tests;
