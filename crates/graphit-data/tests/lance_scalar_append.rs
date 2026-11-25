use arrow_array::{RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, DeltaResolution, LanceBranchStore, col, lit},
    lance_module_store::{LanceModuleStore, RecordBatchStreamAdapter},
};
use lance::{
    Dataset,
    dataset::{
        CommitBuilder, WriteParams,
        transaction::{Operation, TransactionBuilder},
    },
    index::{CreateIndexBuilder, DatasetIndexInternalExt},
};
use lance_index::{
    IndexType,
    scalar::{BuiltinIndexType, ScalarIndexParams},
};
use lance_table::{format::IndexMetadata, io::manifest::read_manifest_indexes};
use std::{collections::BTreeSet, error::Error, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("source", DataType::Utf8, false),
    ]))
}
fn rows(values: &[(&str, &str)]) -> RecordBatch {
    RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(StringArray::from(
                values.iter().map(|row| row.0).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                values.iter().map(|row| row.1).collect::<Vec<_>>(),
            )),
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
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?,
    )
}
async fn lookup(pin: &BranchSnapshot, path: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut scan = pin.scan();
    scan.filter_expr(col("path").eq(lit(path)));
    scan.project(&["source"])?;
    let mut stream = scan.try_into_stream().await?;
    let mut values = Vec::new();
    while let Some(batch) = stream.try_next().await? {
        let strings = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        values.extend(strings.iter().flatten().map(str::to_owned));
    }
    Ok(values)
}

#[tokio::test]
async fn deferred_scalar_refresh_preserves_unindexed_updates_deletes_and_named_pins() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("source.lance");
    let uri = uri.to_str().unwrap();
    LanceModuleStore::create(uri, schema(), vec![rows(&[("a.rs", "alpha")])]).await?;
    let store = LanceBranchStore::open(uri, schema(), None).await?;
    let first = store
        .capture_target(None)
        .await?
        .prepare_scalar_append("path", "path_source")
        .await?
        .unwrap()
        .commit()
        .await?;
    let inherited = store.capture_target(Some("unmaterialized-source")).await?;
    assert!(
        inherited
            .prepare_scalar_append("path", "path_source")
            .await?
            .is_none()
    );
    assert_eq!(store.capture_target(None).await?.version(), first.version());
    let mut current = first.clone();
    for (path, source) in [
        ("b.rs", "beta"),
        ("c.rs", "gamma"),
        ("d/ação_%_.rs", "delta"),
    ] {
        let before = current.version();
        current = current
            .prepare_merge(vec![rows(&[(path, source)])], vec!["path".into()])
            .await?
            .commit()
            .await?;
        assert_eq!(
            current.version(),
            before + 1,
            "only Source rows publish before maintenance"
        );
        assert_eq!(lookup(&current, path).await?, [source]);
        assert_eq!(indices(uri).await?.len(), 1);
    }
    assert_eq!(
        Dataset::open(uri)
            .await?
            .unindexed_fragments("path_source")
            .await?
            .len(),
        3
    );
    let pending = current
        .prepare_scalar_append("path", "path_source")
        .await?
        .unwrap();
    let key = pending.recovery_key().clone();
    assert_eq!(
        store.capture_target(None).await?.version(),
        current.version()
    );
    current = pending.commit().await?;
    assert_eq!(
        current.resolve(&key).await?,
        DeltaResolution::Applied {
            version: current.version()
        }
    );
    assert!(
        Dataset::open(uri)
            .await?
            .unindexed_fragments("path_source")
            .await?
            .is_empty()
    );
    assert_eq!(
        indices(uri).await?.len(),
        2,
        "one old and one batched segment have different sizes"
    );
    let mut indexed_lookup = current.scan();
    indexed_lookup.filter_expr(col("path").eq(lit("b.rs")));
    assert!(
        indexed_lookup
            .explain_plan(false)
            .await?
            .contains("ScalarIndexQuery")
    );
    assert!(
        current
            .prepare_scalar_append("path", "path_source")
            .await?
            .is_none()
    );
    assert!(
        current
            .prepare_scalar_append("source", "path_source")
            .await
            .is_err()
    );
    let pinned = current.clone();
    current = current
        .prepare_merge(vec![rows(&[("b.rs", "updated")])], vec!["path".into()])
        .await?
        .commit()
        .await?;
    let empty = RecordBatch::new_empty(schema());
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        schema(),
        futures::stream::iter([Ok(empty)]),
    ));
    current = current
        .prepare_replace_paths_stream(&["c.rs".into()], stream, vec!["path".into()])
        .await?
        .commit()
        .await?;
    assert_eq!(lookup(&current, "b.rs").await?, ["updated"]);
    assert!(lookup(&current, "c.rs").await?.is_empty());
    current = current
        .prepare_scalar_append("path", "path_source")
        .await?
        .unwrap()
        .commit()
        .await?;
    assert_eq!(lookup(&current, "b.rs").await?, ["updated"]);
    assert!(lookup(&current, "c.rs").await?.is_empty());
    assert_eq!(lookup(&pinned, "b.rs").await?, ["beta"]);
    assert_eq!(lookup(&pinned, "c.rs").await?, ["gamma"]);
    assert!(lookup(&first, "b.rs").await?.is_empty());
    let main_version = current.version();
    let branch = store
        .capture_target(Some("source-topic"))
        .await?
        .materialize()
        .await?;
    let branch = branch
        .prepare_merge(vec![rows(&[("branch.rs", "branch")])], vec!["path".into()])
        .await?
        .commit()
        .await?;
    let branch = branch
        .prepare_scalar_append("path", "path_source")
        .await?
        .unwrap()
        .commit()
        .await?;
    assert_eq!(lookup(&branch, "branch.rs").await?, ["branch"]);
    assert!(lookup(&current, "branch.rs").await?.is_empty());
    assert_eq!(store.capture_target(None).await?.version(), main_version);
    Ok(())
}

#[tokio::test]
async fn covered_scalar_backlog_merges_indices_without_rewriting_source_or_history() -> TestResult {
    const FILES: u32 = 8;
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("source.lance");
    let uri = uri.to_str().unwrap();
    let paths = (0..FILES)
        .map(|id| format!("file{id}.rs"))
        .collect::<Vec<_>>();
    let sources = paths
        .iter()
        .map(|path| format!("source {path}"))
        .collect::<Vec<_>>();
    let batch = RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(StringArray::from(paths.clone())),
            Arc::new(StringArray::from(sources.clone())),
        ],
    )?;
    let mut dataset = Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], schema()),
        uri,
        Some(WriteParams {
            max_rows_per_file: 1,
            max_rows_per_group: 1,
            ..Default::default()
        }),
    )
    .await?;
    let data_files = dataset
        .fragments()
        .iter()
        .flat_map(|fragment| fragment.files.iter().map(|file| file.path.clone()))
        .collect::<BTreeSet<_>>();
    let params = ScalarIndexParams::for_builtin(BuiltinIndexType::BTree);
    let mut segments = Vec::new();
    for fragment in 0..FILES {
        segments.push(
            CreateIndexBuilder::new(&mut dataset, &["path"], IndexType::BTree, &params)
                .name("path_source".into())
                .replace(true)
                .fragments(vec![fragment])
                .execute_uncommitted()
                .await?,
        );
    }
    let tx = TransactionBuilder::new(
        dataset.version_id(),
        Operation::CreateIndex {
            new_indices: segments,
            removed_indices: vec![],
        },
    )
    .build();
    CommitBuilder::new(Arc::new(dataset)).execute(tx).await?;
    assert_eq!(indices(uri).await?.len(), FILES as usize);
    let store = LanceBranchStore::open(uri, schema(), None).await?;
    let old = store.capture_target(None).await?;
    let inherited = store.capture_target(Some("unmaterialized-backlog")).await?;
    assert!(
        inherited
            .prepare_scalar_append("path", "path_source")
            .await?
            .is_none()
    );
    let old_dirs = index_directories(uri)?;
    let pending = old
        .prepare_scalar_append("path", "path_source")
        .await?
        .expect("covered segments still need consolidation");
    assert_eq!(indices(uri).await?.len(), FILES as usize);
    let current = pending.commit().await?;
    let metadata = indices(uri).await?;
    assert_eq!(metadata.len(), 1);
    let coverage = metadata
        .iter()
        .flat_map(|index| index.fragment_bitmap.as_ref().unwrap().iter())
        .collect::<BTreeSet<_>>();
    assert_eq!(coverage, (0..FILES).collect());
    let dirs = index_directories(uri)?;
    assert!(
        old_dirs.is_subset(&dirs),
        "committed history remains available"
    );
    let published = metadata
        .iter()
        .map(|index| index.uuid.to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        dirs.difference(&old_dirs).cloned().collect::<BTreeSet<_>>(),
        published,
        "private intermediate merges are removed"
    );
    let actual_data_files = Dataset::open(uri)
        .await?
        .fragments()
        .iter()
        .flat_map(|fragment| fragment.files.iter().map(|file| file.path.clone()))
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_data_files, data_files);
    for (path, source) in paths.iter().zip(sources) {
        assert_eq!(lookup(&old, path).await?, [source.as_str()]);
        assert_eq!(lookup(&current, path).await?, [source]);
    }
    assert!(
        current
            .prepare_scalar_append("path", "path_source")
            .await?
            .is_none()
    );
    assert_eq!(
        store.capture_target(None).await?.version(),
        current.version()
    );
    Ok(())
}

#[tokio::test]
async fn scalar_refresh_keeps_strict_competing_commit_and_recovery_identity() -> TestResult {
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("source.lance");
    let uri = uri.to_str().unwrap();
    LanceModuleStore::create(uri, schema(), vec![rows(&[("a.rs", "alpha")])]).await?;
    let store = LanceBranchStore::open(uri, schema(), None).await?;
    let first = store
        .capture_target(None)
        .await?
        .prepare_scalar_append("path", "path_source")
        .await?
        .unwrap()
        .commit()
        .await?;
    let data = first
        .prepare_merge(vec![rows(&[("b.rs", "beta")])], vec!["path".into()])
        .await?
        .commit()
        .await?;
    let a = data
        .prepare_scalar_append("path", "path_source")
        .await?
        .unwrap();
    let b = data
        .prepare_scalar_append("path", "path_source")
        .await?
        .unwrap();
    let ak = a.recovery_key().clone();
    let bk = b.recovery_key().clone();
    let (a, b) = tokio::join!(a.commit(), b.commit());
    assert_ne!(a.is_ok(), b.is_ok());
    let (winner, loser) = if a.is_ok() { (&ak, &bk) } else { (&bk, &ak) };
    let failure = a.err().or(b.err()).unwrap();
    assert_eq!(failure.recovery_key(), loser);
    assert!(
        matches!(
            failure.sdk_error(),
            lance::Error::CommitConflict { .. } | lance::Error::RetryableCommitConflict { .. }
        ),
        "unexpected losing cause: {failure:?}"
    );
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
    assert_eq!(lookup(&current, "a.rs").await?, ["alpha"]);
    assert_eq!(lookup(&current, "b.rs").await?, ["beta"]);
    assert_eq!(lookup(&first, "a.rs").await?, ["alpha"]);
    assert!(lookup(&first, "b.rs").await?.is_empty());
    Ok(())
}
