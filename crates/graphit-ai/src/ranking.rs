//! Pure ranking over caller-owned scores and vectors. No model or provider discovery.
use crate::context::{ContextError, InferenceContext};
use std::{cmp::Ordering, fmt, mem::size_of};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankingError {
    Context(ContextError),
    InvalidPolicy,
    Limit,
    Cardinality,
    Dimensions,
    NonFinite,
    ZeroNorm,
    Allocation,
}
impl fmt::Display for RankingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(error) => error.fmt(f),
            Self::InvalidPolicy => f.write_str("invalid ranking policy"),
            Self::Limit => f.write_str("ranking limit exceeded"),
            Self::Cardinality => f.write_str("ranking cardinality mismatch"),
            Self::Dimensions => f.write_str("ranking vector dimensions mismatch"),
            Self::NonFinite => f.write_str("ranking value is not finite"),
            Self::ZeroNorm => f.write_str("ranking vector has zero norm"),
            Self::Allocation => f.write_str("ranking allocation failed"),
        }
    }
}
impl std::error::Error for RankingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ContextError> for RankingError {
    fn from(error: ContextError) -> Self {
        Self::Context(error)
    }
}

/// Finite logical work limits, not an operating-system RSS or scheduling limit.
#[derive(Clone, Copy, Debug)]
pub struct RankingLimits {
    max_candidates: usize,
    max_dimensions: usize,
    max_vector_values: usize,
}
impl RankingLimits {
    pub fn new(
        max_candidates: usize,
        max_dimensions: usize,
        max_vector_values: usize,
    ) -> Result<Self, RankingError> {
        for (count, width) in [
            (max_candidates, size_of::<f64>().max(size_of::<usize>())),
            (max_dimensions, size_of::<f32>()),
            (max_vector_values, size_of::<f32>()),
        ] {
            if count == 0
                || count
                    .checked_mul(width)
                    .is_none_or(|bytes| bytes > isize::MAX as usize)
            {
                return Err(RankingError::InvalidPolicy);
            }
        }
        Ok(Self {
            max_candidates,
            max_dimensions,
            max_vector_values,
        })
    }
    fn candidates(self, count: usize) -> Result<(), RankingError> {
        if count > self.max_candidates {
            Err(RankingError::Limit)
        } else {
            Ok(())
        }
    }
}

/// Returns each input position exactly once, ordered by descending score,
/// ascending caller index, then original position. Numeric signed zeros tie.
pub fn rank_order(
    indices: &[usize],
    scores: &[f64],
    limits: RankingLimits,
    context: InferenceContext<'_>,
) -> Result<Vec<usize>, RankingError> {
    context.check()?;
    limits.candidates(indices.len())?;
    if indices.len() != scores.len() {
        return Err(RankingError::Cardinality);
    }
    for score in scores {
        context.check()?;
        if !score.is_finite() {
            return Err(RankingError::NonFinite);
        }
    }
    let mut order = Vec::new();
    order
        .try_reserve_exact(indices.len())
        .map_err(|_| RankingError::Allocation)?;
    for position in 0..indices.len() {
        context.check()?;
        order.push(position);
    }
    context.check()?;
    // The final position tie makes every comparison deterministic, so the
    // in-place unstable sorter needs no stable-sort scratch allocation.
    order.sort_unstable_by(|&left, &right| {
        let score = if scores[left] > scores[right] {
            Ordering::Less
        } else if scores[left] < scores[right] {
            Ordering::Greater
        } else {
            Ordering::Equal
        };
        score
            .then_with(|| indices[left].cmp(&indices[right]))
            .then_with(|| left.cmp(&right))
    });
    context.check()?;
    Ok(order)
}

/// Computes one cosine per document from already-produced f32 vectors. Total
/// values include the query. An empty document list performs no vector work.
pub fn similarity_scores(
    query: &[f32],
    documents: &[Vec<f32>],
    dimensions: usize,
    limits: RankingLimits,
    context: InferenceContext<'_>,
) -> Result<Vec<f64>, RankingError> {
    context.check()?;
    limits.candidates(documents.len())?;
    if dimensions == 0 {
        return Err(RankingError::Dimensions);
    }
    if dimensions > limits.max_dimensions {
        return Err(RankingError::Limit);
    }
    if documents.is_empty() {
        context.check()?;
        return Ok(Vec::new());
    }
    let total = documents
        .len()
        .checked_add(1)
        .and_then(|count| count.checked_mul(dimensions))
        .ok_or(RankingError::Limit)?;
    if total > limits.max_vector_values {
        return Err(RankingError::Limit);
    }
    if query.len() != dimensions {
        return Err(RankingError::Dimensions);
    }
    for document in documents {
        context.check()?;
        if document.len() != dimensions {
            return Err(RankingError::Dimensions);
        }
    }
    let mut query_norm = 0.0;
    for &value in query {
        context.check()?;
        if !value.is_finite() {
            return Err(RankingError::NonFinite);
        }
        query_norm += f64::from(value).powi(2);
    }
    if query_norm == 0.0 {
        return Err(RankingError::ZeroNorm);
    }
    let query_norm = query_norm.sqrt();
    let mut scores = Vec::new();
    scores
        .try_reserve_exact(documents.len())
        .map_err(|_| RankingError::Allocation)?;
    for document in documents {
        context.check()?;
        let (mut dot, mut norm) = (0.0, 0.0);
        for (&query_value, &value) in query.iter().zip(document) {
            context.check()?;
            if !value.is_finite() {
                return Err(RankingError::NonFinite);
            }
            let value = f64::from(value);
            dot += f64::from(query_value) * value;
            norm += value * value;
        }
        if norm == 0.0 {
            return Err(RankingError::ZeroNorm);
        }
        let score = dot / (query_norm * norm.sqrt());
        if !score.is_finite() {
            return Err(RankingError::NonFinite);
        }
        scores.push(score);
    }
    context.check()?;
    Ok(scores)
}
