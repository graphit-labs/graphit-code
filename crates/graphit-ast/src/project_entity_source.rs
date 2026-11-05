//! Entity extents and Source association observed from a confirmed PROJECT graph.
//! The caller retains authorization, graph and Source captures through delivery.
use crate::{
    icebug_query::{QueryOptions, execute},
    imported_graph::EntitySourceError,
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::CapturedLocalGraph;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io, sync::Arc};
use tokio::runtime::Handle;

/// Observed metadata, not authorization or a Source capture.
pub struct ProjectEntitySource {
    pub(crate) start: usize,
    pub(crate) end: usize,
    hash: [u8; 32],
}
impl ProjectEntitySource {
    pub(crate) fn matches_text(&self, text: &str) -> bool {
        Sha256::digest(text.as_bytes()).as_slice() == self.hash
    }
}
fn scalar<'a>(cell: &'a Value, kind: &str) -> Result<&'a str, EntitySourceError> {
    let object = cell.as_object().ok_or(EntitySourceError::InvalidExtent)?;
    if object.len() != 2 || object.get("type").and_then(Value::as_str) != Some(kind) {
        return Err(EntitySourceError::InvalidExtent);
    }
    object
        .get("value")
        .and_then(Value::as_str)
        .ok_or(EntitySourceError::InvalidExtent)
}
fn binding(row: &[Value]) -> Result<ProjectEntitySource, EntitySourceError> {
    if row.len() != 3 {
        return Err(EntitySourceError::InvalidExtent);
    }
    let start = scalar(&row[0], "INT64")?
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or(EntitySourceError::InvalidExtent)?;
    let end = scalar(&row[1], "INT64")?
        .parse::<usize>()
        .ok()
        .filter(|n| *n >= start)
        .ok_or(EntitySourceError::InvalidExtent)?;
    let digest = scalar(&row[2], "STRING")?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(EntitySourceError::InvalidExtent);
    }
    let mut hash = [0; 32];
    for (i, pair) in digest.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        hash[i] = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_| EntitySourceError::InvalidExtent)?,
            16,
        )
        .map_err(|_| EntitySourceError::InvalidExtent)?;
    }
    Ok(ProjectEntitySource { start, end, hash })
}

/// Resolve a real exact entity extent using the supplied captured PROJECT graph.
/// Query values remain parameters and only captured schema labels are quoted.
/// Call on the existing owned blocking job; no graph or authorization is opened here.
pub fn resolve_project_entity_source(
    graph: &Arc<CapturedLocalGraph>,
    original: &Arc<SnapshotContext>,
    runtime: &Handle,
    path: &str,
    entity: &str,
    entity_type: Option<&str>,
    options: QueryOptions,
) -> Result<ProjectEntitySource, EntitySourceError> {
    let schema = graph
        .schema()
        .map_err(io::Error::other)?
        .node_tables()
        .map(|table| {
            (
                table.label().to_owned(),
                table
                    .columns()
                    .map(|(name, kind)| (name.to_owned(), kind.to_owned()))
                    .collect(),
            )
        })
        .collect();
    let result = resolve_observed_entity(
        schema,
        original,
        path,
        entity,
        entity_type,
        options,
        |query, parameters| execute(graph, original, runtime, query, parameters, options),
    );
    graph.schema().map_err(io::Error::other)?;
    result
}

/// Resolve the same entity contract against the transactional native graph.
/// The composing caller retains its PROJECT admission and FILE read fence.
pub fn resolve_live_project_entity_source(
    graph: &crate::live_graph::LiveGraph,
    original: &Arc<SnapshotContext>,
    path: &str,
    entity: &str,
    entity_type: Option<&str>,
    options: QueryOptions,
) -> Result<ProjectEntitySource, EntitySourceError> {
    let schema = graph.schema()?.nodes.into_iter().collect();
    resolve_observed_entity(
        schema,
        original,
        path,
        entity,
        entity_type,
        options,
        |query, parameters| graph.query(query, parameters, options, original),
    )
}

fn resolve_observed_entity(
    schema: Vec<(String, BTreeMap<String, String>)>,
    original: &Arc<SnapshotContext>,
    path: &str,
    entity: &str,
    entity_type: Option<&str>,
    options: QueryOptions,
    mut run: impl FnMut(&str, &BTreeMap<String, Value>) -> io::Result<crate::icebug_query::QueryResult>,
) -> Result<ProjectEntitySource, EntitySourceError> {
    original.check()?;
    if path.is_empty()
        || path.contains('\0')
        || entity.is_empty()
        || entity.contains('\0')
        || entity_type.is_some_and(|s| s.is_empty() || s.contains('\0'))
        || options.max_rows < 2
    {
        return Err(EntitySourceError::InvalidRequest);
    }
    let mut tables = Vec::new();
    let mut known = false;
    for (label, fields) in schema {
        if entity_type.is_some_and(|kind| kind != label) {
            continue;
        }
        known = true;
        if [
            ("name", "STRING"),
            ("path", "STRING"),
            ("start_line", "INT64"),
            ("end_line", "INT64"),
            ("source_sha256", "STRING"),
            ("available", "BOOL"),
            ("extent_status", "STRING"),
        ]
        .iter()
        .all(|(name, kind)| fields.get(*name).map(String::as_str) == Some(*kind))
        {
            tables.push(label);
        } else if entity_type.is_some() {
            return Err(EntitySourceError::InvalidExtent);
        }
    }
    if entity_type.is_some() && !known {
        return Err(EntitySourceError::UnknownType);
    }
    let parameters = BTreeMap::from([
        ("path".into(), Value::String(path.into())),
        ("entity".into(), Value::String(entity.into())),
    ]);
    for folded in [false, true] {
        let mut found = None;
        for label in &tables {
            original.check()?;
            let label = label.replace('`', "``");
            let name = if folded {
                "lower(n.name) = lower($entity)"
            } else {
                "n.name = $entity"
            };
            let query = format!(
                "MATCH (n:`{label}`) WHERE n.path = $path AND {name} AND n.available = true AND n.extent_status = 'Exact' RETURN n.start_line AS source_start, n.end_line AS source_end, n.source_sha256 AS source_hash LIMIT 2"
            );
            let result = run(&query, &parameters)?;
            if result.columns != ["source_start", "source_end", "source_hash"]
                || result.rows.len() > 2
            {
                return Err(EntitySourceError::InvalidExtent);
            }
            for row in result.rows {
                let item = binding(&row)?;
                if found.replace(item).is_some() {
                    return Err(EntitySourceError::Ambiguous);
                }
            }
        }
        if let Some(found) = found {
            original.check()?;
            return Ok(found);
        }
    }
    Err(EntitySourceError::Unavailable)
}
