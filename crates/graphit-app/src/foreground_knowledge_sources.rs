//! Progressive Knowledge publication and independent background embeddings.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]

use crate::{
    ast_profile_loader::project_config,
    foreground_memory_jobs::{MemoryJobResources, background_embedding_rpc_timeout},
    knowledge_semantic::{
        KnowledgeSemanticIndexLimits, KnowledgeSemanticIndexResult, KnowledgeSemanticModel,
        index_knowledge_semantics_selection,
    },
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, ProjectSelectionLimits, ScopeLimits, SourceReadLimits,
    },
    config::ConfigSnapshot,
    file_lock::Cancellation,
    module_settings::{Module, ModuleSettings},
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::{auth_startup::DaemonAuth, supervisor::ProjectContext};
use graphit_data::{
    knowledge_document_lifecycle::KnowledgeDocumentLifecycleLimits,
    knowledge_document_projection::KnowledgeDocumentProjectionLimits,
    knowledge_document_update::KnowledgeDocumentUpdateResult,
    knowledge_index::{
        KnowledgeGeneration, KnowledgeIndexContentLimits, KnowledgeIndexReadLimits,
        KnowledgeIndexWriteLimits,
    },
};
use graphit_hub::{NativeBinding, policy::HubResourcePolicy};
use graphit_knowledge::{
    compilation_fingerprint::CompilationFingerprintLimits,
    desired_corpus::{KnowledgeDesiredPreparationLimits, KnowledgeSourceCaptureLimits},
    desired_reconciliation::{
        KnowledgeDesiredReconciliationLimits, KnowledgeDesiredReconciliationResult,
        reconcile_knowledge_source_changes,
    },
    document::DocumentParseLimits,
    document_authored_references::DocumentAuthoredReferenceLimits,
    native_document_mapping::NativeAuthoredMappingLimits,
    publication_metadata::PublicationMetadataLimits,
    references::ReferenceLimits,
    settings::{KnowledgeSettings, KnowledgeSettingsLimits},
};
use std::{
    collections::BTreeSet,
    error::Error,
    fmt, io,
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
    sync::Arc,
};

pub(super) struct KnowledgeSourceConfig {
    pub installation: Arc<InstallationPaths>,
    pub base: Arc<ConfigSnapshot>,
    pub policy: HubResourcePolicy,
    pub scope: ScopeLimits,
    pub native: NativeDocumentFileLimits,
    pub binding: NativeBinding,
}

/// Only stable resources are retained; every operation creates its own admissions.
pub(super) struct ForegroundKnowledgeSources {
    auth: Arc<DaemonAuth>,
    config: KnowledgeSourceConfig,
}

#[derive(Debug)]
pub(super) enum KnowledgeSemanticStatus {
    Disabled,
    ModelsUnavailable,
    Unchanged,
    Published(Box<KnowledgeSemanticIndexResult>),
}
#[derive(Debug)]
pub(super) struct KnowledgeProjectPublication {
    pub documents: KnowledgeDesiredReconciliationResult,
    pub semantics: KnowledgeSemanticStatus,
}

/// The entire real documental outcome survives every later semantic failure.
#[derive(Debug)]
pub(super) struct KnowledgeAfterPublication {
    pub documents: Box<KnowledgeDesiredReconciliationResult>,
    cause: io::Error,
}
impl fmt::Display for KnowledgeProjectPublication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "document generation {}",
            selected_generation(&self.documents).version
        )?;
        match &self.semantics {
            KnowledgeSemanticStatus::Unchanged => f.write_str("; no pending embeddings"),
            KnowledgeSemanticStatus::Disabled => f.write_str("; embeddings disabled"),
            KnowledgeSemanticStatus::ModelsUnavailable => {
                f.write_str("; embedding model unavailable")
            }
            KnowledgeSemanticStatus::Published(result) => {
                write!(f, "; semantic outcome {result:?}")
            }
        }
    }
}

impl fmt::Display for KnowledgeAfterPublication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Knowledge semantic phase failed after document generation {}",
            selected_generation(&self.documents).version
        )
    }
}
impl Error for KnowledgeAfterPublication {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}

pub(super) fn only_knowledge_semantics_failed(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|cause| cause.is::<KnowledgeAfterPublication>())
}

pub(super) fn knowledge_source_pending_paths(
    error: &io::Error,
) -> Option<&BTreeSet<std::path::PathBuf>> {
    match error.get_ref()?.downcast_ref::<graphit_knowledge::desired_reconciliation::KnowledgeDesiredReconciliationError>()? {
        graphit_knowledge::desired_reconciliation::KnowledgeDesiredReconciliationError::Published { pending_paths, .. }
            if !pending_paths.is_empty() => Some(pending_paths),
        _ => None,
    }
}

impl ForegroundKnowledgeSources {
    pub fn new(auth: Arc<DaemonAuth>, config: KnowledgeSourceConfig) -> Self {
        Self { auth, config }
    }

    pub async fn index(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        first_or_controls: bool,
        resume_semantics: bool,
        paths: &BTreeSet<std::path::PathBuf>,
        source_preemption: &Cancellation,
        background: &Cancellation,
        cancellation: &Cancellation,
        resources: Option<&Arc<MemoryJobResources>>,
    ) -> io::Result<Option<KnowledgeProjectPublication>> {
        if cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Knowledge cancelled before admission or publication",
            ));
        }
        let request = graphit_hub::policy::RequestContext::service(
            self.config.policy.content(),
            cancellation,
        )
        .map_err(io::Error::other)?;
        let before_file = || {
            request.check()?;
            if source_preemption.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Knowledge yielded between FILE publications",
                ));
            }
            Ok(())
        };
        before_file()?;
        if project.root() != selected.root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let config = project_config(
            &self.config.installation,
            project,
            &self.config.base,
            request.snapshot(),
        )?;
        let modules = ModuleSettings::resolve(&config)?;
        if !modules.enabled(Module::Knowledge) {
            return Ok(None);
        }
        let limits = self.operational_limits()?;
        let settings =
            KnowledgeSettings::resolve(request.snapshot(), &config, None, limits.capture.settings)
                .map_err(io::Error::other)?;
        let structural = first_or_controls || relevant_paths(project.root(), paths, &settings)?;
        if !structural && !resume_semantics {
            return Ok(None);
        }
        request.check()?;
        let captured = CapturedPublicationScope::project(
            project,
            selected.id,
            self.config.scope,
            request.snapshot(),
        )?;
        let record = captured.binding()?;
        let scope = ScopeInput {
            installation: self.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &record,
                },
                version: &selected.version,
            }),
            providers: &[],
            binding: self.config.binding,
            global_version: "",
        };
        let runtime = self
            .auth
            .runtime()
            .check(request.snapshot())
            .map_err(io::Error::other)?;
        let raw = std::str::from_utf8(
            runtime
                .secret(request.snapshot())
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(raw, &request)
            .map_err(io::Error::other)?;
        let authority = ProjectRecordAuthority::new(self.auth.issuer());
        let read = authority
            .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Knowledge)
            .map_err(io::Error::other)?;
        let write = authority
            .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Knowledge)
            .map_err(io::Error::other)?;
        let changed = if first_or_controls {
            None
        } else {
            Some(
                paths
                    .iter()
                    .map(|path| {
                        path.strip_prefix(project.root())
                            .map(Path::to_owned)
                            .map_err(io::Error::other)
                    })
                    .collect::<io::Result<BTreeSet<_>>>()?,
            )
        };
        let empty = BTreeSet::new();
        let changes = if structural {
            changed.as_ref()
        } else {
            Some(&empty)
        };
        let (sender, pending) = tokio::sync::mpsc::unbounded_channel::<String>();
        let produce = async {
            let notify = |slug: &str| {
                let _ = sender.send(slug.to_owned());
            };
            let mut documents = reconcile_knowledge_source_changes(
                request.snapshot(),
                &read,
                &write,
                &config,
                "document",
                changes,
                &notify,
                &before_file,
                limits,
            )
            .await;
            if !background.is_cancelled()
                && resume_semantics
                && structural
                && modules.enabled(Module::Embedding)
                && resources.is_some()
            {
                // A changed FILE does not supersede unfinished inference for an
                // intact FILE, nor the rest of a changed model's vector space.
                // Resume from committed metadata after the delta; never reparse.
                let resumed = reconcile_knowledge_source_changes(
                    request.snapshot(),
                    &read,
                    &write,
                    &config,
                    "document",
                    Some(&empty),
                    &notify,
                    &before_file,
                    limits,
                )
                .await;
                if let Err(cause) = resumed {
                    documents = match documents {
                        Ok(publication) => Err(graphit_knowledge::desired_reconciliation::KnowledgeDesiredReconciliationError::Published {
                            publication: Box::new(publication), pending_paths: BTreeSet::new(), cause: Box::new(cause),
                        }),
                        Err(source) => Err(source),
                    };
                }
            }
            drop(sender);
            documents
        };
        let semantic = async {
            if !modules.enabled(Module::Embedding) {
                drop(pending);
                return Ok(KnowledgeSemanticStatus::Disabled);
            }
            let Some(resources) = resources else {
                drop(pending);
                return Ok(KnowledgeSemanticStatus::ModelsUnavailable);
            };
            self.embed_pending_loop(
                project,
                selected,
                background,
                resources,
                raw,
                KnowledgeSemanticIndexLimits {
                    read: limits.index.read,
                    write: limits.index.write,
                },
                pending,
            )
            .await
        };
        let (documents, semantic) = tokio::join!(produce, semantic);
        let documents = match documents {
            Ok(documents) => documents,
            Err(graphit_knowledge::desired_reconciliation::KnowledgeDesiredReconciliationError::DataLifecycle(
                graphit_data::knowledge_document_lifecycle::KnowledgeDocumentLifecycleError::NoCorpus,
            )) if !structural => return semantic.map(|_| None),
            Err(cause) => return Err(io::Error::new(
                if source_preemption.is_cancelled() { io::ErrorKind::Interrupted } else {
                    request.check().err().map_or(io::ErrorKind::Other, |error| error.kind())
                }, cause,
            )),
        };
        match semantic {
            Ok(semantics) => Ok(Some(KnowledgeProjectPublication {
                documents,
                semantics,
            })),
            Err(cause) => Err(io::Error::new(
                if background.is_cancelled() {
                    io::ErrorKind::Interrupted
                } else {
                    cause.kind()
                },
                KnowledgeAfterPublication {
                    documents: Box::new(documents),
                    cause,
                },
            )),
        }
    }

    /// Admissions and their physical captures belong to one FILE operation,
    /// never to an idle receiver. Model identity is shared by the finite stream.
    pub(super) async fn embed_pending_loop(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        background: &Cancellation,
        resources: &MemoryJobResources,
        raw: &str,
        limits: KnowledgeSemanticIndexLimits,
        mut pending: tokio::sync::mpsc::UnboundedReceiver<String>,
    ) -> io::Result<KnowledgeSemanticStatus> {
        let request =
            graphit_hub::policy::RequestContext::service(self.config.policy.content(), background)
                .map_err(io::Error::other)?;
        let mut metadata = None;
        let mut latest = None;
        let mut completed = BTreeSet::new();
        loop {
            let next = tokio::select! {
                biased;
                () = background.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                next = pending.recv() => next,
            };
            let Some(slug) = next else { break };
            if !completed.insert(slug.clone()) {
                continue;
            }
            // Revalidate identity, authorization and project/version binding
            // after the wait. All of these captures drop before the next recv.
            request.check()?;
            let captured = CapturedPublicationScope::project(
                project,
                selected.id,
                self.config.scope,
                request.snapshot(),
            )?;
            let record = captured.binding()?;
            let scope = ScopeInput {
                installation: self.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &captured,
                        record: &record,
                    },
                    version: &selected.version,
                }),
                providers: &[],
                binding: self.config.binding,
                global_version: "",
            };
            let caller = self
                .auth
                .issuer()
                .authenticate_runtime(raw, &request)
                .map_err(io::Error::other)?;
            let authority = ProjectRecordAuthority::new(self.auth.issuer());
            let read = authority
                .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Knowledge)
                .map_err(io::Error::other)?;
            let write = authority
                .admit_authenticated_write(
                    &caller,
                    &request,
                    scope,
                    ProjectWriteOperation::Knowledge,
                )
                .map_err(io::Error::other)?;
            if metadata.is_none() {
                let metadata_request = crate::foreground_memory_jobs::background_embedding_request(
                    &self.config.policy,
                    request.snapshot(),
                    background,
                )?;
                metadata = Some(
                    resources
                        .metadata(&metadata_request)
                        .await
                        .map_err(io::Error::other)?,
                );
            }
            let metadata = metadata.as_ref().expect("metadata admitted for this FILE");
            let model = KnowledgeSemanticModel {
                metadata,
                batch_limits: resources.rpc_limits(),
                rpc_timeout: background_embedding_rpc_timeout(&self.config.policy),
            };
            let selected = BTreeSet::from([slug]);
            latest = Some(
                index_knowledge_semantics_selection(
                    request.snapshot(),
                    &read,
                    &write,
                    &model,
                    KnowledgeSemanticIndexLimits {
                        read: limits.read,
                        write: limits.write,
                    },
                    |operation| resources.embed_batch(request.snapshot(), metadata, operation),
                    &selected,
                )
                .await
                .map_err(io::Error::other)?,
            );
        }
        Ok(match latest {
            Some(result) => KnowledgeSemanticStatus::Published(Box::new(result)),
            None => KnowledgeSemanticStatus::Unchanged,
        })
    }

    fn operational_limits(&self) -> io::Result<KnowledgeDesiredReconciliationLimits> {
        let content = self.config.policy.content();
        let positive = |value| NonZeroUsize::new(value).ok_or(io::ErrorKind::InvalidInput);
        let input = positive(content.max_input_bytes)?;
        let output = positive(content.max_output_bytes)?;
        let scalar = positive(content.max_scalar_bytes)?;
        let nodes = positive(content.max_nodes)?;
        let entries = positive(content.max_collection_entries)?;
        let depth = positive(content.max_depth)?;
        let parser = DocumentParseLimits {
            max_document_bytes: input,
            max_input_bytes: input,
            max_frontmatter_bytes: input,
            max_yaml_events: nodes,
            max_markdown_events: nodes,
            max_yaml_depth: depth,
            max_scalar_bytes: scalar,
            max_links: entries,
            max_projection_bytes: output,
            max_summary_bytes: scalar,
        };
        let metadata = PublicationMetadataLimits {
            max_input_bytes: input,
            max_title_bytes: scalar,
            max_kind_bytes: scalar,
            max_description_bytes: scalar,
            max_effective_summary_bytes: parser.max_summary_bytes,
            max_effective_metadata_bytes: output,
            max_owned_metadata_bytes: output,
            max_root_entries: entries,
            max_typed_nodes: nodes,
            max_typed_depth: parser.max_yaml_depth,
            max_typed_scalar_bytes: parser.max_scalar_bytes,
            max_typed_events: parser.max_yaml_events,
            max_typed_lookahead_characters: parser.max_document_bytes,
            max_typed_flow_depth: parser.max_yaml_depth,
        };
        let content_limits = KnowledgeIndexContentLimits {
            max_identifier_bytes: scalar,
        };
        let read = KnowledgeIndexReadLimits {
            content: content_limits,
            native: self.config.native,
            batch_rows: NonZeroU32::new(8192).expect("SDK batch size is positive"),
        };
        let write = KnowledgeIndexWriteLimits {
            content: content_limits,
            native: self.config.native,
            artifacts: self.config.policy.artifacts(),
        };
        let file_bytes = u64::try_from(input.get()).map_err(io::Error::other)?;
        Ok(KnowledgeDesiredReconciliationLimits {
            capture: KnowledgeSourceCaptureLimits {
                settings: KnowledgeSettingsLimits {
                    max_input_bytes: input,
                    max_docs_dir_bytes: scalar,
                    max_path_components: depth,
                    max_extension_tokens: entries,
                    max_extensions: entries,
                    max_extension_bytes: scalar,
                    max_projection_bytes: output,
                },
                selection: ProjectSelectionLimits {
                    max_depth: depth.get(),
                    max_input_items: entries.get(),
                    max_input_bytes: input.get(),
                    max_observed_path_bytes: graphit_core::native_path::MAX_PATH_BYTES,
                    max_selected_file_bytes: file_bytes,
                    max_control_file_bytes: file_bytes,
                },
                read: SourceReadLimits::new(input.get(), input.get(), file_bytes)?,
            },
            preparation: KnowledgeDesiredPreparationLimits {
                parser,
                metadata,
                mapping: NativeAuthoredMappingLimits {
                    max_field_bytes: input,
                    max_uri_bytes: scalar,
                    max_target_bytes: scalar,
                },
            },
            fingerprint: CompilationFingerprintLimits {
                max_preimage_bytes: output,
            },
            references: ReferenceLimits {
                max_identifier_bytes: scalar,
            },
            authored_references: DocumentAuthoredReferenceLimits {
                max_results: entries,
                max_identifier_bytes: scalar,
            },
            projection: KnowledgeDocumentProjectionLimits {
                max_documents: entries,
                max_observations: entries,
                max_rows: entries,
                max_field_bytes: input,
                max_text_bytes: output,
            },
            index: KnowledgeDocumentLifecycleLimits { read, write },
        })
    }
}

fn selected_generation(documents: &KnowledgeDesiredReconciliationResult) -> &KnowledgeGeneration {
    match documents {
        KnowledgeDesiredReconciliationResult::First { publication, .. } => {
            publication.named.as_ref().unwrap_or(&publication.default)
        }
        KnowledgeDesiredReconciliationResult::Existing(
            KnowledgeDocumentUpdateResult::Unchanged { generation }
            | KnowledgeDocumentUpdateResult::Updated { generation, .. },
        ) => generation,
    }
}

fn relevant_paths(
    root: &Path,
    paths: &BTreeSet<std::path::PathBuf>,
    settings: &KnowledgeSettings,
) -> io::Result<bool> {
    for path in paths {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        let allowed = relative
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                settings.extensions().iter().any(|selected| {
                    selected
                        .strip_prefix('.')
                        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(extension))
                })
            });
        if settings.include_readme()
            && relative
                .parent()
                .is_some_and(|parent| parent.as_os_str().is_empty())
            && relative
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.eq_ignore_ascii_case("readme"))
            && allowed
        {
            return Ok(true);
        }
        if settings.docs_dir() != "." && !relative.starts_with(settings.docs_dir()) {
            continue;
        }
        if allowed {
            return Ok(true);
        }
        // A subtree itself can disappear in one watcher event.
        if relative == Path::new(settings.docs_dir()) {
            return Ok(true);
        }
    }
    Ok(false)
}
