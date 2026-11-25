//! Maintained ANTLR recognizers using the official C++ runtime and operation-local trees.

use crate::{
    RangeError, SourceText,
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
};
use graphit_core::file_io::SnapshotContext;
use std::{collections::BTreeMap, error::Error, fmt, io, ops::Range};

#[path = "antlr_engine/bridge.rs"]
mod bridge;
#[path = "antlr_engine/embedded.rs"]
pub(crate) mod embedded;
#[path = "antlr_engine/xpath.rs"]
mod xpath;
pub use xpath::{ParsedQueries, QueryMatch, parse_queries};
#[path = "antlr_engine/profile.rs"]
mod profile;
pub use profile::{
    CapturePath, CaptureStep, ParsedProfileQueries, PreparedProfileQueries, ProfileMatch,
    ProfileQuery, parse_profile_queries,
};
#[path = "antlr_engine/cobol_copy.rs"]
pub mod cobol_copy;
pub mod generic_model;
pub mod model;
#[path = "antlr_engine/preprocessing.rs"]
pub mod preprocessing;
#[path = "antlr_engine/semantic.rs"]
pub mod semantic;
use bridge::ffi;
pub use bridge::ffi::{DiagnosticPhase, UnindexedReason};
pub use embedded::{EmbeddedError, PlPgSqlBody, PostgreSqlSyntax, parse_postgres_with_plpgsql};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    PlSql,
    PostgreSql,
    Db2,
    TSql,
    Cobol85,
}
impl Dialect {
    pub const fn directory(self) -> &'static str {
        match self {
            Self::PlSql => "plsql",
            Self::PostgreSql => "postgresql",
            Self::Db2 => "db2",
            Self::TSql => "tsql",
            Self::Cobol85 => "cobol85",
        }
    }
    pub const fn entry_rule(self) -> &'static str {
        match self {
            Self::PlSql => "sql_script",
            Self::PostgreSql => "root",
            Self::Db2 => "db2_file",
            Self::TSql => "tsql_file",
            Self::Cobol85 => "startRule",
        }
    }
    fn native(self) -> ffi::NativeDialect {
        match self {
            Self::PlSql => ffi::NativeDialect::PlSql,
            Self::PostgreSql => ffi::NativeDialect::PostgreSql,
            Self::Db2 => ffi::NativeDialect::Db2,
            Self::TSql => ffi::NativeDialect::TSql,
            Self::Cobol85 => ffi::NativeDialect::Cobol85,
        }
    }
}

#[derive(Debug)]
pub enum AntlrEngineError {
    Operation(io::Error),
    NativeFailure {
        cause: cxx::Exception,
        control: Option<io::Error>,
    },
    QueryFailure {
        query_index: usize,
        cause: cxx::Exception,
        control: Option<io::Error>,
    },
    NativeContract {
        message: String,
        control: Option<io::Error>,
    },
    Source(RangeError),
    Syntax {
        diagnostics: Vec<SyntaxDiagnostic>,
    },
    InvalidSourceSpan {
        start: usize,
        end: usize,
    },
    CobolLine {
        line: usize,
        indicator: Option<char>,
    },
}
impl fmt::Display for AntlrEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => error.fmt(f),
            Self::NativeFailure { cause, control } => write!(
                f,
                "ANTLR native failure: {cause}; operation error: {control:?}"
            ),
            Self::QueryFailure {
                query_index,
                cause,
                control,
            } => write!(
                f,
                "ANTLR query {query_index} failed: {cause}; operation error: {control:?}"
            ),
            Self::NativeContract { message, control } => write!(
                f,
                "ANTLR boundary contract: {message}; operation error: {control:?}"
            ),
            Self::Source(error) => error.fmt(f),
            Self::Syntax { diagnostics } => write!(
                f,
                "ANTLR rejected recovered syntax ({} diagnostics)",
                diagnostics.len()
            ),
            Self::InvalidSourceSpan { start, end } => {
                write!(f, "ANTLR span {start}..{end} is outside UTF-8 source")
            }
            Self::CobolLine { line, indicator } => write!(
                f,
                "invalid COBOL physical line {line}, indicator {indicator:?}"
            ),
        }
    }
}
impl Error for AntlrEngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::NativeFailure { cause, .. } | Self::QueryFailure { cause, .. } => Some(cause),
            Self::NativeContract {
                control: Some(e), ..
            } => Some(e),
            Self::Source(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for AntlrEngineError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}
impl From<RangeError> for AntlrEngineError {
    fn from(error: RangeError) -> Self {
        Self::Source(error)
    }
}
fn contract(message: impl Into<String>) -> AntlrEngineError {
    AntlrEngineError::NativeContract {
        message: message.into(),
        control: None,
    }
}

pub(crate) struct OperationControl<'a> {
    context: &'a SnapshotContext,
    first_error: Option<io::Error>,
    query_index: Option<usize>,
}
impl OperationControl<'_> {
    fn check(&mut self) -> bool {
        if self.first_error.is_some() {
            return false;
        }
        match self.context.check() {
            Ok(()) => true,
            Err(error) => {
                self.first_error = Some(error);
                false
            }
        }
    }
}

/// Classification is retained even when there is no reliable source range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpanStatus {
    Regular,
    Eof,
    Unindexed(UnindexedReason),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxToken {
    pub kind: String,
    pub channel: u64,
    pub span: Option<Range<usize>>,
    pub status: SpanStatus,
    pub text: Option<String>,
    pub eof: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeClass {
    Rule,
    Terminal,
    Error,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxNode {
    pub kind: String,
    pub class: NodeClass,
    pub span: Option<Range<usize>>,
    pub status: SpanStatus,
    pub parent: Option<usize>,
    pub error: bool,
    pub missing: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxDiagnostic {
    pub grammar: String,
    pub phase: DiagnosticPhase,
    pub line: usize,
    pub column: usize,
    pub span: Option<Range<usize>>,
    pub status: SpanStatus,
    pub message: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutineBody {
    pub host_parent: usize,
    pub span: Option<Range<usize>>,
    pub status: SpanStatus,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedSyntax<'s> {
    source: &'s str,
    pub nodes: Vec<SyntaxNode>,
    pub tokens: Vec<SyntaxToken>,
    /// Recognized, nonempty dollar PL/pgSQL body ranges in prepared UTF-8 bytes.
    pub embedded_bodies: Vec<Range<usize>>,
    /// All recognized body observations, including unindexed recovery bounds.
    pub routine_bodies: Vec<RoutineBody>,
    pub diagnostics: Vec<SyntaxDiagnostic>,
}
pub struct ProjectedSyntax<'p, 'o> {
    pub syntax: &'p ParsedSyntax<'p>,
    pub nodes: Vec<Option<OriginalProjection<'o>>>,
    pub tokens: Vec<Option<OriginalProjection<'o>>>,
    pub diagnostics: Vec<Option<OriginalProjection<'o>>>,
    pub bodies: Vec<Option<OriginalProjection<'o>>>,
}
impl ParsedSyntax<'_> {
    pub(crate) fn validate_prepared(
        &self,
        input: &PreparedInput<'_, '_, '_, '_>,
    ) -> Result<(), OriginError> {
        if !std::ptr::eq(self.source.as_ptr(), input.prepared_text().as_ptr())
            || self.source.len() != input.prepared_text().len()
        {
            return Err(OriginError::InvalidPlan);
        }
        Ok(())
    }

    /// Bind this result to the actual prepared input and preserve every origin piece.
    pub fn project<'p, 'o, 'v, 'plan, 'ctx>(
        &'p self,
        input: &PreparedInput<'o, 'v, 'plan, 'ctx>,
    ) -> Result<ProjectedSyntax<'p, 'o>, OriginError> {
        self.validate_prepared(input)?;
        let project = |span: &Option<Range<usize>>| {
            span.as_ref()
                .map(|span| input.project_prepared(span.start as u64, span.end as u64))
                .transpose()
        };
        Ok(ProjectedSyntax {
            syntax: self,
            nodes: self
                .nodes
                .iter()
                .map(|node| project(&node.span))
                .collect::<Result<_, _>>()?,
            tokens: self
                .tokens
                .iter()
                .map(|token| project(&token.span))
                .collect::<Result<_, _>>()?,
            diagnostics: self
                .diagnostics
                .iter()
                .map(|diagnostic| project(&diagnostic.span))
                .collect::<Result<_, _>>()?,
            bodies: self
                .routine_bodies
                .iter()
                .map(|body| project(&body.span))
                .collect::<Result<_, _>>()?,
        })
    }
}
fn usize_value(value: u64) -> Result<usize, AntlrEngineError> {
    usize::try_from(value).map_err(|_| contract("native value exceeds usize"))
}
fn name(names: &[String], id: u64) -> Result<String, AntlrEngineError> {
    names
        .get(usize_value(id)?)
        .cloned()
        .ok_or_else(|| contract("native name ID exceeds its table"))
}

/// Map only requested scalar boundaries in one UTF-8 scan. No UTF-32 copy is made.
struct Boundaries<'a> {
    source: &'a str,
    bytes: BTreeMap<u64, usize>,
    count: u64,
}
impl<'a> Boundaries<'a> {
    fn new(
        source: &'a str,
        native: &ffi::NativeSyntax,
        context: &SnapshotContext,
    ) -> Result<Self, AntlrEngineError> {
        SourceText::new(source.as_bytes(), usize::MAX)?;
        let mut bytes = BTreeMap::new();
        for span in native
            .tokens
            .iter()
            .map(|v| &v.span)
            .chain(native.nodes.iter().map(|v| &v.span))
            .chain(native.diagnostics.iter().map(|v| &v.span))
            .chain(native.bodies.iter().map(|v| &v.span))
        {
            context.check()?;
            if span.kind != ffi::SpanKind::Unindexed {
                bytes.insert(span.start, 0);
                bytes.insert(span.end, 0);
            }
        }
        let prefix = usize::from(source.starts_with('\u{feff}'));
        let mut scalar = 0u64;
        for (physical_scalar, (byte, _)) in source.char_indices().enumerate() {
            context.check()?;
            if physical_scalar < prefix {
                continue;
            }
            if let Some(value) = bytes.get_mut(&scalar) {
                *value = byte;
            }
            scalar = scalar
                .checked_add(1)
                .ok_or_else(|| contract("scalar count overflow"))?;
        }
        if let Some(value) = bytes.get_mut(&scalar) {
            *value = source.len();
        }
        if scalar != native.stream_scalar_count || bytes.keys().any(|key| *key > scalar) {
            return Err(contract("SDK scalar count or bound differs from source"));
        }
        Ok(Self {
            source,
            bytes,
            count: scalar,
        })
    }
    fn span(
        &self,
        span: &ffi::NativeSpan,
    ) -> Result<(Option<Range<usize>>, SpanStatus), AntlrEngineError> {
        match span.kind {
            ffi::SpanKind::Unindexed => {
                if span.start != 0
                    || span.end != 0
                    || !matches!(
                        span.reason,
                        ffi::UnindexedReason::InvalidSdkIndex
                            | ffi::UnindexedReason::InsertedRecoveryToken
                            | ffi::UnindexedReason::MissingTokenBounds
                            | ffi::UnindexedReason::MissingRuleBoundary
                            | ffi::UnindexedReason::ReversedRuleBoundary
                            | ffi::UnindexedReason::MissingDiagnosticPosition
                    )
                {
                    return Err(contract("invalid unindexed span classification"));
                }
                Ok((None, SpanStatus::Unindexed(span.reason)))
            }
            ffi::SpanKind::RegularHalfOpen | ffi::SpanKind::Eof => {
                if span.reason != ffi::UnindexedReason::NotApplicable
                    || span.start > span.end
                    || span.end > self.count
                {
                    return Err(contract("invalid indexed span"));
                }
                let status = if span.kind == ffi::SpanKind::Eof {
                    if span.start != self.count || span.end != self.count {
                        return Err(contract("invalid EOF span"));
                    }
                    SpanStatus::Eof
                } else {
                    SpanStatus::Regular
                };
                let start = *self
                    .bytes
                    .get(&span.start)
                    .ok_or_else(|| contract("missing start boundary"))?;
                let end = *self
                    .bytes
                    .get(&span.end)
                    .ok_or_else(|| contract("missing end boundary"))?;
                if self.source.get(start..end).is_none() {
                    return Err(AntlrEngineError::InvalidSourceSpan { start, end });
                }
                Ok((Some(start..end), status))
            }
            _ => Err(contract("unknown span kind")),
        }
    }
}
pub(super) fn checked_span(
    source: &str,
    span: Option<Range<usize>>,
) -> Result<Option<Range<usize>>, AntlrEngineError> {
    if let Some(span) = &span
        && source.get(span.clone()).is_none()
    {
        return Err(AntlrEngineError::InvalidSourceSpan {
            start: span.start,
            end: span.end,
        });
    }
    Ok(span)
}

fn run<'s>(
    dialect: Dialect,
    source: &'s str,
    context: &SnapshotContext,
    lex_only: bool,
    paths: &[String],
) -> Result<ParsedQueries<'s>, AntlrEngineError> {
    run_native_selected(source, context, lex_only, paths, &[], |control| {
        if lex_only {
            ffi::lex(dialect.native(), source, control)
        } else if paths.is_empty() {
            ffi::recognize(dialect.native(), source, control)
        } else {
            ffi::recognize_queries(dialect.native(), source, paths, control)
        }
    })
    .map(|(parsed, _)| parsed)
}

type CapturedQueries<'s> = (ParsedQueries<'s>, Vec<Vec<Option<usize>>>);

fn run_native_selected<'s>(
    source: &'s str,
    context: &SnapshotContext,
    lex_only: bool,
    paths: &[String],
    profile_slots: &[usize],
    recognize: impl FnOnce(&mut OperationControl<'_>) -> Result<ffi::NativeSyntax, cxx::Exception>,
) -> Result<CapturedQueries<'s>, AntlrEngineError> {
    context.check()?;
    let mut control = OperationControl {
        context,
        first_error: None,
        query_index: None,
    };
    let native = recognize(&mut control);
    let native = native.map_err(|cause| match control.query_index {
        Some(query_index) => AntlrEngineError::QueryFailure {
            query_index,
            cause,
            control: control.first_error.take(),
        },
        None => AntlrEngineError::NativeFailure {
            cause,
            control: control.first_error.take(),
        },
    })?;
    match native.status {
        ffi::NativeStatus::Stopped => {
            if !native.tokens.is_empty()
                || !native.nodes.is_empty()
                || !native.diagnostics.is_empty()
                || !native.bodies.is_empty()
                || !native.query_matches.is_empty()
                || !native.grammar_names.is_empty()
                || !native.rule_names.is_empty()
                || !native.token_kind_names.is_empty()
                || native.stream_scalar_count != 0
            {
                return Err(contract("stopped operation returned partial output"));
            }
            return Err(match control.first_error.take() {
                Some(error) => AntlrEngineError::Operation(error),
                None => contract("stopped operation has no operation error"),
            });
        }
        ffi::NativeStatus::Success => {
            if control.first_error.is_some() {
                return Err(AntlrEngineError::NativeContract {
                    message: "native success after operation stopped".into(),
                    control: control.first_error.take(),
                });
            }
        }
        _ => return Err(contract("unknown native status")),
    }
    context.check()?;
    if lex_only
        && (!native.nodes.is_empty() || !native.bodies.is_empty() || !native.rule_names.is_empty())
    {
        return Err(contract("lex-only operation returned parser output"));
    }
    let bounds = Boundaries::new(source, &native, context)?;
    let mut result = ParsedSyntax {
        source,
        nodes: Vec::with_capacity(native.nodes.len()),
        tokens: Vec::with_capacity(native.tokens.len()),
        embedded_bodies: Vec::new(),
        routine_bodies: Vec::new(),
        diagnostics: Vec::new(),
    };
    for token in &native.tokens {
        context.check()?;
        let kind = name(&native.token_kind_names, token.kind_id)?;
        if token.eof != (kind == "EOF") {
            return Err(contract("EOF token identity disagrees with vocabulary"));
        }
        let (span, status) = bounds.span(&token.span)?;
        if status == SpanStatus::Eof && !token.eof {
            return Err(contract("non-EOF token has EOF span"));
        }
        result.tokens.push(SyntaxToken {
            kind,
            channel: token.channel,
            span,
            status,
            text: Some(token.text.clone()),
            eof: token.eof,
        });
    }
    for node in &native.nodes {
        context.check()?;
        let (kind, class, error) = match node.kind {
            ffi::NodeKind::Rule => (
                name(&native.rule_names, node.kind_id)?,
                NodeClass::Rule,
                false,
            ),
            ffi::NodeKind::Terminal => (
                name(&native.token_kind_names, node.kind_id)?,
                NodeClass::Terminal,
                false,
            ),
            ffi::NodeKind::Error => (
                name(&native.token_kind_names, node.kind_id)?,
                NodeClass::Error,
                true,
            ),
            _ => return Err(contract("unknown native node kind")),
        };
        let parent = if node.has_parent {
            let index = usize_value(node.parent)?;
            if index >= result.nodes.len() {
                return Err(contract("native parent is not an earlier node"));
            }
            Some(index)
        } else {
            if node.parent != 0 {
                return Err(contract("unused parent value is nonzero"));
            }
            None
        };
        let (span, status) = bounds.span(&node.span)?;
        result.nodes.push(SyntaxNode {
            kind,
            class,
            span,
            status,
            parent,
            error,
            missing: node.missing,
        });
    }
    for diagnostic in &native.diagnostics {
        context.check()?;
        if !matches!(
            diagnostic.phase,
            ffi::DiagnosticPhase::Lexer | ffi::DiagnosticPhase::Parser
        ) {
            return Err(contract("unknown diagnostic phase"));
        }
        let (span, status) = bounds.span(&diagnostic.span)?;
        result.diagnostics.push(SyntaxDiagnostic {
            grammar: name(&native.grammar_names, diagnostic.grammar_id)?,
            phase: diagnostic.phase,
            line: usize_value(diagnostic.line)?,
            column: usize_value(diagnostic.column)?,
            span,
            status,
            message: diagnostic.message.clone(),
        });
    }
    for body in &native.bodies {
        context.check()?;
        let host_parent = usize_value(body.host_parent)?;
        if result
            .nodes
            .get(host_parent)
            .is_none_or(|node| node.kind != "createfunc_opt_list")
        {
            return Err(contract("routine body parent is not its host option list"));
        }
        let (span, status) = bounds.span(&body.span)?;
        if let Some(span) = &span
            && !span.is_empty()
        {
            result.embedded_bodies.push(span.clone());
        }
        result.routine_bodies.push(RoutineBody {
            host_parent,
            span,
            status,
        });
    }
    let mut matches = Vec::with_capacity(native.query_matches.len());
    let mut captures = Vec::with_capacity(if profile_slots.is_empty() {
        0
    } else {
        native.query_matches.len()
    });
    let query_count = if profile_slots.is_empty() {
        paths.len()
    } else {
        profile_slots.len()
    };
    let mut previous = None;
    for item in &native.query_matches {
        context.check()?;
        let query_index = usize_value(item.query_index)?;
        let node_index = usize_value(item.node_index)?;
        let pair = (query_index, node_index);
        if query_index >= query_count
            || node_index >= result.nodes.len()
            || previous.is_some_and(|prior| prior >= pair)
        {
            return Err(contract("query matches are not unique ordered real nodes"));
        }
        previous = Some(pair);
        let capture_count = profile_slots.get(query_index).copied().unwrap_or(0);
        if item.captures.len() != capture_count {
            return Err(contract("profile match has incorrect capture arity"));
        }
        let mut captured = Vec::with_capacity(capture_count);
        for capture in &item.captures {
            context.check()?;
            let index = usize_value(capture.node_index)?;
            if capture.has_node {
                if index >= result.nodes.len() {
                    return Err(contract("capture is not a real node"));
                }
                captured.push(Some(index));
            } else {
                if index != 0 {
                    return Err(contract("absent capture has a nonzero index"));
                }
                captured.push(None);
            }
        }
        if !profile_slots.is_empty() {
            captures.push(captured);
        }
        matches.push(QueryMatch {
            query_index,
            node_index,
        });
    }
    context.check()?;
    Ok((
        ParsedQueries {
            syntax: result,
            matches,
        },
        captures,
    ))
}
/// Parse with the SDK's LL prediction and default recovery, retaining diagnostics.
pub fn parse<'s>(
    dialect: Dialect,
    source: &'s str,
    context: &SnapshotContext,
) -> Result<ParsedSyntax<'s>, AntlrEngineError> {
    run(dialect, source, context, false, &[]).map(|result| result.syntax)
}
/// Reject actual diagnostics, error nodes and SDK-inserted missing tokens.
pub fn parse_strict<'s>(
    dialect: Dialect,
    source: &'s str,
    context: &SnapshotContext,
) -> Result<ParsedSyntax<'s>, AntlrEngineError> {
    let syntax = parse(dialect, source, context)?;
    if !syntax.diagnostics.is_empty() || syntax.nodes.iter().any(|node| node.error || node.missing)
    {
        return Err(AntlrEngineError::Syntax {
            diagnostics: syntax.diagnostics,
        });
    }
    Ok(syntax)
}
pub(super) fn lex<'s>(
    dialect: Dialect,
    source: &'s str,
    context: &SnapshotContext,
) -> Result<ParsedSyntax<'s>, AntlrEngineError> {
    run(dialect, source, context, true, &[]).map(|result| result.syntax)
}
