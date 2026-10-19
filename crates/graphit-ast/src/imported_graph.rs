//! Authenticated reads of the exact installed native AST graph.
use graphit_auth::{
    AuthError, AuthenticatedCaller, ImportedAstReadAdmission, Permission, RoleIssuer, ScopeCapture,
};
use graphit_core::{
    artifact_store::{ScopeContext, SnapshotPolicy, SourceReadLimits, StoreOwner},
    file_io::SnapshotContext,
    module_settings::Module,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
};
use graphit_data::{
    CapturedLocalGraph, DataReadError, GraphGeneration, GraphNeighborhoodLimits,
    GraphNeighborhoodPage, GraphNeighborhoodRequest, GraphSample, GraphSampleLimits, GraphSchema,
    LocalGraphPolicy,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget,
    policy::RequestContext,
    registry::{HubRegistry, HubScope, InstalledBackend, InstalledPayload},
};
use std::{error::Error, fmt, fs, io, path::Path, sync::Arc};

mod entity_source;
pub use entity_source::{
    EntitySourceError, resolve_imported_entity_window, resolve_remote_entity_window,
};

/// Already captured installation and the caller's original operation context.
/// As in imported Source, the Rust composer supplies the issuer's own installation;
/// the incoming transport cannot provide a scope or registry authority.
pub struct ImportedGraphInput<'a, 'r> {
    pub registry: &'r HubRegistry,
    pub id: &'r ArtifactId,
    pub version: &'r ConcreteVersion,
    pub installation: ScopeCapture<'a>,
    pub file_policy: &'r SnapshotPolicy,
    pub read_limits: SourceReadLimits,
    pub graph_policy: LocalGraphPolicy,
    pub directory_limits: NativeDocumentFileLimits,
    pub request: &'a RequestContext<'a>,
}

#[derive(Debug)]
pub enum ImportedGraphError {
    Authentication(AuthError),
    InvalidInstallation,
    InstallationNotFound,
    InvalidOrigin,
    Operation(io::Error),
    Graph(DataReadError),
}
impl fmt::Display for ImportedGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication(e) => write!(f, "installed graph authentication: {e}"),
            Self::InvalidInstallation => f.write_str("invalid installed graph installation"),
            Self::InstallationNotFound => f.write_str("requested native AST graph not installed"),
            Self::InvalidOrigin => f.write_str("invalid native AST graph origin"),
            Self::Operation(e) => write!(f, "installed graph operation: {e}"),
            Self::Graph(e) => write!(f, "installed graph data: {e}"),
        }
    }
}
impl Error for ImportedGraphError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Authentication(e) => Some(e),
            Self::Operation(e) => Some(e),
            Self::Graph(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for ImportedGraphError {
    fn from(e: io::Error) -> Self {
        Self::Operation(e)
    }
}
impl From<DataReadError> for ImportedGraphError {
    fn from(e: DataReadError) -> Self {
        Self::Graph(e)
    }
}

/// A request-scoped reader, never an authority reconstructed from an artifact ID.
/// ```compile_fail
/// fn escape(g: graphit_ast::imported_graph::ImportedGraph<'_, '_>) {
///     let _ = g.graph;
/// }
/// ```
pub struct ImportedGraph<'a, 'r> {
    authority: Authority<'a>,
    input: ImportedGraphInput<'a, 'r>,
    root: CapturedDocumentDirectory,
    directory: CapturedDocumentDirectory,
    graph: Arc<CapturedLocalGraph>,
}

enum Authority<'a> {
    Native(Box<AuthenticatedCaller<'a>>),
    Admitted(&'a ImportedAstReadAdmission<'a>),
}
impl Authority<'_> {
    fn recheck(&self) -> Result<(), ImportedGraphError> {
        match self {
            Self::Native(caller) => caller.authorize(Permission::Read(Module::Ast)).map(|_| ()),
            Self::Admitted(read) => read.recheck(),
        }
        .map_err(ImportedGraphError::Authentication)
    }
}

fn validate_installation(input: &ImportedGraphInput<'_, '_>) -> Result<(), ImportedGraphError> {
    let original = input.request.snapshot();
    validate_installation_scope(input.registry, input.installation, original)
}

fn validate_installation_scope(
    registry: &HubRegistry,
    installation: ScopeCapture<'_>,
    original: &SnapshotContext,
) -> Result<(), ImportedGraphError> {
    original.check()?;
    let StoreOwner::Installation { installation_id } = installation.live.owner() else {
        return Err(ImportedGraphError::InvalidInstallation);
    };
    if !matches!(registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
        || registry.path().parent() != Some(installation.live.owner_root())
    {
        return Err(ImportedGraphError::InvalidInstallation);
    }
    let context = ScopeContext::from_snapshot(original);
    if !installation
        .record
        .matches_scope_bounded(installation.live, &context)?
    {
        return Err(ImportedGraphError::InvalidInstallation);
    }
    installation.live.revalidate_installation(&context)?;
    original.check()?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct InstalledAstContext {
    pub id: String,
    pub version: String,
    pub context: String,
    pub backend: &'static str,
}

/// Observe the real native AST installations. Captures grant no authorization;
/// the transport retains and rechecks its installation ReadAst admission.
pub fn installed_ast_contexts(
    registry: &HubRegistry,
    installation: ScopeCapture<'_>,
    original: &SnapshotContext,
) -> Result<Vec<InstalledAstContext>, ImportedGraphError> {
    validate_installation_scope(registry, installation, original)?;
    let document = registry.load()?;
    let mut contexts = Vec::new();
    for (kind, _, artifact) in document.entries() {
        original.check()?;
        if kind != ArtifactType::Ast {
            continue;
        }
        for installed in &artifact.installations {
            if installed.target != InstallationTarget::GlobalNative
                || !matches!(
                    installed.backend,
                    InstalledBackend::Native | InstalledBackend::RemoteNative(_)
                )
                || !matches!(installed.payload, InstalledPayload::Original { .. })
            {
                continue;
            }
            let id = artifact.id.as_str().to_owned();
            let version = installed.original_source.version().as_str().to_owned();
            contexts.push(InstalledAstContext {
                context: format!("{id}@{version}"),
                id,
                version,
                backend: if matches!(installed.backend, InstalledBackend::RemoteNative(_)) {
                    "remote_native"
                } else {
                    "native"
                },
            });
        }
    }
    validate_installation_scope(registry, installation, original)?;
    Ok(contexts)
}

fn observe_directory(path: &Path) -> Result<(), ImportedGraphError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ImportedGraphError::InvalidOrigin);
    }
    Ok(())
}
fn capture_directory(
    path: &Path,
    input: &ImportedGraphInput<'_, '_>,
) -> Result<CapturedDocumentDirectory, ImportedGraphError> {
    input.request.snapshot().check()?;
    observe_directory(path)?;
    let result =
        CapturedDocumentDirectory::capture(path, input.directory_limits, input.request.snapshot())?;
    if result.canonical_path() != path {
        return Err(ImportedGraphError::InvalidOrigin);
    }
    Ok(result)
}
fn observe_schema(path: &Path) -> Result<(), ImportedGraphError> {
    let metadata = fs::symlink_metadata(path.join("schema.cypher"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ImportedGraphError::InvalidOrigin);
    }
    Ok(())
}

impl<'a, 'r> ImportedGraph<'a, 'r> {
    /// Run on the caller's existing blocking job while its runtime remains alive.
    pub fn query(
        &self,
        runtime: &tokio::runtime::Handle,
        query: &str,
        parameters: &std::collections::BTreeMap<String, serde_json::Value>,
        options: crate::icebug_query::QueryOptions,
    ) -> Result<crate::icebug_query::QueryResult, ImportedGraphError> {
        self.revalidate()?;
        let result = crate::icebug_query::execute(
            &self.graph,
            self.input.request.snapshot(),
            runtime,
            query,
            parameters,
            options,
        );
        self.revalidate()?;
        result.map_err(Into::into)
    }
    pub fn revalidate(&self) -> Result<(), ImportedGraphError> {
        self.authority.recheck()?;
        validate_installation(&self.input)?;
        let original = self.input.request.snapshot();
        observe_directory(self.root.canonical_path())?;
        self.root.revalidate(original)?;
        observe_directory(self.directory.canonical_path())?;
        self.directory.revalidate(original)?;
        observe_schema(self.directory.canonical_path())?;
        original.check()?;
        Ok(())
    }
    /// Borrow the actual captured reader for the caller's existing owned Data job.
    /// Retain this owner and its admission, and revalidate before delivery.
    pub fn reader(&self) -> Result<&Arc<CapturedLocalGraph>, ImportedGraphError> {
        self.revalidate()?;
        Ok(&self.graph)
    }
    pub fn generation(&self) -> Result<GraphGeneration, ImportedGraphError> {
        self.revalidate()?;
        self.graph.schema()?;
        self.revalidate()?;
        Ok(self.graph.generation())
    }
    pub fn schema(&self) -> Result<GraphSchema<'_>, ImportedGraphError> {
        self.revalidate()?;
        let result = self.graph.schema()?;
        self.revalidate()?;
        Ok(result)
    }
    pub fn sample_graph(
        &self,
        limits: GraphSampleLimits,
    ) -> Result<GraphSample, ImportedGraphError> {
        self.revalidate()?;
        let result = self.graph.sample_graph(limits);
        self.revalidate()?;
        result.map_err(Into::into)
    }
    pub fn neighborhood_graph(
        &self,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
    ) -> Result<GraphNeighborhoodPage, ImportedGraphError> {
        self.revalidate()?;
        let result = self.graph.neighborhood_graph(request, limits);
        self.revalidate()?;
        result.map_err(Into::into)
    }
}

fn open_authorized<'a, 'r>(
    authority: Authority<'a>,
    input: ImportedGraphInput<'a, 'r>,
) -> Result<ImportedGraph<'a, 'r>, ImportedGraphError> {
    authority.recheck()?;
    validate_installation(&input)?;
    let document = input.registry.load()?;
    validate_installation(&input)?;
    let source = document
        .get_version(ArtifactType::Ast, input.id.as_str(), input.version)
        .and_then(|artifact| {
            artifact.installations.iter().find(|installation| {
                installation.target == InstallationTarget::GlobalNative
                    && installation.backend == InstalledBackend::Native
                    && matches!(installation.payload, InstalledPayload::Original { .. })
                    && installation.original_source.version() == input.version
            })
        })
        .ok_or(ImportedGraphError::InstallationNotFound)?
        .original_source
        .clone();
    let root = capture_directory(source.directory().as_path(), &input)?;
    let directory = capture_directory(&root.canonical_path().join("graph.icebug"), &input)?;
    observe_schema(directory.canonical_path())?;
    let graph = CapturedLocalGraph::capture(
        directory.canonical_path(),
        input.file_policy,
        input.read_limits,
        Arc::clone(input.request.snapshot()),
        input.graph_policy,
    )?;
    let result = ImportedGraph {
        authority,
        input,
        root,
        directory,
        graph: Arc::new(graph),
    };
    result.revalidate()?;
    Ok(result)
}

pub fn open_imported_graph_runtime<'a, 'r>(
    issuer: &'a RoleIssuer,
    raw: &str,
    input: ImportedGraphInput<'a, 'r>,
) -> Result<ImportedGraph<'a, 'r>, ImportedGraphError> {
    let caller = issuer
        .authenticate_runtime(raw, input.request)
        .map_err(ImportedGraphError::Authentication)?;
    open_authorized(Authority::Native(Box::new(caller)), input)
}
pub fn open_imported_graph_local<'a, 'r>(
    issuer: &'a RoleIssuer,
    raw: &str,
    input: ImportedGraphInput<'a, 'r>,
) -> Result<ImportedGraph<'a, 'r>, ImportedGraphError> {
    let caller = issuer
        .authenticate_local(raw, input.request)
        .map_err(ImportedGraphError::Authentication)?;
    open_authorized(Authority::Native(Box::new(caller)), input)
}

/// Open an exact installed AST using its separate, verified Native or Web admission.
pub fn open_imported_graph_admitted<'a, 'r>(
    read: &'a ImportedAstReadAdmission<'a>,
    input: ImportedGraphInput<'a, 'r>,
) -> Result<ImportedGraph<'a, 'r>, ImportedGraphError> {
    if !Arc::ptr_eq(read.request().snapshot(), input.request.snapshot())
        || !std::ptr::eq(read.installation().live, input.installation.live)
        || !std::ptr::eq(read.installation().record, input.installation.record)
    {
        return Err(ImportedGraphError::Authentication(AuthError::Denied));
    }
    open_authorized(Authority::Admitted(read), input)
}
