use super::*;
use crate::semantic_graph::scip_document::AstGraphIdentityChange;
use arrow_array::builder::FixedSizeBinaryBuilder;
use std::collections::HashSet;

/// Apply the same captured FILE's SCIP identity promotion to lexical, prepared
/// or corpus rows. Only identity columns and their content guard are replaced;
/// vectors, model metadata and projection bytes remain shared Arrow buffers.
pub fn promote_identities(
    batch: &RecordBatch,
    changes: &[AstGraphIdentityChange],
    original: &SnapshotContext,
) -> io::Result<RecordBatch> {
    original.check()?;
    if changes.is_empty() || batch.num_rows() == 0 {
        return Ok(batch.clone());
    }
    let uid = array::<StringArray>(batch, "uid")?;
    let key = array::<StringArray>(batch, "data_key")?;
    if uid.null_count() != 0 || key.null_count() != 0 {
        return Err(invalid());
    }
    let mut lookup = HashMap::with_capacity(changes.len());
    for change in changes {
        original.check()?;
        if [
            &change.previous_uid,
            &change.uid,
            &change.data_key,
            &change.previous_key,
            &change.key,
        ]
        .iter()
        .any(|value| value.is_empty())
        {
            return Err(invalid());
        }
        if let Some(previous) = lookup.insert(
            (change.previous_uid.as_str(), change.data_key.as_str()),
            change,
        ) && previous != change
        {
            return Err(invalid());
        }
    }
    let selected = (0..batch.num_rows())
        .map(|row| lookup.get(&(uid.value(row), key.value(row))).copied())
        .collect::<Vec<_>>();
    if selected.iter().all(Option::is_none) {
        original.check()?;
        return Ok(batch.clone());
    }
    let promoted: ArrayRef = Arc::new(StringArray::from_iter_values(
        selected
            .iter()
            .enumerate()
            .map(|(row, change)| change.map_or_else(|| uid.value(row), |c| c.uid.as_str())),
    ));
    let entity_key = if batch.column_by_name("entity_key").is_some() {
        let observed = array::<StringArray>(batch, "entity_key")?;
        if observed.null_count() != 0 {
            return Err(invalid());
        }
        for (row, change) in selected.iter().enumerate() {
            original.check()?;
            if let Some(change) = change
                && (observed.is_null(row) || observed.value(row) != change.previous_key)
            {
                return Err(invalid());
            }
        }
        Some(Arc::new(StringArray::from_iter_values(
            selected.iter().enumerate().map(|(row, change)| {
                change.map_or_else(|| observed.value(row), |c| c.key.as_str())
            }),
        )) as ArrayRef)
    } else {
        None
    };
    let columns = batch
        .schema()
        .fields()
        .iter()
        .zip(batch.columns())
        .map(|(field, column)| match field.name().as_str() {
            "uid" => promoted.clone(),
            "entity_key" => entity_key.as_ref().unwrap().clone(),
            _ => column.clone(),
        })
        .collect();
    let mut updated = RecordBatch::try_new(batch.schema(), columns).map_err(io::Error::other)?;
    let updated_uid = array::<StringArray>(&updated, "uid")?;
    let mut identities = HashSet::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        original.check()?;
        if !identities.insert((updated_uid.value(row), key.value(row))) {
            return Err(invalid());
        }
    }
    if batch.column_by_name("input_fingerprint").is_some() {
        let observed = array::<FixedSizeBinaryArray>(batch, "input_fingerprint")?;
        if observed.value_length() != 32 {
            return Err(invalid());
        }
        let mut guards = FixedSizeBinaryBuilder::with_capacity(batch.num_rows(), 32);
        for (row, change) in selected.iter().enumerate() {
            original.check()?;
            if observed.is_null(row) {
                guards.append_null();
            } else {
                let guard = if change.is_some() {
                    if observed.value(row) != fingerprint(batch, row)? {
                        return Err(invalid());
                    }
                    fingerprint(&updated, row)?
                } else {
                    observed.value(row).try_into().map_err(|_| invalid())?
                };
                guards.append_value(guard).map_err(io::Error::other)?;
            }
        }
        let column = updated
            .schema()
            .index_of("input_fingerprint")
            .map_err(io::Error::other)?;
        let mut columns = updated.columns().to_vec();
        columns[column] = Arc::new(guards.finish());
        updated = RecordBatch::try_new(updated.schema(), columns).map_err(io::Error::other)?;
    }
    original.check()?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{FixedSizeListArray, Float32Array};
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    fn original() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        )
    }
    fn change() -> AstGraphIdentityChange {
        AstGraphIdentityChange {
            label: "Function".into(),
            data_key: "functions".into(),
            start_byte: 3,
            end_byte: 8,
            previous_uid: "syntax".into(),
            uid: "semantic".into(),
            previous_key: "syntax-key".into(),
            key: "semantic-key".into(),
        }
    }
    fn fixed(values: &[[u8; 32]]) -> ArrayRef {
        let mut builder = FixedSizeBinaryBuilder::with_capacity(values.len(), 32);
        for value in values {
            builder.append_value(value).unwrap();
        }
        Arc::new(builder.finish())
    }
    fn fixture() -> RecordBatch {
        let mut fields = Vec::new();
        let mut columns: Vec<ArrayRef> = Vec::new();
        for (name, values) in [
            ("uid", ["syntax", "sibling"]),
            ("data_key", ["functions", "functions"]),
            ("entity_key", ["syntax-key", "sibling-key"]),
            ("projection", ["fn ação_日本語() {}", "fn sibling() {}"]),
        ] {
            fields.push(Field::new(name, DataType::Utf8, false));
            columns.push(Arc::new(StringArray::from(values.to_vec())));
        }
        for name in [
            "projection_sha256",
            "full_source_sha256",
            "vector_id",
            "input_fingerprint",
        ] {
            fields.push(Field::new(name, DataType::FixedSizeBinary(32), false));
            columns.push(fixed(&[[7; 32], [9; 32]]));
        }
        for name in ["raw_version", "model_generation"] {
            fields.push(Field::new(name, DataType::UInt64, false));
            columns.push(Arc::new(UInt64Array::from(vec![11, 13])));
        }
        let item = Arc::new(Field::new("item", DataType::Float32, true));
        fields.push(Field::new(
            VECTOR,
            DataType::FixedSizeList(item.clone(), 2),
            false,
        ));
        columns.push(Arc::new(FixedSizeListArray::new(
            item,
            2,
            Arc::new(Float32Array::from(vec![0.25, 0.75, 1.0, 0.0])),
            None,
        )));
        let schema = Arc::new(Schema::new_with_metadata(
            fields,
            HashMap::from([("fixture".into(), "same captured FILE".into())]),
        ));
        let batch = RecordBatch::try_new(schema.clone(), columns).unwrap();
        let mut columns = batch.columns().to_vec();
        columns[schema.index_of("input_fingerprint").unwrap()] = fixed(&[
            fingerprint(&batch, 0).unwrap(),
            fingerprint(&batch, 1).unwrap(),
        ]);
        RecordBatch::try_new(schema, columns).unwrap()
    }
    #[test]
    fn promotion_preserves_vectors_model_source_and_unaffected_row_without_copying_buffers() {
        let before = fixture();
        let after = promote_identities(&before, &[change()], &original()).unwrap();
        assert_eq!(after.schema(), before.schema());
        assert_eq!(
            array::<StringArray>(&after, "uid").unwrap().value(0),
            "semantic"
        );
        assert_eq!(
            array::<StringArray>(&after, "entity_key").unwrap().value(0),
            "semantic-key"
        );
        for name in [
            VECTOR,
            "projection",
            "projection_sha256",
            "full_source_sha256",
            "raw_version",
            "model_generation",
            "vector_id",
            "data_key",
        ] {
            assert!(
                Arc::ptr_eq(
                    before.column_by_name(name).unwrap(),
                    after.column_by_name(name).unwrap()
                ),
                "{name}"
            );
        }
        let guards = array::<FixedSizeBinaryArray>(&after, "input_fingerprint").unwrap();
        assert_eq!(guards.value(0), fingerprint(&after, 0).unwrap());
        assert_ne!(
            guards.value(0),
            array::<FixedSizeBinaryArray>(&before, "input_fingerprint")
                .unwrap()
                .value(0)
        );
        assert_eq!(
            guards.value(1),
            array::<FixedSizeBinaryArray>(&before, "input_fingerprint")
                .unwrap()
                .value(1)
        );
        assert_eq!(
            array::<StringArray>(&after, "uid").unwrap().value(1),
            "sibling"
        );
    }
    #[test]
    fn prepared_rows_without_lexical_key_are_promoted_and_no_match_reuses_all_arrays() {
        let full = fixture();
        let prepared = full.project(&[0, 1, 3]).unwrap();
        let after = promote_identities(&prepared, &[change()], &original()).unwrap();
        assert_eq!(
            array::<StringArray>(&after, "uid").unwrap().value(0),
            "semantic"
        );
        assert!(Arc::ptr_eq(prepared.column(2), after.column(2)));
        let mut absent = change();
        absent.previous_uid = "absent".into();
        let unchanged = promote_identities(&full, &[absent], &original()).unwrap();
        assert!(
            full.columns()
                .iter()
                .zip(unchanged.columns())
                .all(|(a, b)| Arc::ptr_eq(a, b))
        );
    }
    #[test]
    fn conflicting_maps_wrong_keys_and_collisions_fail_without_changing_input() {
        let batch = fixture();
        let first = change();
        let mut conflict = first.clone();
        conflict.uid = "different".into();
        assert!(promote_identities(&batch, &[first.clone(), conflict], &original()).is_err());
        let mut wrong_key = first.clone();
        wrong_key.previous_key = "foreign".into();
        assert!(promote_identities(&batch, &[wrong_key], &original()).is_err());
        let mut collision = first;
        collision.uid = "sibling".into();
        assert!(promote_identities(&batch, &[collision], &original()).is_err());
        assert_eq!(
            array::<StringArray>(&batch, "uid").unwrap().value(0),
            "syntax"
        );
    }
    #[test]
    fn stale_fingerprint_cancel_and_deadline_are_not_repaired_or_ignored() {
        let batch = fixture();
        let mut columns = batch.columns().to_vec();
        columns[batch.schema().index_of("input_fingerprint").unwrap()] = fixed(&[[0; 32], [0; 32]]);
        let corrupt = RecordBatch::try_new(batch.schema(), columns).unwrap();
        assert!(promote_identities(&corrupt, &[change()], &original()).is_err());
        let cancelled = original();
        cancelled.cancellation().cancel();
        assert_eq!(
            promote_identities(&batch, &[], &cancelled)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
        assert_eq!(
            promote_identities(&batch, &[change()], &expired)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
}
