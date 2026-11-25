//! Language preprocessing with explicit original-byte provenance.

use super::{AntlrEngineError, Dialect};
use crate::prepared_input::{ByteSpan, OriginAnchor, OriginPlan, OriginSegment, SyntheticReason};
use graphit_core::file_io::SnapshotContext;
use std::{collections::BTreeMap, ops::Range};

/// Prepared UTF-8 and the exact coordinate plan for its transform.
#[derive(Debug)]
pub struct PreparedSource {
    text: String,
    segments: Vec<OriginSegment<'static>>,
    removed: Vec<ByteSpan>,
    anchors: Vec<(u64, Vec<u64>)>,
    diagnostics: Vec<super::SyntaxDiagnostic>,
}

impl PreparedSource {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn diagnostics(&self) -> &[super::SyntaxDiagnostic] {
        &self.diagnostics
    }

    /// Consume the borrowed plan while its boundary-anchor storage is alive.
    /// Parser byte spans should be projected with `project_prepared`.
    pub fn with_origin_plan<T>(&self, consume: impl FnOnce(OriginPlan<'_>) -> T) -> T {
        let anchors: Vec<_> = self
            .anchors
            .iter()
            .map(|(prepared, candidates)| OriginAnchor {
                prepared: *prepared,
                original_candidates: candidates,
            })
            .collect();
        consume(OriginPlan {
            segments: &self.segments,
            removed_original: &self.removed,
            anchors: &anchors,
        })
    }
}

struct Builder<'s> {
    original: &'s str,
    text: String,
    segments: Vec<OriginSegment<'static>>,
}

impl<'s> Builder<'s> {
    fn new(original: &'s str) -> Self {
        Self {
            original,
            text: String::with_capacity(original.len()),
            segments: Vec::new(),
        }
    }

    fn copy(&mut self, original: Range<usize>) {
        if original.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(&self.original[original.clone()]);
        let prepared = ByteSpan {
            start: start as u64,
            end: self.text.len() as u64,
        };
        let original = ByteSpan {
            start: original.start as u64,
            end: original.end as u64,
        };
        if let Some(OriginSegment::Direct {
            prepared: previous,
            original: source,
        }) = self.segments.last_mut()
            && previous.end == prepared.start
            && source.end == original.start
        {
            previous.end = prepared.end;
            source.end = original.end;
            return;
        }
        self.segments
            .push(OriginSegment::Direct { prepared, original });
    }

    fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        let prepared = ByteSpan {
            start: start as u64,
            end: self.text.len() as u64,
        };
        if let Some(OriginSegment::Synthetic {
            prepared: previous, ..
        }) = self.segments.last_mut()
        {
            previous.end = prepared.end;
        } else {
            self.segments.push(OriginSegment::Synthetic {
                prepared,
                reason: SyntheticReason::InsertedByTransform,
            });
        }
    }

    fn finish(self) -> PreparedSource {
        let mut copied: Vec<_> = self
            .segments
            .iter()
            .filter_map(|segment| match segment {
                OriginSegment::Direct { original, .. } => Some(*original),
                _ => None,
            })
            .collect();
        copied.sort_by_key(|span| span.start);
        let mut removed = Vec::new();
        let mut cursor = 0;
        for span in copied {
            if cursor < span.start {
                removed.push(ByteSpan {
                    start: cursor,
                    end: span.start,
                });
            }
            cursor = span.end;
        }
        if cursor < self.original.len() as u64 {
            removed.push(ByteSpan {
                start: cursor,
                end: self.original.len() as u64,
            });
        }
        let mut anchors: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        anchors.entry(0).or_default().push(0);
        anchors
            .entry(self.text.len() as u64)
            .or_default()
            .push(self.original.len() as u64);
        for segment in &self.segments {
            if let OriginSegment::Direct { prepared, original } = segment {
                anchors
                    .entry(prepared.start)
                    .or_default()
                    .push(original.start);
                anchors.entry(prepared.end).or_default().push(original.end);
            }
        }
        for candidates in anchors.values_mut() {
            candidates.sort_unstable();
            candidates.dedup();
        }
        PreparedSource {
            text: self.text,
            segments: self.segments,
            removed,
            anchors: anchors.into_iter().collect(),
            diagnostics: Vec::new(),
        }
    }
}

/// Strip only exported materialized-view `USING (...)` metadata. Real lexer
/// tokens determine parentheses, quoted strings and statement boundaries.
/// Masking preserves line/byte positions, but replaced bytes are synthetic.
pub fn prepare_plsql(
    source: &str,
    context: &SnapshotContext,
) -> Result<PreparedSource, AntlrEngineError> {
    let lexical = super::lex(Dialect::PlSql, source, context)?;
    if !lexical.diagnostics.is_empty() {
        let mut builder = Builder::new(source);
        builder.copy(0..source.len());
        let mut prepared = builder.finish();
        prepared.diagnostics = lexical.diagnostics;
        return Ok(prepared);
    }
    let tokens: Vec<_> = lexical
        .tokens
        .iter()
        .filter(|token| token.channel == 0)
        .collect();
    let mut removed = Vec::new();
    let mut materialized_view = false;
    let mut index = 0;
    while index < tokens.len() {
        context.check()?;
        if tokens[index].kind.as_str() == "SEMICOLON" {
            materialized_view = false;
        }
        if tokens.get(index..index + 3).is_some_and(|prefix| {
            prefix[0].kind.as_str() == "CREATE"
                && prefix[1].kind.as_str() == "MATERIALIZED"
                && prefix[2].kind.as_str() == "VIEW"
        }) {
            materialized_view = true;
        }
        if materialized_view
            && index > 0
            && tokens[index - 1].kind.as_str() == "RIGHT_PAREN"
            && tokens[index].kind.as_str() == "USING"
            && tokens
                .get(index + 1)
                .is_some_and(|token| token.kind.as_str() == "LEFT_PAREN")
        {
            let mut depth = 0;
            let mut close = None;
            for (candidate, token) in tokens.iter().enumerate().skip(index + 1) {
                context.check()?;
                match token.kind.as_str() {
                    "LEFT_PAREN" => depth += 1,
                    "RIGHT_PAREN" => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(candidate);
                            break;
                        }
                    }
                    "SEMICOLON" => break,
                    _ => {}
                }
            }
            if let Some(close) = close
                && tokens
                    .get(close + 1)
                    .is_some_and(|token| matches!(token.kind.as_str(), "REFRESH" | "AS" | "BUILD"))
                && let (Some(start), Some(end)) = (
                    tokens[index].span.as_ref().map(|span| span.start),
                    tokens[close].span.as_ref().map(|span| span.end),
                )
            {
                removed.push(start..end);
                index = close;
            }
        }
        index += 1;
    }
    let mut builder = Builder::new(source);
    let mut cursor = 0;
    for range in removed {
        super::checked_span(source, Some(range.clone()))?;
        builder.copy(cursor..range.start);
        let mask: String = source[range.clone()]
            .bytes()
            .map(|byte| {
                if matches!(byte, b'\n' | b'\r') {
                    char::from(byte)
                } else {
                    ' '
                }
            })
            .collect();
        builder.insert(&mask);
        cursor = range.end;
    }
    builder.copy(cursor..source.len());
    context.check()?;
    Ok(builder.finish())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CobolFormat {
    Fixed,
    Variable,
    Tandem,
}

fn column(line: &str, index: usize) -> usize {
    line.char_indices()
        .nth(index)
        .map_or(line.len(), |(offset, _)| offset)
}

const COMMENT_ENTRIES: [&str; 6] = [
    "AUTHOR.",
    "INSTALLATION.",
    "DATE-WRITTEN.",
    "DATE-COMPILED.",
    "SECURITY.",
    "REMARKS.",
];

/// COBOL physical-line stages 1–4: columns, indicator comments/debug lines,
/// continuations, trailing separators and comment-entry escaping.
pub fn prepare_cobol(
    source: &str,
    format: CobolFormat,
    context: &SnapshotContext,
) -> Result<PreparedSource, AntlrEngineError> {
    let mut builder = Builder::new(source);
    let mut offset = 0;
    let mut previous_newline = None;
    let mut comment_entry = false;
    context.check()?;
    for (line_number, physical) in source.split_inclusive('\n').enumerate() {
        context.check()?;
        let line = physical.strip_suffix('\n').unwrap_or(physical);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let bom_bytes = if line_number == 0 && line.starts_with('\u{feff}') {
            3
        } else {
            0
        };
        let line = &line[bom_bytes..];
        let content_offset = offset + bom_bytes;
        let indicator_column = if format == CobolFormat::Tandem { 0 } else { 6 };
        if !line.trim().is_empty() && line.chars().count() <= indicator_column {
            return Err(AntlrEngineError::CobolLine {
                line: line_number + 1,
                indicator: None,
            });
        }
        let indicator = line.chars().nth(indicator_column).unwrap_or(' ');
        if !matches!(indicator, ' ' | '-' | '*' | '/' | 'D' | 'd') {
            return Err(AntlrEngineError::CobolLine {
                line: line_number + 1,
                indicator: Some(indicator),
            });
        }
        let continuation = indicator == '-';
        if line_number > 0 && !continuation {
            if let Some(newline) = previous_newline {
                builder.copy(newline);
            } else {
                builder.insert("\n");
            }
        }
        let begin = column(line, indicator_column + 1);
        let end = if format == CobolFormat::Fixed {
            column(line, 72)
        } else {
            line.len()
        };
        let content = line[begin..end].trim_end_matches([' ', '\t']);
        let mut begin = begin;
        let end = begin + content.len();
        if continuation {
            let trimmed = content.trim_start_matches([' ', '\t']);
            begin += content.len() - trimmed.len();
            if trimmed.starts_with(['\'', '"']) {
                begin += 1;
            }
            builder.copy(content_offset + begin..content_offset + end);
        } else {
            if format != CobolFormat::Tandem {
                builder.insert("      ");
            }
            let trimmed = content.trim_start_matches([' ', '\t']);
            let trigger = COMMENT_ENTRIES.iter().find(|trigger| {
                trimmed
                    .get(..trigger.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(trigger))
            });
            let terminates = [
                "PROGRAM-ID.",
                "ENVIRONMENT",
                "DATA.",
                "DATA DIVISION",
                "PROCEDURE",
            ]
            .iter()
            .any(|prefix| {
                trimmed
                    .get(..prefix.len())
                    .is_some_and(|text| text.eq_ignore_ascii_case(prefix))
            });
            let area_a_blank = content.chars().take(4).all(char::is_whitespace);
            if terminates || (!area_a_blank && trigger.is_none() && !matches!(indicator, '*' | '/'))
            {
                comment_entry = false;
            }
            if matches!(indicator, '*' | '/') {
                builder.insert("*> ");
            } else {
                builder.insert(" ");
            }
            if let Some(trigger) = trigger {
                let trigger_end = begin + content.len() - trimmed.len() + trigger.len();
                builder.copy(content_offset + begin..content_offset + trigger_end);
                builder.insert(" *>CE ");
                builder.copy(content_offset + trigger_end..content_offset + end);
                comment_entry = format != CobolFormat::Tandem;
            } else {
                if comment_entry {
                    builder.insert("*>CE ");
                }
                builder.copy(content_offset + begin..content_offset + end);
            }
        }
        if content.ends_with([',', ';']) {
            builder.insert(" ");
        }
        previous_newline = physical
            .ends_with('\n')
            .then(|| offset + physical.len() - 1..offset + physical.len());
        offset += physical.len();
    }
    if let Some(newline) = previous_newline {
        builder.copy(newline);
    }
    context.check()?;
    Ok(builder.finish())
}

/// Choose the maintained fixed/variable heuristic from the first 50 lines.
pub fn detect_cobol_format(source: &str) -> CobolFormat {
    let mut eligible = 0;
    let mut numeric = 0;
    for line in source
        .lines()
        .take(50)
        .filter(|line| line.chars().count() >= 7)
    {
        eligible += 1;
        if line
            .chars()
            .take(6)
            .all(|character| character.is_ascii_digit())
        {
            numeric += 1;
        }
    }
    if eligible == 0 || numeric * 10 > eligible * 3 {
        CobolFormat::Fixed
    } else {
        CobolFormat::Variable
    }
}
