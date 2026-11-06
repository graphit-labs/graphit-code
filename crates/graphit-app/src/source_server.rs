//! Source MCP transport over the existing authenticated AST composer.
use axum::{Router, http::request::Parts};
use graphit_ast::imported_source::{
    ImportedSourceError, ImportedSourceInput, inspect_imported_source_local,
    inspect_imported_source_runtime,
};
use graphit_ast::source_handler::SourceHandlerError;
use graphit_auth::{ProjectCapture, ScopeCapture, ScopeInput};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, RetainedBytesLimits, ScopeLimits, SnapshotPolicy,
        SourceReadLimits,
    },
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::ProjectId,
    project_paths::{InstallationPaths, ProjectPaths},
    tcp_listener::BoundListener,
    version_context::VersionContext,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_hub::{
    ArtifactId, ConcreteVersion, NativeBinding, native::ContentLimits, policy::RequestContext,
    registry::HubRegistry,
};
use hyper::service::{Service as HyperService, service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder as ConnectionBuilder,
    service::TowerToHyperService,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorCode,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext as McpRequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
#[cfg(test)]
use std::sync::Mutex;
use std::{
    io,
    path::Path,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, sync::oneshot, task::JoinSet};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const TOOL: &str = "graphit_ast_source";
mod entity;
pub(crate) mod now;
pub use now::BrokerRequest;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod authored_resources;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod file_installation;
mod file_publication;
pub(crate) mod hub_catalog;
mod hub_unpublish;
pub(crate) mod knowledge_ask;
mod knowledge_conversation;
mod knowledge_export;
mod knowledge_live;
mod knowledge_log;
mod knowledge_search;
mod knowledge_source;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod local_file_install;
mod memory_index;
mod memory_search;
mod module_skill;
mod native_installation;
mod native_publication;
pub(crate) mod project_hook;
mod remote_hub_catalog;
pub(crate) mod remote_native;
mod semantic;
mod sync;
pub(crate) mod web_authored_resources;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod web_file_mutations;
pub(crate) mod web_file_publication;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod web_file_reads;
pub(crate) mod web_hub_catalog;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod web_hub_contexts;
pub(crate) mod web_hub_unpublish;
pub(crate) mod web_hub_upload;

async fn serve_http_connection<T>(io: T, router: Router, stop: CancellationToken)
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let acknowledgement = Arc::new(OnceLock::new());
    let observed = Arc::clone(&acknowledgement);
    let service = TowerToHyperService::new(router);
    let service = service_fn(move |request| {
        let response = service.call(request);
        let observed = Arc::clone(&observed);
        async move {
            response.await.inspect(|response| {
                if let Some(ack) = response
                    .extensions()
                    .get::<crate::daemon_control::StopAcknowledgement>()
                {
                    let _ = observed.set(ack.deadline);
                }
            })
        }
    });
    let builder = ConnectionBuilder::new(TokioExecutor::new()).http1_only();
    let connection = builder.serve_connection_with_upgrades(TokioIo::new(io), service);
    tokio::pin!(connection);
    tokio::select! {
        biased;
        _ = &mut connection => return,
        () = stop.cancelled() => {},
    }
    if let Some(deadline) = acknowledgement.get().copied() {
        connection.as_mut().graceful_shutdown();
        tokio::select! {
            biased;
            _ = &mut connection => {},
            () = tokio::time::sleep_until(deadline.into()) => {},
        }
    }
}

#[cfg(test)]
mod transport_tests;

/// Existing producer limits and a finite operation deadline selected by the service owner.
#[derive(Clone)]
pub struct SourcePolicy {
    pub graph_query: graphit_ast::icebug_query::QueryOptions,
    pub installation: InstallationPaths,
    pub native_install: crate::native_install_http::NativeInstallPolicy,
    pub content: ContentLimits,
    pub timeout: Duration,
    pub scope: ScopeLimits,
    pub file: SnapshotPolicy,
    pub read: SourceReadLimits,
    pub path: RetainedBytesLimits,
    pub index: NativeDocumentFileLimits,
    pub binding: NativeBinding,
    pub registry: HubRegistry,
}

type Projects = Vec<(ProjectPaths, ProjectId, VersionContext)>;

#[cfg(test)]
type NativeGate = (
    std::sync::mpsc::Sender<Cancellation>,
    std::sync::mpsc::Receiver<()>,
);

pub(crate) type LocalModelControlSlot =
    Arc<std::sync::RwLock<Option<crate::foreground_project_control::ProjectControl>>>;

pub(crate) type SemanticModelSlot =
    Arc<std::sync::RwLock<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>>>;

pub(crate) type HumanActivitySlot = Arc<
    std::sync::RwLock<
        Option<(
            graphit_daemon::supervisor::ProjectContext,
            tokio::sync::watch::Sender<std::time::Instant>,
        )>,
    >,
>;

struct State {
    module_skill: Tool,
    live_sessions: Arc<crate::knowledge_live::LiveSessionService>,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    native_http: reqwest::Client,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    native_allow_loopback_http: bool,
    live_projects: Arc<crate::live_project_selections::LiveProjectSelections>,
    semantic: SemanticModelSlot,
    local_models: LocalModelControlSlot,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    native_verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
    now: Option<now::NowService>,
    hook: Option<Arc<project_hook::HookPolicy>>,
    human_activity: Option<HumanActivitySlot>,
    user_activity: Option<tokio::sync::watch::Sender<Option<std::time::Instant>>>,
    ask: Option<Arc<crate::knowledge_ask_service::AskAvailability>>,
    conversations: Option<crate::knowledge_conversation::ConversationAccess>,
    graph: crate::graph_server::GraphServer,
    admin: crate::admin_auth::AdminAuth,
    install: crate::native_install_http::NativeInstallHttp,
    roles: crate::managed_roles::http::ManagedRolesHttp,
    role_tools: Option<crate::role_tool_server::RoleToolServer>,
    tasks: Option<crate::task_server::TaskServer>,
    auth: Arc<DaemonAuth>,
    policy: SourcePolicy,
    stop: CancellationToken,
    jobs: tokio::sync::Mutex<NativeSourceJobs>,
    #[cfg(test)]
    gate: Mutex<Option<NativeGate>>,
}

// Completed worker failures belong to the service, independent of later requests.
#[derive(Default)]
struct NativeSourceJobs {
    tasks: JoinSet<()>,
    failure: Option<tokio::task::JoinError>,
}
impl NativeSourceJobs {
    fn reap(&mut self) {
        while let Some(completion) = self.tasks.try_join_next() {
            if let Err(error) = completion {
                self.failure.get_or_insert(error);
            }
        }
    }
}

impl State {
    fn semantic_model(
        &self,
    ) -> io::Result<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>> {
        self.semantic
            .read()
            .map(|model| model.clone())
            .map_err(|_| io::Error::other("semantic model state poisoned"))
    }
}

/// Cloneable SDK handler; authentication is performed for each incoming call.
#[derive(Clone)]
pub struct SourceServer(Arc<State>);

impl SourceServer {
    pub(crate) fn daemon_status(&self) -> crate::daemon_control::Reply {
        crate::daemon_control::Reply::status(&self.0.stop)
    }

    pub(crate) fn graph_server(&self) -> crate::graph_server::GraphServer {
        self.0.graph.clone()
    }

    pub(crate) async fn ast_search_web<'a>(
        &'a self,
        selection: &(
            graphit_core::project_paths::ProjectPaths,
            graphit_core::project_identity::ProjectId,
            graphit_core::version_context::VersionContext,
        ),
        input: serde_json::Value,
        caller: &'a graphit_auth::broker::oauth::web::AuthenticatedWebCaller<'a>,
        read: &'a graphit_auth::ProjectReadAdmission<'a>,
        request: &'a RequestContext<'a>,
    ) -> io::Result<serde_json::Value> {
        semantic::execute_web(&self.0, selection, input, caller, read, request).await
    }

    pub(crate) async fn ast_search_native<'a>(
        &'a self,
        selection: &(
            graphit_core::project_paths::ProjectPaths,
            graphit_core::project_identity::ProjectId,
            graphit_core::version_context::VersionContext,
        ),
        input: serde_json::Value,
        caller: &'a graphit_auth::AuthenticatedCaller<'a>,
        read: &'a graphit_auth::ProjectReadAdmission<'a>,
        request: &'a RequestContext<'a>,
    ) -> io::Result<serde_json::Value> {
        semantic::execute_native(&self.0, selection, input, caller, read, request).await
    }

    pub(crate) fn policy(&self) -> &SourcePolicy {
        &self.0.policy
    }

    pub(crate) fn live_projects(
        &self,
    ) -> Arc<crate::live_project_selections::LiveProjectSelections> {
        Arc::clone(&self.0.live_projects)
    }
    pub(crate) fn ast_search_context(
        &self,
    ) -> io::Result<crate::graph_server::search::Context<'_>> {
        Ok(crate::graph_server::search::Context {
            policy: &self.0.policy,
            auth: &self.0.auth,
            config: self.0.graph.project_config()?,
            local_models: &self.0.local_models,
            verifiers: self.0.native_verifiers.as_deref(),
            models: &self.0.semantic,
        })
    }

    pub(crate) fn attach_local_model_control(
        &mut self,
        control: crate::foreground_project_control::ProjectControl,
    ) -> io::Result<()> {
        let mut slot = self
            .0
            .local_models
            .write()
            .map_err(|_| io::Error::other("local model control state poisoned"))?;
        if slot.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        *slot = Some(control);
        Ok(())
    }

    pub(crate) fn semantic_slot(&self) -> SemanticModelSlot {
        Arc::clone(&self.0.semantic)
    }

    pub(crate) fn semantic_model(
        &self,
    ) -> io::Result<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>> {
        self.0.semantic_model()
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn shares_semantic_model(
        &self,
        model: &Arc<crate::foreground_memory_jobs::MemoryJobResources>,
    ) -> bool {
        self.semantic_model()
            .ok()
            .flatten()
            .is_some_and(|shared| Arc::ptr_eq(&shared, model))
    }

    pub(crate) fn project_modules(
        &self,
        original: &RequestContext<'_>,
    ) -> io::Result<Arc<[graphit_core::module_settings::ModuleSettings]>> {
        let hook = self.0.hook.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "project configuration not captured",
            )
        })?;
        let snapshot = self.0.live_projects.snapshot()?;
        snapshot
            .projects
            .iter()
            .map(|(paths, _, _)| {
                hook.selected_modules(&self.0.policy.installation, paths, original.snapshot())
            })
            .collect::<io::Result<Vec<_>>>()
            .map(Arc::from)
    }

    pub(crate) fn attach_semantic_model(
        &mut self,
        model: Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
    ) -> io::Result<()> {
        if model
            .as_ref()
            .is_some_and(|model| !Arc::ptr_eq(&self.0.auth, &model.auth))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if let Some(tasks) = &self.0.tasks {
            tasks.attach_memory_model(model.clone())?;
        }
        *self
            .0
            .semantic
            .write()
            .map_err(|_| io::Error::other("semantic model state poisoned"))? = model;
        Ok(())
    }

    /// Empty PROJECT membership is valid; configure consumers before sharing the handler.
    pub fn new(
        auth: Arc<DaemonAuth>,
        projects: Arc<Projects>,
        current: usize,
        policy: SourcePolicy,
        stop: CancellationToken,
    ) -> io::Result<Self> {
        if (!projects.is_empty() && projects.get(current).is_none())
            || policy.timeout.is_zero()
            || Instant::now().checked_add(policy.timeout).is_none()
            || projects
                .iter()
                .any(|(paths, _, version)| version.scope() != paths.root())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid Source service input",
            ));
        }
        // Validate the existing policy before serving; this context is not retained.
        let cancel = Cancellation::default();
        RequestContext::new(policy.content, &cancel, Instant::now() + policy.timeout).map_err(
            |_| io::Error::new(io::ErrorKind::InvalidInput, "invalid Source content policy"),
        )?;
        let live_projects = Arc::new(crate::live_project_selections::LiveProjectSelections::new(
            Arc::clone(&projects),
            current,
        ));
        let mut graph =
            crate::graph_server::GraphServer::new(auth.clone(), policy.clone(), stop.clone())?;
        graph.attach_projects(
            graphit_core::config::ConfigSnapshot::new(policy.installation.brand(), [])?,
            Arc::clone(&live_projects),
        )?;
        let admin = crate::admin_auth::AdminAuth::new(
            auth.clone(),
            policy.content,
            policy.timeout,
            stop.clone(),
        );
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
            auth.installation().live.owner()
        else {
            return Err(io::Error::other("Source Native installation owner invalid"));
        };
        let native_registry = HubRegistry::global(
            &policy.installation,
            installation_id,
            policy.native_install.hub.hub(),
        )?;
        let install = crate::native_install_http::NativeInstallHttp::new(
            Arc::clone(&auth),
            policy.installation.clone(),
            native_registry,
            policy.native_install.clone(),
            stop.clone(),
        )?;
        let roles = crate::managed_roles::http::ManagedRolesHttp::new(
            Arc::clone(&auth),
            Arc::clone(&projects),
            current,
            policy.clone(),
            stop.clone(),
        );
        Ok(Self(Arc::new(State {
            module_skill: module_skill::definition(policy.installation.brand())?,
            live_sessions: Arc::new(crate::knowledge_live::LiveSessionService::new(
                crate::knowledge_live::HistoryLimits::capture(policy.index, policy.content)?,
            )),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            native_http: graph.native_http(),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            native_allow_loopback_http: false,
            live_projects,
            semantic: Arc::new(std::sync::RwLock::new(None)),
            local_models: Arc::new(std::sync::RwLock::new(None)),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            native_verifiers: None,
            now: None,
            hook: None,
            human_activity: None,
            user_activity: None,
            ask: None,
            conversations: None,
            graph,
            admin,
            install,
            roles,
            role_tools: None,
            tasks: None,
            auth,
            policy,
            stop,
            jobs: tokio::sync::Mutex::new(NativeSourceJobs::default()),
            #[cfg(test)]
            gate: Mutex::new(None),
        })))
    }

    /// Share the daemon's existing discovery/JWKS owner; this grants no access.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub fn attach_native_verifiers(
        &mut self,
        verifiers: Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Native verifiers must be captured before serving"))?;
        if state.native_verifiers.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        state
            .graph
            .attach_native_verifiers(Arc::clone(&verifiers))?;
        state.native_verifiers = Some(verifiers);
        Ok(())
    }

    /// Share the Foreground-owned activity slot before cloning or serving Source.
    pub(crate) fn attach_human_activity(&mut self, activity: HumanActivitySlot) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Human activity must be captured before serving"))?;
        if state.human_activity.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        state.human_activity = Some(activity);
        Ok(())
    }

    /// Receives accepted human observations only; it carries no USER authority.
    pub(crate) fn attach_user_activity(
        &mut self,
        activity: tokio::sync::watch::Sender<Option<std::time::Instant>>,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("USER activity must be captured before serving"))?;
        if state.user_activity.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        state.user_activity = Some(activity);
        Ok(())
    }

    /// Capture native Task policy before sharing the Source handler.
    pub fn attach_tasks(&mut self, policy: crate::task_server::TaskPolicy) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Task must be captured before serving"))?;
        if state.tasks.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let snapshot = state.live_projects.snapshot()?;
        let current = snapshot.current.unwrap_or(0);
        let tasks = crate::task_server::TaskServer::new(
            Arc::clone(&state.auth),
            Arc::clone(&snapshot.projects),
            current,
            policy,
            state.stop.child_token(),
        )?
        .with_live_projects(Arc::clone(&state.live_projects))?;
        let tasks = tasks
            .with_native_storage(
                state.native_http.clone(),
                state.native_allow_loopback_http,
                state.native_verifiers.clone(),
            )?
            .with_live_sessions(Arc::clone(&state.live_sessions))?;
        tasks.attach_local_model_control(Arc::clone(&state.local_models))?;
        if let Some(hook) = &state.hook {
            tasks.attach_project_configuration((
                state.policy.installation.clone(),
                Arc::clone(hook),
            ))?;
        }
        state.tasks = Some(tasks);
        if let Some(tasks) = &state.tasks {
            tasks.attach_ask(state.ask.clone())?;
        }
        Ok(())
    }

    pub(crate) fn task_server(&self) -> io::Result<crate::task_server::TaskServer> {
        self.0
            .tasks
            .clone()
            .ok_or_else(|| io::ErrorKind::NotConnected.into())
    }

    pub(crate) fn now_service(&self) -> Option<now::NowService> {
        self.0.now.clone()
    }

    /// Capture Now policies and the daemon's existing verifier cache before serving.
    pub fn attach_now(
        &mut self,
        verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
        config: &graphit_core::config::ConfigSnapshot,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Now must be captured before serving"))?;
        let snapshot = state.live_projects.snapshot()?;
        let service = now::NowService::new(
            Arc::clone(&state.auth),
            Arc::clone(&snapshot.projects),
            state.policy.clone(),
            config,
            verifiers,
            state.stop.child_token(),
        )?
        .with_live_projects(Arc::clone(&state.live_projects))?;
        if let Some(roles) = state.role_tools.as_mut() {
            roles.attach_now(&service)?;
        }
        state.now = Some(service);
        Ok(())
    }

    pub(crate) fn attach_global_paths(
        &mut self,
        paths: crate::managed_roles::global_paths::NativeGlobalPaths,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0).ok_or_else(|| {
            io::Error::other("native global paths must be captured before serving")
        })?;
        state.roles.attach_global_paths(paths)
    }

    /// Attach the run-owned Role tools using the daemon's actual configuration.
    pub(crate) fn attach_role_tools(
        &mut self,
        config: graphit_core::config::ConfigSnapshot,
    ) -> io::Result<()> {
        self.attach_project_hook(&config)?;
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Role tools must be captured before serving"))?;
        let snapshot = state.live_projects.snapshot()?;
        let current = snapshot.current.unwrap_or(0);
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        {
            state.native_allow_loopback_http = config
                .get::<bool>(&graphit_core::config::ConfigKey::new(
                    "broker.allow_loopback_http",
                )?)?
                .unwrap_or(false);
        }
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        state
            .auth
            .issuer()
            .set_record_storage_resolver(Arc::new(
                crate::record_storage::RecordStorageResolver::new(
                    state.native_http.clone(),
                    state.native_verifiers.clone(),
                    state.native_allow_loopback_http,
                    state.policy.timeout,
                    Arc::downgrade(&state.auth),
                    state.policy.content,
                ),
            ))
            .map_err(io::Error::other)?;
        state.roles.attach_hub_transport(
            state.native_http.clone(),
            state.native_allow_loopback_http,
            state.native_verifiers.clone(),
        )?;
        state
            .graph
            .attach_projects(config.clone(), Arc::clone(&state.live_projects))?;
        let role_tools = crate::role_tool_server::RoleToolServer::new(
            Arc::clone(&state.auth),
            Arc::clone(&snapshot.projects),
            current,
            crate::role_tool_server::RoleToolPolicy::capture(state.policy.clone(), config)?,
            state.stop.child_token(),
        )?
        .with_live_projects(Arc::clone(&state.live_projects))?
        .with_graph_server(state.graph.clone())?;
        let role_tools = role_tools
            .with_semantic_slot(Arc::clone(&state.semantic))?
            .with_local_model_control(Arc::clone(&state.local_models))?
            .with_hub_transport(
                state.native_http.clone(),
                state.native_allow_loopback_http,
                state.native_verifiers.clone(),
            )?;
        state.role_tools = Some(role_tools);
        Ok(())
    }

    /// Publish the current owner's Memory wake hints to the attached Role handler.
    pub(crate) fn attach_memory_rescans(
        &self,
        handles: Arc<[graphit_daemon::supervisor::ProjectRescanHandle]>,
    ) -> io::Result<()> {
        self.0
            .role_tools
            .as_ref()
            .ok_or_else(|| io::Error::other("Memory rescans require attached Role tools"))?
            .attach_memory_rescans(handles)
    }

    pub(crate) fn project_configuration(
        &self,
    ) -> io::Result<(InstallationPaths, Arc<project_hook::HookPolicy>)> {
        Ok((
            self.0.policy.installation.clone(),
            Arc::clone(
                self.0
                    .hook
                    .as_ref()
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?,
            ),
        ))
    }

    fn attach_project_hook(
        &mut self,
        config: &graphit_core::config::ConfigSnapshot,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0).ok_or_else(|| {
            io::Error::other("Hook configuration must be captured before serving")
        })?;
        let snapshot = state.live_projects.snapshot()?;
        let hook = project_hook::HookPolicy::capture(&state.policy, config, &snapshot.projects)?;
        state.roles.attach_context(
            config.clone(),
            hook.memory_limits(),
            Arc::clone(&state.live_projects),
        )?;
        state.hook = Some(Arc::new(hook));
        if let Some(tasks) = &state.tasks {
            tasks.attach_project_configuration((
                state.policy.installation.clone(),
                Arc::clone(state.hook.as_ref().expect("hook attached")),
            ))?;
        }
        Ok(())
    }

    /// Install configured existing clients before cloning or serving the handler.
    pub fn attach_knowledge_conversations(
        &mut self,
        access: crate::knowledge_conversation::ConversationAccess,
    ) -> io::Result<()> {
        if access.providers.is_empty()
            || access.providers.iter().enumerate().any(|(i, (name, _))| {
                name.is_empty()
                    || access.providers[..i]
                        .iter()
                        .any(|(previous, _)| previous == name)
            })
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let state = Arc::get_mut(&mut self.0).ok_or_else(|| {
            io::Error::other("Conversation clients must be captured before serving")
        })?;
        state.conversations = Some(access);
        Ok(())
    }

    /// Capture the selected Ask provider once before cloning/serving this handler.
    /// Failed readiness retains the installed owner for observed shutdown.
    pub async fn attach_ask(
        &mut self,
        config: &graphit_core::config::ConfigSnapshot,
        environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Source Ask must be captured before serving"))?;
        let configured = config.get::<String>(&graphit_core::config::ConfigKey::new("ai.cli")?)?;
        let (provider, program) = crate::knowledge_ask_service::captured_cli(
            state.policy.binding,
            configured.as_deref(),
        )?;
        let available = crate::knowledge_ask_service::AskAvailability::capture(
            provider,
            &program,
            environment,
            // Resolution is installation-scoped. Each admitted operation supplies its own cwd.
            state.policy.installation.root(),
            state.policy.index,
        )?;
        // Install custody before readiness: failed/cancelled startup retains the host.
        state.ask = Some(Arc::new(available));
        if let Some(tasks) = &state.tasks {
            tasks.attach_ask(state.ask.clone())?;
        }
        if let Some(service) = state.ask.as_ref().and_then(|ask| ask.service()) {
            service.ready(original).await?;
        }
        Ok(())
    }

    /// Stops admission and joins running native operations, including blocking FILE reads.
    /// Call before removing the DaemonAuth runtime key or dropping its singleton owner.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        self.0.live_sessions.request_stop();
        let now = match &self.0.now {
            Some(now) => now.shutdown().await,
            None => Ok(()),
        };
        if let Some(service) = self.0.ask.as_ref().and_then(|ask| ask.service()) {
            service.stop();
        }
        let tasks = match &self.0.tasks {
            Some(tasks) => tasks.shutdown().await,
            None => Ok(()),
        };
        let graph = self.0.graph.shutdown().await;
        let admin = self.0.admin.shutdown().await;
        let install = self.0.install.shutdown().await;
        let roles = self.0.roles.shutdown().await;
        let role_tools = match &self.0.role_tools {
            Some(tools) => tools.shutdown().await,
            None => Ok(()),
        };
        // Keep the owned set in State while joining: cancellation of a shutdown
        // waiter must not drop/abort a still-running native job.
        let mut jobs = self.0.jobs.lock().await;
        while let Some(result) = jobs.tasks.join_next().await {
            if let Err(error) = result {
                jobs.failure.get_or_insert(error);
            }
        }
        let ask = match self.0.ask.as_ref().and_then(|ask| ask.service()) {
            Some(service) => service.shutdown().await,
            None => Ok(()),
        };
        let live = if let Some(deadline) = Instant::now().checked_add(self.0.policy.timeout) {
            if self.0.live_sessions.shutdown(deadline).await {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "Live execution or owned process remains pending",
                ))
            }
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        };
        if let Some(error) = jobs.failure.take() {
            Err(io::Error::other(error))
        } else {
            let result = graph
                .and(admin)
                .and(install)
                .and(roles)
                .and(role_tools)
                .and(tasks);
            result.and(now).and(ask).and(live)
        }
    }

    /// Serves the listener already reserved by Core. No bind probe or credential discovery.
    pub async fn serve(&self, listener: BoundListener) -> io::Result<()> {
        self.serve_composed(listener, Router::new(), None).await
    }

    pub(crate) async fn serve_composed(
        &self,
        listener: BoundListener,
        additional: Router,
        ui_url: Option<String>,
    ) -> io::Result<()> {
        if !listener.address().ip().is_loopback() && !listener.address().ip().is_unspecified() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Source listener requires loopback or an explicit wildcard address",
            ));
        }
        let listener = listener.into_listener();
        listener.set_nonblocking(true)?;
        let mut listener = tokio::net::TcpListener::from_std(listener)?;
        let factory = self.clone();
        let mut config = StreamableHttpServerConfig::default();
        config.cancellation_token = self.0.stop.clone();
        config.max_request_body_bytes = self.0.policy.content.max_input_bytes;
        let service = StreamableHttpService::new(
            move || Ok(factory.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        let stop = self.0.stop.clone();
        let router = Router::new()
            .nest_service("/mcp", service)
            .merge(crate::daemon_control::router(
                Arc::clone(&self.0.auth),
                self.0.policy.content,
                self.0.policy.timeout,
                self.0.stop.clone(),
                ui_url,
            ))
            .merge(self.0.admin.router())
            .merge(self.0.install.router())
            .merge(self.0.roles.router())
            .merge(self.0.graph.router())
            .merge(additional);
        let router = match &self.0.tasks {
            Some(tasks) => router.merge(tasks.native_router()),
            None => router,
        };
        let router = match &self.0.role_tools {
            Some(tools) => router.nest("/roles", tools.router()),
            None => router,
        };
        let router = match &self.0.now {
            Some(now) => router.merge(now.router()),
            None => router,
        };
        let mut connections = JoinSet::new();
        let mut result = Ok(());
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => break,
                joined = connections.join_next(), if !connections.is_empty() => {
                    if let Some(Err(error)) = joined {
                        result = Err(io::Error::other(error));
                        break;
                    }
                }
                (stream, _) = axum::serve::Listener::accept(&mut listener) => {
                    connections.spawn(serve_http_connection(stream, router.clone(), stop.clone()));
                }
            }
        }
        drop(listener);
        stop.cancel();
        // HTTP cancellation stops ingress; accepted native jobs retain their owners until joined.
        let joined = self.shutdown().await;
        while let Some(connection) = connections.join_next().await {
            if let Err(error) = connection {
                result = result.and(Err(io::Error::other(error)));
            }
        }
        result.and(joined)
    }

    async fn inspect(
        &self,
        input: Arguments,
        credential: Zeroizing<String>,
        local: bool,
        transport_cancel: CancellationToken,
    ) -> CallToolResult {
        let snapshot = match self.0.live_projects.snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => return failure("Source PROJECT selection unavailable"),
        };
        let projects = Arc::clone(&snapshot.projects);
        let current = snapshot.current.unwrap_or(0);
        if projects.is_empty() && input.context.is_none() {
            return failure("Source PROJECT selection unavailable");
        }
        let selected = match input.select(&projects, current, &self.0.policy.registry) {
            Ok(selected) => selected,
            Err(message) => return failure(message),
        };
        if let Selected::Local(index)
        | Selected::Imported {
            fallback: Some(index),
            ..
        } = &selected
        {
            let (paths, id, version) = &projects[*index];
            if !matches!(
                self.0.live_projects.is_active(paths, *id, version),
                Ok(true)
            ) {
                return failure("Source PROJECT selection unavailable");
            }
        }
        let cancel = Cancellation::default();
        let guard = CancelOnDrop(cancel.clone());
        let original_cancel = cancel.clone();
        let state = Arc::clone(&self.0);
        let handle = Handle::current();
        let deadline = Instant::now() + state.policy.timeout;
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = tokio::select! {
                jobs = state.jobs.lock() => jobs,
                () = transport_cancel.cancelled() => return failure("Source request cancelled"),
                () = state.stop.cancelled() => return failure("Source service stopped"),
                () = tokio::time::sleep_until(deadline.into()) => return failure("Source request deadline expired"),
            };
            jobs.reap();
            if state.stop.is_cancelled() {
                return failure("Source service stopped");
            }
            let owner = Arc::clone(&state);
            jobs.tasks.spawn_blocking(move || {
                #[cfg(test)]
                {
                    let gate = owner.gate.lock().expect("fixture gate").take();
                    if let Some((started, release)) = gate {
                        started
                            .send(original_cancel.clone())
                            .expect("fixture started");
                        release.recv().expect("fixture release");
                    }
                }
                let result = handle.block_on(async {
                    let request =
                        RequestContext::new(owner.policy.content, &original_cancel, deadline)
                            .map_err(|_| ReadFailure::Context)?;
                    read(
                        &owner,
                        &projects,
                        selected,
                        input,
                        &credential,
                        local,
                        &request,
                    )
                    .await
                });
                let _ = send.send(result);
            });
        }
        let result = tokio::select! {
            result = receive => match result {
                Ok(Ok(_)) if transport_cancel.is_cancelled() || cancel.is_cancelled() => failure("Source request cancelled"),
                Ok(Ok(_)) if state.stop.is_cancelled() => failure("Source service stopped"),
                Ok(Ok(_)) if Instant::now() >= deadline => failure("Source request deadline expired"),
                Ok(Ok(source)) => CallToolResult::success(vec![ContentBlock::text(source)]),
                Ok(Err(error)) => failure(error.category()),
                Err(_) => failure("Source native operation failed"),
            },
            () = transport_cancel.cancelled() => failure("Source request cancelled"),
            () = state.stop.cancelled() => failure("Source service stopped"),
            () = tokio::time::sleep_until(deadline.into()) => failure("Source request deadline expired"),
        };
        drop(guard);
        if let Ok(mut jobs) = state.jobs.try_lock() {
            jobs.reap();
        }
        result
    }
}

struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Debug)]
enum ReadFailure {
    Context,
    OutputLimit,
    Capture(io::Error),
    Project(crate::project_graph::ReadError),
    Imported(ImportedSourceError),
    Remote(Box<crate::remote_native_mount::ErrorKind>),
    Entity(graphit_ast::imported_graph::EntitySourceError),
}
impl ReadFailure {
    fn category(&self) -> &'static str {
        match self {
            Self::Remote(error) => match error.as_ref() {
                crate::remote_native_mount::ErrorKind::Auth(_) => {
                    "Source authentication or imported permission denied"
                }
                crate::remote_native_mount::ErrorKind::Service(_)
                | crate::remote_native_mount::ErrorKind::Credentials(_) => {
                    "Source remote service or credentials unavailable"
                }
                _ => "Source remote acquisition or selection failed",
            },
            Self::Project(crate::project_graph::ReadError::Authentication(_)) => {
                "Source authentication or PROJECT permission denied"
            }
            Self::Project(crate::project_graph::ReadError::Source(error)) => source_category(error),
            Self::Project(_) => "Source PROJECT graph or bound text unavailable or invalid",
            Self::Entity(error) => match error {
                graphit_ast::imported_graph::EntitySourceError::Imported(error)
                    if matches!(
                        error.as_ref(),
                        graphit_ast::imported_graph::ImportedGraphError::Authentication(_)
                    ) =>
                {
                    "Source authentication or permission denied"
                }
                graphit_ast::imported_graph::EntitySourceError::Ambiguous => {
                    "Source entity selection is ambiguous"
                }
                graphit_ast::imported_graph::EntitySourceError::UnknownType => {
                    "Source entity type unavailable"
                }
                graphit_ast::imported_graph::EntitySourceError::Unavailable => {
                    "Source entity or bound text unavailable"
                }
                graphit_ast::imported_graph::EntitySourceError::InvalidRequest => {
                    "Source entity request invalid"
                }
                graphit_ast::imported_graph::EntitySourceError::InvalidExtent => {
                    "Source entity extent invalid"
                }
                _ => "Source entity acquisition or selection failed",
            },
            Self::Imported(ImportedSourceError::Authentication(_)) => {
                "Source authentication or permission denied"
            }
            Self::Imported(ImportedSourceError::InstallationNotFound) => {
                "Source imported installation/version unavailable"
            }
            Self::Imported(ImportedSourceError::SourceUnavailable) => {
                "Source imported text unavailable without explicit project fallback"
            }
            Self::Imported(
                ImportedSourceError::InvalidInstallation | ImportedSourceError::InvalidOrigin,
            ) => "Source imported origin invalid",
            Self::Imported(
                ImportedSourceError::InvalidFallback | ImportedSourceError::InvalidPath,
            ) => "Source imported project or path invalid",
            Self::Imported(ImportedSourceError::Storage(_)) => {
                "Source imported storage unavailable or invalid"
            }
            Self::Imported(ImportedSourceError::Indexed(_)) => {
                "Source imported indexed content invalid"
            }
            Self::Imported(ImportedSourceError::Inspection(_)) => {
                "Source imported inspection failed"
            }
            Self::Imported(ImportedSourceError::Operation(_)) => "Source imported operation failed",
            Self::Context => "Source request cancelled or deadline expired",
            Self::OutputLimit => "Source response exceeds configured output limit",
            Self::Capture(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                "Source project access denied"
            }
            Self::Capture(_) => "Source project capture failed",
        }
    }
}

fn source_category(error: &SourceHandlerError) -> &'static str {
    match error {
        SourceHandlerError::Authentication(_) => "Source authentication or permission denied",
        SourceHandlerError::Index(_) => "Source index unavailable or invalid",
        SourceHandlerError::Indexed(_) => "Source indexed content invalid",
        SourceHandlerError::Inspection(_) => "Source inspection failed",
        SourceHandlerError::Operation(_) => "Source operation failed",
        SourceHandlerError::MissingProject | SourceHandlerError::InvalidPath => {
            "Source project or path invalid"
        }
    }
}

async fn read(
    state: &State,
    projects: &Projects,
    selected: Selected,
    input: Arguments,
    credential: &str,
    local: bool,
    request: &RequestContext<'_>,
) -> Result<String, ReadFailure> {
    request
        .snapshot()
        .check()
        .map_err(|_| ReadFailure::Context)?;
    let project_index = match &selected {
        Selected::Local(_) => None,
        Selected::Imported { fallback, .. } if input.source.entity.is_none() => *fallback,
        Selected::Imported { .. } => None,
    };
    let captured = if let Some(index) = project_index {
        let (paths, id, _) = &projects[index];
        let project =
            CapturedPublicationScope::project(paths, *id, state.policy.scope, request.snapshot())
                .map_err(ReadFailure::Capture)?;
        let binding = project.binding().map_err(ReadFailure::Capture)?;
        Some((project, binding))
    } else {
        None
    };
    let project = captured.as_ref().map(|(live, record)| ProjectCapture {
        scope: ScopeCapture { live, record },
        version: &projects[project_index.expect("captured selected project")].2,
    });
    let selection = input.source.selection();
    let result = match &selected {
        Selected::Local(index) => {
            let (paths, id, version) = &projects[*index];
            state
                .graph
                .inspect_source_native(
                    (paths, *id, version),
                    &input.source,
                    credential,
                    local,
                    request,
                )
                .await
                .map_err(ReadFailure::Project)?
        }
        Selected::Imported { id, version, .. } => {
            if let Some(result) = remote_native::inspect_with_auth(
                state,
                id,
                version,
                &input.source,
                project,
                request,
                credential,
                local,
            )
            .await
            .map_err(|error| ReadFailure::Remote(Box::new(error)))?
            {
                if result.selection.source.len() > state.policy.content.max_output_bytes {
                    return Err(ReadFailure::OutputLimit);
                }
                return Ok(result.selection.source);
            }
            if input.source.entity.is_some() {
                let result =
                    entity::inspect(state, id, version, &input, request, credential, local)
                        .await
                        .map_err(ReadFailure::Entity)?;
                if result.selection.source.len() > state.policy.content.max_output_bytes {
                    return Err(ReadFailure::OutputLimit);
                }
                return Ok(result.selection.source);
            }
            let input = ImportedSourceInput {
                registry: &state.policy.registry,
                id,
                version,
                installation: state.auth.installation(),
                fallback: project,
                requested_relative: Path::new(&input.source.path),
                selection: &selection,
                file_policy: &state.policy.file,
                read_limits: state.policy.read,
                path_limits: state.policy.path,
                index_limits: state.policy.index,
                request,
            };
            if local {
                inspect_imported_source_local(state.auth.issuer(), credential, input).await
            } else {
                inspect_imported_source_runtime(state.auth.issuer(), credential, input).await
            }
            .map_err(ReadFailure::Imported)?
        }
    };
    if result.selection.source.len() > state.policy.content.max_output_bytes {
        return Err(ReadFailure::OutputLimit);
    }
    Ok(result.selection.source)
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

type Arguments = crate::ast_source::SourceArguments;

enum Selected {
    Local(usize),
    Imported {
        id: ArtifactId,
        version: ConcreteVersion,
        fallback: Option<usize>,
    },
}
impl Arguments {
    fn select(
        &self,
        projects: &Projects,
        current: usize,
        registry: &HubRegistry,
    ) -> Result<Selected, &'static str> {
        self.source.validate()?;
        if self
            .project_dir
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.contains('\0'))
            || self
                .project_version
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.contains('\0'))
        {
            return Err("Source project selection invalid");
        }
        if let Some(context) = &self.context {
            let (id, version) = context
                .rsplit_once('@')
                .filter(|(id, version)| !id.is_empty() && !version.is_empty())
                .ok_or("Source imported context must be id@version")?;
            let limits = registry.limits().projection();
            let id =
                ArtifactId::new(id, limits).map_err(|_| "Source imported context ID invalid")?;
            let version = ConcreteVersion::new(version, limits)
                .map_err(|_| "Source imported context version invalid")?;
            let fallback = if let Some(root) = self.project_dir.as_deref() {
                Some(select_project(
                    projects,
                    Path::new(root),
                    self.project_version.as_deref(),
                )?)
            } else if self.project_version.is_some() {
                return Err("Source imported fallback requires explicit project_dir");
            } else {
                None
            };
            return Ok(Selected::Imported {
                id,
                version,
                fallback,
            });
        }
        if self.project_dir.is_none() && self.project_version.is_none() {
            return Ok(Selected::Local(current));
        }
        let root = self
            .project_dir
            .as_deref()
            .map(Path::new)
            .unwrap_or(projects[current].0.root());
        if self.project_version.is_none() && root == projects[current].0.root() {
            return Ok(Selected::Local(current));
        }
        self.select_project(projects, root).map(Selected::Local)
    }
    fn select_project(&self, projects: &Projects, root: &Path) -> Result<usize, &'static str> {
        select_read_project(projects, root, self.project_version.as_deref())
    }
}

fn select_project(
    projects: &Projects,
    root: &Path,
    requested_version: Option<&str>,
) -> Result<usize, &'static str> {
    let mut matches = projects
        .iter()
        .enumerate()
        .filter(|(_, (paths, _, version))| {
            paths.root() == root && requested_version.is_none_or(|v| version.version() == v)
        });
    let (index, _) = matches.next().ok_or("Source project/version unavailable")?;
    if matches.next().is_some() {
        return Err("Source project/version selection ambiguous");
    }
    Ok(index)
}

fn select_read_project(
    projects: &Projects,
    root: &Path,
    requested_version: Option<&str>,
) -> Result<usize, &'static str> {
    crate::live_project_selections::select_project_version(projects, root, requested_version)
        .map_err(|code| match code {
            "selection_ambiguous" => "Source project/version selection ambiguous",
            _ => "Source project/version unavailable",
        })
}

fn definition() -> Tool {
    static TOOL_DEFINITION: OnceLock<Tool> = OnceLock::new();
    TOOL_DEFINITION.get_or_init(build_definition).clone()
}

fn build_definition() -> Tool {
    let mut properties = serde_json::Map::new();
    for field in [
        "path",
        "entity",
        "entity_type",
        "pattern",
        "project_dir",
        "project_version",
        "context",
    ] {
        properties.insert(field.into(), json!({"type":"string"}));
    }
    for field in ["head", "tail", "start_line", "end_line", "before", "after"] {
        properties.insert(field.into(), json!({"type":"integer"}));
    }
    for field in ["regex", "line_numbers"] {
        properties.insert(field.into(), json!({"type":"boolean"}));
    }
    Tool::new(TOOL, "Read Source from a retained local project or installed AST context id@version; imported fallback requires explicit project_dir.",
        json!({"type":"object","properties":properties,"required":["path"],"additionalProperties":false})
            .as_object().expect("object literal").clone())
}
fn failure(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

impl ServerHandler for SourceServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        (|| {
            if name == self.0.module_skill.name {
                return Some(self.0.module_skill.clone());
            }
            if let Some(tool) = self.0.tasks.as_ref().and_then(|tasks| tasks.get_tool(name)) {
                return Some(tool);
            }
            if let Some(tool) = self.0.graph.tool(name) {
                return Some(tool);
            }
            if name == semantic::TOOL {
                return Some(semantic::definition());
            }
            if name == memory_search::TOOL {
                return Some(memory_search::definition());
            }
            if name == memory_index::TOOL {
                return Some(memory_index::definition());
            }
            if name == knowledge_log::TOOL {
                return Some(knowledge_log::definition());
            }
            if name == knowledge_search::TOOL {
                return Some(knowledge_search::definition());
            }
            if name == knowledge_source::TOOL {
                return Some(knowledge_source::definition());
            }
            if name == knowledge_export::TOOL {
                return Some(knowledge_export::definition());
            }
            if name == file_publication::TOOL {
                return Some(file_publication::definition());
            }
            if name == native_publication::TOOL {
                return Some(native_publication::definition());
            }
            if name == remote_hub_catalog::TOOL {
                return Some(remote_hub_catalog::definition());
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            if let Some(tool) = authored_resources::definition(name) {
                return Some(tool);
            }
            if name == hub_unpublish::TOOL {
                return Some(hub_unpublish::definition());
            }
            if name == remote_hub_catalog::SEARCH_TOOL {
                return Some(remote_hub_catalog::search_definition());
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            if let Some(tool) = file_installation::definition(name) {
                return Some(tool);
            }
            if name == native_installation::TOOL {
                return Some(native_installation::definition());
            }
            if name == knowledge_live::TOOL {
                return Some(knowledge_live::definition());
            }
            if let Some(tool) = self.0.now.as_ref().and_then(|now| now.tool(name)) {
                return Some(tool);
            }
            if name == project_hook::TOOL && self.0.hook.is_some() {
                return Some(project_hook::definition());
            }
            if name == knowledge_ask::TOOL && knowledge_ask::available(&self.0) {
                return Some(knowledge_ask::definition());
            }
            if name == knowledge_conversation::TOOL && self.0.conversations.is_some() {
                return Some(knowledge_conversation::definition());
            }
            (name == TOOL).then(definition)
        })()
        .map(|tool| {
            if matches!(name, TOOL | project_hook::TOOL) {
                tool
            } else {
                crate::mcp_output::decorate_tool(tool)
            }
        })
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: McpRequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools = vec![definition(), self.0.module_skill.clone()];
        if self.0.tasks.is_some() {
            tools.extend(crate::task_server::catalog::tools().iter().cloned());
        }
        tools.extend(self.0.graph.tools());
        tools.push(semantic::definition());
        tools.push(memory_search::definition());
        tools.push(memory_index::definition());
        tools.push(knowledge_log::definition());
        tools.push(knowledge_search::definition());
        tools.push(knowledge_source::definition());
        tools.push(knowledge_export::definition());
        tools.push(native_publication::definition());
        tools.push(file_publication::definition());
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        tools.extend(file_installation::definitions());
        tools.push(remote_hub_catalog::definition());
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        tools.extend(
            [
                crate::authored_resources::LIST,
                crate::authored_resources::REVIEW,
                crate::authored_resources::CONTENT,
            ]
            .into_iter()
            .filter_map(authored_resources::definition),
        );
        tools.push(hub_unpublish::definition());
        tools.push(remote_hub_catalog::search_definition());
        tools.push(native_installation::definition());
        tools.push(knowledge_live::definition());
        if let Some(now) = &self.0.now {
            tools.extend(now.tools());
        }
        if self.0.hook.is_some() {
            tools.push(project_hook::definition());
        }
        if knowledge_ask::available(&self.0) {
            tools.push(knowledge_ask::definition());
        }
        if self.0.conversations.is_some() {
            tools.push(knowledge_conversation::definition());
        }
        Ok(ListToolsResult::with_all_items(
            tools
                .into_iter()
                .map(|tool| {
                    if matches!(tool.name.as_ref(), TOOL | project_hook::TOOL) {
                        tool
                    } else {
                        crate::mcp_output::decorate_tool(tool)
                    }
                })
                .collect(),
        ))
    }
    async fn call_tool(
        &self,
        mut call: CallToolRequestParams,
        context: McpRequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        // Task owns its MCP boundary and receives the caller's original selection.
        if self
            .0
            .tasks
            .as_ref()
            .is_some_and(|tasks| tasks.get_tool(&call.name).is_some())
        {
            return self.call_unformatted(call, context).await;
        }
        if matches!(call.name.as_ref(), TOOL | project_hook::TOOL) {
            return self.call_unformatted(call, context).await;
        }
        let format = crate::mcp_output::select_format(&mut call)?;
        let read_only = self
            .get_tool(&call.name)
            .and_then(|tool| tool.annotations)
            .and_then(|annotations| annotations.read_only_hint)
            == Some(true);
        let cancel = context.ct.clone();
        let deadline = Instant::now() + self.0.policy.timeout;
        let response = self.call_unformatted(call, context).await?;
        let response = crate::mcp_output::format_response(
            response,
            format,
            self.0.policy.content.max_output_bytes,
        )?;
        if let CallToolResponse::Complete(result) = &response
            && result.is_error != Some(true)
            && read_only
        {
            if cancel.is_cancelled() || self.0.stop.is_cancelled() {
                return Ok(failure("MCP request cancelled or service stopped").into());
            }
            if Instant::now() >= deadline {
                return Ok(failure("MCP request deadline expired").into());
            }
        }
        Ok(response)
    }
}

impl SourceServer {
    async fn call_unformatted(
        &self,
        call: CallToolRequestParams,
        context: McpRequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if call.name == self.0.module_skill.name {
            return Ok(module_skill::call(self, call, context).await.into());
        }
        if let Some(tasks) = &self.0.tasks
            && tasks.get_tool(&call.name).is_some()
        {
            let origin = context
                .extensions
                .get::<Parts>()
                .and_then(|parts| parts.headers.get("x-graphit-auth-origin"))
                .and_then(|origin| origin.to_str().ok());
            if !matches!(origin, Some("runtime" | "local")) {
                return Ok(
                    failure("Root Task requires native Runtime or Local credentials").into(),
                );
            }
            return tasks.call_tool(call, context).await;
        }
        if self.0.graph.tool(&call.name).is_some() {
            let Some(parts) = context.extensions.get::<Parts>() else {
                return Ok(failure("graph incoming HTTP credentials required").into());
            };
            return Ok(self
                .0
                .graph
                .call_mcp(call, parts.headers.clone(), context.ct)
                .await
                .into());
        }
        if call.name == semantic::TOOL {
            let Some(parts) = context.extensions.get::<Parts>() else {
                return Ok(failure("AST search incoming HTTP credentials required").into());
            };
            return Ok(self
                .0
                .graph
                .call_search_mcp(self, call, parts.headers.clone(), context.ct)
                .await
                .into());
        }
        if call.name == memory_search::TOOL {
            return Ok(memory_search::call(self, call, context).await.into());
        }
        if call.name == memory_index::TOOL {
            return Ok(memory_index::call(self, call, context).await.into());
        }
        if let Some(now) = &self.0.now
            && now.tool(call.name.as_ref()).is_some()
        {
            let Some(parts) = context.extensions.get::<Parts>() else {
                return Ok(failure("Now incoming credentials required").into());
            };
            let credential = match now.credential(&parts.headers) {
                Ok(value) => value,
                Err(_) => return Ok(failure("Now credentials required").into()),
            };
            return Ok(now.call(call, credential, context.ct, None).await.into());
        }
        if call.name == project_hook::TOOL {
            return Ok(project_hook::call(self, call, context).await.into());
        }
        if call.name == knowledge_ask::TOOL {
            return Ok(knowledge_ask::call(self, call, context).await.into());
        }
        if call.name == knowledge_log::TOOL {
            return Ok(knowledge_log::call(self, call, context).await.into());
        }
        if call.name == knowledge_search::TOOL {
            return Ok(knowledge_search::call(self, call, context).await.into());
        }
        if call.name == knowledge_source::TOOL {
            return Ok(knowledge_source::call(self, call, context).await.into());
        }
        if call.name == knowledge_export::TOOL {
            return Ok(knowledge_export::call(self, call, context).await.into());
        }
        if call.name == native_publication::TOOL || call.name == file_publication::TOOL {
            return Ok(native_publication::call(self, call, context).await.into());
        }
        if call.name == remote_hub_catalog::TOOL || call.name == remote_hub_catalog::SEARCH_TOOL {
            return Ok(remote_hub_catalog::call(self, call, context).await.into());
        }
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        if matches!(
            call.name.as_ref(),
            crate::authored_resources::LIST
                | crate::authored_resources::REVIEW
                | crate::authored_resources::CONTENT
        ) {
            return Ok(authored_resources::call(self, call, context).await.into());
        }
        if call.name == hub_unpublish::TOOL {
            return Ok(hub_unpublish::call(self, call, context).await.into());
        }
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        if file_installation::handles(call.name.as_ref()) {
            return Ok(file_installation::call(self, call, context).await.into());
        }
        if call.name == native_installation::TOOL {
            return Ok(native_installation::call(self, call, context).await.into());
        }
        if call.name == knowledge_live::TOOL {
            return Ok(knowledge_live::call(
                self,
                Arc::clone(&self.0.live_sessions),
                call,
                context,
            )
            .await
            .into());
        }
        if call.name == knowledge_conversation::TOOL {
            return Ok(knowledge_conversation::call(self, call, context)
                .await
                .into());
        }
        if call.name != TOOL {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "unknown tool",
                None,
            ));
        }
        let input: Arguments =
            match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
                Ok(input) => input,
                Err(_) => return Ok(failure("Source arguments invalid").into()),
            };
        let Some(parts) = context.extensions.get::<Parts>() else {
            return Ok(failure("Source incoming HTTP credentials required").into());
        };
        let local = match parts
            .headers
            .get("x-graphit-auth-origin")
            .and_then(|v| v.to_str().ok())
        {
            Some("runtime") => false,
            Some("local") => true,
            _ => return Ok(failure("Source credential origin missing or unsupported").into()),
        };
        let Some(raw) = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|v| !v.is_empty())
        else {
            return Ok(failure("Source bearer credential required").into());
        };
        let credential = Zeroizing::new(raw.to_owned());
        Ok(self
            .inspect(input, credential, local, context.ct)
            .await
            .into())
    }
}

#[cfg(test)]
pub(crate) fn test_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("loopback fixture HTTP client")
}

#[cfg(test)]
#[path = "../tests/support/knowledge_ask_server.rs"]
pub(crate) mod test_fixture;
#[cfg(test)]
use crate::native_install_http::NativeInstallPolicy;

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_auth::Permission;
    use graphit_core::{brand::Brand, module_settings::Module};
    use std::sync::mpsc;

    fn fixture() -> (tempfile::TempDir, SourceServer, String) {
        fixture_with_permissions(&[Permission::Read(Module::Ast)])
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_shutdown_joins_the_same_source_without_live_work() {
        let (_directory, server, _credential) = fixture();
        let jobs = server.0.jobs.lock().await;
        let mut first = Box::pin(server.shutdown());
        let mut second = Box::pin(server.shutdown());

        // Both shutdowns have started while the real owned drain is held.
        std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(first.as_mut(), context).is_pending());
            assert!(std::future::Future::poll(second.as_mut(), context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(jobs);

        let (first, second) = tokio::join!(first, second);
        first.expect("first shutdown joins the source");
        second.expect("concurrent shutdown observes the same completed source");
    }

    pub(super) fn fixture_with_permissions(
        permissions: &[Permission],
    ) -> (tempfile::TempDir, SourceServer, String) {
        let fixture =
            super::test_fixture::fixture(permissions, Brand::default(), NativeBinding::KiroV3);
        assert!(Arc::ptr_eq(&fixture.auth, &fixture.server.0.auth));
        let snapshot = fixture.server.0.live_projects.snapshot().unwrap();
        assert!(Arc::ptr_eq(&fixture.projects, &snapshot.projects));
        assert_eq!(
            fixture.paths.root(),
            fixture.server.0.policy.installation.root()
        );
        (fixture.temp, fixture.server, fixture.raw)
    }

    fn arguments() -> Arguments {
        serde_json::from_value(json!({"path":"raw"})).unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ask_join_failure_still_drains_the_real_retained_live_child() {
        let mut fixture = super::test_fixture::fixture(
            &[
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ],
            Brand::default(),
            NativeBinding::KiroV3,
        );
        let (owner, native, _native_directory) =
            crate::knowledge_live::tests::native::retained_native_child_for_shutdown(
                &fixture.server.0.live_sessions,
                &fixture,
            )
            .await;
        assert!(owner.spawned());
        assert!(!owner.clean_joined().await);
        let (ask, _ask_directory) =
            crate::knowledge_ask_service::tests::with_terminal_host_join_failure().await;
        Arc::get_mut(&mut fixture.server.0).unwrap().ask = Some(Arc::new(
            crate::knowledge_ask_service::AskAvailability::Available(Box::new(ask)),
        ));

        let error = fixture.server.shutdown().await.unwrap_err();
        let join = error
            .get_ref()
            .unwrap()
            .downcast_ref::<tokio::task::JoinError>()
            .unwrap();
        assert!(join.is_panic());
        assert!(
            error
                .to_string()
                .contains("controlled join failure after actual Host shutdown")
        );
        // This only reads the actual join receipt; it does not observe/join the child.
        assert!(
            owner.clean_joined().await,
            "{}",
            owner.public_observation().await
        );
        fixture.server.shutdown().await.unwrap();
        native.service().unwrap().shutdown().await.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn empty_inventory_composes_before_clone_and_tracks_first_and_last_project() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = super::test_fixture::fixture(
            &[Permission::Read(Module::Ast)],
            Brand::default(),
            NativeBinding::Codex,
        );
        let mut policy = fixture.server.policy().clone();
        let task = crate::foreground_tasks::task_policy(
            &policy.native_install.hub,
            policy.scope,
            policy.index,
            policy.binding,
        )
        .unwrap();
        policy.content = task.content;
        let config =
            graphit_core::config::ConfigSnapshot::new(policy.installation.brand(), []).unwrap();
        let mut server = SourceServer::new(
            Arc::clone(&fixture.auth),
            Arc::new(Vec::new()),
            0,
            policy,
            CancellationToken::new(),
        )
        .unwrap();
        server.attach_role_tools(config.clone()).unwrap();
        server.attach_tasks(task).unwrap();
        server.attach_now(None, &config).unwrap();

        let bin = fixture.paths.root().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let program = bin.join("codex");
        std::fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        server
            .attach_ask(&config, vec![("PATH".into(), "bin".into())], &original)
            .await
            .unwrap();
        assert_eq!(
            server.0.ask.as_ref().unwrap().service().unwrap().program(),
            program
        );
        assert!(server.0.tasks.is_some());
        assert!(server.0.role_tools.is_some());
        assert!(server.0.now.is_some());
        assert!(server.0.hook.is_some());

        let served = server.clone();
        let live = server.live_projects();
        let empty = live.snapshot().unwrap();
        assert!(empty.projects.is_empty());
        assert_eq!(empty.current, None);
        let read = || {
            served.inspect(
                arguments(),
                Zeroizing::new(fixture.raw.clone()),
                false,
                CancellationToken::new(),
            )
        };
        assert_eq!(read().await.is_error, Some(true));
        live.replace(Arc::clone(&fixture.projects), 0).unwrap();
        assert!(empty.projects.is_empty(), "captured roster stays immutable");
        assert_eq!(live.snapshot().unwrap().current, Some(0));
        let result = read().await;
        assert_ne!(result.is_error, Some(true), "{result:?}");
        assert!(format!("{:?}", result.content).contains("actual native payload"));
        let active = live.snapshot().unwrap();
        assert_eq!(
            live.replace(Arc::clone(&fixture.projects), fixture.projects.len())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(Arc::ptr_eq(&active, &live.snapshot().unwrap()));
        let (paths, id, _) = &fixture.projects[0];
        live.retire_checkout(*id, paths.root()).unwrap();
        assert_eq!(live.snapshot().unwrap().current, None);
        assert_eq!(read().await.is_error, Some(true));
        live.replace(Arc::clone(&fixture.projects), 0).unwrap();
        live.replace(Arc::new(Vec::new()), 0).unwrap();
        assert_eq!(live.snapshot().unwrap().current, None);
        assert_eq!(read().await.is_error, Some(true));
        server.shutdown().await.unwrap();
        fixture.server.shutdown().await.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn empty_inventory_preserves_source_and_task_policy_validation() {
        let fixture = super::test_fixture::fixture(
            &[Permission::Read(Module::Ast)],
            Brand::default(),
            NativeBinding::Codex,
        );
        let source = fixture.server.policy().clone();
        let task = crate::foreground_tasks::task_policy(
            &source.native_install.hub,
            source.scope,
            source.index,
            source.binding,
        )
        .unwrap();
        for invalid_content in [false, true] {
            let mut source = source.clone();
            let mut task = task.clone();
            if invalid_content {
                source.content.max_input_bytes = 0;
                task.content.max_input_bytes = 0;
            } else {
                source.timeout = Duration::ZERO;
                task.timeout = Duration::ZERO;
            }
            assert_eq!(
                SourceServer::new(
                    Arc::clone(&fixture.auth),
                    Arc::new(Vec::new()),
                    0,
                    source,
                    CancellationToken::new(),
                )
                .err()
                .unwrap()
                .kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(
                crate::task_server::TaskServer::new(
                    Arc::clone(&fixture.auth),
                    Arc::new(Vec::new()),
                    0,
                    task,
                    CancellationToken::new(),
                )
                .err()
                .unwrap()
                .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert_eq!(
            SourceServer::new(
                Arc::clone(&fixture.auth),
                Arc::clone(&fixture.projects),
                fixture.projects.len(),
                source,
                CancellationToken::new(),
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            crate::task_server::TaskServer::new(
                Arc::clone(&fixture.auth),
                Arc::clone(&fixture.projects),
                fixture.projects.len(),
                task,
                CancellationToken::new(),
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn actual_http_cancel_joins_native_job_and_sibling_reads_real_payload() {
        use rmcp::{
            model::{CallToolRequest, ClientRequest, ProtocolVersion},
            service::{ClientLifecycleMode, ClientServiceExt},
            transport::{
                StreamableHttpClientTransport,
                streamable_http_client::StreamableHttpClientTransportConfig,
            },
        };
        let (_temp, server, raw) = fixture();
        let listener = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &Cancellation::default(),
        )
        .unwrap();
        let uri = format!("http://{}/mcp", listener.address());
        let serving = server.clone();
        let served = tokio::spawn(async move { SourceServer::serve(&serving, listener).await });
        let mut headers = std::collections::HashMap::new();
        let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
        bearer.set_sensitive(true);
        headers.insert(axum::http::header::AUTHORIZATION, bearer);
        headers.insert(
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("runtime"),
        );
        let client = ()
            .serve_with_lifecycle(
                StreamableHttpClientTransport::with_client(
                    crate::source_server::test_http_client(),
                    StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
                ),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::LATEST],
                },
            )
            .await
            .unwrap();
        let (started, observed) = mpsc::channel();
        let (release, waiting) = mpsc::channel();
        *server.0.gate.lock().unwrap() = Some((started, waiting));
        let params = || {
            CallToolRequestParams::new(TOOL)
                .with_arguments(json!({"path":"raw"}).as_object().unwrap().clone())
        };
        let pending = client
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(params())),
                Default::default(),
            )
            .await
            .unwrap();
        let cancelled = tokio::task::spawn_blocking(move || {
            observed.recv_timeout(Duration::from_secs(5)).unwrap()
        })
        .await
        .unwrap();
        pending.cancel(None).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !cancelled.is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let sibling = client.call_tool(params()).await.unwrap();
        assert_eq!(sibling.is_error, Some(false));
        assert_eq!(
            sibling.content[0].as_text().unwrap().text,
            "actual native payload"
        );
        let (disconnect_started, disconnect_observed) = mpsc::channel();
        let (disconnect_release, disconnect_waiting) = mpsc::channel();
        *server.0.gate.lock().unwrap() = Some((disconnect_started, disconnect_waiting));
        let _disconnecting = client
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(params())),
                Default::default(),
            )
            .await
            .unwrap();
        let disconnected = tokio::task::spawn_blocking(move || {
            disconnect_observed
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
        })
        .await
        .unwrap();
        // The current stateless SDK transport closes an unfinished response.
        client.cancel().await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !disconnected.is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let owner = Arc::downgrade(&server.0.auth);
        {
            let shutdown = server.shutdown();
            tokio::pin!(shutdown);
            assert!(futures_util::poll!(&mut shutdown).is_pending());
            assert!(owner.upgrade().is_some());
            // Leaving this scope cancels a polled waiter, preserving the owned JoinSet.
        }
        {
            let shutdown = server.shutdown();
            tokio::pin!(shutdown);
            assert!(futures_util::poll!(&mut shutdown).is_pending());
            release.send(()).unwrap();
            disconnect_release.send(()).unwrap();
            shutdown.await.unwrap();
        }
        served.await.unwrap().unwrap();
        assert!(server.0.jobs.lock().await.tasks.is_empty());
        drop(server);
        assert!(owner.upgrade().is_none());
    }

    #[tokio::test]
    async fn original_deadline_and_service_cancellation_refuse_payload() {
        let (_temp, server, raw) = fixture();
        let (started, observed) = mpsc::channel();
        let (release, waiting) = mpsc::channel();
        *server.0.gate.lock().unwrap() = Some((started, waiting));
        let a = server.clone();
        let credential = raw.clone();
        let task = tokio::spawn(async move {
            a.inspect(
                arguments(),
                Zeroizing::new(credential),
                false,
                CancellationToken::new(),
            )
            .await
        });
        let core = tokio::task::spawn_blocking(move || {
            observed.recv_timeout(Duration::from_secs(5)).unwrap()
        })
        .await
        .unwrap();
        server.0.stop.cancel();
        assert_eq!(task.await.unwrap().is_error, Some(true));
        assert!(core.is_cancelled());
        release.send(()).unwrap();
        server.shutdown().await.unwrap();
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(20);
        let request = RequestContext::new(server.0.policy.content, &cancel, deadline).unwrap();
        tokio::time::sleep_until(deadline.into()).await;
        let result = read(
            &server.0,
            &server.0.live_projects.snapshot().unwrap().projects,
            Selected::Local(0),
            arguments(),
            &raw,
            false,
            &request,
        )
        .await;
        assert!(matches!(result, Err(ReadFailure::Context)));
    }
}

#[cfg(test)]
#[path = "../tests/support/task_root_server.rs"]
mod task_root_server;

#[cfg(all(test, target_os = "linux"))]
pub(crate) fn replace_fixture_graph(
    fixture: &mut test_fixture::Fixture,
    graph: crate::graph_server::GraphServer,
) {
    Arc::get_mut(&mut fixture.server.0)
        .expect("compose before cloning Source")
        .graph = graph;
}

#[cfg(test)]
#[path = "source_server/path_schema_tests.rs"]
mod path_schema_tests;
