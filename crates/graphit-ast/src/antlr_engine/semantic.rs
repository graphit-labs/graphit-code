//! Raw semantic occurrences from effective profiles, without entity publication.

#[path = "semantic/postgresql.rs"]
pub mod postgresql;

use super::{
    AntlrEngineError, Dialect, NodeClass, ParsedSyntax,
    profile::{CapturePath, CaptureStep, PreparedProfileQueries, ProfileQuery},
};
use crate::{
    SourceRange,
    catalog::{EffectiveProfile, Query},
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
    profiles::{ProfileParser, QuerySource},
};
use std::{collections::BTreeMap, error::Error, fmt, io};

pub struct CapturedNode<'o, 'v> {
    pub node_index: usize,
    pub prepared: Option<SourceRange<'v>>,
    pub original: Option<OriginalProjection<'o>>,
}

pub enum CaptureBinding<'o, 'v> {
    NotConfigured,
    Missing,
    Present(CapturedNode<'o, 'v>),
}

pub struct SemanticOccurrence<'profile, 'o, 'v> {
    pub query_index: usize,
    pub query: &'profile Query,
    pub matched: CapturedNode<'o, 'v>,
    pub name: CaptureBinding<'o, 'v>,
    pub value: CaptureBinding<'o, 'v>,
    pub span: CaptureBinding<'o, 'v>,
    pub parent: CaptureBinding<'o, 'v>,
    pub qualifier: CaptureBinding<'o, 'v>,
    /// Context indices in nearest-to-outer order, excluding the matched node.
    pub contexts: Vec<usize>,
}

pub struct SemanticContext<'profile, 'o, 'v> {
    pub selector: &'profile str,
    pub label: Option<&'profile str>,
    pub name_is_explicit: bool,
    pub matched: CapturedNode<'o, 'v>,
    pub name: CaptureBinding<'o, 'v>,
}

pub struct SemanticDocument<'profile, 'o, 'v> {
    pub profile: &'profile EffectiveProfile,
    pub syntax: ParsedSyntax<'v>,
    pub occurrences: Vec<SemanticOccurrence<'profile, 'o, 'v>>,
    pub contexts: Vec<SemanticContext<'profile, 'o, 'v>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticQueryOrigin {
    Profile { query_index: usize },
    Context { selector: String },
}

#[derive(Debug)]
pub enum SemanticError {
    Operation(io::Error),
    UnsupportedParser(ProfileParser),
    UnsupportedGrammar(String),
    UnsupportedProfileField {
        query_index: Option<usize>,
        field: &'static str,
    },
    UnsupportedQuerySource {
        query_index: usize,
        source: QuerySource,
        grammar: String,
    },
    UnsupportedStartRule {
        requested: String,
        expected: &'static str,
    },
    ContextMustBeRule {
        selector: String,
        node_index: usize,
    },
    OverlappingContextSelectors {
        node_index: usize,
        first_selector: String,
        second_selector: String,
    },
    Engine {
        query_origin: Option<SemanticQueryOrigin>,
        cause: AntlrEngineError,
    },
    Origin(OriginError),
    NativeContract(&'static str),
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::UnsupportedParser(p) => {
                write!(f, "semantic ANTLR consumer cannot use parser {p:?}")
            }
            Self::UnsupportedGrammar(g) => write!(f, "unsupported semantic ANTLR grammar {g:?}"),
            Self::UnsupportedProfileField { query_index, field } => write!(
                f,
                "ANTLR profile field {field:?} is unsupported in query {query_index:?}; use native XPath and structured captures"
            ),
            Self::UnsupportedQuerySource {
                query_index,
                source,
                grammar,
            } => write!(
                f,
                "query {query_index} source {source:?} is unsupported by grammar {grammar:?}"
            ),
            Self::UnsupportedStartRule {
                requested,
                expected,
            } => write!(
                f,
                "start rule {requested:?} differs from SDK entry {expected:?}"
            ),
            Self::ContextMustBeRule {
                selector,
                node_index,
            } => {
                write!(
                    f,
                    "context {selector:?} selected non-rule node {node_index}"
                )
            }
            Self::OverlappingContextSelectors {
                node_index,
                first_selector,
                second_selector,
            } => write!(
                f,
                "context selectors {first_selector:?} and {second_selector:?} selected the same native node {node_index}"
            ),
            Self::Engine {
                query_origin,
                cause,
            } => write!(f, "semantic query {query_origin:?}: {cause}"),
            Self::Origin(e) => e.fmt(f),
            Self::NativeContract(m) => write!(f, "semantic native boundary: {m}"),
        }
    }
}
impl Error for SemanticError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Engine { cause, .. } => Some(cause),
            Self::Origin(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for SemanticError {
    fn from(value: io::Error) -> Self {
        Self::Operation(value)
    }
}

pub(crate) struct QuerySlots {
    pub(crate) name: usize,
    pub(crate) value: Option<usize>,
    pub(crate) span: Option<usize>,
    pub(crate) parent: Option<usize>,
    pub(crate) qualifier: Option<usize>,
}
struct ContextSpec<'a> {
    selector: &'a str,
    label: Option<&'a str>,
    name_is_explicit: bool,
}

pub(crate) fn capture_steps(
    steps: &crate::catalog::EffectiveList<crate::catalog::Effective<crate::catalog::CaptureStep>>,
) -> CapturePath {
    steps
        .iter()
        .map(|step| {
            let step = step.value();
            if !step.xpath().value().is_empty() {
                CaptureStep::XPath(step.xpath().value().clone())
            } else if !step.nearest_descendant().value().is_empty() {
                CaptureStep::NearestDescendant(step.nearest_descendant().value().clone())
            } else {
                CaptureStep::NearestAncestor(step.nearest_ancestor().value().clone())
            }
        })
        .collect()
}
pub(crate) fn dialect(profile: &EffectiveProfile) -> Result<Dialect, SemanticError> {
    if *profile.parser().value() != ProfileParser::Antlr4 {
        return Err(SemanticError::UnsupportedParser(*profile.parser().value()));
    }
    for (field, provenance) in [
        (
            "context_name_queries",
            profile.context_name_queries().provenance(),
        ),
        ("context_types", profile.context_types().provenance()),
    ] {
        if !matches!(provenance, crate::catalog::ProvenanceView::Default) {
            return Err(SemanticError::UnsupportedProfileField {
                query_index: None,
                field,
            });
        }
    }
    for (query_index, entry) in profile.queries().value().iter().enumerate() {
        let query = entry.value();
        for (field, provenance) in [
            ("name_capture", query.name_capture().provenance()),
            ("value_capture", query.value_capture().provenance()),
            ("span_capture", query.span_capture().provenance()),
            ("parent_capture", query.parent_capture().provenance()),
            ("qualifier_capture", query.qualifier_capture().provenance()),
            (
                "declaration_capture",
                query.declaration_capture().provenance(),
            ),
            ("context_capture", query.context_capture().provenance()),
        ] {
            if !matches!(provenance, crate::catalog::ProvenanceView::Default) {
                return Err(SemanticError::UnsupportedProfileField {
                    query_index: Some(query_index),
                    field,
                });
            }
        }
    }
    let dialect = match profile.grammar().value().as_str() {
        "antlr-plsql" => Dialect::PlSql,
        "antlr-postgresql" => Dialect::PostgreSql,
        "antlr-db2" => Dialect::Db2,
        "antlr-tsql" => Dialect::TSql,
        "antlr-cobol85" => Dialect::Cobol85,
        grammar => return Err(SemanticError::UnsupportedGrammar(grammar.into())),
    };
    if let Some(rule) = profile.start_rule().value()
        && rule != dialect.entry_rule()
    {
        return Err(SemanticError::UnsupportedStartRule {
            requested: rule.clone(),
            expected: dialect.entry_rule(),
        });
    }
    Ok(dialect)
}

pub(crate) fn captured<'o, 'v>(
    input: &PreparedInput<'o, 'v, '_, '_>,
    syntax: &ParsedSyntax<'v>,
    node_index: usize,
) -> Result<CapturedNode<'o, 'v>, SemanticError> {
    input.original_context().check()?;
    let node = syntax
        .nodes
        .get(node_index)
        .ok_or(SemanticError::NativeContract(
            "capture ordinal exceeds syntax nodes",
        ))?;
    let (prepared, original) = if let Some(span) = &node.span {
        let start = u64::try_from(span.start)
            .map_err(|_| SemanticError::Origin(OriginError::ArithmeticOverflow))?;
        let end = u64::try_from(span.end)
            .map_err(|_| SemanticError::Origin(OriginError::ArithmeticOverflow))?;
        let prepared = input
            .prepared_span(start, end)
            .map_err(|e| projection_error(input, e))?;
        let original = input
            .project_prepared(start, end)
            .map_err(|e| projection_error(input, e))?;
        (Some(prepared), Some(original))
    } else {
        (None, None)
    };
    Ok(CapturedNode {
        node_index,
        prepared,
        original,
    })
}
fn projection_error(input: &PreparedInput<'_, '_, '_, '_>, error: OriginError) -> SemanticError {
    match input.original_context().check() {
        Err(e) => SemanticError::Operation(e),
        Ok(()) => SemanticError::Origin(error),
    }
}
pub(crate) fn binding<'o, 'v>(
    slot: Option<usize>,
    captures: &[Option<usize>],
    input: &PreparedInput<'o, 'v, '_, '_>,
    syntax: &ParsedSyntax<'v>,
) -> Result<CaptureBinding<'o, 'v>, SemanticError> {
    let Some(slot) = slot else {
        return Ok(CaptureBinding::NotConfigured);
    };
    match captures.get(slot).ok_or(SemanticError::NativeContract(
        "capture slot missing from native result",
    ))? {
        Some(index) => captured(input, syntax, *index).map(CaptureBinding::Present),
        None => Ok(CaptureBinding::Missing),
    }
}

/// Parse once and retain every profile occurrence, capture and context observation.
/// Names are raw source captures; publication, decoding and ownership are later stages.
pub fn parse_semantic_occurrences<'profile, 'o, 'v>(
    profile: &'profile EffectiveProfile,
    input: &PreparedInput<'o, 'v, '_, '_>,
) -> Result<SemanticDocument<'profile, 'o, 'v>, SemanticError> {
    input.original_context().check()?;
    for (query_index, entry) in profile.queries().value().iter().enumerate() {
        input.original_context().check()?;
        if *entry.value().source().value() != QuerySource::Primary {
            return Err(SemanticError::UnsupportedQuerySource {
                query_index,
                source: *entry.value().source().value(),
                grammar: profile.grammar().value().clone(),
            });
        }
    }
    parse_semantic_primary_occurrences(profile, input)
}

pub(crate) fn query_layout(query: &Query) -> QuerySlots {
    let mut next = 1;
    let mut optional = |key: &str| {
        query
            .captures()
            .value()
            .iter()
            .any(|(name, _)| name.value() == key)
            .then(|| {
                let slot = next;
                next += 1;
                slot
            })
    };
    QuerySlots {
        name: 0,
        value: optional("value"),
        span: optional("span"),
        parent: optional("parent"),
        qualifier: optional("qualifier"),
    }
}
impl QuerySlots {
    pub(crate) fn capture_count(&self) -> usize {
        1 + [self.value, self.span, self.parent, self.qualifier]
            .iter()
            .flatten()
            .count()
    }
}
pub(crate) fn query_plan(query: &Query) -> (ProfileQuery, QuerySlots) {
    let slots = query_layout(query);
    let mut captures = Vec::with_capacity(slots.capture_count());
    for key in ["name", "value", "span", "parent", "qualifier"] {
        match query
            .captures()
            .value()
            .iter()
            .find(|(name, _)| name.value() == key)
        {
            Some((_, steps)) => captures.push(capture_steps(steps.value())),
            None if key == "name" => captures.push(Vec::new()),
            None => {}
        }
    }
    (
        ProfileQuery {
            pattern: query.pattern().value().clone(),
            select: query.select().value().clone(),
            keyword: query.keyword().value().clone(),
            child_rule: query.child_rule().value().clone(),
            negate: *query.negate().value(),
            captures,
        },
        slots,
    )
}

pub(crate) fn parse_semantic_primary_occurrences<'profile, 'o, 'v>(
    profile: &'profile EffectiveProfile,
    input: &PreparedInput<'o, 'v, '_, '_>,
) -> Result<SemanticDocument<'profile, 'o, 'v>, SemanticError> {
    let control = input.original_context();
    control.check()?;
    let dialect = dialect(profile)?;
    let preparing = profile.antlr_primary_queries().get().is_none();
    let mut queries = Vec::new();
    let mut slots = Vec::new();
    let mut indices = Vec::new();
    for (query_index, entry) in profile.queries().value().iter().enumerate() {
        control.check()?;
        let query = entry.value();
        if *query.source().value() != QuerySource::Primary {
            continue;
        }
        if preparing {
            let (plan, layout) = query_plan(query);
            queries.push(plan);
            slots.push(layout);
        } else {
            slots.push(query_layout(query));
        }
        indices.push(query_index);
    }
    let mut context_specs = Vec::new();
    for configured in profile.antlr_contexts().value().iter() {
        control.check()?;
        let configured = configured.value();
        context_specs.push(ContextSpec {
            selector: configured
                .pattern()
                .value()
                .strip_prefix("//")
                .unwrap_or(configured.pattern().value()),
            label: (!configured.label().value().is_empty())
                .then_some(configured.label().value().as_str()),
            name_is_explicit: !configured.name().value().is_empty(),
        });
        if preparing {
            queries.push(ProfileQuery {
                pattern: configured.pattern().value().clone(),
                keyword: configured.keyword().value().clone(),
                child_rule: configured.child_rule().value().clone(),
                negate: *configured.negate().value(),
                captures: vec![capture_steps(configured.name().value())],
                ..ProfileQuery::default()
            });
        }
    }
    let prepare_or_parse = || {
        let cell = profile.antlr_primary_queries();
        if cell.get().is_none() {
            let prepared = PreparedProfileQueries::new(dialect, false, &queries, control)?;
            let _ = cell.set(prepared);
        }
        cell.get()
            .ok_or_else(|| super::contract("missing prepared primary profile"))?
            .parse(input.prepared_text(), control)
    };
    let parsed = prepare_or_parse().map_err(|cause| {
        let query_origin = match &cause {
            AntlrEngineError::QueryFailure { query_index, .. } if *query_index < slots.len() => {
                Some(SemanticQueryOrigin::Profile {
                    query_index: indices[*query_index],
                })
            }
            AntlrEngineError::QueryFailure { query_index, .. } => query_index
                .checked_sub(slots.len())
                .and_then(|index| context_specs.get(index))
                .map(|spec| SemanticQueryOrigin::Context {
                    selector: spec.selector.into(),
                }),
            _ => None,
        };
        SemanticError::Engine {
            query_origin,
            cause,
        }
    })?;
    let mut context_matches = BTreeMap::new();
    for matched in &parsed.matches {
        control.check()?;
        if matched.query_index < slots.len() {
            continue;
        }
        let spec = matched
            .query_index
            .checked_sub(slots.len())
            .and_then(|index| context_specs.get(index))
            .ok_or(SemanticError::NativeContract(
                "context query ordinal exceeds its plan",
            ))?;
        let node =
            parsed
                .syntax
                .nodes
                .get(matched.node_index)
                .ok_or(SemanticError::NativeContract(
                    "context ordinal exceeds syntax nodes",
                ))?;
        if node.class != NodeClass::Rule {
            return Err(SemanticError::ContextMustBeRule {
                selector: spec.selector.into(),
                node_index: matched.node_index,
            });
        }
        if let Some((previous, _)) = context_matches.insert(matched.node_index, (spec, matched)) {
            return Err(SemanticError::OverlappingContextSelectors {
                node_index: matched.node_index,
                first_selector: previous.selector.into(),
                second_selector: spec.selector.into(),
            });
        }
    }
    let mut contexts = Vec::new();
    let mut context_indices = BTreeMap::new();
    for (node_index, (spec, matched)) in context_matches {
        control.check()?;
        context_indices.insert(node_index, contexts.len());
        contexts.push(SemanticContext {
            selector: spec.selector,
            label: spec.label,
            name_is_explicit: spec.name_is_explicit,
            matched: captured(input, &parsed.syntax, node_index)?,
            name: binding(Some(0), &matched.captures, input, &parsed.syntax)?,
        });
    }
    let mut occurrences = Vec::new();
    for matched in &parsed.matches {
        control.check()?;
        if matched.query_index >= slots.len() {
            continue;
        }
        let layout = &slots[matched.query_index];
        let query_index = indices[matched.query_index];
        let query = profile.queries().value().as_slice()[query_index].value();
        let mut scopes = Vec::new();
        let mut cursor = parsed
            .syntax
            .nodes
            .get(matched.node_index)
            .ok_or(SemanticError::NativeContract(
                "match ordinal exceeds syntax nodes",
            ))?
            .parent;
        while let Some(parent) = cursor {
            control.check()?;
            if let Some(index) = context_indices.get(&parent) {
                scopes.push(*index);
            }
            let node = parsed
                .syntax
                .nodes
                .get(parent)
                .ok_or(SemanticError::NativeContract(
                    "parent ordinal exceeds syntax nodes",
                ))?;
            if node.parent.is_some_and(|next| next >= parent) {
                return Err(SemanticError::NativeContract(
                    "native parent order is not acyclic preorder",
                ));
            }
            cursor = node.parent;
        }
        occurrences.push(SemanticOccurrence {
            query_index,
            query,
            matched: captured(input, &parsed.syntax, matched.node_index)?,
            name: binding(Some(layout.name), &matched.captures, input, &parsed.syntax)?,
            value: binding(layout.value, &matched.captures, input, &parsed.syntax)?,
            span: binding(layout.span, &matched.captures, input, &parsed.syntax)?,
            parent: binding(layout.parent, &matched.captures, input, &parsed.syntax)?,
            qualifier: binding(layout.qualifier, &matched.captures, input, &parsed.syntax)?,
            contexts: scopes,
        });
    }
    control.check()?;
    Ok(SemanticDocument {
        profile,
        syntax: parsed.syntax,
        occurrences,
        contexts,
    })
}
