//! Exact coordinate relations over borrowed source bytes.
//!
//! This module validates coordinate tables, not the truth of a transform,
//! filesystem authority or parser output.

use crate::{RangeError, SourceRange, SourceText};
use graphit_core::file_io::SnapshotContext;
use std::{error::Error, fmt, io::ErrorKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteSpan {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticReason {
    InsertedByTransform,
    RecoveryToken,
    Wrapper,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnmappableReason {
    PartialMappedAtom,
    MissingAnchor,
    RemovedPortion,
}

#[derive(Clone, Copy, Debug)]
pub enum OriginSegment<'p> {
    Direct {
        prepared: ByteSpan,
        original: ByteSpan,
    },
    MappedAtom {
        prepared: ByteSpan,
        original_fragments: &'p [ByteSpan],
    },
    Synthetic {
        prepared: ByteSpan,
        reason: SyntheticReason,
    },
}

impl OriginSegment<'_> {
    fn prepared(self) -> ByteSpan {
        match self {
            Self::Direct { prepared, .. }
            | Self::MappedAtom { prepared, .. }
            | Self::Synthetic { prepared, .. } => prepared,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OriginAnchor<'p> {
    pub prepared: u64,
    pub original_candidates: &'p [u64],
}

#[derive(Clone, Copy, Debug)]
pub struct OriginPlan<'p> {
    pub segments: &'p [OriginSegment<'p>],
    pub removed_original: &'p [ByteSpan],
    pub anchors: &'p [OriginAnchor<'p>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginError {
    Cancelled,
    DeadlineExceeded,
    ContextDenied,
    Range(RangeError),
    ArithmeticOverflow,
    InvalidPlan,
    Allocation,
}

impl fmt::Display for OriginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "source mapping cancelled",
            Self::DeadlineExceeded => "source mapping deadline exceeded",
            Self::ContextDenied => "source mapping context denied",
            Self::Range(_) => "source mapping range is invalid",
            Self::ArithmeticOverflow => "source mapping arithmetic overflow",
            Self::InvalidPlan => "source mapping plan is invalid",
            Self::Allocation => "source mapping allocation failed",
        })
    }
}

impl Error for OriginError {}

impl From<RangeError> for OriginError {
    fn from(value: RangeError) -> Self {
        Self::Range(value)
    }
}

pub enum OriginPiece<'o> {
    Original(SourceRange<'o>),
    Synthetic(SyntheticReason),
    Unmappable(UnmappableReason),
}

pub enum OriginalProjection<'o> {
    Exact(SourceRange<'o>),
    Compound(Vec<OriginPiece<'o>>),
    Synthetic(SyntheticReason),
    Unmappable(UnmappableReason),
    Ambiguous(Vec<SourceRange<'o>>),
}

#[derive(Debug, Eq, PartialEq)]
pub enum PreparedProjection {
    Unique(Vec<ByteSpan>),
    Ambiguous(Vec<Vec<ByteSpan>>),
    Removed,
    Unmappable(UnmappableReason),
}

/// Validated coordinate relations borrowing both sources, the origin plan and
/// the request context. Projections preserve original bytes and ambiguity.
pub struct PreparedInput<'o, 'v, 'p, 'c> {
    original_view: SourceText<'o>,
    prepared_view: SourceText<'v>,
    original_text: &'o str,
    prepared_text: &'v str,
    plan: OriginPlan<'p>,
    context: &'c SnapshotContext,
}

fn original_spans(plan: OriginPlan<'_>) -> impl Iterator<Item = ByteSpan> + '_ {
    plan.segments
        .iter()
        .flat_map(|segment| match segment {
            OriginSegment::Direct { original, .. } => std::slice::from_ref(original),
            OriginSegment::MappedAtom {
                original_fragments, ..
            } => original_fragments,
            OriginSegment::Synthetic { .. } => &[],
        })
        .copied()
}

fn to_usize(value: u64) -> Result<usize, OriginError> {
    usize::try_from(value).map_err(|_| OriginError::ArithmeticOverflow)
}

fn to_u64(value: usize) -> Result<u64, OriginError> {
    u64::try_from(value).map_err(|_| OriginError::ArithmeticOverflow)
}

fn translate(value: u64, from: u64, to: u64) -> Result<u64, OriginError> {
    value
        .checked_sub(from)
        .and_then(|offset| to.checked_add(offset))
        .ok_or(OriginError::ArithmeticOverflow)
}

fn check_context(context: &SnapshotContext) -> Result<(), OriginError> {
    context.check().map_err(|error| match error.kind() {
        ErrorKind::Interrupted => OriginError::Cancelled,
        ErrorKind::TimedOut => OriginError::DeadlineExceeded,
        _ => OriginError::ContextDenied,
    })
}

impl<'o, 'v, 'p, 'c> PreparedInput<'o, 'v, 'p, 'c> {
    pub fn new(
        original_bytes: &'o [u8],
        prepared_bytes: &'v [u8],
        plan: OriginPlan<'p>,
        original_context: &'c SnapshotContext,
    ) -> Result<Self, OriginError> {
        check_context(original_context)?;
        let original_view = SourceText::new(original_bytes, original_bytes.len().max(1))?;
        check_context(original_context)?;
        let prepared_view = SourceText::new(prepared_bytes, prepared_bytes.len().max(1))?;
        let original_text = original_view.range(0, original_bytes.len())?.text();
        let prepared_text = prepared_view.range(0, prepared_bytes.len())?.text();
        check_context(original_context)?;
        let input = Self {
            original_view,
            prepared_view,
            original_text,
            prepared_text,
            plan,
            context: original_context,
        };
        input.validate_plan()?;
        input.check()?;
        Ok(input)
    }

    pub fn original_text(&self) -> &'o str {
        self.original_text
    }

    pub fn prepared_text(&self) -> &'v str {
        self.prepared_text
    }

    /// Borrow the validated mapping for lossless native embedded composition.
    pub fn origin_plan(&self) -> OriginPlan<'p> {
        self.plan
    }

    /// Borrow the original request retained by this prepared input.
    pub fn original_context(&self) -> &'c SnapshotContext {
        self.context
    }

    fn check(&self) -> Result<(), OriginError> {
        check_context(self.context)
    }

    fn push<T>(&self, values: &mut Vec<T>, value: T) -> Result<(), OriginError> {
        self.check()?;
        values.try_reserve(1).map_err(|_| OriginError::Allocation)?;
        values.push(value);
        Ok(())
    }

    fn original_range(&self, span: ByteSpan) -> Result<SourceRange<'o>, OriginError> {
        self.check()?;
        if span.start > span.end {
            return Err(OriginError::Range(RangeError::ReversedRange));
        }
        let (start, end) = (to_usize(span.start)?, to_usize(span.end)?);

        let range = self.original_view.range(start, end)?;
        self.check()?;
        Ok(range)
    }

    fn prepared_range(&self, span: ByteSpan) -> Result<SourceRange<'v>, OriginError> {
        self.check()?;
        if span.start > span.end {
            return Err(OriginError::Range(RangeError::ReversedRange));
        }
        let (start, end) = (to_usize(span.start)?, to_usize(span.end)?);

        let range = self.prepared_view.range(start, end)?;
        self.check()?;
        Ok(range)
    }

    pub fn prepared_span(&self, start: u64, end: u64) -> Result<SourceRange<'v>, OriginError> {
        self.prepared_range(ByteSpan { start, end })
    }

    fn validate_plan(&self) -> Result<(), OriginError> {
        let plan = self.plan;
        let mut cursor = 0;
        for segment in plan.segments {
            self.check()?;
            let prepared = segment.prepared();
            self.prepared_range(prepared)?;
            if prepared.start != cursor || prepared.start == prepared.end {
                return Err(OriginError::InvalidPlan);
            }
            cursor = prepared.end;
            match segment {
                OriginSegment::Direct { original, .. } => {
                    self.original_range(*original)?;
                    if original.end - original.start != prepared.end - prepared.start {
                        return Err(OriginError::InvalidPlan);
                    }
                    let source = self
                        .original_text
                        .as_bytes()
                        .get(to_usize(original.start)?..to_usize(original.end)?)
                        .ok_or(OriginError::InvalidPlan)?;
                    let target = self
                        .prepared_text
                        .as_bytes()
                        .get(to_usize(prepared.start)?..to_usize(prepared.end)?)
                        .ok_or(OriginError::InvalidPlan)?;
                    for (left, right) in source.chunks(64 * 1024).zip(target.chunks(64 * 1024)) {
                        self.check()?;
                        if left != right {
                            return Err(OriginError::InvalidPlan);
                        }
                    }
                }
                OriginSegment::MappedAtom {
                    original_fragments, ..
                } => {
                    if original_fragments.is_empty() {
                        return Err(OriginError::InvalidPlan);
                    }
                    for span in *original_fragments {
                        self.check()?;
                        self.original_range(*span)?;
                        if span.start == span.end {
                            return Err(OriginError::InvalidPlan);
                        }
                    }
                }
                OriginSegment::Synthetic { .. } => {}
            }
        }
        if cursor != to_u64(self.prepared_text.len())? {
            return Err(OriginError::InvalidPlan);
        }
        let mut previous_end = 0;
        for span in plan.removed_original {
            self.check()?;
            self.original_range(*span)?;
            if span.start < previous_end || span.start == span.end {
                return Err(OriginError::InvalidPlan);
            }
            previous_end = span.end;
        }
        let mut previous_anchor = None;
        for anchor in plan.anchors {
            self.check()?;
            self.prepared_range(ByteSpan {
                start: anchor.prepared,
                end: anchor.prepared,
            })?;
            if previous_anchor.is_some_and(|point| anchor.prepared <= point)
                || anchor.original_candidates.is_empty()
            {
                return Err(OriginError::InvalidPlan);
            }
            previous_anchor = Some(anchor.prepared);
            for (index, point) in anchor.original_candidates.iter().enumerate() {
                self.check()?;
                self.original_range(ByteSpan {
                    start: *point,
                    end: *point,
                })?;
                for previous in &anchor.original_candidates[..index] {
                    self.check()?;
                    if previous == point {
                        return Err(OriginError::InvalidPlan);
                    }
                }
            }
            for segment in plan.segments {
                self.check()?;
                if let OriginSegment::Direct { prepared, original } = segment
                    && prepared.start < anchor.prepared
                    && anchor.prepared < prepared.end
                {
                    let point = translate(anchor.prepared, prepared.start, original.start)?;
                    if anchor.original_candidates != [point] {
                        return Err(OriginError::InvalidPlan);
                    }
                }
            }
        }
        self.validate_original_coverage()
    }

    fn validate_original_coverage(&self) -> Result<(), OriginError> {
        let plan = self.plan;
        let length = to_u64(self.original_text.len())?;
        let mut cursor = 0;
        while cursor < length {
            self.check()?;
            let mut next = length;
            let mut mapped = false;
            let mut removed = false;
            for span in original_spans(plan) {
                self.check()?;
                if span.start <= cursor && cursor < span.end {
                    mapped = true;
                }
                for endpoint in [span.start, span.end] {
                    if cursor < endpoint {
                        next = next.min(endpoint);
                    }
                }
            }
            for span in plan.removed_original {
                self.check()?;
                if span.start <= cursor && cursor < span.end {
                    removed = true;
                }
                for endpoint in [span.start, span.end] {
                    if cursor < endpoint {
                        next = next.min(endpoint);
                    }
                }
            }
            if mapped == removed || next <= cursor {
                return Err(OriginError::InvalidPlan);
            }
            cursor = next;
        }
        Ok(())
    }

    pub fn project_prepared(
        &self,
        start: u64,
        end: u64,
    ) -> Result<OriginalProjection<'o>, OriginError> {
        self.prepared_range(ByteSpan { start, end })?;
        if start == end {
            return self.project_empty(start);
        }
        let plan = self.plan;
        let mut pieces = Vec::new();
        for segment in plan.segments {
            self.check()?;
            let prepared = segment.prepared();
            let low = start.max(prepared.start);
            let high = end.min(prepared.end);
            if low >= high {
                continue;
            }
            match segment {
                OriginSegment::Direct { original, .. } => {
                    let range = self.original_range(ByteSpan {
                        start: translate(low, prepared.start, original.start)?,
                        end: translate(high, prepared.start, original.start)?,
                    })?;
                    self.push(&mut pieces, OriginPiece::Original(range))?;
                }
                OriginSegment::MappedAtom {
                    original_fragments, ..
                } => {
                    if low != prepared.start || high != prepared.end {
                        self.push(
                            &mut pieces,
                            OriginPiece::Unmappable(UnmappableReason::PartialMappedAtom),
                        )?;
                    } else {
                        for span in *original_fragments {
                            self.check()?;
                            let range = self.original_range(*span)?;
                            self.push(&mut pieces, OriginPiece::Original(range))?;
                        }
                    }
                }
                OriginSegment::Synthetic { reason, .. } => {
                    self.push(&mut pieces, OriginPiece::Synthetic(*reason))?;
                }
            }
        }
        self.check()?;
        if pieces.len() == 1 {
            match pieces.pop() {
                Some(OriginPiece::Original(range)) => Ok(OriginalProjection::Exact(range)),
                Some(OriginPiece::Synthetic(reason)) => Ok(OriginalProjection::Synthetic(reason)),
                Some(OriginPiece::Unmappable(reason)) => Ok(OriginalProjection::Unmappable(reason)),
                None => Err(OriginError::InvalidPlan),
            }
        } else if pieces.is_empty() {
            Err(OriginError::InvalidPlan)
        } else {
            Ok(OriginalProjection::Compound(pieces))
        }
    }

    fn project_empty(&self, point: u64) -> Result<OriginalProjection<'o>, OriginError> {
        let plan = self.plan;
        for segment in plan.segments {
            self.check()?;
            if let OriginSegment::Direct { prepared, original } = segment
                && prepared.start < point
                && point < prepared.end
            {
                let original_point = translate(point, prepared.start, original.start)?;
                let range = self.original_range(ByteSpan {
                    start: original_point,
                    end: original_point,
                })?;
                self.check()?;
                return Ok(OriginalProjection::Exact(range));
            }
        }
        for anchor in plan.anchors {
            self.check()?;
            if anchor.prepared == point {
                let mut candidates = Vec::new();
                for original in anchor.original_candidates {
                    self.check()?;
                    let range = self.original_range(ByteSpan {
                        start: *original,
                        end: *original,
                    })?;
                    self.push(&mut candidates, range)?;
                }
                self.check()?;
                if candidates.len() == 1 {
                    return candidates
                        .pop()
                        .map(OriginalProjection::Exact)
                        .ok_or(OriginError::InvalidPlan);
                }
                return Ok(OriginalProjection::Ambiguous(candidates));
            }
        }
        self.check()?;
        Ok(OriginalProjection::Unmappable(
            UnmappableReason::MissingAnchor,
        ))
    }

    pub fn inverse_original(
        &self,
        start: u64,
        end: u64,
    ) -> Result<PreparedProjection, OriginError> {
        self.original_range(ByteSpan { start, end })?;
        if start == end {
            return self.inverse_empty(start);
        }
        let plan = self.plan;
        let mut removed_bytes = 0u64;
        for removed in plan.removed_original {
            self.check()?;
            let low = start.max(removed.start);
            let high = end.min(removed.end);
            if low < high {
                removed_bytes = removed_bytes
                    .checked_add(high - low)
                    .ok_or(OriginError::ArithmeticOverflow)?;
            }
        }
        if removed_bytes != 0 {
            self.check()?;
            return Ok(if removed_bytes == end - start {
                PreparedProjection::Removed
            } else {
                PreparedProjection::Unmappable(UnmappableReason::RemovedPortion)
            });
        }
        self.inverse_paths(start, end)
    }

    fn inverse_empty(&self, point: u64) -> Result<PreparedProjection, OriginError> {
        let plan = self.plan;
        let mut candidates = Vec::new();
        for segment in plan.segments {
            self.check()?;
            if let OriginSegment::Direct { prepared, original } = segment
                && original.start < point
                && point < original.end
            {
                let prepared_point = translate(point, original.start, prepared.start)?;
                self.add_point_candidate(&mut candidates, prepared_point)?;
            }
        }
        for anchor in plan.anchors {
            self.check()?;
            for original in anchor.original_candidates {
                self.check()?;
                if *original == point {
                    self.add_point_candidate(&mut candidates, anchor.prepared)?;
                }
            }
        }
        self.check()?;
        if candidates.is_empty() {
            for removed in plan.removed_original {
                self.check()?;
                if removed.start < point && point < removed.end {
                    self.check()?;
                    return Ok(PreparedProjection::Removed);
                }
            }
            self.check()?;
            return Ok(PreparedProjection::Unmappable(
                UnmappableReason::MissingAnchor,
            ));
        }
        if candidates.len() == 1 {
            return candidates
                .pop()
                .map(PreparedProjection::Unique)
                .ok_or(OriginError::InvalidPlan);
        }
        Ok(PreparedProjection::Ambiguous(candidates))
    }

    fn add_point_candidate(
        &self,
        candidates: &mut Vec<Vec<ByteSpan>>,
        point: u64,
    ) -> Result<(), OriginError> {
        // Equal prepared positions are the same physical occurrence. This does
        // not merge or deduplicate nonempty forward origin fragments.
        let span = ByteSpan {
            start: point,
            end: point,
        };
        for candidate in candidates.iter() {
            self.check()?;
            if candidate.as_slice() == [span] {
                return Ok(());
            }
        }
        let mut path = Vec::new();
        self.push(&mut path, span)?;
        self.push(candidates, path)
    }

    fn next_original_boundary(&self, cursor: u64, end: u64) -> Result<u64, OriginError> {
        let plan = self.plan;
        let mut next = end;
        for original in original_spans(plan) {
            self.check()?;
            for endpoint in [original.start, original.end] {
                if cursor < endpoint {
                    next = next.min(endpoint);
                }
            }
        }
        Ok(next)
    }

    fn atom_interval(&self, fragments: &[ByteSpan]) -> Result<Option<ByteSpan>, OriginError> {
        let mut first = None;
        let mut end = 0;
        for span in fragments {
            self.check()?;
            if first.is_some() && span.start != end {
                return Ok(None);
            }
            first.get_or_insert(span.start);
            end = span.end;
        }
        Ok(first.map(|start| ByteSpan { start, end }))
    }

    fn clone_path(&self, path: &[ByteSpan]) -> Result<Vec<ByteSpan>, OriginError> {
        let mut result = Vec::new();
        for span in path {
            self.check()?;
            self.push(&mut result, *span)?;
        }
        Ok(result)
    }

    fn inverse_paths(&self, start: u64, end: u64) -> Result<PreparedProjection, OriginError> {
        struct State {
            cursor: u64,
            path: Vec<ByteSpan>,
        }
        let plan = self.plan;
        let mut frontier = Vec::new();
        self.push(
            &mut frontier,
            State {
                cursor: start,
                path: Vec::new(),
            },
        )?;
        let mut complete = Vec::new();
        while let Some(state) = frontier.pop() {
            self.check()?;
            if state.cursor == end {
                self.push(&mut complete, state.path)?;
                continue;
            }
            let boundary = self.next_original_boundary(state.cursor, end)?;
            for segment in plan.segments.iter().rev() {
                self.check()?;
                let edge = match segment {
                    OriginSegment::Direct { prepared, original }
                        if original.start <= state.cursor && state.cursor < original.end =>
                    {
                        let next = boundary.min(original.end);
                        Some((
                            next,
                            ByteSpan {
                                start: translate(state.cursor, original.start, prepared.start)?,
                                end: translate(next, original.start, prepared.start)?,
                            },
                        ))
                    }
                    OriginSegment::MappedAtom {
                        prepared,
                        original_fragments,
                    } => match self.atom_interval(original_fragments)? {
                        Some(original) if original.start == state.cursor && original.end <= end => {
                            Some((original.end, *prepared))
                        }
                        _ => None,
                    },
                    _ => None,
                };
                if let Some((cursor, span)) = edge {
                    if cursor <= state.cursor {
                        return Err(OriginError::InvalidPlan);
                    }
                    let mut path = self.clone_path(&state.path)?;
                    self.push(&mut path, span)?;
                    self.push(&mut frontier, State { cursor, path })?;
                }
            }
        }
        self.check()?;
        if complete.is_empty() {
            Ok(PreparedProjection::Unmappable(
                UnmappableReason::PartialMappedAtom,
            ))
        } else if complete.len() == 1 {
            complete
                .pop()
                .map(PreparedProjection::Unique)
                .ok_or(OriginError::InvalidPlan)
        } else {
            Ok(PreparedProjection::Ambiguous(complete))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coordinate_translation_checks_subtraction_and_addition() {
        assert_eq!(translate(1, 2, 0), Err(OriginError::ArithmeticOverflow));
        assert_eq!(
            translate(1, 0, u64::MAX),
            Err(OriginError::ArithmeticOverflow)
        );
        assert_eq!(translate(4, 2, 3), Ok(5));
    }
}
