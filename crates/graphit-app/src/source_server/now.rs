//! Now handlers retain the existing caller, issuer and subscriber through delivery.
use super::{Projects, SourcePolicy};
use crate::task_server::request::{Credential, Origin};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use graphit_auth::{
    ProjectCapture, ProjectRecordAuthority, ScopeCapture, ScopeInput, ServiceBindingView,
    ServiceCredentialLease, ServiceMode, ServiceSelection, ServiceUseError,
};
use graphit_core::{
    artifact_store::CapturedPublicationScope,
    config::{ConfigKey, ConfigSnapshot},
    file_lock::Cancellation,
    project_identity::ProjectId,
};
use graphit_daemon::{
    auth_startup::DaemonAuth,
    broker_verifiers::{BrokerVerifierCache, BrokerVerifierCacheError},
    now_subscription::{
        NowRemoteCode, NowSubscriptionConfig, NowSubscriptionError, start_now_subscription,
    },
    project_now::{ProjectNowError, ProjectNowLimits, read_project_now, read_project_now_role},
};
use graphit_http::{
    client::{ClientPolicy, Limits},
    websocket::{WebSocketClient, WebSocketLimits},
};
use graphit_hub::{
    native::RoleCredentialReference,
    policy::{RequestContext, ResourceSetting},
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    num::NonZeroUsize,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, mpsc, oneshot},
    task::{JoinError, JoinSet},
};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;

/// Broker selection DATA; the receiving service still authenticates and authorizes it.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerRequest {
    pub project_id: ProjectId,
    pub provider: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub version: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub instance_id: Option<String>,
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    value: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(value).map(Some)
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Local,
    Broker,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    project_id: ProjectId,
    #[serde(default, deserialize_with = "present")]
    mode: Option<Mode>,
    #[serde(default, deserialize_with = "present")]
    version: Option<String>,
    #[serde(default, deserialize_with = "present")]
    provider: Option<String>,
    #[serde(default, deserialize_with = "present")]
    instance_id: Option<String>,
}
impl Arguments {
    fn mode(&self, instances: bool) -> Result<Mode, Failure> {
        let mode = if instances {
            if self.mode.is_some() {
                return Err(Failure::Input);
            }
            Mode::Broker
        } else {
            self.mode.ok_or(Failure::Input)?
        };
        if self
            .version
            .as_ref()
            .is_some_and(|v| v.is_empty() || v.contains('\0'))
            || self
                .provider
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.contains('\0'))
            || self
                .instance_id
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.contains('\0'))
        {
            return Err(Failure::Input);
        }
        match mode {
            Mode::Local
                if self.version.is_none()
                    || self.provider.is_some()
                    || self.instance_id.is_some() =>
            {
                Err(Failure::Input)
            }
            Mode::Broker if self.provider.is_none() => Err(Failure::Input),
            _ => Ok(mode),
        }
    }
}
#[derive(Clone)]
struct Policy {
    heads: ProjectNowLimits,
    network: Limits,
    websocket: WebSocketLimits,
    allow_loopback_http: bool,
}
type ProjectSelection = (
    graphit_core::project_paths::ProjectPaths,
    graphit_core::project_identity::ProjectId,
    graphit_core::version_context::VersionContext,
);

struct Shared {
    live_projects: Arc<crate::live_project_selections::LiveProjectSelections>,
    auth: Arc<DaemonAuth>,
    source: SourcePolicy,
    task: crate::task_server::TaskPolicy,
    policy: Policy,
    verifiers: Option<Arc<BrokerVerifierCache>>,
    tools: [Tool; 2],
    stop: CancellationToken,
    jobs: Mutex<Jobs>,
}
#[derive(Default)]
struct Jobs {
    tasks: JoinSet<()>,
    first_failure: Option<JoinError>,
}
impl Jobs {
    fn record(&mut self, result: Result<(), JoinError>) {
        if let Err(error) = result {
            self.first_failure.get_or_insert(error);
        }
    }
    fn reap(&mut self) {
        while let Some(result) = self.tasks.try_join_next() {
            self.record(result);
        }
    }
}
/// A concrete endpoint owner; credentials and Original live only in its request jobs.
#[derive(Clone)]
pub(crate) struct NowService(Arc<Shared>);
impl NowService {
    pub(crate) fn fork(&self, stop: CancellationToken) -> Self {
        Self(Arc::new(Shared {
            live_projects: Arc::clone(&self.0.live_projects),
            auth: Arc::clone(&self.0.auth),
            source: self.0.source.clone(),
            task: self.0.task.clone(),
            policy: self.0.policy.clone(),
            verifiers: self.0.verifiers.clone(),
            tools: self.0.tools.clone(),
            stop,
            jobs: Mutex::new(Jobs::default()),
        }))
    }
    pub(crate) fn role_tools(&self, role: &graphit_auth::VerifiedRoleGrant<'_>) -> Vec<Tool> {
        use graphit_auth::{ExternalEffect, Permission};
        use graphit_core::module_settings::Module;
        let local = [Module::Task, Module::Memory, Module::Knowledge]
            .into_iter()
            .any(|m| role.authorize(Permission::Read(m)).is_ok());
        let remote = role
            .authorize(Permission::External(ExternalEffect::RemoteRequest))
            .is_ok();
        self.0
            .tools
            .iter()
            .enumerate()
            .filter(|(i, _)| if *i == 0 { local || remote } else { remote })
            .map(|(_, t)| t.clone())
            .collect()
    }
    pub(crate) fn new(
        auth: Arc<DaemonAuth>,
        projects: Arc<Projects>,
        source: SourcePolicy,
        config: &ConfigSnapshot,
        verifiers: Option<Arc<BrokerVerifierCache>>,
        stop: CancellationToken,
    ) -> io::Result<Self> {
        let hub = &source.native_install.hub;
        let task =
            crate::foreground_tasks::task_policy(hub, source.scope, source.index, source.binding)?;
        let scalar =
            |v| NonZeroUsize::new(v).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput));
        let transport = usize::try_from(hub.limit(ResourceSetting::TransportBytes).value)
            .map_err(io::Error::other)?;
        let batch_rows = task.read.batch_rows;
        let content = hub.content();
        let policy = Policy {
            heads: ProjectNowLimits {
                task: task.read,
                memory: graphit_records::memory_store::MemoryHeadReadLimits {
                    batch_rows,
                    native: source.index,
                },
                knowledge: graphit_data::knowledge_index::KnowledgeHeadReadLimits {
                    max_slug_bytes: scalar(content.max_scalar_bytes)?,
                    batch_rows,
                    native: source.index,
                },
            },
            network: Limits {
                request_body_bytes: transport,
                response_body_bytes: transport,
                url_bytes: 8192,
                header_bytes: 65536,
                header_count: 32,
                max_in_flight: usize::try_from(hub.limit(ResourceSetting::TransportInflight).value)
                    .map_err(io::Error::other)?,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(30),
                max_certificates: 8,
                max_certificate_bytes: 65536,
            },
            websocket: WebSocketLimits {
                frame_bytes: scalar(transport)?,
                message_bytes: scalar(transport)?,
                send_bytes: scalar(content.max_input_bytes)?,
            },
            allow_loopback_http: config
                .get::<bool>(&ConfigKey::new("broker.allow_loopback_http")?)?
                .unwrap_or(false),
        };
        let brand = source.installation.brand();
        let common = json!({"project_id":{"type":"string"},"provider":{"type":"string","minLength":1},"version":{"type":"string","minLength":1},"instance_id":{"type":"string","minLength":1}});
        let mut properties = common.clone();
        properties["project_id"]["description"] =
            json!("Explicit canonical PROJECT ID; selectors confer no authority.");
        properties["version"]["description"] =
            json!("Explicit retained version, required for local mode; optional Broker filter.");
        properties["provider"]["description"] =
            json!("Required for broker mode; forbidden for local mode.");
        properties["instance_id"]["description"] =
            json!("Optional Broker instance filter; forbidden for local mode.");
        properties["mode"] = json!({"type":"string","enum":["local","broker"],"description":"Required mode. Local requires version and rejects provider/instance_id; broker requires provider."});
        let tools = [
            Tool::new(brand.mcp_tool_name(&["now"])?, "Read current local heads or the first authorized Broker full snapshot. Local requires an explicit retained version and rejects provider/instance_id; Broker requires a provider.", Arc::new(serde_json::from_value(json!({
                "type":"object",
                "properties":properties,
                "required":["project_id","mode"],
                "additionalProperties":false
            })).map_err(io::Error::other)?)),
            Tool::new(brand.mcp_tool_name(&["instances"])?, "List currently online Broker instances and versions with their full Now heads. Selector DATA confers no access.", Arc::new(serde_json::from_value(json!({"type":"object","properties":common,"required":["project_id","provider"],"additionalProperties":false})).map_err(io::Error::other)?)),
        ];
        Ok(Self(Arc::new(Shared {
            live_projects: Arc::new(crate::live_project_selections::LiveProjectSelections::new(
                Arc::clone(&projects),
                0,
            )),
            auth,
            source,
            task,
            policy,
            verifiers,
            tools,
            stop,
            jobs: Mutex::new(Jobs::default()),
        })))
    }
    pub(crate) fn browser_content(&self) -> graphit_hub::native::ContentLimits {
        self.0.source.content
    }
    pub(crate) fn browser_websocket_limits(&self) -> WebSocketLimits {
        self.0.policy.websocket
    }
    pub(crate) fn browser_stopped(&self) -> &CancellationToken {
        &self.0.stop
    }
    pub(crate) fn browser_client(
        &self,
        verifier: &graphit_auth::broker::BrokerVerifier,
    ) -> io::Result<WebSocketClient> {
        WebSocketClient::new(
            ClientPolicy {
                origin: verifier.endpoint(),
                allow_loopback_http: self.0.policy.allow_loopback_http,
                proxy: None,
                additional_roots: &[],
            },
            self.0.policy.network,
            self.0.policy.websocket,
        )
        .map_err(io::Error::other)
    }
    pub(crate) fn browser_encode(&self, value: &impl Serialize) -> io::Result<Vec<u8>> {
        encode(value, self.0.source.content.max_output_bytes).map_err(io::Error::other)
    }
    pub(crate) fn browser_selection(
        &self,
        selection: &crate::task_server::request::Selection,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
    ) -> io::Result<ProjectSelection> {
        original.check()?;
        if self.0.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let snapshot = self.0.live_projects.snapshot()?;
        let selected = selection
            .select(
                &snapshot.projects,
                snapshot.current.ok_or(io::ErrorKind::NotFound)?,
            )
            .map_err(io::Error::other)?;
        let selected = snapshot
            .projects
            .get(selected)
            .ok_or(io::ErrorKind::NotFound)?;
        let identity =
            graphit_core::project_identity::ProjectIdentityStore::at(&selected.0).load()?;
        if identity.is_none_or(|identity| identity.id() != selected.1) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        original.check()?;
        Ok(selected.clone())
    }
    pub(crate) fn browser_projects(&self) -> io::Result<(Arc<Projects>, Option<usize>)> {
        let snapshot = self.0.live_projects.snapshot()?;
        Ok((Arc::clone(&snapshot.projects), snapshot.current))
    }

    pub(crate) fn tools(&self) -> impl Iterator<Item = Tool> + '_ {
        self.0.tools.iter().cloned()
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
    pub(crate) fn tool(&self, name: &str) -> Option<Tool> {
        self.0.tools.iter().find(|t| t.name == name).cloned()
    }
    pub(crate) fn credential(&self, headers: &HeaderMap) -> Result<Credential, &'static str> {
        if headers.get_all(header::AUTHORIZATION).iter().count() != 1
            || headers.get_all("x-graphit-auth-origin").iter().count() != 1
        {
            return Err("credential_required");
        }
        Credential::read(headers, &self.0.task)
    }
    pub(crate) async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        while let Some(result) = jobs.tasks.join_next().await {
            jobs.record(result);
        }
        match jobs.first_failure.take() {
            Some(error) => Err(io::Error::other(error)),
            None => Ok(()),
        }
    }
    pub(crate) fn router(&self) -> Router {
        Router::new()
            .route("/api/now/subscribe", post(subscribe))
            .layer(DefaultBodyLimit::max(self.0.source.content.max_input_bytes))
            .with_state(self.clone())
    }
    pub(crate) async fn call(
        &self,
        call: CallToolRequestParams,
        credential: Credential,
        transport: CancellationToken,
        role_selection: Option<ProjectSelection>,
    ) -> CallToolResult {
        let instances = call.name == self.0.tools[1].name;
        let input = match serde_json::from_value::<Arguments>(Value::Object(
            call.arguments.unwrap_or_default(),
        )) {
            Ok(v) => v,
            Err(_) => return failure(Failure::Input),
        };
        let mode = match input.mode(instances) {
            Ok(v) => v,
            Err(e) => return failure(e),
        };
        let cancel = Cancellation::default();
        let _guard = CancelOnDrop(cancel.clone());
        let deadline = Instant::now() + self.0.source.timeout;
        let (send, receive) = oneshot::channel();
        let owner = self.clone();
        let worker_cancel = cancel.clone();
        let mut jobs = tokio::select! {
            jobs = self.0.jobs.lock() => jobs,
            () = transport.cancelled() => return failure(Failure::Cancelled),
            () = self.0.stop.cancelled() => return failure(Failure::Unavailable),
            () = tokio::time::sleep_until(deadline.into()) => return failure(Failure::Request(io::ErrorKind::TimedOut.into())),
        };
        jobs.reap();
        if self.0.stop.is_cancelled() {
            return failure(Failure::Unavailable);
        }
        jobs.tasks.spawn(async move {
            let original =
                match RequestContext::new(owner.0.source.content, &worker_cancel, deadline) {
                    Ok(v) => v,
                    Err(_) => {
                        let _ = send.send(Err(Failure::Cancelled));
                        return;
                    }
                };
            let operation =
                owner.execute(&input, mode, &credential, role_selection, &original, None);
            tokio::pin!(operation);
            let result = tokio::select! {
                result = &mut operation => result,
                () = owner.0.stop.cancelled() => { worker_cancel.cancel(); operation.await },
            };
            let _ = send.send(result);
        });
        drop(jobs);
        let result = tokio::select! {
            result = receive => result.unwrap_or(Err(Failure::Unavailable)),
            () = transport.cancelled() => Err(Failure::Cancelled),
            () = self.0.stop.cancelled() => Err(Failure::Unavailable),
            () = tokio::time::sleep_until(deadline.into()) => Err(Failure::Request(io::ErrorKind::TimedOut.into())),
        };
        match result {
            Ok(value)
                if !transport.is_cancelled()
                    && !cancel.is_cancelled()
                    && !self.0.stop.is_cancelled()
                    && Instant::now() < deadline =>
            {
                crate::mcp_output::typed(value)
            }
            Ok(_) if Instant::now() >= deadline => {
                failure(Failure::Request(io::ErrorKind::TimedOut.into()))
            }
            Ok(_) => failure(Failure::Cancelled),
            Err(e) => failure(e),
        }
    }
    async fn execute(
        &self,
        input: &Arguments,
        mode: Mode,
        credential: &Credential,
        role_selection: Option<ProjectSelection>,
        original: &RequestContext<'_>,
        stream: Option<mpsc::Sender<Result<Bytes, Failure>>>,
    ) -> Result<Value, Failure> {
        let issuer = self.0.auth.issuer();
        let snapshot = self.0.live_projects.snapshot().map_err(Failure::Scope)?;
        let projects = &snapshot.projects;
        let selected = if mode == Mode::Local || matches!(credential.origin, Origin::Role(_)) {
            if let Some(previous) = role_selection {
                let index = projects
                    .iter()
                    .position(|(paths, id, version)| {
                        paths.root() == previous.0.root()
                            && *id == previous.1
                            && version == &previous.2
                    })
                    .ok_or(Failure::Denied)?;
                let (_, id, version) = projects.get(index).ok_or(Failure::Input)?;
                if *id != input.project_id
                    || input
                        .version
                        .as_deref()
                        .is_some_and(|v| v != version.version())
                {
                    return Err(Failure::Denied);
                }
                Some(index)
            } else {
                let mut matches = projects.iter().enumerate().filter(|(_, (_, id, version))| {
                    *id == input.project_id
                        && input
                            .version
                            .as_deref()
                            .is_none_or(|v| v == version.version())
                });
                let (index, _) = matches.next().ok_or(Failure::Input)?;
                if matches.next().is_some() {
                    return Err(Failure::Input);
                }
                Some(index)
            }
        } else {
            None
        };
        let captured = selected
            .map(|index| {
                let (paths, id, _) = &projects[index];
                if !self
                    .0
                    .live_projects
                    .is_active(paths, *id, &projects[index].2)
                    .map_err(Failure::Scope)?
                {
                    return Err(Failure::Denied);
                }
                original.check().map_err(Failure::Request)?;
                let identity = graphit_core::project_identity::ProjectIdentityStore::at(paths)
                    .load()
                    .map_err(Failure::Scope)?;
                original.check().map_err(Failure::Request)?;
                if identity.is_none_or(|identity| identity.id() != *id) {
                    return Err(Failure::Denied);
                }
                let live = CapturedPublicationScope::project(
                    paths,
                    *id,
                    self.0.source.scope,
                    original.snapshot(),
                )
                .map_err(Failure::Scope)?;
                let record = live.binding().map_err(Failure::Scope)?;
                Ok::<_, Failure>((live, record))
            })
            .transpose()?;
        let scope = ScopeInput {
            installation: self.0.auth.installation(),
            project: captured
                .as_ref()
                .zip(selected)
                .map(|((live, record), index)| ProjectCapture {
                    scope: ScopeCapture { live, record },
                    version: &projects[index].2,
                }),
            providers: &[],
            binding: credential.role_binding.unwrap_or(self.0.source.binding),
            global_version: "",
        };
        let authority = ProjectRecordAuthority::new(issuer);
        match &credential.origin {
            Origin::Role(encoded) => {
                let reference = RoleCredentialReference::decode(
                    encoded.as_bytes(),
                    self.0.source.content.max_input_bytes,
                    self.0.task.role_reference,
                    original,
                )
                .map_err(|_| Failure::Authentication)?;
                let grant = issuer
                    .verify(&credential.bearer, &reference, scope, original)
                    .map_err(|_| Failure::Authentication)?;
                let result = if mode == Mode::Local {
                    let now = read_project_now_role(
                        &authority,
                        &grant,
                        original,
                        scope,
                        self.0.policy.heads,
                    )
                    .await
                    .map_err(Failure::Local)?;
                    serde_json::to_value(now).map_err(Failure::Encode)?
                } else {
                    let provider = input.provider.as_deref().ok_or(Failure::Input)?;
                    let selection = issuer
                        .capture_role_presence_read_service(&grant, provider)
                        .map_err(Failure::Service)?;
                    self.remote(selection, input, original, stream).await?
                };
                grant.actor().map_err(|_| Failure::Authentication)?;
                original.check().map_err(Failure::Request)?;
                Ok(result)
            }
            Origin::Runtime | Origin::Local => {
                let caller = match credential.origin {
                    Origin::Runtime => issuer.authenticate_runtime(&credential.bearer, original),
                    _ => issuer.authenticate_local(&credential.bearer, original),
                }
                .map_err(|_| Failure::Authentication)?;
                let result = if mode == Mode::Local {
                    let now =
                        read_project_now(&authority, &caller, original, scope, self.0.policy.heads)
                            .await
                            .map_err(Failure::Local)?;
                    serde_json::to_value(now).map_err(Failure::Encode)?
                } else {
                    let provider = input.provider.as_deref().ok_or(Failure::Input)?;
                    let selection = issuer
                        .capture_presence_read_service(&caller, provider)
                        .map_err(Failure::Service)?;
                    self.remote(selection, input, original, stream).await?
                };
                caller.actor().map_err(|_| Failure::Authentication)?;
                original.check().map_err(Failure::Request)?;
                Ok(result)
            }
        }
    }
    async fn remote(
        &self,
        mut selection: ServiceSelection<'_>,
        input: &Arguments,
        original: &RequestContext<'_>,
        stream: Option<mpsc::Sender<Result<Bytes, Failure>>>,
    ) -> Result<Value, Failure> {
        if selection.mode().map_err(Failure::Service)? == ServiceMode::BrokerNative {
            let cache = self.0.verifiers.as_ref().ok_or(Failure::Unavailable)?;
            let verifier = cache.get(&mut selection).map_err(Failure::Verifier)?;
            let lease = selection
                .native_credential(&verifier)
                .await
                .map_err(Failure::Service)?;
            self.receive(&lease, input, original, stream).await
        } else {
            let lease = selection.credential().map_err(Failure::Service)?;
            self.receive(&lease, input, original, stream).await
        }
    }
    async fn receive<'a>(
        &self,
        lease: &ServiceCredentialLease<'a>,
        input: &Arguments,
        original: &RequestContext<'a>,
        stream: Option<mpsc::Sender<Result<Bytes, Failure>>>,
    ) -> Result<Value, Failure> {
        let ServiceBindingView::Broker { endpoint, .. } =
            lease.binding().map_err(Failure::Service)?
        else {
            return Err(Failure::Denied);
        };
        let client = WebSocketClient::new(
            ClientPolicy {
                origin: endpoint,
                allow_loopback_http: self.0.policy.allow_loopback_http,
                proxy: None,
                additional_roots: &[],
            },
            self.0.policy.network,
            self.0.policy.websocket,
        )
        .map_err(|_| Failure::Unavailable)?;
        let config = NowSubscriptionConfig {
            project_id: input.project_id,
            version: input.version.as_deref(),
            instance_id: input.instance_id.as_deref(),
            command_bytes: self.0.policy.websocket.send_bytes,
            snapshot_bytes: self.0.policy.websocket.message_bytes,
        };
        let mut subscriber = start_now_subscription(&client, lease, original, config)
            .await
            .map_err(Failure::Subscription)?;
        let result = async {
            let snapshot = tokio::select! {
                    snapshot = subscriber.next() => snapshot.map_err(Failure::Subscription)?,
                    () = original.cancellation().cancelled() => return Err(Failure::Cancelled),
                    () = self.0.stop.cancelled() => return Err(Failure::Unavailable),
            };
            let Some(send) = &stream else {
                return serde_json::to_value(snapshot).map_err(Failure::Encode);
            };
            let mut pending = Some(snapshot);
            loop {
                // Keep this future until it finishes: next() owns the socket while receiving.
                let next = subscriber.next();
                tokio::pin!(next);
                loop {
                        tokio::select! {
                            biased;
                            () = original.cancellation().cancelled() => return Err(Failure::Cancelled),
                            () = self.0.stop.cancelled() => return Err(Failure::Unavailable),
                            result = send.reserve(), if pending.is_some() => {
                                let permit = result.map_err(|_| Failure::Cancelled)?;
                                original.check().map_err(Failure::Request)?;
                                lease.check().map_err(Failure::Service)?;
                                let mut bytes = encode(&pending, self.0.source.content.max_output_bytes)?;
                                pending.take();
                                bytes.push(b'\n');
                                original.check().map_err(Failure::Request)?;
                                lease.check().map_err(Failure::Service)?;
                                permit.send(Ok(Bytes::from(bytes)));
                            }
                            result = &mut next => {
                                pending = Some(result.map_err(Failure::Subscription)?);
                                break;
                            }
                        }
                }
            }
        }
        .await;
        let closed = subscriber.shutdown().await.map_err(Failure::Subscription);
        result.and_then(|value| {
            closed?;
            original.check().map_err(Failure::Request)?;
            lease.check().map_err(Failure::Service)?;
            Ok(value)
        })
    }
}

enum Failure {
    Input,
    Authentication,
    Denied,
    Cancelled,
    Unavailable,
    Limit,
    Request(io::Error),
    Scope(io::Error),
    Service(ServiceUseError),
    Subscription(NowSubscriptionError),
    Verifier(BrokerVerifierCacheError),
    Local(ProjectNowError),
    Encode(serde_json::Error),
}
impl Failure {
    fn code(&self) -> &'static str {
        match self {
            Self::Input => "now_invalid_arguments",
            Self::Authentication => "now_authentication_denied",
            Self::Denied => "now_permission_denied",
            Self::Cancelled => "now_cancelled",
            Self::Limit => "now_output_limit",
            Self::Request(e) if e.kind() == io::ErrorKind::TimedOut => "now_deadline",
            Self::Request(e) if e.kind() == io::ErrorKind::Interrupted => "now_cancelled",
            Self::Subscription(NowSubscriptionError::Http { status: 401, .. }) => {
                "now_authentication_denied"
            }
            Self::Subscription(NowSubscriptionError::Remote(NowRemoteCode::Unauthenticated)) => {
                "now_authentication_denied"
            }
            Self::Subscription(NowSubscriptionError::Remote(NowRemoteCode::Forbidden)) => {
                "now_permission_denied"
            }
            Self::Subscription(NowSubscriptionError::Remote(NowRemoteCode::Deadline)) => {
                "now_deadline"
            }
            Self::Subscription(NowSubscriptionError::Http { status: 403, .. }) => {
                "now_permission_denied"
            }
            Self::Subscription(NowSubscriptionError::Http { status: 504, .. }) => "now_deadline",
            Self::Service(ServiceUseError::Auth(_)) => "now_permission_denied",
            Self::Service(_) => "now_service_unavailable",
            Self::Subscription(_) => "now_subscription_failed",
            Self::Verifier(_) => "now_verifier_unavailable",
            Self::Local(_) => "now_local_unavailable",
            Self::Scope(_) => "now_scope_unavailable",
            Self::Encode(_) => "now_encoding_failed",
            Self::Request(_) | Self::Unavailable => "now_unavailable",
        }
    }
    fn status(&self) -> StatusCode {
        match self {
            Self::Input | Self::Subscription(NowSubscriptionError::InvalidInput) => {
                StatusCode::BAD_REQUEST
            }
            Self::Authentication => StatusCode::UNAUTHORIZED,
            Self::Subscription(NowSubscriptionError::Remote(NowRemoteCode::Unauthenticated)) => {
                StatusCode::UNAUTHORIZED
            }
            Self::Subscription(NowSubscriptionError::Remote(NowRemoteCode::Forbidden)) => {
                StatusCode::FORBIDDEN
            }
            Self::Subscription(NowSubscriptionError::Remote(NowRemoteCode::Deadline)) => {
                StatusCode::GATEWAY_TIMEOUT
            }
            Self::Denied | Self::Service(ServiceUseError::Auth(_)) => StatusCode::FORBIDDEN,
            Self::Subscription(NowSubscriptionError::Http { status, .. }) => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
            }
            _ => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}
fn failure(error: Failure) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(error.code())])
}
struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut output = LimitedBytes {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut output, value).map_err(|e| {
        if e.io_error_kind() == Some(io::ErrorKind::FileTooLarge) {
            Failure::Limit
        } else {
            Failure::Encode(e)
        }
    })?;
    Ok(output.bytes)
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
struct ResponseStream {
    first: Option<Bytes>,
    lines: ReceiverStream<Result<Bytes, Failure>>,
    _guard: CancelOnDrop,
    ended: bool,
}
impl Stream for ResponseStream {
    type Item = io::Result<Bytes>;
    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if self._guard.0.is_cancelled() {
            self.first.take();
            self.lines.close();
            self.ended = true;
            return Poll::Ready(Some(Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "now_stream_ended",
            ))));
        }
        if let Some(first) = self.first.take() {
            return Poll::Ready(Some(Ok(first)));
        }
        Pin::new(&mut self.lines)
            .poll_next(context)
            .map(|item| item.map(|result| result.map_err(|error| io::Error::other(error.code()))))
    }
}
async fn subscribe(State(service): State<NowService>, headers: HeaderMap, body: Bytes) -> Response {
    if service.0.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if headers.contains_key(header::ORIGIN) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<axum::http::uri::Authority>().ok());
    if headers.get_all(header::HOST).iter().count() != 1
        || host.as_ref().is_none_or(|host| {
            host.host() != "localhost"
                && !host
                    .host()
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let credential = match service.credential(&headers) {
        Ok(v) => v,
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let input = match serde_json::from_slice::<BrokerRequest>(&body) {
        Ok(v) => Arguments {
            project_id: v.project_id,
            mode: Some(Mode::Broker),
            provider: Some(v.provider),
            version: v.version,
            instance_id: v.instance_id,
        },
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if input.mode(false).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let cancel = Cancellation::default();
    let guard = CancelOnDrop(cancel.clone());
    let (send, mut receive) = mpsc::channel(1);
    let owner = service.clone();
    let mut jobs = service.0.jobs.lock().await;
    jobs.reap();
    if service.0.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    jobs.tasks.spawn(async move {
        let original = RequestContext::service(owner.0.source.content, &cancel);
        let result = match original {
            Ok(original) => {
                let operation = owner.execute(
                    &input,
                    Mode::Broker,
                    &credential,
                    None,
                    &original,
                    Some(send.clone()),
                );
                tokio::pin!(operation);
                tokio::select! {
                    result = &mut operation => result,
                    () = owner.0.stop.cancelled() => { cancel.cancel(); operation.await },
                }
            }
            Err(_) => Err(Failure::Cancelled),
        };
        if let Err(error) = result {
            let _ = send.try_send(Err(error));
            cancel.cancel();
        }
    });
    drop(jobs);
    let first = tokio::select! {
        value = receive.recv() => match value {
            Some(Ok(bytes)) => bytes,
            Some(Err(error)) => return (error.status(), error.code()).into_response(),
            None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        },
        () = service.0.stop.cancelled() => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let stream = ResponseStream {
        first: Some(first),
        lines: ReceiverStream::new(receive),
        _guard: guard,
        ended: false,
    };
    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/x-ndjson"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    headers.insert(
        "x-content-type-options",
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response
}

impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(e) | Self::Scope(e) => Some(e),
            Self::Service(e) => Some(e),
            Self::Subscription(e) => Some(e),
            Self::Verifier(e) => Some(e),
            Self::Local(e) => Some(e),
            Self::Encode(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "now_tests.rs"]
mod tests;
