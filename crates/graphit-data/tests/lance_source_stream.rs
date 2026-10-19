use arrow_array::{RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::{StreamExt, stream};
use graphit_data::{
    lance_branch_store::{
        BranchError, BranchSnapshot, DeltaResolution, LanceBranchStore, col, lit,
    },
    lance_module_store::{
        LanceModuleStore, RecordBatchStreamAdapter, Resolution, SendableRecordBatchStream,
        StoreError,
    },
};
use std::{collections::BTreeMap, error::Error, sync::Arc};

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("path", DataType::Utf8, true),
        Field::new("source", DataType::Utf8, true),
    ]))
}
fn batch(schema: &Arc<Schema>, paths: Vec<Option<&str>>, values: Vec<Option<&str>>) -> RecordBatch {
    RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(StringArray::from(paths)),
            Arc::new(StringArray::from(values)),
        ],
    )
    .unwrap()
}
fn batches(schema: &Arc<Schema>, batches: Vec<RecordBatch>) -> SendableRecordBatchStream {
    Box::pin(RecordBatchStreamAdapter::new(
        Arc::clone(schema),
        stream::iter(batches.into_iter().map(Ok)),
    ))
}
fn late_error(schema: &Arc<Schema>) -> SendableRecordBatchStream {
    let first = batch(schema, vec![Some("partial")], vec![Some("unpublished")]);
    Box::pin(RecordBatchStreamAdapter::new(
        Arc::clone(schema),
        stream::iter([
            Ok(first),
            Err(std::io::Error::other("source failed after first FILE").into()),
        ]),
    ))
}
async fn target(uri: &str, schema: &Arc<Schema>) -> BranchSnapshot {
    LanceBranchStore::open(uri, Arc::clone(schema), None)
        .await
        .unwrap()
        .capture_target(None)
        .await
        .unwrap()
}
async fn rows(snapshot: &BranchSnapshot) -> BTreeMap<String, Option<String>> {
    let mut scan = snapshot.scan().try_into_stream().await.unwrap();
    let mut rows = BTreeMap::new();
    while let Some(batch) = scan.next().await {
        let batch = batch.unwrap();
        let paths = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let sources = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for (path, source) in paths.iter().zip(sources.iter()) {
            rows.insert(path.unwrap().to_owned(), source.map(str::to_owned));
        }
    }
    rows
}

#[tokio::test]
async fn create_overwrite_empty_recovery_and_stale_cas() {
    let temp = tempfile::tempdir().unwrap();
    let uri = temp
        .path()
        .join("source.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let schema = schema();
    let input = vec![
        batch(&schema, vec![Some("a")], vec![Some("")]),
        batch(&schema, vec![Some("b")], vec![None]),
    ];
    let prepared =
        LanceModuleStore::prepare_create_stream(&uri, Arc::clone(&schema), batches(&schema, input))
            .await
            .unwrap();
    assert!(!temp.path().join("source.lance/_versions").exists());
    let key = prepared.recovery_key().clone();
    let (store, returned) = prepared.commit().await.unwrap();
    assert_eq!(key, returned);
    assert_eq!(
        store.resolve(&key).await.unwrap(),
        Resolution::Applied { version: 1 }
    );
    let pin = target(&uri, &schema).await;
    assert_eq!(
        rows(&pin).await,
        BTreeMap::from([("a".into(), Some("".into())), ("b".into(), None)])
    );
    let pending = pin
        .prepare_overwrite_stream(
            Arc::clone(&schema),
            batches(&schema, vec![RecordBatch::new_empty(Arc::clone(&schema))]),
        )
        .await
        .unwrap();
    let stale = pin
        .prepare_overwrite_stream(
            Arc::clone(&schema),
            batches(
                &schema,
                vec![batch(&schema, vec![Some("stale")], vec![Some("x")])],
            ),
        )
        .await
        .unwrap();
    let overwrite_key = pending.recovery_key().clone();
    let stale_key = stale.recovery_key().clone();
    let next = pending.commit().await.unwrap();
    assert!(rows(&next).await.is_empty());
    assert_eq!(
        next.resolve(&overwrite_key).await.unwrap(),
        DeltaResolution::Applied { version: 2 }
    );
    let failure = match stale.commit().await {
        Ok(_) => panic!("stale overwrite published"),
        Err(e) => e,
    };
    assert_eq!(failure.recovery_key(), &stale_key);
    assert!(failure.source().is_some());
    assert_eq!(
        next.resolve(&stale_key).await.unwrap(),
        DeltaResolution::Conflicted { version: 2 }
    );
    assert_eq!(rows(&pin).await.len(), 2);
    let empty_uri = temp.path().join("empty.lance").to_str().unwrap().to_owned();
    let (empty, _) = LanceModuleStore::prepare_create_stream(
        &empty_uri,
        Arc::clone(&schema),
        batches(&schema, vec![RecordBatch::new_empty(Arc::clone(&schema))]),
    )
    .await
    .unwrap()
    .commit()
    .await
    .unwrap();
    assert_eq!(empty.snapshot().version(), 1);
    assert!(rows(&target(&empty_uri, &schema).await).await.is_empty());
}

#[tokio::test]
async fn merge_keep_delete_if_empty_delta_and_strict_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let uri = temp
        .path()
        .join("source.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let schema = schema();
    LanceModuleStore::prepare_create_stream(
        &uri,
        Arc::clone(&schema),
        batches(
            &schema,
            vec![batch(
                &schema,
                vec![Some("keep"), Some("gone"), Some("updated")],
                vec![Some("old"), Some("old"), Some("old")],
            )],
        ),
    )
    .await
    .unwrap()
    .commit()
    .await
    .unwrap();
    let pin = target(&uri, &schema).await;
    let prepared = pin
        .prepare_merge_stream(
            batches(
                &schema,
                vec![batch(
                    &schema,
                    vec![Some("updated"), Some("new")],
                    vec![None, Some("")],
                )],
            ),
            vec!["path".into()],
            None,
        )
        .await
        .unwrap();
    let key = prepared.recovery_key().clone();
    let stale = pin
        .prepare_merge_stream(
            batches(
                &schema,
                vec![batch(&schema, vec![Some("stale")], vec![Some("x")])],
            ),
            vec!["path".into()],
            None,
        )
        .await
        .unwrap();
    let stale_key = stale.recovery_key().clone();
    let next = prepared.commit().await.unwrap();
    assert_eq!(
        next.resolve(&key).await.unwrap(),
        DeltaResolution::Applied { version: 2 }
    );
    assert_eq!(
        rows(&next).await,
        BTreeMap::from([
            ("keep".into(), Some("old".into())),
            ("gone".into(), Some("old".into())),
            ("updated".into(), None),
            ("new".into(), Some("".into()))
        ])
    );
    let failure = match stale.commit().await {
        Ok(_) => panic!("stale merge published"),
        Err(e) => e,
    };
    assert_eq!(failure.recovery_key(), &stale_key);
    assert_eq!(
        next.resolve(&stale_key).await.unwrap(),
        DeltaResolution::Conflicted { version: 2 }
    );
    let deleted = next
        .prepare_merge_stream(
            batches(&schema, vec![RecordBatch::new_empty(Arc::clone(&schema))]),
            vec!["path".into()],
            Some(col("path").eq(lit("gone"))),
        )
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let result = rows(&deleted).await;
    assert_eq!(result.len(), 3);
    assert!(!result.contains_key("gone"));
    assert!(result.contains_key("keep"));
}

#[tokio::test]
async fn schema_null_duplicate_and_late_errors_never_publish() {
    let temp = tempfile::tempdir().unwrap();
    let uri = temp
        .path()
        .join("source.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let schema = schema();
    let error = match LanceModuleStore::prepare_create_stream(
        &uri,
        Arc::clone(&schema),
        late_error(&schema),
    )
    .await
    {
        Ok(_) => panic!("late failure prepared"),
        Err(e) => e,
    };
    assert!(error.to_string().contains("source failed after first FILE"));
    assert!(error.source().is_some());
    assert!(
        LanceModuleStore::open(&uri, Arc::clone(&schema))
            .await
            .is_err()
    );
    assert!(
        LanceModuleStore::prepare_create_stream(
            &uri,
            Arc::clone(&schema),
            batches(&schema, vec![])
        )
        .await
        .is_err()
    );
    let other = Arc::new(Schema::new(vec![Field::new("other", DataType::Utf8, true)]));
    assert!(matches!(
        LanceModuleStore::prepare_create_stream(
            &uri,
            Arc::clone(&schema),
            batches(&other, vec![RecordBatch::new_empty(Arc::clone(&other))])
        )
        .await,
        Err(StoreError::SchemaMismatch)
    ));
    assert!(
        LanceModuleStore::prepare_create_stream(
            &uri,
            Arc::clone(&schema),
            batches(
                &schema,
                vec![
                    RecordBatch::new_empty(Arc::clone(&schema)),
                    RecordBatch::new_empty(Arc::clone(&other))
                ]
            )
        )
        .await
        .is_err()
    );
    LanceModuleStore::create(
        &uri,
        Arc::clone(&schema),
        vec![batch(&schema, vec![Some("original")], vec![Some("body")])],
    )
    .await
    .unwrap();
    let pin = target(&uri, &schema).await;
    assert!(
        pin.prepare_overwrite_stream(Arc::clone(&schema), late_error(&schema))
            .await
            .is_err()
    );
    assert!(
        pin.prepare_merge_stream(
            late_error(&schema),
            vec!["path".into()],
            Some(col("path").eq(lit("original")))
        )
        .await
        .is_err()
    );
    assert!(matches!(
        pin.prepare_merge_stream(
            batches(&other, vec![RecordBatch::new_empty(other.clone())]),
            vec!["path".into()],
            None
        )
        .await,
        Err(BranchError::SchemaMismatch)
    ));
    assert!(matches!(
        pin.prepare_merge_stream(
            batches(&schema, vec![RecordBatch::new_empty(schema.clone())]),
            vec!["source".into(), "source".into()],
            None
        )
        .await,
        Err(BranchError::InvalidJoinColumns)
    ));
    let null_error = match pin
        .prepare_merge_stream(
            batches(
                &schema,
                vec![batch(&schema, vec![None], vec![Some("body")])],
            ),
            vec!["path".into()],
            None,
        )
        .await
    {
        Ok(_) => panic!("null key prepared"),
        Err(e) => e,
    };
    assert!(null_error.to_string().contains("NullKey"));
    let duplicate = match pin
        .prepare_merge_stream(
            batches(
                &schema,
                vec![
                    batch(&schema, vec![Some("same")], vec![Some("one")]),
                    batch(&schema, vec![Some("same")], vec![Some("two")]),
                ],
            ),
            vec!["path".into()],
            None,
        )
        .await
    {
        Ok(_) => panic!("duplicate key prepared"),
        Err(e) => e,
    };
    assert!(duplicate.source().is_some());
    assert!(duplicate.to_string().contains("DuplicateKey"));
    let reopened = target(&uri, &schema).await;
    assert_eq!(reopened.version(), 1);
    assert_eq!(
        rows(&reopened).await,
        BTreeMap::from([("original".into(), Some("body".into()))])
    );
}

#[tokio::test]
async fn bounded_borrowed_producer_reaches_eof_before_prepare_returns() {
    let temp = tempfile::tempdir().unwrap();
    let uri = temp
        .path()
        .join("source.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let schema = schema();
    let body_owner = String::from("borrowed FILE contents");
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let receiver = stream::unfold(receiver, |mut receiver| async {
        receiver.recv().await.map(|batch| (Ok(batch), receiver))
    });
    let input: SendableRecordBatchStream =
        Box::pin(RecordBatchStreamAdapter::new(schema.clone(), receiver));
    let producer = async {
        for path in ["a", "b", "c"] {
            let batch = batch(&schema, vec![Some(path)], vec![Some(body_owner.as_str())]);
            sender.send(batch).await.unwrap();
        }
        drop(sender);
    };
    let ((), prepared) = futures::join!(
        producer,
        LanceModuleStore::prepare_create_stream(&uri, schema.clone(), input)
    );
    let (store, key) = prepared.unwrap().commit().await.unwrap();
    assert_eq!(
        store.resolve(&key).await.unwrap(),
        Resolution::Applied { version: 1 }
    );
    let pin = target(&uri, &schema).await;
    assert_eq!(rows(&pin).await.len(), 3);
    assert!(
        rows(&pin)
            .await
            .values()
            .all(|source| source.as_deref() == Some(body_owner.as_str()))
    );
}

#[tokio::test]
async fn inherited_refusal_and_named_streams_preserve_default() {
    let temp = tempfile::tempdir().unwrap();
    let uri = temp
        .path()
        .join("source.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let schema = schema();
    LanceModuleStore::create(
        &uri,
        schema.clone(),
        vec![batch(&schema, vec![Some("default")], vec![Some("body")])],
    )
    .await
    .unwrap();
    let store = LanceBranchStore::open(&uri, schema.clone(), None)
        .await
        .unwrap();
    let inherited = store.capture_target(Some("named")).await.unwrap();
    assert!(matches!(
        inherited
            .prepare_overwrite_stream(
                schema.clone(),
                batches(&schema, vec![RecordBatch::new_empty(schema.clone())])
            )
            .await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert!(matches!(
        inherited
            .prepare_merge_stream(
                batches(&schema, vec![RecordBatch::new_empty(schema.clone())]),
                vec!["path".into()],
                Some(col("path").eq(lit("default")))
            )
            .await,
        Err(BranchError::InheritedSnapshot)
    ));
    let named = inherited.materialize().await.unwrap();
    let named = named
        .prepare_overwrite_stream(
            schema.clone(),
            batches(
                &schema,
                vec![batch(
                    &schema,
                    vec![Some("only-named")],
                    vec![Some("named body")],
                )],
            ),
        )
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let pending = named
        .prepare_merge_stream(
            batches(
                &schema,
                vec![batch(&schema, vec![Some("new-named")], vec![None])],
            ),
            vec!["path".into()],
            Some(col("path").eq(lit("only-named"))),
        )
        .await
        .unwrap();
    let key = pending.recovery_key().clone();
    let next = pending.commit().await.unwrap();
    assert_eq!(next.physical_branch(), Some("named"));
    assert_eq!(
        rows(&next).await,
        BTreeMap::from([("new-named".into(), None)])
    );
    assert_eq!(
        next.resolve(&key).await.unwrap(),
        DeltaResolution::Applied { version: 3 }
    );
    assert_eq!(
        rows(&target(&uri, &schema).await).await,
        BTreeMap::from([("default".into(), Some("body".into()))])
    );
}
