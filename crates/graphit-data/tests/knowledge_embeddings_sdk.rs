//! SDK storage qualification; this is not a Knowledge embedding service.
use arrow_array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator,
    StringArray, builder::FixedSizeBinaryBuilder,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::{
    Dataset,
    dataset::{
        CommitBuilder, WriteMode, WriteParams,
        transaction::{Operation, Transaction},
        write::InsertBuilder,
    },
};
use lance_linalg::distance::DistanceType;
use std::{error::Error, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
struct Corpus {
    _directory: tempfile::TempDir,
    uri: String,
    captured: Arc<Dataset>,
}
fn schema(width: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("slug", DataType::Utf8, false),
        Field::new("input_fingerprint", DataType::FixedSizeBinary(32), false),
        Field::new("vector_id", DataType::FixedSizeBinary(32), false),
        Field::new("projection_recipe", DataType::Utf8, false),
        Field::new("role", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, false)),
                width,
            ),
            false,
        ),
    ]))
}
// Lance 13 reconstructs primitive FixedSizeList children as item/nullable=true.
// This is an explicit expected SDK schema, not a normalization of observed rows.
fn sdk_schema(width: i32) -> Arc<Schema> {
    let input = schema(width);
    let mut fields = input.fields()[..5].to_vec();
    fields.push(Arc::new(Field::new(
        "vector",
        DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), width),
        false,
    )));
    Arc::new(Schema::new(fields))
}
fn rows(width: i32, values: &[(&str, &[f32])]) -> RecordBatch {
    let mut fingerprints = FixedSizeBinaryBuilder::with_capacity(values.len(), 32);
    let mut configs = FixedSizeBinaryBuilder::with_capacity(values.len(), 32);
    for (i, (_, vector)) in values.iter().enumerate() {
        assert_eq!(vector.len(), usize::try_from(width).unwrap());
        fingerprints
            .append_value([u8::try_from(i + 1).unwrap(); 32])
            .unwrap();
        configs.append_value([7; 32]).unwrap();
    }
    let vector = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, false)),
        width,
        Arc::new(Float32Array::from_iter_values(
            values.iter().flat_map(|(_, vector)| vector.iter().copied()),
        )),
        None,
    )
    .unwrap();
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from_iter_values(
            values.iter().map(|(slug, _)| *slug),
        )),
        Arc::new(fingerprints.finish()),
        Arc::new(configs.finish()),
        Arc::new(StringArray::from(vec!["sdk-fixture-v1"; values.len()])),
        Arc::new(StringArray::from(vec!["Document"; values.len()])),
        Arc::new(vector),
    ];
    RecordBatch::try_new(schema(width), columns).unwrap()
}
fn expected_sdk_rows(input: &RecordBatch) -> RecordBatch {
    let vector = input
        .column(5)
        .as_any()
        .downcast_ref::<FixedSizeListArray>()
        .unwrap();
    let mut columns = input.columns()[..5].to_vec();
    columns.push(Arc::new(
        FixedSizeListArray::try_new(
            Arc::new(Field::new("item", DataType::Float32, true)),
            vector.value_length(),
            Arc::clone(vector.values()),
            None,
        )
        .unwrap(),
    ));
    RecordBatch::try_new(sdk_schema(vector.value_length()), columns).unwrap()
}
async fn corpus(batch: RecordBatch) -> Result<Corpus> {
    let directory = tempfile::tempdir()?;
    let uri = directory
        .path()
        .join("semantic.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let batch_schema = batch.schema();
    let dataset = Dataset::write(
        RecordBatchIterator::new([Ok(batch)], batch_schema),
        &uri,
        None,
    )
    .await?;
    Ok(Corpus {
        _directory: directory,
        uri,
        captured: Arc::new(dataset),
    })
}
async fn image(dataset: &Dataset) -> Result<Vec<RecordBatch>> {
    let batches: Vec<RecordBatch> = dataset
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    for batch in &batches {
        for column in batch.columns() {
            assert_eq!(column.null_count(), 0);
        }
        let vector = batch
            .column(5)
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .unwrap();
        assert_eq!(vector.values().null_count(), 0);
        let DataType::FixedSizeList(child, _) = vector.data_type() else {
            panic!("SDK vector must retain its FixedSizeList type");
        };
        assert_eq!(child.name(), "item");
        assert!(child.is_nullable());
    }
    Ok(batches)
}
async fn hits(
    dataset: &Dataset,
    query: &[f32],
    metric: DistanceType,
) -> Result<Vec<(String, f32)>> {
    let mut scanner = dataset.scan();
    scanner.project(&["slug"])?;
    scanner.nearest("vector", &Float32Array::from(query.to_vec()), 3)?;
    scanner.distance_metric(metric).use_index(false);
    let mut stream = scanner.try_into_stream().await?;
    let mut out = Vec::new();
    while let Some(batch) = stream.try_next().await? {
        let slugs = batch
            .column_by_name("slug")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let distances = batch
            .column_by_name("_distance")
            .unwrap()
            .as_any()
            .downcast_ref::<Float32Array>()
            .unwrap();
        assert_eq!(distances.null_count(), 0);
        for i in 0..batch.num_rows() {
            assert!(distances.value(i).is_finite());
            out.push((slugs.value(i).to_owned(), distances.value(i)));
        }
    }
    Ok(out)
}
async fn prepare(
    captured: &Arc<Dataset>,
    mode: WriteMode,
    batch: RecordBatch,
) -> Result<Transaction> {
    let params = WriteParams {
        mode,
        ..Default::default()
    };
    Ok(InsertBuilder::new(Arc::clone(captured))
        .with_params(&params)
        .execute_uncommitted(vec![batch])
        .await?)
}
async fn publish(
    captured: &Arc<Dataset>,
    transaction: Transaction,
) -> std::result::Result<Dataset, lance::Error> {
    CommitBuilder::new(Arc::clone(captured))
        .with_max_retries(0)
        .with_skip_auto_cleanup(true)
        .execute(transaction)
        .await
}
async fn uuid(dataset: &Dataset, version: u64) -> Result<String> {
    Ok(dataset
        .read_version_transaction(version)
        .await?
        .transaction
        .unwrap()
        .uuid)
}

#[tokio::test]
async fn six_columns_roundtrip_and_flat_nn_metrics_keep_the_same_pin() -> Result<()> {
    let input = rows(2, &[("a", &[1., 0.]), ("b", &[2., 1.]), ("c", &[0., 2.])]);
    assert_eq!(input.schema(), schema(2));
    let expected = expected_sdk_rows(&input); // Expected Arrow buffers are shared.
    let f = corpus(input).await?;
    assert_eq!(image(&f.captured).await?, vec![expected]);
    let base = f.captured.version().version;
    let cases = [
        (DistanceType::L2, vec![("a", 0.), ("b", 2.), ("c", 5.)]),
        (
            DistanceType::Cosine,
            vec![("a", 0.), ("b", 1. - 2. / 5_f32.sqrt()), ("c", 1.)],
        ),
        (DistanceType::Dot, vec![("b", -1.), ("a", 0.), ("c", 1.)]),
    ];
    for (metric, expected) in cases {
        let actual = hits(&f.captured, &[1., 0.], metric).await?;
        assert_eq!(actual.len(), expected.len());
        for ((slug, distance), (expected_slug, expected_distance)) in actual.iter().zip(expected) {
            assert_eq!(slug, expected_slug);
            assert!(
                (distance - expected_distance).abs() < 1e-5,
                "{metric:?}: {distance}"
            );
        }
    }
    assert_eq!(Dataset::open(&f.uri).await?.version().version, base);
    Ok(())
}

#[tokio::test]
async fn invalid_k_empty_query_and_wrong_dimensions_are_sdk_errors_without_publication()
-> Result<()> {
    let f = corpus(rows(2, &[("a", &[1., 0.])])).await?;
    let base = f.captured.version().version;
    for (query, k) in [
        (vec![1., 0.], 0),
        (vec![], 1),
        (vec![1.], 1),
        (vec![1., 0., 0.], 1),
    ] {
        let mut scan = f.captured.scan();
        let Err(error) = scan.nearest("vector", &Float32Array::from(query), k) else {
            panic!("invalid nearest input must return its SDK error");
        };
        assert!(
            matches!(error, lance::Error::InvalidInput { .. }),
            "{error:?}"
        );
        assert!(!error.to_string().is_empty());
    }
    assert_eq!(Dataset::open(&f.uri).await?.version().version, base);
    Ok(())
}

#[tokio::test]
async fn uncommitted_overwrite_changes_dimension_only_after_strict_commit() -> Result<()> {
    let f = corpus(rows(2, &[("old", &[1., 0.])])).await?;
    let before = image(&f.captured).await?;
    let base = f.captured.version().version;
    let tx = prepare(
        &f.captured,
        WriteMode::Overwrite,
        rows(3, &[("new", &[1., 0., 0.])]),
    )
    .await?;
    assert_eq!(tx.read_version, base);
    let Operation::Overwrite {
        schema: replacement_schema,
        ..
    } = &tx.operation
    else {
        panic!("expected actual SDK Overwrite");
    };
    assert_eq!(
        Schema::from(replacement_schema),
        sdk_schema(3).as_ref().clone()
    );
    let attempt = tx.uuid.clone();
    assert_eq!(Dataset::open(&f.uri).await?.version().version, base);
    assert_eq!(image(&f.captured).await?, before);
    let next = publish(&f.captured, tx).await?;
    assert_eq!(next.version().version, base + 1);
    assert_eq!(uuid(&next, base + 1).await?, attempt);
    assert_eq!(Schema::from(next.schema()), sdk_schema(3).as_ref().clone());
    assert_eq!(
        image(&next).await?,
        vec![expected_sdk_rows(&rows(3, &[("new", &[1., 0., 0.])]))]
    );
    assert_eq!(
        hits(&next, &[1., 0., 0.], DistanceType::L2).await?,
        vec![("new".into(), 0.)]
    );
    assert_eq!(image(&f.captured).await?, before);
    assert_eq!(
        hits(&f.captured, &[1., 0.], DistanceType::L2).await?,
        vec![("old".into(), 0.)]
    );
    let mut scan = next.scan();
    assert!(matches!(
        scan.nearest("vector", &Float32Array::from(vec![1., 0.]), 1),
        Err(lance::Error::InvalidInput { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn competing_dimension_overwrites_keep_the_winner_and_its_transaction_uuid() -> Result<()> {
    let f = corpus(rows(2, &[("old", &[1., 0.])])).await?;
    let before = image(&f.captured).await?;
    let base = f.captured.version().version;
    let first = prepare(
        &f.captured,
        WriteMode::Overwrite,
        rows(3, &[("first", &[1., 0., 0.])]),
    )
    .await?;
    let second = prepare(
        &f.captured,
        WriteMode::Overwrite,
        rows(4, &[("second", &[1., 0., 0., 0.])]),
    )
    .await?;
    let first_uuid = first.uuid.clone();
    let second_uuid = second.uuid.clone();
    assert_ne!(first_uuid, second_uuid);
    let winner = publish(&f.captured, first).await?;
    let winner_image = image(&winner).await?;
    assert_eq!(
        Schema::from(winner.schema()),
        sdk_schema(3).as_ref().clone()
    );
    assert_eq!(
        winner_image,
        vec![expected_sdk_rows(&rows(3, &[("first", &[1., 0., 0.])]))]
    );
    let error = publish(&f.captured, second).await.unwrap_err();
    assert!(
        matches!(error, lance::Error::CommitConflict { .. }),
        "{error:?}"
    );
    assert!(!error.is_commit_status_unknown());
    let reopened = Dataset::open(&f.uri).await?;
    assert_eq!(reopened.version().version, base + 1);
    assert_eq!(
        Schema::from(reopened.schema()),
        sdk_schema(3).as_ref().clone()
    );
    assert_eq!(image(&reopened).await?, winner_image);
    assert_eq!(uuid(&reopened, base + 1).await?, first_uuid);
    assert_ne!(uuid(&reopened, base + 1).await?, second_uuid);
    assert_eq!(image(&f.captured).await?, before);
    Ok(())
}

#[tokio::test]
async fn a_foreign_append_prevents_old_overwrite_from_rebasing_or_adopting_rows() -> Result<()> {
    let f = corpus(rows(2, &[("old", &[1., 0.])])).await?;
    let before = image(&f.captured).await?;
    let base = f.captured.version().version;
    let old = prepare(
        &f.captured,
        WriteMode::Overwrite,
        rows(3, &[("obsolete", &[1., 0., 0.])]),
    )
    .await?;
    let old_uuid = old.uuid.clone();
    let foreign = prepare(
        &f.captured,
        WriteMode::Append,
        rows(2, &[("foreign", &[0., 1.])]),
    )
    .await?;
    let foreign_uuid = foreign.uuid.clone();
    let winner = publish(&f.captured, foreign).await?;
    let winner_image = image(&winner).await?;
    // Compare all six values independently of SDK batch segmentation/order.
    // Each input batch assigned its single row fingerprint [1; 32].
    let winner_rows: Vec<_> = winner_image
        .iter()
        .flat_map(|batch| (0..batch.num_rows()).map(move |i| batch.slice(i, 1)))
        .collect();
    assert_eq!(winner_rows.len(), 2);
    assert!(winner_rows.contains(&expected_sdk_rows(&rows(2, &[("old", &[1., 0.])]))));
    assert!(winner_rows.contains(&expected_sdk_rows(&rows(2, &[("foreign", &[0., 1.])]))));
    let error = publish(&f.captured, old).await.unwrap_err();
    assert!(
        matches!(error, lance::Error::CommitConflict { .. }),
        "{error:?}"
    );
    let latest = Dataset::open(&f.uri).await?;
    assert_eq!(latest.version().version, base + 1);
    assert_eq!(
        Schema::from(latest.schema()),
        sdk_schema(2).as_ref().clone()
    );
    assert_eq!(latest.count_rows(None).await?, 2);
    assert_eq!(image(&latest).await?, winner_image);
    assert_eq!(uuid(&latest, base + 1).await?, foreign_uuid);
    assert_ne!(uuid(&latest, base + 1).await?, old_uuid);
    assert_eq!(image(&f.captured).await?, before);
    Ok(())
}
