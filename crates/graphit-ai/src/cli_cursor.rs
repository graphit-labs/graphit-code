//! Cursor print NDJSON over the caller's real Process admission and Original.
use crate::cli_cursor_wire::validate_cursor_limits;
use crate::cli_cursor_wire::{
    CursorDecodedFrame, CursorEvent, CursorEventData, CursorEventView, CursorWireError,
    CursorWireLimits, decode_cursor_frame,
};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Counts, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use std::{collections::TryReserveError, fmt, future::Future, io, sync::Arc};
#[derive(Clone, Copy)]
pub struct CursorLimits {
    pub wire: CursorWireLimits,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub frames: u64,
    pub response_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorClassification {
    Unknown,
    Structured,
}
#[derive(Clone, Copy, Debug)]
pub enum CursorReason {
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
pub trait CursorSink: Send {
    /// Cooperatively poll, remain cancellation-safe and leave no detached work.
    fn push<'a>(
        &'a mut self,
        event: CursorEventView<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStatus {
    Success,
    Error,
}
pub struct CursorObservedResult {
    pub duration_ms: u64,
    pub duration_api_ms: u64,
    pub request_id: Option<String>,
}
pub struct CursorObservedInit {
    pub session_id: String,
    pub model: String,
}
pub struct CursorReceipt {
    process: Option<Result<Outcome, Failure>>,
    accepted: Counts,
    frames: u64,
    known_events: u64,
    classification: CursorClassification,
    init: Option<CursorObservedInit>,
    observed_result: Option<CursorObservedResult>,
    native_result: Option<String>,
    result: Option<CursorStatus>,
    candidate: Option<String>,
    finalized: bool,
}
impl CursorReceipt {
    fn empty() -> Self {
        Self {
            process: None,
            accepted: Counts::default(),
            frames: 0,
            known_events: 0,
            classification: CursorClassification::Unknown,
            init: None,
            observed_result: None,
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
    pub fn classification(&self) -> CursorClassification {
        self.classification
    }
    pub fn observed_init(&self) -> Option<&CursorObservedInit> {
        self.init.as_ref()
    }
    /// Native durations and identifier only; these values do not imply token usage.
    pub fn observed_result(&self) -> Option<&CursorObservedResult> {
        self.observed_result.as_ref()
    }
    /// Byte-preserved native result; exposed only to the authorized caller.
    pub fn native_result_json(&self) -> Option<&str> {
        self.native_result.as_deref()
    }
    pub fn result(&self) -> Option<CursorStatus> {
        self.result
    }
    pub fn answer(&self) -> Option<&str> {
        if self.finalized {
            self.candidate.as_deref()
        } else {
            None
        }
    }
}
impl fmt::Debug for CursorReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CursorReceipt")
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
    Wire(CursorWireError),
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
        }
    }
}
impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for Cause {}
pub struct CursorFailure {
    reason: CursorReason,
    receipt: CursorReceipt,
    cause: Option<Cause>,
}
impl CursorFailure {
    pub fn reason(&self) -> CursorReason {
        self.reason
    }
    pub fn receipt(&self) -> &CursorReceipt {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut CursorReceipt {
        &mut self.receipt
    }
}
impl fmt::Debug for CursorFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CursorFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for CursorFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cursor subprocess {:?}", self.reason)
    }
}
impl std::error::Error for CursorFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if matches!(self.reason, CursorReason::Process) {
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
    limits: CursorLimits,
    pending: Vec<u8>,
    pending_cr: bool,
    receipt: CursorReceipt,
    failure: Option<CursorReason>,
    cause: Option<Cause>,
}
impl<S: CursorSink> Bridge<'_, S> {
    fn check(&mut self, original: &SnapshotContext) -> Result<(), CursorReason> {
        original.check().map_err(|e| {
            let kind = e.kind();
            self.cause = Some(Cause::Io(e));
            CursorReason::Context(kind)
        })
    }
    fn copy(&mut self, text: &str) -> Result<String, CursorReason> {
        let mut value = String::new();
        value.try_reserve_exact(text.len()).map_err(|e| {
            self.cause = Some(Cause::Allocation(e));
            CursorReason::Allocation
        })?;
        value.push_str(text);
        Ok(value)
    }
    fn observe(
        &mut self,
        event: &CursorEvent<'_>,
        original: &SnapshotContext,
    ) -> Result<(), CursorReason> {
        self.check(original)?;
        match &event.data {
            CursorEventData::Init(e) => {
                if self.receipt.init.is_some() {
                    return Err(CursorReason::State);
                }
                let session_id = self.copy(&e.session_id)?;
                let model = self.copy(&e.model)?;
                self.receipt.init = Some(CursorObservedInit { session_id, model });
            }
            CursorEventData::User(e) | CursorEventData::Assistant(e) => {
                self.session(&e.session_id)?
            }
            CursorEventData::ToolCall(e) => self.session(&e.session_id)?,
            CursorEventData::Result(e) => {
                self.session(&e.session_id)?;
                self.receipt.result = Some(if e.successful() {
                    CursorStatus::Success
                } else {
                    CursorStatus::Error
                });
                let request_id = match &e.request_id {
                    Some(id) => Some(self.copy(id)?),
                    None => None,
                };
                self.receipt.observed_result = Some(CursorObservedResult {
                    duration_ms: e.duration_ms,
                    duration_api_ms: e.duration_api_ms,
                    request_id,
                });
                if e.successful() {
                    if e.result.len() > self.limits.response_bytes {
                        return Err(CursorReason::ResponseLimit);
                    }
                    self.receipt.candidate = Some(self.copy(&e.result)?);
                    self.receipt.native_result = Some(self.copy(event.native.get())?);
                }
            }
        }
        self.check(original)
    }
    fn session(&self, id: &str) -> Result<(), CursorReason> {
        if self
            .receipt
            .init
            .as_ref()
            .is_none_or(|init| init.session_id != id)
        {
            Err(CursorReason::State)
        } else {
            Ok(())
        }
    }
    async fn frame(&mut self, original: &SnapshotContext) -> Result<(), CursorReason> {
        self.check(original)?;
        if self.receipt.frames >= self.limits.frames {
            return Err(CursorReason::FrameLimit);
        }
        // Consume before awaiting: cancellation drops this buffer, with no EOF replay.
        let mut bytes = std::mem::take(&mut self.pending);
        let event = decode_cursor_frame(&bytes, &self.limits.wire, original).map_err(|e| {
            self.cause = Some(Cause::Wire(e));
            CursorReason::Wire
        })?;
        self.receipt.frames += 1;
        // The documented terminal ends the stream, including unknown audited frames.
        if self.receipt.result.is_some() {
            return Err(CursorReason::State);
        }
        if let CursorDecodedFrame::Known(event) = event {
            self.receipt.classification = CursorClassification::Structured;
            self.observe(&event, original)?;
            // The final unterminated frame is delivered after Process has returned;
            // retain original cancellation/deadline while that callback is pending.
            let deadline = original.required_deadline().map_err(|error| {
                let kind = error.kind();
                if self.cause.is_none() {
                    self.cause = Some(Cause::Io(error));
                }
                CursorReason::Context(kind)
            })?;
            tokio::select! {
                biased;
                _ = original.cancellation().cancelled() => self.check(original)?,
                _ = tokio::time::sleep_until(deadline.into()) => self.check(original)?,
                result = self.sink.push(&event, original) => result.map_err(CursorReason::Sink)?,
            }
            self.receipt.known_events += 1;
            if matches!(&event.data, CursorEventData::Result(e) if !e.successful()) {
                return Err(CursorReason::Provider);
            }
        }
        bytes.clear();
        self.pending = bytes;
        self.check(original)
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), CursorReason> {
        let next = self
            .pending
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limits.wire.frame_bytes)
            .ok_or(CursorReason::FrameLimit)?;
        self.pending
            .try_reserve_exact(next - self.pending.len())
            .map_err(|e| {
                self.cause = Some(Cause::Allocation(e));
                CursorReason::Allocation
            })?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    fn segment(&mut self, bytes: &[u8], terminated: bool) -> Result<(), CursorReason> {
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
    ) -> Result<(), CursorReason> {
        self.check(original)?;
        let (count, cap, reason) = match chunk.stream() {
            Stream::Stdout => (
                &mut self.receipt.accepted.stdout,
                self.limits.stdout_bytes,
                CursorReason::StdoutLimit,
            ),
            Stream::Stderr => (
                &mut self.receipt.accepted.stderr,
                self.limits.stderr_bytes,
                CursorReason::StderrLimit,
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
impl<S: CursorSink> IncrementalSink for Bridge<'_, S> {
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
                CursorReason::Sink(e) => e,
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
pub async fn run_cursor<S: CursorSink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    limits: CursorLimits,
    sink: &mut S,
) -> Result<CursorReceipt, CursorFailure> {
    let mut bridge = Bridge {
        sink,
        limits,
        pending: Vec::new(),
        pending_cr: false,
        receipt: CursorReceipt::empty(),
        failure: None,
        cause: None,
    };
    let preflight = bridge.check(original).and_then(|()| {
        validate_cursor_limits(&limits.wire).map_err(|e| {
            bridge.cause = Some(Cause::Wire(e));
            CursorReason::Policy
        })?;
        if limits.stdout_bytes == 0
            || limits.stderr_bytes == 0
            || limits.frames == 0
            || limits.response_bytes == 0
        {
            return Err(CursorReason::Policy);
        }
        Ok(())
    });
    if let Err(reason) = preflight {
        return Err(CursorFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|e| CursorFailure {
            reason: CursorReason::Admission(e),
            receipt: CursorReceipt::empty(),
            cause: None,
        })?;
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    let terminal_reason = match &terminal {
        Err(_) => Some(CursorReason::Process),
        Ok(o) if !o.status.success() => Some(CursorReason::NonzeroExit),
        Ok(_) => None,
    };
    let mut reason = bridge.failure.or(terminal_reason);
    let (pipes, remaining) = match &terminal {
        Ok(o) => (o.pipes, o.remaining.is_some()),
        Err(e) => (e.pipes, e.remaining.is_some()),
    };
    if reason.is_none() && remaining {
        reason = Some(CursorReason::Process);
    }
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
                reason = Some(CursorReason::UnfinishedPipe(stream));
                break;
            }
        }
    }
    if reason.is_none() {
        reason = match bridge.receipt.result {
            None => Some(CursorReason::MissingResult),
            Some(CursorStatus::Error) => Some(CursorReason::Provider),
            Some(CursorStatus::Success) => None,
        };
        if reason.is_none() {
            bridge.receipt.finalized = true;
        }
    }
    if reason.is_some() {
        bridge.receipt.candidate = None;
        bridge.receipt.native_result = None;
    }
    match reason {
        None => Ok(bridge.receipt),
        Some(reason) => Err(CursorFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        }),
    }
}
