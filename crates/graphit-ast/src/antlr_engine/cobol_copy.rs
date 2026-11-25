//! Raw COPY observations from the official preprocessor, alongside the main COBOL tree.
//! Copybooks are not opened and REPLACING clauses are not executed.

use super::{
    AntlrEngineError, CaptureStep, Dialect, NodeClass, ParsedSyntax, PreparedProfileQueries,
    SpanStatus,
    semantic::{self, SemanticDocument, SemanticError, SemanticOccurrence, SemanticQueryOrigin},
};
use crate::{catalog::EffectiveProfile, prepared_input::PreparedInput, profiles::QuerySource};
use std::collections::BTreeMap;

/// Each occurrence's node ordinals address `syntax`, never the main tree.
pub struct CobolPreprocessorDocument<'profile, 'o, 'v> {
    pub syntax: ParsedSyntax<'v>,
    pub occurrences: Vec<CobolPreprocessorOccurrence<'profile, 'o, 'v>>,
}
impl CobolPreprocessorDocument<'_, '_, '_> {
    pub const fn grammar(&self) -> &'static str {
        "Cobol85Preprocessor.g4"
    }
}
pub struct CobolSemanticDocument<'profile, 'o, 'v> {
    pub main: SemanticDocument<'profile, 'o, 'v>,
    pub preprocessor: CobolPreprocessorDocument<'profile, 'o, 'v>,
}
pub struct CobolPreprocessorOccurrence<'profile, 'o, 'v> {
    pub occurrence: SemanticOccurrence<'profile, 'o, 'v>,
    pub scope: CopyProgramScope,
}
/// Scope is linked only by an identical native COPY keyword and main-tree parents.
#[derive(Debug, Eq, PartialEq)]
pub enum CopyProgramScope {
    Linked {
        main_anchor_node_index: usize,
        program_node_index: usize,
        /// Indices in `main.contexts`, nearest-to-outer.
        context_indices: Vec<usize>,
    },
    NoAnchor,
    UnindexedAnchor,
    OutsideProgram,
    AmbiguousAnchor,
}

pub fn parse_cobol_semantic_occurrences<'profile, 'o, 'v>(
    profile: &'profile EffectiveProfile,
    input: &PreparedInput<'o, 'v, '_, '_>,
) -> Result<CobolSemanticDocument<'profile, 'o, 'v>, SemanticError> {
    let control = input.original_context();
    control.check()?;
    // Validate compatibility before either SDK is invoked.
    for (query_index, entry) in profile.queries().value().iter().enumerate() {
        control.check()?;
        if *entry.value().source().value() == QuerySource::CobolPreprocessor
            && (profile.grammar().value() != "antlr-cobol85"
                || *profile.parser().value() != crate::profiles::ProfileParser::Antlr4)
        {
            return Err(SemanticError::UnsupportedQuerySource {
                query_index,
                source: QuerySource::CobolPreprocessor,
                grammar: profile.grammar().value().clone(),
            });
        }
    }
    if semantic::dialect(profile)? != Dialect::Cobol85 {
        return Err(SemanticError::UnsupportedGrammar(
            profile.grammar().value().clone(),
        ));
    }
    let main = semantic::parse_semantic_primary_occurrences(profile, input)?;
    let preparing = profile.antlr_preprocessor_queries().get().is_none();
    let mut queries = Vec::new();
    let mut layouts = Vec::new();
    for (index, entry) in profile.queries().value().iter().enumerate() {
        control.check()?;
        if *entry.value().source().value() != QuerySource::CobolPreprocessor {
            continue;
        }
        let layout = semantic::query_layout(entry.value());
        let anchor_slot = layout.capture_count();
        if preparing {
            let (mut query, _) = semantic::query_plan(entry.value());
            query.captures.push(vec![
                CaptureStep::NearestAncestor("copyStatement".into()),
                CaptureStep::XPath("/COPY".into()),
            ]);
            queries.push(query);
        }
        layouts.push((index, layout, anchor_slot));
    }
    let prepare_or_parse = || {
        let cell = profile.antlr_preprocessor_queries();
        if cell.get().is_none() {
            let prepared = PreparedProfileQueries::new(Dialect::Cobol85, true, &queries, control)?;
            let _ = cell.set(prepared);
        }
        cell.get()
            .ok_or_else(|| super::contract("missing prepared preprocessor profile"))?
            .parse(input.prepared_text(), control)
    };
    let parsed = prepare_or_parse().map_err(|cause| {
        let query_origin =
            match &cause {
                AntlrEngineError::QueryFailure { query_index, .. } => layouts
                    .get(*query_index)
                    .map(|(index, _, _)| SemanticQueryOrigin::Profile {
                        query_index: *index,
                    }),
                _ => None,
            };
        SemanticError::Engine {
            query_origin,
            cause,
        }
    })?;
    main.syntax
        .validate_prepared(input)
        .map_err(SemanticError::Origin)?;
    parsed
        .syntax
        .validate_prepared(input)
        .map_err(SemanticError::Origin)?;
    let mut anchors: BTreeMap<(usize, usize), Option<usize>> = BTreeMap::new();
    for (index, node) in main.syntax.nodes.iter().enumerate() {
        control.check()?;
        if node.kind == "COPY"
            && node.class != NodeClass::Rule
            && !node.missing
            && node.status == SpanStatus::Regular
            && let Some(span) = &node.span
        {
            anchors
                .entry((span.start, span.end))
                .and_modify(|candidate| *candidate = None)
                .or_insert(Some(index));
        }
    }
    let contexts: BTreeMap<_, _> = main
        .contexts
        .iter()
        .enumerate()
        .map(|(index, context)| (context.matched.node_index, index))
        .collect();
    let mut occurrences = Vec::with_capacity(parsed.matches.len());
    for matched in parsed.matches {
        control.check()?;
        let (query_index, layout, anchor_slot) =
            layouts
                .get(matched.query_index)
                .ok_or(SemanticError::NativeContract(
                    "COPY query ordinal exceeds plan",
                ))?;
        let query = profile.queries().value().as_slice()[*query_index].value();
        let anchor = *matched
            .captures
            .get(*anchor_slot)
            .ok_or(SemanticError::NativeContract(
                "COPY anchor capture slot missing",
            ))?;
        let scope = link_scope(
            anchor,
            &parsed.syntax,
            &main.syntax,
            &anchors,
            &contexts,
            control,
        )?;
        occurrences.push(CobolPreprocessorOccurrence {
            occurrence: SemanticOccurrence {
                query_index: *query_index,
                query,
                matched: semantic::captured(input, &parsed.syntax, matched.node_index)?,
                name: semantic::binding(
                    Some(layout.name),
                    &matched.captures,
                    input,
                    &parsed.syntax,
                )?,
                value: semantic::binding(layout.value, &matched.captures, input, &parsed.syntax)?,
                span: semantic::binding(layout.span, &matched.captures, input, &parsed.syntax)?,
                parent: semantic::binding(layout.parent, &matched.captures, input, &parsed.syntax)?,
                qualifier: semantic::binding(
                    layout.qualifier,
                    &matched.captures,
                    input,
                    &parsed.syntax,
                )?,
                contexts: Vec::new(),
            },
            scope,
        });
    }
    control.check()?;
    Ok(CobolSemanticDocument {
        main,
        preprocessor: CobolPreprocessorDocument {
            syntax: parsed.syntax,
            occurrences,
        },
    })
}

fn link_scope(
    anchor: Option<usize>,
    preprocessor: &ParsedSyntax<'_>,
    main: &ParsedSyntax<'_>,
    anchors: &BTreeMap<(usize, usize), Option<usize>>,
    contexts: &BTreeMap<usize, usize>,
    control: &graphit_core::file_io::SnapshotContext,
) -> Result<CopyProgramScope, SemanticError> {
    let Some(anchor) = anchor else {
        return Ok(CopyProgramScope::NoAnchor);
    };
    let node = preprocessor
        .nodes
        .get(anchor)
        .ok_or(SemanticError::NativeContract(
            "COPY anchor ordinal exceeds tree",
        ))?;
    if node.kind != "COPY"
        || node.class == NodeClass::Rule
        || node.missing
        || node.status != SpanStatus::Regular
    {
        return Ok(CopyProgramScope::UnindexedAnchor);
    }
    let Some(span) = &node.span else {
        return Ok(CopyProgramScope::UnindexedAnchor);
    };
    let Some(candidate) = anchors.get(&(span.start, span.end)) else {
        return Ok(CopyProgramScope::NoAnchor);
    };
    let Some(anchor) = candidate else {
        return Ok(CopyProgramScope::AmbiguousAnchor);
    };
    let mut cursor = main.nodes[*anchor].parent;
    let mut program_node_index = None;
    let mut context_indices = Vec::new();
    while let Some(index) = cursor {
        control.check()?;
        let node = main.nodes.get(index).ok_or(SemanticError::NativeContract(
            "COPY parent ordinal exceeds main tree",
        ))?;
        if node.class == NodeClass::Rule
            && node.kind == "programUnit"
            && program_node_index.is_none()
        {
            program_node_index = Some(index);
        }
        if let Some(context) = contexts.get(&index) {
            context_indices.push(*context);
        }
        if node.parent.is_some_and(|parent| parent >= index) {
            return Err(SemanticError::NativeContract(
                "COPY main parent order is not acyclic preorder",
            ));
        }
        cursor = node.parent;
    }
    Ok(match program_node_index {
        Some(program_node_index) => CopyProgramScope::Linked {
            main_anchor_node_index: *anchor,
            program_node_index,
            context_indices,
        },
        None => CopyProgramScope::OutsideProgram,
    })
}
