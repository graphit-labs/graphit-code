//! Qwen Code JSONL over the caller's real Process admission and Original.
use crate::cli_qwen_wire::validate_qwen_limits;
use crate::cli_qwen_wire::{
    QwenDecodedFrame, QwenEvent, QwenEventData, QwenEventView, QwenWireError, QwenWireLimits,
    decode_qwen_frame,
};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Counts, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use serde::Deserialize;
use std::{collections::TryReserveError, fmt, future::Future, io, sync::Arc};
#[derive(Clone, Copy)]
pub struct QwenLimits {
    pub wire: QwenWireLimits,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub frames: u64,
    pub response_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QwenClassification {
    Unknown,
    Structured,
}
#[derive(Clone, Copy, Debug)]
pub enum QwenReason {
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
pub trait QwenSink: Send {
    /// Cooperatively poll, remain cancellation-safe and leave no detached work.
    fn push<'a>(
        &'a mut self,
        event: QwenEventView<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QwenStatus {
    Success,
    Error,
}
pub use crate::cli_qwen_wire::QwenUsage;
pub struct QwenObservedInit {
    pub session_id: String,
    pub model: String,
}
pub struct QwenReceipt {
    process: Option<Result<Outcome, Failure>>,
    accepted: Counts,
    frames: u64,
    known_events: u64,
    classification: QwenClassification,
    init: Option<QwenObservedInit>,
    usage: Option<QwenUsage>,
    native_result: Option<String>,
    result: Option<QwenStatus>,
    candidate: Option<String>,
    finalized: bool,
}
impl QwenReceipt {
    fn empty() -> Self {
        Self {
            process: None,
            accepted: Counts::default(),
            frames: 0,
            known_events: 0,
            classification: QwenClassification::Unknown,
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
    pub fn classification(&self) -> QwenClassification {
        self.classification
    }
    pub fn observed_init(&self) -> Option<&QwenObservedInit> {
        self.init.as_ref()
    }
    /// Native stats only; None is unavailable. Tool calls do not count native RPCs.
    pub fn usage(&self) -> Option<&QwenUsage> {
        self.usage.as_ref()
    }
    /// Byte-preserved native result; exposed only to the authorized caller.
    pub fn native_result_json(&self) -> Option<&str> {
        self.native_result.as_deref()
    }
    pub fn result(&self) -> Option<QwenStatus> {
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
impl fmt::Debug for QwenReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QwenReceipt")
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
    Wire(QwenWireError),
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
pub struct QwenFailure {
    reason: QwenReason,
    receipt: QwenReceipt,
    cause: Option<Cause>,
}
impl QwenFailure {
    pub fn reason(&self) -> QwenReason {
        self.reason
    }
    pub fn receipt(&self) -> &QwenReceipt {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut QwenReceipt {
        &mut self.receipt
    }
}
impl fmt::Debug for QwenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QwenFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for QwenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Qwen subprocess {:?}", self.reason)
    }
}
impl std::error::Error for QwenFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if matches!(self.reason, QwenReason::Process) {
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
    limits: QwenLimits,
    pending: Vec<u8>,
    pending_cr: bool,
    receipt: QwenReceipt,
    failure: Option<QwenReason>,
    cause: Option<Cause>,
}
impl<S: QwenSink> Bridge<'_, S> {
    fn check(&mut self, original: &SnapshotContext) -> Result<(), QwenReason> {
        original.check().map_err(|e| {
            let kind = e.kind();
            self.cause = Some(Cause::Io(e));
            QwenReason::Context(kind)
        })
    }
    fn copy(&mut self, text: &str) -> Result<String, QwenReason> {
        let mut value = String::new();
        value.try_reserve_exact(text.len()).map_err(|e| {
            self.cause = Some(Cause::Allocation(e));
            QwenReason::Allocation
        })?;
        value.push_str(text);
        Ok(value)
    }
    fn observe(
        &mut self,
        event: &QwenEvent<'_>,
        original: &SnapshotContext,
    ) -> Result<(), QwenReason> {
        self.check(original)?;
        match &event.data {
            QwenEventData::Init(e) => {
                if self.receipt.init.is_some() {
                    return Err(QwenReason::State);
                }
                let session_id = self.copy(&e.session_id)?;
                let model = self.copy(&e.model)?;
                self.receipt.init = Some(QwenObservedInit { session_id, model });
            }
            QwenEventData::Stream(e) => {
                if self
                    .receipt
                    .init
                    .as_ref()
                    .is_none_or(|init| init.session_id != e.session_id)
                {
                    return Err(QwenReason::State);
                }
            }
            QwenEventData::System(e) => {
                if self
                    .receipt
                    .init
                    .as_ref()
                    .is_some_and(|init| init.session_id != e.session_id)
                {
                    return Err(QwenReason::State);
                }
            }
            QwenEventData::Assistant(e) | QwenEventData::User(e) => {
                if self
                    .receipt
                    .init
                    .as_ref()
                    .is_none_or(|init| init.session_id != e.session_id)
                {
                    return Err(QwenReason::State);
                }
            }
            QwenEventData::Result(e) => {
                // A later native error must invalidate an earlier success, without guessing origin.
                if self.receipt.result.is_some() && e.successful() {
                    return Err(QwenReason::State);
                }
                if e.successful()
                    && self
                        .receipt
                        .init
                        .as_ref()
                        .is_none_or(|init| init.session_id != e.session_id)
                {
                    return Err(QwenReason::State);
                }
                self.receipt.result = Some(if e.successful() {
                    QwenStatus::Success
                } else {
                    QwenStatus::Error
                });
                self.receipt.native_result = Some(self.copy(event.native.get())?);
                self.receipt.candidate = None;
                if e.successful()
                    && let Some(text) = &e.result
                {
                    if text.len() > self.limits.response_bytes {
                        return Err(QwenReason::ResponseLimit);
                    }
                    self.receipt.candidate = Some(self.copy(text)?);
                }
                {
                    let raw = e.usage;
                    let usage: QwenUsage = serde_json::from_str(raw.get()).map_err(|e| {
                        self.cause = Some(Cause::Json(e));
                        QwenReason::Wire
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
    ) -> Result<(), QwenReason> {
        use serde::{
            Deserializer,
            de::{self, MapAccess, Visitor},
        };
        #[derive(Deserialize)]
        struct Tokens {
            #[serde(rename = "inputTokens")]
            _input_tokens: u64,
            #[serde(rename = "outputTokens")]
            _output_tokens: u64,
            #[serde(rename = "cacheReadInputTokens")]
            _cache_read_input_tokens: u64,
            #[serde(rename = "cacheCreationInputTokens")]
            _cache_creation_input_tokens: u64,
            #[serde(rename = "webSearchRequests")]
            _web_search_requests: u64,
            #[serde(rename = "contextWindow")]
            _context_window: u64,
        }
        struct Models<'a> {
            original: &'a SnapshotContext,
            limits: QwenLimits,
            reason: &'a mut Option<QwenReason>,
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
                        .map(|e| QwenReason::Context(e.kind()))
                        .or_else(|| {
                            (model.is_empty() || model.len() > self.limits.wire.id_bytes)
                                .then_some(QwenReason::State)
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
            QwenReason::Wire
        })?;
        d.end().map_err(|e| {
            self.cause = Some(Cause::Json(e));
            QwenReason::Wire
        })
    }
    async fn frame(&mut self, original: &SnapshotContext) -> Result<(), QwenReason> {
        self.check(original)?;
        if self.receipt.frames >= self.limits.frames {
            return Err(QwenReason::FrameLimit);
        }
        // Consume before awaiting: cancellation drops this buffer, with no EOF replay.
        let mut bytes = std::mem::take(&mut self.pending);
        let event = decode_qwen_frame(&bytes, &self.limits.wire, original).map_err(|e| {
            self.cause = Some(Cause::Wire(e));
            QwenReason::Wire
        })?;
        self.receipt.frames += 1;
        if let QwenDecodedFrame::Known(event) = event {
            self.receipt.classification = QwenClassification::Structured;
            self.observe(&event, original)?;
            // The final unterminated frame is delivered after Process has returned;
            // retain original cancellation/deadline while that callback is pending.
            let deadline = original.required_deadline().map_err(|error| {
                let kind = error.kind();
                if self.cause.is_none() {
                    self.cause = Some(Cause::Io(error));
                }
                QwenReason::Context(kind)
            })?;
            tokio::select! {
                biased;
                _ = original.cancellation().cancelled() => self.check(original)?,
                _ = tokio::time::sleep_until(deadline.into()) => self.check(original)?,
                result = self.sink.push(&event, original) => result.map_err(QwenReason::Sink)?,
            }
            self.receipt.known_events += 1;
            // Native subagent errors have no origin marker. Fail conservatively; never infer one.
            if matches!(&event.data, QwenEventData::Result(e) if !e.successful()) {
                return Err(QwenReason::Provider);
            }
        }
        bytes.clear();
        self.pending = bytes;
        self.check(original)
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), QwenReason> {
        let next = self
            .pending
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limits.wire.frame_bytes)
            .ok_or(QwenReason::FrameLimit)?;
        self.pending
            .try_reserve_exact(next - self.pending.len())
            .map_err(|e| {
                self.cause = Some(Cause::Allocation(e));
                QwenReason::Allocation
            })?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    fn segment(&mut self, bytes: &[u8], terminated: bool) -> Result<(), QwenReason> {
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
    ) -> Result<(), QwenReason> {
        self.check(original)?;
        let (count, cap, reason) = match chunk.stream() {
            Stream::Stdout => (
                &mut self.receipt.accepted.stdout,
                self.limits.stdout_bytes,
                QwenReason::StdoutLimit,
            ),
            Stream::Stderr => (
                &mut self.receipt.accepted.stderr,
                self.limits.stderr_bytes,
                QwenReason::StderrLimit,
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
impl<S: QwenSink> IncrementalSink for Bridge<'_, S> {
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
                QwenReason::Sink(e) => e,
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
pub async fn run_qwen<S: QwenSink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    limits: QwenLimits,
    sink: &mut S,
) -> Result<QwenReceipt, QwenFailure> {
    let mut bridge = Bridge {
        sink,
        limits,
        pending: Vec::new(),
        pending_cr: false,
        receipt: QwenReceipt::empty(),
        failure: None,
        cause: None,
    };
    let preflight = bridge.check(original).and_then(|()| {
        validate_qwen_limits(&limits.wire).map_err(|e| {
            bridge.cause = Some(Cause::Wire(e));
            QwenReason::Policy
        })?;
        if limits.stdout_bytes == 0
            || limits.stderr_bytes == 0
            || limits.frames == 0
            || limits.response_bytes == 0
        {
            return Err(QwenReason::Policy);
        }
        Ok(())
    });
    if let Err(reason) = preflight {
        return Err(QwenFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|e| QwenFailure {
            reason: QwenReason::Admission(e),
            receipt: QwenReceipt::empty(),
            cause: None,
        })?;
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    let terminal_reason = match &terminal {
        Err(_) => Some(QwenReason::Process),
        Ok(o) if !o.status.success() => Some(QwenReason::NonzeroExit),
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
                reason = Some(QwenReason::UnfinishedPipe(stream));
                break;
            }
        }
    }
    if reason.is_none() {
        reason = match bridge.receipt.result {
            None => Some(QwenReason::MissingResult),
            Some(QwenStatus::Error) => Some(QwenReason::Provider),
            Some(QwenStatus::Success) => None,
        };
        if reason.is_none() {
            bridge.receipt.finalized = true;
        }
    }
    match reason {
        None => Ok(bridge.receipt),
        Some(reason) => Err(QwenFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        }),
    }
}
