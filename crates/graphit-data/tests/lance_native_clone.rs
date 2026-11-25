use arrow_array::{
    ArrayRef, Int64Array, RecordBatch, RecordBatchIterator, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::lance_module_store::{LanceModuleStore, ModuleSnapshot, NativeModuleClone};
use lance::{Dataset, dataset::WriteParams, index::DatasetIndexExt};
use lance_index::{IndexType, scalar::ScalarIndexParams};
use std::{
    error::Error,
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("body", DataType::Utf8, false),
    ]))
}
fn image() -> RecordBatch {
    RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef,
            Arc::new(StringArray::from(vec!["one", "two", "three"])) as ArrayRef,
        ],
    )
    .unwrap()
}
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}
async fn create(uri: &str) -> Result<Dataset, lance::Error> {
    Dataset::write(
        RecordBatchIterator::new(vec![Ok(image())], schema()),
        uri,
        Some(WriteParams {
            enable_stable_row_ids: true,
            ..Default::default()
        }),
    )
    .await
}
async fn scan(snapshot: &ModuleSnapshot) -> Result<Vec<(i64, u64)>, lance::Error> {
    let mut scan = snapshot.scan();
    scan.with_row_id();
    let batches: Vec<RecordBatch> = scan.try_into_stream().await?.try_collect().await?;
    let mut out = Vec::new();
    for batch in batches {
        let ids = batch
            .column_by_name("id")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        let rowids = batch
            .column_by_name("_rowid")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        for i in 0..batch.num_rows() {
            out.push((ids.value(i), rowids.value(i)));
        }
    }
    out.sort();
    Ok(out)
}
fn error(result: io::Result<NativeModuleClone>) -> io::Error {
    match result {
        Ok(_) => panic!("clone unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn clones_selected_pins_with_rowids_deletion_index_and_owned_cleanup() -> TestResult {
    let source = TempDir::new()?;
    let corpus = source.path().join("source 🦀.lance");
    let uri = corpus.to_str().unwrap();
    let mut sdk = create(uri).await?;
    let first = LanceModuleStore::open(uri, schema()).await?.snapshot();
    sdk.delete("id = 2").await?;
    sdk.create_index_builder(&["id"], IndexType::BTree, &ScalarIndexParams::default())
        .name("id_index".into())
        .await?;
    let latest = LanceModuleStore::open(uri, schema()).await?.snapshot();
    assert_eq!((first.version(), latest.version()), (1, 3));
    let ctx = original();
    let first_rows = scan(&first).await?;
    let latest_rows = scan(&latest).await?;
    assert_eq!(
        first_rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(latest_rows.iter().map(|r| r.0).collect::<Vec<_>>(), [1, 3]);
    let output_parent = TempDir::new()?;
    for (pin, expected) in [(&first, &first_rows), (&latest, &latest_rows)] {
        let stage = TempDir::new_in(output_parent.path())?;
        let path = stage.path().to_path_buf();
        let handle = pin.spawn_native_clone_local(stage, Arc::clone(&ctx))?;
        let cloned = handle.await??;
        let metadata = cloned.native_metadata(&ctx).await?;
        assert_eq!(
            metadata.manifest_location().version,
            metadata.manifest().version
        );
        let output = LanceModuleStore::open(metadata.uri(), schema())
            .await?
            .snapshot();
        assert_eq!(scan(&output).await?, *expected);
        assert_eq!(output.schema(), schema().as_ref());
        let input = pin.native_metadata(&ctx).await?;
        assert_eq!(metadata.indices().len(), input.indices().len());
        assert_eq!(metadata.manifest().fragments, input.manifest().fragments);
        if let Some(index) = input.indices().first() {
            assert_eq!(metadata.indices()[0].uuid, index.uuid);
            assert_eq!(metadata.indices()[0].name, "id_index");
        }
        assert_eq!(cloned.directory(), path);
        assert!(path.exists());
        drop(output);
        drop(cloned);
        assert!(!path.exists());
    }
    assert_eq!(scan(&first).await?, first_rows);
    assert_eq!(scan(&latest).await?, latest_rows);
    assert_eq!(Dataset::open(uri).await?.version_id(), 3);
    Ok(())
}

#[tokio::test]
async fn nonempty_stage_and_missing_source_file_preserve_errors_and_cleanup() -> TestResult {
    let source = TempDir::new()?;
    let corpus = source.path().join("source.lance");
    let uri = corpus.to_str().unwrap();
    create(uri).await?;
    let pin = LanceModuleStore::open(uri, schema()).await?.snapshot();
    let stage = TempDir::new_in(source.path())?;
    let path = stage.path().to_path_buf();
    std::fs::write(path.join("occupied"), b"owned fixture")?;
    let failure = error(pin.spawn_native_clone_local(stage, original())?.await?);
    assert_eq!(failure.kind(), io::ErrorKind::InvalidInput);
    assert!(!path.exists());
    let ctx = original();
    let metadata = pin.native_metadata(&ctx).await?;
    let data = metadata.manifest().fragments[0]
        .referenced_lance_files()
        .next()
        .unwrap()
        .path
        .clone();
    std::fs::remove_file(corpus.join("data").join(data))?;
    let stage = TempDir::new_in(source.path())?;
    let path = stage.path().to_path_buf();
    let failure = error(pin.spawn_native_clone_local(stage, ctx)?.await?);
    assert_eq!(failure.kind(), io::ErrorKind::Other);
    assert!(
        failure
            .get_ref()
            .unwrap()
            .downcast_ref::<lance::Error>()
            .is_some()
    );
    assert!(failure.source().is_some());
    assert!(!path.exists());
    Ok(())
}

#[tokio::test]
async fn external_base_and_blob_schema_are_unsupported_inputs() -> TestResult {
    let source = TempDir::new()?;
    let corpus = source.path().join("source.lance");
    let uri = corpus.to_str().unwrap();
    let mut sdk = create(uri).await?;
    let shallow = source.path().join("shallow.lance");
    sdk.shallow_clone(shallow.to_str().unwrap(), 1, None)
        .await?;
    let pin = LanceModuleStore::open(shallow.to_str().unwrap(), schema())
        .await?
        .snapshot();
    assert!(
        !pin.native_metadata(&original())
            .await?
            .manifest()
            .base_paths
            .is_empty()
    );
    let stage = TempDir::new_in(source.path())?;
    let path = stage.path().to_path_buf();
    assert_eq!(
        error(pin.spawn_native_clone_local(stage, original())?.await?).kind(),
        io::ErrorKind::Unsupported
    );
    assert!(!path.exists());
    let blob_schema = Arc::new(Schema::new(vec![lance::blob::blob_field("payload", true)]));
    let blob_path = source.path().join("blob.lance");
    let blob = LanceModuleStore::create(
        blob_path.to_str().unwrap(),
        Arc::clone(&blob_schema),
        vec![RecordBatch::new_empty(blob_schema)],
    )
    .await?
    .snapshot();
    let stage = TempDir::new_in(source.path())?;
    let path = stage.path().to_path_buf();
    assert_eq!(
        error(blob.spawn_native_clone_local(stage, original())?.await?).kind(),
        io::ErrorKind::Unsupported
    );
    assert!(!path.exists());
    Ok(())
}

#[tokio::test]
async fn original_rejects_cancelled_or_expired_start_without_copy() -> TestResult {
    let source = TempDir::new()?;
    let corpus = source.path().join("source.lance");
    create(corpus.to_str().unwrap()).await?;
    let pin = LanceModuleStore::open(corpus.to_str().unwrap(), schema())
        .await?
        .snapshot();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    for (context, kind) in [
        (
            SnapshotContext::new(Instant::now() + Duration::from_secs(60), cancellation),
            io::ErrorKind::Interrupted,
        ),
        (
            SnapshotContext::new(Instant::now(), Cancellation::default()),
            io::ErrorKind::TimedOut,
        ),
    ] {
        let stage = TempDir::new_in(source.path())?;
        let path = stage.path().to_path_buf();
        match pin.spawn_native_clone_local(stage, Arc::new(context)) {
            Err(e) => assert_eq!(e.kind(), kind),
            Ok(_) => panic!("invalid Original admitted"),
        };
        assert!(!path.exists());
    }
    Ok(())
}

#[test]
fn queued_worker_retains_stage_until_join_and_observes_original() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()?;
    let source = TempDir::new()?;
    let corpus = source.path().join("source.lance");
    let pin = runtime.block_on(async {
        create(corpus.to_str().unwrap()).await?;
        LanceModuleStore::open(corpus.to_str().unwrap(), schema())
            .await
            .map(|s| s.snapshot())
    })?;
    let stage = TempDir::new_in(source.path())?;
    let path = stage.path().to_path_buf();
    let (started, wait) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        started.send(()).unwrap();
        blocked.recv().unwrap();
    });
    wait.recv()?;
    let context = original();
    let handle = {
        let _enter = runtime.enter();
        pin.spawn_native_clone_local(stage, Arc::clone(&context))?
    };
    context.cancellation().cancel();
    assert!(path.exists());
    release.send(())?;
    runtime.block_on(blocker)?;
    let failure = error(runtime.block_on(handle)?);
    assert_eq!(failure.kind(), io::ErrorKind::Interrupted);
    assert!(!path.exists());
    let stage = TempDir::new_in(source.path())?;
    let path = stage.path().to_path_buf();
    assert!(pin.spawn_native_clone_local(stage, original()).is_err());
    assert!(!path.exists());
    Ok(())
}
