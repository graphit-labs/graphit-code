//! A per-run queue, not an authority cache. Auth/IO borrow the original run job.
use super::{BrowserAuth, Session};
use crate::{
    knowledge_kiro_process::KiroProcess,
    knowledge_live::{
        Access, LiveSessionService, ReadContexts, SendRequest, Session as LiveSession,
    },
    live_agent_tools::{self, CallContext, SelectedContext},
    reference_explorer::{Identity, Resources},
};
use axum::{
    Router,
    extract::{FromRequestParts, Path},
    http::{header, request::Parts},
};
use graphit_auth::{ScopeCapture, broker::BrokerRequestContext};
use graphit_core::file_io::SnapshotContext;
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext as Incoming,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::json;
use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex, Weak},
    time::Instant,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub(crate) const URL_ENV: &str = "GRAPHIT_LIVE_MCP_URL";
pub(crate) const BEARER_ENV: &str = "GRAPHIT_LIVE_MCP_BEARER";
pub(crate) const SESSION_ENV: &str = "GRAPHIT_LIVE_WEB_SESSION";
const SESSION_HEADER: &str = "x-graphit-live-web-session";

#[derive(Clone, Hash, PartialEq, Eq)]
struct Key {
    web: String,
    live: String,
    turn: String,
}
#[derive(Default)]
struct Routing {
    runs: Mutex<HashMap<Key, Arc<Route>>>,
}
#[derive(Clone, Default)]
pub(crate) struct LiveMcpBroker(Arc<Routing>);

enum Operation {
    List,
    Call(Box<CallToolRequestParams>),
}
struct Message {
    bearer: Zeroizing<String>,
    operation: Operation,
    incoming: CancellationToken,
    reply: oneshot::Sender<CallToolResult>,
}
struct Route {
    key: Key,
    entry: Arc<Session>,
    session: LiveSession,
    read_contexts: ReadContexts,
    issuer: String,
    subject: String,
    provider_revision: u64,
    original: Arc<SnapshotContext>,
    send: mpsc::Sender<Message>,
    stop: CancellationToken,
    broker: Weak<Routing>,
}
pub(crate) struct OwnedRunRoute {
    route: Arc<Route>,
    sessions: BrowserAuth,
    receive: mpsc::Receiver<Message>,
}
/// Signal only this route. Its caller still awaits the same borrowed serve future.
#[derive(Clone)]
pub(crate) struct RunRouteStop(CancellationToken);
impl RunRouteStop {
    pub(crate) fn stop(&self) {
        self.0.cancel();
    }
}
/// Only the browser parent can supply its actual private registered session.
pub(super) struct Registration<'a> {
    pub sessions: &'a BrowserAuth,
    pub web_session: &'a str,
    pub entry: Arc<Session>,
    pub session: &'a LiveSession,
    pub read_contexts: &'a ReadContexts,
    pub turn: &'a SendRequest,
    pub identity: Identity<'a>,
    pub original: &'a RequestContext<'a>,
}
pub(crate) struct RunContext<'a> {
    pub resources: &'a Resources<'a>,
    pub backend: &'a graphit_auth::broker::project_access::BrokerProjectAccess,
    pub original: &'a RequestContext<'a>,
    pub access: &'a Access<'a>,
    pub live: &'a LiveSessionService,
    pub session: &'a LiveSession,
    pub turn: &'a SendRequest,
    pub selected: &'a [SelectedContext<'a>],
    pub source: Option<&'a crate::source_server::SourceServer>,
    pub ast: &'a [crate::live_agent_ast::AstSelection<'a>],
    pub model: Option<&'a Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
    pub allowed: &'a (dyn Fn() -> bool + Sync),
}

fn denied() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}
fn failure(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"error":{"code":code}}))
}
fn io_failure(error: &io::Error) -> CallToolResult {
    failure(match error.kind() {
        io::ErrorKind::Interrupted => "cancelled",
        io::ErrorKind::TimedOut => "deadline",
        io::ErrorKind::PermissionDenied => "permission_denied",
        _ => "live_mcp_unavailable",
    })
}
fn entry_check(entry: &Session, original: &SnapshotContext) -> io::Result<()> {
    original.check()?;
    if entry.cancelled.is_cancelled() || entry.expires <= Instant::now() {
        return Err(denied());
    }
    Ok(())
}
async fn current_entry(
    sessions: &BrowserAuth,
    key: &Key,
    entry: &Arc<Session>,
    original: &SnapshotContext,
) -> io::Result<()> {
    entry_check(entry, original)?;
    let deadline = original.required_deadline()?;
    let entries = tokio::select! {
        entries=sessions.0.sessions.lock()=>entries,
        ()=sessions.0.stop.cancelled()=>return Err(denied()),
        ()=original.cancellation().cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(deadline.into())=>return Err(io::ErrorKind::TimedOut.into()),
    };
    if !entries
        .get(&key.web)
        .is_some_and(|actual| Arc::ptr_eq(actual, entry))
    {
        return Err(denied());
    }
    entry_check(entry, original)
}
impl Route {
    fn same_session(&self, session: &LiveSession) -> bool {
        self.session.id == session.id
            && self.session.cursor.generation == session.cursor.generation
            && self.session.project == session.project
            && self.session.binding == session.binding
            && self.session.agent == session.agent
            && self.session.artifacts == session.artifacts
            && self.session.model == session.model
    }
    fn identity(&self, identity: Identity<'_>, original: &RequestContext<'_>) -> io::Result<()> {
        if !Arc::ptr_eq(&self.original, original.snapshot()) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        entry_check(&self.entry, original.snapshot())?;
        let (caller, verifier, _) = identity;
        caller
            .check(verifier, BrokerRequestContext::new(original.snapshot()))
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
        if caller.issuer() != self.issuer
            || caller.subject() != self.subject
            || caller.provider_revision() != self.provider_revision
        {
            return Err(denied());
        }
        Ok(())
    }
    fn publish(self: &Arc<Self>) -> io::Result<()> {
        entry_check(&self.entry, &self.original)?;
        if self.stop.is_cancelled() || self.send.is_closed() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let broker = self.broker.upgrade().ok_or(io::ErrorKind::BrokenPipe)?;
        let mut runs = broker
            .runs
            .lock()
            .map_err(|_| io::Error::other("Live MCP routing lock"))?;
        if let Some(previous) = runs.get(&self.key) {
            if !Arc::ptr_eq(previous, self) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
        } else {
            runs.insert(self.key.clone(), Arc::clone(self));
        }
        Ok(())
    }
}
impl LiveMcpBroker {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(super) async fn begin_run(&self, input: Registration<'_>) -> io::Result<OwnedRunRoute> {
        let key = Key {
            web: input.web_session.to_owned(),
            live: input.session.id.clone(),
            turn: input.turn.turn_id.clone(),
        };
        let limits = input.original.content_limits();
        for value in [&key.web, &key.live, &key.turn] {
            if value.is_empty() || value.len() > limits.max_scalar_bytes || value.contains('\0') {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        current_entry(
            input.sessions,
            &key,
            &input.entry,
            input.original.snapshot(),
        )
        .await?;
        let (caller, verifier, provider) = input.identity;
        let (configured, actual) = input
            .sessions
            .0
            .providers
            .get(input.entry.provider)
            .ok_or_else(denied)?;
        if configured != provider || !std::ptr::eq(actual.as_ref(), verifier) {
            return Err(denied());
        }
        let flow = input
            .sessions
            .0
            .flow(input.entry.provider)
            .map_err(|_| denied())?;
        let proof = flow
            .verify_access(
                &input.entry.registered,
                caller.bearer_for_broker(),
                BrokerRequestContext::new(input.original.snapshot()),
            )
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
        let credentials = tokio::select! {
            credentials=input.entry.credentials.lock()=>credentials,
            ()=input.original.cancellation().cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
            ()=tokio::time::sleep_until(input.original.snapshot().required_deadline()?.into())=>return Err(io::ErrorKind::TimedOut.into()),
        };
        if credentials.subject() != proof.subject() {
            return Err(denied());
        }
        drop(credentials);
        let issuer = proof.issuer().to_owned();
        let subject = proof.subject().to_owned();
        let provider_revision = proof.provider_revision();
        drop(proof);
        let (send, receive) = mpsc::channel(1);
        let route = Arc::new(Route {
            key,
            entry: input.entry,
            session: input.session.clone(),
            read_contexts: input.read_contexts.clone(),
            issuer,
            subject,
            provider_revision,
            original: Arc::clone(input.original.snapshot()),
            send,
            stop: CancellationToken::new(),
            broker: Arc::downgrade(&self.0),
        });
        route.identity(input.identity, input.original)?;
        Ok(OwnedRunRoute {
            route,
            sessions: input.sessions.clone(),
            receive,
        })
    }
    pub(crate) fn router(&self, limits: ContentLimits, stop: CancellationToken) -> Router {
        let factory = Forwarder {
            broker: self.clone(),
            limits,
        };
        let mut config = StreamableHttpServerConfig::default();
        // The bridge owns no MCP session authority or replay cache.
        config.legacy_session_mode = false;
        config.json_response = true;
        config.max_request_body_bytes = limits.max_input_bytes;
        config.cancellation_token = stop;
        let service = StreamableHttpService::new(
            move || Ok(factory.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        Router::new()
            .route_service("/api/live/sessions/{id}/turns/{turn}/mcp", service)
            .layer(axum::middleware::map_response(
                super::private_response_async,
            ))
    }
}
impl Drop for OwnedRunRoute {
    fn drop(&mut self) {
        self.route.stop.cancel();
        self.receive.close();
        if let Some(broker) = self.route.broker.upgrade() {
            let mut runs = broker.runs.lock().unwrap_or_else(|e| e.into_inner());
            if runs
                .get(&self.route.key)
                .is_some_and(|actual| Arc::ptr_eq(actual, &self.route))
            {
                runs.remove(&self.route.key);
            }
        }
    }
}
impl OwnedRunRoute {
    /// Capture before borrowing this owner in serve; stopping never cancels Original.
    pub(crate) fn stop_handle(&self) -> RunRouteStop {
        RunRouteStop(self.route.stop.clone())
    }
    fn finish(&mut self) -> io::Result<()> {
        self.receive.close();
        while let Ok(message) = self.receive.try_recv() {
            let _ = message.reply.send(failure("live_mcp_unavailable"));
        }
        self.route.original.check()
    }
    /// Construction does not publish a route; only the real native launch does.
    pub(crate) fn launch<'a>(
        &self,
        identity: Identity<'a>,
        original: &'a RequestContext<'a>,
        address: SocketAddr,
        installation: &graphit_core::project_paths::InstallationPaths,
        executable: &std::path::Path,
    ) -> io::Result<LiveMcpLaunch<'a>> {
        self.route.identity(identity, original)?;
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut url = url::Url::parse(&format!("http://{address}/"))
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        url.path_segments_mut()
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?
            .extend([
                "api",
                "live",
                "sessions",
                &self.route.key.live,
                "turns",
                &self.route.key.turn,
                "mcp",
            ]);
        let private = Zeroizing::new(vec![
            url.to_string(),
            identity.0.bearer_for_broker().to_owned(),
            self.route.key.web.clone(),
        ]);
        for value in &private[1..] {
            header::HeaderValue::from_str(value)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        }
        Ok(LiveMcpLaunch {
            route: Arc::clone(&self.route),
            identity,
            original,
            private,
            global_variable: installation.brand().environment_variable("GLOBAL_DIR")?,
            global_directory: installation.root().as_os_str().to_owned(),
            executable_directory: executable
                .parent()
                .ok_or(io::ErrorKind::InvalidInput)?
                .to_owned(),
        })
    }
    /// Poll this future independently alongside run_turn, then retain the native join.
    pub(crate) async fn serve(&mut self, input: RunContext<'_>) -> io::Result<()> {
        if !self.route.same_session(input.session)
            || input.turn.turn_id != self.route.key.turn
            || !Arc::ptr_eq(&self.route.original, input.original.snapshot())
            || !Arc::ptr_eq(
                input.access.original().snapshot(),
                input.original.snapshot(),
            )
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let stored = input
            .live
            .read_contexts(
                input.access,
                &input.session.id,
                &input.session.cursor.generation,
            )
            .map_err(|error| io::Error::from(error.kind()))?;
        if !self.route.same_session(&stored.session) || stored.contexts != self.route.read_contexts
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let deadline = input.original.snapshot().required_deadline()?;
        loop {
            let message = tokio::select! {
                biased;
                ()=self.route.stop.cancelled()=>return self.finish(),
                message=self.receive.recv()=>message.ok_or(io::ErrorKind::BrokenPipe)?,
                ()=input.original.cancellation().cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
                ()=tokio::time::sleep_until(deadline.into())=>return Err(io::ErrorKind::TimedOut.into()),
                ()=self.sessions.0.stop.cancelled()=>return Err(denied()),
            };
            if message.reply.is_closed() || message.incoming.is_cancelled() {
                continue;
            }
            let result = {
                let mut work = Box::pin(self.execute(&input, &message));
                if matches!(&message.operation, Operation::Call(call) if live_agent_tools::is_ast_tool(&call.name))
                {
                    // Retain the same future, admission, lease and blocking JoinHandle.
                    // Capture the first observed cancellation before awaiting quiescence.
                    tokio::select! {
                        biased;
                        ()=self.route.stop.cancelled()=>{ let _ = work.await; Err(io::ErrorKind::BrokenPipe.into()) },
                        ()=message.incoming.cancelled()=>{ let _ = work.await; Err(io::ErrorKind::Interrupted.into()) },
                        ()=input.original.cancellation().cancelled()=>{ let _ = work.await; Err(io::ErrorKind::Interrupted.into()) },
                        ()=tokio::time::sleep_until(deadline.into())=>{ let _ = work.await; Err(io::ErrorKind::TimedOut.into()) },
                        result=&mut work=>result,
                    }
                } else {
                    tokio::select! {
                        biased;
                        ()=self.route.stop.cancelled()=>work.await,
                        result=&mut work=>result,
                        ()=message.incoming.cancelled()=>Err(io::ErrorKind::Interrupted.into()),
                        ()=input.original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
                        ()=tokio::time::sleep_until(deadline.into())=>Err(io::ErrorKind::TimedOut.into()),
                    }
                }
            };
            if self.route.stop.is_cancelled() {
                return self.finish();
            }
            let result = result.unwrap_or_else(|e| io_failure(&e));
            if !message.incoming.is_cancelled() {
                let _ = message.reply.send(result);
            }
        }
    }
    async fn execute(
        &self,
        input: &RunContext<'_>,
        message: &Message,
    ) -> io::Result<CallToolResult> {
        current_entry(
            &self.sessions,
            &self.route.key,
            &self.route.entry,
            input.original.snapshot(),
        )
        .await?;
        input.access.read()?;
        if !(input.allowed)() {
            return Err(denied());
        }
        let (provider, verifier) = self
            .sessions
            .0
            .providers
            .get(self.route.entry.provider)
            .ok_or_else(denied)?;
        let flow = self
            .sessions
            .0
            .flow(self.route.entry.provider)
            .map_err(|_| denied())?;
        let caller = flow
            .verify_access(
                &self.route.entry.registered,
                &message.bearer,
                BrokerRequestContext::new(input.original.snapshot()),
            )
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
        let identity = (&caller, verifier.as_ref(), provider.as_str());
        self.route.identity(identity, input.original)?;
        let binding = input.access.scope().binding()?;
        let read = input
            .resources
            .auth
            .issuer()
            .admit_web_user_knowledge_read(
                &caller,
                verifier,
                input.original,
                provider,
                ScopeCapture {
                    live: input.access.scope(),
                    record: &binding,
                },
            )
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
        let access = Access::user(&read, None)?;
        let context = CallContext {
            resources: input.resources,
            backend: input.backend,
            identity,
            original: input.original,
            access: &access,
            live: input.live,
            session: input.session,
            read_contexts: &self.route.read_contexts,
            turn: input.turn,
            selected: input.selected,
            source: input.source,
            ast: input.ast,
            incoming: &message.incoming,
            model: input.model,
            allowed: input.allowed,
        };
        let result = match &message.operation {
            Operation::List => live_agent_tools::list(context),
            Operation::Call(call) => {
                Box::pin(live_agent_tools::call(context, call.as_ref().clone())).await
            }
        };
        current_entry(
            &self.sessions,
            &self.route.key,
            &self.route.entry,
            input.original.snapshot(),
        )
        .await?;
        self.route.identity(identity, input.original)?;
        access.read()?;
        if !(input.allowed)() {
            return Err(denied());
        }
        Ok(match result {
            Ok(value) => value,
            Err(error) => error.response(),
        })
    }
}

/// A borrowed launch proof plus process-only secrets; never Debug or Serialize.
pub(crate) struct LiveMcpLaunch<'a> {
    route: Arc<Route>,
    identity: Identity<'a>,
    original: &'a RequestContext<'a>,
    private: Zeroizing<Vec<String>>,
    global_variable: String,
    global_directory: std::ffi::OsString,
    executable_directory: std::path::PathBuf,
}
impl LiveMcpLaunch<'_> {
    pub(crate) fn apply_to(
        &self,
        original: &Arc<SnapshotContext>,
        transport: &mut KiroProcess,
    ) -> io::Result<()> {
        if !Arc::ptr_eq(original, self.original.snapshot())
            || !Arc::ptr_eq(original, &transport.original)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.route.identity(self.identity, self.original)?;
        let inherited_path = transport
            .environment
            .iter()
            .find(|(key, _)| key == std::ffi::OsStr::new("PATH"))
            .map(|(_, value)| value);
        let path = std::env::join_paths(
            std::iter::once(self.executable_directory.clone())
                .chain(inherited_path.into_iter().flat_map(std::env::split_paths)),
        )
        .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;
        self.route.publish()?;
        transport.environment.retain(|(key, _)| {
            ![URL_ENV, BEARER_ENV, SESSION_ENV]
                .iter()
                .any(|reserved| key.as_os_str() == std::ffi::OsStr::new(reserved))
                && key != std::ffi::OsStr::new("PATH")
                && key != std::ffi::OsStr::new(&self.global_variable)
        });
        transport.environment.extend(
            [URL_ENV, BEARER_ENV, SESSION_ENV]
                .into_iter()
                .zip(self.private.iter())
                .map(|(key, value)| (key.into(), value.into())),
        );
        transport.environment.extend([
            ("PATH".into(), path),
            (
                self.global_variable.clone().into(),
                self.global_directory.clone(),
            ),
        ]);
        Ok(())
    }
    pub(crate) fn private_output(&self) -> &[String] {
        &self.private
    }
}

#[derive(Clone)]
struct Forwarder {
    broker: LiveMcpBroker,
    limits: ContentLimits,
}
impl Forwarder {
    async fn forward(&self, operation: Operation, context: Incoming<RoleServer>) -> CallToolResult {
        let work=async {
            let mut parts=context.extensions.get::<Parts>().cloned().ok_or_else(denied)?;
            if parts.uri.query().is_some(){return Err(denied());}
            let Path((live,turn))=Path::<(String,String)>::from_request_parts(&mut parts,&()).await.map_err(|_|denied())?;
            let web=parts.headers.get(SESSION_HEADER).and_then(|v|v.to_str().ok()).filter(|s|!s.is_empty()&&s.len()<=self.limits.max_scalar_bytes).ok_or_else(denied)?.to_owned();
            let bearer=Zeroizing::new(parts.headers.get(header::AUTHORIZATION).and_then(|v|v.to_str().ok()).and_then(|s|s.strip_prefix("Bearer ")).filter(|s|!s.is_empty()&&s.len()<=self.limits.max_input_bytes).ok_or_else(denied)?.to_owned());
            let route=self.broker.0.runs.lock().map_err(|_|io::Error::other("Live MCP routing lock"))?.get(&Key {web,live,turn}).cloned().ok_or_else(denied)?;
            route.original.check()?;
            if route.stop.is_cancelled() {return Err(io::ErrorKind::BrokenPipe.into());}
            let deadline=route.original.required_deadline()?;
            let(send,receive)=oneshot::channel();
            let message=Message {bearer,operation,incoming:context.ct.clone(),reply:send};
            tokio::select! {
                result=route.send.send(message)=>result.map_err(|_|io::Error::from(io::ErrorKind::BrokenPipe))?,
                ()=context.ct.cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
                ()=route.stop.cancelled()=>return Err(io::ErrorKind::BrokenPipe.into()),
                ()=route.original.cancellation().cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
                ()=tokio::time::sleep_until(deadline.into())=>return Err(io::ErrorKind::TimedOut.into()),
            }
            let response=tokio::select! {
                ()=route.stop.cancelled()=>return Err(io::ErrorKind::BrokenPipe.into()),
                result=receive=>result.map_err(|_|io::Error::from(io::ErrorKind::BrokenPipe))?,
                ()=context.ct.cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
                ()=route.original.cancellation().cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
                ()=tokio::time::sleep_until(deadline.into())=>return Err(io::ErrorKind::TimedOut.into()),
            };
            route.original.check()?;
            entry_check(&route.entry,&route.original)?;
            if route.stop.is_cancelled() {return Err(io::ErrorKind::BrokenPipe.into());}
            Ok::<_,io::Error>(response)
        }.await;
        work.unwrap_or_else(|e| io_failure(&e))
    }
}
impl ServerHandler for Forwarder {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        live_agent_tools::potential_catalog()
            .into_iter()
            .find(|tool| tool.name == name)
            .map(crate::mcp_output::decorate_tool)
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: Incoming<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let result = self.forward(Operation::List, context).await;
        if result.is_error == Some(true) {
            return Err(ErrorData::invalid_request(
                "Live tool admission denied",
                None,
            ));
        }
        let tools: Vec<Tool> = result
            .structured_content
            .as_ref()
            .and_then(|value| value.get("tools"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .ok_or_else(|| ErrorData::internal_error("Live catalogue unavailable", None))?;
        let tools = live_agent_tools::catalog_from_tools(&tools)
            .ok_or_else(|| ErrorData::internal_error("Live catalogue unavailable", None))?;
        Ok(ListToolsResult::with_all_items(
            tools
                .into_iter()
                .map(crate::mcp_output::decorate_tool)
                .collect(),
        ))
    }
    async fn call_tool(
        &self,
        mut call: CallToolRequestParams,
        context: Incoming<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let format = crate::mcp_output::select_format(&mut call)?;
        if self.get_tool(&call.name).is_none() {
            return Err(ErrorData::invalid_params("unknown Live tool", None));
        }
        let incoming = context.ct.clone();
        let result = self.forward(Operation::Call(Box::new(call)), context).await;
        let result =
            crate::mcp_output::format_result(result, format, self.limits.max_output_bytes)?;
        let result = if result.is_error != Some(true) && incoming.is_cancelled() {
            crate::mcp_output::format_result(
                failure("cancelled"),
                format,
                self.limits.max_output_bytes,
            )?
        } else {
            result
        };
        Ok(result.into())
    }
}
