use super::*;

#[cfg(unix)]
#[test]
fn owned_copy_survives_storage_permission_hardening_after_fresh_capture() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let storage = fixture
        .store
        .runtime_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    fs::set_permissions(storage, fs::Permissions::from_mode(0o755)).unwrap();
    let store = fixture.fresh_store();
    store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .publish(
            &fixture.key(),
            PhysicalSource::ExternalSource(&fixture.source),
            &fixture.destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    let bytes = fs::read(fixture.store.manifest_path()).unwrap();
    fs::set_permissions(storage, fs::Permissions::from_mode(0o700)).unwrap();

    // Hardening invalidates a live observation too: reading requires recapture.
    assert_eq!(store.load().unwrap_err().stage(), ArtifactStage::Read);
    let fresh = fixture.fresh_store();
    assert!(fresh.load().unwrap().is_some());
    assert_eq!(fs::read(fresh.manifest_path()).unwrap(), bytes);
    let mut writer = fresh
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    assert!(
        writer
            .verify_owned_copy(&fixture.key(), &fixture.destination, NodeKind::File)
            .unwrap()
    );
    writer
        .publish(
            &ArtifactKey::new("codex", "command", "another").unwrap(),
            PhysicalSource::ExternalSource(&fixture.source),
            &fixture.destination.with_extension("another"),
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
}

#[cfg(unix)]
#[test]
fn owned_copy_rejects_storage_permission_relaxation_even_after_fresh_capture() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let storage = fixture
        .store
        .runtime_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    fs::set_permissions(storage, fs::Permissions::from_mode(0o700)).unwrap();
    let store = fixture.fresh_store();
    store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .publish(
            &fixture.key(),
            PhysicalSource::ExternalSource(&fixture.source),
            &fixture.destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    let bytes = fs::read(fixture.store.manifest_path()).unwrap();
    fs::set_permissions(storage, fs::Permissions::from_mode(0o755)).unwrap();
    let fresh = fixture.fresh_store();
    let failure = fresh.load().unwrap_err();
    assert_eq!(failure.stage(), ArtifactStage::Read);
    assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(fresh.manifest_path()).unwrap(), bytes);
}

#[test]
fn owned_copy_verification_does_not_require_original_source() {
    for global in [false, true] {
        let fixture = Fixture::with_owner(global);
        let key = fixture.key();
        let mut writer = fixture.writer();
        assert!(
            !writer
                .verify_owned_copy(&key, &fixture.destination, NodeKind::File)
                .unwrap()
        );
        writer
            .publish(
                &key,
                PhysicalSource::ExternalSource(&fixture.source),
                &fixture.destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            )
            .unwrap();
        fs::remove_file(&fixture.source).unwrap();
        assert!(
            writer
                .verify_owned_copy(&key, &fixture.destination, NodeKind::File)
                .unwrap()
        );
        assert!(
            writer
                .verify_owned_copy(&key, &fixture.destination, NodeKind::Directory)
                .is_err()
        );
        assert!(
            writer
                .verify_owned_copy(
                    &key,
                    &fixture.destination.with_extension("other"),
                    NodeKind::File
                )
                .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn owned_copy_verification_never_treats_corruption_as_a_missing_key() {
    for global in [false, true] {
        let fixture = Fixture::with_owner(global);
        let key = fixture.key();
        let mut writer = fixture.writer();
        writer
            .publish(
                &key,
                PhysicalSource::ExternalSource(&fixture.source),
                &fixture.destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            )
            .unwrap();
        fs::rename(
            &fixture.destination,
            fixture.destination.with_extension("old"),
        )
        .unwrap();
        fs::write(&fixture.destination, b"modified bytes").unwrap();
        assert!(
            writer
                .verify_owned_copy(&key, &fixture.destination, NodeKind::File)
                .is_err()
        );
        fs::remove_file(&fixture.destination).unwrap();
        assert_eq!(
            writer
                .verify_owned_copy(&key, &fixture.destination, NodeKind::File)
                .unwrap_err()
                .stage(),
            ArtifactStage::Conflict
        );
    }
}
