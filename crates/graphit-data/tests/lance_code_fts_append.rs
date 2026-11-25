use arrow_array::{RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, DeltaResolution, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use lance::{
    Dataset,
    dataset::{
        CommitBuilder, WriteParams,
        builder::DatasetBuilder,
        transaction::{Operation, TransactionBuilder},
    },
    index::{CreateIndexBuilder, DatasetIndexExt},
    session::{CacheSpec, Session},
};
use lance_core::cache::MokaCacheBackend;
use lance_index::{
    IndexType,
    scalar::{FullTextSearchQuery, InvertedIndexParams},
};
use lance_table::{format::IndexMetadata, io::manifest::read_manifest_indexes};
use std::{collections::BTreeSet, error::Error, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
    ]))
}
fn rows(id: &str) -> RecordBatch {
    RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(StringArray::from(vec![id])),
            Arc::new(StringArray::from(vec![format!("commonword {id}")])),
        ],
    )
    .unwrap()
}
async fn indices(uri: &str) -> Result<Vec<IndexMetadata>, Box<dyn Error>> {
    let dataset = Dataset::open(uri).await?;
    let store = dataset.object_store(None).await?;
    Ok(read_manifest_indexes(
        store.as_ref(),
        dataset.manifest_location(),
        dataset.manifest(),
    )
    .await?)
}
fn index_directories(uri: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    Ok(
        std::fs::read_dir(std::path::Path::new(uri).join("_indices"))?
            .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?,
    )
}
async fn hits(pin: &BranchSnapshot, query: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut scan = pin.scan();
    scan.project(&["id"])?;
    scan.full_text_search(
        FullTextSearchQuery::new(query.into())
            .with_columns(&["content".into()])?
            .limit(None),
    )?;
    let mut stream = scan.try_into_stream().await?;
    let mut ids = BTreeSet::new();
    while let Some(batch) = stream.try_next().await? {
        let column = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        ids.extend(column.iter().flatten().map(str::to_owned));
    }
    Ok(ids)
}

#[tokio::test]
async fn progressive_posting_merges_preserve_coverage_pins_noop_and_branch() -> TestResult {
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("search.lance");
    let uri = uri.to_str().unwrap();
    LanceModuleStore::create(uri, schema(), vec![rows("file0")]).await?;
    let store = LanceBranchStore::open(uri, schema(), None).await?;
    let initial = store.capture_target(None).await?;
    let first = initial
        .prepare_code_fts_index("content", "code")
        .await?
        .commit()
        .await?;
    let mut current = first.clone();
    let mut expected = BTreeSet::from(["file0".to_owned()]);
    for number in 1..32_u32 {
        let previous = indices(uri).await?;
        let id = format!("file{number}");
        let data = current
            .prepare_merge(vec![rows(&id)], vec!["id".into()])
            .await?
            .commit()
            .await?;
        expected.insert(id.clone());
        // Search is available even before maintenance covers the new fragment.
        assert_eq!(hits(&data, &id).await?, BTreeSet::from([id]));
        let pending = data
            .prepare_code_fts_append("content", "code")
            .await?
            .unwrap();
        let recovery = pending.recovery_key().clone();
        assert_eq!(store.capture_target(None).await?.version(), data.version());
        current = pending.commit().await?;
        assert_eq!(
            current.resolve(&recovery).await?,
            DeltaResolution::Applied {
                version: current.version()
            }
        );
        let metadata = indices(uri).await?;
        // Equal one-fragment appends combine like binary carries. This checks
        // actual SDK segments, not a product resource cap or producer counter.
        assert_eq!(metadata.len(), (number + 1).count_ones() as usize);
        let coverage: BTreeSet<_> = metadata
            .iter()
            .flat_map(|m| m.fragment_bitmap.as_ref().unwrap().iter())
            .collect();
        assert_eq!(coverage, (0..=number).collect());
        if number % 2 == 0 {
            // A small append does not rewrite the larger established postings.
            assert!(
                previous
                    .iter()
                    .all(|old| metadata.iter().any(|new| new.uuid == old.uuid))
            );
        }
        assert_eq!(hits(&current, "commonword").await?, expected);
        assert_eq!(
            hits(&first, "commonword").await?,
            BTreeSet::from(["file0".into()])
        );
        assert!(
            current
                .prepare_code_fts_append("content", "code")
                .await?
                .is_none()
        );
        assert_eq!(
            store.capture_target(None).await?.version(),
            current.version()
        );
    }
    let reopened = LanceBranchStore::open(uri, schema(), None)
        .await?
        .capture_target(None)
        .await?;
    assert_eq!(hits(&reopened, "commonword").await?, expected);
    let branch = store
        .capture_target(Some("other-version"))
        .await?
        .materialize()
        .await?;
    let branch = branch
        .prepare_merge(vec![rows("branchonly")], vec!["id".into()])
        .await?
        .commit()
        .await?;
    let branch = branch
        .prepare_code_fts_append("content", "code")
        .await?
        .unwrap()
        .commit()
        .await?;
    assert_eq!(
        hits(&branch, "branchonly").await?,
        BTreeSet::from(["branchonly".into()])
    );
    assert!(hits(&reopened, "branchonly").await?.is_empty());
    assert_eq!(
        store.capture_target(None).await?.version(),
        current.version()
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_posting_merges_keep_strict_commit_and_recovery_identity() -> TestResult {
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("search.lance");
    let uri = uri.to_str().unwrap();
    LanceModuleStore::create(uri, schema(), vec![rows("first")]).await?;
    let store = LanceBranchStore::open(uri, schema(), None).await?;
    let base = store
        .capture_target(None)
        .await?
        .prepare_code_fts_index("content", "code")
        .await?
        .commit()
        .await?;
    let data = base
        .prepare_merge(vec![rows("second")], vec!["id".into()])
        .await?
        .commit()
        .await?;
    let a = data
        .prepare_code_fts_append("content", "code")
        .await?
        .unwrap();
    let b = data
        .prepare_code_fts_append("content", "code")
        .await?
        .unwrap();
    let ak = a.recovery_key().clone();
    let bk = b.recovery_key().clone();
    let (a, b) = tokio::join!(a.commit(), b.commit());
    assert_ne!(a.is_ok(), b.is_ok());
    let (winner, loser) = if a.is_ok() { (&ak, &bk) } else { (&bk, &ak) };
    let current = store.capture_target(None).await?;
    assert_eq!(current.version(), data.version() + 1);
    assert_eq!(
        current.resolve(winner).await?,
        DeltaResolution::Applied {
            version: current.version()
        }
    );
    assert_eq!(
        current.resolve(loser).await?,
        DeltaResolution::Conflicted {
            version: current.version()
        }
    );
    assert_eq!(indices(uri).await?.len(), 1);
    assert_eq!(
        hits(&current, "commonword").await?,
        BTreeSet::from(["first".into(), "second".into()])
    );
    assert_eq!(
        hits(&base, "commonword").await?,
        BTreeSet::from(["first".into()])
    );
    Ok(())
}

#[tokio::test]
async fn fully_covered_backlog_consolidates_without_opening_all_segments() -> TestResult {
    const FILES: u32 = 535;
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("search.lance");
    let uri = uri.to_str().unwrap();
    let ids: Vec<_> = (0..FILES).map(|n| format!("file{n}")).collect();
    let batch = RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(StringArray::from(ids.clone())),
            Arc::new(StringArray::from(
                ids.iter()
                    .map(|id| format!("commonword {id}"))
                    .collect::<Vec<_>>(),
            )),
        ],
    )?;
    Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], schema()),
        uri,
        Some(WriteParams {
            max_rows_per_file: 1,
            max_rows_per_group: 1,
            ..Default::default()
        }),
    )
    .await?;
    // Reproduce the former FILE-per-segment layout with the same mature SDK.
    // No product compatibility path is retained to build this old layout.
    let session = Session::with_cache_backends(
        CacheSpec::Backend(Arc::new(MokaCacheBackend::no_cache())),
        CacheSpec::Default,
        Default::default(),
    );
    let mut dataset = DatasetBuilder::from_uri(uri)
        .with_session(Arc::new(session))
        .load()
        .await?;
    let params = InvertedIndexParams::code().max_token_length(None);
    let mut segments = Vec::new();
    for fragment in 0..FILES {
        let segment =
            CreateIndexBuilder::new(&mut dataset, &["content"], IndexType::Inverted, &params)
                .name("code".into())
                .replace(true)
                .fragments(vec![fragment])
                .execute_uncommitted()
                .await?;
        segments.push(dataset.merge_existing_index_segments(vec![segment]).await?);
    }
    let tx = TransactionBuilder::new(
        dataset.version_id(),
        Operation::CreateIndex {
            new_indices: segments,
            removed_indices: vec![],
        },
    )
    .build();
    let dataset = CommitBuilder::new(Arc::new(dataset)).execute(tx).await?;
    assert_eq!(indices(uri).await?.len(), FILES as usize);
    drop(dataset);
    let store = LanceBranchStore::open(uri, schema(), None).await?;
    let old = store.capture_target(None).await?;
    let old_files = index_directories(uri)?;
    let pending = old
        .prepare_code_fts_append("content", "code")
        .await?
        .expect("covered backlog still needs consolidation");
    assert_eq!(
        indices(uri).await?.len(),
        FILES as usize,
        "preparation does not publish"
    );
    let current = pending.commit().await?;
    let metadata = indices(uri).await?;
    assert_eq!(metadata.len(), FILES.count_ones() as usize);
    let expected_new: BTreeSet<_> = metadata
        .iter()
        .map(|index| index.uuid.to_string())
        .filter(|uuid| !old_files.contains(uuid))
        .collect();
    assert_eq!(
        index_directories(uri)?
            .difference(&old_files)
            .cloned()
            .collect::<BTreeSet<_>>(),
        expected_new,
        "successful consolidation retains only its published outputs, not private intermediates"
    );
    assert!(
        old_files.is_subset(&index_directories(uri)?),
        "old snapshot files remain intact"
    );
    let coverage: BTreeSet<_> = metadata
        .iter()
        .flat_map(|m| m.fragment_bitmap.as_ref().unwrap().iter())
        .collect();
    assert_eq!(coverage, (0..FILES).collect());
    assert_eq!(
        hits(&current, "commonword").await?,
        ids.into_iter().collect()
    );
    assert!(
        current
            .prepare_code_fts_append("content", "code")
            .await?
            .is_none()
    );
    assert_eq!(
        store.capture_target(None).await?.version(),
        current.version()
    );
    let reopened = LanceBranchStore::open(uri, schema(), None)
        .await?
        .capture_target(None)
        .await?;
    assert_eq!(
        hits(&reopened, "file534").await?,
        BTreeSet::from(["file534".into()])
    );
    assert_eq!(old.version() + 1, current.version());
    Ok(())
}

#[tokio::test]
async fn finalized_staged_postings_survive_fresh_sessions_and_real_corruption_is_error()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("search.lance");
    let uri = uri.to_str().unwrap();
    LanceModuleStore::create(uri, schema(), vec![rows("first")]).await?;
    let store = LanceBranchStore::open(uri, schema(), None).await?;
    let first = store
        .capture_target(None)
        .await?
        .prepare_code_fts_index("content", "code")
        .await?
        .commit()
        .await?;
    let data = first
        .prepare_merge(vec![rows("second")], vec!["id".into()])
        .await?
        .commit()
        .await?;
    let current = data
        .prepare_code_fts_append("content", "code")
        .await?
        .expect("new fragment requires native postings")
        .commit()
        .await?;
    let (fresh, _) =
        BranchSnapshot::capture_native(uri, Arc::new(Session::default()), current.version())
            .await?;
    assert_eq!(
        hits(&fresh, "commonword").await?,
        BTreeSet::from(["first".into(), "second".into()])
    );
    assert_eq!(
        hits(&first, "commonword").await?,
        BTreeSet::from(["first".into()])
    );
    for index in indices(uri).await? {
        assert!(
            std::path::Path::new(uri)
                .join("_indices")
                .join(index.uuid.to_string())
                .join(lance_index::scalar::inverted::METADATA_FILE)
                .is_file(),
            "each published SDK UUID retains its finalized metadata"
        );
    }
    let index = indices(uri).await?.remove(0);
    std::fs::remove_dir_all(
        std::path::Path::new(uri)
            .join("_indices")
            .join(index.uuid.to_string()),
    )?;
    let (uncached, _) =
        BranchSnapshot::capture_native(uri, Arc::new(Session::default()), current.version())
            .await?;
    assert!(
        hits(&uncached, "commonword").await.is_err(),
        "missing committed postings are corruption, not an empty result or fallback"
    );
    Ok(())
}

#[tokio::test]
async fn empty_null_and_tokenless_fragments_keep_native_fts_coverage() -> TestResult {
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("search.lance");
    let uri = uri.to_str().unwrap();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, true),
    ]));
    let row = |id: &str, content: Option<&str>| {
        RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec![id])),
                Arc::new(StringArray::from(vec![content])),
            ],
        )
        .unwrap()
    };
    LanceModuleStore::create(
        uri,
        Arc::clone(&schema),
        vec![RecordBatch::new_empty(Arc::clone(&schema))],
    )
    .await?;
    let store = LanceBranchStore::open(uri, schema.clone(), None).await?;
    let mut current = store
        .capture_target(None)
        .await?
        .prepare_code_fts_index("content", "code")
        .await?
        .commit()
        .await?;
    let mut expected = BTreeSet::new();
    for (id, content) in [
        ("null", None),
        ("empty", Some("")),
        ("word", Some("nebula")),
        ("blank", Some(" \n\t")),
        ("unicode", Some("nebula ação")),
    ] {
        current = current
            .prepare_merge(vec![row(id, content)], vec!["id".into()])
            .await?
            .commit()
            .await?;
        if content.is_some_and(|text| text.contains("nebula")) {
            expected.insert(id.to_owned());
        }
        current = current
            .prepare_code_fts_append("content", "code")
            .await?
            .expect("uncovered fragment is included even without searchable terms")
            .commit()
            .await?;
        let (fresh, _) =
            BranchSnapshot::capture_native(uri, Arc::new(Session::default()), current.version())
                .await?;
        assert_eq!(hits(&fresh, "nebula").await?, expected);
        assert!(
            fresh
                .prepare_code_fts_append("content", "code")
                .await?
                .is_none(),
            "term-free fragments are covered without repeated index work"
        );
        current = fresh;
    }
    assert_eq!(
        hits(&current, "ação").await?,
        BTreeSet::from(["unicode".into()])
    );
    Ok(())
}
