//! Bounded named reads with actual Core publication and recovery.
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactLimits, ArtifactStore, CapturedPublicationScope, CopyLimits,
        NativeDocumentOperationId, NativeDocumentOutcome, NativeDocumentPhase,
        NativeDocumentRecovery, NativeDocumentRecoveryRequest, ScopeLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentFileLimits, NativeDocumentNameLimits,
    },
    project_paths::InstallationPaths,
};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn store(root: &Path) -> ArtifactStore {
    let installation =
        InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("owner")))
            .unwrap();
    let scope = CapturedPublicationScope::installation(
        &installation,
        InstallationId::new("terminal-fixture").unwrap(),
        [root.to_owned()],
        ScopeLimits::new(4, 65536, 65536).unwrap(),
        &native_capture_request(),
    )
    .unwrap();
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 64).unwrap();
    ArtifactStore::new(scope, ArtifactLimits::new(1024 * 1024, 1024, copy).unwrap())
}
fn context(cancel: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancel,
    ))
}
fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(1024, 65536, 256, 65536).unwrap()
}
fn names() -> NativeDocumentNameLimits {
    NativeDocumentNameLimits::new(32, 65536).unwrap()
}
fn operation(root: &Path) -> NativeDocumentOperationId {
    let id = NativeDocumentOperationId::generate();
    let mut file = fs::File::create(root.join("caller-operation.json")).unwrap();
    write!(file, "{id}").unwrap();
    file.sync_all().unwrap();
    id
}

#[test]
fn named_read_observes_current_file_with_exact_content_bound() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();

    fs::write(tmp.path().join("agent.json"), b"new").unwrap();
    let found = parent
        .read_named_bounded("agent.json".as_ref(), 3, names(), &ctx)
        .unwrap();
    assert_eq!(found.native_name().unwrap().as_os_str(), "agent.json");
    assert_eq!(found.observation().bytes(), Some(b"new".as_slice()));
}

#[test]
fn missing_and_empty_named_reads_preserve_filesystem() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
    let missing = parent
        .read_named_bounded("missing".as_ref(), 2, names(), &ctx)
        .unwrap();
    assert!(missing.native_name().is_none());
    assert!(missing.observation().bytes().is_none());

    drop(
        parent
            .read_named_bounded("unused".as_ref(), 4, names(), &ctx)
            .unwrap(),
    );

    fs::write(tmp.path().join("empty"), b"").unwrap();
    let empty = parent
        .read_named_bounded("empty".as_ref(), 0, names(), &ctx)
        .unwrap();
    assert_eq!(empty.observation().bytes(), Some(b"".as_slice()));
    assert_eq!(empty.native_name().unwrap().as_os_str(), "empty");

    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn invalid_or_interrupted_named_read_preserves_target() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, cap, expected) in [
        ("../outside", 3, io::ErrorKind::InvalidInput),
        ("agent.json", usize::MAX, io::ErrorKind::InvalidInput),
        ("agent.json", 1025, io::ErrorKind::InvalidInput),
    ] {
        let ctx = context(Cancellation::default());
        let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
        assert_eq!(
            parent
                .read_named_bounded(name.as_ref(), cap, names(), &ctx)
                .err()
                .unwrap()
                .kind(),
            expected
        );
    }
    for deadline in [false, true] {
        let cancel = Cancellation::default();
        let ctx = if deadline {
            Arc::new(SnapshotContext::new(
                Instant::now() - Duration::from_secs(1),
                cancel,
            ))
        } else {
            let ctx = context(cancel.clone());
            cancel.cancel();
            ctx
        };
        let fresh = context(Cancellation::default());
        let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &fresh).unwrap();
        assert_eq!(
            parent
                .read_named_bounded("agent.json".as_ref(), 3, names(), &ctx)
                .err()
                .unwrap()
                .kind(),
            if deadline {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::Interrupted
            }
        );
    }
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[test]
fn growth_above_content_bound_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();

    fs::write(tmp.path().join("agent.json"), b"new").unwrap();
    let observed = parent
        .read_named_bounded("agent.json".as_ref(), 3, names(), &ctx)
        .unwrap();
    fs::write(tmp.path().join("agent.json"), b"large").unwrap();
    assert_eq!(
        parent
            .revalidate_named_observation(&observed, &ctx)
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert_eq!(
        parent
            .read_named_bounded("agent.json".as_ref(), 3, names(), &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );

    assert_eq!(fs::read(tmp.path().join("agent.json")).unwrap(), b"large");
}

#[test]
fn published_native_document_can_be_read_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir(root.join("owner")).unwrap();
    let s = store(&root);
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(&root, limits(), &ctx).unwrap();
    let old = parent.read("agent.json".as_ref(), &ctx).unwrap();

    let mut writer = s
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .with_io_context(ctx.clone())
        .unwrap();
    let id = operation(&root);
    let key = ArtifactKey::new("native", "agent", "role").unwrap();
    let stage = writer
        .prepare_native_document(id, &key, &parent, &old, b"new")
        .unwrap_or_else(|f| panic!("{:?}", f.error()));
    let resolution = stage
        .publish()
        .unwrap_or_else(|f| panic!("{:?}", f.error()));
    assert_eq!(resolution.outcome(), NativeDocumentOutcome::Applied);

    let found = parent
        .read_named_bounded("agent.json".as_ref(), 3, names(), &ctx)
        .unwrap();
    assert_eq!(found.native_name().unwrap().as_os_str(), "agent.json");
    assert!(found.observation().receipt() == resolution.destination_receipt());
    assert_eq!(found.observation().bytes(), Some(b"new".as_slice()));

    resolution.acknowledge().unwrap().release().unwrap();
    assert!(s.load().unwrap().unwrap().pending().is_none());
}

#[test]
fn cancellation_after_actual_core_effect_keeps_pending_until_fresh_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir(root.join("owner")).unwrap();
    let s = store(&root);
    let cancel = Cancellation::default();
    let ctx = context(cancel.clone());
    let parent = CapturedDocumentDirectory::capture(&root, limits(), &ctx).unwrap();
    let old = parent.read("agent.json".as_ref(), &ctx).unwrap();

    let mut writer = s
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .with_io_context(ctx.clone())
        .unwrap();
    let id = operation(&root);
    let key = ArtifactKey::new("native", "agent", "role").unwrap();
    let resolution = writer
        .prepare_native_document(id, &key, &parent, &old, b"new")
        .unwrap_or_else(|f| panic!("{:?}", f.error()))
        .publish()
        .unwrap_or_else(|f| panic!("{:?}", f.error()));

    cancel.cancel();
    assert_eq!(
        parent
            .read_named_bounded("agent.json".as_ref(), 3, names(), &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );

    drop(resolution);
    drop(writer);
    let pending = s.load().unwrap().unwrap();
    assert!(
        matches!(pending.pending(),Some(graphit_core::artifact_store::ArtifactIntent::NativeDocument(p)) if p.phase()==NativeDocumentPhase::Resolved)
    );
    assert_eq!(fs::read(root.join("agent.json")).unwrap(), b"new");
    let fresh = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(&root, limits(), &fresh).unwrap();
    let identity = parent.directory_identity();
    let scope = s.scope().binding().unwrap();
    let destination = root.join("agent.json");
    let mut writer = s
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .with_io_context(fresh)
        .unwrap();
    let NativeDocumentRecovery::Resolution(resolution) = writer
        .recover_native_document(
            NativeDocumentRecoveryRequest {
                operation_id: id,
                key: &key,
                destination: &destination,
                parent_identity: &identity,
                previous: None,
                scope: &scope,
            },
            &parent,
        )
        .unwrap()
    else {
        panic!("expected Applied resolution")
    };
    assert_eq!(resolution.outcome(), NativeDocumentOutcome::Applied);
    resolution.acknowledge().unwrap().release().unwrap();
    assert_eq!(fs::read(destination).unwrap(), b"new");
}

fn native_capture_request() -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        graphit_core::file_lock::Cancellation::default(),
    ))
}
