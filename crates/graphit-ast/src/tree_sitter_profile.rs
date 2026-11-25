//! Raw effective-profile query observations over the actual prepared input.

use std::{
    error::Error,
    fmt, io,
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

use graphit_core::file_io::SnapshotContext;

use crate::{
    SourceRange,
    antlr_engine::{EmbeddedError, ParsedSyntax, PlPgSqlBody, embedded::parse_plpgsql_bodies},
    catalog::{Effective, EffectiveProfile, ProvenanceView, Query},
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
    profiles::{ProfileParser, QuerySource},
    tree_sitter_engine::{
        Capture, CompiledQuery, EngineError, GrammarLanguage, ParseControl, ParsedTree,
        PropertySetting, SyntaxNode,
    },
};

#[derive(Debug)]
pub enum TreeSitterProfileError {
    Operation(io::Error),
    ParserMismatch,
    UnsupportedQuerySource {
        query_index: usize,
        source: QuerySource,
    },
    EmbeddedProfileMismatch,
    UnsupportedAntlrQueryFields {
        query_index: usize,
    },
    UnsupportedAntlrContexts,
    Engine(EngineError),
    Origin(OriginError),
    Embedded(EmbeddedError),
}

impl fmt::Display for TreeSitterProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => error.fmt(formatter),
            Self::ParserMismatch => formatter.write_str("profile does not select Tree-sitter"),
            Self::UnsupportedQuerySource {
                query_index,
                source,
            } => write!(
                formatter,
                "query {query_index} source {source:?} cannot use Tree-sitter"
            ),
            Self::UnsupportedAntlrContexts => {
                formatter.write_str("ANTLR contexts cannot be used in a Tree-sitter profile")
            }
            Self::UnsupportedAntlrQueryFields { query_index } => write!(
                formatter,
                "query {query_index} declares ANTLR selectors for a Tree-sitter profile"
            ),
            Self::EmbeddedProfileMismatch => {
                formatter.write_str("embedded profile does not select PL/pgSQL")
            }
            Self::Engine(error) => error.fmt(formatter),
            Self::Origin(error) => error.fmt(formatter),
            Self::Embedded(error) => error.fmt(formatter),
        }
    }
}

impl Error for TreeSitterProfileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Engine(error) => Some(error),
            Self::Origin(error) => Some(error),
            Self::Embedded(error) => Some(error),
            Self::ParserMismatch
            | Self::EmbeddedProfileMismatch
            | Self::UnsupportedQuerySource { .. }
            | Self::UnsupportedAntlrQueryFields { .. }
            | Self::UnsupportedAntlrContexts => None,
        }
    }
}

impl From<io::Error> for TreeSitterProfileError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}

impl From<EngineError> for TreeSitterProfileError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}

impl From<OriginError> for TreeSitterProfileError {
    fn from(error: OriginError) -> Self {
        Self::Origin(error)
    }
}

impl From<EmbeddedError> for TreeSitterProfileError {
    fn from(error: EmbeddedError) -> Self {
        Self::Embedded(error)
    }
}

/// Native body ownership and observations in the prepared host's coordinates.
pub struct PlPgSqlProfileMatches<'profile, 'o, 'v> {
    pub profile: &'profile EffectiveProfile,
    pub body: PlPgSqlBody<'v>,
    pub nodes: Vec<SyntaxNode>,
    pub groups: Vec<TreeSitterQueryGroup<'profile, 'o, 'v>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryCaptureRole<'profile> {
    pub name: &'profile str,
    pub capture_index: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryCaptureRoles<'profile> {
    pub name: QueryCaptureRole<'profile>,
    pub value: Option<QueryCaptureRole<'profile>>,
    pub span: Option<QueryCaptureRole<'profile>>,
    pub declaration: Option<QueryCaptureRole<'profile>>,
    pub context: Option<QueryCaptureRole<'profile>>,
    pub parent: Option<QueryCaptureRole<'profile>>,
    pub qualifier: Option<QueryCaptureRole<'profile>>,
}

impl<'profile> QueryCaptureRoles<'profile> {
    fn new(query: &'profile Query, compiled: &CompiledQuery) -> Self {
        let role = |name| QueryCaptureRole {
            name,
            capture_index: compiled.capture_index_for_name(name),
        };
        let optional = |name: &'profile str| (!name.is_empty()).then(|| role(name));
        let name = query.name_capture().value().as_str();
        Self {
            name: role(if name.is_empty() { "name" } else { name }),
            value: optional(query.value_capture().value()),
            span: optional(query.span_capture().value()),
            declaration: optional(query.declaration_capture().value()),
            context: optional(query.context_capture().value()),
            parent: optional(query.parent_capture().value()),
            qualifier: optional(query.qualifier_capture().value()),
        }
    }
}

/// Present indices address slots in this match's captures, never SDK capture IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureBinding {
    NotConfigured,
    Missing { declared: bool },
    Present(Vec<usize>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryCaptureBindings {
    pub name: CaptureBinding,
    pub value: CaptureBinding,
    pub span: CaptureBinding,
    pub declaration: CaptureBinding,
    pub context: CaptureBinding,
    pub parent: CaptureBinding,
    pub qualifier: CaptureBinding,
}

pub struct TreeSitterProfileCapture<'o, 'v> {
    pub native: Capture,
    pub prepared: SourceRange<'v>,
    pub original: OriginalProjection<'o>,
}

pub struct TreeSitterProfileMatch<'o, 'v> {
    pub pattern_index: usize,
    pub properties: Vec<PropertySetting>,
    pub captures: Vec<TreeSitterProfileCapture<'o, 'v>>,
    pub bindings: QueryCaptureBindings,
}

pub struct TreeSitterQueryGroup<'profile, 'o, 'v> {
    pub query: &'profile Effective<Query>,
    pub roles: QueryCaptureRoles<'profile>,
    pub matches: Vec<TreeSitterProfileMatch<'o, 'v>>,
    compiled: Arc<CompiledQuery>,
}

impl TreeSitterQueryGroup<'_, '_, '_> {
    pub fn capture_names(&self) -> &[&str] {
        self.compiled.capture_names()
    }
}

pub struct TreeSitterProfileMatches<'profile, 'o, 'v> {
    pub profile: &'profile EffectiveProfile,
    pub tree: ParsedTree,
    /// SDK prepared coordinates; parent is an index in this vector.
    pub nodes: Vec<SyntaxNode>,
    pub groups: Vec<TreeSitterQueryGroup<'profile, 'o, 'v>>,
}

struct PreparedQuery<'profile> {
    query: &'profile Effective<Query>,
    roles: QueryCaptureRoles<'profile>,
    compiled: Arc<CompiledQuery>,
}

pub struct PreparedTreeSitterProfile<'profile> {
    profile: &'profile EffectiveProfile,
    language: GrammarLanguage,
    queries: Vec<PreparedQuery<'profile>>,
}

fn check_control(
    original: &SnapshotContext,
    control: ParseControl<'_>,
) -> Result<(), TreeSitterProfileError> {
    original.check()?;
    if control
        .cancellation
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        return Err(EngineError::Cancelled.into());
    }
    if control
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        return Err(EngineError::TimedOut.into());
    }
    Ok(())
}

fn binding(
    role: Option<QueryCaptureRole<'_>>,
    captures: &[TreeSitterProfileCapture<'_, '_>],
    original: &SnapshotContext,
) -> Result<CaptureBinding, TreeSitterProfileError> {
    original.check()?;
    let Some(role) = role else {
        return Ok(CaptureBinding::NotConfigured);
    };
    let mut indices = Vec::new();
    for (index, capture) in captures.iter().enumerate() {
        original.check()?;
        if capture.native.name == role.name {
            indices.push(index);
        }
    }
    Ok(if indices.is_empty() {
        CaptureBinding::Missing {
            declared: role.capture_index.is_some(),
        }
    } else {
        CaptureBinding::Present(indices)
    })
}

impl<'profile> PreparedTreeSitterProfile<'profile> {
    /// Compile an additional native pattern using this exact retained grammar generation.
    pub fn compile_query(
        &self,
        pattern: &str,
        original: &SnapshotContext,
    ) -> Result<CompiledQuery, TreeSitterProfileError> {
        original.check()?;
        let compiled = self.language.compile_query(pattern);
        original.check()?;
        Ok(compiled?)
    }

    /// Roles refer to capture indices from the retained compiled query generation.
    pub fn query_roles(&self) -> impl Iterator<Item = (usize, &QueryCaptureRoles<'profile>)> {
        self.queries
            .iter()
            .enumerate()
            .map(|(index, query)| (index, &query.roles))
    }

    /// The caller supplies the grammar selected by its real profile loader.
    pub fn prepare(
        profile: &'profile EffectiveProfile,
        language: GrammarLanguage,
        original: &SnapshotContext,
    ) -> Result<Self, TreeSitterProfileError> {
        original.check()?;
        let result = (|| {
            if *profile.parser().value() != ProfileParser::TreeSitter {
                return Err(TreeSitterProfileError::ParserMismatch);
            }
            if !matches!(
                profile.antlr_contexts().provenance(),
                ProvenanceView::Default
            ) {
                return Err(TreeSitterProfileError::UnsupportedAntlrContexts);
            }
            for (query_index, query) in profile.queries().value().iter().enumerate() {
                original.check()?;
                let query_value = query.value();
                if [
                    query_value.select().provenance(),
                    query_value.keyword().provenance(),
                    query_value.child_rule().provenance(),
                    query_value.negate().provenance(),
                    query_value.captures().provenance(),
                ]
                .into_iter()
                .any(|provenance| !matches!(provenance, ProvenanceView::Default))
                {
                    return Err(TreeSitterProfileError::UnsupportedAntlrQueryFields {
                        query_index,
                    });
                }
                if *query.value().source().value() != QuerySource::Primary {
                    return Err(TreeSitterProfileError::UnsupportedQuerySource {
                        query_index,
                        source: *query.value().source().value(),
                    });
                }
            }
            let mut queries = Vec::new();
            for query in profile.queries().value().iter() {
                original.check()?;
                let compiled = language.compile_query(query.value().pattern().value());
                original.check()?;
                let compiled = Arc::new(compiled?);
                let roles = QueryCaptureRoles::new(query.value(), &compiled);
                original.check()?;
                queries.push(PreparedQuery {
                    query,
                    roles,
                    compiled,
                });
            }
            Ok(Self {
                profile,
                language,
                queries,
            })
        })();
        original.check()?;
        result
    }

    /// Parse once and return complete observations, including recovery flags.
    pub fn collect_matches<'o, 'v>(
        &self,
        input: &PreparedInput<'o, 'v, '_, '_>,
    ) -> Result<TreeSitterProfileMatches<'profile, 'o, 'v>, TreeSitterProfileError> {
        let original = input.original_context();
        original.check()?;
        let result = (|| {
            let tree = self.language.parse_with_context(
                input.prepared_text(),
                ParseControl::default(),
                original,
            )?;
            original.check()?;
            let nodes = tree.nodes_with_context(ParseControl::default(), original)?;
            let groups = self.collect_groups(input, ParseControl::default(), |query| {
                Ok(tree.matches_with_context(query, ParseControl::default(), original)?)
            })?;
            Ok(TreeSitterProfileMatches {
                profile: self.profile,
                tree,
                nodes,
                groups,
            })
        })();
        original.check()?;
        result
    }
    /// Reuse a PostgreSQL host from this same input and original operation.
    pub fn collect_plpgsql_bodies<'o, 'v>(
        &self,
        host: &ParsedSyntax<'v>,
        input: &PreparedInput<'o, 'v, '_, '_>,
        control: ParseControl<'_>,
    ) -> Result<Vec<PlPgSqlProfileMatches<'profile, 'o, 'v>>, TreeSitterProfileError> {
        let original = input.original_context();
        check_control(original, control)?;
        if self.profile.language().value() != "plpgsql" {
            return Err(TreeSitterProfileError::EmbeddedProfileMismatch);
        }
        host.validate_prepared(input)?;
        let result = (|| {
            let bodies = parse_plpgsql_bodies(host, &self.language, control, original)?;
            let mut observations = Vec::with_capacity(bodies.len());
            for body in bodies {
                check_control(original, control)?;
                let nodes = body.nodes_with_context(control, original)?;
                let groups = self.collect_groups(input, control, |query| {
                    Ok(body.matches_with_context(query, control, original)?)
                })?;
                observations.push(PlPgSqlProfileMatches {
                    profile: self.profile,
                    body,
                    nodes,
                    groups,
                });
            }
            Ok(observations)
        })();
        check_control(original, control)?;
        result
    }

    fn collect_groups<'o, 'v>(
        &self,
        input: &PreparedInput<'o, 'v, '_, '_>,
        control: ParseControl<'_>,
        mut execute: impl FnMut(
            &CompiledQuery,
        ) -> Result<
            Vec<crate::tree_sitter_engine::QueryMatch>,
            TreeSitterProfileError,
        >,
    ) -> Result<Vec<TreeSitterQueryGroup<'profile, 'o, 'v>>, TreeSitterProfileError> {
        let original = input.original_context();
        let mut groups = Vec::with_capacity(self.queries.len());
        for prepared in &self.queries {
            check_control(original, control)?;
            let native = execute(&prepared.compiled)?;
            check_control(original, control)?;
            let mut matches = Vec::new();
            for found in native {
                check_control(original, control)?;
                let mut captures = Vec::new();
                for capture in found.captures {
                    check_control(original, control)?;
                    let start = u64::try_from(capture.range.start)
                        .map_err(|_| OriginError::ArithmeticOverflow)?;
                    let end = u64::try_from(capture.range.end)
                        .map_err(|_| OriginError::ArithmeticOverflow)?;
                    captures.push(TreeSitterProfileCapture {
                        prepared: input.prepared_span(start, end)?,
                        original: input.project_prepared(start, end)?,
                        native: capture,
                    });
                }
                let roles = prepared.roles;
                let bindings = QueryCaptureBindings {
                    name: binding(Some(roles.name), &captures, original)?,
                    value: binding(roles.value, &captures, original)?,
                    span: binding(roles.span, &captures, original)?,
                    declaration: binding(roles.declaration, &captures, original)?,
                    context: binding(roles.context, &captures, original)?,
                    parent: binding(roles.parent, &captures, original)?,
                    qualifier: binding(roles.qualifier, &captures, original)?,
                };
                matches.push(TreeSitterProfileMatch {
                    pattern_index: found.pattern_index,
                    properties: found.properties,
                    captures,
                    bindings,
                });
            }
            groups.push(TreeSitterQueryGroup {
                query: prepared.query,
                roles: prepared.roles,
                matches,
                compiled: Arc::clone(&prepared.compiled),
            });
        }
        check_control(original, control)?;
        Ok(groups)
    }
}
