//! First publication of captured compiled documents and explicit authored metadata.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use crate::{
    document::parse_document,
    document_authored_references::{
        DocumentAuthoredReferenceError, project_document_authored_references,
    },
    native_document_mapping::{
        AuthoredDocumentInput, NativeAuthoredMappingError, prepare_native_authored_mapping,
    },
    native_index_publication::NativeDocumentPublicationLimits,
};
use graphit_auth::{ProjectWriteAdmission, ProjectWriteOperation};
use graphit_core::{artifact_store::StoreOwner, file_io::SnapshotContext};
use graphit_data::{
    knowledge_document_projection::{self as projection, KnowledgeDocumentProjectionError},
    knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
    },
    knowledge_first_publication::publish_native_authored_with_reference_updates,
    knowledge_index::{KnowledgeFirstPublication, KnowledgeIndexError},
    knowledge_references::{
        KnowledgeAuthoredReference, KnowledgeReferenceScope, KnowledgeReferenceTargetKind,
    },
};
use graphit_records::reference_projection::{
    AuthoredReferenceUpdate, LogicalScope, ReferenceTargetKind, SourceNamespace,
};
use std::{collections::TryReserveError, error::Error, fmt, sync::Arc};

#[derive(Debug)]
pub enum NativeAuthoredFirstPublicationError {
    Mapping(NativeAuthoredMappingError),
    Authored(DocumentAuthoredReferenceError),
    Projection(KnowledgeDocumentProjectionError),
    Data(KnowledgeIndexError),
    Allocation(TryReserveError),
}
impl fmt::Display for NativeAuthoredFirstPublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge authored first publication: {self:?}")
    }
}
impl Error for NativeAuthoredFirstPublicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Mapping(e) => e,
            Self::Authored(e) => e,
            Self::Projection(e) => e,
            Self::Data(e) => e,
            Self::Allocation(e) => e,
        })
    }
}
fn data(error: impl Into<KnowledgeIndexError>) -> NativeAuthoredFirstPublicationError {
    NativeAuthoredFirstPublicationError::Data(error.into())
}

/// Publish compiled captured inputs and their real authored metadata together.
/// Omission yields incomplete None; an explicit [] yields known-empty Some.
/// Parsed YAML owners and borrowed tuple views remain live through publication.
pub async fn publish_native_documents_with_authored_references(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    documents: &[AuthoredDocumentInput<'_, '_>],
    limits: NativeDocumentPublicationLimits,
) -> Result<KnowledgeFirstPublication, NativeAuthoredFirstPublicationError> {
    let check = || -> Result<(), NativeAuthoredFirstPublicationError> {
        if write.operation() != ProjectWriteOperation::Knowledge {
            return Err(data(KnowledgeIndexError::WrongAdmission));
        }
        original.check().map_err(data)?;
        if !Arc::ptr_eq(original, write.request().map_err(data)?.snapshot()) {
            return Err(data(KnowledgeIndexError::DifferentRequest));
        }
        write.recheck().map_err(data)?;
        Ok(())
    };
    check()?;
    let project = write.project().map_err(data)?;
    let StoreOwner::Project { project_id } = project.scope.live.owner() else {
        return Err(data(KnowledgeIndexError::WrongAdmission));
    };
    let publication = limits;
    let mapping = prepare_native_authored_mapping(
        original,
        documents,
        publication.parse,
        publication.mapping,
        publication.fingerprint,
        publication.references,
    )
    .map_err(NativeAuthoredFirstPublicationError::Mapping)?;
    let mapped = mapping
        .documents(original)
        .map_err(NativeAuthoredFirstPublicationError::Mapping)?;
    let mut views = Vec::new();
    views
        .try_reserve_exact(mapped.len())
        .map_err(NativeAuthoredFirstPublicationError::Allocation)?;
    for document in mapped {
        check()?;
        let targets = document.resolved_wiki_targets();
        let mut view = Vec::new();
        view.try_reserve_exact(targets.len())
            .map_err(NativeAuthoredFirstPublicationError::Allocation)?;
        view.extend(targets.iter().map(String::as_str));
        views.push(view);
    }
    let fields = mapping
        .authored_fields(original, &views)
        .map_err(NativeAuthoredFirstPublicationError::Mapping)?;
    let rows = projection::prepare(original, &fields, &[], publication.projection)
        .map_err(NativeAuthoredFirstPublicationError::Projection)?;
    let mut parsed = Vec::new();
    parsed
        .try_reserve_exact(documents.len())
        .map_err(NativeAuthoredFirstPublicationError::Allocation)?;
    for input in documents {
        check()?;
        input.compiled.check_request(original).map_err(data)?;
        parsed.push(
            parse_document(
                original,
                input.compiled.whole_source().as_bytes(),
                &input.compiled.recipe_policy().fallback_title,
                publication.parse,
            )
            .map_err(DocumentAuthoredReferenceError::Syntax)
            .map_err(NativeAuthoredFirstPublicationError::Authored)?,
        );
    }
    let mut authored = Vec::new();
    authored
        .try_reserve_exact(parsed.len())
        .map_err(NativeAuthoredFirstPublicationError::Allocation)?;
    for parsed in &parsed {
        check()?;
        let decision = project_document_authored_references(
            original,
            parsed,
            SourceNamespace {
                scope: LogicalScope::Project(*project_id),
                context: None,
            },
            publication.parse,
            publication.authored_references,
        )
        .map_err(NativeAuthoredFirstPublicationError::Authored)?;
        authored.push(match decision {
            AuthoredReferenceUpdate::Preserve => None,
            AuthoredReferenceUpdate::Replace(refs) => {
                let mut converted = Vec::new();
                converted
                    .try_reserve_exact(refs.len())
                    .map_err(NativeAuthoredFirstPublicationError::Allocation)?;
                for r in refs {
                    check()?;
                    converted.push(KnowledgeAuthoredReference {
                        target_kind: match r.target_kind {
                            ReferenceTargetKind::Task => KnowledgeReferenceTargetKind::Task,
                            ReferenceTargetKind::Session => KnowledgeReferenceTargetKind::Session,
                            ReferenceTargetKind::Memory => KnowledgeReferenceTargetKind::Memory,
                            ReferenceTargetKind::Knowledge => {
                                KnowledgeReferenceTargetKind::Knowledge
                            }
                        },
                        scope: match r.scope {
                            LogicalScope::Project(id) => KnowledgeReferenceScope::Project(id),
                            LogicalScope::User(id) => KnowledgeReferenceScope::User(id),
                        },
                        id: r.id,
                        context: r.context,
                        relation: r.relation,
                        field: r.field,
                    });
                }
                Some(converted)
            }
        });
    }
    let mut updates = Vec::new();
    updates
        .try_reserve_exact(documents.len())
        .map_err(NativeAuthoredFirstPublicationError::Allocation)?;
    for (input, authored) in documents.iter().zip(&authored) {
        check()?;
        updates.push(KnowledgeDocumentReferenceUpdate {
            slug: input.slug,
            authored: match authored {
                None => KnowledgeDocumentAuthoredUpdate::Preserve,
                Some(refs) => KnowledgeDocumentAuthoredUpdate::Replace(refs),
            },
        });
    }
    check()?;
    publish_native_authored_with_reference_updates(
        original,
        write,
        &rows,
        &updates,
        publication.index,
    )
    .await
    .map_err(NativeAuthoredFirstPublicationError::Data)
}
