//! OpenCode 2 through ACP protocol v1, with exact private-session loading.
//! The caller owns native process retirement.
use crate::{
    cli_acp_support::QuietSdk,
    cli_opencode_wire::{OpencodeUpdate, OpencodeWireError, decode_update},
};
use agent_client_protocol::{
    Agent, Client, ConnectTo, ConnectionTo, Error as SdkError, UntypedMessage,
    schema::{ProtocolVersion, v1::*},
};
use graphit_core::file_io::SnapshotContext;
use std::{fmt, future::Future, io, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, Notify};

pub enum OpencodeSessionStart {
    Create,
    Load(SessionId),
}
impl fmt::Debug for OpencodeSessionStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Create => "Create",
            Self::Load(_) => "Load",
        })
    }
}
pub struct OpencodeRequest {
    pub session: OpencodeSessionStart,
    pub cwd: PathBuf,
    /// None preserves the native session/default; Some is an explicit literal selection.
    pub model: Option<String>,
    /// An explicit published primary-capable agent, selected through the announced mode option.
    pub agent: Option<String>,
    pub prompt: String,
}
#[derive(Clone, Copy, Debug)]
pub struct OpencodeLimits {
    pub prompt_text_bytes: usize,
    pub response_text_bytes: usize,
    pub notifications: u64,
    pub id_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpencodePhase {
    Creating,
    Loading,
    Configuring,
    Prompting,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpencodeReason {
    Policy,
    PromptLimit,
    ResponseLimit,
    NotificationLimit,
    IdLimit,
    Wire,
    Protocol,
    Compatibility,
    Capability,
    Configuration,
    Session,
    Permission,
    Terminal,
    MissingText,
    IncomingClosed,
    Allocation,
    Context(io::ErrorKind),
    Sink(io::ErrorKind),
    Sdk,
}
pub trait OpencodeSink: Send {
    /// Revalidate the consumer's typed admissions before native effects and after waits.
    fn recheck(&self, original: &SnapshotContext) -> io::Result<()>;
    /// Observe Create or confirm the exact private Load target after its correlated ACK.
    fn session_bound(
        &mut self,
        id: &str,
        loaded: bool,
        original: &SnapshotContext,
    ) -> io::Result<()>;
    /// Serial and cooperative: cancellation of this future must leave no detached work.
    fn push<'a>(
        &'a mut self,
        phase: OpencodePhase,
        update: &'a OpencodeUpdate,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = io::Result<()>> + Send + 'a;
}

/// Private native fields are available only through explicit inspection.
pub struct OpencodeReceipt {
    initialize: Option<InitializeResponse>,
    created: Option<NewSessionResponse>,
    loaded: Option<LoadSessionResponse>,
    configured: Option<SetSessionConfigOptionResponse>,
    response: Option<PromptResponse>,
    driver: Option<Result<(), SdkError>>,
    cancel_enqueue: Option<Result<(), SdkError>>,
    observed: u64,
    replayed: u64,
    accepted: u64,
    denied_permissions: u64,
    candidate: Option<String>,
    eligible: bool,
}
impl OpencodeReceipt {
    fn new() -> Self {
        Self {
            initialize: None,
            created: None,
            loaded: None,
            configured: None,
            response: None,
            driver: None,
            cancel_enqueue: None,
            observed: 0,
            replayed: 0,
            accepted: 0,
            denied_permissions: 0,
            candidate: None,
            eligible: false,
        }
    }
    pub fn initialize(&self) -> Option<&InitializeResponse> {
        self.initialize.as_ref()
    }
    pub fn created(&self) -> Option<&NewSessionResponse> {
        self.created.as_ref()
    }
    pub fn loaded(&self) -> Option<&LoadSessionResponse> {
        self.loaded.as_ref()
    }
    /// Native notifications audited while loading; excluded from new-turn progress.
    pub fn replayed(&self) -> u64 {
        self.replayed
    }
    pub fn configured(&self) -> Option<&SetSessionConfigOptionResponse> {
        self.configured.as_ref()
    }
    pub fn response(&self) -> Option<&PromptResponse> {
        self.response.as_ref()
    }
    pub fn driver(&self) -> Option<&Result<(), SdkError>> {
        self.driver.as_ref()
    }
    /// Queue acceptance, never an acknowledgement of agent cancellation or native join.
    pub fn cancel_enqueue(&self) -> Option<&Result<(), SdkError>> {
        self.cancel_enqueue.as_ref()
    }
    pub fn observed(&self) -> u64 {
        self.observed
    }
    pub fn accepted(&self) -> u64 {
        self.accepted
    }
    pub fn denied_permissions(&self) -> u64 {
        self.denied_permissions
    }
    pub fn candidate_text(&self) -> Option<&str> {
        self.candidate.as_deref()
    }
    /// ACP eligibility only. The caller must still prove child/EOF/custody and authority.
    pub fn eligible_text(&self) -> Option<&str> {
        self.eligible.then_some(self.candidate.as_deref()).flatten()
    }
}
impl fmt::Debug for OpencodeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpencodeReceipt")
            .field("initialized", &self.initialize.is_some())
            .field("created", &self.created.is_some())
            .field("loaded", &self.loaded.is_some())
            .field("replayed", &self.replayed)
            .field("configured", &self.configured.is_some())
            .field("terminal", &self.response.is_some())
            .field("driver_observed", &self.driver.is_some())
            .field(
                "cancel_queued",
                &self.cancel_enqueue.as_ref().map(Result::is_ok),
            )
            .field("observed", &self.observed)
            .field("accepted", &self.accepted)
            .field("denied_permissions", &self.denied_permissions)
            .field("candidate_bytes", &self.candidate.as_ref().map(String::len))
            .field("eligible", &self.eligible)
            .finish()
    }
}
enum Cause {
    Reason(OpencodeReason),
    Context(io::Error),
    Sink(io::Error),
    Sdk(SdkError),
    Decode(serde_json::Error),
    Allocation(std::collections::TryReserveError),
}
pub struct OpencodeFailure {
    receipt: Box<OpencodeReceipt>,
    cause: Cause,
}
impl OpencodeFailure {
    pub fn receipt(&self) -> &OpencodeReceipt {
        &self.receipt
    }
    pub fn reason(&self) -> OpencodeReason {
        match &self.cause {
            Cause::Reason(reason) => *reason,
            Cause::Context(error) => OpencodeReason::Context(error.kind()),
            Cause::Sink(error) => OpencodeReason::Sink(error.kind()),
            Cause::Sdk(_) => OpencodeReason::Sdk,
            Cause::Decode(_) => OpencodeReason::Wire,
            Cause::Allocation(_) => OpencodeReason::Allocation,
        }
    }
    pub fn sdk_error(&self) -> Option<&SdkError> {
        match &self.cause {
            Cause::Sdk(error) => Some(error),
            _ => None,
        }
    }
    pub fn decode_error(&self) -> Option<&serde_json::Error> {
        match &self.cause {
            Cause::Decode(error) => Some(error),
            _ => None,
        }
    }
    pub fn allocation_error(&self) -> Option<&std::collections::TryReserveError> {
        match &self.cause {
            Cause::Allocation(error) => Some(error),
            _ => None,
        }
    }
}
impl fmt::Debug for OpencodeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpencodeFailure")
            .field("reason", &self.reason())
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for OpencodeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for OpencodeFailure {}

struct State {
    receipt: OpencodeReceipt,
    failure: Option<Cause>,
    phase: OpencodePhase,
    session_id: Option<SessionId>,
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
        if let Err(error) = original.check() {
            self.fail(Cause::Context(error)).await;
        }
        if self.state.lock().await.failure.is_some() {
            Err(SdkError::internal_error())
        } else {
            Ok(())
        }
    }
}
async fn sink_call<T>(
    original: &SnapshotContext,
    shared: &Shared,
    future: impl Future<Output = io::Result<T>>,
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
        _ = original.cancellation().cancelled() => Err(Cause::Context(original.check().expect_err("cancelled Original"))),
        _ = tokio::time::sleep_until(deadline.into()) => Err(Cause::Context(original.check().expect_err("expired Original"))),
        result = future => result.map_err(Cause::Sink),
    };
    match result {
        Ok(value) => Ok(value),
        Err(cause) => {
            shared.fail(cause).await;
            Err(SdkError::internal_error())
        }
    }
}
async fn recheck<S: OpencodeSink>(
    original: &SnapshotContext,
    shared: &Shared,
    sink: &Mutex<&mut S>,
) -> Result<(), SdkError> {
    sink_call(original, shared, async {
        sink.lock().await.recheck(original)
    })
    .await
}

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
        _ = original.cancellation().cancelled() => Err(Cause::Context(original.check().expect_err("cancelled Original"))),
        _ = tokio::time::sleep_until(deadline.into()) => Err(Cause::Context(original.check().expect_err("expired Original"))),
        _ = shared.failed.notified() => return Err(SdkError::internal_error()),
        result = future => result.map_err(Cause::Sdk),
        _ = connection.incoming_closed() => Err(Cause::Reason(OpencodeReason::IncomingClosed)),
    };
    match result {
        Ok(value) => Ok(value),
        Err(cause) => {
            shared.fail(cause).await;
            Err(SdkError::internal_error())
        }
    }
}

fn selected<'a>(
    options: &'a [SessionConfigOption],
    config_id: &str,
    value_id: &str,
) -> Result<&'a SessionConfigSelect, OpencodeReason> {
    let mut models = options
        .iter()
        .filter(|option| option.id.0.as_ref() == config_id);
    let option = models.next().ok_or(OpencodeReason::Configuration)?;
    if models.next().is_some() {
        return Err(OpencodeReason::Configuration);
    }
    let value = serde_json::to_value(option).map_err(|_| OpencodeReason::Configuration)?;
    if value.get("category").and_then(serde_json::Value::as_str) != Some(config_id) {
        return Err(OpencodeReason::Configuration);
    }
    let SessionConfigKind::Select(select) = &option.kind else {
        return Err(OpencodeReason::Configuration);
    };
    let SessionConfigSelectOptions::Ungrouped(values) = &select.options else {
        return Err(OpencodeReason::Configuration);
    };
    if !values
        .iter()
        .any(|value| value.value.0.as_ref() == value_id)
    {
        return Err(OpencodeReason::Configuration);
    }
    Ok(select)
}

async fn deliver<S: OpencodeSink>(
    original: &SnapshotContext,
    shared: &Shared,
    limits: OpencodeLimits,
    sink: &Mutex<&mut S>,
    message: UntypedMessage,
) {
    if recheck(original, shared, sink).await.is_err() {
        return;
    }
    {
        let mut state = shared.state.lock().await;
        if state
            .receipt
            .observed
            .checked_add(state.receipt.replayed)
            .is_none_or(|count| count >= limits.notifications)
        {
            drop(state);
            shared
                .fail(Cause::Reason(OpencodeReason::NotificationLimit))
                .await;
            return;
        }
        if state.phase == OpencodePhase::Loading {
            state.receipt.replayed += 1;
        } else {
            state.receipt.observed += 1;
        }
    }
    let update = match decode_update(message, limits.id_bytes) {
        Ok(Some(update)) => update,
        Ok(None) => return,
        Err(error) => {
            shared
                .fail(match error {
                    OpencodeWireError::Decode(error) => Cause::Decode(error),
                    OpencodeWireError::IdLimit => Cause::Reason(OpencodeReason::IdLimit),
                    OpencodeWireError::Envelope => Cause::Reason(OpencodeReason::Wire),
                })
                .await;
            return;
        }
    };
    let phase = {
        let state = shared.state.lock().await;
        if let Some(id) = &state.session_id {
            if update.session_id() != Some(id.0.as_ref()) {
                drop(state);
                shared.fail(Cause::Reason(OpencodeReason::Session)).await;
                return;
            }
        } else {
            return;
        }
        state.phase
    };
    // The SDK runs notification callbacks in dispatch order. A Load response is
    // delivered only after the earlier replay callbacks have completed.
    if phase == OpencodePhase::Loading {
        return;
    }
    let text = match &update {
        OpencodeUpdate::Known(event) if phase == OpencodePhase::Prompting => match &event.update {
            SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                ContentBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    };
    if let Some(text) = text {
        let state = shared.state.lock().await;
        if state
            .receipt
            .candidate
            .as_ref()
            .map_or(0, String::len)
            .checked_add(text.len())
            .is_none_or(|n| n > limits.response_text_bytes)
        {
            drop(state);
            shared
                .fail(Cause::Reason(OpencodeReason::ResponseLimit))
                .await;
            return;
        }
    }
    let delivered = sink_call(original, shared, async {
        sink.lock().await.push(phase, &update, original).await
    })
    .await;
    if delivered.is_err() || recheck(original, shared, sink).await.is_err() {
        return;
    }
    let mut state = shared.state.lock().await;
    if let Some(text) = text {
        let candidate = state.receipt.candidate.get_or_insert_with(String::new);
        if let Err(error) = candidate.try_reserve(text.len()) {
            drop(state);
            shared.fail(Cause::Allocation(error)).await;
            return;
        }
        candidate.push_str(text);
    }
    state.receipt.accepted += 1;
}

async fn session<S: OpencodeSink>(
    original: &SnapshotContext,
    shared: &Shared,
    connection: ConnectionTo<Agent>,
    request: OpencodeRequest,
    limits: OpencodeLimits,
    sink: &Mutex<&mut S>,
) -> Result<(), SdkError> {
    recheck(original, shared, sink).await?;
    let initialized = wait(
        original,
        shared,
        &connection,
        connection
            .send_request(InitializeRequest::new(ProtocolVersion::V1))
            .block_task(),
    )
    .await?;
    let protocol_ok = initialized.protocol_version == ProtocolVersion::V1;
    let native = serde_json::to_value(&initialized);
    let compatible = native.as_ref().is_ok_and(|native| {
        native
            .pointer("/agentInfo/name")
            .and_then(serde_json::Value::as_str)
            == Some("OpenCode")
            && native
                .pointer("/agentInfo/version")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|version| version.starts_with("2."))
    });
    let can_load = native.as_ref().is_ok_and(|native| {
        native
            .pointer("/agentCapabilities/loadSession")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    });
    drop(native);
    shared.state.lock().await.receipt.initialize = Some(initialized);
    recheck(original, shared, sink).await?;
    if !protocol_ok {
        shared.fail(Cause::Reason(OpencodeReason::Protocol)).await;
        return Err(SdkError::internal_error());
    }
    if !compatible {
        shared
            .fail(Cause::Reason(OpencodeReason::Compatibility))
            .await;
        return Err(SdkError::internal_error());
    }
    let loading = matches!(&request.session, OpencodeSessionStart::Load(_));
    let mut active_id = None;
    let turn = async {
        let id = match request.session {
            OpencodeSessionStart::Create => {
                recheck(original, shared, sink).await?;
                let created = wait(
                    original,
                    shared,
                    &connection,
                    connection
                        .send_request(NewSessionRequest::new(request.cwd.clone()))
                        .block_task(),
                )
                .await?;
                let id = created.session_id.clone();
                shared.state.lock().await.receipt.created = Some(created);
                active_id = Some(id.clone());
                id
            }
            OpencodeSessionStart::Load(id) => {
                if !can_load {
                    shared.fail(Cause::Reason(OpencodeReason::Capability)).await;
                    return Err(SdkError::internal_error());
                }
                // Select the private target before replay; no synthetic Create response.
                shared.state.lock().await.session_id = Some(id.clone());
                recheck(original, shared, sink).await?;
                active_id = Some(id.clone());
                let loaded = wait(
                    original,
                    shared,
                    &connection,
                    connection
                        .send_request(LoadSessionRequest::new(id.clone(), request.cwd.clone()))
                        .block_task(),
                )
                .await?;
                shared.state.lock().await.receipt.loaded = Some(loaded);
                id
            }
        };
        recheck(original, shared, sink).await?;
        if id.0.is_empty() || id.0.contains('\0') || id.0.len() > limits.id_bytes {
            shared.fail(Cause::Reason(OpencodeReason::IdLimit)).await;
            return Err(SdkError::internal_error());
        }
        shared.state.lock().await.session_id = Some(id.clone());
        sink_call(original, shared, async {
            sink.lock()
                .await
                .session_bound(id.0.as_ref(), loading, original)
        })
        .await?;
        recheck(original, shared, sink).await?;
        // The original loaded model binding must be checked before agent configuration
        // can change the currently advertised options.
        if loading && let Some(model) = request.model.as_deref() {
            let selection = {
                let state = shared.state.lock().await;
                state
                    .receipt
                    .loaded
                    .as_ref()
                    .and_then(|loaded| loaded.config_options.as_deref())
                    .ok_or(OpencodeReason::Configuration)
                    .and_then(|options| selected(options, "model", model))
                    .and_then(|select| {
                        if select.current_value.0.as_ref() == model {
                            Ok(())
                        } else {
                            Err(OpencodeReason::Configuration)
                        }
                    })
            };
            if let Err(reason) = selection {
                shared.fail(Cause::Reason(reason)).await;
                return Err(SdkError::internal_error());
            }
            recheck(original, shared, sink).await?;
        }
        if let Some(agent) = request.agent.as_deref() {
            let selection = {
                let state = shared.state.lock().await;
                let options = if loading {
                    state
                        .receipt
                        .loaded
                        .as_ref()
                        .and_then(|loaded| loaded.config_options.as_deref())
                } else {
                    state
                        .receipt
                        .created
                        .as_ref()
                        .and_then(|created| created.config_options.as_deref())
                };
                options
                    .ok_or(OpencodeReason::Configuration)
                    .and_then(|options| selected(options, "mode", agent))
                    .map(|_| ())
            };
            if let Err(reason) = selection {
                shared.fail(Cause::Reason(reason)).await;
                return Err(SdkError::internal_error());
            }
            recheck(original, shared, sink).await?;
            shared.state.lock().await.phase = OpencodePhase::Configuring;
            let configured = wait(
                original,
                shared,
                &connection,
                connection
                    .send_request(SetSessionConfigOptionRequest::new(
                        id.clone(),
                        SessionConfigId::new("mode"),
                        SessionConfigValueId::new(agent.to_owned()),
                    ))
                    .block_task(),
            )
            .await?;
            let confirmed = selected(&configured.config_options, "mode", agent)
                .is_ok_and(|select| select.current_value.0.as_ref() == agent)
                && request.model.as_deref().is_none_or(|model| {
                    selected(&configured.config_options, "model", model).is_ok()
                });
            drop(configured);
            recheck(original, shared, sink).await?;
            if !confirmed {
                shared
                    .fail(Cause::Reason(OpencodeReason::Configuration))
                    .await;
                return Err(SdkError::internal_error());
            }
            recheck(original, shared, sink).await?;
        }
        if let Some(model) = request.model.as_deref() {
            // An agent ACK already validated the currently offered model above.
            if request.agent.is_none() {
                let selection = {
                    let state = shared.state.lock().await;
                    let options = if loading {
                        state
                            .receipt
                            .loaded
                            .as_ref()
                            .and_then(|loaded| loaded.config_options.as_deref())
                    } else {
                        state
                            .receipt
                            .created
                            .as_ref()
                            .and_then(|created| created.config_options.as_deref())
                    };
                    options
                        .ok_or(OpencodeReason::Configuration)
                        .and_then(|options| selected(options, "model", model))
                        .map(|_| ())
                };
                if let Err(reason) = selection {
                    shared.fail(Cause::Reason(reason)).await;
                    return Err(SdkError::internal_error());
                }
            }
            recheck(original, shared, sink).await?;
            shared.state.lock().await.phase = OpencodePhase::Configuring;
            let configured = wait(
                original,
                shared,
                &connection,
                connection
                    .send_request(SetSessionConfigOptionRequest::new(
                        id.clone(),
                        SessionConfigId::new("model"),
                        SessionConfigValueId::new(model.to_owned()),
                    ))
                    .block_task(),
            )
            .await?;
            let confirmed = selected(&configured.config_options, "model", model)
                .is_ok_and(|select| select.current_value.0.as_ref() == model)
                && request.agent.as_deref().is_none_or(|agent| {
                    selected(&configured.config_options, "mode", agent)
                        .is_ok_and(|select| select.current_value.0.as_ref() == agent)
                });
            shared.state.lock().await.receipt.configured = Some(configured);
            recheck(original, shared, sink).await?;
            if !confirmed {
                shared
                    .fail(Cause::Reason(OpencodeReason::Configuration))
                    .await;
                return Err(SdkError::internal_error());
            }
            recheck(original, shared, sink).await?;
        }
        shared.state.lock().await.phase = OpencodePhase::Prompting;
        let response = wait(
            original,
            shared,
            &connection,
            connection
                .send_request(PromptRequest::new(
                    id,
                    vec![ContentBlock::Text(TextContent::new(request.prompt))],
                ))
                .block_task(),
        )
        .await?;
        let ended = response.stop_reason == StopReason::EndTurn;
        {
            let mut state = shared.state.lock().await;
            state.receipt.response = Some(response);
            state.phase = OpencodePhase::Finished;
        }
        recheck(original, shared, sink).await?;
        if !ended {
            shared.fail(Cause::Reason(OpencodeReason::Terminal)).await;
            return Err(SdkError::internal_error());
        }
        shared.check(original).await
    }
    .await;
    if turn.is_err()
        && let Some(id) = active_id
    {
        let queued = connection.send_notification(CancelNotification::new(id));
        shared.state.lock().await.receipt.cancel_enqueue = Some(queued);
    }
    turn
}

pub async fn run_opencode<T: ConnectTo<Client> + 'static, S: OpencodeSink>(
    original: &Arc<SnapshotContext>,
    transport: T,
    request: OpencodeRequest,
    limits: OpencodeLimits,
    sink: &mut S,
) -> Result<OpencodeReceipt, OpencodeFailure> {
    let cause = if let Err(error) = original.required_deadline() {
        Some(Cause::Context(error))
    } else if limits.prompt_text_bytes == 0
        || limits.response_text_bytes == 0
        || limits.notifications == 0
        || limits.id_bytes == 0
        || !request.cwd.is_absolute()
        || request
            .model
            .as_ref()
            .is_some_and(|model| model.is_empty() || model.contains('\0'))
        || request
            .agent
            .as_ref()
            .is_some_and(|agent| agent.is_empty() || agent.contains('\0'))
    {
        Some(Cause::Reason(OpencodeReason::Policy))
    } else if request.prompt.len() > limits.prompt_text_bytes {
        Some(Cause::Reason(OpencodeReason::PromptLimit))
    } else if request
        .model
        .as_ref()
        .is_some_and(|model| model.len() > limits.id_bytes)
        || request
            .agent
            .as_ref()
            .is_some_and(|agent| agent.len() > limits.id_bytes)
        || matches!(&request.session, OpencodeSessionStart::Load(id) if id.0.is_empty() || id.0.contains('\0') || id.0.len() > limits.id_bytes)
    {
        Some(Cause::Reason(OpencodeReason::IdLimit))
    } else {
        None
    };
    if let Some(cause) = cause {
        return Err(OpencodeFailure {
            receipt: Box::new(OpencodeReceipt::new()),
            cause,
        });
    }
    let loading = matches!(&request.session, OpencodeSessionStart::Load(_));
    let sink = Mutex::new(sink);
    let shared = Shared {
        state: Mutex::new(State {
            receipt: OpencodeReceipt::new(),
            failure: None,
            phase: if loading {
                OpencodePhase::Loading
            } else {
                OpencodePhase::Creating
            },
            session_id: None,
        }),
        failed: Notify::new(),
    };
    let driver = QuietSdk::new(async {
        Client
            .builder()
            .on_receive_notification(
                async |message: UntypedMessage, _| {
                    deliver(original, &shared, limits, &sink, message).await;
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async |_: RequestPermissionRequest, responder, _| {
                    let _ = shared.check(original).await;
                    shared.state.lock().await.receipt.denied_permissions += 1;
                    shared.fail(Cause::Reason(OpencodeReason::Permission)).await;
                    responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(transport, async |connection: ConnectionTo<Agent>| {
                if let Err(error) =
                    session(original, &shared, connection, request, limits, &sink).await
                {
                    shared.fail(Cause::Sdk(error)).await;
                }
                // Let the SDK drain any queued cancel; a private failure is never logged.
                Ok(())
            })
            .await
    })
    .await;
    let _ = recheck(original, &shared, &sink).await;
    let State {
        mut receipt,
        mut failure,
        ..
    } = shared.state.into_inner();
    receipt.driver = Some(driver);
    if failure.is_none() && receipt.driver.as_ref().is_some_and(Result::is_err) {
        failure = Some(Cause::Reason(OpencodeReason::Sdk));
    }
    if failure.is_none() {
        failure = original.check().err().map(Cause::Context);
    }
    if failure.is_none()
        && receipt
            .response
            .as_ref()
            .is_none_or(|response| response.stop_reason != StopReason::EndTurn)
    {
        failure = Some(Cause::Reason(OpencodeReason::Terminal));
    }
    if failure.is_none() && receipt.candidate.is_none() {
        failure = Some(Cause::Reason(OpencodeReason::MissingText));
    }
    match failure {
        Some(cause) => Err(OpencodeFailure {
            receipt: Box::new(receipt),
            cause,
        }),
        None => {
            receipt.eligible = true;
            Ok(receipt)
        }
    }
}
