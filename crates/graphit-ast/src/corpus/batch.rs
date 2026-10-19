use super::*;
use crate::native_nn::{
    NATIVE_ENTITY_PROJECTION_RECIPE, NativeNnModel, native_nn_projection_schema, native_nn_schema,
};
use arrow_array::{BooleanArray, builder::FixedSizeBinaryBuilder, new_null_array};

/// Join the complete lexical FILE with its eligible parser projections. Existing
/// compatible vectors survive a Source/Raw revision with identical projection.
pub fn encode_file(
    schema: Arc<Schema>,
    lexical: RecordBatch,
    prepared: RecordBatch,
    previous: Option<&RecordBatch>,
    original: &SnapshotContext,
) -> io::Result<RecordBatch> {
    original.check()?;
    validate_schema(&schema)?;
    if lexical.schema() != crate::entity_search::schema()
        || prepared.schema() != native_nn_projection_schema()
    {
        return Err(invalid());
    }
    crate::native_nn::validate_native_nn_projection(&prepared, original)
        .map_err(io::Error::other)?;
    let lexical_keys = identities(&lexical)?;
    let prepared_keys = identities(&prepared)?;
    if prepared_keys
        .keys()
        .any(|key| !lexical_keys.contains_key(key))
    {
        return Err(invalid());
    }
    let uid = array::<StringArray>(&lexical, "uid")?;
    let key = array::<StringArray>(&lexical, "data_key")?;
    let selected = (0..lexical.num_rows())
        .map(|row| {
            prepared_keys
                .get(&(uid.value(row).into(), key.value(row).into()))
                .copied()
        })
        .collect::<Vec<_>>();
    let previous_keys = previous.map(identities).transpose()?.unwrap_or_default();
    if let Some(previous) = previous {
        validate_schema(previous.schema().as_ref())?;
    }
    let previous_vector_id = previous
        .map(|batch| {
            super::model(batch.schema().as_ref()).map(|model| model.map(|model| model.vector_id))
        })
        .transpose()?
        .flatten();
    let mut fingerprints = FixedSizeBinaryBuilder::with_capacity(lexical.num_rows(), 32);
    let mut reuse = Vec::with_capacity(lexical.num_rows());
    for (row, projection) in selected.iter().enumerate() {
        original.check()?;
        let Some(projection) = projection.map(|row| row as usize) else {
            fingerprints.append_null();
            reuse.push(None);
            continue;
        };
        for name in [
            "uid",
            "data_key",
            "kind",
            "path",
            "language",
            "grammar",
            "raw_version",
        ] {
            if lexical
                .column_by_name(name)
                .ok_or_else(invalid)?
                .slice(row, 1)
                .to_data()
                != prepared
                    .column_by_name(name)
                    .ok_or_else(invalid)?
                    .slice(projection, 1)
                    .to_data()
            {
                return Err(invalid());
            }
        }
        let full =
            array::<FixedSizeBinaryArray>(&prepared, "full_source_sha256")?.value(projection);
        let observed = array::<StringArray>(&lexical, "source_sha256")?;
        let expected_hex = full
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if observed.is_null(row) || observed.value(row) != expected_hex {
            return Err(invalid());
        }
        let expected = fingerprint(&prepared, projection)?;
        fingerprints
            .append_value(expected)
            .map_err(io::Error::other)?;
        let prior = previous_keys
            .get(&(uid.value(row).into(), key.value(row).into()))
            .copied();
        let compatible = match (previous, prior) {
            (Some(old), Some(index))
                if old
                    .column_by_name(VECTOR)
                    .is_some_and(|a| !a.is_null(index as usize)) =>
            {
                let index = index as usize;
                let hashes = array::<FixedSizeBinaryArray>(old, "projection_sha256")?;
                let ids = array::<FixedSizeBinaryArray>(old, "vector_id")?;
                let fingerprints = array::<FixedSizeBinaryArray>(old, "input_fingerprint")?;
                if fingerprints.is_null(index)
                    || fingerprints.value(index) != fingerprint(old, index)?
                    || ids.is_null(index)
                    || ids.value(index) != previous_vector_id.ok_or_else(invalid)?
                {
                    return Err(invalid());
                }
                !hashes.is_null(index)
                    && hashes.value(index)
                        == array::<FixedSizeBinaryArray>(&prepared, "projection_sha256")?
                            .value(projection)
                    && old.schema().field_with_name(VECTOR).ok()
                        == schema.field_with_name(VECTOR).ok()
            }
            _ => false,
        };
        reuse.push(if compatible { prior } else { None });
    }
    let fingerprints: ArrayRef = Arc::new(fingerprints.finish());
    let strings = |value: &'static str| -> ArrayRef {
        Arc::new(StringArray::from(
            selected
                .iter()
                .map(|row| row.map(|_| value))
                .collect::<Vec<_>>(),
        ))
    };
    let mut columns = Vec::new();
    for field in schema.fields() {
        let name = field.name().as_str();
        let array = match name {
            "projection_name" => take(
                prepared.column_by_name("name").ok_or_else(invalid)?,
                selected.clone(),
            )?,
            "input_fingerprint" => fingerprints.clone(),
            "projection_recipe" => strings(NATIVE_ENTITY_PROJECTION_RECIPE),
            "role" => strings("CodeDocument"),
            "vector_id" | "model_generation" | VECTOR => {
                match previous.and_then(|old| old.column_by_name(name)) {
                    Some(array) if array.data_type() == field.data_type() => {
                        take(array, reuse.clone())?
                    }
                    _ => new_null_array(field.data_type(), lexical.num_rows()),
                }
            }
            _ => match lexical.column_by_name(name) {
                Some(array) => array.clone(),
                None => take(
                    prepared.column_by_name(name).ok_or_else(invalid)?,
                    selected.clone(),
                )?,
            },
        };
        columns.push(array);
    }
    original.check()?;
    RecordBatch::try_new(schema, columns).map_err(io::Error::other)
}

/// Return genuine prepared observations for inference; lexical-only rows remain
/// in the corpus and are excluded from this projection without fabricated ranges.
pub fn projections(batch: &RecordBatch, original: &SnapshotContext) -> io::Result<RecordBatch> {
    original.check()?;
    validate_schema(batch.schema().as_ref())?;
    let values = array::<StringArray>(batch, "projection")?;
    let selected = BooleanArray::from(
        (0..batch.num_rows())
            .map(|row| !values.is_null(row))
            .collect::<Vec<_>>(),
    );
    let batch =
        arrow_select::filter::filter_record_batch(batch, &selected).map_err(io::Error::other)?;
    let schema = native_nn_projection_schema();
    let arrays = schema
        .fields()
        .iter()
        .map(|field| {
            batch
                .column_by_name(if field.name() == "name" {
                    "projection_name"
                } else {
                    field.name()
                })
                .cloned()
                .ok_or_else(invalid)
        })
        .collect::<io::Result<Vec<_>>>()?;
    let prepared = RecordBatch::try_new(schema, arrays).map_err(io::Error::other)?;
    crate::native_nn::validate_native_nn_projection(&prepared, original)
        .map_err(io::Error::other)?;
    original.check()?;
    Ok(prepared)
}

/// Native NN payload is a transient view of eligible rows in this same corpus,
/// never a second persisted dataset. Identity filtering precedes NN validation.
pub fn vectors(
    batch: &RecordBatch,
    model: &NativeNnModel<'_>,
    original: &SnapshotContext,
) -> io::Result<RecordBatch> {
    original.check()?;
    validate_schema(batch.schema().as_ref())?;
    let schema = native_nn_schema(model).map_err(io::Error::other)?;
    if batch.schema().field_with_name(VECTOR).ok() != Some(vector_fields(model)?.field(0)) {
        return Ok(RecordBatch::new_empty(schema));
    }
    let ids = array::<FixedSizeBinaryArray>(batch, "vector_id")?;
    let vector = batch.column_by_name(VECTOR).ok_or_else(invalid)?;
    let mask = BooleanArray::from(
        (0..batch.num_rows())
            .map(|row| {
                !vector.is_null(row) && !ids.is_null(row) && ids.value(row) == model.vector_id
            })
            .collect::<Vec<_>>(),
    );
    let batch =
        arrow_select::filter::filter_record_batch(batch, &mask).map_err(io::Error::other)?;
    let fingerprints = array::<FixedSizeBinaryArray>(&batch, "input_fingerprint")?;
    for row in 0..batch.num_rows() {
        original.check()?;
        if fingerprints.is_null(row) || fingerprints.value(row) != fingerprint(&batch, row)? {
            return Err(invalid());
        }
        let role = array::<StringArray>(&batch, "role")?;
        let recipe = array::<StringArray>(&batch, "projection_recipe")?;
        if role.is_null(row)
            || recipe.is_null(row)
            || role.value(row) != "CodeDocument"
            || recipe.value(row) != NATIVE_ENTITY_PROJECTION_RECIPE
        {
            return Err(invalid());
        }
    }
    let arrays = schema
        .fields()
        .iter()
        .map(|field| {
            batch
                .column_by_name(if field.name() == "name" {
                    "projection_name"
                } else {
                    field.name()
                })
                .cloned()
                .ok_or_else(invalid)
        })
        .collect::<io::Result<Vec<_>>>()?;
    let result = RecordBatch::try_new(schema, arrays).map_err(io::Error::other)?;
    crate::native_nn::validate_native_nn(&result, model, original).map_err(io::Error::other)?;
    Ok(result)
}

/// Patch only extant entities whose complete current fingerprint still matches
/// the inferred observation. The caller commits against this exact corpus pin;
/// strict CAS rejects intervening publication without resurrecting missing rows.
pub fn attach_embeddings(
    current: &RecordBatch,
    inferred: &RecordBatch,
    model: &NativeNnModel<'_>,
    original: &SnapshotContext,
) -> io::Result<RecordBatch> {
    original.check()?;
    validate_schema(current.schema().as_ref())?;
    crate::native_nn::validate_native_nn(inferred, model, original).map_err(io::Error::other)?;
    if current.schema().field_with_name(VECTOR).ok() != Some(vector_fields(model)?.field(0)) {
        return Err(invalid());
    }
    let current_keys = identities(current)?;
    let uid = array::<StringArray>(inferred, "uid")?;
    let key = array::<StringArray>(inferred, "data_key")?;
    let fingerprints = array::<FixedSizeBinaryArray>(current, "input_fingerprint")?;
    let mut selected = Vec::new();
    let mut patches = Vec::new();
    for row in 0..inferred.num_rows() {
        original.check()?;
        let Some(index) = current_keys
            .get(&(uid.value(row).into(), key.value(row).into()))
            .copied()
        else {
            continue;
        };
        if !fingerprints.is_null(index as usize)
            && fingerprints.value(index as usize) != fingerprint(current, index as usize)?
        {
            return Err(invalid());
        }
        if fingerprints.is_null(index as usize)
            || fingerprints.value(index as usize) != fingerprint(inferred, row)?
        {
            continue;
        }
        selected.push(Some(index));
        patches.push(Some(u32::try_from(row).map_err(|_| invalid())?));
    }
    let mut ids = FixedSizeBinaryBuilder::with_capacity(selected.len(), 32);
    for _ in &selected {
        ids.append_value(model.vector_id)
            .map_err(io::Error::other)?;
    }
    let ids: ArrayRef = Arc::new(ids.finish());
    let arrays = current
        .schema()
        .fields()
        .iter()
        .map(|field| match field.name().as_str() {
            "vector_id" => Ok(ids.clone()),
            "model_generation" | VECTOR => take(
                inferred.column_by_name(field.name()).ok_or_else(invalid)?,
                patches.clone(),
            ),
            _ => take(
                current.column_by_name(field.name()).ok_or_else(invalid)?,
                selected.clone(),
            ),
        })
        .collect::<io::Result<Vec<_>>>()?;
    original.check()?;
    RecordBatch::try_new(current.schema(), arrays).map_err(io::Error::other)
}
