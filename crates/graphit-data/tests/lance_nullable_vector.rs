use arrow_array::{Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{
    lance_branch_store::{BranchSnapshot, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn Error>>;
fn original(cancellation: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(90),
        cancellation,
    ))
}

#[tokio::test]
async fn nullable_vector_evolves_without_rewriting_text_and_preserves_branch_and_cas() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("corpus.lance");
    let uri = uri.to_str().unwrap();
    let schema = Arc::new(Schema::new(vec![Field::new("text", DataType::Utf8, false)]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(StringArray::from(vec!["first", "second"]))],
    )?;
    LanceModuleStore::create(uri, schema.clone(), vec![batch]).await?;
    let store = LanceBranchStore::open(uri, schema.clone(), None).await?;
    let base = store.capture_target(None).await?;
    let named = store
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await?;
    let before = lance::Dataset::open(uri)
        .await?
        .checkout_branch("feature")
        .await?;
    let fragments = before.manifest().fragments.clone();
    let original = original(Cancellation::default());
    let evolved = named
        .add_nullable_columns(vector_fields("vector", 3), &original)
        .await?;
    assert_eq!(evolved.physical_branch(), Some("feature"));
    assert_eq!(evolved.version(), named.version() + 1);
    let after = lance::Dataset::open(uri)
        .await?
        .checkout_branch("feature")
        .await?;
    assert_eq!(after.manifest().fragments, fragments);
    assert!(base.schema().field_with_name("vector").is_err());
    let batches: Vec<RecordBatch> = evolved
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
    assert!(
        batches
            .iter()
            .all(|b| b.column_by_name("vector").unwrap().null_count() == b.num_rows())
    );
    assert_eq!(
        batches[0]
            .column_by_name("text")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(0),
        "first"
    );
    assert_eq!(
        evolved
            .add_nullable_columns(vector_fields("vector", 3), &original)
            .await?
            .version(),
        evolved.version()
    );
    assert!(
        named
            .add_nullable_columns(vector_fields("other_vector", 2), &original)
            .await
            .is_err()
    );
    assert!(
        evolved
            .add_nullable_columns(vector_fields("vector", 7), &original)
            .await
            .is_err()
    );
    let dropped = evolved.drop_vector("vector", &original).await?;
    let changed = dropped
        .add_nullable_columns(vector_fields("vector", 7), &original)
        .await?;
    assert_eq!(
        changed.schema().field_with_name("vector")?.data_type(),
        &DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 7)
    );
    let current = BranchSnapshot::capture_observed(uri, Some("feature"), None).await?;
    assert_eq!(current.version(), changed.version());
    assert_eq!(store.capture_target(None).await?.version(), base.version());
    let cancellation = Cancellation::default();
    let cancelled = original_context(&cancellation);
    cancellation.cancel();
    assert!(changed.drop_vector("vector", &cancelled).await.is_err());
    assert_eq!(
        BranchSnapshot::capture_observed(uri, Some("feature"), None)
            .await?
            .version(),
        changed.version()
    );
    Ok(())
}

fn original_context(cancellation: &Cancellation) -> Arc<SnapshotContext> {
    original(cancellation.clone())
}

fn vector_fields(name: &str, dimensions: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new(
            name,
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensions,
            ),
            true,
        ),
        Field::new("input_fingerprint", DataType::FixedSizeBinary(32), true),
        Field::new("vector_id", DataType::FixedSizeBinary(32), true),
        Field::new("projection_recipe", DataType::Utf8, true),
        Field::new("role", DataType::Utf8, true),
    ]))
}

#[tokio::test]
async fn real_vector_index_is_retained_and_searches_later_unindexed_rows() -> TestResult {
    use arrow_array::{FixedSizeListArray, Float32Array, types::Float32Type};
    use lance::index::{DatasetIndexExt, vector::VectorIndexParams};
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("indexed.lance");
    let uri = uri.to_str().unwrap();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        vector_fields("vector", 2).field(0).clone(),
    ]));
    let rows = |ids: Vec<&str>, vectors: Vec<Option<Vec<Option<f32>>>>| {
        RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(ids)),
                Arc::new(FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(vectors, 2)),
            ],
        )
    };
    let initial = rows(
        vec!["first", "second", "lexical_only"],
        vec![
            Some(vec![Some(1.), Some(0.)]),
            Some(vec![Some(0.), Some(1.)]),
            None,
        ],
    )?;
    LanceModuleStore::create(uri, schema.clone(), vec![initial]).await?;
    let store = LanceBranchStore::open(uri, schema.clone(), None).await?;
    let base = store.capture_target(None).await?;
    let original = original(Cancellation::default());
    let params = VectorIndexParams::ivf_flat(1, "l2".try_into()?);
    let prepared = base
        .prepare_vector_index("vector", "entity_vector", &params, &original)
        .await?
        .expect("first index");
    let indexed = prepared.commit().await?;
    let native = lance::Dataset::open(uri).await?;
    assert!(
        native
            .load_indices()
            .await?
            .iter()
            .any(|index| index.name == "entity_vector")
    );
    assert!(
        indexed
            .prepare_vector_index("vector", "entity_vector", &params, &original)
            .await?
            .is_none()
    );
    let later = rows(vec!["later"], vec![Some(vec![Some(0.01), Some(0.01)])])?;
    let current = indexed
        .prepare_merge(vec![later], vec!["id".into()])
        .await?
        .commit()
        .await?;
    let mut scan = current.scan();
    scan.prefilter(true);
    scan.nearest("vector", &Float32Array::from(vec![0., 0.]), 1)?;
    let batches: Vec<RecordBatch> = scan.try_into_stream().await?.try_collect().await?;
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
    assert_eq!(
        batches[0]
            .column_by_name("id")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(0),
        "later"
    );
    assert!(
        current
            .prepare_vector_index("vector", "entity_vector", &params, &original)
            .await?
            .is_none()
    );
    Ok(())
}
