//! Graph output from the same retained FILE models used by NativeNN.
use super::*;
use graphit_ast::semantic_graph::{AstGraphNode, AstGraphValue, AstSemanticGraphBatch};

mod scip_index;
use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter};
pub(super) use scip_index::ScipIndex;

/// Publish syntax from the current FILE without waiting for a compiler index.
pub(super) fn append(
    writer: IcebugWriter,
    batch: AstSemanticGraphBatch,
    index: &ScipIndex,
    text: &str,
    live: &mut super::progressive::FilePublication,
    original: &Arc<SnapshotContext>,
) -> io::Result<IcebugWriter> {
    stage_syntax(writer, batch, None, index, text, live, original)
}

pub(super) fn append_oracle(
    writer: IcebugWriter,
    batch: AstSemanticGraphBatch,
    rows: graphit_ast::semantic_graph::oracle::AstOracleGraphRows,
    index: &ScipIndex,
    text: &str,
    live: &mut super::progressive::FilePublication,
    original: &Arc<SnapshotContext>,
) -> io::Result<IcebugWriter> {
    stage_syntax(writer, batch, Some(rows), index, text, live, original)
}

fn stage_syntax(
    writer: IcebugWriter,
    batch: AstSemanticGraphBatch,
    rows: Option<graphit_ast::semantic_graph::oracle::AstOracleGraphRows>,
    index: &ScipIndex,
    text: &str,
    live: &mut super::progressive::FilePublication,
    original: &Arc<SnapshotContext>,
) -> io::Result<IcebugWriter> {
    index.stage(&writer, &batch, rows.as_ref(), original)?;
    // Only this parser profile owns its deferred Oracle outcomes.
    let oracle_outcomes = rows
        .as_ref()
        .into_iter()
        .flat_map(|rows| rows.references())
        .map(|reference| {
            let outcome = reference.outcome();
            (
                Some(outcome.relation_index()),
                outcome.source_index(),
                outcome.capture_index(),
            )
        })
        .collect::<BTreeSet<_>>();
    if let Some(rows) = rows {
        if !rows.references().is_empty() {
            live.oracle_file = Some(batch.file_observation().clone());
        }
        if !rows.references().is_empty() || !rows.endpoints().is_empty() {
            live.oracle.push(rows);
        }
    }
    let (consumed, document) = index.syntax(batch.into_parts(), text, original)?;
    live.scip_document = Some(document);
    let parts = consumed.parts;
    if parts.association.path() != live.path
        || parts.association.source_sha256() != &live.source_sha256
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    live.search.push(graphit_ast::entity_search::encode_file(
        &parts, text, original,
    )?);
    live.definitions.extend(consumed.definitions);
    live.references
        .extend(consumed.references.into_iter().filter(|reference| {
            !oracle_outcomes.contains(&(
                reference.relation_index,
                reference.source_index,
                reference.capture_index,
            ))
        }));
    live.nodes.extend(parts.nodes.into_iter().map(node));
    live.edges.extend(parts.edges.into_iter().map(|edge| {
        let (kind, source_key, target_key, uid) = edge.into_parts();
        GraphEdge {
            kind,
            source_key,
            target_key,
            uid,
        }
    }));
    live.nodes.push(node(parts.file_observation));
    Ok(writer)
}

/// Materialize the original parser rows once, after compiler observations are ready.
/// The SQLite staging retains their original node order and Source association.
pub(super) fn materialize(
    writer: IcebugWriter,
    batch: AstSemanticGraphBatch,
    oracle_rows: Option<graphit_ast::semantic_graph::oracle::AstOracleGraphRows>,
    index: &ScipIndex,
    text: &str,
    nn_source: Option<(
        graphit_ast::native_nn::NativeNnSource<'_>,
        graphit_ast::symbols::IdentityLimits,
    )>,
    live: &mut super::progressive::FilePublication,
    original: &Arc<SnapshotContext>,
) -> io::Result<IcebugWriter> {
    if oracle_rows
        .as_ref()
        .is_some_and(|rows| !rows.references().is_empty())
    {
        live.oracle_file = Some(batch.file_observation().clone());
    }
    let parts = index.consume(
        &writer,
        batch.into_parts(),
        text,
        nn_source,
        oracle_rows.as_ref(),
        live,
        original,
    )?;
    if parts.association.path() != live.path
        || parts.association.source_sha256() != &live.source_sha256
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let lexical = graphit_ast::entity_search::encode_file(&parts, text, original)?;
    live.search.push(lexical);

    if let Some(rows) = oracle_rows
        && (!rows.references().is_empty() || !rows.endpoints().is_empty())
    {
        live.oracle.push(rows);
    }
    live.nodes.extend(parts.nodes.into_iter().map(node));
    live.edges.extend(parts.edges.into_iter().map(|edge| {
        let (kind, source_key, target_key, uid) = edge.into_parts();
        GraphEdge {
            kind,
            source_key,
            target_key,
            uid,
        }
    }));
    live.nodes.push(node(parts.file_observation));
    Ok(writer)
}

pub(super) fn node(node: AstGraphNode) -> GraphNode {
    let (label, key, properties) = node.into_parts();
    GraphNode {
        label,
        key,
        properties: properties
            .into_iter()
            .map(|(name, value)| {
                let value = match value {
                    AstGraphValue::String(value) => GraphValue::String(value),
                    AstGraphValue::Int64(value) => GraphValue::Int64(value),
                    AstGraphValue::Bool(value) => GraphValue::Bool(value),
                };
                (name, value)
            })
            .collect(),
    }
}
