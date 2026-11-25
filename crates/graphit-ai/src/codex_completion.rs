//! Codex model policy data over the pinned wire decoder.
//! A completed model turn is neither Process success nor native authority.
//! Each run retains the caller's original context and accounts its actual Graphit attempts.
use crate::cli_codex_wire::{
    CodexDecodedFrame, CodexEventView, CodexItemBodyView, CodexItemPhase, CodexUsage,
    CodexWireError, CodexWireLimits, OwnedCodexEvent, decode_codex_frame,
};
use graphit_core::file_io::SnapshotContext;
use std::{error::Error, fmt, io, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptPurpose {
    Request,
    Retry,
    Synthesis,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRpcCalls {
    Unobserved,
}
#[derive(Clone, Copy)]
pub struct CompletionLimits {
    pub max_graphit_attempts: u64,
    pub max_response_bytes: usize,
    pub max_model_id_bytes: usize,
}
impl CompletionLimits {
    fn valid(self) -> bool {
        self.max_graphit_attempts > 0 && self.max_response_bytes > 0 && self.max_model_id_bytes > 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodexNormalizedUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub cache_write_input_tokens: u64,
}
impl TryFrom<CodexUsage> for CodexNormalizedUsage {
    type Error = ();
    fn try_from(v: CodexUsage) -> Result<Self, Self::Error> {
        Ok(Self {
            input_tokens: v.input_tokens.try_into().map_err(|_| ())?,
            cached_input_tokens: v.cached_input_tokens.try_into().map_err(|_| ())?,
            output_tokens: v.output_tokens.try_into().map_err(|_| ())?,
            reasoning_output_tokens: v.reasoning_output_tokens.try_into().map_err(|_| ())?,
            cache_write_input_tokens: v.cache_write_input_tokens.try_into().map_err(|_| ())?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Estimator {
    Utf8KnownTextBytesFloorDiv4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstimateCoverage {
    KnownTextOnlyNativeTokenizerUnknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnownTextEstimate {
    pub input_text_bytes: u64,
    pub output_text_bytes: u64,
}
impl KnownTextEstimate {
    pub fn estimator(self) -> Estimator {
        Estimator::Utf8KnownTextBytesFloorDiv4
    }
    pub fn coverage(self) -> EstimateCoverage {
        EstimateCoverage::KnownTextOnlyNativeTokenizerUnknown
    }
    pub fn input_tokens(self) -> u64 {
        self.input_text_bytes / 4
    }
    pub fn output_tokens(self) -> u64 {
        self.output_text_bytes / 4
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageUnavailableReason {
    NoNativeUsage,
    InvalidNativeUsage,
    AbandonedAttempt,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageEvidence {
    Observed(CodexNormalizedUsage),
    Estimated(KnownTextEstimate),
    Unavailable(UsageUnavailableReason),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UsageTotals {
    pub observed_input_tokens: u128,
    pub observed_cached_input_tokens: u128,
    pub observed_output_tokens: u128,
    pub observed_reasoning_output_tokens: u128,
    pub observed_cache_write_input_tokens: u128,
    pub estimated_known_input_tokens: u128,
    pub estimated_known_output_tokens: u128,
    pub estimated_input_text_bytes: u128,
    pub estimated_output_text_bytes: u128,
    pub observed_attempts: u64,
    pub estimated_attempts: u64,
    pub unavailable_attempts: u64,
}
impl UsageTotals {
    fn with(self, usage: UsageEvidence) -> Option<Self> {
        let mut next = self;
        // Compute a prospective copy; commit only after every checked addition.
        match usage {
            UsageEvidence::Observed(v) => {
                next.observed_input_tokens = next
                    .observed_input_tokens
                    .checked_add(v.input_tokens.into())?;
                next.observed_cached_input_tokens = next
                    .observed_cached_input_tokens
                    .checked_add(v.cached_input_tokens.into())?;
                next.observed_output_tokens = next
                    .observed_output_tokens
                    .checked_add(v.output_tokens.into())?;
                next.observed_reasoning_output_tokens = next
                    .observed_reasoning_output_tokens
                    .checked_add(v.reasoning_output_tokens.into())?;
                next.observed_cache_write_input_tokens = next
                    .observed_cache_write_input_tokens
                    .checked_add(v.cache_write_input_tokens.into())?;
                next.observed_attempts = next.observed_attempts.checked_add(1)?;
            }
            UsageEvidence::Estimated(v) => {
                next.estimated_known_input_tokens = next
                    .estimated_known_input_tokens
                    .checked_add(v.input_tokens().into())?;
                next.estimated_known_output_tokens = next
                    .estimated_known_output_tokens
                    .checked_add(v.output_tokens().into())?;
                next.estimated_input_text_bytes = next
                    .estimated_input_text_bytes
                    .checked_add(v.input_text_bytes.into())?;
                next.estimated_output_text_bytes = next
                    .estimated_output_text_bytes
                    .checked_add(v.output_text_bytes.into())?;
                next.estimated_attempts = next.estimated_attempts.checked_add(1)?;
            }
            UsageEvidence::Unavailable(_) => {
                next.unavailable_attempts = next.unavailable_attempts.checked_add(1)?;
            }
        }
        Some(next)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionFailureKind {
    InvalidLimits,
    ModelId,
    Phase,
    ResponseLimit,
    InvalidUsage,
    MaxGraphitAttempts,
    ArithmeticOverflow,
    NativeIo,
    Wire,
    Provider,
}
struct NativeCause(io::Error);
impl fmt::Debug for NativeCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeCause")
            .field("kind", &self.0.kind())
            .field("code", &self.0.raw_os_error())
            .finish()
    }
}
impl fmt::Display for NativeCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native request: {:?}", self.0.kind())
    }
}
impl Error for NativeCause {}
enum FailureCause {
    Policy(CompletionFailureKind),
    Native(NativeCause),
    Wire(CodexWireError),
    Provider { _event: OwnedCodexEvent },
    InvalidUsage { _event: OwnedCodexEvent },
}
impl FailureCause {
    fn kind(&self) -> CompletionFailureKind {
        match self {
            Self::Policy(kind) => *kind,
            Self::Native(_) => CompletionFailureKind::NativeIo,
            Self::Wire(_) => CompletionFailureKind::Wire,
            Self::Provider { .. } => CompletionFailureKind::Provider,
            Self::InvalidUsage { .. } => CompletionFailureKind::InvalidUsage,
        }
    }
}
pub struct CompletionFailure {
    cause: FailureCause,
    usage: UsageEvidence,
    ordinal: Option<u64>,
    purpose: Option<AttemptPurpose>,
}
impl CompletionFailure {
    pub fn kind(&self) -> CompletionFailureKind {
        self.cause.kind()
    }
    pub fn usage(&self) -> UsageEvidence {
        self.usage
    }
    pub fn ordinal(&self) -> Option<u64> {
        self.ordinal
    }
    pub fn purpose(&self) -> Option<AttemptPurpose> {
        self.purpose
    }
    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match &self.cause {
            FailureCause::Native(v) => Some(v.0.kind()),
            FailureCause::Wire(v) => v.io_kind(),
            _ => None,
        }
    }
    fn negative(cause: FailureCause) -> Self {
        Self {
            cause,
            usage: UsageEvidence::Unavailable(UsageUnavailableReason::NoNativeUsage),
            ordinal: None,
            purpose: None,
        }
    }
}
impl fmt::Debug for CompletionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompletionFailure")
            .field("kind", &self.kind())
            .field("io_kind", &self.io_kind())
            .finish()
    }
}
impl fmt::Display for CompletionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "completion: {:?}", self.kind())
    }
}
impl Error for CompletionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.cause {
            FailureCause::Native(v) => Some(v),
            FailureCause::Wire(v) => Some(v),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelEvidenceUnavailableReason {
    NoCompletedTurn,
    NoCompletedModelMessage,
}
pub struct OwnedModelEnd<'n> {
    requested_model: Option<&'n str>,
    usage: UsageEvidence,
    ordinal: u64,
    purpose: AttemptPurpose,
    reason: Option<ModelEvidenceUnavailableReason>,
}
impl<'n> OwnedModelEnd<'n> {
    pub fn requested_model(&self) -> Option<&'n str> {
        self.requested_model
    }
    pub fn usage(&self) -> UsageEvidence {
        self.usage
    }
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }
    pub fn purpose(&self) -> AttemptPurpose {
        self.purpose
    }
    pub fn reason(&self) -> Option<ModelEvidenceUnavailableReason> {
        self.reason
    }
    pub fn native_rpc_calls(&self) -> NativeRpcCalls {
        NativeRpcCalls::Unobserved
    }
}
pub struct OwnedModelAnswer<'n> {
    message: OwnedCodexEvent,
    end: OwnedModelEnd<'n>,
}
impl<'n> OwnedModelAnswer<'n> {
    pub fn text(&self) -> &str {
        match self.message.view() {
            CodexEventView::Item {
                phase: CodexItemPhase::Completed,
                item,
            } => match item.body {
                CodexItemBodyView::AgentMessage { text } => text,
                _ => unreachable!("only completed model messages are retained"),
            },
            _ => unreachable!("only completed model messages are retained"),
        }
    }
    pub fn end(&self) -> &OwnedModelEnd<'n> {
        &self.end
    }
}
pub enum CompletionOutcome<'n> {
    Answer(OwnedModelAnswer<'n>),
    EvidenceUnavailable(OwnedModelEnd<'n>),
    Failed(CompletionFailure),
}
pub struct CodexCompletionRun<'n> {
    original: &'n Arc<SnapshotContext>,
    requested_model: Option<&'n str>,
    wire_limits: &'n CodexWireLimits,
    limits: CompletionLimits,
    attempts: u64,
    totals: UsageTotals,
    gate: Option<CompletionFailureKind>,
}
impl<'n> CodexCompletionRun<'n> {
    #[expect(
        clippy::result_large_err,
        reason = "Preserve the actual owned wire/provider cause inline without another error allocation."
    )]
    pub fn new(
        original: &'n Arc<SnapshotContext>,
        requested_model: Option<&'n str>,
        wire_limits: &'n CodexWireLimits,
        limits: CompletionLimits,
    ) -> Result<Self, CompletionFailure> {
        original
            .check()
            .map_err(|e| CompletionFailure::negative(FailureCause::Native(NativeCause(e))))?;
        if !limits.valid() {
            return Err(CompletionFailure::negative(FailureCause::Policy(
                CompletionFailureKind::InvalidLimits,
            )));
        }
        if requested_model
            .is_some_and(|model| model.is_empty() || model.len() > limits.max_model_id_bytes)
        {
            return Err(CompletionFailure::negative(FailureCause::Policy(
                CompletionFailureKind::ModelId,
            )));
        }
        wire_limits
            .validate()
            .map_err(|e| CompletionFailure::negative(FailureCause::Wire(e)))?;
        Ok(Self {
            original,
            requested_model,
            wire_limits,
            limits,
            attempts: 0,
            totals: UsageTotals::default(),
            gate: None,
        })
    }
    pub fn original(&self) -> &'n Arc<SnapshotContext> {
        self.original
    }
    pub(crate) fn frame_bytes(&self) -> usize {
        self.wire_limits.frame_bytes
    }
    pub fn admitted_attempts(&self) -> u64 {
        self.attempts
    }
    pub fn totals(&self) -> UsageTotals {
        self.totals
    }
    pub fn native_rpc_calls(&self) -> NativeRpcCalls {
        NativeRpcCalls::Unobserved
    }
    #[expect(
        clippy::result_large_err,
        reason = "Transfer the actual owned cause and usage evidence without another error allocation."
    )]
    pub fn begin_attempt<'r>(
        &'r mut self,
        purpose: AttemptPurpose,
        estimate: Option<KnownTextEstimate>,
    ) -> Result<CodexCompletionAttempt<'r, 'n>, CompletionFailure> {
        self.original
            .check()
            .map_err(|e| CompletionFailure::negative(FailureCause::Native(NativeCause(e))))?;
        if let Some(kind) = self.gate {
            return Err(CompletionFailure::negative(FailureCause::Policy(kind)));
        }
        if self.attempts >= self.limits.max_graphit_attempts {
            return Err(CompletionFailure::negative(FailureCause::Policy(
                CompletionFailureKind::MaxGraphitAttempts,
            )));
        }
        self.attempts += 1;
        let ordinal = self.attempts;
        Ok(CodexCompletionAttempt {
            run: self,
            ordinal,
            purpose,
            estimate,
            frames: 0,
            thread_seen: false,
            phase: Phase::BeforeTurn,
            message: None,
            observed: None,
            invalid_usage: false,
            failure: None,
            accounted: false,
        })
    }
    fn account(&mut self, usage: UsageEvidence) -> Option<CompletionFailureKind> {
        let Some(next) = self.totals.with(usage) else {
            self.gate = Some(CompletionFailureKind::ArithmeticOverflow);
            return self.gate;
        };
        self.totals = next; // Exact observed amounts are evidence, not token quotas.
        self.gate
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    BeforeTurn,
    Active,
    Completed,
}
pub struct CodexCompletionAttempt<'r, 'n> {
    run: &'r mut CodexCompletionRun<'n>,
    ordinal: u64,
    purpose: AttemptPurpose,
    estimate: Option<KnownTextEstimate>,
    frames: u64,
    thread_seen: bool,
    phase: Phase,
    message: Option<OwnedCodexEvent>,
    observed: Option<CodexNormalizedUsage>,
    invalid_usage: bool,
    failure: Option<CompletionFailure>,
    accounted: bool,
}
impl<'r, 'n> CodexCompletionAttempt<'r, 'n> {
    fn latch(&mut self, cause: FailureCause) {
        if self.failure.is_none() {
            // Release the unused candidate; the first cause remains sticky.
            self.message = None;
            let mut failure = CompletionFailure::negative(cause);
            failure.ordinal = Some(self.ordinal);
            failure.purpose = Some(self.purpose);
            self.failure = Some(failure);
        }
    }
    fn control(&mut self) {
        if self.failure.is_none()
            && let Err(error) = self.run.original.check()
        {
            self.latch(FailureCause::Native(NativeCause(error)));
        }
    }
    pub(crate) fn decode_frame(&mut self, raw: &[u8]) -> Option<CodexDecodedFrame> {
        self.control();
        if self.failure.is_some() {
            return None;
        }
        let Some(next) = self.frames.checked_add(1) else {
            self.latch(FailureCause::Policy(
                CompletionFailureKind::ArithmeticOverflow,
            ));
            return None;
        };
        self.frames = next;
        match decode_codex_frame(raw, self.run.wire_limits, self.run.original) {
            Ok(frame) => {
                // The frame's native usage is observed before a later sink/context failure.
                // This does not publish a phase transition, a message or an answer.
                if self.phase == Phase::Active
                    && let CodexDecodedFrame::Known(event) = &frame
                    && let CodexEventView::TurnCompleted { usage } = event.view()
                {
                    match CodexNormalizedUsage::try_from(usage) {
                        Ok(usage) => self.observed = Some(usage),
                        Err(()) => self.invalid_usage = true,
                    }
                }
                Some(frame)
            }
            Err(error) => {
                self.latch(FailureCause::Wire(error));
                None
            }
        }
    }
    pub(crate) fn observe_decoded(
        &mut self,
        event: OwnedCodexEvent,
    ) -> Result<(), &CompletionFailure> {
        self.control();
        if self.failure.is_none() {
            self.observe_known(event);
        }
        self.control();
        self.failure.as_ref().map_or(Ok(()), Err)
    }
    pub fn observe_frame(&mut self, raw: &[u8]) -> Result<(), &CompletionFailure> {
        if let Some(CodexDecodedFrame::Known(event)) = self.decode_frame(raw) {
            self.observe_known(event);
        }
        self.control();
        self.failure.as_ref().map_or(Ok(()), Err)
    }
    /// Account an interrupted attempt without claiming EOF or publishing an answer.
    pub(crate) fn finish_without_wire_eof(mut self) -> (UsageEvidence, Option<CompletionFailure>) {
        self.control();
        let usage = self.usage(true);
        if let Some(kind) = self.run.account(usage) {
            self.latch(FailureCause::Policy(kind));
        }
        self.accounted = true;
        let failure = self.failure.take().map(|mut failure| {
            failure.usage = usage;
            failure
        });
        (usage, failure)
    }
    fn observe_known(&mut self, event: OwnedCodexEvent) {
        match event.view() {
            CodexEventView::Error { .. } => self.latch(FailureCause::Provider { _event: event }),
            CodexEventView::ThreadStarted { .. }
                if self.phase == Phase::BeforeTurn && !self.thread_seen =>
            {
                self.thread_seen = true
            }
            CodexEventView::TurnStarted if self.phase == Phase::BeforeTurn => {
                self.phase = Phase::Active
            }
            CodexEventView::TurnFailed { .. } if self.phase == Phase::Active => {
                self.latch(FailureCause::Provider { _event: event })
            }
            CodexEventView::TurnCompleted { usage } if self.phase == Phase::Active => {
                match CodexNormalizedUsage::try_from(usage) {
                    Ok(usage) => {
                        self.observed = Some(usage);
                        self.phase = Phase::Completed;
                    }
                    Err(()) => {
                        self.invalid_usage = true;
                        self.latch(FailureCause::InvalidUsage { _event: event });
                    }
                }
            }
            CodexEventView::Item { phase, item } if self.phase == Phase::Active => {
                if phase == CodexItemPhase::Completed
                    && let CodexItemBodyView::AgentMessage { text } = item.body
                {
                    if text.len() > self.run.limits.max_response_bytes {
                        self.latch(FailureCause::Policy(CompletionFailureKind::ResponseLimit));
                    } else {
                        self.message = Some(event);
                    }
                }
            }
            _ => self.latch(FailureCause::Policy(CompletionFailureKind::Phase)),
        }
    }
    fn usage(&self, abandoned: bool) -> UsageEvidence {
        if let Some(usage) = self.observed {
            UsageEvidence::Observed(usage)
        } else if self.invalid_usage {
            UsageEvidence::Unavailable(UsageUnavailableReason::InvalidNativeUsage)
        } else if let Some(estimate) = self.estimate {
            UsageEvidence::Estimated(estimate)
        } else {
            UsageEvidence::Unavailable(if abandoned {
                UsageUnavailableReason::AbandonedAttempt
            } else {
                UsageUnavailableReason::NoNativeUsage
            })
        }
    }
    pub fn finish_at_wire_eof(mut self) -> CompletionOutcome<'n> {
        self.control();
        let usage = self.usage(false);
        let gate = self.run.account(usage);
        self.accounted = true;
        if let Some(kind) = gate {
            self.latch(FailureCause::Policy(kind));
        }
        self.control();
        if let Some(mut failure) = self.failure.take() {
            failure.usage = usage;
            return CompletionOutcome::Failed(failure);
        }
        let reason = if self.phase != Phase::Completed {
            Some(ModelEvidenceUnavailableReason::NoCompletedTurn)
        } else if self.message.is_none() {
            Some(ModelEvidenceUnavailableReason::NoCompletedModelMessage)
        } else {
            None
        };
        let end = OwnedModelEnd {
            requested_model: self.run.requested_model,
            usage,
            ordinal: self.ordinal,
            purpose: self.purpose,
            reason,
        };
        if reason.is_none() {
            CompletionOutcome::Answer(OwnedModelAnswer {
                message: self.message.take().expect("completed model message"),
                end,
            })
        } else {
            self.message = None;
            CompletionOutcome::EvidenceUnavailable(end)
        }
    }
}
impl Drop for CodexCompletionAttempt<'_, '_> {
    fn drop(&mut self) {
        if !self.accounted {
            // Pure fixed data only. Drop does not claim IO drain, SDK effects or native quiescence.
            let usage = self.usage(true);
            self.run.account(usage);
            self.accounted = true;
        }
    }
}

#[cfg(test)]
#[path = "codex_completion_tests.rs"]
mod tests;
