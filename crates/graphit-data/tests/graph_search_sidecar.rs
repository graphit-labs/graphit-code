use arrow_array::{ArrayRef, Float32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{
    graph_search_sidecar::{DIRECTORY, FullTextSearchQuery, GraphSearchSpool},
    graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter},
    lance_module_store::LanceModuleStore,
};
use std::{
    collections::BTreeMap,
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn Error>>;
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("name", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, true),
    ]))
}
fn node() -> GraphNode {
    GraphNode {
        label: "Function".into(),
        key: "observed primary key".into(),
        properties: BTreeMap::from([("name".into(), GraphValue::String("Function".into()))]),
    }
}
#[tokio::test]
async fn code_bm25_sidecar_publishes_nullable_body_and_retains_pin() -> TestResult {
    let original = original();
    let output = IcebugWriter::new(Arc::clone(&original))?
        .push_file([node()], std::iter::empty::<GraphEdge>())?
        .finish()?;
    let mut spool = GraphSearchSpool::new(schema(), Arc::clone(&original))?;
    let batch = RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(StringArray::from(vec![
                "searchable_without_vector",
                "body_owner",
            ])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                None,
                Some("fn code_only_keyword() {}"),
            ])) as ArrayRef,
        ],
    )?;
    spool.push(&batch)?;
    let pin = spool.finish(&output, &["name", "content"]).await?;
    assert_eq!(pin.version(), 3, "Main and both real FTS manifests");
    let opened = LanceModuleStore::open(
        output.directory().join(DIRECTORY).to_str().unwrap(),
        schema(),
    )
    .await?
    .snapshot();
    assert_eq!(opened.version(), pin.version());
    for (query, expected) in [
        ("searchable_without_vector", "searchable_without_vector"),
        ("code_only_keyword", "body_owner"),
    ] {
        let mut scanner = opened.scan();
        scanner.project(&["name", "_score"])?;
        scanner.full_text_search(
            FullTextSearchQuery::new(query.into())
                .with_columns(&["name".into(), "content".into()])?
                .limit(Some(1)),
        )?;
        let mut stream = scanner.try_into_stream().await?;
        let mut names = Vec::new();
        while let Some(batch) = stream.try_next().await? {
            let strings = batch
                .column_by_name("name")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let scores = batch
                .column_by_name("_score")
                .unwrap()
                .as_any()
                .downcast_ref::<Float32Array>()
                .unwrap();
            for i in 0..batch.num_rows() {
                assert!(scores.value(i).is_finite());
                names.push(strings.value(i).to_owned());
            }
        }
        assert_eq!(names, [expected]);
    }
    Ok(())
}
#[tokio::test]
async fn sidecar_rejects_other_original_and_keeps_explicit_empty_dataset() -> TestResult {
    let original = original();
    let output = IcebugWriter::new(Arc::clone(&original))?
        .push_file([node()], std::iter::empty::<GraphEdge>())?
        .finish()?;
    let other = GraphSearchSpool::new(schema(), self::original())?;
    assert_eq!(
        other.finish(&output, &["name"]).await.err().unwrap().kind(),
        std::io::ErrorKind::InvalidInput
    );
    let spool = GraphSearchSpool::new(schema(), Arc::clone(&original))?;
    let snapshot = spool.finish(&output, &["name", "content"]).await?;
    assert_eq!(snapshot.version(), 1);
    let batches: Vec<RecordBatch> = snapshot
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 0);
    Ok(())
}
