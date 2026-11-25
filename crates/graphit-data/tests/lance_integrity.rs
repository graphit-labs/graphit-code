use arrow_array::{RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use graphit_data::lance_module_store::LanceModuleStore;
use lance::Dataset;
use lance::index::DatasetIndexExt;
use lance_index::{IndexType, scalar::ScalarIndexParams};
use std::{error::Error as StdError, sync::Arc};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn StdError>>;

struct Fixture {
    _temp: TempDir,
    uri: String,
    dataset: Dataset,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn StdError>> {
        let temp = TempDir::new()?;
        let uri = temp
            .path()
            .join("manifest.lance")
            .to_str()
            .unwrap()
            .to_owned();
        let schema = Arc::new(Schema::new(vec![Field::new("body", DataType::Utf8, false)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(StringArray::from(vec!["retained"]))],
        )?;
        LanceModuleStore::create(&uri, schema, vec![batch]).await?;
        let dataset = Dataset::open(&uri).await?;
        Ok(Self {
            _temp: temp,
            uri,
            dataset,
        })
    }
}

#[tokio::test]
async fn upstream_valid_sdk_manifest_remains_readable() -> TestResult {
    let fixture = Fixture::new().await?;
    let reopened = Dataset::open(&fixture.uri).await?;
    assert_eq!(reopened.count_rows(None).await?, 1);
    assert_eq!(
        reopened.manifest().version,
        fixture.dataset.manifest().version
    );
    Ok(())
}

#[tokio::test]
async fn upstream_valid_inline_index_remains_readable() -> TestResult {
    let mut fixture = Fixture::new().await?;
    fixture
        .dataset
        .create_index_builder(&["body"], IndexType::BTree, &ScalarIndexParams::default())
        .name("body_index".into())
        .await?;
    let reopened = Dataset::open(&fixture.uri).await?;
    assert!(reopened.manifest().index_section.is_some());
    let store = reopened.object_store(None).await?;
    let indices = lance_table::io::manifest::read_manifest_indexes(
        &store,
        reopened.manifest_location(),
        reopened.manifest(),
    )
    .await?;
    assert_eq!(indices.len(), 1);
    assert_eq!(indices[0].name, "body_index");
    assert_eq!(reopened.count_rows(None).await?, 1);
    Ok(())
}
