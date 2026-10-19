//! The native child uses its process-only Web bridge, never a runtime credential.
use super::{catalog_from_tools, potential_catalog};
use crate::browser_auth::live_mcp::{BEARER_ENV, SESSION_ENV, URL_ENV};
use graphit_hub::native::ContentLimits;
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use rmcp::{
    ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, ListToolsResult, PaginatedRequestParams,
        ProtocolVersion, ServerCapabilities, ServerConfig, Tool,
    },
    service::{
        ClientLifecycleMode, ClientServiceExt, RequestContext, RxJsonRpcMessage, TxJsonRpcMessage,
    },
    transport::{
        StreamableHttpClientTransport, Transport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::{
    collections::HashMap,
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::oneshot,
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

struct Shared {
    uri: Zeroizing<String>,
    headers: HashMap<HeaderName, HeaderValue>,
    client: reqwest::Client,
    limits: ContentLimits,
    timeout: Duration,
    stop: CancellationToken,
    work: Mutex<JoinSet<()>>,
}
#[derive(Clone)]
pub(crate) struct LiveMcpProxy(Arc<Shared>);

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid Live MCP environment")
}
fn remote_error() -> ErrorData {
    ErrorData::internal_error("Live MCP transport unavailable", None)
}

impl LiveMcpProxy {
    /// Signal the current serve_stream owner; its caller still awaits that future.
    pub(crate) fn request_stop(&self) {
        self.0.stop.cancel();
    }
    pub(crate) fn from_environment(limits: ContentLimits, timeout: Duration) -> io::Result<Self> {
        let read = |key| {
            std::env::var(key)
                .map(Zeroizing::new)
                .map_err(|_| invalid())
        };
        Self::configured(
            read(URL_ENV)?,
            read(BEARER_ENV)?,
            read(SESSION_ENV)?,
            limits,
            timeout,
        )
    }
    pub(crate) fn configured(
        uri: Zeroizing<String>,
        bearer: Zeroizing<String>,
        session: Zeroizing<String>,
        limits: ContentLimits,
        timeout: Duration,
    ) -> io::Result<Self> {
        if uri.len() > limits.max_input_bytes
            || bearer.is_empty()
            || bearer.len() > limits.max_input_bytes
            || session.is_empty()
            || session.len() > limits.max_scalar_bytes
            || timeout.is_zero()
        {
            return Err(invalid());
        }
        let url = url::Url::parse(&uri).map_err(|_| invalid())?;
        let parts: Vec<_> = url.path_segments().ok_or_else(invalid)?.collect();
        if url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.port_or_known_default() == Some(0)
            || !url
                .host_str()
                .and_then(|host| {
                    host.trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .ok()
                })
                .is_some_and(|ip| ip.is_loopback())
            || parts.len() != 7
            || parts[..3] != ["api", "live", "sessions"]
            || parts[3].is_empty()
            || parts[4] != "turns"
            || parts[5].is_empty()
            || parts[6] != "mcp"
        {
            return Err(invalid());
        }
        let mut authorization = Zeroizing::new(String::from("Bearer "));
        authorization.push_str(&bearer);
        let mut authorization = HeaderValue::from_str(&authorization).map_err(|_| invalid())?;
        authorization.set_sensitive(true);
        let mut session = HeaderValue::from_str(&session).map_err(|_| invalid())?;
        session.set_sensitive(true);
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .map_err(|_| invalid())?;
        Ok(Self(Arc::new(Shared {
            uri,
            headers: HashMap::from([
                (AUTHORIZATION, authorization),
                (
                    HeaderName::from_static("x-graphit-live-web-session"),
                    session,
                ),
            ]),
            client,
            limits,
            timeout,
            stop: CancellationToken::new(),
            work: Mutex::new(JoinSet::new()),
        })))
    }
    /// Own every outgoing SDK connection until its close has actually joined.
    async fn forward(
        &self,
        call: Option<CallToolRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<Output, ErrorData> {
        if let Some(call) = &call {
            if !potential_catalog()
                .iter()
                .any(|tool| tool.name == call.name)
            {
                return Err(ErrorData::invalid_params("unknown Live tool", None));
            }
            if serde_json::to_vec(call).map_err(|_| remote_error())?.len()
                > self.0.limits.max_input_bytes
            {
                return Err(ErrorData::invalid_params("Live arguments too large", None));
            }
        }
        let cancel = context.ct.child_token();
        let _cancel = cancel.clone().drop_guard();
        let (send, receive) = oneshot::channel();
        {
            let mut work = self.0.work.lock().map_err(|_| remote_error())?;
            if self.0.stop.is_cancelled() {
                return Err(remote_error());
            }
            // Join completed owners rather than retaining their handles per call.
            while work.try_join_next().is_some() {}
            let shared = Arc::clone(&self.0);
            work.spawn(async move {
                let result = exchange(&shared, call, &cancel).await;
                let _ = send.send(result);
            });
        }
        receive.await.map_err(|_| remote_error())?
    }
    pub(crate) async fn serve_stream<R, W>(self, read: R, write: W) -> io::Result<()>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let _stop = self.0.stop.clone().drop_guard();
        let read = EofReader {
            inner: read,
            stop: self.0.stop.clone(),
        };
        let result = match self
            .clone()
            .serve_with_ct((read, write), self.0.stop.clone())
            .await
        {
            Ok(service) => service
                .waiting()
                .await
                .map(|_| ())
                .map_err(|_| io::Error::other("Live stdio service failed")),
            Err(_) => Err(io::Error::other("Live stdio initialization failed")),
        };
        self.0.stop.cancel();
        let mut work = std::mem::take(
            &mut *self
                .0
                .work
                .lock()
                .map_err(|_| io::Error::other("Live stdio work lock"))?,
        );
        let mut join_failed = false;
        while let Some(joined) = work.join_next().await {
            join_failed |= joined.is_err();
        }
        if join_failed {
            return Err(io::Error::other("Live transport join failed"));
        }
        result
    }
}
enum Output {
    List(ListToolsResult),
    Call(CallToolResponse),
}
// SDK startup can drop its transport before there is a RunningService to close.
// Return it to the existing exchange owner; that owner always awaits SDK close.
struct ReturningTransport<T> {
    inner: Option<T>,
    returned: Option<oneshot::Sender<T>>,
}
impl<T> Drop for ReturningTransport<T> {
    fn drop(&mut self) {
        if let (Some(transport), Some(returned)) = (self.inner.take(), self.returned.take()) {
            let _ = returned.send(transport);
        }
    }
}
impl<T: Transport<RoleClient>> Transport<RoleClient> for ReturningTransport<T> {
    type Error = T::Error;

    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleClient>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner
            .as_mut()
            .expect("owned SDK transport")
            .send(message)
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.inner
            .as_mut()
            .expect("owned SDK transport")
            .receive()
            .await
    }
    async fn close(&mut self) -> Result<(), Self::Error> {
        self.inner
            .as_mut()
            .expect("owned SDK transport")
            .close()
            .await
    }
}
async fn exchange(
    shared: &Shared,
    call: Option<CallToolRequestParams>,
    cancel: &CancellationToken,
) -> Result<Output, ErrorData> {
    let deadline = tokio::time::Instant::now() + shared.timeout;
    let config = StreamableHttpClientTransportConfig::with_uri(shared.uri.as_str().to_owned())
        .custom_headers(shared.headers.clone());
    let transport = StreamableHttpClientTransport::with_client(shared.client.clone(), config);
    let (returned, receive_transport) = oneshot::channel();
    let connected = {
        let startup = ().serve_with_lifecycle(
            ReturningTransport {
                inner: Some(transport),
                returned: Some(returned),
            },
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::LATEST],
            },
        );
        tokio::pin!(startup);
        tokio::select! {
            result = &mut startup => result.map_err(|_| remote_error()),
            () = cancel.cancelled() => Err(remote_error()),
            () = shared.stop.cancelled() => Err(remote_error()),
            () = tokio::time::sleep_until(deadline) => Err(remote_error()),
        }
    };
    let result = if let Ok(connected) = connected {
        let request = async {
            if let Some(call) = call {
                connected
                    .peer()
                    .call_tool(call)
                    .await
                    .map(|value| Output::Call(value.into()))
                    .map_err(|_| remote_error())
            } else {
                let tools = connected
                    .peer()
                    .list_all_tools()
                    .await
                    .map_err(|_| remote_error())?;
                let expected = catalog_from_tools(&tools).ok_or_else(remote_error)?;
                Ok(Output::List(ListToolsResult::with_all_items(
                    expected
                        .into_iter()
                        .map(crate::mcp_output::decorate_tool)
                        .collect(),
                )))
            }
        };
        let result = tokio::select! {
            result=request=>result,
            ()=cancel.cancelled()=>Err(remote_error()),
            ()=shared.stop.cancelled()=>Err(remote_error()),
            ()=tokio::time::sleep_until(deadline)=>Err(remote_error()),
        };
        match connected.cancel().await {
            Ok(_) => result,
            Err(_) => Err(remote_error()),
        }
    } else {
        Err(remote_error())
    };
    let mut transport = receive_transport.await.map_err(|_| remote_error())?;
    transport.close().await.map_err(|_| remote_error())?;
    if cancel.is_cancelled() || shared.stop.is_cancelled() {
        return Err(remote_error());
    }
    result
}
impl ServerHandler for LiveMcpProxy {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        potential_catalog()
            .into_iter()
            .find(|tool| tool.name == name)
            .map(crate::mcp_output::decorate_tool)
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        match self.forward(None, context).await? {
            Output::List(value) => Ok(value),
            Output::Call(_) => Err(remote_error()),
        }
    }
    async fn call_tool(
        &self,
        call: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        match self.forward(Some(call), context).await? {
            Output::Call(value) => Ok(value),
            Output::List(_) => Err(remote_error()),
        }
    }
}
struct EofReader<R> {
    inner: R,
    stop: CancellationToken,
}
impl<R: AsyncRead + Unpin> AsyncRead for EofReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buffer.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(context, buffer);
        if buffer.remaining() > 0
            && matches!(&result, Poll::Ready(Ok(())))
            && before == buffer.filled().len()
        {
            self.stop.cancel();
        }
        if matches!(&result, Poll::Ready(Err(_))) {
            self.stop.cancel();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn content() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        }
    }
    fn proxy(uri: &str) -> io::Result<LiveMcpProxy> {
        LiveMcpProxy::configured(
            Zeroizing::new(uri.into()),
            Zeroizing::new("transport-test-secret".into()),
            Zeroizing::new("transport-test-session".into()),
            content(),
            Duration::from_secs(5),
        )
    }
    #[test]
    fn live_mcp_proxy_rejects_nonlocal_or_ambiguous_endpoints_without_secret_echo() {
        for uri in [
            "http://localhost:1234/api/live/sessions/s/turns/t/mcp",
            "https://127.0.0.1:1234/api/live/sessions/s/turns/t/mcp",
            "http://192.0.2.1:1234/api/live/sessions/s/turns/t/mcp",
            "http://user:secret@127.0.0.1:1234/api/live/sessions/s/turns/t/mcp",
            "http://127.0.0.1:1234/mcp",
            "http://127.0.0.1:1234/api/live/sessions/s/turns/t/mcp?secret=x",
            "http://127.0.0.1:1234/api/live/sessions/s/turns/t/mcp#private",
        ] {
            let error = proxy(uri).err().expect("endpoint must be rejected");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(error.to_string(), "invalid Live MCP environment");
        }
        proxy("http://127.0.0.1:1234/api/live/sessions/s/turns/t/mcp").unwrap();
        proxy("http://127.0.0.1:80/api/live/sessions/s/turns/t/mcp").unwrap();
        proxy("http://127.0.0.1/api/live/sessions/s/turns/t/mcp").unwrap();
        assert!(proxy("http://127.0.0.1:0/api/live/sessions/s/turns/t/mcp").is_err());
        proxy("http://[::1]:1234/api/live/sessions/s/turns/t/mcp").unwrap();
    }
    #[tokio::test]
    async fn live_mcp_stdio_eof_drains_every_owner_even_after_an_owner_panics() {
        let proxy = proxy("http://127.0.0.1:1234/api/live/sessions/s/turns/t/mcp").unwrap();
        let joined = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&joined);
        let stop = proxy.0.stop.clone();
        {
            let mut work = proxy.0.work.lock().unwrap();
            work.spawn(async { panic!("controlled transport-owner failure") });
            work.spawn(async move {
                stop.cancelled().await;
                tokio::task::yield_now().await;
                observed.store(true, Ordering::Release);
            });
        }
        let (peer, stream) = tokio::io::duplex(4096);
        drop(peer);
        let (read, write) = tokio::io::split(stream);
        let error = proxy.serve_stream(read, write).await.unwrap_err();
        assert_eq!(error.to_string(), "Live transport join failed");
        assert!(
            joined.load(Ordering::Acquire),
            "a failed owner cannot skip joining the next one"
        );
    }
    #[tokio::test]
    async fn live_mcp_stdio_stop_waits_for_the_existing_transport_owner() {
        let proxy = proxy("http://127.0.0.1:1234/api/live/sessions/s/turns/t/mcp").unwrap();
        let signal = proxy.clone();
        let joined = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&joined);
        let stop = proxy.0.stop.clone();
        proxy.0.work.lock().unwrap().spawn(async move {
            stop.cancelled().await;
            tokio::task::yield_now().await;
            observed.store(true, Ordering::Release);
        });
        let (_peer, stream) = tokio::io::duplex(4096);
        let (read, write) = tokio::io::split(stream);
        let owner = tokio::spawn(proxy.serve_stream(read, write));
        signal.request_stop();
        // Cancellation before initialize is an error, but never skips the join.
        assert!(owner.await.unwrap().is_err());
        assert!(joined.load(Ordering::Acquire));
    }
    async fn pending_http_startup_is_joined(eof: bool) {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let handler_entered = Arc::clone(&entered);
        let handler_release = Arc::clone(&release);
        let app = axum::Router::new().route(
            "/api/live/sessions/s/turns/t/mcp",
            axum::routing::post(move |axum::Json(request): axum::Json<serde_json::Value>| {
                let entered = Arc::clone(&handler_entered);
                let release = Arc::clone(&handler_release);
                async move {
                    assert_eq!(request["method"], "server/discover");
                    entered.notify_one();
                    release.acquire().await.unwrap().forget();
                    axum::http::StatusCode::BAD_GATEWAY
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop_server = CancellationToken::new();
        let stopped = stop_server.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
                .unwrap();
        });
        let proxy = proxy(&format!("http://{address}/api/live/sessions/s/turns/t/mcp")).unwrap();
        let signal = proxy.clone();
        let (native, stream) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(stream);
        let mut owner = tokio::spawn(proxy.serve_stream(read, write));
        let (read, write) = tokio::io::split(native);
        let mut client = Some(().serve((read, write)).await.unwrap());
        let peer = client.as_ref().unwrap().peer().clone();
        let request = tokio::spawn(async move { peer.list_all_tools().await });
        tokio::time::timeout(Duration::from_secs(3), entered.notified())
            .await
            .expect("actual first HTTP request must be pending");
        if eof {
            client.take().unwrap().cancel().await.unwrap();
        } else {
            signal.request_stop();
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(200), &mut owner)
                .await
                .is_err(),
            "stdio cannot detach its pending SDK HTTP worker"
        );
        release.add_permits(1);
        tokio::time::timeout(Duration::from_secs(3), owner)
            .await
            .expect("transport close must finish after HTTP responds")
            .expect("stdio owner must not panic")
            .unwrap();
        assert!(request.await.unwrap().is_err());
        if let Some(client) = client {
            client.cancel().await.unwrap();
        }
        stop_server.cancel();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn live_mcp_stdio_stop_joins_pending_http_startup_owner() {
        pending_http_startup_is_joined(false).await;
    }
    #[tokio::test]
    async fn live_mcp_stdio_eof_joins_pending_http_startup_owner() {
        pending_http_startup_is_joined(true).await;
    }
}
