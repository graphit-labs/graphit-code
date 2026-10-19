//! Publish authorized source changes through the existing per-file producers.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use crate::{
    compilation_fingerprint::CompilationFingerprintLimits,
    desired_corpus::{
        DesiredCorpusError, KnowledgeDesiredPreparationLimits, KnowledgeSourceCaptureLimits,
    },
    document_authored_references::DocumentAuthoredReferenceError,
    native_document_mapping::NativeAuthoredMappingError,
    references::ReferenceLimits,
};
use graphit_auth::{
    AuthError, ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission,
    ProjectWriteOperation,
};
use graphit_core::{
    artifact_store::ArtifactStore, config::ConfigSnapshot, file_io::SnapshotContext,
};
use graphit_data::{
    knowledge_document_lifecycle::{
        KnowledgeDocumentLifecycleError, KnowledgeDocumentLifecycleLimits,
        KnowledgeSourceFirstPublicationError,
    },
    knowledge_document_projection::{
        KnowledgeDocumentProjectionError, KnowledgeDocumentProjectionLimits,
    },
    knowledge_document_update::KnowledgeDocumentUpdateResult,
    knowledge_index::KnowledgeFirstPublication,
    knowledge_references::{
        KnowledgeAuthoredReference, KnowledgeReferenceScope, KnowledgeReferenceTargetKind,
    },
    lance_module_store::RecoveryKey,
};
use graphit_records::reference_projection::{
    AuthoredReferenceUpdate, LogicalScope, ReferenceTargetKind,
};
use std::{collections::TryReserveError, error::Error, fmt, io, sync::Arc};

#[derive(Clone, Copy)]
pub struct KnowledgeDesiredReconciliationLimits {
    pub capture: KnowledgeSourceCaptureLimits,
    pub preparation: KnowledgeDesiredPreparationLimits,
    pub fingerprint: CompilationFingerprintLimits,
    pub references: ReferenceLimits,
    pub authored_references: crate::document_authored_references::DocumentAuthoredReferenceLimits,
    pub projection: KnowledgeDocumentProjectionLimits,
    pub index: KnowledgeDocumentLifecycleLimits,
}

#[derive(Debug)]
pub enum KnowledgeDesiredReconciliationResult {
    First {
        publication: KnowledgeFirstPublication,
        recovery_key: RecoveryKey,
    },
    Existing(KnowledgeDocumentUpdateResult),
}

#[derive(Debug)]
pub enum KnowledgeDesiredReconciliationError {
    Auth(AuthError),
    Request(io::Error),
    WrongAdmission,
    DifferentRequest,
    ScopeMismatch,
    BrandMismatch,
    FileWorker(tokio::task::JoinError),
    Source(Box<DesiredCorpusError>),
    Mapping(NativeAuthoredMappingError),
    Authored(DocumentAuthoredReferenceError),
    Projection(KnowledgeDocumentProjectionError),
    DataFirst(KnowledgeSourceFirstPublicationError),
    DataImage(graphit_data::knowledge_index::KnowledgeIndexError),
    Embeddings(Box<graphit_data::knowledge_embeddings::KnowledgeEmbeddingError>),
    DataLifecycle(KnowledgeDocumentLifecycleError),
    Allocation(TryReserveError),
    Overflow(&'static str),
    Published {
        publication: Box<KnowledgeDesiredReconciliationResult>,
        pending_paths: std::collections::BTreeSet<std::path::PathBuf>,
        cause: Box<Self>,
    },
}
impl fmt::Display for KnowledgeDesiredReconciliationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge desired reconciliation: {self:?}")
    }
}
impl Error for KnowledgeDesiredReconciliationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(e) => Some(e),
            Self::Request(e) => Some(e),
            Self::FileWorker(e) => Some(e),
            Self::Source(e) => Some(e.as_ref()),
            Self::Mapping(e) => Some(e),
            Self::Authored(e) => Some(e),
            Self::Projection(e) => Some(e),
            Self::DataFirst(e) => Some(e),
            Self::DataImage(e) => Some(e),
            Self::Embeddings(e) => Some(e.as_ref()),
            Self::DataLifecycle(e) => Some(e),
            Self::Allocation(e) => Some(e),
            Self::Published { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
impl From<DesiredCorpusError> for KnowledgeDesiredReconciliationError {
    fn from(error: DesiredCorpusError) -> Self {
        Self::Source(Box::new(error))
    }
}
type Result<T> = std::result::Result<T, KnowledgeDesiredReconciliationError>;

pub(crate) fn check(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
) -> Result<()> {
    check_inner(original, read, write, &|| {})
}

pub(crate) fn auth_error(
    original: &SnapshotContext,
    cause: AuthError,
) -> KnowledgeDesiredReconciliationError {
    match crate::desired_corpus::request_after_auth(original, &cause) {
        Some(request) => KnowledgeDesiredReconciliationError::Request(request),
        None => KnowledgeDesiredReconciliationError::Auth(cause),
    }
}

fn check_inner(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    after_original: &impl Fn(),
) -> Result<()> {
    use KnowledgeDesiredReconciliationError as E;
    if read.module() != ProjectReadModule::Knowledge
        || write.operation() != ProjectWriteOperation::Knowledge
    {
        return Err(E::WrongAdmission);
    }
    original.check().map_err(E::Request)?;
    after_original();
    if !Arc::ptr_eq(
        original,
        read.request()
            .map_err(|cause| auth_error(original, cause))?
            .snapshot(),
    ) || !Arc::ptr_eq(
        original,
        write
            .request()
            .map_err(|cause| auth_error(original, cause))?
            .snapshot(),
    ) {
        return Err(E::DifferentRequest);
    }
    read.recheck()
        .map_err(|cause| auth_error(original, cause))?;
    write
        .recheck()
        .map_err(|cause| auth_error(original, cause))?;
    let r = read
        .project()
        .map_err(|cause| auth_error(original, cause))?;
    let w = write
        .project()
        .map_err(|cause| auth_error(original, cause))?;
    if r.scope.live.owner() != w.scope.live.owner()
        || r.scope.live.runtime_root() != w.scope.live.runtime_root()
        || r.version.version() != w.version.version()
        || r.version.lance_branch() != w.version.lance_branch()
    {
        return Err(E::ScopeMismatch);
    }
    Ok(())
}

/// Publish each selected FILE independently. `changes` contains project-relative
/// paths; an empty set resumes embedding work without reparsing saved documents.
/// Notifications contain only committed slugs, never source owners or bodies.
#[allow(clippy::too_many_arguments)]
pub async fn reconcile_knowledge_source_changes(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    config: &ConfigSnapshot,
    kind_on_absent: &str,
    changes: Option<&std::collections::BTreeSet<std::path::PathBuf>>,
    on_file: &(dyn Fn(&str) + Sync),
    before_file: &(dyn Fn() -> io::Result<()> + Sync),
    limits: KnowledgeDesiredReconciliationLimits,
) -> Result<KnowledgeDesiredReconciliationResult> {
    check(original, read, write)?;
    before_file().map_err(KnowledgeDesiredReconciliationError::Request)?;
    let project = read
        .project()
        .map_err(|cause| auth_error(original, cause))?;
    if config.brand() != project.scope.live.brand() {
        return Err(KnowledgeDesiredReconciliationError::BrandMismatch);
    }
    if changes.is_some_and(std::collections::BTreeSet::is_empty) {
        use graphit_data::knowledge_document_lifecycle::{
            KnowledgeSourceOrigins, read_native_knowledge_source_origins,
        };
        return match read_native_knowledge_source_origins(original, read, limits.index.read)
            .await
            .map_err(KnowledgeDesiredReconciliationError::DataLifecycle)?
        {
            KnowledgeSourceOrigins::NoCorpus => {
                Err(KnowledgeDesiredReconciliationError::DataLifecycle(
                    KnowledgeDocumentLifecycleError::NoCorpus,
                ))
            }
            KnowledgeSourceOrigins::Published {
                generation, heads, ..
            } => {
                for head in heads {
                    check(original, read, write)?;
                    before_file().map_err(KnowledgeDesiredReconciliationError::Request)?;
                    on_file(&head.slug);
                }
                Ok(KnowledgeDesiredReconciliationResult::Existing(
                    KnowledgeDocumentUpdateResult::Unchanged { generation },
                ))
            }
        };
    }
    let artifacts = ArtifactStore::new(project.scope.live.clone(), limits.index.write.artifacts);
    let request = Arc::clone(original);
    tokio::task::spawn_blocking(move || artifacts.prepare_project_runtime_with_context(&request))
        .await
        .map_err(|cause| KnowledgeDesiredReconciliationError::Request(io::Error::other(cause)))?
        .map_err(|cause| {
            KnowledgeDesiredReconciliationError::Request(io::Error::new(cause.kind(), cause))
        })?;
    check(original, read, write)?;
    before_file().map_err(KnowledgeDesiredReconciliationError::Request)?;
    let selection = crate::desired_corpus::capture_selection_worker(
        original,
        read,
        config,
        changes,
        limits.capture,
    )
    .await?;
    check(original, read, write)?;
    before_file().map_err(KnowledgeDesiredReconciliationError::Request)?;
    crate::source_stream::publish_source_changes(
        original,
        read,
        write,
        &selection,
        kind_on_absent,
        changes,
        on_file,
        before_file,
        limits,
    )
    .await
}

pub(crate) fn convert_authored_references<'a>(
    decision: AuthoredReferenceUpdate<'a>,
    before: &impl Fn() -> Result<()>,
) -> Result<Option<Vec<KnowledgeAuthoredReference<'a>>>> {
    use KnowledgeDesiredReconciliationError as E;
    Ok(match decision {
        AuthoredReferenceUpdate::Preserve => None,
        AuthoredReferenceUpdate::Replace(refs) => {
            let mut converted = Vec::new();
            converted
                .try_reserve_exact(refs.len())
                .map_err(E::Allocation)?;
            for r in refs {
                before()?;
                converted.push(KnowledgeAuthoredReference {
                    target_kind: match r.target_kind {
                        ReferenceTargetKind::Task => KnowledgeReferenceTargetKind::Task,
                        ReferenceTargetKind::Session => KnowledgeReferenceTargetKind::Session,
                        ReferenceTargetKind::Memory => KnowledgeReferenceTargetKind::Memory,
                        ReferenceTargetKind::Knowledge => KnowledgeReferenceTargetKind::Knowledge,
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
    })
}

#[cfg(test)]
mod tests;
