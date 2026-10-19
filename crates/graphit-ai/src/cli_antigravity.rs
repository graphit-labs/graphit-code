//! One-shot native Antigravity stdin/NDJSON over the caller-owned Process supervisor.
use crate::cli_antigravity_wire::validate_antigravity_limits;
use crate::cli_antigravity_wire::{
    AntigravityEvent, AntigravityEventData, AntigravityEventView, AntigravityWireError,
    AntigravityWireLimits, decode_antigravity_frame,
};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Counts, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use std::{collections::TryReserveError, fmt, future::Future, io, sync::Arc};
#[derive(Clone, Copy)]
pub struct AntigravityLimits {
    pub wire: AntigravityWireLimits,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub frames: u64,
    pub response_bytes: usize,
}
#[derive(Clone, Copy, Debug)]
pub enum AntigravityReason {
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
pub trait AntigravitySink: Send {
    /// Cooperatively poll, remain cancellation-safe and leave no detached work.
    fn push<'a>(
        &'a mut self,
        event: AntigravityEventView<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntigravityStatus {
    Success,
    Error,
}
pub struct AntigravityObservedInit {
    pub session_id: String,
    pub model: Option<String>,
}
pub struct AntigravityReceipt {
    process: Option<Result<Outcome, Failure>>,
    accepted: Counts,
    frames: u64,
    known_events: u64,
    init: Option<AntigravityObservedInit>,
    native_result: Option<String>,
    result: Option<AntigravityStatus>,
    candidate: Option<String>,
    finalized: bool,
}
impl AntigravityReceipt {
    fn empty() -> Self {
        Self {
            process: None,
            accepted: Counts::default(),
            frames: 0,
            known_events: 0,
            init: None,
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
    pub fn observed_init(&self) -> Option<&AntigravityObservedInit> {
        self.init.as_ref()
    }
    /// Byte-preserved native result; exposed only to the authorized caller.
    pub fn native_result_json(&self) -> Option<&str> {
        self.native_result.as_deref()
    }
    pub fn result(&self) -> Option<AntigravityStatus> {
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
impl fmt::Debug for AntigravityReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AntigravityReceipt")
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
    Wire(AntigravityWireError),
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
pub struct AntigravityFailure {
    reason: AntigravityReason,
    receipt: AntigravityReceipt,
    cause: Option<Cause>,
}
impl AntigravityFailure {
    pub fn reason(&self) -> AntigravityReason {
        self.reason
    }
    pub fn receipt(&self) -> &AntigravityReceipt {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut AntigravityReceipt {
        &mut self.receipt
    }
}
impl fmt::Debug for AntigravityFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AntigravityFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for AntigravityFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Antigravity subprocess {:?}", self.reason)
    }
}
impl std::error::Error for AntigravityFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if matches!(self.reason, AntigravityReason::Process) {
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
    limits: AntigravityLimits,
    pending: Vec<u8>,
    pending_cr: bool,
    receipt: AntigravityReceipt,
    failure: Option<AntigravityReason>,
    cause: Option<Cause>,
}
impl<S: AntigravitySink> Bridge<'_, S> {
    fn check(&mut self, original: &SnapshotContext) -> Result<(), AntigravityReason> {
        original.check().map_err(|e| {
            let kind = e.kind();
            self.cause = Some(Cause::Io(e));
            AntigravityReason::Context(kind)
        })
    }
    fn copy(&mut self, text: &str) -> Result<String, AntigravityReason> {
        let mut value = String::new();
        value.try_reserve_exact(text.len()).map_err(|e| {
            self.cause = Some(Cause::Allocation(e));
            AntigravityReason::Allocation
        })?;
        value.push_str(text);
        Ok(value)
    }
    fn observe(
        &mut self,
        event: &AntigravityEvent<'_>,
        original: &SnapshotContext,
    ) -> Result<(), AntigravityReason> {
        self.check(original)?;
        match &event.data {
            AntigravityEventData::Init(init) => {
                if self.receipt.init.is_some() {
                    return Err(AntigravityReason::State);
                }
                let session_id = self.copy(&init.conversation_id)?;
                let model = match &init.init.model {
                    Some(model) => Some(self.copy(model)?),
                    None => None,
                };
                self.receipt.init = Some(AntigravityObservedInit { session_id, model });
            }
            AntigravityEventData::Step(step) => self.session(&step.conversation_id)?,
            AntigravityEventData::Result(result) => {
                if result.successful() || self.receipt.init.is_some() {
                    self.session(&result.conversation_id)?;
                }
                self.receipt.result = Some(if result.successful() {
                    AntigravityStatus::Success
                } else {
                    AntigravityStatus::Error
                });
                if result.successful() {
                    let response = result
                        .response
                        .as_deref()
                        .expect("validated native success");
                    if response.len() > self.limits.response_bytes {
                        return Err(AntigravityReason::ResponseLimit);
                    }
                    self.receipt.candidate = Some(self.copy(response)?);
                    self.receipt.native_result = Some(self.copy(event.native.get())?);
                }
            }
        }
        self.check(original)
    }
    fn session(&self, id: &str) -> Result<(), AntigravityReason> {
        if self
            .receipt
            .init
            .as_ref()
            .is_none_or(|init| init.session_id != id)
        {
            Err(AntigravityReason::State)
        } else {
            Ok(())
        }
    }
    async fn frame(&mut self, original: &SnapshotContext) -> Result<(), AntigravityReason> {
        self.check(original)?;
        if self.receipt.frames >= self.limits.frames {
            return Err(AntigravityReason::FrameLimit);
        }
        // Consume before awaiting: cancellation drops this buffer, with no EOF replay.
        let mut bytes = std::mem::take(&mut self.pending);
        let event = decode_antigravity_frame(&bytes, &self.limits.wire, original).map_err(|e| {
            self.cause = Some(Cause::Wire(e));
            AntigravityReason::Wire
        })?;
        self.receipt.frames += 1;
        // The documented terminal ends the stream, including unknown audited frames.
        if self.receipt.result.is_some() {
            return Err(AntigravityReason::State);
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
                AntigravityReason::Context(kind)
            })?;
            tokio::select! {
                biased;
                _ = original.cancellation().cancelled() => self.check(original)?,
                _ = tokio::time::sleep_until(deadline.into()) => self.check(original)?,
                result = self.sink.push(&event, original) => result.map_err(AntigravityReason::Sink)?,
            }
            self.receipt.known_events += 1;
            if matches!(&event.data, AntigravityEventData::Result(e) if !e.successful()) {
                return Err(AntigravityReason::Provider);
            }
        }
        bytes.clear();
        self.pending = bytes;
        self.check(original)
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), AntigravityReason> {
        let next = self
            .pending
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limits.wire.frame_bytes)
            .ok_or(AntigravityReason::FrameLimit)?;
        self.pending
            .try_reserve_exact(next - self.pending.len())
            .map_err(|e| {
                self.cause = Some(Cause::Allocation(e));
                AntigravityReason::Allocation
            })?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    fn segment(&mut self, bytes: &[u8], terminated: bool) -> Result<(), AntigravityReason> {
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
    ) -> Result<(), AntigravityReason> {
        self.check(original)?;
        let (count, cap, reason) = match chunk.stream() {
            Stream::Stdout => (
                &mut self.receipt.accepted.stdout,
                self.limits.stdout_bytes,
                AntigravityReason::StdoutLimit,
            ),
            Stream::Stderr => (
                &mut self.receipt.accepted.stderr,
                self.limits.stderr_bytes,
                AntigravityReason::StderrLimit,
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
impl<S: AntigravitySink> IncrementalSink for Bridge<'_, S> {
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
                AntigravityReason::Sink(e) => e,
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
pub async fn run_antigravity<S: AntigravitySink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    limits: AntigravityLimits,
    sink: &mut S,
) -> Result<AntigravityReceipt, AntigravityFailure> {
    let mut bridge = Bridge {
        sink,
        limits,
        pending: Vec::new(),
        pending_cr: false,
        receipt: AntigravityReceipt::empty(),
        failure: None,
        cause: None,
    };
    let preflight = bridge.check(original).and_then(|()| {
        validate_antigravity_limits(&limits.wire).map_err(|e| {
            bridge.cause = Some(Cause::Wire(e));
            AntigravityReason::Policy
        })?;
        if limits.stdout_bytes == 0
            || limits.stderr_bytes == 0
            || limits.frames == 0
            || limits.response_bytes == 0
        {
            return Err(AntigravityReason::Policy);
        }
        Ok(())
    });
    if let Err(reason) = preflight {
        return Err(AntigravityFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|e| AntigravityFailure {
            reason: AntigravityReason::Admission(e),
            receipt: AntigravityReceipt::empty(),
            cause: None,
        })?;
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    let terminal_reason = match &terminal {
        Err(_) => Some(AntigravityReason::Process),
        Ok(o) if !o.status.success() => Some(AntigravityReason::NonzeroExit),
        Ok(_) => None,
    };
    let mut reason = bridge.failure.or(terminal_reason);
    let (pipes, remaining) = match &terminal {
        Ok(o) => (o.pipes, o.remaining.is_some()),
        Err(e) => (e.pipes, e.remaining.is_some()),
    };
    if reason.is_none() && remaining {
        reason = Some(AntigravityReason::Process);
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
                reason = Some(AntigravityReason::UnfinishedPipe(stream));
                break;
            }
        }
    }
    if reason.is_none() {
        reason = match bridge.receipt.result {
            None => Some(AntigravityReason::MissingResult),
            Some(AntigravityStatus::Error) => Some(AntigravityReason::Provider),
            Some(AntigravityStatus::Success) => None,
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
        Some(reason) => Err(AntigravityFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        }),
    }
}
