//! Exact cosine scores for persisted variable-width Float32 vectors.
#![doc = include_str!("../../../docs/specs/rust_vector_distance.md")]

use arrow_array::{Array, Float32Array, ListArray};
use lance_linalg::distance::cosine::cosine_distance;
use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VectorDistanceError {
    EmptyQuery,
    NonFiniteQuery {
        element: usize,
    },
    VectorType,
    Dimensions {
        row: usize,
        expected: usize,
        actual: usize,
    },
    NullElement {
        row: usize,
        element: usize,
    },
    NonFiniteVector {
        row: usize,
        element: usize,
    },
    NonFiniteDistance {
        row: usize,
    },
}
impl fmt::Display for VectorDistanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Vector distance: {self:?}")
    }
}
impl Error for VectorDistanceError {}

/// Scores are aligned with input rows; a null list returns `None`.
///
/// No score is returned if any non-null vector is invalid. Caller identity,
/// source eligibility, complete scanning and result selection remain outside
/// this borrowed batch operation.
pub fn cosine_distances(
    vectors: &ListArray,
    query: &[f32],
) -> Result<Vec<Option<f32>>, VectorDistanceError> {
    if query.is_empty() {
        return Err(VectorDistanceError::EmptyQuery);
    }
    if let Some(element) = query.iter().position(|value| !value.is_finite()) {
        return Err(VectorDistanceError::NonFiniteQuery { element });
    }
    let values = vectors
        .values()
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or(VectorDistanceError::VectorType)?;
    let offsets = vectors.value_offsets();
    let mut scores = Vec::with_capacity(vectors.len());
    for row in 0..vectors.len() {
        if vectors.is_null(row) {
            scores.push(None);
            continue;
        }
        // Arrow's safe ListArray constructors validate these nonnegative offsets.
        let start = offsets[row] as usize;
        let end = offsets[row + 1] as usize;
        let actual = end - start;
        if actual != query.len() {
            return Err(VectorDistanceError::Dimensions {
                row,
                expected: query.len(),
                actual,
            });
        }
        for index in start..end {
            let element = index - start;
            if values.is_null(index) {
                return Err(VectorDistanceError::NullElement { row, element });
            }
            if !values.value(index).is_finite() {
                return Err(VectorDistanceError::NonFiniteVector { row, element });
            }
        }
        let distance = cosine_distance(query, &values.values()[start..end]);
        if !distance.is_finite() {
            return Err(VectorDistanceError::NonFiniteDistance { row });
        }
        scores.push(Some(distance));
    }
    Ok(scores)
}
