//! Kimi Print JSONL for managed Role execution, over the existing Process owner.
//! Native assistant output can include hooks; it is never a documentary answer.
pub use crate::cli_codex_wire::CodexWireLimits as KimiPrintLimits;
use crate::cli_codex_wire::{CodexWireError, audit_json_frame};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Cleanup, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{borrow::Cow, collections::TryReserveError, fmt, future::Future, io, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub enum KimiPrintReason {
    Policy,
    Wire,
    Schema,
    FrameLimit,
    Allocation,
    Sink(SinkFailure),
    Context(io::ErrorKind),
    Recheck(io::ErrorKind),
    Admission(OperationAdmissionFailure),
    Process,
    NonzeroExit,
    Cleanup,
    UnfinishedPipe(Stream),
}

/// A bounded borrowed native message, excluded from diagnostics.
pub enum KimiPrintEventData<'a> {
    Assistant {
        content: Option<Cow<'a, str>>,
        tool_calls: Option<&'a RawValue>,
    },
    Tool {
        tool_call_id: Cow<'a, str>,
        content: Cow<'a, str>,
    },
    Meta {
        kind: Cow<'a, str>,
    },
    Unknown,
}
pub struct KimiPrintEvent<'a> {
    pub native: &'a RawValue,
    pub data: KimiPrintEventData<'a>,
}
pub trait KimiPrintSink: Send {
    /// Cooperatively poll; be cancellation-safe on drop and leave no detached work.
    fn push<'a>(
        &'a mut self,
        event: KimiPrintEvent<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}

pub struct KimiPrintReceipt {
    process: Option<Result<Outcome, Failure>>,
}
impl KimiPrintReceipt {
    fn empty() -> Self {
        Self { process: None }
    }
    pub fn process(&self) -> Option<&Result<Outcome, Failure>> {
        self.process.as_ref()
    }
    /// Keeps the same real Remaining owner available for caller-owned retirement.
    pub fn process_mut(&mut self) -> Option<&mut Result<Outcome, Failure>> {
        self.process.as_mut()
    }
}
impl fmt::Debug for KimiPrintReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KimiPrintReceipt")
            .field("process", &self.process)
            .finish()
    }
}
enum Cause {
    Context(io::Error),
    Recheck(io::Error),
    Allocation(TryReserveError),
    Wire(CodexWireError),
    Json(serde_json::Error),
}
impl fmt::Debug for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(e) => f.debug_tuple("Context").field(&e.kind()).finish(),
            Self::Recheck(e) => f.debug_tuple("Recheck").field(&e.kind()).finish(),
            Self::Allocation(e) => {
                let _ = e;
                f.write_str("Allocation")
            }
            Self::Wire(e) => fmt::Debug::fmt(e, f),
            Self::Json(e) => f.debug_tuple("Schema").field(&e.classify()).finish(),
        }
    }
}
impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for Cause {}
pub struct KimiPrintFailure {
    reason: KimiPrintReason,
    receipt: KimiPrintReceipt,
    cause: Option<Cause>,
}
impl KimiPrintFailure {
    pub fn reason(&self) -> KimiPrintReason {
        self.reason
    }
    pub fn receipt(&self) -> &KimiPrintReceipt {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut KimiPrintReceipt {
        &mut self.receipt
    }
}
impl fmt::Debug for KimiPrintFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KimiPrintFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for KimiPrintFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Kimi Print {:?}", self.reason)
    }
}
impl std::error::Error for KimiPrintFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.reason {
            KimiPrintReason::Process => self
                .receipt
                .process
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .map(|error| error as &(dyn std::error::Error + 'static)),
            KimiPrintReason::Policy
            | KimiPrintReason::Wire
            | KimiPrintReason::Schema
            | KimiPrintReason::Allocation
            | KimiPrintReason::Context(_)
            | KimiPrintReason::Recheck(_) => self
                .cause
                .as_ref()
                .map(|cause| cause as &(dyn std::error::Error + 'static)),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(borrow)]
    role: Cow<'a, str>,
}
#[derive(Deserialize)]
struct Assistant<'a> {
    #[serde(borrow, default)]
    content: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    tool_calls: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Tool<'a> {
    #[serde(borrow)]
    tool_call_id: Cow<'a, str>,
    #[serde(borrow)]
    content: Cow<'a, str>,
}
#[derive(Deserialize)]
struct Meta<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
}
#[derive(Deserialize)]
struct ToolCall<'a> {
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
    #[serde(borrow)]
    id: Cow<'a, str>,
    #[serde(borrow)]
    function: ToolFunction<'a>,
}
#[derive(Deserialize)]
struct ToolFunction<'a> {
    #[serde(borrow)]
    name: Cow<'a, str>,
    #[serde(borrow)]
    #[serde(rename = "arguments")]
    _arguments: Cow<'a, str>,
}

struct Bridge<'a, S> {
    recheck: &'a (dyn Fn() -> io::Result<()> + Sync),
    sink: &'a mut S,
    limits: KimiPrintLimits,
    pending: Vec<u8>,
    receipt: KimiPrintReceipt,
    failure: Option<KimiPrintReason>,
    cause: Option<Cause>,
}
impl<S: KimiPrintSink> Bridge<'_, S> {
    fn check(&mut self, original: &SnapshotContext) -> Result<(), KimiPrintReason> {
        original.check().map_err(|e| {
            let kind = e.kind();
            if self.cause.is_none() {
                self.cause = Some(Cause::Context(e));
            }
            KimiPrintReason::Context(kind)
        })?;
        (self.recheck)().map_err(|e| {
            let kind = e.kind();
            if self.cause.is_none() {
                self.cause = Some(Cause::Recheck(e));
            }
            KimiPrintReason::Recheck(kind)
        })
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), KimiPrintReason> {
        let next = self
            .pending
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limits.frame_bytes)
            .ok_or(KimiPrintReason::FrameLimit)?;
        self.pending
            .try_reserve(next - self.pending.len())
            .map_err(|e| {
                self.cause = Some(Cause::Allocation(e));
                KimiPrintReason::Allocation
            })?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    async fn frame(&mut self, original: &SnapshotContext) -> Result<(), KimiPrintReason> {
        self.check(original)?;
        // A CR is legal JSON whitespace; preserve bytes without a separate CR buffer.
        if self
            .pending
            .iter()
            .all(|&byte| matches!(byte, b' ' | b'\t' | b'\r'))
        {
            self.pending.clear();
            return Ok(());
        }
        // Move the frame before awaiting so a dropped callback cannot replay it at EOF.
        let mut bytes = std::mem::take(&mut self.pending);
        audit_json_frame(&bytes, &self.limits, original).map_err(|e| {
            self.cause = Some(Cause::Wire(e));
            KimiPrintReason::Wire
        })?;
        let event = decode(&bytes, self.limits.id_bytes).map_err(|e| {
            self.cause = Some(Cause::Json(e));
            KimiPrintReason::Schema
        })?;
        let deadline = original.required_deadline().map_err(|e| {
            let kind = e.kind();
            self.cause = Some(Cause::Context(e));
            KimiPrintReason::Context(kind)
        })?;
        tokio::select! {
            biased;
            _ = original.cancellation().cancelled() => {
                self.check(original)?;
            },
            _ = tokio::time::sleep_until(deadline.into()) => {
                self.check(original)?;
            },
            result = self.sink.push(event, original) => result.map_err(KimiPrintReason::Sink)?,
        }
        bytes.clear();
        self.pending = bytes;
        self.check(original)
    }
    async fn deliver(
        &mut self,
        chunk: Chunk<'_>,
        original: &SnapshotContext,
    ) -> Result<(), KimiPrintReason> {
        self.check(original)?;
        if chunk.stream() == Stream::Stderr {
            return self.check(original);
        }
        let mut bytes = chunk.bytes();
        while let Some(lf) = bytes.iter().position(|&b| b == b'\n') {
            self.append(&bytes[..lf])?;
            self.frame(original).await?;
            bytes = &bytes[lf + 1..];
        }
        self.append(bytes)?;
        self.check(original)
    }
}
impl<S: KimiPrintSink> IncrementalSink for Bridge<'_, S> {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if self.failure.is_some() {
            return Err(SinkFailure::ConsumerFailed);
        }
        self.deliver(chunk, original).await.map_err(|reason| {
            self.failure = Some(reason);
            match reason {
                KimiPrintReason::Sink(e) => e,
                _ => SinkFailure::ConsumerFailed,
            }
        })
    }
}

fn schema_error() -> serde_json::Error {
    <serde_json::Error as serde::de::Error>::custom("invalid Kimi Print message")
}
fn decode(bytes: &[u8], id_bytes: usize) -> Result<KimiPrintEvent<'_>, serde_json::Error> {
    let native: &RawValue = serde_json::from_slice(bytes)?;
    let envelope: Envelope<'_> = serde_json::from_slice(bytes)?;
    let identifier = |value: &str| !value.is_empty() && value.len() <= id_bytes;
    if !identifier(&envelope.role) {
        return Err(schema_error());
    }
    let data = match envelope.role.as_ref() {
        "assistant" => {
            let value: Assistant<'_> = serde_json::from_slice(bytes)?;
            if let Some(raw) = value.tool_calls {
                let calls: Vec<ToolCall<'_>> = serde_json::from_str(raw.get())?;
                if calls.is_empty()
                    || calls.iter().any(|call| {
                        call.kind != "function"
                            || !identifier(&call.id)
                            || !identifier(&call.function.name)
                    })
                {
                    return Err(schema_error());
                }
            }
            if value.content.is_none() && value.tool_calls.is_none() {
                return Err(schema_error());
            }
            KimiPrintEventData::Assistant {
                content: value.content,
                tool_calls: value.tool_calls,
            }
        }
        "tool" => {
            let value: Tool<'_> = serde_json::from_slice(bytes)?;
            if !identifier(&value.tool_call_id) {
                return Err(schema_error());
            }
            KimiPrintEventData::Tool {
                tool_call_id: value.tool_call_id,
                content: value.content,
            }
        }
        "meta" => {
            let value: Meta<'_> = serde_json::from_slice(bytes)?;
            if !identifier(&value.kind) {
                return Err(schema_error());
            }
            KimiPrintEventData::Meta { kind: value.kind }
        }
        _ => KimiPrintEventData::Unknown,
    };
    Ok(KimiPrintEvent { native, data })
}

/// The caller composes the documented Print argv/profile and private environment.
/// This adapter supplies no launch grant and never changes the qualified web flow.
#[expect(
    clippy::result_large_err,
    reason = "Keep the actual Process terminal and Remaining owner."
)]
pub async fn run_kimi_print<S: KimiPrintSink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    recheck: &(dyn Fn() -> io::Result<()> + Sync),
    limits: KimiPrintLimits,
    sink: &mut S,
) -> Result<KimiPrintReceipt, KimiPrintFailure> {
    let mut bridge = Bridge {
        recheck,
        sink,
        limits,
        pending: Vec::new(),
        receipt: KimiPrintReceipt::empty(),
        failure: None,
        cause: None,
    };
    let before = bridge.check(original).and_then(|()| {
        limits.validate().map_err(|error| {
            bridge.cause = Some(Cause::Wire(error));
            KimiPrintReason::Policy
        })
    });
    if let Err(reason) = before {
        return Err(KimiPrintFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|e| KimiPrintFailure {
            reason: KimiPrintReason::Admission(e),
            receipt: KimiPrintReceipt::empty(),
            cause: None,
        })?;
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    let mut reason = bridge.failure.or(match &terminal {
        Err(_) => Some(KimiPrintReason::Process),
        Ok(outcome) if !outcome.status.success() => Some(KimiPrintReason::NonzeroExit),
        Ok(_) => None,
    });
    let pipes = match &terminal {
        Ok(o) => o.pipes,
        Err(e) => e.pipes,
    };
    // Only observed stdout EOF permits final unterminated-frame decoding.
    if bridge.failure.is_none() && pipes.stdout == PipeEnd::Eof && !bridge.pending.is_empty() {
        let last = bridge.frame(original).await;
        if reason.is_none() {
            reason = last.err();
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
                reason = Some(KimiPrintReason::UnfinishedPipe(stream));
                break;
            }
        }
    }
    if reason.is_none()
        && matches!(&terminal, Ok(o) if o.cleanup != Cleanup::Complete || o.remaining.is_some())
    {
        reason = Some(KimiPrintReason::Cleanup);
    }
    bridge.receipt.process = Some(terminal);
    match reason {
        Some(reason) => Err(KimiPrintFailure {
            reason,
            receipt: bridge.receipt,
            cause: bridge.cause,
        }),
        None => Ok(bridge.receipt),
    }
}
