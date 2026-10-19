//! Grok native streaming-json over the caller-owned Process supervisor.
use crate::cli_grok_wire::validate_grok_limits;
use crate::cli_grok_wire::{
    GrokEvent, GrokEventData, GrokEventView, GrokWireError, GrokWireLimits, decode_grok_frame,
};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Counts, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use std::{collections::TryReserveError, fmt, future::Future, io, sync::Arc};
#[derive(Clone, Copy)]
pub struct GrokLimits {
    pub wire: GrokWireLimits,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub frames: u64,
    pub response_bytes: usize,
}
#[derive(Clone, Copy, Debug)]
pub enum GrokReason {
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
pub trait GrokSink: Send {
    /// Cooperatively poll, remain cancellation-safe and leave no detached work.
    fn push<'a>(
        &'a mut self,
        event: GrokEventView<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrokStatus {
    Success,
    Error,
}
pub struct GrokReceipt {
    process: Option<Result<Outcome, Failure>>,
    accepted: Counts,
    frames: u64,
    known_events: u64,
    native_result: Option<String>,
    result: Option<GrokStatus>,
    candidate: Option<String>,
    finalized: bool,
}
impl GrokReceipt {
    fn empty() -> Self {
        Self {
            process: None,
            accepted: Counts::default(),
            frames: 0,
            known_events: 0,
            native_result: None,
            result: None,
            candidate: Some(String::new()),
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
    /// Byte-preserved native result; exposed only to the authorized caller.
    pub fn native_result_json(&self) -> Option<&str> {
        self.native_result.as_deref()
    }
    pub fn answer(&self) -> Option<&str> {
        if self.finalized {
            self.candidate.as_deref()
        } else {
            None
        }
    }
}
impl fmt::Debug for GrokReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrokReceipt")
            .field("process", &self.process)
            .field("accepted", &self.accepted)
            .field("frames", &self.frames)
            .field("known_events", &self.known_events)
            .field("result", &self.result)
            .field("finalized", &self.finalized)
            .finish()
    }
}
enum Cause {
    Io(io::Error),
    Allocation(TryReserveError),
    Wire(GrokWireError),
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
pub struct GrokFailure {
    reason: GrokReason,
    receipt: GrokReceipt,
    cause: Option<Cause>,
}
impl GrokFailure {
    pub fn reason(&self) -> GrokReason {
        self.reason
    }
    pub fn receipt(&self) -> &GrokReceipt {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut GrokReceipt {
        &mut self.receipt
    }
}
impl fmt::Debug for GrokFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrokFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for GrokFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Grok subprocess {:?}", self.reason)
    }
}
impl std::error::Error for GrokFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if matches!(self.reason, GrokReason::Process) {
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
    limits: GrokLimits,
    pending: Vec<u8>,
    pending_cr: bool,
    receipt: GrokReceipt,
    failure: Option<GrokReason>,
    cause: Option<Cause>,
}
impl<S: GrokSink> Bridge<'_, S> {
    fn check(&mut self, original: &SnapshotContext) -> Result<(), GrokReason> {
        original.check().map_err(|e| {
            let kind = e.kind();
            self.cause = Some(Cause::Io(e));
            GrokReason::Context(kind)
        })
    }
    fn copy(&mut self, text: &str) -> Result<String, GrokReason> {
        let mut value = String::new();
        value.try_reserve_exact(text.len()).map_err(|e| {
            self.cause = Some(Cause::Allocation(e));
            GrokReason::Allocation
        })?;
        value.push_str(text);
        Ok(value)
    }
    fn observe(
        &mut self,
        event: &GrokEvent<'_>,
        original: &SnapshotContext,
    ) -> Result<(), GrokReason> {
        self.check(original)?;
        match &event.data {
            GrokEventData::Text(text) => {
                let candidate = self.receipt.candidate.as_mut().expect("active candidate");
                candidate
                    .len()
                    .checked_add(text.data.len())
                    .filter(|&length| length <= self.limits.response_bytes)
                    .ok_or(GrokReason::ResponseLimit)?;
                candidate.try_reserve(text.data.len()).map_err(|error| {
                    self.cause = Some(Cause::Allocation(error));
                    GrokReason::Allocation
                })?;
                candidate.push_str(&text.data);
            }
            GrokEventData::End(end) => {
                let success = end.stop_reason == "end_turn";
                self.receipt.result = Some(if success {
                    GrokStatus::Success
                } else {
                    GrokStatus::Error
                });
                if success {
                    self.receipt.native_result = Some(self.copy(event.native.get())?);
                }
            }
            GrokEventData::Error(_) | GrokEventData::MaxTurnsReached => {
                self.receipt.result = Some(GrokStatus::Error);
            }
            GrokEventData::Progress => {}
        }
        self.check(original)
    }
    async fn frame(&mut self, original: &SnapshotContext) -> Result<(), GrokReason> {
        self.check(original)?;
        if self.receipt.frames >= self.limits.frames {
            return Err(GrokReason::FrameLimit);
        }
        // Consume before awaiting: cancellation drops this buffer, with no EOF replay.
        let mut bytes = std::mem::take(&mut self.pending);
        let event = decode_grok_frame(&bytes, &self.limits.wire, original).map_err(|e| {
            self.cause = Some(Cause::Wire(e));
            GrokReason::Wire
        })?;
        self.receipt.frames += 1;
        // The documented terminal ends the stream, including unknown audited frames.
        if self.receipt.result.is_some() {
            return Err(GrokReason::State);
        }
        if let Some(event) = event {
            self.observe(&event, original)?;
            // The final unterminated frame is delivered after Process has returned;
            // retain original cancellation/deadline while that callback is pending.
            let deadline = original.required_deadline().map_err(|error| {
                let kind = error.kind();
                if self.cause.is_none() {
                    self.cause = Some(Cause::Io(error));
                }
                GrokReason::Context(kind)
            })?;
            tokio::select! {
                biased;
                _ = original.cancellation().cancelled() => self.check(original)?,
                _ = tokio::time::sleep_until(deadline.into()) => self.check(original)?,
                result = self.sink.push(&event, original) => result.map_err(GrokReason::Sink)?,
            }
            self.receipt.known_events += 1;
            if self.receipt.result == Some(GrokStatus::Error) {
                return Err(GrokReason::Provider);
            }
        }
        bytes.clear();
        self.pending = bytes;
        self.check(original)
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), GrokReason> {
        let next = self
            .pending
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limits.wire.frame_bytes)
            .ok_or(GrokReason::FrameLimit)?;
        self.pending
            .try_reserve_exact(next - self.pending.len())
            .map_err(|e| {
                self.cause = Some(Cause::Allocation(e));
                GrokReason::Allocation
            })?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    fn segment(&mut self, bytes: &[u8], terminated: bool) -> Result<(), GrokReason> {
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
    ) -> Result<(), GrokReason> {
        self.check(original)?;
        let (count, cap, reason) = match chunk.stream() {
            Stream::Stdout => (
                &mut self.receipt.accepted.stdout,
                self.limits.stdout_bytes,
                GrokReason::StdoutLimit,
            ),
            Stream::Stderr => (
                &mut self.receipt.accepted.stderr,
                self.limits.stderr_bytes,
                GrokReason::StderrLimit,
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
impl<S: GrokSink> IncrementalSink for Bridge<'_, S> {
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
                GrokReason::Sink(e) => e,
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
pub async fn run_grok<S: GrokSink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    limits: GrokLimits,
    sink: &mut S,
) -> Result<GrokReceipt, GrokFailure> {
    let mut bridge = Bridge {
        sink,
        limits,
        pending: Vec::new(),
        pending_cr: false,
        receipt: GrokReceipt::empty(),
        failure: None,
        cause: None,
    };
    let preflight = bridge.check(original).and_then(|()| {
        validate_grok_limits(&limits.wire).map_err(|e| {
            bridge.cause = Some(Cause::Wire(e));
            GrokReason::Policy
        })?;
        if limits.stdout_bytes == 0
            || limits.stderr_bytes == 0
            || limits.frames == 0
            || limits.response_bytes == 0
        {
            return Err(GrokReason::Policy);
        }
        Ok(())
    });
    if let Err(reason) = preflight {
        return Err(GrokFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|e| GrokFailure {
            reason: GrokReason::Admission(e),
            receipt: GrokReceipt::empty(),
            cause: None,
        })?;
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    let terminal_reason = match &terminal {
        Err(_) => Some(GrokReason::Process),
        Ok(o) if !o.status.success() => Some(GrokReason::NonzeroExit),
        Ok(_) => None,
    };
    let mut reason = bridge.failure.or(terminal_reason);
    let (pipes, remaining) = match &terminal {
        Ok(o) => (o.pipes, o.remaining.is_some()),
        Err(e) => (e.pipes, e.remaining.is_some()),
    };
    if reason.is_none() && remaining {
        reason = Some(GrokReason::Process);
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
                reason = Some(GrokReason::UnfinishedPipe(stream));
                break;
            }
        }
    }
    if reason.is_none() {
        reason = match bridge.receipt.result {
            None => Some(GrokReason::MissingResult),
            Some(GrokStatus::Error) => Some(GrokReason::Provider),
            Some(GrokStatus::Success) => None,
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
        Some(reason) => Err(GrokFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        }),
    }
}
