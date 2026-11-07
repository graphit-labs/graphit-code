use super::*;
use std::sync::Arc;

fn original(cancellation: Cancellation) -> std::sync::Arc<crate::file_io::SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        cancellation,
    ))
}

#[test]
fn genuine_project_and_installation_writers_keep_original_until_physical_close() {
    for global in [false, true] {
        let fixture = Fixture::with_owner(global);
        let request = original(Cancellation::default());
        let writer = fixture.store.writer_with_context(&request).unwrap();
        let native = writer.native.as_ref().unwrap();
        assert!(native.request.same_request(&request));
        assert!(Arc::ptr_eq(writer.io_context.as_ref().unwrap(), &request));
        assert_eq!(
            writer.io_context.as_ref().unwrap().deadline(),
            request.deadline()
        );
        let lock = fixture.store.runtime_root().join("artifact-store.lock");
        assert!(FileLock::is_locked(&lock).unwrap());
        drop(writer);
        assert!(!FileLock::is_locked(&lock).unwrap());
    }
}

#[test]
fn continuous_writer_keeps_no_deadline_and_original_cancellation_until_close() {
    let fixture = Fixture::new();
    let request = Arc::new(SnapshotContext::service(Cancellation::default()));
    let writer = fixture.store.writer_with_context(&request).unwrap();
    assert_eq!(writer.io_context.as_ref().unwrap().deadline(), None);
    assert!(
        writer
            .native
            .as_ref()
            .unwrap()
            .request
            .same_request(&request)
    );
    let lock = fixture.store.runtime_root().join("artifact-store.lock");
    request.cancellation().cancel();
    assert_eq!(
        writer.revalidate(&request).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(FileLock::is_locked(&lock).unwrap());
    drop(writer);
    assert!(!FileLock::is_locked(&lock).unwrap());
}

#[test]
fn project_writer_births_global_store_outside_checkout_and_returns_runtime_guard() {
    let directory = tempfile::tempdir().unwrap();
    let checkout = directory.path().join("checkout");
    let global = directory.path().join("global");
    fs::create_dir(&checkout).unwrap();
    let installation = crate::project_paths::InstallationPaths::resolve(
        crate::brand::Brand::default(),
        directory.path(),
        None,
        Some(&global),
    )
    .unwrap();
    let paths = ProjectPaths::at(&checkout, &installation).unwrap();
    let request = original(Cancellation::default());
    let scope = CapturedPublicationScope::project(
        &paths,
        ID.parse().unwrap(),
        ScopeLimits::new(4, 4096, 128).unwrap(),
        &request,
    )
    .unwrap();
    assert!(!global.exists());
    let store = ArtifactStore::new(
        scope.clone(),
        ArtifactLimits::new(65536, 1024, CopyLimits::new(65536, 128, 4096, 8).unwrap()).unwrap(),
    );
    let mut writer = store.writer_with_context(&request).unwrap();
    let captured = writer
        .create_record_directory(ProjectRecordDirectory::Memory, record_limits())
        .unwrap();
    let runtime = global.join("projects").join(ID);
    assert_eq!(store.runtime_root(), fs::canonicalize(&runtime).unwrap());
    assert!(runtime.join("artifact-store.lock").is_file());
    fs::write(runtime.join("memory/selected"), b"GLOBAL").unwrap();
    assert_eq!(
        captured
            .read(std::ffi::OsStr::new("selected"), &request)
            .unwrap()
            .bytes(),
        Some(b"GLOBAL".as_slice())
    );
    assert_eq!(fs::read_dir(&checkout).unwrap().count(), 0);
    drop(writer);
    let reader = crate::native_document_file::CapturedProjectDocumentDirectory::capture_runtime(
        &scope,
        Path::new("memory"),
        record_limits(),
        &request,
    )
    .unwrap();
    reader.revalidate(&request).unwrap();
    assert!(!FileLock::is_locked(runtime.join("artifact-store.lock")).unwrap());
}

#[test]
fn cancelled_original_denies_before_any_control_birth() {
    let fixture = Fixture::new();
    let signal = Cancellation::default();
    let request = original(signal.clone());
    signal.cancel();
    let error = fixture.store.writer_with_context(&request).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(!fixture.store.runtime_root().exists());
}

#[test]
fn cancellation_during_final_binding_never_returns_an_acquired_writer() {
    let fixture = Fixture::new();
    let signal = Cancellation::default();
    let request = original(signal.clone());
    native_writer_acquisition_hooks::AFTER_BINDING.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || signal.cancel()));
    });
    let error = fixture.store.writer_with_context(&request).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    // Birth was real: its permanent lock remains, but the failed writer closes it.
    let lock = fixture.store.runtime_root().join("artifact-store.lock");
    assert!(lock.is_file());
    assert!(!FileLock::is_locked(lock).unwrap());
}

#[test]
fn actual_writer_contention_observes_original_cancel_and_preserves_held_lock() {
    let fixture = Fixture::new();
    let first = original(Cancellation::default());
    let held = fixture.store.writer_with_context(&first).unwrap();
    let signal = Cancellation::default();
    let request = original(signal.clone());
    let deadline = request.deadline();
    crate::file_lock::BEFORE_WAIT.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || signal.cancel()));
    });
    let error = fixture.store.writer_with_context(&request).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(request.deadline(), deadline);
    assert!(FileLock::is_locked(fixture.store.runtime_root().join("artifact-store.lock")).unwrap());
    drop(held);
}

#[test]
fn native_writer_rejects_even_the_same_io_context_instead_of_rebinding() {
    let fixture = Fixture::new();
    let request = original(Cancellation::default());
    let writer = fixture.store.writer_with_context(&request).unwrap();
    let error = writer.with_io_context(Arc::clone(&request)).unwrap_err();
    assert_eq!(error.stage(), ArtifactStage::Validate);
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(
        !FileLock::is_locked(fixture.store.runtime_root().join("artifact-store.lock")).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn owner_root_replacement_does_not_birth_control_in_the_attacker_root() {
    let fixture = Fixture::new();
    let root = fixture.source.parent().unwrap();
    let holder = tempfile::tempdir().unwrap();
    let retained = holder.path().join("retained-owner");
    let runtime_relative = fixture
        .store
        .runtime_root()
        .strip_prefix(root)
        .unwrap()
        .to_owned();
    fs::rename(root, &retained).unwrap();
    fs::create_dir(root).unwrap();
    let request = original(Cancellation::default());
    let result = fixture.store.writer_with_context(&request);
    assert!(result.is_err());
    assert!(!root.join(&runtime_relative).exists());
    assert!(!retained.join(&runtime_relative).exists());
    fs::remove_dir(root).unwrap();
    fs::rename(retained, root).unwrap();
}

fn record_limits() -> crate::native_document_file::NativeDocumentFileLimits {
    crate::native_document_file::NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
#[test]
fn record_directory_birth_returns_original_guard_and_private_readable_directory() {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    for kind in [
        ProjectRecordDirectory::Task,
        ProjectRecordDirectory::Knowledge,
        ProjectRecordDirectory::Memory,
    ] {
        let fixture = Fixture::new();
        let request = original(Cancellation::default());
        let mut writer = fixture.store.writer_with_context(&request).unwrap();
        let captured = writer
            .create_record_directory(kind, record_limits())
            .unwrap();
        let path = fixture.store.runtime_root().join(kind.directory_name());
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        captured.revalidate(&request).unwrap();
        fs::write(path.join("document"), b"owned corpus").unwrap();
        assert_eq!(
            captured
                .read(std::ffi::OsStr::new("document"), &request)
                .unwrap()
                .bytes(),
            Some(b"owned corpus".as_slice())
        );
        let foreign = original(Cancellation::default());
        assert_eq!(
            captured.revalidate(&foreign).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        drop(writer);
        assert!(
            !FileLock::is_locked(fixture.store.runtime_root().join("artifact-store.lock")).unwrap()
        );
        captured.revalidate(&request).unwrap();
        drop(captured);
        assert_eq!(fs::read(path.join("document")).unwrap(), b"owned corpus");
    }
}
#[cfg(unix)]
#[test]
fn record_directory_birth_rejects_every_existing_entry_without_adoption() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    for directory in [
        ProjectRecordDirectory::Task,
        ProjectRecordDirectory::Knowledge,
        ProjectRecordDirectory::Memory,
    ] {
        for kind in ["directory", "symlink", "file", "socket"] {
            let fixture = Fixture::new();
            let request = original(Cancellation::default());
            let mut writer = fixture.store.writer_with_context(&request).unwrap();
            let path = fixture
                .store
                .runtime_root()
                .join(directory.directory_name());
            let mut socket = None;
            match kind {
                "directory" => {
                    fs::create_dir(&path).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                    fs::write(path.join("canary"), b"keep").unwrap();
                }
                "symlink" => symlink(&fixture.source, &path).unwrap(),
                "socket" => socket = Some(std::os::unix::net::UnixListener::bind(&path).unwrap()),
                _ => fs::write(&path, b"keep").unwrap(),
            }
            let before = fs::symlink_metadata(&path).unwrap();
            assert_eq!(
                writer
                    .create_record_directory(directory, record_limits())
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::AlreadyExists
            );
            let after = fs::symlink_metadata(&path).unwrap();
            assert_eq!(
                (before.dev(), before.ino(), before.mode()),
                (after.dev(), after.ino(), after.mode())
            );
            match kind {
                "directory" => assert_eq!(fs::read(path.join("canary")).unwrap(), b"keep"),
                "symlink" => assert_eq!(fs::read_link(&path).unwrap(), fixture.source),
                "socket" => assert!(socket.is_some()),
                _ => assert_eq!(fs::read(&path).unwrap(), b"keep"),
            }
        }
    }
}
#[test]
fn knowledge_directory_birth_is_serialized_by_the_original_writer_lock() {
    let fixture = Fixture::new();
    let first = original(Cancellation::default());
    let mut writer = fixture.store.writer_with_context(&first).unwrap();
    let captured = writer
        .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
        .unwrap();
    let cancellation = Cancellation::default();
    let second = original(cancellation.clone());
    crate::file_lock::BEFORE_WAIT
        .with(|hook| *hook.borrow_mut() = Some(Box::new(move || cancellation.cancel())));
    assert_eq!(
        fixture
            .store
            .writer_with_context(&second)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    drop(writer);
    let later = original(Cancellation::default());
    let mut later_writer = fixture.store.writer_with_context(&later).unwrap();
    assert_eq!(
        later_writer
            .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    captured.revalidate(&first).unwrap();
}
#[test]
fn record_directory_birth_checks_original_cancellation_and_bounds_before_mkdir() {
    for kind in [
        ProjectRecordDirectory::Task,
        ProjectRecordDirectory::Knowledge,
        ProjectRecordDirectory::Memory,
    ] {
        let fixture = Fixture::new();
        let signal = Cancellation::default();
        let request = original(signal.clone());
        let mut writer = fixture.store.writer_with_context(&request).unwrap();
        let small =
            crate::native_document_file::NativeDocumentFileLimits::new(65536, 4096, 1, 65536)
                .unwrap();
        assert_eq!(
            writer
                .create_record_directory(kind, small)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert!(
            !fixture
                .store
                .runtime_root()
                .join(kind.directory_name())
                .exists()
        );
        for small in [
            crate::native_document_file::NativeDocumentFileLimits::new(65536, 1, 128, 65536)
                .unwrap(),
            crate::native_document_file::NativeDocumentFileLimits::new(
                65536,
                crate::file_io::native_name_bytes(
                    fixture
                        .store
                        .runtime_root()
                        .join(kind.directory_name())
                        .as_os_str(),
                )
                .unwrap()
                    - 1,
                128,
                65536,
            )
            .unwrap(),
        ] {
            assert_eq!(
                writer
                    .create_record_directory(kind, small)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::FileTooLarge
            );
            assert!(
                !fixture
                    .store
                    .runtime_root()
                    .join(kind.directory_name())
                    .exists()
            );
        }
        signal.cancel();
        assert_eq!(
            writer
                .create_record_directory(kind, record_limits())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(
            !fixture
                .store
                .runtime_root()
                .join(kind.directory_name())
                .exists()
        );
    }
}

#[test]
fn record_directory_birth_rejects_normal_and_installation_writers() {
    for kind in [
        ProjectRecordDirectory::Task,
        ProjectRecordDirectory::Knowledge,
        ProjectRecordDirectory::Memory,
    ] {
        let fixture = Fixture::new();
        let mut writer = fixture.writer();
        assert_eq!(
            writer
                .create_record_directory(kind, record_limits())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(
            !fixture
                .store
                .runtime_root()
                .join(kind.directory_name())
                .exists()
        );
        let global = Fixture::with_owner(true);
        let request = original(Cancellation::default());
        let mut writer = global.store.writer_with_context(&request).unwrap();
        assert_eq!(
            writer
                .create_record_directory(kind, record_limits())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(
            !global
                .store
                .runtime_root()
                .join(kind.directory_name())
                .exists()
        );
    }
}

#[test]
fn knowledge_directory_birth_rejects_expired_original_before_creation() {
    let fixture = Fixture::new();
    let request = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(2),
        Cancellation::default(),
    ));
    let mut writer = fixture.store.writer_with_context(&request).unwrap();
    std::thread::sleep(
        request
            .deadline()
            .unwrap()
            .saturating_duration_since(Instant::now()),
    );
    assert_eq!(
        writer
            .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(!fixture.store.runtime_root().join("knowledge").exists());
}
#[test]
fn record_directory_birth_after_mkdir_cancel_reports_error_and_real_residue() {
    for kind in [
        ProjectRecordDirectory::Task,
        ProjectRecordDirectory::Knowledge,
        ProjectRecordDirectory::Memory,
    ] {
        let fixture = Fixture::new();
        let signal = Cancellation::default();
        let request = original(signal.clone());
        let mut writer = fixture.store.writer_with_context(&request).unwrap();
        native_writer_acquisition_hooks::AFTER_RECORD_BIRTH
            .with(|hook| *hook.borrow_mut() = Some(Box::new(move || signal.cancel())));
        assert_eq!(
            writer
                .create_record_directory(kind, record_limits())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        let path = fixture.store.runtime_root().join(kind.directory_name());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(path).unwrap().count(), 0);
    }
}

#[cfg(unix)]
#[test]
fn record_directory_birth_parent_replaced_before_mkdir_has_no_attacker_effect() {
    for kind in [
        ProjectRecordDirectory::Task,
        ProjectRecordDirectory::Knowledge,
        ProjectRecordDirectory::Memory,
    ] {
        let fixture = Fixture::new();
        let request = original(Cancellation::default());
        let mut writer = fixture.store.writer_with_context(&request).unwrap();
        let runtime = fixture.store.runtime_root();
        let retained = runtime.with_file_name("retained-runtime");
        fs::rename(runtime, &retained).unwrap();
        fs::create_dir(runtime).unwrap();
        assert!(
            writer
                .create_record_directory(kind, record_limits())
                .is_err()
        );
        assert!(!runtime.join(kind.directory_name()).exists());
        assert!(!retained.join(kind.directory_name()).exists());
    }
}

#[cfg(unix)]
#[test]
fn knowledge_directory_birth_child_swapped_after_mkdir_never_returns_a_guard() {
    let fixture = Fixture::new();
    let request = original(Cancellation::default());
    let mut writer = fixture.store.writer_with_context(&request).unwrap();
    let path = fixture.store.runtime_root().join("knowledge");
    let swapped = path.with_file_name("retained-knowledge");
    let (changed, retained) = (path.clone(), swapped.clone());
    native_writer_acquisition_hooks::AFTER_RECORD_BIRTH.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            fs::rename(&changed, &retained).unwrap();
            fs::create_dir(&changed).unwrap();
        }))
    });
    assert!(
        writer
            .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
            .is_err()
    );
    assert!(path.is_dir());
    assert!(swapped.is_dir());
}
#[cfg(unix)]
#[test]
fn knowledge_directory_birth_guard_rejects_later_directory_and_owner_replacement() {
    let fixture = Fixture::new();
    let request = original(Cancellation::default());
    let mut writer = fixture.store.writer_with_context(&request).unwrap();
    let captured = writer
        .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
        .unwrap();
    let path = fixture.store.runtime_root().join("knowledge");
    let retained = path.with_file_name("retained-knowledge");
    fs::rename(&path, &retained).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(captured.revalidate(&request).is_err());
    fs::remove_dir(&path).unwrap();
    fs::rename(&retained, &path).unwrap();
    captured.revalidate(&request).unwrap();
    let root = fixture.source.parent().unwrap();
    let holder = tempfile::tempdir().unwrap();
    let old = holder.path().join("owner");
    fs::rename(root, &old).unwrap();
    fs::create_dir(root).unwrap();
    assert!(captured.revalidate(&request).is_err());
    fs::remove_dir(root).unwrap();
    fs::rename(old, root).unwrap();
}
#[test]
fn record_directory_existing_read_never_births_runtime_lock_or_corpus() {
    for kind in [
        ProjectRecordDirectory::Task,
        ProjectRecordDirectory::Knowledge,
        ProjectRecordDirectory::Memory,
    ] {
        use crate::native_document_file::CapturedProjectDocumentDirectory;
        let fixture = Fixture::new();
        let request = original(Cancellation::default());
        let relative = fixture
            .store
            .runtime_root()
            .strip_prefix(fixture.store.scope.owner_root())
            .unwrap()
            .join(kind.directory_name());
        assert_eq!(
            CapturedProjectDocumentDirectory::capture(
                &fixture.store.scope,
                &relative,
                record_limits(),
                &request
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!fixture.store.runtime_root().exists());
        let mut writer = fixture.store.writer_with_context(&request).unwrap();
        let born = writer
            .create_record_directory(kind, record_limits())
            .unwrap();
        drop(born);
        drop(writer);
        let lock = fixture.store.runtime_root().join("artifact-store.lock");
        fs::remove_file(&lock).unwrap();
        let captured = CapturedProjectDocumentDirectory::capture(
            &fixture.store.scope,
            &relative,
            record_limits(),
            &request,
        )
        .unwrap();
        assert!(
            captured
                .read(std::ffi::OsStr::new("absent"), &request)
                .unwrap()
                .bytes()
                .is_none()
        );
        assert!(!lock.exists());
        assert!(
            !fixture
                .store
                .runtime_root()
                .join(kind.directory_name())
                .join("absent")
                .exists()
        );
    }
}

#[cfg(unix)]
#[test]
fn knowledge_directory_birth_parent_swapped_after_mkdir_leaves_only_original_residue() {
    let fixture = Fixture::new();
    let request = original(Cancellation::default());
    let mut writer = fixture.store.writer_with_context(&request).unwrap();
    let runtime = fixture.store.runtime_root().to_owned();
    let retained = runtime.with_file_name("retained-runtime");
    let (changed, old) = (runtime.clone(), retained.clone());
    native_writer_acquisition_hooks::AFTER_RECORD_BIRTH.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            fs::rename(&changed, &old).unwrap();
            fs::create_dir(&changed).unwrap();
        }))
    });
    assert!(
        writer
            .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
            .is_err()
    );
    assert!(!runtime.join("knowledge").exists());
    assert!(retained.join("knowledge").is_dir());
}

#[test]
fn record_directories_are_distinct_children_of_the_same_original_writer() {
    let fixture = Fixture::new();
    let request = original(Cancellation::default());
    let mut writer = fixture.store.writer_with_context(&request).unwrap();
    let task = writer
        .create_record_directory(ProjectRecordDirectory::Task, record_limits())
        .unwrap();
    let knowledge = writer
        .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
        .unwrap();
    let memory = writer
        .create_record_directory(ProjectRecordDirectory::Memory, record_limits())
        .unwrap();
    let task_path = fixture.store.runtime_root().join("task");
    let knowledge_path = fixture.store.runtime_root().join("knowledge");
    let memory_path = fixture.store.runtime_root().join("memory");
    fs::write(memory_path.join("document"), b"memory corpus").unwrap();
    fs::write(task_path.join("document"), b"task corpus").unwrap();
    fs::write(knowledge_path.join("document"), b"knowledge corpus").unwrap();
    assert_eq!(
        task.read(std::ffi::OsStr::new("document"), &request)
            .unwrap()
            .bytes(),
        Some(b"task corpus".as_slice())
    );
    assert_eq!(
        knowledge
            .read(std::ffi::OsStr::new("document"), &request)
            .unwrap()
            .bytes(),
        Some(b"knowledge corpus".as_slice())
    );
    assert_eq!(
        memory
            .read(std::ffi::OsStr::new("document"), &request)
            .unwrap()
            .bytes(),
        Some(b"memory corpus".as_slice())
    );
    memory.revalidate(&request).unwrap();
    task.revalidate(&request).unwrap();
    knowledge.revalidate(&request).unwrap();
}

#[test]
fn after_wait_lock_replacement_is_rejected_or_blocked_by_native_sharing() {
    let fixture = Fixture::new();
    let first = original(Cancellation::default());
    let held = fixture.store.writer_with_context(&first).unwrap();
    let expected = crate::native_file_identity::handle_id(held._lock.file()).unwrap();
    let path = fixture.store.runtime_root().join("artifact-store.lock");
    let changed = path.clone();
    crate::file_lock::BEFORE_WAIT.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            #[cfg(unix)]
            {
                fs::remove_file(&changed).unwrap();
                graphit_fs::create_private_lock_file(&changed).unwrap();
            }
            #[cfg(windows)]
            assert!(fs::remove_file(&changed).is_err());
            drop(held);
        }));
    });
    let second = original(Cancellation::default());
    let acquired = fixture.store.writer_with_context(&second);
    #[cfg(unix)]
    {
        let _ = expected;
        assert_eq!(acquired.unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(!FileLock::is_locked(&path).unwrap());
    }
    #[cfg(windows)]
    {
        let writer = acquired.unwrap();
        let observed = crate::native_file_identity::handle_id(writer._lock.file()).unwrap();
        assert!(
            observed == expected,
            "postwait writer must retain the original lock identity"
        );
    }
}

#[cfg(windows)]
#[test]
fn windows_held_control_and_record_handles_block_namespace_replacement() {
    let fixture = Fixture::new();
    let request = original(Cancellation::default());
    let mut writer = fixture.store.writer_with_context(&request).unwrap();
    let captured = writer
        .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
        .unwrap();
    let runtime = fixture.store.runtime_root();
    let corpus = runtime.join("knowledge");
    assert!(fs::rename(runtime, runtime.with_file_name("foreign-runtime")).is_err());
    assert!(fs::rename(&corpus, corpus.with_file_name("foreign-knowledge")).is_err());
    captured.revalidate(&request).unwrap();
    assert!(
        writer
            .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
            .is_err()
    );
}
