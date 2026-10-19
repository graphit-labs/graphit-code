use super::*;
use futures_util::TryStreamExt;
use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
use graphit_hub::remote_storage::registry_key;
use object_store::{ObjectStoreExt, memory::InMemory};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::Arc,
    time::{Duration, Instant},
};

fn policy() -> HubResourcePolicy {
    let brand = Brand::new("graphit").unwrap();
    let config = ConfigSnapshot::new(
        &brand,
        [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
    )
    .unwrap();
    HubResourcePolicy::resolve(&config).unwrap()
}
fn producer() -> ProjectId {
    "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()
}
fn metadata() -> Metadata {
    Metadata {
        provider: "fixture-provider".into(),
        artifact_type: crate::remote_file_install::client::Kind::Skill,
        id: "fixture/file-publisher".into(),
        version: "release/one".into(),
        name: "FILE publisher".into(),
        description: None,
        tags: vec![],
        author: None,
        dependencies: None,
        expected_sha256: None,
        set_latest: false,
    }
}
fn capture(
    root: &std::path::Path,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> CapturedSourceReader {
    let reader = CapturedSourceReader::capture(
        root,
        graphit_fs::NodeKind::Directory,
        &policy
            .snapshot_policy(
                graphit_core::artifact_store::CopyPolicy::RejectFileLinks,
                [],
            )
            .unwrap(),
        graphit_core::artifact_store::SourceReadLimits::new(
            64 * 1024,
            64 * 1024,
            policy.copy().total_file_bytes_limit(),
        )
        .unwrap(),
        Arc::clone(original.snapshot()),
    )
    .unwrap();
    assert_eq!(reader.retained_bytes(), 0);
    reader
}
fn seed(root: &std::path::Path, text: &str) {
    std::fs::create_dir_all(root.join("assets/empty")).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        format!("---\nname: file-publisher\ndescription: fixture\n---\n{text}"),
    )
    .unwrap();
    std::fs::write(root.join("assets/literal.txt"), text).unwrap();
}
async fn send(
    store: &dyn ObjectStore,
    metadata: &Metadata,
    root: &std::path::Path,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<Receipt> {
    let reader = capture(root, policy, original);
    let prepared = prepare(metadata, producer(), &reader, policy, original, &|| {
        original.check()
    })?;
    publish(
        Input {
            store,
            storage_prefix: "tenant/hub",
            prepared,
        },
        original,
        || original.check(),
    )
    .await
}
fn digest(text: &str) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap();
    }
    bytes
}
fn payload_key(metadata: &Metadata, hash: &str, policy: &HubResourcePolicy) -> ObjectPath {
    let (id, version) = metadata.validate(policy).unwrap();
    ObjectPath::from(format!(
        "tenant/hub/{}payload.zip",
        release_prefix(
            producer(),
            metadata.artifact_type.artifact_type(),
            &id,
            &version,
            &digest(hash)
        )
        .unwrap()
    ))
}
async fn catalog(
    store: &dyn ObjectStore,
    metadata: &Metadata,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> RemoteArtifactManifest {
    let (id, _) = metadata.validate(policy).unwrap();
    let key = ObjectPath::from(format!(
        "tenant/hub/{}",
        registry_key(producer(), metadata.artifact_type.artifact_type(), &id).unwrap()
    ));
    let bytes = store.get(&key).await.unwrap().bytes().await.unwrap();
    RemoteArtifactManifest::decode(
        &bytes,
        native_publication::metadata_limits(policy.content(), policy.projection()),
        original,
    )
    .unwrap()
}

#[tokio::test]
async fn final_zip_roundtrips_and_explicit_replacement_preserves_old_releases() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let root = tempfile::tempdir().unwrap();
    seed(root.path(), "payload_one");
    let store = InMemory::new();
    let mut metadata = metadata();
    metadata.dependencies = Some(vec![Dependency {
        artifact_type: ArtifactType::Rule,
        id: "fixture/declared".into(),
        selector: "release/one".into(),
    }]);
    let first = send(&store, &metadata, root.path(), &policy, &original)
        .await
        .unwrap();
    let first_key = payload_key(&metadata, &first.archive_sha256, &policy);
    let first_bytes = store.get(&first_key).await.unwrap().bytes().await.unwrap();
    assert_eq!(hex(&Sha256::digest(&first_bytes)), first.archive_sha256);
    let current = catalog(&store, &metadata, &policy, &original).await;
    assert!(current.latest().is_none());
    assert_eq!(current.dependencies().len(), 1);
    let mut spool = tempfile::tempfile().unwrap();
    spool.write_all(&first_bytes).unwrap();
    let parent = tempfile::tempdir().unwrap();
    let unpacked = graphit_hub::file_payload::stage_file_payload(
        &mut spool,
        parent.path(),
        policy.copy(),
        original.snapshot(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(unpacked.path().join("assets/literal.txt")).unwrap(),
        "payload_one"
    );
    assert!(unpacked.path().join("assets/empty").is_dir());

    metadata.version = "release/two".into();
    metadata.set_latest = true;
    metadata.dependencies = None;
    seed(root.path(), "payload_two");
    let second = send(&store, &metadata, root.path(), &policy, &original)
        .await
        .unwrap();
    let second_key = payload_key(&metadata, &second.archive_sha256, &policy);
    seed(root.path(), "changed_same_version");
    let error = send(&store, &metadata, root.path(), &policy, &original)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    let conflict = error.get_ref().unwrap().downcast_ref::<Failure>().unwrap();
    assert_eq!(conflict.receipt.payload, "not_attempted");
    metadata.expected_sha256 = Some(second.archive_sha256.clone());
    let replacement = send(&store, &metadata, root.path(), &policy, &original)
        .await
        .unwrap();
    assert_ne!(replacement.archive_sha256, second.archive_sha256);
    let repeated = send(&store, &metadata, root.path(), &policy, &original)
        .await
        .unwrap();
    assert_eq!(repeated.archive_sha256, replacement.archive_sha256);
    assert_eq!(
        store.get(&first_key).await.unwrap().bytes().await.unwrap(),
        first_bytes
    );
    assert!(store.get(&second_key).await.is_ok());
    let current = catalog(&store, &metadata, &policy, &original).await;
    assert_eq!(current.versions().len(), 2);
    assert_eq!(current.latest().unwrap().as_str(), "release/two");
    assert_eq!(current.dependencies().len(), 1);
    let payloads = store.list(None).try_collect::<Vec<_>>().await.unwrap();
    assert_eq!(
        payloads
            .iter()
            .filter(|o| o.location.as_ref().ends_with("/payload.zip"))
            .count(),
        3
    );
}

#[tokio::test]
async fn orphan_payload_is_not_adopted_by_digest_and_changed_source_reports_actual_partial() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let root = tempfile::tempdir().unwrap();
    seed(root.path(), "orphan");
    let metadata = metadata();
    let store = InMemory::new();
    let reader = capture(root.path(), &policy, &original);
    let prepared = prepare(&metadata, producer(), &reader, &policy, &original, &|| {
        original.check()
    })
    .unwrap();
    let mut bytes = Vec::new();
    prepared
        .archive
        .file
        .stream(0)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let key = payload_key(&metadata, &hex(&prepared.archive.sha256), &policy);
    store.put(&key, bytes.into()).await.unwrap();
    let error = publish(
        Input {
            store: &store,
            storage_prefix: "tenant/hub",
            prepared,
        },
        &original,
        || original.check(),
    )
    .await
    .unwrap_err();
    assert!(
        native_publication::cause_code(&error).contains("conflict")
            || error.kind() == io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        store
            .list(None)
            .try_collect::<Vec<_>>()
            .await
            .unwrap()
            .len(),
        1
    );

    let fresh = InMemory::new();
    let reader = capture(root.path(), &policy, &original);
    let prepared = prepare(&metadata, producer(), &reader, &policy, &original, &|| {
        original.check()
    })
    .unwrap();
    std::fs::write(
        root.path().join("assets/literal.txt"),
        "source changed after captured ZIP",
    )
    .unwrap();
    let error = publish(
        Input {
            store: &fresh,
            storage_prefix: "tenant/hub",
            prepared,
        },
        &original,
        || original.check(),
    )
    .await
    .unwrap_err();
    let effect = error.get_ref().unwrap().downcast_ref::<Failure>().unwrap();
    assert_eq!(effect.receipt.payload, "published");
    assert_eq!(effect.receipt.catalog, "not_attempted");
    assert_eq!(
        fresh
            .list(None)
            .try_collect::<Vec<_>>()
            .await
            .unwrap()
            .iter()
            .filter(|o| o.location.as_ref().ends_with("/payload.zip"))
            .count(),
        1
    );
}

#[tokio::test]
async fn original_cancel_and_expiry_before_storage_leave_no_objects() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    seed(root.path(), "cancel");
    let metadata = metadata();
    let store = InMemory::new();
    let reader = capture(root.path(), &policy, &original);
    let prepared = prepare(&metadata, producer(), &reader, &policy, &original, &|| {
        original.check()
    })
    .unwrap();
    cancel.cancel();
    assert_eq!(
        publish(
            Input {
                store: &store,
                storage_prefix: "tenant/hub",
                prepared
            },
            &original,
            || original.check()
        )
        .await
        .unwrap_err()
        .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(
        store
            .list(None)
            .try_collect::<Vec<_>>()
            .await
            .unwrap()
            .is_empty()
    );
    let expiry = Cancellation::default();
    let original = RequestContext::new(
        policy.content(),
        &expiry,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let reader = capture(root.path(), &policy, &original);
    let prepared = prepare(&metadata, producer(), &reader, &policy, &original, &|| {
        original.check()
    })
    .unwrap();
    tokio::time::sleep_until(original.deadline().unwrap().into()).await;
    assert_eq!(
        publish(
            Input {
                store: &store,
                storage_prefix: "tenant/hub",
                prepared
            },
            &original,
            || original.check()
        )
        .await
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(
        store
            .list(None)
            .try_collect::<Vec<_>>()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn actual_catalog_create_and_update_cas_reject_stale_competitors() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let mut metadata = metadata();
    let store = InMemory::new();
    let (id, _) = metadata.validate(&policy).unwrap();
    let limits = native_publication::metadata_limits(policy.content(), policy.projection());
    let identity = || CatalogIdentity {
        storage_prefix: "tenant/hub",
        producer: producer(),
        artifact_type: ArtifactType::Skill,
        id: &id,
        limits,
    };
    let a = native_publication::read_catalog(&store, identity(), &original, &|| original.check())
        .await
        .unwrap();
    let b = native_publication::read_catalog(&store, identity(), &original, &|| original.check())
        .await
        .unwrap();
    let first =
        encoded_catalog(&metadata, producer(), &[1u8; 32], None, limits, &original).unwrap();
    let mut winner = "not_attempted";
    let mut loser = "not_attempted";
    native_publication::write_catalog(&store, a, first.clone(), &mut winner, &original, &|| {
        original.check()
    })
    .await
    .unwrap();
    assert!(
        native_publication::write_catalog(&store, b, first, &mut loser, &original, &|| original
            .check())
        .await
        .is_err()
    );
    assert_eq!(winner, "published");
    assert_eq!(loser, "rejected");
    let a = native_publication::read_catalog(&store, identity(), &original, &|| original.check())
        .await
        .unwrap();
    let b = native_publication::read_catalog(&store, identity(), &original, &|| original.check())
        .await
        .unwrap();
    metadata.version = "release/two".into();
    let second = encoded_catalog(
        &metadata,
        producer(),
        &[2u8; 32],
        a.current.as_ref(),
        limits,
        &original,
    )
    .unwrap();
    metadata.version = "release/three".into();
    let third = encoded_catalog(
        &metadata,
        producer(),
        &[3u8; 32],
        b.current.as_ref(),
        limits,
        &original,
    )
    .unwrap();
    native_publication::write_catalog(&store, a, second, &mut winner, &original, &|| {
        original.check()
    })
    .await
    .unwrap();
    assert!(
        native_publication::write_catalog(&store, b, third, &mut loser, &original, &|| original
            .check())
        .await
        .is_err()
    );
    let current = catalog(&store, &metadata, &policy, &original).await;
    assert_eq!(current.versions().len(), 2);
    assert!(
        current
            .versions()
            .iter()
            .any(|v| v.version().as_str() == "release/two")
    );
    assert!(
        !current
            .versions()
            .iter()
            .any(|v| v.version().as_str() == "release/three")
    );
}

#[derive(Debug)]
struct AfterCatalog {
    inner: InMemory,
    catalog: ObjectPath,
    committed: std::sync::atomic::AtomicBool,
}
impl std::fmt::Display for AfterCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("catalog commit observation")
    }
}
#[async_trait::async_trait]
impl ObjectStore for AfterCatalog {
    async fn put_opts(
        &self,
        path: &ObjectPath,
        payload: object_store::PutPayload,
        opts: object_store::PutOptions,
    ) -> object_store::Result<object_store::PutResult> {
        let result = self.inner.put_opts(path, payload, opts).await;
        if result.is_ok() && path == &self.catalog {
            self.committed
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        result
    }
    async fn put_multipart_opts(
        &self,
        path: &ObjectPath,
        opts: object_store::PutMultipartOptions,
    ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
        self.inner.put_multipart_opts(path, opts).await
    }
    async fn get_opts(
        &self,
        path: &ObjectPath,
        opts: object_store::GetOptions,
    ) -> object_store::Result<object_store::GetResult> {
        self.inner.get_opts(path, opts).await
    }
    fn delete_stream(
        &self,
        paths: futures_util::stream::BoxStream<'static, object_store::Result<ObjectPath>>,
    ) -> futures_util::stream::BoxStream<'static, object_store::Result<ObjectPath>> {
        self.inner.delete_stream(paths)
    }
    fn list(
        &self,
        prefix: Option<&ObjectPath>,
    ) -> futures_util::stream::BoxStream<'static, object_store::Result<object_store::ObjectMeta>>
    {
        self.inner.list(prefix)
    }
    async fn list_with_delimiter(
        &self,
        prefix: Option<&ObjectPath>,
    ) -> object_store::Result<object_store::ListResult> {
        self.inner.list_with_delimiter(prefix).await
    }
    async fn copy_opts(
        &self,
        from: &ObjectPath,
        to: &ObjectPath,
        options: object_store::CopyOptions,
    ) -> object_store::Result<()> {
        self.inner.copy_opts(from, to, options).await
    }
}

#[tokio::test]
async fn final_guard_failure_retains_successful_catalog_and_payload_effects() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let root = tempfile::tempdir().unwrap();
    seed(root.path(), "committed");
    let metadata = metadata();
    let (id, _) = metadata.validate(&policy).unwrap();
    let key = ObjectPath::from(format!(
        "tenant/hub/{}",
        registry_key(producer(), ArtifactType::Skill, &id).unwrap()
    ));
    let store = AfterCatalog {
        inner: InMemory::new(),
        catalog: key,
        committed: std::sync::atomic::AtomicBool::new(false),
    };
    let check = || {
        original.check()?;
        if store.committed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    };
    let reader = capture(root.path(), &policy, &original);
    let prepared = prepare(&metadata, producer(), &reader, &policy, &original, &check).unwrap();
    let error = publish(
        Input {
            store: &store,
            storage_prefix: "tenant/hub",
            prepared,
        },
        &original,
        check,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    let effect = error.get_ref().unwrap().downcast_ref::<Failure>().unwrap();
    assert_eq!(effect.stage, "delivery");
    assert_eq!(effect.receipt.catalog, "published");
    assert_eq!(effect.receipt.payload, "published");
    assert_eq!(
        catalog(&store.inner, &metadata, &policy, &original)
            .await
            .versions()
            .len(),
        1
    );
    assert!(
        store
            .inner
            .get(&payload_key(
                &metadata,
                &effect.receipt.archive_sha256,
                &policy
            ))
            .await
            .is_ok()
    );
}

#[test]
fn consumer_rejected_member_names_fail_before_storage() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let root = tempfile::tempdir().unwrap();
    seed(root.path(), "names");
    let metadata = metadata();
    std::fs::write(root.path().join("assets/UPPER"), "one").unwrap();
    std::fs::write(root.path().join("assets/upper"), "two").unwrap();
    let reader = capture(root.path(), &policy, &original);
    let result = prepare(&metadata, producer(), &reader, &policy, &original, &|| {
        original.check()
    });
    assert!(matches!(result,Err(error) if error.kind()==io::ErrorKind::InvalidData));
}

fn received_zip() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .set_comment("received exact archive; preserve this comment")
        .unwrap();
    writer
        .start_file(
            "SKILL.md",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o600),
        )
        .unwrap();
    writer
        .write_all(b"---\nname: received\ndescription: exact bytes\n---\nreceived body")
        .unwrap();
    writer.finish().unwrap().into_inner()
}
fn received_owner(bytes: &[u8]) -> tempfile::TempDir {
    let owner = tempfile::tempdir().unwrap();
    let mut file = graphit_fs::create_private_file(&owner.path().join("payload.zip")).unwrap();
    file.write_all(bytes).unwrap();
    owner
}
#[tokio::test]
async fn received_zip_publishes_exact_final_bytes_and_cleans_its_private_owner() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let bytes = received_zip();
    let owner = received_owner(&bytes);
    let path = owner.path().to_owned();
    let mut metadata = metadata();
    metadata.set_latest = true;
    let prepared = prepare_received(&metadata, producer(), owner, &policy, &original, &|| {
        original.check()
    })
    .unwrap();
    let store = InMemory::new();
    let result = publish(
        Input {
            store: &store,
            storage_prefix: "tenant/hub",
            prepared,
        },
        &original,
        || original.check(),
    )
    .await
    .unwrap();
    let published = store
        .get(&payload_key(&metadata, &result.archive_sha256, &policy))
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(published.as_ref(), bytes);
    assert_eq!(result.archive_sha256, hex(&Sha256::digest(&bytes)));
    assert_eq!(result.payload, "published");
    assert_eq!(result.catalog, "published");
    assert!(!path.exists());
    let manifest = catalog(&store, &metadata, &policy, &original).await;
    assert_eq!(manifest.latest().unwrap().as_str(), metadata.version);
    assert_eq!(manifest.versions()[0].hash(), &Sha256::digest(&bytes)[..]);
}
#[test]
fn received_zip_rejects_invalid_bytes_limits_and_cancel_and_drops_owner() {
    let policy = policy();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let metadata = metadata();
    for bytes in [b"not a ZIP".to_vec(), {
        let mut bytes = received_zip();
        bytes.truncate(bytes.len() - 5);
        bytes
    }] {
        let owner = received_owner(&bytes);
        let path = owner.path().to_owned();
        assert!(
            prepare_received(&metadata, producer(), owner, &policy, &original, &|| {
                original.check()
            })
            .is_err()
        );
        assert!(!path.exists());
    }
    let owner = received_owner(&received_zip());
    let path = owner.path().to_owned();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path.join("payload.zip"))
        .unwrap();
    file.set_len(policy.copy().total_file_bytes_limit() + 1)
        .unwrap();
    drop(file);
    assert!(
        matches!(prepare_received(&metadata, producer(), owner, &policy, &original, &|| original.check()), Err(error) if error.kind() == io::ErrorKind::FileTooLarge)
    );
    assert!(!path.exists());
    let owner = received_owner(&received_zip());
    let path = owner.path().to_owned();
    cancel.cancel();
    assert!(
        matches!(prepare_received(&metadata, producer(), owner, &policy, &original, &|| original.check()), Err(error) if error.kind() == io::ErrorKind::Interrupted)
    );
    assert!(!path.exists());
}
