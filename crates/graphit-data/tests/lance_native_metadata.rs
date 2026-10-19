use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::lance_module_store::{LanceModuleStore, NativeModuleMetadata};
use lance::{Dataset, index::DatasetIndexExt};
use lance_index::{IndexType, scalar::ScalarIndexParams};
use lance_table::io::manifest::read_manifest_indexes;
use std::{
    error::Error,
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![Field::new("body", DataType::Utf8, false)]))
}
fn image(body: &str) -> RecordBatch {
    RecordBatch::try_new(
        schema(),
        vec![Arc::new(StringArray::from(vec![body])) as ArrayRef],
    )
    .unwrap()
}
fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
async fn indexed(uri: &str) -> Result<Dataset, lance::Error> {
    let mut dataset = Dataset::open(uri).await?;
    dataset
        .create_index_builder(&["body"], IndexType::BTree, &ScalarIndexParams::default())
        .name("body_index".into())
        .replace(true)
        .await?;
    Ok(dataset)
}

#[tokio::test]
async fn metadata_borrows_the_selected_sdk_version_and_raw_indices() -> TestResult {
    let temp = TempDir::new()?;
    let path = temp.path().join("corpus 🦀.lance");
    let uri = path.to_str().unwrap();
    let first_writer = LanceModuleStore::create(uri, schema(), vec![image("first")]).await?;
    let first = first_writer.snapshot();
    let second_writer = first
        .prepare_replace(vec![image("second")])
        .await?
        .commit()
        .await?;
    let second = second_writer.snapshot();
    let sdk = indexed(uri).await?;
    let third = LanceModuleStore::open(uri, schema()).await?.snapshot();
    assert_eq!(
        (first.version(), second.version(), third.version()),
        (1, 2, 3)
    );
    drop(first_writer);
    drop(second_writer);
    for service in [false, true] {
        let original = if service {
            SnapshotContext::service(Cancellation::default())
        } else {
            original()
        };
        let one = first.native_metadata(&original).await?;
        let two = second.native_metadata(&original).await?;
        let three = third.native_metadata(&original).await?;
        assert_eq!(one.uri(), uri);
        assert_eq!(one.manifest().version, 1);
        assert_eq!(two.manifest().version, 2);
        assert_eq!(three.manifest().version, 3);
        assert_eq!(one.manifest_location().version, one.manifest().version);
        assert_eq!(two.manifest_location().version, two.manifest().version);
        assert_eq!(three.manifest_location().version, three.manifest().version);
        assert_ne!(one.manifest_location().path, two.manifest_location().path);
        assert_ne!(two.manifest_location().path, three.manifest_location().path);
        let files = |metadata: &NativeModuleMetadata<'_>| {
            metadata
                .manifest()
                .fragments
                .iter()
                .flat_map(|fragment| fragment.referenced_lance_files())
                .map(|file| file.path.clone())
                .collect::<Vec<_>>()
        };
        assert_ne!(files(&one), files(&two));
        assert!(one.indices().is_empty());
        assert!(two.indices().is_empty());
        assert_eq!(three.indices().len(), 1);
        assert_eq!(three.indices()[0].name, "body_index");
        let store = sdk.object_store(None).await?;
        let raw = read_manifest_indexes(&store, sdk.manifest_location(), sdk.manifest()).await?;
        assert_eq!(three.indices()[0].uuid, raw[0].uuid);
        assert_eq!(three.indices()[0].files, raw[0].files);
        assert_eq!(three.manifest().fragments, sdk.manifest().fragments);
        assert_eq!(three.manifest_location().path, sdk.manifest_location().path);
    }
    Ok(())
}

#[tokio::test]
async fn original_cancellation_and_deadline_apply_to_the_no_index_fast_path() -> TestResult {
    let temp = TempDir::new()?;
    let path = temp.path().join("corpus.lance");
    let snapshot = LanceModuleStore::create(path.to_str().unwrap(), schema(), vec![image("body")])
        .await?
        .snapshot();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(60), cancellation);
    assert_eq!(
        snapshot
            .native_metadata(&cancelled)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let service = SnapshotContext::service(Cancellation::default());
    assert!(
        snapshot
            .native_metadata(&service)
            .await?
            .indices()
            .is_empty()
    );
    service.cancellation().cancel();
    assert_eq!(
        snapshot.native_metadata(&service).await.unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        snapshot.native_metadata(&expired).await.unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(
        snapshot
            .native_metadata(&original())
            .await?
            .indices()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn missing_physical_index_manifest_preserves_the_actual_sdk_cause() -> TestResult {
    let temp = TempDir::new()?;
    let path = temp.path().join("corpus.lance");
    let uri = path.to_str().unwrap();
    LanceModuleStore::create(uri, schema(), vec![image("body")]).await?;
    let sdk = indexed(uri).await?;
    let snapshot = LanceModuleStore::open(uri, schema()).await?.snapshot();
    assert!(sdk.manifest().index_section.is_some());
    sdk.object_store(None)
        .await?
        .delete(&sdk.manifest_location().path)
        .await?;
    let error = snapshot.native_metadata(&original()).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(
        error
            .get_ref()
            .unwrap()
            .downcast_ref::<lance::Error>()
            .is_some()
    );
    assert!(error.source().is_some());
    Ok(())
}
