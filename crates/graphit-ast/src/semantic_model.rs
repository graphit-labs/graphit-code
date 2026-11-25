//! JSON profile observations assembled into genuine syntax identities.
//!
//! The parsed owner retains the real collector and Original. The assembled model
//! borrows that owner; it does not publish a graph or restore source from a DTO.

use std::{collections::HashMap, error::Error, fmt, io, sync::Arc};

use graphit_core::file_io::SnapshotContext;
use serde_json::value::RawValue;

use crate::{
    RangeError, SourceRange, SourceText,
    catalog::{EffectiveProfile, ProvenanceView, Query},
    entity_extent::{EntityExtent, EntityExtentError},
    prepared_input::{OriginError, OriginalProjection, PreparedInput},
    profiles::QueryKind,
    semantic_fields::{ContributionConflict, ContributionConflictField, ExtentObservation},
    symbols::{
        IdentityDocument, IdentityDocumentInput, IdentityError, IdentityLimits, SyntaxIdentity,
    },
    tree_sitter_context::{
        ContextError, ContextName, NativeContextObservation, PreparedTreeSitterContexts,
    },
    tree_sitter_profile::{
        CaptureBinding, TreeSitterProfileCapture, TreeSitterProfileError, TreeSitterProfileMatch,
        TreeSitterProfileMatches,
    },
};

#[derive(Debug)]
pub enum SemanticModelError {
    Operation(io::Error),
    Profile(TreeSitterProfileError),
    Context(ContextError),
    Origin(OriginError),
    Identity(IdentityError),
    Extent(EntityExtentError),
    Range(RangeError),
    UnsupportedProfile,
    UnsupportedContextConfiguration,
    UnsupportedQueryConfiguration { query_index: usize },
    NativeContract(&'static str),
}

impl fmt::Display for SemanticModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::Profile(e) => e.fmt(f),
            Self::Context(e) => e.fmt(f),
            Self::Origin(e) => e.fmt(f),
            Self::Identity(e) => e.fmt(f),
            Self::Extent(e) => e.fmt(f),
            Self::Range(e) => e.fmt(f),
            Self::UnsupportedProfile => f.write_str("semantic adapter requires the JSON profile"),
            Self::UnsupportedContextConfiguration => {
                f.write_str("JSON semantic adapter cannot execute this context configuration")
            }
            Self::UnsupportedQueryConfiguration { query_index } => {
                write!(
                    f,
                    "JSON semantic adapter cannot execute query {query_index} configuration"
                )
            }
            Self::NativeContract(message) => f.write_str(message),
        }
    }
}

impl Error for SemanticModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Profile(e) => Some(e),
            Self::Context(e) => Some(e),
            Self::Origin(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Extent(e) => Some(e),
            Self::Range(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for SemanticModelError {
    fn from(value: io::Error) -> Self {
        Self::Operation(value)
    }
}
impl From<TreeSitterProfileError> for SemanticModelError {
    fn from(value: TreeSitterProfileError) -> Self {
        Self::Profile(value)
    }
}
impl From<ContextError> for SemanticModelError {
    fn from(value: ContextError) -> Self {
        Self::Context(value)
    }
}
impl From<OriginError> for SemanticModelError {
    fn from(value: OriginError) -> Self {
        Self::Origin(value)
    }
}
impl From<IdentityError> for SemanticModelError {
    fn from(value: IdentityError) -> Self {
        Self::Identity(value)
    }
}
impl From<EntityExtentError> for SemanticModelError {
    fn from(value: EntityExtentError) -> Self {
        Self::Extent(value)
    }
}
impl From<RangeError> for SemanticModelError {
    fn from(value: RangeError) -> Self {
        Self::Range(value)
    }
}

/// Produced exclusively by collecting an actual PreparedInput.
pub struct ParsedJsonDocument<'profile, 'o, 'v, 'request> {
    observations: TreeSitterProfileMatches<'profile, 'o, 'v>,
    identity: IdentityDocument<'o, 'request>,
    source: SourceText<'o>,
    original: &'request SnapshotContext,
    node_indices: HashMap<usize, usize>,
    enclosing_pairs: Vec<Option<usize>>,
    pair_keys: HashMap<usize, usize>,
    recovery: Vec<bool>,
    contexts: HashMap<usize, NativeContextObservation<'o, 'v>>,
    origins: HashMap<usize, OriginalProjection<'o>>,
}

impl<'profile: 'o, 'o, 'v, 'request> ParsedJsonDocument<'profile, 'o, 'v, 'request> {
    pub fn collect(
        factory: &PreparedTreeSitterContexts<'profile>,
        input: &PreparedInput<'o, 'v, '_, 'request>,
        relative_components: &'o [&'o str],
        limits: IdentityLimits,
    ) -> Result<Self, SemanticModelError> {
        let original = input.original_context();
        original.check()?;
        let result = (|| {
            let (observations, contexts) = factory.collect(input)?.into_parts();
            let profile = observations.profile;
            if profile.language().value() != "json"
                || profile.grammar().value() != "tree-sitter-json"
            {
                return Err(SemanticModelError::UnsupportedProfile);
            }
            validate_configuration(profile)?;
            let source = SourceText::new(input.original_text().as_bytes(), limits.source_bytes())?;
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
            let mut node_indices = HashMap::new();
            let mut enclosing_pairs = Vec::with_capacity(observations.nodes.len());
            let mut pair_keys = HashMap::new();
            let mut recovery = Vec::with_capacity(observations.nodes.len());
            let mut origins = HashMap::new();
            for (index, node) in observations.nodes.iter().enumerate() {
                original.check()?;
                if node_indices.insert(node.node_id, index).is_some() {
                    return Err(SemanticModelError::NativeContract(
                        "duplicate SDK node identity",
                    ));
                }
                let enclosing = if let Some(parent) = node.parent {
                    if parent >= index {
                        return Err(SemanticModelError::NativeContract(
                            "SDK nodes are not parent-first",
                        ));
                    }
                    if observations.nodes[parent].kind == "pair" {
                        Some(parent)
                    } else {
                        enclosing_pairs[parent]
                    }
                } else {
                    None
                };
                enclosing_pairs.push(enclosing);
                recovery.push(node.error || node.missing);
                if node.kind == "string"
                    && node.field.as_deref() == Some("key")
                    && let Some(parent) = node
                        .parent
                        .filter(|&parent| observations.nodes[parent].kind == "pair")
                {
                    pair_keys.insert(parent, index);
                }
                if matches!(
                    node.kind.as_str(),
                    "pair" | "string" | "number" | "true" | "false" | "null"
                ) {
                    let start = u64::try_from(node.range.start).map_err(|_| {
                        SemanticModelError::NativeContract("SDK byte offset cannot fit u64")
                    })?;
                    let end = u64::try_from(node.range.end).map_err(|_| {
                        SemanticModelError::NativeContract("SDK byte offset cannot fit u64")
                    })?;
                    origins.insert(index, input.project_prepared(start, end)?);
                }
            }
            for index in (0..observations.nodes.len()).rev() {
                original.check()?;
                if let Some(parent) = observations.nodes[index].parent {
                    recovery[parent] |= recovery[index];
                }
            }
            let contexts = contexts
                .into_iter()
                .map(|context| (context.trigger.node_index, context))
                .collect();
            Ok(Self {
                observations,
                identity,
                source,
                original,
                node_indices,
                enclosing_pairs,
                pair_keys,
                recovery,
                contexts,
                origins,
            })
        })();
        original.check()?;
        result
    }

    pub(crate) fn source(&self) -> &'o str {
        self.source.as_str()
    }
    pub(crate) fn original_context(&self) -> &'request SnapshotContext {
        self.original
    }
    pub(crate) fn identity_document(&self) -> &IdentityDocument<'o, 'request> {
        &self.identity
    }

    pub fn observations(&self) -> &TreeSitterProfileMatches<'profile, 'o, 'v> {
        &self.observations
    }

    pub fn assemble(
        &self,
    ) -> Result<JsonSemanticDocument<'_, 'profile, 'o, 'v, 'request>, SemanticModelError> {
        self.original.check()?;
        let result = (|| {
            let mut assembly = Assembly {
                parsed: self,
                entities: Vec::new(),
                diagnostics: Vec::new(),
                entity_indices: HashMap::new(),
            };
            for (group_index, group) in self.observations.groups.iter().enumerate() {
                self.original.check()?;
                for (match_index, matched) in group.matches.iter().enumerate() {
                    self.original.check()?;
                    assembly.observe(group_index, match_index, group.query.value(), matched)?;
                }
            }
            assembly.associate_diagnostics()?;
            Ok(JsonSemanticDocument {
                parsed: self,
                entities: assembly.entities,
                diagnostics: assembly.diagnostics,
            })
        })();
        self.original.check()?;
        result
    }

    fn node(&self, index: usize) -> &crate::tree_sitter_engine::SyntaxNode {
        &self.observations.nodes[index]
    }

    fn captured_node(
        &self,
        capture: &TreeSitterProfileCapture<'o, 'v>,
    ) -> Result<usize, SemanticModelError> {
        self.node_indices
            .get(&capture.native.node_id)
            .copied()
            .ok_or(SemanticModelError::NativeContract(
                "capture is not in the retained SDK tree",
            ))
    }

    fn lexical_node(&self, index: usize) -> Option<usize> {
        match self.node(index).kind.as_str() {
            "string_content" => self
                .node(index)
                .parent
                .filter(|&parent| self.node(parent).kind == "string"),
            "string" | "number" | "true" | "false" | "null" => Some(index),
            _ => None,
        }
    }

    fn key_pair(&self, lexical: usize) -> Option<usize> {
        (self.node(lexical).kind == "string" && self.node(lexical).field.as_deref() == Some("key"))
            .then_some(self.node(lexical).parent)
            .flatten()
            .filter(|&parent| self.node(parent).kind == "pair")
    }

    fn pair_key(&self, pair: usize) -> Option<usize> {
        self.pair_keys.get(&pair).copied()
    }

    fn nearest_pair(&self, index: usize) -> Option<usize> {
        self.enclosing_pairs[index]
    }

    fn ancestor(&self, ancestor: usize, descendant: usize) -> bool {
        let mut current = Some(descendant);
        while let Some(index) = current {
            if index == ancestor {
                return true;
            }
            current = self.node(index).parent;
        }
        false
    }

    fn recovery(&self, index: usize) -> Result<bool, SemanticModelError> {
        self.original.check()?;
        Ok(self.recovery[index])
    }

    fn extent(&self, index: usize) -> Result<ExtentObservation<'_, 'o>, SemanticModelError> {
        let origin = self
            .origins
            .get(&index)
            .ok_or(SemanticModelError::NativeContract(
                "semantic delimiter lacks its original projection",
            ))?;
        match origin {
            OriginalProjection::Exact(range) => Ok(ExtentObservation::Exact(EntityExtent::new(
                &self.source,
                *range,
                self.original,
            )?)),
            _ => Ok(ExtentObservation::NonExact(origin)),
        }
    }
}

fn validate_configuration(profile: &EffectiveProfile) -> Result<(), SemanticModelError> {
    let kinds = profile.context_types();
    if matches!(kinds.provenance(), ProvenanceView::Default)
        || kinds.value().iter().any(|(kind, _)| kind.value() != "pair")
        || !profile.text_normalizers().value().is_empty()
    {
        return Err(SemanticModelError::UnsupportedContextConfiguration);
    }
    for (query_index, query) in profile.queries().value().iter().enumerate() {
        if !query.value().name_reject().value().is_empty() {
            return Err(SemanticModelError::UnsupportedQueryConfiguration { query_index });
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsonOccurrence {
    pub query_index: usize,
    pub match_index: usize,
    pub name_slot: Option<usize>,
    pub value_slot: Option<usize>,
}

#[derive(Debug)]
pub enum JsonIssue {
    UnsupportedQueryKind,
    MissingCapture {
        role: &'static str,
        declared: bool,
    },
    UnsupportedNode {
        role: &'static str,
        node_index: usize,
    },
    Recovery {
        node_index: usize,
    },
    NonExact {
        role: &'static str,
        node_index: usize,
    },
    AmbiguousParent,
    InvalidParent,
    InvalidSpan,
    OriginalLiteralKindMismatch {
        node_index: usize,
    },
    Literal(serde_json::Error),
}

pub struct JsonDiagnostic<'doc, 'o> {
    pub occurrence: JsonOccurrence,
    pub issue: JsonIssue,
    pub original: Option<&'doc OriginalProjection<'o>>,
}

pub struct JsonNamespace<'doc, 'o> {
    pub node_index: usize,
    pub label: &'doc str,
    pub name: Arc<str>,
    pub anchor: SourceRange<'o>,
    pub extent: ExtentObservation<'doc, 'o>,
}

pub enum JsonLiteral<'o> {
    String {
        raw: SourceRange<'o>,
        decoded: Arc<str>,
    },
    Number {
        raw: SourceRange<'o>,
        decoded: &'o RawValue,
    },
    Bool {
        raw: SourceRange<'o>,
        decoded: bool,
    },
    Null {
        raw: SourceRange<'o>,
    },
}

impl<'o> JsonLiteral<'o> {
    pub fn raw(&self) -> SourceRange<'o> {
        match self {
            Self::String { raw, .. }
            | Self::Number { raw, .. }
            | Self::Bool { raw, .. }
            | Self::Null { raw } => *raw,
        }
    }

    fn name(&self) -> Arc<str> {
        match self {
            Self::String { decoded, .. } => Arc::clone(decoded),
            _ => Arc::from(self.raw().text()),
        }
    }

    fn agrees(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String { decoded: a, .. }, Self::String { decoded: b, .. }) => a == b,
            (Self::Number { decoded: a, .. }, Self::Number { decoded: b, .. }) => {
                a.get() == b.get()
            }
            (Self::Bool { decoded: a, .. }, Self::Bool { decoded: b, .. }) => a == b,
            (Self::Null { .. }, Self::Null { .. }) => true,
            _ => false,
        }
    }
}

pub enum JsonValueObservation<'o> {
    NotConfigured,
    Missing,
    Unavailable,
    Present(JsonLiteral<'o>),
}

pub struct JsonContribution<'doc, 'o, 'v> {
    pub occurrence: JsonOccurrence,
    pub name_capture: &'doc TreeSitterProfileCapture<'o, 'v>,
    pub extent: ExtentObservation<'doc, 'o>,
    pub declared_value: JsonValueObservation<'o>,
}

pub struct JsonEntity<'doc, 'o, 'v, 'request> {
    identity: SyntaxIdentity<'doc, 'o, 'request>,
    data_key: &'doc str,
    name: Arc<str>,
    name_raw: SourceRange<'o>,
    namespace: Vec<JsonNamespace<'doc, 'o>>,
    contributions: Vec<JsonContribution<'doc, 'o, 'v>>,
    conflicts: Vec<ContributionConflict>,
    diagnostic_indices: Vec<usize>,
}

pub enum JsonEntityExtent<'a, 'doc, 'o, 'v> {
    Exact(&'a EntityExtent<'o>),
    NonExact(&'doc OriginalProjection<'o>),
    Missing,
    Conflict(&'a [JsonContribution<'doc, 'o, 'v>]),
}

pub enum JsonDeclaredValue<'a, 'doc, 'o, 'v> {
    NotConfigured,
    Missing,
    Unavailable,
    Present(&'a JsonLiteral<'o>),
    Conflict(&'a [JsonContribution<'doc, 'o, 'v>]),
}

impl<'doc, 'o, 'v, 'request> JsonEntity<'doc, 'o, 'v, 'request> {
    pub fn identity(&self) -> &SyntaxIdentity<'doc, 'o, 'request> {
        &self.identity
    }
    pub fn data_key(&self) -> &'doc str {
        self.data_key
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn name_raw(&self) -> SourceRange<'o> {
        self.name_raw
    }
    pub fn namespace(&self) -> &[JsonNamespace<'doc, 'o>] {
        &self.namespace
    }
    pub fn contributions(&self) -> &[JsonContribution<'doc, 'o, 'v>] {
        &self.contributions
    }
    pub fn conflicts(&self) -> &[ContributionConflict] {
        &self.conflicts
    }
    pub fn diagnostic_indices(&self) -> &[usize] {
        &self.diagnostic_indices
    }

    pub fn extent(&self) -> JsonEntityExtent<'_, 'doc, 'o, 'v> {
        if self
            .conflicts
            .iter()
            .any(|conflict| conflict.field == ContributionConflictField::Extent)
        {
            return JsonEntityExtent::Conflict(&self.contributions);
        }
        for contribution in &self.contributions {
            match &contribution.extent {
                ExtentObservation::NonExact(origin) => return JsonEntityExtent::NonExact(origin),
                ExtentObservation::Missing => return JsonEntityExtent::Missing,
                ExtentObservation::Exact(_) => {}
            }
        }
        match &self.contributions[0].extent {
            ExtentObservation::Exact(extent) => JsonEntityExtent::Exact(extent),
            ExtentObservation::NonExact(origin) => JsonEntityExtent::NonExact(origin),
            ExtentObservation::Missing => JsonEntityExtent::Missing,
        }
    }

    pub fn declared_value(&self) -> JsonDeclaredValue<'_, 'doc, 'o, 'v> {
        if self
            .conflicts
            .iter()
            .any(|conflict| conflict.field == ContributionConflictField::Value)
        {
            return JsonDeclaredValue::Conflict(&self.contributions);
        }
        let mut present = None;
        for contribution in &self.contributions {
            match &contribution.declared_value {
                JsonValueObservation::Unavailable => return JsonDeclaredValue::Unavailable,
                JsonValueObservation::Missing => return JsonDeclaredValue::Missing,
                JsonValueObservation::Present(value) => present = Some(value),
                JsonValueObservation::NotConfigured => {}
            }
        }
        present.map_or(JsonDeclaredValue::NotConfigured, JsonDeclaredValue::Present)
    }
}

pub struct JsonSemanticDocument<'doc, 'profile, 'o, 'v, 'request> {
    parsed: &'doc ParsedJsonDocument<'profile, 'o, 'v, 'request>,
    entities: Vec<JsonEntity<'doc, 'o, 'v, 'request>>,
    diagnostics: Vec<JsonDiagnostic<'doc, 'o>>,
}

pub struct JsonCodeProjection<'a, 'doc, 'o, 'v, 'request> {
    pub entity_index: usize,
    pub entity: &'a JsonEntity<'doc, 'o, 'v, 'request>,
    pub extent: &'a EntityExtent<'o>,
    pub text: &'o str,
    pub language: &'a str,
    pub grammar: &'a str,
}

pub struct JsonUnavailableProjection<'a> {
    pub entity_index: usize,
    pub diagnostics: &'a [usize],
    pub conflicts: &'a [ContributionConflict],
}

pub enum JsonProjection<'a, 'doc, 'o, 'v, 'request> {
    Available(JsonCodeProjection<'a, 'doc, 'o, 'v, 'request>),
    Unavailable(JsonUnavailableProjection<'a>),
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> JsonSemanticDocument<'doc, 'profile, 'o, 'v, 'request> {
    pub fn parsed(&self) -> &'doc ParsedJsonDocument<'profile, 'o, 'v, 'request> {
        self.parsed
    }
    pub fn entities(&self) -> &[JsonEntity<'doc, 'o, 'v, 'request>] {
        &self.entities
    }
    pub fn diagnostics(&self) -> &[JsonDiagnostic<'doc, 'o>] {
        &self.diagnostics
    }

    pub fn code_projections(
        &self,
    ) -> Result<Vec<JsonProjection<'_, 'doc, 'o, 'v, 'request>>, SemanticModelError> {
        self.parsed.original.check()?;
        let result = (|| {
            let profile = self.parsed.observations.profile;
            let mut projections = Vec::new();
            for (entity_index, entity) in self.entities.iter().enumerate() {
                self.parsed.original.check()?;
                if !profile
                    .embed_labels()
                    .value()
                    .iter()
                    .any(|label| label.value() == entity.identity.label())
                {
                    continue;
                }
                if entity.diagnostic_indices.is_empty()
                    && entity.conflicts.is_empty()
                    && let JsonEntityExtent::Exact(extent) = entity.extent()
                    && entity
                        .namespace
                        .iter()
                        .all(|owner| matches!(owner.extent, ExtentObservation::Exact(_)))
                {
                    projections.push(JsonProjection::Available(JsonCodeProjection {
                        entity_index,
                        entity,
                        extent,
                        text: extent.range().text(),
                        language: profile.language().value(),
                        grammar: profile.grammar().value(),
                    }));
                } else {
                    projections.push(JsonProjection::Unavailable(JsonUnavailableProjection {
                        entity_index,
                        diagnostics: &entity.diagnostic_indices,
                        conflicts: &entity.conflicts,
                    }));
                }
            }
            Ok(projections)
        })();
        self.parsed.original.check()?;
        result
    }
}

struct Assembly<'doc, 'profile, 'o, 'v, 'request> {
    parsed: &'doc ParsedJsonDocument<'profile, 'o, 'v, 'request>,
    entities: Vec<JsonEntity<'doc, 'o, 'v, 'request>>,
    diagnostics: Vec<JsonDiagnostic<'doc, 'o>>,
    entity_indices: HashMap<(String, &'doc str), usize>,
}

struct EntitySeed<'doc, 'o, 'request> {
    identity: SyntaxIdentity<'doc, 'o, 'request>,
    data_key: &'doc str,
    name: Arc<str>,
    name_raw: SourceRange<'o>,
    namespace: Vec<JsonNamespace<'doc, 'o>>,
}

impl<'doc, 'profile: 'o, 'o, 'v, 'request> Assembly<'doc, 'profile, 'o, 'v, 'request> {
    fn diagnostic(
        &mut self,
        occurrence: JsonOccurrence,
        issue: JsonIssue,
        original: Option<&'doc OriginalProjection<'o>>,
    ) -> usize {
        let index = self.diagnostics.len();
        self.diagnostics.push(JsonDiagnostic {
            occurrence,
            issue,
            original,
        });
        index
    }

    fn literal(
        &mut self,
        node_index: usize,
        occurrence: JsonOccurrence,
        role: &'static str,
    ) -> Result<Option<JsonLiteral<'o>>, SemanticModelError> {
        self.parsed.original.check()?;
        if self.parsed.recovery(node_index)? {
            self.diagnostic(
                occurrence,
                JsonIssue::Recovery { node_index },
                self.parsed.origins.get(&node_index),
            );
            return Ok(None);
        }
        let origin =
            self.parsed
                .origins
                .get(&node_index)
                .ok_or(SemanticModelError::NativeContract(
                    "literal is not a projected SDK node",
                ))?;
        let OriginalProjection::Exact(raw) = origin else {
            self.diagnostic(
                occurrence,
                JsonIssue::NonExact { role, node_index },
                Some(origin),
            );
            return Ok(None);
        };
        let decoded = match self.parsed.node(node_index).kind.as_str() {
            "string" => {
                serde_json::from_str::<String>(raw.text()).map(|value| JsonLiteral::String {
                    raw: *raw,
                    decoded: Arc::from(value),
                })
            }
            "number" => serde_json::from_str::<&RawValue>(raw.text())
                .map(|decoded| JsonLiteral::Number { raw: *raw, decoded }),
            "true" | "false" => serde_json::from_str::<bool>(raw.text())
                .map(|decoded| JsonLiteral::Bool { raw: *raw, decoded }),
            "null" => {
                serde_json::from_str::<()>(raw.text()).map(|()| JsonLiteral::Null { raw: *raw })
            }
            _ => {
                return Err(SemanticModelError::NativeContract(
                    "invalid literal decoder dispatch",
                ));
            }
        };
        match decoded {
            Ok(value) => {
                // RawValue has already validated the whole JSON value. Its
                // lexical category must also agree with the prepared SDK node;
                // origin Exact alone does not establish that agreement.
                if let JsonLiteral::Number { decoded, .. } = &value
                    && !matches!(decoded.get().as_bytes().first(), Some(b'-' | b'0'..=b'9'))
                {
                    self.diagnostic(
                        occurrence,
                        JsonIssue::OriginalLiteralKindMismatch { node_index },
                        Some(origin),
                    );
                    return Ok(None);
                }
                Ok(Some(value))
            }
            Err(cause) => {
                self.diagnostic(occurrence, JsonIssue::Literal(cause), Some(origin));
                Ok(None)
            }
        }
    }

    fn namespace(
        &mut self,
        lexical: usize,
        skip_pair: Option<usize>,
        explicit: Option<(usize, &'doc str)>,
        occurrence: JsonOccurrence,
    ) -> Result<Option<Vec<JsonNamespace<'doc, 'o>>>, SemanticModelError> {
        let mut owners = Vec::new();
        let mut current = self.parsed.nearest_pair(lexical);
        while let Some(parent) = current {
            self.parsed.original.check()?;
            let explicitly_owned = explicit.is_some_and(|(pair, _)| pair == parent);
            let names_this_capture = self.parsed.contexts.get(&parent).is_some_and(|context| {
                matches!(&context.name, ContextName::Found { selected, .. }
                    if self.parsed.lexical_node(selected.node_index) == Some(lexical))
            });
            if Some(parent) != skip_pair && (explicitly_owned || !names_this_capture) {
                let label = explicit
                    .filter(|(pair, _)| *pair == parent)
                    .map(|(_, label)| label)
                    .or_else(|| {
                        self.parsed
                            .contexts
                            .get(&parent)
                            .map(|context| context.label.as_str())
                    });
                if let Some(label) = label {
                    let key = match self
                        .parsed
                        .contexts
                        .get(&parent)
                        .map(|context| &context.name)
                    {
                        Some(ContextName::Found { selected, .. }) => {
                            self.parsed.lexical_node(selected.node_index)
                        }
                        Some(ContextName::Missing { .. }) => None,
                        None => self.parsed.pair_key(parent),
                    };
                    let Some(key) = key else {
                        self.diagnostic(
                            occurrence,
                            JsonIssue::InvalidParent,
                            self.parsed.origins.get(&parent),
                        );
                        return Ok(None);
                    };
                    let Some(name) = self.literal(key, occurrence, "owner")? else {
                        return Ok(None);
                    };
                    let extent = self.parsed.extent(parent)?;
                    if self.parsed.recovery(parent)? {
                        self.diagnostic(
                            occurrence,
                            JsonIssue::Recovery { node_index: parent },
                            self.parsed.origins.get(&parent),
                        );
                    }
                    if !matches!(extent, ExtentObservation::Exact(_)) {
                        self.diagnostic(
                            occurrence,
                            JsonIssue::NonExact {
                                role: "owner",
                                node_index: parent,
                            },
                            self.parsed.origins.get(&parent),
                        );
                    }
                    owners.push(JsonNamespace {
                        node_index: parent,
                        label,
                        name: name.name(),
                        anchor: name.raw(),
                        extent,
                    });
                }
            }
            current = self.parsed.nearest_pair(parent);
        }
        owners.reverse();
        Ok(Some(owners))
    }

    fn identity(
        &self,
        label: &str,
        anchor: SourceRange<'o>,
        name: &str,
        namespace: &[JsonNamespace<'doc, 'o>],
    ) -> Result<SyntaxIdentity<'doc, 'o, 'request>, SemanticModelError> {
        let mut owners = Vec::new();
        for owner in namespace {
            self.parsed.original.check()?;
            let preceding: Vec<_> = owners.iter().collect();
            let identity = self.parsed.identity.symbol(
                owner.label,
                owner.anchor,
                &preceding,
                crate::symbols::LogicalDeclaration::named(&owner.name),
            )?;
            owners.push(identity);
        }
        let frames: Vec<_> = owners.iter().collect();
        Ok(self.parsed.identity.symbol(
            label,
            anchor,
            &frames,
            crate::symbols::LogicalDeclaration::named(name),
        )?)
    }

    fn associate_diagnostics(&mut self) -> Result<(), SemanticModelError> {
        if self.diagnostics.is_empty() {
            return Ok(());
        }
        // Index actual declaration observations only. A failed owner selection
        // must not create an identity, but can invalidate declarations genuinely
        // produced for that same native subject by another query, in either order.
        let mut targets: HashMap<(usize, &'doc str, &'doc str), Vec<usize>> = HashMap::new();
        for (entity_index, entity) in self.entities.iter().enumerate() {
            self.parsed.original.check()?;
            for contribution in &entity.contributions {
                let query = self.parsed.observations.groups[contribution.occurrence.query_index]
                    .query
                    .value();
                if entity.identity.label() != query.graph_label().value() {
                    continue;
                }
                let captured = self.parsed.captured_node(contribution.name_capture)?;
                if let Some(lexical) = self.parsed.lexical_node(captured) {
                    let indices = targets
                        .entry((
                            lexical,
                            query.graph_label().value(),
                            query.data_key().value(),
                        ))
                        .or_default();
                    if !indices.contains(&entity_index) {
                        indices.push(entity_index);
                    }
                }
            }
        }
        for (diagnostic_index, diagnostic) in self.diagnostics.iter().enumerate() {
            self.parsed.original.check()?;
            let Some(slot) = diagnostic.occurrence.name_slot else {
                continue;
            };
            let group = &self.parsed.observations.groups[diagnostic.occurrence.query_index];
            let query = group.query.value();
            let capture = &group.matches[diagnostic.occurrence.match_index].captures[slot];
            let node = self.parsed.captured_node(capture)?;
            if let Some(lexical) = self.parsed.lexical_node(node)
                && let Some(indices) = targets.get(&(
                    lexical,
                    query.graph_label().value().as_str(),
                    query.data_key().value().as_str(),
                ))
            {
                for &index in indices {
                    self.entities[index]
                        .diagnostic_indices
                        .push(diagnostic_index);
                }
            }
        }
        for entity in &mut self.entities {
            entity.diagnostic_indices.sort_unstable();
            entity.diagnostic_indices.dedup();
        }
        Ok(())
    }

    fn add(
        &mut self,
        seed: EntitySeed<'doc, 'o, 'request>,
        contribution: JsonContribution<'doc, 'o, 'v>,
        diagnostics: Vec<usize>,
    ) -> usize {
        let EntitySeed {
            identity,
            data_key,
            name,
            name_raw,
            namespace,
        } = seed;
        let key = (identity.uid().as_str().to_owned(), data_key);
        if let Some(&index) = self.entity_indices.get(&key) {
            let entity = &mut self.entities[index];
            let next = entity.contributions.len();
            for (previous, other) in entity.contributions.iter().enumerate() {
                if let (ExtentObservation::Exact(a), ExtentObservation::Exact(b)) =
                    (&other.extent, &contribution.extent)
                    && (a.range().start() != b.range().start()
                        || a.range().end() != b.range().end())
                {
                    entity.conflicts.push(ContributionConflict {
                        field: ContributionConflictField::Extent,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
                if let (JsonValueObservation::Present(a), JsonValueObservation::Present(b)) =
                    (&other.declared_value, &contribution.declared_value)
                    && !a.agrees(b)
                {
                    entity.conflicts.push(ContributionConflict {
                        field: ContributionConflictField::Value,
                        first_contribution: previous,
                        second_contribution: next,
                    });
                }
            }
            entity.contributions.push(contribution);
            entity.diagnostic_indices.extend(diagnostics);
            index
        } else {
            let index = self.entities.len();
            self.entities.push(JsonEntity {
                identity,
                data_key,
                name,
                name_raw,
                namespace,
                contributions: vec![contribution],
                conflicts: Vec::new(),
                diagnostic_indices: diagnostics,
            });
            self.entity_indices.insert(key, index);
            index
        }
    }

    fn slots<'a>(
        &mut self,
        binding: &'a CaptureBinding,
        occurrence: JsonOccurrence,
        role: &'static str,
    ) -> &'a [usize] {
        match binding {
            CaptureBinding::Present(slots) => slots,
            CaptureBinding::Missing { declared } => {
                self.diagnostic(
                    occurrence,
                    JsonIssue::MissingCapture {
                        role,
                        declared: *declared,
                    },
                    None,
                );
                &[]
            }
            CaptureBinding::NotConfigured => &[],
        }
    }

    fn parent(
        &mut self,
        matched: &'doc TreeSitterProfileMatch<'o, 'v>,
        lexical: usize,
        skip_pair: Option<usize>,
        occurrence: JsonOccurrence,
        query: &'doc Query,
    ) -> Result<Option<Option<(usize, &'doc str)>>, SemanticModelError> {
        if matches!(matched.bindings.parent, CaptureBinding::NotConfigured) {
            return Ok(Some(None));
        }
        let mut parent = None;
        let slots = self.slots(&matched.bindings.parent, occurrence, "parent");
        if slots.is_empty() {
            return Ok(None);
        }
        for &slot in slots {
            self.parsed.original.check()?;
            let captured = &matched.captures[slot];
            let node = self.parsed.captured_node(captured)?;
            let selected = self
                .parsed
                .lexical_node(node)
                .and_then(|node| self.parsed.key_pair(node));
            let Some(pair) = selected
                .filter(|&pair| Some(pair) != skip_pair && self.parsed.ancestor(pair, lexical))
            else {
                self.diagnostic(
                    occurrence,
                    JsonIssue::InvalidParent,
                    Some(&captured.original),
                );
                return Ok(None);
            };
            if parent.is_some_and(|previous| previous != pair) {
                self.diagnostic(
                    occurrence,
                    JsonIssue::AmbiguousParent,
                    Some(&captured.original),
                );
                return Ok(None);
            }
            parent = Some(pair);
        }
        Ok(Some(
            parent.map(|pair| (pair, query.parent_label().value().as_str())),
        ))
    }

    fn observe(
        &mut self,
        query_index: usize,
        match_index: usize,
        query: &'doc Query,
        matched: &'doc TreeSitterProfileMatch<'o, 'v>,
    ) -> Result<(), SemanticModelError> {
        let base = JsonOccurrence {
            query_index,
            match_index,
            name_slot: None,
            value_slot: None,
        };
        if *query.r#type().value() != QueryKind::Entity {
            self.diagnostic(base, JsonIssue::UnsupportedQueryKind, None);
            return Ok(());
        }
        let names = self.slots(&matched.bindings.name, base, "name");
        for &name_slot in names {
            self.parsed.original.check()?;
            let occurrence = JsonOccurrence {
                name_slot: Some(name_slot),
                ..base
            };
            let first_diagnostic = self.diagnostics.len();
            let name_capture = &matched.captures[name_slot];
            let captured_node = self.parsed.captured_node(name_capture)?;
            if self.parsed.node(captured_node).error || self.parsed.node(captured_node).missing {
                self.diagnostic(
                    occurrence,
                    JsonIssue::Recovery {
                        node_index: captured_node,
                    },
                    Some(&name_capture.original),
                );
                continue;
            }
            let Some(lexical) = self.parsed.lexical_node(captured_node) else {
                self.diagnostic(
                    occurrence,
                    JsonIssue::UnsupportedNode {
                        role: "name",
                        node_index: captured_node,
                    },
                    Some(&name_capture.original),
                );
                continue;
            };
            let Some(name) = self.literal(lexical, occurrence, "name")? else {
                continue;
            };
            let pair = self.parsed.key_pair(lexical);
            let delimiter = pair.unwrap_or(lexical);
            let Some(parent) = self.parent(matched, lexical, pair, occurrence, query)? else {
                continue;
            };
            let Some(namespace) = self.namespace(lexical, pair, parent, occurrence)? else {
                continue;
            };
            let identity = self.identity(
                query.graph_label().value(),
                name.raw(),
                &name.name(),
                &namespace,
            )?;
            let mut spans = Vec::new();
            match &matched.bindings.span {
                CaptureBinding::NotConfigured => spans.push(Some(delimiter)),
                CaptureBinding::Missing { declared } => {
                    self.diagnostic(
                        occurrence,
                        JsonIssue::MissingCapture {
                            role: "span",
                            declared: *declared,
                        },
                        None,
                    );
                    spans.push(None);
                }
                CaptureBinding::Present(slots) => {
                    for &slot in slots {
                        let capture = &matched.captures[slot];
                        let selected = self.parsed.captured_node(capture)?;
                        let valid = if pair.is_some() {
                            self.parsed.node(selected).kind == "pair"
                                && self.parsed.ancestor(selected, lexical)
                        } else {
                            selected == lexical
                        };
                        if valid {
                            spans.push(Some(selected));
                        } else {
                            self.diagnostic(
                                occurrence,
                                JsonIssue::InvalidSpan,
                                Some(&capture.original),
                            );
                            spans.push(None);
                        }
                    }
                }
            }
            let value_slots: Vec<Option<usize>> = match &matched.bindings.value {
                CaptureBinding::Present(slots) => slots.iter().copied().map(Some).collect(),
                _ => vec![None],
            };
            let mut entity_index = None;
            let mut first_identity = Some(identity);
            for span in spans {
                for &value_slot in &value_slots {
                    self.parsed.original.check()?;
                    let contribution_start = self.diagnostics.len();
                    let occurrence = JsonOccurrence {
                        value_slot,
                        ..occurrence
                    };
                    let extent = if let Some(span) = span {
                        if self.parsed.recovery(span)? {
                            self.diagnostic(
                                occurrence,
                                JsonIssue::Recovery { node_index: span },
                                self.parsed.origins.get(&span),
                            );
                        }
                        let extent = self.parsed.extent(span)?;
                        if !matches!(extent, ExtentObservation::Exact(_)) {
                            self.diagnostic(
                                occurrence,
                                JsonIssue::NonExact {
                                    role: "extent",
                                    node_index: span,
                                },
                                self.parsed.origins.get(&span),
                            );
                        }
                        extent
                    } else {
                        ExtentObservation::Missing
                    };
                    let mut child = None;
                    let declared_value = match (&matched.bindings.value, value_slot) {
                        (CaptureBinding::NotConfigured, _) => JsonValueObservation::NotConfigured,
                        (CaptureBinding::Missing { declared }, _) => {
                            self.diagnostic(
                                occurrence,
                                JsonIssue::MissingCapture {
                                    role: "value",
                                    declared: *declared,
                                },
                                None,
                            );
                            JsonValueObservation::Missing
                        }
                        (_, Some(slot)) => {
                            let capture = &matched.captures[slot];
                            let node = self.parsed.captured_node(capture)?;
                            let value_node = self.parsed.lexical_node(node).filter(|&node| {
                                pair.is_some() && self.parsed.nearest_pair(node) == pair
                            });
                            if let Some(node) = value_node {
                                match self.literal(node, occurrence, "value")? {
                                    Some(value) => {
                                        child = Some((node, value.name(), value.raw(), slot));
                                        JsonValueObservation::Present(value)
                                    }
                                    None => JsonValueObservation::Unavailable,
                                }
                            } else {
                                self.diagnostic(
                                    occurrence,
                                    JsonIssue::UnsupportedNode {
                                        role: "value",
                                        node_index: node,
                                    },
                                    Some(&capture.original),
                                );
                                JsonValueObservation::Unavailable
                            }
                        }
                        _ => JsonValueObservation::Missing,
                    };
                    let identity = first_identity.take().map_or_else(
                        || {
                            self.identity(
                                query.graph_label().value(),
                                name.raw(),
                                &name.name(),
                                &namespace,
                            )
                        },
                        Ok,
                    )?;
                    let own_namespace = self.namespace(lexical, pair, parent, occurrence)?.ok_or(
                        SemanticModelError::NativeContract("validated JSON namespace disappeared"),
                    )?;
                    let diagnostics: Vec<_> = (first_diagnostic..contribution_start)
                        .chain(contribution_start..self.diagnostics.len())
                        .collect();
                    let index = self.add(
                        EntitySeed {
                            identity,
                            data_key: query.data_key().value(),
                            name: name.name(),
                            name_raw: name.raw(),
                            namespace: own_namespace,
                        },
                        JsonContribution {
                            occurrence,
                            name_capture,
                            extent,
                            declared_value,
                        },
                        diagnostics,
                    );
                    entity_index = Some(index);
                    if let Some((node, child_name, anchor, slot)) = child
                        && !query.value_label().value().is_empty()
                    {
                        let first_child_diagnostic = self.diagnostics.len();
                        let child_namespace = self.namespace(
                            node,
                            None,
                            pair.map(|pair| (pair, query.graph_label().value().as_str())),
                            occurrence,
                        )?;
                        if let Some(child_namespace) = child_namespace {
                            let child_identity = self.identity(
                                query.value_label().value(),
                                anchor,
                                &child_name,
                                &child_namespace,
                            )?;
                            let child_extent = self.parsed.extent(node)?;
                            let child_diagnostics =
                                (first_child_diagnostic..self.diagnostics.len()).collect();
                            self.add(
                                EntitySeed {
                                    identity: child_identity,
                                    data_key: query.data_key().value(),
                                    name: child_name,
                                    name_raw: anchor,
                                    namespace: child_namespace,
                                },
                                JsonContribution {
                                    occurrence,
                                    name_capture: &matched.captures[slot],
                                    extent: child_extent,
                                    declared_value: JsonValueObservation::NotConfigured,
                                },
                                child_diagnostics,
                            );
                        }
                    }
                }
            }
            if let Some(index) = entity_index {
                self.entities[index]
                    .diagnostic_indices
                    .extend(first_diagnostic..self.diagnostics.len());
                self.entities[index].diagnostic_indices.sort_unstable();
                self.entities[index].diagnostic_indices.dedup();
            }
        }
        Ok(())
    }
}
