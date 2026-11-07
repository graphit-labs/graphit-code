use super::*;
use crate::{
    entity_extent::EntityExtent, profiles::QueryKind, symbols::SyntaxIdentity,
    tree_sitter_context::ContextName,
};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericOccurrence {
    pub query_index: usize,
    pub match_index: usize,
    pub name_slot: usize,
    pub value_slot: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericEntityIssue {
    Name(GenericNameIssue),
    MissingName,
    MissingValue,
    AmbiguousValue,
    SelfValue,
    NonExactValue,
    RecoveredValue,
    MissingParent,
    AmbiguousParent,
    IncompleteContext,
    MissingContext,
    InvalidContext,
    NonExactContextExtent,
    AmbiguousContext,
    OwnerCycle,
    InconsistentOwnerChain,
    ConflictingName,
    ConflictingExtent,
    ConflictingDeclaration,
    InvalidDeclaration,
    ConflictingValue,
}

#[derive(Clone, Debug)]
pub struct GenericEntityDiagnostic {
    pub occurrence: Option<GenericOccurrence>,
    pub context_index: Option<usize>,
    pub issue: GenericEntityIssue,
}

/// A concrete SDK recovery node, even when no entity query matched.
pub struct GenericSyntaxRecovery<'o> {
    pub node_index: usize,
    pub node_id: usize,
    pub kind: String,
    pub error: bool,
    pub missing: bool,
    pub prepared: std::ops::Range<usize>,
    pub original: OriginalProjection<'o>,
}

pub struct GenericContribution<'o> {
    pub occurrence: GenericOccurrence,
    pub name_node_index: usize,
    pub extent: GenericExtent<'o>,
    pub declaration: GenericDeclaration,
    pub contexts: Vec<GenericOwnedContext>,
    pub values: Vec<SourceRange<'o>>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum GenericOwnerRef {
    Entity(usize),
    Namespace(usize),
}

struct NamespaceDetails<'o> {
    extent: GenericExtent<'o>,
    conflicts: Vec<GenericEntityIssue>,
}

/// One original native owner, in the retained identity's outer-to-inner order.
pub struct GenericOwnerFrame<'doc, 'o> {
    pub label: &'doc str,
    pub anchor: SourceRange<'o>,
    pub extent: Option<&'doc EntityExtent<'o>>,
    pub conflicts: &'doc [GenericEntityIssue],
}

pub struct GenericEntity<'doc, 'o, 'request> {
    pub(super) identity: SyntaxIdentity<'doc, 'o, 'request>,
    pub(super) name: GenericName<'o>,
    pub(super) data_key: String,
    pub(super) contributions: Vec<GenericContribution<'o>>,
    pub(super) owners: Vec<usize>,
    owner_chain: Vec<GenericOwnerRef>,
    pub(super) conflicts: Vec<GenericEntityIssue>,
    pub(super) value: Option<SourceRange<'o>>,
}

impl<'doc, 'o, 'request> GenericEntity<'doc, 'o, 'request> {
    pub fn identity(&self) -> &SyntaxIdentity<'doc, 'o, 'request> {
        &self.identity
    }
    pub fn name(&self) -> GenericName<'o> {
        self.name
    }
    pub fn data_key(&self) -> &str {
        &self.data_key
    }
    pub fn contributions(&self) -> &[GenericContribution<'o>] {
        &self.contributions
    }
    pub fn owners(&self) -> &[usize] {
        &self.owners
    }
    pub fn conflicts(&self) -> &[GenericEntityIssue] {
        &self.conflicts
    }
    pub fn value(&self) -> Option<SourceRange<'o>> {
        self.value
    }
    pub fn extent(&self) -> Option<&EntityExtent<'o>> {
        if self
            .conflicts
            .contains(&GenericEntityIssue::ConflictingExtent)
        {
            return None;
        }
        self.contributions.iter().find_map(|c| match &c.extent {
            GenericExtent::Exact { extent, .. } => Some(extent),
            _ => None,
        })
    }
}

/// Borrows the retained identity document and observations; consumers cannot outlive the parse.
pub struct GenericSemanticDocument<'doc, 'profile, 'o, 'v, 'request> {
    pub(super) identity: &'doc IdentityDocument<'o, 'request>,
    pub(super) observations: &'doc TreeSitterProfileMatches<'profile, 'o, 'v>,
    pub(super) original: &'request SnapshotContext,
    pub(super) source: SourceText<'o>,
    pub(super) entities: Vec<GenericEntity<'doc, 'o, 'request>>,
    pub(super) namespaces: Vec<SyntaxIdentity<'doc, 'o, 'request>>,
    namespace_details: Vec<NamespaceDetails<'o>>,
    pub(super) diagnostics: Vec<GenericEntityDiagnostic>,
    pub(super) syntax_recoveries: Vec<GenericSyntaxRecovery<'o>>,
    pub(super) exports: Vec<GenericExport>,
    pub(super) complexities: Vec<GenericComplexity>,
    pub(super) relations: Vec<GenericRelation>,
    pub(super) scopes: Vec<GenericSemanticScope>,
}

impl<'doc, 'profile, 'o, 'v, 'request> GenericSemanticDocument<'doc, 'profile, 'o, 'v, 'request> {
    pub fn exports(&self) -> Result<&[GenericExport], GenericModelError> {
        self.original.check()?;
        Ok(&self.exports)
    }
    pub fn complexities(&self) -> Result<&[GenericComplexity], GenericModelError> {
        self.original.check()?;
        Ok(&self.complexities)
    }
    pub fn relations(&self) -> Result<&[GenericRelation], GenericModelError> {
        self.original.check()?;
        Ok(&self.relations)
    }
    pub fn identity_document(
        &self,
    ) -> Result<&'doc IdentityDocument<'o, 'request>, GenericModelError> {
        self.original.check()?;
        Ok(self.identity)
    }
    pub fn original_context(&self) -> Result<&'request SnapshotContext, GenericModelError> {
        self.original.check()?;
        Ok(self.original)
    }
    pub fn original_text(&self) -> Result<&'o str, GenericModelError> {
        self.original.check()?;
        Ok(self.source.as_str())
    }

    pub fn source(&self) -> Result<SourceText<'o>, GenericModelError> {
        self.original.check()?;
        Ok(self.source.clone())
    }
    pub fn observations(
        &self,
    ) -> Result<&'doc TreeSitterProfileMatches<'profile, 'o, 'v>, GenericModelError> {
        self.original.check()?;
        Ok(self.observations)
    }
    pub fn entities(&self) -> Result<&[GenericEntity<'doc, 'o, 'request>], GenericModelError> {
        self.original.check()?;
        Ok(&self.entities)
    }
    pub fn owner_frames(
        &self,
        entity_index: usize,
    ) -> Result<impl Iterator<Item = GenericOwnerFrame<'_, 'o>>, GenericModelError> {
        self.original.check()?;
        let entity = self
            .entities
            .get(entity_index)
            .ok_or(GenericModelError::NativeContract(
                "owner frame entity is outside the retained document",
            ))?;
        Ok(entity.owner_chain.iter().map(|owner| match *owner {
            GenericOwnerRef::Entity(index) => {
                let owner = &self.entities[index];
                GenericOwnerFrame {
                    label: owner.identity.label(),
                    anchor: owner.name.raw(),
                    extent: owner.extent(),
                    conflicts: &owner.conflicts,
                }
            }
            GenericOwnerRef::Namespace(index) => {
                let identity = &self.namespaces[index];
                let details = &self.namespace_details[index];
                let extent = match &details.extent {
                    GenericExtent::Exact { extent, .. } => Some(extent),
                    _ => None,
                };
                GenericOwnerFrame {
                    label: identity.label(),
                    anchor: *identity.subject(),
                    extent,
                    conflicts: &details.conflicts,
                }
            }
        }))
    }

    pub fn namespaces(&self) -> Result<&[SyntaxIdentity<'doc, 'o, 'request>], GenericModelError> {
        self.original.check()?;
        Ok(&self.namespaces)
    }
    pub fn syntax_recoveries(&self) -> Result<&[GenericSyntaxRecovery<'o>], GenericModelError> {
        self.original.check()?;
        Ok(&self.syntax_recoveries)
    }
    pub fn diagnostics(&self) -> Result<&[GenericEntityDiagnostic], GenericModelError> {
        self.original.check()?;
        Ok(&self.diagnostics)
    }
}

pub(super) enum GenericScopeOwner {
    Entities(Vec<usize>),
    Namespace,
    Unresolved,
}
pub(super) struct GenericSemanticScope {
    pub node_index: usize,
    pub owner: GenericScopeOwner,
}

enum CandidateOrigin {
    Query(GenericOccurrence),
    Value {
        occurrence: GenericOccurrence,
        owner: usize,
    },
    Context(usize),
}
struct Candidate<'o> {
    origin: CandidateOrigin,
    label: String,
    data_key: String,
    name: GenericName<'o>,
    name_node: usize,
    extent: GenericExtent<'o>,
    declaration: GenericDeclaration,
    contexts: Vec<GenericOwnedContext>,
    issues: Vec<GenericEntityIssue>,
    values: Vec<SourceRange<'o>>,
}
impl Candidate<'_> {
    fn occurrence(&self) -> Option<GenericOccurrence> {
        match self.origin {
            CandidateOrigin::Query(o) | CandidateOrigin::Value { occurrence: o, .. } => Some(o),
            CandidateOrigin::Context(_) => None,
        }
    }
    fn context_index(&self) -> Option<usize> {
        match self.origin {
            CandidateOrigin::Context(c) => Some(c),
            _ => None,
        }
    }
}
struct OwnerPlan {
    valid: bool,
    parent: Option<usize>,
    expected: Option<Vec<usize>>,
}

fn same_subject(a: &Candidate<'_>, b: &Candidate<'_>) -> bool {
    a.name_node == b.name_node
        && a.label == b.label
        && a.name.raw().start() == b.name.raw().start()
        && a.name.raw().end() == b.name.raw().end()
}

fn physical_candidates(
    candidates: &[Candidate<'_>],
    node: usize,
    label: Option<&str>,
) -> Vec<usize> {
    let mut result = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if label.is_some_and(|label| label != candidate.label)
            || !(candidate.name_node == node
                || matches!(candidate.extent, GenericExtent::Exact { node_index, .. } if node_index == node)
                || matches!(candidate.declaration, GenericDeclaration::Native { node_index, .. } if node_index == node))
        {
            continue;
        }
        if !result
            .iter()
            .any(|&prior| same_subject(&candidates[prior], candidate))
        {
            result.push(index);
        }
    }
    result
}

fn chain(
    index: usize,
    plans: &[OwnerPlan],
    state: &mut [u8],
    chains: &mut [Option<Vec<usize>>],
    original: &SnapshotContext,
) -> Result<Option<Vec<usize>>, GenericModelError> {
    original.check()?;
    if state[index] == 2 {
        return Ok(chains[index].clone());
    }
    if state[index] == 1 || !plans[index].valid {
        return Ok(None);
    }
    state[index] = 1;
    let result = if let Some(parent) = plans[index].parent {
        chain(parent, plans, state, chains, original)?.map(|mut chain| {
            chain.push(parent);
            chain
        })
    } else {
        Some(Vec::new())
    };
    let result = result.filter(|chain| {
        plans[index]
            .expected
            .as_ref()
            .is_none_or(|expected| expected == chain)
    });
    state[index] = 2;
    chains[index] = result.clone();
    Ok(result)
}

impl<'profile: 'o, 'o, 'v, 'request>
    ParsedGenericTreeSitterDocument<'_, 'profile, 'o, 'v, '_, 'request>
{
    pub fn assemble(
        &self,
    ) -> Result<GenericSemanticDocument<'_, 'profile, 'o, 'v, 'request>, GenericModelError> {
        let original = self.original_context()?;
        let observations = self.observations()?;
        let mut syntax_recoveries = Vec::new();
        for (node_index, node) in observations.nodes.iter().enumerate() {
            original.check()?;
            if node.error || node.missing {
                syntax_recoveries.push(GenericSyntaxRecovery {
                    node_index,
                    node_id: node.node_id,
                    kind: node.kind.clone(),
                    error: node.error,
                    missing: node.missing,
                    prepared: node.range.clone(),
                    original: self.node_origin(node_index)?,
                });
            }
        }
        let mut candidates = Vec::new();
        let mut diagnostics = Vec::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            original.check()?;
            let query = group.query.value();
            if *query.r#type().value() != QueryKind::Entity {
                continue;
            }
            for (match_index, matched) in group.matches.iter().enumerate() {
                let CaptureBinding::Present(names) = &matched.bindings.name else {
                    diagnostics.push(GenericEntityDiagnostic {
                        occurrence: Some(GenericOccurrence {
                            query_index,
                            match_index,
                            name_slot: 0,
                            value_slot: None,
                        }),
                        context_index: None,
                        issue: GenericEntityIssue::MissingName,
                    });
                    continue;
                };
                for &name_slot in names {
                    original.check()?;
                    let occurrence = GenericOccurrence {
                        query_index,
                        match_index,
                        name_slot,
                        value_slot: None,
                    };
                    let name = match self.capture_name(query_index, match_index, name_slot)? {
                        Ok(name) => name,
                        Err(issue) => {
                            diagnostics.push(GenericEntityDiagnostic {
                                occurrence: Some(occurrence),
                                context_index: None,
                                issue: GenericEntityIssue::Name(issue),
                            });
                            continue;
                        }
                    };
                    let (contexts, issues) =
                        match self.capture_contexts(query_index, match_index, name_slot)? {
                            Ok(contexts) => (contexts, Vec::new()),
                            Err(issue) => (Vec::new(), vec![issue]),
                        };
                    let candidate = Candidate {
                        origin: CandidateOrigin::Query(occurrence),
                        label: query.graph_label().value().clone(),
                        data_key: query.data_key().value().clone(),
                        name,
                        name_node: self.node_index(matched.captures[name_slot].native.node_id)?,
                        extent: self.capture_extent(query_index, match_index, name_slot)?,
                        declaration: self.declaration(query_index, match_index, name_slot)?,
                        contexts,
                        issues,
                        values: Vec::new(),
                    };
                    let owner = candidates.len();
                    candidates.push(candidate);
                    let mut units = Vec::new();
                    for &slot in names {
                        let unit = match self.capture_extent(query_index, match_index, slot)? {
                            GenericExtent::Exact { node_index, .. } => {
                                Some(observations.nodes[node_index].range.clone())
                            }
                            _ => None,
                        };
                        units.push((slot, unit));
                    }
                    let values = match &matched.bindings.value {
                        CaptureBinding::NotConfigured => continue,
                        CaptureBinding::Missing { .. } => {
                            diagnostics.push(GenericEntityDiagnostic {
                                occurrence: Some(occurrence),
                                context_index: None,
                                issue: GenericEntityIssue::MissingValue,
                            });
                            continue;
                        }
                        CaptureBinding::Present(values) => values,
                    };
                    for &value_slot in values {
                        original.check()?;
                        let capture = &matched.captures[value_slot];
                        if names.len() > 1 {
                            let associated: Vec<_> = names
                                .iter()
                                .copied()
                                .filter(|&slot| {
                                    let name = &matched.captures[slot].native;
                                    let unit = units
                                        .iter()
                                        .find(|(name_slot, _)| *name_slot == slot)
                                        .and_then(|(_, unit)| unit.clone());
                                    unit.is_some_and(|unit| {
                                        unit.start <= capture.native.range.start
                                            && unit.end >= capture.native.range.end
                                    }) || (name.range.start <= capture.native.range.start
                                        && name.range.end >= capture.native.range.end)
                                })
                                .collect();
                            if associated.len() != 1 {
                                diagnostics.push(GenericEntityDiagnostic {
                                    occurrence: Some(GenericOccurrence {
                                        value_slot: Some(value_slot),
                                        ..occurrence
                                    }),
                                    context_index: None,
                                    issue: GenericEntityIssue::AmbiguousValue,
                                });
                                continue;
                            }
                            if associated[0] != name_slot {
                                continue;
                            }
                        }
                        let value_occurrence = GenericOccurrence {
                            value_slot: Some(value_slot),
                            ..occurrence
                        };
                        let OriginalProjection::Exact(raw) = &capture.original else {
                            diagnostics.push(GenericEntityDiagnostic {
                                occurrence: Some(value_occurrence),
                                context_index: None,
                                issue: GenericEntityIssue::NonExactValue,
                            });
                            continue;
                        };
                        let node_index = self.node_index(capture.native.node_id)?;
                        if self.subtree_recovered(node_index)? {
                            diagnostics.push(GenericEntityDiagnostic {
                                occurrence: Some(value_occurrence),
                                context_index: None,
                                issue: GenericEntityIssue::RecoveredValue,
                            });
                            continue;
                        }
                        candidates[owner].values.push(*raw);
                        let name = match names::name_view(&self.source, *raw, true)? {
                            Ok(name) => name,
                            Err(issue) => {
                                diagnostics.push(GenericEntityDiagnostic {
                                    occurrence: Some(value_occurrence),
                                    context_index: None,
                                    issue: GenericEntityIssue::Name(issue),
                                });
                                continue;
                            }
                        };
                        if query.value_label().value() == &candidates[owner].label
                            && name.raw().start() == candidates[owner].name.raw().start()
                            && name.raw().end() == candidates[owner].name.raw().end()
                        {
                            diagnostics.push(GenericEntityDiagnostic {
                                occurrence: Some(value_occurrence),
                                context_index: None,
                                issue: GenericEntityIssue::SelfValue,
                            });
                            continue;
                        }
                        candidates.push(Candidate {
                            origin: CandidateOrigin::Value {
                                occurrence: value_occurrence,
                                owner,
                            },
                            label: query.value_label().value().clone(),
                            data_key: query.data_key().value().clone(),
                            name,
                            name_node: node_index,
                            extent: GenericExtent::Exact {
                                extent: EntityExtent::new(&self.source, *raw, original)?,
                                node_index,
                                capture_indices: vec![value_slot],
                            },
                            declaration: GenericDeclaration::NotConfigured,
                            contexts: Vec::new(),
                            issues: Vec::new(),
                            values: Vec::new(),
                        });
                    }
                }
            }
        }
        let contexts = self.context.contexts()?;
        let mut context_candidates = Vec::new();
        for (context_index, context) in contexts.iter().enumerate() {
            original.check()?;
            let unit = context.owner.as_ref().unwrap_or(&context.trigger);
            let mut scoped: Vec<usize> = Vec::new();
            for (candidate_index, candidate) in candidates.iter().enumerate() {
                if candidate
                    .contexts
                    .iter()
                    .any(|owned| owned.node_index == context.trigger.node_index)
                    && !scoped
                        .iter()
                        .any(|&prior| same_subject(&candidates[prior], candidate))
                {
                    scoped.push(candidate_index);
                }
            }
            if !scoped.is_empty() {
                if scoped.len() > 1 {
                    diagnostics.push(GenericEntityDiagnostic {
                        occurrence: None,
                        context_index: Some(context_index),
                        issue: GenericEntityIssue::AmbiguousContext,
                    });
                    for &candidate in &scoped {
                        if !candidates[candidate]
                            .issues
                            .contains(&GenericEntityIssue::AmbiguousContext)
                        {
                            candidates[candidate]
                                .issues
                                .push(GenericEntityIssue::AmbiguousContext);
                        }
                    }
                }
                context_candidates.push(if scoped.len() == 1 {
                    Some(scoped[0])
                } else {
                    None
                });
                continue;
            }

            let requires_entity = observations
                .profile
                .context_requires_entity()
                .value()
                .iter()
                .any(|kind| kind.value() == &observations.nodes[context.trigger.node_index].kind);
            let selected_name = match &context.name {
                ContextName::Found { selected, .. } => Some(selected),
                ContextName::Missing { .. } => None,
            };
            let mut possible = physical_candidates(&candidates, unit.node_index, None);
            if let Some(selected) = selected_name {
                for candidate in physical_candidates(&candidates, selected.node_index, None) {
                    if !possible
                        .iter()
                        .any(|&prior| same_subject(&candidates[prior], &candidates[candidate]))
                    {
                        possible.push(candidate);
                    }
                }
            }
            if requires_entity {
                possible.retain(|&index| {
                    !matches!(&candidates[index].origin, CandidateOrigin::Context(_))
                });
            }
            if unit.recovery.error
                || unit.recovery.missing
                || unit.recovery.contains_error
                || unit.recovery.contains_missing
            {
                context_candidates.push(None);
                continue;
            }
            if possible.len() > 1 {
                let matching_label: Vec<_> = possible
                    .iter()
                    .copied()
                    .filter(|&index| candidates[index].label == context.label)
                    .collect();
                if matching_label.len() == 1 {
                    possible = matching_label;
                }
            }
            if !possible.is_empty() {
                context_candidates.push(if possible.len() == 1 {
                    Some(possible[0])
                } else {
                    None
                });
                continue;
            }
            if requires_entity {
                context_candidates.push(None);
                continue;
            }
            let Some(selected) = selected_name else {
                context_candidates.push(None);
                continue;
            };
            if selected.recovery.error
                || selected.recovery.missing
                || selected.recovery.contains_error
                || selected.recovery.contains_missing
            {
                context_candidates.push(None);
                continue;
            }
            let OriginalProjection::Exact(raw) = &selected.original else {
                context_candidates.push(None);
                continue;
            };
            let selected_candidate = match possible.as_slice() {
                [candidate] => Some(*candidate),
                [] => {
                    let name = match names::name_view(&self.source, *raw, false)? {
                        Ok(name) => name,
                        Err(_) => {
                            context_candidates.push(None);
                            continue;
                        }
                    };
                    let (extent, issues) = match &unit.original {
                        OriginalProjection::Exact(extent) => (
                            GenericExtent::Exact {
                                extent: EntityExtent::new(&self.source, *extent, original)?,
                                node_index: unit.node_index,
                                capture_indices: Vec::new(),
                            },
                            Vec::new(),
                        ),
                        _ => {
                            let issue = GenericEntityIssue::NonExactContextExtent;
                            diagnostics.push(GenericEntityDiagnostic {
                                occurrence: None,
                                context_index: Some(context_index),
                                issue: issue.clone(),
                            });
                            (
                                GenericExtent::NonExact {
                                    capture_indices: Vec::new(),
                                },
                                vec![issue],
                            )
                        }
                    };
                    let index = candidates.len();
                    candidates.push(Candidate {
                        origin: CandidateOrigin::Context(context_index),
                        label: context.label.clone(),
                        data_key: String::new(),
                        name,
                        name_node: selected.node_index,
                        extent,
                        declaration: GenericDeclaration::NotConfigured,
                        contexts: Vec::new(),
                        issues,
                        values: Vec::new(),
                    });
                    Some(index)
                }
                _ => None,
            };
            context_candidates.push(selected_candidate);
        }
        let mut scope_owners: BTreeMap<usize, Option<usize>> = BTreeMap::new();
        for (context_index, context) in contexts.iter().enumerate() {
            let owner = context_candidates[context_index];
            scope_owners.entry(context.trigger.node_index).and_modify(|existing| {
                if !matches!((*existing,owner),(Some(left),Some(right)) if same_subject(&candidates[left],&candidates[right])) {
                    *existing=None;
                }
            }).or_insert(owner);
        }
        let mut explicit_scopes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            for owned in &candidate.contexts {
                let owners = explicit_scopes.entry(owned.node_index).or_default();
                if !owners
                    .iter()
                    .any(|&prior| same_subject(&candidates[prior], candidate))
                {
                    owners.push(candidate_index);
                }
            }
        }
        for (node_index, owners) in explicit_scopes {
            if owners.len() > 1 {
                for &owner in &owners {
                    if !candidates[owner]
                        .issues
                        .contains(&GenericEntityIssue::AmbiguousContext)
                    {
                        candidates[owner]
                            .issues
                            .push(GenericEntityIssue::AmbiguousContext);
                    }
                }
            }
            scope_owners.insert(
                node_index,
                if owners.len() == 1 {
                    Some(owners[0])
                } else {
                    None
                },
            );
        }
        let mut canonical = Vec::new();
        for (index, candidate) in candidates.iter().enumerate() {
            canonical.push(
                (0..index)
                    .find(|&prior| same_subject(&candidates[prior], candidate))
                    .unwrap_or(index),
            );
        }
        let mut plans = Vec::new();
        for (index, candidate) in candidates.iter().enumerate() {
            original.check()?;
            let mut plan = OwnerPlan {
                valid: true,
                parent: None,
                expected: None,
            };
            let mut context_chain = Vec::new();
            match candidate.origin {
                CandidateOrigin::Value { owner, .. } => {
                    plan.parent = Some(canonical[owner]);
                }
                CandidateOrigin::Query(occurrence) => {
                    let matched = &observations.groups[occurrence.query_index].matches
                        [occurrence.match_index];
                    match &matched.bindings.parent {
                        CaptureBinding::NotConfigured => {
                            let own_unit = match candidate.declaration {
                                GenericDeclaration::Native { node_index, .. } => Some(node_index),
                                GenericDeclaration::NotConfigured | GenericDeclaration::Missing => {
                                    match candidate.extent {
                                        GenericExtent::Exact { node_index, .. } => Some(node_index),
                                        _ => None,
                                    }
                                }
                                GenericDeclaration::Conflict { .. }
                                | GenericDeclaration::Invalid { .. } => None,
                            };
                            let mut ancestors = Vec::new();
                            let mut node = Some(candidate.name_node);
                            while let Some(index) = node {
                                ancestors.push(index);
                                node = observations.nodes[index].parent;
                            }
                            for node in ancestors.into_iter().rev() {
                                let Some(&owner) = scope_owners.get(&node) else {
                                    continue;
                                };
                                if let Some(unit) = own_unit
                                    && self.native_ancestor(unit, node)?
                                    && self.native_ancestor(node, candidate.name_node)?
                                {
                                    continue;
                                }
                                match owner {
                                    Some(owner) if canonical[owner] != canonical[index] => {
                                        context_chain.push(canonical[owner])
                                    }
                                    Some(_) => (),
                                    None => plan.valid = false,
                                }
                            }
                            context_chain.dedup();
                            plan.parent = context_chain.last().copied();
                            plan.expected = Some(context_chain);
                        }
                        CaptureBinding::Missing { .. } => {
                            plan.valid = false;
                            diagnostics.push(GenericEntityDiagnostic {
                                occurrence: Some(occurrence),
                                context_index: None,
                                issue: GenericEntityIssue::MissingParent,
                            });
                        }
                        CaptureBinding::Present(slots) => {
                            let label = observations.groups[occurrence.query_index]
                                .query
                                .value()
                                .parent_label()
                                .value();
                            let mut possible = HashSet::new();
                            for &slot in slots {
                                for parent in physical_candidates(
                                    &candidates,
                                    self.node_index(matched.captures[slot].native.node_id)?,
                                    Some(label),
                                ) {
                                    possible.insert(canonical[parent]);
                                }
                            }
                            match possible.len() {
                                1 => plan.parent = possible.into_iter().next(),
                                count => {
                                    plan.valid = false;
                                    diagnostics.push(GenericEntityDiagnostic {
                                        occurrence: Some(occurrence),
                                        context_index: None,
                                        issue: if count == 0 {
                                            GenericEntityIssue::MissingParent
                                        } else {
                                            GenericEntityIssue::AmbiguousParent
                                        },
                                    });
                                }
                            }
                        }
                    }
                }
                CandidateOrigin::Context(_) => {
                    let mut ancestors = Vec::new();
                    let mut current = observations.nodes[candidate.name_node].parent;
                    while let Some(node) = current {
                        ancestors.push(node);
                        current = observations.nodes[node].parent;
                    }
                    for node in ancestors.into_iter().rev() {
                        let Some(&owner) = scope_owners.get(&node) else {
                            continue;
                        };
                        match owner {
                            Some(owner) if canonical[owner] != canonical[index] => {
                                context_chain.push(canonical[owner])
                            }
                            Some(_) => (),
                            None => plan.valid = false,
                        }
                    }
                    context_chain.dedup();
                    plan.parent = context_chain.last().copied();
                    plan.expected = Some(context_chain);
                }
            }
            if !plan.valid {
                diagnostics.push(GenericEntityDiagnostic {
                    occurrence: candidate.occurrence(),
                    context_index: candidate.context_index(),
                    issue: GenericEntityIssue::IncompleteContext,
                });
            }
            plans.push(plan);
        }
        let mut state = vec![0; candidates.len()];
        let mut chains = vec![None; candidates.len()];
        for index in 0..candidates.len() {
            chain(index, &plans, &mut state, &mut chains, original)?;
        }
        let mut identities: Vec<Option<SyntaxIdentity<'_, 'o, 'request>>> =
            (0..candidates.len()).map(|_| None).collect();
        let mut order: Vec<usize> = (0..candidates.len()).collect();
        order.sort_by_key(|&index| chains[index].as_ref().map_or(usize::MAX, Vec::len));
        for &index in &order {
            original.check()?;
            let Some(chain) = &chains[index] else {
                diagnostics.push(GenericEntityDiagnostic {
                    occurrence: candidates[index].occurrence(),
                    context_index: candidates[index].context_index(),
                    issue: GenericEntityIssue::InconsistentOwnerChain,
                });
                continue;
            };
            let owners: Option<Vec<_>> = chain
                .iter()
                .map(|&owner| identities[owner].as_ref())
                .collect();
            let Some(owners) = owners else {
                return Err(GenericModelError::NativeContract(
                    "owner identity order is inconsistent",
                ));
            };
            let identity = self.identity.symbol(
                &candidates[index].label,
                candidates[index].name.raw(),
                &owners,
                crate::symbols::LogicalDeclaration::named(candidates[index].name.display()),
            )?;
            identities[index] = Some(identity);
        }
        let mut entities: Vec<GenericEntity<'_, 'o, 'request>> = Vec::new();
        let mut namespaces = Vec::new();
        let mut namespace_details = Vec::new();
        let mut by_key = BTreeMap::new();
        let mut candidate_entity = vec![None; candidates.len()];
        let mut candidate_namespace = vec![None; candidates.len()];
        for (index, candidate) in candidates.into_iter().enumerate() {
            original.check()?;
            let Some(identity) = identities[index].take() else {
                continue;
            };
            let Some(occurrence) = candidate.occurrence() else {
                candidate_namespace[index] = Some(namespaces.len());
                namespaces.push(identity);
                namespace_details.push(NamespaceDetails {
                    extent: candidate.extent,
                    conflicts: candidate.issues,
                });
                continue;
            };
            let key = (
                identity.uid().as_str().to_owned(),
                candidate.data_key.clone(),
            );
            let declaration_issue = match candidate.declaration {
                GenericDeclaration::Conflict { .. } => {
                    Some(GenericEntityIssue::ConflictingDeclaration)
                }
                GenericDeclaration::Invalid { .. } => Some(GenericEntityIssue::InvalidDeclaration),
                _ => None,
            };
            if let Some(issue) = &declaration_issue {
                diagnostics.push(GenericEntityDiagnostic {
                    occurrence: Some(occurrence),
                    context_index: None,
                    issue: issue.clone(),
                });
            }
            for issue in &candidate.issues {
                diagnostics.push(GenericEntityDiagnostic {
                    occurrence: Some(occurrence),
                    context_index: None,
                    issue: issue.clone(),
                });
            }
            let context_issues = candidate.issues;
            let contribution = GenericContribution {
                occurrence,
                name_node_index: candidate.name_node,
                extent: candidate.extent,
                declaration: candidate.declaration,
                contexts: candidate.contexts,
                values: candidate.values,
            };
            let entity_index = if let Some(&entity_index) = by_key.get(&key) {
                let entity: &mut GenericEntity<'_, 'o, 'request> = &mut entities[entity_index];
                if entity.name.display() != candidate.name.display()
                    || entity.name.representation() != candidate.name.representation()
                {
                    entity.conflicts.push(GenericEntityIssue::ConflictingName);
                }
                let extent_signature = |e: &GenericExtent<'_>| match e {
                    GenericExtent::Exact { extent, .. } => {
                        Some((extent.range().start(), extent.range().end()))
                    }
                    _ => None,
                };
                if entity
                    .contributions
                    .iter()
                    .any(|c| extent_signature(&c.extent) != extent_signature(&contribution.extent))
                {
                    entity.conflicts.push(GenericEntityIssue::ConflictingExtent);
                }
                let declaration_node = |declaration: &GenericDeclaration| match declaration {
                    GenericDeclaration::Native { node_index, .. } => Some(*node_index),
                    _ => None,
                };
                if entity.contributions.iter().any(|c| {
                    matches!(
                        (declaration_node(&c.declaration), declaration_node(&contribution.declaration)),
                        (Some(existing), Some(incoming)) if existing != incoming
                    )
                }) {
                    entity
                        .conflicts
                        .push(GenericEntityIssue::ConflictingDeclaration);
                }
                entity.contributions.push(contribution);
                entity_index
            } else {
                let entity_index = entities.len();
                by_key.insert(key, entity_index);
                entities.push(GenericEntity {
                    identity,
                    name: candidate.name,
                    data_key: candidate.data_key,
                    contributions: vec![contribution],
                    owners: Vec::new(),
                    owner_chain: Vec::new(),
                    conflicts: Vec::new(),
                    value: None,
                });
                entity_index
            };
            if let Some(issue) = declaration_issue {
                let entity = &mut entities[entity_index];
                if !entity.conflicts.contains(&issue) {
                    entity.conflicts.push(issue);
                }
            }
            for issue in context_issues {
                let entity = &mut entities[entity_index];
                if !entity.conflicts.contains(&issue) {
                    entity.conflicts.push(issue);
                }
            }
            candidate_entity[index] = Some(entity_index);
        }
        let mut endpoints: Vec<Vec<usize>> = vec![Vec::new(); canonical.len()];
        for (candidate, entity) in candidate_entity.iter().enumerate() {
            if let Some(entity) = entity {
                let group = &mut endpoints[canonical[candidate]];
                if !group.contains(entity) {
                    group.push(*entity);
                }
            }
        }
        let scopes = scope_owners
            .into_iter()
            .map(|(node_index, owner)| {
                let owner = match owner {
                    Some(candidate) if !endpoints[canonical[candidate]].is_empty() => {
                        GenericScopeOwner::Entities(endpoints[canonical[candidate]].clone())
                    }
                    Some(candidate) if candidate_namespace[canonical[candidate]].is_some() => {
                        GenericScopeOwner::Namespace
                    }
                    _ => GenericScopeOwner::Unresolved,
                };
                GenericSemanticScope { node_index, owner }
            })
            .collect();
        for (candidate, entity) in candidate_entity.iter().enumerate() {
            if let (Some(entity), Some(chain)) = (entity, &chains[candidate]) {
                let mut owners = Vec::new();
                let mut owner_chain = Vec::new();
                let mut ambiguous = false;
                for &owner in chain {
                    match endpoints[canonical[owner]].as_slice() {
                        [] => {
                            let namespace = candidate_namespace[canonical[owner]].ok_or(
                                GenericModelError::NativeContract(
                                    "identity owner has no retained native endpoint",
                                ),
                            )?;
                            owner_chain.push(GenericOwnerRef::Namespace(namespace));
                        }
                        [endpoint] => {
                            owners.push(*endpoint);
                            owner_chain.push(GenericOwnerRef::Entity(*endpoint));
                        }
                        _ => ambiguous = true,
                    }
                }
                let target = &mut entities[*entity];
                if ambiguous {
                    target.conflicts.push(GenericEntityIssue::AmbiguousParent);
                    target.owners.clear();
                    target.owner_chain.clear();
                } else if (!target.owners.is_empty() && target.owners != owners)
                    || (!target.owner_chain.is_empty() && target.owner_chain != owner_chain)
                {
                    target
                        .conflicts
                        .push(GenericEntityIssue::InconsistentOwnerChain);
                    target.owners.clear();
                    target.owner_chain.clear();
                } else {
                    target.owners = owners;
                    target.owner_chain = owner_chain;
                }
            }
        }
        for entity in &mut entities {
            let values: Vec<_> = entity
                .contributions
                .iter()
                .flat_map(|c| c.values.iter().copied())
                .collect();
            if let Some(first) = values.first() {
                if values.iter().all(|value| value.text() == first.text()) {
                    entity.value = Some(*first);
                } else {
                    entity.conflicts.push(GenericEntityIssue::ConflictingValue);
                }
            }
        }
        original.check()?;
        let mut document = GenericSemanticDocument {
            identity: &self.identity,
            observations,
            original,
            source: self.source.clone(),
            entities,
            namespaces,
            namespace_details,
            diagnostics,
            syntax_recoveries,
            exports: Vec::new(),
            complexities: Vec::new(),
            relations: Vec::new(),
            scopes,
        };
        for (index, entity) in document.entities.iter().enumerate() {
            document.exports.push(self.entity_export(index, &document)?);
            let declaration = entity
                .contributions
                .iter()
                .filter_map(|c| match c.declaration {
                    GenericDeclaration::Native { node_index, .. } => Some(node_index),
                    _ => None,
                })
                .collect::<HashSet<_>>();
            let context_issues = entity
                .conflicts
                .iter()
                .filter(|issue| {
                    matches!(
                        issue,
                        GenericEntityIssue::MissingContext
                            | GenericEntityIssue::InvalidContext
                            | GenericEntityIssue::AmbiguousContext
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut owned_contexts = entity
                .contributions
                .iter()
                .flat_map(|contribution| {
                    contribution
                        .contexts
                        .iter()
                        .map(|context| context.node_index)
                })
                .collect::<Vec<_>>();
            owned_contexts.sort_unstable();
            owned_contexts.dedup();
            let complexity = if entity.conflicts.iter().any(|issue| {
                matches!(
                    issue,
                    GenericEntityIssue::ConflictingDeclaration
                        | GenericEntityIssue::InvalidDeclaration
                )
            }) {
                GenericComplexity::DeclarationUnavailable {
                    evidence: entity
                        .contributions
                        .iter()
                        .map(|c| c.declaration.clone())
                        .collect(),
                }
            } else if !context_issues.is_empty() {
                GenericComplexity::ContextUnavailable {
                    issues: context_issues,
                }
            } else {
                self.complexity_with_contexts(
                    if declaration.len() == 1 {
                        declaration.into_iter().next()
                    } else {
                        None
                    },
                    &owned_contexts,
                )?
            };
            document.complexities.push(complexity);
        }
        document.relations = self.entity_relations(&document)?;
        Ok(document)
    }

    pub(super) fn native_ancestor(
        &self,
        ancestor: usize,
        mut node: usize,
    ) -> Result<bool, GenericModelError> {
        let observations = self.observations()?;
        loop {
            self.original_context()?;
            if node == ancestor {
                return Ok(true);
            }
            match observations.nodes[node].parent {
                Some(parent) => node = parent,
                None => return Ok(false),
            }
        }
    }

    pub(super) fn subtree_recovered(&self, index: usize) -> Result<bool, GenericModelError> {
        let mut stack = vec![index];
        while let Some(index) = stack.pop() {
            self.original_context()?;
            let node = &self.observations()?.nodes[index];
            if node.error || node.missing {
                return Ok(true);
            }
            stack.extend(self.children[index].iter().copied());
        }
        Ok(false)
    }
}
