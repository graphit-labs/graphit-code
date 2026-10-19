//! Profile-driven ANTLR fallback over retained PostgreSQL, TSQL and COBOL trees.
//!
//! PostgreSQL must use [`ParsedGenericAntlrDocument::collect_postgresql`] so its
//! PL/pgSQL observations are not lost. COBOL input must already carry the real
//! preprocessing origin plan. Tree addresses are always qualified; a COPY
//! preprocessor ordinal and a body ordinal never address the host tree.

use super::{
    NodeClass, SpanStatus, SyntaxDiagnostic, SyntaxNode,
    cobol_copy::{CobolSemanticDocument, CopyProgramScope, parse_cobol_semantic_occurrences},
    semantic::{
        CaptureBinding, CapturedNode, SemanticDocument, SemanticError, SemanticOccurrence,
        parse_semantic_occurrences,
        postgresql::{
            PostgreSqlSemanticDocument, PostgreSqlSemanticError,
            parse_postgresql_semantic_occurrences,
        },
    },
};
use crate::{
    RangeError, SourceRange, SourceText,
    catalog::{EffectiveProfile, Query},
    prepared_input::{
        OriginError, OriginPiece, OriginSegment, OriginalProjection, PreparedInput, SyntheticReason,
    },
    profiles::QueryKind,
    symbols::{IdentityDocument, IdentityDocumentInput, IdentityError, IdentityLimits},
    tree_sitter_engine::ParseControl,
    tree_sitter_profile::{CaptureBinding as BodyBinding, PreparedTreeSitterProfile},
};
use graphit_core::file_io::SnapshotContext;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, io,
};

#[derive(Debug)]
pub enum GenericAntlrModelError {
    Operation(io::Error),
    Semantic(SemanticError),
    PostgreSql(PostgreSqlSemanticError),
    Origin(OriginError),
    Identity(IdentityError),
    Regex(regex::Error),
    Range(RangeError),
    UnsupportedProfile,
    NativeContract(&'static str),
}
impl fmt::Display for GenericAntlrModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f), Self::Semantic(e) => e.fmt(f),
            Self::PostgreSql(e) => e.fmt(f), Self::Origin(e) => e.fmt(f),
            Self::Identity(e) => e.fmt(f), Self::Regex(e) => e.fmt(f), Self::Range(e) => e.fmt(f),
            Self::UnsupportedProfile => f.write_str("generic ANTLR collection requires TSQL/COBOL, or the PostgreSQL body-aware constructor"),
            Self::NativeContract(e) => f.write_str(e),
        }
    }
}
impl Error for GenericAntlrModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Semantic(e) => Some(e),
            Self::PostgreSql(e) => Some(e),
            Self::Origin(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Regex(e) => Some(e),
            Self::Range(e) => Some(e),
            Self::UnsupportedProfile | Self::NativeContract(_) => None,
        }
    }
}
macro_rules! from_error {
    ($variant:ident, $ty:ty) => {
        impl From<$ty> for GenericAntlrModelError {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(Operation, io::Error);
from_error!(Semantic, SemanticError);
from_error!(PostgreSql, PostgreSqlSemanticError);
from_error!(Origin, OriginError);
from_error!(Identity, IdentityError);
from_error!(Regex, regex::Error);
from_error!(Range, RangeError);

/// Native ordinals are local to this tree, including each separate PL/pgSQL body.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
pub enum GenericAntlrTree {
    Main,
    CobolPreprocessor,
    PostgreSqlBody(usize),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
pub struct GenericAntlrAddress {
    pub tree: GenericAntlrTree,
    pub node_index: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct GenericAntlrOccurrenceAddress {
    pub tree: GenericAntlrTree,
    pub query_index: usize,
    /// Main/preprocessor occurrence ordinal, or match ordinal within a body group.
    pub match_index: usize,
}

/// Real collector owners. Public access is observational; construction stays private.
pub enum GenericAntlrObservations<'profile, 'o, 'v> {
    TSql(SemanticDocument<'profile, 'o, 'v>),
    Cobol(CobolSemanticDocument<'profile, 'o, 'v>),
    PostgreSql(PostgreSqlSemanticDocument<'profile, 'profile, 'o, 'v>),
}
impl<'profile, 'o, 'v> GenericAntlrObservations<'profile, 'o, 'v> {
    pub fn main(&self) -> &SemanticDocument<'profile, 'o, 'v> {
        match self {
            Self::TSql(d) => d,
            Self::Cobol(d) => &d.main,
            Self::PostgreSql(d) => &d.host,
        }
    }
}

/// Provenance of a physical Original extent; Compound is never relabelled Exact.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub enum GenericAntlrExtentProvenance {
    ExactProjection,
    /// Ordered Original pieces with every gap explicitly removed by preprocessing.
    SequentialOriginalCoverage,
}

/// An exact native name; unavailable/conflicting extents remain explicit.
pub struct GenericAntlrEntity<'profile, 'o> {
    uid: String,
    label: &'profile str,
    data_key: &'profile str,
    name: SourceRange<'o>,
    extent: Option<SourceRange<'o>>,
    extent_provenance: Option<GenericAntlrExtentProvenance>,
    extent_conflict: bool,
    declaration: GenericAntlrAddress,
    ancestors: Vec<GenericAntlrAddress>,
    owners: Vec<usize>,
    contributions: Vec<GenericAntlrOccurrenceAddress>,
    values: Vec<SourceRange<'o>>,
}
impl<'profile, 'o> GenericAntlrEntity<'profile, 'o> {
    pub fn uid(&self) -> &str {
        &self.uid
    }
    pub fn label(&self) -> &'profile str {
        self.label
    }
    pub fn data_key(&self) -> &'profile str {
        self.data_key
    }
    pub fn name(&self) -> SourceRange<'o> {
        self.name
    }
    pub fn extent(&self) -> Option<SourceRange<'o>> {
        self.extent
    }
    pub fn extent_provenance(&self) -> Option<GenericAntlrExtentProvenance> {
        self.extent_provenance
    }
    pub fn extent_conflict(&self) -> bool {
        self.extent_conflict
    }
    pub fn declaration(&self) -> GenericAntlrAddress {
        self.declaration
    }
    pub fn owner_indices(&self) -> &[usize] {
        &self.owners
    }
    pub fn contributions(&self) -> &[GenericAntlrOccurrenceAddress] {
        &self.contributions
    }
    pub fn values(&self) -> &[SourceRange<'o>] {
        &self.values
    }
}

/// Acquisition owner tied to the genuine prepared input and original request.
pub struct ParsedGenericAntlrDocument<'profile, 'o, 'v, 'request> {
    raw: GenericAntlrObservations<'profile, 'o, 'v>,
    identity: IdentityDocument<'o, 'request>,
    source: &'o str,
    original: &'request SnapshotContext,
    entities: Vec<GenericAntlrEntity<'profile, 'o>>,
}
impl<'profile: 'o, 'o, 'v, 'request> ParsedGenericAntlrDocument<'profile, 'o, 'v, 'request> {
    /// TSQL uses the primary collector; COBOL retains both native trees and COPY scopes.
    pub fn collect(
        profile: &'profile EffectiveProfile,
        input: &PreparedInput<'o, 'v, '_, 'request>,
        relative_components: &'o [&'o str],
        limits: IdentityLimits,
    ) -> Result<Self, GenericAntlrModelError> {
        input.original_context().check()?;
        let raw = match profile.grammar().value().as_str() {
            "antlr-tsql" => {
                GenericAntlrObservations::TSql(parse_semantic_occurrences(profile, input)?)
            }
            "antlr-cobol85" => {
                GenericAntlrObservations::Cobol(parse_cobol_semantic_occurrences(profile, input)?)
            }
            _ => return Err(GenericAntlrModelError::UnsupportedProfile),
        };
        Self::finish(raw, input, relative_components, limits)
    }
    /// Body-aware PostgreSQL collection using an already prepared real PL/pgSQL profile.
    pub fn collect_postgresql(
        profile: &'profile EffectiveProfile,
        body_profile: &PreparedTreeSitterProfile<'profile>,
        input: &PreparedInput<'o, 'v, '_, 'request>,
        relative_components: &'o [&'o str],
        limits: IdentityLimits,
        control: ParseControl<'_>,
    ) -> Result<Self, GenericAntlrModelError> {
        let raw = GenericAntlrObservations::PostgreSql(parse_postgresql_semantic_occurrences(
            profile,
            body_profile,
            input,
            control,
        )?);
        Self::finish(raw, input, relative_components, limits)
    }
    fn finish(
        raw: GenericAntlrObservations<'profile, 'o, 'v>,
        input: &PreparedInput<'o, 'v, '_, 'request>,
        relative_components: &'o [&'o str],
        limits: IdentityLimits,
    ) -> Result<Self, GenericAntlrModelError> {
        let original = input.original_context();
        original.check()?;
        let profile = raw.main().profile;
        let identity = IdentityDocument::new(
            IdentityDocumentInput {
                relative_components,
                language: profile.language().value(),
                grammar: profile.grammar().value(),
                original_bytes: input.original_text().as_bytes(),
            },
            limits,
            original,
        )?;
        let mut candidates = Vec::new();
        collect_main(&raw, input, &mut candidates)?;
        if let GenericAntlrObservations::PostgreSql(document) = &raw {
            collect_bodies(document, input, &mut candidates)?;
        }
        let mut entities: Vec<GenericAntlrEntity<'profile, 'o>> = Vec::new();
        let mut keys = BTreeMap::new();
        for candidate in candidates {
            original.check()?;
            let mut frames = Vec::new();
            for (label, name) in &candidate.frames {
                original.check()?;
                let owners = frames.iter().collect::<Vec<_>>();
                frames.push(identity.symbol(label, *name, &owners)?);
            }
            let owners = frames.iter().collect::<Vec<_>>();
            let uid = identity
                .symbol(
                    candidate.query.graph_label().value(),
                    candidate.name,
                    &owners,
                )?
                .uid()
                .as_str()
                .to_owned();
            let key = (uid.clone(), candidate.query.data_key().value().clone());
            if let Some(&index) = keys.get(&key) {
                let entity: &mut GenericAntlrEntity<'_, '_> = &mut entities[index];
                entity.contributions.push(candidate.occurrence);
                if entity.extent.map(|r| (r.start(), r.end()))
                    != candidate.extent.map(|r| (r.start(), r.end()))
                {
                    entity.extent = None;
                    entity.extent_provenance = None;
                    entity.extent_conflict = true;
                }
                if entity.extent.is_some()
                    && candidate.extent_provenance
                        == Some(GenericAntlrExtentProvenance::SequentialOriginalCoverage)
                {
                    entity.extent_provenance = candidate.extent_provenance;
                }
                if let Some(value) = candidate.value {
                    entity.values.push(value);
                }
                continue;
            }
            keys.insert(key, entities.len());
            entities.push(GenericAntlrEntity {
                uid,
                label: candidate.query.graph_label().value(),
                data_key: candidate.query.data_key().value(),
                name: candidate.name,
                extent: candidate.extent,
                extent_provenance: candidate.extent_provenance,
                extent_conflict: false,
                declaration: candidate.declaration,
                ancestors: candidate.ancestors,
                owners: Vec::new(),
                contributions: vec![candidate.occurrence],
                values: candidate.value.into_iter().collect(),
            });
        }
        for index in 0..entities.len() {
            original.check()?;
            let owners = entities[index]
                .ancestors
                .iter()
                .find_map(|address| {
                    let found = entities
                        .iter()
                        .enumerate()
                        .filter(|(other, entity)| *other != index && entity.declaration == *address)
                        .map(|(other, _)| other)
                        .collect::<Vec<_>>();
                    (!found.is_empty()).then_some(found)
                })
                .unwrap_or_default();
            entities[index].owners = owners;
        }
        original.check()?;
        Ok(Self {
            raw,
            identity,
            source: input.original_text(),
            original,
            entities,
        })
    }
    pub fn source(&self) -> &'o str {
        self.source
    }
    pub fn original_context(&self) -> &'request SnapshotContext {
        self.original
    }
    pub fn identity_document(&self) -> &IdentityDocument<'o, 'request> {
        &self.identity
    }
    pub fn observations(&self) -> &GenericAntlrObservations<'profile, 'o, 'v> {
        &self.raw
    }
    pub fn entities(&self) -> &[GenericAntlrEntity<'profile, 'o>] {
        &self.entities
    }
}

struct Candidate<'profile, 'o> {
    query: &'profile Query,
    name: SourceRange<'o>,
    extent: Option<SourceRange<'o>>,
    extent_provenance: Option<GenericAntlrExtentProvenance>,
    value: Option<SourceRange<'o>>,
    declaration: GenericAntlrAddress,
    occurrence: GenericAntlrOccurrenceAddress,
    ancestors: Vec<GenericAntlrAddress>,
    frames: Vec<(&'profile str, SourceRange<'o>)>,
}
fn exact<'o>(node: &CapturedNode<'o, '_>) -> Option<SourceRange<'o>> {
    match &node.original {
        Some(OriginalProjection::Exact(range)) => Some(*range),
        _ => None,
    }
}
fn bound<'o>(binding: &CaptureBinding<'o, '_>) -> Option<SourceRange<'o>> {
    match binding {
        CaptureBinding::Present(node) => exact(node),
        _ => None,
    }
}

// This is physical Original coverage, not a replacement for the native projection.
// Raw native projections remain unchanged in the retained observations.
fn original_extent<'o>(
    input: &PreparedInput<'o, '_, '_, '_>,
    start: usize,
    end: usize,
    cobol_padding: bool,
) -> Result<Option<(SourceRange<'o>, GenericAntlrExtentProvenance)>, GenericAntlrModelError> {
    let original = input.original_context();
    original.check()?;
    let projection = input.project_prepared(
        u64::try_from(start).map_err(|_| OriginError::ArithmeticOverflow)?,
        u64::try_from(end).map_err(|_| OriginError::ArithmeticOverflow)?,
    )?;
    let pieces = match projection {
        OriginalProjection::Exact(range) => {
            return Ok(Some((range, GenericAntlrExtentProvenance::ExactProjection)));
        }
        OriginalProjection::Compound(pieces) => pieces,
        _ => return Ok(None),
    };
    // The maintained COBOL preprocessor coalesces its six-column padding and
    // blank indicator into seven spaces at each fixed/variable source line.
    // Only that verified trivia may be skipped when
    // deriving physical Original coverage; the raw Compound stays untouched.
    for segment in input.origin_plan().segments {
        original.check()?;
        if let OriginSegment::Synthetic { prepared, reason } = segment {
            let low =
                usize::try_from(prepared.start).map_err(|_| OriginError::ArithmeticOverflow)?;
            let high =
                usize::try_from(prepared.end).map_err(|_| OriginError::ArithmeticOverflow)?;
            if low < end
                && high > start
                && (!cobol_padding
                    || *reason != SyntheticReason::InsertedByTransform
                    || input.prepared_text().get(low..high) != Some("       ")
                    || (low != 0 && input.prepared_text().as_bytes()[low - 1] != b'\n'))
            {
                return Ok(None);
            }
        }
    }
    let source = SourceText::new(
        input.original_text().as_bytes(),
        input.original_text().len().max(1),
    )?;
    let removed = input.origin_plan().removed_original;
    let mut removed_index = 0;
    let mut first = None;
    let mut cursor = 0;
    for piece in pieces {
        original.check()?;
        let range = match piece {
            OriginPiece::Original(range) => range,
            OriginPiece::Synthetic(SyntheticReason::InsertedByTransform) if cobol_padding => {
                continue;
            }
            _ => return Ok(None),
        };
        let canonical = source.range(range.start(), range.end())?;
        if range.start() == range.end()
            || canonical.text().as_ptr() != range.text().as_ptr()
            || canonical.text().len() != range.text().len()
            || canonical.start_point() != range.start_point()
            || canonical.end_point() != range.end_point()
        {
            return Ok(None);
        }
        if first.is_none() {
            first = Some(range.start());
            cursor = range.end();
            continue;
        }
        if range.start() < cursor {
            return Ok(None);
        }
        while cursor < range.start() {
            original.check()?;
            let cursor_wire = u64::try_from(cursor).map_err(|_| OriginError::ArithmeticOverflow)?;
            while removed
                .get(removed_index)
                .is_some_and(|span| span.end <= cursor_wire)
            {
                original.check()?;
                removed_index += 1;
            }
            let Some(span) = removed.get(removed_index) else {
                return Ok(None);
            };
            if span.start > cursor_wire {
                return Ok(None);
            }
            cursor = usize::try_from(span.end)
                .map_err(|_| OriginError::ArithmeticOverflow)?
                .min(range.start());
        }
        cursor = range.end();
    }
    original.check()?;
    Ok(first
        .map(|start| source.range(start, cursor))
        .transpose()?
        .map(|range| {
            (
                range,
                GenericAntlrExtentProvenance::SequentialOriginalCoverage,
            )
        }))
}
fn ancestors(
    nodes: &[SyntaxNode],
    start: usize,
    context: &SnapshotContext,
) -> Result<Vec<usize>, GenericAntlrModelError> {
    let mut current = Some(start);
    let mut result = Vec::new();
    while let Some(index) = current {
        context.check()?;
        let node = nodes
            .get(index)
            .ok_or(GenericAntlrModelError::NativeContract(
                "foreign ANTLR node ordinal",
            ))?;
        if node.parent.is_some_and(|parent| parent >= index) {
            return Err(GenericAntlrModelError::NativeContract(
                "ANTLR parent is not an ancestor",
            ));
        }
        result.push(index);
        current = node.parent;
    }
    Ok(result)
}
fn clean(
    nodes: &[SyntaxNode],
    index: usize,
    context: &SnapshotContext,
) -> Result<bool, GenericAntlrModelError> {
    Ok(ancestors(nodes, index, context)?.iter().all(|i| {
        let n = &nodes[*i];
        !n.error && !n.missing && n.class != NodeClass::Error && n.status == SpanStatus::Regular
    }))
}
fn admit(query: &Query, name: SourceRange<'_>) -> Result<bool, GenericAntlrModelError> {
    if name.text().is_empty() || query.graph_label().value().is_empty() {
        return Ok(false);
    }
    let policy = query.name_reject().value();
    Ok(policy.is_empty() || !regex::Regex::new(policy)?.is_match(name.text()))
}
// SDK error flags classify the individual node. A missing sibling of a name
// still makes its declaration unavailable, even when every name ancestor is a rule.
fn recovered_declarations(
    nodes: &[SyntaxNode],
    original: &SnapshotContext,
) -> Result<BTreeSet<usize>, GenericAntlrModelError> {
    let mut recovered = BTreeSet::new();
    for (index, node) in nodes.iter().enumerate() {
        original.check()?;
        if node.error || node.missing || node.class == NodeClass::Error {
            recovered.extend(ancestors(nodes, index, original)?);
        }
    }
    Ok(recovered)
}
fn diagnostic_free(
    diagnostics: &[SyntaxDiagnostic],
    span: Option<&std::ops::Range<usize>>,
    original: &SnapshotContext,
) -> Result<bool, GenericAntlrModelError> {
    for diagnostic in diagnostics {
        original.check()?;
        // Inserted tokens are not guaranteed to appear as error descendants.
        // An unlocated diagnostic cannot justify an exact declaration either.
        let (Some(diagnostic), Some(span)) = (&diagnostic.span, span) else {
            return Ok(false);
        };
        if diagnostic.start <= span.end && diagnostic.end >= span.start {
            return Ok(false);
        }
    }
    Ok(true)
}
fn collect_main<'profile, 'o, 'v>(
    raw: &GenericAntlrObservations<'profile, 'o, 'v>,
    input: &PreparedInput<'o, 'v, '_, '_>,
    out: &mut Vec<Candidate<'profile, 'o>>,
) -> Result<(), GenericAntlrModelError> {
    let main = raw.main();
    let recovered = recovered_declarations(&main.syntax.nodes, input.original_context())?;
    for (index, occurrence) in main.occurrences.iter().enumerate() {
        collect_occurrence(
            main,
            &main.syntax.nodes,
            &recovered,
            &main.syntax.diagnostics,
            GenericAntlrTree::Main,
            index,
            occurrence,
            &[],
            matches!(raw, GenericAntlrObservations::Cobol(_)),
            input,
            out,
        )?;
    }
    if let GenericAntlrObservations::Cobol(document) = raw {
        let recovered = recovered_declarations(
            &document.preprocessor.syntax.nodes,
            input.original_context(),
        )?;
        for (index, found) in document.preprocessor.occurrences.iter().enumerate() {
            let contexts = match &found.scope {
                CopyProgramScope::Linked {
                    context_indices, ..
                } => context_indices.as_slice(),
                _ => &[],
            };
            collect_occurrence(
                main,
                &document.preprocessor.syntax.nodes,
                &recovered,
                &document.preprocessor.syntax.diagnostics,
                GenericAntlrTree::CobolPreprocessor,
                index,
                &found.occurrence,
                contexts,
                true,
                input,
                out,
            )?;
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn collect_occurrence<'profile, 'o, 'v>(
    main: &SemanticDocument<'profile, 'o, 'v>,
    nodes: &[SyntaxNode],
    recovered: &BTreeSet<usize>,
    diagnostics: &[SyntaxDiagnostic],
    tree: GenericAntlrTree,
    index: usize,
    occurrence: &SemanticOccurrence<'profile, 'o, 'v>,
    host_contexts: &[usize],
    cobol_padding: bool,
    input: &PreparedInput<'o, 'v, '_, '_>,
    out: &mut Vec<Candidate<'profile, 'o>>,
) -> Result<(), GenericAntlrModelError> {
    let original = input.original_context();
    original.check()?;
    if *occurrence.query.r#type().value() != QueryKind::Entity {
        return Ok(());
    }
    let CaptureBinding::Present(capture) = &occurrence.name else {
        return Ok(());
    };
    let Some(name) = exact(capture) else {
        return Ok(());
    };
    if !clean(nodes, capture.node_index, original)?
        || !clean(nodes, occurrence.matched.node_index, original)?
        || !admit(occurrence.query, name)?
    {
        return Ok(());
    }
    let chain = ancestors(nodes, capture.node_index, original)?;
    let configured = main.profile.declaration_types().value();
    // A matching labeled native ancestor describes the body rather than just
    // its header. If the profile configures a context name, it must share the
    // entity's exact anchor; otherwise its default capture is the whole rule.
    let mut body_context = None;
    if tree == GenericAntlrTree::Main {
        for &index in &occurrence.contexts {
            original.check()?;
            let context =
                main.contexts
                    .get(index)
                    .ok_or(GenericAntlrModelError::NativeContract(
                        "foreign ANTLR context ordinal",
                    ))?;
            if context.label == Some(occurrence.query.graph_label().value().as_str())
                && chain.contains(&context.matched.node_index)
                && (!context.name_is_explicit
                    || bound(&context.name).is_some_and(|anchor| {
                        anchor.start() == name.start() && anchor.end() == name.end()
                    }))
            {
                body_context = Some(context.matched.node_index);
                break;
            }
        }
    }
    let declaration = if let CaptureBinding::Present(span) = &occurrence.span {
        span.node_index
    } else if let Some(index) = body_context {
        index
    } else {
        chain
            .iter()
            .copied()
            .find(|i| {
                nodes[*i].class == NodeClass::Rule
                    && configured.iter().any(|k| k.value() == &nodes[*i].kind)
            })
            .unwrap_or(occurrence.matched.node_index)
    };
    let trusted = diagnostic_free(diagnostics, nodes[declaration].span.as_ref(), original)?;
    let projected_extent = match &occurrence.span {
        CaptureBinding::Present(span) if clean(nodes, span.node_index, original)? => {
            match span.prepared {
                Some(range) => original_extent(input, range.start(), range.end(), cobol_padding)?,
                None => None,
            }
        }
        CaptureBinding::NotConfigured
            if (body_context == Some(declaration)
                || configured
                    .iter()
                    .any(|kind| kind.value() == &nodes[declaration].kind))
                && clean(nodes, declaration, original)? =>
        {
            match &nodes[declaration].span {
                Some(span) => original_extent(input, span.start, span.end, cobol_padding)?,
                None => None,
            }
        }
        _ => None,
    }
    .filter(|(range, _)| {
        trusted
            && !recovered.contains(&declaration)
            && range.start() <= name.start()
            && range.end() >= name.end()
    });
    let extent = projected_extent.map(|(range, _)| range);
    let extent_provenance = projected_extent.map(|(_, provenance)| provenance);
    let mut owner_addresses = ancestors(nodes, declaration, original)?
        .into_iter()
        .skip(1)
        .map(|node_index| GenericAntlrAddress { tree, node_index })
        .collect::<Vec<_>>();
    let contexts = if tree == GenericAntlrTree::Main {
        occurrence.contexts.as_slice()
    } else {
        host_contexts
    };
    let mut frames = Vec::new();
    for &context_index in contexts.iter().rev() {
        original.check()?;
        let context =
            main.contexts
                .get(context_index)
                .ok_or(GenericAntlrModelError::NativeContract(
                    "foreign ANTLR context ordinal",
                ))?;
        if let (Some(label), Some(name)) = (context.label, bound(&context.name))
            && (context.matched.node_index != declaration || tree != GenericAntlrTree::Main)
        {
            frames.push((label, name));
        }
    }
    if tree != GenericAntlrTree::Main {
        for &context_index in host_contexts {
            let context = &main.contexts[context_index];
            owner_addresses.push(GenericAntlrAddress {
                tree: GenericAntlrTree::Main,
                node_index: context.matched.node_index,
            });
        }
    }
    out.push(Candidate {
        query: occurrence.query,
        name,
        extent,
        extent_provenance,
        value: bound(&occurrence.value),
        declaration: GenericAntlrAddress {
            tree,
            node_index: declaration,
        },
        occurrence: GenericAntlrOccurrenceAddress {
            tree,
            query_index: occurrence.query_index,
            match_index: index,
        },
        ancestors: owner_addresses,
        frames,
    });
    Ok(())
}

fn collect_bodies<'profile, 'o, 'v>(
    document: &PostgreSqlSemanticDocument<'profile, 'profile, 'o, 'v>,
    input: &PreparedInput<'o, 'v, '_, '_>,
    out: &mut Vec<Candidate<'profile, 'o>>,
) -> Result<(), GenericAntlrModelError> {
    let original = input.original_context();
    for (body_index, body) in document.bodies.iter().enumerate() {
        let tree = GenericAntlrTree::PostgreSqlBody(body_index);
        let nodes = &body.observations.nodes;
        let mut recovered_declarations = BTreeSet::new();
        for (index, node) in nodes.iter().enumerate() {
            original.check()?;
            if node.error || node.missing {
                let mut current = Some(index);
                while let Some(index) = current {
                    original.check()?;
                    let node = &nodes[index];
                    if node.parent.is_some_and(|parent| parent >= index) {
                        return Err(GenericAntlrModelError::NativeContract(
                            "body parent is not an ancestor",
                        ));
                    }
                    recovered_declarations.insert(index);
                    current = node.parent;
                }
            }
        }
        for scoped in &body.matches {
            original.check()?;
            let group = &body.observations.groups[scoped.query_index];
            let query = group.query.value();
            if *query.r#type().value() != QueryKind::Entity {
                continue;
            }
            let found = &group.matches[scoped.match_index];
            let BodyBinding::Present(names) = &found.bindings.name else {
                continue;
            };
            for &slot in names {
                original.check()?;
                let capture = &found.captures[slot];
                let OriginalProjection::Exact(name) = &capture.original else {
                    continue;
                };
                if !admit(query, *name)? {
                    continue;
                }
                let name_node = scoped.capture_nodes[slot];
                let mut chain = Vec::new();
                let mut current = Some(name_node);
                let mut recovered = false;
                while let Some(index) = current {
                    original.check()?;
                    let node = &nodes[index];
                    recovered |= node.error || node.missing;
                    if node.parent.is_some_and(|parent| parent >= index) {
                        return Err(GenericAntlrModelError::NativeContract(
                            "body parent is not an ancestor",
                        ));
                    }
                    chain.push(index);
                    current = node.parent;
                }
                if recovered {
                    continue;
                }
                let explicit = match &found.bindings.span {
                    BodyBinding::Present(spans) if spans.len() == 1 => Some(spans[0]),
                    _ => None,
                };
                let declaration = explicit
                    .map(|span| scoped.capture_nodes[span])
                    .unwrap_or_else(|| {
                        chain
                            .iter()
                            .copied()
                            .find(|i| {
                                body.observations
                                    .profile
                                    .declaration_types()
                                    .value()
                                    .iter()
                                    .any(|k| k.value() == &nodes[*i].kind)
                            })
                            .unwrap_or(name_node)
                    });
                let projected_extent = if let Some(slot) = explicit {
                    let range = found.captures[slot].prepared;
                    original_extent(input, range.start(), range.end(), false)?
                } else if matches!(found.bindings.span, BodyBinding::NotConfigured)
                    && body
                        .observations
                        .profile
                        .declaration_types()
                        .value()
                        .iter()
                        .any(|k| k.value() == &nodes[declaration].kind)
                {
                    let range = &nodes[declaration].range;
                    original_extent(input, range.start, range.end, false)?
                } else {
                    None
                }
                .filter(|(range, _)| {
                    !recovered_declarations.contains(&declaration)
                        && range.start() <= name.start()
                        && range.end() >= name.end()
                });
                let extent = projected_extent.map(|(range, _)| range);
                let extent_provenance = projected_extent.map(|(_, provenance)| provenance);
                let mut ancestors = chain
                    .iter()
                    .copied()
                    .skip_while(|i| *i != declaration)
                    .skip(1)
                    .map(|node_index| GenericAntlrAddress { tree, node_index })
                    .collect::<Vec<_>>();
                let mut frames = Vec::new();
                for &index in body.host_contexts.iter().rev() {
                    let context = &document.host.contexts[index];
                    if let (Some(label), Some(name)) = (context.label, bound(&context.name)) {
                        frames.push((label, name));
                    }
                }
                for &index in &body.host_contexts {
                    ancestors.push(GenericAntlrAddress {
                        tree: GenericAntlrTree::Main,
                        node_index: document.host.contexts[index].matched.node_index,
                    });
                }
                let value = match &found.bindings.value {
                    BodyBinding::Present(slots) if slots.len() == 1 => {
                        match &found.captures[slots[0]].original {
                            OriginalProjection::Exact(range) => Some(*range),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                out.push(Candidate {
                    query,
                    name: *name,
                    extent,
                    extent_provenance,
                    value,
                    declaration: GenericAntlrAddress {
                        tree,
                        node_index: declaration,
                    },
                    occurrence: GenericAntlrOccurrenceAddress {
                        tree,
                        query_index: scoped.query_index,
                        match_index: scoped.match_index,
                    },
                    ancestors,
                    frames,
                });
            }
        }
    }
    Ok(())
}
