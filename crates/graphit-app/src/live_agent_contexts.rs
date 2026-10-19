//! Selected Knowledge pins for Live preparation. Selectors are never grants.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use crate::{
    knowledge_live::{ArtifactSelection, PreparedSelection, ProjectSelection},
    live_agent_tools::SelectedContext,
    reference_explorer::{ArtifactSelector, Resources, SourceKey},
    source_server::remote_native::Opened,
    task_server::TaskPolicy,
};
use graphit_auth::{
    ImportedKnowledgeReadAdmission, InstalledHubReadAdmission, ProjectReadAdmission,
    ServiceCredentialLease,
};
use graphit_core::{artifact_store::StoreOwner, file_io::SnapshotContext};
use graphit_data::{
    knowledge_embeddings::{KnowledgeContextCapture, capture_native_knowledge_context},
    knowledge_index::{KnowledgeGeneration, physical_schema},
    lance_module_store::ModuleSnapshot,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget,
    registry::{HubScope, InstalledBackend},
};
use graphit_knowledge::imported_knowledge::{
    CapturedImportedKnowledge, ImportedKnowledgeInput, ImportedKnowledgeOpen,
    ImportedKnowledgeReadLimits, open_imported_knowledge,
};
use std::{io, sync::Arc};

/// Typed selection data is retained before readers that borrow its IDs.
pub(crate) struct ContextSelection<'a> {
    pub(crate) selection: &'a ArtifactSelection,
    pub(crate) source: SourceKey,
    pub(crate) imported: Option<(ArtifactId, ConcreteVersion)>,
    pub(crate) project_index: Option<usize>,
}

/// The Create request's selected PROJECT, never the dashboard selection.
pub(crate) fn project_selection(project: &ProjectSelection) -> ArtifactSelection {
    ArtifactSelection {
        source: "project".into(),
        project_id: Some(project.project_id.clone()),
        instance_id: None,
        project_kind: None,
        artifact_type: "knowledge".into(),
        id: project.project_id.clone(),
        version: project.project_version.clone(),
    }
}

fn denied() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}
fn invalid() -> io::Error {
    io::ErrorKind::InvalidInput.into()
}
fn unavailable() -> io::Error {
    io::ErrorKind::NotFound.into()
}

/// Resolve only selected Knowledge contexts under installation metadata authority.
/// PROJECT read admissions and imported-data admissions are supplied separately.
pub(crate) fn select_contexts<'a>(
    resources: &Resources<'_>,
    hub: &InstalledHubReadAdmission<'_>,
    selections: impl IntoIterator<Item = &'a ArtifactSelection>,
) -> io::Result<Vec<ContextSelection<'a>>> {
    hub.recheck().map_err(|_| denied())?;
    let mut selected = Vec::new();
    for selection in selections {
        if selection.artifact_type != "knowledge" {
            return Err(invalid());
        }
        let (source, project_index, imported) = match selection.source.as_str() {
            "project" => {
                if selection.project_id.as_deref() != Some(selection.id.as_str())
                    || selection.instance_id.is_some()
                    || selection.project_kind.is_some()
                {
                    return Err(denied());
                }
                let mut matching =
                    resources
                        .projects
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, id, version))| {
                            id.to_string() == selection.id && version.version() == selection.version
                        });
                let (index, project) = matching.next().ok_or_else(unavailable)?;
                if matching.next().is_some() {
                    return Err(invalid());
                }
                resources.require_active(project).map_err(|failure| {
                    if let Err(cause) = hub.request().snapshot().check() {
                        return cause;
                    }
                    let (status, code) = failure.response();
                    let kind = match failure {
                        crate::reference_explorer::Failure::Physical(cause) => cause.kind(),
                        crate::reference_explorer::Failure::Stopped => io::ErrorKind::Interrupted,
                        _ => match status.as_u16() {
                            400 | 409 => io::ErrorKind::InvalidInput,
                            401 | 403 => io::ErrorKind::PermissionDenied,
                            404 => io::ErrorKind::NotFound,
                            408 | 504 => io::ErrorKind::TimedOut,
                            _ => io::ErrorKind::Other,
                        },
                    };
                    io::Error::new(kind, code)
                })?;
                (SourceKey::ProjectKnowledge, Some(index), None)
            }
            "hub" => {
                let id = ArtifactId::new(&selection.id, resources.policy.role_reference)
                    .map_err(|_| invalid())?;
                let version =
                    ConcreteVersion::new(&selection.version, resources.policy.role_reference)
                        .map_err(|_| invalid())?;
                let policy = resources
                    .policy
                    .imported_knowledge
                    .as_ref()
                    .ok_or_else(unavailable)?;
                let scope = hub.installation();
                let StoreOwner::Installation { installation_id } = scope.live.owner() else {
                    return Err(denied());
                };
                if policy.registry.path().parent() != Some(scope.live.owner_root())
                    || !matches!(policy.registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
                {
                    return Err(denied());
                }
                hub.recheck().map_err(|_| denied())?;
                let document = policy.registry.load()?;
                let artifact = document
                    .get_version(ArtifactType::Knowledge, id.as_str(), &version)
                    .ok_or_else(unavailable)?;
                let mut entries = artifact.installations.iter().filter(|installation| {
                    installation.target == InstallationTarget::GlobalNative
                        && matches!(
                            installation.backend,
                            InstalledBackend::Native | InstalledBackend::RemoteNative(_)
                        )
                });
                let installation = entries.next().ok_or_else(unavailable)?;
                if entries.next().is_some()
                    || crate::live_agent_catalog::selection(&installation.original_source)
                        != *selection
                {
                    return Err(denied());
                }
                hub.recheck().map_err(|_| denied())?;
                (
                    SourceKey::ImportedKnowledge {
                        artifact: ArtifactSelector {
                            id: selection.id.clone(),
                            version: selection.version.clone(),
                            context_alias: None,
                        },
                    },
                    None,
                    Some((id, version)),
                )
            }
            _ => return Err(invalid()),
        };
        selected.push(ContextSelection {
            selection,
            source,
            imported,
            project_index,
        });
    }
    hub.recheck().map_err(|_| denied())?;
    Ok(selected)
}

/// Preserve each SDK cursor representation without coercing installation bindings.
#[derive(serde::Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub(crate) enum EvidenceReferences {
    Native(graphit_data::knowledge_reference_query::KnowledgeReferenceSnapshotRead),
    Imported(graphit_knowledge::imported_reference_query::ImportedKnowledgeReferencesRead),
}

pub(crate) struct EvidenceDocument {
    pub(crate) document: Box<graphit_data::knowledge_index::KnowledgeDocument>,
    pub(crate) refs: EvidenceReferences,
}

fn reference_error(
    error: graphit_data::knowledge_reference_query::KnowledgeReferenceQueryError,
) -> io::Error {
    let kind = if matches!(
        error,
        graphit_data::knowledge_reference_query::KnowledgeReferenceQueryError::UnknownSource
    ) {
        io::ErrorKind::NotFound
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, error)
}

enum Reader<'a> {
    Project {
        capture: Box<KnowledgeContextCapture>,
        read: &'a ProjectReadAdmission<'a>,
    },
    Imported(Box<CapturedImportedKnowledge<'a>>),
    Remote {
        opened: &'a Opened<'a>,
        pin: &'a ModuleSnapshot,
        lease: &'a ServiceCredentialLease<'a>,
    },
}

pub(crate) struct CapturedKnowledgeContext<'a> {
    selected: &'a ContextSelection<'a>,
    original: Arc<SnapshotContext>,
    generation: KnowledgeGeneration,
    reader: Reader<'a>,
}
impl CapturedKnowledgeContext<'_> {
    pub(crate) fn prepared(&self) -> PreparedSelection<'_> {
        PreparedSelection::ReadContext {
            selection: self.selected.selection,
            original: &self.original,
        }
    }
    pub(crate) fn selected(&self) -> SelectedContext<'_> {
        SelectedContext {
            selection: self.selected.selection,
            source: &self.selected.source,
        }
    }
    pub(crate) async fn document(
        &self,
        slug: &str,
        limits: graphit_data::knowledge_index::KnowledgeIndexReadLimits,
    ) -> io::Result<Option<Box<graphit_data::knowledge_index::KnowledgeDocument>>> {
        use graphit_data::{
            knowledge_index::KnowledgeIndexError,
            knowledge_snapshot::{KnowledgeSnapshotPage, read_knowledge_snapshot_guarded},
        };
        self.recheck()?;
        let page = match &self.reader {
            Reader::Project { capture, read } => capture
                .page(&self.original, read, slug, limits)
                .await
                .map_err(io::Error::other)?,
            Reader::Imported(reader) => {
                reader.page(slug, limits).await.map_err(io::Error::other)?
            }
            Reader::Remote { pin, .. } => {
                read_knowledge_snapshot_guarded(&self.original, pin, slug, limits, || {
                    self.recheck().map_err(KnowledgeIndexError::Request)
                })
                .await
                .map_err(io::Error::other)?
            }
        };
        self.recheck()?;
        Ok(match page {
            KnowledgeSnapshotPage::Found { document, .. } => Some(document),
            _ => None,
        })
    }
    pub(crate) async fn references(
        &self,
        slug: &str,
        limits: graphit_data::knowledge_index::KnowledgeIndexReadLimits,
        page_size: std::num::NonZeroUsize,
    ) -> io::Result<EvidenceReferences> {
        use graphit_data::{
            knowledge_index::KnowledgeIndexError,
            knowledge_reference_query::{
                KnowledgeReferenceQuery, read_knowledge_reference_snapshot_guarded,
            },
        };
        self.recheck()?;
        let query = KnowledgeReferenceQuery {
            outgoing: Some(slug),
            incoming: None,
            page_size,
            outgoing_cursor: None,
            incoming_cursor: None,
        };
        let refs=match &self.reader {
            Reader::Project{capture,read}=>EvidenceReferences::Native(capture.references(&self.original,read,query,limits).await.map_err(reference_error)?),
            Reader::Imported(reader)=>EvidenceReferences::Imported(reader.references(
                graphit_knowledge::imported_reference_query::ImportedKnowledgeReferencesQuery{outgoing:Some(slug),incoming:None,page_size,outgoing_cursor:None,incoming_cursor:None},
                limits).await.map_err(|error| match error {
                    graphit_knowledge::imported_reference_query::ImportedKnowledgeReferenceError::Query(error)=>reference_error(error),
                    error=>io::Error::other(error),
                })?),
            Reader::Remote{pin,..}=>EvidenceReferences::Native(read_knowledge_reference_snapshot_guarded(&self.original,pin,&self.generation,query,limits,
                ||self.recheck().map_err(KnowledgeIndexError::Request)).await.map_err(reference_error)?),
        };
        self.recheck()?;
        Ok(refs)
    }
    /// Body and references are both read from this one retained Source generation.
    pub(crate) async fn page(
        &self,
        slug: &str,
        limits: graphit_data::knowledge_index::KnowledgeIndexReadLimits,
        page_size: std::num::NonZeroUsize,
    ) -> io::Result<Option<EvidenceDocument>> {
        let Some(document) = self.document(slug, limits).await? else {
            return Ok(None);
        };
        let refs = self.references(slug, limits, page_size).await?;
        Ok(Some(EvidenceDocument { document, refs }))
    }
    pub(crate) fn recheck(&self) -> io::Result<()> {
        self.original.check()?;
        match &self.reader {
            Reader::Project { capture, read } => capture
                .recheck(&self.original, read)
                .map_err(io::Error::other)?,
            Reader::Imported(reader) => reader.recheck().map_err(io::Error::other)?,
            Reader::Remote { opened, lease, .. } => {
                opened.check(lease).map_err(io::Error::other)?;
            }
        }
        self.original.check()
    }
}

pub(crate) async fn capture_project<'a>(
    selected: &'a ContextSelection<'a>,
    original: &Arc<SnapshotContext>,
    read: &'a ProjectReadAdmission<'a>,
    policy: &TaskPolicy,
) -> io::Result<CapturedKnowledgeContext<'a>> {
    capture_project_optional(selected, original, read, policy)
        .await?
        .ok_or_else(unavailable)
}

pub(crate) async fn capture_project_optional<'a>(
    selected: &'a ContextSelection<'a>,
    original: &Arc<SnapshotContext>,
    read: &'a ProjectReadAdmission<'a>,
    policy: &TaskPolicy,
) -> io::Result<Option<CapturedKnowledgeContext<'a>>> {
    if !matches!(selected.source, SourceKey::ProjectKnowledge) {
        return Err(invalid());
    }
    let project = read.project().map_err(|_| denied())?;
    if project.version.version() != selected.selection.version
        || !matches!(project.scope.live.owner(), StoreOwner::Project { project_id } if project_id.to_string() == selected.selection.id)
    {
        return Err(denied());
    }
    let Some(capture) = capture_native_knowledge_context(original, read, policy.read.native)
        .await
        .map_err(io::Error::other)?
    else {
        return Ok(None);
    };
    let generation = capture.generation().clone();
    let context = CapturedKnowledgeContext {
        selected,
        original: Arc::clone(original),
        generation,
        reader: Reader::Project {
            capture: Box::new(capture),
            read,
        },
    };
    context.recheck()?;
    Ok(Some(context))
}

pub(crate) async fn capture_local_imported<'a>(
    selected: &'a ContextSelection<'a>,
    resources: &Resources<'a>,
    read: &'a ImportedKnowledgeReadAdmission<'a>,
) -> io::Result<CapturedKnowledgeContext<'a>> {
    if !matches!(selected.source, SourceKey::ImportedKnowledge { .. }) {
        return Err(invalid());
    }
    let policy = resources
        .policy
        .imported_knowledge
        .as_ref()
        .ok_or_else(unavailable)?;
    let (id, version) = selected.imported.as_ref().ok_or_else(invalid)?;
    let opened = open_imported_knowledge(
        read,
        ImportedKnowledgeInput {
            registry: &policy.registry,
            id,
            version,
            context_alias: None,
        },
        ImportedKnowledgeReadLimits {
            directory: policy.directory,
            index: crate::foreground_knowledge::limits(resources.policy)
                .map_err(io::Error::other)?,
        },
    )
    .await
    .map_err(io::Error::other)?;
    let ImportedKnowledgeOpen::Published(reader) = opened else {
        return Err(unavailable());
    };
    if selected.selection.project_id.as_deref()
        != Some(reader.observation().producer.to_string().as_str())
    {
        return Err(denied());
    }
    let generation = reader.observation().generation.clone();
    let context = CapturedKnowledgeContext {
        selected,
        original: Arc::clone(read.request().snapshot()),
        generation,
        reader: Reader::Imported(reader),
    };
    context.recheck()?;
    Ok(context)
}

/// The operation owns Opened, its exact Wiki pin and the HubRead lease lexically.
pub(crate) async fn capture_remote_imported<'a>(
    selected: &'a ContextSelection<'a>,
    opened: &'a Opened<'a>,
    pin: &'a ModuleSnapshot,
    lease: &'a ServiceCredentialLease<'a>,
) -> io::Result<CapturedKnowledgeContext<'a>> {
    if !matches!(selected.source, SourceKey::ImportedKnowledge { .. })
        || opened.selection.source().artifact_type() != ArtifactType::Knowledge
        || crate::live_agent_catalog::selection(opened.selection.source()) != *selected.selection
        || pin.schema() != physical_schema().as_ref()
    {
        return Err(denied());
    }
    opened.check(lease).map_err(io::Error::other)?;
    let original = opened.selection.request().snapshot();
    // Mount::wiki already checked the exact schema/version, manifest location and SHA.
    // The retained URI is data; rereading manifest indexes adds no context validation.
    let generation = KnowledgeGeneration {
        corpus_uri: pin.corpus_uri().to_owned(),
        version: pin.version(),
        physical_branch: None,
        inherited_target: None,
    };
    let context = CapturedKnowledgeContext {
        selected,
        original: Arc::clone(original),
        generation,
        reader: Reader::Remote { opened, pin, lease },
    };
    context.recheck()?;
    Ok(context)
}
