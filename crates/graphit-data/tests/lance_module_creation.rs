use arrow_array::{Array, ArrayRef, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::lance_module_store::{LanceModuleStore, RecoveryKey, Resolution, StoreError};
use std::{error::Error, path::Path, sync::Arc};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new_with_metadata(
        vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("body", DataType::Utf8, true),
        ],
        [("fixture".into(), "Main creation 🦀".into())].into(),
    ))
}

fn image(schema: Arc<Schema>) -> Result<RecordBatch, arrow_schema::ArrowError> {
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec!["α", "β", "γ"])),
        Arc::new(StringArray::from(vec![
            Some("linha1\r\noriginal\0🦀"),
            None,
            Some(""),
        ])),
    ];
    RecordBatch::try_new(schema, columns)
}

fn uri(temp: &TempDir, name: &str) -> Result<String, Box<dyn Error>> {
    Ok(temp
        .path()
        .join(name)
        .to_str()
        .ok_or("fixture path is not UTF-8")?
        .into())
}

async fn rows(store: &LanceModuleStore) -> Result<Vec<(String, Option<String>)>, Box<dyn Error>> {
    let snapshot = store.snapshot();
    let batches: Vec<RecordBatch> = snapshot
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    let mut result = Vec::new();
    for batch in batches {
        assert_eq!(batch.schema().as_ref(), snapshot.schema());
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let bodies = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for row in 0..batch.num_rows() {
            assert!(!ids.is_null(row));
            result.push((
                ids.value(row).into(),
                (!bodies.is_null(row)).then(|| bodies.value(row).into()),
            ));
        }
    }
    Ok(result)
}

fn expected_rows() -> Vec<(String, Option<String>)> {
    vec![
        ("α".into(), Some("linha1\r\noriginal\0🦀".into())),
        ("β".into(), None),
        ("γ".into(), Some("".into())),
    ]
}

async fn assert_unpublished(path: &str, schema: Arc<Schema>) {
    match LanceModuleStore::open(path, schema).await {
        Err(StoreError::Sdk(cause)) if matches!(*cause, lance::Error::DatasetNotFound { .. }) => (),
        Err(error) => panic!("expected an unpublished Main, got {error}"),
        Ok(_) => panic!("preparation unexpectedly published Main"),
    }
}

#[tokio::test]
async fn prepared_creation_exposes_key_before_publication_and_preserves_payload() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "prepared.lance")?;
    let schema = schema();
    let pending =
        LanceModuleStore::prepare_create(&path, schema.clone(), vec![image(schema.clone())?])
            .await?;
    let key = pending.recovery_key().clone();
    assert_eq!(key.corpus_uri(), path);
    assert_eq!(key.base_version(), 0);
    assert!(!key.transaction_id().is_empty());
    assert_unpublished(&path, schema.clone()).await;
    let (store, committed_key) = pending.commit().await?;
    assert_eq!(committed_key, key);
    assert_eq!(store.snapshot().version(), 1);
    assert_eq!(store.snapshot().schema(), schema.as_ref());
    assert_eq!(rows(&store).await?, expected_rows());
    assert_eq!(
        store.resolve(&key).await?,
        Resolution::Applied { version: 1 }
    );
    let reopened = LanceModuleStore::open(&path, schema).await?;
    assert_eq!(rows(&reopened).await?, expected_rows());
    Ok(())
}

#[tokio::test]
async fn competing_prepared_creations_preserve_winner_and_loser_key() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "competing.lance")?;
    let schema = schema();
    let winner =
        LanceModuleStore::prepare_create(&path, schema.clone(), vec![image(schema.clone())?])
            .await?;
    let loser = LanceModuleStore::prepare_create(&path, schema.clone(), vec![]).await?;
    let losing_key = loser.recovery_key().clone();
    assert_ne!(
        winner.recovery_key().transaction_id(),
        losing_key.transaction_id()
    );
    assert_unpublished(&path, schema.clone()).await;
    let (store, winner_key) = winner.commit().await?;
    let failure = match loser.commit().await {
        Err(StoreError::Creation(failure)) => failure,
        Err(error) => panic!("expected the competing SDK commit cause and key, got {error}"),
        Ok(_) => panic!("second prepared Create overwrote the winner"),
    };
    assert_eq!(failure.recovery_key(), &losing_key);
    assert!(failure.source().is_some());
    assert_eq!(failure.to_string(), failure.sdk_error().to_string());
    assert_eq!(
        store.resolve(&losing_key).await?,
        Resolution::Conflicted { version: 1 }
    );
    assert_eq!(
        store.resolve(&winner_key).await?,
        Resolution::Applied { version: 1 }
    );
    let reopened = LanceModuleStore::open(&path, schema).await?;
    assert_eq!(reopened.snapshot().version(), 1);
    assert_eq!(rows(&reopened).await?, expected_rows());
    Ok(())
}

#[tokio::test]
async fn invalid_and_abandoned_preparations_publish_nothing_and_empty_commit_keeps_schema()
-> TestResult {
    let temp = TempDir::new()?;
    let invalid = uri(&temp, "invalid-prepared.lance")?;
    let schema = schema();
    let other = Arc::new(Schema::new(vec![Field::new(
        "other",
        DataType::Utf8,
        false,
    )]));
    assert!(matches!(
        LanceModuleStore::prepare_create(
            &invalid,
            schema.clone(),
            vec![RecordBatch::new_empty(other)]
        )
        .await,
        Err(StoreError::SchemaMismatch)
    ));
    assert!(!Path::new(&invalid).exists());
    let path = uri(&temp, "abandoned.lance")?;
    let pending =
        LanceModuleStore::prepare_create(&path, schema.clone(), vec![image(schema.clone())?])
            .await?;
    assert_unpublished(&path, schema.clone()).await;
    drop(pending);
    assert_unpublished(&path, schema.clone()).await;
    let empty = uri(&temp, "prepared-empty.lance")?;
    let pending = LanceModuleStore::prepare_create(&empty, schema.clone(), vec![]).await?;
    assert_unpublished(&empty, schema.clone()).await;
    let (store, key) = pending.commit().await?;
    assert_eq!(store.snapshot().schema(), schema.as_ref());
    assert!(rows(&store).await?.is_empty());
    assert_eq!(
        store.resolve(&key).await?,
        Resolution::Applied { version: 1 }
    );
    Ok(())
}

#[tokio::test]
async fn caller_can_decline_commit_after_its_source_changes() -> TestResult {
    let temp = TempDir::new()?;
    let source = temp.path().join("authored.md");
    std::fs::write(&source, "original 🦀")?;
    let captured = std::fs::read(&source)?;
    let path = uri(&temp, "declined.lance")?;
    let schema = schema();
    let pending =
        LanceModuleStore::prepare_create(&path, schema.clone(), vec![image(schema.clone())?])
            .await?;
    std::fs::write(&source, "changed 🦀")?;
    assert_ne!(std::fs::read(&source)?, captured);
    // This proves the actual prepare/commit boundary, not Knowledge authorization
    // or its physical capture guard, which the Knowledge caller tests separately.
    drop(pending);
    assert_unpublished(&path, schema).await;
    Ok(())
}

#[tokio::test]
async fn public_creation_returns_actual_key_and_complete_reopen_payload() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "first.lance")?;
    let schema = schema();
    let (store, key) =
        LanceModuleStore::create_with_recovery(&path, schema.clone(), vec![image(schema.clone())?])
            .await?;
    assert_eq!(key.corpus_uri(), path);
    assert_eq!(key.base_version(), 0);
    assert!(!key.transaction_id().is_empty());
    assert_eq!(store.snapshot().version(), 1);
    assert_eq!(store.snapshot().schema(), schema.as_ref());
    assert_eq!(rows(&store).await?, expected_rows());
    assert_eq!(
        store.resolve(&key).await?,
        Resolution::Applied { version: 1 }
    );
    let encoded = serde_json::to_vec(&key)?;
    drop(store);
    let restored: RecoveryKey = serde_json::from_slice(&encoded)?;
    assert_eq!(restored, key);
    let reopened = LanceModuleStore::open(&path, schema.clone()).await?;
    assert_eq!(rows(&reopened).await?, expected_rows());
    assert_eq!(reopened.snapshot().schema(), schema.as_ref());
    assert_eq!(
        reopened.resolve(&restored).await?,
        Resolution::Applied { version: 1 }
    );
    let other_path = uri(&temp, "other.lance")?;
    let (other, _) = LanceModuleStore::create_with_recovery(&other_path, schema, vec![]).await?;
    assert!(matches!(
        other.resolve(&restored).await,
        Err(StoreError::CorpusMismatch)
    ));
    Ok(())
}

#[tokio::test]
async fn public_empty_creation_retains_full_schema_and_actual_key() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "empty.lance")?;
    let schema = schema();
    let (store, key) =
        LanceModuleStore::create_with_recovery(&path, schema.clone(), vec![]).await?;
    assert_eq!(store.snapshot().version(), 1);
    assert_eq!(store.snapshot().schema(), schema.as_ref());
    assert!(rows(&store).await?.is_empty());
    assert_eq!(key.corpus_uri(), path);
    assert_eq!(key.base_version(), 0);
    assert!(!key.transaction_id().is_empty());
    assert_eq!(
        store.resolve(&key).await?,
        Resolution::Applied { version: 1 }
    );
    let reopened = LanceModuleStore::open(&path, schema.clone()).await?;
    assert_eq!(reopened.snapshot().schema(), schema.as_ref());
    assert!(rows(&reopened).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn public_creation_preserves_validation_and_duplicate_failure_phase() -> TestResult {
    let temp = TempDir::new()?;
    let missing = uri(&temp, "invalid.lance")?;
    let schema = schema();
    let incompatible = Arc::new(Schema::new(vec![Field::new(
        "different",
        DataType::Utf8,
        false,
    )]));
    assert!(matches!(
        LanceModuleStore::create_with_recovery(
            &missing,
            schema.clone(),
            vec![RecordBatch::new_empty(incompatible)]
        )
        .await,
        Err(StoreError::SchemaMismatch)
    ));
    assert!(!Path::new(&missing).exists());
    let path = uri(&temp, "existing.lance")?;
    let (store, winner_key) =
        LanceModuleStore::create_with_recovery(&path, schema.clone(), vec![image(schema.clone())?])
            .await?;
    let error = match LanceModuleStore::create_with_recovery(&path, schema.clone(), vec![]).await {
        Ok(_) => panic!("duplicate Create unexpectedly overwrote the existing corpus"),
        Err(error) => error,
    };
    assert!(error.source().is_some());
    match &error {
        StoreError::Creation(failure) => {
            let key = failure.recovery_key();
            assert_eq!(key.corpus_uri(), path);
            assert_eq!(key.base_version(), 0);
            assert!(!key.transaction_id().is_empty());
            assert_ne!(key.transaction_id(), winner_key.transaction_id());
            assert!(failure.source().is_some());
            assert_eq!(failure.to_string(), failure.sdk_error().to_string());
            assert_eq!(
                store.resolve(key).await?,
                Resolution::Conflicted { version: 1 }
            );
            println!("duplicate Create failed at commit with an actual recovery key");
        }
        StoreError::Sdk(cause) => {
            assert_eq!(error.source().unwrap().to_string(), cause.to_string());
            println!("duplicate Create failed before a returned commit key");
        }
        other => panic!("unexpected duplicate Create failure: {other}"),
    }
    let reopened = LanceModuleStore::open(&path, schema).await?;
    assert_eq!(reopened.snapshot().version(), 1);
    assert_eq!(rows(&reopened).await?, expected_rows());
    assert_eq!(
        reopened.resolve(&winner_key).await?,
        Resolution::Applied { version: 1 }
    );
    Ok(())
}
