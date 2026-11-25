//! Native SDK delta preparation for progressively published lexical/vector rows.
use arrow_array::{Array, RecordBatch, StringArray};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, PreparedDelta},
    lance_module_store::RecordBatchStreamAdapter,
};
use std::{io, sync::Arc};

/// Prepare a FILE replacement using the SDK delta API. Empty input removes only
/// that FILE. The caller retains write authorization and commits/reconciles the
/// returned strict-fenced transaction; this function does not publish implicitly.
pub async fn prepare_file(
    snapshot: &BranchSnapshot,
    path: &str,
    batch: RecordBatch,
    original: &Arc<SnapshotContext>,
) -> io::Result<PreparedDelta> {
    original.check()?;
    if path.is_empty()
        || path.contains('\0')
        || crate::corpus::validate_schema(batch.schema().as_ref()).is_err()
        || snapshot.schema() != batch.schema().as_ref()
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let paths = array::<StringArray>(&batch, "path")?;
    if paths.iter().any(|value| value != Some(path)) {
        return Err(invalid());
    }
    let schema = batch.schema();
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        schema,
        futures::stream::iter([Ok(batch)]),
    ));
    let result = snapshot
        .prepare_replace_paths_stream(
            &[path.to_owned()],
            stream,
            vec!["uid".into(), "data_key".into()],
        )
        .await
        .map_err(io::Error::other)?;
    original.check()?;
    Ok(result)
}

pub async fn delete_file(
    snapshot: &BranchSnapshot,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> io::Result<PreparedDelta> {
    prepare_file(
        snapshot,
        path,
        RecordBatch::new_empty(Arc::new(snapshot.schema().clone())),
        original,
    )
    .await
}

fn array<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> io::Result<&'a T> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref())
        .ok_or_else(invalid)
}
fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

#[cfg(test)]
mod tests;
