//! Incremental Codex JSONL over the existing caller-owned Process supervisor.
use crate::{
    cli_codex_wire::{CodexDecodedFrame, CodexEventView},
    codex_completion::{
        AttemptPurpose, CodexCompletionAttempt, CodexCompletionRun, CompletionOutcome,
        KnownTextEstimate, UsageEvidence, UsageUnavailableReason,
    },
};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{
    Chunk, Counts, Failure, IncrementalSink, OperationAdmissionFailure, Outcome, PipeEnd,
    ProcessHost, Request, Retention, SinkFailure, Stream,
};
use std::{collections::TryReserveError, fmt, future::Future, io};

#[derive(Clone, Copy, Debug)]
pub struct CodexStreamLimits {
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexClassification {
    Unknown,
    Structured,
}
#[derive(Clone, Copy, Debug)]
pub enum CodexReason {
    Policy,
    StdoutLimit,
    StderrLimit,
    FrameLimit,
    Allocation,
    State,
    Sink(SinkFailure),
    Context(io::ErrorKind),
    Admission(OperationAdmissionFailure),
    Process,
    NonzeroExit,
    UnfinishedPipe(Stream),
}
/// Cooperatively poll; cancellation-safe on drop; leave no detached work.
/// Payload views are explicit and excluded from consumer diagnostics.
pub trait CodexSink: Send {
    fn push<'a>(
        &'a mut self,
        event: CodexEventView<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}
pub struct CodexReceipt<'n> {
    completion: Option<CompletionOutcome<'n>>,
    incomplete_usage: Option<UsageEvidence>,
    process: Option<Result<Outcome, Failure>>,
    accepted: Counts,
    frames: u64,
    known_events: u64,
    classification: CodexClassification,
}
/// Moves the original completion and Process custody without finishing or accounting again.
/// Only `completion` retains the requested-model borrow; the other components are owned.
pub struct CodexReceiptParts<'n> {
    pub completion: Option<CompletionOutcome<'n>>,
    pub process: Option<Result<Outcome, Failure>>,
    pub usage: UsageEvidence,
    pub accepted: Counts,
    pub frames: u64,
    pub known_events: u64,
    pub classification: CodexClassification,
}
impl<'n> CodexReceipt<'n> {
    fn empty() -> Self {
        Self {
            completion: None,
            incomplete_usage: None,
            process: None,
            accepted: Counts::default(),
            frames: 0,
            known_events: 0,
            classification: CodexClassification::Unknown,
        }
    }
    /// Transfers every observation and the actual Process outcome/failure to the caller.
    /// The caller retains any remaining stop owner and must complete its real cleanup.
    pub fn into_parts(self) -> CodexReceiptParts<'n> {
        let usage = self.usage();
        CodexReceiptParts {
            completion: self.completion,
            process: self.process,
            usage,
            accepted: self.accepted,
            frames: self.frames,
            known_events: self.known_events,
            classification: self.classification,
        }
    }
    pub fn completion(&self) -> Option<&CompletionOutcome<'n>> {
        self.completion.as_ref()
    }
    /// Actual attempt evidence is available even when execution cannot produce completion.
    pub fn usage(&self) -> UsageEvidence {
        match &self.completion {
            Some(CompletionOutcome::Answer(answer)) => answer.end().usage(),
            Some(CompletionOutcome::EvidenceUnavailable(end)) => end.usage(),
            Some(CompletionOutcome::Failed(error)) => error.usage(),
            None => self.incomplete_usage.unwrap_or(UsageEvidence::Unavailable(
                UsageUnavailableReason::NoNativeUsage,
            )),
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
    pub fn classification(&self) -> CodexClassification {
        self.classification
    }
}
impl fmt::Debug for CodexReceipt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodexReceipt")
            .field("accepted", &self.accepted)
            .field("frames", &self.frames)
            .field("known_events", &self.known_events)
            .field("classification", &self.classification)
            .field("process", &self.process)
            .finish()
    }
}
// Real causes are retained privately; their payload-bearing Display is never delegated.
enum Cause {
    Io(io::Error),
    Allocation(TryReserveError),
}
impl fmt::Debug for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => f.debug_tuple("Context").field(&e.kind()).finish(),
            Self::Allocation(cause) => {
                let _ = cause;
                f.write_str("Allocation")
            }
        }
    }
}
impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for Cause {}
/// The same owned IO/allocation cause, with the consumer's payload-safe diagnostics.
pub struct CodexOwnedCause(Cause);
impl fmt::Debug for CodexOwnedCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}
impl fmt::Display for CodexOwnedCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}
impl std::error::Error for CodexOwnedCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}
/// The original failure components, without replacing its native terminal or first cause.
/// State and Process causes remain in the corresponding completion and Process components.
pub struct CodexFailureParts<'n> {
    pub reason: CodexReason,
    pub receipt: CodexReceipt<'n>,
    pub cause: Option<CodexOwnedCause>,
}
pub struct CodexFailure<'n> {
    reason: CodexReason,
    receipt: CodexReceipt<'n>,
    cause: Option<Cause>,
}
impl<'n> CodexFailure<'n> {
    /// Transfers the real reason, receipt and optional IO/allocation cause.
    /// Consume borrowed completion data inside its lifetime before returning owned components.
    pub fn into_parts(self) -> CodexFailureParts<'n> {
        CodexFailureParts {
            reason: self.reason,
            receipt: self.receipt,
            cause: self.cause.map(CodexOwnedCause),
        }
    }
    pub fn reason(&self) -> CodexReason {
        self.reason
    }
    pub fn receipt(&self) -> &CodexReceipt<'n> {
        &self.receipt
    }
    pub fn receipt_mut(&mut self) -> &mut CodexReceipt<'n> {
        &mut self.receipt
    }
}
impl fmt::Debug for CodexFailure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodexFailure")
            .field("reason", &self.reason)
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl fmt::Display for CodexFailure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Codex subprocess {:?}", self.reason)
    }
}
impl std::error::Error for CodexFailure<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.reason {
            CodexReason::Context(_) | CodexReason::Allocation => self
                .cause
                .as_ref()
                .map(|v| v as &(dyn std::error::Error + 'static)),
            CodexReason::State => match &self.receipt.completion {
                Some(CompletionOutcome::Failed(failure)) => Some(failure),
                _ => None,
            },
            CodexReason::Process => self
                .receipt
                .process
                .as_ref()
                .and_then(|v| v.as_ref().err())
                .map(|v| v as &(dyn std::error::Error + 'static)),
            _ => None,
        }
    }
}
struct Bridge<'s, 'r, 'n, S> {
    attempt: CodexCompletionAttempt<'r, 'n>,
    sink: &'s mut S,
    limits: CodexStreamLimits,
    frame_limit: usize,
    pending: Vec<u8>,
    pending_cr: bool,
    receipt: CodexReceipt<'n>,
    failure: Option<CodexReason>,
    cause: Option<Cause>,
}
impl<S: CodexSink> Bridge<'_, '_, '_, S> {
    fn check(&mut self, original: &SnapshotContext) -> Result<(), CodexReason> {
        original.check().map_err(|e| {
            let kind = e.kind();
            if self.cause.is_none() {
                self.cause = Some(Cause::Io(e));
            }
            CodexReason::Context(kind)
        })
    }
    async fn frame(&mut self, original: &SnapshotContext) -> Result<(), CodexReason> {
        self.check(original)?;
        let decoded = self
            .attempt
            .decode_frame(&self.pending)
            .ok_or(CodexReason::State)?;
        self.receipt.frames = self
            .receipt
            .frames
            .checked_add(1)
            .ok_or(CodexReason::State)?;
        // The owned decoder output no longer borrows frame bytes. Mark consumed before
        // awaiting the sink: a dropped callback must not leave the frame for EOF replay.
        self.pending.clear();
        if let CodexDecodedFrame::Known(event) = decoded {
            self.receipt.classification = CodexClassification::Structured;
            // Final-fragment callbacks occur after Process has returned. They still
            // borrow the original cancellation/deadline and must be safe on drop.
            let deadline = original.required_deadline().map_err(|error| {
                let kind = error.kind();
                if self.cause.is_none() {
                    self.cause = Some(Cause::Io(error));
                }
                CodexReason::Context(kind)
            })?;
            tokio::select! {
                biased;
                _ = original.cancellation().cancelled() => self.check(original)?,
                _ = tokio::time::sleep_until(deadline.into()) => self.check(original)?,
                result = self.sink.push(event.view(), original) => result.map_err(CodexReason::Sink)?,
            }
            self.receipt.known_events += 1;
            self.attempt
                .observe_decoded(event)
                .map_err(|_| CodexReason::State)?;
        }
        self.check(original)
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), CodexReason> {
        let next = self
            .pending
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.frame_limit)
            .ok_or(CodexReason::FrameLimit)?;
        self.pending
            .try_reserve_exact(next - self.pending.len())
            .map_err(|e| {
                self.cause = Some(Cause::Allocation(e));
                CodexReason::Allocation
            })?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    fn segment(&mut self, bytes: &[u8], terminated: bool) -> Result<(), CodexReason> {
        if self.pending_cr && (!bytes.is_empty() || !terminated) {
            // An empty unterminated segment supplies no new byte; keep the lookahead.
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
    ) -> Result<(), CodexReason> {
        self.check(original)?;
        let (count, cap, reason) = match chunk.stream() {
            Stream::Stdout => (
                &mut self.receipt.accepted.stdout,
                self.limits.stdout_bytes,
                CodexReason::StdoutLimit,
            ),
            Stream::Stderr => (
                &mut self.receipt.accepted.stderr,
                self.limits.stderr_bytes,
                CodexReason::StderrLimit,
            ),
        };
        let next = count
            .checked_add(chunk.bytes().len() as u64)
            .filter(|&v| v <= cap as u64)
            .ok_or(reason)?;
        *count = next; // bytes admitted to this consumer; sink event acceptance is counted separately.
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
impl<S: CodexSink> IncrementalSink for Bridge<'_, '_, '_, S> {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if self.failure.is_some() {
            return Err(SinkFailure::ConsumerFailed);
        }
        match self.deliver(chunk, original).await {
            Ok(()) => Ok(()),
            Err(reason) => {
                self.failure = Some(reason);
                Err(match reason {
                    CodexReason::Sink(e) => e,
                    _ => SinkFailure::ConsumerFailed,
                })
            }
        }
    }
}
/// Await the real supervisor and its cleanup. An exit status alone never proves wire EOF.
#[expect(
    clippy::result_large_err,
    reason = "Preserve the real terminal and owned evidence inline."
)]
pub async fn run_codex<'n, S: CodexSink>(
    run: &mut CodexCompletionRun<'n>,
    host: &ProcessHost,
    request: Request,
    purpose: AttemptPurpose,
    estimate: Option<KnownTextEstimate>,
    limits: CodexStreamLimits,
    sink: &mut S,
) -> Result<CodexReceipt<'n>, CodexFailure<'n>> {
    let original = run.original();
    let mut receipt = CodexReceipt::empty();
    if let Err(e) = original.check() {
        return Err(CodexFailure {
            reason: CodexReason::Context(e.kind()),
            receipt,
            cause: Some(Cause::Io(e)),
        });
    }
    let frame_limit = run.frame_bytes();
    if limits.stdout_bytes == 0
        || limits.stderr_bytes == 0
        || frame_limit == 0
        || frame_limit > 8 * 1024 * 1024
    {
        return Err(CodexFailure {
            reason: CodexReason::Policy,
            receipt,
            cause: None,
        });
    }
    let (_stop, receiver) =
        graphit_process::operation(original, host).map_err(|e| CodexFailure {
            reason: CodexReason::Admission(e),
            receipt: CodexReceipt::empty(),
            cause: None,
        })?;
    let attempt = match run.begin_attempt(purpose, estimate) {
        Ok(v) => v,
        Err(e) => {
            receipt.completion = Some(CompletionOutcome::Failed(e));
            return Err(CodexFailure {
                reason: CodexReason::State,
                receipt,
                cause: None,
            });
        }
    };
    let mut bridge = Bridge {
        attempt,
        sink,
        limits,
        frame_limit,
        pending: Vec::new(),
        pending_cr: false,
        receipt,
        failure: None,
        cause: None,
    };
    let terminal =
        graphit_process::run(request, original, Retention::None, &mut bridge, receiver).await;
    // Fix the first terminal cause before checking later fragment/context observations.
    let terminal_reason = match &terminal {
        Err(_) => Some(CodexReason::Process),
        Ok(o) if !o.status.success() => Some(CodexReason::NonzeroExit),
        Ok(_) => None,
    };
    let mut reason = bridge.failure.or(terminal_reason);
    let pipes = match &terminal {
        Ok(o) => o.pipes,
        Err(e) => e.pipes,
    };
    // Only true stdout EOF permits final-fragment decode, including on a failed terminal.
    if bridge.failure.is_none() && pipes.stdout == PipeEnd::Eof {
        let final_result = if bridge.pending_cr {
            bridge.pending_cr = false;
            bridge.append(b"\r")
        } else {
            Ok(())
        };
        let final_result = match final_result {
            Ok(()) if !bridge.pending.is_empty() => bridge.frame(original).await,
            other => other,
        };
        if reason.is_none() {
            reason = final_result.err();
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
                reason = Some(CodexReason::UnfinishedPipe(stream));
                break;
            }
        }
    }
    let mut receipt = bridge.receipt;
    receipt.process = Some(terminal);
    if reason.is_none() {
        let completion = bridge.attempt.finish_at_wire_eof();
        if matches!(completion, CompletionOutcome::Failed(_)) {
            reason = Some(CodexReason::State);
        }
        receipt.completion = Some(completion);
    } else {
        let (usage, failure) = bridge.attempt.finish_without_wire_eof();
        if let Some(failure) = failure {
            receipt.completion = Some(CompletionOutcome::Failed(failure));
        } else {
            receipt.incomplete_usage = Some(usage);
        }
    }
    match reason {
        None => Ok(receipt),
        Some(reason) => Err(CodexFailure {
            reason,
            receipt,
            cause: bridge.cause,
        }),
    }
}
