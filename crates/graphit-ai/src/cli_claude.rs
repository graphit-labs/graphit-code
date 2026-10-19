//! Claude Code JSONL over the caller's real Process admission and Original.
use crate::cli_claude_wire::validate_claude_limits;
use crate::cli_claude_wire::{
    ClaudeDecodedFrame, ClaudeEvent, ClaudeEventData, ClaudeEventView, ClaudeWireError,
    ClaudeWireLimits, decode_claude_frame,
};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Counts, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use serde::Deserialize;
use std::{collections::TryReserveError, fmt, future::Future, io, sync::Arc};
#[derive(Clone, Copy)]
pub struct ClaudeLimits {
    pub wire: ClaudeWireLimits,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub frames: u64,
    pub response_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeClassification {
    Unknown,
    Structured,
}
#[derive(Clone, Copy, Debug)]
pub enum ClaudeReason {
    Policy,
    Wire,
    State,
    StdoutLimit,
    StderrLimit,
    FrameLimit,
    ResponseLimit,
    Allocation,
    Sink(SinkFailure),
    Context(io::ErrorKind),
    Admission(OperationAdmissionFailure),
    Process,
    NonzeroExit,
    UnfinishedPipe(Stream),
    MissingResult,
    Provider,
}
pub trait ClaudeSink: Send {
    /// Cooperatively poll, remain cancellation-safe and leave no detached work.
    fn push<'a>(
        &'a mut self,
        event: ClaudeEventView<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeStatus {
    Success,
    Error,
}
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ClaudeTokenCounts {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
}
pub struct ClaudeObservedInit {
    pub session_id: String,
    pub model: String,
}
pub struct ClaudeReceipt {
    process: Option<Result<Outcome, Failure>>,
    accepted: Counts,
    frames: u64,
    known_events: u64,
    classification: ClaudeClassification,
    init: Option<ClaudeObservedInit>,
    usage: Option<ClaudeTokenCounts>,
    native_result: Option<String>,
    result: Option<ClaudeStatus>,
    candidate: Option<String>,
    finalized: bool,
}
impl ClaudeReceipt {
    fn empty() -> Self {
        Self {
            process: None,
            accepted: Counts::default(),
            frames: 0,
            known_events: 0,
            classification: ClaudeClassification::Unknown,
            init: None,
            usage: None,
            native_result: None,
            result: None,
            candidate: None,
            finalized: false,
        }
    }
    pub fn process(&self) -> Option<&Result<Outcome, Failure>> {
        self.process.as_ref()
    }
    pub fn process_mut(&mut self) -> Option<&mut Result<Outcome, Failure>> {
        self.process.as_mut()
    }
    pub fn accepted(&self) -> Counts {
        self.accepted
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn known_events(&self) -> u64 {
        self.known_events
    }
    pub fn classification(&self) -> ClaudeClassification {
        self.classification
    }
    pub fn observed_init(&self) -> Option<&ClaudeObservedInit> {
        self.init.as_ref()
    }
    /// Native stats only; None is unavailable. Tool calls do not count native RPCs.
    pub fn usage(&self) -> Option<&ClaudeTokenCounts> {
        self.usage.as_ref()
    }
    /// Byte-preserved native result; exposed only to the authorized caller.
    pub fn native_result_json(&self) -> Option<&str> {
        self.native_result.as_deref()
    }
    pub fn result(&self) -> Option<ClaudeStatus> {
        self.result
    }
    /// Explicit observed candidate, including on failure; never a finalized answer.
    pub fn candidate_text(&self) -> Option<&str> {
        self.candidate.as_deref()
    }
    pub fn answer(&self) -> Option<&str> {
        if self.finalized {
            self.candidate.as_deref()
        } else {
            None
        }
    }
}
impl fmt::Debug for ClaudeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeReceipt")
            .field("process", &self.process)
            .field("accepted", &self.accepted)
            .field("frames", &self.frames)
            .field("known_events", &self.known_events)
            .field("classification", &self.classification)
            .field("result", &self.result)
            .field("finalized", &self.finalized)
            .finish()
    }
}
enum Cause {
    Io(io::Error),
    Allocation(TryReserveError),
    Wire(ClaudeWireError),
    Json(serde_json::Error),
}
impl fmt::Debug for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => f.debug_tuple("Context").field(&e.kind()).finish(),
            Self::Allocation(e) => {
                let _ = e;
                f.write_str("Allocation")
            }
            Self::Wire(e) => fmt::Debug::fmt(e, f),
            Self::Json(e) => f
                .debug_struct("UsageSchema")
                .field("category", &e.classify())
                .field("line", &e.line())
                .field("column", &e.column())
                .finish(),
        }
    }
}
impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for Cause {}
pub struct ClaudeFailure {
    reason: ClaudeReason,
    receipt: ClaudeReceipt,
    cause: Option<Cause>,
}
impl ClaudeFailure {
    pub fn reason(&self) -> ClaudeReason {
        self.reason
    }
    pub fn receipt(&self) -> &ClaudeReceipt {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut ClaudeReceipt {
        &mut self.receipt
    }
}
impl fmt::Debug for ClaudeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for ClaudeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Claude subprocess {:?}", self.reason)
    }
}
impl std::error::Error for ClaudeFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if matches!(self.reason, ClaudeReason::Process) {
            self.receipt
                .process
                .as_ref()
                .and_then(|r| r.as_ref().err())
                .map(|e| e as &(dyn std::error::Error + 'static))
        } else {
            self.cause
                .as_ref()
                .map(|e| e as &(dyn std::error::Error + 'static))
        }
    }
}
struct Bridge<'s, S> {
    sink: &'s mut S,
    limits: ClaudeLimits,
    pending: Vec<u8>,
    pending_cr: bool,
    receipt: ClaudeReceipt,
    failure: Option<ClaudeReason>,
    cause: Option<Cause>,
}
impl<S: ClaudeSink> Bridge<'_, S> {
    fn check(&mut self, original: &SnapshotContext) -> Result<(), ClaudeReason> {
        original.check().map_err(|e| {
            let kind = e.kind();
            self.cause = Some(Cause::Io(e));
            ClaudeReason::Context(kind)
        })
    }
    fn copy(&mut self, text: &str) -> Result<String, ClaudeReason> {
        let mut value = String::new();
        value.try_reserve_exact(text.len()).map_err(|e| {
            self.cause = Some(Cause::Allocation(e));
            ClaudeReason::Allocation
        })?;
        value.push_str(text);
        Ok(value)
    }
    fn observe(
        &mut self,
        event: &ClaudeEvent<'_>,
        original: &SnapshotContext,
    ) -> Result<(), ClaudeReason> {
        self.check(original)?;
        match &event.data {
            ClaudeEventData::Init(e) => {
                if self.receipt.init.is_some() {
                    return Err(ClaudeReason::State);
                }
                let session_id = self.copy(&e.session_id)?;
                let model = self.copy(&e.model)?;
                self.receipt.init = Some(ClaudeObservedInit { session_id, model });
            }
            ClaudeEventData::Stream(e) => {
                if self
                    .receipt
                    .init
                    .as_ref()
                    .is_none_or(|init| init.session_id != e.session_id)
                {
                    return Err(ClaudeReason::State);
                }
            }
            ClaudeEventData::Assistant(e) | ClaudeEventData::User(e) => {
                if let Some(id) = &e.session_id
                    && self
                        .receipt
                        .init
                        .as_ref()
                        .is_none_or(|init| init.session_id != *id)
                {
                    return Err(ClaudeReason::State);
                }
            }
            ClaudeEventData::Result(e) if e.primary() => {
                if self.receipt.result.is_some() {
                    return Err(ClaudeReason::State);
                }
                if e.successful()
                    && self
                        .receipt
                        .init
                        .as_ref()
                        .is_none_or(|init| init.session_id != e.session_id)
                {
                    return Err(ClaudeReason::State);
                }
                self.receipt.result = Some(if e.successful() {
                    ClaudeStatus::Success
                } else {
                    ClaudeStatus::Error
                });
                self.receipt.native_result = Some(self.copy(event.native.get())?);
                if let Some(text) = &e.result {
                    if text.len() > self.limits.response_bytes {
                        return Err(ClaudeReason::ResponseLimit);
                    }
                    self.receipt.candidate = Some(self.copy(text)?);
                }
                if let Some(raw) = e.usage {
                    let usage: ClaudeTokenCounts =
                        serde_json::from_str(raw.get()).map_err(|e| {
                            self.cause = Some(Cause::Json(e));
                            ClaudeReason::Wire
                        })?;
                    self.receipt.usage = Some(usage);
                }
                if let Some(raw) = e.model_usage {
                    self.model_usage(raw, original)?;
                }
            }
            _ => {}
        }
        self.check(original)
    }
    fn model_usage(
        &mut self,
        raw: &serde_json::value::RawValue,
        original: &SnapshotContext,
    ) -> Result<(), ClaudeReason> {
        use serde::{
            Deserializer,
            de::{self, MapAccess, Visitor},
        };
        #[derive(Deserialize)]
        struct Tokens {
            #[serde(rename = "inputTokens")]
            _input_tokens: Option<u64>,
            #[serde(rename = "outputTokens")]
            _output_tokens: Option<u64>,
            #[serde(rename = "cacheReadInputTokens")]
            _cache_read_input_tokens: Option<u64>,
            #[serde(rename = "cacheCreationInputTokens")]
            _cache_creation_input_tokens: Option<u64>,
        }
        struct Models<'a> {
            original: &'a SnapshotContext,
            limits: ClaudeLimits,
            reason: &'a mut Option<ClaudeReason>,
        }
        impl<'de> Visitor<'de> for Models<'_> {
            type Value = ();
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("native model usage")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
                while let Some((model, _counts)) =
                    map.next_entry::<std::borrow::Cow<'de, str>, Tokens>()?
                {
                    let error = self
                        .original
                        .check()
                        .err()
                        .map(|e| ClaudeReason::Context(e.kind()))
                        .or_else(|| {
                            (model.is_empty() || model.len() > self.limits.wire.id_bytes)
                                .then_some(ClaudeReason::State)
                        });
                    if let Some(error) = error {
                        *self.reason = Some(error);
                        return Err(de::Error::custom("model usage"));
                    }
                }
                Ok(())
            }
        }
        let mut reason = None;
        let mut d = serde_json::Deserializer::from_str(raw.get());
        let parsed = d.deserialize_map(Models {
            original,
            limits: self.limits,
            reason: &mut reason,
        });
        if let Some(reason) = reason {
            return Err(reason);
        }
        parsed.map_err(|e| {
            self.cause = Some(Cause::Json(e));
            ClaudeReason::Wire
        })?;
        d.end().map_err(|e| {
            self.cause = Some(Cause::Json(e));
            ClaudeReason::Wire
        })
    }
    async fn frame(&mut self, original: &SnapshotContext) -> Result<(), ClaudeReason> {
        self.check(original)?;
        if self.receipt.frames >= self.limits.frames {
            return Err(ClaudeReason::FrameLimit);
        }
        // Consume before awaiting: cancellation drops this buffer, with no EOF replay.
        let mut bytes = std::mem::take(&mut self.pending);
        let event = decode_claude_frame(&bytes, &self.limits.wire, original).map_err(|e| {
            self.cause = Some(Cause::Wire(e));
            ClaudeReason::Wire
        })?;
        self.receipt.frames += 1;
        if let ClaudeDecodedFrame::Known(event) = event {
            self.receipt.classification = ClaudeClassification::Structured;
            self.observe(&event, original)?;
            // The final unterminated frame is delivered after Process has returned;
            // retain original cancellation/deadline while that callback is pending.
            let deadline = original.required_deadline().map_err(|error| {
                let kind = error.kind();
                if self.cause.is_none() {
                    self.cause = Some(Cause::Io(error));
                }
                ClaudeReason::Context(kind)
            })?;
            tokio::select! {
                biased;
                _ = original.cancellation().cancelled() => self.check(original)?,
                _ = tokio::time::sleep_until(deadline.into()) => self.check(original)?,
                result = self.sink.push(&event, original) => result.map_err(ClaudeReason::Sink)?,
            }
            self.receipt.known_events += 1;
            if matches!(&event.data, ClaudeEventData::Result(e) if e.primary() && !e.successful())
                || matches!(&event.data, ClaudeEventData::Assistant(e) if e.parent_tool_use_id.is_none() && e.error.is_some())
            {
                return Err(ClaudeReason::Provider);
            }
        }
        bytes.clear();
        self.pending = bytes;
        self.check(original)
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), ClaudeReason> {
        let next = self
            .pending
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limits.wire.frame_bytes)
            .ok_or(ClaudeReason::FrameLimit)?;
        self.pending
            .try_reserve_exact(next - self.pending.len())
            .map_err(|e| {
                self.cause = Some(Cause::Allocation(e));
                ClaudeReason::Allocation
            })?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    fn segment(&mut self, bytes: &[u8], terminated: bool) -> Result<(), ClaudeReason> {
        if self.pending_cr && (!bytes.is_empty() || !terminated) {
            if bytes.is_empty() {
                return Ok(());
            }
            self.append(b"\r")?;
        }
        if bytes.is_empty() && terminated {
            self.pending_cr = false;
            return Ok(());
        }
        self.pending_cr = false;
        if let Some(content) = bytes.strip_suffix(b"\r") {
            self.append(content)?;
            self.pending_cr = !terminated;
        } else {
            self.append(bytes)?;
        }
        Ok(())
    }
    async fn deliver(
        &mut self,
        chunk: Chunk<'_>,
        original: &SnapshotContext,
    ) -> Result<(), ClaudeReason> {
        self.check(original)?;
        let (count, cap, reason) = match chunk.stream() {
            Stream::Stdout => (
                &mut self.receipt.accepted.stdout,
                self.limits.stdout_bytes,
                ClaudeReason::StdoutLimit,
            ),
            Stream::Stderr => (
                &mut self.receipt.accepted.stderr,
                self.limits.stderr_bytes,
                ClaudeReason::StderrLimit,
            ),
        };
        *count = count
            .checked_add(chunk.bytes().len() as u64)
            .filter(|&n| n <= cap as u64)
            .ok_or(reason)?;
        if chunk.stream() == Stream::Stderr {
            return self.check(original);
        }
        let mut rest = chunk.bytes();
        while let Some(lf) = rest.iter().position(|&b| b == b'\n') {
            self.segment(&rest[..lf], true)?;
            self.frame(original).await?;
            rest = &rest[lf + 1..];
        }
        self.segment(rest, false)?;
        self.check(original)
    }
}
impl<S: ClaudeSink> IncrementalSink for Bridge<'_, S> {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if self.failure.is_some() {
            return Err(SinkFailure::ConsumerFailed);
        }
        self.deliver(chunk, original).await.map_err(|e| {
            self.failure = Some(e);
            match e {
                ClaudeReason::Sink(e) => e,
                _ => SinkFailure::ConsumerFailed,
            }
        })
    }
}
/// Await actual Process cleanup; success requires a native result and true pipe EOF.
#[expect(
    clippy::result_large_err,
    reason = "Retain real terminal and remaining owner inline."
)]
pub async fn run_claude<S: ClaudeSink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    limits: ClaudeLimits,
    sink: &mut S,
) -> Result<ClaudeReceipt, ClaudeFailure> {
    let mut bridge = Bridge {
        sink,
        limits,
        pending: Vec::new(),
        pending_cr: false,
        receipt: ClaudeReceipt::empty(),
        failure: None,
        cause: None,
    };
    let preflight = bridge.check(original).and_then(|()| {
        validate_claude_limits(&limits.wire).map_err(|e| {
            bridge.cause = Some(Cause::Wire(e));
            ClaudeReason::Policy
        })?;
        if limits.stdout_bytes == 0
            || limits.stderr_bytes == 0
            || limits.frames == 0
            || limits.response_bytes == 0
        {
            return Err(ClaudeReason::Policy);
        }
        Ok(())
    });
    if let Err(reason) = preflight {
        return Err(ClaudeFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|e| ClaudeFailure {
            reason: ClaudeReason::Admission(e),
            receipt: ClaudeReceipt::empty(),
            cause: None,
        })?;
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    let terminal_reason = match &terminal {
        Err(_) => Some(ClaudeReason::Process),
        Ok(o) if !o.status.success() => Some(ClaudeReason::NonzeroExit),
        Ok(_) => None,
    };
    let mut reason = bridge.failure.or(terminal_reason);
    let (pipes, remaining) = match &terminal {
        Ok(o) => (o.pipes, o.remaining.is_some()),
        Err(e) => (e.pipes, e.remaining.is_some()),
    };
    // Custody first. Never await an EOF callback while a failed terminal has Remaining.
    bridge.receipt.process = Some(terminal);
    if reason.is_none() && !remaining && pipes.stdout == PipeEnd::Eof {
        let end = if bridge.pending_cr {
            bridge.pending_cr = false;
            bridge.append(b"\r")
        } else {
            Ok(())
        };
        let end = match end {
            Ok(()) if !bridge.pending.is_empty() => bridge.frame(original).await,
            other => other,
        };
        if reason.is_none() {
            reason = end.err();
        }
    }
    if reason.is_none() {
        reason = bridge.check(original).err();
    }
    if reason.is_none() {
        for (stream, end) in [
            (Stream::Stdout, pipes.stdout),
            (Stream::Stderr, pipes.stderr),
        ] {
            if end != PipeEnd::Eof {
                reason = Some(ClaudeReason::UnfinishedPipe(stream));
                break;
            }
        }
    }
    if reason.is_none() {
        reason = match bridge.receipt.result {
            None => Some(ClaudeReason::MissingResult),
            Some(ClaudeStatus::Error) => Some(ClaudeReason::Provider),
            Some(ClaudeStatus::Success) => None,
        };
        if reason.is_none() {
            bridge.receipt.finalized = true;
        }
    }
    match reason {
        None => Ok(bridge.receipt),
        Some(reason) => Err(ClaudeFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        }),
    }
}
