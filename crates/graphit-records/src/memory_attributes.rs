//! Pure attribute aggregation for a validated Memory consolidation plan.
//!
//! Inputs and outputs are metadata, not authority or proof of a complete merge.
//! The caller validates live members, coverage and preservation before using
//! this result, and enforces admission, fencing and publication separately.

use graphit_core::file_io::SnapshotContext;
use std::{fmt, io};

/// Memory classification; precedence is explicit rather than declaration order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryClass {
    Fact,
    Skill,
    Tension,
    Decision,
    Convention,
    Correction,
}

impl MemoryClass {
    const fn precedence(self) -> u8 {
        match self {
            Self::Fact => 0,
            Self::Skill => 1,
            Self::Tension => 2,
            Self::Decision => 3,
            Self::Convention => 4,
            Self::Correction => 5,
        }
    }
}

/// Immutable domain facts. Neither flag grants access or a mutation right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryAttributes {
    pub classification: MemoryClass,
    pub mandatory: bool,
    pub important: bool,
}

/// Aggregation errors carry no record identity or content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryAttributeError {
    EmptyInput,
    Request(io::ErrorKind),
}

impl fmt::Display for MemoryAttributeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("attribute sources must be nonempty"),
            Self::Request(kind) => write!(formatter, "attribute request: {kind:?}"),
        }
    }
}

impl std::error::Error for MemoryAttributeError {}

fn check_request(context: &SnapshotContext) -> Result<(), MemoryAttributeError> {
    context
        .check()
        .map_err(|error| MemoryAttributeError::Request(error.kind()))
}

/// OR flags independently and retain the strongest classification.
///
/// Runs iteratively in O(N) time and O(1) memory without IO or allocation by the
/// fold itself. Core may allocate when constructing a cancellation error. Checks
/// the supplied original context at entry, for each source, and before
/// returning success. Request failure at entry precedes empty-input errors.
/// This does not select a survivor or validate/publish a storage merge.
pub fn merge_attributes(
    sources: &[MemoryAttributes],
    context: &SnapshotContext,
) -> Result<MemoryAttributes, MemoryAttributeError> {
    check_request(context)?;
    let mut merged = *sources.first().ok_or(MemoryAttributeError::EmptyInput)?;
    for source in sources {
        check_request(context)?;
        merged.mandatory |= source.mandatory;
        merged.important |= source.important;
        if source.classification.precedence() > merged.classification.precedence() {
            merged.classification = source.classification;
        }
    }
    check_request(context)?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    const CLASSES: [MemoryClass; 6] = [
        MemoryClass::Fact,
        MemoryClass::Skill,
        MemoryClass::Tension,
        MemoryClass::Decision,
        MemoryClass::Convention,
        MemoryClass::Correction,
    ];

    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        )
    }

    fn attributes(classification: MemoryClass, flags: u8) -> MemoryAttributes {
        MemoryAttributes {
            classification,
            mandatory: flags & 1 != 0,
            important: flags & 2 != 0,
        }
    }

    #[test]
    fn all_flag_pairs_are_independent_and_inputs_are_unchanged() {
        let context = context();
        for left in 0..4 {
            for right in 0..4 {
                let sources = [
                    attributes(MemoryClass::Fact, left),
                    attributes(MemoryClass::Fact, right),
                ];
                let original = sources;
                assert_eq!(
                    merge_attributes(&sources, &context),
                    Ok(attributes(MemoryClass::Fact, left | right))
                );
                assert_eq!(sources, original);
            }
        }
    }

    #[test]
    fn all_class_pairs_follow_precedence_in_both_orders() {
        let context = context();
        for (left_index, left) in CLASSES.iter().enumerate() {
            for (right_index, right) in CLASSES.iter().enumerate() {
                let sources = [attributes(*left, 0), attributes(*right, 0)];
                let expected = attributes(CLASSES[left_index.max(right_index)], 0);
                assert_eq!(merge_attributes(&sources, &context), Ok(expected));
                assert_eq!(
                    merge_attributes(&[sources[1], sources[0]], &context),
                    Ok(expected)
                );
            }
        }
    }

    #[test]
    fn six_class_permutations_preserve_separately_sourced_flags() {
        let context = context();
        let base = CLASSES.map(|class| match class {
            MemoryClass::Fact => attributes(class, 1),
            MemoryClass::Skill => attributes(class, 2),
            _ => attributes(class, 0),
        });
        let mut order = [0, 1, 2, 3, 4, 5];
        let mut permutations = 0;
        loop {
            let sources = order.map(|index| base[index]);
            assert_eq!(
                merge_attributes(&sources, &context),
                Ok(attributes(MemoryClass::Correction, 3))
            );
            permutations += 1;
            let Some(pivot) = (0..5).rev().find(|index| order[*index] < order[*index + 1]) else {
                break;
            };
            let successor = (pivot + 1..6)
                .rev()
                .find(|index| order[*index] > order[pivot])
                .unwrap();
            order.swap(pivot, successor);
            order[pivot + 1..].reverse();
        }
        assert_eq!(permutations, 720);
    }

    #[test]
    fn repeated_sources_are_idempotent() {
        let context = context();
        for class in CLASSES {
            for flags in 0..4 {
                let source = attributes(class, flags);
                assert_eq!(merge_attributes(&[source, source], &context), Ok(source));
            }
        }
    }

    #[test]
    fn empty_input_is_rejected() {
        let context = context();
        assert_eq!(
            merge_attributes(&[], &context),
            Err(MemoryAttributeError::EmptyInput)
        );
    }

    #[test]
    fn original_cancelled_and_expired_requests_take_precedence_without_io() {
        let cancelled = context();
        cancelled.cancellation().cancel();
        assert_eq!(
            merge_attributes(&[], &cancelled),
            Err(MemoryAttributeError::Request(io::ErrorKind::Interrupted))
        );
        let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
        assert_eq!(
            merge_attributes(&[], &expired),
            Err(MemoryAttributeError::Request(io::ErrorKind::TimedOut))
        );
    }

    #[test]
    fn long_input_is_iterative() {
        let context = context();
        let source = attributes(MemoryClass::Convention, 3);
        let sources = vec![source; 10_000];
        assert_eq!(merge_attributes(&sources, &context), Ok(source));
    }

    #[test]
    fn original_shared_request_preserves_cancellation() {
        let original = std::sync::Arc::new(context());
        let shared = std::sync::Arc::clone(&original);
        let source = attributes(MemoryClass::Correction, 3);
        assert_eq!(merge_attributes(&[source], &shared), Ok(source));
        original.cancellation().cancel();
        assert_eq!(
            merge_attributes(&[source], &shared),
            Err(MemoryAttributeError::Request(io::ErrorKind::Interrupted))
        );
    }
}
