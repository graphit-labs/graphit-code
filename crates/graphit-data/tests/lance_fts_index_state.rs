use arrow_array::{ArrayRef, RecordBatch, StringArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_data::lance_branch_store::JsonFtsIndexMode;
use graphit_data::{
    lance_branch_store::{BranchError, BranchSnapshot, JsonFtsIndexState, LanceBranchStore},
    lance_module_store::LanceModuleStore,
};
use lance::{Dataset, index::DatasetIndexExt};
use lance_index::{
    IndexType,
    scalar::{InvertedIndexParams, ScalarIndexParams, inverted::DocumentGranularity},
};
use object_store::ObjectStoreExt;
use std::{error::Error, sync::Arc};
type TestResult = Result<(), Box<dyn Error>>;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("payload", DataType::Utf8, false),
        Field::new("other", DataType::Utf8, false),
        Field::new("marker", DataType::UInt64, false),
    ]))
}
fn rows(empty: bool) -> RecordBatch {
    let values = if empty { vec![] } else { vec!["a"] };
    let payload = if empty {
        vec![]
    } else {
        vec!["{\"body\":\"original\"}"]
    };
    let other = if empty {
        vec![]
    } else {
        vec!["{\"body\":\"foreign\"}"]
    };
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(values.clone())),
        Arc::new(StringArray::from(payload)),
        Arc::new(StringArray::from(other)),
        Arc::new(UInt64Array::from(vec![7; values.len()])),
    ];
    RecordBatch::try_new(schema(), columns).unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    uri: String,
    store: LanceBranchStore,
}
async fn fixture(empty: bool) -> Result<Fixture, Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let uri = temp
        .path()
        .join("index.lance")
        .to_string_lossy()
        .into_owned();
    LanceModuleStore::create(&uri, schema(), vec![rows(empty)]).await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    Ok(Fixture {
        _temp: temp,
        uri,
        store,
    })
}
async fn image(pin: &BranchSnapshot) -> Result<Vec<RecordBatch>, lance::Error> {
    pin.scan().try_into_stream().await?.try_collect().await
}
async fn metadata(uri: &str) -> Result<Vec<lance_table::format::IndexMetadata>, lance::Error> {
    let dataset = Dataset::open(uri).await?;
    let store = dataset.object_store(None).await?;
    lance_table::io::manifest::read_manifest_indexes(
        store.as_ref(),
        dataset.manifest_location(),
        dataset.manifest(),
    )
    .await
}
fn text_params() -> InvertedIndexParams {
    InvertedIndexParams::default()
        .base_tokenizer("simple".into())
        .max_token_length(None)
        .lower_case(true)
        .stem(false)
        .remove_stop_words(false)
        .ascii_folding(false)
}

#[tokio::test]
async fn missing_ready_old_named_and_inherited_pins_are_observational() -> TestResult {
    let f = fixture(false).await?;
    let old = f.store.capture_target(None).await?;
    let original = image(&old).await?;
    assert_eq!(
        old.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    let main = old
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let version = main.version();
    assert_eq!(
        main.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    assert_eq!(
        old.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    assert_eq!(
        main.json_fts_index_state("other").await?,
        JsonFtsIndexState::Missing
    );
    assert_eq!(image(&main).await?, original);
    assert_eq!(f.store.capture_target(None).await?.version(), version);

    let inherited = f.store.capture_target(Some("future")).await?;
    assert_eq!(inherited.inherited_target(), Some("future"));
    assert_eq!(
        inherited.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    assert!(matches!(
        Dataset::open(&f.uri).await?.branches().get("future").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    let named = inherited.materialize().await?;
    let named = named
        .prepare_json_fts_index("other", "other-content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    assert_eq!(
        named.json_fts_index_state("other").await?,
        JsonFtsIndexState::Ready
    );
    assert_eq!(
        named.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    assert_eq!(
        main.json_fts_index_state("other").await?,
        JsonFtsIndexState::Missing
    );
    assert_eq!(f.store.capture_target(None).await?.version(), version);
    assert_eq!(image(&named).await?, original);

    let empty = fixture(true).await?;
    let empty = empty.store.capture_target(None).await?;
    assert_eq!(
        empty.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    assert_eq!(empty.version(), 1);
    Ok(())
}

#[tokio::test]
async fn identical_metadata_details_do_not_certify_physical_json_policy() -> TestResult {
    let text = fixture(false).await?;
    let mut dataset = Dataset::open(&text.uri).await?;
    dataset
        .create_index(
            &["payload"],
            IndexType::Inverted,
            Some("content".into()),
            &text_params(),
            false,
        )
        .await?;
    let pin = text.store.capture_target(None).await?;
    let version = pin.version();
    assert_eq!(
        pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Incompatible
    );
    assert_eq!(text.store.capture_target(None).await?.version(), version);

    let json = fixture(false).await?;
    let pin = json.store.capture_target(None).await?;
    let pin = pin
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    assert_eq!(
        pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    assert_eq!(
        metadata(&text.uri).await?[0].index_details,
        metadata(&json.uri).await?[0].index_details
    );
    let mut dataset = Dataset::open(&json.uri).await?;
    let wrong = text_params()
        .lance_tokenizer("json".into())
        .lower_case(false);
    dataset
        .create_index(
            &["payload"],
            IndexType::Inverted,
            Some("content".into()),
            &wrong,
            true,
        )
        .await?;
    let wrong_pin = json.store.capture_target(None).await?;
    assert_eq!(
        wrong_pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Incompatible
    );
    assert_eq!(
        pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    Ok(())
}

#[tokio::test]
async fn column_binding_competing_names_and_invalid_columns_fail_closed() -> TestResult {
    let f = fixture(false).await?;
    let pin = f.store.capture_target(None).await?;
    let other = pin
        .prepare_json_fts_index("other", "other-content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    assert_eq!(
        other.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    assert_eq!(
        other.json_fts_index_state("other").await?,
        JsonFtsIndexState::Ready
    );
    let indexed = other
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let ambiguous = indexed
        .prepare_json_fts_index("payload", "another-content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let version = ambiguous.version();
    assert_eq!(
        ambiguous.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Incompatible
    );
    assert_eq!(
        ambiguous.json_fts_index_state("other").await?,
        JsonFtsIndexState::Ready
    );
    for column in ["missing", "marker"] {
        assert!(matches!(
            ambiguous.json_fts_index_state(column).await,
            Err(BranchError::JsonIndexColumn(value)) if value == column
        ));
    }
    assert_eq!(f.store.capture_target(None).await?.version(), version);
    assert_eq!(image(&ambiguous).await?, image(&pin).await?);
    Ok(())
}

#[tokio::test]
async fn unreadable_physical_index_and_manifest_are_errors_not_missing() -> TestResult {
    let f = fixture(false).await?;
    let pin = f.store.capture_target(None).await?;
    let pin = pin
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let dataset = Dataset::open(&f.uri).await?;
    let stored = metadata(&f.uri).await?;
    let directory = dataset.indices_dir().join(stored[0].uuid.to_string());
    let store = dataset.object_store(None).await?;
    let files: Vec<_> = store.inner.list(Some(&directory)).try_collect().await?;
    assert!(!files.is_empty());
    for file in files {
        store.inner.delete(&file.location).await?;
    }
    let error = pin.json_fts_index_state("payload").await.unwrap_err();
    assert!(matches!(error, BranchError::Sdk(_)));
    assert!(error.source().is_some());

    let error = match pin
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Replace)
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("unreadable physical replacement must fail"),
    };
    assert!(matches!(error, BranchError::Sdk(_)));
    assert!(error.source().is_some());

    // Use SDK-provided physical paths; this is only this private TempDir corpus.
    store
        .inner
        .delete(&dataset.manifest_location().path)
        .await?;
    let error = pin.json_fts_index_state("payload").await.unwrap_err();
    assert!(matches!(error, BranchError::Sdk(_)));
    assert!(error.source().is_some());
    let error = match pin
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Replace)
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("unreadable manifest replacement must fail"),
    };
    assert!(matches!(error, BranchError::Sdk(_)));
    assert!(error.source().is_some());
    Ok(())
}

#[tokio::test]
async fn scalar_index_and_rejected_list_granularity_do_not_authorize_json_fts() -> TestResult {
    let f = fixture(false).await?;
    let mut dataset = Dataset::open(&f.uri).await?;
    dataset
        .create_index(
            &["payload"],
            IndexType::BTree,
            Some("payload-btree".into()),
            &ScalarIndexParams::default(),
            false,
        )
        .await?;
    let pin = f.store.capture_target(None).await?;
    let version = pin.version();
    assert_eq!(
        pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    let before = metadata(&f.uri).await?;
    let wrong = text_params().document_granularity(DocumentGranularity::ListElement);
    assert!(matches!(
        dataset
            .create_index(
                &["payload"],
                IndexType::Inverted,
                Some("list-elements".into()),
                &wrong,
                false
            )
            .await,
        Err(lance::Error::InvalidInput { .. })
    ));
    assert_eq!(metadata(&f.uri).await?, before);
    assert_eq!(f.store.capture_target(None).await?.version(), version);
    assert_eq!(
        pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    Ok(())
}

#[tokio::test]
async fn empty_published_json_index_has_observed_physical_policy() -> TestResult {
    let f = fixture(true).await?;
    let old = f.store.capture_target(None).await?;
    let pin = old
        .prepare_json_fts_index("payload", "content", JsonFtsIndexMode::Create)
        .await?
        .commit()
        .await?;
    let version = pin.version();
    let before = metadata(&f.uri).await?;
    assert_eq!(before.len(), 1);
    assert_eq!(
        pin.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Ready
    );
    assert_eq!(
        old.json_fts_index_state("payload").await?,
        JsonFtsIndexState::Missing
    );
    assert_eq!(f.store.capture_target(None).await?.version(), version);
    assert_eq!(metadata(&f.uri).await?, before);
    assert_eq!(image(&pin).await?, image(&old).await?);
    Ok(())
}
