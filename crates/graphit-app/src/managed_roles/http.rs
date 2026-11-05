//! Jobs remain owned until the actual native publisher finishes after EOF/stop.
use super::{InstallRequest, InstallResources, execute_install};
use crate::source_server::SourcePolicy;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use graphit_core::{
    config::{ConfigSnapshot, ConfigSource, ConfigStore},
    file_lock::Cancellation,
    module_settings::ModuleSettings,
    project_identity::ProjectId,
    project_paths::ProjectPaths,
    version_context::VersionContext,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_hub::{
    native::ContentError,
    policy::{RequestContext, RequestError},
};
use std::{io, sync::Arc, time::Instant};
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinSet,
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;
pub const ROUTE: &str = "/admin/roles";
pub const RUN_ROUTE: &str = "/admin/roles/run";
struct RetainedRun {
    credential: Zeroizing<String>,
    execution: crate::managed_role_run::RunExecution,
}
struct HubTransport {
    http: reqwest::Client,
    allow_loopback_http: bool,
    verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
}
struct Shared {
    auth: Arc<DaemonAuth>,
    live_projects: Arc<crate::live_project_selections::LiveProjectSelections>,
    policy: SourcePolicy,
    hub_transport: Option<HubTransport>,
    global_paths: Option<super::global_paths::NativeGlobalPaths>,
    context: Option<(
        ConfigSnapshot,
        graphit_records::memory_store::MemoryStoreReadLimits,
    )>,
    stop: CancellationToken,
    jobs: Mutex<JoinSet<io::Result<()>>>,
    first_failure: std::sync::Mutex<Option<Arc<io::Error>>>,
    retained_runs: Mutex<Vec<RetainedRun>>,
}
impl Shared {
    fn record_failure(&self, error: io::Error) {
        self.first_failure
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_or_insert_with(|| Arc::new(error));
    }

    fn record_job_result(&self, result: Result<io::Result<()>, tokio::task::JoinError>) {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => self.record_failure(error),
            Err(error) => self.record_failure(io::Error::other(error)),
        }
    }
}
#[derive(Clone)]
pub struct ManagedRolesHttp(Arc<Shared>);
impl ManagedRolesHttp {
    pub fn new(
        auth: Arc<DaemonAuth>,
        projects: Arc<Vec<(ProjectPaths, ProjectId, VersionContext)>>,
        current: usize,
        policy: SourcePolicy,
        stop: CancellationToken,
    ) -> Self {
        Self(Arc::new(Shared {
            auth,
            live_projects: Arc::new(crate::live_project_selections::LiveProjectSelections::new(
                projects, current,
            )),
            policy,
            hub_transport: None,
            global_paths: None,
            context: None,
            stop,
            jobs: Mutex::new(JoinSet::new()),
            first_failure: std::sync::Mutex::new(None),
            retained_runs: Mutex::new(Vec::new()),
        }))
    }
    pub(crate) fn attach_global_paths(
        &mut self,
        paths: super::global_paths::NativeGlobalPaths,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0).ok_or_else(|| {
            io::Error::other("native global paths must be captured before serving")
        })?;
        if state.global_paths.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        state.global_paths = Some(paths);
        Ok(())
    }
    pub(crate) fn attach_hub_transport(
        &mut self,
        http: reqwest::Client,
        allow_loopback_http: bool,
        verifiers: Option<Arc<graphit_daemon::broker_verifiers::BrokerVerifierCache>>,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Role transport must be captured before serving"))?;
        if state.hub_transport.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        state.hub_transport = Some(HubTransport {
            http,
            allow_loopback_http,
            verifiers,
        });
        Ok(())
    }
    pub(crate) fn attach_context(
        &mut self,
        config: ConfigSnapshot,
        memory: graphit_records::memory_store::MemoryStoreReadLimits,
        live: Arc<crate::live_project_selections::LiveProjectSelections>,
    ) -> io::Result<()> {
        let state = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("Role context must be captured before serving"))?;
        if config.brand() != state.policy.installation.brand() || state.context.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Role project configuration alignment invalid",
            ));
        }
        state.context = Some((config, memory));
        state.live_projects = live;
        Ok(())
    }
    pub(crate) fn configured_binding(
        &self,
        project: Option<&std::path::Path>,
        original: &RequestContext<'_>,
    ) -> io::Result<graphit_hub::NativeBinding> {
        self.configured_snapshot(project, original)?
            .get::<String>(&graphit_core::config::ConfigKey::new("agent")?)?
            .map(|value| {
                value
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
            })
            .transpose()
            .map(|value| value.unwrap_or(self.0.policy.binding))
    }

    fn configured_snapshot(
        &self,
        project: Option<&std::path::Path>,
        original: &RequestContext<'_>,
    ) -> io::Result<ConfigSnapshot> {
        original.check()?;
        let base = &self
            .0
            .context
            .as_ref()
            .ok_or_else(|| io::Error::other("Role context unavailable"))?
            .0;
        let global = ConfigStore::global(&self.0.policy.installation)?.load()?;
        let snapshot = self.0.live_projects.snapshot()?;
        let project = project
            .map(|root| {
                let paths = snapshot
                    .projects
                    .iter()
                    .find(|(paths, _, _)| paths.root() == root)
                    .ok_or(io::ErrorKind::NotFound)?;
                ConfigStore::project(&paths.0)?.load()
            })
            .transpose()?;
        let layers = [
            ConfigSource::Command,
            ConfigSource::Environment,
            ConfigSource::Distribution,
        ]
        .into_iter()
        .filter_map(|kind| base.layer(kind).cloned())
        .chain([global])
        .chain(project);
        let config = ConfigSnapshot::new(base.brand(), layers)?;
        original.check()?;
        Ok(config)
    }
    /// Called only from an owned blocking Source management operation.
    pub(crate) fn sync_native(
        &self,
        input: &crate::sync_entry::Arguments,
        credential: &str,
        original: &RequestContext<'_>,
    ) -> io::Result<serde_json::Value> {
        original.check()?;
        let (snapshot, modules, docs_dir) =
            self.capture_context(original, input.project_dir.as_deref())?;
        let git_hooks = if let Some(root) = input.project_dir.as_deref() {
            let config = self.configured_snapshot(Some(root), original)?;
            crate::git_hook_commands::configured(&config)?;
            let paths = snapshot
                .projects
                .iter()
                .find(|(paths, _, _)| paths.root() == root)
                .ok_or(io::ErrorKind::NotFound)?;
            let repository = crate::git_records::repository_present(root)?;
            original.check()?;
            if repository {
                let timeout = original
                    .snapshot()
                    .required_deadline()?
                    .saturating_duration_since(std::time::Instant::now());
                crate::git_records::reconcile_hooks(
                    &paths.0,
                    &config,
                    false,
                    original.cancellation(),
                    timeout,
                )?;
                Some(serde_json::json!({"state":"reconciled"}))
            } else {
                Some(serde_json::json!({"state":"non_git"}))
            }
        } else {
            None
        };
        let current = snapshot.current.unwrap_or(0);
        let resources = || -> io::Result<InstallResources<'_>> {
            Ok(InstallResources {
                auth: &self.0.auth,
                global_paths: self.0.global_paths.as_ref(),
                projects: &snapshot.projects,
                current,
                policy: &self.0.policy,
                modules: &modules,
                memory_read: self
                    .0
                    .context
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Role context unavailable"))?
                    .1,
            })
        };
        let native = self.0.hub_transport.as_ref().map(|transport| {
            crate::compiled_skills::remote::NativeSkillTransport {
                http: transport.http.clone(),
                verifiers: transport.verifiers.clone(),
                allow_loopback_http: transport.allow_loopback_http,
                runtime: tokio::runtime::Handle::current(),
            }
        });
        let request = InstallRequest {
            destination: if input.global {
                super::InstallDestination::Global
            } else {
                super::InstallDestination::Project
            },
            project_dir: input.project_dir.clone(),
            project_version: input.project_version.clone(),
            binding: input.agent,
            roles: Vec::new(),
            skills_only: false,
            rules: Vec::new(),
            skill_aliases: Default::default(),
            skill_bundles: Default::default(),
            module_skill: None,
        };
        let roles = execute_install(
            resources()?,
            credential,
            &request,
            docs_dir.as_deref(),
            original,
            native.as_ref(),
        );
        let mut outcome = serde_json::json!({"roles_skills":roles.outcome});
        if let Some(git_hooks) = git_hooks {
            outcome["git_hooks"] = git_hooks;
        }
        if let Some(cause) = roles.cause {
            return Err(
                crate::source_server::file_installation::FileEffectFailure::observed(
                    outcome, cause,
                ),
            );
        }
        if !input.global {
            let source_dir = input
                .coordinator_source
                .clone()
                .map(Ok)
                .unwrap_or_else(|| {
                    std::env::current_exe()?
                        .parent()
                        .map(|path| path.join("graphit-runtime/agents/coordinator"))
                        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
                })?;
            let request = crate::project_coordinator_install::CoordinatorRequest {
                operation: crate::project_hook_install::HookOperation::Install,
                binding: input.agent.unwrap_or(self.0.policy.binding),
                kimi_config_dir: crate::project_hook_install::client::kimi_config_dir(
                    input.agent.unwrap_or(self.0.policy.binding),
                )?,
                source_dir: Some(source_dir),
                project_dir: input
                    .project_dir
                    .clone()
                    .ok_or(io::ErrorKind::InvalidInput)?,
                project_version: input.project_version.clone(),
            };
            let coordinator = crate::project_coordinator_install::execute(
                resources()?,
                credential,
                &request,
                original,
            );
            outcome["coordinator_hooks_mcp"] = serde_json::to_value(coordinator.outcome)?;
            if let Some(cause) = coordinator.cause {
                return Err(
                    crate::source_server::file_installation::FileEffectFailure::observed(
                        outcome, cause,
                    ),
                );
            }
        }
        Ok(outcome)
    }
    pub(crate) fn capture_context(
        &self,
        original: &RequestContext<'_>,
        docs_project: Option<&std::path::Path>,
    ) -> io::Result<(
        Arc<crate::live_project_selections::LiveProjectSnapshot>,
        Vec<ModuleSettings>,
        Option<String>,
    )> {
        let snapshot = self.0.live_projects.snapshot()?;
        let (modules, docs_dir) = Self::modules(&self.0, &snapshot, original, docs_project)?;
        Ok((snapshot, modules, docs_dir))
    }
    fn modules(
        state: &Shared,
        snapshot: &crate::live_project_selections::LiveProjectSnapshot,
        original: &RequestContext<'_>,
        docs_project: Option<&std::path::Path>,
    ) -> io::Result<(Vec<ModuleSettings>, Option<String>)> {
        let base = &state
            .context
            .as_ref()
            .ok_or_else(|| io::Error::other("Role context unavailable"))?
            .0;
        original.snapshot().check()?;
        let global = ConfigStore::global(&state.policy.installation)?.load()?;
        let mut docs_dir = None;
        let modules = snapshot
            .projects
            .iter()
            .map(|(paths, _, _)| {
                original.snapshot().check()?;
                let layers = [
                    ConfigSource::Command,
                    ConfigSource::Environment,
                    ConfigSource::Distribution,
                ]
                .into_iter()
                .filter_map(|kind| base.layer(kind).cloned())
                .chain([global.clone(), ConfigStore::project(paths)?.load()?]);
                let config = ConfigSnapshot::new(base.brand(), layers)?;
                let modules = ModuleSettings::resolve(&config)?;
                if docs_project == Some(paths.root()) {
                    docs_dir = Some(crate::compiled_skills::docs_dir(
                        &config,
                        state.policy.content,
                        original,
                    )?);
                }
                original.snapshot().check()?;
                Ok(modules)
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok((modules, docs_dir))
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route(ROUTE, post(install))
            .route(RUN_ROUTE, post(run))
            .route(crate::project_hook_install::ROUTE, post(install_hooks))
            .route(
                crate::project_coordinator_install::ROUTE,
                post(install_coordinator),
            )
            .layer(DefaultBodyLimit::max(
                self.0.policy.native_install.hub.content().max_input_bytes,
            ))
            .with_state(self.clone())
    }
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        while let Some(result) = jobs.join_next().await {
            self.0.record_job_result(result);
        }
        let mut retained = self.0.retained_runs.lock().await;
        let mut index = 0;
        while index < retained.len() {
            let owned = &mut retained[index];
            let Some(residual) = owned.execution.residual.as_mut() else {
                retained.swap_remove(index);
                continue;
            };
            let (closed, cause) = residual
                .close(
                    &self.0.auth,
                    &owned.credential,
                    self.0.policy.content,
                    self.0.policy.timeout,
                )
                .await;
            if let Some(cause) = cause {
                self.0.record_failure(cause);
            }
            if closed {
                retained.swap_remove(index);
            } else {
                index += 1;
            }
        }
        let failure = {
            let mut first = self
                .0
                .first_failure
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if retained.is_empty() {
                // All jobs and residual owners drained; deliver their error once.
                first.take()
            } else {
                first.clone()
            }
        };
        match failure {
            Some(error) => Err(io::Error::new(error.kind(), error)),
            None => Ok(()),
        }
    }
}
struct CancelOnDrop(Option<Cancellation>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.cancel();
        }
    }
}
enum PublicationKind {
    Roles,
    Hooks,
    Coordinator,
}
enum PublicationRequest {
    Roles(InstallRequest),
    Hooks(crate::project_hook_install::HookRequest),
    Coordinator(crate::project_coordinator_install::CoordinatorRequest),
}
async fn install(state: State<ManagedRolesHttp>, headers: HeaderMap, body: Bytes) -> Response {
    publish(state, headers, body, PublicationKind::Roles).await
}
async fn install_hooks(
    state: State<ManagedRolesHttp>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    publish(state, headers, body, PublicationKind::Hooks).await
}
async fn install_coordinator(
    state: State<ManagedRolesHttp>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    publish(state, headers, body, PublicationKind::Coordinator).await
}
async fn publish(
    State(server): State<ManagedRolesHttp>,
    headers: HeaderMap,
    body: Bytes,
    kind: PublicationKind,
) -> Response {
    let cancel = Cancellation::default();
    let mut guard = CancelOnDrop(Some(cancel.clone()));
    let Some(deadline) = Instant::now().checked_add(server.0.policy.timeout) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if server.0.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if headers.get_all("x-graphit-auth-origin").iter().count() != 1
        || headers
            .get("x-graphit-auth-origin")
            .and_then(|value| value.to_str().ok())
            != Some("runtime")
        || headers.get_all(header::AUTHORIZATION).iter().count() != 1
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| {
            !value.is_empty()
                && value.len() <= server.0.policy.content.max_scalar_bytes
                && !value.bytes().any(|byte| byte.is_ascii_whitespace())
        })
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let credential = Zeroizing::new(raw.to_owned());
    if server.0.context.is_none() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let request = match kind {
        PublicationKind::Roles => {
            serde_json::from_slice::<InstallRequest>(&body).map(PublicationRequest::Roles)
        }
        PublicationKind::Hooks => {
            serde_json::from_slice::<crate::project_hook_install::HookRequest>(&body)
                .map(PublicationRequest::Hooks)
        }
        PublicationKind::Coordinator => {
            serde_json::from_slice::<crate::project_coordinator_install::CoordinatorRequest>(&body)
                .map(PublicationRequest::Coordinator)
        }
    };
    let request = match request {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let (send, receive) = mpsc::channel::<Result<Bytes, io::Error>>(1);
    let owner = Arc::clone(&server.0);
    let mut jobs = owner.jobs.lock().await;
    while let Some(result) = jobs.try_join_next() {
        owner.record_job_result(result);
    }
    if owner.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let state = Arc::clone(&owner);
    jobs.spawn(async move {
        let worker = Arc::clone(&state);
        let worker_cancel = cancel.clone();
        let runtime = tokio::runtime::Handle::current();
        let work = tokio::task::spawn_blocking(move || {
            let original = RequestContext::new(
                worker.policy.native_install.hub.content(),
                &worker_cancel,
                deadline,
            )
            .map_err(|cause| {
                let kind = match &cause {
                    RequestError::Content(ContentError::Cancelled) => io::ErrorKind::Interrupted,
                    RequestError::Content(ContentError::TimedOut) => io::ErrorKind::TimedOut,
                    _ => io::ErrorKind::Other,
                };
                io::Error::new(kind, cause)
            })?;
            let docs_project = match &request {
                PublicationRequest::Roles(request) => request.project_dir.as_deref(),
                _ => None,
            };
            let (snapshot, modules, docs_dir) =
                ManagedRolesHttp(Arc::clone(&worker)).capture_context(&original, docs_project)?;
            let current = match snapshot.current {
                Some(current) => current,
                None if matches!(&request, PublicationRequest::Roles(request) if request.destination == super::InstallDestination::Global && request.project_dir.is_none()) => 0,
                None => return Err(io::ErrorKind::NotFound.into()),
            };
            let resources = InstallResources {
                auth: &worker.auth,
                global_paths: worker.global_paths.as_ref(),
                projects: &snapshot.projects,
                current,
                policy: &worker.policy,
                modules: &modules,
                memory_read: worker
                    .context
                    .as_ref()
                    .expect("capture checked before job")
                    .1,
            };
            let native = worker.hub_transport.as_ref().map(|transport| {
                crate::compiled_skills::remote::NativeSkillTransport {
                    http: transport.http.clone(),
                    verifiers: transport.verifiers.clone(),
                    allow_loopback_http: transport.allow_loopback_http,
                    runtime,
                }
            });
            let (bytes, effects) = match request {
                PublicationRequest::Roles(request) => {
                    let execution = execute_install(
                        resources,
                        &credential,
                        &request,
                        docs_dir.as_deref(),
                        &original,
                        native.as_ref(),
                    );
                    (
                        serde_json::to_vec(&execution.outcome).map_err(io::Error::other)?,
                        !execution.outcome.roles.is_empty() || !execution.outcome.skills.is_empty(),
                    )
                }
                PublicationRequest::Hooks(request) => {
                    let execution = crate::project_hook_install::execute(
                        resources,
                        &credential,
                        &request,
                        &original,
                    );
                    (
                        serde_json::to_vec(&execution.outcome).map_err(io::Error::other)?,
                        !execution.outcome.hooks.is_empty(),
                    )
                }
                PublicationRequest::Coordinator(request) => {
                    let execution = crate::project_coordinator_install::execute(
                        resources,
                        &credential,
                        &request,
                        &original,
                    );
                    (
                        serde_json::to_vec(&execution.outcome).map_err(io::Error::other)?,
                        execution.outcome.publication.is_some()
                            || execution.outcome.mcp.is_some()
                            || !execution.outcome.hooks.is_empty(),
                    )
                }
            };
            if bytes.len() > worker.policy.native_install.hub.content().max_output_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "native publication terminal exceeds output bound",
                ));
            }
            Ok::<_, io::Error>((bytes, effects))
        });
        tokio::pin!(work);
        let mut closed = true;
        let mut stopped = true;
        let result = loop {
            tokio::select! {
                result=&mut work=>break result.map_err(io::Error::other)?,
                ()=send.closed(),if closed=>{cancel.cancel();closed=false;},
                ()=state.stop.cancelled(),if stopped=>{cancel.cancel();stopped=false;},
            }
        };
        let (bytes, effects) = match result {
            // A request may stop before the worker constructs or checks its
            // Original. The worker is already joined, and Source can drain normally.
            Err(error)
                if error.raw_os_error().is_none()
                    && ((error.kind() == io::ErrorKind::Interrupted && cancel.is_cancelled())
                        || (error.kind() == io::ErrorKind::TimedOut && Instant::now() >= deadline)) =>
            {
                return Ok(());
            }
            result => result?,
        };
        if send.send(Ok(Bytes::from(bytes))).await.is_err() && effects {
            eprintln!("managed role publication effects occurred before recipient closed");
        }
        // Work and physical effects were joined before this best-effort delivery.
        Ok(())
    });
    drop(jobs);
    guard.0.take();
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(ReceiverStream::new(receive)),
    )
        .into_response()
}

async fn run(State(server): State<ManagedRolesHttp>, headers: HeaderMap, body: Bytes) -> Response {
    if server.0.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if headers.get_all("x-graphit-auth-origin").iter().count() != 1
        || headers
            .get("x-graphit-auth-origin")
            .and_then(|value| value.to_str().ok())
            != Some("runtime")
        || headers.get_all(header::AUTHORIZATION).iter().count() != 1
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| {
            !value.is_empty()
                && value.len() <= server.0.policy.content.max_scalar_bytes
                && !value.bytes().any(|byte| byte.is_ascii_whitespace())
        })
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let credential = Zeroizing::new(raw.to_owned());
    if server.0.context.is_none() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let request = match serde_json::from_slice::<crate::managed_role_run::RunRequest>(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let cancel = Cancellation::default();
    let mut guard = CancelOnDrop(Some(cancel.clone()));
    let Some(deadline) = Instant::now().checked_add(server.0.policy.timeout) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let (send, receive) = mpsc::channel::<Result<Bytes, io::Error>>(1);
    let owner = Arc::clone(&server.0);
    let mut jobs = owner.jobs.lock().await;
    while let Some(result) = jobs.try_join_next() {
        owner.record_job_result(result);
    }
    if owner.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let state = Arc::clone(&owner);
    jobs.spawn(async move {
        let worker = Arc::clone(&state);
        let worker_cancel = cancel.clone();
        let runtime = tokio::runtime::Handle::current();
        let work = tokio::task::spawn_blocking(move || {
            let original = RequestContext::new(worker.policy.content, &worker_cancel, deadline)
                .map_err(io::Error::other)?;
            let (snapshot, modules, _) = ManagedRolesHttp(Arc::clone(&worker)).capture_context(&original, None)?;
            let current = snapshot.current.ok_or(io::ErrorKind::NotFound)?;
            let selected = super::select_project(&snapshot.projects, current, &request.install)?;
            let index = snapshot.projects.iter().position(|candidate| std::ptr::eq(candidate, selected))
                .ok_or(io::ErrorKind::NotFound)?;
            let projects = [selected.clone()];
            let selected_modules = [modules[index]];
            drop(snapshot);
            drop(modules);
            let execution = runtime.block_on(crate::managed_role_run::execute_run(InstallResources {
                auth: &worker.auth, global_paths: worker.global_paths.as_ref(), projects: &projects, current: 0, policy: &worker.policy,
                modules: &selected_modules,
                memory_read: worker.context.as_ref().expect("capture checked before job").1,
            }, &credential, &request, &original));
            let encoded = serde_json::to_vec(&execution.outcome).map_err(io::Error::other);
            if execution.residual.is_some() {
                worker.retained_runs.blocking_lock().push(RetainedRun { credential, execution });
            }
            let encoded = encoded?;
            if encoded.len() > worker.policy.content.max_output_bytes { return Err(io::ErrorKind::FileTooLarge.into()); }
            Ok::<_, io::Error>(encoded)
        });
        tokio::pin!(work);
        let mut recipient_open = true;
        let mut service_running = true;
        let encoded = loop {
            tokio::select! {
                result = &mut work => break result.map_err(io::Error::other)??,
                () = send.closed(), if recipient_open => { cancel.cancel(); recipient_open=false; }
                () = state.stop.cancelled(), if service_running => { cancel.cancel(); service_running=false; }
            }
        };
        // The recipient may have cancelled while the owned worker was draining.
        let _ = send.send(Ok(Bytes::from(encoded))).await;
        Ok(())
    });
    drop(jobs);
    guard.0.take();
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(ReceiverStream::new(receive)),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
