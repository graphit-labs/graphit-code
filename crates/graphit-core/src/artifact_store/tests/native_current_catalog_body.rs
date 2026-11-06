use super::*;
use std::sync::Arc;

fn original(signal: Cancellation) -> std::sync::Arc<crate::file_io::SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        signal,
    ))
}
fn seed(fixture: &Fixture) {
    fixture
        .writer()
        .save(
            &Document::empty(&fixture.store).unwrap(),
            ArtifactStage::Commit,
        )
        .unwrap();
}
fn no_scratch(fixture: &Fixture) {
    assert!(
        fs::read_dir(fixture.store.runtime_root())
            .unwrap()
            .all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b".artifact-store-")
            })
    );
}
#[test]
fn ordinary_and_native_writers_retain_one_private_permanent_lock() {
    let fixture = Fixture::new();
    let path = fixture.store.runtime_root().join("artifact-store.lock");
    let writer = fixture.writer();
    graphit_fs::verify_private_file(writer._lock.file()).unwrap();
    let identity = crate::native_file_identity::handle_id(writer._lock.file()).unwrap();
    assert_eq!(writer._lock.file().metadata().unwrap().len(), 0);
    assert!(FileLock::is_locked(&path).unwrap());
    drop(writer);
    let request = original(Cancellation::default());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    graphit_fs::verify_private_file(writer._lock.file()).unwrap();
    assert!(crate::native_file_identity::handle_id(writer._lock.file()).unwrap() == identity);
    assert!(FileLock::is_locked(&path).unwrap());
    writer.revalidate(&request).unwrap();
    drop(writer);
    let writer = fixture.writer();
    graphit_fs::verify_private_file(writer._lock.file()).unwrap();
    assert!(crate::native_file_identity::handle_id(writer._lock.file()).unwrap() == identity);
    assert!(FileLock::is_locked(&path).unwrap());
    drop(writer);
    assert!(!FileLock::is_locked(&path).unwrap());
}

#[cfg(windows)]
#[test]
fn foreign_default_lock_stays_denied_without_repair_or_replacement() {
    let fixture = Fixture::new();
    let context = ScopeContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    native_source::ensure_control_parent(&fixture.store.scope, &context).unwrap();
    let _control =
        control::ControlDirectory::create_or_open(&fixture.store.scope, &context).unwrap();
    let path = fixture.store.runtime_root().join("artifact-store.lock");
    let foreign = FileLock::try_acquire(&path).unwrap();
    let file = foreign.file().try_clone().unwrap();
    foreign.release().unwrap();
    let identity = crate::native_file_identity::handle_id(&file).unwrap();
    assert_eq!(
        graphit_fs::verify_private_file(&file).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    let request = original(Cancellation::default());
    let native_error = fixture.store.writer_with_context(&request).err().unwrap();
    assert_eq!(native_error.stage(), ArtifactStage::Lock);
    assert_eq!(native_error.kind(), io::ErrorKind::PermissionDenied);
    let normal_error = fixture
        .store
        .writer(Duration::from_secs(30), &Cancellation::default())
        .err()
        .unwrap();
    assert_eq!(normal_error.stage(), ArtifactStage::Lock);
    assert_eq!(normal_error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        graphit_fs::verify_private_file(&file).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    let metadata = fs::symlink_metadata(&path).unwrap();
    assert!(crate::native_file_identity::physical_id(&path, &metadata).unwrap() == identity);
    assert_eq!(metadata.len(), 0);
}

#[test]
fn loaded_catalogue_decodes_the_current_physical_body() {
    let fixture = Fixture::new();
    seed(&fixture);
    let request = original(Cancellation::default());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    let loaded = writer.loaded_catalog().unwrap();
    assert!(loaded.document.pending.is_none());
    assert_eq!(loaded.document.scope.owner(), fixture.store.owner());
}
#[test]
fn missing_catalogue_remains_absent_until_save() {
    let fixture = Fixture::new();
    let request = original(Cancellation::default());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    assert!(writer.loaded_catalog().unwrap().document.pending.is_none());
    assert!(!fixture.store.manifest_path().exists());
}
#[test]
fn cancelled_request_denies_catalogue_read() {
    let fixture = Fixture::new();
    seed(&fixture);
    let signal = Cancellation::default();
    let request = original(signal.clone());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    signal.cancel();
    assert_eq!(
        writer.loaded_catalog().err().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    no_scratch(&fixture);
}
#[test]
fn cancellation_after_read_does_not_return_a_decoded_catalogue() {
    let fixture = Fixture::new();
    seed(&fixture);
    let signal = Cancellation::default();
    let request = original(signal.clone());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    catalog::test_hooks::AFTER_READ
        .with(|hook| *hook.borrow_mut() = Some(Box::new(move |_| signal.cancel())));
    assert_eq!(
        writer.loaded_catalog().err().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    no_scratch(&fixture);
}
#[test]
fn expired_request_denies_catalogue_read() {
    let fixture = Fixture::new();
    seed(&fixture);
    let deadline = Instant::now() + Duration::from_secs(1);
    let request = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
    let writer = fixture.store.writer_with_context(&request).unwrap();
    std::thread::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
    );
    assert_eq!(
        writer.loaded_catalog().err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    no_scratch(&fixture);
}
#[cfg(unix)]
#[test]
fn post_read_inode_growth_is_a_conflict() {
    let fixture = Fixture::new();
    seed(&fixture);
    let request = original(Cancellation::default());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    catalog::test_hooks::AFTER_READ.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|path| {
            use std::io::Write;
            fs::OpenOptions::new()
                .append(true)
                .open(path)
                .unwrap()
                .write_all(b" ")
                .unwrap();
        }))
    });
    assert_eq!(
        writer.loaded_catalog().err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    no_scratch(&fixture);
}
#[test]
fn loaded_catalogue_cannot_be_saved_by_another_writer() {
    let first = Fixture::new();
    let second = Fixture::new();
    seed(&first);
    seed(&second);
    let first_request = original(Cancellation::default());
    let second_request = original(Cancellation::default());
    let first_writer = first.store.writer_with_context(&first_request).unwrap();
    let second_writer = second.store.writer_with_context(&second_request).unwrap();
    let loaded = first_writer.loaded_catalog().unwrap();
    assert_eq!(
        second_writer
            .save_native_projection(loaded, None, ArtifactStage::Commit)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    no_scratch(&second);
}
#[cfg(unix)]
#[test]
fn changed_baseline_denies_save_before_scratch() {
    let fixture = Fixture::new();
    seed(&fixture);
    let request = original(Cancellation::default());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    let loaded = writer.loaded_catalog().unwrap();
    let path = fixture.store.manifest_path();
    let mut changed = fs::read(&path).unwrap();
    changed[0] = b'[';
    fs::write(&path, &changed).unwrap();
    assert_eq!(
        writer
            .save_native_projection(loaded, None, ArtifactStage::Commit)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(path).unwrap(), changed);

    no_scratch(&fixture);
}
#[test]
fn scratch_is_cleaned_on_success_and_sync_failure() {
    for fail_sync in [false, true] {
        let fixture = Fixture::new();
        seed(&fixture);
        let request = original(Cancellation::default());
        let writer = fixture.store.writer_with_context(&request).unwrap();
        let old = fs::read(fixture.store.manifest_path()).unwrap();
        if fail_sync {
            catalog::test_hooks::SYNC.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(|_| Err(io::ErrorKind::WouldBlock.into())))
            });
        }
        let result = writer.save_native_projection(
            writer.loaded_catalog().unwrap(),
            None,
            ArtifactStage::Commit,
        );
        if fail_sync {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);

            assert_eq!(fs::read(fixture.store.manifest_path()).unwrap(), old);
        } else {
            result.unwrap();

            assert!(catalog::read(&fixture.store).unwrap().is_some());
        }
        no_scratch(&fixture);
    }
}

#[test]
fn cancellation_after_scratch_sync_prevents_first_publication() {
    let fixture = Fixture::new();
    let signal = Cancellation::default();
    let request = original(signal.clone());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    catalog::test_hooks::SYNC.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |_| {
            signal.cancel();
            Ok(())
        }));
    });
    assert_eq!(
        writer
            .save_native_projection(
                writer.loaded_catalog().unwrap(),
                None,
                ArtifactStage::Commit
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!fixture.store.manifest_path().exists());
    no_scratch(&fixture);
}

#[cfg(windows)]
#[test]
fn current_body_handle_blocks_write_delete_and_replacement_until_close() {
    let fixture = Fixture::new();
    seed(&fixture);
    let request = original(Cancellation::default());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    let loaded = writer.loaded_catalog().unwrap();
    let path = fixture.store.manifest_path();
    let before = fs::read(&path).unwrap();
    assert!(fs::OpenOptions::new().append(true).open(&path).is_err());
    assert!(fs::remove_file(&path).is_err());
    assert!(fs::rename(&path, path.with_extension("foreign")).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    drop(loaded);
    drop(writer);
    fs::OpenOptions::new().append(true).open(&path).unwrap();
    no_scratch(&fixture);
}
