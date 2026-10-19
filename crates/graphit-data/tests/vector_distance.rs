use arrow_array::{
    Array, ArrayRef, ListArray, RecordBatch, RecordBatchIterator, StringArray,
    builder::{Float32Builder, ListBuilder, StringBuilder},
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::lance_branch_store::JsonFtsIndexMode;
use graphit_data::{
    lance_branch_store::{
        BranchError, BranchSnapshot, DeltaResolution, JsonFtsIndexState, LanceBranchStore,
    },
    vector_distance::{VectorDistanceError as E, cosine_distances},
};
use lance::Dataset;
use lance_index::scalar::{
    FullTextSearchQuery,
    inverted::query::{MatchQuery, Operator},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::Arc,
};
type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

fn vectors(rows: &[Option<&[f32]>]) -> ListArray {
    let mut builder = ListBuilder::new(Float32Builder::new());
    for row in rows {
        if let Some(values) = row {
            builder.values().append_slice(values);
            builder.append(true);
        } else {
            builder.append(false);
        }
    }
    builder.finish()
}
fn close(actual: Option<f32>, expected: f32) {
    assert!(
        (actual.unwrap() - expected).abs() < 1e-6,
        "{actual:?} != {expected}"
    );
}
#[test]
fn alignment_slices_offsets_and_real_dimensions() {
    let values = vectors(&[
        Some(&[0., 1.]),
        Some(&[1., 0.]),
        None,
        Some(&[-1., 0.]),
        Some(&[0., 1.]),
    ]);
    let scores = cosine_distances(&values.slice(1, 3), &[1., 0.]).unwrap();
    assert_eq!(scores.len(), 3);
    close(scores[0], 0.);
    assert_eq!(scores[1], None);
    close(scores[2], 2.);
    let scores = cosine_distances(
        &vectors(&[Some(&[0., 0., 1.]), Some(&[1., 0., 0.])]),
        &[0., 0., 1.],
    )
    .unwrap();
    close(scores[0], 0.);
    close(scores[1], 1.);
}
#[test]
fn query_and_late_dimension_errors_return_no_scores() {
    let values = vectors(&[Some(&[1., 0.]), Some(&[1.])]);
    assert_eq!(cosine_distances(&values, &[]), Err(E::EmptyQuery));
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            cosine_distances(&values, &[1., invalid]),
            Err(E::NonFiniteQuery { element: 1 })
        );
    }
    assert_eq!(
        cosine_distances(&values, &[1., 0.]),
        Err(E::Dimensions {
            row: 1,
            expected: 2,
            actual: 1
        })
    );
    assert_eq!(
        cosine_distances(&vectors(&[Some(&[])]), &[1.]),
        Err(E::Dimensions {
            row: 0,
            expected: 1,
            actual: 0
        })
    );
}
#[test]
fn child_type_and_null_elements_are_not_absent_vectors() {
    let mut text = ListBuilder::new(StringBuilder::new());
    text.values().append_value("1");
    text.append(true);
    assert_eq!(cosine_distances(&text.finish(), &[1.]), Err(E::VectorType));
    let mut values = ListBuilder::new(Float32Builder::new());
    values.values().append_value(1.);
    values.values().append_null();
    values.append(true);
    assert_eq!(
        cosine_distances(&values.finish(), &[1., 0.]),
        Err(E::NullElement { row: 0, element: 1 })
    );
    assert_eq!(
        cosine_distances(&vectors(&[None, None]), &[1., 0.]),
        Ok(vec![None, None])
    );
}
#[test]
fn late_nonfinite_vectors_and_sdk_numerical_results_fail_whole_batch() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            cosine_distances(
                &vectors(&[Some(&[1., 0.]), Some(&[1., invalid])]),
                &[1., 0.]
            ),
            Err(E::NonFiniteVector { row: 1, element: 1 })
        );
    }
    assert_eq!(
        cosine_distances(&vectors(&[Some(&[1., 0.]), Some(&[0., 0.])]), &[1., 0.]),
        Err(E::NonFiniteDistance { row: 1 })
    );
    assert_eq!(
        cosine_distances(&vectors(&[Some(&[1., 0.])]), &[0., 0.]),
        Err(E::NonFiniteDistance { row: 0 })
    );
    assert_eq!(
        cosine_distances(
            &vectors(&[Some(&[f32::MAX, f32::MAX])]),
            &[f32::MAX, f32::MAX]
        ),
        Err(E::NonFiniteDistance { row: 0 })
    );
}
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("row_key", DataType::Utf8, false),
        Field::new("payload", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
            true,
        ),
    ]))
}
fn rows(input: &[(&str, Option<&[f32]>)]) -> RecordBatch {
    RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(StringArray::from(
                input.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(
                input
                    .iter()
                    .map(|(key, _)| format!("{{\"body\":\"{key}\"}}"))
                    .collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(vectors(
                &input.iter().map(|(_, vector)| *vector).collect::<Vec<_>>(),
            )),
        ],
    )
    .unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    uri: String,
    store: LanceBranchStore,
}
async fn fixture(
    input: &[(&str, Option<&[f32]>)],
) -> Result<Fixture, Box<dyn Error + Send + Sync>> {
    let temp = tempfile::tempdir()?;
    let uri = temp
        .path()
        .join("vectors.lance")
        .to_string_lossy()
        .into_owned();
    Dataset::write(
        RecordBatchIterator::new(vec![Ok(rows(input))], schema()),
        &uri,
        None,
    )
    .await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    Ok(Fixture {
        _temp: temp,
        uri,
        store,
    })
}
async fn scores(
    pin: &BranchSnapshot,
    query: &[f32],
) -> Result<(BTreeMap<String, Option<f32>>, usize), Box<dyn Error + Send + Sync>> {
    let mut scanner = pin.scan();
    scanner.project(&["row_key", "vector"])?;
    scanner.batch_size(2);
    let mut stream = scanner.try_into_stream().await?;
    let mut out = BTreeMap::new();
    let mut batches = 0;
    while let Some(batch) = stream.try_next().await? {
        batches += 1;
        let keys = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let vectors = batch
            .column(1)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        for (row, score) in cosine_distances(vectors, query)?.into_iter().enumerate() {
            assert!(out.insert(keys.value(row).to_owned(), score).is_none());
        }
    }
    Ok((out, batches))
}
async fn fts(
    pin: &BranchSnapshot,
    term: &str,
) -> Result<BTreeSet<String>, Box<dyn Error + Send + Sync>> {
    let query = MatchQuery::new(format!("body,str,{term}"))
        .with_column(Some("payload".into()))
        .with_operator(Operator::Or)
        .with_fuzziness(Some(0));
    let mut scanner = pin.scan();
    scanner.project(&["row_key"])?;
    scanner.full_text_search(FullTextSearchQuery::new_query(query.into()).limit(None))?;
    let mut stream = scanner.try_into_stream().await?;
    let mut result = BTreeSet::new();
    while let Some(batch) = stream.try_next().await? {
        let keys = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for row in 0..keys.len() {
            result.insert(keys.value(row).to_owned());
        }
    }
    Ok(result)
}
#[tokio::test]
async fn sdk_list_roundtrip_reopen_and_complete_multiple_batches() -> TestResult {
    let names = (0..9).map(|i| format!("row{i}")).collect::<Vec<_>>();
    let input = names
        .iter()
        .enumerate()
        .map(|(i, key)| {
            (
                key.as_str(),
                if i == 4 { None } else { Some(&[1., 0.][..]) },
            )
        })
        .collect::<Vec<_>>();
    let f = fixture(&input).await?;
    let pin = f.store.capture_target(None).await?;
    assert_eq!(pin.schema(), schema().as_ref());
    let reopened = LanceBranchStore::open(&f.uri, schema(), None)
        .await?
        .capture_target(None)
        .await?;
    let (observed, batches) = scores(&reopened, &[1., 0.]).await?;
    assert_eq!(observed.len(), 9);
    assert!(batches > 1, "actual SDK emitted {batches} batches");
    assert_eq!(observed["row4"], None);
    for (key, value) in observed {
        if key != "row4" {
            close(value, 0.);
        }
    }
    assert_eq!(pin.version(), reopened.version());
    Ok(())
}
#[tokio::test]
async fn sdk_strict_merge_stale_cas_recovery_old_pin_and_fts_preservation() -> TestResult {
    let f = fixture(&[
        ("alpha", Some(&[1., 0.])),
        ("beta", Some(&[0., 1.])),
        ("absent", None),
    ])
    .await?;
    let pin = f.store.capture_target(None).await?;
    assert_eq!(
        pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    let indexed = pin
        .prepare_json_fts_index("payload", "search", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    assert_eq!(
        indexed.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    let winner = indexed
        .prepare_merge(
            vec![rows(&[("alpha", Some(&[0., 1.]))])],
            vec!["row_key".into()],
        )
        .await?;
    let winning_key = winner.recovery_key().clone();
    let loser = indexed
        .prepare_merge(
            vec![rows(&[("beta", Some(&[1., 0.]))])],
            vec!["row_key".into()],
        )
        .await?;
    let lost_key = loser.recovery_key().clone();
    let published = winner.commit().await?;
    let failure = match loser.commit().await {
        Ok(_) => panic!("stale CAS must fail"),
        Err(failure) => failure,
    };
    assert_eq!(failure.recovery_key(), &lost_key);
    assert!(failure.source().is_some());
    assert_eq!(
        published.resolve(&winning_key).await?,
        DeltaResolution::Applied {
            version: published.version()
        }
    );
    assert_eq!(
        published.resolve(&lost_key).await?,
        DeltaResolution::Conflicted {
            version: published.version()
        }
    );
    assert_eq!(
        published.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    assert_eq!(
        fts(&published, "alpha").await?,
        BTreeSet::from(["alpha".into()])
    );
    let (current, _) = scores(&published, &[1., 0.]).await?;
    close(current["alpha"], 1.);
    close(current["beta"], 1.);
    assert_eq!(current["absent"], None);
    close(scores(&indexed, &[1., 0.]).await?.0["alpha"], 0.);
    close(
        scores(&f.store.capture_target(None).await?, &[1., 0.])
            .await?
            .0["alpha"],
        1.,
    );
    let other = fixture(&[("alpha", Some(&[1., 0.]))]).await?;
    assert!(matches!(
        other
            .store
            .capture_target(None)
            .await?
            .resolve(&winning_key)
            .await,
        Err(BranchError::RecoveryBinding)
    ));
    Ok(())
}
#[tokio::test]
async fn sdk_named_branch_isolation_and_inherited_pin() -> TestResult {
    let f = fixture(&[("alpha", Some(&[1., 0.])), ("absent", None)]).await?;
    let inherited = f.store.capture_target(Some("feature")).await?;
    assert_eq!(inherited.inherited_target(), Some("feature"));
    assert_eq!(inherited.physical_branch(), None);
    close(scores(&inherited, &[1., 0.]).await?.0["alpha"], 0.);
    assert_eq!(
        f.store
            .capture_target(Some("feature"))
            .await?
            .inherited_target(),
        Some("feature")
    );
    let branch = inherited.clone().materialize().await?;
    let pending = branch
        .prepare_merge(
            vec![rows(&[("alpha", Some(&[0., 1.]))])],
            vec!["row_key".into()],
        )
        .await?;
    let key = pending.recovery_key().clone();
    let changed = pending.commit().await?;
    assert_eq!(changed.physical_branch(), Some("feature"));
    close(scores(&changed, &[1., 0.]).await?.0["alpha"], 1.);
    close(scores(&inherited, &[1., 0.]).await?.0["alpha"], 0.);
    let default = f.store.capture_target(None).await?;
    close(scores(&default, &[1., 0.]).await?.0["alpha"], 0.);
    assert!(matches!(
        default.resolve(&key).await,
        Err(BranchError::RecoveryBinding)
    ));
    close(
        scores(&f.store.capture_target(Some("feature")).await?, &[1., 0.])
            .await?
            .0["alpha"],
        1.,
    );
    Ok(())
}
#[tokio::test]
async fn sdk_model_width_recompute_uses_same_list_schema() -> TestResult {
    let f = fixture(&[("alpha", Some(&[1., 0.])), ("absent", None)]).await?;
    let old = f.store.capture_target(None).await?;
    let changed = old
        .prepare_merge(
            vec![rows(&[("alpha", Some(&[0., 0., 1.]))])],
            vec!["row_key".into()],
        )
        .await?
        .commit()
        .await?;
    assert_eq!(changed.schema(), old.schema());
    let (new_scores, _) = scores(&changed, &[0., 0., 1.]).await?;
    close(new_scores["alpha"], 0.);
    assert_eq!(new_scores["absent"], None);
    assert!(scores(&changed, &[1., 0.]).await.is_err());
    close(scores(&old, &[1., 0.]).await?.0["alpha"], 0.);
    let reopened = LanceBranchStore::open(&f.uri, schema(), None)
        .await?
        .capture_target(None)
        .await?;
    close(scores(&reopened, &[0., 0., 1.]).await?.0["alpha"], 0.);
    Ok(())
}
