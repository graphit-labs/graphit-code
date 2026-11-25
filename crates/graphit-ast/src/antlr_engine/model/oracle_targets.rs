//! Native Oracle object references for the configured Graph target rules.
use super::*;
use oracle_bindings::AntlrOracleBindingIssue as Issue;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct NamePart {
    decoded: String,
    quote: Option<char>,
}
pub(crate) type ObjectName = Vec<NamePart>;

pub(crate) fn key(name: &ObjectName) -> Result<String, Issue> {
    let parts = name
        .iter()
        .map(|part| {
            if part.quote.is_some() {
                Ok(part.decoded.clone())
            } else if part.decoded.is_ascii() {
                Ok(part.decoded.to_ascii_uppercase())
            } else {
                Err(Issue::DatabaseCharacterSet)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string(&parts).map_err(|_| Issue::NameCodec)
}

pub(super) fn required_origins(raw: &SemanticDocument<'_, '_, '_>) -> Vec<usize> {
    let nodes = &raw.syntax.nodes;
    let mut children = vec![Vec::new(); nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        if let Some(parent) = n.parent {
            children[parent].push(i);
        }
    }
    let mut roots = Vec::new();
    for occurrence in &raw.occurrences {
        if let CaptureBinding::Present(name) = &occurrence.name
            && matches!(
                nodes[name.node_index].kind.as_str(),
                "table_name" | "tableview_name"
            )
        {
            roots.push(name.node_index);
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        if n.kind == "schema_name" && n.parent.is_some_and(|p| nodes[p].kind == "create_table") {
            roots.push(i);
        }
    }
    let mut selected = std::collections::BTreeSet::new();
    for root in roots {
        let mut pending = vec![root];
        while let Some(i) = pending.pop() {
            selected.insert(i);
            pending.extend(children[i].iter().copied());
        }
        let mut ancestor = nodes[root].parent;
        while let Some(i) = ancestor {
            selected.insert(i);
            ancestor = nodes[i].parent;
        }
    }
    selected.into_iter().collect()
}

fn name(p: &ParsedAntlrDocument<'_, '_, '_, '_>, root: usize) -> Result<ObjectName, Issue> {
    p.bindings.usable(p, root)?;
    let mut pending = vec![root];
    let mut parts = Vec::new();
    while let Some(i) = pending.pop() {
        let node = p.node(i);
        if matches!(
            node.kind.as_str(),
            "INTRODUCER" | "link_name" | "partition_extension_clause" | "xmltable"
        ) {
            return Err(Issue::UnsupportedReference);
        }
        if node.kind == "id_expression" {
            let Some(OriginalProjection::Exact(raw)) = p.origins.get(&i) else {
                return Err(Issue::NonExactOrigin);
            };
            p.bindings.usable(p, i)?;
            let decoded = oracle_bindings::word(raw.text())?;
            parts.push(NamePart {
                decoded: decoded.decoded.to_string(),
                quote: decoded.quote,
            });
        } else {
            pending.extend(p.bindings.children(i).iter().rev().copied());
        }
    }
    if !(1..=2).contains(&parts.len()) {
        return Err(Issue::UnsupportedReference);
    }
    Ok(parts)
}

pub(crate) fn endpoint(
    document: &AntlrSemanticDocument<'_, '_, '_, '_, '_>,
    entity_index: usize,
) -> Result<ObjectName, Issue> {
    let p = document.parsed;
    let entity = &document.entities[entity_index];
    if p.db2.is_some() || p.node(entity.declaration_node_index()).kind != "create_table" {
        return Err(Issue::UnsupportedType);
    }
    let declaration = &p.declarations[&entity.declaration_node_index()];
    let mut result = Vec::new();
    for &i in p.bindings.children(entity.declaration_node_index()) {
        if p.node(i).kind == "schema_name" {
            result.extend(name(p, i)?);
        }
    }
    result.extend(name(p, declaration.name.ok_or(Issue::MissingNativeField)?)?);
    if !(1..=2).contains(&result.len()) {
        return Err(Issue::UnsupportedReference);
    }
    Ok(result)
}

pub(crate) struct Reference {
    pub name: ObjectName,
    pub source: usize,
}

pub(crate) fn reference(
    document: &AntlrSemanticDocument<'_, '_, '_, '_, '_>,
    occurrence_index: usize,
) -> Result<Reference, Issue> {
    let p = document.parsed;
    if p.db2.is_some() {
        return Err(Issue::UnsupportedReference);
    }
    let occurrence = &p.raw.occurrences[occurrence_index];
    let CaptureBinding::Present(capture) = &occurrence.name else {
        return Err(Issue::MissingNativeField);
    };
    if p.node(capture.node_index).kind != "tableview_name" {
        return Err(Issue::UnsupportedReference);
    }
    let name = name(p, capture.node_index)?;
    let mut ancestor = p.node(capture.node_index).parent;
    let mut body = None;
    while let Some(i) = ancestor {
        let node = p.node(i);
        if node.kind == "select_only_statement"
            && p.bindings
                .children(i)
                .iter()
                .any(|&child| p.node(child).kind == "with_clause")
        {
            return Err(Issue::UnsupportedScope);
        }
        if matches!(
            node.kind.as_str(),
            "parameter" | "variable_declaration" | "default_value_part"
        ) {
            return Err(Issue::UnsupportedScope);
        }
        if node.kind == "body" {
            body = Some(i);
        }
        if oracle_callable_label(&node.kind).is_some() {
            let body = body.ok_or(Issue::UnsupportedScope)?;
            if p.node(body).parent != Some(i) {
                return Err(Issue::UnsupportedScope);
            }
            p.bindings.usable(p, i)?;
            p.bindings.usable(p, body)?;
            let indices: Vec<_> = document
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    e.declaration_node_index() == i
                        && matches!(e.identity().label(), "Function" | "Procedure")
                })
                .map(|(i, _)| i)
                .collect();
            let [source] = indices.as_slice() else {
                return Err(Issue::UnsupportedScope);
            };
            return Ok(Reference {
                name,
                source: *source,
            });
        }
        ancestor = node.parent;
    }
    Err(Issue::UnsupportedScope)
}
