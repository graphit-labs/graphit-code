use super::rust_semantic;
use arrow_array::RecordBatch;
use arrow_schema::Schema;
use futures::TryStreamExt;
use graphit_ast::{corpus, native_nn::NativeNnModel};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::BranchSnapshot, lance_module_store::SendableRecordBatchStream,
};
use std::{error::Error, sync::Arc};
pub fn schema(model: &NativeNnModel<'_>) -> Result<Arc<Schema>, Box<dyn Error>> {
    let base = corpus::schema();
    let mut fields = base.fields().to_vec();
    fields.extend(corpus::vector_fields(model)?.fields().iter().cloned());
    Ok(Arc::new(Schema::new_with_metadata(
        fields,
        base.metadata().clone(),
    )))
}
pub fn encode(
    raw: &graphit_ast::indexed_source::IndexedSourceSnapshot,
    components: &[&str],
    model: &NativeNnModel<'_>,
    generation: u64,
    original: &Arc<SnapshotContext>,
) -> Result<RecordBatch, Box<dyn Error>> {
    let graphit_ast::source_inspection::IndexedSource::Available(text) = raw.source(original)?
    else {
        return Err("fixture Source missing".into());
    };
    super::rust_semantic::rust_document!(document, components, text, original);
    let graph = graphit_ast::semantic_graph::assemble_graph_ts(
        graphit_ast::semantic_graph::AstGraphTsInput {
            document: &document,
            source: graphit_ast::native_nn::NativeNnSource::Indexed(raw),
        },
        original,
    )?
    .into_parts();
    let lexical = graphit_ast::entity_search::encode_file(&graph, text, original)?;
    let inferred = super::rust_semantic::encode(raw, components, model, generation, original)?;
    let prepared_schema = graphit_ast::native_nn::native_nn_projection_schema();
    let prepared = RecordBatch::try_new(
        prepared_schema.clone(),
        prepared_schema
            .fields()
            .iter()
            .map(|f| inferred.column_by_name(f.name()).unwrap().clone())
            .collect(),
    )?;
    let batch = corpus::encode_file(schema(model)?, lexical, prepared, None, original)?;
    // attach returns only inferred entities; lexical-only rows stay in the
    // fixture corpus through an identity replacement below.
    let enriched = corpus::attach_embeddings(&batch, &inferred, model, original)?;
    let identity = |batch: &RecordBatch, row: usize| {
        let value = |name| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<arrow_array::StringArray>()
                .unwrap()
                .value(row)
                .to_owned()
        };
        (value("uid"), value("data_key"))
    };
    let inferred_ids = (0..enriched.num_rows())
        .map(|row| identity(&enriched, row))
        .collect::<std::collections::HashSet<_>>();
    let mut rows = vec![enriched];
    rows.extend(
        (0..batch.num_rows())
            .filter(|row| !inferred_ids.contains(&identity(&batch, *row)))
            .map(|row| batch.slice(row, 1)),
    );
    Ok(arrow_select::concat::concat_batches(
        &batch.schema(),
        &rows,
    )?)
}
pub async fn publish(
    pin: &BranchSnapshot,
    mut stream: SendableRecordBatchStream,
    model: &NativeNnModel<'_>,
    original: &Arc<SnapshotContext>,
) -> Result<BranchSnapshot, Box<dyn Error>> {
    let mut batches = Vec::new();
    while let Some(batch) = stream.try_next().await? {
        corpus::validate_schema(batch.schema().as_ref())?;
        batches.push(batch);
    }
    let wanted = corpus::vector_fields(model)?;
    let mut pin = pin.clone();
    if pin.schema().field_with_name("vector").ok() != Some(wanted.field(0)) {
        pin = pin.drop_vector("vector", original).await?;
        pin = pin.add_nullable_columns(wanted, original).await?;
    }
    let stream = Box::pin(
        graphit_data::lance_module_store::RecordBatchStreamAdapter::new(
            Arc::new(pin.schema().clone()),
            futures::stream::iter(batches.into_iter().map(Ok)),
        ),
    );
    Ok(pin
        .prepare_merge_stream(
            stream,
            vec!["uid".into(), "data_key".into()],
            Some(graphit_data::lance_branch_store::lit(true)),
        )
        .await?
        .commit()
        .await?)
}
