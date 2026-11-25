use arrow_array::{Array, ArrayRef, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::lance_module_store::{
    LanceModuleStore, ModuleSnapshot, RecoveryKey, Resolution, StoreError,
};
use std::{error::Error, sync::Arc};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("family", DataType::Utf8, false),
        Field::new("id", DataType::Utf8, false),
        Field::new("revision", DataType::UInt64, false),
        Field::new("body", DataType::Utf8, true),
    ]))
}
fn image(revision: u64) -> RecordBatch {
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec!["task", "session", "audit"])),
        Arc::new(StringArray::from(vec!["task-1", "session-1", "event-1"])),
        Arc::new(UInt64Array::from(vec![revision; 3])),
        Arc::new(StringArray::from(vec![
            Some("conteúdo 🦀"),
            None,
            Some("immutable event"),
        ])),
    ];
    RecordBatch::try_new(schema(), columns).unwrap()
}
fn uri(temp: &TempDir, name: &str) -> String {
    temp.path().join(name).to_string_lossy().into_owned()
}

fn model_schema(dimensions: i32, model: &str) -> Arc<Schema> {
    Arc::new(Schema::new_with_metadata(
        vec![Field::new(
            "embedding",
            DataType::FixedSizeList(
                // Lance reconstructs primitive list children as nullable.
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensions,
            ),
            false,
        )],
        [("model".into(), model.into())].into(),
    ))
}

async fn batches(snapshot: &ModuleSnapshot) -> Result<Vec<RecordBatch>, lance::Error> {
    snapshot.scan().try_into_stream().await?.try_collect().await
}
async fn count(snapshot: &ModuleSnapshot) -> Result<usize, lance::Error> {
    Ok(batches(snapshot)
        .await?
        .iter()
        .map(RecordBatch::num_rows)
        .sum())
}
async fn revisions(snapshot: &ModuleSnapshot) -> Result<Vec<u64>, lance::Error> {
    Ok(batches(snapshot)
        .await?
        .iter()
        .flat_map(|batch| {
            batch
                .column(2)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .values()
                .to_vec()
        })
        .collect())
}

#[tokio::test]
async fn empty_create_reopen_and_read_does_not_create_missing() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "task.lance");
    let store = LanceModuleStore::create(&path, schema(), vec![]).await?;
    assert_eq!(store.snapshot().version(), 1);
    assert_eq!(count(&store.snapshot()).await?, 0);
    let reopened = LanceModuleStore::open(&path, schema()).await?;
    assert_eq!(reopened.snapshot().schema(), schema().as_ref());
    assert_eq!(count(&reopened.snapshot()).await?, 0);
    let missing = uri(&temp, "missing.lance");
    assert!(LanceModuleStore::open(&missing, schema()).await.is_err());
    assert!(!std::path::Path::new(&missing).exists());
    Ok(())
}

#[tokio::test]
async fn complete_families_publish_together_and_old_snapshot_stays_fixed() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "task.lance");
    let store = LanceModuleStore::create(&path, schema(), vec![image(1)]).await?;
    let old = store.snapshot();
    let pending = old.prepare_replace(vec![image(2)]).await?;
    let key = pending.recovery_key().clone();
    assert_eq!(
        revisions(&LanceModuleStore::open(&path, schema()).await?.snapshot()).await?,
        vec![1; 3]
    );
    let next = pending.commit().await?;
    assert_eq!(next.snapshot().version(), 2);
    assert_eq!(revisions(&old).await?, vec![1; 3]);
    let reopened = LanceModuleStore::open(&path, schema()).await?;
    assert_eq!(revisions(&reopened.snapshot()).await?, vec![2; 3]);
    assert_eq!(
        reopened.resolve(&key).await?,
        Resolution::Applied { version: 2 }
    );
    let output = batches(&reopened.snapshot()).await?;
    let families: Vec<_> = output
        .iter()
        .flat_map(|b| {
            b.column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .iter()
                .map(|s| s.unwrap().to_owned())
        })
        .collect();
    assert_eq!(families, vec!["task", "session", "audit"]);
    let body = output[0]
        .column(3)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(body.value(0), "conteúdo 🦀");
    assert!(body.is_null(1));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_writers_never_rebase_or_lose_the_winner() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "task.lance");
    let store = LanceModuleStore::create(&path, schema(), vec![image(1)]).await?;
    let a = store.snapshot().prepare_replace(vec![image(2)]).await?;
    let b = LanceModuleStore::open(&path, schema())
        .await?
        .snapshot()
        .prepare_replace(vec![image(3)])
        .await?;
    let ak = a.recovery_key().clone();
    let bk = b.recovery_key().clone();
    let (ar, br) = tokio::join!(a.commit(), b.commit());
    assert_ne!(ar.is_ok(), br.is_ok(), "exactly one strict commit succeeds");
    let (winner, loser, winner_revision) = if ar.is_ok() {
        (&ak, &bk, 2)
    } else {
        (&bk, &ak, 3)
    };
    let reopened = LanceModuleStore::open(&path, schema()).await?;
    assert_eq!(reopened.snapshot().version(), 2);
    assert_eq!(
        revisions(&reopened.snapshot()).await?,
        vec![winner_revision; 3]
    );
    assert_eq!(
        reopened.resolve(winner).await?,
        Resolution::Applied { version: 2 }
    );
    assert_eq!(
        reopened.resolve(loser).await?,
        Resolution::Conflicted { version: 2 }
    );
    let error = ar.err().or(br.err()).unwrap();
    assert_eq!(error.recovery_key(), loser);
    assert!(error.source().is_some());
    Ok(())
}

#[tokio::test]
async fn schema_mismatch_is_rejected_without_a_published_write() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "task.lance");
    let store = LanceModuleStore::create(&path, schema(), vec![image(1)]).await?;
    let wrong = Arc::new(Schema::new(vec![Field::new(
        "other",
        DataType::UInt64,
        false,
    )]));
    let bad = RecordBatch::new_empty(Arc::clone(&wrong));
    assert!(matches!(
        store.snapshot().prepare_replace(vec![bad]).await,
        Err(StoreError::SchemaMismatch)
    ));
    assert!(matches!(
        LanceModuleStore::open(&path, wrong).await,
        Err(StoreError::SchemaMismatch)
    ));
    let reopened = LanceModuleStore::open(&path, schema()).await?;
    assert_eq!(reopened.snapshot().version(), 1);
    assert_eq!(revisions(&reopened.snapshot()).await?, vec![1; 3]);
    assert!(
        LanceModuleStore::create(&path, schema(), vec![image(9)])
            .await
            .is_err()
    );
    assert_eq!(
        revisions(&LanceModuleStore::open(&path, schema()).await?.snapshot()).await?,
        vec![1; 3]
    );
    Ok(())
}

#[tokio::test]
async fn abandoned_prepare_and_lost_ack_recover_by_serialized_key() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "task.lance");
    let store = LanceModuleStore::create(&path, schema(), vec![image(1)]).await?;
    let abandoned = store.snapshot().prepare_replace(vec![image(99)]).await?;
    let abandoned_key = abandoned.recovery_key().clone();
    drop(abandoned);
    assert_eq!(store.resolve(&abandoned_key).await?, Resolution::Unresolved);
    assert_eq!(
        revisions(&LanceModuleStore::open(&path, schema()).await?.snapshot()).await?,
        vec![1; 3]
    );
    let pending = store.snapshot().prepare_replace(vec![image(2)]).await?;
    let bytes = serde_json::to_vec(pending.recovery_key())?;
    std::fs::write(temp.path().join("recovery.json"), bytes)?;
    drop(pending.commit().await?); // Commit completed; caller deliberately loses the acknowledgement.
    drop(store);
    let recovered: RecoveryKey =
        serde_json::from_slice(&std::fs::read(temp.path().join("recovery.json"))?)?;
    let reopened = LanceModuleStore::open(&path, schema()).await?;
    assert_eq!(
        reopened.resolve(&recovered).await?,
        Resolution::Applied { version: 2 }
    );
    assert_eq!(
        reopened.resolve(&abandoned_key).await?,
        Resolution::Conflicted { version: 2 }
    );
    assert_eq!(reopened.snapshot().version(), 2); // Resolution does not write a duplicate version.
    Ok(())
}

#[tokio::test]
async fn query_projection_and_invalid_recovery_keys_preserve_scope() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "task.lance");
    let store = LanceModuleStore::create(&path, schema(), vec![image(u64::MAX)]).await?;
    let snapshot = store.snapshot();
    let mut scan = snapshot.scan();
    scan.filter("family = 'session'")?
        .project(&["id", "body"])?;
    let projected: Vec<RecordBatch> = scan.try_into_stream().await?.try_collect().await?;
    assert_eq!(
        projected.iter().map(RecordBatch::num_rows).sum::<usize>(),
        1
    );
    assert_eq!(
        projected[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(0),
        "session-1"
    );
    assert!(projected[0].column(1).is_null(0));
    assert_eq!(revisions(&snapshot).await?, vec![u64::MAX; 3]);
    let pending = snapshot.prepare_replace(vec![]).await?;
    let key = pending.recovery_key();
    let mut raw = serde_json::to_value(key)?;
    raw["corpus_uri"] = serde_json::json!(uri(&temp, "other.lance"));
    let other: RecoveryKey = serde_json::from_value(raw)?;
    assert!(matches!(
        store.resolve(&other).await,
        Err(StoreError::CorpusMismatch)
    ));
    let mut raw = serde_json::to_value(key)?;
    raw["base_version"] = serde_json::json!(u64::MAX);
    let overflow: RecoveryKey = serde_json::from_value(raw)?;
    assert!(matches!(
        store.resolve(&overflow).await,
        Err(StoreError::InvalidRecoveryKey)
    ));
    let mut raw = serde_json::to_value(key)?;
    raw["transaction_id"] = serde_json::json!("");
    let empty: RecoveryKey = serde_json::from_value(raw)?;
    assert!(matches!(
        store.resolve(&empty).await,
        Err(StoreError::InvalidRecoveryKey)
    ));
    let emptied = pending.commit().await?;
    assert_eq!(count(&emptied.snapshot()).await?, 0);
    Ok(())
}

#[tokio::test]
async fn explicit_overwrite_preserves_new_schema_for_empty_and_metadata_changes() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "model.lance");
    let first = model_schema(2, "model-a");
    let store = LanceModuleStore::create(&path, Arc::clone(&first), vec![]).await?;
    let old = store.snapshot();
    let same_width = model_schema(2, "model-b");
    let next = old
        .prepare_overwrite(Arc::clone(&same_width), vec![])
        .await?
        .commit()
        .await?;
    assert_eq!(next.snapshot().schema(), same_width.as_ref());
    assert_eq!(next.snapshot().version(), 2);
    assert_eq!(old.schema(), first.as_ref());
    assert_eq!(old.version(), 1);
    assert_eq!(count(&old).await?, 0);
    assert!(matches!(
        LanceModuleStore::open(&path, first).await,
        Err(StoreError::SchemaMismatch)
    ));
    let wider = model_schema(3, "model-c");
    let pending = next
        .snapshot()
        .prepare_overwrite(Arc::clone(&wider), vec![])
        .await?;
    let key: RecoveryKey = serde_json::from_slice(&serde_json::to_vec(pending.recovery_key())?)?;
    drop(pending.commit().await?);
    let reopened = LanceModuleStore::open(&path, Arc::clone(&wider)).await?;
    assert_eq!(reopened.snapshot().schema(), wider.as_ref());
    assert_eq!(count(&reopened.snapshot()).await?, 0);
    assert_eq!(
        reopened.resolve(&key).await?,
        Resolution::Applied { version: 3 }
    );
    assert!(matches!(
        LanceModuleStore::open(&path, same_width).await,
        Err(StoreError::SchemaMismatch)
    ));
    Ok(())
}

#[tokio::test]
async fn explicit_overwrite_rejects_mixed_schema_and_keeps_strict_recovery() -> TestResult {
    let temp = TempDir::new()?;
    let path = uri(&temp, "task.lance");
    let store = LanceModuleStore::create(&path, schema(), vec![image(1)]).await?;
    let old = store.snapshot();
    let changed = model_schema(3, "model-b");
    assert!(matches!(
        old.prepare_overwrite(Arc::clone(&changed), vec![image(2)])
            .await,
        Err(StoreError::SchemaMismatch)
    ));
    assert!(matches!(
        old.prepare_overwrite(
            Arc::clone(&changed),
            vec![RecordBatch::new_empty(Arc::clone(&changed)), image(2)],
        )
        .await,
        Err(StoreError::SchemaMismatch)
    ));
    assert_eq!(revisions(&old).await?, vec![1; 3]);
    let abandoned = old.prepare_overwrite(Arc::clone(&changed), vec![]).await?;
    let abandoned_key = abandoned.recovery_key().clone();
    drop(abandoned);
    assert_eq!(store.resolve(&abandoned_key).await?, Resolution::Unresolved);
    let replacement = old.prepare_replace(vec![image(2)]).await?;
    let losing_key = replacement.recovery_key().clone();
    let overwrite = old.prepare_overwrite(Arc::clone(&changed), vec![]).await?;
    let winner_key = overwrite.recovery_key().clone();
    let next = overwrite.commit().await?;
    let error = replacement
        .commit()
        .await
        .err()
        .expect("stale writer must conflict");
    assert_eq!(error.recovery_key(), &losing_key);
    assert!(error.source().is_some());
    let reopened = LanceModuleStore::open(&path, changed).await?;
    assert_eq!(next.snapshot().version(), 2);
    assert_eq!(
        reopened.resolve(&winner_key).await?,
        Resolution::Applied { version: 2 }
    );
    assert_eq!(
        reopened.resolve(&losing_key).await?,
        Resolution::Conflicted { version: 2 }
    );
    assert_eq!(
        reopened.resolve(&abandoned_key).await?,
        Resolution::Conflicted { version: 2 }
    );
    assert_eq!(revisions(&old).await?, vec![1; 3]);
    Ok(())
}
