use arrow_array::{ArrayRef, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::lance_branch_store::{BranchSnapshot, LanceBranchStore};
use graphit_data::lance_module_store::LanceModuleStore;
use lance::{Dataset, dataset::scanner::Scanner, index::DatasetIndexExt};
use lance_index::{IndexType, scalar::ScalarIndexParams};
use std::{
    collections::HashMap,
    error::Error,
    io,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new_with_metadata(
        vec![
            Field::new("path", DataType::Utf8, false),
            Field::new("source", DataType::Utf8, true),
        ],
        HashMap::from([
            ("graphit.module".into(), "ast-source".into()),
            ("graphit.ast.source.format".into(), "1".into()),
        ]),
    ))
}
fn batch(values: &[(&str, &str)]) -> RecordBatch {
    RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(StringArray::from(
                values.iter().map(|v| v.0).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(
                values.iter().map(|v| v.1).collect::<Vec<_>>(),
            )) as ArrayRef,
        ],
    )
    .unwrap()
}
async fn values(dataset: &Dataset) -> Result<Vec<(String, String)>, lance::Error> {
    let batches: Vec<RecordBatch> = dataset
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    let mut result = Vec::new();
    for batch in batches {
        let paths = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let bodies = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for i in 0..batch.num_rows() {
            result.push((paths.value(i).to_owned(), bodies.value(i).to_owned()));
        }
    }
    result.sort();
    Ok(result)
}
async fn selected_branch_sdk(file_url: bool) -> TestResult {
    let root = TempDir::new()?;
    let corpus = root.path().join("source % 🦀.lance");
    let uri = if file_url {
        url::Url::from_file_path(&corpus).unwrap().to_string()
    } else {
        corpus.to_str().unwrap().to_owned()
    };
    let main = LanceModuleStore::create(
        &uri,
        schema(),
        vec![batch(&[("inherited", "main one"), ("replaced", "old")])],
    )
    .await?;
    let branches = LanceBranchStore::open(&uri, schema(), None).await?;
    let named = branches
        .capture_target(Some("dev"))
        .await?
        .materialize()
        .await?;
    let named = named
        .prepare_merge(
            vec![batch(&[
                ("replaced", "branch changed"),
                ("branch-only", "branch"),
            ])],
            vec!["path".into()],
        )
        .await?
        .commit()
        .await?;
    let _new_main = main
        .snapshot()
        .prepare_replace(vec![batch(&[("main-only", "main two")])])
        .await?
        .commit()
        .await?;
    let mut selected = Dataset::open(&uri).await?.checkout_branch("dev").await?;
    assert_eq!(selected.version_id(), named.version());
    assert_eq!(selected.manifest().branch.as_deref(), Some("dev"));
    assert!(!selected.manifest().base_paths.is_empty());
    eprintln!(
        "SDK selected branch URI={}; version={}; base_paths={:?}",
        selected.uri(),
        selected.version_id(),
        selected.manifest().base_paths
    );
    let expected = values(&selected).await?;
    assert_eq!(expected.len(), 3);
    if file_url {
        let rejected = TempDir::new_in(root.path())?;
        let error = selected
            .deep_clone(
                rejected.path().to_str().unwrap(),
                selected.version_id(),
                None,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("empty path segment"), "{error}");
        assert_eq!(values(&selected).await?, expected);
        return Ok(());
    }
    let stage = TempDir::new_in(root.path())?;
    let target = stage.path().to_str().unwrap();
    let version = selected.version_id();
    let cloned = selected.deep_clone(target, version, None).await?;
    assert_eq!(values(&cloned).await?, expected);
    assert!(cloned.manifest().base_paths.is_empty());
    assert_eq!(Schema::from(cloned.schema()), *schema());
    assert_eq!(values(&selected).await?, expected);
    assert_eq!(
        values(&Dataset::open(&uri).await?).await?,
        vec![("main-only".into(), "main two".into())]
    );
    // The consumer must not depend on any original file, even through a base.
    std::fs::rename(&corpus, root.path().join("producer-offline"))?;
    assert_eq!(values(&Dataset::open(cloned.uri()).await?).await?, expected);
    Ok(())
}
#[tokio::test]
async fn official_sdk_reads_file_uri_branches_but_clone_requires_native_bases() -> TestResult {
    selected_branch_sdk(true).await
}

#[tokio::test]
async fn mature_sdk_clones_selected_plain_path_branch_with_local_inherited_and_own_data()
-> TestResult {
    selected_branch_sdk(false).await
}

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}
async fn row_ids(mut scanner: Scanner) -> Result<Vec<(String, String, u64)>, lance::Error> {
    scanner.with_row_id();
    let batches: Vec<RecordBatch> = scanner.try_into_stream().await?.try_collect().await?;
    let mut rows = Vec::new();
    for batch in batches {
        let paths = batch
            .column_by_name("path")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let sources = batch
            .column_by_name("source")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let ids = batch
            .column_by_name("_rowid")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        for i in 0..batch.num_rows() {
            rows.push((
                paths.value(i).to_owned(),
                sources.value(i).to_owned(),
                ids.value(i),
            ));
        }
    }
    rows.sort();
    Ok(rows)
}
async fn assert_data_clone(pin: &BranchSnapshot, parent: &Path, indexed: bool) -> TestResult {
    let before = row_ids(pin.scan()).await?;
    let ctx = original();
    let stage = TempDir::new_in(parent)?;
    let path = stage.path().to_owned();
    let cloned = pin
        .spawn_native_clone_local(stage, Arc::clone(&ctx))?
        .await??;
    let metadata = cloned.native_metadata(&ctx).await?;
    assert!(metadata.manifest().base_paths.is_empty());
    assert!(metadata.manifest().branch.is_none());
    assert_eq!(
        metadata.manifest().version,
        metadata.manifest_location().version
    );
    if indexed {
        assert!(
            metadata
                .indices()
                .iter()
                .any(|i| i.name == "path_index" && i.base_id.is_none())
        );
    }
    let output = LanceModuleStore::open(metadata.uri(), Arc::new(pin.schema().clone()))
        .await?
        .snapshot();
    assert_eq!(row_ids(output.scan()).await?, before);
    assert_eq!(output.schema(), pin.schema());
    assert_eq!(row_ids(pin.scan()).await?, before);
    drop(output);
    drop(cloned);
    assert!(!path.exists());
    Ok(())
}
#[tokio::test]
async fn data_clones_captured_named_pins_with_inherited_deletion_index_and_own_rows() -> TestResult
{
    let root = TempDir::new()?;
    let corpus = root.path().join("source 🦀.lance");
    let uri = corpus.to_str().unwrap().to_owned();
    LanceModuleStore::create(
        &uri,
        schema(),
        vec![batch(&[
            ("inherited", "main"),
            ("replaced", "old"),
            ("deleted", "gone"),
        ])],
    )
    .await?;
    let mut sdk = Dataset::open(&uri).await?;
    sdk.delete("path = 'deleted'").await?;
    sdk.create_index_builder(&["path"], IndexType::BTree, &ScalarIndexParams::default())
        .name("path_index".into())
        .await?;
    let branches = LanceBranchStore::open(&uri, schema(), None).await?;
    let first = branches
        .capture_target(Some("dev"))
        .await?
        .materialize()
        .await?;
    assert_eq!(first.version(), sdk.version_id());
    let captured = Dataset::open(&uri).await?.checkout_branch("dev").await?;
    assert!(captured.manifest().fragments.iter().any(|f| {
        f.deletion_file
            .as_ref()
            .is_some_and(|d| d.base_id.is_some())
    }));
    assert!(
        captured
            .load_indices()
            .await?
            .iter()
            .any(|i| i.base_id.is_some())
    );
    let next = first
        .prepare_merge(
            vec![batch(&[("replaced", "changed"), ("branch-only", "branch")])],
            vec!["path".into()],
        )
        .await?
        .commit()
        .await?;
    assert_eq!(next.version(), first.version() + 1);
    let _advanced_main = LanceModuleStore::open(&uri, schema())
        .await?
        .snapshot()
        .prepare_replace(vec![batch(&[("main-only", "new")])])
        .await?
        .commit()
        .await?;
    assert_data_clone(&first, root.path(), true).await?;
    assert_data_clone(&next, root.path(), true).await?;
    assert_eq!(row_ids(first.scan()).await?.len(), 2);
    assert_eq!(row_ids(next.scan()).await?.len(), 3);
    assert_eq!(first.physical_branch(), Some("dev"));
    assert_eq!(next.physical_branch(), Some("dev"));
    assert_eq!(
        values(&Dataset::open(&uri).await?).await?,
        vec![("main-only".into(), "new".into())]
    );
    assert_eq!(
        Dataset::open(&uri)
            .await?
            .checkout_branch("dev")
            .await?
            .version_id(),
        next.version()
    );
    Ok(())
}
#[tokio::test]
async fn data_clones_default_and_inherited_pins_without_materializing_or_adopting_main()
-> TestResult {
    let root = TempDir::new()?;
    let uri = url::Url::from_file_path(root.path().join("source.lance"))
        .unwrap()
        .to_string();
    let main =
        LanceModuleStore::create(&uri, schema(), vec![batch(&[("old", "captured")])]).await?;
    let branches = LanceBranchStore::open(&uri, schema(), None).await?;
    let default = branches.capture_target(None).await?;
    let inherited = branches.capture_target(Some("not-created")).await?;
    assert_eq!(inherited.inherited_target(), Some("not-created"));
    let _advanced = main
        .snapshot()
        .prepare_replace(vec![batch(&[("new", "latest")])])
        .await?
        .commit()
        .await?;
    assert_data_clone(&default, root.path(), false).await?;
    assert_data_clone(&inherited, root.path(), false).await?;
    assert_eq!((default.version(), inherited.version()), (1, 1));
    assert_eq!(inherited.inherited_target(), Some("not-created"));
    assert_eq!(inherited.physical_branch(), None);
    assert!(matches!(
        Dataset::open(&uri)
            .await?
            .branches()
            .get("not-created")
            .await,
        Err(lance::Error::RefNotFound { .. })
    ));
    Ok(())
}
#[tokio::test]
async fn data_clones_sdk_captured_absolute_bases_from_canonical_parent() -> TestResult {
    // Product callers resolve project-relative selections under an admitted
    // canonical parent. The SDK does not normalize dot segments in clone bases.
    let source = TempDir::new_in(".")?;
    let corpus = source.path().canonicalize()?.join("relative.lance");
    let uri = corpus.to_str().unwrap();
    LanceModuleStore::create(uri, schema(), vec![batch(&[("one", "body")])]).await?;
    let pin = LanceBranchStore::open(uri, schema(), None)
        .await?
        .capture_target(Some("dev"))
        .await?
        .materialize()
        .await?;
    let sdk = Dataset::open(uri).await?.checkout_branch("dev").await?;
    assert!(
        sdk.manifest()
            .base_paths
            .values()
            .all(|base| Path::new(&base.path).is_absolute() || url::Url::parse(&base.path).is_ok())
    );
    let output_parent = TempDir::new()?;
    assert_data_clone(&pin, output_parent.path(), false).await?;
    Ok(())
}
#[tokio::test]
async fn data_clones_empty_pin_with_unreferenced_absent_base() -> TestResult {
    let source = TempDir::new()?;
    let corpus = source.path().join("source.lance");
    let uri = url::Url::from_file_path(&corpus).unwrap().to_string();
    LanceModuleStore::create(&uri, schema(), vec![batch(&[("one", "body")])]).await?;
    let mut sdk = Dataset::open(&uri).await?;
    let target = TempDir::new()?;
    let shallow = target.path().join("shallow.lance");
    let shallow_uri = url::Url::from_file_path(&shallow).unwrap().to_string();
    sdk.shallow_clone(&shallow_uri, sdk.version_id(), None)
        .await?;
    let empty = LanceModuleStore::open(&shallow_uri, schema())
        .await?
        .snapshot()
        .prepare_replace(vec![RecordBatch::new_empty(schema())])
        .await?
        .commit()
        .await?;
    let empty = empty.snapshot();
    let metadata = empty.native_metadata(&original()).await?;
    assert!(!metadata.manifest().base_paths.is_empty());
    assert!(metadata.manifest().fragments.is_empty());
    assert!(metadata.indices().is_empty());
    drop(sdk);
    drop(source);
    let pin = LanceBranchStore::open(&shallow_uri, schema(), None)
        .await?
        .capture_target(None)
        .await?;
    assert_data_clone(&pin, target.path(), false).await?;
    let cancelled = original();
    cancelled.cancellation().cancel();
    assert_eq!(
        pin.spawn_native_clone_local(TempDir::new()?, cancelled)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    Ok(())
}

#[tokio::test]
async fn data_rejects_real_foreign_shallow_base_without_copying() -> TestResult {
    let source = TempDir::new()?;
    let corpus = source.path().join("source.lance");
    let uri = url::Url::from_file_path(&corpus).unwrap().to_string();
    LanceModuleStore::create(
        &uri,
        schema(),
        vec![batch(&[("one", "deleted"), ("two", "body")])],
    )
    .await?;
    let mut sdk = Dataset::open(&uri).await?;
    sdk.delete("path = 'one'").await?;
    sdk.create_index_builder(&["path"], IndexType::BTree, &ScalarIndexParams::default())
        .name("foreign_path_index".into())
        .await?;
    let foreign = source.path().join("foreign.lance");
    let foreign_uri = url::Url::from_file_path(&foreign).unwrap().to_string();
    sdk.shallow_clone(&foreign_uri, sdk.version_id(), None)
        .await?;
    let pin = LanceBranchStore::open(&foreign_uri, schema(), None)
        .await?
        .capture_target(None)
        .await?;
    let captured = LanceModuleStore::open(&foreign_uri, schema())
        .await?
        .snapshot();
    let metadata = captured.native_metadata(&original()).await?;
    assert!(
        metadata
            .manifest()
            .fragments
            .iter()
            .flat_map(|fragment| fragment.referenced_lance_files())
            .any(|file| file.base_id.is_some())
    );
    assert!(metadata.manifest().fragments.iter().any(|fragment| {
        fragment
            .deletion_file
            .as_ref()
            .is_some_and(|file| file.base_id.is_some())
    }));
    assert!(
        metadata
            .indices()
            .iter()
            .any(|index| index.base_id.is_some())
    );
    let stage = TempDir::new()?;
    let path = stage.path().to_owned();
    let result = pin.spawn_native_clone_local(stage, original())?.await?;
    assert!(matches!(result, Err(ref e) if e.kind() == io::ErrorKind::Unsupported));
    assert!(!path.exists());
    assert_eq!(row_ids(pin.scan()).await?.len(), 1);
    Ok(())
}
