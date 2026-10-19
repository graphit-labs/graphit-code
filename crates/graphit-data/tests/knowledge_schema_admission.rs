use arrow_array::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use graphit_data::{
    knowledge_index::physical_schema,
    lance_module_store::{LanceModuleStore, StoreError},
};
use std::{error::Error, sync::Arc};

#[tokio::test]
async fn canonical_knowledge_schema_admits_snapshot_and_preserves_open_errors()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("index.lance");
    let uri = path.to_str().unwrap();
    let schema = physical_schema();
    let created = LanceModuleStore::create(
        uri,
        Arc::clone(&schema),
        vec![RecordBatch::new_empty(Arc::clone(&schema))],
    )
    .await?;
    let version = created.snapshot().version();
    drop(created);

    let opened = LanceModuleStore::open(uri, Arc::clone(&schema)).await?;
    let snapshot = opened.snapshot();
    drop(opened);
    assert_eq!(snapshot.version(), version);
    assert_eq!(snapshot.schema(), schema.as_ref());

    // A nullable slug cannot be admitted as the canonical native Knowledge corpus.
    let mut fields: Vec<_> = schema
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    fields[0] = Field::new("slug", DataType::Utf8, true);
    let incompatible = Arc::new(Schema::new(fields));
    assert!(matches!(
        LanceModuleStore::open(uri, incompatible).await,
        Err(StoreError::SchemaMismatch)
    ));

    let invalid = root.path().join("invalid.lance");
    std::fs::create_dir(&invalid)?;
    let error = LanceModuleStore::open(invalid.to_str().unwrap(), schema)
        .await
        .err()
        .expect("a directory without a Lance manifest must not open");
    assert!(matches!(error, StoreError::Sdk(_)));
    assert!(error.source().is_some());
    assert_eq!(snapshot.version(), version);
    Ok(())
}
