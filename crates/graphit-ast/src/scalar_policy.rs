//! YAML Core 1.2 scalar kinds over the original UTF-8 input.
//!
//! Granit owns YAML syntax and decoded scalar text. The fixed regular expressions
//! implement the Core schema's scalar resolution table, without numeric conversion.
//! Admission bounds input before parser construction; event accounting is performed
//! after native emission and does not claim a hard native allocation limit.

use std::{borrow::Cow, fmt};

use regex::{RegexSet, RegexSetBuilder};
use serde_saphyr::granit_parser::{ErrorKind, Event, Parser, ScalarStyle, Tag};

use crate::SourceText;

const CORE_PATTERNS: [&str; 4] = [
    r"\A(?:|~|null|Null|NULL)\z",
    r"\A(?:true|True|TRUE|false|False|FALSE)\z",
    r"\A(?:[-+]?[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+)\z",
    r"\A(?:[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))\z",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarKind {
    String,
    Boolean,
    Null,
    Numeric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarPolicyErrorCode {
    InvalidLimits,
    InputTooLarge,
    InvalidUtf8,
    Parse,
    QuotaExceeded,
    UnsupportedTag,
    ForbiddenAnchorAlias,
    WrongScalarKind,
    InvalidScalar,
    InvalidSpan,
    MissingScalarSpan,
    AmbiguousScalarSpan,
    RegexPolicyBuild,
}

/// Diagnostics contain no source text, paths, or vendor error rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalarPolicyError {
    pub code: ScalarPolicyErrorCode,
    pub span: Option<OriginalByteSpan>,
}

impl fmt::Display for ScalarPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.code)
    }
}

impl std::error::Error for ScalarPolicyError {}

fn error(code: ScalarPolicyErrorCode, span: Option<OriginalByteSpan>) -> ScalarPolicyError {
    ScalarPolicyError { code, span }
}

/// Coordinates are data, not evidence of source identity or file authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalByteSpan {
    start: usize,
    end: usize,
}

impl OriginalByteSpan {
    pub fn new(start: usize, end: usize) -> Result<Self, ScalarPolicyError> {
        if start > end {
            return Err(error(ScalarPolicyErrorCode::InvalidSpan, None));
        }
        Ok(Self { start, end })
    }

    pub fn start(self) -> usize {
        self.start
    }

    pub fn end(self) -> usize {
        self.end
    }
}

/// Byte metadata from serde-saphyr's Location::span, never character lengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodecSpan {
    offset: u64,
    len: u64,
}

impl CodecSpan {
    pub fn new(offset: u64, len: u64) -> Self {
        Self { offset, len }
    }

    pub fn from_parts(offset: Option<u64>, len: Option<u64>) -> Result<Self, ScalarPolicyError> {
        match (offset, len) {
            (Some(offset), Some(len)) => Ok(Self::new(offset, len)),
            _ => Err(error(ScalarPolicyErrorCode::MissingScalarSpan, None)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RegexLimits {
    /// Estimated size policy for each NFA, not aggregate allocation.
    pub compiled_size_policy_bytes: usize,
    /// Approximate native DFA cache policy; eviction is not a public quota error.
    pub dfa_cache_policy_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ScalarLimits {
    input_bytes: usize,
    depth: usize,
    events: usize,
    scalar_records: usize,
    decoded_text_bytes: usize,
    directive_bytes: usize,
    reserved_directive_params: usize,
}

impl ScalarLimits {
    pub fn new(
        input_bytes: usize,
        depth: usize,
        events: usize,
        scalar_records: usize,
        decoded_text_bytes: usize,
        directive_bytes: usize,
        reserved_directive_params: usize,
    ) -> Result<Self, ScalarPolicyError> {
        if [
            input_bytes,
            depth,
            events,
            scalar_records,
            decoded_text_bytes,
            directive_bytes,
            reserved_directive_params,
        ]
        .contains(&0)
            || u32::try_from(input_bytes).is_err()
        {
            return Err(error(ScalarPolicyErrorCode::InvalidLimits, None));
        }
        Ok(Self {
            input_bytes,
            depth,
            events,
            scalar_records,
            decoded_text_bytes,
            directive_bytes,
            reserved_directive_params,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScalarUsage {
    pub input_bytes: usize,
    pub events: usize,
    pub scalar_records: usize,
    pub decoded_text_bytes: usize,
    pub max_depth: usize,
}

pub struct ScalarRecord<'a> {
    text: Cow<'a, str>,
    kind: ScalarKind,
    span: OriginalByteSpan,
}

impl ScalarRecord<'_> {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn kind(&self) -> ScalarKind {
        self.kind
    }

    pub fn span(&self) -> OriginalByteSpan {
        self.span
    }
}

/// Constructed only after the native stream completed successfully.
pub struct ScalarAudit<'a> {
    input: &'a [u8],
    text: &'a str,
    source: SourceText<'a>,
    records: Vec<ScalarRecord<'a>>,
    usage: ScalarUsage,
    codec_origin_delta: usize,
}

impl ScalarAudit<'_> {
    pub fn input(&self) -> &[u8] {
        self.input
    }

    pub fn records(&self) -> &[ScalarRecord<'_>] {
        &self.records
    }

    pub fn usage(&self) -> ScalarUsage {
        self.usage
    }

    /// The fixed string codec entry removes at most two leading BOMs.
    /// This translates its metadata; it never rewrites the admitted input.
    pub fn original_span(&self, span: CodecSpan) -> Result<OriginalByteSpan, ScalarPolicyError> {
        let start = usize::try_from(span.offset)
            .ok()
            .and_then(|offset| offset.checked_add(self.codec_origin_delta))
            .ok_or_else(|| error(ScalarPolicyErrorCode::InvalidSpan, None))?;
        let end = usize::try_from(span.len)
            .ok()
            .and_then(|len| start.checked_add(len))
            .ok_or_else(|| error(ScalarPolicyErrorCode::InvalidSpan, None))?;
        self.source
            .range(start, end)
            .map_err(|_| error(ScalarPolicyErrorCode::InvalidSpan, None))?;
        OriginalByteSpan::new(start, end)
    }

    fn record_at(&self, span: OriginalByteSpan) -> Result<&ScalarRecord<'_>, ScalarPolicyError> {
        if self.text.get(span.start..span.end).is_none() {
            return Err(error(ScalarPolicyErrorCode::InvalidSpan, None));
        }
        let mut matches = self.records.iter().filter(|record| record.span == span);
        let record = matches
            .next()
            .ok_or_else(|| error(ScalarPolicyErrorCode::MissingScalarSpan, Some(span)))?;
        if matches.next().is_some() {
            return Err(error(
                ScalarPolicyErrorCode::AmbiguousScalarSpan,
                Some(span),
            ));
        }
        Ok(record)
    }

    pub fn text_at(&self, span: OriginalByteSpan) -> Result<&str, ScalarPolicyError> {
        let record = self.record_at(span)?;
        if record.kind != ScalarKind::String {
            return Err(error(ScalarPolicyErrorCode::WrongScalarKind, Some(span)));
        }
        Ok(record.text())
    }

    pub fn bool_at(&self, span: OriginalByteSpan) -> Result<bool, ScalarPolicyError> {
        let record = self.record_at(span)?;
        if record.kind != ScalarKind::Boolean {
            return Err(error(ScalarPolicyErrorCode::WrongScalarKind, Some(span)));
        }
        Ok(matches!(record.text(), "true" | "True" | "TRUE"))
    }

    pub fn null_at(&self, span: OriginalByteSpan) -> Result<(), ScalarPolicyError> {
        let record = self.record_at(span)?;
        if record.kind != ScalarKind::Null {
            return Err(error(ScalarPolicyErrorCode::WrongScalarKind, Some(span)));
        }
        Ok(())
    }
}

pub struct ScalarPolicy {
    core: RegexSet,
}

impl ScalarPolicy {
    pub fn new(limits: RegexLimits) -> Result<Self, ScalarPolicyError> {
        if limits.compiled_size_policy_bytes == 0 || limits.dfa_cache_policy_bytes == 0 {
            return Err(error(ScalarPolicyErrorCode::InvalidLimits, None));
        }
        let core = RegexSetBuilder::new(CORE_PATTERNS)
            .size_limit(limits.compiled_size_policy_bytes)
            .dfa_size_limit(limits.dfa_cache_policy_bytes)
            .build()
            .map_err(|_| error(ScalarPolicyErrorCode::RegexPolicyBuild, None))?;
        Ok(Self { core })
    }

    fn classify(
        &self,
        text: &str,
        style: ScalarStyle,
        tag: Option<&Tag>,
        span: OriginalByteSpan,
    ) -> Result<ScalarKind, ScalarPolicyError> {
        let matches = self.core.matches(text);
        let implicit = if matches.matched(0) {
            ScalarKind::Null
        } else if matches.matched(1) {
            ScalarKind::Boolean
        } else if matches.matched(2) || matches.matched(3) {
            ScalarKind::Numeric
        } else {
            ScalarKind::String
        };
        if let Some(tag) = tag {
            if tag.parts() == ("", "!") || tag.core_suffix() == Some("str") {
                return Ok(ScalarKind::String);
            }
            let (kind, valid) = match tag.core_suffix() {
                Some("null") => (ScalarKind::Null, matches.matched(0)),
                Some("bool") => (ScalarKind::Boolean, matches.matched(1)),
                Some("int") => (ScalarKind::Numeric, matches.matched(2)),
                Some("float") => (ScalarKind::Numeric, matches.matched(3)),
                _ => return Err(error(ScalarPolicyErrorCode::UnsupportedTag, Some(span))),
            };
            if !valid {
                return Err(error(ScalarPolicyErrorCode::InvalidScalar, Some(span)));
            }
            return Ok(kind);
        }
        Ok(if style == ScalarStyle::Plain {
            implicit
        } else {
            ScalarKind::String
        })
    }

    pub fn audit<'a>(
        &self,
        input: &'a [u8],
        limits: &ScalarLimits,
    ) -> Result<ScalarAudit<'a>, ScalarPolicyError> {
        if input.len() > limits.input_bytes {
            return Err(error(ScalarPolicyErrorCode::InputTooLarge, None));
        }
        let text = std::str::from_utf8(input)
            .map_err(|_| error(ScalarPolicyErrorCode::InvalidUtf8, None))?;
        let source = SourceText::new(input, limits.input_bytes)
            .map_err(|_| error(ScalarPolicyErrorCode::InvalidSpan, None))?;
        let options = serde_saphyr::granit_parser::options! {
            emit_comments: false,
            simple_key_max_lookahead: limits.input_bytes,
            flow_nesting_limit: limits.depth,
            block_nesting_limit: limits.depth,
            max_directive_bytes: limits.directive_bytes,
            max_reserved_directive_params: limits.reserved_directive_params,
        };
        let mut parser = Parser::new_from_str_with_options(text, options);
        let mut records = Vec::new();
        let mut usage = ScalarUsage {
            input_bytes: input.len(),
            ..ScalarUsage::default()
        };
        let (mut depth, mut documents, mut complete) = (0usize, 0usize, false);
        while let Some(event) = parser.next_event() {
            let (event, native_span) = event.map_err(|cause| {
                let span = cause
                    .marker()
                    .byte_offset()
                    .filter(|&offset| text.get(offset..offset).is_some())
                    .map(|offset| OriginalByteSpan {
                        start: offset,
                        end: offset,
                    });
                let code = match cause.kind() {
                    ErrorKind::RecursionLimitExceeded
                    | ErrorKind::DirectiveByteLimitExceeded { .. }
                    | ErrorKind::TooManyReservedDirectiveParams { .. } => {
                        ScalarPolicyErrorCode::QuotaExceeded
                    }
                    _ => ScalarPolicyErrorCode::Parse,
                };
                error(code, span)
            })?;
            let span = native_span
                .byte_range()
                .and_then(|range| {
                    text.get(range.clone()).map(|_| OriginalByteSpan {
                        start: range.start,
                        end: range.end,
                    })
                })
                .ok_or_else(|| error(ScalarPolicyErrorCode::InvalidSpan, None))?;
            usage.events = charge(usage.events, 1, limits.events, span)?;
            if event.anchor_id().is_some() || event.alias_id().is_some() {
                return Err(error(
                    ScalarPolicyErrorCode::ForbiddenAnchorAlias,
                    Some(span),
                ));
            }
            match event {
                Event::DocumentStart(..) => {
                    documents = charge(documents, 1, 1, span)?;
                }
                Event::Scalar(value, style, _, tag) => {
                    usage.scalar_records =
                        charge(usage.scalar_records, 1, limits.scalar_records, span)?;
                    usage.decoded_text_bytes = charge(
                        usage.decoded_text_bytes,
                        value.len(),
                        limits.decoded_text_bytes,
                        span,
                    )?;
                    let kind = self.classify(&value, style, tag.as_deref(), span)?;
                    records.push(ScalarRecord {
                        text: value,
                        kind,
                        span,
                    });
                }
                Event::SequenceStart(_, _, tag) => {
                    collection_tag(tag.as_deref(), "seq", span)?;
                    depth = charge(depth, 1, limits.depth, span)?;
                    usage.max_depth = usage.max_depth.max(depth);
                }
                Event::MappingStart(_, _, tag) => {
                    collection_tag(tag.as_deref(), "map", span)?;
                    depth = charge(depth, 1, limits.depth, span)?;
                    usage.max_depth = usage.max_depth.max(depth);
                }
                Event::SequenceEnd | Event::MappingEnd => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| error(ScalarPolicyErrorCode::Parse, Some(span)))?;
                }
                Event::StreamEnd => {
                    complete = true;
                }
                Event::StreamStart | Event::DocumentEnd => {}
                _ => return Err(error(ScalarPolicyErrorCode::Parse, Some(span))),
            }
        }
        if !complete || depth != 0 {
            return Err(error(ScalarPolicyErrorCode::Parse, None));
        }
        let bom = b"\xef\xbb\xbf";
        let codec_origin_delta = if input.starts_with(bom) {
            if input.get(3..6) == Some(bom.as_slice()) {
                6
            } else {
                3
            }
        } else {
            0
        };
        Ok(ScalarAudit {
            input,
            text,
            source,
            records,
            usage,
            codec_origin_delta,
        })
    }
}

fn charge(
    current: usize,
    amount: usize,
    limit: usize,
    span: OriginalByteSpan,
) -> Result<usize, ScalarPolicyError> {
    current
        .checked_add(amount)
        .filter(|&next| next <= limit)
        .ok_or_else(|| error(ScalarPolicyErrorCode::QuotaExceeded, Some(span)))
}

fn collection_tag(
    tag: Option<&Tag>,
    expected: &str,
    span: OriginalByteSpan,
) -> Result<(), ScalarPolicyError> {
    if tag.is_some_and(|tag| tag.parts() != ("", "!") && tag.core_suffix() != Some(expected)) {
        return Err(error(ScalarPolicyErrorCode::UnsupportedTag, Some(span)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_metadata_never_selects_an_arbitrary_record() {
        let span = OriginalByteSpan::new(0, 1).unwrap();
        let audit = ScalarAudit {
            input: b"x",
            text: "x",
            source: SourceText::new(b"x", 1).unwrap(),
            codec_origin_delta: 0,
            usage: ScalarUsage::default(),
            records: vec![
                ScalarRecord {
                    text: Cow::Borrowed("x"),
                    kind: ScalarKind::String,
                    span,
                },
                ScalarRecord {
                    text: Cow::Borrowed("x"),
                    kind: ScalarKind::String,
                    span,
                },
            ],
        };
        assert_eq!(
            audit.text_at(span).err().unwrap().code,
            ScalarPolicyErrorCode::AmbiguousScalarSpan
        );
    }

    #[test]
    fn ledger_overflow_is_a_quota_error() {
        let span = OriginalByteSpan::new(0, 0).unwrap();
        assert_eq!(
            charge(usize::MAX, 1, usize::MAX, span).err().unwrap().code,
            ScalarPolicyErrorCode::QuotaExceeded
        );
    }
}
