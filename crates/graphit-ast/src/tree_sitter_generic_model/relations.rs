use super::entities::GenericScopeOwner;
use super::*;
use crate::{profiles::QueryKind, symbols::ContextFrame};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericRelationTarget {
    Resolved { entity_index: usize },
    NotFound,
    Ambiguous { entity_indices: Vec<usize> },
    Unresolved { reason: &'static str },
}
#[derive(Clone, Debug)]
pub struct GenericRelation {
    pub occurrence: GenericOccurrence,
    pub kind: String,
    pub source_index: Option<usize>,
    pub evidence_node_indices: Vec<usize>,
    pub target: GenericRelationTarget,
}

fn frames_equal(left: &[ContextFrame], right: &[ContextFrame]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.label() == b.label() && a.start() == b.start() && a.end() == b.end())
}

impl GenericSemanticDocument<'_, '_, '_, '_, '_> {
    pub(super) fn lexical_targets(
        &self,
        name: &str,
        labels: &[&str],
        namespace: &[ContextFrame],
    ) -> Vec<usize> {
        for depth in (0..=namespace.len()).rev() {
            let mut found: Vec<usize> = Vec::new();
            for (index, entity) in self.entities.iter().enumerate() {
                if entity.name.display() != name
                    || entity.identity.label() == "Export"
                    || (!labels.is_empty() && !labels.contains(&entity.identity.label()))
                    || !frames_equal(entity.identity.context(), &namespace[..depth])
                {
                    continue;
                }
                if !found.iter().any(|&prior| {
                    self.entities[prior].identity.uid().as_str() == entity.identity.uid().as_str()
                        && self.entities[prior].data_key == entity.data_key
                }) {
                    found.push(index);
                }
            }
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }
}

impl ParsedGenericTreeSitterDocument<'_, '_, '_, '_, '_, '_> {
    pub(super) fn entity_relations(
        &self,
        document: &GenericSemanticDocument<'_, '_, '_, '_, '_>,
    ) -> Result<Vec<GenericRelation>, GenericModelError> {
        let observations = self.observations()?;
        let mut relations: Vec<GenericRelation> = Vec::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            let query = group.query.value();
            if *query.r#type().value() != QueryKind::Relation {
                continue;
            }
            let kind = query.relation_type().value();
            let rule = observations
                .profile
                .target_rules()
                .value()
                .iter()
                .find(|(key, _)| key.value() == kind)
                .map(|(_, rule)| rule.value());
            let mut labels = Vec::new();
            for related in &observations.groups {
                let related = related.query.value();
                if *related.r#type().value() == QueryKind::Relation
                    && related.relation_type().value() == kind
                {
                    for label in related.target_labels().value().iter() {
                        if !labels.contains(&label.value().as_str()) {
                            labels.push(label.value().as_str());
                        }
                    }
                }
            }
            if let Some(rule) = rule {
                for label in rule.labels().value().iter() {
                    if !labels.contains(&label.value().as_str()) {
                        labels.push(label.value().as_str());
                    }
                }
            }
            if labels.is_empty() {
                for entity_query in &observations.groups {
                    let entity_query = entity_query.query.value();
                    if *entity_query.r#type().value() != QueryKind::Entity {
                        continue;
                    }
                    for label in [
                        entity_query.graph_label().value(),
                        entity_query.value_label().value(),
                    ] {
                        if !label.is_empty() && !labels.contains(&label.as_str()) {
                            labels.push(label.as_str());
                        }
                    }
                }
            }
            let fallback = if query.target_fallback().value().is_empty() {
                rule.map(|r| r.fallback().value().as_str()).unwrap_or("")
            } else {
                query.target_fallback().value().as_str()
            };
            for (match_index, matched) in group.matches.iter().enumerate() {
                let CaptureBinding::Present(names) = &matched.bindings.name else {
                    continue;
                };
                for &name_slot in names {
                    self.original_context()?;
                    let occurrence = GenericOccurrence {
                        query_index,
                        match_index,
                        name_slot,
                        value_slot: None,
                    };
                    let node_index = self.node_index(matched.captures[name_slot].native.node_id)?;
                    let mut evidence = vec![node_index];
                    let mut source_candidates: Vec<usize> = Vec::new();
                    let mut source_scope_issue = None;
                    let mut ancestor = observations.nodes[node_index].parent;
                    while let Some(node) = ancestor {
                        if let Some(scope) = document
                            .scopes
                            .iter()
                            .find(|scope| scope.node_index == node)
                        {
                            evidence.push(node);
                            match &scope.owner {
                                GenericScopeOwner::Entities(owners) => {
                                    source_candidates.extend(owners.iter().copied())
                                }
                                GenericScopeOwner::Namespace => {
                                    source_scope_issue =
                                        Some("native namespace has no Entity source")
                                }
                                GenericScopeOwner::Unresolved => {
                                    source_scope_issue = Some("unresolved native source scope")
                                }
                            }
                            break;
                        }
                        if observations
                            .profile
                            .anon_func_types()
                            .value()
                            .iter()
                            .any(|kind| kind.value() == &observations.nodes[node].kind)
                        {
                            evidence.push(node);
                            source_scope_issue = Some("unowned anonymous source scope");
                            break;
                        }

                        for (index, entity) in document.entities.iter().enumerate() {
                            if entity.contributions.iter().any(|c| match c.declaration {
                                GenericDeclaration::Native { node_index, .. } => node_index == node,
                                _ => matches!(c.extent, GenericExtent::Exact { node_index, .. } if node_index == node),
                            }) && !source_candidates.iter().any(|&prior| document.entities[prior].identity.uid().as_str() == entity.identity.uid().as_str() && document.entities[prior].data_key == entity.data_key) { source_candidates.push(index); }
                        }
                        if !source_candidates.is_empty() {
                            break;
                        }
                        ancestor = observations.nodes[node].parent;
                    }
                    let source_index = match source_candidates.as_slice() {
                        [source] => Some(*source),
                        _ => None,
                    };
                    let mut relation = GenericRelation {
                        occurrence,
                        kind: kind.clone(),
                        source_index,
                        evidence_node_indices: Vec::new(),
                        target: GenericRelationTarget::Unresolved {
                            reason: "no unique native source declaration",
                        },
                    };
                    let name = self.capture_name(query_index, match_index, name_slot)?;
                    let is_declaration_name = document.entities.iter().any(|entity| {
                        entity
                            .contributions
                            .iter()
                            .any(|contribution| contribution.name_node_index == node_index)
                    });
                    let target = if let Some(reason) = source_scope_issue {
                        GenericRelationTarget::Unresolved { reason }
                    } else if is_declaration_name {
                        GenericRelationTarget::Unresolved {
                            reason: "capture is a declaration subject, not a reference",
                        }
                    } else if source_candidates.len() > 1 {
                        GenericRelationTarget::Unresolved {
                            reason: "ambiguous native source declaration",
                        }
                    } else if let Ok(name) = name {
                        let source = source_index.map(|index| &document.entities[index]);
                        let namespace = source
                            .map(|entity| entity.identity.context())
                            .unwrap_or(&[]);
                        let qualifiers = match &matched.bindings.qualifier {
                            CaptureBinding::NotConfigured
                            | CaptureBinding::Missing { declared: true } => Vec::new(),
                            CaptureBinding::Missing { declared: false } => {
                                return Err(GenericModelError::NativeContract(
                                    "undeclared configured qualifier escaped preparation",
                                ));
                            }
                            CaptureBinding::Present(slots) => slots
                                .iter()
                                .map(|&slot| (slot, &matched.captures[slot]))
                                .collect(),
                        };
                        let mut qualifier_texts = Vec::new();
                        let mut qualifier_valid = true;
                        for (slot, capture) in qualifiers {
                            if let OriginalProjection::Exact(raw) = &capture.original {
                                if !qualifier_texts.contains(&raw.text()) {
                                    qualifier_texts.push(raw.text());
                                }
                                evidence
                                    .push(self.node_index(matched.captures[slot].native.node_id)?);
                            } else {
                                qualifier_valid = false;
                            }
                        }
                        if !qualifier_valid || qualifier_texts.len() > 1 {
                            GenericRelationTarget::Unresolved {
                                reason: "nonexact or ambiguous native qualifier",
                            }
                        } else {
                            let qualifier = qualifier_texts.first().copied();
                            let self_qualified = qualifier.is_some_and(|qualifier| {
                                observations
                                    .profile
                                    .self_keywords()
                                    .value()
                                    .iter()
                                    .any(|keyword| {
                                        keyword.value().strip_suffix('.').unwrap_or(keyword.value())
                                            == qualifier
                                    })
                            });
                            let fallback_labels = [fallback];
                            let mut label_options = vec![labels.as_slice()];
                            if !fallback.is_empty() {
                                label_options.push(fallback_labels.as_slice());
                            }
                            let mut unique: Vec<usize> = Vec::new();
                            for labels in label_options {
                                let candidates = if qualifier.is_none() || self_qualified {
                                    let mut candidates: Vec<usize> = Vec::new();
                                    // An entity's own native scope is visible before its enclosing namespace.
                                    if let Some(source) = source {
                                        for (index, entity) in document.entities.iter().enumerate()
                                        {
                                            if entity.name.display() == name.display()
                                                && (labels.is_empty()
                                                    || labels.contains(&entity.identity.label()))
                                                && entity.identity.context().last().is_some_and(
                                                    |frame| {
                                                        frame.start() == source.name.raw().start()
                                                            && frame.end()
                                                                == source.name.raw().end()
                                                            && frame.label()
                                                                == source.identity.label()
                                                    },
                                                )
                                                && entity.identity.context().len()
                                                    == source.identity.context().len() + 1
                                                && frames_equal(
                                                    &entity.identity.context()
                                                        [..source.identity.context().len()],
                                                    namespace,
                                                )
                                            {
                                                candidates.push(index);
                                            }
                                        }
                                    }
                                    if candidates.is_empty() {
                                        document.lexical_targets(name.display(), labels, namespace)
                                    } else {
                                        candidates
                                    }
                                } else {
                                    let owners = document.lexical_targets(
                                        qualifier.unwrap_or(""),
                                        &[],
                                        namespace,
                                    );
                                    if owners.len() == 1 {
                                        let owner = &document.entities[owners[0]];
                                        document
                                            .entities
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, entity)| {
                                                entity.name.display() == name.display()
                                                    && (labels.is_empty()
                                                        || labels
                                                            .contains(&entity.identity.label()))
                                                    && entity.identity.context().len()
                                                        == owner.identity.context().len() + 1
                                                    && frames_equal(
                                                        &entity.identity.context()
                                                            [..owner.identity.context().len()],
                                                        owner.identity.context(),
                                                    )
                                                    && entity.identity.context().last().is_some_and(
                                                        |frame| {
                                                            frame.start()
                                                                == owner.name.raw().start()
                                                                && frame.end()
                                                                    == owner.name.raw().end()
                                                                && frame.label()
                                                                    == owner.identity.label()
                                                        },
                                                    )
                                            })
                                            .map(|(index, _)| index)
                                            .collect()
                                    } else {
                                        Vec::new()
                                    }
                                };
                                for index in candidates {
                                    if !unique.iter().any(|&prior| {
                                        document.entities[prior].identity.uid().as_str()
                                            == document.entities[index].identity.uid().as_str()
                                            && document.entities[prior].data_key
                                                == document.entities[index].data_key
                                    }) {
                                        unique.push(index);
                                    }
                                }
                                if !unique.is_empty() {
                                    break;
                                }
                            }
                            match unique.as_slice() {
                                [] => GenericRelationTarget::NotFound,
                                [target] => GenericRelationTarget::Resolved {
                                    entity_index: *target,
                                },
                                _ => GenericRelationTarget::Ambiguous {
                                    entity_indices: unique,
                                },
                            }
                        }
                    } else {
                        GenericRelationTarget::Unresolved {
                            reason: "missing source or ineligible native name",
                        }
                    };
                    relation.target = target;
                    relation.evidence_node_indices = evidence;
                    relations.push(relation);
                }
            }
        }
        Ok(relations)
    }
}
