//! Local MCP tools authenticated by the actual run-owned Role credential.
use crate::{
    source_server::SourcePolicy,
    task_server::{
        TaskPolicy, TaskServer,
        request::{Arguments, Credential, Origin, Selection},
    },
};
use axum::{
    Router,
    http::{HeaderMap, request::Parts},
};
use graphit_auth::{ProjectCapture, ProjectRecordAuthority, ScopeCapture, ScopeInput};
use graphit_core::{
    artifact_store::CapturedPublicationScope, config::ConfigSnapshot, file_lock::Cancellation,
    project_identity::ProjectId, project_paths::ProjectPaths, version_context::VersionContext,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_data::knowledge_index::{KnowledgeHeadReadLimits, KnowledgeIndexReadLimits};
use graphit_hub::{native::RoleCredentialReference, policy::RequestContext};
use graphit_knowledge::document_source::DocumentSourceLimits;
use graphit_records::memory_store::{MemoryStoreReadLimits, MemoryStoreWriteLimits};
use graphit_runtime::instructions::InstructionLimits;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext as McpRequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};
use std::{io, num::NonZeroUsize, sync::Arc, time::Instant};
use tokio::{sync::oneshot, task::JoinSet};
use tokio_util::sync::CancellationToken;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod ast;
mod catalog;
mod git;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub;
mod knowledge;
mod memory;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod references;
pub(crate) mod session_hook;
mod skill;
mod source;

/// Effective producer policies; none grants access or installs a profile.
#[derive(Clone)]
pub struct RoleToolPolicy {
    pub source: SourcePolicy,
    pub memory_read: MemoryStoreReadLimits,
    pub memory_write: MemoryStoreWriteLimits,
    pub task_max_query_bytes: NonZeroUsize,
    pub knowledge: KnowledgeIndexReadLimits,
    pub knowledge_heads: KnowledgeHeadReadLimits,
    pub document_source: DocumentSourceLimits,
    pub instructions: InstructionLimits,
    pub config: ConfigSnapshot,
}
impl RoleToolPolicy {
    /// Resolve the current content policies; these bounds do not grant access.
    pub(crate) fn capture(source: SourcePolicy, config: ConfigSnapshot) -> io::Result<Self> {
        use graphit_data::knowledge_index::KnowledgeIndexContentLimits;
        use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};
        let content = source.native_install.hub.content();
        let n = |value| {
            NonZeroUsize::new(value).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))
        };
        let rows = n(content.max_collection_entries)?;
        let text = n(content.max_output_bytes)?;
        let input = n(content.max_input_bytes)?;
        let scalar = n(content.max_scalar_bytes)?;
        let batch_rows = NonZeroU32::new(8192).expect("Lance scanner batch default is positive");
        let memory_read = MemoryStoreReadLimits {
            max_query_bytes: text,
            batch_rows,
            native: source.index,
        };
        let policy = Self {
            memory_read,
            memory_write: MemoryStoreWriteLimits {
                read: memory_read,
                artifacts: source.native_install.hub.artifacts(),
                max_input_bytes: input,
                max_row_bytes: input,
            },
            task_max_query_bytes: text,
            knowledge: KnowledgeIndexReadLimits {
                content: KnowledgeIndexContentLimits {
                    max_identifier_bytes: scalar,
                },
                native: source.index,
                batch_rows,
            },
            knowledge_heads: KnowledgeHeadReadLimits {
                max_slug_bytes: scalar,
                native: source.index,
                batch_rows,
            },
            document_source: DocumentSourceLimits {
                max_input_bytes: input,
                max_targets: rows,
                max_virtual_bytes: input,
                max_virtual_lines: n(content.max_nodes)?,
                max_output_bytes: text,
                max_matches: rows,
            },
            instructions: InstructionLimits {
                max_templates: rows,
                max_template_bytes: input,
                max_values: rows,
                max_variable_bytes: scalar,
                max_output_bytes: text,
                fuel: NonZeroU64::new(10_000).expect("current renderer fuel is positive"),
                recursion: n(content.max_depth)?,
            },
            source,
            config,
        };
        Ok(policy)
    }
}
/// Optional non-authority selector data for a run outside the daemon current project.
pub const ROLE_SELECTION_HEADER: &str = "x-graphit-role-selection";
type SelectedProject = (ProjectPaths, ProjectId, VersionContext);
type Projects = Vec<SelectedProject>;
fn header_selection(
    headers: &HeaderMap,
    policy: &TaskPolicy,
) -> Result<Option<Selection>, &'static str> {
    headers
        .get(ROLE_SELECTION_HEADER)
        .map(|value| {
            // The captured selector is UTF-8 JSON, including literal native paths.
            let raw = value.as_bytes();
            if raw.len() > policy.content.max_input_bytes {
                return Err("invalid_selection");
            }
            serde_json::from_slice(raw).map_err(|_| "invalid_selection")
        })
        .transpose()
}
#[cfg(test)]
type NativeGate = (
    std::sync::mpsc::Sender<Cancellation>,
    tokio::sync::oneshot::Receiver<()>,
);
struct State {
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    graph: crate::graph_server::GraphServer,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    semantic: crate::source_server::SemanticModelSlot,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    local_models: crate::source_server::LocalModelControlSlot,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    hub_http: Option<reqwest::Client>,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    hub_allow_loopback_http: bool,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    hub_verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    memory_rescans: std::sync::RwLock<Arc<[graphit_daemon::supervisor::ProjectRescanHandle]>>,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    now: Option<crate::source_server::now::NowService>,
    #[cfg(test)]
    gate: std::sync::Mutex<Option<NativeGate>>,
    #[cfg(test)]
    before_delivery: std::sync::Mutex<Option<NativeGate>>,
    auth: Arc<DaemonAuth>,
    live_projects: Arc<crate::live_project_selections::LiveProjectSelections>,
    task: TaskPolicy,
    policy: RoleToolPolicy,
    tasks: TaskServer,
    stop: CancellationToken,
    jobs: tokio::sync::Mutex<JoinSet<()>>,
}
/// A dedicated Role endpoint; a Runtime or Local credential cannot substitute for a Role.
#[derive(Clone)]
pub struct RoleToolServer(Arc<State>);
impl RoleToolServer {
    /// Build a route while retaining the component for cancellation and shutdown.
    /// The daemon supplies its own issuer and previously resolved project tuples.
    ///
    /// ```no_run
    /// use std::{io, sync::Arc};
    /// use axum::Router;
    /// use graphit_app::role_tool_server::{RoleToolPolicy, RoleToolServer};
    /// use graphit_core::{project_identity::ProjectId, project_paths::ProjectPaths,
    ///     version_context::VersionContext};
    /// use graphit_daemon::auth_startup::DaemonAuth;
    /// use tokio_util::sync::CancellationToken;
    /// fn attach(
    ///     auth: Arc<DaemonAuth>,
    ///     projects: Arc<Vec<(ProjectPaths, ProjectId, VersionContext)>>,
    ///     current: usize,
    ///     policy: RoleToolPolicy,
    ///     stop: CancellationToken,
    /// ) -> io::Result<(RoleToolServer, Router)> {
    ///     let roles = RoleToolServer::new(auth, projects, current, policy, stop)?;
    ///     let router = Router::new().nest("/roles", roles.router());
    ///     Ok((roles, router))
    /// }
    /// ```
    pub fn new(
        auth: Arc<DaemonAuth>,
        projects: Arc<Projects>,
        current: usize,
        policy: RoleToolPolicy,
        stop: CancellationToken,
    ) -> io::Result<Self> {
        let task = crate::foreground_tasks::task_policy(
            &policy.source.native_install.hub,
            policy.source.scope,
            policy.source.index,
            policy.source.binding,
        )?;
        let tasks = TaskServer::new(
            Arc::clone(&auth),
            Arc::clone(&projects),
            current,
            task.clone(),
            stop.child_token(),
        )?;
        if policy.config.brand() != policy.source.installation.brand()
            || policy.source.installation.brand() != auth.installation().live.brand()
            || policy.source.installation.root() != auth.installation().live.owner_root()
            || task.content.max_input_bytes != policy.source.content.max_input_bytes
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "inconsistent Role tool policy",
            ));
        }
        let live_projects = Arc::new(crate::live_project_selections::LiveProjectSelections::new(
            Arc::clone(&projects),
            current,
        ));
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        let graph = {
            let mut graph = crate::graph_server::GraphServer::new(
                Arc::clone(&auth),
                policy.source.clone(),
                stop.child_token(),
            )?;
            graph.attach_projects(policy.config.clone(), Arc::clone(&live_projects))?;
            graph
        };
        Ok(Self(Arc::new(State {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            graph,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            semantic: Arc::new(std::sync::RwLock::new(None)),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            local_models: Arc::new(std::sync::RwLock::new(None)),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            hub_http: None,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            hub_allow_loopback_http: false,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            hub_verifiers: None,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            memory_rescans: std::sync::RwLock::new(Arc::from([])),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            now: None,
            #[cfg(test)]
            gate: std::sync::Mutex::new(None),
            #[cfg(test)]
            before_delivery: std::sync::Mutex::new(None),
            auth,
            live_projects,
            task,
            policy,
            tasks,
            stop,
            jobs: tokio::sync::Mutex::new(JoinSet::new()),
        })))
    }
    /// Share the owner's operational selection before any route clones are served.
    pub(crate) fn with_live_projects(
        self,
        live: Arc<crate::live_project_selections::LiveProjectSelections>,
    ) -> io::Result<Self> {
        let mut state = Arc::try_unwrap(self.0).map_err(|_| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Role selections already shared",
            )
        })?;
        state.tasks = state.tasks.with_live_projects(Arc::clone(&live))?;
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        state
            .graph
            .attach_projects(state.policy.config.clone(), Arc::clone(&live))?;
        state.live_projects = live;
        Ok(Self(Arc::new(state)))
    }
    /// Reuse the daemon's graph registry and readers before serving Role routes.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn with_graph_server(
        mut self,
        graph: crate::graph_server::GraphServer,
    ) -> io::Result<Self> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Role graph already shared"))?;
        state.graph = graph;
        Ok(self)
    }
    /// Share transport owners before serving. Every operation still captures Role authority.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn with_hub_transport(
        mut self,
        http: reqwest::Client,
        allow_loopback_http: bool,
        verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
    ) -> io::Result<Self> {
        let state = Arc::get_mut(&mut self.0).ok_or_else(|| {
            io::Error::other("Role Hub transport must be captured before serving")
        })?;
        state.hub_http = Some(http);
        state.hub_allow_loopback_http = allow_loopback_http;
        state.hub_verifiers = verifiers;
        Ok(self)
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn with_local_model_control(
        mut self,
        slot: crate::source_server::LocalModelControlSlot,
    ) -> io::Result<Self> {
        let state = Arc::get_mut(&mut self.0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Role model control already shared",
            )
        })?;
        state.tasks.attach_local_model_control(Arc::clone(&slot))?;
        state.local_models = slot;
        Ok(self)
    }

    /// Share the daemon's actual model slot; it contains no Role admission.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn with_semantic_slot(
        mut self,
        slot: crate::source_server::SemanticModelSlot,
    ) -> io::Result<Self> {
        let state = Arc::get_mut(&mut self.0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Role model slot already shared",
            )
        })?;
        state.semantic = slot;
        Ok(self)
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn attach_now(
        &mut self,
        now: &crate::source_server::now::NowService,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Role Now must be captured before serving"))?;
        state.now = Some(now.fork(state.stop.child_token()));
        Ok(())
    }
    /// Replace the owner's current wake hints; no handle grants project access.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn attach_memory_rescans(
        &self,
        handles: Arc<[graphit_daemon::supervisor::ProjectRescanHandle]>,
    ) -> io::Result<()> {
        self.0.tasks.attach_rescans(Arc::clone(&handles))?;
        *self
            .0
            .memory_rescans
            .write()
            .map_err(|_| io::Error::other("Memory rescans poisoned"))? = handles;
        Ok(())
    }
    /// Dispatch to the retained owner's exact selection after caller authorization.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn request_project_rescan(
        &self,
        paths: &ProjectPaths,
        id: ProjectId,
        version: &VersionContext,
    ) -> io::Result<()> {
        let handles = self
            .0
            .memory_rescans
            .read()
            .map_err(|_| io::Error::other("PROJECT rescans unavailable"))?;
        let handle = handles
            .iter()
            .find(|handle| {
                let key = handle.key();
                key.id == id && key.checkout == paths.root() && key.version == version.version()
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "PROJECT rescan owner unavailable")
            })?;
        if handle.request_rescan() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "PROJECT rescan owner stopped",
            ))
        }
    }
    /// Cancels admission and joins actual native operations before issuer teardown.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        if let Some(now) = &self.0.now {
            now.shutdown().await?;
        }
        let tasks = self.0.tasks.shutdown().await;
        let mut jobs = self.0.jobs.lock().await;
        let mut failed = false;
        while let Some(result) = jobs.join_next().await {
            failed |= result.is_err();
        }
        tasks?;
        if failed {
            Err(io::Error::other("Role native operation failed to join"))
        } else {
            Ok(())
        }
    }
    /// Merge this router at an explicit Role path in the daemon-owned listener.
    pub fn router(&self) -> Router {
        let factory = self.clone();
        let mut config = StreamableHttpServerConfig::default();
        config.cancellation_token = self.0.stop.clone();
        config.max_request_body_bytes = self.0.task.content.max_input_bytes;
        Router::new().nest_service(
            "/mcp",
            StreamableHttpService::new(
                move || Ok(factory.clone()),
                Arc::new(LocalSessionManager::default()),
                config,
            ),
        )
    }
    async fn call(
        &self,
        name: String,
        arguments: Arguments,
        credential: Credential,
        transport: CancellationToken,
    ) -> CallToolResult {
        if !matches!(&credential.origin, Origin::Role(_)) {
            return failed("role_credential_required");
        }
        let brand = self.0.policy.source.installation.brand();
        if let Some((operation, write)) = catalog::task_operation(brand, &name) {
            return crate::task_server::mcp_result(
                self.0
                    .tasks
                    .call(arguments, operation, write, credential, transport)
                    .await,
            );
        }
        let snapshot = match self.0.live_projects.snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => return failed("selection_unavailable"),
        };
        let Some(current) = snapshot.current else {
            return failed("selection_unavailable");
        };
        let selected = match arguments.selection.select(&snapshot.projects, current) {
            Ok(selected) => snapshot.projects[selected].clone(),
            Err(cause) => return failed(cause),
        };
        drop(snapshot);
        if let Err(cause) = require_active(&self.0, &selected) {
            return crate::mcp_output::typed_error(cause);
        }
        let operation = match catalog::operation(brand, &name) {
            Some(op) => op,
            None => return failed("unknown_tool"),
        };
        let cancel = Cancellation::default();
        let _cancel = CancelOnDrop(cancel.clone());
        let state = Arc::clone(&self.0);
        let deadline = Instant::now() + state.task.timeout;
        let (mut send, receive) = oneshot::channel();
        {
            let mut jobs = tokio::select! {
                jobs=state.jobs.lock()=>jobs,
                ()=transport.cancelled()=>return failed("cancelled"),
                ()=state.stop.cancelled()=>return failed("stopped"),
                ()=tokio::time::sleep_until(deadline.into())=>return failed("deadline"),
            };
            while jobs.try_join_next().is_some() {}
            if state.stop.is_cancelled() {
                return failed("stopped");
            }
            let owner = Arc::clone(&state);
            let original_cancel = cancel.clone();
            jobs.spawn(async move {
                let result = if operation.git() {
                    git::run(
                        Arc::clone(&owner),
                        selected,
                        operation,
                        arguments.input,
                        credential,
                        original_cancel,
                        deadline,
                        &mut send,
                    )
                    .await
                    .map(RoleOutput::Structured)
                } else {
                    execute(
                        &owner,
                        selected,
                        operation,
                        arguments.input,
                        credential,
                        &original_cancel,
                        deadline,
                    )
                    .await
                    .map(|value| {
                        if operation == catalog::Operation::SessionHook {
                            RoleOutput::Text(
                                value.as_str().expect("typed Role hook output").to_owned(),
                            )
                        } else {
                            RoleOutput::Structured(value)
                        }
                    })
                };
                let _ = send.send(result);
            });
        }
        let result = tokio::select! {
            biased;
            result=receive=>result.unwrap_or_else(|_|Err(error("native_job_failed"))),
            ()=transport.cancelled()=>Err(error("cancelled")),
            ()=state.stop.cancelled()=>Err(error("stopped")),
            ()=tokio::time::sleep_until(deadline.into())=>Err(error("deadline")),
        };
        match result {
            Ok(RoleOutput::Text(value)) => CallToolResult::success(vec![ContentBlock::text(value)]),
            Ok(RoleOutput::Structured(value)) => crate::mcp_output::typed(value),
            Err(value) => crate::mcp_output::typed_error(value),
        }
    }
}
enum RoleOutput {
    Structured(Value),
    Text(String),
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
type ToolResult = Result<Value, Value>;
fn require_active(state: &State, selected: &SelectedProject) -> Result<(), Value> {
    let (paths, id, version) = selected;
    if !state
        .live_projects
        .is_active(paths, *id, version)
        .map_err(|_| error("selection_unavailable"))?
    {
        return Err(error("selection_unavailable"));
    }
    Ok(())
}

fn error(code: &'static str) -> Value {
    json!({"code":code})
}
fn failed(code: &'static str) -> CallToolResult {
    crate::mcp_output::typed_error(error(code))
}
fn parse<T: serde::de::DeserializeOwned>(input: Option<Value>) -> Result<T, Value> {
    serde_json::from_value(input.unwrap_or_else(|| json!({})))
        .map_err(|_| error("invalid_arguments"))
}
async fn execute(
    state: &State,
    selected: SelectedProject,
    operation: catalog::Operation,
    input: Option<Value>,
    credential: Credential,
    cancel: &Cancellation,
    deadline: Instant,
) -> ToolResult {
    let original = RequestContext::new(state.task.content, cancel, deadline)
        .map_err(|_| error("request_cancelled_or_expired"))?;
    require_active(state, &selected)?;
    let (paths, id, version) = &selected;
    let project =
        CapturedPublicationScope::project(paths, *id, state.task.scope, original.snapshot())
            .map_err(|_| error("project_capture_failed"))?;
    let binding = project
        .binding()
        .map_err(|_| error("project_binding_failed"))?;
    let scope = ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: credential.role_binding.unwrap_or(state.task.binding),
        global_version: "",
    };
    let Origin::Role(reference) = &credential.origin else {
        return Err(error("role_credential_required"));
    };
    let reference = RoleCredentialReference::decode(
        reference.as_bytes(),
        state.task.content.max_input_bytes,
        state.task.role_reference,
        &original,
    )
    .map_err(|_| error("invalid_role_reference"))?;
    // The existing Source composer owns Role verification for this operation.
    if operation == catalog::Operation::Source {
        let value = source::read(
            state,
            scope,
            &credential.bearer,
            &reference,
            input,
            &original,
        )
        .await?;
        require_active(state, &selected)?;
        return Ok(value);
    }
    let grant = state
        .auth
        .issuer()
        .verify(&credential.bearer, &reference, scope, &original)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    #[cfg(test)]
    {
        let gate = state.gate.lock().unwrap().take();
        if let Some((started, release)) = gate {
            started.send(cancel.clone()).unwrap();
            let _ = release.await;
            original
                .snapshot()
                .check()
                .map_err(|_| error("request_cancelled_or_expired"))?;
        }
    }
    require_active(state, &selected)?;
    let effective_binding = scope.binding;
    let authority = ProjectRecordAuthority::new(state.auth.issuer());
    let value =
        match operation {
            catalog::Operation::Skill => {
                skill::read(state, paths, effective_binding, &grant, input, &original)
            }
            catalog::Operation::SessionHook => {
                session_hook::read(state, paths, reference.role(), &grant, input, &original).await
            }
            catalog::Operation::TaskSearch => {
                let admission = authority
                    .admit_read(&grant, &original, graphit_auth::ProjectReadModule::Task)
                    .map_err(|_| error("authentication_or_permission_denied"))?;
                let query: graphit_records::task_store::TaskLexicalQuery = parse(input)?;
                let semantic = query.mode != graphit_records::task_store::TaskSearchMode::Fts;
                let resources = if semantic {
                    let fresh = crate::ast_profile_loader::project_config(
                        &state.policy.source.installation,
                        paths,
                        &state.policy.config,
                        original.snapshot(),
                    )
                    .map_err(|_| error("configuration_unavailable"))?;
                    if !graphit_core::module_settings::ModuleSettings::resolve(&fresh)
                        .map_err(|_| error("configuration_unavailable"))?
                        .enabled(graphit_core::module_settings::Module::Embedding)
                    {
                        return Err(error("model_disabled"));
                    }
                    state
                        .semantic
                        .read()
                        .map_err(|_| error("model_unavailable"))?
                        .clone()
                } else {
                    None
                };
                let mut service = if semantic {
                    Some(
                        state
                            .auth
                            .issuer()
                            .capture_role_service(&grant, graphit_auth::ServicePurpose::Embedding)
                            .map_err(|_| error("authentication_or_permission_denied"))?,
                    )
                } else {
                    None
                };
                let read = crate::task_server::search::run(
                    &original,
                    &admission,
                    query,
                    &state.task,
                    state.policy.task_max_query_bytes,
                    resources.as_deref(),
                    service.as_mut(),
                )
                .await;
                match read {
                    Ok(value) => {
                        serde_json::to_value(value).map_err(|_| error("response_encoding_failed"))
                    }
                    Err(value) => Err(serde_json::to_value(value)
                        .map_err(|_| error("response_encoding_failed"))?),
                }
            }
            op if op.memory() => {
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                let rescans = if op.write() {
                    let handles = state
                        .memory_rescans
                        .read()
                        .map_err(|_| error("rescan_unavailable"))?;
                    Some(Arc::clone(&*handles))
                } else {
                    None
                };
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                let rescan = rescans.as_deref().and_then(|handles| {
                    handles.iter().find(|handle| {
                        let key = handle.key();
                        key.id == *id
                            && key.checkout == paths.root()
                            && key.version == version.version()
                    })
                });
                #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
                let rescan = None;
                memory::execute(state, &authority, &grant, op, input, &original, rescan).await
            }
            op if op.knowledge() => {
                knowledge::execute(state, &authority, &grant, op, input, &original).await
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            op if op.hub() => {
                hub::read(
                    state,
                    &authority,
                    &grant,
                    &selected,
                    effective_binding,
                    op,
                    input,
                    &original,
                )
                .await
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            op if op.references() => {
                references::read(state, &authority, &grant, op, input, &original).await
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            op if op.ast() => {
                ast::execute(state, &authority, &grant, &selected, op, input, &original).await
            }
            _ => Err(error("unknown_tool")),
        }?;
    #[cfg(test)]
    {
        let gate = state.before_delivery.lock().unwrap().take();
        if let Some((started, release)) = gate {
            started.send(cancel.clone()).unwrap();
            let _ = release.await;
        }
    }
    let post = original
        .snapshot()
        .check()
        .map_err(|_| error("request_cancelled_or_expired"))
        .and_then(|()| {
            grant
                .actor()
                .map(|_| ())
                .map_err(|_| error("authentication_or_permission_denied"))
        })
        .and_then(|()| require_active(state, &selected));
    if let Err(cause) = post {
        return Err(delivery_failure(operation, &value, cause));
    }
    let length = if operation == catalog::Operation::SessionHook {
        value.as_str().expect("typed Role hook output").len()
    } else {
        serde_json::to_vec(&value)
            .map_err(|_| error("response_encoding_failed"))?
            .len()
    };
    if length > state.task.content.max_output_bytes {
        return Err(delivery_failure(operation, &value, error("response_limit")));
    }
    Ok(value)
}
fn delivery_failure(operation: catalog::Operation, value: &Value, cause: Value) -> Value {
    if operation.write() {
        json!({"code":"write_confirmed", "memory":{"id":value["result"]["id"],"revision":value["result"]["revision"]}, "cause":cause})
    } else {
        cause
    }
}
fn role_tool(brand: &graphit_core::brand::Brand, tool: Tool) -> Tool {
    if catalog::operation(brand, &tool.name) == Some(catalog::Operation::SessionHook) {
        tool
    } else {
        crate::mcp_output::decorate_tool(tool)
    }
}
impl ServerHandler for RoleToolServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        if let Some(tool) = self.0.now.as_ref().and_then(|now| now.tool(name)) {
            return Some(crate::mcp_output::decorate_tool(tool));
        }
        catalog::tools(self.0.policy.source.installation.brand())
            .into_iter()
            .find(|tool| tool.name == name)
            .map(|tool| role_tool(self.0.policy.source.installation.brand(), tool))
    }
    async fn list_tools(
        &self,
        params: Option<PaginatedRequestParams>,
        context: McpRequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if params.is_some_and(|p| p.cursor.is_some()) {
            return Err(ErrorData::invalid_params("unsupported cursor", None));
        }
        let parts = context
            .extensions
            .get::<Parts>()
            .ok_or_else(|| ErrorData::invalid_params("role credential required", None))?;
        let credential = Credential::read(&parts.headers, &self.0.task)
            .map_err(|_| ErrorData::invalid_params("role credential required", None))?;
        let cancel = Cancellation::default();
        let _cancel = CancelOnDrop(cancel.clone());
        if context.ct.is_cancelled() || self.0.stop.is_cancelled() {
            cancel.cancel();
        }
        let original = RequestContext::new(
            self.0.task.content,
            &cancel,
            Instant::now() + self.0.task.timeout,
        )
        .map_err(|_| ErrorData::invalid_params("request expired", None))?;
        if !matches!(&credential.origin, Origin::Role(_)) {
            return Err(ErrorData::invalid_params("role credential required", None));
        }
        let snapshot = self
            .0
            .live_projects
            .snapshot()
            .map_err(|_| ErrorData::invalid_params("Role selection unavailable", None))?;
        let current = snapshot
            .current
            .ok_or_else(|| ErrorData::invalid_params("Role selection unavailable", None))?;
        let selected = header_selection(&parts.headers, &self.0.task)
            .map_err(|_| ErrorData::invalid_params("invalid Role selection", None))?
            .unwrap_or_default()
            .select(&snapshot.projects, current)
            .map_err(|_| ErrorData::invalid_params("Role selection unavailable", None))?;
        let selected = &snapshot.projects[selected];
        require_active(&self.0, selected)
            .map_err(|_| ErrorData::invalid_params("Role selection unavailable", None))?;
        let (paths, id, version) = selected;
        let project =
            CapturedPublicationScope::project(paths, *id, self.0.task.scope, original.snapshot())
                .map_err(|_| ErrorData::invalid_params("project capture failed", None))?;
        let record = project
            .binding()
            .map_err(|_| ErrorData::invalid_params("project binding failed", None))?;
        let scope = ScopeInput {
            installation: self.0.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &project,
                    record: &record,
                },
                version,
            }),
            providers: &[],
            binding: credential.role_binding.unwrap_or(self.0.task.binding),
            global_version: "",
        };
        let Origin::Role(reference) = &credential.origin else {
            return Err(ErrorData::invalid_params("role credential required", None));
        };
        let reference = RoleCredentialReference::decode(
            reference.as_bytes(),
            self.0.task.content.max_input_bytes,
            self.0.task.role_reference,
            &original,
        )
        .map_err(|_| ErrorData::invalid_params("role reference invalid", None))?;
        let grant = self
            .0
            .auth
            .issuer()
            .verify(&credential.bearer, &reference, scope, &original)
            .map_err(|_| ErrorData::invalid_params("role credential denied", None))?;
        let mut tools: Vec<Tool> = catalog::tools(paths.brand())
            .into_iter()
            .filter(|tool| catalog::allowed(paths.brand(), tool.name.as_ref(), &grant))
            .collect();
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        if let Some(now) = &self.0.now {
            tools.extend(now.role_tools(&grant));
        }
        if context.ct.is_cancelled() || self.0.stop.is_cancelled() {
            cancel.cancel();
        }
        original
            .snapshot()
            .check()
            .map_err(|_| ErrorData::invalid_params("request expired", None))?;
        grant
            .actor()
            .map_err(|_| ErrorData::invalid_params("role credential denied", None))?;
        require_active(&self.0, selected)
            .map_err(|_| ErrorData::invalid_params("Role selection unavailable", None))?;
        Ok(ListToolsResult::with_all_items(
            tools
                .into_iter()
                .map(|tool| role_tool(paths.brand(), tool))
                .collect(),
        ))
    }
    async fn call_tool(
        &self,
        mut call: CallToolRequestParams,
        context: McpRequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let raw_hook = catalog::operation(self.0.policy.source.installation.brand(), &call.name)
            == Some(catalog::Operation::SessionHook);
        let brand = self.0.policy.source.installation.brand();
        let write = catalog::task_operation(brand, &call.name)
            .map(|(_, write)| write)
            .or_else(|| catalog::operation(brand, &call.name).map(catalog::Operation::write))
            .unwrap_or(false);
        let format = if raw_hook {
            graphit_core::structured_output::OutputFormat::Json
        } else {
            crate::mcp_output::select_format(&mut call)?
        };
        let incoming = context.ct.clone();
        let result: Result<CallToolResponse, ErrorData> = async {
            let Some(parts) = context.extensions.get::<Parts>() else {
                return Ok(failed("role_credential_required").into());
            };
            let credential = match Credential::read(&parts.headers, &self.0.task) {
                Ok(v) => v,
                Err(e) => return Ok(failed(e).into()),
            };
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            if let Some(now) = &self.0.now
                && now.tool(call.name.as_ref()).is_some()
            {
                if !matches!(&credential.origin, Origin::Role(_)) {
                    return Ok(failed("role_credential_required").into());
                }
                let snapshot = match self.0.live_projects.snapshot() {
                    Ok(snapshot) => snapshot,
                    Err(_) => return Ok(failed("project_selection_unavailable").into()),
                };
                let selected =
                    match header_selection(&parts.headers, &self.0.task).and_then(|value| {
                        let current = snapshot.current.ok_or("project_selection_unavailable")?;
                        value
                            .unwrap_or_default()
                            .select(&snapshot.projects, current)
                            .map(|index| snapshot.projects[index].clone())
                    }) {
                        Ok(value) => value,
                        Err(error) => return Ok(failed(error).into()),
                    };
                drop(snapshot);
                return Ok(now
                    .call(call, credential, context.ct, Some(selected))
                    .await
                    .into());
            }
            let mut arguments = match serde_json::from_value::<Arguments>(Value::Object(
                call.arguments.unwrap_or_default(),
            )) {
                Ok(v) => v,
                Err(_) => return Ok(failed("invalid_arguments").into()),
            };
            let selected = match header_selection(&parts.headers, &self.0.task) {
                Ok(v) => v,
                Err(e) => return Ok(failed(e).into()),
            };
            if arguments.selection.project_dir.is_none()
                && arguments.selection.project_id.is_none()
                && arguments.selection.project_version.is_none()
                && let Some(selected) = selected
            {
                arguments.selection = selected;
            }
            Ok(self
                .call(call.name.into_owned(), arguments, credential, context.ct)
                .await
                .into())
        }
        .await;
        let result = crate::mcp_output::format_response(
            result?,
            format,
            self.0.task.content.max_output_bytes,
        )?;
        let CallToolResponse::Complete(mut result) = result else {
            return Ok(result);
        };
        // A confirmed write is not undone by a cancellation observed after its receipt.
        if !write
            && result.is_error != Some(true)
            && (incoming.is_cancelled() || self.0.stop.is_cancelled())
        {
            let code = if incoming.is_cancelled() {
                "cancelled"
            } else {
                "stopped"
            };
            result = crate::mcp_output::format_result(
                failed(code),
                format,
                self.0.task.content.max_output_bytes,
            )?;
        }
        if raw_hook {
            // Session hooks deliver the exact instruction text expected by the agent adapter.
            result.structured_content = None;
        }
        Ok(result.into())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
