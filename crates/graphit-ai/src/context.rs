//! The caller's original cancellation signal and absolute deadline.
use graphit_core::file_lock::Cancellation;
use std::{fmt, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextError {
    Cancelled,
    Deadline,
}
impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "inference cancelled",
            Self::Deadline => "inference deadline exceeded",
        })
    }
}
impl std::error::Error for ContextError {}

/// Borrowing preserves the identity of the original signal; copying this context
/// never extends the deadline or creates a new cancellation state.
#[derive(Clone, Copy)]
pub struct InferenceContext<'a> {
    cancellation: &'a Cancellation,
    deadline: Instant,
}
impl<'a> InferenceContext<'a> {
    pub fn new(cancellation: &'a Cancellation, deadline: Instant) -> Self {
        Self {
            cancellation,
            deadline,
        }
    }
    pub fn cancellation(self) -> &'a Cancellation {
        self.cancellation
    }
    pub fn deadline(self) -> Instant {
        self.deadline
    }
    pub fn check(self) -> Result<(), ContextError> {
        if self.cancellation.is_cancelled() {
            Err(ContextError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(ContextError::Deadline)
        } else {
            Ok(())
        }
    }
}
