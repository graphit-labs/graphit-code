//! Knowledge Ask uses one authenticated SDK connection and one progress gate per call.
use super::*;
use graphit_core::file_io::SnapshotContext;
use rmcp::{
    ClientHandler, RoleClient,
    model::{
        CancelledNotificationParam, ClientRequest, ProgressNotificationParam, ProgressToken,
        ServerResult,
    },
    service::{NotificationContext, Peer, PeerRequestOptions, ServiceError},
};
use serde_json::{Value, json};
use std::sync::Mutex as SyncMutex;
use tokio::sync::Notify;

pub(super) const TOOL: &str = "graphit_knowledge_ask";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeliveryError {
    Cancelled,
    Deadline,
    Token,
    Ordinal,
    Duplicate,
    Limit,
    Forward,
    Contract,
    Tail,
    Gap,
    Transport,
}
impl DeliveryError {
    fn label(self) -> &'static str {
        match self {
            Self::Cancelled => "Knowledge Ask forwarding cancelled",
            Self::Deadline => "Knowledge Ask forwarding deadline expired",
            Self::Token => "Knowledge Ask progress token invalid",
            Self::Ordinal => "Knowledge Ask progress ordinal invalid",
            Self::Duplicate => "Knowledge Ask progress ordinal repeated",
            Self::Limit => "Knowledge Ask forwarding content limit exceeded",
            Self::Forward => "Knowledge Ask progress delivery failed",
            Self::Contract => "Knowledge Ask native receipt invalid",
            Self::Tail => "Knowledge Ask observed progress exceeds accepted prefix",
            Self::Gap => "Knowledge Ask accepted progress prefix missing",
            Self::Transport => "Knowledge Ask transport or credential failed",
        }
    }
}
fn stopped(original: &SnapshotContext) -> Option<DeliveryError> {
    original.check().err().map(|error| {
        if error.kind() == io::ErrorKind::TimedOut {
            DeliveryError::Deadline
        } else {
            DeliveryError::Cancelled
        }
    })
}
fn token_fits(token: &ProgressToken, limit: usize) -> bool {
    match &token.0 {
        rmcp::model::NumberOrString::String(value) => value.len() <= limit,
        rmcp::model::NumberOrString::Number(_) => true,
    }
}
enum NativeDeliveryError {
    Forward(ServiceError),
    Cancellation(ServiceError),
    Cleanup(tokio::task::JoinError),
    Context(io::Error),
}
impl NativeDeliveryError {
    fn cause(&self) -> &(dyn std::error::Error + 'static) {
        match self {
            Self::Forward(error) | Self::Cancellation(error) => error,
            Self::Cleanup(error) => error,
            Self::Context(error) => error,
        }
    }
}
struct GateState {
    bound: Option<ProgressToken>,
    closed: bool,
    active: usize,
    next: u64,
    observed: u64,
    sending: bool,
    first: Option<DeliveryError>,
    native: Option<NativeDeliveryError>,
}
struct Gate {
    state: SyncMutex<GateState>,
    changed: Notify,
    closed: CancellationToken,
    original: Arc<SnapshotContext>,
    deadline: std::time::Instant,
    limits: ContentLimits,
    peer: Peer<RoleServer>,
    upstream: Option<ProgressToken>,
    #[cfg(test)]
    processed: Notify,
}
impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.lock().expect("progress state");
        f.debug_struct("KnowledgeAskProgress")
            .field("closed", &state.closed)
            .field("active", &state.active)
            .field("next", &state.next)
            .field("first", &state.first)
            .finish_non_exhaustive()
    }
}
struct Entered(Arc<Gate>);
impl Drop for Entered {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().expect("progress state");
        state.active -= 1;
        drop(state);
        self.0.changed.notify_waiters();
    }
}
impl Gate {
    fn new(
        original: Arc<SnapshotContext>,
        deadline: std::time::Instant,
        limits: ContentLimits,
        peer: Peer<RoleServer>,
        upstream: Option<ProgressToken>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: SyncMutex::new(GateState {
                bound: None,
                closed: false,
                active: 0,
                next: 1,
                observed: 0,
                sending: false,
                first: None,
                native: None,
            }),
            changed: Notify::new(),
            closed: CancellationToken::new(),
            original,
            deadline,
            limits,
            peer,
            upstream,
            #[cfg(test)]
            processed: Notify::new(),
        })
    }
    fn fail(&self, error: DeliveryError) {
        {
            let mut state = self.state.lock().expect("progress state");
            state.first.get_or_insert(error);
            if state.native.is_none()
                && matches!(error, DeliveryError::Cancelled | DeliveryError::Deadline)
                && let Err(cause) = self.original.check()
            {
                state.native = Some(NativeDeliveryError::Context(cause));
            }
        }
        self.changed.notify_waiters();
        self.original.cancellation().cancel();
    }
    fn fail_native(&self, error: DeliveryError, native: NativeDeliveryError) {
        {
            let mut state = self.state.lock().expect("progress state");
            state.first.get_or_insert(error);
            if state.native.is_none() {
                state.native = Some(native);
            }
        }
        self.changed.notify_waiters();
        self.original.cancellation().cancel();
    }
    fn bind(&self, token: ProgressToken) {
        if !token_fits(&token, self.limits.max_scalar_bytes) {
            self.fail(DeliveryError::Limit);
            return;
        }
        self.state.lock().expect("progress state").bound = Some(token);
        self.changed.notify_waiters();
    }
    fn close(&self) {
        self.state.lock().expect("progress state").closed = true;
        self.closed.cancel();
        self.changed.notify_waiters();
    }
    fn first(&self) -> Option<DeliveryError> {
        self.state.lock().expect("progress state").first
    }
    async fn drain(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.state.lock().expect("progress state").active == 0 {
                return;
            }
            changed.await;
        }
    }
    async fn forward(
        self: &Arc<Self>,
        mut event: ProgressNotificationParam,
        mut context: NotificationContext<RoleClient>,
    ) {
        let _entered = {
            let mut state = self.state.lock().expect("progress state");
            if state.closed {
                return;
            }
            state.active += 1;
            Entered(Arc::clone(self))
        };
        self.changed.notify_waiters();
        if let Some(error) = stopped(&self.original) {
            self.fail(error);
            return;
        }
        if !event.progress.is_finite()
            || event.progress <= 0.0
            || event.progress.fract() != 0.0
            || event.progress > 9_007_199_254_740_992.0
        {
            self.fail(DeliveryError::Ordinal);
            return;
        }
        let ordinal = event.progress as u64;
        {
            let mut state = self.state.lock().expect("progress state");
            state.observed = state.observed.max(ordinal);
        }
        loop {
            // Register before examining state: bind/ordinal completion cannot lose a wakeup.
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let action = {
                let mut state = self.state.lock().expect("progress state");
                if state.closed || state.first.is_some() {
                    return;
                }
                match &state.bound {
                    None => None,
                    Some(token) if token != &event.progress_token => {
                        Some(Err(DeliveryError::Token))
                    }
                    Some(_) if ordinal < state.next => Some(Err(DeliveryError::Duplicate)),
                    Some(_) if ordinal == state.next && !state.sending => {
                        state.sending = true;
                        Some(Ok(()))
                    }
                    Some(_) => None,
                }
            };
            match action {
                Some(Err(error)) => {
                    self.fail(error);
                    return;
                }
                Some(Ok(())) => break,
                None => tokio::select! {
                    () = &mut changed => (),
                    () = self.closed.cancelled() => return,
                    () = self.original.cancellation().cancelled() => { self.fail(DeliveryError::Cancelled); return; },
                    () = tokio::time::sleep_until(self.deadline.into()) => { self.fail(DeliveryError::Deadline); return; },
                },
            }
        }
        let result = async {
            if let Some(error) = stopped(&self.original) { return Err(error); }
            if let Some(token) = &self.upstream {
                if !token_fits(token, self.limits.max_scalar_bytes)
                    || event.message.as_ref().is_some_and(|message| message.len() > self.limits.max_output_bytes) {
                    return Err(DeliveryError::Limit);
                }
                event.progress_token = token.clone();
                event.meta = context.extensions.remove::<rmcp::model::NotificationMetaObject>()
                    .or(event.meta).or_else(|| (!context.meta.is_empty()).then_some(context.meta));
                tokio::select! {
                    result = self.peer.notify_progress(event) => if let Err(error)=result {
                        self.fail_native(DeliveryError::Forward,NativeDeliveryError::Forward(error));
                        return Err(DeliveryError::Forward);
                    },
                    () = self.closed.cancelled() => return Err(DeliveryError::Cancelled),
                    () = self.original.cancellation().cancelled() => return Err(DeliveryError::Cancelled),
                    () = tokio::time::sleep_until(self.deadline.into()) => return Err(DeliveryError::Deadline),
                }
            }
            stopped(&self.original).map_or(Ok(()), Err)
        }.await;
        {
            let mut state = self.state.lock().expect("progress state");
            state.sending = false;
            if result.is_ok() {
                state.next = ordinal + 1;
            }
        }
        if let Err(error) = result {
            self.fail(error);
        }
        self.changed.notify_waiters();
    }
    async fn prefix(&self, target: u64) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let (observed, next, first) = {
                let state = self.state.lock().expect("progress state");
                (state.observed, state.next, state.first)
            };
            if observed > target {
                self.fail(DeliveryError::Tail);
                return;
            }
            if first.is_some() || next > target {
                return;
            }
            tokio::select! {
                () = &mut changed => (),
                () = self.original.cancellation().cancelled() => { self.fail(DeliveryError::Gap); return; },
                () = tokio::time::sleep_until(self.deadline.into()) => { self.fail(DeliveryError::Gap); return; },
            }
        }
    }
}
#[derive(Clone)]
struct Progress(Arc<Gate>);
impl ClientHandler for Progress {
    async fn on_progress(
        &self,
        event: ProgressNotificationParam,
        context: NotificationContext<RoleClient>,
    ) {
        self.0.forward(event, context).await;
        #[cfg(test)]
        self.0.processed.notify_one();
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Accepted {
    Prefix(u64),
    NativeErrorNoAcceptedTarget,
}
fn accepted(result: &CallToolResult) -> Result<Accepted, DeliveryError> {
    if let Some(receipt) = result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("receipt"))
    {
        let counter = match receipt.get("provider") {
            None => "known_events",
            Some(Value::String(provider))
                if provider == "codex"
                    || provider == "claude"
                    || provider == "qwen"
                    || provider == "cursor"
                    || provider == "antigravity"
                    || provider == "grok" =>
            {
                "known_events"
            }
            Some(Value::String(provider))
                if provider == "kiro"
                    || provider == "opencode"
                    || provider == "kimi"
                    || provider == "copilot" =>
            {
                "accepted_notifications"
            }
            Some(_) => return Err(DeliveryError::Contract),
        };
        return receipt
            .get(counter)
            .and_then(Value::as_u64)
            .map(Accepted::Prefix)
            .ok_or(DeliveryError::Contract);
    }
    if result.is_error == Some(true) {
        return Ok(Accepted::NativeErrorNoAcceptedTarget);
    }
    match result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
    {
        Some("no_corpus" | "published_empty" | "empty_selection") => Ok(Accepted::Prefix(0)),
        _ => Err(DeliveryError::Contract),
    }
}
fn deny(mut result: CallToolResult, error: DeliveryError) -> CallToolResult {
    let mut payload = result
        .structured_content
        .take()
        .unwrap_or_else(|| json!({"error_content":result.content}));
    if !payload.is_object() {
        payload = json!({"native_structured_content":payload});
    }
    if let Some(completion) = payload
        .get_mut("receipt")
        .and_then(|v| v.get_mut("completion"))
        .and_then(Value::as_object_mut)
    {
        completion.remove("answer");
    }
    if let Some(receipt) = payload.get_mut("receipt").and_then(Value::as_object_mut) {
        receipt.remove("native_result_json");
    }
    payload["delivery_error"] = json!(error.label());
    result.content = vec![ContentBlock::text(error.label())];
    result.structured_content = Some(payload);
    result.is_error = Some(true);
    result
}
fn inject(
    call: &mut CallToolRequestParams,
    state: &State,
    original: &RequestContext<'_>,
) -> io::Result<()> {
    original.snapshot().check()?;
    let Some((root, version)) = &state.current else {
        return Ok(());
    };
    let args = call.arguments.get_or_insert_default();
    if args.contains_key("context") {
        return Ok(());
    }
    let same = args
        .get("project_dir")
        .is_none_or(|v| v.as_str().is_some_and(|s| Path::new(s) == root));
    if same {
        if !args.contains_key("project_dir") {
            let root = root.to_str().expect("validated UTF-8 root");
            if root.len() > state.content.max_scalar_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            original.snapshot().check()?;
            args.insert("project_dir".to_owned(), Value::String(root.to_owned()));
        }
        if let Some(version) = version
            && !args.contains_key("project_version")
        {
            if version.len() > state.content.max_scalar_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            original.snapshot().check()?;
            args.insert("project_version".to_owned(), Value::String(version.clone()));
        }
    }
    Ok(())
}
struct Forwarded {
    native: Result<CallToolResult, Option<ServiceError>>,
    delivery: Option<DeliveryError>,
    native_delivery: Option<NativeDeliveryError>,
}
impl std::fmt::Debug for Forwarded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnowledgeAskForwarded")
            .field("native_result", &self.native.is_ok())
            .field("delivery", &self.delivery)
            .field(
                "native_delivery_cause",
                &self
                    .native_delivery
                    .as_ref()
                    .map(|error| error.cause().is::<ServiceError>()),
            )
            .finish_non_exhaustive()
    }
}
impl Forwarded {
    fn public(self) -> CallToolResult {
        let result = match self.native {
            Ok(result) => result,
            Err(Some(error)) => {
                let (class, code) = match &error {
                    ServiceError::McpError(error) => ("mcp", Some(error.code.0)),
                    ServiceError::TransportSend(_) => ("transport_send", None),
                    ServiceError::TransportClosed => ("transport_closed", None),
                    ServiceError::UnexpectedResponse => ("unexpected_response", None),
                    ServiceError::SubscriptionLagged { .. } => ("subscription_lagged", None),
                    ServiceError::Cancelled { .. } => ("native_cancelled", None),
                    ServiceError::Timeout { .. } => ("native_timeout", None),
                    ServiceError::InputRequiredRoundsExceeded { .. } => {
                        ("input_rounds_exceeded", None)
                    }
                    _ => ("sdk_error", None),
                };
                CallToolResult::structured_error(
                    json!({"native_error_class":class,"mcp_error_code":code}),
                )
            }
            Err(None) => CallToolResult::error(vec![ContentBlock::text(
                "Knowledge Ask native response not observed",
            )]),
        };
        match self.delivery {
            Some(error) => deny(result, error),
            None => result,
        }
    }
}
async fn cancel_notification(peer: &Peer<RoleClient>, id: rmcp::model::RequestId, gate: &Gate) {
    let notification = peer.notify_cancelled(CancelledNotificationParam::new(Some(id), None));
    tokio::pin!(notification);
    // One SDK poll cannot prove enqueue or delivery. Never delay owned shutdown on this waiter.
    let result =
        std::future::poll_fn(|cx| std::task::Poll::Ready(notification.as_mut().poll(cx))).await;
    if let std::task::Poll::Ready(Err(error)) = result {
        gate.fail_native(
            DeliveryError::Transport,
            NativeDeliveryError::Cancellation(error),
        );
    }
}
async fn execute(
    state: &State,
    mut call: CallToolRequestParams,
    incoming: Incoming<RoleServer>,
    original: &RequestContext<'_>,
) -> io::Result<Forwarded> {
    let deadline = original.snapshot().required_deadline()?;
    inject(&mut call, state, original)?;
    original.snapshot().check()?;
    if incoming
        .meta
        .get("progressToken")
        .and_then(Value::as_str)
        .is_some_and(|token| token.len() > state.content.max_scalar_bytes)
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let upstream = incoming.meta.get_progress_token();
    call.meta.get_or_insert_default().extend(incoming.meta);
    if upstream
        .as_ref()
        .is_some_and(|token| !token_fits(token, state.content.max_scalar_bytes))
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let gate = Gate::new(
        Arc::clone(original.snapshot()),
        deadline,
        state.content,
        incoming.peer,
        upstream,
    );
    let address =
        crate::source_entry::address(&state.installation, state.key_limits, original).await?;
    let connection = connect_source(
        &state.installation,
        address,
        &state.client,
        state.key_limits,
        original,
        Progress(Arc::clone(&gate)),
    )
    .await?;
    let request = async {
        // Recheck at the actual first poll after handshake, before SDK can enqueue IO.
        let remaining = match original
            .snapshot()
            .check()
            .map(|()| deadline.saturating_duration_since(std::time::Instant::now()))
        {
            Ok(remaining) => remaining,
            Err(error) => {
                let class = if error.kind() == io::ErrorKind::TimedOut {
                    DeliveryError::Deadline
                } else {
                    DeliveryError::Cancelled
                };
                gate.fail_native(class, NativeDeliveryError::Context(error));
                return Err(None);
            }
        };
        connection
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(call)),
                PeerRequestOptions::with_timeout(remaining),
            )
            .await
            .map_err(Some)
    };
    let handle = tokio::select! {
        result = request => result,
        () = original.cancellation().cancelled() => Err(None),
        () = tokio::time::sleep_until(deadline.into()) => Err(None),
    };
    let native: Result<CallToolResult, Option<ServiceError>> = match handle {
        Ok(handle) => {
            gate.bind(handle.progress_token.clone());
            let id = handle.id.clone();
            let response = handle.await_response();
            tokio::pin!(response);
            let result = tokio::select! {
                biased;
                result = &mut response => Some(result),
                () = original.cancellation().cancelled() => {
                    gate.fail(DeliveryError::Cancelled);
                    gate.close();
                    cancel_notification(connection.peer(), id, &gate).await;
                    None
                },
                () = tokio::time::sleep_until(deadline.into()) => {
                    gate.fail(DeliveryError::Deadline);
                    gate.close();
                    cancel_notification(connection.peer(), id, &gate).await;
                    None
                },
            };
            match result {
                Some(Ok(ServerResult::CallToolResult(result))) => Ok(result),
                Some(Ok(_)) => {
                    gate.fail(DeliveryError::Contract);
                    Err(None)
                }
                Some(Err(error)) => Err(Some(error)),
                None => Err(None),
            }
        }
        Err(error) => {
            gate.fail(stopped(original.snapshot()).unwrap_or(DeliveryError::Transport));
            Err(error)
        }
    };
    if let Ok(result) = &native {
        match accepted(result) {
            Ok(Accepted::Prefix(target)) => gate.prefix(target).await,
            Ok(Accepted::NativeErrorNoAcceptedTarget) => (),
            Err(error) => gate.fail(error),
        }
    }
    if let Some(error) = stopped(original.snapshot()) {
        gate.fail(error);
    }
    gate.close();
    gate.drain().await;
    let cleanup = connection.cancel().await;
    if let Err(error) = cleanup {
        gate.fail_native(
            DeliveryError::Transport,
            NativeDeliveryError::Cleanup(error),
        );
    }
    // SDK callbacks not polled yet are outside its service join. Their closed gate permits zero IO.
    if let Ok(result) = &native
        && let Ok(Accepted::Prefix(target)) = accepted(result)
        && gate.state.lock().expect("progress state").observed > target
    {
        gate.fail(DeliveryError::Tail);
    }
    Ok(Forwarded {
        native,
        delivery: gate.first(),
        native_delivery: gate.state.lock().expect("progress state").native.take(),
    })
}

pub(super) async fn call(
    proxy: &SourceProxy,
    mut call: CallToolRequestParams,
    context: Incoming<RoleServer>,
) -> Result<CallToolResponse, ErrorData> {
    let format = crate::mcp_output::select_format(&mut call)?;
    // Progress receipts and denial gates consume the typed JSON contract first.
    crate::mcp_output::json_call(&mut call);
    let deadline = Instant::now().checked_add(proxy.0.timeout).ok_or_else(|| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            "invalid Knowledge Ask deadline",
            None,
        )
    })?;
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let (send, receive) = oneshot::channel();
    let incoming = context.ct.clone();
    {
        let mut jobs = proxy.0.jobs.lock().await;
        while jobs.try_join_next().is_some() {}
        if proxy.0.stop.is_cancelled() || incoming.is_cancelled() {
            return crate::mcp_output::format_response(
                CallToolResult::error(vec![ContentBlock::text("Knowledge Ask proxy stopped")])
                    .into(),
                format,
                proxy.0.content.max_output_bytes,
            );
        }
        let state = Arc::clone(&proxy.0);
        let cancellation = cancel.clone();
        let worker_incoming = incoming.clone();
        jobs.spawn(async move {
            let result = async {
                let original = RequestContext::new(state.content, &cancellation, deadline)
                    .map_err(io::Error::other)?;
                let operation = execute(&state, call, context, &original);
                tokio::pin!(operation);
                tokio::select! {
                    result = &mut operation => result,
                    () = worker_incoming.cancelled() => { cancellation.cancel(); operation.await },
                    () = state.stop.cancelled() => { cancellation.cancel(); operation.await },
                }
            }
            .await;
            let _ = send.send(result);
        });
    }
    let result = receive.await.map_err(|_| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            "Knowledge Ask proxy worker failed",
            None,
        )
    })?;
    let mut result = result.map(Forwarded::public).unwrap_or_else(|error| {
        CallToolResult::error(vec![ContentBlock::text(match error.kind() {
            io::ErrorKind::Interrupted => DeliveryError::Cancelled.label(),
            io::ErrorKind::TimedOut => DeliveryError::Deadline.label(),
            io::ErrorKind::FileTooLarge => DeliveryError::Limit.label(),
            _ => DeliveryError::Transport.label(),
        })])
    });
    // This last fence never discards a receipt or a previously projected first delivery cause.
    if result
        .structured_content
        .as_ref()
        .and_then(|v| v.get("delivery_error"))
        .is_none()
    {
        if cancel.is_cancelled() || incoming.is_cancelled() || proxy.0.stop.is_cancelled() {
            result = deny(result, DeliveryError::Cancelled);
        } else if Instant::now() >= deadline {
            result = deny(result, DeliveryError::Deadline);
        }
    }
    if let Ok(mut jobs) = proxy.0.jobs.try_lock() {
        while jobs.try_join_next().is_some() {}
    }
    crate::mcp_output::format_response(result.into(), format, proxy.0.content.max_output_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_selection_keeps_remote_receipts_json_until_delivery_gate() {
        use graphit_core::structured_output::OutputFormat;
        for (flag, expected) in [
            (None, OutputFormat::Toon),
            (Some(true), OutputFormat::Toon),
            (Some(false), OutputFormat::Json),
        ] {
            let mut call = CallToolRequestParams::new(TOOL);
            if let Some(flag) = flag {
                call.arguments
                    .get_or_insert_default()
                    .insert("ai_optimized".into(), Value::Bool(flag));
            }
            let format = crate::mcp_output::select_format(&mut call).unwrap();
            assert_eq!(format, expected);
            crate::mcp_output::json_call(&mut call);
            assert_eq!(call.arguments.as_ref().unwrap()["ai_optimized"], false);
            let result = CallToolResult::structured(json!({"receipt": {
                "provider":"codex", "known_events":2,
                "completion":{"answer":"private rejected answer"},
                "native_result_json":"private rejected native payload"
            }}));
            assert_eq!(accepted(&result), Ok(Accepted::Prefix(2)));
            // A format choice must not bypass denial or reintroduce removed content.
            let denied = deny(result, DeliveryError::Gap);
            let result = crate::mcp_output::format_result(denied, format, 64 * 1024).unwrap();
            assert_eq!(result.is_error, Some(true));
            let wire = serde_json::to_string(&result).unwrap();
            assert!(wire.contains("delivery_error"));
            assert!(!wire.contains("private rejected answer"));
            assert!(!wire.contains("private rejected native payload"));
            assert_eq!(
                result.structured_content.is_some(),
                format == OutputFormat::Json
            );
        }
        let mut invalid = CallToolRequestParams::new(TOOL)
            .with_arguments(json!({"ai_optimized":"true"}).as_object().unwrap().clone());
        assert!(crate::mcp_output::select_format(&mut invalid).is_err());
    }
    #[test]
    fn native_error_without_receipt_does_not_invent_accepted_target() {
        let error =
            CallToolResult::error(vec![ContentBlock::text("native failure after execution")]);
        assert_eq!(accepted(&error), Ok(Accepted::NativeErrorNoAcceptedTarget));
        let projected = deny(error, DeliveryError::Forward);
        let payload = projected.structured_content.unwrap();
        assert!(payload.get("receipt").is_none());
        assert!(payload.get("status").is_none());
        assert!(
            payload
                .to_string()
                .contains("native failure after execution")
        );
    }
    #[test]
    fn malformed_receipt_cannot_be_target_zero_even_on_native_error() {
        for receipt in [
            json!({}),
            json!({"known_events":-1}),
            Value::Null,
            json!({"known_events":1.5}),
        ] {
            let result = CallToolResult::structured_error(json!({"receipt":receipt}));
            assert_eq!(accepted(&result), Err(DeliveryError::Contract));
        }
        assert_eq!(
            accepted(&CallToolResult::structured(json!({"status":"no_corpus"}))),
            Ok(Accepted::Prefix(0))
        );
        assert_eq!(
            accepted(&CallToolResult::structured(json!({"status":"answer"}))),
            Err(DeliveryError::Contract)
        );
    }
    #[test]
    fn kiro_receipt_uses_only_real_accepted_counter_without_codex_fallback() {
        let result = CallToolResult::structured(json!({"receipt":{
            "provider":"kiro", "observed_notifications":7,
            "accepted_notifications":2, "known_events":99
        }}));
        assert_eq!(accepted(&result), Ok(Accepted::Prefix(2)));
        for receipt in [
            json!({"provider":"kiro", "observed_notifications":7, "known_events":99}),
            json!({"provider":"kiro", "accepted_notifications":null}),
            json!({"provider":"kiro", "accepted_notifications":-1}),
            json!({"provider":"kiro", "accepted_notifications":1.5}),
            json!({"provider":"kiro", "accepted_notifications":"2"}),
        ] {
            assert_eq!(
                accepted(&CallToolResult::structured_error(
                    json!({"receipt":receipt})
                )),
                Err(DeliveryError::Contract)
            );
        }
    }
    #[test]
    fn opencode_receipt_requires_its_actual_accepted_notifications() {
        assert_eq!(
            accepted(&CallToolResult::structured(json!({"receipt":{
                "provider":"opencode","observed_notifications":7,
                "accepted_notifications":2,"known_events":99
            }}))),
            Ok(Accepted::Prefix(2))
        );
        for receipt in [
            json!({"provider":"opencode","observed_notifications":7,"known_events":99}),
            json!({"provider":"opencode","accepted_notifications":null}),
            json!({"provider":"opencode","accepted_notifications":-1}),
            json!({"provider":"opencode","accepted_notifications":1.5}),
            json!({"provider":"opencode","accepted_notifications":"2"}),
        ] {
            assert_eq!(
                accepted(&CallToolResult::structured_error(
                    json!({"receipt":receipt})
                )),
                Err(DeliveryError::Contract)
            );
        }
    }
    #[test]
    fn claude_provider_requires_its_real_prefix_and_rejects_unknown_provider_modes() {
        assert_eq!(
            accepted(&CallToolResult::structured(json!({"receipt":{
                "provider":"claude", "known_events":2, "frames":9, "accepted_notifications":99
            }}))),
            Ok(Accepted::Prefix(2))
        );
        for receipt in [
            json!({"provider":"claude", "known_events":null}),
            json!({"provider":"claude", "known_events":-1}),
            json!({"provider":"claude", "known_events":1.5}),
            json!({"provider":"claude", "accepted_notifications":2}),
            json!({"provider":"future", "known_events":2}),
            json!({"provider":"gemini", "known_events":2}),
            json!({"provider":null, "known_events":2}),
            json!({"provider":true, "known_events":2}),
        ] {
            assert_eq!(
                accepted(&CallToolResult::structured_error(
                    json!({"receipt":receipt})
                )),
                Err(DeliveryError::Contract)
            );
        }
    }
    #[tokio::test]
    async fn claude_receipt_prefix_keeps_real_push_tail_and_missing_event_gap() {
        for (count, expected) in [
            (0, Some(DeliveryError::Tail)),
            (1, None),
            (2, Some(DeliveryError::Gap)),
        ] {
            let h = Harness::new(
                if expected == Some(DeliveryError::Gap) {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(5)
                },
                false,
            );
            h.gate
                .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
            h.event(1.0, 99).await;
            h.gate.processed.notified().await;
            let result = CallToolResult::structured_error(json!({
                "execution_error":"native_failure", "receipt":{"provider":"claude",
                    "known_events":count, "frames":7, "usage":{"output_tokens":3},
                    "native_result_json":"{\"result\":\"forbidden\"}",
                    "completion":{"answer":"accepted then denied"},
                    "process":{"cleanup":"Complete","status":9}}
            }));
            let Ok(Accepted::Prefix(target)) = accepted(&result) else {
                panic!("Claude real prefix")
            };
            h.gate.prefix(target).await;
            assert_eq!(h.gate.first(), expected);
            if let Some(error) = expected {
                let denied = deny(result, error).structured_content.unwrap();
                assert_eq!(denied["receipt"]["known_events"], count);
                assert_eq!(denied["receipt"]["frames"], 7);
                assert_eq!(denied["receipt"]["usage"]["output_tokens"], 3);
                assert_eq!(denied["receipt"]["process"]["status"], 9);
                assert_eq!(denied["execution_error"], "native_failure");
                assert!(denied["receipt"]["completion"].get("answer").is_none());
                assert!(denied["receipt"].get("native_result_json").is_none());
            }
            h.stop().await;
        }
    }
    #[test]
    fn delivery_denial_preserves_native_receipt_usage_metadata_and_cause() {
        let mut result = CallToolResult::structured(
            json!({"status":"failed", "execution_error":"native_nonzero", "receipt":{"known_events":2,"completion":{"answer":"forbidden", "usage":{"output_tokens":7}},"process":{"status":9,"cleanup":"complete"}}}),
        );
        result.meta = Some(rmcp::model::MetaObject(
            json!({"native":"metadata"}).as_object().unwrap().clone(),
        ));
        let result = deny(result, DeliveryError::Tail);
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.meta.unwrap()["native"], "metadata");
        let payload = result.structured_content.unwrap();
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert_eq!(
            payload["receipt"]["completion"]["usage"]["output_tokens"],
            7
        );
        assert_eq!(payload["receipt"]["process"]["status"], 9);
        assert_eq!(payload["execution_error"], "native_nonzero");
        assert!(
            !result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("forbidden")
        );
    }
    use rmcp::{
        model::{JsonRpcMessage, ServerNotification},
        service::{RunningService, RxJsonRpcMessage, TxJsonRpcMessage, serve_directly},
        transport::{Transport, async_rw::AsyncRwTransport},
    };
    struct Empty;
    impl ServerHandler for Empty {}
    struct Observe<T> {
        inner: T,
        events: Arc<SyncMutex<Vec<ProgressNotificationParam>>>,
    }
    impl<T: Transport<RoleClient>> Transport<RoleClient> for Observe<T> {
        type Error = T::Error;
        fn send(
            &mut self,
            item: TxJsonRpcMessage<RoleClient>,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            self.inner.send(item)
        }
        async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
            let item = self.inner.receive().await;
            if let Some(JsonRpcMessage::Notification(notification)) = &item
                && let ServerNotification::ProgressNotification(event) = &notification.notification
            {
                let mut params = event.params.clone();
                params.meta = event
                    .extensions
                    .get::<rmcp::model::NotificationMetaObject>()
                    .cloned()
                    .or(params.meta);
                self.events.lock().unwrap().push(params);
            }
            item
        }
        async fn close(&mut self) -> Result<(), Self::Error> {
            self.inner.close().await
        }
    }
    struct Harness {
        gate: Arc<Gate>,
        down: RunningService<RoleServer, Empty>,
        handler: RunningService<RoleClient, Progress>,
        outer: RunningService<RoleServer, Empty>,
        client: RunningService<RoleClient, ()>,
        events: Arc<SyncMutex<Vec<ProgressNotificationParam>>>,
    }
    impl Harness {
        fn new(deadline: Duration, upstream: bool) -> Self {
            let (local, remote) = tokio::io::duplex(65536);
            let outer = serve_directly::<RoleServer, _, _, _, _>(Empty, remote, None);
            let (read, write) = tokio::io::split(local);
            let events = Arc::new(SyncMutex::new(Vec::new()));
            let client = serve_directly::<RoleClient, _, _, _, _>(
                (),
                Observe {
                    inner: AsyncRwTransport::new_client(read, write),
                    events: Arc::clone(&events),
                },
                None,
            );
            let original = Arc::new(SnapshotContext::new(
                Instant::now() + deadline,
                Cancellation::default(),
            ));
            let gate = Gate::new(
                Arc::clone(&original),
                original.required_deadline().unwrap(),
                ContentLimits {
                    max_input_bytes: 65536,
                    max_output_bytes: 65536,
                    max_depth: 32,
                    max_nodes: 4096,
                    max_scalar_bytes: 4096,
                    max_collection_entries: 64,
                },
                outer.peer().clone(),
                upstream.then(|| {
                    ProgressToken(rmcp::model::NumberOrString::String(
                        "upstream-secret-token".into(),
                    ))
                }),
            );
            let (local, remote) = tokio::io::duplex(65536);
            let down = serve_directly::<RoleServer, _, _, _, _>(Empty, remote, None);
            let handler =
                serve_directly::<RoleClient, _, _, _, _>(Progress(Arc::clone(&gate)), local, None);
            Self {
                gate,
                down,
                handler,
                outer,
                client,
                events,
            }
        }
        async fn entered(&self, count: usize) {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let changed = self.gate.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if self.gate.state.lock().unwrap().active >= count {
                        return;
                    }
                    changed.await;
                }
            })
            .await
            .unwrap();
        }
        async fn event(&self, ordinal: f64, token: i64) {
            let mut event = ProgressNotificationParam::new(
                ProgressToken(rmcp::model::NumberOrString::Number(token)),
                ordinal,
            );
            event.total = Some(7.0);
            event.message = Some(
                r#"{"native":1e400,"negative_zero":-0,"big":123456789012345678901234567890}"#
                    .to_owned(),
            );
            event.meta = Some(serde_json::from_value(json!({"literal":"metadata"})).unwrap());
            self.down.peer().notify_progress(event).await.unwrap();
        }
        async fn stop(self) {
            self.gate.close();
            self.gate.drain().await;
            self.down.cancel().await.unwrap();
            self.handler.cancel().await.unwrap();
            self.outer.cancel().await.unwrap();
            self.client.cancel().await.unwrap();
        }
    }
    #[tokio::test]
    async fn sdk_early_callback_waits_binding_and_orders_two_before_one_before_final() {
        let h = Harness::new(Duration::from_secs(5), true);
        h.event(2.0, 99).await;
        h.entered(1).await;
        assert!(h.events.lock().unwrap().is_empty());
        let gate = Arc::clone(&h.gate);
        let final_waiter = tokio::spawn(async move {
            gate.prefix(2).await;
        });
        tokio::task::yield_now().await;
        assert!(!final_waiter.is_finished());
        h.gate
            .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
        h.event(1.0, 99).await;
        tokio::time::timeout(Duration::from_secs(3), final_waiter)
            .await
            .unwrap()
            .unwrap();
        h.gate.close();
        h.gate.drain().await;
        // The typed observer records wire receive order, not notification-task scheduling.
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if h.events.lock().unwrap().len() == 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        {
            let events = h.events.lock().unwrap();
            assert_eq!(
                events.iter().map(|e| e.progress).collect::<Vec<_>>(),
                [1.0, 2.0]
            );
            assert_eq!(
                events[0].progress_token,
                ProgressToken(rmcp::model::NumberOrString::String(
                    "upstream-secret-token".into()
                ))
            );
            assert_eq!(events[0].total, Some(7.0));
            assert!(events[0].message.as_ref().unwrap().contains("1e400"));
            assert!(events[0].message.as_ref().unwrap().contains(":-0"));
            assert_eq!(
                events[0].meta.as_ref().unwrap().get("literal"),
                Some(&json!("metadata"))
            );
        }
        assert!(!format!("{:?}", h.gate).contains("secret-token"));
        h.stop().await;
    }
    #[tokio::test]
    async fn sdk_close_drains_entered_waiter_and_late_callback_performs_zero_io() {
        let h = Harness::new(Duration::from_secs(5), true);
        h.event(1.0, 99).await;
        h.entered(1).await;
        h.gate.close();
        h.gate.drain().await;
        assert_eq!(h.gate.state.lock().unwrap().active, 0);
        h.gate.processed.notified().await;
        h.event(1.0, 99).await;
        tokio::time::timeout(Duration::from_secs(3), h.gate.processed.notified())
            .await
            .unwrap();
        assert!(h.events.lock().unwrap().is_empty());
        assert_eq!(h.gate.state.lock().unwrap().observed, 1);
        h.stop().await;
    }
    #[tokio::test]
    async fn sdk_pending_binding_obeys_original_cancel_and_deadline() {
        for deadline in [false, true] {
            let h = Harness::new(
                if deadline {
                    Duration::from_millis(75)
                } else {
                    Duration::from_secs(5)
                },
                true,
            );
            h.event(1.0, 99).await;
            h.entered(1).await;
            if !deadline {
                h.gate.original.cancellation().cancel();
            }
            tokio::time::timeout(Duration::from_secs(3), h.gate.processed.notified())
                .await
                .unwrap();
            assert_eq!(
                h.gate.first(),
                Some(if deadline {
                    DeliveryError::Deadline
                } else {
                    DeliveryError::Cancelled
                })
            );
            assert!(matches!(&h.gate.state.lock().unwrap().native,
                Some(NativeDeliveryError::Context(error)) if error.kind()==if deadline {io::ErrorKind::TimedOut} else {io::ErrorKind::Interrupted}));
            h.gate.close();
            h.gate.drain().await;
            assert!(h.events.lock().unwrap().is_empty());
            h.stop().await;
        }
    }
    #[tokio::test]
    async fn sdk_invalid_token_ordinal_duplicate_and_tail_keep_first_delivery_cause() {
        for (ordinal, token, expected) in [
            (1.0, 98, DeliveryError::Token),
            (0.0, 99, DeliveryError::Ordinal),
            (1.5, 99, DeliveryError::Ordinal),
            (9_007_199_254_740_994.0, 99, DeliveryError::Ordinal),
        ] {
            let h = Harness::new(Duration::from_secs(5), true);
            h.gate
                .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
            h.event(ordinal, token).await;
            h.gate.processed.notified().await;
            h.gate.fail(DeliveryError::Transport);
            assert_eq!(h.gate.first(), Some(expected));
            assert!(h.events.lock().unwrap().is_empty());
            h.stop().await;
        }
        let h = Harness::new(Duration::from_secs(5), false);
        h.gate
            .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
        h.event(1.0, 99).await;
        h.gate.processed.notified().await;
        assert_eq!(h.gate.state.lock().unwrap().next, 2);
        assert!(h.events.lock().unwrap().is_empty());
        h.gate.prefix(0).await;
        assert_eq!(h.gate.first(), Some(DeliveryError::Tail));
        h.stop().await;
        let h = Harness::new(Duration::from_secs(5), false);
        h.gate
            .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
        h.event(1.0, 99).await;
        h.gate.processed.notified().await;
        h.event(1.0, 99).await;
        h.gate.processed.notified().await;
        assert_eq!(h.gate.first(), Some(DeliveryError::Duplicate));
        h.stop().await;
    }
    #[tokio::test]
    async fn kiro_ignored_notifications_do_not_create_gap_and_unaccepted_push_is_tail() {
        for accepted_count in [1, 0] {
            let h = Harness::new(Duration::from_secs(5), true);
            h.gate
                .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
            h.event(1.0, 99).await;
            h.gate.processed.notified().await;
            let mut result = CallToolResult::structured(json!({
                "execution_error":"native_post_push_denial",
                "receipt":{"provider":"kiro", "observed_notifications":7,
                    "accepted_notifications":accepted_count,
                    "completion":{"answer":"native answer"},
                    "process":{"cleanup":"complete"}}
            }));
            result.meta = Some(rmcp::model::MetaObject(
                json!({"native":"metadata"}).as_object().unwrap().clone(),
            ));
            let Ok(Accepted::Prefix(target)) = accepted(&result) else {
                panic!("real Kiro target required")
            };
            h.gate.prefix(target).await;
            if accepted_count == 1 {
                assert_eq!(h.gate.first(), None);
                assert_eq!(
                    result.structured_content.unwrap()["receipt"]["completion"]["answer"],
                    "native answer"
                );
            } else {
                assert_eq!(h.gate.first(), Some(DeliveryError::Tail));
                let denied = deny(result, h.gate.first().unwrap());
                assert_eq!(denied.is_error, Some(true));
                assert_eq!(denied.meta.unwrap()["native"], "metadata");
                let payload = denied.structured_content.unwrap();
                assert_eq!(payload["receipt"]["observed_notifications"], 7);
                assert_eq!(payload["receipt"]["accepted_notifications"], 0);
                assert_eq!(payload["receipt"]["process"]["cleanup"], "complete");
                assert_eq!(payload["execution_error"], "native_post_push_denial");
                assert!(payload["receipt"]["completion"].get("answer").is_none());
            }
            h.stop().await;
        }
    }
    #[tokio::test]
    async fn missing_accepted_prefix_fails_without_fabricating_native_result() {
        let h = Harness::new(Duration::from_millis(50), false);
        h.gate.prefix(1).await;
        assert_eq!(h.gate.first(), Some(DeliveryError::Gap));
        assert!(h.events.lock().unwrap().is_empty());
        h.stop().await;
    }
    #[test]
    fn native_sdk_cause_stays_private_and_public_code_is_actual() {
        let native = ServiceError::McpError(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "synthetic-secret-native-error",
            Some(json!({"secret":"synthetic-secret-native-data"})),
        ));
        let outcome = Forwarded {
            native: Err(Some(native)),
            delivery: None,
            native_delivery: None,
        };
        assert!(!format!("{outcome:?}").contains("synthetic-secret"));
        assert!(
            matches!(&outcome.native, Err(Some(ServiceError::McpError(error))) if error.code == ErrorCode::INVALID_PARAMS)
        );
        let result = outcome.public();
        assert_eq!(
            result.structured_content.as_ref().unwrap()["mcp_error_code"],
            ErrorCode::INVALID_PARAMS.0
        );
        assert_eq!(
            result.structured_content.as_ref().unwrap()["native_error_class"],
            "mcp"
        );
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("synthetic-secret")
        );
    }
    #[tokio::test]
    async fn actual_progress_copy_respects_existing_output_limit() {
        let h = Harness::new(Duration::from_secs(5), true);
        h.gate
            .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
        let mut event = ProgressNotificationParam::new(
            ProgressToken(rmcp::model::NumberOrString::Number(99)),
            1.0,
        );
        event.message = Some("x".repeat(65537));
        h.down.peer().notify_progress(event).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), h.gate.processed.notified())
            .await
            .unwrap();
        assert_eq!(h.gate.first(), Some(DeliveryError::Limit));
        assert!(h.events.lock().unwrap().is_empty());
        h.stop().await;
    }
    #[tokio::test]
    async fn cancellation_notification_first_poll_cannot_hold_shutdown_waiter() {
        let h = Harness::new(Duration::from_secs(5), false);
        h.gate.close();
        {
            let future = cancel_notification(
                h.handler.peer(),
                rmcp::model::NumberOrString::Number(1),
                &h.gate,
            );
            tokio::pin!(future);
            let result =
                std::future::poll_fn(|cx| std::task::Poll::Ready(future.as_mut().poll(cx))).await;
            assert!(result.is_ready());
        }
        h.stop().await;
    }
    #[tokio::test]
    async fn real_forward_sdk_error_remains_private_with_first_public_cause() {
        let mut h = Harness::new(Duration::from_secs(5), true);
        h.outer.close().await.unwrap();
        h.gate
            .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
        h.event(1.0, 99).await;
        tokio::time::timeout(Duration::from_secs(3), h.gate.processed.notified())
            .await
            .unwrap();
        assert_eq!(h.gate.first(), Some(DeliveryError::Forward));
        {
            let state = h.gate.state.lock().unwrap();
            assert!(matches!(
                &state.native,
                Some(NativeDeliveryError::Forward(ServiceError::TransportClosed))
            ));
        }
        assert!(!format!("{:?}", h.gate).contains("upstream-secret"));
        h.stop().await;
    }
    #[test]
    fn qwen_provider_requires_its_real_prefix_and_rejects_unknown_provider_modes() {
        assert_eq!(
            accepted(&CallToolResult::structured(json!({"receipt":{
                "provider":"qwen", "known_events":2, "frames":9, "accepted_notifications":99
            }}))),
            Ok(Accepted::Prefix(2))
        );
        for receipt in [
            json!({"provider":"qwen", "known_events":null}),
            json!({"provider":"qwen", "known_events":-1}),
            json!({"provider":"qwen", "known_events":1.5}),
            json!({"provider":"qwen", "accepted_notifications":2}),
            json!({"provider":"future", "known_events":2}),
            json!({"provider":"gemini", "known_events":2}),
            json!({"provider":null, "known_events":2}),
            json!({"provider":true, "known_events":2}),
        ] {
            assert_eq!(
                accepted(&CallToolResult::structured_error(
                    json!({"receipt":receipt})
                )),
                Err(DeliveryError::Contract)
            );
        }
    }
    #[tokio::test]
    async fn qwen_receipt_prefix_keeps_real_push_tail_and_missing_event_gap() {
        for (count, expected) in [
            (0, Some(DeliveryError::Tail)),
            (1, None),
            (2, Some(DeliveryError::Gap)),
        ] {
            let h = Harness::new(
                if expected == Some(DeliveryError::Gap) {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(5)
                },
                false,
            );
            h.gate
                .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
            h.event(1.0, 99).await;
            h.gate.processed.notified().await;
            let result = CallToolResult::structured_error(json!({
                "execution_error":"native_failure", "receipt":{"provider":"qwen",
                    "known_events":count, "frames":7, "usage":{"output_tokens":3},
                    "native_result_json":"{\"result\":\"forbidden\"}",
                    "completion":{"answer":"accepted then denied"},
                    "process":{"cleanup":"Complete","status":9}}
            }));
            let Ok(Accepted::Prefix(target)) = accepted(&result) else {
                panic!("Qwen real prefix")
            };
            h.gate.prefix(target).await;
            assert_eq!(h.gate.first(), expected);
            if let Some(error) = expected {
                let denied = deny(result, error).structured_content.unwrap();
                assert_eq!(denied["receipt"]["known_events"], count);
                assert_eq!(denied["receipt"]["frames"], 7);
                assert_eq!(denied["receipt"]["usage"]["output_tokens"], 3);
                assert_eq!(denied["receipt"]["process"]["status"], 9);
                assert_eq!(denied["execution_error"], "native_failure");
                assert!(denied["receipt"]["completion"].get("answer").is_none());
                assert!(denied["receipt"].get("native_result_json").is_none());
            }
            h.stop().await;
        }
    }
    #[test]
    fn cursor_provider_requires_its_real_prefix_and_rejects_unknown_provider_modes() {
        assert_eq!(
            accepted(&CallToolResult::structured(json!({"receipt":{
                "provider":"cursor", "known_events":2, "frames":9, "accepted_notifications":99
            }}))),
            Ok(Accepted::Prefix(2))
        );
        for receipt in [
            json!({"provider":"cursor", "known_events":null}),
            json!({"provider":"cursor", "known_events":-1}),
            json!({"provider":"cursor", "known_events":1.5}),
            json!({"provider":"cursor", "accepted_notifications":2}),
            json!({"provider":"future", "known_events":2}),
            json!({"provider":"gemini", "known_events":2}),
            json!({"provider":null, "known_events":2}),
            json!({"provider":true, "known_events":2}),
        ] {
            assert_eq!(
                accepted(&CallToolResult::structured_error(
                    json!({"receipt":receipt})
                )),
                Err(DeliveryError::Contract)
            );
        }
    }
    #[tokio::test]
    async fn cursor_receipt_prefix_keeps_real_push_tail_and_missing_event_gap() {
        for (count, expected) in [
            (0, Some(DeliveryError::Tail)),
            (1, None),
            (2, Some(DeliveryError::Gap)),
        ] {
            let h = Harness::new(
                if expected == Some(DeliveryError::Gap) {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(5)
                },
                false,
            );
            h.gate
                .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
            h.event(1.0, 99).await;
            h.gate.processed.notified().await;
            let result = CallToolResult::structured_error(json!({
                "execution_error":"native_failure", "receipt":{"provider":"cursor",
                    "known_events":count, "frames":7, "usage":{"output_tokens":3},
                    "native_result_json":"{\"result\":\"forbidden\"}",
                    "completion":{"answer":"accepted then denied"},
                    "process":{"cleanup":"Complete","status":9}}
            }));
            let Ok(Accepted::Prefix(target)) = accepted(&result) else {
                panic!("Cursor real prefix")
            };
            h.gate.prefix(target).await;
            assert_eq!(h.gate.first(), expected);
            if let Some(error) = expected {
                let denied = deny(result, error).structured_content.unwrap();
                assert_eq!(denied["receipt"]["known_events"], count);
                assert_eq!(denied["receipt"]["frames"], 7);
                assert_eq!(denied["receipt"]["usage"]["output_tokens"], 3);
                assert_eq!(denied["receipt"]["process"]["status"], 9);
                assert_eq!(denied["execution_error"], "native_failure");
                assert!(denied["receipt"]["completion"].get("answer").is_none());
                assert!(denied["receipt"].get("native_result_json").is_none());
            }
            h.stop().await;
        }
    }
    #[test]
    fn kimi_provider_requires_actual_callback_prefix_without_vendor_usage_counter() {
        assert_eq!(
            accepted(&CallToolResult::structured(
                json!({"receipt":{"provider":"kimi","accepted_notifications":2}})
            )),
            Ok(Accepted::Prefix(2))
        );
        for receipt in [
            json!({"provider":"kimi","known_events":2}),
            json!({"provider":"kimi","accepted_notifications":null}),
            json!({"provider":"kimi","accepted_notifications":-1}),
            json!({"provider":"kimi","accepted_notifications":1.5}),
        ] {
            assert_eq!(
                accepted(&CallToolResult::structured_error(
                    json!({"receipt":receipt})
                )),
                Err(DeliveryError::Contract)
            );
        }
    }
    #[tokio::test]
    async fn kimi_callback_prefix_requires_delivery_and_denial_strips_answer_and_native_result() {
        for (count, expected) in [
            (0, Some(DeliveryError::Tail)),
            (1, None),
            (2, Some(DeliveryError::Gap)),
        ] {
            let h = Harness::new(
                if expected == Some(DeliveryError::Gap) {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(5)
                },
                false,
            );
            h.gate
                .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
            h.event(1.0, 99).await;
            h.gate.processed.notified().await;
            let result = CallToolResult::structured_error(
                json!({"execution_error":"native_failure","receipt":{"provider":"kimi","accepted_notifications":count,"native_result_json":"{\"type\":\"turn.ended\",\"reason\":\"completed\"}","completion":{"answer":"candidate"},"process":{"status_code":9,"cleanup":"Complete"}}}),
            );
            let Ok(Accepted::Prefix(target)) = accepted(&result) else {
                panic!("Kimi callback prefix")
            };
            h.gate.prefix(target).await;
            assert_eq!(h.gate.first(), expected);
            if let Some(error) = expected {
                let payload = deny(result, error).structured_content.unwrap();
                assert_eq!(payload["receipt"]["accepted_notifications"], count);
                assert_eq!(payload["receipt"]["process"]["status_code"], 9);
                assert_eq!(payload["execution_error"], "native_failure");
                assert!(payload["receipt"]["completion"].get("answer").is_none());
                assert!(payload["receipt"].get("native_result_json").is_none());
            }
            h.stop().await;
        }
    }
    #[test]
    fn copilot_provider_requires_actual_callback_prefix_without_vendor_usage_counter() {
        assert_eq!(
            accepted(&CallToolResult::structured(
                json!({"receipt":{"provider":"copilot","accepted_notifications":2}})
            )),
            Ok(Accepted::Prefix(2))
        );
        for receipt in [
            json!({"provider":"copilot","known_events":2}),
            json!({"provider":"copilot","accepted_notifications":null}),
            json!({"provider":"copilot","accepted_notifications":-1}),
            json!({"provider":"copilot","accepted_notifications":1.5}),
        ] {
            assert_eq!(
                accepted(&CallToolResult::structured_error(
                    json!({"receipt":receipt})
                )),
                Err(DeliveryError::Contract)
            );
        }
    }
    #[tokio::test]
    async fn copilot_callback_prefix_requires_delivery_and_denial_strips_answer_and_native_result()
    {
        for (count, expected) in [
            (0, Some(DeliveryError::Tail)),
            (1, None),
            (2, Some(DeliveryError::Gap)),
        ] {
            let h = Harness::new(
                if expected == Some(DeliveryError::Gap) {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(5)
                },
                false,
            );
            h.gate
                .bind(ProgressToken(rmcp::model::NumberOrString::Number(99)));
            h.event(1.0, 99).await;
            h.gate.processed.notified().await;
            let result = CallToolResult::structured_error(
                json!({"execution_error":"native_failure","receipt":{"provider":"copilot","accepted_notifications":count,"native_result_json":"{\"type\":\"turn.ended\",\"reason\":\"completed\"}","completion":{"answer":"candidate"},"process":{"status_code":9,"cleanup":"Complete"}}}),
            );
            let Ok(Accepted::Prefix(target)) = accepted(&result) else {
                panic!("Copilot callback prefix")
            };
            h.gate.prefix(target).await;
            assert_eq!(h.gate.first(), expected);
            if let Some(error) = expected {
                let payload = deny(result, error).structured_content.unwrap();
                assert_eq!(payload["receipt"]["accepted_notifications"], count);
                assert_eq!(payload["receipt"]["process"]["status_code"], 9);
                assert_eq!(payload["execution_error"], "native_failure");
                assert!(payload["receipt"]["completion"].get("answer").is_none());
                assert!(payload["receipt"].get("native_result_json").is_none());
            }
            h.stop().await;
        }
    }
}
