use arrow_array::{ArrayRef, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::lance_branch_store::JsonFtsIndexMode;
use graphit_data::{
    lance_branch_store::{BranchError, BranchSnapshot, DeltaResolution, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use lance::{Dataset, index::DatasetIndexExt};
use lance_index::{
    IndexType,
    optimize::OptimizeOptions,
    scalar::{
        FullTextSearchQuery, InvertedIndexParams, ScalarIndexParams,
        inverted::query::{FtsQuery, MatchQuery, Operator},
    },
};
use std::{collections::BTreeSet, error::Error, path::Path, sync::Arc};
type TestResult = Result<(), Box<dyn Error>>;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("payload", DataType::Utf8, true),
        Field::new("other", DataType::Utf8, false),
        Field::new("marker", DataType::UInt64, false),
    ]))
}
fn rows(values: &[(&str, Option<&str>, &str)]) -> RecordBatch {
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            values.iter().map(|r| r.0).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            values.iter().map(|r| r.1).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            values.iter().map(|r| r.2).collect::<Vec<_>>(),
        )),
        Arc::new(UInt64Array::from(vec![17; values.len()])),
    ];
    RecordBatch::try_new(schema(), arrays).unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    uri: String,
    store: LanceBranchStore,
}
async fn fixture(values: &[(&str, Option<&str>, &str)]) -> Result<Fixture, Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let uri = temp
        .path()
        .join("records.lance")
        .to_string_lossy()
        .into_owned();
    LanceModuleStore::create(&uri, schema(), vec![rows(values)]).await?;
    Ok(Fixture {
        store: LanceBranchStore::open(&uri, schema(), None).await?,
        _temp: temp,
        uri,
    })
}
async fn image(snapshot: &BranchSnapshot) -> Result<Vec<RecordBatch>, lance::Error> {
    snapshot.scan().try_into_stream().await?.try_collect().await
}
async fn hits(
    snapshot: &BranchSnapshot,
    column: &str,
    word: &str,
) -> Result<BTreeSet<String>, lance::Error> {
    let query = MatchQuery::new(format!("body,str,{word}"))
        .with_column(Some(column.into()))
        .with_operator(Operator::Or);
    let mut scanner = snapshot.scan();
    scanner.project(&["id"])?;
    scanner.full_text_search(FullTextSearchQuery::new_query(FtsQuery::Match(query)).limit(None))?;
    let batches: Vec<RecordBatch> = scanner.try_into_stream().await?.try_collect().await?;
    let mut result = BTreeSet::new();
    for batch in batches {
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for id in ids.iter().flatten() {
            result.insert(id.to_owned());
        }
    }
    Ok(result)
}
async fn indices(uri: &str) -> Result<Vec<lance_table::format::IndexMetadata>, lance::Error> {
    let dataset = Dataset::open(uri).await?;
    let store = dataset.object_store(None).await?;
    lance_table::io::manifest::read_manifest_indexes(
        store.as_ref(),
        dataset.manifest_location(),
        dataset.manifest(),
    )
    .await
}
fn files(root: &Path) -> std::io::Result<BTreeSet<std::path::PathBuf>> {
    fn visit(
        root: &Path,
        current: &Path,
        out: &mut BTreeSet<std::path::PathBuf>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                visit(root, &entry.path(), out)?;
            } else {
                out.insert(entry.path().strip_prefix(root).unwrap().to_path_buf());
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    visit(root, root, &mut out)?;
    Ok(out)
}

#[tokio::test]
async fn new_index_is_unpublished_then_strictly_committed_with_original_rows_and_key() -> TestResult
{
    let long = "x".repeat(160);
    let payload =
        serde_json::json!({"body":format!("Running the função my_symbol {long}")}).to_string();
    let f = fixture(&[("a", Some(&payload), "{\"body\":\"foreign\"}")]).await?;
    let old = f.store.capture_target(None).await?;
    let original = image(&old).await?;
    let pending = old
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?;
    let key = pending.recovery_key().clone();
    assert_eq!(key.corpus_uri(), f.uri);
    assert_eq!(key.base_version(), old.version());
    assert_eq!(key.physical_branch(), None);
    assert!(!key.transaction_id().is_empty());
    assert_eq!(f.store.capture_target(None).await?.version(), old.version());
    assert!(indices(&f.uri).await?.is_empty());
    assert_eq!(old.resolve(&key).await?, DeltaResolution::Unresolved);
    let next = pending.commit().await?;
    assert_eq!(next.version(), old.version() + 1);
    assert_eq!(image(&next).await?, original);
    assert_eq!(image(&old).await?, original);
    assert_eq!(old.version(), 1);
    assert_eq!(
        next.resolve(&key).await?,
        DeltaResolution::Applied {
            version: next.version()
        }
    );
    let metadata = indices(&f.uri).await?;
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].name, "content");
    assert_eq!(metadata[0].dataset_version, old.version());
    assert_ne!(metadata[0].uuid.to_string(), key.transaction_id());
    let reopened = f.store.capture_target(None).await?;
    for token in ["RUNNING", "the", "FUNÇÃO", "my_symbol", &long] {
        assert_eq!(
            hits(&reopened, "payload", token).await?,
            BTreeSet::from(["a".into()])
        );
    }
    for token in ["run", "funcao", "missing"] {
        assert!(hits(&reopened, "payload", token).await?.is_empty());
    }
    assert_eq!(
        f.store.capture_target(None).await?.version(),
        next.version()
    );
    assert_eq!(indices(&f.uri).await?[0].uuid, metadata[0].uuid);
    Ok(())
}

#[tokio::test]
async fn inherited_rejects_before_io_and_named_publication_preserves_main_and_old_pin() -> TestResult
{
    let f = fixture(&[("a", Some("{\"body\":\"marker\"}"), "{}")]).await?;
    let main = f.store.capture_target(None).await?;
    let inherited = f.store.capture_target(Some("feature")).await?;
    let before = files(Path::new(&f.uri))?;
    assert!(matches!(
        inherited
            .prepare_json_fts_index("missing", "content", JsonFtsIndexMode::Create)
            .await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert_eq!(files(Path::new(&f.uri))?, before);
    let branch = inherited.materialize().await?;
    let old = image(&branch).await?;
    let pending = branch
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?;
    let key = pending.recovery_key().clone();
    assert_eq!(key.physical_branch(), Some("feature"));
    let next = pending.commit().await?;
    assert_eq!(next.version(), branch.version() + 1);
    assert_eq!(next.physical_branch(), Some("feature"));
    assert_eq!(image(&next).await?, old);
    assert_eq!(image(&branch).await?, old);
    assert_eq!(
        next.resolve(&key).await?,
        DeltaResolution::Applied {
            version: next.version()
        }
    );
    assert!(matches!(
        main.resolve(&key).await,
        Err(BranchError::RecoveryBinding)
    ));
    assert_eq!(
        f.store.capture_target(None).await?.version(),
        main.version()
    );
    assert!(indices(&f.uri).await?.is_empty());
    let reopened = f.store.capture_target(Some("feature")).await?;
    assert_eq!(
        hits(&reopened, "payload", "marker").await?,
        BTreeSet::from(["a".into()])
    );
    Ok(())
}

#[tokio::test]
async fn name_collision_same_or_other_column_rejects_and_foreign_index_survives() -> TestResult {
    let f = fixture(&[(
        "a",
        Some("{\"body\":\"primary\"}"),
        "{\"body\":\"foreign\"}",
    )])
    .await?;
    let initial = f.store.capture_target(None).await?;
    let foreign = initial
        .prepare_json_fts_index("other", "foreign", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let before = files(Path::new(&f.uri))?;
    let foreign_metadata = indices(&f.uri).await?;
    for column in ["other", "payload"] {
        match foreign
            .prepare_json_fts_index(column, "foreign", JsonFtsIndexMode::Create)
            .await
        {
            Err(BranchError::Sdk(cause)) => assert!(
                matches!(cause.as_ref(), lance::Error::Index { .. }),
                "{cause:?}"
            ),
            _ => panic!("occupied index name must preserve SDK Index cause"),
        }
    }
    assert_eq!(files(Path::new(&f.uri))?, before);
    let next = foreign
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let metadata = indices(&f.uri).await?;
    assert_eq!(metadata.len(), 2);
    assert!(
        metadata
            .iter()
            .any(|i| i.uuid == foreign_metadata[0].uuid && i.name == "foreign")
    );
    assert_eq!(
        hits(&next, "other", "foreign").await?,
        BTreeSet::from(["a".into()])
    );
    assert_eq!(
        hits(&next, "payload", "primary").await?,
        BTreeSet::from(["a".into()])
    );
    assert_eq!(image(&next).await?, image(&initial).await?);
    Ok(())
}

#[tokio::test]
async fn malformed_late_json_and_trailing_value_are_typed_before_any_index_file() -> TestResult {
    let mut values = vec![("valid", Some("{\"body\":\"valid\"}"), "{}"); 10_000];
    values.push(("invalid", Some("{invalid"), "{}"));
    let deep = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    for invalid in [
        "{invalid",
        "{} trailing",
        "{} {}",
        "",
        "{\"body\":1e400}",
        r#"{"body":"\ud800"}"#,
        r#"{"body":"\udc00"}"#,
        r#"{"body":"\q"}"#,
        &deep,
    ] {
        values.last_mut().unwrap().1 = Some(invalid);
        let f = fixture(&values).await?;
        let snapshot = f.store.capture_target(None).await?;
        let before = files(Path::new(&f.uri))?;
        match snapshot
            .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
            .await
        {
            Err(error @ BranchError::InvalidJsonIndexValue { .. }) => {
                assert!(error.source().is_some());
                let BranchError::InvalidJsonIndexValue { column, row, cause } = error else {
                    unreachable!()
                };
                assert_eq!(column, "payload");
                assert!(row > 0);
                assert!(cause.is_syntax() || cause.is_eof());
            }
            _ => panic!("late JSON must be rejected by Data validation"),
        }
        assert_eq!(files(Path::new(&f.uri))?, before);
        assert_eq!(
            f.store.capture_target(None).await?.version(),
            snapshot.version()
        );
        assert!(indices(&f.uri).await?.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn sdk_valid_pairs_duplicate_keys_and_normal_nesting_are_not_overrejected() -> TestResult {
    let f = fixture(&[(
        "a",
        Some(r#"{"body":"first","body":"last \uD83D\uDE00","nested":[[[{"key":42}]]]}"#),
        "{}",
    )])
    .await?;
    let initial = f.store.capture_target(None).await?;
    let next = initial
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    assert_eq!(
        hits(&next, "payload", "last").await?,
        BTreeSet::from(["a".into()])
    );
    assert!(hits(&next, "payload", "first").await?.is_empty());
    assert_eq!(image(&next).await?, image(&initial).await?);
    Ok(())
}

#[tokio::test]
async fn null_missing_and_wrong_type_are_distinct_and_empty_dataset_can_be_indexed() -> TestResult {
    let f = fixture(&[("a", Some("{}"), "{}"), ("null", None, "{}")]).await?;
    let snapshot = f.store.capture_target(None).await?;
    let before = files(Path::new(&f.uri))?;
    assert!(
        matches!(snapshot.prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create).await, Err(BranchError::NullJsonIndexValue { column, .. }) if column == "payload")
    );
    for column in ["missing", "marker"] {
        assert!(
            matches!(snapshot.prepare_json_fts_index(column, "content", JsonFtsIndexMode::Create).await, Err(BranchError::JsonIndexColumn(c)) if c == column)
        );
    }
    assert_eq!(files(Path::new(&f.uri))?, before);
    assert_eq!(f.store.capture_target(None).await?.version(), 1);
    let empty = fixture(&[]).await?;
    let initial = empty.store.capture_target(None).await?;
    let next = initial
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    assert_eq!(next.version(), 2);
    assert!(hits(&next, "payload", "missing").await?.is_empty());
    assert_eq!(
        image(&next)
            .await?
            .iter()
            .map(RecordBatch::num_rows)
            .sum::<usize>(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn competing_indices_do_not_rebase_or_adopt_the_winner() -> TestResult {
    let f = fixture(&[(
        "a",
        Some("{\"body\":\"primary\"}"),
        "{\"body\":\"foreign\"}",
    )])
    .await?;
    let snapshot = f.store.capture_target(None).await?;
    let first = snapshot
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?;
    let second = snapshot
        .prepare_json_fts_index("other", "foreign", JsonFtsIndexMode::Create)
        .await?;
    let first_key = first.recovery_key().clone();
    let second_key = second.recovery_key().clone();
    assert_ne!(first_key.transaction_id(), second_key.transaction_id());
    let winner = first.commit().await?;
    let failure = match second.commit().await {
        Err(e) => e,
        Ok(_) => panic!("stale index cannot rebase"),
    };
    assert_eq!(failure.recovery_key(), &second_key);
    assert!(failure.source().is_some());
    assert!(
        matches!(failure.sdk_error(), lance::Error::CommitConflict { .. }),
        "{:?}",
        failure.sdk_error()
    );
    assert_eq!(
        winner.resolve(&first_key).await?,
        DeltaResolution::Applied { version: 2 }
    );
    assert_eq!(
        winner.resolve(&second_key).await?,
        DeltaResolution::Conflicted { version: 2 }
    );
    assert_eq!(f.store.capture_target(None).await?.version(), 2);
    let metadata = indices(&f.uri).await?;
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].name, "content");
    assert_eq!(image(&winner).await?, image(&snapshot).await?);
    let foreign = fixture(&[("a", Some("{}"), "{}")]).await?;
    assert!(matches!(
        foreign
            .store
            .capture_target(None)
            .await?
            .resolve(&first_key)
            .await,
        Err(BranchError::RecoveryBinding)
    ));
    Ok(())
}

#[tokio::test]
async fn concurrent_record_delta_preserves_winner_and_unpublished_index_recovery() -> TestResult {
    let f = fixture(&[("a", Some("{\"body\":\"original\"}"), "{}")]).await?;
    let snapshot = f.store.capture_target(None).await?;
    let pending = snapshot
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?;
    let index_key = pending.recovery_key().clone();
    let delta = snapshot
        .prepare_merge(
            vec![rows(&[("b", Some("{\"body\":\"winner\"}"), "{}")])],
            vec!["id".into()],
        )
        .await?;
    let delta_key = delta.recovery_key().clone();
    let winner = delta.commit().await?;
    let expected = image(&winner).await?;
    let failure = match pending.commit().await {
        Err(e) => e,
        Ok(_) => panic!("stale index cannot overwrite delta"),
    };
    assert_eq!(failure.recovery_key(), &index_key);
    assert!(
        matches!(failure.sdk_error(), lance::Error::CommitConflict { .. }),
        "{:?}",
        failure.sdk_error()
    );
    let current = f.store.capture_target(None).await?;
    assert_eq!(current.version(), 2);
    assert_eq!(image(&current).await?, expected);
    assert_eq!(expected.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
    assert!(indices(&f.uri).await?.is_empty());
    assert_eq!(
        current.resolve(&index_key).await?,
        DeltaResolution::Conflicted { version: 2 }
    );
    assert_eq!(
        current.resolve(&delta_key).await?,
        DeltaResolution::Applied { version: 2 }
    );
    assert_eq!(
        image(&snapshot)
            .await?
            .iter()
            .map(RecordBatch::num_rows)
            .sum::<usize>(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn replacement_removes_all_real_segments_and_preserves_foreign_and_old_pins() -> TestResult {
    let f = fixture(&[
        ("a", Some(r#"{"body":"alpha"}"#), r#"{"body":"foreign"}"#),
        ("c", Some(r#"{"body":"stable"}"#), r#"{"body":"foreign"}"#),
    ])
    .await?;
    let original = f.store.capture_target(None).await?;
    let indexed = original
        .prepare_json_fts_index("other", "foreign", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let old_image = image(&indexed).await?;
    let added = indexed
        .prepare_merge(
            vec![rows(&[(
                "b",
                Some(r#"{"body":"beta"}"#),
                r#"{"body":"foreign"}"#,
            )])],
            vec!["id".into()],
        )
        .await?
        .commit()
        .await?;
    let mut dataset = Dataset::open(&f.uri).await?;
    dataset.optimize_indices(&OptimizeOptions::append()).await?;
    let segmented = f.store.capture_target(None).await?;
    segmented
        .prepare_merge(
            vec![rows(&[(
                "a",
                Some(r#"{"body":"gamma"}"#),
                r#"{"body":"foreign"}"#,
            )])],
            vec!["id".into()],
        )
        .await?
        .commit()
        .await?;
    let base = f.store.capture_target(None).await?;
    assert!(base.version() > added.version());
    let before = indices(&f.uri).await?;
    let targets: BTreeSet<_> = before
        .iter()
        .filter(|i| i.name == "content")
        .map(|i| i.uuid)
        .collect();
    let foreign: BTreeSet<_> = before
        .iter()
        .filter(|i| i.name == "foreign")
        .map(|i| i.uuid)
        .collect();
    assert!(
        targets.len() >= 2,
        "SDK append must produce real index segments"
    );
    assert!(!foreign.is_empty());
    let expected = image(&base).await?;
    let prepared = base
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Replace)
        .await?;
    let key = prepared.recovery_key().clone();
    assert_eq!(
        f.store.capture_target(None).await?.version(),
        base.version()
    );
    let next = prepared.commit().await?;
    assert_eq!(next.version(), base.version() + 1);
    assert_eq!(
        next.resolve(&key).await?,
        DeltaResolution::Applied {
            version: next.version()
        }
    );
    let after = indices(&f.uri).await?;
    let refreshed: Vec<_> = after.iter().filter(|i| i.name == "content").collect();
    assert_eq!(refreshed.len(), 1);
    assert!(!targets.contains(&refreshed[0].uuid));
    assert!(!after.iter().any(|i| targets.contains(&i.uuid)));
    assert_eq!(
        after
            .iter()
            .filter(|i| i.name == "foreign")
            .map(|i| i.uuid)
            .collect::<BTreeSet<_>>(),
        foreign
    );
    assert_eq!(image(&next).await?, expected);
    assert_eq!(image(&indexed).await?, old_image);
    assert_eq!(
        hits(&indexed, "payload", "alpha").await?,
        BTreeSet::from(["a".into()])
    );
    assert!(hits(&indexed, "payload", "beta").await?.is_empty());
    assert!(hits(&next, "payload", "alpha").await?.is_empty());
    assert_eq!(
        hits(&next, "payload", "gamma").await?,
        BTreeSet::from(["a".into()])
    );
    assert_eq!(
        hits(&next, "payload", "beta").await?,
        BTreeSet::from(["b".into()])
    );
    assert_eq!(
        hits(&next, "other", "foreign").await?,
        BTreeSet::from(["a".into(), "b".into(), "c".into()])
    );
    Ok(())
}

#[tokio::test]
async fn replacement_preconditions_fail_before_building_or_publishing() -> TestResult {
    for case in ["missing", "field", "kind", "policy", "ambiguous"] {
        let f = fixture(&[("a", Some(r#"{"body":"alpha"}"#), r#"{"body":"foreign"}"#)]).await?;
        let initial = f.store.capture_target(None).await?;
        match case {
            "missing" => {
                initial
                    .prepare_json_fts_index("other", "foreign", JsonFtsIndexMode::Create)
                    .await?
                    .commit()
                    .await?;
            }
            "field" => {
                initial
                    .prepare_json_fts_index("other", "content", JsonFtsIndexMode::Create)
                    .await?
                    .commit()
                    .await?;
            }
            "kind" => {
                Dataset::open(&f.uri)
                    .await?
                    .create_index(
                        &["payload"],
                        IndexType::BTree,
                        Some("content".into()),
                        &ScalarIndexParams::default(),
                        false,
                    )
                    .await?;
            }
            "policy" => {
                // Valid JSON rows do not turn a physical simple-text index into JSON FTS.
                let params = InvertedIndexParams::default()
                    .base_tokenizer("simple".into())
                    .max_token_length(None)
                    .lower_case(true)
                    .stem(false)
                    .remove_stop_words(false)
                    .ascii_folding(false);
                Dataset::open(&f.uri)
                    .await?
                    .create_index(
                        &["payload"],
                        IndexType::Inverted,
                        Some("content".into()),
                        &params,
                        false,
                    )
                    .await?;
            }
            "ambiguous" => {
                initial
                    .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
                    .await?
                    .commit()
                    .await?
                    .prepare_json_fts_index("payload", "competing", JsonFtsIndexMode::Create)
                    .await?
                    .commit()
                    .await?;
            }
            _ => unreachable!(),
        }
        let base = f.store.capture_target(None).await?;
        let before = files(Path::new(&f.uri))?;
        let error = match base
            .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Replace)
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("{case} must reject replacement before building"),
        };
        if case == "missing" {
            assert!(
                matches!(error, BranchError::JsonIndexReplacementMissing { column, name } if column == "payload" && name == "content")
            );
        } else {
            assert!(
                matches!(error, BranchError::JsonIndexReplacementIncompatible { column, name } if column == "payload" && name == "content"),
                "case {case}"
            );
        }
        assert_eq!(files(Path::new(&f.uri))?, before, "case {case}");
        assert_eq!(
            f.store.capture_target(None).await?.version(),
            base.version(),
            "case {case}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn competing_replacements_keep_the_winner_and_real_recovery_identity() -> TestResult {
    let f = fixture(&[("a", Some(r#"{"body":"alpha"}"#), "{}")]).await?;
    let base = f
        .store
        .capture_target(None)
        .await?
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let first = base
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Replace)
        .await?;
    let second = base
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Replace)
        .await?;
    let first_key = first.recovery_key().clone();
    let second_key = second.recovery_key().clone();
    assert_ne!(first_key.transaction_id(), second_key.transaction_id());
    let winner = first.commit().await?;
    let winner_indices = indices(&f.uri).await?;
    let error = match second.commit().await {
        Err(error) => error,
        Ok(_) => panic!("stale replacement must not rebase"),
    };
    assert_eq!(error.recovery_key(), &second_key);
    assert!(std::ptr::eq(
        error
            .source()
            .and_then(|cause| cause.downcast_ref::<lance::Error>())
            .expect("preserved SDK cause"),
        error.sdk_error(),
    ));
    assert_eq!(
        winner.resolve(&first_key).await?,
        DeltaResolution::Applied {
            version: winner.version()
        }
    );
    assert_eq!(
        winner.resolve(&second_key).await?,
        DeltaResolution::Conflicted {
            version: winner.version()
        }
    );
    assert_eq!(indices(&f.uri).await?, winner_indices);
    assert_eq!(
        f.store.capture_target(None).await?.version(),
        winner.version()
    );
    assert_eq!(image(&winner).await?, image(&base).await?);
    assert_eq!(
        hits(&winner, "payload", "alpha").await?,
        BTreeSet::from(["a".into()])
    );
    Ok(())
}
