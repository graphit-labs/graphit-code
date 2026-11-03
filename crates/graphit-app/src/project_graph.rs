//! Native PROJECT graph location and authenticated read composition.
use graphit_core::{project_paths::ProjectPaths, version_context::VersionContext};
use std::io;

use crate::{live_project_selections::LiveProjectSelections, source_server::SourcePolicy};
use graphit_auth::{
    AuthError, Permission, ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord},
    config::ConfigSnapshot,
    module_settings::{Module, ModuleSettings},
    project_identity::ProjectId,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_data::DataReadError;
#[cfg(test)]
use graphit_data::{CapturedLocalGraph, LocalGraphPolicy};
use graphit_hub::policy::RequestContext;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) enum ReadError {
    Authentication(AuthError),
    Operation(io::Error),
    Graph(DataReadError),
    Source(graphit_ast::source_handler::SourceHandlerError),
    Entity(graphit_ast::imported_graph::EntitySourceError),
    Inactive,
    Disabled,
    Absent,
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Authentication(_) => "PROJECT AST authentication failed",
            Self::Operation(_) => "PROJECT AST operation failed",
            Self::Graph(_) => "PROJECT AST graph read failed",
            Self::Source(_) => "PROJECT AST Source read failed",
            Self::Entity(_) => "PROJECT AST entity Source failed",
            Self::Inactive => "PROJECT selection inactive",
            Self::Disabled => "PROJECT AST disabled",
            Self::Absent => "PROJECT AST publication absent",
        })
    }
}
impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authentication(error) => Some(error),
            Self::Operation(error) => Some(error),
            Self::Graph(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::Entity(error) => Some(error),
            Self::Inactive | Self::Disabled | Self::Absent => None,
        }
    }
}
impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}
impl From<DataReadError> for ReadError {
    fn from(error: DataReadError) -> Self {
        Self::Graph(error)
    }
}

// Validation of native storage describes its captured origin, not Cypher input.
// Keep cancellation/deadline and access failures in their existing terminal path.
fn native_capture_error(error: io::Error) -> ReadError {
    if error.kind() == io::ErrorKind::InvalidData {
        ReadError::Graph(DataReadError::Source(error))
    } else {
        ReadError::Operation(error)
    }
}

pub(crate) struct ReadInput<'a> {
    pub auth: &'a DaemonAuth,
    pub policy: &'a SourcePolicy,
    pub base: &'a ConfigSnapshot,
    pub live: &'a LiveProjectSelections,
    pub selected: (&'a ProjectPaths, ProjectId, &'a VersionContext),
    pub credential: &'a str,
    pub local: bool,
    pub request: &'a RequestContext<'a>,
}

/// Exercise the same explicit portable export used by CLI/Hub while retaining
/// its staging owner and the caller's PROJECT admission through the assertion.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn with_exported_graph<T>(
    server: &crate::graph_server::GraphServer,
    input: ReadInput<'_>,
    operation: impl FnOnce(&Arc<CapturedLocalGraph>, LocalGraphPolicy) -> T,
) -> Result<T, ReadError> {
    let policy = input.policy;
    let base = input.base;
    let live = input.live;
    let paths = input.selected.0;
    let original = Arc::clone(input.request.snapshot());
    let config =
        crate::ast_profile_loader::project_config(&policy.installation, paths, base, &original)?;
    let graph_policy = LocalGraphPolicy::resolve(&config, &original)?;
    with_read(input, |capture| {
        let (_output, graph) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(server.capture_current_export(capture, paths, base, live, &original))
        })?;
        let result = operation(&graph, graph_policy);
        capture.check(policy, base, live)?;
        graph.schema()?;
        original.check()?;
        Ok(result)
    })?
}

/// Authenticate once and retain the same PROJECT admission through the native
/// transaction. A locator never grants access.
pub(crate) fn with_read<T>(
    input: ReadInput<'_>,
    operation: impl FnOnce(&PreparedRead) -> T,
) -> Result<T, ReadError> {
    let original = input.request.snapshot();
    original.check()?;
    let caller = if input.local {
        input
            .auth
            .issuer()
            .authenticate_local(input.credential, input.request)
    } else {
        input
            .auth
            .issuer()
            .authenticate_runtime(input.credential, input.request)
    }
    .map_err(ReadError::Authentication)?;
    caller
        .authorize(Permission::Read(Module::Ast))
        .map_err(ReadError::Authentication)?;
    let capture = PreparedRead::capture(
        input.policy,
        input.live,
        input.selected,
        Arc::clone(original),
    )?;
    capture.check(input.policy, input.base, input.live)?;
    let scope = ScopeInput {
        installation: input.auth.installation(),
        project: Some(capture.project()),
        providers: &[],
        binding: input.policy.binding,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(input.auth.issuer());
    let admission = authority
        .admit_authenticated_read(&caller, input.request, scope, ProjectReadModule::Ast)
        .map_err(ReadError::Authentication)?;
    admission.recheck().map_err(ReadError::Authentication)?;
    let result = operation(&capture);
    capture.check(input.policy, input.base, input.live)?;
    admission.recheck().map_err(ReadError::Authentication)?;
    original.check()?;
    Ok(result)
}

/// One actual physical PROJECT capture. Authentication is retained by the caller;
/// this owner can be shared with a blocking Data job without moving that admission.
pub(crate) struct PreparedRead {
    scope: CapturedPublicationScope,
    record: ScopeBindingRecord,
    paths: ProjectPaths,
    id: ProjectId,
    version: VersionContext,
    original: Arc<graphit_core::file_io::SnapshotContext>,
    live_capture: std::sync::OnceLock<LiveCapture>,
}
struct LiveCapture {
    index: Arc<crate::foreground_ast_sources::progressive::LiveIndex>,
    directory: graphit_core::native_document_file::CapturedDocumentDirectory,
}
impl PreparedRead {
    pub(crate) fn original(&self) -> &Arc<graphit_core::file_io::SnapshotContext> {
        &self.original
    }
    pub(crate) fn capture(
        policy: &SourcePolicy,
        live: &LiveProjectSelections,
        selected: (&ProjectPaths, ProjectId, &VersionContext),
        original: Arc<graphit_core::file_io::SnapshotContext>,
    ) -> Result<Self, ReadError> {
        original.check()?;
        let (paths, id, version) = selected;
        if !live.is_active(paths, id, version)? {
            return Err(ReadError::Inactive);
        }
        let scope = CapturedPublicationScope::project(paths, id, policy.scope, &original)?;
        let record = scope.binding()?;
        Ok(Self {
            scope,
            record,
            paths: paths.clone(),
            id,
            version: version.clone(),
            original,
            live_capture: std::sync::OnceLock::new(),
        })
    }
    pub(crate) fn project(&self) -> ProjectCapture<'_> {
        ProjectCapture {
            scope: ScopeCapture {
                live: &self.scope,
                record: &self.record,
            },
            version: &self.version,
        }
    }
    pub(crate) fn check_active(&self, live: &LiveProjectSelections) -> Result<(), ReadError> {
        self.original.check()?;
        if !live.is_active(&self.paths, self.id, &self.version)? {
            return Err(ReadError::Inactive);
        }
        Ok(())
    }
    pub(crate) fn check(
        &self,
        policy: &SourcePolicy,
        base: &ConfigSnapshot,
        live: &LiveProjectSelections,
    ) -> Result<(), ReadError> {
        self.check_active(live)?;
        if let Some(capture) = self.live_capture.get() {
            capture
                .directory
                .revalidate(&self.original)
                .map_err(native_capture_error)?;
        }
        let config = crate::ast_profile_loader::project_config(
            &policy.installation,
            &self.paths,
            base,
            &self.original,
        )?;
        if !ModuleSettings::resolve(&config)?.enabled(Module::Ast) {
            return Err(ReadError::Disabled);
        }
        Ok(())
    }
    pub(crate) async fn open_live(
        &self,
        installation: ScopeCapture<'_>,
        registry: &crate::foreground_ast_sources::progressive::LiveAstRegistry,
        policy: &SourcePolicy,
        base: &ConfigSnapshot,
        live: &LiveProjectSelections,
    ) -> Result<bool, ReadError> {
        self.check(policy, base, live)?;
        if let Some(capture) = self.live_capture.get() {
            capture
                .directory
                .revalidate(&self.original)
                .map_err(native_capture_error)?;
            return Ok(true);
        }
        let Some((index, directory)) = registry
            .capture(
                installation,
                self.project(),
                policy.index,
                &self.original,
                false,
            )
            .await
            .map_err(native_capture_error)?
        else {
            return Ok(false);
        };
        directory
            .revalidate(&self.original)
            .map_err(native_capture_error)?;
        self.live_capture
            .set(LiveCapture { index, directory })
            .map_err(|_| io::Error::from(io::ErrorKind::AlreadyExists))?;
        self.check(policy, base, live)?;
        Ok(true)
    }
    pub(crate) fn live_index(
        &self,
    ) -> Option<&Arc<crate::foreground_ast_sources::progressive::LiveIndex>> {
        self.live_capture.get().map(|capture| &capture.index)
    }
    pub(crate) async fn inspect_source(
        &self,
        policy: &SourcePolicy,
        base: &ConfigSnapshot,
        live: &LiveProjectSelections,
        installation: ScopeCapture<'_>,
        registry: &crate::foreground_ast_sources::progressive::LiveAstRegistry,
        source: &crate::ast_source::SourceParameters,
    ) -> Result<graphit_ast::source_inspection::SourceInspection, ReadError> {
        self.check_active(live)?;
        source.validate().map_err(|message| {
            ReadError::Operation(io::Error::new(io::ErrorKind::InvalidInput, message))
        })?;
        if source.entity.is_some() {
            self.open_live(installation, registry, policy, base, live)
                .await?;
        }
        let entity = match source.entity.as_deref() {
            Some(name) => {
                let live = self.live_index().ok_or(ReadError::Absent)?;
                Some(
                    graphit_ast::project_entity_source::resolve_live_project_entity_source(
                        &live.graph,
                        &self.original,
                        &source.path,
                        name,
                        source.entity_type.as_deref(),
                        policy.graph_query,
                    )
                    .map_err(ReadError::Entity)?,
                )
            }
            None => None,
        };
        let selection = source.selection();
        let result = graphit_ast::source_handler::inspect_captured_source(
            graphit_ast::source_handler::CapturedSourceInput {
                scope: ScopeInput {
                    installation,
                    project: Some(self.project()),
                    providers: &[],
                    binding: policy.binding,
                    global_version: "",
                },
                requested_relative: std::path::Path::new(&source.path),
                selection: &selection,
                file_policy: &policy.file,
                read_limits: policy.read,
                path_limits: policy.path,
                index_limits: policy.index,
                original: &self.original,
                entity: entity.as_ref(),
            },
        )
        .await;
        self.check_active(live)?;
        if source.entity.is_some() {
            self.check(policy, base, live)?;
        }
        self.original.check()?;
        result.map_err(ReadError::Source)
    }
}
