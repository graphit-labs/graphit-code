//! Request-scoped readonly access to registered local Native Knowledge.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod semantic;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod source;
use graphit_auth::{AuthError, ImportedKnowledgeReadAdmission};
use graphit_core::{
    artifact_store::StoreOwner,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    project_identity::ProjectId,
};
use graphit_data::{
    knowledge_index::{
        KnowledgeGeneration, KnowledgeHeadRead, KnowledgeHeadReadLimits, KnowledgeIndexError,
        KnowledgeIndexReadLimits,
    },
    knowledge_snapshot::{
        KnowledgeSnapshotPage, read_knowledge_heads_snapshot_guarded,
        read_knowledge_snapshot_guarded,
    },
    lance_module_store::ModuleSnapshot,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget,
    registry::{HubRegistry, HubScope, Installation, InstalledBackend, InstalledPayload},
};
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub use semantic::{
    CapturedImportedKnowledgeSemantic, ImportedKnowledgeEmbeddingInput,
    ImportedKnowledgeSemanticError,
};
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub use source::{
    CapturedImportedKnowledgeSource, ImportedKnowledgePhysicalSource, ImportedKnowledgeSourceError,
};
use std::{error::Error, fmt, io, path::Path};

#[derive(Clone, Copy)]
pub struct ImportedKnowledgeInput<'a> {
    pub registry: &'a HubRegistry,
    pub id: &'a ArtifactId,
    pub version: &'a ConcreteVersion,
    pub context_alias: Option<&'a str>,
}
#[derive(Clone, Copy)]
pub struct ImportedKnowledgeReadLimits {
    pub directory: NativeDocumentFileLimits,
    pub index: KnowledgeIndexReadLimits,
}
/// Selection and generation observations only; they grant no access.
pub struct ImportedKnowledgeObservation<'a> {
    pub producer: ProjectId,
    pub id: &'a ArtifactId,
    pub version: &'a ConcreteVersion,
    pub context_alias: Option<&'a str>,
    pub generation: KnowledgeGeneration,
}
pub enum ImportedKnowledgeOpen<'a> {
    RegistryAbsent,
    Published(Box<CapturedImportedKnowledge<'a>>),
}
#[derive(Debug)]
pub enum ImportedKnowledgeReadError {
    Auth(AuthError),
    Registry(io::Error),
    Capture(io::Error),
    Data(KnowledgeIndexError),
    InvalidRegistryScope,
    UnsupportedBinding,
    InvalidLocator,
    SourceSelectionChanged,
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[derive(Debug)]
pub enum ImportedKnowledgeSearchError {
    Reader(ImportedKnowledgeReadError),
    Query(graphit_data::knowledge_search::KnowledgeSearchError),
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
impl fmt::Display for ImportedKnowledgeSearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reader(error) => write!(f, "imported Knowledge search: {error}"),
            Self::Query(error) => write!(f, "imported Knowledge search: {error}"),
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
impl Error for ImportedKnowledgeSearchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reader(error) => Some(error),
            Self::Query(error) => Some(error),
        }
    }
}
impl fmt::Display for ImportedKnowledgeReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auth(_) => "imported Knowledge authority check failed",
            Self::Registry(_) => "imported Knowledge registry read failed",
            Self::Capture(_) => "imported Knowledge directory capture failed",
            Self::Data(_) => "imported Knowledge SDK read failed",
            Self::InvalidRegistryScope => {
                "imported Knowledge registry belongs to another installation"
            }
            Self::UnsupportedBinding => "imported Knowledge binding is not local Native Original",
            Self::InvalidLocator => "imported Knowledge locator is invalid",
            Self::SourceSelectionChanged => "imported Knowledge selection changed",
        })
    }
}
impl Error for ImportedKnowledgeReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(e) => Some(e),
            Self::Registry(e) | Self::Capture(e) => Some(e),
            Self::Data(e) => Some(e),
            _ => None,
        }
    }
}
/// Retains a private SDK pin and the caller's real borrowed admission.
/// ```compile_fail
/// fn escape(reader: &graphit_knowledge::imported_knowledge::CapturedImportedKnowledge<'_>) {
///     let _ = &reader.snapshot;
/// }
/// ```
pub struct CapturedImportedKnowledge<'a> {
    admission: &'a ImportedKnowledgeReadAdmission<'a>,
    input: ImportedKnowledgeInput<'a>,
    installation: Installation,
    root: CapturedDocumentDirectory,
    directory: CapturedDocumentDirectory,
    snapshot: ModuleSnapshot,
    pin: graphit_data::lance_branch_store::BranchSnapshot,
    observation: ImportedKnowledgeObservation<'a>,
}
fn select(
    admission: &ImportedKnowledgeReadAdmission<'_>,
    input: ImportedKnowledgeInput<'_>,
) -> Result<Option<Installation>, ImportedKnowledgeReadError> {
    admission
        .recheck()
        .map_err(ImportedKnowledgeReadError::Auth)?;
    let scope = admission.installation();
    let StoreOwner::Installation { installation_id } = scope.live.owner() else {
        return Err(ImportedKnowledgeReadError::InvalidRegistryScope);
    };
    if !matches!(input.registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
        || input.registry.path().parent() != Some(scope.live.owner_root())
    {
        return Err(ImportedKnowledgeReadError::InvalidRegistryScope);
    }
    let document = input
        .registry
        .load()
        .map_err(ImportedKnowledgeReadError::Registry)?;
    admission
        .recheck()
        .map_err(ImportedKnowledgeReadError::Auth)?;
    let Some(artifact) =
        document.get_version(ArtifactType::Knowledge, input.id.as_str(), input.version)
    else {
        return Ok(None);
    };
    let selected = artifact
        .installations
        .iter()
        .find(|i| i.target == InstallationTarget::GlobalNative)
        .ok_or(ImportedKnowledgeReadError::UnsupportedBinding)?;
    Ok(Some(selected.clone()))
}
fn check_binding(
    installation: &Installation,
    input: ImportedKnowledgeInput<'_>,
) -> Result<(), ImportedKnowledgeReadError> {
    if !matches!(installation.backend, InstalledBackend::Native)
        || !matches!(
            installation.payload,
            InstalledPayload::Original {
                selection_binding: None
            }
        )
        || installation.alias.is_some()
        || installation.entry_name.is_some()
        || installation.original_source.artifact_type() != ArtifactType::Knowledge
        || installation.original_source.id() != input.id
        || installation.original_source.version() != input.version
    {
        return Err(ImportedKnowledgeReadError::UnsupportedBinding);
    }
    Ok(())
}
fn current(
    admission: &ImportedKnowledgeReadAdmission<'_>,
    input: ImportedKnowledgeInput<'_>,
    expected: &Installation,
) -> Result<(), ImportedKnowledgeReadError> {
    if select(admission, input)?.as_ref() != Some(expected) {
        return Err(ImportedKnowledgeReadError::SourceSelectionChanged);
    }
    Ok(())
}
fn capture(
    admission: &ImportedKnowledgeReadAdmission<'_>,
    path: &Path,
    limits: NativeDocumentFileLimits,
) -> Result<CapturedDocumentDirectory, ImportedKnowledgeReadError> {
    admission
        .recheck()
        .map_err(ImportedKnowledgeReadError::Auth)?;
    let captured = CapturedDocumentDirectory::capture(path, limits, admission.request().snapshot())
        .map_err(ImportedKnowledgeReadError::Capture)?;
    if captured.canonical_path() != path {
        return Err(ImportedKnowledgeReadError::InvalidLocator);
    }
    Ok(captured)
}
/// Open the current default SDK generation of an exact registered artifact version.
/// ConcreteVersion is never a Lance branch or dataset version. A required corpus
/// that is missing or corrupt is an error, not an unpublished registry outcome.
pub async fn open_imported_knowledge<'a>(
    admission: &'a ImportedKnowledgeReadAdmission<'a>,
    input: ImportedKnowledgeInput<'a>,
    limits: ImportedKnowledgeReadLimits,
) -> Result<ImportedKnowledgeOpen<'a>, ImportedKnowledgeReadError> {
    admission
        .recheck()
        .map_err(ImportedKnowledgeReadError::Auth)?;
    if let Some(alias) = input.context_alias
        && (alias.is_empty() || alias.len() > limits.index.content.max_identifier_bytes.get())
    {
        return Err(ImportedKnowledgeReadError::InvalidLocator);
    }
    let Some(installation) = select(admission, input)? else {
        if select(admission, input)?.is_some() {
            return Err(ImportedKnowledgeReadError::SourceSelectionChanged);
        }
        return Ok(ImportedKnowledgeOpen::RegistryAbsent);
    };
    check_binding(&installation, input)?;
    let root = capture(
        admission,
        installation.original_source.directory().as_path(),
        limits.directory,
    )?;
    current(admission, input, &installation)?;
    let path = root.canonical_path().join("index.lance");
    let directory = capture(admission, &path, limits.directory)?;
    current(admission, input, &installation)?;
    root.revalidate(admission.request().snapshot())
        .map_err(ImportedKnowledgeReadError::Capture)?;
    directory
        .revalidate(admission.request().snapshot())
        .map_err(ImportedKnowledgeReadError::Capture)?;
    let uri = local_locator(directory.canonical_path())?;
    let pin = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(&uri, None, None)
        .await
        .map_err(|e| ImportedKnowledgeReadError::Data(KnowledgeIndexError::Branch(e)))?;
    graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(pin.schema())
        .map_err(|e| {
            ImportedKnowledgeReadError::Data(KnowledgeIndexError::Request(io::Error::other(e)))
        })?;
    let snapshot = pin.module_snapshot();
    let generation = KnowledgeGeneration {
        corpus_uri: uri,
        version: snapshot.version(),
        physical_branch: None,
        inherited_target: None,
    };
    let reader = CapturedImportedKnowledge {
        admission,
        input,
        observation: ImportedKnowledgeObservation {
            producer: installation.original_source.producer(),
            id: input.id,
            version: input.version,
            context_alias: input.context_alias,
            generation,
        },
        installation,
        root,
        directory,
        snapshot,
        pin,
    };
    reader.revalidate()?;
    Ok(ImportedKnowledgeOpen::Published(Box::new(reader)))
}
fn local_locator(path: &Path) -> Result<String, ImportedKnowledgeReadError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or(ImportedKnowledgeReadError::InvalidLocator)
}
impl<'a> CapturedImportedKnowledge<'a> {
    /// Lexical search on this retained pin and actual imported admission.
    /// Caller selection remains available through `observation`; payload namespaces
    /// are read from C8 and never inferred from the mount's producer or alias.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub async fn search(
        &self,
        query: graphit_data::knowledge_search::KnowledgeLexicalQuery<'_>,
        limits: graphit_data::knowledge_search::KnowledgeSearchLimits,
        preview_limits: crate::preview::MarkdownPreviewLimits,
    ) -> Result<graphit_data::knowledge_search::KnowledgeSearchPage, ImportedKnowledgeSearchError>
    {
        use ImportedKnowledgeSearchError as E;
        self.revalidate().map_err(E::Reader)?;
        // The registry tuple is part of the continuation's ordinary binding.
        let selection = crate::imported_reference_query::selection_binding(
            self.admission.request().snapshot(),
            &self.installation,
            self.input,
        )
        .map_err(|error| {
            E::Query(graphit_data::knowledge_search::KnowledgeSearchError::Data(
                KnowledgeIndexError::Request(io::Error::other(error)),
            ))
        })?;
        let selection = serde_json::to_string(&selection).map_err(|error| {
            E::Query(graphit_data::knowledge_search::KnowledgeSearchError::Data(
                KnowledgeIndexError::ReferenceSerialization(error),
            ))
        })?;
        if query.caller_binding.len() > limits.max_query_bytes.get() {
            return Err(E::Query(
                graphit_data::knowledge_search::KnowledgeSearchError::InvalidQuery("query_bytes"),
            ));
        }
        let binding = format!("{}:{}", selection.len(), selection) + query.caller_binding;
        let binding = crate::search::preview_binding(&binding, preview_limits);
        let query = graphit_data::knowledge_search::KnowledgeLexicalQuery {
            caller_binding: &binding,
            ..query
        };
        let mut first_failure = None;
        let result = graphit_data::knowledge_snapshot::search_knowledge_snapshot_guarded(
            self.admission.request().snapshot(),
            &self.snapshot,
            &self.observation.generation,
            query,
            limits,
            || match self.revalidate() {
                Ok(()) => Ok(()),
                Err(error) => {
                    if first_failure.is_none() {
                        first_failure = Some(error);
                    }
                    Err(KnowledgeIndexError::InvalidField("external_guard"))
                }
            },
            |body, summary, remaining| {
                crate::search::render_preview(
                    self.admission.request().snapshot(),
                    body,
                    summary,
                    preview_limits,
                    remaining,
                )
            },
        )
        .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        let page = result.map_err(E::Query)?;
        self.revalidate().map_err(E::Reader)?;
        Ok(page)
    }
    /// Read references on the retained pin, rechecking the actual imported authority.
    pub async fn references(
        &self,
        query: crate::imported_reference_query::ImportedKnowledgeReferencesQuery<'_>,
        limits: graphit_data::knowledge_index::KnowledgeIndexReadLimits,
    ) -> Result<
        crate::imported_reference_query::ImportedKnowledgeReferencesRead,
        crate::imported_reference_query::ImportedKnowledgeReferenceError,
    > {
        use crate::imported_reference_query::{
            ImportedKnowledgeReferenceError as E, selection_binding, wrap_read,
        };
        self.revalidate().map_err(E::Reader)?;
        let original = self.admission.request().snapshot();
        let binding = selection_binding(original, &self.installation, self.input)?;
        let query = query.data_query(&binding)?;
        let mut first_failure = None;
        let result =
            graphit_data::knowledge_reference_query::read_knowledge_reference_snapshot_guarded(
                original,
                &self.snapshot,
                &self.observation.generation,
                query,
                limits,
                || match self.revalidate() {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        if first_failure.is_none() {
                            first_failure = Some(error);
                        }
                        Err(KnowledgeIndexError::InvalidField("external_guard"))
                    }
                },
            )
            .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        let read = wrap_read(result.map_err(E::Query)?, binding);
        self.revalidate().map_err(E::Reader)?;
        Ok(read)
    }
    /// Recheck the retained registry selection, physical captures and authority.
    pub fn recheck(&self) -> Result<(), ImportedKnowledgeReadError> {
        self.revalidate()
    }

    pub fn observation(&self) -> &ImportedKnowledgeObservation<'a> {
        &self.observation
    }
    fn revalidate(&self) -> Result<(), ImportedKnowledgeReadError> {
        current(self.admission, self.input, &self.installation)?;
        let original = self.admission.request().snapshot();
        self.root
            .revalidate(original)
            .map_err(ImportedKnowledgeReadError::Capture)?;
        self.directory
            .revalidate(original)
            .map_err(ImportedKnowledgeReadError::Capture)?;
        self.admission
            .recheck()
            .map_err(ImportedKnowledgeReadError::Auth)
    }
    pub async fn page(
        &self,
        slug: &str,
        limits: KnowledgeIndexReadLimits,
    ) -> Result<KnowledgeSnapshotPage, ImportedKnowledgeReadError> {
        self.revalidate()?;
        let mut failure = None;
        let result = read_knowledge_snapshot_guarded(
            self.admission.request().snapshot(),
            &self.snapshot,
            slug,
            limits,
            || match self.revalidate() {
                Ok(()) => Ok(()),
                Err(error) => {
                    failure = Some(error);
                    Err(KnowledgeIndexError::InvalidField("external_guard"))
                }
            },
        )
        .await;
        if let Some(error) = failure {
            return Err(error);
        }
        let page = result.map_err(ImportedKnowledgeReadError::Data)?;
        self.revalidate()?;
        Ok(page)
    }
    pub async fn heads(
        &self,
        limits: KnowledgeHeadReadLimits,
    ) -> Result<KnowledgeHeadRead, ImportedKnowledgeReadError> {
        self.revalidate()?;
        let mut failure = None;
        let result = read_knowledge_heads_snapshot_guarded(
            self.admission.request().snapshot(),
            &self.snapshot,
            limits,
            || match self.revalidate() {
                Ok(()) => Ok(()),
                Err(error) => {
                    failure = Some(error);
                    Err(KnowledgeIndexError::InvalidField("external_guard"))
                }
            },
        )
        .await;
        if let Some(error) = failure {
            return Err(error);
        }
        let heads = result.map_err(ImportedKnowledgeReadError::Data)?;
        self.revalidate()?;
        Ok(KnowledgeHeadRead::Published {
            generation: self.observation.generation.clone(),
            heads,
        })
    }
}
