use super::*;
use crate::{
    artifact_store::{ScopeLimits, StoreOwner},
    brand::Brand,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_fs::NodeKind;
use std::time::{Duration, Instant};

fn request() -> std::sync::Arc<crate::file_io::SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
fn fixture() -> (
    tempfile::TempDir,
    ProjectPaths,
    std::sync::Arc<crate::file_io::SnapshotContext>,
    CapturedPublicationScope,
) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    fs::create_dir_all(root.join(".graphit")).unwrap();
    fs::write(root.join(".graphit/catalog"), b"DATA").unwrap();
    fs::write(root.join(".graphit/sibling"), b"UNSELECTED-CANARY").unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temporary.path(),
        None,
        Some(&temporary.path().join("global")),
    )
    .unwrap();
    let paths = ProjectPaths::at(&root, &installation).unwrap();
    let birth = request();
    let scope = CapturedPublicationScope::project(
        &paths,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &birth,
    )
    .unwrap();
    (temporary, paths, birth, scope)
}
fn capture(
    scope: &CapturedPublicationScope,
    current: &std::sync::Arc<crate::file_io::SnapshotContext>,
) -> CapturedProjectDocumentDirectory {
    CapturedProjectDocumentDirectory::capture(scope, Path::new(".graphit"), limits(), current)
        .unwrap()
}

#[test]
fn global_runtime_empty_selection_probes_without_birth_and_captures_existing_root() {
    let (temporary, paths, _birth, scope) = fixture();
    let current = request();
    assert_eq!(
        CapturedProjectDocumentDirectory::probe_runtime(&scope, Path::new(""), limits(), &current,)
            .unwrap(),
        DirectoryPresence::Absent,
    );
    assert_eq!(
        CapturedProjectDocumentDirectory::capture_runtime(
            &scope,
            Path::new(""),
            limits(),
            &current,
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::NotFound,
    );
    assert!(!temporary.path().join("global").exists());
    fs::create_dir_all(scope.runtime_root()).unwrap();
    fs::write(scope.runtime_root().join("selected"), b"RUNTIME-ROOT").unwrap();
    assert_eq!(
        CapturedProjectDocumentDirectory::probe_runtime(&scope, Path::new(""), limits(), &current,)
            .unwrap(),
        DirectoryPresence::PresentDirectory,
    );
    let runtime = CapturedProjectDocumentDirectory::capture_runtime(
        &scope,
        Path::new(""),
        limits(),
        &current,
    )
    .unwrap();
    assert_eq!(
        runtime
            .read(OsStr::new("selected"), &current)
            .unwrap()
            .bytes(),
        Some(b"RUNTIME-ROOT".as_slice())
    );
    assert!(!paths.root().join(".graphit/runtime").exists());
    for invalid in ["../memory", "/memory", "memory/"] {
        assert_eq!(
            CapturedProjectDocumentDirectory::probe_runtime(
                &scope,
                Path::new(invalid),
                limits(),
                &current,
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    current.cancellation().cancel();
    assert_eq!(
        CapturedProjectDocumentDirectory::capture_runtime(
            &scope,
            Path::new(""),
            limits(),
            &current,
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn global_runtime_probe_is_read_only_and_storage_does_not_rebase_checkout_reads() {
    let (temporary, paths, _birth, scope) = fixture();
    let current = request();
    assert_eq!(
        CapturedProjectDocumentDirectory::probe_runtime(
            &scope,
            &Path::new("memory").join("index.lance"),
            limits(),
            &current
        )
        .unwrap(),
        DirectoryPresence::Absent
    );
    assert!(!temporary.path().join("global").exists());
    let source = capture(&scope, &current);
    assert_eq!(
        source
            .read(OsStr::new("catalog"), &current)
            .unwrap()
            .bytes(),
        Some(b"DATA".as_slice())
    );
    fs::create_dir_all(scope.runtime_root().join("memory/index.lance")).unwrap();
    fs::write(
        scope.runtime_root().join("memory/index.lance/selected"),
        b"GLOBAL",
    )
    .unwrap();
    let runtime = CapturedProjectDocumentDirectory::capture_runtime(
        &scope,
        &Path::new("memory").join("index.lance"),
        limits(),
        &current,
    )
    .unwrap();
    assert_eq!(
        runtime
            .read(OsStr::new("selected"), &current)
            .unwrap()
            .bytes(),
        Some(b"GLOBAL".as_slice())
    );
    assert!(!paths.root().join(".graphit/runtime").exists());
    assert!(
        CapturedProjectDocumentDirectory::capture(
            &scope,
            &Path::new("memory").join("index.lance"),
            limits(),
            &current
        )
        .is_err()
    );
    for invalid in ["../memory", "/memory", "memory/"] {
        assert_eq!(
            CapturedProjectDocumentDirectory::probe_runtime(
                &scope,
                Path::new(invalid),
                limits(),
                &current
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn global_runtime_capture_keeps_project_identity_and_original_cancellation() {
    let (_temporary, paths, _birth, scope) = fixture();
    let current = request();
    fs::create_dir_all(scope.runtime_root().join("task")).unwrap();
    fs::write(scope.runtime_root().join("task/selected"), b"THIS-PROJECT").unwrap();
    let foreign = CapturedPublicationScope::project(
        &paths,
        "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap(),
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &current,
    )
    .unwrap();
    assert_eq!(
        CapturedProjectDocumentDirectory::probe_runtime(
            &foreign,
            Path::new("task"),
            limits(),
            &current
        )
        .unwrap(),
        DirectoryPresence::Absent
    );
    let runtime = CapturedProjectDocumentDirectory::capture_runtime(
        &scope,
        Path::new("task"),
        limits(),
        &current,
    )
    .unwrap();
    assert_eq!(
        runtime
            .read(OsStr::new("selected"), &current)
            .unwrap()
            .bytes(),
        Some(b"THIS-PROJECT".as_slice())
    );
    current.cancellation().cancel();
    assert_eq!(
        runtime
            .read(OsStr::new("selected"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
}

#[cfg(unix)]
#[test]
fn global_runtime_replacement_is_rejected_without_reading_foreign_contents() {
    let (temporary, _paths, _birth, scope) = fixture();
    let current = request();
    fs::create_dir_all(scope.runtime_root().join("task")).unwrap();
    fs::write(scope.runtime_root().join("task/selected"), b"ORIGINAL").unwrap();
    let runtime = CapturedProjectDocumentDirectory::capture_runtime(
        &scope,
        Path::new("task"),
        limits(),
        &current,
    )
    .unwrap();
    let original = temporary.path().join("retained-global");
    fs::rename(temporary.path().join("global"), &original).unwrap();
    fs::create_dir_all(scope.runtime_root().join("task")).unwrap();
    fs::write(scope.runtime_root().join("task/selected"), b"FOREIGN").unwrap();
    assert_eq!(
        runtime
            .read(OsStr::new("selected"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        fs::read(scope.runtime_root().join("task/selected")).unwrap(),
        b"FOREIGN"
    );
    assert_eq!(
        fs::read(original.join("projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/task/selected")).unwrap(),
        b"ORIGINAL"
    );
}
#[test]
fn existing_parent_fixed_reads_and_absent_create_nothing_or_read_siblings() {
    let (_temp, paths, _birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    let value = captured.read(OsStr::new("catalog"), &current).unwrap();
    assert_eq!(value.bytes(), Some(b"DATA".as_slice()));
    let absent = captured.read(OsStr::new("missing"), &current).unwrap();
    assert!(absent.bytes().is_none());

    assert!(!paths.root().join(".graphit/missing").exists());
    assert_eq!(
        fs::read(paths.root().join(".graphit/sibling")).unwrap(),
        b"UNSELECTED-CANARY"
    );
    captured.revalidate_observation(&value, &current).unwrap();
}
#[test]
fn missing_parent_is_not_document_absence_and_selection_is_strict() {
    let (_temp, _paths, _birth, scope) = fixture();
    let current = request();
    for path in [".", "../.graphit", ".graphit/", "/.graphit"] {
        assert!(
            CapturedProjectDocumentDirectory::capture(&scope, Path::new(path), limits(), &current)
                .is_err()
        );
    }
    assert_eq!(
        CapturedProjectDocumentDirectory::capture(&scope, Path::new("absent"), limits(), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    let captured = capture(&scope, &current);
    for invalid in ["", ".", "..", "nested/catalog", "/catalog"] {
        assert_eq!(
            captured
                .read(OsStr::new(invalid), &current)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
#[test]
fn foreign_request_denies_before_work_and_birth_scope_reuses_current_borrow() {
    let (_temp, _paths, birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &birth)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(matches!(scope.owner(), StoreOwner::Project { .. }));
    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .unwrap()
            .bytes(),
        Some(b"DATA".as_slice())
    );
}
#[test]
fn observation_is_last_owner_of_root_use_and_retained_metadata() {
    let (_temp, _paths, _birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    let observation = captured.read(OsStr::new("catalog"), &current).unwrap();
    let weak = Arc::downgrade(&captured.chain);
    drop(captured);
    assert!(weak.upgrade().is_some());
    assert_eq!(observation.bytes(), Some(b"DATA".as_slice()));
    drop(observation);
    assert!(weak.upgrade().is_none());
}

#[test]
fn project_observation_cannot_enter_published_path_or_stage_domain() {
    let (_temp, paths, _birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    let observation = captured.read(OsStr::new("catalog"), &current).unwrap();
    let published =
        CapturedDocumentDirectory::capture(&paths.root().join(".graphit"), limits(), &current)
            .unwrap();
    assert_eq!(
        published
            .check_expected(&observation, &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        published
            .revalidate_observation(&observation, &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn birthroot_projection_is_actual_root_and_does_not_substitute_child_identity() {
    use crate::artifact_store::{OwnerRootBindingLimits, OwnerRootIdentity};
    let (_temp, paths, _birth, scope) = fixture();
    let current = request();
    let value = scope
        .project_owner_binding(OwnerRootBindingLimits::new(64, 1024).unwrap(), &current)
        .unwrap();
    let root = read_directory::open_root_readonly(paths.root(), &mut || Ok(())).unwrap();
    let child =
        read_directory::open_root_readonly(&paths.root().join(".graphit"), &mut || Ok(())).unwrap();
    let expected = handle_id(&root).unwrap();
    assert!(expected != handle_id(&child).unwrap());
    match (expected, value.root_identity()) {
        (
            PhysicalId::Unix { device, inode },
            OwnerRootIdentity::Unix {
                device: observed_device,
                inode: observed_inode,
            },
        ) => {
            assert_eq!(device, observed_device);
            assert_eq!(inode, observed_inode);
        }
        (
            PhysicalId::Windows {
                volume,
                index,
                created,
            },
            OwnerRootIdentity::Windows {
                volume: observed_volume,
                index: observed_index,
                created: observed_created,
            },
        ) => {
            assert_eq!(volume, observed_volume);
            assert_eq!(index, observed_index);
            assert_eq!(created, observed_created);
        }
        _ => panic!("wrong root identity platform"),
    }
    assert_eq!(value.root_kind(), NodeKind::Directory);
    assert_eq!(value.brand(), scope.brand().key());

    assert!(
        !value
            .wire()
            .windows(paths.root().as_os_str().as_encoded_bytes().len())
            .any(|bytes| bytes == paths.root().as_os_str().as_encoded_bytes())
    );
    let wire_length = value.wire().len();
    drop(value);
    assert!(
        scope
            .project_owner_binding(
                OwnerRootBindingLimits::new(7, wire_length).unwrap(),
                &current
            )
            .is_ok()
    );
    assert!(
        scope
            .project_owner_binding(OwnerRootBindingLimits::new(6, 1024).unwrap(), &current)
            .is_err()
    );
    assert!(
        scope
            .project_owner_binding(
                OwnerRootBindingLimits::new(7, wire_length - 1).unwrap(),
                &current
            )
            .is_err()
    );
}

#[test]
fn file_limits_have_exact_boundaries() {
    let (_temp, _paths, _birth, scope) = fixture();
    let current = request();
    let directory = capture(&scope, &current);
    assert_eq!(
        directory
            .read(OsStr::new("catalog"), &current)
            .unwrap()
            .bytes(),
        Some(b"DATA".as_slice())
    );
    for index in 0..4 {
        let succeeds = |limit| {
            let mut file = [65536, 4096, 128, 65536];
            file[index] = limit;
            let limits = NativeDocumentFileLimits::new(file[0], file[1], file[2], file[3]).unwrap();
            let current = request();
            let result = CapturedProjectDocumentDirectory::capture(
                &scope,
                Path::new(".graphit"),
                limits,
                &current,
            )
            .and_then(|directory| directory.read(OsStr::new("catalog"), &current));
            let positive = match &result {
                Ok(observation) => {
                    assert_eq!(observation.bytes(), Some(b"DATA".as_slice()));
                    true
                }
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
                    false
                }
            };
            drop(result);
            positive
        };
        let mut high = [65536, 4096, 128, 65536][index];
        assert!(succeeds(high));
        let mut low = 1;
        while low < high {
            let middle = low + (high - low) / 2;
            if succeeds(middle) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(succeeds(high));
        assert!(high > 1);
        assert!(!succeeds(high - 1));
    }
}

#[test]
fn each_existing_scope_capacity_is_explicit_and_independently_exercised() {
    let (_temp, paths, _old_birth, old_scope) = fixture();
    drop(old_scope);
    for index in 0..3 {
        let succeeds = |limit| {
            let mut bounds = [4, 4096, 256];
            bounds[index] = limit;
            let scope_limits = ScopeLimits::new(bounds[0], bounds[1], bounds[2]).unwrap();
            let birth = request();
            let scope = CapturedPublicationScope::project(
                &paths,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                scope_limits,
                &birth,
            );
            match scope {
                Ok(scope) => {
                    let current = request();
                    let value = CapturedProjectDocumentDirectory::capture(
                        &scope,
                        Path::new(".graphit"),
                        limits(),
                        &current,
                    )
                    .and_then(|directory| directory.read(OsStr::new("catalog"), &current));
                    let ok = value
                        .as_ref()
                        .is_ok_and(|value| value.bytes() == Some(b"DATA".as_slice()));
                    drop(value);
                    ok
                }
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
                    false
                }
            }
        };
        let mut high = [4, 4096, 256][index];
        assert!(succeeds(high));
        let mut low = 1;
        while low < high {
            let middle = low + (high - low) / 2;
            if succeeds(middle) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(succeeds(high));
        if high > 1 {
            assert!(!succeeds(high - 1));
        } else {
            let mut bounds = [4, 4096, 256];
            bounds[index] = 0;
            assert!(ScopeLimits::new(bounds[0], bounds[1], bounds[2]).is_err());
        }
    }
}

#[test]
fn installation_owner_is_denied_before_project_read_work() {
    use crate::installation_identity::InstallationId;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("global");
    fs::create_dir(&root).unwrap();
    let paths =
        InstallationPaths::resolve(Brand::default(), temporary.path(), None, Some(&root)).unwrap();
    let birth = request();
    let scope = CapturedPublicationScope::installation(
        &paths,
        InstallationId::new("fixture-installation").unwrap(),
        [root.clone()],
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &birth,
    )
    .unwrap();
    let current = request();
    assert_eq!(
        CapturedProjectDocumentDirectory::capture(
            &scope,
            Path::new(".graphit"),
            limits(),
            &current
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        CapturedProjectDocumentDirectory::capture(&scope, Path::new(""), limits(), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn deadline_during_body_stops_original_without_reset_or_refund() {
    let (_temp, _paths, _birth, scope) = fixture();
    let deadline = Instant::now() + Duration::from_secs(5);
    let original = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
    let current = Arc::clone(&original);
    let captured = capture(&scope, &current);
    test_hooks::AFTER_CHUNK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            std::thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
        }))
    });
    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );

    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        captured.revalidate(&current).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn cancellation_before_and_during_body_preserves_original_spending() {
    let (_temp, _paths, _birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    let cancellation = current.cancellation().clone();
    test_hooks::AFTER_CHUNK
        .with(|slot| *slot.borrow_mut() = Some(Box::new(move || cancellation.cancel())));
    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );

    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
}
#[cfg(unix)]
#[test]
fn root_replaced_after_first_fence_never_reads_replacement_body() {
    let (_temp, paths, _birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    let root = paths.root().to_path_buf();
    let saved = root.with_extension("saved");
    test_hooks::BEFORE_OPEN.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            fs::rename(&root, &saved).unwrap();
            fs::create_dir_all(root.join(".graphit")).unwrap();
            fs::write(root.join(".graphit/catalog"), b"REPLACEMENT-BODY").unwrap();
        }))
    });
    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
}
#[cfg(unix)]
#[test]
fn parent_replaced_after_first_fence_never_reads_replacement_body() {
    let (_temp, paths, _birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    let parent = paths.root().join(".graphit");
    let saved = paths.root().join("saved");
    test_hooks::BEFORE_OPEN.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            fs::rename(&parent, &saved).unwrap();
            fs::create_dir(&parent).unwrap();
            fs::write(parent.join("catalog"), b"REPLACEMENT-BODY").unwrap();
        }))
    });
    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
}
#[cfg(unix)]
#[test]
fn growth_during_body_read_returns_no_partial_observation() {
    use std::io::Write;
    let (_temp, paths, _birth, scope) = fixture();
    let current = request();
    let captured = capture(&scope, &current);
    let path = paths.root().join(".graphit/catalog");
    test_hooks::AFTER_CHUNK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"GROWTH")
                .unwrap();
        }))
    });
    assert_eq!(
        captured
            .read(OsStr::new("catalog"), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
}
#[cfg(unix)]
#[test]
fn links_hardlinks_and_nonfiles_are_not_absent() {
    use std::os::unix::fs::symlink;
    let (_temp, paths, _birth, scope) = fixture();
    let parent = paths.root().join(".graphit");
    symlink("catalog", parent.join("linked")).unwrap();
    fs::hard_link(parent.join("catalog"), parent.join("hardlinked")).unwrap();
    fs::create_dir(parent.join("directory")).unwrap();
    let current = request();
    let captured = capture(&scope, &current);
    for name in ["linked", "hardlinked", "directory"] {
        let error = captured.read(OsStr::new(name), &current).err().unwrap();
        assert_ne!(error.kind(), io::ErrorKind::NotFound);
    }
}

#[test]
fn physically_readonly_file_is_a_positive_readonly_project_observation() {
    let (_temp, paths, _birth, scope) = fixture();
    let path = paths.root().join(".graphit/catalog");
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    let current = request();
    let captured = capture(&scope, &current);
    let observation = captured.read(OsStr::new("catalog"), &current).unwrap();
    assert_eq!(observation.bytes(), Some(b"DATA".as_slice()));
    captured
        .revalidate_observation(&observation, &current)
        .unwrap();

    drop(observation);
    drop(captured);
    // Restore only the known fixture's original permissions after all handles close.
    fs::set_permissions(&path, original).unwrap();
}

#[test]
fn directory_revalidation_checks_request_before_original_cancellation() {
    let (_temp, _paths, _birth, scope) = fixture();
    let original = request();
    let foreign = request();
    let captured = Arc::new(capture(&scope, &original));
    captured.revalidate(&original).unwrap();
    assert_eq!(
        captured.revalidate(&foreign).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    original.cancellation().cancel();
    assert_eq!(
        captured.revalidate(&foreign).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        captured.revalidate(&original).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn directory_revalidation_allows_mutable_sdk_files() {
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    let captured = Arc::new(capture(&scope, &original));
    let borrowed = Arc::clone(&captured);
    drop(captured);
    let directory = paths.root().join(".graphit");
    fs::write(directory.join("catalog"), b"SDK-COMMIT").unwrap();
    borrowed.revalidate(&original).unwrap();
    fs::write(directory.join("new-fragment"), b"FRAGMENT").unwrap();
    borrowed.revalidate(&original).unwrap();
    fs::remove_file(directory.join("sibling")).unwrap();
    borrowed.revalidate(&original).unwrap();
    assert_eq!(fs::read(directory.join("catalog")).unwrap(), b"SDK-COMMIT");
}

#[cfg(unix)]
#[test]
fn directory_revalidation_rejects_replaced_root_or_parent() {
    for replace_root in [true, false] {
        let (_temp, paths, _birth, scope) = fixture();
        let original = request();
        let captured = capture(&scope, &original);
        captured.revalidate(&original).unwrap();
        let directory = if replace_root {
            paths.root().to_path_buf()
        } else {
            paths.root().join(".graphit")
        };
        fs::rename(&directory, directory.with_extension("saved")).unwrap();
        fs::create_dir_all(paths.root().join(".graphit")).unwrap();
        fs::write(paths.root().join(".graphit/catalog"), b"REPLACEMENT").unwrap();
        assert_eq!(
            captured.revalidate(&original).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

fn empty_project() -> (
    tempfile::TempDir,
    ProjectPaths,
    Arc<SnapshotContext>,
    CapturedPublicationScope,
) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    fs::create_dir(&root).unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temporary.path(),
        None,
        Some(&temporary.path().join("global")),
    )
    .unwrap();
    let paths = ProjectPaths::at(&root, &installation).unwrap();
    let original = request();
    let scope = CapturedPublicationScope::project(
        &paths,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &original,
    )
    .unwrap();
    (temporary, paths, original, scope)
}
#[test]
fn child_directory_probe_empty_root_handles_first_use_without_runtime_birth() {
    let (_temporary, paths, original, scope) = empty_project();
    let root =
        CapturedProjectDocumentDirectory::capture(&scope, Path::new(""), limits(), &original)
            .unwrap();
    assert!(Arc::ptr_eq(root.chain.request(), &original));
    let opened = read_directory::open_root_readonly(paths.root(), &mut || Ok(())).unwrap();
    assert!(*root.chain.identity().unwrap() == handle_id(&opened).unwrap());
    assert_eq!(
        root.probe_child_directory(OsStr::new(".graphit"), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
    assert!(!paths.root().join(".graphit").exists());
    fs::create_dir(paths.root().join(".graphit")).unwrap();
    assert_eq!(
        root.probe_child_directory(OsStr::new(".graphit"), &original)
            .unwrap(),
        DirectoryPresence::PresentDirectory
    );
    let brand = CapturedProjectDocumentDirectory::capture(
        &scope,
        Path::new(".graphit"),
        limits(),
        &original,
    )
    .unwrap();
    assert_eq!(
        brand
            .probe_child_directory(OsStr::new("runtime"), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
    assert!(!paths.root().join(".graphit/runtime").exists());
    fs::create_dir(paths.root().join(".graphit/runtime")).unwrap();
    let runtime = CapturedProjectDocumentDirectory::capture(
        &scope,
        &Path::new(".graphit").join("runtime"),
        limits(),
        &original,
    )
    .unwrap();
    assert_eq!(
        runtime
            .probe_child_directory(OsStr::new("knowledge"), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
    assert!(!paths.root().join(".graphit/runtime/knowledge").exists());
    fs::create_dir(paths.root().join(".graphit/runtime/knowledge")).unwrap();
    let knowledge = CapturedProjectDocumentDirectory::capture(
        &scope,
        &Path::new(".graphit").join("runtime").join("knowledge"),
        limits(),
        &original,
    )
    .unwrap();
    assert_eq!(
        knowledge
            .probe_child_directory(OsStr::new("index.lance"), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
    fs::create_dir(paths.root().join(".graphit/runtime/knowledge/index.lance")).unwrap();
    assert_eq!(
        knowledge
            .probe_child_directory(OsStr::new("index.lance"), &original)
            .unwrap(),
        DirectoryPresence::PresentDirectory
    );
    assert!(
        !paths
            .root()
            .join(".graphit/runtime/artifact-store.lock")
            .exists()
    );
    knowledge.revalidate(&original).unwrap();
}
#[test]
fn child_directory_probe_empty_root_respects_root_caps_and_selected_depth() {
    let (_temporary, paths, original, scope) = empty_project();
    let root_bytes = native_name_bytes(scope.owner_root().as_os_str()).unwrap();
    let exact = NativeDocumentFileLimits::new(65536, root_bytes, 1, 65536).unwrap();
    CapturedProjectDocumentDirectory::capture(&scope, Path::new(""), exact, &original)
        .unwrap()
        .revalidate(&original)
        .unwrap();
    {
        let small = NativeDocumentFileLimits::new(65536, root_bytes - 1, 1, 65536).unwrap();
        assert_eq!(
            CapturedProjectDocumentDirectory::capture(&scope, Path::new(""), small, &original)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
    assert_eq!(
        CapturedProjectDocumentDirectory::capture(&scope, Path::new(".graphit"), exact, &original)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert!(NativeDocumentFileLimits::new(65536, 4096, 0, 65536).is_err());
    for invalid in [".", "..", "/", "../project"] {
        assert_eq!(
            CapturedProjectDocumentDirectory::capture(
                &scope,
                Path::new(invalid),
                limits(),
                &original
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(!paths.root().join(".graphit").exists());
}
#[test]
fn child_directory_probe_is_exact_and_drops_temporary_directory_handles() {
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    let captured = capture(&scope, &original);
    let child = paths.root().join(".graphit/knowledge");
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("knowledge"), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
    fs::create_dir(&child).unwrap();
    fs::write(child.join("canary"), b"keep").unwrap();
    for _ in 0..2 {
        assert_eq!(
            captured
                .probe_child_directory(OsStr::new("knowledge"), &original)
                .unwrap(),
            DirectoryPresence::PresentDirectory
        );
        captured.revalidate(&original).unwrap();
    }
    assert_eq!(fs::read(child.join("canary")).unwrap(), b"keep");
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("other"), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
    assert!(!paths.root().join(".graphit/other").exists());
}
#[test]
fn child_directory_probe_rejects_wrong_types_links_and_invalid_names() {
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    let captured = capture(&scope, &original);
    assert!(
        captured
            .probe_child_directory(OsStr::new("catalog"), &original)
            .is_err()
    );
    for name in ["", ".", "..", "nested/child", "/child", "nul\0child"] {
        assert_eq!(
            captured
                .probe_child_directory(OsStr::new(name), &original)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(
            paths.root().join(".graphit"),
            paths.root().join(".graphit/link"),
        )
        .unwrap();
        symlink(
            paths.root().join("missing"),
            paths.root().join(".graphit/dangling"),
        )
        .unwrap();
        for name in ["link", "dangling"] {
            assert!(
                captured
                    .probe_child_directory(OsStr::new(name), &original)
                    .is_err()
            );
        }
    }
    assert_eq!(
        fs::read(paths.root().join(".graphit/catalog")).unwrap(),
        b"DATA"
    );
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("missing"), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
}
#[test]
fn child_directory_probe_checks_exact_child_path_bounds_and_native_names() {
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    let parent_bytes = capture(&scope, &original).parent_path_bytes;
    let restricted = NativeDocumentFileLimits::new(65536, parent_bytes, 128, 65536).unwrap();
    let captured = CapturedProjectDocumentDirectory::capture(
        &scope,
        Path::new(".graphit"),
        restricted,
        &original,
    )
    .unwrap();
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("child"), &original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    let captured = capture(&scope, &original);
    let name = OsStr::new("corpus 🚀");
    fs::create_dir(paths.root().join(".graphit").join(name)).unwrap();
    assert_eq!(
        captured.probe_child_directory(name, &original).unwrap(),
        DirectoryPresence::PresentDirectory
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let name = OsStr::from_bytes(b"native-\xff");
        #[cfg(target_os = "linux")]
        fs::create_dir(paths.root().join(".graphit").join(name)).unwrap();
        assert_eq!(
            captured
                .probe_child_directory(name, &original)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
#[test]
fn child_directory_probe_rejects_foreign_request_and_original_cancel() {
    let (_temp, _paths, _birth, scope) = fixture();
    let signal = Cancellation::default();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        signal.clone(),
    ));
    let captured = capture(&scope, &original);
    let foreign = request();
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("missing"), &foreign)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    signal.cancel();
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("missing"), &foreign)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("missing"), &original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
}
#[test]
fn child_directory_probe_rejects_expired_original_without_reset() {
    let (_temp, _paths, _birth, scope) = fixture();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    ));
    let captured = capture(&scope, &original);
    std::thread::sleep(
        original
            .deadline()
            .unwrap()
            .saturating_duration_since(Instant::now()),
    );
    assert_eq!(
        captured
            .probe_child_directory(OsStr::new("missing"), &original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
}
#[test]
fn child_directory_probe_missing_chain_is_an_error_not_absent() {
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    let captured = capture(&scope, &original);
    #[cfg(windows)]
    let identity = captured.chain.identity().unwrap().clone();
    let retained = paths.root().join("retained");
    let renamed = fs::rename(paths.root().join(".graphit"), &retained);
    #[cfg(unix)]
    {
        renamed.unwrap();
        assert_eq!(
            captured
                .probe_child_directory(OsStr::new("missing"), &original)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }
    #[cfg(windows)]
    {
        assert_eq!(renamed.unwrap_err().raw_os_error(), Some(32));
        captured.revalidate(&original).unwrap();
        assert!(
            physical_id(
                &paths.root().join(".graphit"),
                &fs::symlink_metadata(paths.root().join(".graphit")).unwrap()
            )
            .unwrap()
                == identity
        );
        assert_eq!(
            captured
                .probe_child_directory(OsStr::new("missing"), &original)
                .unwrap(),
            DirectoryPresence::Absent
        );
        assert_eq!(
            fs::read(paths.root().join(".graphit/catalog")).unwrap(),
            b"DATA"
        );
        assert_eq!(
            fs::read(paths.root().join(".graphit/sibling")).unwrap(),
            b"UNSELECTED-CANARY"
        );
        assert!(!retained.exists());
        assert!(!paths.root().join(".graphit/missing").exists());
    }
}
#[test]
fn child_directory_probe_postvalidation_rejects_changed_parent_despite_child_notfound() {
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    let captured = capture(&scope, &original);
    let old = paths.root().join(".graphit");
    let retained = paths.root().join("retained");
    #[cfg(windows)]
    let identity = captured.chain.identity().unwrap().clone();
    test_hooks::BEFORE_OPEN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            let renamed = fs::rename(old, retained);
            #[cfg(unix)]
            renamed.unwrap();
            #[cfg(windows)]
            assert_eq!(renamed.unwrap_err().raw_os_error(), Some(32));
        }))
    });
    #[cfg(unix)]
    assert!(
        captured
            .probe_child_directory(OsStr::new("missing"), &original)
            .is_err()
    );
    #[cfg(windows)]
    {
        assert_eq!(
            captured
                .probe_child_directory(OsStr::new("missing"), &original)
                .unwrap(),
            DirectoryPresence::Absent
        );
        captured.revalidate(&original).unwrap();
        assert!(
            physical_id(
                &paths.root().join(".graphit"),
                &fs::symlink_metadata(paths.root().join(".graphit")).unwrap()
            )
            .unwrap()
                == identity
        );
        assert_eq!(
            fs::read(paths.root().join(".graphit/catalog")).unwrap(),
            b"DATA"
        );
        assert_eq!(
            fs::read(paths.root().join(".graphit/sibling")).unwrap(),
            b"UNSELECTED-CANARY"
        );
        assert!(!paths.root().join("retained").exists());
        assert!(!paths.root().join(".graphit/missing").exists());
    }
}
#[test]
fn child_directory_probe_postvalidation_rejects_replaced_owner_root() {
    let (temporary, paths, original, scope) = empty_project();
    #[cfg(windows)]
    fs::write(paths.root().join("canary"), b"OWNER-ROOT").unwrap();
    let captured =
        CapturedProjectDocumentDirectory::capture(&scope, Path::new(""), limits(), &original)
            .unwrap();
    let root = paths.root().to_owned();
    let old = temporary.path().join("retained-root");
    #[cfg(windows)]
    let identity = captured.chain.identity().unwrap().clone();
    test_hooks::BEFORE_OPEN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            let renamed = fs::rename(&root, &old);
            #[cfg(unix)]
            {
                renamed.unwrap();
                fs::create_dir(root).unwrap();
            }
            #[cfg(windows)]
            assert_eq!(renamed.unwrap_err().raw_os_error(), Some(32));
        }))
    });
    #[cfg(unix)]
    assert!(
        captured
            .probe_child_directory(OsStr::new("missing"), &original)
            .is_err()
    );
    #[cfg(windows)]
    {
        assert_eq!(
            captured
                .probe_child_directory(OsStr::new("missing"), &original)
                .unwrap(),
            DirectoryPresence::Absent
        );
        captured.revalidate(&original).unwrap();
        assert!(
            physical_id(paths.root(), &fs::symlink_metadata(paths.root()).unwrap()).unwrap()
                == identity
        );
        assert_eq!(
            fs::read(paths.root().join("canary")).unwrap(),
            b"OWNER-ROOT"
        );
        assert!(!temporary.path().join("retained-root").exists());
        assert!(!paths.root().join("missing").exists());
    }
}
#[cfg(target_os = "linux")]
#[test]
fn child_directory_probe_propagates_real_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    let captured = capture(&scope, &original);
    let child = paths.root().join(".graphit/denied");
    fs::create_dir(&child).unwrap();
    fs::set_permissions(&child, fs::Permissions::from_mode(0o0)).unwrap();
    let result = captured.probe_child_directory(OsStr::new("denied"), &original);
    fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
    if rustix::process::geteuid().is_root() {
        assert_eq!(result.unwrap(), DirectoryPresence::PresentDirectory);
    } else {
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }
    captured.revalidate(&original).unwrap();
}

#[test]
fn child_directory_probe_empty_root_capture_preserves_cancel_and_deadline() {
    let (_temporary, paths, _birth, scope) = empty_project();
    let signal = Cancellation::default();
    let cancelled = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        signal.clone(),
    ));
    signal.cancel();
    assert_eq!(
        CapturedProjectDocumentDirectory::capture(&scope, Path::new(""), limits(), &cancelled)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert_eq!(
        CapturedProjectDocumentDirectory::capture(&scope, Path::new(""), limits(), &expired)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(!paths.root().join(".graphit").exists());
}
