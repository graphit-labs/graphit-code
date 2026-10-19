//! Explicit export of independently captured graph, Source and search data.
use super::*;
use graphit_ast::{
    entity_search::EntitySearch,
    native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index},
    source_index_locator::{SourceIndexPresence, open_local_source_index},
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, SourceReadLimits},
    file_io::SnapshotContext,
};
use graphit_data::{
    CapturedLocalGraph, LocalGraphPolicy,
    graph_writer::{IcebugOutput, IcebugWriter},
};
use graphit_hub::policy::{HubResourcePolicy, ResourceSetting};
use std::{io::BufWriter, num::NonZeroU32};

mod datasets;

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}
/// Retain PROJECT admission and immutable native dataset snapshots throughout.
/// Only graph/SCIP capture excludes concurrent graph writes. Lance corpora do
/// not share a publication generation and cloning does not hold the graph lock.
pub(super) async fn capture(
    state: &Shared,
    prepared: &crate::project_graph::PreparedRead,
    index: &crate::foreground_ast_sources::progressive::LiveIndex,
    config: &graphit_core::config::ConfigSnapshot,
    original: &Arc<SnapshotContext>,
) -> io::Result<(IcebugOutput, Arc<CapturedLocalGraph>)> {
    original.check()?;
    let hub = HubResourcePolicy::resolve(config).map_err(io::Error::other)?;
    let graph_policy = LocalGraphPolicy::resolve(config, original)?;
    let SourceIndexPresence::Available(raw) = open_local_source_index(
        state.auth.installation(),
        prepared.project(),
        original,
        state.policy.index,
    )
    .await
    .map_err(io::Error::other)?
    else {
        return Err(io::ErrorKind::NotFound.into());
    };
    let corpus_owner = match open_local_native_nn_index(
        state.auth.installation(),
        prepared.project(),
        original,
        state.policy.index,
    )
    .await
    .map_err(io::Error::other)?
    {
        NativeNnIndexPresence::Absent => return Err(io::ErrorKind::NotFound.into()),
        NativeNnIndexPresence::Available(corpus) => corpus,
    };
    let corpus = corpus_owner
        .snapshot(original)
        .map_err(io::Error::other)?
        .clone();
    graphit_ast::corpus::validate_schema(corpus.schema())?;
    let graph_write = index.write_fence(original).await?;
    let files = index.graph.files(original)?;
    let writer = index
        .graph
        .write_snapshot(IcebugWriter::new(Arc::clone(original))?, original)?;
    let output = writer.finish()?;
    let limit = NonZeroU32::new(
        u32::try_from(hub.limit(ResourceSetting::SnapshotBytes).value).map_err(io::Error::other)?,
    )
    .ok_or_else(invalid)?;
    {
        use scip::types::{Metadata, TextEncoding, ToolInfo};
        let mut sink = BufWriter::new(std::fs::File::create(
            output.directory().join("index.scip"),
        )?);
        graphit_ast::scip_stream::write_metadata(
            &mut sink,
            &Metadata {
                project_root: "file:///workspace".into(),
                text_document_encoding: TextEncoding::UTF8.into(),
                tool_info: Some(ToolInfo {
                    name: "graphit".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            },
            original,
            limit,
        )
        .map_err(io::Error::other)?;
        index.graph.visit_scip_documents(original, |document| {
            graphit_ast::scip_stream::write_document(&mut sink, &document, original, limit)
                .map_err(io::Error::other)
        })?;
        index.graph.visit_scip_externals(original, |symbol| {
            graphit_ast::scip_stream::write_external_symbol(&mut sink, &symbol, original, limit)
                .map_err(io::Error::other)
        })?;
        std::io::Write::flush(&mut sink)?;
    }
    // The graph and its SCIP documents are captured. Other corpora remain
    // independent native snapshots; cloning holds no graph writer lock.
    drop(graph_write);
    datasets::source(
        raw.snapshot(original).map_err(io::Error::other)?,
        files,
        &output.directory().join("source.lance"),
        original,
    )
    .await?;
    datasets::corpus(
        &corpus,
        &output.directory().join(graphit_ast::corpus::DIRECTORY),
        original,
    )
    .await?;
    raw.revalidate(original).map_err(io::Error::other)?;
    corpus_owner
        .revalidate(original)
        .map_err(io::Error::other)?;
    let reader = CapturedSourceReader::capture_generated(
        &output.directory(),
        graphit_fs::NodeKind::Directory,
        SourceReadLimits::generated(
            usize::try_from(hub.limit(ResourceSetting::RetainedBytes).value)
                .map_err(io::Error::other)?,
            64 * 1024,
        )?,
        Arc::clone(original),
    )?;
    let graph = Arc::new(
        CapturedLocalGraph::from_reader(reader, Arc::clone(original), graph_policy)
            .map_err(io::Error::other)?,
    );
    let search = EntitySearch::open(Arc::clone(&graph), Arc::clone(original))
        .await?
        .ok_or_else(invalid)?;
    let model = graphit_ast::corpus::model(corpus.schema())?;
    // A Raw version may be newer than a still-published FILE. Detect that
    // interval before publishing, rather than packaging mismatched Source.
    search.validate_export(model.as_ref()).await?;
    original.check()?;
    Ok((output, graph))
}
