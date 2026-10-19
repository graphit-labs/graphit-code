//! First publication of explicit compiled documents through the native Data producer.
//! Availability follows Data's publisher on the supported native platforms.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use crate::{
    compilation_fingerprint::CompilationFingerprintLimits,
    document::DocumentParseLimits,
    native_document_mapping::{
        AuthoredDocumentInput, NativeAuthoredMappingError, NativeAuthoredMappingLimits,
        prepare_native_authored_mapping,
    },
    references::ReferenceLimits,
};
use graphit_auth::{ProjectWriteAdmission, ProjectWriteOperation};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    knowledge_document_projection::{
        self as projection, KnowledgeDocumentProjectionError, KnowledgeDocumentProjectionLimits,
    },
    knowledge_index::{
        KnowledgeFirstPublication, KnowledgeIndexError, KnowledgeIndexWriteLimits,
        publish_native_authored,
    },
};
use std::{collections::TryReserveError, error::Error, fmt, sync::Arc};

/// Existing producer bounds, supplied explicitly by the caller.
#[derive(Clone, Copy)]
pub struct NativeDocumentPublicationLimits {
    pub parse: DocumentParseLimits,
    pub mapping: NativeAuthoredMappingLimits,
    pub fingerprint: CompilationFingerprintLimits,
    pub references: ReferenceLimits,
    pub authored_references: crate::document_authored_references::DocumentAuthoredReferenceLimits,
    pub projection: KnowledgeDocumentProjectionLimits,
    pub index: KnowledgeIndexWriteLimits,
}

/// Keeps the original producer error, including a confirmed partial default generation.
#[derive(Debug)]
pub enum NativeDocumentPublicationError {
    Mapping(NativeAuthoredMappingError),
    Projection(KnowledgeDocumentProjectionError),
    Data(KnowledgeIndexError),
    Allocation(TryReserveError),
}
impl fmt::Display for NativeDocumentPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mapping(error) => write!(formatter, "document publication mapping: {error}"),
            Self::Projection(error) => {
                write!(formatter, "document publication projection: {error}")
            }
            Self::Data(error) => write!(formatter, "document publication storage: {error}"),
            Self::Allocation(error) => {
                write!(formatter, "document publication allocation: {error}")
            }
        }
    }
}
impl Error for NativeDocumentPublicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Mapping(error) => error,
            Self::Projection(error) => error,
            Self::Data(error) => error,
            Self::Allocation(error) => error,
        })
    }
}

/// Publish a finite, explicit set of already compiled documents for the first time.
///
/// The admission must authorize Knowledge on this exact original request. Checks precede
/// preparation; Data rechecks at its own publication boundaries. All preparation succeeds
/// before the first storage call. This does not update, adopt or retry an existing corpus.
/// An empty set creates a published empty generation. Storage failures retain their cause;
/// `Data(PartialDefault { .. })` retains the default generation already confirmed by Data.
pub async fn publish_native_documents(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    documents: &[AuthoredDocumentInput<'_, '_>],
    limits: NativeDocumentPublicationLimits,
) -> Result<KnowledgeFirstPublication, NativeDocumentPublicationError> {
    let check = || -> Result<(), KnowledgeIndexError> {
        if admission.operation() != ProjectWriteOperation::Knowledge {
            return Err(KnowledgeIndexError::WrongAdmission);
        }
        original.check()?;
        if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
            return Err(KnowledgeIndexError::DifferentRequest);
        }
        admission.recheck()?;
        Ok(())
    };
    check().map_err(NativeDocumentPublicationError::Data)?;
    let mapping = prepare_native_authored_mapping(
        original,
        documents,
        limits.parse,
        limits.mapping,
        limits.fingerprint,
        limits.references,
    )
    .map_err(NativeDocumentPublicationError::Mapping)?;
    let mapped = mapping
        .documents(original)
        .map_err(NativeDocumentPublicationError::Mapping)?;
    let mut views = Vec::new();
    views
        .try_reserve_exact(mapped.len())
        .map_err(NativeDocumentPublicationError::Allocation)?;
    for document in mapped {
        original
            .check()
            .map_err(KnowledgeIndexError::Request)
            .map_err(NativeDocumentPublicationError::Data)?;
        let targets = document.resolved_wiki_targets();
        let mut view = Vec::new();
        view.try_reserve_exact(targets.len())
            .map_err(NativeDocumentPublicationError::Allocation)?;
        view.extend(targets.iter().map(String::as_str));
        views.push(view);
    }
    let fields = mapping
        .authored_fields(original, &views)
        .map_err(NativeDocumentPublicationError::Mapping)?;
    let rows = projection::prepare(original, &fields, &[], limits.projection)
        .map_err(NativeDocumentPublicationError::Projection)?;
    publish_native_authored(original, admission, &rows, limits.index)
        .await
        .map_err(NativeDocumentPublicationError::Data)
}
