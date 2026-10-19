use super::*;

#[test]
fn owned_missing_file_copy_is_recreated_and_reopens_idempotently() {
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        let mut writer = f.writer();
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::File).unwrap(),
            ArtifactChange::Created
        );
        let old = writer.load().unwrap().unwrap().entries()[0].clone();
        fs::remove_file(&f.destination).unwrap();
        fs::write(&f.source, b"current source").unwrap();
        assert_eq!(
            writer
                .check_publication(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
                )
                .unwrap(),
            PublicationCheck::NeedsPublication
        );
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::File).unwrap(),
            ArtifactChange::Updated
        );
        assert_eq!(fs::read(&f.destination).unwrap(), b"current source");
        let state = writer.load().unwrap().unwrap();
        assert!(state.pending().is_none());
        assert!(state.entries()[0] != old);
        drop(writer);
        let mut writer = f
            .fresh_store()
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap();
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::NoPending);
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::File).unwrap(),
            ArtifactChange::Unchanged
        );
    }
}

#[test]
fn owned_missing_file_copy_recovers_before_and_after_exclusive_move() {
    for stage in [
        ArtifactStage::BeforeRename,
        ArtifactStage::Publish,
        ArtifactStage::Commit,
    ] {
        let f = Fixture::new();
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        fs::remove_file(&f.destination).unwrap();
        writer.fail_at.set(Some(stage));
        assert!(copy_fixture(&mut writer, &f, NodeKind::File).is_err());
        assert!(writer.load().unwrap().unwrap().pending().is_some());
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        assert!(writer.load().unwrap().unwrap().pending().is_none());
    }
}

#[test]
fn owned_missing_file_copy_preserves_foreign_races_and_unowned_files() {
    for symlink in [false, true] {
        if symlink && !cfg!(unix) {
            continue;
        }
        let f = Fixture::new();
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        let old = writer.load().unwrap().unwrap().entries()[0].clone();
        fs::remove_file(&f.destination).unwrap();
        writer.fail_at.set(Some(ArtifactStage::BeforeRename));
        assert!(copy_fixture(&mut writer, &f, NodeKind::File).is_err());
        if symlink {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&f.source, &f.destination).unwrap();
        } else {
            fs::write(&f.destination, b"foreign arrival").unwrap();
        }
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        assert!(writer.load().unwrap().unwrap().entries()[0] == old);
        if symlink {
            assert!(
                fs::symlink_metadata(&f.destination)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        } else {
            assert_eq!(fs::read(&f.destination).unwrap(), b"foreign arrival");
        }
    }
    let f = Fixture::new();
    fs::write(&f.destination, fs::read(&f.source).unwrap()).unwrap();
    let mut writer = f.writer();
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::File)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
}

#[test]
fn owned_missing_file_copy_does_not_recreate_directories_or_missing_parents() {
    for directory in [false, true] {
        let f = Fixture::new();
        let mut writer = f.writer();
        let kind = if directory {
            fs::remove_file(&f.source).unwrap();
            fs::create_dir(&f.source).unwrap();
            NodeKind::Directory
        } else {
            NodeKind::File
        };
        copy_fixture(&mut writer, &f, kind).unwrap();
        if directory {
            fs::remove_dir(&f.destination).unwrap();
        } else {
            fs::remove_file(&f.destination).unwrap();
            fs::remove_dir(f.destination.parent().unwrap()).unwrap();
        }
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            copy_fixture(&mut writer, &f, kind).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        assert!(!f.destination.exists());
    }
}

#[test]
fn owned_missing_file_copy_does_not_recreate_links_or_move_slots() {
    for link in [false, true] {
        let f = Fixture::new();
        let mut writer = f.writer();
        let mode = if link {
            ArtifactMode::Link
        } else {
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
        };
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                mode,
            )
            .unwrap();
        fs::remove_file(&f.destination).unwrap();
        let target = if link {
            f.destination.clone()
        } else {
            f.destination.with_file_name("other-slot")
        };
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &target,
                    NodeKind::File,
                    ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
                )
                .unwrap_err()
                .kind(),
            if link {
                io::ErrorKind::AlreadyExists
            } else {
                io::ErrorKind::NotFound
            }
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        assert!(!target.exists());
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn owned_missing_file_copy_keeps_original_cancellation_before_move() {
    let f = Fixture::new();
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    fs::remove_file(&f.destination).unwrap();
    drop(writer);
    let original = native_capture_request();
    let mut writer = f.store.writer_with_context(&original).unwrap();
    writer.cancel_at.set(Some((ArtifactStage::BeforeRename, 1)));
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::File)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(original.cancellation().is_cancelled());
    assert!(!f.destination.exists());
    drop(writer);
    let mut writer = f.writer();
    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
}
