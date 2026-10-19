//! Explicitly selected AST readers for Live. Persisted stamps are data, never grants.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod read;
#[cfg(test)]
pub(crate) use read::test_query_join;

use crate::{
    graph_server::GraphServer,
    knowledge_live::{ArtifactSelection, PreparedSelection},
    reference_explorer::Resources,
    source_server::{SourceServer, remote_native::Opened},
};
use graphit_ast::{
    entity_search::EntitySearch,
    imported_graph::{ImportedGraph, ImportedGraphInput, open_imported_graph_admitted},
    source_index_locator::{SourceIndexPresence, open_local_source_index},
};
use graphit_auth::{
    ImportedAstReadAdmission, InstalledHubReadAdmission, ProjectReadAdmission, ProjectReadModule,
    ScopeCapture, ServiceCredentialLease,
};
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, project_identity::ProjectId,
    project_paths::ProjectPaths, version_context::VersionContext,
};
use graphit_data::{CapturedLocalGraph, remote_graph::RemoteIcebugGraph};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget,
    policy::RequestContext,
    registry::{HubScope, InstalledBackend},
};
use std::{io, sync::Arc};

/// IDs are owned before the readers borrowing them are opened by the caller.
pub(crate) struct AstSelection<'a> {
    pub(crate) selection: &'a ArtifactSelection,
    pub(crate) project_index: Option<usize>,
    pub(crate) imported: Option<(ArtifactId, ConcreteVersion)>,
}

fn denied() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}
fn invalid() -> io::Error {
    io::ErrorKind::InvalidInput.into()
}
fn absent() -> io::Error {
    io::ErrorKind::NotFound.into()
}

pub(crate) fn select_contexts<'a>(
    resources: &Resources<'_>,
    hub: &InstalledHubReadAdmission<'_>,
    source: &SourceServer,
    selections: impl IntoIterator<Item = &'a ArtifactSelection>,
) -> io::Result<Vec<AstSelection<'a>>> {
    hub.recheck().map_err(|_| denied())?;
    let mut result = Vec::new();
    for selection in selections {
        if selection.artifact_type != "ast" {
            return Err(invalid());
        }
        let (project_index, imported) = match selection.source.as_str() {
            "project" => {
                if selection.project_id.as_deref() != Some(selection.id.as_str())
                    || selection.instance_id.is_some()
                    || selection.project_kind.is_some()
                {
                    return Err(denied());
                }
                let mut projects =
                    resources
                        .projects
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, id, version))| {
                            id.to_string() == selection.id && version.version() == selection.version
                        });
                let (index, project) = projects.next().ok_or_else(absent)?;
                if projects.next().is_some() {
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
                (Some(index), None)
            }
            "hub" => {
                let id = ArtifactId::new(&selection.id, resources.policy.role_reference)
                    .map_err(|_| invalid())?;
                let version =
                    ConcreteVersion::new(&selection.version, resources.policy.role_reference)
                        .map_err(|_| invalid())?;
                let scope = hub.installation();
                let StoreOwner::Installation { installation_id } = scope.live.owner() else {
                    return Err(denied());
                };
                let registry = &source.policy().registry;
                if registry.path().parent() != Some(scope.live.owner_root())
                    || !matches!(registry.scope(),HubScope::Global {installation_id:id} if id == installation_id.as_str())
                {
                    return Err(denied());
                }
                hub.recheck().map_err(|_| denied())?;
                let document = registry.load()?;
                let artifact = document
                    .get_version(ArtifactType::Ast, id.as_str(), &version)
                    .ok_or_else(absent)?;
                let mut entries = artifact.installations.iter().filter(|v| {
                    v.target == InstallationTarget::GlobalNative
                        && matches!(
                            v.backend,
                            InstalledBackend::Native | InstalledBackend::RemoteNative(_)
                        )
                });
                let installed = entries.next().ok_or_else(absent)?;
                if entries.next().is_some()
                    || crate::live_agent_catalog::selection(&installed.original_source)
                        != *selection
                {
                    return Err(denied());
                }
                (None, Some((id, version)))
            }
            _ => return Err(invalid()),
        };
        result.push(AstSelection {
            selection,
            project_index,
            imported,
        });
    }
    hub.recheck().map_err(|_| denied())?;
    Ok(result)
}

enum GraphPin {
    Project,
    Local(Arc<CapturedLocalGraph>),
    Remote(Arc<RemoteIcebugGraph>),
}
enum Reader<'a> {
    Project {
        capture: Box<crate::project_graph::PreparedRead>,
        read: &'a ProjectReadAdmission<'a>,
        installation: ScopeCapture<'a>,
    },
    Imported {
        graph: Box<ImportedGraph<'a, 'a>>,
        read: &'a ImportedAstReadAdmission<'a>,
    },
    Remote {
        opened: &'a Opened<'a>,
        lease: &'a ServiceCredentialLease<'a>,
        read: &'a ImportedAstReadAdmission<'a>,
    },
}

pub(crate) struct CapturedAstContext<'a> {
    selected: &'a AstSelection<'a>,
    source: &'a SourceServer,
    server: &'a GraphServer,
    request: &'a RequestContext<'a>,
    original: Arc<SnapshotContext>,
    graph: GraphPin,
    reader: Reader<'a>,
}
impl CapturedAstContext<'_> {
    pub(crate) fn selection(&self) -> &ArtifactSelection {
        self.selected.selection
    }
    pub(crate) fn prepared(&self) -> PreparedSelection<'_> {
        PreparedSelection::ReadContext {
            selection: self.selected.selection,
            original: &self.original,
        }
    }
    pub(crate) fn recheck(&self) -> io::Result<()> {
        self.request.snapshot().check()?;
        match &self.reader {
            Reader::Project { capture, read, .. } => {
                self.server
                    .recheck_project_search(capture, read, self.request)?;
            }
            Reader::Imported { graph, read, .. } => {
                read.recheck().map_err(io::Error::other)?;
                graph.revalidate().map_err(io::Error::other)?;
            }
            Reader::Remote {
                opened,
                lease,
                read,
                ..
            } => {
                read.recheck().map_err(io::Error::other)?;
                opened.check(lease).map_err(io::Error::other)?;
            }
        }
        if let GraphPin::Local(graph) = &self.graph {
            graph.revalidate(&self.original).map_err(io::Error::other)?;
        }
        self.request.snapshot().check()
    }
    fn finish(self) -> io::Result<Self> {
        self.recheck()?;
        Ok(self)
    }
}

pub(crate) async fn capture_project<'a>(
    selected: &'a AstSelection<'a>,
    project: &(ProjectPaths, ProjectId, VersionContext),
    server: &'a GraphServer,
    source: &'a SourceServer,
    read: &'a ProjectReadAdmission<'a>,
    installation: ScopeCapture<'a>,
    request: &'a RequestContext<'a>,
) -> io::Result<CapturedAstContext<'a>> {
    if selected.project_index.is_none()
        || selected.imported.is_some()
        || selected.selection.id != project.1.to_string()
        || selected.selection.version != project.2.version()
        || read.module() != ProjectReadModule::Ast
    {
        return Err(denied());
    }
    let capture = server.capture_project_read(project, read, request)?;
    CapturedAstContext {
        selected,
        source,
        server,
        request,
        original: Arc::clone(request.snapshot()),
        graph: GraphPin::Project,
        reader: Reader::Project {
            capture: Box::new(capture),
            read,
            installation,
        },
    }
    .finish()
}

pub(crate) async fn capture_local_imported<'a>(
    selected: &'a AstSelection<'a>,
    server: &'a GraphServer,
    source: &'a SourceServer,
    read: &'a ImportedAstReadAdmission<'a>,
) -> io::Result<CapturedAstContext<'a>> {
    let (id, version) = selected.imported.as_ref().ok_or_else(invalid)?;
    let policy = source.policy();
    let graph = Box::new(
        open_imported_graph_admitted(
            read,
            ImportedGraphInput {
                registry: &policy.registry,
                id,
                version,
                installation: read.installation(),
                file_policy: &policy.file,
                read_limits: policy.read,
                graph_policy: policy.native_install.graph,
                directory_limits: policy.index,
                request: read.request(),
            },
        )
        .map_err(io::Error::other)?,
    );
    let pin = Arc::clone(graph.reader().map_err(io::Error::other)?);
    CapturedAstContext {
        selected,
        source,
        server,
        request: read.request(),
        original: Arc::clone(read.request().snapshot()),
        graph: GraphPin::Local(pin),
        reader: Reader::Imported { graph, read },
    }
    .finish()
}

pub(crate) async fn capture_remote_imported<'a>(
    selected: &'a AstSelection<'a>,
    server: &'a GraphServer,
    source: &'a SourceServer,
    opened: &'a Opened<'a>,
    lease: &'a ServiceCredentialLease<'a>,
    read: &'a ImportedAstReadAdmission<'a>,
) -> io::Result<CapturedAstContext<'a>> {
    read.recheck().map_err(io::Error::other)?;
    if !Arc::ptr_eq(
        read.request().snapshot(),
        opened.selection.request().snapshot(),
    ) || selected.imported.is_none()
        || opened.selection.source().artifact_type() != ArtifactType::Ast
        || crate::live_agent_catalog::selection(opened.selection.source()) != *selected.selection
    {
        return Err(denied());
    }
    opened.check(lease).map_err(io::Error::other)?;
    let graph = opened
        .mount
        .graph(
            &opened.selection,
            lease,
            source.policy().native_install.graph,
        )
        .await
        .map_err(io::Error::other)?;
    CapturedAstContext {
        selected,
        source,
        server,
        request: opened.selection.request(),
        original: Arc::clone(opened.selection.request().snapshot()),
        graph: GraphPin::Remote(graph),
        reader: Reader::Remote {
            opened,
            lease,
            read,
        },
    }
    .finish()
}
