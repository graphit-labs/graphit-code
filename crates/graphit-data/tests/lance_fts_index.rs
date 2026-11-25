//! SDK qualification only: no production index publisher or Memory admission.
use arrow_array::{
    ArrayRef, BooleanArray, RecordBatch, RecordBatchIterator, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::{TryStreamExt, stream::BoxStream};
use lance::{
    Dataset,
    dataset::{
        CommitBuilder, WriteMode, WriteParams,
        refs::Ref,
        transaction::{Operation, Transaction as SdkTransaction, TransactionBuilder},
    },
    index::{CreateIndexBuilder, DatasetIndexExt},
};
use lance_index::{
    IndexType,
    scalar::{
        FullTextSearchQuery, InvertedIndexParams,
        inverted::query::{FtsQuery, MatchQuery, Operator},
    },
};
use lance_io::object_store::ObjectStore;
use lance_table::{
    format::{IndexMetadata, Manifest, Transaction},
    io::commit::{
        CommitError, CommitHandler, ManifestLocation, ManifestNamingScheme, ManifestWriter,
        PredecessorIdentity, commit_handler_from_url,
    },
};
use object_store::{ObjectStore as PublicObjectStore, path::Path};
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error, sync::Arc};
type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

// Actual 18-column Memory layout; isolated SDK rows are not domain-certified histories.
fn schema() -> Arc<Schema> {
    let fields = [
        ("row_key", DataType::Utf8),
        ("kind", DataType::Utf8),
        ("record_id", DataType::Utf8),
        ("project_id", DataType::Utf8),
        ("operation_version", DataType::Utf8),
        ("idempotency_key", DataType::Utf8),
        ("idempotency_actor", DataType::Utf8),
        ("title", DataType::Utf8),
        ("revision", DataType::UInt64),
        ("created_at", DataType::UInt64),
        ("updated_at", DataType::UInt64),
        ("created_by", DataType::Utf8),
        ("updated_by", DataType::Utf8),
        ("superseded", DataType::Boolean),
        ("tombstone", DataType::Boolean),
        ("reference_generation", DataType::UInt64),
        ("references", DataType::Utf8),
        ("payload", DataType::Utf8),
    ];
    Arc::new(Schema::new(
        fields
            .into_iter()
            .map(|(n, t)| Field::new(n, t, false))
            .collect::<Vec<_>>(),
    ))
}
fn rows(values: &[(&str, &str, bool, Value)]) -> RecordBatch {
    let fields = schema();
    let columns: Vec<ArrayRef> = fields
        .fields()
        .iter()
        .map(|f| match f.data_type() {
            DataType::UInt64 => Arc::new(UInt64Array::from(vec![1; values.len()])) as ArrayRef,
            DataType::Boolean => Arc::new(BooleanArray::from(
                values
                    .iter()
                    .map(|r| f.name() == "tombstone" && r.2)
                    .collect::<Vec<_>>(),
            )),
            DataType::Utf8 => Arc::new(StringArray::from(
                values
                    .iter()
                    .map(|r| match f.name().as_str() {
                        "row_key" => r.0.to_owned(),
                        "record_id" if r.0.starts_with("history-") => "same-logical-memory".into(),
                        "record_id" => r.0.to_owned(),
                        "kind" => r.1.to_owned(),
                        "payload" => serde_json::to_string(&r.3).unwrap(),
                        "references" => "[]".into(),
                        _ => "operationalmetadata".into(),
                    })
                    .collect::<Vec<_>>(),
            )),
            _ => unreachable!("closed fixture schema"),
        })
        .collect();
    RecordBatch::try_new(fields, columns).unwrap()
}
fn memory(title: &str, body: &str) -> Value {
    json!({"title":title,"body":body,"attributes":{"classification":"convention","mandatory":false,"important":true},"tags":["tagmarker"],"audit":"auditmarker"})
}
fn params(cap: Option<usize>) -> InvertedIndexParams {
    InvertedIndexParams::default()
        .lance_tokenizer("json".into())
        .base_tokenizer("simple".into())
        .max_token_length(cap)
        .lower_case(true)
        .stem(false)
        .remove_stop_words(false)
        .ascii_folding(false)
}
struct Fixture {
    _temp: tempfile::TempDir,
    uri: String,
    dataset: Dataset,
    delegate: Arc<dyn CommitHandler>,
}
async fn fixture(
    values: &[(&str, &str, bool, Value)],
) -> Result<Fixture, Box<dyn Error + Send + Sync>> {
    let temp = tempfile::tempdir()?;
    let uri = temp
        .path()
        .join("memory.lance")
        .to_string_lossy()
        .into_owned();
    let delegate = commit_handler_from_url(&uri, &None).await?;
    let batch = rows(values);
    let dataset = Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], schema()),
        &uri,
        Some(WriteParams {
            commit_handler: Some(Arc::clone(&delegate)),
            ..Default::default()
        }),
    )
    .await?;
    Ok(Fixture {
        _temp: temp,
        uri,
        dataset,
        delegate,
    })
}
async fn build(
    dataset: &Dataset,
    name: &str,
    cap: Option<usize>,
    replace: bool,
) -> lance::Result<IndexMetadata> {
    let mut captured = dataset.clone();
    let settings = params(cap);
    CreateIndexBuilder::new(&mut captured, &["payload"], IndexType::Inverted, &settings)
        .name(name.into())
        .replace(replace)
        .execute_uncommitted()
        .await
}
fn transaction(
    dataset: &Dataset,
    index: IndexMetadata,
    removed: Vec<IndexMetadata>,
) -> SdkTransaction {
    TransactionBuilder::new(
        dataset.version().version,
        Operation::CreateIndex {
            new_indices: vec![index],
            removed_indices: removed,
        },
    )
    .build()
}
async fn publish(
    dataset: &Dataset,
    delegate: &Arc<dyn CommitHandler>,
    tx: SdkTransaction,
) -> lance::Result<Dataset> {
    let location = dataset.branch_location();
    let fence = IndexFence {
        delegate: Arc::clone(delegate),
        path: location.path,
        branch: location.branch,
        version: dataset.version().version + 1,
    };
    CommitBuilder::new(Arc::new(dataset.clone()))
        .with_max_retries(0)
        .with_skip_auto_cleanup(true)
        .with_commit_handler(Arc::new(fence))
        .execute(tx)
        .await
}
const PATHS: &[&str] = &[
    "title",
    "body",
    "tags",
    "attributes.classification",
    "memory.title",
    "memory.body",
    "memory.tags",
    "memory.attributes.classification",
    "after.title",
    "after.body",
    "after.tags",
    "after.attributes.classification",
];
fn literal_query(value: &str) -> String {
    PATHS
        .iter()
        .flat_map(|path| {
            value
                .split(';')
                .filter(|v| !v.is_empty())
                .map(move |term| format!("{path},str,{term}"))
        })
        .collect::<Vec<_>>()
        .join(";")
}
async fn hits(
    dataset: &Dataset,
    query: &str,
    filter: Option<&str>,
) -> Result<BTreeSet<String>, Box<dyn Error + Send + Sync>> {
    let matched = MatchQuery::new(literal_query(query))
        .with_column(Some("payload".into()))
        .with_operator(Operator::Or)
        .with_fuzziness(Some(0));
    let mut scanner = dataset.scan();
    scanner
        .full_text_search(FullTextSearchQuery::new_query(FtsQuery::from(matched)).limit(None))?;
    scanner.project(&["row_key"])?;
    if let Some(filter) = filter {
        scanner.filter(filter)?;
        scanner.prefilter(true);
    }
    let batches: Vec<RecordBatch> = scanner.try_into_stream().await?.try_collect().await?;
    let mut out = BTreeSet::new();
    for batch in batches {
        let keys = batch
            .column_by_name("row_key")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for i in 0..batch.num_rows() {
            out.insert(keys.value(i).into());
        }
    }
    Ok(out)
}
async fn indexed(
    values: &[(&str, &str, bool, Value)],
    cap: Option<usize>,
) -> Result<Fixture, Box<dyn Error + Send + Sync>> {
    let mut f = fixture(values).await?;
    let index = build(&f.dataset, "content", cap, false).await?;
    let tx = transaction(&f.dataset, index, vec![]);
    f.dataset = publish(&f.dataset, &f.delegate, tx).await?;
    Ok(f)
}
#[tokio::test]
async fn uncommitted_strict_publication_pin_branch_and_uuid_recovery() -> TestResult {
    let mut f = fixture(&[("head", "head", false, memory("titlemarker", "bodymarker"))]).await?;
    let base = f.dataset.version().version;
    let named = f
        .dataset
        .create_branch("feature", Ref::Version(None, Some(base)), None)
        .await?;
    let index = build(&named, "content", None, false).await?;
    assert_eq!(index.dataset_version, base);
    assert_eq!(named.version().version, base);
    assert!(named.load_indices().await?.is_empty());
    let tx = transaction(&named, index.clone(), vec![]);
    let uuid = tx.uuid.clone();
    assert_ne!(uuid, index.uuid.to_string());
    let published = publish(&named, &f.delegate, tx).await?;
    assert_eq!(published.version().version, base + 1);
    assert_eq!(
        published.branch_location().branch.as_deref(),
        Some("feature")
    );
    assert_eq!(
        published
            .read_version_transaction(base + 1)
            .await?
            .transaction
            .unwrap()
            .uuid,
        uuid
    );
    assert_eq!(Dataset::open(&f.uri).await?.version().version, base);
    assert!(named.load_indices().await?.is_empty());
    let reopened = f
        .dataset
        .checkout_version(Ref::Version(Some("feature".into()), None))
        .await?;
    assert_eq!(
        hits(&reopened, "bodymarker", None).await?,
        BTreeSet::from(["head".into()])
    );
    Ok(())
}
#[tokio::test]
async fn foreign_winner_prevents_rebase_and_uuid_adoption() -> TestResult {
    let f = fixture(&[("old", "head", false, memory("old", "shared"))]).await?;
    let index = build(&f.dataset, "content", None, false).await?;
    let tx = transaction(&f.dataset, index, vec![]);
    let loser = tx.uuid.clone();
    let base = f.dataset.version().version;
    let winner = Dataset::write(
        RecordBatchIterator::new(
            vec![Ok(rows(&[(
                "winner",
                "head",
                false,
                memory("new", "shared"),
            )]))],
            schema(),
        ),
        &f.uri,
        Some(WriteParams {
            mode: WriteMode::Append,
            commit_handler: Some(Arc::clone(&f.delegate)),
            ..Default::default()
        }),
    )
    .await?;
    let failure = publish(&f.dataset, &f.delegate, tx).await.unwrap_err();
    println!("stale index cause: {failure}");
    assert!(!failure.to_string().is_empty());
    let reopened = Dataset::open(&f.uri).await?;
    assert_eq!(reopened.version().version, base + 1);
    assert_eq!(reopened.count_rows(None).await?, 2);
    let observed = reopened
        .read_version_transaction(base + 1)
        .await?
        .transaction
        .unwrap();
    assert_ne!(observed.uuid, loser);
    assert_eq!(
        observed.uuid,
        winner
            .read_version_transaction(base + 1)
            .await?
            .transaction
            .unwrap()
            .uuid
    );
    assert!(reopened.load_indices().await?.is_empty());
    Ok(())
}
#[tokio::test]
async fn json_paths_literal_encoding_unicode_long_tokens_and_tombstones() -> TestResult {
    let long = "x".repeat(160);
    let long_unicode = "é".repeat(40);
    let live = memory(
        "TitleMarker",
        "bodymarker alpha,beta gamma\ndelta função the Running my_symbol",
    );
    let archived = memory("archiveword", "archivedbody");
    let creation =
        json!({"memory":archived,"input":{"body":"inputmarker"},"reason":"reasonmarker"});
    let revision = json!({"after":memory("revisionword",&format!("{long} {long_unicode}")),"before":memory("beforemarker","beforemarker"),"reason":"reasonmarker"});
    let values = [
        ("head", "head", false, live),
        ("creation", "creation", false, creation),
        ("revision", "revision", false, revision),
        (
            "deleted",
            "head",
            true,
            memory("deletedword", "deletedword"),
        ),
    ];
    let f = indexed(&values, None).await?;

    for q in [
        "TitleMarker",
        "titlemarker",
        "bodymarker",
        "tagmarker",
        "convention",
        "alpha;beta",
        "alpha,beta",
        "gamma\ndelta",
        "função",
        "the",
        "Running",
        "my_symbol",
        "archiveword",
        "archivedbody",
        "revisionword",
        &long,
        &long_unicode,
        "FUNÇÃO",
    ] {
        assert!(!hits(&f.dataset, q, None).await?.is_empty(), "query={q}");
    }
    assert!(hits(&f.dataset, "funcao", None).await?.is_empty());
    assert!(hits(&f.dataset, "run", None).await?.is_empty());
    for field in ["tagmarker", "convention"] {
        assert_eq!(
            hits(&f.dataset, field, None).await?,
            BTreeSet::from([
                "head".into(),
                "creation".into(),
                "revision".into(),
                "deleted".into()
            ])
        );
    }
    for q in [
        "auditmarker",
        "inputmarker",
        "reasonmarker",
        "beforemarker",
        "operationalmetadata",
        "audit,str,auditmarker",
        "unused;reason,str,reasonmarker",
    ] {
        assert!(
            hits(&f.dataset, q, None).await?.is_empty(),
            "forbidden query={q}"
        );
    }
    assert_eq!(
        hits(&f.dataset, "deletedword", None).await?,
        BTreeSet::from(["deleted".into()])
    );
    assert!(
        hits(&f.dataset, "deletedword", Some("tombstone = false"))
            .await?
            .is_empty()
    );
    assert!(hits(&f.dataset, "nonexistent", None).await?.is_empty());
    Ok(())
}
#[tokio::test]
async fn explicit_token_cap_is_strict_utf8_bytes_and_none_removes_filter() -> TestResult {
    let f = indexed(
        &[
            (
                "ascii",
                "head",
                false,
                memory("", "aaaa bbbbb cccccc ddddddd"),
            ),
            ("unicode", "head", false, memory("", "éé ééé")),
        ],
        Some(6),
    )
    .await?;
    assert_eq!(
        hits(&f.dataset, "aaaa", None).await?,
        BTreeSet::from(["ascii".into()])
    );
    assert_eq!(
        hits(&f.dataset, "bbbbb", None).await?,
        BTreeSet::from(["ascii".into()])
    );
    assert!(hits(&f.dataset, "cccccc", None).await?.is_empty());
    assert!(hits(&f.dataset, "ddddddd", None).await?.is_empty());
    assert_eq!(
        hits(&f.dataset, "éé", None).await?,
        BTreeSet::from(["unicode".into()])
    );
    assert!(hits(&f.dataset, "ééé", None).await?.is_empty());
    Ok(())
}
#[tokio::test]
async fn eof_unindexed_append_known_refresh_and_old_pin() -> TestResult {
    let mut input = Vec::new();
    let keys = (0..120)
        .map(|i| {
            if i < 2 {
                format!("history-{i}")
            } else {
                format!("row{i}")
            }
        })
        .collect::<Vec<_>>();
    for key in &keys {
        input.push((
            key.as_str(),
            if key.starts_with("history-") {
                "revision"
            } else {
                "head"
            },
            false,
            if key.starts_with("history-") {
                json!({"after": memory("shared", "shared")})
            } else {
                memory("shared", "shared")
            },
        ));
    }
    let f = indexed(&input, None).await?;
    let pin = f.dataset.clone();
    let all = hits(&pin, "shared", None).await?;
    assert_eq!(all.len(), 120);
    assert!(all.contains("history-0") && all.contains("history-1"));
    let mut scan = pin.scan();
    scan.project(&["record_id"])?;
    let physical: Vec<RecordBatch> = scan.try_into_stream().await?.try_collect().await?;
    let aliases = physical
        .iter()
        .map(|batch| {
            let ids = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            (0..batch.num_rows())
                .filter(|i| ids.value(*i) == "same-logical-memory")
                .count()
        })
        .sum::<usize>();
    assert_eq!(aliases, 2);
    let appended = Dataset::write(
        RecordBatchIterator::new(
            vec![Ok(rows(&[(
                "append",
                "head",
                false,
                memory("new", "appendword shared"),
            )]))],
            schema(),
        ),
        &f.uri,
        Some(WriteParams {
            mode: WriteMode::Append,
            ..Default::default()
        }),
    )
    .await?;
    assert_eq!(
        hits(&appended, "appendword", None).await?,
        BTreeSet::from(["append".into()])
    );
    let known = appended.load_indices().await?.as_ref().clone();
    assert_eq!(known.len(), 1);
    let next_index = build(&appended, "content", None, true).await?;
    let next = publish(
        &appended,
        &f.delegate,
        transaction(&appended, next_index, known),
    )
    .await?;
    assert_eq!(hits(&next, "shared", None).await?.len(), 121);
    assert_eq!(hits(&pin, "shared", None).await?.len(), 120);
    assert!(hits(&pin, "appendword", None).await?.is_empty());
    assert_eq!(next.count_rows(None).await?, 121);
    Ok(())
}
#[tokio::test]
async fn malformed_json_sdk_outcome_is_not_no_match() -> TestResult {
    let mut batch = rows(&[("bad", "head", false, memory("", ""))]);
    let mut columns = batch.columns().to_vec();
    columns[17] = Arc::new(StringArray::from(vec!["{invalid"]));
    batch = RecordBatch::try_new(schema(), columns)?;
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("bad.lance").to_string_lossy().into_owned();
    let dataset = Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], schema()),
        &uri,
        None,
    )
    .await?;
    let outcome = tokio::spawn(async move { build(&dataset, "bad", None, false).await }).await;
    match outcome {
        Ok(Err(error)) => {
            println!("invalid JSON SDK error: {error:?}");
            assert!(matches!(error, lance::Error::IO { .. }));
            assert!(
                error
                    .source()
                    .unwrap()
                    .to_string()
                    .contains("JSON parse error")
            );
        }
        Err(error) => {
            println!("invalid JSON SDK join: {error:?}");
            assert!(error.is_panic(), "unexpected join failure {error}");
        }
        Ok(Ok(_)) => panic!("malformed JSON must not be qualified as valid indexed content"),
    }
    assert_eq!(Dataset::open(&uri).await?.version().version, 1);
    Ok(())
}

#[tokio::test]
async fn sdk_missing_empty_and_no_match_are_distinct_observations() -> TestResult {
    let f = fixture(&[]).await?;
    assert_eq!(f.dataset.count_rows(None).await?, 0);
    assert!(hits(&f.dataset, "absent", None).await?.is_empty());
    let missing = f._temp.path().join("missing.lance");
    assert!(Dataset::open(&missing.to_string_lossy()).await.is_err());
    assert!(!missing.exists());
    assert_eq!(Dataset::open(&f.uri).await?.version().version, 1);
    Ok(())
}

#[tokio::test]
async fn malformed_raw_triplet_query_is_an_error_not_empty_results() -> TestResult {
    let f = indexed(&[("row", "head", false, memory("", "alpha"))], None).await?;
    let outcome: lance::Result<Vec<RecordBatch>> = async {
        let mut scanner = f.dataset.scan();
        let raw = MatchQuery::new("body,str,alpha;beta".into()).with_column(Some("payload".into()));
        scanner.full_text_search(FullTextSearchQuery::new_query(raw.into()).limit(None))?;
        scanner.try_into_stream().await?.try_collect().await
    }
    .await;
    let error =
        outcome.expect_err("raw malformed JSON query must not return empty successful results");
    println!("malformed query SDK error: {error:?}");
    assert!(matches!(error, lance::Error::InvalidInput { .. }));
    assert!(error.to_string().contains("Invalid triple format"));
    assert_eq!(
        hits(&f.dataset, "alpha;beta", None).await?,
        BTreeSet::from(["row".into()])
    );
    Ok(())
}

#[derive(Debug)]
struct IndexFence {
    delegate: Arc<dyn CommitHandler>,
    path: Path,
    branch: Option<String>,
    version: u64,
}
impl IndexFence {
    fn check(&self, manifest: &Manifest, path: &Path) -> Result<(), CommitError> {
        if path != &self.path || manifest.branch != self.branch || manifest.version != self.version
        {
            Err(CommitError::CommitConflict)
        } else {
            Ok(())
        }
    }
}

#[async_trait::async_trait]
#[allow(clippy::too_many_arguments)]
impl CommitHandler for IndexFence {
    fn is_version_not_found_definitive(&self) -> bool {
        self.delegate.is_version_not_found_definitive()
    }
    fn propagate_commit_error_after_success(&self) -> bool {
        self.delegate.propagate_commit_error_after_success()
    }
    async fn resolve_latest_location(
        &self,
        path: &Path,
        store: &ObjectStore,
    ) -> lance_core::Result<ManifestLocation> {
        self.delegate.resolve_latest_location(path, store).await
    }
    async fn resolve_version_location(
        &self,
        path: &Path,
        version: u64,
        store: &dyn PublicObjectStore,
    ) -> lance_core::Result<ManifestLocation> {
        self.delegate
            .resolve_version_location(path, version, store)
            .await
    }
    async fn version_exists(
        &self,
        path: &Path,
        version: u64,
        store: &dyn PublicObjectStore,
        scheme: ManifestNamingScheme,
    ) -> lance_core::Result<bool> {
        self.delegate
            .version_exists(path, version, store, scheme)
            .await
    }
    fn list_detached_manifest_locations<'a>(
        &self,
        path: &Path,
        store: &'a ObjectStore,
    ) -> BoxStream<'a, lance_core::Result<ManifestLocation>> {
        self.delegate.list_detached_manifest_locations(path, store)
    }
    fn list_manifest_locations<'a>(
        &self,
        path: &Path,
        store: &'a ObjectStore,
        descending: bool,
    ) -> BoxStream<'a, lance_core::Result<ManifestLocation>> {
        self.delegate
            .list_manifest_locations(path, store, descending)
    }
    fn list_manifest_locations_since<'a>(
        &self,
        path: &Path,
        store: &'a ObjectStore,
        version: u64,
    ) -> BoxStream<'a, lance_core::Result<ManifestLocation>> {
        self.delegate
            .list_manifest_locations_since(path, store, version)
    }
    async fn commit(
        &self,
        manifest: &mut Manifest,
        indices: Option<Vec<IndexMetadata>>,
        path: &Path,
        store: &ObjectStore,
        writer: ManifestWriter,
        scheme: ManifestNamingScheme,
        transaction: Option<Transaction>,
    ) -> Result<ManifestLocation, CommitError> {
        self.check(manifest, path)?;
        self.delegate
            .commit(manifest, indices, path, store, writer, scheme, transaction)
            .await
    }
    fn supports_predecessor_condition(&self) -> bool {
        self.delegate.supports_predecessor_condition()
    }
    async fn resolve_latest_identity(
        &self,
        path: &Path,
        store: &ObjectStore,
    ) -> lance_core::Result<Option<PredecessorIdentity>> {
        self.delegate.resolve_latest_identity(path, store).await
    }
    async fn resolve_identity(
        &self,
        path: &Path,
        store: &ObjectStore,
        version: u64,
    ) -> lance_core::Result<Option<PredecessorIdentity>> {
        self.delegate.resolve_identity(path, store, version).await
    }
    async fn commit_after(
        &self,
        manifest: &mut Manifest,
        indices: Option<Vec<IndexMetadata>>,
        path: &Path,
        store: &ObjectStore,
        writer: ManifestWriter,
        scheme: ManifestNamingScheme,
        transaction: Option<Transaction>,
        predecessor: &PredecessorIdentity,
    ) -> Result<ManifestLocation, CommitError> {
        self.check(manifest, path)?;
        self.delegate
            .commit_after(
                manifest,
                indices,
                path,
                store,
                writer,
                scheme,
                transaction,
                predecessor,
            )
            .await
    }
    async fn forget_version(
        &self,
        path: &Path,
        version: u64,
        identity: &str,
    ) -> lance_core::Result<()> {
        self.delegate.forget_version(path, version, identity).await
    }
    async fn delete(&self, path: &Path) -> lance_core::Result<()> {
        self.delegate.delete(path).await
    }
}
