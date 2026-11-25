//! Native SDK XPath matches addressing nodes in the same parsed syntax.

use super::{AntlrEngineError, Dialect, ParsedSyntax};
use graphit_core::file_io::SnapshotContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryMatch {
    pub query_index: usize,
    pub node_index: usize,
}

#[derive(Debug)]
pub struct ParsedQueries<'s> {
    pub syntax: ParsedSyntax<'s>,
    pub matches: Vec<QueryMatch>,
}

/// Parse once and select real nodes with the official ANTLR XPath elements.
///
/// Matches are unique per query/node and ordered by query then native preorder.
/// Invalid paths return a query error; recovery and origins remain in `syntax`.
pub fn parse_queries<'s>(
    dialect: Dialect,
    source: &'s str,
    paths: &[String],
    context: &SnapshotContext,
) -> Result<ParsedQueries<'s>, AntlrEngineError> {
    super::run(dialect, source, context, false, paths)
}
