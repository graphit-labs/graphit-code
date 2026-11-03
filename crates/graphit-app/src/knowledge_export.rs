//! Authenticated text export of a complete Knowledge Source to a fresh directory.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use graphit_auth::{
    AuthError, AuthenticatedCaller, ExternalEffect, Obligation, Permission, ProjectReadAdmission,
    ProjectReadModule, ProjectRecordAuthority, RoleIssuer, ScopeCapture, ScopeInput,
};
use graphit_core::{file_io::SnapshotContext, native_document_file::NativeDocumentFileLimits};
use graphit_data::{
    knowledge_index::{KnowledgeDocument, KnowledgeGeneration, KnowledgeIndexReadLimits},
    knowledge_search::{
        KnowledgeSearchError, KnowledgeSearchRead, capture_native_knowledge_search,
    },
};
use graphit_hub::policy::RequestContext;
use graphit_knowledge::{
    document_communities::DocumentCommunities,
    imported_knowledge::{
        ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadLimits,
        ImportedKnowledgeSourceError, open_imported_knowledge,
    },
    markdown_export::{
        MarkdownExportError, MarkdownExportLimits, MarkdownExportModes, MarkdownExportPlan,
        MarkdownExportStage,
    },
    okf_export::KnowledgeTextExportFormat,
};
use std::{
    error::Error,
    fmt,
    fs::File,
    io,
    path::{Component, Path, PathBuf},
};

/// The destination is data; the authenticated caller supplies filesystem authority.
/// Its parent must exist and the destination must be absent (FreshDestination).
pub struct KnowledgeTextExportRequest<'a> {
    pub destination: &'a Path,
    pub format: KnowledgeTextExportFormat,
}

/// Caller-selected producer policies; this SDK supplies no product defaults.
#[derive(Clone)]
pub struct KnowledgeTextExportPolicy {
    pub index: KnowledgeIndexReadLimits,
    pub text: MarkdownExportLimits,
    pub modes: MarkdownExportModes,
}

#[derive(Debug)]
pub enum KnowledgeTextExportOutcome {
    RegistryAbsent,
    NoCorpus,
    PublishedEmpty(KnowledgeGeneration),
    Published(Box<KnowledgeTextPublished>),
}

/// A successfully created output and its actual captured Source observation.
#[derive(Debug)]
pub struct KnowledgeTextPublished {
    pub output: PathBuf,
    pub generation: KnowledgeGeneration,
    pub communities: DocumentCommunities,
    pub fingerprint: [u8; 32],
    /// Publication is known successful even when private spool cleanup fails.
    pub cleanup: Result<(), KnowledgeTextExportError>,
}

#[derive(Debug)]
pub enum KnowledgeTextExportError {
    Request(io::Error),
    Authorization(AuthError),
    Index(KnowledgeSearchError),
    Imported(ImportedKnowledgeSourceError),
    Preparation(MarkdownExportError),
    Input(io::Error),
    Cleanup(io::Error),
    /// The exclusive rename returned an error; no generic rollback/no-effect claim is made.
    Publication {
        cause: io::Error,
        output: PathBuf,
        cleanup: io::Result<()>,
    },
    /// Rename succeeded before the final authority/context check failed.
    AfterPublication {
        output: PathBuf,
        generation: Box<KnowledgeGeneration>,
        fingerprint: [u8; 32],
        cause: Box<KnowledgeTextExportError>,
        cleanup: io::Result<()>,
    },
}
impl fmt::Display for KnowledgeTextExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(_) => f.write_str("Knowledge export request failed"),
            Self::Authorization(_) => f.write_str("Knowledge export access denied"),
            Self::Index(_) => f.write_str("Knowledge export corpus read failed"),
            Self::Imported(_) => f.write_str("Knowledge export imported Source read failed"),
            Self::Preparation(_) => f.write_str("Knowledge export text preparation failed"),
            Self::Input(_) => f.write_str("Knowledge export input failed"),
            Self::Cleanup(_) => f.write_str("Knowledge export private cleanup failed"),
            Self::Publication { .. } => {
                f.write_str("Knowledge export exclusive directory publication failed")
            }
            Self::AfterPublication { .. } => {
                f.write_str("Knowledge export published before the final request check failed")
            }
        }
    }
}
impl Error for KnowledgeTextExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) | Self::Input(e) | Self::Cleanup(e) => Some(e),
            Self::Authorization(e) => Some(e),
            Self::Index(e) => Some(e),
            Self::Imported(e) => Some(e),
            Self::Preparation(e) => Some(e),
            Self::Publication { cause, .. } => Some(cause),
            Self::AfterPublication { cause, .. } => Some(cause.as_ref()),
        }
    }
}

fn check_access(
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
    caller: &AuthenticatedCaller<'_>,
) -> Result<(), KnowledgeTextExportError> {
    original
        .snapshot()
        .check()
        .map_err(KnowledgeTextExportError::Request)?;
    admission
        .recheck()
        .map_err(KnowledgeTextExportError::Authorization)?;
    if caller
        .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
        .map_err(KnowledgeTextExportError::Authorization)?
        != Obligation::Unconditional
    {
        return Err(KnowledgeTextExportError::Authorization(AuthError::Denied));
    }
    Ok(())
}

fn destination_parent(destination: &Path) -> Result<&Path, KnowledgeTextExportError> {
    if !destination.is_absolute()
        || destination.file_name().is_none()
        || destination
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(KnowledgeTextExportError::Input(
            io::ErrorKind::InvalidInput.into(),
        ));
    }
    destination
        .parent()
        .ok_or_else(|| KnowledgeTextExportError::Input(io::ErrorKind::InvalidInput.into()))
}

/// Export one complete Source using real PROJECT ReadKnowledge and unconditional
/// PublishedFilesystem authority. The absolute output may be outside the project.
/// Existing outputs are preserved by an exclusive atomic rename, never replaced.
pub async fn export_text_authenticated<'a>(
    issuer: &'a RoleIssuer,
    caller: &'a AuthenticatedCaller<'a>,
    scope: ScopeInput<'a>,
    original: &'a RequestContext<'a>,
    request: KnowledgeTextExportRequest<'_>,
    policy: &KnowledgeTextExportPolicy,
) -> Result<KnowledgeTextExportOutcome, KnowledgeTextExportError> {
    original
        .snapshot()
        .check()
        .map_err(KnowledgeTextExportError::Request)?;
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_read(caller, original, scope, ProjectReadModule::Knowledge)
        .map_err(KnowledgeTextExportError::Authorization)?;
    check_access(original, &admission, caller)?;
    let read = capture_native_knowledge_search(original.snapshot(), &admission, policy.index)
        .await
        .map_err(KnowledgeTextExportError::Index)?;
    check_access(original, &admission, caller)?;
    let capture = match read {
        KnowledgeSearchRead::NoCorpus => return Ok(KnowledgeTextExportOutcome::NoCorpus),
        KnowledgeSearchRead::Published(capture) => capture,
    };
    let generation = capture.generation().clone();
    let mut stage = None;
    // Retain the actual validated parent descriptor through rename and cleanup.
    let mut _output_parent = None;
    while let Some(documents) = capture
        .next_documents(original.snapshot(), &admission)
        .await
        .map_err(KnowledgeTextExportError::Index)?
    {
        check_access(original, &admission, caller)?;
        for document in documents {
            check_access(original, &admission, caller)?;
            if stage.is_none() {
                capture
                    .validate_export_destination(
                        original.snapshot(),
                        &admission,
                        request.destination,
                    )
                    .map_err(KnowledgeTextExportError::Index)?;
            }
            stage_document(
                &mut stage,
                &mut _output_parent,
                original.snapshot(),
                &request,
                policy,
                document,
            )?;
        }
    }
    // Some(empty) is not EOF; only None completes this retained Source stream.
    check_access(original, &admission, caller)?;
    finish_text(stage, original, request, generation, || {
        check_access(original, &admission, caller)
    })
}

/// Exact installed Knowledge selection and its physical read policy; no PROJECT selection.
pub struct ImportedKnowledgeTextSource<'a> {
    pub installation: ScopeCapture<'a>,
    pub input: ImportedKnowledgeInput<'a>,
    pub directory: NativeDocumentFileLimits,
}

/// Admit the same authenticated caller in the installation and stream the exact
/// imported Source pin. No PROJECT authority, NN open or physical dataset clone.
pub async fn export_imported_text_authenticated<'a>(
    issuer: &'a RoleIssuer,
    caller: &'a AuthenticatedCaller<'a>,
    source: ImportedKnowledgeTextSource<'a>,
    original: &'a RequestContext<'a>,
    request: KnowledgeTextExportRequest<'_>,
    policy: &KnowledgeTextExportPolicy,
) -> Result<KnowledgeTextExportOutcome, KnowledgeTextExportError> {
    original
        .snapshot()
        .check()
        .map_err(KnowledgeTextExportError::Request)?;
    if !std::ptr::eq(caller.request(), original) {
        return Err(KnowledgeTextExportError::Authorization(AuthError::Denied));
    }
    check_external(original, caller)?;
    let admission = issuer
        .admit_authenticated_imported_knowledge_read(caller, source.installation)
        .map_err(KnowledgeTextExportError::Authorization)?;
    let opened = open_imported_knowledge(
        &admission,
        source.input,
        ImportedKnowledgeReadLimits {
            directory: source.directory,
            index: policy.index,
        },
    )
    .await
    .map_err(|cause| {
        KnowledgeTextExportError::Imported(ImportedKnowledgeSourceError::Reader(cause))
    })?;
    let reader = match opened {
        ImportedKnowledgeOpen::RegistryAbsent => {
            admission
                .recheck()
                .map_err(KnowledgeTextExportError::Authorization)?;
            check_external(original, caller)?;
            return Ok(KnowledgeTextExportOutcome::RegistryAbsent);
        }
        ImportedKnowledgeOpen::Published(reader) => reader,
    };
    let capture = reader
        .capture_source(policy.index)
        .await
        .map_err(KnowledgeTextExportError::Imported)?;
    let generation = capture.source_generation().clone();
    let recheck = || {
        check_external(original, caller)?;
        capture
            .recheck()
            .map_err(KnowledgeTextExportError::Imported)
    };
    recheck()?;
    let mut stage = None;
    let mut _output_parent = None;
    while let Some(documents) = capture
        .next_documents()
        .await
        .map_err(KnowledgeTextExportError::Imported)?
    {
        recheck()?;
        for document in documents {
            recheck()?;
            if stage.is_none() {
                reader
                    .validate_export_destination(request.destination)
                    .map_err(|cause| {
                        KnowledgeTextExportError::Imported(ImportedKnowledgeSourceError::Reader(
                            cause,
                        ))
                    })?;
            }
            stage_document(
                &mut stage,
                &mut _output_parent,
                original.snapshot(),
                &request,
                policy,
                document,
            )?;
        }
    }
    recheck()?;
    finish_text(stage, original, request, generation, recheck)
}

fn check_external(
    original: &RequestContext<'_>,
    caller: &AuthenticatedCaller<'_>,
) -> Result<(), KnowledgeTextExportError> {
    original
        .snapshot()
        .check()
        .map_err(KnowledgeTextExportError::Request)?;
    if caller
        .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
        .map_err(KnowledgeTextExportError::Authorization)?
        != Obligation::Unconditional
    {
        return Err(KnowledgeTextExportError::Authorization(AuthError::Denied));
    }
    Ok(())
}

fn stage_document<'a>(
    stage: &mut Option<MarkdownExportStage<'a>>,
    parent_handle: &mut Option<File>,
    original: &'a SnapshotContext,
    request: &KnowledgeTextExportRequest<'_>,
    policy: &KnowledgeTextExportPolicy,
    document: KnowledgeDocument,
) -> Result<(), KnowledgeTextExportError> {
    if stage.is_none() {
        let parent = destination_parent(request.destination)?;
        *parent_handle = Some(
            graphit_fs::read_directory::open_root_readonly(parent, &mut || original.check())
                .map_err(KnowledgeTextExportError::Input)?,
        );
        *stage = Some(
            MarkdownExportStage::new_in(
                original,
                parent,
                policy.modes,
                policy.text,
                request.format.clone(),
            )
            .map_err(KnowledgeTextExportError::Preparation)?,
        );
    }
    stage
        .as_mut()
        .expect("initialized for this document")
        .push(original, document)
        .map_err(KnowledgeTextExportError::Preparation)
}

fn finish_text(
    stage: Option<MarkdownExportStage<'_>>,
    original: &RequestContext<'_>,
    request: KnowledgeTextExportRequest<'_>,
    generation: KnowledgeGeneration,
    recheck: impl Fn() -> Result<(), KnowledgeTextExportError>,
) -> Result<KnowledgeTextExportOutcome, KnowledgeTextExportError> {
    recheck()?;
    let Some(stage) = stage else {
        return Ok(KnowledgeTextExportOutcome::PublishedEmpty(generation));
    };
    let plan = stage
        .finish(original.snapshot())
        .map_err(KnowledgeTextExportError::Preparation)?;
    publish_text_plan(original, request, plan, generation, recheck)
}

// A concrete publisher shared by consumers retaining their own real read authority.
fn publish_text_plan(
    original: &RequestContext<'_>,
    request: KnowledgeTextExportRequest<'_>,
    plan: MarkdownExportPlan,
    generation: KnowledgeGeneration,
    recheck: impl Fn() -> Result<(), KnowledgeTextExportError>,
) -> Result<KnowledgeTextExportOutcome, KnowledgeTextExportError> {
    recheck()?;
    plan.payload
        .revalidate_content(original.snapshot())
        .map_err(KnowledgeTextExportError::Input)?;
    recheck()?;
    let fingerprint = plan.payload.fingerprint();
    let output = request.destination.to_owned();
    if let Err(cause) = graphit_fs::rename_exclusive(&plan.directory(), request.destination) {
        let (_, cleanup) = plan.close();
        return Err(KnowledgeTextExportError::Publication {
            cause,
            output,
            cleanup,
        });
    }
    // Rename is now a known completed effect, recorded before any fallible final work.
    if let Err(cause) = recheck() {
        let (_, cleanup) = plan.close();
        return Err(KnowledgeTextExportError::AfterPublication {
            output,
            generation: Box::new(generation),
            fingerprint,
            cause: Box::new(cause),
            cleanup,
        });
    }
    let (communities, cleanup) = plan.close();
    if let Err(cause) = recheck() {
        return Err(KnowledgeTextExportError::AfterPublication {
            output,
            generation: Box::new(generation),
            fingerprint,
            cause: Box::new(cause),
            cleanup,
        });
    }
    Ok(KnowledgeTextExportOutcome::Published(Box::new(
        KnowledgeTextPublished {
            output,
            generation,
            communities,
            fingerprint,
            cleanup: cleanup.map_err(KnowledgeTextExportError::Cleanup),
        },
    )))
}
