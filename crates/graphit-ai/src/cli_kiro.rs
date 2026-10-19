//! One ACP prompt on the caller's SDK transport and original execution context.
use crate::cli_acp_support::QuietSdk;
use crate::cli_kiro_wire::{KiroWireError, decode_notification};
use agent_client_protocol::{
    Agent, Client, ConnectTo, ConnectionTo, Error as SdkError, UntypedMessage,
    schema::{
        ProtocolVersion,
        v1::{
            CancelNotification, ContentBlock, DeleteSessionRequest, DeleteSessionResponse,
            InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse, Meta,
            NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionConfigId,
            SessionConfigKind, SessionConfigOption, SessionConfigSelect,
            SessionConfigSelectOptions, SessionConfigValueId, SessionId, SessionNotification,
            SessionUpdate, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
            StopReason, TextContent,
        },
    },
};
use graphit_core::file_io::SnapshotContext;
use std::{collections::TryReserveError, fmt, future::Future, io, sync::Arc, task::Poll};
use tokio::sync::{Mutex, Notify};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KiroEngine {
    V3,
}
pub enum KiroSessionStart {
    New(NewSessionRequest),
    Load(LoadSessionRequest),
}
pub struct KiroConfigSelection {
    pub config_id: SessionConfigId,
    pub value_id: SessionConfigValueId,
    pub meta: Option<Meta>,
}
pub struct KiroPrompt {
    pub text: String,
    pub selection: Option<KiroConfigSelection>,
}
impl KiroPrompt {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            selection: None,
        }
    }
}
/// Consumer operation stage, not a native replay marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KiroDeliveryPhase {
    Loading,
    Configuring,
    Prompting,
    Deleting,
}
#[derive(Clone, Copy, Debug)]
pub struct KiroLimits {
    pub prompt_text_bytes: usize,
    pub response_text_bytes: usize,
    pub notifications: u64,
    /// Session, requested config/value, message and tool-call identifiers admitted by us.
    pub id_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KiroReason {
    Policy,
    PromptLimit,
    ResponseLimit,
    NotificationLimit,
    IdLimit,
    Wire,
    Allocation,
    Context(io::ErrorKind),
    Sink(io::ErrorKind),
    Sdk,
    IncomingClosed,
    MissingText,
    UnsupportedLoad,
    UnsupportedDelete,
    UnsupportedAdditionalDirectories,
    UnsupportedConfiguration,
    ConfigurationNotFound,
    AmbiguousConfiguration,
    UnsupportedConfigurationKind,
    ConfigurationValueNotOffered,
    UnconfirmedConfiguration,
}
pub trait KiroSink: Send {
    /// Cooperatively poll; dropping a pending delivery must leave no detached work.
    fn push<'a>(
        &'a mut self,
        phase: KiroDeliveryPhase,
        event: &'a SessionNotification,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = io::Result<()>> + Send + 'a;
}
pub struct KiroReceipt {
    engine: KiroEngine,
    session: Option<SessionId>,
    initialize_response: Option<InitializeResponse>,
    delete_response: Option<DeleteSessionResponse>,
    new_response: Option<NewSessionResponse>,
    load_response: Option<LoadSessionResponse>,
    config_response: Option<SetSessionConfigOptionResponse>,
    response: Option<PromptResponse>,
    driver: Option<Result<(), SdkError>>,
    cancel_enqueue: Option<Result<(), SdkError>>,
    observed: u64,
    accepted: u64,
    candidate: Option<String>,
    finalized: bool,
}
impl KiroReceipt {
    fn new(engine: KiroEngine) -> Self {
        Self {
            engine,
            session: None,
            initialize_response: None,
            delete_response: None,
            new_response: None,
            load_response: None,
            config_response: None,
            response: None,
            driver: None,
            cancel_enqueue: None,
            observed: 0,
            accepted: 0,
            candidate: None,
            finalized: false,
        }
    }
    pub fn engine(&self) -> KiroEngine {
        self.engine
    }
    pub fn session(&self) -> Option<&SessionId> {
        self.session.as_ref()
    }
    pub fn response(&self) -> Option<&PromptResponse> {
        self.response.as_ref()
    }
    /// Actual initialization and capabilities; contains native operational data.
    pub fn initialize_response(&self) -> Option<&InitializeResponse> {
        self.initialize_response.as_ref()
    }
    /// Actual SDK deletion response. Does not establish storage erasure or Process cleanup.
    pub fn delete_response(&self) -> Option<&DeleteSessionResponse> {
        self.delete_response.as_ref()
    }
    /// Native payload for explicit inspection; never log it.
    pub fn load_response(&self) -> Option<&LoadSessionResponse> {
        self.load_response.as_ref()
    }
    /// Native payload for explicit inspection; never log it.
    pub fn new_response(&self) -> Option<&NewSessionResponse> {
        self.new_response.as_ref()
    }
    /// Full native configuration state, not a model-availability guarantee.
    pub fn config_response(&self) -> Option<&SetSessionConfigOptionResponse> {
        self.config_response.as_ref()
    }
    /// Actual connect_with result, not a Process terminal/EOF/cleanup receipt.
    pub fn driver(&self) -> Option<&Result<(), SdkError>> {
        self.driver.as_ref()
    }
    /// Acceptance by the SDK outgoing queue, not acknowledgement by the agent.
    pub fn cancel_enqueue(&self) -> Option<&Result<(), SdkError>> {
        self.cancel_enqueue.as_ref()
    }
    pub fn observed(&self) -> u64 {
        self.observed
    }
    pub fn accepted(&self) -> u64 {
        self.accepted
    }
    pub fn candidate_text(&self) -> Option<&str> {
        self.candidate.as_deref()
    }
    pub fn answer(&self) -> Option<&str> {
        self.finalized
            .then_some(self.candidate.as_deref())
            .flatten()
    }
}
impl fmt::Debug for KiroReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KiroReceipt")
            .field("engine", &self.engine)
            .field("session_observed", &self.session.is_some())
            .field(
                "initialize_response_observed",
                &self.initialize_response.is_some(),
            )
            .field("delete_response_observed", &self.delete_response.is_some())
            .field("response_observed", &self.response.is_some())
            .field("load_response_observed", &self.load_response.is_some())
            .field("new_response_observed", &self.new_response.is_some())
            .field("config_response_observed", &self.config_response.is_some())
            .field("driver_success", &self.driver.as_ref().map(Result::is_ok))
            .field(
                "cancel_enqueued",
                &self.cancel_enqueue.as_ref().map(Result::is_ok),
            )
            .field("observed", &self.observed)
            .field("accepted", &self.accepted)
            .field("finalized", &self.finalized)
            .finish()
    }
}
enum Cause {
    Reason(KiroReason),
    Context(io::Error),
    Sink(io::Error),
    Wire(serde_json::Error),
    Allocation(TryReserveError),
    Sdk(SdkError),
}
impl Cause {
    fn reason(&self) -> KiroReason {
        match self {
            Self::Reason(r) => *r,
            Self::Context(e) => KiroReason::Context(e.kind()),
            Self::Sink(e) => KiroReason::Sink(e.kind()),
            Self::Wire(_) => KiroReason::Wire,
            Self::Allocation(_) => KiroReason::Allocation,
            Self::Sdk(_) => KiroReason::Sdk,
        }
    }
}
pub struct KiroFailure {
    receipt: Box<KiroReceipt>,
    cause: Cause,
}
impl KiroFailure {
    pub fn receipt(&self) -> &KiroReceipt {
        &self.receipt
    }
    pub fn reason(&self) -> KiroReason {
        self.cause.reason()
    }
    /// Explicit inspection only; do not log SDK payloads or private identifiers.
    pub fn sdk_error(&self) -> Option<&SdkError> {
        match &self.cause {
            Cause::Sdk(e) => Some(e),
            _ => self.receipt.driver.as_ref()?.as_ref().err(),
        }
    }
    pub fn io_error(&self) -> Option<&io::Error> {
        match &self.cause {
            Cause::Context(e) | Cause::Sink(e) => Some(e),
            _ => None,
        }
    }
    pub fn decode_error(&self) -> Option<&serde_json::Error> {
        match &self.cause {
            Cause::Wire(e) => Some(e),
            _ => None,
        }
    }
    pub fn allocation_error(&self) -> Option<&TryReserveError> {
        match &self.cause {
            Cause::Allocation(e) => Some(e),
            _ => None,
        }
    }
}
impl fmt::Debug for KiroFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KiroFailure")
            .field("reason", &self.reason())
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for KiroFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for KiroFailure {}

struct State {
    receipt: KiroReceipt,
    failure: Option<Cause>,
    phase: Option<KiroDeliveryPhase>,
}
struct Shared {
    state: Mutex<State>,
    failed: Notify,
}
impl Shared {
    async fn fail(&self, cause: Cause) {
        let mut state = self.state.lock().await;
        if state.failure.is_none() {
            state.failure = Some(cause);
            self.failed.notify_one();
        }
    }
    async fn check(&self, original: &SnapshotContext) -> Result<(), SdkError> {
        if let Err(e) = original.check() {
            self.fail(Cause::Context(e)).await;
        }
        if self.state.lock().await.failure.is_some() {
            Err(SdkError::internal_error())
        } else {
            Ok(())
        }
    }
}

/// Original context is polled inside the driver, never by abandoning connect_with.
async fn wait<T>(
    original: &SnapshotContext,
    shared: &Shared,
    connection: &ConnectionTo<Agent>,
    future: impl Future<Output = Result<T, SdkError>>,
) -> Result<T, SdkError> {
    shared.check(original).await?;
    let deadline = match original.required_deadline() {
        Ok(deadline) => deadline,
        Err(error) => {
            shared.fail(Cause::Context(error)).await;
            return Err(SdkError::internal_error());
        }
    };
    let result = tokio::select! {
        biased;
        _ = original.cancellation().cancelled() => Err(Cause::Context(original.check().expect_err("cancelled original"))),
        _ = tokio::time::sleep_until(deadline.into()) => Err(Cause::Context(original.check().expect_err("expired original"))),
        _ = shared.failed.notified() => return Err(SdkError::internal_error()),
        value = future => value.map_err(Cause::Sdk),
        _ = connection.incoming_closed() => Err(Cause::Reason(KiroReason::IncomingClosed)),
    };
    match result {
        Ok(value) => Ok(value),
        Err(cause) => {
            shared.fail(cause).await;
            Err(SdkError::internal_error())
        }
    }
}

async fn deliver<S: KiroSink>(
    original: &SnapshotContext,
    shared: &Shared,
    limits: KiroLimits,
    sink: &mut S,
    message: UntypedMessage,
) {
    if shared.check(original).await.is_err() {
        return;
    }
    {
        let mut state = shared.state.lock().await;
        if state.receipt.observed == limits.notifications {
            drop(state);
            shared
                .fail(Cause::Reason(KiroReason::NotificationLimit))
                .await;
            return;
        }
        state.receipt.observed += 1;
    }
    let event = match decode_notification(message, limits.id_bytes) {
        Ok(None) => return,
        Ok(Some(event)) => event,
        Err(KiroWireError::IdLimit) => {
            shared.fail(Cause::Reason(KiroReason::IdLimit)).await;
            return;
        }
        Err(KiroWireError::Decode(e)) => {
            shared.fail(Cause::Wire(e)).await;
            return;
        }
    };
    if shared.check(original).await.is_err() {
        return;
    }
    let phase = {
        let mut state = shared.state.lock().await;
        if state.receipt.session.as_ref() != Some(&event.session_id) {
            return;
        }
        let Some(phase) = state.phase else {
            return;
        };
        if phase == KiroDeliveryPhase::Prompting
            && let SessionUpdate::AgentMessageChunk(chunk) = &event.update
            && let ContentBlock::Text(text) = &chunk.content
        {
            let old = state.receipt.candidate.as_ref().map_or(0, String::len);
            if old
                .checked_add(text.text.len())
                .is_none_or(|n| n > limits.response_text_bytes)
            {
                drop(state);
                shared.fail(Cause::Reason(KiroReason::ResponseLimit)).await;
                return;
            }
            let candidate = state.receipt.candidate.get_or_insert_with(String::new);
            if let Err(e) = candidate.try_reserve(text.text.len()) {
                drop(state);
                shared.fail(Cause::Allocation(e)).await;
                return;
            }
            candidate.push_str(&text.text);
        }
        phase
    };
    if shared.check(original).await.is_err() {
        return;
    }
    let deadline = match original.required_deadline() {
        Ok(deadline) => deadline,
        Err(error) => {
            shared.fail(Cause::Context(error)).await;
            return;
        }
    };
    let delivered = tokio::select! {
        biased;
        _ = original.cancellation().cancelled() => Err(Cause::Context(original.check().expect_err("cancelled original"))),
        _ = tokio::time::sleep_until(deadline.into()) => Err(Cause::Context(original.check().expect_err("expired original"))),
        value = sink.push(phase, &event, original) => value.map_err(Cause::Sink),
    };
    match delivered {
        Ok(()) => {
            if shared.check(original).await.is_ok() {
                shared.state.lock().await.receipt.accepted += 1;
            }
        }
        Err(cause) => shared.fail(cause).await,
    }
}

fn selected<'a>(
    options: Option<&'a [SessionConfigOption]>,
    choice: &KiroConfigSelection,
) -> Result<&'a SessionConfigSelect, KiroReason> {
    let options = options.ok_or(KiroReason::UnsupportedConfiguration)?;
    let mut matches = options
        .iter()
        .filter(|option| option.id == choice.config_id);
    let option = matches.next().ok_or(KiroReason::ConfigurationNotFound)?;
    if matches.next().is_some() {
        return Err(KiroReason::AmbiguousConfiguration);
    }
    let SessionConfigKind::Select(select) = &option.kind else {
        return Err(KiroReason::UnsupportedConfigurationKind);
    };
    let offered = match &select.options {
        SessionConfigSelectOptions::Ungrouped(values) => {
            values.iter().any(|value| value.value == choice.value_id)
        }
        SessionConfigSelectOptions::Grouped(groups) => groups.iter().any(|group| {
            group
                .options
                .iter()
                .any(|value| value.value == choice.value_id)
        }),
        _ => false,
    };
    if !offered {
        return Err(KiroReason::ConfigurationValueNotOffered);
    }
    Ok(select)
}

async fn session(
    original: &SnapshotContext,
    shared: &Shared,
    connection: ConnectionTo<Agent>,
    request: KiroSessionStart,
    prompt: KiroPrompt,
    limits: KiroLimits,
) -> Result<(), SdkError> {
    shared.check(original).await?;
    let initialized = wait(
        original,
        shared,
        &connection,
        connection
            .send_request(InitializeRequest::new(ProtocolVersion::V1))
            .block_task(),
    )
    .await?;
    let supports_load = initialized.agent_capabilities.load_session;
    let supports_roots = initialized
        .agent_capabilities
        .session_capabilities
        .additional_directories
        .is_some();
    shared.state.lock().await.receipt.initialize_response = Some(initialized);
    shared.check(original).await?;
    let mut active = false;
    let turn = async {
        let id = match request {
            KiroSessionStart::New(request) => {
                let created = wait(
                    original,
                    shared,
                    &connection,
                    connection.send_request(request).block_task(),
                )
                .await?;
                // Keep the actual created ID even on a post-response interruption.
                let post_response = shared.check(original).await;
                if created.session_id.0.len() > limits.id_bytes {
                    shared.fail(Cause::Reason(KiroReason::IdLimit)).await;
                    let result =
                        connection.send_notification(CancelNotification::new(created.session_id));
                    shared.state.lock().await.receipt.cancel_enqueue = Some(result);
                    return Err(SdkError::internal_error());
                }
                let id = created.session_id.clone();
                {
                    let mut state = shared.state.lock().await;
                    state.receipt.session = Some(id.clone());
                    state.receipt.new_response = Some(created);
                }
                active = true;
                post_response?;
                id
            }
            KiroSessionStart::Load(request) => {
                if !supports_load {
                    shared
                        .fail(Cause::Reason(KiroReason::UnsupportedLoad))
                        .await;
                    return Err(SdkError::internal_error());
                }
                if !request.additional_directories.is_empty() && !supports_roots {
                    shared
                        .fail(Cause::Reason(KiroReason::UnsupportedAdditionalDirectories))
                        .await;
                    return Err(SdkError::internal_error());
                }
                shared.check(original).await?;
                let id = request.session_id.clone();
                {
                    let mut state = shared.state.lock().await;
                    state.receipt.session = Some(id.clone());
                    state.phase = Some(KiroDeliveryPhase::Loading);
                }
                shared.check(original).await?;
                let mut sent = std::pin::pin!(connection.send_request(request).block_task());
                // The SDK settles preparation/publication failures immediately.
                // An error already ready leaves no observed request to cancel.
                // This poll does not yield or claim a peer/publication ACK.
                let ready = std::future::poll_fn(|cx| {
                    Poll::Ready(match sent.as_mut().poll(cx) {
                        Poll::Ready(result) => Some(result),
                        Poll::Pending => None,
                    })
                })
                .await;
                let loaded = match ready {
                    Some(Err(error)) => {
                        shared.fail(Cause::Sdk(error)).await;
                        return Err(SdkError::internal_error());
                    }
                    Some(Ok(loaded)) => {
                        active = true;
                        loaded
                    }
                    None => {
                        active = true;
                        wait(original, shared, &connection, sent.as_mut()).await?
                    }
                };
                shared.state.lock().await.receipt.load_response = Some(loaded);
                shared.check(original).await?;
                id
            }
        };
        shared.check(original).await?;
        if let Some(mut choice) = prompt.selection {
            let validation = {
                let state = shared.state.lock().await;
                let offers = state
                    .receipt
                    .new_response
                    .as_ref()
                    .and_then(|response| response.config_options.as_deref())
                    .or_else(|| {
                        state
                            .receipt
                            .load_response
                            .as_ref()
                            .and_then(|response| response.config_options.as_deref())
                    });
                selected(offers, &choice).map(|_| ())
            };
            if let Err(reason) = validation {
                shared.fail(Cause::Reason(reason)).await;
                return Err(SdkError::internal_error());
            }
            shared.check(original).await?;
            shared.state.lock().await.phase = Some(KiroDeliveryPhase::Configuring);
            // Construction alone is not proof of publication or peer acknowledgement.
            let sent = connection
                .send_request(
                    SetSessionConfigOptionRequest::new(
                        id.clone(),
                        choice.config_id.clone(),
                        choice.value_id.clone(),
                    )
                    .meta(choice.meta.take()),
                )
                .block_task();
            let configured = wait(original, shared, &connection, sent).await?;
            shared.state.lock().await.receipt.config_response = Some(configured);
            shared.check(original).await?;
            let confirmed = {
                let state = shared.state.lock().await;
                let response = state
                    .receipt
                    .config_response
                    .as_ref()
                    .expect("observed config response");
                selected(Some(&response.config_options), &choice)
                    .is_ok_and(|select| select.current_value == choice.value_id)
            };
            if !confirmed {
                shared
                    .fail(Cause::Reason(KiroReason::UnconfirmedConfiguration))
                    .await;
                return Err(SdkError::internal_error());
            }
            shared.check(original).await?;
        }
        shared.state.lock().await.phase = Some(KiroDeliveryPhase::Prompting);
        // No await between phase transition and SDK request publication.
        let sent = connection
            .send_request(PromptRequest::new(
                id,
                vec![ContentBlock::Text(TextContent::new(prompt.text))],
            ))
            .block_task();
        let result = wait(original, shared, &connection, sent).await?;
        shared.state.lock().await.receipt.response = Some(result);
        shared.check(original).await
    }
    .await;
    if turn.is_err() && active {
        // Queue session/cancel before the same SDK driver's close/drain phase.
        let id = shared
            .state
            .lock()
            .await
            .receipt
            .session
            .clone()
            .expect("active session");
        let result = connection.send_notification(CancelNotification::new(id));
        shared.state.lock().await.receipt.cancel_enqueue = Some(result);
    }
    turn
}

async fn delete_session(
    original: &SnapshotContext,
    shared: &Shared,
    connection: ConnectionTo<Agent>,
    request: DeleteSessionRequest,
    before_delete: &(dyn Fn() -> io::Result<()> + Sync),
) -> Result<(), SdkError> {
    shared.check(original).await?;
    let initialized = wait(
        original,
        shared,
        &connection,
        connection
            .send_request(InitializeRequest::new(ProtocolVersion::V1))
            .block_task(),
    )
    .await?;
    let supported = initialized
        .agent_capabilities
        .session_capabilities
        .delete
        .is_some();
    shared.state.lock().await.receipt.initialize_response = Some(initialized);
    shared.check(original).await?;
    if !supported {
        shared
            .fail(Cause::Reason(KiroReason::UnsupportedDelete))
            .await;
        return Err(SdkError::internal_error());
    }
    if let Err(error) = before_delete() {
        shared.fail(Cause::Context(error)).await;
        return Err(SdkError::internal_error());
    }
    shared.check(original).await?;
    {
        let mut state = shared.state.lock().await;
        state.receipt.session = Some(request.session_id.clone());
        state.phase = Some(KiroDeliveryPhase::Deleting);
    }
    let deleted = wait(
        original,
        shared,
        &connection,
        connection.send_request(request).block_task(),
    )
    .await?;
    // Preserve the native response even when the following Original check denies delivery.
    shared.state.lock().await.receipt.delete_response = Some(deleted);
    shared.check(original).await
}

enum Operation<'a> {
    Prompt {
        request: KiroSessionStart,
        prompt: KiroPrompt,
    },
    Delete {
        request: DeleteSessionRequest,
        before_delete: &'a (dyn Fn() -> io::Result<()> + Sync),
    },
}

pub async fn run_kiro<T: ConnectTo<Client> + 'static, S: KiroSink>(
    original: &Arc<SnapshotContext>,
    transport: T,
    engine: KiroEngine,
    request: KiroSessionStart,
    prompt: KiroPrompt,
    limits: KiroLimits,
    sink: &mut S,
) -> Result<KiroReceipt, KiroFailure> {
    run(
        original,
        transport,
        engine,
        Operation::Prompt { request, prompt },
        limits,
        sink,
    )
    .await
}

/// Delete a selected session from the agent's list using its actual advertised capability.
/// The caller resolves private identity, authorizes effects, and owns native child cleanup.
pub async fn delete_kiro_session<T: ConnectTo<Client> + 'static, S: KiroSink>(
    original: &Arc<SnapshotContext>,
    transport: T,
    engine: KiroEngine,
    request: DeleteSessionRequest,
    limits: KiroLimits,
    sink: &mut S,
    before_delete: &(dyn Fn() -> io::Result<()> + Sync),
) -> Result<KiroReceipt, KiroFailure> {
    run(
        original,
        transport,
        engine,
        Operation::Delete {
            request,
            before_delete,
        },
        limits,
        sink,
    )
    .await
}

async fn run<T: ConnectTo<Client> + 'static, S: KiroSink>(
    original: &Arc<SnapshotContext>,
    transport: T,
    engine: KiroEngine,
    operation: Operation<'_>,
    limits: KiroLimits,
    sink: &mut S,
) -> Result<KiroReceipt, KiroFailure> {
    let mut receipt = KiroReceipt::new(engine);
    let early = if let Err(e) = original.required_deadline() {
        Some(Cause::Context(e))
    } else if limits.prompt_text_bytes == 0
        || limits.response_text_bytes == 0
        || limits.notifications == 0
        || limits.id_bytes == 0
    {
        Some(Cause::Reason(KiroReason::Policy))
    } else if matches!(&operation, Operation::Prompt{prompt,..} if prompt.text.len() > limits.prompt_text_bytes)
    {
        Some(Cause::Reason(KiroReason::PromptLimit))
    } else if matches!(&operation, Operation::Prompt { request: KiroSessionStart::Load(r), .. } if r.session_id.0.len() > limits.id_bytes)
        || matches!(&operation, Operation::Prompt{prompt,..} if prompt.selection.as_ref().is_some_and(|selection| {
            selection.config_id.0.len() > limits.id_bytes
                || selection.value_id.0.len() > limits.id_bytes
        }))
        || matches!(&operation, Operation::Delete { request, .. } if request.session_id.0.len() > limits.id_bytes)
    {
        Some(Cause::Reason(KiroReason::IdLimit))
    } else {
        None
    };
    let early = early.or_else(|| match &operation {
        Operation::Delete {
            request,
            before_delete,
        } => {
            if request.session_id.0.is_empty() || request.session_id.0.contains('\0') {
                Some(Cause::Reason(KiroReason::Policy))
            } else {
                before_delete().err().map(Cause::Context)
            }
        }
        Operation::Prompt { .. } => None,
    });
    if let Some(cause) = early {
        return Err(KiroFailure {
            receipt: Box::new(receipt),
            cause,
        });
    }
    let shared = Shared {
        state: Mutex::new(State {
            receipt,
            failure: None,
            phase: None,
        }),
        failed: Notify::new(),
    };
    let driver = QuietSdk::new(async {
        Client
            .builder()
            .on_receive_notification(
                async |message: UntypedMessage, _| {
                    deliver(original, &shared, limits, sink, message).await;
                    // Our private cause wakes main; returning payload errors here could log them.
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(transport, async |connection: ConnectionTo<Agent>| {
                let result = match operation {
                    Operation::Prompt { request, prompt } => {
                        session(original, &shared, connection, request, prompt, limits).await
                    }
                    Operation::Delete {
                        request,
                        before_delete,
                    } => {
                        delete_session(original, &shared, connection, request, before_delete).await
                    }
                };
                if let Err(error) = result {
                    let missing = shared.state.lock().await.failure.is_none();
                    if missing {
                        shared.fail(Cause::Sdk(error)).await;
                    }
                }
                // Consumer failure is private. A successful main closure permits
                // the SDK outgoing drain to carry the queued session/cancel.
                // Physical/handler failures still surface from connect_with.
                Ok(())
            })
            .await
    })
    .await;
    let State {
        receipt: saved,
        mut failure,
        phase: _,
    } = shared.state.into_inner();
    receipt = saved;
    receipt.driver = Some(driver);
    if failure.is_none() && receipt.driver.as_ref().is_some_and(Result::is_err) {
        failure = Some(Cause::Reason(KiroReason::Sdk));
    }
    if failure.is_none()
        && let Err(e) = original.check()
    {
        failure = Some(Cause::Context(e));
    }
    if failure.is_none()
        && receipt
            .response
            .as_ref()
            .is_some_and(|r| r.stop_reason == StopReason::EndTurn)
    {
        if receipt.candidate.is_none() {
            failure = Some(Cause::Reason(KiroReason::MissingText));
        } else {
            receipt.finalized = true;
        }
    }
    match failure {
        Some(cause) => Err(KiroFailure {
            receipt: Box::new(receipt),
            cause,
        }),
        None => Ok(receipt),
    }
}
