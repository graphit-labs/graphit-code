//! Conditional vector enrichment of extant rows in the single AST corpus.
use arrow_array::{Array, RecordBatch, StringArray};
use graphit_ast::{
    corpus,
    native_nn::{NativeNnModel, native_nn_schema},
    native_nn_publication::{NativeNnPublication, finish_publication},
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::DataFusionError};
use std::{io, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, sleep_until},
};

const CHECKPOINT_ROWS: usize = 64;
pub(crate) const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(60);
pub(crate) struct FileInput {
    pub(crate) path: String,
    pub(crate) batches: mpsc::Receiver<Result<RecordBatch, DataFusionError>>,
    pub(crate) completed: oneshot::Receiver<()>,
    pub(crate) checkpoint_interval: Duration,
}
async fn control_failure(original: &SnapshotContext) -> io::Error {
    match original.deadline() {
        Some(deadline) => {
            tokio::select! { biased; () = original.cancellation().cancelled() => io::ErrorKind::Interrupted.into(), () = sleep_until(Instant::from_std(deadline)) => io::ErrorKind::TimedOut.into() }
        }
        None => {
            original.cancellation().cancelled().await;
            io::ErrorKind::Interrupted.into()
        }
    }
}
fn stream_failure(error: DataFusionError) -> io::Error {
    let kind = match &error {
        DataFusionError::IoError(error) => error.kind(),
        DataFusionError::External(error) => error
            .downcast_ref::<io::Error>()
            .map_or(io::ErrorKind::Other, io::Error::kind),
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}

pub(crate) async fn publish_file<F, Fut>(
    initial: &BranchSnapshot,
    file: FileInput,
    model: &NativeNnModel<'_>,
    original: &Arc<SnapshotContext>,
    recheck: &F,
    fence: &Arc<tokio::sync::Mutex<()>>,
    on_commit: &mut impl FnMut(&NativeNnPublication),
) -> io::Result<Option<NativeNnPublication>>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = io::Result<()>>,
{
    let FileInput {
        path,
        mut batches,
        completed,
        checkpoint_interval,
    } = file;
    let schema = native_nn_schema(model).map_err(io::Error::other)?;
    let mut pending = Vec::new();
    let mut rows = 0usize;
    let mut flush_at = None;
    let mut latest = None;
    let control = control_failure(original);
    tokio::pin!(control);
    loop {
        original.check()?;
        let incoming = tokio::select! {
            biased;
            error = &mut control => return Err(error),
            () = async { match flush_at { Some(deadline) => sleep_until(deadline).await, None => std::future::pending().await } }, if rows>0 => None,
            batch = batches.recv() => match batch { Some(batch) => Some(Some(batch)), None => Some(None) },
        };
        match incoming {
            Some(Some(batch)) => {
                let batch = batch.map_err(stream_failure)?;
                if batch.schema() != schema {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                let paths = batch
                    .column_by_name("path")
                    .and_then(|a| a.as_any().downcast_ref::<StringArray>())
                    .ok_or(io::ErrorKind::InvalidData)?;
                if paths.iter().any(|value| value != Some(path.as_str())) {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                rows += batch.num_rows();
                pending.push(batch);
                flush_at.get_or_insert_with(|| Instant::now() + checkpoint_interval);
                if rows < CHECKPOINT_ROWS {
                    continue;
                }
            }
            Some(None) => break,
            None => (),
        }
        if let Some(saved) = flush(
            initial,
            std::mem::take(&mut pending),
            model,
            original,
            recheck,
            fence,
            on_commit,
        )
        .await?
        {
            latest = Some(saved);
        }
        rows = 0;
        flush_at = None;
    }
    tokio::select! { biased; error=&mut control => return Err(error), result=completed => result.map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe,"AST inference ended without completing the FILE"))? }
    recheck().await?;
    original.check()?;
    if rows != 0
        && let Some(saved) =
            flush(initial, pending, model, original, recheck, fence, on_commit).await?
    {
        latest = Some(saved);
    }
    Ok(latest)
}

async fn flush<F, Fut>(
    initial: &BranchSnapshot,
    batches: Vec<RecordBatch>,
    model: &NativeNnModel<'_>,
    original: &Arc<SnapshotContext>,
    recheck: &F,
    fence: &Arc<tokio::sync::Mutex<()>>,
    on_commit: &mut impl FnMut(&NativeNnPublication),
) -> io::Result<Option<NativeNnPublication>>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = io::Result<()>>,
{
    if batches.is_empty() {
        return Ok(None);
    }
    let batch = arrow_select::concat::concat_batches(
        &native_nn_schema(model).map_err(io::Error::other)?,
        &batches,
    )
    .map_err(io::Error::other)?;
    let _fence = super::progressive::write_fence(fence, original).await?;
    recheck().await?;
    original.check()?;
    let current = BranchSnapshot::capture_observed(
        initial.module_snapshot().corpus_uri(),
        initial.physical_branch(),
        None,
    )
    .await
    .map_err(io::Error::other)?;
    let expected = corpus::vector_fields(model)?;
    if current.schema().field_with_name(corpus::VECTOR).ok() != Some(expected.field(0)) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let Some(prepared) = corpus::prepare_embeddings(&current, &batch, model, original).await?
    else {
        return Ok(None);
    };
    recheck().await?;
    original.check()?;
    let key = prepared.recovery_key().clone();
    let mut saved =
        finish_publication(prepared.commit().await, key, original).map_err(io::Error::other)?;
    on_commit(&saved);
    if let Some(error) = saved.take_completion_error() {
        return Err(error);
    }
    recheck().await?;
    let pin = saved.snapshot(original).map_err(io::Error::other)?;
    if let Some(index) = pin
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
        recheck().await?;
        original.check()?;
        index.commit().await.map_err(io::Error::other)?;
        recheck().await?;
    }
    Ok(Some(saved))
}
