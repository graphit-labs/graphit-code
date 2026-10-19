//! Authenticated native Task MCP operations and composable HTTP routes.
#![doc = include_str!("../../../docs/specs/rust_task_mcp.md")]
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State as AxumState},
    http::{HeaderMap, StatusCode, request::Parts},
    routing::post,
};
use graphit_auth::{
    AuthError, ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture, ScopeInput,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    file_lock::Cancellation,
    project_identity::ProjectId,
    project_paths::ProjectPaths,
    tcp_listener::BoundListener,
    version_context::VersionContext,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_hub::{
    NativeBinding, ProjectionLimits,
    native::{ContentLimits, RoleCredentialReference},
    policy::RequestContext,
};
use graphit_records::{
    task_protocol::execute_read,
    task_store::{TaskStoreError, TaskStoreReadLimits, TaskStoreWriteLimits},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ErrorCode, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext as McpRequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, sync::oneshot, task::JoinSet};
use tokio_util::sync::CancellationToken;
pub(crate) mod browser;
pub(crate) mod catalog;
mod git_records;
pub(crate) mod request;
pub(crate) mod search;
use request::{Arguments, Credential, Operation, Origin};

type Projects = Vec<(ProjectPaths, ProjectId, VersionContext)>;

/// Existing installation registry and native capture limits for readonly Web imports.
#[derive(Clone)]
pub struct ImportedKnowledgePolicy {
    pub registry: graphit_hub::registry::HubRegistry,
    pub directory: graphit_core::native_document_file::NativeDocumentFileLimits,
}

/// Existing native producer limits and a finite operation deadline selected by the owner.
#[derive(Clone)]
pub struct TaskPolicy {
    pub imported_knowledge: Option<ImportedKnowledgePolicy>,
    pub content: ContentLimits,
    pub timeout: Duration,
    pub scope: ScopeLimits,
    pub read: TaskStoreReadLimits,
    pub write: TaskStoreWriteLimits,
    pub role_reference: ProjectionLimits,
    pub binding: NativeBinding,
}
struct State {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    local_models: std::sync::OnceLock<crate::source_server::LocalModelControlSlot>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    native_http: reqwest::Client,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    native_allow_loopback_http: bool,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    native_verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    live_sessions: Option<Arc<crate::knowledge_live::LiveSessionService>>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    project_configuration: std::sync::RwLock<
        Option<(
            graphit_core::project_paths::InstallationPaths,
            Arc<crate::source_server::project_hook::HookPolicy>,
        )>,
    >,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    ask: std::sync::RwLock<Option<Arc<crate::knowledge_ask_service::AskAvailability>>>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    memory_model: std::sync::RwLock<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>>,
    rescans: std::sync::RwLock<Arc<[graphit_daemon::supervisor::ProjectRescanHandle]>>,
    live_projects: Arc<crate::live_project_selections::LiveProjectSelections>,
    auth: Arc<DaemonAuth>,
    policy: TaskPolicy,
    stop: CancellationToken,
    jobs: tokio::sync::Mutex<JoinSet<()>>,
    #[cfg(test)]
    gate: std::sync::Mutex<Option<(oneshot::Sender<Cancellation>, std::sync::mpsc::Receiver<()>)>>,
}
/// Per-call authentication; native work remains owned until explicitly joined.
#[derive(Clone)]
pub struct TaskServer(Arc<State>);
impl TaskServer {
    /// Empty membership keeps global operations available without a PROJECT selection.
    pub fn new(
        auth: Arc<DaemonAuth>,
        projects: Arc<Projects>,
        current: usize,
        policy: TaskPolicy,
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
                "invalid Task service input",
            ));
        }
        let cancel = Cancellation::default();
        RequestContext::new(policy.content, &cancel, Instant::now() + policy.timeout).map_err(
            |_| io::Error::new(io::ErrorKind::InvalidInput, "invalid Task content policy"),
        )?;
        Ok(Self(Arc::new(State {
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            local_models: std::sync::OnceLock::new(),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            native_http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .map_err(io::Error::other)?,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            native_allow_loopback_http: false,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            native_verifiers: None,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            live_sessions: None,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            project_configuration: std::sync::RwLock::new(None),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            ask: std::sync::RwLock::new(None),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            memory_model: std::sync::RwLock::new(None),
            rescans: std::sync::RwLock::new(Arc::from([])),
            live_projects: Arc::new(crate::live_project_selections::LiveProjectSelections::new(
                Arc::clone(&projects),
                current,
            )),
            auth,
            policy,
            stop,
            jobs: tokio::sync::Mutex::new(JoinSet::new()),
            #[cfg(test)]
            gate: std::sync::Mutex::new(None),
        })))
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn with_native_storage(
        mut self,
        client: reqwest::Client,
        allow_loopback_http: bool,
        verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
    ) -> io::Result<Self> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Task storage transport must attach before serving"))?;
        state.native_http = client;
        state.native_allow_loopback_http = allow_loopback_http;
        state.native_verifiers = verifiers;
        Ok(self)
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn with_live_sessions(
        mut self,
        sessions: Arc<crate::knowledge_live::LiveSessionService>,
    ) -> io::Result<Self> {
        Arc::get_mut(&mut self.0)
            .ok_or(io::ErrorKind::AlreadyExists)?
            .live_sessions = Some(sessions);
        Ok(self)
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn live_sessions(&self) -> Option<&crate::knowledge_live::LiveSessionService> {
        self.0.live_sessions.as_deref()
    }

    pub(crate) fn with_live_projects(
        mut self,
        live: Arc<crate::live_project_selections::LiveProjectSelections>,
    ) -> io::Result<Self> {
        Arc::get_mut(&mut self.0)
            .ok_or(io::ErrorKind::AlreadyExists)?
            .live_projects = live;
        Ok(self)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn attach_local_model_control(
        &self,
        slot: crate::source_server::LocalModelControlSlot,
    ) -> io::Result<()> {
        match self.0.local_models.set(slot) {
            Ok(()) => Ok(()),
            Err(slot)
                if self
                    .0
                    .local_models
                    .get()
                    .is_some_and(|current| Arc::ptr_eq(current, &slot)) =>
            {
                Ok(())
            }
            Err(_) => Err(io::ErrorKind::AlreadyExists.into()),
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn local_model_control(
        &self,
    ) -> io::Result<crate::source_server::LocalModelControlSlot> {
        self.0
            .local_models
            .get()
            .cloned()
            .ok_or_else(|| io::ErrorKind::NotConnected.into())
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn attach_project_configuration(
        &self,
        configuration: (
            graphit_core::project_paths::InstallationPaths,
            Arc<crate::source_server::project_hook::HookPolicy>,
        ),
    ) -> io::Result<()> {
        *self
            .0
            .project_configuration
            .write()
            .map_err(|_| io::Error::other("project configuration lock poisoned"))? =
            Some(configuration);
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn selected_embedding_enabled(
        &self,
        paths: &graphit_core::project_paths::ProjectPaths,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
    ) -> io::Result<bool> {
        let (installation, hook) = self
            .0
            .project_configuration
            .read()
            .map_err(|_| io::Error::other("project configuration lock poisoned"))?
            .clone()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?;
        Ok(hook
            .selected_modules(&installation, paths, original)?
            .enabled(graphit_core::module_settings::Module::Embedding))
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn attach_memory_model(
        &self,
        resources: Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
    ) -> io::Result<()> {
        *self
            .0
            .memory_model
            .write()
            .map_err(|_| io::Error::other("Memory model lock poisoned"))? = resources;
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn attach_ask(
        &self,
        available: Option<Arc<crate::knowledge_ask_service::AskAvailability>>,
    ) -> io::Result<()> {
        *self
            .0
            .ask
            .write()
            .map_err(|_| io::Error::other("Ask state poisoned"))? = available;
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn ask(
        &self,
    ) -> io::Result<Option<Arc<crate::knowledge_ask_service::AskAvailability>>> {
        Ok(self
            .0
            .ask
            .read()
            .map_err(|_| io::Error::other("Ask state poisoned"))?
            .clone())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) async fn retain_browser_ask(
        &self,
        work: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> io::Result<()> {
        let mut jobs = self.0.jobs.lock().await;
        while let Some(result) = jobs.try_join_next() {
            result.map_err(io::Error::other)?;
        }
        if self.0.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        jobs.spawn(work);
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn memory_model(
        &self,
    ) -> io::Result<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>> {
        Ok(self
            .0
            .memory_model
            .read()
            .map_err(|_| io::Error::other("Memory model lock poisoned"))?
            .clone())
    }
    pub(crate) fn attach_rescans(
        &self,
        handles: Arc<[graphit_daemon::supervisor::ProjectRescanHandle]>,
    ) -> io::Result<()> {
        *self
            .0
            .rescans
            .write()
            .map_err(|_| io::Error::other("Task rescans poisoned"))? = handles;
        Ok(())
    }
    fn wake_embeddings(
        state: &State,
        paths: &ProjectPaths,
        id: ProjectId,
        version: &VersionContext,
    ) {
        if let Ok(handles) = state.rescans.read()
            && let Some(handle) = handles.iter().find(|handle| {
                let key = handle.key();
                key.id == id && key.checkout == paths.root() && key.version == version.version()
            })
        {
            handle.request_records();
        }
    }
    /// Stops admission and joins actual native work before DaemonAuth teardown.
    /// Cancelling a shutdown waiter leaves the JoinSet owned by this component.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        let mut failed = false;
        while let Some(result) = jobs.join_next().await {
            failed |= result.is_err();
        }
        if failed {
            Err(io::Error::other("Task native operation failed to join"))
        } else {
            Ok(())
        }
    }
    /// Native Task routes share the Source listener without a second MCP service.
    pub fn native_router(&self) -> Router {
        Router::new()
            .route(crate::git_records::ROUTE, post(git_records::http))
            .route("/api/tasks/native/{operation}", post(http_operation))
            .layer(DefaultBodyLimit::max(self.0.policy.content.max_input_bytes))
            .with_state(self.clone())
    }
    /// Standalone Task service with the same native routes and MCP handler.
    pub fn router(&self) -> Router {
        let factory = self.clone();
        let mut config = StreamableHttpServerConfig::default();
        config.cancellation_token = self.0.stop.clone();
        config.max_request_body_bytes = self.0.policy.content.max_input_bytes;
        let service = StreamableHttpService::new(
            move || Ok(factory.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        self.native_router().nest_service("/mcp", service)
    }
    /// Serves an already reserved loopback listener; shared application routing is external.
    ///
    /// ```no_run
    /// use graphit_app::task_server::TaskServer;
    /// use graphit_core::tcp_listener::BoundListener;
    /// async fn run(server: &TaskServer, listener: BoundListener) -> std::io::Result<()> {
    ///     server.serve_listener(listener).await
    /// }
    /// ```
    pub async fn serve_listener(&self, listener: BoundListener) -> io::Result<()> {
        if !listener.address().ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Task listener must be loopback",
            ));
        }
        let listener = listener.into_listener();
        listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(listener)?;
        let result = axum::serve(listener, self.router())
            .with_graceful_shutdown(self.0.stop.clone().cancelled_owned())
            .await;
        result.and(self.shutdown().await)
    }
    pub(crate) async fn call(
        &self,
        arguments: Arguments,
        operation: &str,
        write: bool,
        credential: Credential,
        transport_cancel: CancellationToken,
    ) -> Result<Value, Failure> {
        let snapshot = self
            .0
            .live_projects
            .snapshot()
            .map_err(|_| Failure::new("selection_unavailable"))?;
        let projects = Arc::clone(&snapshot.projects);
        let current = snapshot
            .current
            .ok_or_else(|| Failure::new("selection_unavailable"))?;
        let selected = if write {
            arguments.selection.select(&projects, current)
        } else {
            arguments.selection.select_read(&projects, current)
        }
        .map_err(Failure::new)?;
        let operation =
            Operation::parse(operation, write, arguments.input).map_err(Failure::new)?;
        let (paths, id, version) = &projects[selected];
        if !self
            .0
            .live_projects
            .is_active(paths, *id, version)
            .map_err(|_| Failure::new("selection_unavailable"))?
        {
            return Err(Failure::new("selection_unavailable"));
        }
        let changes_public_text = matches!(&operation, Operation::Write(input) if matches!(input,
            graphit_records::task_protocol::WriteRequest::CreateTask(_) |
            graphit_records::task_protocol::WriteRequest::ReviseTask(_) |
            graphit_records::task_protocol::WriteRequest::VerifyTaskCheck(_) |
            graphit_records::task_protocol::WriteRequest::SupersedeTaskCheck(_) |
            graphit_records::task_protocol::WriteRequest::AddTaskComment(_)));
        let cancel = Cancellation::default();
        let _guard = CancelOnDrop(cancel.clone());
        let state = Arc::clone(&self.0);
        let deadline = Instant::now()
            .checked_add(state.policy.timeout)
            .ok_or_else(|| Failure::new("deadline"))?;
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = tokio::select! {
                jobs = state.jobs.lock() => jobs,
                () = transport_cancel.cancelled() => return Err(Failure::new("cancelled")),
                () = state.stop.cancelled() => return Err(Failure::new("stopped")),
                () = tokio::time::sleep_until(deadline.into()) => return Err(Failure::new("deadline")),
            };
            while jobs.try_join_next().is_some() {}
            if state.stop.is_cancelled() {
                return Err(Failure::new("stopped"));
            }
            let owner = Arc::clone(&state);
            let original_cancel = cancel.clone();
            let handle = Handle::current();
            // Scope capture, configuration and native stores also perform synchronous I/O.
            // Keep that work off the async scheduler while retaining its owned join handle.
            jobs.spawn_blocking(move || {
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
                    match RequestContext::new(owner.policy.content, &original_cancel, deadline) {
                        Ok(original) => {
                            Box::pin(execute(
                                &owner, &projects, selected, operation, credential, &original,
                            ))
                            .await
                        }
                        Err(_) => Err(Failure::new("request_cancelled_or_expired")),
                    }
                });
                // Publication belongs to the retained native job, even if its
                // transport waiter has disappeared or delivery failed after commit.
                if changes_public_text
                    && (result.is_ok() || result.as_ref().is_err_and(|error| error.committed))
                {
                    let (paths, id, version) = &projects[selected];
                    TaskServer::wake_embeddings(&owner, paths, *id, version);
                }
                let _ = send.send(result);
            });
        }
        // Native delivery checks use the original request. Do not reinterpret an already
        // confirmed write as rolled back merely because the transport was cancelled.
        let result = tokio::select! {
            biased;
            result = receive => result.unwrap_or_else(|_| Err(Failure::new("native_job_failed"))),
            () = transport_cancel.cancelled() => Err(Failure::new("cancelled")),
            () = state.stop.cancelled() => Err(Failure::new("stopped")),
            () = tokio::time::sleep_until(deadline.into()) => Err(Failure::new("deadline")),
        };
        if let Ok(mut jobs) = state.jobs.try_lock() {
            while jobs.try_join_next().is_some() {}
        }
        if result.is_ok() && !write {
            if transport_cancel.is_cancelled() {
                return Err(Failure::new("cancelled"));
            }
            if state.stop.is_cancelled() {
                return Err(Failure::new("stopped"));
            }
            if Instant::now() >= deadline {
                return Err(Failure::new("deadline"));
            }
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

#[derive(Serialize)]
pub(crate) struct Failure {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirmed_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<Box<Failure>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    committed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    index_publication: Option<Box<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_key: Option<Box<graphit_data::lance_branch_store::DeltaRecoveryKey>>,
}
impl Failure {
    fn new(code: &'static str) -> Self {
        Self {
            code,
            confirmed_version: None,
            cause: None,
            committed: false,
            index_publication: None,
            recovery_key: None,
        }
    }
    fn auth(_: AuthError) -> Self {
        Self::new("authentication_or_permission_denied")
    }
    pub(crate) fn store(error: TaskStoreError) -> Self {
        let code = match error {
            TaskStoreError::Confirmed { version, cause } => {
                return Self {
                    code: "write_confirmed",
                    confirmed_version: Some(version),
                    cause: Some(Box::new(Self::store(*cause))),
                    committed: true,
                    index_publication: None,
                    recovery_key: None,
                };
            }
            TaskStoreError::LexicalIndexConfirmed { publication, cause } => {
                return Self {
                    code: "index_confirmed",
                    confirmed_version: Some(publication.generation.version),
                    cause: Some(Box::new(Self::store(*cause))),
                    committed: true,
                    index_publication: Some(Box::new(json!({
                        "generation":publication.generation,
                        "recovery_key":publication.recovery_key,
                    }))),
                    recovery_key: None,
                };
            }
            TaskStoreError::Auth(_) => "authentication_or_permission_denied",
            TaskStoreError::Request(error) => request_failure_code(&error),
            TaskStoreError::Reference(
                graphit_records::reference_projection::ReferenceProjectionError::Request(error),
            ) => request_failure_code(&error),
            TaskStoreError::Reference(_) => "reference_projection_failure",
            TaskStoreError::DifferentRequest => "different_original_request",
            TaskStoreError::WrongAdmission => "wrong_admission",
            TaskStoreError::IncompleteCorpus => "incomplete_corpus",
            TaskStoreError::IdempotencyConflict => "idempotency_conflict",
            TaskStoreError::MissingSession => "session_not_found",
            TaskStoreError::MissingTask => "task_not_found",
            TaskStoreError::InvalidCursor => "invalid_cursor",
            TaskStoreError::AlreadyClaimed => "already_claimed",
            TaskStoreError::LeaseExpired => "lease_expired",
            TaskStoreError::FenceMismatch => "fence_mismatch",
            TaskStoreError::RevisionConflict => "revision_conflict",
            TaskStoreError::CountsUnavailable => "counts_unavailable",
            TaskStoreError::Specification(_) => "invalid_specification",
            TaskStoreError::Gate(_) => "gate_blocked",
            TaskStoreError::Invalid(_) => "invalid_native_input",
            TaskStoreError::Artifact(_) => "artifact_failure",
            TaskStoreError::Arrow(_) => "arrow_failure",
            TaskStoreError::Json(_) => "persisted_json_failure",
            TaskStoreError::Store(_) => "store_failure",
            TaskStoreError::Branch(_) => "branch_failure",
            TaskStoreError::Commit(cause) => {
                let mut failure = Self::new("commit_failure");
                failure.recovery_key = Some(Box::new(cause.recovery_key().clone()));
                return failure;
            }
            TaskStoreError::WriterAcquisition(_) => "writer_acquisition_failure",
            TaskStoreError::Random(_) => "random_source_failure",
            TaskStoreError::VectorDistance(_) => "vector_distance_failure",
        };
        Self::new(code)
    }
    fn status(&self) -> StatusCode {
        match self.code {
            "credential_required" | "credential_origin_required" | "role_reference_required" => {
                StatusCode::UNAUTHORIZED
            }
            "authentication_or_permission_denied" => StatusCode::FORBIDDEN,
            "invalid_arguments"
            | "invalid_selection"
            | "selection_ambiguous"
            | "invalid_cursor" => StatusCode::BAD_REQUEST,
            "selection_unavailable" => StatusCode::NOT_FOUND,
            "cancelled" | "deadline" | "request_cancelled_or_expired" => {
                StatusCode::REQUEST_TIMEOUT
            }
            "stopped" => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
// Storage resolvers retain typed authorization causes inside I/O errors. Public codes
// distinguish those from cancellation without returning credentials or remote messages.
fn request_failure_code(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
            return "request_cancelled_or_expired";
        }
        io::ErrorKind::PermissionDenied => return "authentication_or_permission_denied",
        _ => (),
    }
    let mut cause: Option<&(dyn std::error::Error + 'static)> =
        error.get_ref().map(|error| error as _);
    while let Some(error) = cause {
        if error.is::<AuthError>()
            || error.is::<graphit_auth::profile::ProfileError>()
            || error
                .downcast_ref::<graphit_auth::ServiceUseError>()
                .is_some_and(|error| {
                    !matches!(
                        error,
                        graphit_auth::ServiceUseError::Store(_)
                            | graphit_auth::ServiceUseError::Broker(_)
                    )
                })
        {
            return "authentication_or_permission_denied";
        }
        cause = error.source();
    }
    "store_failure"
}

fn check_live_request(
    state: &State,
    paths: &ProjectPaths,
    id: ProjectId,
    version: &VersionContext,
    original: &RequestContext<'_>,
) -> Result<(), Failure> {
    original
        .check()
        .map_err(|_| Failure::new("request_cancelled_or_expired"))?;
    if state.stop.is_cancelled() {
        return Err(Failure::new("stopped"));
    }
    if !state
        .live_projects
        .is_active(paths, id, version)
        .map_err(|_| Failure::new("selection_unavailable"))?
    {
        return Err(Failure::new("selection_unavailable"));
    }
    Ok(())
}
async fn execute(
    state: &State,
    projects: &Projects,
    selected: usize,
    operation: Operation,
    credential: Credential,
    original: &RequestContext<'_>,
) -> Result<Value, Failure> {
    let (paths, id, version) = &projects[selected];
    check_live_request(state, paths, *id, version, original)?;
    let project =
        CapturedPublicationScope::project(paths, *id, state.policy.scope, original.snapshot())
            .map_err(|_| Failure::new("project_capture_failed"))?;
    let binding = project
        .binding()
        .map_err(|_| Failure::new("project_binding_failed"))?;
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
        binding: state.policy.binding,
        global_version: "",
    };
    let issuer = state.auth.issuer();
    let authority = ProjectRecordAuthority::new(issuer);
    let write = matches!(operation, Operation::Write(_));
    let value = match credential.origin {
        Origin::Role(reference) => {
            let reference = RoleCredentialReference::decode(
                reference.as_bytes(),
                state.policy.content.max_input_bytes,
                state.policy.role_reference,
                original,
            )
            .map_err(|_| Failure::new("invalid_role_reference"))?;
            let grant = issuer
                .verify(&credential.bearer, &reference, scope, original)
                .map_err(Failure::auth)?;
            match operation {
                Operation::Read(input) => {
                    let admission = authority
                        .admit_read(&grant, original, ProjectReadModule::Task)
                        .map_err(Failure::auth)?;
                    check_live_request(state, paths, *id, version, original)?;
                    let semantic = matches!(&input, graphit_records::task_protocol::ReadRequest::Search(query)
                        if query.mode != graphit_records::task_store::TaskSearchMode::Fts);
                    let service = if semantic {
                        Some(issuer.capture_role_service(&grant, graphit_auth::ServicePurpose::Embedding)
                            .map_err(|_| Failure::new("authentication_or_permission_denied"))?)
                    } else { None };
                    bounded_value(
                        execute_task_read(state, paths, input, original, &admission, service).await?,
                        state.policy.content.max_output_bytes,
                    )
                }
                Operation::Write(input) => {
                    let admission = authority
                        .admit_write(&grant, original, input.required_operation())
                        .map_err(Failure::auth)?;
                    check_live_request(state, paths, *id, version, original)?;
                    bounded_value(
                        graphit_records::task_protocol::execute_write(
                            input,
                            original.snapshot(),
                            &admission,
                            state.policy.write,
                        )
                        .await
                        .map_err(Failure::store)?,
                        state.policy.content.max_output_bytes,
                    )
                }
            }
        }
        origin => {
            let caller = match origin {
                Origin::Runtime => issuer.authenticate_runtime(&credential.bearer, original),
                Origin::Local => issuer.authenticate_local(&credential.bearer, original),
                Origin::Role(_) => unreachable!("handled role credential"),
            }
            .map_err(Failure::auth)?;
            match operation {
                Operation::Read(input) => {
                    let admission = authority
                        .admit_authenticated_read(&caller, original, scope, ProjectReadModule::Task)
                        .map_err(Failure::auth)?;
                    check_live_request(state, paths, *id, version, original)?;
                    let semantic = matches!(&input, graphit_records::task_protocol::ReadRequest::Search(query)
                        if query.mode != graphit_records::task_store::TaskSearchMode::Fts);
                    let service = if semantic {
                        Some(issuer.capture_service(&caller, graphit_auth::ServicePurpose::Embedding)
                            .map_err(|_| Failure::new("authentication_or_permission_denied"))?)
                    } else { None };
                    bounded_value(
                        execute_task_read(state, paths, input, original, &admission, service).await?,
                        state.policy.content.max_output_bytes,
                    )
                }
                Operation::Write(input) => {
                    let admission = authority
                        .admit_authenticated_write(
                            &caller,
                            original,
                            scope,
                            input.required_operation(),
                        )
                        .map_err(Failure::auth)?;
                    check_live_request(state, paths, *id, version, original)?;
                    bounded_value(
                        graphit_records::task_protocol::execute_write(
                            input,
                            original.snapshot(),
                            &admission,
                            state.policy.write,
                        )
                        .await
                        .map_err(Failure::store)?,
                        state.policy.content.max_output_bytes,
                    )
                }
            }
        }
    }
    .map_err(|error| {
        let mut failure = Failure::new(
            if error.io_error_kind() == Some(io::ErrorKind::FileTooLarge) {
                "response_limit"
            } else {
                "response_encoding_failed"
            },
        );
        failure.committed = write;
        failure
    })?;
    if !write {
        check_live_request(state, paths, *id, version, original)?;
    }
    Ok(value)
}
async fn execute_task_read(
    state: &State,
    paths: &ProjectPaths,
    input: graphit_records::task_protocol::ReadRequest,
    original: &RequestContext<'_>,
    admission: &graphit_auth::ProjectReadAdmission<'_>,
    mut service: Option<graphit_auth::ServiceSelection<'_>>,
) -> Result<graphit_records::task_protocol::ReadResponse, Failure> {
    if let graphit_records::task_protocol::ReadRequest::Search(query) = input {
        let semantic = query.mode != graphit_records::task_store::TaskSearchMode::Fts;
        let resources = if semantic {
            let (installation, hook) = state
                .project_configuration
                .read()
                .map_err(|_| Failure::new("configuration_unavailable"))?
                .clone()
                .ok_or_else(|| Failure::new("configuration_unavailable"))?;
            if !hook
                .selected_modules(&installation, paths, original.snapshot())
                .map_err(|_| Failure::new("configuration_unavailable"))?
                .enabled(graphit_core::module_settings::Module::Embedding)
            {
                return Err(Failure::new("model_disabled"));
            }
            state
                .memory_model
                .read()
                .map_err(|_| Failure::new("model_unavailable"))?
                .clone()
        } else {
            None
        };
        return search::run(
            original,
            admission,
            query,
            &state.policy,
            std::num::NonZeroUsize::new(state.policy.content.max_input_bytes)
                .ok_or_else(|| Failure::new("invalid_policy"))?,
            resources.as_deref(),
            service.as_mut(),
        )
        .await
        .map(graphit_records::task_protocol::ReadResponse::Search);
    }
    execute_read(
        input,
        original.snapshot(),
        admission,
        state.policy.read,
        std::num::NonZeroUsize::new(state.policy.content.max_input_bytes)
            .ok_or_else(|| Failure::new("invalid_policy"))?,
    )
    .await
    .map_err(Failure::store)
}

/// Apply the existing output bound before constructing a duplicate JSON tree.
pub(crate) fn bounded_value<T: Serialize>(
    native: T,
    maximum: usize,
) -> Result<Value, serde_json::Error> {
    let bytes = browser::codec::encode_response(&native, maximum)?;
    drop(native);
    serde_json::from_slice(&bytes)
}
pub(crate) fn mcp_result(result: Result<Value, Failure>) -> CallToolResult {
    match result {
        Ok(value) => crate::mcp_output::typed(value),
        Err(error) => crate::mcp_output::typed_error(
            serde_json::to_value(error).expect("static failure schema"),
        ),
    }
}
impl ServerHandler for TaskServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        catalog::tools()
            .iter()
            .find(|tool| tool.name == name)
            .cloned()
            .map(crate::mcp_output::decorate_tool)
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: McpRequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(
            catalog::tools()
                .iter()
                .cloned()
                .map(crate::mcp_output::decorate_tool)
                .collect::<Vec<_>>(),
        ))
    }
    async fn call_tool(
        &self,
        mut call: CallToolRequestParams,
        context: McpRequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let format = crate::mcp_output::select_format(&mut call)?;
        let incoming = context.ct.clone();
        let deadline = Instant::now()
            .checked_add(self.0.policy.timeout)
            .ok_or_else(|| ErrorData::internal_error("invalid Task MCP deadline", None))?;
        let finish = |result| {
            crate::mcp_output::format_result(
                mcp_result(result),
                format,
                self.0.policy.content.max_output_bytes,
            )
            .map(Into::into)
        };
        let Some((_, operation, write, _)) = catalog::OPERATIONS
            .iter()
            .find(|(name, _, _, _)| *name == call.name)
        else {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "unknown tool",
                None,
            ));
        };
        let arguments = match serde_json::from_value::<Arguments>(Value::Object(
            call.arguments.unwrap_or_default(),
        )) {
            Ok(arguments) => arguments,
            Err(_) => return finish(Err(Failure::new("invalid_arguments"))),
        };
        let credential = match context
            .extensions
            .get::<Parts>()
            .ok_or("credential_required")
            .and_then(|parts| Credential::read(&parts.headers, &self.0.policy))
        {
            Ok(credential) => credential,
            Err(code) => return finish(Err(Failure::new(code))),
        };
        let response = finish(
            self.call(arguments, operation, *write, credential, context.ct)
                .await,
        )?;
        // Formatting can outlive a read's Original. Confirmed writes retain
        // their native result; late delivery cancellation cannot imply rollback.
        if !*write
            && matches!(&response, CallToolResponse::Complete(result) if result.is_error != Some(true))
        {
            if incoming.is_cancelled() {
                return finish(Err(Failure::new("cancelled")));
            }
            if self.0.stop.is_cancelled() {
                return finish(Err(Failure::new("stopped")));
            }
            if Instant::now() >= deadline {
                return finish(Err(Failure::new("deadline")));
            }
        }
        Ok(response)
    }
}
type HttpResult = Result<Json<Value>, (StatusCode, Json<Value>)>;
async fn http_operation(
    AxumState(server): AxumState<TaskServer>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    Json(arguments): Json<Arguments>,
) -> HttpResult {
    let Some((_, operation, write, _)) = catalog::OPERATIONS
        .iter()
        .find(|(_, name, _, _)| *name == operation)
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error":{"code":"unknown_operation"}})),
        ));
    };
    let result = match Credential::read(&headers, &server.0.policy) {
        Ok(credential) => {
            server
                .call(
                    arguments,
                    operation,
                    *write,
                    credential,
                    CancellationToken::new(),
                )
                .await
        }
        Err(code) => Err(Failure::new(code)),
    };
    result
        .map(Json)
        .map_err(|error| (error.status(), Json(json!({"error":error}))))
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
#[path = "record_storage_tests.rs"]
mod record_storage_tests;
