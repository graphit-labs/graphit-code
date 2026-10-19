//! One Lance corpus for lexical entities, parser projections and nullable vectors.
//! Raw FILE Source and the structural graph retain their separate formats.
use arrow_array::{Array, ArrayRef, FixedSizeBinaryArray, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use graphit_core::file_io::SnapshotContext;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, sync::Arc};

mod batch;
mod publication;
pub use batch::{attach_embeddings, encode_file, projections, vectors};
pub use publication::prepare_embeddings;

pub const DIRECTORY: &str = "corpus.lance";
pub const FORMAT: &str = "ast-corpus-1";
pub const VECTOR: &str = "vector";

/// The fixed AST module root for graph cohorts, derived from captured scopes.
/// Dataset locations use Core local_corpus::resolve instead; this grants no authority.
pub fn project_directory(
    installation: graphit_auth::ScopeCapture<'_>,
    project: graphit_auth::ProjectCapture<'_>,
) -> io::Result<std::path::PathBuf> {
    if !matches!(
        installation.live.owner(),
        graphit_core::artifact_store::StoreOwner::Installation { .. }
    ) || installation.live.brand() != project.scope.live.brand()
        || project.version.scope() != project.scope.live.owner_root()
    {
        return Err(invalid());
    }
    let graphit_core::artifact_store::StoreOwner::Project { project_id } =
        project.scope.live.owner()
    else {
        return Err(invalid());
    };
    Ok(installation
        .live
        .owner_root()
        .join("projects")
        .join(project_id.to_string())
        .join("ast"))
}

/// Model-independent rows are immediately searchable. The SDK adds the nullable
/// fixed-width vector field only when the real model dimensions are known.
pub fn schema() -> Arc<Schema> {
    let lexical = crate::entity_search::schema();
    let prepared = crate::native_nn::native_nn_projection_schema();
    let mut fields = lexical.fields().to_vec();
    for field in prepared.fields() {
        if lexical.field_with_name(field.name()).is_err() {
            fields.push(Arc::new(field.as_ref().clone().with_nullable(true)));
        }
    }
    fields.push(Arc::new(Field::new(
        "projection_name",
        DataType::Utf8,
        true,
    )));
    fields.extend([
        Arc::new(Field::new(
            "input_fingerprint",
            DataType::FixedSizeBinary(32),
            true,
        )),
        Arc::new(Field::new("vector_id", DataType::FixedSizeBinary(32), true)),
        Arc::new(Field::new("model_generation", DataType::UInt64, true)),
        Arc::new(Field::new("projection_recipe", DataType::Utf8, true)),
        Arc::new(Field::new("role", DataType::Utf8, true)),
    ]);
    let mut metadata = lexical.metadata().clone();
    metadata.insert("graphit.ast.corpus.format".into(), FORMAT.into());
    Arc::new(Schema::new_with_metadata(fields, metadata))
}

pub fn validate_schema(observed: &Schema) -> io::Result<()> {
    let base = schema();
    // Validate the columns actually consumed, by name. Native historical
    // snapshots may retain other columns; they do not participate in reads.
    if observed.metadata() != base.metadata()
        || base
            .fields()
            .iter()
            .any(|field| observed.field_with_name(field.name()).ok() != Some(field.as_ref()))
    {
        return Err(invalid());
    }
    if let Ok(field) = observed.field_with_name(VECTOR)
        && (!field.is_nullable()
            || !matches!(field.data_type(), DataType::FixedSizeList(item, width)
                if *width > 0 && item.as_ref() == &Field::new("item", DataType::Float32, true)))
    {
        return Err(invalid());
    }
    model(observed)?;
    Ok(())
}

pub fn vector_fields(model: &crate::native_nn::NativeNnModel<'_>) -> io::Result<Arc<Schema>> {
    let width = i32::try_from(model.dimensions).map_err(|_| invalid())?;
    if width <= 0 {
        return Err(invalid());
    }
    let metadata = crate::native_nn::native_nn_schema(model)
        .map_err(io::Error::other)?
        .metadata()
        .clone();
    Ok(Arc::new(Schema::new(vec![
        Field::new(
            VECTOR,
            DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), width),
            true,
        )
        .with_metadata(metadata),
    ])))
}

/// The actual model descriptor retained on the nullable vector field.
pub fn model(schema: &Schema) -> io::Result<Option<crate::native_nn::NativeNnModel<'_>>> {
    let Ok(field) = schema.field_with_name(VECTOR) else {
        return Ok(None);
    };
    let metadata = field.metadata();
    let get = |key| metadata.get(key).map(String::as_str).ok_or_else(invalid);
    let encoded = get("graphit.ast.nn.vector_id")?;
    if encoded.len() != 64 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let mut vector_id = [0; 32];
    for (index, byte) in vector_id.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16).map_err(|_| invalid())?;
    }
    let model = crate::native_nn::NativeNnModel {
        vector_id,
        dimensions: get("graphit.ast.nn.dimensions")?
            .parse()
            .map_err(|_| invalid())?,
        model_id: get("graphit.ast.nn.model_id")?,
        revision: get("graphit.ast.nn.revision")?,
    };
    if vector_fields(&model)?.field(0) != field {
        return Err(invalid());
    }
    Ok(Some(model))
}

pub fn vector_dimensions(schema: &Schema) -> Option<u64> {
    match schema.field_with_name(VECTOR).ok()?.data_type() {
        DataType::FixedSizeList(_, width) => u64::try_from(*width).ok(),
        _ => None,
    }
}

/// Candidate eligibility for native NN and bounded coverage probes.
pub fn vector_filter(
    model: &crate::native_nn::NativeNnModel<'_>,
) -> graphit_data::lance_branch_store::Expr {
    use graphit_data::lance_branch_store::{col, lit};
    col(VECTOR).is_not_null().and(col("vector_id").eq(lit(
        lance::deps::datafusion::common::ScalarValue::FixedSizeBinary(
            32,
            Some(model.vector_id.to_vec()),
        ),
    )))
}

fn array<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> io::Result<&'a T> {
    batch
        .column_by_name(name)
        .and_then(|a| a.as_any().downcast_ref())
        .ok_or_else(invalid)
}

fn identities(batch: &RecordBatch) -> io::Result<HashMap<(String, String), u32>> {
    let uid = array::<StringArray>(batch, "uid")?;
    let key = array::<StringArray>(batch, "data_key")?;
    let mut rows = HashMap::new();
    for row in 0..batch.num_rows() {
        if uid.is_null(row)
            || key.is_null(row)
            || rows
                .insert(
                    (uid.value(row).to_owned(), key.value(row).to_owned()),
                    u32::try_from(row).map_err(|_| invalid())?,
                )
                .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(rows)
}

fn fingerprint(batch: &RecordBatch, row: usize) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    for name in ["uid", "data_key"] {
        let values = array::<StringArray>(batch, name)?;
        if values.is_null(row) {
            return Err(invalid());
        }
        hash.update((values.value(row).len() as u64).to_le_bytes());
        hash.update(values.value(row).as_bytes());
    }
    for name in ["projection_sha256", "full_source_sha256"] {
        let values = array::<FixedSizeBinaryArray>(batch, name)?;
        if values.is_null(row) {
            return Err(invalid());
        }
        hash.update(values.value(row));
    }
    let raw = array::<UInt64Array>(batch, "raw_version")?;
    if raw.is_null(row) {
        return Err(invalid());
    }
    hash.update(raw.value(row).to_le_bytes());
    hash.update(crate::native_nn::NATIVE_ENTITY_PROJECTION_RECIPE);
    Ok(hash.finalize().into())
}

fn take(array: &ArrayRef, indices: Vec<Option<u32>>) -> io::Result<ArrayRef> {
    arrow_select::take::take(
        array.as_ref(),
        &arrow_array::UInt32Array::from(indices),
        None,
    )
    .map_err(io::Error::other)
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "incoherent AST corpus entity or embedding",
    )
}
