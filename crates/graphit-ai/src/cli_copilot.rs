//! Copilot prompt-mode JSONL over the existing Process owner; no SDK or TCP service.
use crate::cli_codex_wire::{CodexWireLimits, audit_json_frame};
use graphit_auth::{ProjectReadAdmission, ProjectReadModule};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Cleanup, Failure, IncrementalSink, Outcome, PipeEnd, ProcessHost, Request, Retention,
    SinkFailure, Stream,
};
use secrecy::zeroize::Zeroizing;
use serde::Deserialize;
use serde_json::{Value, value::RawValue};
use std::{error::Error, fmt, future::Future, io, sync::Arc};

#[derive(Clone, Copy)]
pub struct CopilotLimits {
    pub wire: CodexWireLimits,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub prompt_text_bytes: usize,
    pub response_text_bytes: usize,
}
pub enum CopilotSessionStart {
    Create,
    Resume(String),
}
impl fmt::Debug for CopilotSessionStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Create => "Create",
            Self::Resume(_) => "Resume",
        })
    }
}
pub struct CopilotRequest {
    pub command: Request,
    pub session: CopilotSessionStart,
    pub model: String,
    pub prompt: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopilotReason {
    Policy,
    Limit,
    Allocation,
    Wire,
    Protocol,
    Permission,
    Terminal,
    MissingText,
    Closed,
    Admission,
    Process,
    NonzeroExit,
    Context(io::ErrorKind),
    Sink(io::ErrorKind),
}
pub trait CopilotSink: Send {
    fn recheck(&self, original: &SnapshotContext) -> io::Result<()>;
    /// Native result confirms the selected session. This is not a pre-prompt ACK.
    fn session_bound(
        &mut self,
        id: &str,
        resumed: bool,
        original: &SnapshotContext,
    ) -> io::Result<()>;
    fn push<'a>(
        &'a mut self,
        event: &'a RawValue,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = io::Result<()>> + Send + 'a;
}
#[derive(Default)]
pub struct CopilotReceipt {
    session: Option<String>,
    prompt: Option<String>,
    turn: Option<String>,
    message: Option<String>,
    text: String,
    terminal: Option<Box<RawValue>>,
    native_error: Option<Box<RawValue>>,
    nonterminal_error: Option<Box<RawValue>>,
    eligible: bool,
    process: Option<Result<Outcome, Failure>>,
}
impl CopilotReceipt {
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }
    pub fn turn(&self) -> Option<&str> {
        self.turn.as_deref()
    }
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn terminal(&self) -> Option<&RawValue> {
        self.terminal.as_deref()
    }
    pub fn native_error(&self) -> Option<&RawValue> {
        self.native_error.as_deref()
    }
    pub fn nonterminal_error(&self) -> Option<&RawValue> {
        self.nonterminal_error.as_deref()
    }
    pub fn eligible_text(&self) -> Option<&str> {
        self.eligible.then_some(self.text.as_str())
    }
    pub fn process(&self) -> Option<&Result<Outcome, Failure>> {
        self.process.as_ref()
    }
    pub fn process_mut(&mut self) -> Option<&mut Result<Outcome, Failure>> {
        self.process.as_mut()
    }
}
impl fmt::Debug for CopilotReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CopilotReceipt")
            .field("eligible", &self.eligible)
            .field("process", &self.process)
            .finish_non_exhaustive()
    }
}
pub struct CopilotFailure {
    receipt: Box<CopilotReceipt>,
    reason: CopilotReason,
}
impl CopilotFailure {
    pub fn reason(&self) -> CopilotReason {
        self.reason
    }
    pub fn receipt(&self) -> &CopilotReceipt {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut CopilotReceipt {
        &mut self.receipt
    }
}
impl fmt::Debug for CopilotFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CopilotFailure").field(&self.reason).finish()
    }
}
impl fmt::Display for CopilotFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl Error for CopilotFailure {}
fn check(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
) -> Result<(), CopilotReason> {
    original
        .check()
        .map_err(|e| CopilotReason::Context(e.kind()))?;
    if admission.module() != ProjectReadModule::Knowledge {
        return Err(CopilotReason::Admission);
    }
    let (_, request) = admission
        .project_and_request()
        .map_err(|_| CopilotReason::Admission)?;
    if !Arc::ptr_eq(original, request.snapshot()) {
        return Err(CopilotReason::Admission);
    }
    Ok(())
}
fn session_id(value: &str, maximum: usize) -> bool {
    value.len() <= maximum
        && uuid::Uuid::parse_str(value).is_ok_and(|id| id.hyphenated().to_string() == value)
}
fn text<'a>(data: &'a Value, field: &str) -> Result<&'a str, CopilotReason> {
    data.get(field)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or(CopilotReason::Protocol)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event<'a> {
    #[serde(rename = "type", borrow)]
    kind: &'a str,
    #[serde(default, borrow)]
    id: Option<&'a str>,
    #[serde(default, borrow)]
    agent_id: Option<&'a str>,
    #[serde(default)]
    data: Value,
    #[serde(default, borrow)]
    session_id: Option<&'a str>,
    #[serde(default)]
    exit_code: Option<i32>,
}
struct Bridge<'a, 'r, S> {
    original: &'a Arc<SnapshotContext>,
    admission: &'a ProjectReadAdmission<'r>,
    sink: &'a mut S,
    limits: CopilotLimits,
    expected: Option<String>,
    pending: Vec<u8>,
    stdout: usize,
    receipt: CopilotReceipt,
    ended: bool,
    failure: Option<CopilotReason>,
}
impl<S: CopilotSink> Bridge<'_, '_, S> {
    fn check(&self) -> Result<(), CopilotReason> {
        check(self.original, self.admission)?;
        self.sink
            .recheck(self.original)
            .map_err(|e| CopilotReason::Sink(e.kind()))
    }
    async fn frame(&mut self) -> Result<(), CopilotReason> {
        self.check()?;
        let mut bytes = std::mem::take(&mut self.pending);
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.is_empty() {
            return Err(CopilotReason::Wire);
        }
        audit_json_frame(&bytes, &self.limits.wire, self.original)
            .map_err(|_| CopilotReason::Wire)?;
        let raw: Box<RawValue> = serde_json::from_slice(&bytes).map_err(|_| CopilotReason::Wire)?;
        let event: Event<'_> = serde_json::from_slice(&bytes).map_err(|_| CopilotReason::Wire)?;
        if self.receipt.terminal.is_some() {
            return Err(CopilotReason::Protocol);
        }
        let private = self.observe(&event, &raw)?;
        if !private {
            self.check()?;
            let deadline = self
                .original
                .required_deadline()
                .map_err(|e| CopilotReason::Context(e.kind()))?;
            // Also covers final EOF fragments, after Process has returned custody.
            tokio::select! {
                biased;
                _ = self.original.cancellation().cancelled() => return Err(CopilotReason::Context(io::ErrorKind::Interrupted)),
                _ = tokio::time::sleep_until(deadline.into()) => return Err(CopilotReason::Context(io::ErrorKind::TimedOut)),
                result = self.sink.push(&raw, self.original) => result.map_err(|e| CopilotReason::Sink(e.kind()))?,
            }
            self.check()?;
        }
        self.pending = bytes;
        self.pending.clear();
        Ok(())
    }
    fn observe(&mut self, event: &Event<'_>, raw: &RawValue) -> Result<bool, CopilotReason> {
        let copy_raw =
            || RawValue::from_string(raw.get().to_owned()).map_err(|_| CopilotReason::Allocation);
        if event.kind == "result" {
            let id = event
                .session_id
                .filter(|id| session_id(id, self.limits.wire.id_bytes))
                .ok_or(CopilotReason::Protocol)?;
            if self
                .expected
                .as_deref()
                .is_some_and(|expected| expected != id)
            {
                return Err(CopilotReason::Protocol);
            }
            self.receipt.terminal = Some(copy_raw()?);
            if event.exit_code != Some(0) {
                self.receipt.native_error = Some(copy_raw()?);
                return Err(CopilotReason::Terminal);
            }
            if !self.ended || self.receipt.message.is_none() {
                return Err(CopilotReason::Terminal);
            }
            if self.receipt.text.trim().is_empty() {
                return Err(CopilotReason::MissingText);
            }
            self.check()?;
            self.sink
                .session_bound(id, self.expected.is_some(), self.original)
                .map_err(|e| CopilotReason::Sink(e.kind()))?;
            self.receipt.session = Some(id.to_owned());
            self.check()?;
            return Ok(true);
        }
        if matches!(event.kind, "session.start" | "session.resume") {
            return Ok(true);
        }
        let child = event.agent_id.is_some()
            || event
                .data
                .get("parentToolCallId")
                .is_some_and(|v| !v.is_null());
        if event.kind == "session.error" {
            if child || event.data.get("errorType").and_then(Value::as_str) == Some("model_call") {
                self.receipt.nonterminal_error.get_or_insert(copy_raw()?);
                return Ok(true);
            }
            self.receipt.native_error = Some(copy_raw()?);
            return Err(CopilotReason::Terminal);
        }
        if event.kind == "abort" {
            return Err(CopilotReason::Terminal);
        }
        if event.kind == "permission.requested" {
            return Err(CopilotReason::Permission);
        }
        if child {
            return Ok(false);
        }
        let bounded_id = |v: &str| {
            if v.is_empty() || v.len() > self.limits.wire.id_bytes {
                Err(CopilotReason::Protocol)
            } else {
                Ok(v.to_owned())
            }
        };
        match event.kind {
            "user.message" => {
                if self.receipt.prompt.is_some() {
                    return Err(CopilotReason::Protocol);
                }
                self.receipt.prompt = Some(bounded_id(event.id.ok_or(CopilotReason::Protocol)?)?);
            }
            "assistant.turn_start" => {
                if self.receipt.prompt.is_none() || (self.receipt.turn.is_some() && !self.ended) {
                    return Err(CopilotReason::Protocol);
                }
                self.receipt.turn = Some(bounded_id(text(&event.data, "turnId")?)?);
                self.receipt.message = None;
                self.receipt.text.clear();
                self.ended = false;
            }
            "assistant.message" => {
                if event
                    .data
                    .get("originatingMessageId")
                    .and_then(Value::as_str)
                    != self.receipt.prompt.as_deref()
                {
                    return Ok(false);
                }
                if self.ended
                    || self.receipt.turn.is_none()
                    || event.data.get("turnId").and_then(Value::as_str)
                        != self.receipt.turn.as_deref()
                {
                    return Err(CopilotReason::Protocol);
                }
                match (event.data.get("chunkIndex"), event.data.get("chunkCount")) {
                    (None, None) => (),
                    (Some(i), Some(n)) => match (i.as_u64(), n.as_u64()) {
                        (Some(i), Some(n)) if n > 0 && i < n => {
                            if i != n - 1 {
                                return Ok(false);
                            }
                        }
                        _ => return Err(CopilotReason::Protocol),
                    },
                    _ => return Err(CopilotReason::Protocol),
                }
                let content = event
                    .data
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or(CopilotReason::Protocol)?;
                if content.len() > self.limits.response_text_bytes {
                    return Err(CopilotReason::Limit);
                }
                self.receipt.text = content.to_owned();
                self.receipt.message = Some(bounded_id(text(&event.data, "messageId")?)?);
            }
            "assistant.turn_end" => {
                if self.ended
                    || self.receipt.turn.is_none()
                    || event.data.get("turnId").and_then(Value::as_str)
                        != self.receipt.turn.as_deref()
                {
                    return Err(CopilotReason::Protocol);
                }
                self.ended = true;
            }
            _ => (),
        }
        Ok(false)
    }
}
impl<S: CopilotSink> IncrementalSink for Bridge<'_, '_, S> {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if self.failure.is_some() {
            return Err(SinkFailure::ConsumerFailed);
        }
        let result = async {
            self.check()?;
            if chunk.stream() == Stream::Stderr {
                return Ok(());
            }
            self.stdout = self
                .stdout
                .checked_add(chunk.bytes().len())
                .filter(|n| *n <= self.limits.output_bytes)
                .ok_or(CopilotReason::Limit)?;
            for segment in chunk.bytes().split_inclusive(|b| *b == b'\n') {
                let terminated = segment.last() == Some(&b'\n');
                let content = if terminated {
                    &segment[..segment.len() - 1]
                } else {
                    segment
                };
                let length = self
                    .pending
                    .len()
                    .checked_add(content.len())
                    .filter(|n| *n <= self.limits.wire.frame_bytes)
                    .ok_or(CopilotReason::Limit)?;
                self.pending
                    .try_reserve(length - self.pending.len())
                    .map_err(|_| CopilotReason::Allocation)?;
                self.pending.extend_from_slice(content);
                if terminated {
                    self.frame().await?;
                }
            }
            self.check()
        }
        .await;
        result.map_err(|reason| {
            self.failure = Some(reason);
            SinkFailure::ConsumerFailed
        })
    }
}
/// Await the existing supervisor to completion; never timeout-drop its cleanup owner.
pub async fn run_copilot<S: CopilotSink>(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    host: &ProcessHost,
    request: CopilotRequest,
    limits: CopilotLimits,
    sink: &mut S,
) -> Result<CopilotReceipt, CopilotFailure> {
    let fail = |reason| CopilotFailure {
        receipt: Box::default(),
        reason,
    };
    check(original, admission).map_err(fail)?;
    sink.recheck(original)
        .map_err(|e| fail(CopilotReason::Sink(e.kind())))?;
    original
        .required_deadline()
        .map_err(|e| fail(CopilotReason::Context(e.kind())))?;
    limits
        .wire
        .validate()
        .map_err(|_| fail(CopilotReason::Policy))?;
    let CopilotRequest {
        mut command,
        session,
        model,
        prompt,
    } = request;
    if [
        limits.input_bytes,
        limits.output_bytes,
        limits.prompt_text_bytes,
        limits.response_text_bytes,
    ]
    .into_iter()
    .any(|v| v == 0 || v > isize::MAX as usize)
        || model.is_empty()
        || model.len() > limits.wire.id_bytes
        || model.contains('\0')
        || prompt.is_empty()
        || prompt.len() > limits.prompt_text_bytes
        || prompt.len() > limits.input_bytes
        || prompt.len() > command.limits.stdin_bytes
        || command.stdin.is_some()
    {
        return Err(fail(CopilotReason::Policy));
    }
    let expected = match session {
        CopilotSessionStart::Create => None,
        CopilotSessionStart::Resume(id) if session_id(&id, limits.wire.id_bytes) => Some(id),
        CopilotSessionStart::Resume(_) => return Err(fail(CopilotReason::Policy)),
    };
    command.args.extend([
        "--output-format=json".into(),
        "--no-ask-user".into(),
        "--available-tools=".into(),
        format!("--model={model}").into(),
    ]);
    if let Some(id) = &expected {
        command.args.push(format!("--resume={id}").into());
    }
    command.stdin = Some(Zeroizing::new(prompt.into_bytes()));
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|_| fail(CopilotReason::Admission))?;
    let mut bridge = Bridge {
        original,
        admission,
        sink,
        limits,
        expected,
        pending: Vec::new(),
        stdout: 0,
        receipt: CopilotReceipt::default(),
        ended: false,
        failure: None,
    };
    let terminal =
        graphit_process::run(command, original, Retention::None, &mut bridge, receiver).await;
    let (clean, eof) = match &terminal {
        Ok(o) => (
            o.status.success() && o.cleanup == Cleanup::Complete && o.remaining.is_none(),
            o.pipes.stdout == PipeEnd::Eof && o.pipes.stderr == PipeEnd::Eof,
        ),
        Err(_) => (false, false),
    };
    let mut reason = bridge.failure.or_else(|| {
        original
            .check()
            .err()
            .map(|e| CopilotReason::Context(e.kind()))
    });
    if reason.is_none() && !clean {
        reason = Some(if terminal.is_ok() {
            CopilotReason::NonzeroExit
        } else {
            CopilotReason::Process
        });
    }
    // Install custody before a final-fragment callback can await or fail.
    bridge.receipt.process = Some(terminal);
    if reason.is_none() && eof && !bridge.pending.is_empty() {
        reason = bridge.frame().await.err();
    }
    if reason.is_none() && !eof {
        reason = Some(CopilotReason::Closed);
    }
    if reason.is_none() && bridge.receipt.terminal.is_none() {
        reason = Some(CopilotReason::Terminal);
    }
    if reason.is_none() {
        reason = bridge.check().err();
    }
    bridge.receipt.eligible = reason.is_none();
    match reason {
        Some(reason) => Err(CopilotFailure {
            receipt: Box::new(bridge.receipt),
            reason,
        }),
        None => Ok(bridge.receipt),
    }
}
