//! Tree-sitter parsing and queries with retained grammar ownership.

use std::{
    error::Error,
    fmt,
    ops::{ControlFlow, Range},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use tree_sitter::{
    ParseOptions, Parser, Query, QueryCursor, QueryCursorOptions, StreamingIterator, Tree,
};

use crate::{BytePoint, RangeError, SourceRange, SourceText};

#[path = "tree_sitter_engine/owner.rs"]
mod owner;
pub use owner::GrammarLanguage;

#[allow(unsafe_code)]
#[path = "tree_sitter_engine/builtin.rs"]
mod builtin;
pub use builtin::{BUILTIN_GRAMMAR_NAMES, BuiltinGrammarError, builtin};

#[derive(Debug)]
pub enum EngineError {
    Operation(std::io::Error),
    Language(tree_sitter::LanguageError),
    Query(tree_sitter::QueryError),
    UnsupportedQuerySemantics(String),
    Input(RangeError),
    Cancelled,
    TimedOut,
    InvalidReadOffset,
    TreeUnavailable,
    GrammarMismatch,
    QueryMatchLimit,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::Language(e) => write!(f, "incompatible grammar: {e}"),
            Self::Query(e) => write!(f, "invalid query: {e}"),
            Self::UnsupportedQuerySemantics(op) => write!(f, "unsupported query semantics: {op}"),
            Self::Input(e) => e.fmt(f),
            Self::Cancelled => f.write_str("parsing cancelled"),
            Self::TimedOut => f.write_str("parsing deadline elapsed"),
            Self::InvalidReadOffset => f.write_str("parser requested an invalid source offset"),
            Self::TreeUnavailable => f.write_str("Tree-sitter did not return a tree"),
            Self::GrammarMismatch => {
                f.write_str("query and tree use different language generations")
            }
            Self::QueryMatchLimit => f.write_str("Tree-sitter query exceeded its match limit"),
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Language(e) => Some(e),
            Self::Query(e) => Some(e),
            Self::Input(e) => Some(e),
            _ => None,
        }
    }
}

/// Ordinary cooperative cancellation and deadline checks for synchronous parsing.
#[derive(Clone, Copy, Default)]
pub struct ParseControl<'a> {
    pub cancellation: Option<&'a AtomicBool>,
    pub deadline: Option<Instant>,
}

impl ParseControl<'_> {
    fn check(self) -> Result<(), EngineError> {
        if self
            .cancellation
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(EngineError::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(EngineError::TimedOut);
        }
        Ok(())
    }
}

/// A parsed document. The native tree drops before its grammar owner.
pub struct ParsedTree {
    tree: Tree,
    language: GrammarLanguage,
    source: Arc<str>,
}

/// A compiled query retains its exact language generation.
pub struct CompiledQuery {
    query: Query,
    language: GrammarLanguage,
}

impl CompiledQuery {
    pub(crate) fn capture_names(&self) -> &[&str] {
        self.query.capture_names()
    }

    pub(crate) fn required_single_captures(&self, names: &[&str]) -> bool {
        self.query.pattern_count() > 0
            && names.iter().all(|name| {
                self.query.capture_index_for_name(name).is_some_and(|id| {
                    (0..self.query.pattern_count()).all(|pattern| {
                        self.query.capture_quantifiers(pattern)[id as usize]
                            == tree_sitter::CaptureQuantifier::One
                    })
                })
            })
    }

    pub(crate) fn capture_index_for_name(&self, name: &str) -> Option<u32> {
        self.query.capture_index_for_name(name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxNode {
    /// Native SDK identity, meaningful only within this retained tree.
    pub node_id: usize,
    pub kind: String,
    pub range: Range<usize>,
    pub start: BytePoint,
    pub end: BytePoint,
    pub parent: Option<usize>,
    pub field: Option<String>,
    pub named: bool,
    pub error: bool,
    pub missing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Capture {
    /// Native SDK identity in the same retained tree as its query result.
    pub node_id: usize,
    pub name: String,
    pub text: String,
    pub range: Range<usize>,
    pub start: BytePoint,
    pub end: BytePoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertySetting {
    pub key: String,
    pub value: Option<String>,
    pub capture_id: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryMatch {
    pub pattern_index: usize,
    pub captures: Vec<Capture>,
    pub properties: Vec<PropertySetting>,
}

fn check(
    control: ParseControl<'_>,
    context: Option<&graphit_core::file_io::SnapshotContext>,
) -> Result<(), EngineError> {
    if let Some(context) = context {
        context.check().map_err(EngineError::Operation)?;
    }
    control.check()
}

fn point(point: tree_sitter::Point) -> BytePoint {
    BytePoint {
        row: point.row,
        column: point.column,
    }
}

impl GrammarLanguage {
    pub fn parse(
        &self,
        source: &str,
        control: ParseControl<'_>,
    ) -> Result<ParsedTree, EngineError> {
        self.parse_checked(source, control, None)
    }

    /// Preserve the original Core operation in the SDK progress callback.
    pub fn parse_with_context(
        &self,
        source: &str,
        control: ParseControl<'_>,
        context: &graphit_core::file_io::SnapshotContext,
    ) -> Result<ParsedTree, EngineError> {
        self.parse_checked(source, control, Some(context))
    }

    fn parse_checked(
        &self,
        source: &str,
        control: ParseControl<'_>,
        context: Option<&graphit_core::file_io::SnapshotContext>,
    ) -> Result<ParsedTree, EngineError> {
        let check = || check(control, context);
        check()?;
        // SDK offsets and input sizes are u32. Preserve UTF-8 bytes unchanged.
        SourceText::new(source.as_bytes(), u32::MAX as usize).map_err(EngineError::Input)?;
        let mut parser = Parser::new();
        parser
            .set_language(&self.0.language)
            .map_err(EngineError::Language)?;
        let mut stopped = None;
        let mut progress = |_: &tree_sitter::ParseState| match check() {
            Ok(()) => ControlFlow::Continue(()),
            Err(error) => {
                stopped = Some(error);
                ControlFlow::Break(())
            }
        };
        let mut invalid_offset = false;
        let mut read = |offset, _| match source.as_bytes().get(offset..) {
            Some(bytes) => bytes,
            None => {
                invalid_offset = true;
                &[]
            }
        };
        let tree = parser.parse_with_options(
            &mut read,
            None,
            Some(ParseOptions::new().progress_callback(&mut progress)),
        );
        if let Some(error) = stopped {
            return Err(error);
        }
        check()?;
        if invalid_offset {
            return Err(EngineError::InvalidReadOffset);
        }
        Ok(ParsedTree {
            tree: tree.ok_or(EngineError::TreeUnavailable)?,
            language: self.clone(),
            source: Arc::from(source),
        })
    }

    pub fn compile_query(&self, pattern: &str) -> Result<CompiledQuery, EngineError> {
        if pattern.len() > u32::MAX as usize {
            return Err(EngineError::Input(RangeError::InputTooLarge));
        }
        let query = Query::new(&self.0.language, pattern).map_err(EngineError::Query)?;
        for index in 0..query.pattern_count() {
            if let Some(predicate) = query.general_predicates(index).first() {
                return Err(EngineError::UnsupportedQuerySemantics(
                    predicate.operator.to_string(),
                ));
            }
            if let Some((property, _)) = query.property_predicates(index).first() {
                return Err(EngineError::UnsupportedQuerySemantics(
                    property.key.to_string(),
                ));
            }
        }
        Ok(CompiledQuery {
            query,
            language: self.clone(),
        })
    }

    pub(crate) fn context_kind_exists(
        &self,
        kind: &str,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> Result<bool, EngineError> {
        original.check().map_err(EngineError::Operation)?;
        let language = &self.0.language;
        let exists = [true, false].into_iter().any(|named| {
            let id = language.id_for_node_kind(kind, named);
            language.node_kind_for_id(id) == Some(kind)
                && language.node_kind_is_named(id) == named
                && language.node_kind_is_visible(id)
        });
        original.check().map_err(EngineError::Operation)?;
        Ok(exists)
    }
}

impl ParsedTree {
    pub(crate) fn root_node(&self) -> tree_sitter::Node<'_> {
        self.tree.root_node()
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn has_syntax_errors(&self) -> bool {
        self.tree.root_node().has_error()
    }

    /// Validate an SDK byte range against this document's retained original text.
    pub fn source_range(&self, range: Range<usize>) -> Result<SourceRange<'_>, RangeError> {
        SourceText::new(self.source.as_bytes(), u32::MAX as usize)?.range(range.start, range.end)
    }

    /// Flatten every node with parent indexes, field names and syntax diagnostics.
    pub fn nodes(&self, control: ParseControl<'_>) -> Result<Vec<SyntaxNode>, EngineError> {
        self.nodes_checked(control, None)
    }

    /// Walk this retained tree using the caller's original operation.
    pub fn nodes_with_context(
        &self,
        control: ParseControl<'_>,
        context: &graphit_core::file_io::SnapshotContext,
    ) -> Result<Vec<SyntaxNode>, EngineError> {
        self.nodes_checked(control, Some(context))
    }

    fn nodes_checked(
        &self,
        control: ParseControl<'_>,
        context: Option<&graphit_core::file_io::SnapshotContext>,
    ) -> Result<Vec<SyntaxNode>, EngineError> {
        check(control, context)?;
        let mut cursor = self.tree.walk();
        let mut nodes = Vec::new();
        let mut parents = Vec::new();
        loop {
            check(control, context)?;
            let node = cursor.node();
            let range = node.byte_range();
            self.source
                .get(range.clone())
                .ok_or(EngineError::Input(RangeError::NotCharBoundary))?;
            let index = nodes.len();
            nodes.push(SyntaxNode {
                node_id: node.id(),
                kind: node.kind().to_owned(),
                range,
                start: point(node.start_position()),
                end: point(node.end_position()),
                parent: parents.last().copied(),
                field: cursor.field_name().map(str::to_owned),
                named: node.is_named(),
                error: node.is_error(),
                missing: node.is_missing(),
            });
            if cursor.goto_first_child() {
                parents.push(index);
                continue;
            }
            loop {
                check(control, context)?;
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    check(control, context)?;
                    return Ok(nodes);
                }
                parents.pop();
            }
        }
    }

    pub fn matches(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
    ) -> Result<Vec<QueryMatch>, EngineError> {
        self.matches_checked(query, control, None)
    }

    /// Query this retained tree using the caller's original operation.
    pub fn matches_with_context(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
        context: &graphit_core::file_io::SnapshotContext,
    ) -> Result<Vec<QueryMatch>, EngineError> {
        self.matches_checked(query, control, Some(context))
    }

    fn matches_checked(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
        context: Option<&graphit_core::file_io::SnapshotContext>,
    ) -> Result<Vec<QueryMatch>, EngineError> {
        let mut output = Vec::new();
        self.visit_matches_checked(
            query,
            control,
            context,
            |found| -> Result<(), EngineError> {
                let mut captures = Vec::with_capacity(found.captures().len());
                for capture in found.captures() {
                    check(control, context)?;
                    let range = capture.node.byte_range();
                    let text = self
                        .source
                        .get(range.clone())
                        .ok_or(EngineError::Input(RangeError::NotCharBoundary))?;
                    captures.push(Capture {
                        node_id: capture.node.id(),
                        name: query.query.capture_names()[capture.index as usize].to_owned(),
                        text: text.to_owned(),
                        range,
                        start: point(capture.node.start_position()),
                        end: point(capture.node.end_position()),
                    });
                }
                let mut properties = Vec::new();
                for setting in query.query.property_settings(found.pattern_index) {
                    check(control, context)?;
                    properties.push(PropertySetting {
                        key: setting.key.to_string(),
                        value: setting.value.as_deref().map(str::to_owned),
                        capture_id: setting.capture_id,
                    });
                }
                output.push(QueryMatch {
                    pattern_index: found.pattern_index,
                    captures,
                    properties,
                });
                Ok(())
            },
        )?;
        Ok(output)
    }

    /// Visit native matches without copying the captured source text. All query
    /// consumers share grammar validation, cancellation and SDK limit handling.
    pub(crate) fn visit_matches_with_context<E: From<EngineError>>(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
        context: &graphit_core::file_io::SnapshotContext,
        visit: impl FnMut(&tree_sitter::QueryMatch<'_, '_>) -> Result<(), E>,
    ) -> Result<(), E> {
        self.visit_matches_checked(query, control, Some(context), visit)
    }

    fn visit_matches_checked<E: From<EngineError>>(
        &self,
        query: &CompiledQuery,
        control: ParseControl<'_>,
        context: Option<&graphit_core::file_io::SnapshotContext>,
        mut visit: impl FnMut(&tree_sitter::QueryMatch<'_, '_>) -> Result<(), E>,
    ) -> Result<(), E> {
        check(control, context)?;
        if !self.language.same_language(&query.language) {
            return Err(EngineError::GrammarMismatch.into());
        }
        let mut cursor = QueryCursor::new();
        let mut stopped = None;
        let mut progress = |_: &tree_sitter::QueryCursorState| match check(control, context) {
            Ok(()) => ControlFlow::Continue(()),
            Err(error) => {
                stopped = Some(error);
                ControlFlow::Break(())
            }
        };
        {
            let mut stream = cursor.matches_with_options(
                &query.query,
                self.tree.root_node(),
                self.source.as_bytes(),
                QueryCursorOptions::new().progress_callback(&mut progress),
            );
            while let Some(found) = stream.next() {
                check(control, context)?;
                visit(found)?;
            }
        }
        if let Some(error) = stopped {
            return Err(error.into());
        }
        check(control, context)?;
        if cursor.did_exceed_match_limit() {
            return Err(EngineError::QueryMatchLimit.into());
        }
        Ok(())
    }
}
