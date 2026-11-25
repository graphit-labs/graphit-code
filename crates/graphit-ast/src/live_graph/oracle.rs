//! Source-bound Oracle resolution over affected FILE facts and indexed candidates.
use super::*;
use crate::semantic_graph::oracle::{
    AstOracleGraphEndpoint as Endpoint, apply_oracle_graph_resolution,
    resolve_oracle_graph_reference,
};
fn associated(
    input: &LiveFileInput<'_>,
    association: &crate::semantic_graph::AstGraphAssociation,
) -> io::Result<()> {
    if association.path() != input.path
        || association.raw_version() != input.raw_version
        || association.source_sha256() != &input.source_sha256
    {
        return Err(invalid(
            "Oracle fact belongs to another Source/FILE revision",
        ));
    }
    Ok(())
}
pub(super) fn validate(input: &LiveFileInput<'_>, original: &SnapshotContext) -> io::Result<()> {
    let nodes = input
        .nodes
        .iter()
        .map(|node| (node.key.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut endpoints = BTreeMap::new();
    let mut references = BTreeMap::new();
    for rows in input.oracle {
        for endpoint in rows.endpoints() {
            original.check()?;
            associated(input, endpoint.association())?;
            if nodes
                .get(endpoint.key())
                .is_none_or(|node| node.label != endpoint.label())
            {
                return Err(invalid("Oracle endpoint has no genuine FILE entity"));
            }
            if let Some(previous) = endpoints.insert(endpoint.key(), endpoint)
                && previous != endpoint
            {
                return Err(invalid("Oracle endpoint identity conflicts"));
            }
        }
        for reference in rows.references() {
            original.check()?;
            associated(input, reference.association())?;
            ident(reference.kind())?;
            if reference
                .source_key()
                .is_some_and(|key| !nodes.contains_key(key))
            {
                return Err(invalid("Oracle reference has no genuine FILE source"));
            }
            let slot = (
                reference.outcome().relation_index(),
                reference.outcome().source_index(),
                reference.outcome().capture_index(),
            );
            if let Some(previous) = references.insert(slot, reference)
                && previous != reference
            {
                return Err(invalid("Oracle reference outcome identity conflicts"));
            }
        }
    }
    if !references.is_empty() {
        let file = input
            .oracle_file
            .ok_or_else(|| invalid("Oracle references lack original FILE observation"))?;
        if !nodes.contains_key(file.key()) {
            return Err(invalid("Oracle original FILE observation is missing"));
        }
    }
    Ok(())
}
fn outcome_key(value: &Value) -> io::Result<String> {
    encoded(&(
        &value["relation_index"],
        &value["source_index"],
        &value["capture_index"],
    ))
}
pub(super) fn resolve(
    index: &Index,
    nodes: &mut [GraphNode],
    facts: &mut FileFacts,
    changed: Option<&BTreeSet<String>>,
    original: &Arc<SnapshotContext>,
) -> io::Result<()> {
    let mut references = BTreeMap::new();
    for reference in facts.oracle.iter().flat_map(|rows| rows.references()) {
        if changed.is_some_and(|changed| {
            reference
                .lookup_key()
                .is_none_or(|key| !changed.contains(key))
        }) {
            continue;
        }
        references.insert(
            encoded(&(
                reference.outcome().relation_index(),
                reference.outcome().source_index(),
                reference.outcome().capture_index(),
            ))?,
            reference,
        );
    }
    if references.is_empty() {
        return Ok(());
    }
    let mut file = facts
        .oracle_file
        .clone()
        .ok_or_else(|| invalid("Oracle original FILE observation disappeared"))?;
    let node = nodes
        .iter_mut()
        .find(|node| node.key == file.key())
        .ok_or_else(|| invalid("Oracle current FILE observation disappeared"))?;
    let mut slots = BTreeSet::new();
    for reference in references.into_values() {
        original.check()?;
        let candidates = reference
            .lookup_key()
            .map(|key| index.candidates::<Endpoint>("oracle_def", "lookup", key))
            .transpose()?
            .unwrap_or_default();
        let resolution = resolve_oracle_graph_reference(reference, &candidates, original)
            .map_err(io::Error::other)?;
        apply_oracle_graph_resolution(&mut file, &resolution, original)
            .map_err(io::Error::other)?;
        slots.insert(encoded(&(
            reference.outcome().relation_index(),
            reference.outcome().source_index(),
            reference.outcome().capture_index(),
        ))?);
        let mut group_edges = Vec::new();
        for edge in resolution.into_parts().0 {
            let (kind, source_key, target_key, uid) = edge.into_parts();
            let edge = GraphEdge {
                kind,
                source_key,
                target_key,
                uid,
            };
            if !group_edges.contains(&edge) {
                group_edges.push(edge);
            }
        }
        facts.oracle_edges.insert(
            encoded(&(
                reference.outcome().relation_index(),
                reference.outcome().source_index(),
                reference.outcome().capture_index(),
            ))?,
            group_edges,
        );
    }
    use crate::semantic_graph::AstGraphValue;
    let mut current: Vec<Value> = decoded(
        node_text(node, "resolution_outcomes_json")
            .ok_or_else(|| invalid("Oracle FILE outcomes disappeared"))?,
    )?;
    let Some(AstGraphValue::String(resolved)) = file.properties().get("resolution_outcomes_json")
    else {
        return Err(invalid("Oracle resolved FILE outcomes disappeared"));
    };
    for outcome in decoded::<Vec<Value>>(resolved)? {
        let key = outcome_key(&outcome)?;
        if !slots.contains(&key) {
            continue;
        }
        let positions = current
            .iter()
            .enumerate()
            .filter_map(|(position, value)| {
                (outcome_key(value).ok().as_ref() == Some(&key)).then_some(position)
            })
            .collect::<Vec<_>>();
        let [position] = positions.as_slice() else {
            return Err(invalid("Oracle outcome is not unique in current FILE"));
        };
        current[*position] = outcome;
    }
    node.properties.insert(
        "resolution_outcomes_json".into(),
        GraphValue::String(encoded(&current)?),
    );
    original.check()
}
