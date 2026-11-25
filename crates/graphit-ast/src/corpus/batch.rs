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
    let mut previous_occurrences = None;
    let mut previous_inputs = None;
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
        let mut prior = previous_keys
            .get(&(uid.value(row).into(), key.value(row).into()))
            .copied();
        // Identity may be refined without changing the model input. Reuse one
        // unambiguous observation already loaded for this FILE, not an alias or
        // a second corpus. Exact identities keep the normal fast path.
        if prior.is_none()
            && let Some(old) = previous
        {
            if previous_occurrences.is_none() {
                previous_occurrences = Some(occurrence_rows(old, original)?);
            }
            if let Some(rows) = previous_occurrences.as_ref()
                && !rows.is_empty()
            {
                let occurrence = super::publication::source_occurrence(&prepared, projection)?;
                prior = rows.get(&occurrence).copied().flatten();
            }
            if let Some(index) = prior
                && !super::publication::same_source_occurrence(
                    old,
                    index as usize,
                    &prepared,
                    projection,
                )?
            {
                prior = None;
            }
            // A Source edit can move an otherwise identical model input while
            // also refining its identity. Reuse the model output, not the old
            // observation: the fingerprint and provenance above remain current.
            // Keep publication's occurrence guard strict for in-flight inference.
            if prior.is_none() {
                if previous_inputs.is_none() {
                    previous_inputs = Some(input_rows(old, original)?);
                }
                let path = array::<StringArray>(&prepared, "path")?.value(projection);
                let text = array::<StringArray>(&prepared, "projection")?.value(projection);
                prior = previous_inputs
                    .as_ref()
                    .and_then(|rows| rows.get(&(path, text)))
                    .copied()
                    .flatten();
                if let Some(index) = prior {
                    let old_schema = old.schema();
                    let model = super::model(old_schema.as_ref())?.ok_or_else(invalid)?;
                    // Certify the selected old output against its own truthful
                    // input and model before binding it to the new observation.
                    if vectors(&old.slice(index as usize, 1), &model, original)?.num_rows() != 1 {
                        return Err(invalid());
                    }
                }
            }
        }
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
                let recipes = array::<StringArray>(old, "projection_recipe")?;
                let roles = array::<StringArray>(old, "role")?;
                if fingerprints.is_null(index)
                    || fingerprints.value(index) != fingerprint(old, index)?
                    || ids.is_null(index)
                    || ids.value(index) != previous_vector_id.ok_or_else(invalid)?
                    || recipes.is_null(index)
                    || recipes.value(index) != NATIVE_ENTITY_PROJECTION_RECIPE
                    || roles.is_null(index)
                    || roles.value(index) != "CodeDocument"
                {
                    return Err(invalid());
                }
                !hashes.is_null(index)
                    && hashes.value(index)
                        == array::<FixedSizeBinaryArray>(&prepared, "projection_sha256")?
                            .value(projection)
                    && !array::<StringArray>(old, "projection")?.is_null(index)
                    && array::<StringArray>(old, "projection")?.value(index)
                        == array::<StringArray>(&prepared, "projection")?.value(projection)
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

fn occurrence_rows(
    previous: &RecordBatch,
    original: &SnapshotContext,
) -> io::Result<HashMap<[u8; 32], Option<u32>>> {
    let mut rows = HashMap::new();
    let Some(vectors) = previous.column_by_name(VECTOR) else {
        return Ok(rows);
    };
    for row in 0..previous.num_rows() {
        original.check()?;
        if vectors.is_null(row) {
            continue;
        }
        let key = super::publication::source_occurrence(previous, row)?;
        let row = u32::try_from(row).map_err(|_| invalid())?;
        rows.entry(key)
            .and_modify(|value| *value = None)
            .or_insert(Some(row));
    }
    Ok(rows)
}

fn input_rows<'a>(
    previous: &'a RecordBatch,
    original: &SnapshotContext,
) -> io::Result<HashMap<(&'a str, &'a str), Option<u32>>> {
    let mut rows = HashMap::new();
    let Some(vectors) = previous.column_by_name(VECTOR) else {
        return Ok(rows);
    };
    let paths = array::<StringArray>(previous, "path")?;
    let projections = array::<StringArray>(previous, "projection")?;
    for row in 0..previous.num_rows() {
        original.check()?;
        if vectors.is_null(row) {
            continue;
        }
        if paths.is_null(row) || projections.is_null(row) {
            return Err(invalid());
        }
        // With the same recipe and CodeDocument role the model sees only this
        // text, with a fixed prefix. Never choose among duplicate old inputs.
        let key = (paths.value(row), projections.value(row));
        let row = u32::try_from(row).map_err(|_| invalid())?;
        rows.entry(key)
            .and_modify(|value| *value = None)
            .or_insert(Some(row));
    }
    Ok(rows)
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
