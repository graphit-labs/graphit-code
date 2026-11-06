use arrow_array::{
    ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::{StreamExt, TryStreamExt, stream::BoxStream};
use graphit_data::lance_branch_store::{
    BranchError, BranchSnapshot, DeltaRecoveryKey, DeltaResolution, LanceBranchStore,
    PreparedOverwrite,
};
use graphit_data::lance_module_store::LanceModuleStore;
use lance_io::object_store::{ObjectStoreParams, WrappingObjectStore};
use object_store::{
    CopyOptions, GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, ObjectStore,
    ObjectStoreExt, PutMultipartOptions, PutOptions, PutPayload, PutResult, RenameOptions,
    list::PaginatedListStore, path::Path,
};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
};
type TestResult = Result<(), Box<dyn Error>>;
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("family", DataType::Utf8, false),
        Field::new("id", DataType::Utf8, true),
        Field::new("revision", DataType::UInt64, false),
        Field::new("state", DataType::Utf8, false),
    ]))
}
fn rows(rows: &[(&str, Option<&str>, u64, &str)]) -> RecordBatch {
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.1).collect::<Vec<_>>(),
        )),
        Arc::new(UInt64Array::from(
            rows.iter().map(|r| r.2).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.3).collect::<Vec<_>>(),
        )),
    ];
    RecordBatch::try_new(schema(), columns).unwrap()
}
fn image(revision: u64) -> RecordBatch {
    rows(&[
        ("head", Some("t"), revision, "Running:claim:epoch:run"),
        ("head", Some("untouched"), 7, "Done"),
    ])
}
fn keys() -> Vec<String> {
    vec!["family".into(), "id".into()]
}
async fn values(
    snapshot: &BranchSnapshot,
) -> Result<BTreeMap<(String, String), (u64, String)>, lance::Error> {
    let batches: Vec<RecordBatch> = snapshot
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    let mut values = BTreeMap::new();
    for batch in batches {
        let family = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let id = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let rev = batch
            .column(2)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        let state = batch
            .column(3)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for i in 0..batch.num_rows() {
            values.insert(
                (family.value(i).to_owned(), id.value(i).to_owned()),
                (rev.value(i), state.value(i).to_owned()),
            );
        }
    }
    Ok(values)
}
async fn fixture() -> Result<(tempfile::TempDir, String, LanceModuleStore), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let uri = temp
        .path()
        .join("records.lance")
        .to_string_lossy()
        .into_owned();
    let base = LanceModuleStore::create(&uri, schema(), vec![image(1)]).await?;
    Ok((temp, uri, base))
}

fn vector_schema(width: i32, lineage: &str) -> Arc<Schema> {
    Arc::new(Schema::new_with_metadata(
        vec![
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    width,
                ),
                false,
            ),
        ],
        [("lineage".into(), lineage.into())].into(),
    ))
}

fn vector_row(schema: Arc<Schema>, components: &[f32]) -> RecordBatch {
    let vector = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        components.len().try_into().unwrap(),
        Arc::new(Float32Array::from(components.to_vec())),
        None,
    )
    .unwrap();
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec!["retained"])),
            Arc::new(vector),
        ],
    )
    .unwrap()
}

async fn scanned_rows(snapshot: &BranchSnapshot) -> Result<usize, lance::Error> {
    let batches: Vec<RecordBatch> = snapshot
        .scan()
        .try_into_stream()
        .await?
        .try_collect()
        .await?;
    Ok(batches.iter().map(RecordBatch::num_rows).sum())
}

#[tokio::test]
async fn public_observed_overwrite_preserves_branch_schema_pins_and_strict_cas() -> TestResult {
    let temp = tempfile::tempdir()?;
    let uri = temp
        .path()
        .join("vectors.lance")
        .to_string_lossy()
        .into_owned();
    let default_schema = vector_schema(2, "default");
    LanceModuleStore::create(
        &uri,
        default_schema.clone(),
        vec![vector_row(default_schema.clone(), &[1., 0.])],
    )
    .await?;
    let default = BranchSnapshot::capture_observed(&uri, None, None).await?;
    let inherited = BranchSnapshot::capture_observed(&uri, Some("feature"), None).await?;
    assert_eq!(inherited.inherited_target(), Some("feature"));
    assert_eq!(inherited.physical_branch(), None);
    assert_eq!(inherited.schema(), default_schema.as_ref());
    let initial_named = inherited.materialize().await?;
    let named_schema = vector_schema(3, "named");
    let published = initial_named
        .prepare_overwrite(vec![vector_row(named_schema.clone(), &[1., 0., 0.])])
        .await?
        .commit()
        .await?;
    let named = BranchSnapshot::capture_observed(&uri, Some("feature"), None).await?;
    assert_eq!(named.physical_branch(), Some("feature"));
    assert_eq!(named.inherited_target(), None);
    assert_eq!(named.schema(), named_schema.as_ref());
    assert_eq!(named.version(), published.version());
    assert_eq!(scanned_rows(&named).await?, 1);

    let empty_schema = vector_schema(4, "empty");
    let pending: PreparedOverwrite = named
        .prepare_overwrite(vec![RecordBatch::new_empty(empty_schema.clone())])
        .await?;
    let winner_key = pending.recovery_key().clone();
    let stale = named
        .prepare_overwrite(vec![vector_row(named_schema.clone(), &[0., 1., 0.])])
        .await?;
    let stale_key = stale.recovery_key().clone();
    assert_ne!(winner_key.transaction_id(), stale_key.transaction_id());
    assert_eq!(winner_key.physical_branch(), Some("feature"));
    assert_eq!(winner_key.base_version(), named.version());
    let before = BranchSnapshot::capture_observed(&uri, Some("feature"), None).await?;
    assert_eq!(before.version(), named.version());
    assert_eq!(before.schema(), named_schema.as_ref());
    assert_eq!(scanned_rows(&before).await?, 1);

    let empty = pending.commit().await?;
    assert_eq!(empty.version(), named.version() + 1);
    assert_eq!(empty.physical_branch(), Some("feature"));
    assert_eq!(empty.schema(), empty_schema.as_ref());
    assert_eq!(scanned_rows(&empty).await?, 0);
    let failure = stale
        .commit()
        .await
        .err()
        .expect("stale overwrite must not rebase");
    assert_eq!(failure.recovery_key(), &stale_key);
    assert!(failure.source().is_some());
    let current = BranchSnapshot::capture_observed(&uri, Some("feature"), None).await?;
    assert_eq!(current.version(), empty.version());
    assert_eq!(current.schema(), empty_schema.as_ref());
    assert_eq!(scanned_rows(&current).await?, 0);
    assert_eq!(
        current.resolve(&winner_key).await?,
        DeltaResolution::Applied {
            version: empty.version()
        }
    );
    assert_eq!(
        current.resolve(&stale_key).await?,
        DeltaResolution::Conflicted {
            version: empty.version()
        }
    );
    assert_eq!(named.schema(), named_schema.as_ref());
    assert_eq!(scanned_rows(&named).await?, 1);
    assert_eq!(initial_named.schema(), default_schema.as_ref());
    assert_eq!(scanned_rows(&initial_named).await?, 1);
    let latest_default = BranchSnapshot::capture_observed(&uri, None, None).await?;
    assert_eq!(latest_default.version(), default.version());
    assert_eq!(latest_default.schema(), default_schema.as_ref());
    assert_eq!(scanned_rows(&latest_default).await?, 1);
    Ok(())
}

#[tokio::test]
async fn public_overwrite_rejects_missing_schema_mixed_batches_and_inherited_target() -> TestResult
{
    let (_temp, uri, _) = fixture().await?;
    let snapshot = BranchSnapshot::capture_observed(&uri, None, None).await?;
    assert!(matches!(
        snapshot.prepare_overwrite(vec![]).await,
        Err(BranchError::EmptyPatch)
    ));
    assert!(matches!(
        snapshot
            .prepare_overwrite(vec![
                image(2),
                RecordBatch::new_empty(vector_schema(2, "other"))
            ])
            .await,
        Err(BranchError::SchemaMismatch)
    ));
    let inherited = BranchSnapshot::capture_observed(&uri, Some("absent"), None).await?;
    assert!(matches!(
        inherited
            .prepare_overwrite(vec![RecordBatch::new_empty(schema())])
            .await,
        Err(BranchError::InheritedSnapshot)
    ));
    let still_inherited = BranchSnapshot::capture_observed(&uri, Some("absent"), None).await?;
    assert_eq!(still_inherited.inherited_target(), Some("absent"));
    assert_eq!(still_inherited.version(), snapshot.version());
    assert_eq!(values(&snapshot).await?, values(&still_inherited).await?);
    Ok(())
}

#[tokio::test]
async fn captured_default_pin_fork_and_atomic_delta_preserve_rows() -> TestResult {
    let (_temp, uri, base) = fixture().await?;
    let base = base
        .snapshot()
        .prepare_replace(vec![image(2)])
        .await?
        .commit()
        .await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let inherited = store.capture_target(Some("feature")).await?;
    assert_eq!(inherited.version(), 2);
    assert_eq!(inherited.inherited_target(), Some("feature"));
    assert!(matches!(
        inherited.prepare_merge(vec![image(3)], keys()).await,
        Err(BranchError::InheritedSnapshot)
    ));
    base.snapshot()
        .prepare_replace(vec![image(3)])
        .await?
        .commit()
        .await?;
    let old_values = values(&inherited).await?;
    let child = inherited.materialize().await?;
    assert_eq!(child.version(), 2);
    assert_eq!(child.physical_branch(), Some("feature"));
    assert_eq!(values(&child).await?, old_values);
    let patch = rows(&[
        ("head", Some("t"), 9, "Running:claim:epoch:run"),
        ("history", Some("t-9"), 9, "immutable"),
        ("audit", Some("a-9"), 9, "event"),
        ("reference", Some("r-9"), 9, "link"),
    ]);
    let pending = child.prepare_merge(vec![patch], keys()).await?;
    let key = pending.recovery_key().clone();
    assert_eq!(values(&child).await?, old_values);
    let next = pending.commit().await?;
    assert_eq!(next.version(), 3);
    let new_values = values(&next).await?;
    assert_eq!(new_values.len(), 5);
    assert_eq!(
        new_values[&("head".into(), "untouched".into())],
        (7, "Done".into())
    );
    assert_eq!(new_values[&("head".into(), "t".into())].0, 9);
    assert_eq!(values(&child).await?, old_values);
    assert_eq!(
        next.resolve(&key).await?,
        DeltaResolution::Applied { version: 3 }
    );
    assert_eq!(store.capture_target(None).await?.version(), 3);
    assert_eq!(values(&store.capture_target(None).await?).await?.len(), 2);
    assert!(matches!(
        store.capture_target(None).await?.resolve(&key).await,
        Err(BranchError::RecoveryBinding)
    ));
    let reopened = store.capture_target(Some("feature")).await?;
    assert_eq!(values(&reopened).await?, new_values);
    let mut scan = reopened.scan();
    scan.filter("family = 'audit'")?.project(&["id"])?;
    let output: Vec<RecordBatch> = scan.try_into_stream().await?.try_collect().await?;
    assert_eq!(output.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
    Ok(())
}

#[tokio::test]
async fn historical_inherited_pin_survives_parent_commits_and_child_materialization() -> TestResult
{
    let (_temp, uri, base) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let inherited = store.capture_target(Some("feature")).await?;
    let original = values(&inherited).await?;
    base.snapshot()
        .prepare_replace(vec![image(2)])
        .await?
        .commit()
        .await?;

    let historical = inherited.at_version(1).await?;
    assert_eq!(historical.corpus_uri(), uri);
    assert_eq!(historical.version(), 1);
    assert_eq!(historical.physical_branch(), None);
    assert_eq!(historical.inherited_target(), Some("feature"));
    assert_eq!(values(&historical).await?, original);
    assert_eq!(store.capture_target(None).await?.version(), 2);
    assert_eq!(
        store
            .capture_target(Some("feature"))
            .await?
            .inherited_target(),
        Some("feature")
    );
    assert!(matches!(
        historical.prepare_overwrite(vec![image(9)]).await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert!(matches!(
        historical.prepare_merge(vec![image(9)], keys()).await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert!(matches!(
        historical.clone().retire_named().await,
        Err(BranchError::InheritedSnapshot)
    ));

    let child = inherited.clone().materialize().await?;
    let child = child
        .prepare_merge(vec![image(9)], keys())
        .await?
        .commit()
        .await?;
    let retained = historical.at_version(1).await?;
    assert_eq!(retained.physical_branch(), None);
    assert_eq!(retained.inherited_target(), Some("feature"));
    assert_eq!(retained.version(), 1);
    assert_eq!(values(&retained).await?, original);
    assert!(matches!(
        retained.prepare_merge(vec![image(10)], keys()).await,
        Err(BranchError::InheritedSnapshot)
    ));
    let named = store.capture_target(Some("feature")).await?;
    assert_eq!(named.inherited_target(), None);
    assert_eq!(named.physical_branch(), Some("feature"));
    assert_eq!(values(&named).await?, values(&child).await?);
    assert_ne!(values(&named).await?, original);
    assert_eq!(store.capture_target(None).await?.version(), 2);
    Ok(())
}

#[tokio::test]
async fn historical_versions_remain_on_the_admitted_default_or_named_branch() -> TestResult {
    let (_temp, uri, base) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let default = store.capture_target(None).await?;
    let child = store
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await?;
    let child = child
        .prepare_merge(vec![image(9)], keys())
        .await?
        .commit()
        .await?;
    base.snapshot()
        .prepare_replace(vec![image(2)])
        .await?
        .commit()
        .await?;

    let default_latest = default.at_version(2).await?;
    let named_latest = child.at_version(2).await?;
    assert_eq!(default_latest.physical_branch(), None);
    assert_eq!(named_latest.physical_branch(), Some("feature"));
    assert_eq!(default_latest.inherited_target(), None);
    assert_eq!(named_latest.inherited_target(), None);
    assert_ne!(values(&default_latest).await?, values(&named_latest).await?);
    assert_eq!(
        values(&default_latest.at_version(1).await?).await?,
        values(&default).await?
    );
    assert_eq!(
        values(&named_latest.at_version(1).await?).await?,
        values(&default).await?
    );
    assert!(matches!(
        default.at_version(0).await,
        Err(BranchError::InvalidVersion)
    ));
    assert!(default.at_version(u64::MAX).await.is_err());
    assert!(child.at_version(u64::MAX).await.is_err());
    let inherited = store.capture_target(Some("absent")).await?;
    assert!(matches!(
        inherited.at_version(0).await,
        Err(BranchError::InvalidVersion)
    ));
    assert!(inherited.at_version(u64::MAX).await.is_err());
    assert_eq!(
        store
            .capture_target(Some("absent"))
            .await?
            .inherited_target(),
        Some("absent")
    );
    Ok(())
}

#[tokio::test]
async fn provenance_inherited_resume_rejects_parent_after_fork_and_keeps_write_guards() -> TestResult
{
    let (_temp, uri, base) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let inherited = store.capture_target(Some("feature")).await?;
    let expected = values(&inherited).await?;
    let (_, absent_provenance) = inherited.at_version_with_provenance(1).await?;
    assert_eq!(absent_provenance, None);
    let child = inherited.materialize().await?;
    let child = child
        .prepare_merge(vec![image(9)], keys())
        .await?
        .commit()
        .await?;
    base.snapshot()
        .prepare_replace(vec![image(2)])
        .await?
        .commit()
        .await?;

    let (parent, provenance) = child.at_inherited_version(1).await?;
    let (named, named_provenance) = child.at_version_with_provenance(2).await?;
    assert_eq!(named_provenance.as_ref(), Some(&provenance));
    assert_eq!(parent.corpus_uri(), uri);
    assert_eq!(parent.physical_branch(), None);
    assert_eq!(parent.inherited_target(), Some("feature"));
    assert_eq!(values(&parent).await?, expected);
    assert_ne!(values(&named).await?, expected);
    assert!(matches!(
        parent.prepare_merge(vec![image(10)], keys()).await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert!(matches!(
        parent.prepare_overwrite(vec![image(10)]).await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert!(matches!(
        parent.retire_named().await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert!(matches!(
        child.at_inherited_version(0).await,
        Err(BranchError::InvalidVersion)
    ));
    assert!(matches!(
        child.at_inherited_version(2).await,
        Err(BranchError::InvalidVersion)
    ));
    assert_eq!(store.capture_target(None).await?.version(), 2);
    Ok(())
}

#[tokio::test]
async fn provenance_parent_requires_admitted_direct_default_child() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let default = store.capture_target(None).await?;
    assert!(matches!(
        default.at_inherited_version(1).await,
        Err(BranchError::InvalidVersion)
    ));
    let absent = store.capture_target(Some("absent")).await?;
    assert!(matches!(
        absent.at_inherited_version(1).await,
        Err(BranchError::InvalidVersion)
    ));
    let mut sdk = lance::Dataset::open(&uri).await?;
    sdk.create_branch(
        "source",
        lance::dataset::refs::Ref::Version(None, Some(1)),
        None,
    )
    .await?;
    sdk.create_branch(
        "other",
        lance::dataset::refs::Ref::Version(Some("source".into()), Some(1)),
        None,
    )
    .await?;
    let other = store.capture_target(Some("other")).await?;
    assert!(matches!(
        other.at_inherited_version(1).await,
        Err(BranchError::InvalidVersion)
    ));
    let (read, provenance) = other.at_version_with_provenance(1).await?;
    assert_eq!(read.physical_branch(), Some("other"));
    assert!(provenance.is_some());
    assert_eq!(
        store
            .capture_target(Some("absent"))
            .await?
            .inherited_target(),
        Some("absent")
    );
    Ok(())
}

#[tokio::test]
async fn provenance_named_recreation_reopens_current_identity_and_preserves_other_branch()
-> TestResult {
    let (_temp, uri, base) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let sibling = store
        .capture_target(Some("sibling"))
        .await?
        .materialize()
        .await?;
    let original = store
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await?;
    let original = original
        .prepare_merge(vec![image(9)], keys())
        .await?
        .commit()
        .await?;
    let (old_pin, old_provenance) = original
        .at_version_with_provenance(2)
        .await
        .map_err(|e| std::io::Error::other(format!("capture old named version: {e}")))?;
    let old_provenance = old_provenance.unwrap();
    let serialized = serde_json::to_vec(&old_provenance)?;
    let old_provenance: graphit_data::lance_branch_store::BranchProvenance =
        serde_json::from_slice(&serialized)?;
    let mut extended = serde_json::to_value(&old_provenance)?;
    extended["unexpected"] = serde_json::Value::Bool(true);
    assert!(
        serde_json::from_value::<graphit_data::lance_branch_store::BranchProvenance>(extended)
            .is_err()
    );
    assert_eq!(values(&old_pin).await?, values(&original).await?);
    let (_, sibling_provenance) = sibling.at_version_with_provenance(1).await?;
    assert_ne!(sibling_provenance.as_ref(), Some(&old_provenance));
    base.snapshot()
        .prepare_replace(vec![image(2)])
        .await?
        .commit()
        .await?;
    assert_eq!(store.capture_target(None).await?.version(), 2);
    original.clone().retire_named().await?;
    let default_after_retirement = store
        .capture_target(None)
        .await
        .map_err(|e| std::io::Error::other(format!("capture default after retirement: {e}")))?;
    assert_eq!(default_after_retirement.version(), 2);
    let recreated = store
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await
        .map_err(|e| std::io::Error::other(format!("materialize replacement: {e}")))?;
    assert_eq!(recreated.version(), original.version());
    let (fresh, new_provenance) = original
        .at_version_with_provenance(2)
        .await
        .map_err(|e| std::io::Error::other(format!("capture recreated named version: {e}")))?;
    assert_ne!(new_provenance.as_ref(), Some(&old_provenance));
    assert_eq!(values(&fresh).await?, values(&recreated).await?);
    let (_, new_inherited_provenance) = original
        .at_inherited_version(2)
        .await
        .map_err(|e| std::io::Error::other(format!("capture recreated inherited version: {e}")))?;
    assert_eq!(new_provenance.as_ref(), Some(&new_inherited_provenance));
    assert_eq!(
        values(&sibling).await?,
        values(&store.capture_target(Some("sibling")).await?).await?
    );
    Ok(())
}

#[tokio::test]
async fn provenance_capture_rejects_retirement_and_recreation_between_reference_reads() -> TestResult
{
    let (_temp, uri, _) = fixture().await?;
    let plain = LanceBranchStore::open(&uri, schema(), None).await?;
    plain
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await?;
    let (store, faults) = fault_store(&uri).await?;
    let captured = store.capture_target(Some("feature")).await?;
    faults.pause_manifest.store(true, Ordering::SeqCst);
    let replace = async {
        faults.manifest_entered.notified().await;
        plain
            .capture_target(Some("feature"))
            .await?
            .retire_named()
            .await?;
        let replacement = plain
            .capture_target(Some("feature"))
            .await?
            .materialize()
            .await?;
        faults.manifest_resume.notify_one();
        Ok::<_, Box<dyn Error>>(replacement)
    };
    let (read, replacement) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(captured.at_inherited_version(1), replace)
    })
    .await?;
    replacement?;
    assert!(matches!(read, Err(BranchError::RecoveryBinding)));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bootstrap_race_and_visible_destination_never_adopt() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let a = store.capture_target(Some("race")).await?;
    let b = store.capture_target(Some("race")).await?;
    let late = b.clone();
    let (ar, br) = tokio::join!(a.materialize(), b.materialize());
    assert_ne!(ar.is_ok(), br.is_ok());
    let loser = ar.err().or(br.err()).unwrap();
    assert!(
        matches!(
            loser,
            BranchError::CreationConflict { .. } | BranchError::BootstrapIncomplete { .. }
        ),
        "{loser:?}"
    );
    assert!(matches!(
        late.materialize().await,
        Err(BranchError::CreationConflict { cause: None })
    ));
    assert_eq!(store.capture_target(Some("race")).await?.version(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disjoint_concurrent_and_late_patches_publish_only_base_plus_one() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let snapshot = store.capture_target(None).await?;
    let a = snapshot
        .prepare_merge(vec![rows(&[("head", Some("a"), 1, "a")])], keys())
        .await?;
    let b = snapshot
        .prepare_merge(vec![rows(&[("head", Some("b"), 1, "b")])], keys())
        .await?;
    let late = snapshot
        .prepare_merge(vec![rows(&[("head", Some("late"), 1, "late")])], keys())
        .await?;
    let ak = a.recovery_key().clone();
    let bk = b.recovery_key().clone();
    let lk = late.recovery_key().clone();
    let (ar, br) = tokio::join!(a.commit(), b.commit());
    assert_ne!(ar.is_ok(), br.is_ok());
    let winner = if ar.is_ok() { &ak } else { &bk };
    let loser = if ar.is_ok() { &bk } else { &ak };
    let failure = ar.err().or(br.err()).unwrap();
    assert_eq!(failure.recovery_key(), loser);
    assert!(failure.source().is_some());
    assert!(late.commit().await.is_err());
    let latest = store.capture_target(None).await?;
    assert_eq!(latest.version(), 2);
    assert_eq!(values(&latest).await?.len(), 3);
    assert_eq!(
        latest.resolve(winner).await?,
        DeltaResolution::Applied { version: 2 }
    );
    assert_eq!(
        latest.resolve(loser).await?,
        DeltaResolution::Conflicted { version: 2 }
    );
    assert_eq!(
        latest.resolve(&lk).await?,
        DeltaResolution::Conflicted { version: 2 }
    );
    assert_eq!(values(&snapshot).await?.len(), 2);
    Ok(())
}

#[derive(Debug, Default)]
struct Faults {
    mode: AtomicU8,
    calls: AtomicUsize,
    manifest_faults: AtomicUsize,
    pause_manifest: AtomicBool,
    manifest_entered: tokio::sync::Notify,
    manifest_resume: tokio::sync::Notify,
}
#[derive(Debug)]
struct FaultStore {
    inner: Arc<dyn ObjectStore>,
    faults: Arc<Faults>,
}
#[derive(Debug)]
struct Wrapper(Arc<Faults>);
impl WrappingObjectStore for Wrapper {
    fn wrap(&self, _: &str, original: Arc<dyn ObjectStore>) -> Arc<dyn ObjectStore> {
        Arc::new(FaultStore {
            inner: original,
            faults: Arc::clone(&self.0),
        })
    }
    fn wrap_paginated(
        &self,
        _: &str,
        _: Arc<dyn PaginatedListStore>,
    ) -> Option<Arc<dyn PaginatedListStore>> {
        None
    }
}
impl fmt::Display for FaultStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fault wrapper({})", self.inner)
    }
}
fn fault() -> object_store::Error {
    object_store::Error::Generic {
        store: "fixture",
        source: Box::new(std::io::Error::other("material injected transport failure")),
    }
}
fn manifest(path: &Path) -> bool {
    path.as_ref().ends_with(".manifest")
}
impl FaultStore {
    fn tick(&self) {
        self.faults.calls.fetch_add(1, Ordering::SeqCst);
    }
    fn before_manifest(&self, path: &Path, mode: u8) -> object_store::Result<()> {
        if mode == 2 && manifest(path) {
            self.faults.manifest_faults.fetch_add(1, Ordering::SeqCst);
            return Err(fault());
        }
        Ok(())
    }
    fn after_manifest(&self, path: &Path, mode: u8) -> object_store::Result<()> {
        if mode == 3 && manifest(path) {
            self.faults.manifest_faults.fetch_add(1, Ordering::SeqCst);
            self.faults.mode.store(4, Ordering::SeqCst);
            return Err(fault());
        }
        Ok(())
    }
}
#[async_trait::async_trait]
impl ObjectStore for FaultStore {
    async fn put_opts(
        &self,
        path: &Path,
        payload: PutPayload,
        opts: PutOptions,
    ) -> object_store::Result<PutResult> {
        self.tick();
        let mode = self.faults.mode.load(Ordering::SeqCst);
        if mode == 1 && path.as_ref().contains("/_refs/branches/") {
            return Err(fault());
        }
        self.before_manifest(path, mode)?;
        let result = self.inner.put_opts(path, payload, opts).await?;
        self.after_manifest(path, mode)?;
        Ok(result)
    }
    async fn put_multipart_opts(
        &self,
        path: &Path,
        opts: PutMultipartOptions,
    ) -> object_store::Result<Box<dyn MultipartUpload>> {
        self.tick();
        self.inner.put_multipart_opts(path, opts).await
    }
    async fn get_opts(&self, path: &Path, opts: GetOptions) -> object_store::Result<GetResult> {
        self.tick();
        if manifest(path) && !opts.head && self.faults.pause_manifest.swap(false, Ordering::SeqCst)
        {
            self.faults.manifest_entered.notify_one();
            self.faults.manifest_resume.notified().await;
        }
        if self.faults.mode.load(Ordering::SeqCst) == 4 && manifest(path) {
            return Err(fault());
        }
        self.inner.get_opts(path, opts).await
    }
    fn delete_stream(
        &self,
        paths: BoxStream<'static, object_store::Result<Path>>,
    ) -> BoxStream<'static, object_store::Result<Path>> {
        self.tick();
        if self.faults.mode.load(Ordering::SeqCst) < 5 {
            return self.inner.delete_stream(paths);
        }
        let inner = Arc::clone(&self.inner);
        let faults = Arc::clone(&self.faults);
        paths
            .and_then(move |path| {
                let inner = Arc::clone(&inner);
                let faults = Arc::clone(&faults);
                async move {
                    let mode = faults.mode.load(Ordering::SeqCst);
                    let branch = path.as_ref().contains("/_refs/branches/");
                    if (mode == 5 && branch) || (mode == 7 && !branch) {
                        return Err(fault());
                    }
                    inner.delete(&path).await?;
                    if mode == 6 && branch {
                        return Err(fault());
                    }
                    Ok(path)
                }
            })
            .boxed()
    }
    fn list(&self, prefix: Option<&Path>) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
        self.tick();
        self.inner.list(prefix)
    }
    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> object_store::Result<ListResult> {
        self.tick();
        self.inner.list_with_delimiter(prefix).await
    }
    async fn copy_opts(
        &self,
        from: &Path,
        to: &Path,
        opts: CopyOptions,
    ) -> object_store::Result<()> {
        self.tick();
        self.inner.copy_opts(from, to, opts).await
    }
    async fn rename_opts(
        &self,
        from: &Path,
        to: &Path,
        opts: RenameOptions,
    ) -> object_store::Result<()> {
        self.tick();
        let mode = self.faults.mode.load(Ordering::SeqCst);
        self.before_manifest(to, mode)?;
        self.inner.rename_opts(from, to, opts).await?;
        self.after_manifest(to, mode)
    }
}
async fn fault_store(uri: &str) -> Result<(LanceBranchStore, Arc<Faults>), Box<dyn Error>> {
    let faults = Arc::new(Faults::default());
    let params = ObjectStoreParams {
        object_store_wrapper: Some(Arc::new(Wrapper(Arc::clone(&faults)))),
        ..Default::default()
    };
    Ok((
        LanceBranchStore::open(uri, schema(), Some(params)).await?,
        faults,
    ))
}

#[tokio::test]
async fn orphan_manifest_is_incomplete_and_readback_failure_is_unknown() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let (store, faults) = fault_store(&uri).await?;
    let inherited = store.capture_target(Some("orphan")).await?;
    faults.mode.store(1, Ordering::SeqCst);
    let error = inherited.materialize().await.err().unwrap();
    assert!(matches!(
        error,
        BranchError::BootstrapIncomplete { cause: Some(_) }
    ));
    assert!(error.source().is_some());
    faults.mode.store(0, Ordering::SeqCst);
    let inherited = store.capture_target(Some("orphan")).await?;
    assert!(matches!(
        inherited.materialize().await,
        Err(BranchError::BootstrapIncomplete { cause: None })
    ));
    let unknown = store.capture_target(Some("unknown")).await?;
    faults.mode.store(4, Ordering::SeqCst);
    assert!(matches!(
        unknown.materialize().await,
        Err(BranchError::BootstrapUnknown { .. })
    ));
    faults.mode.store(0, Ordering::SeqCst);
    Ok(())
}

#[tokio::test]
async fn valid_fork_survives_failed_delta_and_lost_ack_recovers() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let (store, faults) = fault_store(&uri).await?;
    let child = store
        .capture_target(Some("valid"))
        .await?
        .materialize()
        .await?;
    let pending = child
        .prepare_merge(vec![rows(&[("head", Some("t"), 2, "changed")])], keys())
        .await?;
    let key = pending.recovery_key().clone();
    faults.manifest_faults.store(0, Ordering::SeqCst);
    faults.mode.store(2, Ordering::SeqCst);
    assert!(pending.commit().await.is_err());
    assert!(faults.manifest_faults.load(Ordering::SeqCst) > 0);
    faults.mode.store(0, Ordering::SeqCst);
    assert_eq!(store.capture_target(Some("valid")).await?.version(), 1);
    assert_eq!(child.resolve(&key).await?, DeltaResolution::Unresolved);
    assert_eq!(values(&child).await?[&("head".into(), "t".into())].0, 1);
    let pending = child
        .prepare_merge(vec![rows(&[("head", Some("t"), 3, "changed")])], keys())
        .await?;
    let bytes = serde_json::to_vec(pending.recovery_key())?;
    faults.manifest_faults.store(0, Ordering::SeqCst);
    faults.mode.store(3, Ordering::SeqCst);
    let failure = pending.commit().await.err().unwrap();
    assert!(faults.manifest_faults.load(Ordering::SeqCst) > 0);
    assert!(failure.sdk_error().is_commit_status_unknown());
    assert!(matches!(
        child.resolve(failure.recovery_key()).await,
        Err(BranchError::Sdk(_))
    ));
    faults.mode.store(0, Ordering::SeqCst);
    let key: DeltaRecoveryKey = serde_json::from_slice(&bytes)?;
    assert_eq!(
        child.resolve(&key).await?,
        DeltaResolution::Applied { version: 2 }
    );
    assert_eq!(store.capture_target(Some("valid")).await?.version(), 2);
    Ok(())
}

#[tokio::test]
async fn official_rename_commit_faults_preserve_pre_and_post_effect_state() -> TestResult {
    let (_temp, uri, base) = fixture().await?;
    let faults = Arc::new(Faults::default());
    for (mode, revision) in [(2, 2), (3, 3)] {
        let dataset = Arc::new(
            lance::dataset::builder::DatasetBuilder::from_uri(&uri)
                .with_store_params(ObjectStoreParams {
                    object_store_wrapper: Some(Arc::new(Wrapper(Arc::clone(&faults)))),
                    ..Default::default()
                })
                .load()
                .await?,
        );
        let transaction = lance::dataset::InsertBuilder::new(Arc::clone(&dataset))
            .with_params(&lance::dataset::WriteParams {
                mode: lance::dataset::WriteMode::Overwrite,
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute_uncommitted(vec![image(revision)])
            .await?;
        faults.manifest_faults.store(0, Ordering::SeqCst);
        faults.mode.store(mode, Ordering::SeqCst);
        let error = match lance::dataset::CommitBuilder::new(dataset)
            .with_commit_handler(Arc::new(lance_table::io::commit::RenameCommitHandler))
            .with_max_retries(0)
            .with_skip_auto_cleanup(true)
            .execute(transaction)
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("the transport fault must reach the official rename handler"),
        };
        assert!(faults.manifest_faults.load(Ordering::SeqCst) > 0);
        if mode == 3 {
            assert!(error.is_commit_status_unknown());
        }
        faults.mode.store(0, Ordering::SeqCst);
        let fresh = LanceBranchStore::open(&uri, schema(), None)
            .await?
            .capture_target(None)
            .await?;
        let (expected_version, expected_revision) = if mode == 2 { (1, 1) } else { (2, revision) };
        assert_eq!(fresh.version(), expected_version);
        assert_eq!(
            values(&fresh).await?[&("head".into(), "t".into())].0,
            expected_revision
        );
        assert_eq!(base.snapshot().version(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn validation_and_recovery_bindings_fail_before_sdk_io() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let (store, faults) = fault_store(&uri).await?;
    let snapshot = store.capture_target(None).await?;
    let valid = rows(&[("head", Some("t"), 2, "value")]);
    let duplicate = rows(&[("head", Some("t"), 2, "one"), ("head", Some("t"), 3, "two")]);
    let null = rows(&[("head", None, 2, "value")]);
    let wrong = RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new(
            "other",
            DataType::Utf8,
            false,
        )])),
        vec![Arc::new(StringArray::from(vec!["x"]))],
    )?;
    let before = faults.calls.load(Ordering::SeqCst);
    assert!(matches!(
        snapshot.prepare_merge(vec![], keys()).await,
        Err(BranchError::EmptyPatch)
    ));
    assert!(matches!(
        snapshot
            .prepare_merge(vec![RecordBatch::new_empty(schema())], keys())
            .await,
        Err(BranchError::EmptyPatch)
    ));
    assert!(matches!(
        snapshot.prepare_merge(vec![valid.clone()], vec![]).await,
        Err(BranchError::InvalidJoinColumns)
    ));
    assert!(matches!(
        snapshot
            .prepare_merge(vec![valid.clone()], vec!["id".into(), "id".into()])
            .await,
        Err(BranchError::InvalidJoinColumns)
    ));
    assert!(matches!(
        snapshot
            .prepare_merge(vec![valid.clone()], vec!["absent".into()])
            .await,
        Err(BranchError::InvalidJoinColumns)
    ));
    assert!(matches!(
        snapshot
            .prepare_merge(vec![valid.clone()], vec!["revision".into()])
            .await,
        Err(BranchError::InvalidJoinColumns)
    ));
    assert!(matches!(
        snapshot.prepare_merge(vec![duplicate], keys()).await,
        Err(BranchError::DuplicateKey)
    ));
    assert!(matches!(
        snapshot.prepare_merge(vec![null], keys()).await,
        Err(BranchError::NullKey)
    ));
    assert!(matches!(
        snapshot.prepare_merge(vec![wrong], keys()).await,
        Err(BranchError::SchemaMismatch)
    ));
    assert_eq!(faults.calls.load(Ordering::SeqCst), before);
    let pending = snapshot.prepare_merge(vec![valid], keys()).await?;
    let key = pending.recovery_key();
    let before = faults.calls.load(Ordering::SeqCst);
    for (field, value) in [
        ("physical_branch", serde_json::json!("other")),
        ("corpus_uri", serde_json::json!("other")),
        ("base_version", serde_json::json!(u64::MAX)),
        ("transaction_id", serde_json::json!("")),
    ] {
        let mut raw = serde_json::to_value(key)?;
        raw[field] = value;
        let altered: DeltaRecoveryKey = serde_json::from_value(raw)?;
        assert!(snapshot.resolve(&altered).await.is_err());
    }
    assert_eq!(faults.calls.load(Ordering::SeqCst), before);
    drop(pending);
    assert_eq!(store.capture_target(None).await?.version(), 1);
    let unsafe_uri = format!("unsafe://{uri}");
    assert!(matches!(
        LanceBranchStore::open(&unsafe_uri, schema(), None).await,
        Err(BranchError::UnsupportedCommitScheme(_))
    ));

    Ok(())
}

#[tokio::test]
async fn memory_selector_publishes_conditional_delta() -> TestResult {
    let temp = tempfile::tempdir()?;
    let uri = format!(
        "memory://{}/records.lance",
        temp.path().file_name().unwrap().to_string_lossy()
    );
    let memory = Arc::new(object_store::memory::InMemory::new());
    let params = ObjectStoreParams {
        object_store_wrapper: Some(Arc::new(MemoryWrapper(memory))),
        ..Default::default()
    };
    let write_params = lance::dataset::WriteParams {
        store_params: Some(params.clone()),
        skip_auto_cleanup: true,
        ..Default::default()
    };
    let base = lance::dataset::InsertBuilder::new(uri.as_str())
        .with_params(&write_params)
        .execute(vec![image(1)])
        .await?;
    let store = LanceBranchStore::open(&uri, schema(), Some(params)).await?;
    let snapshot = store.capture_target(None).await?;
    let a = snapshot
        .prepare_merge(vec![rows(&[("head", Some("a"), 2, "a")])], keys())
        .await?;
    let b = snapshot
        .prepare_merge(vec![rows(&[("head", Some("b"), 2, "b")])], keys())
        .await?;
    let next = a.commit().await?;
    assert!(b.commit().await.is_err());
    assert_eq!(next.version(), 2);
    assert_eq!(store.capture_target(None).await?.version(), 2);
    assert_eq!(values(&snapshot).await?.len(), 2);
    assert_eq!(base.version_id(), 1);
    Ok(())
}

#[derive(Debug)]
struct MemoryWrapper(Arc<object_store::memory::InMemory>);
impl WrappingObjectStore for MemoryWrapper {
    fn wrap(&self, _: &str, _: Arc<dyn ObjectStore>) -> Arc<dyn ObjectStore> {
        self.0.clone()
    }
    fn wrap_paginated(
        &self,
        _: &str,
        _: Arc<dyn PaginatedListStore>,
    ) -> Option<Arc<dyn PaginatedListStore>> {
        None
    }
}

#[tokio::test]
async fn present_manifest_missing_external_transaction_preserves_error() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let original = store.capture_target(None).await?;
    let pending = original
        .prepare_merge(vec![rows(&[("head", Some("t"), 2, "next")])], keys())
        .await?;
    let key = pending.recovery_key().clone();
    let next = pending.commit().await?;
    let sdk = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await?;
    let object_store = sdk.object_store(None).await?;
    let mut manifest = sdk.manifest().clone();
    manifest.transaction_file = Some("missing-external-transaction.txn".into());
    // Public SDK writer encodes a real readable legacy-style manifest, without an inline transaction.
    lance_table::io::commit::write_manifest_file_to_path(
        &object_store,
        &mut manifest,
        None,
        &sdk.manifest_location().path,
        None,
    )
    .await?;
    assert_eq!(store.capture_target(None).await?.version(), 2);
    assert_eq!(values(&next).await?.len(), 2);
    match original.resolve(&key).await {
        Err(BranchError::Sdk(cause)) => assert!(cause.is_not_found()),
        other => {
            panic!("present manifest must preserve external transaction read error: {other:?}")
        }
    }
    manifest.transaction_file = None;
    lance_table::io::commit::write_manifest_file_to_path(
        &object_store,
        &mut manifest,
        None,
        &sdk.manifest_location().path,
        None,
    )
    .await?;
    assert!(matches!(
        original.resolve(&key).await,
        Err(BranchError::MissingTransaction)
    ));
    Ok(())
}

#[tokio::test]
async fn named_branch_retirement_preserves_default_sibling_and_inherited_pin() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let default = store.capture_target(None).await?;
    let inherited = store.capture_target(Some("uncreated")).await?;
    let a = store.capture_target(Some("a")).await?.materialize().await?;
    let b = store.capture_target(Some("b")).await?.materialize().await?;
    let a = a
        .prepare_merge(vec![rows(&[("head", Some("t"), 9, "a")])], keys())
        .await?
        .commit()
        .await?;
    let sdk = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await?;
    a.clone().retire_named().await?;
    assert!(matches!(
        sdk.branches().get("a").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    assert_eq!(values(&default).await?[&("head".into(), "t".into())].0, 1);
    assert_eq!(values(&b).await?[&("head".into(), "t".into())].0, 1);
    assert_eq!(values(&inherited).await?, values(&default).await?);
    assert_eq!(inherited.inherited_target(), Some("uncreated"));
    assert!(matches!(
        sdk.branches().get("uncreated").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    // A captured Arc is not an SDK deletion lock. This pin needs the named
    // deletion file which cleanup has removed; preserve the actual read failure.
    assert!(matches!(
        values(&a).await,
        Err(lance::Error::NotFound { .. })
    ));
    assert!(
        sdk.checkout_version(lance::dataset::refs::Ref::Version(
            Some("a".into()),
            Some(a.version())
        ))
        .await
        .is_err()
    );
    assert_eq!(
        store.capture_target(Some("a")).await?.inherited_target(),
        Some("a")
    );
    assert_eq!(
        values(&store.capture_target(None).await?).await?,
        values(&default).await?
    );
    Ok(())
}

#[tokio::test]
async fn named_branch_retirement_rejects_descendants_and_tags_before_effects() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let mut sdk = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await?;
    let mut parent = sdk
        .create_branch(
            "parent",
            lance::dataset::refs::Ref::Version(None, None),
            None,
        )
        .await?;
    parent
        .create_branch(
            "child",
            lance::dataset::refs::Ref::Version(Some("parent".into()), None),
            None,
        )
        .await?;
    assert!(matches!(
        sdk.delete_branch("parent").await,
        Err(lance::Error::RefConflict { .. })
    ));
    assert!(sdk.branches().get("parent").await.is_ok());
    assert!(sdk.branches().get("child").await.is_ok());
    sdk.delete_branch("child").await?;
    sdk.tags()
        .create(
            "retained",
            lance::dataset::refs::Ref::Version(Some("parent".into()), Some(parent.version_id())),
        )
        .await?;
    assert!(matches!(
        sdk.delete_branch("parent").await,
        Err(lance::Error::RefConflict { .. })
    ));
    assert_eq!(
        sdk.tags().get("retained").await?.branch.as_deref(),
        Some("parent")
    );
    assert!(sdk.branches().get("parent").await.is_ok());
    sdk.tags().delete("retained").await?;
    sdk.delete_branch("parent").await?;
    assert!(matches!(
        sdk.branches().get("parent").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    assert_eq!(sdk.version_id(), 1);
    Ok(())
}

#[derive(Debug)]
struct RetirementMemoryWrapper {
    inner: Arc<object_store::memory::InMemory>,
    faults: Arc<Faults>,
}
impl WrappingObjectStore for RetirementMemoryWrapper {
    fn wrap(&self, _: &str, _: Arc<dyn ObjectStore>) -> Arc<dyn ObjectStore> {
        Arc::new(FaultStore {
            inner: self.inner.clone(),
            faults: Arc::clone(&self.faults),
        })
    }
    fn wrap_paginated(
        &self,
        _: &str,
        _: Arc<dyn PaginatedListStore>,
    ) -> Option<Arc<dyn PaginatedListStore>> {
        None
    }
}

#[tokio::test]
async fn named_branch_retirement_preserves_before_effect_lost_ack_and_cleanup_causes() -> TestResult
{
    for mode in [5, 6, 7] {
        let temp = tempfile::tempdir()?;
        let uri = format!(
            "memory://{}/retirement.lance",
            temp.path().file_name().unwrap().to_string_lossy()
        );
        let faults = Arc::new(Faults::default());
        let params = ObjectStoreParams {
            object_store_wrapper: Some(Arc::new(RetirementMemoryWrapper {
                inner: Arc::new(object_store::memory::InMemory::new()),
                faults: Arc::clone(&faults),
            })),
            ..Default::default()
        };
        let write = lance::dataset::WriteParams {
            store_params: Some(params.clone()),
            skip_auto_cleanup: true,
            ..Default::default()
        };
        let mut sdk = lance::dataset::InsertBuilder::new(uri.as_str())
            .with_params(&write)
            .execute(vec![image(1)])
            .await?;
        let branch = sdk
            .create_branch(
                "named",
                lance::dataset::refs::Ref::Version(None, None),
                None,
            )
            .await?;
        // Publish a branch-specific manifest so cleanup actually deletes an object.
        let branch = lance::dataset::InsertBuilder::new(Arc::new(branch))
            .with_params(&lance::dataset::WriteParams {
                mode: lance::dataset::WriteMode::Append,
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute(vec![image(2)])
            .await?;
        assert_eq!(branch.version_id(), 2);
        let store = LanceBranchStore::open(&uri, schema(), Some(params)).await?;
        let selected = store.capture_target(Some("named")).await?;
        faults.mode.store(mode, Ordering::SeqCst);
        let cause = selected
            .retire_named()
            .await
            .expect_err("real injected deletion failure");
        assert!(matches!(cause, BranchError::Sdk(_)));
        assert!(cause.source().is_some());
        assert!(
            cause
                .to_string()
                .contains("material injected transport failure"),
            "{cause}"
        );
        faults.mode.store(0, Ordering::SeqCst);
        let observation = sdk.branches().get("named").await;
        if mode == 5 {
            assert!(observation.is_ok());
        } else {
            assert!(matches!(observation, Err(lance::Error::RefNotFound { .. })));
        }
        assert_eq!(sdk.version_id(), 1);
        // Lost reference acknowledgement and cleanup failure both leave the
        // reference absent. That observation does not prove complete cleanup.
        if mode == 7 {
            assert!(
                sdk.checkout_version(lance::dataset::refs::Ref::Version(
                    Some("named".into()),
                    Some(2)
                ))
                .await
                .is_ok()
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn named_branch_retirement_reused_name_has_new_identity_not_pin_cas() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let mut sdk = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await?;
    sdk.create_branch(
        "named",
        lance::dataset::refs::Ref::Version(None, None),
        None,
    )
    .await?;
    let old = sdk.branches().get("named").await?.identifier;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let pin = store.capture_target(Some("named")).await?;
    sdk.delete_branch("named").await?;
    sdk.create_branch(
        "named",
        lance::dataset::refs::Ref::Version(None, None),
        None,
    )
    .await?;
    let current = sdk.branches().get("named").await?.identifier;
    assert_ne!(old, current);
    assert_eq!(pin.version(), 1);
    assert_eq!(store.capture_target(Some("named")).await?.version(), 1);
    // Both snapshots have the same scalar branch/version. It is not an expected
    // identifier fence: an uncoordinated delete acts on the current reference.
    sdk.delete_branch("named").await?;
    assert!(matches!(
        sdk.branches().get("named").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn named_branch_retirement_prepared_commit_and_recreation_need_owner_serialization()
-> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let store = LanceBranchStore::open(&uri, schema(), None).await?;
    let named = store
        .capture_target(Some("named"))
        .await?
        .materialize()
        .await?;
    let pending = named
        .prepare_merge(vec![rows(&[("head", Some("t"), 9, "late")])], keys())
        .await?;
    let mut sdk = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await?;
    sdk.delete_branch("named").await?;
    let commit = pending.commit().await;
    // The commit handler has a predecessor manifest fence, not a branch-reference
    // ownership fence. A prepared operation is not a promise of a live reference.
    assert!(matches!(
        sdk.branches().get("named").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    match commit {
        Ok(pin) => eprintln!(
            "prepared commit after reference removal published orphan version {}",
            pin.version()
        ),
        Err(cause) => {
            assert!(cause.source().is_some());
            eprintln!("prepared commit after reference removal failed: {cause}");
        }
    }
    let mut first = sdk.clone();
    let mut second = sdk.clone();
    let (a, b) = tokio::join!(
        first.create_branch(
            "recreated",
            lance::dataset::refs::Ref::Version(None, None),
            None
        ),
        second.create_branch(
            "recreated",
            lance::dataset::refs::Ref::Version(None, None),
            None
        )
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let creation_cause = a.err().or_else(|| b.err()).unwrap();
    eprintln!("simultaneous branch creation rejected one caller: {creation_cause:?}");
    assert!(sdk.branches().get("recreated").await.is_ok());
    let (a, b) = tokio::join!(
        first.delete_branch("recreated"),
        second.delete_branch("recreated")
    );
    assert!(a.is_ok() || b.is_ok());
    assert!(matches!(
        sdk.branches().get("recreated").await,
        Err(lance::Error::RefNotFound { .. })
    ));
    assert_eq!(
        values(&store.capture_target(None).await?).await?[&("head".into(), "t".into())].0,
        1
    );
    Ok(())
}

#[tokio::test]
async fn named_branch_retirement_refuses_default_and_inherited_before_io() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let (store, faults) = fault_store(&uri).await?;
    let default = store.capture_target(None).await?;
    let inherited = store.capture_target(Some("absent")).await?;
    let before = faults.calls.load(Ordering::SeqCst);
    assert!(matches!(
        default.retire_named().await,
        Err(BranchError::DefaultSnapshot)
    ));
    assert!(matches!(
        inherited.retire_named().await,
        Err(BranchError::InheritedSnapshot)
    ));
    assert_eq!(faults.calls.load(Ordering::SeqCst), before);
    assert_eq!(store.capture_target(None).await?.version(), 1);
    assert_eq!(
        store
            .capture_target(Some("absent"))
            .await?
            .inherited_target(),
        Some("absent")
    );
    Ok(())
}

#[tokio::test]
async fn retained_version_reuses_its_pin_but_named_provenance_is_still_observed() -> TestResult {
    let (_temp, uri, _) = fixture().await?;
    let (store, faults) = fault_store(&uri).await?;
    let default = store.capture_target(None).await?;
    let inherited = store.capture_target(Some("absent")).await?;
    let named = store
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await?;
    default
        .prepare_merge(vec![image(2)], keys())
        .await?
        .commit()
        .await?;
    for snapshot in [&default, &inherited, &named] {
        let before = faults.calls.load(Ordering::SeqCst);
        let retained = snapshot.at_version(snapshot.version()).await?;
        assert_eq!(faults.calls.load(Ordering::SeqCst), before);
        assert_eq!(retained.version(), snapshot.version());
        assert_eq!(retained.physical_branch(), snapshot.physical_branch());
        assert_eq!(retained.inherited_target(), snapshot.inherited_target());
        assert_eq!(values(&retained).await?, values(snapshot).await?);
        assert_eq!(values(&retained).await?[&("head".into(), "t".into())].0, 1);
    }
    let before = faults.calls.load(Ordering::SeqCst);
    let (_, provenance) = named.at_version_with_provenance(named.version()).await?;
    assert!(faults.calls.load(Ordering::SeqCst) > before);
    assert!(provenance.is_some());
    assert_eq!(store.capture_target(None).await?.version(), 2);
    Ok(())
}
