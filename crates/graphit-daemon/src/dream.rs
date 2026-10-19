//! PROJECT Dream: native proposal data and admitted automatic Memory effects.
pub mod recipe;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod user;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub use user::UserApplication;

use graphit_ai::{
    cli_codex::{CodexOwnedCause, CodexReason, CodexSink, CodexStreamLimits, run_codex},
    cli_codex_wire::{CodexEventView, CodexWireLimits},
    codex_completion::{
        AttemptPurpose, CodexCompletionRun, CompletionFailure, CompletionLimits, CompletionOutcome,
        ModelEvidenceUnavailableReason, UsageEvidence,
    },
};
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::file_io::SnapshotContext;
use graphit_process::{Failure, Outcome, ProcessHost, Request, SinkFailure};
use graphit_records::memory_store::{
    AutomaticDeleteOutcome, MemoryFoldReceipt, MemoryStoreError, MemoryStoreWriteLimits,
    NativeMemoryStore,
};
use std::{fmt, sync::Arc};

struct Events;
impl CodexSink for Events {
    async fn push(
        &mut self,
        _event: CodexEventView<'_>,
        original: &SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original.check().map_err(|_| SinkFailure::Closed)
    }
}

/// Owned observations from the actual SDK; text is proposal data only.
/// Any Process remaining owner must be repaired before releasing the service.
pub struct NativeProposal {
    pub answer: Option<String>,
    pub usage: UsageEvidence,
    pub process: Option<Result<Outcome, Failure>>,
    pub reason: Option<CodexReason>,
    pub cause: Option<CodexOwnedCause>,
    pub completion_failure: Option<CompletionFailure>,
    pub model_unavailable: Option<ModelEvidenceUnavailableReason>,
}
impl fmt::Debug for NativeProposal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeProposal")
            .field("answer_present", &self.answer.is_some())
            .field("usage", &self.usage)
            .field("reason", &self.reason)
            .field("process", &self.process)
            .finish_non_exhaustive()
    }
}

/// Uses the real Codex completion and Process driver, without an executor abstraction.
pub async fn propose(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    model: &str,
    wire: &CodexWireLimits,
    limits: CompletionLimits,
) -> Result<NativeProposal, Box<CompletionFailure>> {
    propose_guarded(original, host, request, model, wire, limits, &mut Events).await
}

/// Runs the same native driver while borrowing the caller's concrete event guard.
/// The guard grants nothing: the caller retains and rechecks its actual admissions.
pub async fn propose_guarded(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    model: &str,
    wire: &CodexWireLimits,
    limits: CompletionLimits,
    events: &mut impl CodexSink,
) -> Result<NativeProposal, Box<CompletionFailure>> {
    let streams = CodexStreamLimits {
        stdout_bytes: request.limits.stdout_bytes,
        stderr_bytes: request.limits.stderr_bytes,
    };
    let mut run = CodexCompletionRun::new(original, Some(model), wire, limits).map_err(Box::new)?;
    let executed = run_codex(
        &mut run,
        host,
        request,
        AttemptPurpose::Request,
        None,
        streams,
        events,
    )
    .await;
    let (receipt, reason, cause) = match executed {
        Ok(receipt) => (receipt, None, None),
        Err(failure) => {
            let parts = failure.into_parts();
            (parts.receipt, Some(parts.reason), parts.cause)
        }
    };
    let parts = receipt.into_parts();
    let (answer, completion_failure, model_unavailable) = match parts.completion {
        Some(CompletionOutcome::Answer(answer)) if reason.is_none() => {
            (Some(answer.text().to_owned()), None, None)
        }
        Some(CompletionOutcome::Failed(failure)) => (None, Some(failure), None),
        Some(CompletionOutcome::EvidenceUnavailable(end)) => (None, None, end.reason()),
        _ => (None, None, None),
    };
    Ok(NativeProposal {
        answer,
        usage: parts.usage,
        process: parts.process,
        reason,
        cause,
        completion_failure,
        model_unavailable,
    })
}

#[derive(Debug)]
pub enum AppliedOperation {
    Fold(MemoryFoldReceipt),
    Delete(AutomaticDeleteOutcome),
}

/// A validated sequence retained in RAM across uncertain effects.
/// The cursor advances only after an actual SDK receipt or domain refusal.
pub struct Application {
    recipe: recipe::PreparedRecipe,
    next: usize,
    receipts: Vec<AppliedOperation>,
}
impl Application {
    pub fn new(recipe: recipe::PreparedRecipe) -> Self {
        Self {
            recipe,
            next: 0,
            receipts: Vec::new(),
        }
    }
    pub fn receipts(&self) -> &[AppliedOperation] {
        &self.receipts
    }
    pub fn is_complete(&self) -> bool {
        self.next == self.recipe.operations.len()
    }

    /// Reobserves the same Runtime-owned input/key before any replay under current admission.
    /// A later failure leaves earlier receipts and the uncertain operation with this owner.
    pub async fn apply(
        &mut self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        write: &ProjectWriteAdmission<'_>,
        limits: MemoryStoreWriteLimits,
    ) -> Result<(), MemoryStoreError> {
        original.check()?;
        if limits.max_input_bytes < self.recipe.maximum {
            return Err(MemoryStoreError::Invalid(
                "recipe exceeds captured write policy",
            ));
        }
        while let Some(operation) = self.recipe.operations.get(self.next) {
            let receipt = match operation {
                recipe::Operation::Fold(input) => {
                    let observed = NativeMemoryStore::reobserve_automatic_fold(
                        original,
                        read,
                        write,
                        input.clone(),
                        limits,
                    )
                    .await?;
                    AppliedOperation::Fold(match observed {
                        Some(receipt) => receipt,
                        None => {
                            NativeMemoryStore::automatic_fold(
                                original,
                                read,
                                write,
                                input.clone(),
                                limits,
                            )
                            .await?
                        }
                    })
                }
                recipe::Operation::Delete(id, input) => {
                    let observed = NativeMemoryStore::reobserve_automatic_delete(
                        original,
                        read,
                        write,
                        *id,
                        input.clone(),
                        limits,
                    )
                    .await?;
                    AppliedOperation::Delete(match observed {
                        Some(effect) => AutomaticDeleteOutcome::Deleted(effect),
                        None => {
                            NativeMemoryStore::automatic_delete(
                                original,
                                read,
                                write,
                                *id,
                                input.clone(),
                                limits,
                            )
                            .await?
                        }
                    })
                }
            };
            self.receipts.push(receipt);
            self.next += 1;
        }
        original.check()?;
        Ok(())
    }
}
