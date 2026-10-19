//! Republication of explicit captured compiled documents through Data's Full18 CAS.
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
use graphit_auth::{
    ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission, ProjectWriteOperation,
};
use graphit_core::{artifact_store::StoreOwner, file_io::SnapshotContext};
use graphit_data::{
    knowledge_document_projection::{self as projection, KnowledgeDocumentProjectionError},
    knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
        KnowledgeDocumentUpdateError, KnowledgeDocumentUpdateLimits, KnowledgeDocumentUpdateResult,
        update_existing_native_knowledge_documents,
    },
    knowledge_index::{KnowledgeGeneration, KnowledgeIndexError, KnowledgeIndexReadLimits},
    knowledge_references::{
        KnowledgeAuthoredReference, KnowledgeReferenceScope, KnowledgeReferenceTargetKind,
    },
};
use graphit_records::reference_projection::{
    AuthoredReferenceUpdate, LogicalScope, ReferenceTargetKind, SourceNamespace,
};
use std::{collections::TryReserveError, error::Error, fmt, sync::Arc};

#[derive(Clone, Copy)]
pub struct NativeDocumentRepublicationLimits {
    pub publication: NativeDocumentPublicationLimits,
    pub read: KnowledgeIndexReadLimits,
}
#[derive(Debug)]
pub enum NativeDocumentRepublicationError {
    Mapping(NativeAuthoredMappingError),
    Authored(DocumentAuthoredReferenceError),
    Projection(KnowledgeDocumentProjectionError),
    Data(KnowledgeDocumentUpdateError),
    Allocation(TryReserveError),
}
impl fmt::Display for NativeDocumentRepublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge document republication: {self:?}")
    }
}
impl Error for NativeDocumentRepublicationError {
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
fn data(error: impl Into<KnowledgeIndexError>) -> NativeDocumentRepublicationError {
    NativeDocumentRepublicationError::Data(KnowledgeDocumentUpdateError::Data(error.into()))
}

/// Recompile/marshal explicit captured inputs, then replace existing Active rows.
/// Metadata absence preserves authored references; `references: []` clears them
/// into a complete Some projection. Parsed scalar owners live through the await.
/// Both Knowledge grants must refer to this Original and the same project/version.
pub async fn republish_native_documents(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    expected: &KnowledgeGeneration,
    documents: &[AuthoredDocumentInput<'_, '_>],
    limits: NativeDocumentRepublicationLimits,
) -> Result<KnowledgeDocumentUpdateResult, NativeDocumentRepublicationError> {
    let check = || -> Result<(), NativeDocumentRepublicationError> {
        if read.module() != ProjectReadModule::Knowledge
            || write.operation() != ProjectWriteOperation::Knowledge
        {
            return Err(data(KnowledgeIndexError::WrongAdmission));
        }
        original.check().map_err(data)?;
        if !Arc::ptr_eq(original, read.request().map_err(data)?.snapshot())
            || !Arc::ptr_eq(original, write.request().map_err(data)?.snapshot())
        {
            return Err(data(KnowledgeIndexError::DifferentRequest));
        }
        read.recheck().map_err(data)?;
        write.recheck().map_err(data)?;
        let r = read.project().map_err(data)?;
        let w = write.project().map_err(data)?;
        if r.scope.live.owner() != w.scope.live.owner()
            || r.scope.live.runtime_root() != w.scope.live.runtime_root()
            || r.version.version() != w.version.version()
            || r.version.lance_branch() != w.version.lance_branch()
        {
            return Err(NativeDocumentRepublicationError::Data(
                KnowledgeDocumentUpdateError::ScopeMismatch,
            ));
        }
        Ok(())
    };
    check()?;
    let project = write.project().map_err(data)?;
    let StoreOwner::Project { project_id } = project.scope.live.owner() else {
        return Err(data(KnowledgeIndexError::WrongAdmission));
    };
    let publication = limits.publication;
    let mapping = prepare_native_authored_mapping(
        original,
        documents,
        publication.parse,
        publication.mapping,
        publication.fingerprint,
        publication.references,
    )
    .map_err(NativeDocumentRepublicationError::Mapping)?;
    let mapped = mapping
        .documents(original)
        .map_err(NativeDocumentRepublicationError::Mapping)?;
    let mut views = Vec::new();
    views
        .try_reserve_exact(mapped.len())
        .map_err(NativeDocumentRepublicationError::Allocation)?;
    for document in mapped {
        check()?;
        let targets = document.resolved_wiki_targets();
        let mut view = Vec::new();
        view.try_reserve_exact(targets.len())
            .map_err(NativeDocumentRepublicationError::Allocation)?;
        view.extend(targets.iter().map(String::as_str));
        views.push(view);
    }
    let fields = mapping
        .authored_fields(original, &views)
        .map_err(NativeDocumentRepublicationError::Mapping)?;
    let rows = projection::prepare(original, &fields, &[], publication.projection)
        .map_err(NativeDocumentRepublicationError::Projection)?;
    let mut parsed = Vec::new();
    parsed
        .try_reserve_exact(documents.len())
        .map_err(NativeDocumentRepublicationError::Allocation)?;
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
            .map_err(NativeDocumentRepublicationError::Authored)?,
        );
    }
    let mut authored = Vec::new();
    authored
        .try_reserve_exact(parsed.len())
        .map_err(NativeDocumentRepublicationError::Allocation)?;
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
        .map_err(NativeDocumentRepublicationError::Authored)?;
        authored.push(match decision {
            AuthoredReferenceUpdate::Preserve => None,
            AuthoredReferenceUpdate::Replace(refs) => {
                let mut converted = Vec::new();
                converted
                    .try_reserve_exact(refs.len())
                    .map_err(NativeDocumentRepublicationError::Allocation)?;
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
        .map_err(NativeDocumentRepublicationError::Allocation)?;
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
    update_existing_native_knowledge_documents(
        original,
        read,
        write,
        expected,
        &rows,
        &updates,
        KnowledgeDocumentUpdateLimits {
            read: limits.read,
            write: publication.index,
        },
    )
    .await
    .map_err(NativeDocumentRepublicationError::Data)
}
