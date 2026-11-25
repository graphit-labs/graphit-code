use super::*;
use futures::TryStreamExt;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, Expr, PreparedDelta, col, lit},
    lance_module_store::RecordBatchStreamAdapter,
};
use std::collections::HashSet;

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
    crate::native_nn::validate_native_nn(inferred, model, original).map_err(io::Error::other)?;
    let keys = identities(inferred)?.into_keys().collect();
    let current = rows_for_keys(snapshot, &keys, original).await?;
    let (current, inferred) = rebind_promoted(snapshot, current, inferred, original).await?;
    let patch = attach_embeddings(&current, &inferred, model, original)?;
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

async fn rows_for_keys(
    snapshot: &BranchSnapshot,
    keys: &HashSet<(String, String)>,
    original: &SnapshotContext,
) -> io::Result<RecordBatch> {
    let mut groups = HashMap::<&str, Vec<Expr>>::new();
    for (uid, key) in keys {
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
        return Ok(RecordBatch::new_empty(Arc::new(snapshot.schema().clone())));
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
    arrow_select::concat::concat_batches(&Arc::new(snapshot.schema().clone()), &batches)
        .map_err(io::Error::other)
}

// A compiler can promote a syntactic UID while inference is in flight. Match
// only missing identities against that same immutable Source occurrence. This
// is an operation-local join, not an alias table or a second identity format.
async fn rebind_promoted(
    snapshot: &BranchSnapshot,
    current: RecordBatch,
    inferred: &RecordBatch,
    original: &SnapshotContext,
) -> io::Result<(RecordBatch, RecordBatch)> {
    let present = identities(&current)?;
    let present_pairs = present
        .keys()
        .map(|(uid, key)| (uid.as_str(), key.as_str()))
        .collect::<HashSet<_>>();
    let uid = array::<StringArray>(inferred, "uid")?;
    let data_key = array::<StringArray>(inferred, "data_key")?;
    let paths = array::<StringArray>(inferred, "path")?;
    let mut missing = HashMap::<[u8; 32], Vec<usize>>::new();
    let mut files = std::collections::BTreeSet::new();
    for row in 0..inferred.num_rows() {
        original.check()?;
        if !present_pairs.contains(&(uid.value(row), data_key.value(row))) {
            missing
                .entry(occurrence(inferred, row)?)
                .or_default()
                .push(row);
            files.insert(paths.value(row));
        }
    }
    if missing.is_empty() {
        return Ok((current, inferred.clone()));
    }
    let mut scan = snapshot.scan();
    let columns = OCCURRENCE_FIELDS
        .iter()
        .copied()
        .chain(["uid", "input_fingerprint"])
        .collect::<Vec<_>>();
    scan.project(&columns).map_err(io::Error::other)?;
    scan.filter_expr(col("path").in_list(files.into_iter().map(lit).collect(), false));
    let mut stream = Box::pin(scan.try_into_stream())
        .await
        .map_err(io::Error::other)?;
    // At most one retained candidate for each missing inferred occurrence.
    // None marks ambiguity; it must not be guessed or resurrected later.
    let mut matched = HashMap::<usize, Option<String>>::new();
    while let Some(batch) = stream.try_next().await.map_err(io::Error::other)? {
        for row in 0..batch.num_rows() {
            original.check()?;
            if array::<FixedSizeBinaryArray>(&batch, "input_fingerprint")?.is_null(row) {
                continue;
            }
            let current_uid = array::<StringArray>(&batch, "uid")?.value(row);
            let current_key = array::<StringArray>(&batch, "data_key")?.value(row);
            if present_pairs.contains(&(current_uid, current_key)) {
                continue;
            }
            let Some(candidates) = missing.get(&occurrence(&batch, row)?) else {
                continue;
            };
            // Distinct inferred identities at an indistinguishable occurrence
            // cannot be rebound safely, even when their projections match.
            let [inferred_row] = candidates.as_slice() else {
                continue;
            };
            if !same_occurrence(&batch, row, inferred, *inferred_row)? {
                continue;
            }
            match matched.entry(*inferred_row) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Some(current_uid.to_owned()));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.insert(None);
                }
            }
        }
    }
    let matched = matched
        .into_iter()
        .filter_map(|(row, uid)| uid.map(|uid| (row, uid)))
        .collect::<HashMap<_, _>>();
    if matched.is_empty() {
        return Ok((current, inferred.clone()));
    }
    let selected = matched
        .iter()
        .map(|(row, uid)| (uid.clone(), data_key.value(*row).to_owned()))
        .collect();
    // Fetch full rows only after the narrow occurrence scan established a unique
    // match, using the same immutable SDK snapshot for both reads.
    let selected = rows_for_keys(snapshot, &selected, original).await?;
    let current = arrow_select::concat::concat_batches(&current.schema(), &[current, selected])
        .map_err(io::Error::other)?;
    let mut columns = inferred.columns().to_vec();
    columns[inferred
        .schema()
        .index_of("uid")
        .map_err(io::Error::other)?] = Arc::new(StringArray::from_iter_values(
        (0..inferred.num_rows()).map(|row| {
            matched
                .get(&row)
                .map_or_else(|| uid.value(row), String::as_str)
        }),
    ));
    original.check()?;
    Ok((
        current,
        RecordBatch::try_new(inferred.schema(), columns).map_err(io::Error::other)?,
    ))
}

// Hash only to locate candidates; equality below still checks every original
// value. No source text or vector is copied into this temporary lookup key.
const OCCURRENCE_FIELDS: &[&str] = &[
    "path",
    "data_key",
    "kind",
    "language",
    "grammar",
    "raw_version",
    "full_source_sha256",
    "projection_sha256",
    "extent",
];

fn occurrence(batch: &RecordBatch, row: usize) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(source_occurrence(batch, row)?);
    let version = array::<UInt64Array>(batch, "raw_version")?;
    if version.is_null(row) {
        return Err(invalid());
    }
    hash.update(version.value(row).to_le_bytes());
    Ok(hash.finalize().into())
}

pub(super) fn source_occurrence(batch: &RecordBatch, row: usize) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    for &name in OCCURRENCE_FIELDS {
        if name == "raw_version" {
            continue;
        }
        let value = batch.column_by_name(name).ok_or_else(invalid)?;
        if value.is_null(row) {
            return Err(invalid());
        }
        match value.data_type() {
            DataType::Utf8 => {
                let text = array::<StringArray>(batch, name)?.value(row);
                hash.update((text.len() as u64).to_le_bytes());
                hash.update(text);
            }
            DataType::UInt64 => {
                hash.update(array::<UInt64Array>(batch, name)?.value(row).to_le_bytes())
            }
            DataType::FixedSizeBinary(_) => {
                hash.update(array::<FixedSizeBinaryArray>(batch, name)?.value(row))
            }
            DataType::Struct(_) => {
                let extent = array::<arrow_array::StructArray>(batch, name)?;
                let range = extent
                    .column_by_name("range")
                    .and_then(|a| a.as_any().downcast_ref::<arrow_array::StructArray>())
                    .ok_or_else(invalid)?;
                for name in ["start", "end"] {
                    let bytes = range
                        .column_by_name(name)
                        .and_then(|a| a.as_any().downcast_ref::<UInt64Array>())
                        .ok_or_else(invalid)?;
                    if bytes.is_null(row) {
                        return Err(invalid());
                    }
                    hash.update(bytes.value(row).to_le_bytes());
                }
            }
            _ => return Err(invalid()),
        }
    }
    Ok(hash.finalize().into())
}

fn same_occurrence(a: &RecordBatch, ar: usize, b: &RecordBatch, br: usize) -> io::Result<bool> {
    let a_version = a.column_by_name("raw_version").ok_or_else(invalid)?;
    let b_version = b.column_by_name("raw_version").ok_or_else(invalid)?;
    Ok(
        a_version.slice(ar, 1).to_data() == b_version.slice(br, 1).to_data()
            && same_source_occurrence(a, ar, b, br)?,
    )
}

pub(super) fn same_source_occurrence(
    a: &RecordBatch,
    ar: usize,
    b: &RecordBatch,
    br: usize,
) -> io::Result<bool> {
    for &name in OCCURRENCE_FIELDS {
        if name == "raw_version" {
            continue;
        }
        let a = a.column_by_name(name).ok_or_else(invalid)?;
        let b = b.column_by_name(name).ok_or_else(invalid)?;
        if a.slice(ar, 1).to_data() != b.slice(br, 1).to_data() {
            return Ok(false);
        }
    }
    Ok(true)
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
