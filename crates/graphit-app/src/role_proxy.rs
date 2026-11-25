//! Run-owned stdio MCP forwarding; credentials never fall back to Runtime.
use crate::{role_tool_server::ROLE_SELECTION_HEADER, task_server::request::ROLE_BINDING_HEADER};
use graphit_core::file_lock::Cancellation;
use graphit_hub::{
    NativeBinding, ProjectionLimits, native::RoleCredentialReference, policy::RequestContext,
};
use rmcp::{
    ClientServiceExt, ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::{ClientLifecycleMode, RequestContext as Incoming, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, oneshot},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub(crate) const BEARER_ENV: &str = "GRAPHIT_ROLE_BEARER";
pub(crate) const REFERENCE_ENV: &str = "GRAPHIT_ROLE_REFERENCE";
pub(crate) const SELECTION_ENV: &str = "GRAPHIT_ROLE_SELECTION";
pub(crate) const BINDING_ENV: &str = "GRAPHIT_ROLE_BINDING";

struct State {
    uri: String,
    client: reqwest::Client,
    headers: HashMap<axum::http::HeaderName, axum::http::HeaderValue>,
    content: graphit_hub::native::ContentLimits,
    timeout: Duration,
    tools: Vec<Tool>,
    stop: CancellationToken,
    jobs: Mutex<OwnedJobs>,
}
#[derive(Default)]
struct OwnedJobs {
    jobs: JoinSet<io::Result<()>>,
    first: Option<io::Error>,
}
impl OwnedJobs {
    fn retain(&mut self, result: Result<io::Result<()>, tokio::task::JoinError>) {
        let cause = match result {
            Ok(result) => result.err(),
            Err(cause) => Some(io::Error::other(cause)),
        };
        if self.first.is_none() {
            self.first = cause;
        }
    }
    fn reap(&mut self) {
        while let Some(result) = self.jobs.try_join_next() {
            self.retain(result);
        }
    }
}
/// The daemon authenticates the real run grant on every outgoing request.
#[derive(Clone)]
pub struct RoleProxy(Arc<State>);
enum Request {
    List,
    Call(Box<CallToolRequestParams>),
}
enum Reply {
    List(Vec<Tool>),
    Call(CallToolResult),
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn joined_transport(reason: rmcp::service::QuitReason) -> io::Result<()> {
    match reason {
        rmcp::service::QuitReason::Cancelled | rmcp::service::QuitReason::Closed => Ok(()),
        rmcp::service::QuitReason::JoinError(cause) => Err(io::Error::other(cause)),
        _ => Err(io::Error::other("Unrecognized Role transport completion")),
    }
}
impl RoleProxy {
    /// Read only the transient child credentials. Missing credentials are terminal.
    pub async fn from_environment(
        address: SocketAddr,
        client: reqwest::Client,
        content: graphit_hub::native::ContentLimits,
        projection: ProjectionLimits,
        timeout: Duration,
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        original.snapshot().required_deadline()?;
        let bearer = std::env::var(BEARER_ENV)
            .map(SecretString::from)
            .map_err(|_| io::Error::from(io::ErrorKind::PermissionDenied))?;
        let reference = std::env::var(REFERENCE_ENV)
            .map(Zeroizing::new)
            .map_err(|_| io::Error::from(io::ErrorKind::PermissionDenied))?;
        let selection = match std::env::var(SELECTION_ENV) {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => return Err(io::ErrorKind::InvalidInput.into()),
        };
        let binding = match std::env::var(BINDING_ENV) {
            Ok(value) => Some(
                value
                    .parse::<NativeBinding>()
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
            ),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => return Err(io::ErrorKind::InvalidInput.into()),
        };
        Self::new(
            address,
            client,
            (content, projection, timeout),
            (bearer, &reference, selection.as_deref(), binding),
            original,
        )
        .await
    }
    /// Credential bytes and selector data are not authority; the remote SDK verifies them.
    pub async fn new(
        address: SocketAddr,
        client: reqwest::Client,
        policy: (
            graphit_hub::native::ContentLimits,
            ProjectionLimits,
            Duration,
        ),
        credentials: (SecretString, &str, Option<&str>, Option<NativeBinding>),
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        let (content, projection, timeout) = policy;
        let (bearer, reference, selection, binding) = credentials;
        let deadline = original.snapshot().required_deadline()?;
        if !address.ip().is_loopback()
            || timeout.is_zero()
            || Instant::now().checked_add(timeout).is_none()
            || bearer.expose_secret().is_empty()
            || bearer.expose_secret().len() > content.max_scalar_bytes
            || bearer
                .expose_secret()
                .bytes()
                .any(|byte| byte.is_ascii_whitespace())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        RoleCredentialReference::decode(
            reference.as_bytes(),
            content.max_input_bytes,
            projection,
            original,
        )
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let mut headers = HashMap::new();
        let mut authorization =
            axum::http::HeaderValue::from_str(&format!("Bearer {}", bearer.expose_secret()))
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        authorization.set_sensitive(true);
        headers.insert(axum::http::header::AUTHORIZATION, authorization);
        headers.insert(
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("role"),
        );
        let mut value = axum::http::HeaderValue::from_str(reference)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        value.set_sensitive(true);
        headers.insert(
            axum::http::HeaderName::from_static("x-graphit-role-reference"),
            value,
        );
        if let Some(binding) = binding {
            headers.insert(
                axum::http::HeaderName::from_static(ROLE_BINDING_HEADER),
                axum::http::HeaderValue::from_static(binding.as_str()),
            );
        }
        if let Some(selection) = selection {
            if selection.len() > content.max_input_bytes {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            serde_json::from_str::<crate::task_server::request::Selection>(selection)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
            headers.insert(
                axum::http::HeaderName::from_static(ROLE_SELECTION_HEADER),
                axum::http::HeaderValue::from_str(selection)
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
            );
        }
        let mut state = State {
            uri: format!("http://{address}/roles/mcp"),
            client,
            headers,
            content,
            timeout,
            tools: Vec::new(),
            stop: CancellationToken::new(),
            jobs: Mutex::new(OwnedJobs::default()),
        };
        let remote = tokio::select! {
            result = connect(&state) => result?,
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        };
        let tools = tokio::select! {
            result=remote.peer().list_all_tools()=>result.map_err(|_|io::Error::other("Role catalogue request failed")),
            ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
            ()=tokio::time::sleep_until(deadline.into())=>Err(io::ErrorKind::TimedOut.into()),
        };
        let joined = remote
            .cancel()
            .await
            .map_err(|_| io::Error::other("Role catalogue transport failed to close"))
            .and_then(joined_transport);
        state.tools = tools?;
        joined?;
        original.snapshot().check()?;
        Ok(Self(Arc::new(state)))
    }
    /// CLI forwarding borrows its caller's original request throughout transport cleanup.
    pub(crate) async fn call_tool_original(
        &self,
        call: CallToolRequestParams,
        original: &RequestContext<'_>,
    ) -> io::Result<CallToolResult> {
        original.snapshot().check()?;
        let deadline = original.snapshot().required_deadline()?;
        let mut jobs = tokio::select! {
            jobs = self.0.jobs.lock() => jobs,
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = self.0.stop.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        };
        let remote = tokio::select! {
            result = connect(&self.0) => result?,
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = self.0.stop.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        };
        let peer = remote.peer().clone();
        let admission = original.snapshot().check();
        if let Err(cause) = admission {
            let _ = remote.cancel().await;
            return Err(cause);
        }
        jobs.reap();
        let (close, closing) = oneshot::channel::<()>();
        let (joined, joining) = oneshot::channel();
        let stop = self.0.stop.clone();
        // Dropping this caller closes the oneshot; the retained owner still joins
        // the negotiated transport without reconstructing the caller's request.
        jobs.jobs.spawn(async move {
            tokio::select! { _ = closing => (), () = stop.cancelled() => () }
            let result = remote
                .cancel()
                .await
                .map_err(|_| io::Error::other("Role MCP transport failed to close"))
                .and_then(joined_transport);
            let reply = result
                .as_ref()
                .map(|_| ())
                .map_err(|cause| io::Error::new(cause.kind(), cause.to_string()));
            let _ = joined.send(reply);
            result
        });
        drop(jobs);
        let result = tokio::select! {
            result = peer.call_tool(call) => result.map_err(|_| io::Error::other("Role MCP request failed")),
            () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
            () = self.0.stop.cancelled() => Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
        };
        drop(close);
        let joined = joining
            .await
            .map_err(|_| io::Error::other("Role MCP cleanup owner failed to join"));
        let result = result?;
        joined??;
        original.snapshot().check()?;
        Ok(result)
    }
    pub async fn serve_stream<R, W>(&self, read: R, write: W) -> io::Result<()>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let service = self
            .clone()
            .serve_with_ct((read, write), self.0.stop.child_token())
            .await
            .map_err(|_| io::Error::other("Role stdio negotiation failed"))?;
        let result = service
            .waiting()
            .await
            .map_err(|_| io::Error::other("Role stdio transport failed"))
            .and_then(joined_transport);
        let joined = self.shutdown().await;
        result?;
        joined
    }
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        while let Some(result) = jobs.jobs.join_next().await {
            jobs.retain(result);
        }
        match jobs.first.take() {
            Some(cause) => Err(cause),
            None => Ok(()),
        }
    }
    async fn forward(
        &self,
        request: Request,
        incoming: &Incoming<RoleServer>,
    ) -> io::Result<Reply> {
        let cancel = Cancellation::default();
        let _guard = CancelOnDrop(cancel.clone());
        let deadline = Instant::now()
            .checked_add(self.0.timeout)
            .ok_or(io::ErrorKind::InvalidInput)?;
        let (send, receive) = oneshot::channel();
        let mut jobs = self.0.jobs.lock().await;
        jobs.reap();
        if self.0.stop.is_cancelled() || incoming.ct.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let state = Arc::clone(&self.0);
        let work_cancel = cancel.clone();
        let incoming_cancel = incoming.ct.clone();
        jobs.jobs.spawn(async move {
            let mut close_failure = None;
            let operation = async {
                let original = RequestContext::new(state.content, &work_cancel, deadline)
                    .map_err(io::Error::other)?;
                let remote = tokio::select! {
                    result = connect(&state) => result?,
                    () = work_cancel.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                    () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
                };
                let call = async {
                    match request {
                        Request::List => remote.peer().list_all_tools().await.map(Reply::List),
                        Request::Call(call) => remote.peer().call_tool(*call).await.map(Reply::Call),
                    }.map_err(|_| io::Error::other("Role MCP request failed"))
                };
                let result = tokio::select! {
                    result = call => result,
                    () = work_cancel.cancelled() => Err(io::ErrorKind::Interrupted.into()),
                    () = state.stop.cancelled() => Err(io::ErrorKind::Interrupted.into()),
                    () = incoming_cancel.cancelled() => Err(io::ErrorKind::Interrupted.into()),
                    () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
                };
                let joined = remote.cancel().await
                    .map_err(|_| io::Error::other("Role MCP transport failed to close"))
                    .and_then(joined_transport);
                if let Err(cause) = &joined {
                    close_failure = Some(io::Error::new(cause.kind(), cause.to_string()));
                }
                let result = result?;
                joined?;
                original.snapshot().check()?;
                Ok(result)
            }.await;
            let _ = send.send(operation);
            close_failure.map_or(Ok(()), Err)
        });
        drop(jobs);
        tokio::select! {
            result = receive => result.map_err(|_| io::Error::other("Role proxy sender closed"))?,
            () = incoming.ct.cancelled() => Err(io::ErrorKind::Interrupted.into()),
            () = self.0.stop.cancelled() => Err(io::ErrorKind::Interrupted.into()),
        }
    }
}
async fn connect(state: &State) -> io::Result<RunningService<RoleClient, ()>> {
    ().serve_with_lifecycle(
        StreamableHttpClientTransport::with_client(
            state.client.clone(),
            StreamableHttpClientTransportConfig::with_uri(state.uri.clone())
                .custom_headers(state.headers.clone()),
        ),
        ClientLifecycleMode::Discover {
            preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
        },
    )
    .await
    .map_err(|_| io::Error::other("Role MCP authentication or negotiation failed"))
}
impl ServerHandler for RoleProxy {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.0.tools.iter().find(|tool| tool.name == name).cloned()
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        incoming: Incoming<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        match self.forward(Request::List, &incoming).await {
            Ok(Reply::List(tools)) => Ok(ListToolsResult::with_all_items(tools)),
            _ => Err(ErrorData::internal_error(
                "Role catalogue unavailable",
                None,
            )),
        }
    }
    async fn call_tool(
        &self,
        call: CallToolRequestParams,
        incoming: Incoming<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        match self.forward(Request::Call(Box::new(call)), &incoming).await {
            Ok(Reply::Call(result)) => Ok(result.into()),
            _ => Ok(CallToolResult::error(vec![ContentBlock::text(
                "Role MCP request cancelled, expired or unavailable",
            )])
            .into()),
        }
    }
}
