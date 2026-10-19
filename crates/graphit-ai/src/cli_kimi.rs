//! Kimi's native REST/WS one-shot flow. Process retirement belongs to the registered owner.
use crate::cli_codex_wire::{CodexWireError, CodexWireLimits, audit_json_frame};
use graphit_auth::{ProjectReadAdmission, ProjectReadModule};
use graphit_core::file_io::SnapshotContext;
use graphit_http::{
    client::{
        BoundedClient, ClientPolicy, Limits, NetworkError, NetworkRequest, NetworkRequestContext,
    },
    websocket::{
        BoundedWebSocket, CloseCode, Message, WebSocketClient, WebSocketError, WebSocketLimits,
        WebSocketRead,
    },
};
use http::{HeaderMap, HeaderValue, Method, header};
use secrecy::zeroize::Zeroizing;
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::{error::Error, fmt, future::Future, io, path::PathBuf, sync::Arc};
use url::Url;

#[path = "cli_kimi_process.rs"]
mod process;
pub use process::{KimiServer, KimiServerObservation, spawn_kimi_server};

/// Credentials have no diagnostic or serialization representation.
pub struct KimiEndpoint {
    origin: Url,
    token: Zeroizing<String>,
}
impl KimiEndpoint {
    /// Only the official non-banner startup line is admitted. Never log this input.
    pub fn from_ready_line(line: &[u8], maximum: usize) -> Result<Self, KimiReason> {
        if line.len() > maximum {
            return Err(KimiReason::Limit);
        }
        let text = std::str::from_utf8(line).map_err(|_| KimiReason::Startup)?;
        let text = text.strip_suffix('\n').unwrap_or(text);
        let text = text.strip_suffix('\r').unwrap_or(text);
        let address = text
            .strip_prefix("Kimi server: ")
            .ok_or(KimiReason::Startup)?;
        let (base, token) = address.split_once("#token=").ok_or(KimiReason::Endpoint)?;
        // URL parsing normalizes numeric aliases and dot segments. Readiness is the
        // exact canonical line produced by Kimi; credentials never enter Url storage.
        if !base.starts_with("http://127.0.0.1:") {
            return Err(KimiReason::Endpoint);
        }
        let origin = Url::parse(base).map_err(|_| KimiReason::Startup)?;
        if origin.as_str() != base
            || origin.scheme() != "http"
            || origin.host_str() != Some("127.0.0.1")
            || origin.port().is_none_or(|port| port == 0)
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
            || token.is_empty()
            || !token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Err(KimiReason::Endpoint);
        }
        let token = Zeroizing::new(token.to_owned());
        Ok(Self { origin, token })
    }
    fn headers(&self) -> Result<HeaderMap, Cause> {
        let value = Zeroizing::new(format!("Bearer {}", self.token.as_str()));
        let mut value =
            HeaderValue::from_str(&value).map_err(|_| Cause::Reason(KimiReason::Endpoint))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, value);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(headers)
    }
    fn path(&self, segments: &[&str]) -> Result<String, Cause> {
        let mut destination = self.origin.clone();
        destination
            .path_segments_mut()
            .map_err(|_| Cause::Reason(KimiReason::Endpoint))?
            .clear()
            .extend(segments);
        Ok(destination.into())
    }
}

#[derive(Clone, Copy)]
pub struct KimiLimits {
    pub wire: CodexWireLimits,
    pub network: Limits,
    pub websocket: WebSocketLimits,
    pub prompt_text_bytes: usize,
    pub response_text_bytes: usize,
}
/// Explicit native identity; no latest session or fallback creation.
pub enum KimiSessionStart {
    Create,
    Resume(String),
}
pub struct KimiRequest {
    pub session: KimiSessionStart,
    pub cwd: PathBuf,
    /// None preserves the native session/default model without a prompt override.
    pub model: Option<String>,
    pub prompt: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KimiReason {
    Policy,
    Startup,
    Endpoint,
    Limit,
    Wire,
    Protocol,
    Session,
    Prompt,
    Permission,
    Terminal,
    MissingText,
    Closed,
    Gap,
    Allocation,
    Admission,
    Http(u16),
    Native(i64),
    Context(io::ErrorKind),
    Sink(io::ErrorKind),
    Network(NetworkError),
    WebSocket,
}
pub trait KimiSink: Send {
    /// Serial borrowed native events; a dropped callback must leave no detached work.
    fn push<'a>(
        &'a mut self,
        event: &'a RawValue,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = io::Result<()>> + Send + 'a;
}
#[derive(Default)]
pub struct KimiReceipt {
    session: Option<String>,
    prompt: Option<String>,
    version: Option<String>,
    turn: Option<u64>,
    text: String,
    terminal: Option<Box<RawValue>>,
    native_error: Option<Box<RawValue>>,
    eligible: bool,
}
impl KimiReceipt {
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    pub fn turn(&self) -> Option<u64> {
        self.turn
    }
    pub fn terminal(&self) -> Option<&RawValue> {
        self.terminal.as_deref()
    }
    /// Native failure payload for explicit inspection, never diagnostics.
    pub fn native_error(&self) -> Option<&RawValue> {
        self.native_error.as_deref()
    }
    pub fn candidate_text(&self) -> &str {
        &self.text
    }
    /// The App must additionally prove process EOF, status, cleanup/join and authority.
    pub fn eligible_text(&self) -> Option<&str> {
        self.eligible.then_some(&self.text)
    }
}
impl fmt::Debug for KimiReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KimiReceipt")
            .field("terminal", &self.terminal.is_some())
            .field("eligible", &self.eligible)
            .finish_non_exhaustive()
    }
}
enum Cause {
    Reason(KimiReason),
    Network(NetworkError),
    WebSocket(WebSocketError),
    Context(io::Error),
    Sink(io::Error),
    Native(Box<dyn Error + Send + Sync>),
    Json(serde_json::Error),
    Wire(CodexWireError),
    WirePolicy(CodexWireError),
    Allocation(std::collections::TryReserveError),
}
impl Cause {
    fn reason(&self) -> KimiReason {
        match self {
            Self::Reason(reason) => *reason,
            Self::Network(error) => KimiReason::Network(*error),
            Self::WebSocket(_) => KimiReason::WebSocket,
            Self::Context(error) => KimiReason::Context(error.kind()),
            Self::Sink(error) => KimiReason::Sink(error.kind()),
            Self::Native(_) => KimiReason::Admission,
            Self::Json(_) | Self::Wire(_) => KimiReason::Wire,
            Self::WirePolicy(_) => KimiReason::Policy,
            Self::Allocation(_) => KimiReason::Allocation,
        }
    }
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reason(_) => None,
            Self::Network(error) => Some(error),
            Self::WebSocket(error) => Some(error),
            Self::Context(error) | Self::Sink(error) => Some(error),
            Self::Native(error) => Some(error.as_ref()),
            Self::Json(error) => Some(error),
            Self::Wire(error) | Self::WirePolicy(error) => Some(error),
            Self::Allocation(error) => Some(error),
        }
    }
}
impl From<NetworkError> for Cause {
    fn from(value: NetworkError) -> Self {
        Self::Network(value)
    }
}
impl From<WebSocketError> for Cause {
    fn from(value: WebSocketError) -> Self {
        Self::WebSocket(value)
    }
}
impl From<serde_json::Error> for Cause {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
pub struct KimiFailure {
    receipt: Box<KimiReceipt>,
    cause: Cause,
}
impl KimiFailure {
    pub fn reason(&self) -> KimiReason {
        self.cause.reason()
    }
    pub fn receipt(&self) -> &KimiReceipt {
        &self.receipt
    }
    /// Explicit causal inspection. The caller must not log native error payloads.
    pub fn native_cause(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.source()
    }
}
impl fmt::Debug for KimiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("KimiFailure").field(&self.reason()).finish()
    }
}
impl fmt::Display for KimiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Kimi {:?}", self.reason())
    }
}
impl Error for KimiFailure {}

fn check_knowledge(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
) -> Result<(), Cause> {
    original.check().map_err(Cause::Context)?;
    if admission.module() != ProjectReadModule::Knowledge {
        return Err(Cause::Reason(KimiReason::Admission));
    }
    admission
        .recheck()
        .map_err(|error| Cause::Native(Box::new(error)))?;
    let request = admission
        .request()
        .map_err(|error| Cause::Native(Box::new(error)))?;
    if !Arc::ptr_eq(original, request.snapshot()) {
        return Err(Cause::Reason(KimiReason::Admission));
    }
    Ok(())
}
fn audit(bytes: &[u8], limits: KimiLimits, original: &SnapshotContext) -> Result<(), Cause> {
    original.check().map_err(Cause::Context)?;
    audit_json_frame(bytes, &limits.wire, original)
        .map(|_| ())
        .map_err(Cause::Wire)
}
fn identifier(value: &str, maximum: usize) -> Result<(), Cause> {
    if value.is_empty() || value.len() > maximum {
        Err(Cause::Reason(KimiReason::Limit))
    } else {
        Ok(())
    }
}
struct Client<'a> {
    original: &'a Arc<SnapshotContext>,
    before: &'a (dyn Fn() -> Result<(), Cause> + Sync),
    endpoint: &'a KimiEndpoint,
    http: BoundedClient,
    limits: KimiLimits,
}
impl Client<'_> {
    fn context(&self) -> Result<NetworkRequestContext<'_>, Cause> {
        Ok(NetworkRequestContext::new(
            self.original.required_deadline().map_err(Cause::Context)?,
            self.original.cancellation(),
        ))
    }
    async fn request(
        &self,
        method: Method,
        path: &[&str],
        body: Value,
        receipt: &mut KimiReceipt,
    ) -> Result<Value, Cause> {
        (self.before)()?;
        let bytes = Zeroizing::new(if method == Method::GET {
            Vec::new()
        } else {
            serde_json::to_vec(&body)?
        });
        let destination = self.endpoint.path(path)?;
        let headers = self.endpoint.headers()?;
        let mut gate = || (self.before)();
        let response = self
            .http
            .execute_guarded(
                NetworkRequest {
                    method,
                    url: &destination,
                    headers: &headers,
                    body: &bytes,
                },
                self.context()?,
                &mut gate,
            )
            .await
            .map_err(|error| match error {
                graphit_http::client::GuardedNetworkError::Network { source, .. } => {
                    Cause::Network(source)
                }
                graphit_http::client::GuardedNetworkError::Admission { source, .. } => source,
            })?;
        (self.before)()?;
        if !response.status().is_success() {
            // Native JSON errors remain inspectable privately; malformed bodies do
            // not replace the first real HTTP status with a JSON inspection error.
            if audit(response.body(), self.limits, self.original).is_ok() {
                receipt.native_error = serde_json::from_slice(response.body()).ok();
            }
            return Err(Cause::Reason(KimiReason::Http(response.status().as_u16())));
        }
        audit(response.body(), self.limits, self.original)?;
        let envelope: Value = serde_json::from_slice(response.body())?;
        let code = envelope
            .get("code")
            .and_then(Value::as_i64)
            .ok_or(Cause::Reason(KimiReason::Protocol))?;
        if code != 0 {
            receipt.native_error = Some(serde_json::from_slice(response.body())?);
            return Err(Cause::Reason(KimiReason::Native(code)));
        }
        envelope
            .get("data")
            .cloned()
            .ok_or(Cause::Reason(KimiReason::Protocol))
    }
    async fn receive(&self, socket: &mut BoundedWebSocket<'_>) -> Result<Zeroizing<String>, Cause> {
        loop {
            (self.before)()?;
            let message = socket.receive().await?;
            (self.before)()?;
            match message {
                WebSocketRead::Message(Message::Text(text)) => {
                    audit(text.as_bytes(), self.limits, self.original)?;
                    return Ok(Zeroizing::new(text.to_string()));
                }
                WebSocketRead::Message(Message::Ping(bytes)) => {
                    socket.send(Message::Pong(bytes)).await?;
                }
                WebSocketRead::Message(Message::Pong(_)) | WebSocketRead::Wakeup => (),
                WebSocketRead::Closed | WebSocketRead::Message(Message::Close { .. }) => {
                    return Err(Cause::Reason(KimiReason::Closed));
                }
                _ => return Err(Cause::Reason(KimiReason::Protocol)),
            }
        }
    }
}
#[derive(Deserialize)]
struct Frame<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(default)]
    id: Option<&'a str>,
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    session_id: Option<&'a str>,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(borrow)]
    payload: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(default)]
    agent_id: Option<&'a str>,
    #[serde(default)]
    turn_id: Option<u64>,
    #[serde(default)]
    prompt_id: Option<&'a str>,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    reason: Option<&'a str>,
    #[serde(default)]
    interrupt_reason: Option<&'a str>,
    #[serde(default, borrow)]
    error: Option<&'a RawValue>,
}

/// Knowledge consumers retain their typed admission and exact original request.
pub async fn run_kimi<S: KimiSink>(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    endpoint: &KimiEndpoint,
    request: KimiRequest,
    limits: KimiLimits,
    sink: &mut S,
) -> Result<KimiReceipt, KimiFailure> {
    let before = || check_knowledge(original, admission);
    run_checked(original, &before, endpoint, request, limits, sink).await
}

/// Execute under the caller's current admission and the same original context.
/// The recheck must retain the real caller authority; native metadata grants none.
pub async fn run_kimi_guarded<S: KimiSink>(
    original: &Arc<SnapshotContext>,
    recheck: &(dyn Fn() -> io::Result<()> + Sync),
    endpoint: &KimiEndpoint,
    request: KimiRequest,
    limits: KimiLimits,
    sink: &mut S,
) -> Result<KimiReceipt, KimiFailure> {
    let before = || {
        original.check().map_err(Cause::Context)?;
        recheck().map_err(|error| Cause::Native(Box::new(error)))
    };
    run_checked(original, &before, endpoint, request, limits, sink).await
}
async fn run_checked<S: KimiSink>(
    original: &Arc<SnapshotContext>,
    before: &(dyn Fn() -> Result<(), Cause> + Sync),
    endpoint: &KimiEndpoint,
    request: KimiRequest,
    limits: KimiLimits,
    sink: &mut S,
) -> Result<KimiReceipt, KimiFailure> {
    let mut receipt = KimiReceipt::default();
    match execute(
        original,
        before,
        endpoint,
        request,
        limits,
        sink,
        &mut receipt,
    )
    .await
    {
        Ok(()) => {
            receipt.eligible = true;
            Ok(receipt)
        }
        Err(cause) => Err(KimiFailure {
            receipt: Box::new(receipt),
            cause,
        }),
    }
}
async fn execute<S: KimiSink>(
    original: &Arc<SnapshotContext>,
    before: &(dyn Fn() -> Result<(), Cause> + Sync),
    endpoint: &KimiEndpoint,
    request: KimiRequest,
    limits: KimiLimits,
    sink: &mut S,
    receipt: &mut KimiReceipt,
) -> Result<(), Cause> {
    before()?;
    limits.wire.validate().map_err(Cause::WirePolicy)?;
    if limits.prompt_text_bytes == 0 || limits.response_text_bytes == 0 {
        return Err(Cause::Reason(KimiReason::Policy));
    }
    if request.prompt.len() > limits.prompt_text_bytes {
        return Err(Cause::Reason(KimiReason::Limit));
    }
    if let Some(model) = request.model.as_deref() {
        identifier(model, limits.wire.id_bytes)?;
    }
    let cwd = request
        .cwd
        .to_str()
        .filter(|_| request.cwd.is_absolute())
        .ok_or(Cause::Reason(KimiReason::Policy))?;
    let policy = || ClientPolicy {
        origin: endpoint.origin.as_str(),
        allow_loopback_http: true,
        proxy: None,
        additional_roots: &[],
    };
    let client = Client {
        original,
        before,
        endpoint,
        http: BoundedClient::new(policy(), limits.network)?,
        limits,
    };
    let version = client
        .request(Method::GET, &["api", "v1", "meta"], Value::Null, receipt)
        .await?;
    let version = version
        .get("server_version")
        .and_then(Value::as_str)
        .ok_or(Cause::Reason(KimiReason::Protocol))?;
    identifier(version, limits.wire.id_bytes)?;
    receipt.version = Some(version.to_owned());
    let (session, expected) = match request.session {
        KimiSessionStart::Create => (
            client
                .request(
                    Method::POST,
                    &["api", "v1", "sessions"],
                    json!({"metadata":{"cwd":cwd}}),
                    receipt,
                )
                .await?,
            None,
        ),
        KimiSessionStart::Resume(id) => {
            identifier(&id, limits.wire.id_bytes)?;
            let session = client
                .request(
                    Method::GET,
                    &["api", "v1", "sessions", &id],
                    Value::Null,
                    receipt,
                )
                .await?;
            (session, Some(id))
        }
    };
    let session_id = session
        .get("id")
        .and_then(Value::as_str)
        .ok_or(Cause::Reason(KimiReason::Session))?;
    identifier(session_id, limits.wire.id_bytes)?;
    if let Some(expected) = expected
        && (session_id != expected
            || session
                .get("metadata")
                .and_then(|value| value.get("cwd"))
                .and_then(Value::as_str)
                != Some(cwd))
    {
        return Err(Cause::Reason(KimiReason::Session));
    }
    let session = session_id;
    receipt.session = Some(session.to_owned());
    let websocket = WebSocketClient::new(policy(), limits.network, limits.websocket)?;
    before()?;
    let mut socket = websocket
        .connect(
            &endpoint.path(&["api", "v1", "ws"])?,
            endpoint.headers()?,
            client.context()?,
        )
        .await?;
    let hello = client.receive(&mut socket).await?;
    let hello: Frame<'_> = serde_json::from_str(&hello)?;
    let payload: Value = serde_json::from_str(hello.payload.get())?;
    if hello.kind != "server_hello"
        || payload.get("protocol_version").and_then(Value::as_u64) != Some(2)
    {
        return Err(Cause::Reason(KimiReason::Protocol));
    }
    let subscribe = json!({"type":"subscribe","id":"subscribe","payload":{"session_ids":[session],"agent_filter":{(session):["main"]}}});
    before()?;
    socket.send(Message::Text(subscribe.to_string())).await?;
    loop {
        let bytes = client.receive(&mut socket).await?;
        let frame: Frame<'_> = serde_json::from_str(&bytes)?;
        if frame.kind != "ack" {
            if frame.kind == "resync_required" {
                return Err(Cause::Reason(KimiReason::Gap));
            }
            continue;
        }
        if frame.id != Some("subscribe") {
            return Err(Cause::Reason(KimiReason::Protocol));
        }
        let code = frame.code.ok_or(Cause::Reason(KimiReason::Protocol))?;
        if code != 0 {
            receipt.native_error = Some(serde_json::from_str(&bytes)?);
            return Err(Cause::Reason(KimiReason::Native(code)));
        }
        let payload: Value = serde_json::from_str(frame.payload.get())?;
        if !payload
            .get("accepted_subscriptions")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(session)))
            || !payload
                .get("resync_required")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        {
            return Err(Cause::Reason(KimiReason::Gap));
        }
        break;
    }
    let mut prompt_body = json!({
        "content": [{"type":"text","text":request.prompt}],
        "agent_id": "main",
        "permission_mode": "manual",
    });
    if let Some(model) = request.model {
        prompt_body["model"] = Value::String(model);
    }
    let prompt = client
        .request(
            Method::POST,
            &["api", "v1", "sessions", session, "prompts"],
            prompt_body,
            receipt,
        )
        .await?;
    let prompt = prompt
        .get("prompt_id")
        .and_then(Value::as_str)
        .ok_or(Cause::Reason(KimiReason::Prompt))?;
    identifier(prompt, limits.wire.id_bytes)?;
    receipt.prompt = Some(prompt.to_owned());
    let mut state = TurnState::default();
    loop {
        let bytes = client.receive(&mut socket).await?;
        let frame: Frame<'_> = serde_json::from_str(&bytes)?;
        if frame.kind == "resync_required" {
            return Err(Cause::Reason(KimiReason::Gap));
        }
        if frame.session_id != Some(session) {
            continue;
        }
        let event: Event<'_> = serde_json::from_str(frame.payload.get())?;
        if frame.kind != event.kind {
            return Err(Cause::Reason(KimiReason::Protocol));
        }
        observe_event(
            receipt,
            &mut state,
            &frame,
            &event,
            prompt,
            limits.wire.id_bytes,
            limits.response_text_bytes,
        )?;
        // Native failures may contain provider credentials/details. Keep their exact
        // payload private and classify before the raw progress callback.
        if event.kind == "error" {
            receipt.native_error = Some(serde_json::from_str(frame.payload.get())?);
            if event.agent_id.is_none_or(|agent| agent == "main") {
                return Err(Cause::Reason(KimiReason::Terminal));
            }
            continue;
        }
        if event.agent_id == Some("main") && event.kind == "turn.ended" {
            accept_terminal(&event, receipt)?;
        }
        before()?;
        let native_frame: &RawValue = serde_json::from_str(&bytes)?;
        deliver(original, sink, native_frame).await?;
        before()?;
        if matches!(
            event.kind,
            "event.approval.requested" | "event.question.requested"
        ) {
            return Err(Cause::Reason(KimiReason::Permission));
        }
        if event.agent_id == Some("main") && event.kind == "turn.ended" {
            break;
        }
    }
    before()?;
    socket.close(CloseCode::Normal).await?;
    client
        .request(Method::POST, &["api", "v1", "shutdown"], json!({}), receipt)
        .await?;
    before()
}

async fn deliver<S: KimiSink>(
    original: &SnapshotContext,
    sink: &mut S,
    event: &RawValue,
) -> Result<(), Cause> {
    original.check().map_err(Cause::Context)?;
    let deadline = original.required_deadline().map_err(Cause::Context)?;
    let delivery = sink.push(event, original);
    tokio::select! {
        biased;
        _ = original.cancellation().cancelled() => Err(Cause::Context(io::ErrorKind::Interrupted.into())),
        _ = tokio::time::sleep_until(deadline.into()) => Err(Cause::Context(io::ErrorKind::TimedOut.into())),
        result = delivery => result.map_err(Cause::Sink),
    }
}

#[derive(Default)]
struct TurnState {
    has_step: bool,
    utf16: usize,
}
fn observe_event(
    receipt: &mut KimiReceipt,
    state: &mut TurnState,
    frame: &Frame<'_>,
    event: &Event<'_>,
    prompt: &str,
    identifier_bytes: usize,
    response_text_bytes: usize,
) -> Result<(), Cause> {
    if matches!(
        event.kind,
        "turn.started" | "turn.step.started" | "assistant.delta" | "turn.ended"
    ) {
        identifier(
            event.agent_id.ok_or(Cause::Reason(KimiReason::Protocol))?,
            identifier_bytes,
        )?;
        if event.turn_id.is_none() {
            return Err(Cause::Reason(KimiReason::Protocol));
        }
        if event.kind == "assistant.delta" && event.delta.is_none() {
            return Err(Cause::Reason(KimiReason::Protocol));
        }
    }

    if event.agent_id == Some("main") {
        if event.kind == "turn.started" {
            if receipt.turn.is_some() || event.prompt_id != Some(prompt) {
                return Err(Cause::Reason(KimiReason::Prompt));
            }
            receipt.turn = Some(event.turn_id.ok_or(Cause::Reason(KimiReason::Protocol))?);
        } else if matches!(
            event.kind,
            "turn.step.started" | "assistant.delta" | "turn.ended"
        ) {
            if receipt.turn.is_none() || event.turn_id != receipt.turn {
                return Err(Cause::Reason(KimiReason::Session));
            }
            match event.kind {
                "turn.step.started" => {
                    receipt.text.clear();
                    state.utf16 = 0;
                    state.has_step = true;
                }
                "assistant.delta" => {
                    if !state.has_step || frame.offset != Some(state.utf16 as u64) {
                        return Err(Cause::Reason(KimiReason::Gap));
                    }
                    let text = event
                        .delta
                        .as_deref()
                        .ok_or(Cause::Reason(KimiReason::Protocol))?;
                    if receipt
                        .text
                        .len()
                        .checked_add(text.len())
                        .is_none_or(|bytes| bytes > response_text_bytes)
                    {
                        return Err(Cause::Reason(KimiReason::Limit));
                    }
                    receipt
                        .text
                        .try_reserve(text.len())
                        .map_err(Cause::Allocation)?;
                    receipt.text.push_str(text);
                    state.utf16 = state
                        .utf16
                        .checked_add(text.encode_utf16().count())
                        .ok_or(Cause::Reason(KimiReason::Limit))?;
                }
                "turn.ended" => {
                    receipt.terminal = Some(serde_json::from_str(frame.payload.get())?);
                }
                _ => unreachable!(),
            }
        }
    }

    Ok(())
}

fn accept_terminal(event: &Event<'_>, receipt: &mut KimiReceipt) -> Result<(), Cause> {
    if let Some(error) = event.error {
        receipt.native_error = Some(serde_json::from_str(error.get())?);
    }
    if event.reason != Some("completed")
        || event.error.is_some()
        || event.interrupt_reason.is_some()
    {
        return Err(Cause::Reason(KimiReason::Terminal));
    }
    if receipt.text.is_empty() {
        return Err(Cause::Reason(KimiReason::MissingText));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observe(
        receipt: &mut KimiReceipt,
        state: &mut TurnState,
        payload: Value,
        offset: Option<u64>,
        limit: usize,
    ) -> Result<(), KimiReason> {
        let payload = serde_json::to_string(&payload).unwrap();
        let event: Event<'_> = serde_json::from_str(&payload).unwrap();
        let frame = Frame {
            kind: event.kind,
            id: None,
            code: None,
            session_id: Some("native_session"),
            offset,
            payload: serde_json::from_str(&payload).unwrap(),
        };
        observe_event(receipt, state, &frame, &event, "native_prompt", 128, limit)
            .map_err(|cause| cause.reason())
    }
    fn start(receipt: &mut KimiReceipt, state: &mut TurnState) {
        observe(
            receipt,
            state,
            json!({"type":"turn.started","agentId":"main","turnId":7,"promptId":"native_prompt"}),
            None,
            128,
        )
        .unwrap();
        observe(
            receipt,
            state,
            json!({"type":"turn.step.started","agentId":"main","turnId":7}),
            None,
            128,
        )
        .unwrap();
    }
    #[test]
    fn principal_step_reset_and_utf16_offsets_keep_literal_text_without_hook_child_or_compaction() {
        let mut receipt = KimiReceipt::default();
        let mut state = TurnState::default();
        start(&mut receipt, &mut state);
        observe(
            &mut receipt,
            &mut state,
            json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"interim"}),
            Some(0),
            128,
        )
        .unwrap();
        for payload in [
            json!({"type":"hook.result","agentId":"main","content":"excluded"}),
            json!({"type":"assistant.delta","agentId":"child","turnId":7,"delta":"excluded"}),
            json!({"type":"compaction.completed","agentId":"main","result":{"summary":"excluded"}}),
        ] {
            observe(&mut receipt, &mut state, payload, Some(0), 128).unwrap();
        }
        assert_eq!(receipt.text, "interim");
        observe(
            &mut receipt,
            &mut state,
            json!({"type":"turn.step.started","agentId":"main","turnId":7}),
            None,
            128,
        )
        .unwrap();
        observe(
            &mut receipt,
            &mut state,
            json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"  🦀\n"}),
            Some(0),
            128,
        )
        .unwrap();
        // Two spaces + surrogate pair + newline are five JS UTF-16 units, seven UTF-8 bytes.
        observe(
            &mut receipt,
            &mut state,
            json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"β\t"}),
            Some(5),
            128,
        )
        .unwrap();
        assert_eq!(receipt.candidate_text(), "  🦀\nβ\t");
        assert_eq!(state.utf16, 7);
        assert!(receipt.eligible_text().is_none());
    }
    #[test]
    fn wrong_prompt_turn_gap_missing_step_and_byte_limit_preserve_first_failure() {
        let mut receipt = KimiReceipt::default();
        let mut state = TurnState::default();
        assert_eq!(
            observe(
                &mut receipt,
                &mut state,
                json!({"type":"turn.started","agentId":"main","turnId":7,"promptId":"foreign"}),
                None,
                128
            ),
            Err(KimiReason::Prompt)
        );
        start(&mut receipt, &mut state);
        assert_eq!(
            observe(
                &mut receipt,
                &mut state,
                json!({"type":"assistant.delta","agentId":"main","turnId":8,"delta":"x"}),
                Some(0),
                128
            ),
            Err(KimiReason::Session)
        );
        assert_eq!(
            observe(
                &mut receipt,
                &mut state,
                json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"x"}),
                None,
                128
            ),
            Err(KimiReason::Gap)
        );
        assert_eq!(
            observe(
                &mut receipt,
                &mut state,
                json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"🦀"}),
                Some(0),
                3
            ),
            Err(KimiReason::Limit)
        );
        assert!(receipt.text.is_empty());
        state.has_step = false;
        assert_eq!(
            observe(
                &mut receipt,
                &mut state,
                json!({"type":"assistant.delta","agentId":"main","turnId":7,"delta":"x"}),
                Some(0),
                128
            ),
            Err(KimiReason::Gap)
        );
    }
    #[test]
    fn native_failed_cancelled_blocked_error_or_interrupt_cannot_be_completed_answer() {
        for terminal in [
            json!({"type":"turn.ended","reason":"failed"}),
            json!({"type":"turn.ended","reason":"cancelled"}),
            json!({"type":"turn.ended","reason":"blocked"}),
            json!({"type":"turn.ended","reason":"completed","error":{"message":"synthetic-private-native-cause"}}),
            json!({"type":"turn.ended","reason":"completed","interruptReason":"user_cancelled"}),
        ] {
            let terminal = serde_json::to_string(&terminal).unwrap();
            let event: Event<'_> = serde_json::from_str(&terminal).unwrap();
            let mut receipt = KimiReceipt {
                text: "candidate".into(),
                ..Default::default()
            };
            assert!(matches!(
                accept_terminal(&event, &mut receipt),
                Err(Cause::Reason(KimiReason::Terminal))
            ));
            assert!(receipt.eligible_text().is_none());
            assert!(!format!("{receipt:?}").contains("synthetic-private"));
            if event.error.is_some() {
                assert!(
                    receipt
                        .native_error()
                        .unwrap()
                        .get()
                        .contains("synthetic-private-native-cause")
                );
            }
        }
        let event: Event<'_> =
            serde_json::from_str(r#"{"type":"turn.ended","reason":"completed"}"#).unwrap();
        assert!(matches!(
            accept_terminal(&event, &mut KimiReceipt::default()),
            Err(Cause::Reason(KimiReason::MissingText))
        ));
        assert!(
            accept_terminal(
                &event,
                &mut KimiReceipt {
                    text: "literal".into(),
                    ..Default::default()
                }
            )
            .is_ok()
        );
    }
    #[tokio::test]
    async fn cancelled_backpressured_callback_is_dropped_under_the_same_original() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Dropped(Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        struct PendingSink {
            original: Arc<SnapshotContext>,
            entered: Arc<tokio::sync::Notify>,
            dropped: Arc<AtomicBool>,
        }
        impl KimiSink for PendingSink {
            async fn push<'a>(
                &'a mut self,
                _: &'a RawValue,
                original: &'a SnapshotContext,
            ) -> io::Result<()> {
                assert!(std::ptr::eq(original, self.original.as_ref()));
                let _guard = Dropped(Arc::clone(&self.dropped));
                self.entered.notify_one();
                std::future::pending().await
            }
        }
        let original = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            graphit_core::file_lock::Cancellation::default(),
        ));
        let entered = Arc::new(tokio::sync::Notify::new());
        let dropped = Arc::new(AtomicBool::new(false));
        let mut sink = PendingSink {
            original: Arc::clone(&original),
            entered: Arc::clone(&entered),
            dropped: Arc::clone(&dropped),
        };
        let context = Arc::clone(&original);
        let task = tokio::spawn(async move {
            let event: &RawValue = serde_json::from_str(r#"{"type":"assistant.delta"}"#).unwrap();
            deliver(&context, &mut sink, event)
                .await
                .map_err(|cause| cause.reason())
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        assert!(!task.is_finished());
        original.cancellation().cancel();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap(),
            Err(KimiReason::Context(io::ErrorKind::Interrupted))
        );
        assert!(dropped.load(Ordering::SeqCst));
    }
}
