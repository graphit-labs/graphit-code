use super::*;
use futures::TryStreamExt;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, Expr, PreparedDelta, col, lit},
    lance_module_store::RecordBatchStreamAdapter,
};

/// Re-read only the inferred identities at the retained current pin. A missing
/// row or a changed fingerprint is skipped. The strict prepared commit fences
/// concurrent changes after this comparison; no unmatched entity can be inserted.
pub async fn prepare_embeddings(
    snapshot: &BranchSnapshot,
    inferred: &RecordBatch,
    model: &crate::native_nn::NativeNnModel<'_>,
    original: &Arc<SnapshotContext>,
) -> io::Result<Option<PreparedDelta>> {
    original.check()?;
    validate_schema(snapshot.schema())?;
    let keys = identities(inferred)?;
    let mut groups = HashMap::<&str, Vec<Expr>>::new();
    for (uid, key) in keys.keys() {
        groups
            .entry(key.as_str())
            .or_default()
            .push(lit(uid.as_str()));
    }
    let predicates = groups
        .into_iter()
        .map(|(key, uids)| {
            // Grouped IN preserves each exact pair; independent IN lists for
            // both columns would also admit their unwanted cross product.
            col("data_key")
                .eq(lit(key))
                .and(col("uid").in_list(uids, false))
        })
        .collect();
    let Some(predicate) = disjunction(predicates) else {
        return Ok(None);
    };
    let mut scanner = snapshot.scan();
    scanner.filter_expr(predicate);
    let mut stream = Box::pin(scanner.try_into_stream())
        .await
        .map_err(io::Error::other)?;
    let mut batches = Vec::new();
    let mut count = 0;
    while let Some(batch) = stream.try_next().await.map_err(io::Error::other)? {
        original.check()?;
        count += batch.num_rows();
        if count > keys.len() {
            return Err(invalid());
        }
        batches.push(batch);
    }
    let current =
        arrow_select::concat::concat_batches(&Arc::new(snapshot.schema().clone()), &batches)
            .map_err(io::Error::other)?;
    let patch = attach_embeddings(&current, inferred, model, original)?;
    if patch.num_rows() == 0 {
        return Ok(None);
    }
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        patch.schema(),
        futures::stream::iter([Ok(patch)]),
    ));
    let prepared = Box::pin(snapshot.prepare_merge_stream(
        stream,
        vec!["uid".into(), "data_key".into()],
        None,
    ))
    .await
    .map_err(io::Error::other)?;
    original.check()?;
    Ok(Some(prepared))
}

// Keep exact identity pairs, but not a left-deep OR chain: the SDK planner
// traverses expressions recursively, so large FILE checkpoints need log depth.
fn disjunction(mut expressions: Vec<Expr>) -> Option<Expr> {
    while expressions.len() > 1 {
        let mut pairs = expressions.into_iter();
        expressions = std::iter::from_fn(|| {
            let left = pairs.next()?;
            Some(match pairs.next() {
                Some(right) => left.or(right),
                None => left,
            })
        })
        .collect();
    }
    expressions.pop()
}
