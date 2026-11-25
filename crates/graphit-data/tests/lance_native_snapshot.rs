use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::lance_module_store::{LanceModuleStore, ModuleSnapshot, StoreError};
use lance::session::Session;
use lance_io::object_store::{
    ObjectStore, ObjectStoreParams, ObjectStoreProvider, providers::local::FileStoreProvider,
};
use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tempfile::TempDir;
use url::Url;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, Default)]
struct ObservedFileProvider(AtomicUsize);

#[async_trait::async_trait]
impl ObjectStoreProvider for ObservedFileProvider {
    async fn new_store(
        &self,
        base_path: Url,
        params: &ObjectStoreParams,
    ) -> lance_core::error::Result<ObjectStore> {
        self.0.fetch_add(1, Ordering::Relaxed);
        FileStoreProvider.new_store(base_path, params).await
    }
}

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

async fn bodies(snapshot: &ModuleSnapshot) -> Result<Vec<String>, lance::Error> {
    let batches: Vec<RecordBatch> = snapshot
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    Ok(batches
        .iter()
        .flat_map(|batch| {
            batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .iter()
                .map(|value| value.unwrap().to_owned())
        })
        .collect())
}

fn session() -> (Arc<Session>, Arc<ObservedFileProvider>) {
    let session = Arc::new(Session::default());
    let provider = Arc::new(ObservedFileProvider::default());
    session.store_registry().insert("file", provider.clone());
    (session, provider)
}

#[tokio::test]
async fn explicit_session_opens_pinned_versions_and_returns_sdk_locations() -> TestResult {
    let temp = TempDir::new()?;
    let path = temp.path().join("corpus 🦀.lance");
    let uri = Url::from_file_path(&path).unwrap().to_string();
    let writer = LanceModuleStore::create(&uri, schema(), vec![image("primeiro")]).await?;
    let (session, provider) = session();
    let (first, first_location) =
        LanceModuleStore::open_native_snapshot(&uri, schema(), session.clone(), 1).await?;
    assert!(provider.0.load(Ordering::Relaxed) > 0);
    assert_eq!(first_location.version, 1);
    assert_eq!(first.snapshot().version(), 1);
    assert!(!first_location.path.as_ref().is_empty());

    let second_writer = writer
        .snapshot()
        .prepare_replace(vec![image("segundo")])
        .await?
        .commit()
        .await?;
    let (second, second_location) =
        LanceModuleStore::open_native_snapshot(&uri, schema(), session, 2).await?;
    assert_eq!(second_location.version, 2);
    assert_ne!(first_location.path, second_location.path);
    assert_eq!(bodies(&first.snapshot()).await?, ["primeiro"]);
    assert_eq!(bodies(&second.snapshot()).await?, ["segundo"]);
    let retained = first.snapshot();
    drop(first);
    drop(second_writer);
    assert_eq!(bodies(&retained).await?, ["primeiro"]);
    Ok(())
}

#[tokio::test]
async fn full_schema_mismatch_and_missing_version_preserve_causes() -> TestResult {
    let temp = TempDir::new()?;
    let path = temp.path().join("corpus.lance");
    let uri = Url::from_file_path(&path).unwrap().to_string();
    LanceModuleStore::create(&uri, schema(), vec![image("original")]).await?;
    let (session, _) = session();
    let wrong = Arc::new(Schema::new(vec![Field::new("body", DataType::Utf8, true)]));
    assert!(matches!(
        LanceModuleStore::open_native_snapshot(&uri, wrong, session.clone(), 1).await,
        Err(StoreError::SchemaMismatch)
    ));
    let error = match LanceModuleStore::open_native_snapshot(&uri, schema(), session, 99).await {
        Ok(_) => panic!("an unpublished version cannot be opened"),
        Err(error) => error,
    };
    assert!(matches!(error, StoreError::Sdk(_)));
    assert!(error.source().is_some());
    assert!(matches!(
        error,
        StoreError::Sdk(cause) if matches!(
            cause.as_ref(),
            lance::Error::VersionNotFound { .. } | lance::Error::DatasetNotFound { .. }
        )
    ));
    assert_eq!(
        bodies(&LanceModuleStore::open(&uri, schema()).await?.snapshot()).await?,
        ["original"]
    );
    Ok(())
}

#[tokio::test]
async fn branch_native_capture_keeps_exact_version_and_private_session() -> TestResult {
    use graphit_data::lance_branch_store::{BranchError, BranchSnapshot};
    let temp = TempDir::new()?;
    let uri = Url::from_file_path(temp.path().join("semantic % 🦀.lance"))
        .unwrap()
        .to_string();
    let writer = LanceModuleStore::create(&uri, schema(), vec![image("retained")]).await?;
    let (private, observed) = session();
    let (first, location) = BranchSnapshot::capture_native(&uri, private.clone(), 1).await?;
    assert_eq!(first.version(), 1);
    assert_eq!(location.version, 1);
    assert_eq!(first.schema(), schema().as_ref());
    assert!(observed.0.load(Ordering::Relaxed) > 0);
    writer
        .snapshot()
        .prepare_replace(vec![image("new head")])
        .await?
        .commit()
        .await?;
    let batches: Vec<RecordBatch> = first.scan().try_into_stream().await?.try_collect().await?;
    let body = batches[0]
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(body.value(0), "retained");
    assert!(matches!(
        BranchSnapshot::capture_native(&uri, private.clone(), 0).await,
        Err(BranchError::InvalidVersion)
    ));
    assert!(
        BranchSnapshot::capture_native(&uri, private, 99)
            .await
            .is_err()
    );
    Ok(())
}
