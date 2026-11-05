//! Finite parser fixtures through the same corpus encoding and conditional update APIs.
use arrow_array::RecordBatch;
use graphit_ast::{
    corpus,
    native_nn::{NativeNnModel, native_nn_projection_schema, validate_native_nn},
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use std::{error::Error, path::Path, sync::Arc};
pub async fn persist(
    uri: &Path,
    lexical: RecordBatch,
    inferred: &RecordBatch,
    model: &NativeNnModel<'_>,
    branch: Option<&str>,
    original: &Arc<SnapshotContext>,
) -> Result<BranchSnapshot, Box<dyn Error>> {
    validate_native_nn(inferred, model, original)?;
    let base = corpus::schema();
    LanceModuleStore::create(
        uri.to_str().ok_or("fixture path")?,
        base.clone(),
        vec![RecordBatch::new_empty(base.clone())],
    )
    .await?;
    let pin = LanceBranchStore::open(uri.to_str().ok_or("fixture path")?, base, None)
        .await?
        .capture_target(branch)
        .await?;
    let pin = if pin.inherited_target().is_some() {
        pin.materialize().await?
    } else {
        pin
    };
    let pin = pin
        .add_nullable_columns(corpus::vector_fields(model)?, original)
        .await?;
    let schema = native_nn_projection_schema();
    let prepared = RecordBatch::try_new(
        schema.clone(),
        schema
            .fields()
            .iter()
            .map(|field| {
                inferred
                    .column_by_name(field.name())
                    .cloned()
                    .ok_or("fixture projection column")
            })
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let rows = corpus::encode_file(
        Arc::new(pin.schema().clone()),
        lexical,
        prepared,
        None,
        original,
    )?;
    let pin = if rows.num_rows() == 0 {
        pin
    } else {
        pin.prepare_merge(vec![rows], vec!["uid".into(), "data_key".into()])
            .await?
            .commit()
            .await?
    };
    match corpus::prepare_embeddings(&pin, inferred, model, original).await? {
        Some(update) => Ok(update.commit().await?),
        None => Ok(pin),
    }
}
