use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use graphit_ast::{
    indexed_source::{
        BranchSourceObservation, IndexedSourceError, MissingSourceReason, read_branch_source,
        read_indexed_source, source_schema,
    },
    source_inspection::{IndexedSource, SourceOrigin, inspect_source},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation, text_slice::Request};
use graphit_data::{
    lance_branch_store::LanceBranchStore,
    lance_module_store::{LanceModuleStore, ModuleSnapshot},
};
use std::{
    error::Error,
    fs, io,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn request() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}

fn batch(rows: &[(&str, Option<&str>)]) -> RecordBatch {
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.1).collect::<Vec<_>>(),
        )),
    ];
    RecordBatch::try_new(source_schema(), columns).unwrap()
}

async fn create(temp: &TempDir, name: &str, rows: &[(&str, Option<&str>)]) -> LanceModuleStore {
    let path = temp.path().join(name);
    LanceModuleStore::create(path.to_str().unwrap(), source_schema(), vec![batch(rows)])
        .await
        .unwrap()
}

fn operation(error: IndexedSourceError, expected: io::ErrorKind) {
    assert!(error.source().is_some());
    match error {
        IndexedSourceError::Operation(error) => assert_eq!(error.kind(), expected),
        other => panic!("expected request IO error, got {other:?}"),
    }
}

#[tokio::test]
async fn raw_utf8_and_empty_survive_reopen_and_core_selection() -> TestResult {
    let temp = TempDir::new()?;
    let raw = "\u{feff}primeira\r\nNUL\0 🦀\r\n";
    let rows = [("empty", Some("")), ("raw", Some(raw))];
    let store = create(&temp, "source.lance", &rows).await;
    drop(store);
    let reopened = LanceModuleStore::open(
        temp.path().join("source.lance").to_str().unwrap(),
        source_schema(),
    )
    .await?;
    let original = request();
    for (path, text) in rows {
        let result = read_indexed_source(&reopened.snapshot(), path, &original).await?;
        assert_eq!(result.path(), path);
        assert_eq!(result.version(), 1);
        assert_eq!(result.branch_observation(), None);
        assert_eq!(result.missing_reason(), None);
        assert_eq!(
            result.source(&original)?,
            IndexedSource::Available(text.unwrap())
        );
        let selected = inspect_source(
            result.source(&original)?,
            None,
            &Request::default(),
            &original,
        )?;
        assert_eq!(selected.origin, SourceOrigin::Indexed);
    }
    Ok(())
}

#[tokio::test]
async fn empty_corpus_absent_path_and_null_have_distinct_outcomes() -> TestResult {
    let temp = TempDir::new()?;
    let original = request();
    let empty = LanceModuleStore::create(
        temp.path().join("empty.lance").to_str().unwrap(),
        source_schema(),
        vec![],
    )
    .await?;
    let absent = read_indexed_source(&empty.snapshot(), "file", &original).await?;
    assert_eq!(
        absent.missing_reason(),
        Some(MissingSourceReason::FileAbsent)
    );
    assert_eq!(absent.source(&original)?, IndexedSource::Missing);
    let store = create(&temp, "source.lance", &[("off", None), ("empty", Some(""))]).await;
    let off = read_indexed_source(&store.snapshot(), "off", &original).await?;
    assert_eq!(
        off.missing_reason(),
        Some(MissingSourceReason::TextNotStored)
    );
    assert_eq!(off.source(&original)?, IndexedSource::Missing);
    let not_found = read_indexed_source(&store.snapshot(), "absent", &original).await?;
    assert_eq!(
        not_found.missing_reason(),
        Some(MissingSourceReason::FileAbsent)
    );
    let available = read_indexed_source(&store.snapshot(), "empty", &original).await?;
    assert_eq!(available.source(&original)?, IndexedSource::Available(""));
    assert!(inspect_source(off.source(&original)?, None, &Request::default(), &original).is_err());
    Ok(())
}

#[tokio::test]
async fn exact_literal_keys_and_duplicates_across_batches() -> TestResult {
    let temp = TempDir::new()?;
    let original = request();
    let literal = "a' OR true -- \\_/% 🦀";
    let store = create(
        &temp,
        "literal.lance",
        &[
            (literal, Some("literal")),
            ("a", Some("neighbor")),
            ("A", Some("uppercase")),
            (" a ", Some("space")),
        ],
    )
    .await;
    let result = read_indexed_source(&store.snapshot(), literal, &original).await?;
    assert_eq!(
        result.source(&original)?,
        IndexedSource::Available("literal")
    );
    for (key, text) in [("a", "neighbor"), ("A", "uppercase"), (" a ", "space")] {
        assert_eq!(
            read_indexed_source(&store.snapshot(), key, &original)
                .await?
                .source(&original)?,
            IndexedSource::Available(text),
        );
    }
    for (index, values) in [[Some("one"), Some("two")], [None, None]]
        .into_iter()
        .enumerate()
    {
        let path = temp.path().join(format!("duplicate-{index}.lance"));
        let duplicate = LanceModuleStore::create(
            path.to_str().unwrap(),
            source_schema(),
            vec![batch(&[("same", values[0])]), batch(&[("same", values[1])])],
        )
        .await?;
        assert!(matches!(
            read_indexed_source(&duplicate.snapshot(), "same", &original).await,
            Err(IndexedSourceError::DuplicatePath),
        ));
    }
    for key in ["", "bad\0path"] {
        assert!(matches!(
            read_indexed_source(&store.snapshot(), key, &original).await,
            Err(IndexedSourceError::InvalidPath),
        ));
    }
    Ok(())
}

#[tokio::test]
async fn complete_schema_including_format_types_order_and_nullability_is_strict() -> TestResult {
    let temp = TempDir::new()?;
    let original = request();
    let metadata = source_schema().metadata().clone();
    let wrong = [
        Schema::new(vec![
            Field::new("path", DataType::Utf8, false),
            Field::new("source", DataType::Utf8, true),
        ]),
        Schema::new_with_metadata(
            vec![
                Field::new("path", DataType::Utf8, true),
                Field::new("source", DataType::Utf8, true),
            ],
            metadata.clone(),
        ),
        Schema::new_with_metadata(
            vec![
                Field::new("path", DataType::Utf8, false),
                Field::new("source", DataType::Binary, true),
            ],
            metadata.clone(),
        ),
        Schema::new_with_metadata(
            vec![
                Field::new("source", DataType::Utf8, true),
                Field::new("path", DataType::Utf8, false),
            ],
            metadata.clone(),
        ),
        Schema::new_with_metadata(
            vec![
                Field::new("path", DataType::Utf8, false),
                Field::new("source", DataType::Utf8, false),
            ],
            metadata,
        ),
    ];
    for (index, schema) in wrong.into_iter().enumerate() {
        let schema = Arc::new(schema);
        let path = temp.path().join(format!("wrong-{index}.lance"));
        let store = LanceModuleStore::create(path.to_str().unwrap(), schema, vec![]).await?;
        assert!(matches!(
            read_indexed_source(&store.snapshot(), "any", &original).await,
            Err(IndexedSourceError::SchemaMismatch),
        ));
    }
    Ok(())
}

#[tokio::test]
async fn actual_publication_and_two_corpora_keep_generation_and_content() -> TestResult {
    let temp = TempDir::new()?;
    let original = request();
    let store = create(&temp, "first.lance", &[("same", Some("A\nB\n"))]).await;
    let old = store.snapshot();
    let retained = read_indexed_source(&old, "same", &original).await?;
    let new_store = old
        .prepare_replace(vec![batch(&[("same", Some("C\nD\n"))])])
        .await?
        .commit()
        .await?;
    let new = new_store.snapshot();
    assert_eq!(retained.version(), old.version());
    assert!(new.version() > retained.version());
    assert_eq!(
        retained.source(&original)?,
        IndexedSource::Available("A\nB\n")
    );
    assert_eq!(
        read_indexed_source(&old, "same", &original)
            .await?
            .source(&original)?,
        IndexedSource::Available("A\nB\n"),
    );
    assert_eq!(
        read_indexed_source(&new, "same", &original)
            .await?
            .source(&original)?,
        IndexedSource::Available("C\nD\n"),
    );
    let second = create(&temp, "second.lance", &[("same", Some("other corpus"))]).await;
    assert_eq!(
        read_indexed_source(&second.snapshot(), "same", &original)
            .await?
            .source(&original)?,
        IndexedSource::Available("other corpus"),
    );
    drop(store);
    drop(old);
    drop(new_store);
    drop(new);
    let selected = inspect_source(
        retained.source(&original)?,
        None,
        &Request {
            start_line: 2,
            end_line: 2,
            line_numbers: true,
            ..Default::default()
        },
        &original,
    )?;
    assert_eq!(selected.origin, SourceOrigin::Indexed);
    assert_eq!(selected.selection.source, "   2: B");
    assert_eq!(selected.selection.total_lines, 3);
    Ok(())
}

#[tokio::test]
async fn original_identity_cancellation_and_deadline_remain_required() -> TestResult {
    let temp = TempDir::new()?;
    let store = create(&temp, "source.lance", &[("file", Some("text"))]).await;
    let snapshot = store.snapshot();
    let original = request();
    let result = read_indexed_source(&snapshot, "file", &original).await?;
    assert!(matches!(
        result.source(&request()),
        Err(IndexedSourceError::RequestMismatch)
    ));
    let cancelled = request();
    cancelled.cancellation().cancel();
    operation(
        read_indexed_source(&snapshot, "file", &cancelled)
            .await
            .err()
            .unwrap(),
        io::ErrorKind::Interrupted,
    );
    original.cancellation().cancel();
    operation(
        result.source(&original).unwrap_err(),
        io::ErrorKind::Interrupted,
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    operation(
        read_indexed_source(&snapshot, "file", &expired)
            .await
            .err()
            .unwrap(),
        io::ErrorKind::TimedOut,
    );
    let deadline = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(2),
        Cancellation::default(),
    ));
    let retained = read_indexed_source(&snapshot, "file", &deadline).await?;
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        deadline.deadline().expect("finite fixture deadline"),
    ))
    .await;
    operation(
        retained.source(&deadline).unwrap_err(),
        io::ErrorKind::TimedOut,
    );
    Ok(())
}

mod inventory {
    use super::*;
    use futures::TryStreamExt;
    use graphit_ast::indexed_source::{branch_source_paths, indexed_source_paths};

    #[tokio::test]
    async fn paths_preserve_null_empty_and_literal_distinct_keys() -> TestResult {
        let temp = TempDir::new()?;
        let store = create(
            &temp,
            "source.lance",
            &[
                ("é", None),
                ("a", Some("")),
                ("A", Some("body")),
                ("e\u{301}", None),
                ("q' /\\_%", Some("body")),
                (" z ", None),
            ],
        )
        .await;
        let original = request();
        assert_eq!(
            indexed_source_paths(&store.snapshot(), &original).await?,
            vec![" z ", "A", "a", "e\u{301}", "q' /\\_%", "é"]
        );
        let off = read_indexed_source(&store.snapshot(), "é", &original).await?;
        assert_eq!(
            off.missing_reason(),
            Some(MissingSourceReason::TextNotStored)
        );
        assert_eq!(off.source(&original)?, IndexedSource::Missing);
        assert_eq!(
            read_indexed_source(&store.snapshot(), "a", &original)
                .await?
                .source(&original)?,
            IndexedSource::Available("")
        );
        let empty = LanceModuleStore::create(
            temp.path().join("empty.lance").to_str().unwrap(),
            source_schema(),
            vec![],
        )
        .await?;
        assert!(
            indexed_source_paths(&empty.snapshot(), &original)
                .await?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn complete_eof_spans_many_batches_and_duplicate_is_global() -> TestResult {
        let temp = TempDir::new()?;
        let count = 16_385;
        let keys: Vec<String> = (0..count)
            .rev()
            .map(|i| format!("file/{i:05}.rs"))
            .collect();
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(
                keys.iter().map(String::as_str).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(vec![None::<&str>; count])),
        ];
        let batch = RecordBatch::try_new(source_schema(), columns)?;
        let store = LanceModuleStore::create(
            temp.path().join("many.lance").to_str().unwrap(),
            source_schema(),
            vec![batch],
        )
        .await?;
        let original = request();
        let mut scanner = store.snapshot().scan();
        scanner.project(&["path"])?;
        let batches: Vec<RecordBatch> = scanner.try_into_stream().await?.try_collect().await?;
        assert!(batches.len() > 1, "fixture must cross actual SDK batches");
        let paths = indexed_source_paths(&store.snapshot(), &original).await?;
        assert_eq!(paths.len(), count);
        assert_eq!(
            paths,
            (0..count)
                .map(|i| format!("file/{i:05}.rs"))
                .collect::<Vec<_>>()
        );
        let mut duplicate_keys = keys;
        duplicate_keys[0] = "same".into();
        duplicate_keys[count - 1] = "same".into();
        let duplicate_batch = RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(
                    duplicate_keys
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(vec![None::<&str>; count])),
            ],
        )?;
        let duplicate = LanceModuleStore::create(
            temp.path().join("duplicate.lance").to_str().unwrap(),
            source_schema(),
            vec![duplicate_batch],
        )
        .await?;
        let mut duplicate_scanner = duplicate.snapshot().scan();
        duplicate_scanner.project(&["path"])?;
        let duplicate_batches: Vec<RecordBatch> = duplicate_scanner
            .try_into_stream()
            .await?
            .try_collect()
            .await?;
        let mut observed_batches = Vec::new();
        for (batch_index, batch) in duplicate_batches.iter().enumerate() {
            let paths = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            for row in 0..batch.num_rows() {
                if paths.value(row) == "same" {
                    observed_batches.push(batch_index);
                }
            }
        }
        assert_eq!(observed_batches.len(), 2);
        assert_ne!(
            observed_batches[0], observed_batches[1],
            "duplicate fixture must cross actual SDK output batches"
        );
        assert!(matches!(
            indexed_source_paths(&duplicate.snapshot(), &original).await,
            Err(IndexedSourceError::DuplicatePath)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn invalid_schema_keys_control_and_real_sdk_failure_are_errors() -> TestResult {
        let temp = TempDir::new()?;
        let original = request();
        for (i, key) in ["", "bad\0key"].into_iter().enumerate() {
            let store = create(&temp, &format!("invalid-{i}.lance"), &[(key, None)]).await;
            assert!(matches!(
                indexed_source_paths(&store.snapshot(), &original).await,
                Err(IndexedSourceError::InvalidPath)
            ));
        }
        let schema = Arc::new(Schema::new(vec![
            Field::new("path", DataType::Utf8, true),
            Field::new("source", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![None::<&str>])),
                Arc::new(StringArray::from(vec![None::<&str>])),
            ],
        )?;
        let store = LanceModuleStore::create(
            temp.path().join("nullable.lance").to_str().unwrap(),
            schema,
            vec![batch],
        )
        .await?;
        assert!(matches!(
            indexed_source_paths(&store.snapshot(), &original).await,
            Err(IndexedSourceError::SchemaMismatch)
        ));
        let store = create(&temp, "source.lance", &[("file", Some("body"))]).await;
        let cancelled = request();
        cancelled.cancellation().cancel();
        operation(
            indexed_source_paths(&store.snapshot(), &cancelled)
                .await
                .unwrap_err(),
            io::ErrorKind::Interrupted,
        );
        let expired = Arc::new(SnapshotContext::new(
            Instant::now(),
            Cancellation::default(),
        ));
        operation(
            indexed_source_paths(&store.snapshot(), &expired)
                .await
                .unwrap_err(),
            io::ErrorKind::TimedOut,
        );
        let uri = temp.path().join("source.lance");
        let native = lance::Dataset::open(uri.to_str().unwrap()).await?;
        let fragments = native.get_fragments();
        let file = &fragments[0].metadata().files[0];
        fs::remove_file(uri.join("data").join(&file.path))?;
        let error = indexed_source_paths(&store.snapshot(), &original)
            .await
            .unwrap_err();
        assert!(matches!(error, IndexedSourceError::Sdk(_)));
        assert!(error.source().is_some());
        Ok(())
    }

    #[tokio::test]
    async fn retained_default_inherited_named_and_module_generations_stay_distinct() -> TestResult {
        let temp = TempDir::new()?;
        let module = create(&temp, "source.lance", &[("base", None)]).await;
        let uri = temp.path().join("source.lance");
        let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
        let default = store.capture_target(None).await?;
        let inherited = store.capture_target(Some("feature")).await?;
        let original = request();
        assert_eq!(
            branch_source_paths(&inherited, &original).await?,
            vec!["base"]
        );
        let native = lance::Dataset::open(uri.to_str().unwrap()).await?;
        assert!(matches!(
            native.branches().get("feature").await,
            Err(lance::Error::RefNotFound { .. })
        ));
        let named = inherited.clone().materialize().await?;
        let current_named = named
            .prepare_overwrite(vec![super::batch(&[("named", None)])])
            .await?
            .commit()
            .await?;
        let current_module = module
            .snapshot()
            .prepare_replace(vec![super::batch(&[("current", Some(""))])])
            .await?
            .commit()
            .await?;
        assert_eq!(
            indexed_source_paths(&module.snapshot(), &original).await?,
            vec!["base"]
        );
        assert_eq!(
            indexed_source_paths(&current_module.snapshot(), &original).await?,
            vec!["current"]
        );
        for branch in [&default, &inherited, &named] {
            assert_eq!(branch_source_paths(branch, &original).await?, vec!["base"]);
        }
        assert_eq!(
            branch_source_paths(&current_named, &original).await?,
            vec!["named"]
        );
        assert_eq!(inherited.inherited_target(), Some("feature"));
        assert_eq!(current_named.physical_branch(), Some("feature"));
        Ok(())
    }
}

#[tokio::test]
async fn lost_real_storage_is_sdk_error_with_cause_and_never_missing() -> TestResult {
    let temp = TempDir::new()?;
    let store = create(&temp, "source.lance", &[("file", Some("raw text"))]).await;
    let snapshot: ModuleSnapshot = store.snapshot();
    // Remove only a real data file in this fixture; the manifest stays present.
    // A retained Dataset handle is not a physical pin against file removal.
    let root = temp.path().join("source.lance");
    let native = lance::Dataset::open(root.to_str().unwrap()).await?;
    assert_eq!(native.version_id(), snapshot.version());
    let fragments = native.get_fragments();
    let file = fragments.first().unwrap().metadata().files.first().unwrap();
    assert_eq!(file.base_id, None);
    let victim = root.join("data").join(&file.path);
    assert!(victim.is_file());
    fs::remove_file(victim)?;
    let error = read_indexed_source(&snapshot, "file", &request())
        .await
        .err()
        .unwrap();
    assert!(matches!(error, IndexedSourceError::Sdk(_)));
    assert!(error.source().is_some());
    Ok(())
}

#[tokio::test]
async fn default_and_inherited_branch_reads_never_materialize() -> TestResult {
    let temp = TempDir::new()?;
    let raw = "\u{feff}A\r\n🦀\0\n";
    let module = create(&temp, "source.lance", &[("file", Some(raw))]).await;
    let uri = temp.path().join("source.lance");
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
    let native = lance::Dataset::open(uri.to_str().unwrap()).await?;
    assert!(matches!(
        native.branches().get("unmaterialized").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    let original = request();
    let default = store.capture_target(None).await?;
    let inherited = store.capture_target(Some("unmaterialized")).await?;
    for (snapshot, observation) in [
        (
            &default,
            BranchSourceObservation {
                physical_branch: None,
                inherited_target: None,
            },
        ),
        (
            &inherited,
            BranchSourceObservation {
                physical_branch: None,
                inherited_target: Some("unmaterialized"),
            },
        ),
    ] {
        let result = read_branch_source(snapshot, "file", &original).await?;
        assert_eq!(result.source(&original)?, IndexedSource::Available(raw));
        assert_eq!(result.version(), snapshot.version());
        assert_eq!(result.branch_observation(), Some(observation));
        assert_eq!(
            inspect_source(
                result.source(&original)?,
                None,
                &Request::default(),
                &original
            )?
            .origin,
            SourceOrigin::Indexed
        );
    }
    assert_eq!(module.snapshot().version(), 1);
    assert!(matches!(
        native.branches().get("unmaterialized").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn named_branch_and_old_captures_keep_their_selected_generations() -> TestResult {
    let temp = TempDir::new()?;
    let base = create(&temp, "source.lance", &[("same", Some("A"))]).await;
    let uri = temp.path().join("source.lance");
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
    let old_default = store.capture_target(None).await?;
    let old_inherited = store.capture_target(Some("feature")).await?;
    // Explicit fixture publication; the Source reader never materializes.
    let old_named = old_inherited.clone().materialize().await?;
    let next_named = old_named
        .prepare_merge(vec![batch(&[("same", Some("B"))])], vec!["path".into()])
        .await?
        .commit()
        .await?;
    base.snapshot()
        .prepare_replace(vec![batch(&[("same", Some("C"))])])
        .await?
        .commit()
        .await?;
    let next_default = store.capture_target(None).await?;
    let reopened_named = store.capture_target(Some("feature")).await?;
    let original = request();
    for (snapshot, text) in [
        (&old_default, "A"),
        (&old_inherited, "A"),
        (&old_named, "A"),
        (&next_named, "B"),
        (&reopened_named, "B"),
        (&next_default, "C"),
    ] {
        let result = read_branch_source(snapshot, "same", &original).await?;
        assert_eq!(result.source(&original)?, IndexedSource::Available(text));
        assert_eq!(result.version(), snapshot.version());
    }
    assert_eq!(old_named.version(), 1);
    assert_eq!(next_named.version(), 2);
    assert_eq!(next_default.version(), 2);
    let retained = read_branch_source(&next_named, "same", &original).await?;
    assert_eq!(
        retained.branch_observation(),
        Some(BranchSourceObservation {
            physical_branch: Some("feature"),
            inherited_target: None
        })
    );
    drop(store);
    drop(next_named);
    drop(reopened_named);
    assert_eq!(retained.source(&original)?, IndexedSource::Available("B"));
    Ok(())
}

#[tokio::test]
async fn branch_decoder_keeps_literal_raw_absence_and_schema_errors() -> TestResult {
    let temp = TempDir::new()?;
    let literal = "a' OR true -- \\_/% 🦀";
    create(
        &temp,
        "source.lance",
        &[
            (literal, Some("\u{feff}raw\r\n\0")),
            ("empty", Some("")),
            ("off", None),
            ("duplicate", Some("one")),
            ("duplicate", None),
        ],
    )
    .await;
    let uri = temp.path().join("source.lance");
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
    let snapshot = store.capture_target(Some("inherited")).await?;
    let original = request();
    for (path, expected, missing) in [
        (literal, IndexedSource::Available("\u{feff}raw\r\n\0"), None),
        ("empty", IndexedSource::Available(""), None),
        (
            "off",
            IndexedSource::Missing,
            Some(MissingSourceReason::TextNotStored),
        ),
        (
            "absent",
            IndexedSource::Missing,
            Some(MissingSourceReason::FileAbsent),
        ),
    ] {
        let result = read_branch_source(&snapshot, path, &original).await?;
        assert_eq!(result.source(&original)?, expected);
        assert_eq!(result.missing_reason(), missing);
    }
    assert!(matches!(
        read_branch_source(&snapshot, "duplicate", &original).await,
        Err(IndexedSourceError::DuplicatePath)
    ));
    for path in ["", "bad\0path"] {
        assert!(matches!(
            read_branch_source(&snapshot, path, &original).await,
            Err(IndexedSourceError::InvalidPath)
        ));
    }
    let wrong_schema = Arc::new(Schema::new(source_schema().fields().clone()));
    let wrong_uri = temp.path().join("wrong.lance");
    LanceModuleStore::create(
        wrong_uri.to_str().unwrap(),
        Arc::clone(&wrong_schema),
        vec![],
    )
    .await?;
    let wrong_store =
        LanceBranchStore::open(wrong_uri.to_str().unwrap(), wrong_schema, None).await?;
    assert!(matches!(
        read_branch_source(&wrong_store.capture_target(None).await?, "file", &original).await,
        Err(IndexedSourceError::SchemaMismatch)
    ));
    Ok(())
}

#[tokio::test]
async fn branch_read_requires_original_identity_and_request_checks() -> TestResult {
    let temp = TempDir::new()?;
    create(&temp, "source.lance", &[("file", Some("text"))]).await;
    let uri = temp.path().join("source.lance");
    let store = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
    let snapshot = store.capture_target(None).await?;
    let original = request();
    let result = read_branch_source(&snapshot, "file", &original).await?;
    assert!(matches!(
        result.source(&request()),
        Err(IndexedSourceError::RequestMismatch)
    ));
    original.cancellation().cancel();
    operation(
        result.source(&original).unwrap_err(),
        io::ErrorKind::Interrupted,
    );
    operation(
        read_branch_source(&snapshot, "file", &original)
            .await
            .err()
            .unwrap(),
        io::ErrorKind::Interrupted,
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    operation(
        read_branch_source(&snapshot, "file", &expired)
            .await
            .err()
            .unwrap(),
        io::ErrorKind::TimedOut,
    );
    Ok(())
}
