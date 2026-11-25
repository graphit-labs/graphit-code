//! Complete native scalar scores, with explicit budgets and original context.
use crate::context::{ContextError, InferenceContext};
use graphit_http::client::NetworkError;
use http::StatusCode;
use std::{fmt, mem::size_of};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRerankError {
    Context(ContextError),
    Network(NetworkError),
    Status(StatusCode),
    Policy,
    Limit,
    Encoding,
    Shape,
    Allocation,
}
impl fmt::Display for NativeRerankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for NativeRerankError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(e) => Some(e),
            Self::Network(e) => Some(e),
            _ => None,
        }
    }
}
impl From<ContextError> for NativeRerankError {
    fn from(e: ContextError) -> Self {
        Self::Context(e)
    }
}
impl From<NetworkError> for NativeRerankError {
    fn from(e: NetworkError) -> Self {
        Self::Network(e)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NativeRerankLimits {
    pub(crate) max_candidates: usize,
    pub(crate) max_batch: usize,
    pub(crate) max_input_utf8: usize,
    pub(crate) max_request_json: usize,
    pub(crate) max_response_json: usize,
}
impl NativeRerankLimits {
    pub fn new(
        max_candidates: usize,
        max_batch: usize,
        max_input_utf8: usize,
        max_request_json: usize,
        max_response_json: usize,
    ) -> Result<Self, NativeRerankError> {
        for value in [
            max_candidates,
            max_batch,
            max_input_utf8,
            max_request_json,
            max_response_json,
        ] {
            if value == 0 || value > isize::MAX as usize {
                return Err(NativeRerankError::Policy);
            }
        }
        let width = size_of::<IndexScore>().max(size_of::<Option<f64>>());
        if max_candidates
            .checked_mul(width)
            .is_none_or(|n| n > isize::MAX as usize)
        {
            return Err(NativeRerankError::Policy);
        }
        Ok(Self {
            max_candidates,
            max_batch,
            max_input_utf8,
            max_request_json,
            max_response_json,
        })
    }
    /// Raw-input preflight, reusable before a consumer captures or loads an optional model.
    pub fn validate_input(
        self,
        query: &str,
        documents: &[&str],
        context: InferenceContext<'_>,
    ) -> Result<(), NativeRerankError> {
        context.check()?;
        if documents.len() > self.max_candidates {
            return Err(NativeRerankError::Limit);
        }
        if documents.is_empty() {
            return Ok(());
        }
        let mut bytes = query.len();
        if bytes > self.max_input_utf8 {
            return Err(NativeRerankError::Limit);
        }
        for document in documents {
            context.check()?;
            bytes = bytes
                .checked_add(document.len())
                .ok_or(NativeRerankError::Limit)?;
            if bytes > self.max_input_utf8 {
                return Err(NativeRerankError::Limit);
            }
        }
        Ok(())
    }
}

/// Wire index is local to one batch; the native scalar remains f64.
pub struct IndexScore {
    pub index: usize,
    pub relevance_score: f64,
}

/// Require the entire unique input set before exposing scores in input order.
pub fn validate_results(
    count: usize,
    rows: &[IndexScore],
    limits: NativeRerankLimits,
    context: InferenceContext<'_>,
) -> Result<Vec<f64>, NativeRerankError> {
    context.check()?;
    if count > limits.max_candidates {
        return Err(NativeRerankError::Limit);
    }
    if rows.len() != count {
        return Err(NativeRerankError::Shape);
    }
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(count)
        .map_err(|_| NativeRerankError::Allocation)?;
    slots.resize(count, None);
    for row in rows {
        context.check()?;
        if row.index >= count || !row.relevance_score.is_finite() {
            return Err(NativeRerankError::Shape);
        }
        if slots[row.index].replace(row.relevance_score).is_some() {
            return Err(NativeRerankError::Shape);
        }
    }
    let mut scores = Vec::new();
    scores
        .try_reserve_exact(count)
        .map_err(|_| NativeRerankError::Allocation)?;
    for slot in slots {
        context.check()?;
        scores.push(slot.ok_or(NativeRerankError::Shape)?);
    }
    context.check()?;
    Ok(scores)
}

/// Admission failures retain their typed cause without formatting caller data.
pub enum GuardedRerankError<E> {
    Rerank {
        source: NativeRerankError,
        effect: graphit_http::client::NetworkEffect,
    },
    Admission {
        source: E,
        effect: graphit_http::client::NetworkEffect,
    },
}
impl<E> GuardedRerankError<E> {
    pub fn effect(&self) -> graphit_http::client::NetworkEffect {
        match self {
            Self::Rerank { effect, .. } | Self::Admission { effect, .. } => *effect,
        }
    }
}
impl<E> fmt::Debug for GuardedRerankError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rerank { source, effect } => f
                .debug_struct("Rerank")
                .field("source", source)
                .field("effect", effect)
                .finish(),
            Self::Admission { effect, .. } => f
                .debug_struct("Admission")
                .field("effect", effect)
                .finish_non_exhaustive(),
        }
    }
}
impl<E> fmt::Display for GuardedRerankError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("guarded reranking failed")
    }
}
impl<E: std::error::Error + 'static> std::error::Error for GuardedRerankError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rerank { source, .. } => Some(source),
            Self::Admission { source, .. } => Some(source),
        }
    }
}
pub(crate) enum RerankFailure<E> {
    Native(NativeRerankError),
    Admission(E),
}
impl<E> From<NativeRerankError> for RerankFailure<E> {
    fn from(error: NativeRerankError) -> Self {
        Self::Native(error)
    }
}
impl<E> From<ContextError> for RerankFailure<E> {
    fn from(error: ContextError) -> Self {
        Self::Native(error.into())
    }
}
impl<E> RerankFailure<E> {
    pub(crate) fn with_effect(
        self,
        effect: graphit_http::client::NetworkEffect,
    ) -> GuardedRerankError<E> {
        match self {
            Self::Native(source) => GuardedRerankError::Rerank { source, effect },
            Self::Admission(source) => GuardedRerankError::Admission { source, effect },
        }
    }
}
pub(crate) fn check_admission<G, E>(
    gate: &mut G,
    context: InferenceContext<'_>,
) -> Result<(), RerankFailure<E>>
where
    G: FnMut() -> Result<(), E>,
{
    context.check()?;
    let admission = gate().map_err(RerankFailure::Admission);
    context.check()?;
    admission
}
