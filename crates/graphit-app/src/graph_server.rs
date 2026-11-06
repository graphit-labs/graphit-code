//! Native graph routes over the daemon's existing authenticated installation.
use crate::source_server::SourcePolicy;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use graphit_ast::imported_graph::{
    ImportedGraph, ImportedGraphError, ImportedGraphInput, open_imported_graph_local,
    open_imported_graph_runtime,
};
use graphit_core::{file_lock::Cancellation, tcp_listener::BoundListener};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_data::{
    GraphAnchorKey, GraphDirection, GraphNeighborhoodCursor, GraphNeighborhoodLimits,
    GraphNeighborhoodRequest, GraphPropertyValueRef, GraphSampleLimits,
};
use graphit_hub::{ArtifactId, ConcreteVersion, policy::RequestContext};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io, sync::Arc, time::Instant};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod browser;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod export;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod imports;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod live;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod native_search;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod remote;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod search;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) use browser::BrowserOperation;

#[cfg(test)]
type NativeGate = (
    std::sync::mpsc::Sender<Cancellation>,
    std::sync::mpsc::Receiver<()>,
);

struct Shared {
    live_ast: Arc<crate::foreground_ast_sources::progressive::LiveAstRegistry>,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    native_http: reqwest::Client,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    native_allow_loopback_http: bool,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    native_verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    project: Option<ProjectAccess>,
    auth: Arc<DaemonAuth>,
    policy: SourcePolicy,
    stop: CancellationToken,
    jobs: Mutex<JoinSet<()>>,
    #[cfg(test)]
    gate: std::sync::Mutex<Option<NativeGate>>,
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
struct ProjectAccess {
    config: graphit_core::config::ConfigSnapshot,
    live: Arc<crate::live_project_selections::LiveProjectSelections>,
}
#[derive(Clone)]
pub struct GraphServer(Arc<Shared>);
impl GraphServer {
    pub(crate) fn live_ast(
        &self,
    ) -> Arc<crate::foreground_ast_sources::progressive::LiveAstRegistry> {
        Arc::clone(&self.0.live_ast)
    }
    /// Retain genuine PROJECT authority even when the published graph is absent.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn capture_project_read(
        &self,
        selected: &(
            graphit_core::project_paths::ProjectPaths,
            graphit_core::project_identity::ProjectId,
            graphit_core::version_context::VersionContext,
        ),
        read: &graphit_auth::ProjectReadAdmission<'_>,
        original: &graphit_hub::policy::RequestContext<'_>,
    ) -> io::Result<crate::project_graph::PreparedRead> {
        read.recheck().map_err(io::Error::other)?;
        let project = read.project().map_err(io::Error::other)?;
        if read.module() != graphit_auth::ProjectReadModule::Ast
            || !Arc::ptr_eq(
                read.request().map_err(io::Error::other)?.snapshot(),
                original.snapshot(),
            )
            || project.scope.live.owner_root() != selected.0.root()
            || !matches!(project.scope.live.owner(),graphit_core::artifact_store::StoreOwner::Project{project_id} if *project_id==selected.1)
            || project.version != &selected.2
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let access = self.0.project.as_ref().ok_or(io::ErrorKind::NotConnected)?;
        let capture = crate::project_graph::PreparedRead::capture(
            &self.0.policy,
            &access.live,
            (&selected.0, selected.1, &selected.2),
            Arc::clone(original.snapshot()),
        )
        .map_err(io::Error::other)?;
        self.recheck_project_search(&capture, read, original)?;
        Ok(capture)
    }
    /// Search captures only its Lance corpus; Source and graph publication may advance independently.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) async fn open_project_search(
        &self,
        selected: &(
            graphit_core::project_paths::ProjectPaths,
            graphit_core::project_identity::ProjectId,
            graphit_core::version_context::VersionContext,
        ),
        read: &graphit_auth::ProjectReadAdmission<'_>,
        original: &graphit_hub::policy::RequestContext<'_>,
    ) -> io::Result<
        Option<(
            crate::project_graph::PreparedRead,
            graphit_ast::entity_search::EntitySearch,
        )>,
    > {
        use graphit_ast::native_nn_index_locator::{
            NativeNnIndexPresence, open_local_native_nn_index,
        };
        let capture = self.capture_project_read(selected, read, original)?;
        let corpus = match open_local_native_nn_index(
            self.0.auth.installation(),
            capture.project(),
            original.snapshot(),
            self.0.policy.index,
        )
        .await
        .map_err(io::Error::other)?
        {
            NativeNnIndexPresence::Absent => return Ok(None),
            NativeNnIndexPresence::Available(corpus) => corpus,
        };
        let snapshot = corpus
            .snapshot(original.snapshot())
            .map_err(io::Error::other)?
            .clone();
        let search = graphit_ast::entity_search::EntitySearch::from_corpus(
            snapshot,
            Arc::clone(original.snapshot()),
        )?;
        self.recheck_project_search(&capture, read, original)?;
        Ok(Some((capture, search)))
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn recheck_project_search(
        &self,
        capture: &crate::project_graph::PreparedRead,
        read: &graphit_auth::ProjectReadAdmission<'_>,
        original: &graphit_hub::policy::RequestContext<'_>,
    ) -> io::Result<()> {
        read.recheck().map_err(io::Error::other)?;
        let access = self.0.project.as_ref().ok_or(io::ErrorKind::NotConnected)?;
        capture
            .check(&self.0.policy, &access.config, &access.live)
            .map_err(io::Error::other)?;
        original.check()?;
        if self.0.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) async fn inspect_source_native<'a>(
        &'a self,
        selected: (
            &graphit_core::project_paths::ProjectPaths,
            graphit_core::project_identity::ProjectId,
            &graphit_core::version_context::VersionContext,
        ),
        source: &crate::ast_source::SourceParameters,
        credential: &str,
        local: bool,
        request: &'a RequestContext<'a>,
    ) -> Result<graphit_ast::source_inspection::SourceInspection, crate::project_graph::ReadError>
    {
        use crate::project_graph::{PreparedRead, ReadError};
        use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ScopeInput};
        let state = &self.0;
        let access = state.project.as_ref().ok_or(ReadError::Inactive)?;
        request.snapshot().check()?;
        let caller = if local {
            state.auth.issuer().authenticate_local(credential, request)
        } else {
            state
                .auth
                .issuer()
                .authenticate_runtime(credential, request)
        }
        .map_err(ReadError::Authentication)?;
        caller
            .authorize(graphit_auth::Permission::Read(
                graphit_core::module_settings::Module::Ast,
            ))
            .map_err(ReadError::Authentication)?;
        let capture = PreparedRead::capture(
            &state.policy,
            &access.live,
            selected,
            Arc::clone(request.snapshot()),
        )?;
        if source.entity.is_some() {
            capture.check(&state.policy, &access.config, &access.live)?;
        }
        let authority = ProjectRecordAuthority::new(state.auth.issuer());
        let admission = authority
            .admit_authenticated_read(
                &caller,
                request,
                ScopeInput {
                    installation: state.auth.installation(),
                    project: Some(capture.project()),
                    providers: &[],
                    binding: state.policy.binding,
                    global_version: "",
                },
                ProjectReadModule::Ast,
            )
            .map_err(ReadError::Authentication)?;
        admission.recheck().map_err(ReadError::Authentication)?;
        let result = capture
            .inspect_source(
                &state.policy,
                &access.config,
                &access.live,
                state.auth.installation(),
                &state.live_ast,
                source,
            )
            .await;
        admission.recheck().map_err(ReadError::Authentication)?;
        if source.entity.is_some() {
            capture.check(&state.policy, &access.config, &access.live)?;
        } else {
            capture.check_active(&access.live)?;
        }
        request.snapshot().check()?;
        result
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        if name != "graphit_ast_imports" && name != export::TOOL {
            self.0.project.as_ref()?;
        }
        Self::tool_definitions()
            .iter()
            .find(|tool| tool.name == name)
            .cloned()
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn tools(&self) -> Vec<rmcp::model::Tool> {
        Self::tool_definitions()
            .iter()
            .filter(|tool| {
                self.0.project.is_some()
                    || tool.name == "graphit_ast_imports"
                    || tool.name == export::TOOL
            })
            .cloned()
            .collect()
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    fn tool_definitions() -> &'static [rmcp::model::Tool] {
        static TOOLS: std::sync::OnceLock<Vec<rmcp::model::Tool>> = std::sync::OnceLock::new();
        TOOLS.get_or_init(|| {
            use serde_json::json;
            [
                (
                    export::TOOL,
                    "Export an admitted immutable AST graph to a native package file.",
                ),
                (
                    "graphit_ast_imports",
                    "List exact installed Native AST contexts.",
                ),
                (
                    "graphit_ast_query",
                    "Execute readonly Cypher over the selected PROJECT or exact installed context.",
                ),
                ("graphit_ast_schema", "Read the selected graph schema."),
                (
                    "graphit_ast_sample",
                    "Sample the selected graph nodes and edges.",
                ),
                (
                    "graphit_ast_neighborhood",
                    "Read a cursor page around an exact graph anchor.",
                ),
            ]
            .into_iter()
            .map(|(name, description)| {
                let mut properties = serde_json::Map::from_iter([
                    ("project_dir".into(), json!({"type":"string"})),
                    ("project_version".into(), json!({"type":"string"})),
                    ("context".into(), json!({"type":"string"})),
                ]);
                if name == "graphit_ast_imports" {
                    properties.clear();
                }
                let mut required = Vec::new();
                if name == export::TOOL {
                    properties.insert("output".into(), json!({"type":"string"}));
                    required.push("output");
                }
                if name == "graphit_ast_query" {
                    properties.insert("query".into(), json!({"type":"string"}));
                    properties.insert("parameters".into(), json!({"type":"object"}));
                    required.push("query");
                }
                if name == "graphit_ast_neighborhood" {
                    for key in ["anchor_label", "relationship", "other_label"] {
                        properties.insert(key.into(), json!({"type":"string"}));
                        required.push(key);
                    }
                    properties.insert("anchor_key".into(), json!({"type":"object"}));
                    properties.insert("direction".into(), json!({"enum":["outgoing","incoming"]}));
                    properties.insert(
                        "limit".into(),
                        json!({"type":"integer","minimum":1,"maximum":1000}),
                    );
                    properties.insert("cursor".into(), json!({"type":"object"}));
                    required.extend(["anchor_key", "direction", "limit"]);
                }
                rmcp::model::Tool::new(
                    name,
                    description,
                    json!({"type":"object",
                    "properties":properties,"required":required,"additionalProperties":false})
                    .as_object()
                    .expect("object schema")
                    .clone(),
                )
            })
            .collect()
        })
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) async fn call_mcp(
        &self,
        call: rmcp::model::CallToolRequestParams,
        headers: HeaderMap,
        cancelled: CancellationToken,
    ) -> rmcp::model::CallToolResult {
        use rmcp::model::{CallToolResult, ContentBlock};
        use serde_json::{Value, json};
        let failure = |message: &str| CallToolResult::error(vec![ContentBlock::text(message)]);
        if call.name == export::TOOL {
            return self.call_export_mcp(call, headers, cancelled).await;
        }
        let operation = match call.name.as_ref() {
            "graphit_ast_imports" => Operation::Imports,
            "graphit_ast_query" => Operation::Query,
            "graphit_ast_schema" => Operation::Schema,
            "graphit_ast_sample" => Operation::Sample,
            "graphit_ast_neighborhood" => Operation::Neighborhood,
            _ => return failure("unknown graph tool"),
        };
        let mut arguments = call.arguments.unwrap_or_default();
        let context = arguments.remove("context");
        let project_dir = arguments.remove("project_dir");
        let project_version = arguments.remove("project_version");
        let (target, body) = if matches!(operation, Operation::Imports) {
            if context.is_some()
                || project_dir.is_some()
                || project_version.is_some()
                || !arguments.is_empty()
            {
                return failure("imports arguments invalid");
            }
            (Target::Installed, json!({}))
        } else if let Some(context) = context {
            if project_dir.is_some() || project_version.is_some() {
                return failure("graph selection ambiguous");
            }
            let Some((id, version)) = context.as_str().and_then(|value| value.rsplit_once('@'))
            else {
                return failure("installed graph context must be id@version");
            };
            arguments.insert("id".into(), Value::String(id.to_owned()));
            arguments.insert("version".into(), Value::String(version.to_owned()));
            (Target::Installed, Value::Object(arguments))
        } else {
            let mut selection = serde_json::Map::new();
            if let Some(value) = project_dir {
                selection.insert("project_dir".into(), value);
            }
            if let Some(value) = project_version {
                selection.insert("project_version".into(), value);
            }
            let mut body = json!({"selection":selection});
            match operation {
                Operation::Query => body["query"] = Value::Object(arguments),
                Operation::Neighborhood => body["neighborhood"] = Value::Object(arguments),
                Operation::Schema | Operation::Sample if arguments.is_empty() => {}
                _ => return failure("graph arguments invalid"),
            }
            (Target::Project, body)
        };
        let Ok(bytes) = serde_json::to_vec(&body) else {
            return failure("graph arguments invalid");
        };
        if bytes.len() > self.0.policy.content.max_input_bytes {
            return failure("graph input too large");
        }
        let response = tokio::select! {
            response = self.read_target(headers, Bytes::from(bytes), operation, target) => response,
            () = cancelled.cancelled() => return failure("graph operation cancelled"),
        };
        let status = response.status();
        let Ok(bytes) =
            axum::body::to_bytes(response.into_body(), self.0.policy.content.max_output_bytes)
                .await
        else {
            return failure("graph response unavailable");
        };
        if !status.is_success() {
            return crate::mcp_output::typed_error(
                json!({"status":"error","http_status":status.as_u16(),
                "message":String::from_utf8_lossy(&bytes)}),
            );
        }
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => crate::mcp_output::typed(value),
            Err(_) => failure("graph response invalid"),
        }
    }
    pub fn new(
        auth: Arc<DaemonAuth>,
        policy: SourcePolicy,
        stop: CancellationToken,
    ) -> io::Result<Self> {
        let cancel = Cancellation::default();
        let deadline = Instant::now()
            .checked_add(policy.timeout)
            .filter(|_| !policy.timeout.is_zero())
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        RequestContext::new(policy.content, &cancel, deadline).map_err(io::Error::other)?;
        let query = policy.graph_query;
        if (query.buffer_pool_bytes != 0 && query.buffer_pool_bytes < 4096)
            || query.query_threads == 0
            || query.max_rows == 0
            || query.max_bytes == 0
            || query.max_depth == 0
            || query.max_rows > policy.content.max_collection_entries
            || query.max_bytes > policy.content.max_output_bytes
            || query.max_depth > policy.content.max_depth
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid graph query options",
            ));
        }
        if std::fs::canonicalize(policy.installation.root())?
            != auth.installation().live.owner_root()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "graph installation mismatch",
            ));
        }
        Ok(Self(Arc::new(Shared {
            live_ast: Arc::default(),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            native_http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .map_err(io::Error::other)?,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            native_allow_loopback_http: false,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            native_verifiers: None,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            project: None,
            auth,
            policy,
            stop,
            jobs: Mutex::new(JoinSet::new()),
            #[cfg(test)]
            gate: std::sync::Mutex::new(None),
        })))
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn attach_native_verifiers(
        &mut self,
        verifiers: Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("graph verifiers must attach before serving"))?;
        if state.native_verifiers.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        state.native_verifiers = Some(verifiers);
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn project_config(&self) -> io::Result<&graphit_core::config::ConfigSnapshot> {
        self.0
            .project
            .as_ref()
            .map(|project| &project.config)
            .ok_or_else(|| io::ErrorKind::NotConnected.into())
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn native_http(&self) -> reqwest::Client {
        self.0.native_http.clone()
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn attach_projects(
        &mut self,
        config: graphit_core::config::ConfigSnapshot,
        live: Arc<crate::live_project_selections::LiveProjectSelections>,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("graph context must attach before serving"))?;
        state.native_allow_loopback_http = config
            .get::<bool>(&graphit_core::config::ConfigKey::new(
                "broker.allow_loopback_http",
            )?)?
            .unwrap_or(false);
        state.project = Some(ProjectAccess { config, live });
        Ok(())
    }
    pub fn router(&self) -> Router {
        let router = Router::new()
            .route("/api/graphs/imports", post(imports))
            .route("/api/graphs/query", post(query))
            .route("/api/graphs/schema", post(schema))
            .route("/api/graphs/sample", post(sample))
            .route("/api/graphs/neighborhood", post(neighborhood));
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        let router = if self.0.project.is_some() {
            router
                .route("/api/graphs/project/query", post(project_query))
                .route("/api/graphs/project/schema", post(project_schema))
                .route("/api/graphs/project/sample", post(project_sample))
                .route(
                    "/api/graphs/project/neighborhood",
                    post(project_neighborhood),
                )
        } else {
            router
        };
        router
            .layer(DefaultBodyLimit::max(self.0.policy.content.max_input_bytes))
            .with_state(self.clone())
    }
    /// Keep native jobs owned even if the shutdown waiter is cancelled.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        let mut failed = false;
        while let Some(result) = jobs.join_next().await {
            failed |= result.is_err();
        }
        if failed {
            Err(io::Error::other("graph operation failed to join"))
        } else {
            Ok(())
        }
    }
    pub async fn serve(&self, listener: BoundListener) -> io::Result<()> {
        if !listener.address().ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "graph listener must be loopback",
            ));
        }
        let listener = listener.into_listener();
        listener.set_nonblocking(true)?;
        let result = axum::serve(tokio::net::TcpListener::from_std(listener)?, self.router())
            .with_graceful_shutdown(self.0.stop.clone().cancelled_owned())
            .await;
        result.and(self.shutdown().await)
    }
    async fn read(&self, headers: HeaderMap, body: Bytes, operation: Operation) -> Response {
        self.read_target(headers, body, operation, Target::Installed)
            .await
    }
    async fn read_target(
        &self,
        headers: HeaderMap,
        body: Bytes,
        operation: Operation,
        target: Target,
    ) -> Response {
        let cancel = Cancellation::default();
        let guard = CancelOnDrop(cancel.clone());
        let deadline = Instant::now() + self.0.policy.timeout;
        let credentials = match credential(&headers) {
            Ok(v) => v,
            Err(status) => return status.into_response(),
        };
        let state = Arc::clone(&self.0);
        let original_cancel = cancel.clone();
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = tokio::select! {
                jobs = state.jobs.lock() => jobs,
                () = state.stop.cancelled() => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
                () = tokio::time::sleep_until(deadline.into()) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
            };
            while jobs.try_join_next().is_some() {}
            if state.stop.is_cancelled() {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            let owner = Arc::clone(&state);
            jobs.spawn_blocking(move || {
                #[cfg(test)]
                {
                    let gate = owner.gate.lock().unwrap().take();
                    if let Some((started, release)) = gate {
                        started.send(original_cancel.clone()).unwrap();
                        release.recv().unwrap();
                    }
                }
                let result = match target {
                    Target::Installed => read(
                        &owner,
                        &credentials.0,
                        credentials.1,
                        &body,
                        operation,
                        &original_cancel,
                        deadline,
                    ),
                    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                    Target::Project => read_project(
                        &owner,
                        &credentials.0,
                        credentials.1,
                        &body,
                        operation,
                        &original_cancel,
                        deadline,
                    ),
                };
                let _ = send.send(result);
            });
        }
        let result = tokio::select! {
            value = receive => match value {
                Ok(Ok(bytes)) if !cancel.is_cancelled() && !state.stop.is_cancelled() && Instant::now() < deadline =>
                    ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
                Ok(Ok(_)) => StatusCode::GATEWAY_TIMEOUT.into_response(),
                Ok(Err(failure)) => failure.into_response(),
                Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            },
            () = state.stop.cancelled() => StatusCode::SERVICE_UNAVAILABLE.into_response(),
            () = tokio::time::sleep_until(deadline.into()) => StatusCode::GATEWAY_TIMEOUT.into_response(),
        };
        drop(guard);
        if let Ok(mut jobs) = state.jobs.try_lock() {
            while jobs.try_join_next().is_some() {}
        }
        result
    }
}

#[cfg(test)]
#[path = "../tests/support/graph_server.rs"]
pub(crate) mod tests;

#[cfg(test)]
mod installation_binding_tests {
    use super::*;

    #[tokio::test]
    async fn installation_binding_accepts_native_alias_and_rejects_distinct_root() {
        let fixture = crate::source_server::test_fixture::fixture(
            &[graphit_auth::Permission::Read(
                graphit_core::module_settings::Module::Ast,
            )],
            graphit_core::brand::Brand::default(),
            graphit_hub::NativeBinding::KiroV3,
        );
        let server = fixture.server.graph_server();
        let mut policy = server.0.policy.clone();
        let root = policy.installation.root().to_owned();
        let child = root.join("binding-alias");
        std::fs::create_dir(&child).unwrap();
        let alias = child.join("..");
        assert_ne!(
            alias.as_path(),
            server.0.auth.installation().live.owner_root()
        );
        policy.installation = graphit_core::project_paths::InstallationPaths::resolve(
            policy.installation.brand().clone(),
            &root,
            policy.installation.home_directory(),
            Some(&alias),
        )
        .unwrap();
        GraphServer::new(
            Arc::clone(&server.0.auth),
            policy.clone(),
            CancellationToken::new(),
        )
        .unwrap();

        let other = tempfile::tempdir().unwrap();
        policy.installation = graphit_core::project_paths::InstallationPaths::resolve(
            policy.installation.brand().clone(),
            &root,
            policy.installation.home_directory(),
            Some(other.path()),
        )
        .unwrap();
        let error = GraphServer::new(Arc::clone(&server.0.auth), policy, CancellationToken::new())
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "graph installation mismatch");
    }
}

struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn credential(headers: &HeaderMap) -> Result<(Zeroizing<String>, bool), StatusCode> {
    if headers.get_all("x-graphit-auth-origin").iter().count() != 1
        || headers.get_all(header::AUTHORIZATION).iter().count() != 1
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let local = match headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return Err(StatusCode::UNAUTHORIZED),
    };
    let raw = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok((Zeroizing::new(raw.to_owned()), local))
}
#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Imports,
    Query,
    Schema,
    Sample,
    Neighborhood,
}
#[derive(Clone, Copy)]
enum Target {
    Installed,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Project,
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
macro_rules! project_route {
    ($name:ident, $operation:ident) => {
        async fn $name(
            State(server): State<GraphServer>,
            headers: HeaderMap,
            body: Bytes,
        ) -> Response {
            server
                .read_target(headers, body, Operation::$operation, Target::Project)
                .await
        }
    };
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
project_route!(project_query, Query);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
project_route!(project_schema, Schema);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
project_route!(project_sample, Sample);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
project_route!(project_neighborhood, Neighborhood);
async fn imports(State(server): State<GraphServer>, headers: HeaderMap, body: Bytes) -> Response {
    server.read(headers, body, Operation::Imports).await
}
async fn query(State(server): State<GraphServer>, headers: HeaderMap, body: Bytes) -> Response {
    server.read(headers, body, Operation::Query).await
}
async fn schema(State(server): State<GraphServer>, headers: HeaderMap, body: Bytes) -> Response {
    server.read(headers, body, Operation::Schema).await
}
async fn sample(State(server): State<GraphServer>, headers: HeaderMap, body: Bytes) -> Response {
    server.read(headers, body, Operation::Sample).await
}
async fn neighborhood(
    State(server): State<GraphServer>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    server.read(headers, body, Operation::Neighborhood).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    id: String,
    version: String,
    query: String,
    #[serde(default)]
    parameters: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
    version: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Neighborhood {
    id: String,
    version: String,
    anchor_label: String,
    anchor_key: Anchor,
    relationship: String,
    other_label: String,
    direction: Direction,
    limit: usize,
    cursor: Option<Cursor>,
}
#[derive(Clone, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    deny_unknown_fields,
    rename_all = "snake_case"
)]
enum Anchor {
    String(String),
    Int64(String),
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    Outgoing,
    Incoming,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    generation: [u8; 32],
    fingerprint: [u8; 32],
    last_uid: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryData {
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) parameters: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeighborhoodData {
    anchor_label: String,
    anchor_key: Anchor,
    relationship: String,
    other_label: String,
    direction: Direction,
    limit: usize,
    cursor: Option<Cursor>,
}
impl From<Neighborhood> for NeighborhoodData {
    fn from(value: Neighborhood) -> Self {
        Self {
            anchor_label: value.anchor_label,
            anchor_key: value.anchor_key,
            relationship: value.relationship,
            other_label: value.other_label,
            direction: value.direction,
            limit: value.limit,
            cursor: value.cursor,
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSelection {
    project_dir: Option<graphit_core::native_path::NativePath>,
    project_version: Option<String>,
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRequest {
    selection: ProjectSelection,
    query: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
}

enum GraphView<'a, 'r> {
    Installed(Box<ImportedGraph<'a, 'r>>),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Remote(
        Arc<graphit_data::remote_graph::RemoteIcebugGraph>,
        Arc<graphit_core::file_io::SnapshotContext>,
    ),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    WebInstalled(
        Arc<graphit_data::CapturedLocalGraph>,
        Arc<graphit_core::file_io::SnapshotContext>,
    ),
}
impl GraphView<'_, '_> {
    fn schema(&self) -> Result<graphit_data::GraphSchema<'_>, ReadFailure> {
        match self {
            Self::Installed(graph) => graph.schema().map_err(Into::into),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Remote(graph, _) => Ok(graph.schema()),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::WebInstalled(graph, _) => graph
                .schema()
                .map_err(|e| ImportedGraphError::Graph(e).into()),
        }
    }
    fn generation(&self) -> Result<graphit_data::GraphGeneration, ReadFailure> {
        Ok(self.schema()?.generation())
    }
    fn sample_graph(
        &self,
        limits: GraphSampleLimits,
    ) -> Result<graphit_data::GraphSample, ReadFailure> {
        match self {
            Self::Installed(graph) => graph.sample_graph(limits).map_err(Into::into),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Remote(graph, original) => tokio::runtime::Handle::current()
                .block_on(graph.sample_graph(limits, Arc::clone(original)))
                .map_err(|error| {
                    ReadFailure::Remote(Box::new(crate::remote_native_mount::ErrorKind::Graph(
                        error,
                    )))
                }),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::WebInstalled(graph, _) => graph
                .sample_graph(limits)
                .map_err(|e| ImportedGraphError::Graph(e).into()),
        }
    }
    fn neighborhood_graph(
        &self,
        request: GraphNeighborhoodRequest<'_>,
        limits: GraphNeighborhoodLimits,
    ) -> Result<graphit_data::GraphNeighborhoodPage, ReadFailure> {
        match self {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Remote(graph, original) => tokio::runtime::Handle::current()
                .block_on(graph.neighborhood_graph(request, limits, Arc::clone(original)))
                .map_err(|error| {
                    ReadFailure::Remote(Box::new(crate::remote_native_mount::ErrorKind::Graph(
                        error,
                    )))
                }),
            Self::Installed(graph) => graph
                .neighborhood_graph(request, limits)
                .map_err(Into::into),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::WebInstalled(graph, _) => graph
                .neighborhood_graph(request, limits)
                .map_err(|e| ImportedGraphError::Graph(e).into()),
        }
    }
    fn query(
        &self,
        runtime: &tokio::runtime::Handle,
        input: &QueryData,
        options: graphit_ast::icebug_query::QueryOptions,
    ) -> Result<graphit_ast::icebug_query::QueryResult, ReadFailure> {
        match self {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Remote(graph, original) => graphit_ast::icebug_query::execute_remote(
                graph,
                original,
                runtime,
                &input.query,
                &input.parameters,
                options,
            )
            .map_err(ReadFailure::Query),
            Self::Installed(graph) => graph
                .query(runtime, &input.query, &input.parameters, options)
                .map_err(|error| match error {
                    ImportedGraphError::Operation(error) => ReadFailure::Query(error),
                    error => ReadFailure::Imported(error),
                }),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::WebInstalled(graph, original) => graphit_ast::icebug_query::execute(
                graph,
                original,
                runtime,
                &input.query,
                &input.parameters,
                options,
            )
            .map_err(ReadFailure::Query),
        }
    }
}

pub(crate) enum ReadFailure {
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Broker(graphit_auth::broker::project_access::BrokerProjectError),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Project(crate::project_graph::ReadError),
    BadInput,
    Context,
    Imported(ImportedGraphError),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Remote(Box<crate::remote_native_mount::ErrorKind>),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Entity(graphit_ast::imported_graph::EntitySourceError),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    ImportedSource(graphit_ast::imported_source::ImportedSourceError),
    Output(io::Error),
    Query(io::Error),
}
impl IntoResponse for ReadFailure {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Remote(error) => {
                return (error.status(), error.code()).into_response();
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Broker(error) => {
                use graphit_auth::{AuthError, broker::project_access::BrokerProjectError};
                let status = match error {
                    BrokerProjectError::Denied
                    | BrokerProjectError::Authority(AuthError::Denied) => StatusCode::FORBIDDEN,
                    _ => StatusCode::BAD_GATEWAY,
                };
                return (status, "AST PROJECT authorization unavailable or denied").into_response();
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Project(error) => {
                use crate::project_graph::ReadError;
                return match error {
                    ReadError::Authentication(error) => {
                        let _ = error;
                        (
                            StatusCode::FORBIDDEN,
                            "graph authentication or PROJECT permission denied",
                        )
                            .into_response()
                    }
                    ReadError::Disabled => {
                        (StatusCode::FORBIDDEN, "AST module disabled").into_response()
                    }
                    ReadError::Inactive => {
                        (StatusCode::CONFLICT, "PROJECT selection no longer active").into_response()
                    }
                    ReadError::Absent => {
                        (StatusCode::NOT_FOUND, "PROJECT graph not published").into_response()
                    }
                    ReadError::Operation(error) => Self::Query(error).into_response(),
                    ReadError::Graph(error) => {
                        (StatusCode::CONFLICT, error.to_string()).into_response()
                    }
                    ReadError::Entity(error) => {
                        use graphit_ast::imported_graph::EntitySourceError as E;
                        let status = match error {
                            E::Unavailable | E::UnknownType => StatusCode::NOT_FOUND,
                            E::InvalidRequest | E::Ambiguous | E::InvalidExtent => {
                                StatusCode::BAD_REQUEST
                            }
                            _ => StatusCode::CONFLICT,
                        };
                        (status, error.to_string()).into_response()
                    }
                    ReadError::Source(error) => {
                        use graphit_ast::source_handler::SourceHandlerError as E;
                        let status = match error {
                            E::Authentication(_) => StatusCode::FORBIDDEN,
                            E::MissingProject | E::InvalidPath => StatusCode::BAD_REQUEST,
                            E::Inspection(
                                graphit_ast::source_inspection::SourceInspectionError::Selection(_),
                            ) => StatusCode::BAD_REQUEST,
                            _ => StatusCode::CONFLICT,
                        };
                        (status, error.to_string()).into_response()
                    }
                };
            }
            Self::Query(error) => {
                let status = match error.kind() {
                    io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
                    io::ErrorKind::NotConnected => StatusCode::SERVICE_UNAVAILABLE,
                    io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
                        StatusCode::BAD_REQUEST
                    }
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
                        StatusCode::GATEWAY_TIMEOUT
                    }
                    _ => StatusCode::CONFLICT,
                };
                return (status, error.to_string()).into_response();
            }
            Self::BadInput => (StatusCode::BAD_REQUEST, "invalid graph request"),
            Self::Context => (
                StatusCode::GATEWAY_TIMEOUT,
                "graph operation cancelled or expired",
            ),
            Self::Imported(ImportedGraphError::Authentication(_)) => (
                StatusCode::FORBIDDEN,
                "graph authentication or permission denied",
            ),
            Self::Imported(ImportedGraphError::InstallationNotFound) => (
                StatusCode::NOT_FOUND,
                "requested native AST graph not installed",
            ),
            Self::Imported(_) => (
                StatusCode::CONFLICT,
                "installed graph origin or data invalid",
            ),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Entity(error) => {
                use graphit_ast::imported_graph::EntitySourceError as E;
                let status = match error {
                    E::Unavailable | E::UnknownType => StatusCode::NOT_FOUND,
                    E::InvalidRequest | E::Ambiguous | E::InvalidExtent => StatusCode::BAD_REQUEST,
                    _ => StatusCode::CONFLICT,
                };
                return (status, error.to_string()).into_response();
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::ImportedSource(error) => {
                use graphit_ast::imported_source::ImportedSourceError as E;
                let status = match error {
                    E::Authentication(_) => StatusCode::FORBIDDEN,
                    E::InstallationNotFound | E::SourceUnavailable => StatusCode::NOT_FOUND,
                    E::InvalidPath | E::InvalidFallback | E::Inspection(_) => {
                        StatusCode::BAD_REQUEST
                    }
                    _ => StatusCode::CONFLICT,
                };
                return (status, error.to_string()).into_response();
            }
            Self::Output(error) => {
                drop(error);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "graph response unavailable",
                )
            }
        };
        (status, message).into_response()
    }
}
impl From<ImportedGraphError> for ReadFailure {
    fn from(e: ImportedGraphError) -> Self {
        Self::Imported(e)
    }
}

fn read(
    state: &Shared,
    raw: &str,
    local: bool,
    body: &[u8],
    operation: Operation,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<Vec<u8>, ReadFailure> {
    let request = RequestContext::new(state.policy.content, cancel, deadline)
        .map_err(|_| ReadFailure::Context)?;
    if matches!(operation, Operation::Imports) {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Empty {}
        serde_json::from_slice::<Empty>(body).map_err(|_| ReadFailure::BadInput)?;
        let caller = if local {
            state.auth.issuer().authenticate_local(raw, &request)
        } else {
            state.auth.issuer().authenticate_runtime(raw, &request)
        }
        .map_err(|e| ReadFailure::Imported(ImportedGraphError::Authentication(e)))?;
        caller
            .authorize(graphit_auth::Permission::Read(
                graphit_core::module_settings::Module::Ast,
            ))
            .map_err(|e| ReadFailure::Imported(ImportedGraphError::Authentication(e)))?;
        let imports = graphit_ast::imported_graph::installed_ast_contexts(
            &state.policy.registry,
            state.auth.installation(),
            request.snapshot(),
        )?;
        if imports.len() > state.policy.content.max_collection_entries {
            return Err(ReadFailure::BadInput);
        }
        let response = encode(
            &serde_json::json!({"imports":imports}),
            state.policy.content.max_output_bytes,
        )?;
        caller
            .authorize(graphit_auth::Permission::Read(
                graphit_core::module_settings::Module::Ast,
            ))
            .map_err(|e| ReadFailure::Imported(ImportedGraphError::Authentication(e)))?;
        request
            .snapshot()
            .check()
            .map_err(|_| ReadFailure::Context)?;
        return Ok(response);
    }
    let neighborhood = if matches!(operation, Operation::Neighborhood) {
        Some(serde_json::from_slice::<Neighborhood>(body).map_err(|_| ReadFailure::BadInput)?)
    } else {
        None
    };
    let query = if matches!(operation, Operation::Query) {
        Some(serde_json::from_slice::<Query>(body).map_err(|_| ReadFailure::BadInput)?)
    } else {
        None
    };
    let selection = match (&neighborhood, &query) {
        (Some(input), _) => Selection {
            id: input.id.clone(),
            version: input.version.clone(),
        },
        (_, Some(input)) => Selection {
            id: input.id.clone(),
            version: input.version.clone(),
        },
        (None, None) => serde_json::from_slice(body).map_err(|_| ReadFailure::BadInput)?,
    };
    let projection = state.policy.registry.limits().projection();
    let id = ArtifactId::new(&selection.id, projection).map_err(|_| ReadFailure::BadInput)?;
    let version =
        ConcreteVersion::new(&selection.version, projection).map_err(|_| ReadFailure::BadInput)?;
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    if let Some(bytes) = remote::read_with_auth(
        state,
        raw,
        local,
        &id,
        &version,
        operation,
        query.as_ref().map(|input| QueryData {
            query: input.query.clone(),
            parameters: input.parameters.clone(),
        }),
        neighborhood
            .as_ref()
            .map(|input| NeighborhoodData::from(input.clone())),
        &request,
    )? {
        return Ok(bytes);
    }
    let input = ImportedGraphInput {
        registry: &state.policy.registry,
        id: &id,
        version: &version,
        installation: state.auth.installation(),
        file_policy: &state.policy.file,
        read_limits: state.policy.read,
        graph_policy: state.policy.native_install.graph,
        directory_limits: state.policy.index,
        request: &request,
    };
    let graph = if local {
        open_imported_graph_local(state.auth.issuer(), raw, input)?
    } else {
        open_imported_graph_runtime(state.auth.issuer(), raw, input)?
    };
    let graph = GraphView::Installed(Box::new(graph));
    let query = query.map(|input| QueryData {
        query: input.query,
        parameters: input.parameters,
    });
    let response = render_operation(
        state,
        &graph,
        operation,
        query,
        neighborhood.map(Into::into),
        state.policy.native_install.graph,
    )?;
    request
        .snapshot()
        .check()
        .map_err(|_| ReadFailure::Context)?;
    Ok(response)
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn read_project(
    state: &Shared,
    raw: &str,
    local: bool,
    body: &[u8],
    operation: Operation,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<Vec<u8>, ReadFailure> {
    let input = parse_project(body, operation)?;
    let access = state.project.as_ref().ok_or(ReadFailure::Context)?;
    let request = RequestContext::new(state.policy.content, cancel, deadline)
        .map_err(|_| ReadFailure::Context)?;
    let snapshot = access
        .live
        .snapshot()
        .map_err(|error| ReadFailure::Project(crate::project_graph::ReadError::Operation(error)))?;
    let index = select_project(state, &input.selection, &snapshot)?;
    let (paths, id, version) = &snapshot.projects[index];
    crate::project_graph::with_read(
        crate::project_graph::ReadInput {
            auth: &state.auth,
            policy: &state.policy,
            base: &access.config,
            live: &access.live,
            selected: (paths, *id, version),
            credential: raw,
            local,
            request: &request,
        },
        |capture| {
            render_project_capture(state, capture, operation, input.query, input.neighborhood)
        },
    )
    .map_err(ReadFailure::Project)?
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn render_project_capture(
    state: &Shared,
    capture: &crate::project_graph::PreparedRead,
    operation: Operation,
    query: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
) -> Result<Vec<u8>, ReadFailure> {
    let access = state.project.as_ref().ok_or(ReadFailure::Context)?;
    let _fence = tokio::runtime::Handle::current()
        .block_on(capture.open_live(
            state.auth.installation(),
            &state.live_ast,
            &state.policy,
            &access.config,
            &access.live,
        ))
        .map_err(ReadFailure::Project)?;
    if let Some(index) = capture.live_index() {
        let result = live::render_live(
            &index.graph,
            capture.original(),
            operation,
            query,
            neighborhood,
            state.policy.graph_query,
            state.policy.content.max_output_bytes,
        );
        capture
            .check(&state.policy, &access.config, &access.live)
            .map_err(ReadFailure::Project)?;
        return result;
    }
    Err(ReadFailure::Project(
        crate::project_graph::ReadError::Absent,
    ))
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn parse_project(body: &[u8], operation: Operation) -> Result<ProjectRequest, ReadFailure> {
    let input: ProjectRequest = serde_json::from_slice(body).map_err(|_| ReadFailure::BadInput)?;
    if !matches!(
        (&operation, &input.query, &input.neighborhood),
        (Operation::Query, Some(_), None)
            | (Operation::Neighborhood, None, Some(_))
            | (Operation::Schema | Operation::Sample, None, None)
    ) {
        return Err(ReadFailure::BadInput);
    }
    Ok(input)
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn select_project(
    state: &Shared,
    selection: &ProjectSelection,
    snapshot: &crate::live_project_selections::LiveProjectSnapshot,
) -> Result<usize, ReadFailure> {
    let current = snapshot
        .current
        .and_then(|index| snapshot.projects.get(index));
    let paths = selection
        .project_dir
        .as_ref()
        .map(|path| path.as_path())
        .or_else(|| current.map(|(paths, _, _)| paths.root()))
        .ok_or(ReadFailure::BadInput)?;
    if selection.project_version.as_deref().is_some_and(|version| {
        version.is_empty()
            || version.contains('\0')
            || version.len() > state.policy.content.max_scalar_bytes
    }) {
        return Err(ReadFailure::BadInput);
    }
    let mut selected =
        snapshot
            .projects
            .iter()
            .enumerate()
            .filter(|(_, (candidate, _, version))| {
                candidate.root() == paths
                    && selection
                        .project_version
                        .as_deref()
                        .is_none_or(|name| version.version() == name)
            });
    let (index, _) = selected.next().ok_or(ReadFailure::BadInput)?;
    if selected.next().is_some() {
        return Err(ReadFailure::BadInput);
    }
    Ok(index)
}

fn render_operation(
    state: &Shared,
    graph: &GraphView<'_, '_>,
    operation: Operation,
    query: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
    policy: graphit_data::LocalGraphPolicy,
) -> Result<Vec<u8>, ReadFailure> {
    let limits = policy.limits();
    let response = match operation {
        Operation::Imports => return Err(ReadFailure::BadInput),
        Operation::Query => {
            let input = query.ok_or(ReadFailure::BadInput)?;
            let runtime = tokio::runtime::Handle::try_current()
                .map_err(|error| ReadFailure::Output(io::Error::other(error)))?;
            let result = graph.query(&runtime, &input, state.policy.graph_query)?;
            encode(&result, state.policy.content.max_output_bytes)?
        }
        Operation::Schema => encode(
            &schema_wire(&graph.schema()?),
            state.policy.content.max_output_bytes,
        )?,
        Operation::Sample => {
            let limits = GraphSampleLimits::new(
                300,
                1000,
                2300,
                state.policy.content.max_scalar_bytes,
                limits.query_decoded_rows,
                limits.max_pages,
                256.min(limits.query_decoded_rows),
            )
            .map_err(ImportedGraphError::Graph)?;
            encode(
                &sample_wire(
                    &graph.sample_graph(limits)?,
                    match graph {
                        GraphView::Installed(_) => false,
                        #[cfg(any(
                            target_os = "linux",
                            target_os = "windows",
                            target_os = "macos"
                        ))]
                        GraphView::Remote(..) => false,
                        #[cfg(any(
                            target_os = "linux",
                            target_os = "windows",
                            target_os = "macos"
                        ))]
                        GraphView::WebInstalled(..) => true,
                    },
                ),
                state.policy.content.max_output_bytes,
            )?
        }
        Operation::Neighborhood => {
            let input = neighborhood.ok_or(ReadFailure::BadInput)?;
            if input.limit == 0 || input.limit > 1000 {
                return Err(ReadFailure::BadInput);
            }
            let generation = graph.generation()?;
            let cursor = match &input.cursor {
                Some(cursor) if cursor.generation == generation.bytes() => {
                    Some(GraphNeighborhoodCursor::from_parts(
                        generation,
                        cursor.fingerprint,
                        &cursor.last_uid,
                    ))
                }
                Some(_) => return Err(ReadFailure::BadInput),
                None => None,
            };
            let anchor_key = match &input.anchor_key {
                Anchor::String(value) => GraphAnchorKey::String(value),
                Anchor::Int64(value) => {
                    GraphAnchorKey::Int64(value.parse().map_err(|_| ReadFailure::BadInput)?)
                }
            };
            let limits = GraphNeighborhoodLimits::new(
                state.policy.content.max_scalar_bytes,
                limits.max_columns,
                limits.query_decoded_rows,
                limits.query_decoded_rows,
                limits.max_pages,
                256.min(limits.query_decoded_rows),
            )
            .map_err(ImportedGraphError::Graph)?;
            let page = graph.neighborhood_graph(
                GraphNeighborhoodRequest {
                    anchor_label: &input.anchor_label,
                    anchor_key,
                    relationship: &input.relationship,
                    other_label: &input.other_label,
                    direction: match input.direction {
                        Direction::Outgoing => GraphDirection::Outgoing,
                        Direction::Incoming => GraphDirection::Incoming,
                    },
                    limit: input.limit,
                    cursor,
                },
                limits,
            )?;
            encode(
                &neighborhood_wire(&page),
                state.policy.content.max_output_bytes,
            )?
        }
    };
    graph.generation()?;
    Ok(response)
}

// Enforce the actual JSON output boundary while serde writes bytes, not a heap estimate.
fn encode(value: &impl Serialize, maximum: usize) -> Result<Vec<u8>, ReadFailure> {
    struct Output {
        bytes: Vec<u8>,
        maximum: usize,
    }
    impl io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "graph JSON output too large",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Output {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut output, value)
        .map_err(|e| ReadFailure::Output(io::Error::other(e)))?;
    Ok(output.bytes)
}

#[derive(Serialize)]
struct Schema<'a> {
    generation: [u8; 32],
    nodes: Vec<NodeSchema<'a>>,
    relationships: Vec<RelationshipSchema<'a>>,
}
struct NodeSchema<'a>(graphit_data::GraphNodeSchema<'a>);
impl Serialize for NodeSchema<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut output = serializer.serialize_struct("NodeSchema", 5)?;
        output.serialize_field("label", self.0.label())?;
        output.serialize_field("primary_key", self.0.primary_key())?;
        output.serialize_field("rows", &self.0.rows().to_string())?;
        output.serialize_field("columns", &self.0.columns().collect::<Vec<_>>())?;
        output.serialize_field(
            "language_counts",
            &self
                .0
                .language_counts()
                .map(|(name, count)| (name, count.to_string()))
                .collect::<Vec<_>>(),
        )?;
        output.end()
    }
}
struct RelationshipSchema<'a>(graphit_data::GraphRelationshipSchema<'a>);
impl Serialize for RelationshipSchema<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut output = serializer.serialize_struct("RelationshipSchema", 5)?;
        output.serialize_field("kind", self.0.kind())?;
        output.serialize_field("from_label", self.0.from_label())?;
        output.serialize_field("to_label", self.0.to_label())?;
        output.serialize_field("rows", &self.0.rows().to_string())?;
        output.serialize_field("properties", &self.0.properties().collect::<Vec<_>>())?;
        output.end()
    }
}
fn schema_wire<'a>(schema: &'a graphit_data::GraphSchema<'a>) -> Schema<'a> {
    Schema {
        generation: schema.generation().bytes(),
        nodes: schema.node_tables().map(NodeSchema).collect(),
        relationships: schema.relationships().map(RelationshipSchema).collect(),
    }
}
/// Preserve the existing graph wire format and its concrete output byte bound.
pub(crate) fn schema_value(
    schema: &graphit_data::GraphSchema<'_>,
    maximum: usize,
) -> io::Result<serde_json::Value> {
    let bytes = encode(&schema_wire(schema), maximum).map_err(|error| match error {
        ReadFailure::Output(cause) | ReadFailure::Query(cause) => cause,
        _ => io::Error::new(io::ErrorKind::InvalidData, "graph schema encoding failed"),
    })?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

#[derive(Serialize)]
struct NodeId {
    table: u32,
    row: String,
}
fn node_id(id: graphit_data::NodeId) -> NodeId {
    NodeId {
        table: id.table(),
        row: id.row().to_string(),
    }
}
#[derive(Serialize)]
struct EdgeId {
    member: u32,
    row: String,
}
#[derive(Serialize)]
struct Sample<'a> {
    generation: [u8; 32],
    nodes: Vec<Node<'a>>,
    edges: Vec<Edge<'a>>,
}
#[derive(Serialize)]
struct Node<'a> {
    id: NodeId,
    label: &'a str,
    name: Option<&'a str>,
    path: Option<&'a str>,
    uid: Option<&'a str>,
    cluster: Option<&'a str>,
    lang: Option<&'a str>,
    line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_key: Option<Property<'a>>,
}
#[derive(Serialize)]
struct Edge<'a> {
    id: EdgeId,
    from: NodeId,
    to: NodeId,
    relationship: &'a str,
    uid: &'a str,
}
fn sample_wire(sample: &graphit_data::GraphSample, native: bool) -> Sample<'_> {
    Sample {
        generation: sample.generation().bytes(),
        nodes: sample
            .nodes()
            .iter()
            .map(|n| Node {
                id: node_id(n.id()),
                label: n.label(),
                name: n.name(),
                path: n.path(),
                uid: n.uid(),
                cluster: n.cluster(),
                lang: n.lang(),
                line: n.line().map(|v| v.to_string()),
                primary_key: native.then(|| n.primary_key()).flatten().map(property),
            })
            .collect(),
        edges: sample
            .edges()
            .iter()
            .map(|e| Edge {
                id: EdgeId {
                    member: e.id().member(),
                    row: e.id().row().to_string(),
                },
                from: node_id(e.from()),
                to: node_id(e.to()),
                relationship: e.relationship(),
                uid: e.uid(),
            })
            .collect(),
    }
}
#[derive(Serialize)]
struct NeighborhoodPage<'a> {
    generation: [u8; 32],
    anchor: NodeId,
    nodes: Vec<PropertyNode<'a>>,
    edges: Vec<PropertyEdge<'a>>,
    next_cursor: Option<Cursor>,
}
#[derive(Serialize)]
struct PropertyNode<'a> {
    id: NodeId,
    label: &'a str,
    properties: Vec<Property<'a>>,
}
#[derive(Serialize)]
struct PropertyEdge<'a> {
    id: EdgeId,
    from: NodeId,
    to: NodeId,
    relationship: &'a str,
    uid: &'a str,
    properties: Vec<Property<'a>>,
}
#[derive(Serialize)]
struct Property<'a> {
    name: &'a str,
    value: PropertyValue<'a>,
}
#[derive(Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
enum PropertyValue<'a> {
    String(&'a str),
    Int64(String),
    Bool(bool),
    Null,
}
fn property(p: &graphit_data::GraphProperty) -> Property<'_> {
    Property {
        name: p.name(),
        value: match p.value() {
            GraphPropertyValueRef::String(v) => PropertyValue::String(v),
            GraphPropertyValueRef::Int64(v) => PropertyValue::Int64(v.to_string()),
            GraphPropertyValueRef::Bool(v) => PropertyValue::Bool(v),
            GraphPropertyValueRef::Null => PropertyValue::Null,
        },
    }
}
fn properties(values: &[graphit_data::GraphProperty]) -> Vec<Property<'_>> {
    values.iter().map(property).collect()
}
fn neighborhood_wire(page: &graphit_data::GraphNeighborhoodPage) -> NeighborhoodPage<'_> {
    NeighborhoodPage {
        generation: page.generation().bytes(),
        anchor: node_id(page.anchor()),
        nodes: page
            .nodes()
            .iter()
            .map(|n| PropertyNode {
                id: node_id(n.id()),
                label: n.label(),
                properties: properties(n.properties()),
            })
            .collect(),
        edges: page
            .edges()
            .iter()
            .map(|e| PropertyEdge {
                id: EdgeId {
                    member: e.id().member(),
                    row: e.id().row().to_string(),
                },
                from: node_id(e.from()),
                to: node_id(e.to()),
                relationship: e.relationship(),
                uid: e.uid(),
                properties: properties(e.properties()),
            })
            .collect(),
        next_cursor: page.next_cursor().map(|c| Cursor {
            generation: c.generation().bytes(),
            fingerprint: c.query_fingerprint(),
            last_uid: c.last_uid().into(),
        }),
    }
}
