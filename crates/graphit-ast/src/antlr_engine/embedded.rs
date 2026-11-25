//! Concrete PL/pgSQL body trees linked to their recovered PostgreSQL host.

use super::{AntlrEngineError, Dialect, ParsedSyntax, parse};
use crate::{
    BytePoint, RangeError, SourceText,
    tree_sitter_engine::{self as ts, CompiledQuery, GrammarLanguage, ParseControl, ParsedTree},
};
use graphit_core::file_io::SnapshotContext;
use std::{
    collections::BTreeMap, error::Error, fmt, ops::Range, sync::atomic::Ordering, time::Instant,
};

#[derive(Debug)]
pub enum EmbeddedError {
    Host(AntlrEngineError),
    TreeSitter(ts::EngineError),
    Source(RangeError),
}

impl fmt::Display for EmbeddedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(error) => error.fmt(f),
            Self::TreeSitter(error) => error.fmt(f),
            Self::Source(error) => error.fmt(f),
        }
    }
}

impl Error for EmbeddedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Host(error) => Some(error),
            Self::TreeSitter(error) => Some(error),
            Self::Source(error) => Some(error),
        }
    }
}

impl From<ts::EngineError> for EmbeddedError {
    fn from(error: ts::EngineError) -> Self {
        Self::TreeSitter(error)
    }
}
impl From<RangeError> for EmbeddedError {
    fn from(error: RangeError) -> Self {
        Self::Source(error)
    }
}

pub struct PostgreSqlSyntax<'a> {
    pub host: ParsedSyntax<'a>,
    pub bodies: Vec<PlPgSqlBody<'a>>,
}

/// Retains the native tree and grammar/library owner; borrows unchanged host bytes.
pub struct PlPgSqlBody<'a> {
    source: &'a str,
    pub span: Range<usize>,
    /// Index into `PostgreSqlSyntax::host.nodes` for this body's option-list rule.
    pub host_parent: usize,
    base: BytePoint,
    tree: ParsedTree,
}

impl PlPgSqlBody<'_> {
    pub fn tree(&self) -> &ParsedTree {
        &self.tree
    }

    fn project(&self, local: Range<usize>) -> Result<Range<usize>, EmbeddedError> {
        let start = self
            .span
            .start
            .checked_add(local.start)
            .ok_or(RangeError::OutOfBounds)?;
        let end = self
            .span
            .start
            .checked_add(local.end)
            .ok_or(RangeError::OutOfBounds)?;
        if end > self.span.end {
            return Err(RangeError::OutOfBounds.into());
        }
        if start > end {
            return Err(RangeError::ReversedRange.into());
        }
        if self.source.get(start..end).is_none() {
            return Err(RangeError::NotCharBoundary.into());
        }
        Ok(start..end)
    }

    fn point(&self, local: BytePoint) -> Result<BytePoint, EmbeddedError> {
        Ok(BytePoint {
            row: self
                .base
                .row
                .checked_add(local.row)
                .ok_or(RangeError::OutOfBounds)?,
            column: if local.row == 0 {
                self.base
                    .column
                    .checked_add(local.column)
                    .ok_or(RangeError::OutOfBounds)?
            } else {
                local.column
            },
        })
    }

    /// Body parent indexes remain local; a root's `None` links to `host_parent`.
    pub fn nodes(&self, control: ParseControl<'_>) -> Result<Vec<ts::SyntaxNode>, EmbeddedError> {
        self.nodes_checked(control, None)
    }

    /// Project native body nodes while preserving the original operation cause.
    pub fn nodes_with_context(
        &self,
        control: ParseControl<'_>,
        context: &SnapshotContext,
    ) -> Result<Vec<ts::SyntaxNode>, EmbeddedError> {
        self.nodes_checked(control, Some(context))
    }

    fn nodes_checked(
        &self,
        control: ParseControl<'_>,
        context: Option<&SnapshotContext>,
    ) -> Result<Vec<ts::SyntaxNode>, EmbeddedError> {
        let mut nodes = match context {
            Some(context) => self.tree.nodes_with_context(control, context)?,
            None => self.tree.nodes(control)?,
        };
        for node in &mut nodes {
            check_tree(control, context)?;
            node.range = self.project(node.range.clone())?;
            node.start = self.point(node.start)?;
            node.end = self.point(node.end)?;
        }
        check_tree(control, context)?;
        Ok(nodes)
    }

    /// Execute the actual caller-compiled query and project its captures to host bytes.
    pub fn matches(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
    ) -> Result<Vec<ts::QueryMatch>, EmbeddedError> {
        self.matches_checked(query, control, None)
    }

    /// Query and project body captures using the caller's original operation.
    pub fn matches_with_context(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
        context: &SnapshotContext,
    ) -> Result<Vec<ts::QueryMatch>, EmbeddedError> {
        self.matches_checked(query, control, Some(context))
    }

    fn matches_checked(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
        context: Option<&SnapshotContext>,
    ) -> Result<Vec<ts::QueryMatch>, EmbeddedError> {
        let mut matches = match context {
            Some(context) => self.tree.matches_with_context(query, control, context)?,
            None => self.tree.matches(query, control)?,
        };
        for found in &mut matches {
            check_tree(control, context)?;
            for capture in &mut found.captures {
                check_tree(control, context)?;
                capture.range = self.project(capture.range.clone())?;
                capture.start = self.point(capture.start)?;
                capture.end = self.point(capture.end)?;
            }
        }
        check_tree(control, context)?;
        Ok(matches)
    }
}

fn check_tree(
    control: ParseControl<'_>,
    context: Option<&SnapshotContext>,
) -> Result<(), EmbeddedError> {
    if let Some(context) = context {
        context.check().map_err(ts::EngineError::Operation)?;
    }
    check(control)
}

fn check_host(control: ParseControl<'_>, context: &SnapshotContext) -> Result<(), EmbeddedError> {
    context
        .check()
        .map_err(AntlrEngineError::Operation)
        .map_err(EmbeddedError::Host)?;
    check(control)
}

fn check(control: ParseControl<'_>) -> Result<(), EmbeddedError> {
    if control
        .cancellation
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        return Err(ts::EngineError::Cancelled.into());
    }
    if control
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        return Err(ts::EngineError::TimedOut.into());
    }
    Ok(())
}

/// Parse the host with recovery, then splice only its recognized dollar PL/pgSQL bodies.
/// The caller supplies the trusted, real PL/pgSQL grammar; no grammar is substituted.
pub fn parse_postgres_with_plpgsql<'a>(
    source: &'a str,
    language: &GrammarLanguage,
    control: ParseControl<'_>,
    context: &SnapshotContext,
) -> Result<PostgreSqlSyntax<'a>, EmbeddedError> {
    check_host(control, context)?;
    SourceText::new(source.as_bytes(), u32::MAX as usize)?;
    let host = parse(Dialect::PostgreSql, source, context).map_err(EmbeddedError::Host)?;
    let bodies = parse_plpgsql_bodies(&host, language, control, context)?;
    Ok(PostgreSqlSyntax { host, bodies })
}

/// Reuse a PostgreSQL host parsed under this same original operation.
/// The caller must supply the real PL/pgSQL grammar. Spans remain in host coordinates.
pub(crate) fn parse_plpgsql_bodies<'s>(
    host: &ParsedSyntax<'s>,
    language: &GrammarLanguage,
    control: ParseControl<'_>,
    context: &SnapshotContext,
) -> Result<Vec<PlPgSqlBody<'s>>, EmbeddedError> {
    check_host(control, context)?;
    let source = host.source;
    // Resolve every requested original body point in one UTF-8 pass. Per-node
    // projection below adds native local byte points without rescanning prefixes.
    let mut starts: BTreeMap<usize, BytePoint> = host
        .routine_bodies
        .iter()
        .filter_map(|body| {
            body.span
                .as_ref()
                .map(|span| (span.start, BytePoint { row: 0, column: 0 }))
        })
        .collect();
    let mut point = BytePoint { row: 0, column: 0 };
    for (byte, character) in source.char_indices() {
        check_host(control, context)?;
        if let Some(base) = starts.get_mut(&byte) {
            *base = point;
        }
        if character == '\n' {
            point.row += 1;
            point.column = 0;
        } else {
            point.column += character.len_utf8();
        }
    }
    if let Some(base) = starts.get_mut(&source.len()) {
        *base = point;
    }
    let mut bodies = Vec::with_capacity(host.embedded_bodies.len());
    for observation in &host.routine_bodies {
        check_host(control, context)?;
        let Some(span) = &observation.span else {
            continue;
        };
        if span.is_empty() {
            continue;
        }
        let body = source
            .get(span.clone())
            .ok_or(RangeError::NotCharBoundary)?;
        let host_parent = observation.host_parent;
        let tree = language.parse_with_context(body, control, context)?;
        bodies.push(PlPgSqlBody {
            source,
            span: span.clone(),
            host_parent,
            base: *starts.get(&span.start).ok_or(RangeError::OutOfBounds)?,
            tree,
        });
    }
    check_host(control, context)?;
    Ok(bodies)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plpgsql_fixture() -> crate::tree_sitter_engine::GrammarLanguage {
        crate::tree_sitter_engine::builtin("tree-sitter-plpgsql").unwrap()
    }

    #[test]
    fn existing_host_is_reused_for_plpgsql_bodies() {
        let language = plpgsql_fixture();
        let context = SnapshotContext::new(
            Instant::now() + std::time::Duration::from_secs(60),
            graphit_core::file_lock::Cancellation::default(),
        );
        let source = "-- α\r\nCREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $MiXeD$BEGIN RETURN; END;$MiXeD$; CREATE FUNCTION empty_body() RETURNS void LANGUAGE plpgsql AS $$$$; SELECT FROM;";
        let host = parse(Dialect::PostgreSql, source, &context).unwrap();
        let before = host.clone();
        assert!(!host.diagnostics.is_empty());
        assert!(
            host.routine_bodies
                .iter()
                .any(|observation| observation.span.as_ref().is_some_and(Range::is_empty))
        );
        let bodies =
            parse_plpgsql_bodies(&host, &language, ParseControl::default(), &context).unwrap();
        assert_eq!(host, before);
        assert_eq!(bodies.len(), 1);
        assert!(std::ptr::eq(
            bodies[0].source.as_ptr(),
            host.source.as_ptr()
        ));
        let observation = host
            .routine_bodies
            .iter()
            .find(|observation| {
                observation
                    .span
                    .as_ref()
                    .is_some_and(|span| !span.is_empty())
            })
            .unwrap();
        assert_eq!(bodies[0].host_parent, observation.host_parent);
        assert_eq!(
            host.nodes[bodies[0].host_parent].kind,
            "createfunc_opt_list"
        );
        assert_eq!(Some(bodies[0].span.clone()), observation.span);
        assert_eq!(
            bodies[0].tree().source(),
            &host.source[bodies[0].span.clone()]
        );
        context.cancellation().cancel();
        let expired = ParseControl {
            cancellation: None,
            deadline: Some(Instant::now()),
        };
        assert!(
            matches!(parse_plpgsql_bodies(&host, &language, expired, &context), Err(EmbeddedError::Host(AntlrEngineError::Operation(error))) if error.kind() == std::io::ErrorKind::Interrupted)
        );
    }
}
