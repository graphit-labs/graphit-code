//! Re-resolve only FILEs reached by changed SCIP symbols, using disk locator indexes.
use super::*;
use crate::semantic_graph::scip_document::{
    AstScipDocumentDefinition as Definition, AstScipDocumentReference as Reference,
    apply_scip_document_resolutions, scip_edge_uid,
};

pub(super) fn symbol_key(path: &str, symbol: &str) -> io::Result<String> {
    if symbol.is_empty() {
        return Err(invalid("SCIP symbol is empty"));
    }
    encoded(&(
        if ::scip::symbol::is_local_symbol(symbol) {
            path
        } else {
            ""
        },
        symbol,
    ))
}
fn associated(
    input: &LiveFileInput<'_>,
    path: &str,
    association: &crate::semantic_graph::AstGraphAssociation,
) -> io::Result<()> {
    if path != input.path
        || association.path() != input.path
        || association.raw_version() != input.raw_version
        || association.source_sha256() != &input.source_sha256
    {
        return Err(invalid("SCIP fact belongs to another Source/FILE revision"));
    }
    Ok(())
}
type Slot = (usize, Option<usize>, Option<usize>);
fn slot(reference: &Reference) -> io::Result<Slot> {
    Ok((
        reference
            .relation_index
            .ok_or_else(|| invalid("SCIP relation index missing"))?,
        reference.source_index,
        reference.capture_index,
    ))
}
struct Observations<'a> {
    nodes: BTreeMap<&'a str, &'a GraphNode>,
    markers: Vec<(&'a GraphNode, BTreeMap<Slot, Value>)>,
}
impl<'a> Observations<'a> {
    fn new(nodes: &'a [GraphNode], references: &[Reference]) -> io::Result<Self> {
        let mut markers = Vec::new();
        if references
            .iter()
            .any(|reference| reference.kind.is_some() && reference.source_key.is_some())
        {
            for node in nodes.iter().filter(|node| {
                matches!(
                    node.properties.get("is_file_observation"),
                    Some(GraphValue::Bool(true))
                )
            }) {
                let outcomes: Vec<Value> = decoded(
                    node_text(node, "resolution_outcomes_json")
                        .ok_or_else(|| invalid("profile FILE lacks outcomes"))?,
                )?;
                let mut slots = BTreeMap::new();
                for mut outcome in outcomes {
                    let index = |name: &str| -> io::Result<Option<usize>> {
                        outcome
                            .get(name)
                            .and_then(Value::as_u64)
                            .map(usize::try_from)
                            .transpose()
                            .map_err(io::Error::other)
                    };
                    let key = (
                        index("relation_index")?
                            .ok_or_else(|| invalid("profile FILE outcome lacks relation index"))?,
                        index("source_index")?,
                        index("capture_index")?,
                    );
                    let mut observed = outcome
                        .as_object_mut()
                        .and_then(|fields| fields.remove("observation"))
                        .ok_or_else(|| invalid("profile FILE outcome lacks observation"))?;
                    if let Some(fields) = observed.as_object_mut() {
                        fields.remove("scip_graph_target");
                    }
                    if slots.insert(key, observed).is_some() {
                        return Err(invalid("profile FILE outcome is not unique"));
                    }
                }
                markers.push((node, slots));
            }
        }
        Ok(Self {
            nodes: nodes.iter().map(|node| (node.key.as_str(), node)).collect(),
            markers,
        })
    }
    fn marker(&self, reference: &Reference) -> io::Result<String> {
        let source = self
            .nodes
            .get(
                reference
                    .source_key
                    .as_deref()
                    .ok_or_else(|| invalid("SCIP source disappeared"))?,
            )
            .ok_or_else(|| invalid("SCIP source disappeared"))?;
        let observation: Value = decoded(
            reference
                .observation_json
                .as_deref()
                .ok_or_else(|| invalid("SCIP reference lacks observation"))?,
        )?;
        let slot = slot(reference)?;
        let mut found = None;
        for (node, outcomes) in &self.markers {
            if ["lang", "grammar"].iter().all(|name| {
                node_text(source, name).is_none_or(|value| node_text(node, name) == Some(value))
            }) && outcomes.get(&slot) == Some(&observation)
                && found.replace(node.key.clone()).is_some()
            {
                return Err(invalid(
                    "SCIP reference matches multiple profile FILE observations",
                ));
            }
        }
        found.ok_or_else(|| invalid("SCIP reference has no matching profile FILE observation"))
    }
}
pub(super) fn validate(input: &LiveFileInput<'_>, original: &SnapshotContext) -> io::Result<()> {
    let nodes = input
        .nodes
        .iter()
        .map(|node| (node.key.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let observations = Observations::new(input.nodes, input.references)?;
    for definition in input.definitions {
        original.check()?;
        associated(input, &definition.path, &definition.association)?;
        symbol_key(input.path, &definition.symbol)?;
        if definition.start_byte >= definition.end_byte
            || nodes
                .get(definition.entity_key.as_str())
                .is_none_or(|node| node.label != definition.label)
        {
            return Err(invalid("SCIP definition has no genuine FILE entity"));
        }
    }
    for reference in input.references {
        original.check()?;
        associated(input, &reference.path, &reference.association)?;
        let (Some(kind), Some(source)) = (&reference.kind, &reference.source_key) else {
            continue;
        };
        ident(kind)?;
        symbol_key(input.path, &reference.symbol)?;
        if reference.start_byte >= reference.end_byte
            || !nodes.contains_key(source.as_str())
            || reference.relation_index.is_none()
            || reference.observation_json.is_none()
            || reference.occurrence_ordinal.is_none()
            || reference.reference_target.is_empty()
        {
            return Err(invalid("SCIP reference has no genuine FILE source/outcome"));
        }
        observations.marker(reference)?;
    }
    Ok(())
}
pub(super) fn resolve(
    index: &Index,
    nodes: &mut [GraphNode],
    facts: &mut FileFacts,
    changed: Option<&BTreeSet<String>>,
    original: &Arc<SnapshotContext>,
) -> io::Result<()> {
    let observations = Observations::new(nodes, &facts.references)?;
    let mut groups = BTreeMap::<(String, Slot), Vec<Reference>>::new();
    let mut seen = BTreeSet::new();
    for reference in &facts.references {
        if !seen.insert(encoded(reference)?) {
            continue;
        }
        if reference.kind.is_some() && reference.source_key.is_some() {
            groups
                .entry((observations.marker(reference)?, slot(reference)?))
                .or_default()
                .push(reference.clone());
        }
    }
    drop(observations);
    let mut by_marker = BTreeMap::<String, Vec<(Reference, Vec<Definition>)>>::new();
    for ((marker, slot), references) in groups {
        original.check()?;
        if let Some(changed) = changed {
            let mut affected = false;
            for reference in &references {
                affected |= changed.contains(&symbol_key(&reference.path, &reference.symbol)?);
            }
            if !affected {
                continue;
            }
        }
        let mut group_edges = Vec::new();
        let mut selected = Vec::new();
        let mut unique = BTreeSet::new();
        for reference in references {
            let mut candidates = BTreeMap::new();
            for candidate in index
                .candidates::<Definition>(
                    "scip_def",
                    "symbol",
                    &symbol_key(&reference.path, &reference.symbol)?,
                )?
                .into_iter()
                .filter(|candidate| reference.target_labels.contains(&candidate.label))
            {
                if let Some(previous) =
                    candidates.insert(candidate.entity_key.clone(), candidate.clone())
                    && previous != candidate
                {
                    return Err(invalid("SCIP target identity conflicts"));
                }
            }
            unique.extend(candidates.keys().cloned());
            selected.push((reference, candidates.into_values().collect::<Vec<_>>()));
        }
        for (reference, targets) in selected {
            if unique.len() == 1
                && let [target] = targets.as_slice()
            {
                let kind = reference
                    .kind
                    .as_deref()
                    .ok_or_else(|| invalid("SCIP relation kind disappeared"))?;
                let source = reference
                    .source_key
                    .as_deref()
                    .ok_or_else(|| invalid("SCIP source disappeared"))?;
                let parser_proof = reference.graph_edge_uids.is_empty()
                    && reference.parser_target.as_deref() == Some(target.entity_key.as_str())
                    && facts.parser_edges.iter().any(|edge| {
                        edge.kind == kind
                            && edge.source_key == source
                            && edge.target_key == target.entity_key
                    });
                if !parser_proof {
                    let edge = GraphEdge {
                        kind: kind.into(),
                        source_key: source.into(),
                        target_key: target.entity_key.clone(),
                        uid: scip_edge_uid(&reference, target).map_err(io::Error::other)?,
                    };
                    if !group_edges.contains(&edge) {
                        group_edges.push(edge);
                    }
                }
            }
            by_marker
                .entry(marker.clone())
                .or_default()
                .push((reference, targets));
        }
        facts
            .scip_edges
            .insert(encoded(&(&marker, slot))?, group_edges);
    }
    for (key, resolutions) in by_marker {
        let node = nodes
            .iter_mut()
            .find(|node| node.key == key)
            .ok_or_else(|| invalid("SCIP source observation disappeared"))?;
        let mut file = ast_node(node)?;
        apply_scip_document_resolutions(&mut file, &resolutions, original)
            .map_err(io::Error::other)?;
        update_outcomes(node, &file)?;
    }
    original.check()
}
