//! PostgreSQL host and PL/pgSQL body observations with explicit tree addresses.

use std::{collections::HashMap, error::Error, fmt, sync::atomic::Ordering, time::Instant};

use super::{SemanticDocument, SemanticError, parse_semantic_occurrences};
use crate::{
    SourceRange,
    catalog::EffectiveProfile,
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
    tree_sitter_engine::{EngineError, ParseControl},
    tree_sitter_profile::{
        PlPgSqlProfileMatches, PreparedTreeSitterProfile, TreeSitterProfileError,
    },
};

pub struct PostgreSqlSemanticDocument<'host, 'body, 'o, 'v> {
    pub host: SemanticDocument<'host, 'o, 'v>,
    pub bodies: Vec<PostgreSqlSemanticBody<'body, 'o, 'v>>,
}

pub struct PostgreSqlSemanticBody<'profile, 'o, 'v> {
    pub observations: PlPgSqlProfileMatches<'profile, 'o, 'v>,
    /// Indices in the host document's contexts, nearest first.
    pub host_contexts: Vec<usize>,
    pub matches: Vec<BodyMatchScope>,
    pub decisions: Vec<BodyDecision<'o, 'v>>,
}

/// Capture slots resolve to ordinals in this body's retained nodes, never host nodes.
pub struct BodyMatchScope {
    pub query_index: usize,
    pub match_index: usize,
    pub capture_nodes: Vec<usize>,
}

/// A raw configured decision observation, not a computed complexity metric.
pub struct BodyDecision<'o, 'v> {
    pub node_index: usize,
    pub prepared: SourceRange<'v>,
    pub original: OriginalProjection<'o>,
}

#[derive(Debug)]
pub enum PostgreSqlSemanticError {
    Host(SemanticError),
    Body(TreeSitterProfileError),
    NativeContract(&'static str),
}

impl fmt::Display for PostgreSqlSemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(error) => error.fmt(f),
            Self::Body(error) => error.fmt(f),
            Self::NativeContract(message) => f.write_str(message),
        }
    }
}

impl Error for PostgreSqlSemanticError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Host(error) => Some(error),
            Self::Body(error) => Some(error),
            Self::NativeContract(_) => None,
        }
    }
}

impl From<SemanticError> for PostgreSqlSemanticError {
    fn from(error: SemanticError) -> Self {
        Self::Host(error)
    }
}
impl From<TreeSitterProfileError> for PostgreSqlSemanticError {
    fn from(error: TreeSitterProfileError) -> Self {
        Self::Body(error)
    }
}
impl From<OriginError> for PostgreSqlSemanticError {
    fn from(error: OriginError) -> Self {
        Self::Body(TreeSitterProfileError::Origin(error))
    }
}

fn check(
    input: &PreparedInput<'_, '_, '_, '_>,
    control: ParseControl<'_>,
) -> Result<(), PostgreSqlSemanticError> {
    input
        .original_context()
        .check()
        .map_err(SemanticError::Operation)?;
    let error = if control
        .cancellation
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        Some(EngineError::Cancelled)
    } else if control
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        Some(EngineError::TimedOut)
    } else {
        None
    };
    match error {
        Some(error) => Err(TreeSitterProfileError::Engine(error).into()),
        None => Ok(()),
    }
}

/// Parse the host once and execute the already prepared PL/pgSQL profile on its bodies.
pub fn parse_postgresql_semantic_occurrences<'host, 'body, 'o, 'v>(
    profile: &'host EffectiveProfile,
    body_profile: &PreparedTreeSitterProfile<'body>,
    input: &PreparedInput<'o, 'v, '_, '_>,
    control: ParseControl<'_>,
) -> Result<PostgreSqlSemanticDocument<'host, 'body, 'o, 'v>, PostgreSqlSemanticError> {
    check(input, control)?;
    if profile.grammar().value() != "antlr-postgresql" {
        return Err(SemanticError::UnsupportedGrammar(profile.grammar().value().clone()).into());
    }
    let host = parse_semantic_occurrences(profile, input)?;
    let observations = body_profile.collect_plpgsql_bodies(&host.syntax, input, control)?;
    let host_contexts: HashMap<_, _> = host
        .contexts
        .iter()
        .enumerate()
        .map(|(index, context)| (context.matched.node_index, index))
        .collect();
    let mut bodies = Vec::with_capacity(observations.len());
    for observations in observations {
        check(input, control)?;
        let mut contexts = Vec::new();
        let mut parent = Some(observations.body.host_parent);
        while let Some(index) = parent {
            check(input, control)?;
            let node =
                host.syntax
                    .nodes
                    .get(index)
                    .ok_or(PostgreSqlSemanticError::NativeContract(
                        "invalid host body parent",
                    ))?;
            if let Some(context) = host_contexts.get(&index) {
                contexts.push(*context);
            }
            if node.parent.is_some_and(|parent| parent >= index) {
                return Err(PostgreSqlSemanticError::NativeContract(
                    "host parent is not an ancestor",
                ));
            }
            parent = node.parent;
        }
        let mut node_indices = HashMap::with_capacity(observations.nodes.len());
        for (index, node) in observations.nodes.iter().enumerate() {
            check(input, control)?;
            if node.parent.is_some_and(|parent| parent >= index)
                || node_indices.insert(node.node_id, index).is_some()
            {
                return Err(PostgreSqlSemanticError::NativeContract(
                    "invalid native body node identity or parent",
                ));
            }
        }
        let mut matches = Vec::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            for (match_index, found) in group.matches.iter().enumerate() {
                check(input, control)?;
                let mut capture_nodes = Vec::with_capacity(found.captures.len());
                for capture in &found.captures {
                    check(input, control)?;
                    let index = node_indices.get(&capture.native.node_id).copied().ok_or(
                        PostgreSqlSemanticError::NativeContract(
                            "capture is absent from its body tree",
                        ),
                    )?;
                    capture_nodes.push(index);
                }
                matches.push(BodyMatchScope {
                    query_index,
                    match_index,
                    capture_nodes,
                });
            }
        }
        let mut decisions = Vec::new();
        for (node_index, node) in observations.nodes.iter().enumerate() {
            check(input, control)?;
            if observations
                .profile
                .complexity()
                .value()
                .node_types()
                .value()
                .iter()
                .any(|kind| kind.value() == &node.kind)
            {
                let start =
                    u64::try_from(node.range.start).map_err(|_| OriginError::ArithmeticOverflow)?;
                let end =
                    u64::try_from(node.range.end).map_err(|_| OriginError::ArithmeticOverflow)?;
                decisions.push(BodyDecision {
                    node_index,
                    prepared: input.prepared_span(start, end)?,
                    original: input.project_prepared(start, end)?,
                });
            }
        }
        bodies.push(PostgreSqlSemanticBody {
            observations,
            host_contexts: contexts,
            matches,
            decisions,
        });
    }
    check(input, control)?;
    Ok(PostgreSqlSemanticDocument { host, bodies })
}
