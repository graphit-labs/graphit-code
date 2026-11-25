//! Prepared official XPath selectors and small capture operations over native trees.

use super::{AntlrEngineError, Dialect, OperationControl, ParsedSyntax, bridge::ffi};
use graphit_core::file_io::SnapshotContext;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureStep {
    /// Official ANTLR XPath, relative to the current capture node.
    XPath(String),
    /// First rule in breadth-first child order; excludes the current node.
    NearestDescendant(String),
    /// Nearest matching parent; excludes the current node and never retries.
    NearestAncestor(String),
}
/// Empty steps select the matched node itself.
pub type CapturePath = Vec<CaptureStep>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProfileQuery {
    /// Official ANTLR XPath selecting the guard anchor, or the result directly.
    pub pattern: String,
    /// Optional official XPath evaluated from each accepted anchor.
    pub select: String,
    /// Case-insensitive ASCII text of a direct terminal child.
    pub keyword: String,
    /// A direct child rule; mutually exclusive with `keyword`.
    pub child_rule: String,
    pub negate: bool,
    pub captures: Vec<CapturePath>,
}

/// Grammar-bound, immutable selectors. No parser, input, or tree is retained.
/// The native plan owns only rule/token IDs, strings and official XPath elements.
pub struct PreparedProfileQueries {
    dialect: Dialect,
    preprocessor: bool,
    slots: Vec<usize>,
    native: cxx::UniquePtr<ffi::PreparedProfileQueries>,
}
// Official direct-child XPath elements only read their bound IDs/inversion when
// evaluating. Per-operation traversal, roots, results and cancellation are local.
#[allow(unsafe_code)]
unsafe impl Send for PreparedProfileQueries {}
#[allow(unsafe_code)]
unsafe impl Sync for PreparedProfileQueries {}
impl std::fmt::Debug for PreparedProfileQueries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedProfileQueries")
            .field("dialect", &self.dialect)
            .field("preprocessor", &self.preprocessor)
            .field("queries", &self.slots.len())
            .finish()
    }
}
impl PreparedProfileQueries {
    pub fn new(
        dialect: Dialect,
        preprocessor: bool,
        queries: &[ProfileQuery],
        context: &SnapshotContext,
    ) -> Result<Self, AntlrEngineError> {
        let queries = native_queries(queries, context)?;
        let mut control = OperationControl {
            context,
            first_error: None,
            query_index: None,
        };
        let native =
            ffi::prepare_profile_queries(dialect.native(), preprocessor, &queries, &mut control)
                .map_err(|cause| {
                    if let Some(error) = control.first_error.take() {
                        AntlrEngineError::Operation(error)
                    } else if let Some(query_index) = control.query_index {
                        AntlrEngineError::QueryFailure {
                            query_index,
                            cause,
                            control: None,
                        }
                    } else {
                        AntlrEngineError::NativeFailure {
                            cause,
                            control: None,
                        }
                    }
                })?;
        context.check()?;
        Ok(Self {
            dialect,
            preprocessor,
            slots: queries.iter().map(|query| query.captures.len()).collect(),
            native,
        })
    }
    pub fn parse<'s>(
        &self,
        source: &'s str,
        context: &SnapshotContext,
    ) -> Result<ParsedProfileQueries<'s>, AntlrEngineError> {
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| super::contract("missing prepared profile"))?;
        let (parsed, captures) =
            super::run_native_selected(source, context, false, &[], &self.slots, |control| {
                ffi::recognize_prepared_profile_queries(
                    self.dialect.native(),
                    self.preprocessor,
                    source,
                    native,
                    control,
                )
            })?;
        profile_matches(parsed, captures, context)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ProfileMatch {
    pub query_index: usize,
    pub node_index: usize,
    pub captures: Vec<Option<usize>>,
}
#[derive(Debug)]
pub struct ParsedProfileQueries<'s> {
    pub syntax: ParsedSyntax<'s>,
    pub matches: Vec<ProfileMatch>,
}
/// Prepare selectors against the grammar, parse once and return ordered captures.
/// Reuse `PreparedProfileQueries` when selecting a profile over multiple files.
pub fn parse_profile_queries<'s>(
    dialect: Dialect,
    source: &'s str,
    queries: &[ProfileQuery],
    context: &SnapshotContext,
) -> Result<ParsedProfileQueries<'s>, AntlrEngineError> {
    PreparedProfileQueries::new(dialect, false, queries, context)?.parse(source, context)
}
fn native_queries(
    queries: &[ProfileQuery],
    context: &SnapshotContext,
) -> Result<Vec<ffi::NativeProfileQuery>, AntlrEngineError> {
    context.check()?;
    let mut result = Vec::with_capacity(queries.len());
    for query in queries {
        context.check()?;
        let mut captures = Vec::with_capacity(query.captures.len());
        for path in &query.captures {
            let mut steps = Vec::with_capacity(path.len());
            for step in path {
                context.check()?;
                let (xpath, nearest_descendant, nearest_ancestor) = match step {
                    CaptureStep::XPath(text) => (text.clone(), String::new(), String::new()),
                    CaptureStep::NearestDescendant(text) => {
                        (String::new(), text.clone(), String::new())
                    }
                    CaptureStep::NearestAncestor(text) => {
                        (String::new(), String::new(), text.clone())
                    }
                };
                steps.push(ffi::NativeCaptureStep {
                    xpath,
                    nearest_descendant,
                    nearest_ancestor,
                });
            }
            captures.push(ffi::NativeCapturePath { steps });
        }
        result.push(ffi::NativeProfileQuery {
            pattern: query.pattern.clone(),
            select: query.select.clone(),
            keyword: query.keyword.clone(),
            child_rule: query.child_rule.clone(),
            negate: query.negate,
            captures,
        });
    }
    Ok(result)
}
fn profile_matches<'s>(
    parsed: super::ParsedQueries<'s>,
    captures: Vec<Vec<Option<usize>>>,
    context: &SnapshotContext,
) -> Result<ParsedProfileQueries<'s>, AntlrEngineError> {
    let mut matches = Vec::with_capacity(parsed.matches.len());
    for (item, captures) in parsed.matches.into_iter().zip(captures) {
        context.check()?;
        matches.push(ProfileMatch {
            query_index: item.query_index,
            node_index: item.node_index,
            captures,
        });
    }
    context.check()?;
    Ok(ParsedProfileQueries {
        syntax: parsed.syntax,
        matches,
    })
}
