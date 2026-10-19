use super::*;
use crate::artifact_store::{
    ArtifactKey, ArtifactLimits, ArtifactMode, ArtifactRecovery, ArtifactStage, ArtifactStore,
    PhysicalSource,
};

fn store(scope: CapturedPublicationScope) -> ArtifactStore {
    let copy = CopyLimits::new(1024 * 1024, 128, 65536, 8).unwrap();
    ArtifactStore::new(scope, ArtifactLimits::new(1024 * 1024, 1024, copy).unwrap())
}
fn key() -> ArtifactKey {
    ArtifactKey::new("ast", "graph", "fixture").unwrap()
}

#[test]
fn confirmed_copy_reuses_verified_content_and_reads_after_writer_drop() {
    for kind in [NodeKind::File, NodeKind::Directory] {
        let (temporary, paths, _birth, scope) = fixture();
        let source = temporary.path().join("copy-source");
        let relative_file = if kind == NodeKind::Directory {
            std::fs::create_dir(&source).unwrap();
            std::fs::write(source.join("data"), b"graph-data").unwrap();
            Path::new("data")
        } else {
            std::fs::write(&source, b"graph-data").unwrap();
            Path::new("")
        };
        let store = store(scope);
        let original = request();
        let mut writer = store.writer_with_context(&original).unwrap();
        writer
            .publish(
                &key(),
                PhysicalSource::ExternalSource(&source),
                &paths.root().join("GraphCopy"),
                kind,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            )
            .unwrap();
        if kind == NodeKind::Directory {
            std::fs::remove_dir_all(&source).unwrap();
        } else {
            std::fs::remove_file(&source).unwrap();
        }
        let retention = RetainedBytesLimits::new(8, 1, 4096).unwrap();
        let metadata_policy = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(8, 128, 65536, 8).unwrap(),
            RetainedFileSet::new([], retention).unwrap(),
            retention,
        )
        .unwrap();
        // Ordinary capture still reads/hashes the full ten-byte payload even
        // with no retained paths, so the same eight-byte limit rejects it.
        assert_eq!(
            CapturedSourceReader::capture_project(
                store.scope(),
                Path::new("GraphCopy"),
                kind,
                &metadata_policy,
                read_limits(),
                &original,
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::FileTooLarge
        );
        // This is the real capture payload-read seam, not a wrapper counter.
        CAPTURE_STEP.with(|slot| {
            *slot.borrow_mut() = Some((
                CaptureStep::AfterPayloadRead,
                Box::new(|| panic!("published copy capture must not read payload")),
            ))
        });
        let reader = writer
            .read_published_copy(&key(), kind, &metadata_policy, read_limits(), &original)
            .unwrap();
        CAPTURE_STEP.with(|slot| assert!(slot.borrow_mut().take().is_some()));
        assert_eq!(reader.retained_bytes(), 0);
        assert_eq!(
            reader
                .nodes()
                .find(|n| n.relative() == relative_file)
                .unwrap()
                .digest(),
            Some(<[u8; 32]>::from(Sha256::digest(b"graph-data")))
        );
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::NoPending);
        drop(writer);
        assert!(
            !crate::file_lock::FileLock::is_locked(
                store.runtime_root().join("artifact-store.lock")
            )
            .unwrap()
        );
        let file = reader.open_file(relative_file).unwrap();
        assert_eq!(file.len(), 10);
        assert_eq!(file.read_range(1, 4).unwrap().as_ref(), b"raph");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file.stream(0).unwrap(), &mut bytes).unwrap();
        assert_eq!(bytes, b"graph-data");
        reader.revalidate_bindings().unwrap();
    }
}

#[test]
fn changed_copy_cannot_adopt_old_digest_or_escape_captured_binding() {
    for replace in [false, true] {
        let (temporary, paths, _birth, scope) = fixture();
        let source = temporary.path().join("copy-source");
        std::fs::write(&source, b"graph-data").unwrap();
        let destination = paths.root().join("GraphCopy");
        let store = store(scope);
        let original = request();
        let mut writer = store.writer_with_context(&original).unwrap();
        writer
            .publish(
                &key(),
                PhysicalSource::ExternalSource(&source),
                &destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            )
            .unwrap();
        let reader = writer
            .read_published_copy(
                &key(),
                NodeKind::File,
                &policy(CopyPolicy::RejectFileLinks),
                read_limits(),
                &original,
            )
            .unwrap();
        let file = reader.open_file(Path::new("")).unwrap();
        let modified = std::fs::metadata(&destination).unwrap().modified().unwrap();
        if replace {
            std::fs::rename(&destination, paths.root().join("OldGraphCopy")).unwrap();
        }
        std::fs::write(&destination, b"other-data").unwrap();
        #[cfg(unix)]
        std::fs::File::options()
            .write(true)
            .open(&destination)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        #[cfg(not(unix))]
        let _ = modified;
        assert!(file.read_range(0, 1).is_err());
        assert!(reader.revalidate_bindings().is_err());
        let error = writer
            .read_published_copy(
                &key(),
                NodeKind::File,
                &policy(CopyPolicy::RejectFileLinks),
                read_limits(),
                &original,
            )
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }
}

#[test]
fn copy_reader_rejects_wrong_original_kind_link_missing_and_pending() {
    let (temporary, paths, _birth, scope) = fixture();
    let source = temporary.path().join("copy-source");
    std::fs::write(&source, b"graph-data").unwrap();
    let destination = paths.root().join("GraphCopy");
    let store = store(scope);
    let original = request();
    let mut writer = store.writer_with_context(&original).unwrap();
    let policy = policy(CopyPolicy::RejectFileLinks);
    let error = writer
        .read_published_copy(&key(), NodeKind::File, &policy, read_limits(), &request())
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    let error = writer
        .read_published_copy(&key(), NodeKind::File, &policy, read_limits(), &original)
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    writer
        .publish(
            &key(),
            PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    let error = writer
        .read_published_copy(&key(), NodeKind::File, &policy, read_limits(), &original)
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    let error = writer
        .read_published_copy(
            &key(),
            NodeKind::Directory,
            &policy,
            read_limits(),
            &original,
        )
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    writer.remove(&key()).unwrap();
    writer.fail_at.set(Some(ArtifactStage::Publish));
    assert!(
        writer
            .publish(
                &key(),
                PhysicalSource::ExternalSource(&source),
                &destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .is_err()
    );
    assert!(writer.load().unwrap().unwrap().pending().is_some());
    let error = writer
        .read_published_copy(&key(), NodeKind::File, &policy, read_limits(), &original)
        .err()
        .unwrap();
    assert_eq!(error.stage(), ArtifactStage::RecoveryRequired);
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

#[test]
fn copy_reader_requires_native_project_scope_and_live_original() {
    let (temporary, paths, _birth, scope) = fixture();
    let store = store(scope);
    let original = request();
    let ordinary = store
        .writer(Duration::from_secs(1), &Cancellation::default())
        .unwrap();
    assert_eq!(
        ordinary
            .read_published_copy(
                &key(),
                NodeKind::File,
                &policy(CopyPolicy::RejectFileLinks),
                read_limits(),
                &original
            )
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    drop(ordinary);
    let writer = store.writer_with_context(&original).unwrap();
    original.cancellation().cancel();
    assert_eq!(
        writer
            .read_published_copy(
                &key(),
                NodeKind::File,
                &policy(CopyPolicy::RejectFileLinks),
                read_limits(),
                &original
            )
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    drop(writer);
    std::fs::create_dir_all(temporary.path().join("global")).unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temporary.path(),
        None,
        Some(&temporary.path().join("global")),
    )
    .unwrap();
    let original = request();
    let scope = CapturedPublicationScope::installation(
        &installation,
        crate::installation_identity::InstallationId::new("fixture").unwrap(),
        [paths.root().join("GraphCopy")],
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &original,
    )
    .unwrap();
    let global = self::store(scope);
    let writer = global.writer_with_context(&original).unwrap();
    assert_eq!(
        writer
            .read_published_copy(
                &key(),
                NodeKind::File,
                &policy(CopyPolicy::RejectFileLinks),
                read_limits(),
                &original
            )
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn copy_reader_refuses_missing_verified_observation_without_rehashing() {
    let (temporary, paths, _birth, scope) = fixture();
    let source = temporary.path().join("copy-source");
    std::fs::write(&source, b"graph-data").unwrap();
    let store = store(scope);
    let original = request();
    let mut writer = store.writer_with_context(&original).unwrap();
    writer
        .publish(
            &key(),
            PhysicalSource::ExternalSource(&source),
            &paths.root().join("GraphCopy"),
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    let mut document = writer.document().unwrap();
    let crate::artifact_store::Proof::Copy { tree, .. } = &mut document.entries[0].receipt else {
        panic!("genuine Copy");
    };
    let mut encoded = serde_json::to_value(&tree.nodes[0]).unwrap();
    encoded.as_object_mut().unwrap().remove("verified");
    assert!(serde_json::from_value::<tree::NodeReceipt>(encoded).is_err());
    tree.nodes[0].verified = None;
    writer.save(&document, ArtifactStage::Commit).unwrap();
    CAPTURE_STEP.with(|slot| {
        *slot.borrow_mut() = Some((
            CaptureStep::AfterPayloadRead,
            Box::new(|| panic!("unverified receipt must not recapture payload")),
        ))
    });
    let error = writer
        .read_published_copy(
            &key(),
            NodeKind::File,
            &policy(CopyPolicy::RejectFileLinks),
            read_limits(),
            &original,
        )
        .err()
        .unwrap();
    CAPTURE_STEP.with(|slot| assert!(slot.borrow_mut().take().is_some()));
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
}

#[test]
fn unreadable_owned_copy_preserves_modes_without_a_readable_receipt() {
    use crate::artifact_store::{GeneratedNode, PayloadMode, PayloadPolicy, ValidatedPayloadPlan};
    use std::os::unix::fs::PermissionsExt;
    let (_temporary, paths, _birth, scope) = fixture();
    let store = store(scope);
    let original = request();
    let mode = PayloadMode::from_permissions(&std::fs::Permissions::from_mode(0o204)).unwrap();
    let plan = ValidatedPayloadPlan::generated(
        [GeneratedNode::file(PathBuf::new(), b"graph-data".to_vec(), mode).unwrap()],
        &PayloadPolicy::new(NodeKind::File, mode, store.limits.copy),
        &original,
    )
    .unwrap();
    let mut writer = store.writer_with_context(&original).unwrap();
    let destination = paths.root().join("GraphCopy");
    writer
        .publish_generated(
            &key(),
            &plan,
            &destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    assert_eq!(
        std::fs::metadata(&destination)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o204
    );
    assert!(writer.load().unwrap().unwrap().pending().is_none());
    let error = writer
        .read_published_copy(
            &key(),
            NodeKind::File,
            &policy(CopyPolicy::RejectFileLinks),
            read_limits(),
            &original,
        )
        .err()
        .unwrap();
    assert!(matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists
    ));
    writer.remove(&key()).unwrap();
}

#[test]
fn publication_parent_creates_native_directories_and_publishes_real_copy() {
    let (temporary, paths, _birth, scope) = fixture();
    let source = temporary.path().join("parent-source");
    std::fs::write(&source, b"parent-copy").unwrap();
    let destination = paths.brand_directory().join("ast-graphs/version/graph");
    let store = store(scope);
    let original = request();
    let mut writer = store.writer_with_context(&original).unwrap();
    writer.ensure_publication_parent(&destination).unwrap();
    writer.ensure_publication_parent(&destination).unwrap();
    writer
        .publish(
            &key(),
            PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    let reader = writer
        .read_published_copy(
            &key(),
            NodeKind::File,
            &policy(CopyPolicy::RejectFileLinks),
            read_limits(),
            &original,
        )
        .unwrap();
    drop(writer);
    assert_eq!(
        reader
            .open_file(Path::new(""))
            .unwrap()
            .read_range(0, 11)
            .unwrap()
            .as_ref(),
        b"parent-copy"
    );
}

#[test]
fn publication_parent_denies_links_files_unowned_paths_and_cancelled_original() {
    for link in [false, true] {
        let (temporary, paths, _birth, scope) = fixture();
        let outside = temporary.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let prefix = paths.brand_directory().join("ast-graphs");
        if link {
            std::os::unix::fs::symlink(&outside, &prefix).unwrap();
        } else {
            std::fs::write(&prefix, b"keep").unwrap();
        }
        let store = store(scope);
        let original = request();
        let writer = store.writer_with_context(&original).unwrap();
        assert!(
            writer
                .ensure_publication_parent(&prefix.join("v/graph"))
                .is_err()
        );
        assert!(
            writer
                .ensure_publication_parent(&outside.join("v/graph"))
                .is_err()
        );
        assert!(!outside.join("v").exists());
        if !link {
            assert_eq!(std::fs::read(&prefix).unwrap(), b"keep");
        }
    }
    let (_temporary, paths, _birth, scope) = fixture();
    let store = store(scope);
    let original = request();
    let writer = store.writer_with_context(&original).unwrap();
    original.cancellation().cancel();
    assert_eq!(
        writer
            .ensure_publication_parent(&paths.brand_directory().join("ast-graphs/v/graph"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!paths.brand_directory().join("ast-graphs").exists());
}

#[test]
fn published_graph_copy_ignores_sibling_activity_outside_capture() {
    let (temporary, paths, _birth, scope) = fixture();
    let source = temporary.path().join("graph-source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("icebug.json"), b"graph manifest").unwrap();
    std::fs::write(source.join("nodes.parquet"), b"graph-data").unwrap();
    let destination = paths.brand_directory().join("ast-graphs/version-one");
    let store = store(scope);
    let original = request();
    let mut writer = store.writer_with_context(&original).unwrap();
    writer.ensure_publication_parent(&destination).unwrap();
    writer
        .publish(
            &key(),
            PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::Directory,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    let reader = writer
        .read_published_copy(
            &key(),
            NodeKind::Directory,
            &policy(CopyPolicy::RejectFileLinks),
            read_limits(),
            &original,
        )
        .unwrap();
    drop(writer);
    let file = reader.open_file(Path::new("nodes.parquet")).unwrap();
    // An atomic activity publication changes the shared .graphit ancestor,
    // but none of this confirmed graph publication or its retained handles.
    std::fs::write(
        paths.brand_directory().join("activity-new.json"),
        b"sibling activity",
    )
    .unwrap();
    reader
        .revalidate_bindings()
        .expect("sibling activity must not invalidate an intact graph");
    assert_eq!(file.read_range(0, 10).unwrap().as_ref(), b"graph-data");
    // Captured file observations still reject a same-length content change.
    std::fs::write(destination.join("nodes.parquet"), b"other-data").unwrap();
    assert!(reader.revalidate_bindings().is_err());
    assert!(file.read_range(0, 10).is_err());
}

#[test]
fn published_graph_copy_rejects_replaced_ancestor_binding() {
    let (temporary, paths, _birth, scope) = fixture();
    let source = temporary.path().join("graph-source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("nodes.parquet"), b"graph-data").unwrap();
    let ancestor = paths.brand_directory().join("ast-graphs");
    let destination = ancestor.join("version-one");
    let store = store(scope);
    let original = request();
    let mut writer = store.writer_with_context(&original).unwrap();
    writer.ensure_publication_parent(&destination).unwrap();
    writer
        .publish(
            &key(),
            PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::Directory,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    let reader = writer
        .read_published_copy(
            &key(),
            NodeKind::Directory,
            &policy(CopyPolicy::RejectFileLinks),
            read_limits(),
            &original,
        )
        .unwrap();
    drop(writer);
    std::fs::rename(&ancestor, paths.brand_directory().join("saved-ast-graphs")).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(destination.join("nodes.parquet"), b"graph-data").unwrap();
    assert!(reader.revalidate_bindings().is_err());
    assert!(reader.open_file(Path::new("nodes.parquet")).is_err());
}
