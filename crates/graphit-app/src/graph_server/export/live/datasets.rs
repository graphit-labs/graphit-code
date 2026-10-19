//! Private datasets for one explicit immutable export, never a live index copy.
use arrow_array::{Array, RecordBatch, StringArray};
use futures_util::{StreamExt, TryStreamExt};
use graphit_ast::{
    corpus, entity_search, indexed_source::source_schema, live_graph::LiveFileObservation,
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, col, lit},
    lance_module_store::{DataFusionError, LanceModuleStore, RecordBatchStreamAdapter},
};
use sha2::{Digest, Sha256};
use std::{io, path::Path, sync::Arc};

pub(super) async fn source(
    raw: &BranchSnapshot,
    mut files: Vec<LiveFileObservation>,
    destination: &Path,
    original: &Arc<SnapshotContext>,
) -> io::Result<u64> {
    original.check()?;
    // Reuse one historical SDK pin at a time. FILEs retain their real parser
    // Raw versions; a newer unrelated Raw commit cannot replace their source.
    files.sort_by(|a, b| (a.raw_version, &a.path).cmp(&(b.raw_version, &b.path)));
    let lifetime = Arc::clone(original);
    let rows = futures_util::stream::try_unfold(
        (files.into_iter(), raw.clone(), None::<BranchSnapshot>),
        move |(mut files, raw, previous)| {
            let original = Arc::clone(&lifetime);
            async move {
                original.check().map_err(DataFusionError::IoError)?;
                let Some(file) = files.next() else {
                    return Ok::<_, DataFusionError>(None);
                };
                let pin = match previous {
                    Some(pin) if pin.version() == file.raw_version => pin,
                    _ => raw
                        .at_version(file.raw_version)
                        .await
                        .map_err(|error| DataFusionError::External(Box::new(error)))?,
                };
                let mut scan = pin.scan();
                scan.project(&["path", "source"])
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                scan.filter_expr(col("path").eq(lit(file.path.as_str())));
                // Two observations distinguish a unique FILE from corrupt duplicates.
                scan.limit(Some(2), None)
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let batches = scan
                    .try_into_stream()
                    .await
                    .map_err(|error| DataFusionError::External(Box::new(error)))?
                    .try_collect::<Vec<_>>()
                    .await
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                original.check().map_err(DataFusionError::IoError)?;
                let batch = arrow_select::concat::concat_batches(&source_schema(), &batches)
                    .map_err(DataFusionError::from)?;
                let invalid = || {
                    DataFusionError::IoError(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "exported FILE has no unique matching Source observation",
                    ))
                };
                if batch.num_rows() != 1 {
                    return Err(invalid());
                }
                let paths = batch
                    .column_by_name("path")
                    .and_then(|array| array.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(invalid)?;
                let text = batch
                    .column_by_name("source")
                    .and_then(|array| array.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(invalid)?;
                if paths.is_null(0) || paths.value(0) != file.path {
                    return Err(invalid());
                }
                if !text.is_null(0)
                    && <[u8; 32]>::from(Sha256::digest(text.value(0).as_bytes()))
                        != file.source_sha256
                {
                    return Err(invalid());
                }
                Ok(Some((batch, (files, raw, Some(pin)))))
            }
        },
    );
    let schema = source_schema();
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        schema.clone(),
        futures_util::stream::iter([Ok(RecordBatch::new_empty(schema.clone()))]).chain(rows),
    ));
    let prepared = LanceModuleStore::prepare_create_stream(
        destination.to_str().ok_or(io::ErrorKind::InvalidInput)?,
        schema,
        stream,
    )
    .await
    .map_err(io::Error::other)?;
    original.check()?;
    let (store, _) = prepared.commit().await.map_err(io::Error::other)?;
    let mut pin = BranchSnapshot::capture_observed(store.snapshot().corpus_uri(), None, None)
        .await
        .map_err(io::Error::other)?;
    if let Some(index) = pin
        .prepare_scalar_append("path", "path_lookup")
        .await
        .map_err(io::Error::other)?
    {
        original.check()?;
        pin = index.commit().await.map_err(io::Error::other)?;
    }
    original.check()?;
    Ok(pin.version())
}

pub(super) async fn corpus(
    snapshot: &BranchSnapshot,
    destination: &Path,
    original: &Arc<SnapshotContext>,
) -> io::Result<u64> {
    original.check()?;
    // Export the captured native branch snapshot independently of graph publication.
    let scan = snapshot.scan();
    let stream = scan.try_into_stream().await.map_err(io::Error::other)?;
    let schema = Arc::new(snapshot.schema().clone());
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        schema.clone(),
        futures_util::stream::iter([Ok(RecordBatch::new_empty(schema.clone()))])
            .chain(stream.map_err(|error| DataFusionError::External(Box::new(error)))),
    ));
    let prepared = LanceModuleStore::prepare_create_stream(
        destination.to_str().ok_or(io::ErrorKind::InvalidInput)?,
        schema,
        stream,
    )
    .await
    .map_err(io::Error::other)?;
    original.check()?;
    prepared.commit().await.map_err(io::Error::other)?;
    let mut pin = BranchSnapshot::capture_observed(
        destination.to_str().ok_or(io::ErrorKind::InvalidInput)?,
        None,
        None,
    )
    .await
    .map_err(io::Error::other)?;
    for column in ["path", "uid", "data_key"] {
        if let Some(index) = pin
            .prepare_scalar_append(column, &format!("{column}_entity"))
            .await
            .map_err(io::Error::other)?
        {
            original.check()?;
            pin = index.commit().await.map_err(io::Error::other)?;
        }
    }
    for column in entity_search::TEXT_COLUMNS {
        if let Some(index) = pin
            .prepare_code_fts_append(column, &format!("{column}_code_fts"))
            .await
            .map_err(io::Error::other)?
        {
            original.check()?;
            pin = index.commit().await.map_err(io::Error::other)?;
        }
    }
    if let Some(model) = corpus::model(pin.schema())? {
        let mut vectors = pin.scan();
        vectors.filter_expr(corpus::vector_filter(&model));
        vectors.project(&["uid"]).map_err(io::Error::other)?;
        vectors.limit(Some(1), None).map_err(io::Error::other)?;
        let has_vectors = vectors
            .try_into_stream()
            .await
            .map_err(io::Error::other)?
            .try_fold(false, |found, batch| async move {
                Ok(found || batch.num_rows() > 0)
            })
            .await
            .map_err(io::Error::other)?;
        original.check()?;
        if has_vectors
            && let Some(index) = pin
                .prepare_vector_index(
                    "vector",
                    "entity_vector",
                    &lance::index::vector::VectorIndexParams::ivf_flat(
                        1,
                        "cosine".try_into().map_err(io::Error::other)?,
                    ),
                    original,
                )
                .await
                .map_err(io::Error::other)?
        {
            original.check()?;
            pin = index.commit().await.map_err(io::Error::other)?;
        }
    }
    original.check()?;
    Ok(pin.version())
}

#[cfg(test)]
mod tests;
