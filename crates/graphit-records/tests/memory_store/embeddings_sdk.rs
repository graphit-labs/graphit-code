use super::fixture::Fixture;
use arrow_array::{
    Array, BooleanArray, FixedSizeBinaryArray, Float32Array, ListArray, RecordBatch, StringArray,
    UInt32Array, UInt64Array,
    builder::{FixedSizeBinaryBuilder, Float32Builder, ListBuilder},
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::lance_branch_store::{BranchSnapshot, LanceBranchStore};
use std::{collections::HashMap, sync::Arc};

pub fn schema() -> Arc<Schema> {
    let mut fields = legacy_schema()
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect::<Vec<_>>();
    fields.extend([
        Field::new("embedding_vector_id", DataType::FixedSizeBinary(32), true),
        Field::new("embedding_fingerprint", DataType::FixedSizeBinary(32), true),
        Field::new("embedding_recipe", DataType::Utf8, true),
        Field::new("embedding_role", DataType::Utf8, true),
        Field::new("embedding_dimensions", DataType::UInt32, true),
        Field::new(
            "embedding_vector",
            DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
            true,
        ),
    ]);
    Arc::new(Schema::new_with_metadata(
        fields,
        HashMap::from([(
            "graphit.memory.layout".into(),
            "project-v2-embedding-list".into(),
        )]),
    ))
}

/// Only used to construct an incompatible historical-schema fixture.
pub fn legacy_schema() -> Arc<Schema> {
    let names = [
        "row_key",
        "kind",
        "record_id",
        "project_id",
        "operation_version",
        "idempotency_key",
        "idempotency_actor",
        "title",
        "revision",
        "created_at",
        "updated_at",
        "created_by",
        "updated_by",
        "superseded",
        "tombstone",
        "reference_generation",
        "references",
        "payload",
    ];
    Arc::new(Schema::new_with_metadata(
        names
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                Field::new(
                    name,
                    match i {
                        8..=10 | 15 => DataType::UInt64,
                        13 | 14 => DataType::Boolean,
                        _ => DataType::Utf8,
                    },
                    false,
                )
            })
            .collect::<Vec<_>>(),
        HashMap::from([("graphit.memory.layout".into(), "project-v1".into())]),
    ))
}
pub async fn pin(f: &Fixture) -> BranchSnapshot {
    let path = f.project.runtime_root().join("memory/index.lance");
    let store = LanceBranchStore::open(path.to_str().unwrap(), schema(), None)
        .await
        .unwrap();
    let branch = f.version.lance_branch();
    store
        .capture_target((!branch.is_empty()).then_some(branch.as_str()))
        .await
        .unwrap()
}
pub async fn all(pin: &BranchSnapshot) -> RecordBatch {
    let mut scan = pin.scan();
    scan.batch_size(65536);
    let mut stream = scan.try_into_stream().await.unwrap();
    let mut batches = vec![];
    while let Some(b) = stream.try_next().await.unwrap() {
        batches.push(b);
    }
    if batches.is_empty() {
        return RecordBatch::new_empty(schema());
    }
    // The fixtures keep a small corpus; combine through typed columns without arrow-select.
    let mut columns: Vec<Arc<dyn Array>> = vec![];
    for c in 0..schema().fields().len() {
        let array: Arc<dyn Array> = match schema().field(c).data_type() {
            DataType::Utf8 => Arc::new(StringArray::from(
                batches
                    .iter()
                    .flat_map(|b| {
                        b.column(c)
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap()
                            .iter()
                    })
                    .collect::<Vec<_>>(),
            )),
            DataType::UInt64 => Arc::new(UInt64Array::from(
                batches
                    .iter()
                    .flat_map(|b| {
                        b.column(c)
                            .as_any()
                            .downcast_ref::<UInt64Array>()
                            .unwrap()
                            .iter()
                    })
                    .collect::<Vec<_>>(),
            )),
            DataType::Boolean => Arc::new(BooleanArray::from(
                batches
                    .iter()
                    .flat_map(|b| {
                        b.column(c)
                            .as_any()
                            .downcast_ref::<BooleanArray>()
                            .unwrap()
                            .iter()
                    })
                    .collect::<Vec<_>>(),
            )),
            DataType::UInt32 => Arc::new(UInt32Array::from(
                batches
                    .iter()
                    .flat_map(|batch| {
                        batch
                            .column(c)
                            .as_any()
                            .downcast_ref::<UInt32Array>()
                            .unwrap()
                            .iter()
                    })
                    .collect::<Vec<_>>(),
            )),
            DataType::FixedSizeBinary(width) => {
                let mut builder = FixedSizeBinaryBuilder::new(*width);
                for batch in &batches {
                    for value in batch
                        .column(c)
                        .as_any()
                        .downcast_ref::<FixedSizeBinaryArray>()
                        .unwrap()
                        .iter()
                    {
                        match value {
                            Some(value) => builder.append_value(value).unwrap(),
                            None => builder.append_null(),
                        }
                    }
                }
                Arc::new(builder.finish())
            }
            DataType::List(_) => {
                let mut builder = ListBuilder::new(Float32Builder::new());
                for batch in &batches {
                    let list = batch
                        .column(c)
                        .as_any()
                        .downcast_ref::<ListArray>()
                        .unwrap();
                    for value in list.iter() {
                        if let Some(value) = value {
                            for item in value
                                .as_any()
                                .downcast_ref::<Float32Array>()
                                .unwrap()
                                .iter()
                            {
                                builder.values().append_option(item);
                            }
                            builder.append(true);
                        } else {
                            builder.append(false);
                        }
                    }
                }
                Arc::new(builder.finish())
            }
            _ => unreachable!(),
        };
        columns.push(array);
    }
    RecordBatch::try_new(schema(), columns).unwrap()
}
pub fn string(batch: &RecordBatch, col: usize, row: usize) -> &str {
    batch
        .column(col)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .value(row)
}
pub fn text_column(batch: &RecordBatch, col: usize, values: Vec<String>) -> RecordBatch {
    let mut columns = batch.columns().to_vec();
    columns[col] = Arc::new(StringArray::from(values));
    RecordBatch::try_new(batch.schema(), columns).unwrap()
}
pub async fn replace(pin: &BranchSnapshot, batch: RecordBatch) {
    pin.prepare_merge(vec![batch], vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}
pub async fn directory_only(
    f: &Fixture,
    ctx: &graphit_hub::policy::RequestContext<'_>,
) -> graphit_core::artifact_store::ArtifactWriter {
    use graphit_core::artifact_store::{ArtifactStore, ProjectRecordDirectory};
    let limits = super::fixture::write_limits();
    let store = ArtifactStore::new(f.project.clone(), limits.artifacts);
    let original = Arc::clone(ctx.snapshot());
    let mut writer = tokio::task::spawn_blocking(move || store.writer_with_context(&original))
        .await
        .unwrap()
        .unwrap();
    writer
        .create_record_directory(ProjectRecordDirectory::Memory, limits.read.native)
        .unwrap();
    writer
}
pub async fn empty_with_schema(
    f: &Fixture,
    ctx: &graphit_hub::policy::RequestContext<'_>,
    schema: Arc<Schema>,
) {
    let _writer = directory_only(f, ctx).await;
    let path = f.project.runtime_root().join("memory/index.lance");
    graphit_data::lance_module_store::LanceModuleStore::create(
        path.to_str().unwrap(),
        Arc::clone(&schema),
        vec![RecordBatch::new_empty(schema)],
    )
    .await
    .unwrap();
}
