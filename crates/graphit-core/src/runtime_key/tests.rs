use super::*;
use crate::{
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    project_paths::InstallationPaths,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    )
}
fn limits() -> RuntimeKeyLimits {
    RuntimeKeyLimits::new(65536, 256, 65536).unwrap()
}
fn installation(root: &Path, ctx: &SnapshotContext) -> PrivateInstallation {
    let paths = InstallationPaths::resolve(
        Brand::new("graphit").unwrap(),
        root.parent().unwrap(),
        None,
        Some(root),
    )
    .unwrap();
    installation_bootstrap::prepare(&paths, BootstrapLimits::new(65536, 256).unwrap(), ctx)
        .into_installation()
        .unwrap()
}
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private");
    drop(graphit_fs::create_private_directory(&root).unwrap());
    (temp, root)
}
fn start(root: &Path) -> RuntimeKeyOwner {
    let ctx = context();
    let out = RuntimeKeyOwner::start(installation(root, &ctx), limits(), &ctx);
    assert_eq!(out.failure(), None);
    out.into_owner().ok().unwrap()
}
fn key(root: &Path) -> PathBuf {
    root.join("daemon/mcp.key")
}
fn stage(root: &Path) -> PathBuf {
    root.join("daemon/.mcp.key.next")
}
fn private_write(path: &Path, bytes: &[u8]) {
    let mut file = graphit_fs::create_private_file(path).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
fn failure(out: &RuntimeKeyMutationOutcome, error: RuntimeKeyError, effect: RuntimeKeyEffect) {
    assert_eq!(out.failure(), Some(error));
    assert_eq!(out.effects().key, effect);
}

fn realm(root: &Path, brand: &str) -> CapturedPublicationScope {
    let paths = InstallationPaths::resolve(
        Brand::new(brand).unwrap(),
        root.parent().unwrap(),
        None,
        Some(root),
    )
    .unwrap();
    CapturedPublicationScope::installation(
        &paths,
        crate::installation_identity::InstallationId::new("fixture-realm").unwrap(),
        [root.to_owned()],
        crate::artifact_store::ScopeLimits::new(8, 65536, 256).unwrap(),
        &native_capture_request(),
    )
    .unwrap()
}

#[test]
fn installation_binding_checks_real_realm_and_exact_single_content_pass() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let scope = realm(&root, "graphit");
    let ctx = context();
    let borrow = owner.check(&ctx).unwrap();
    assert_eq!(borrow.generation(), 1);
    borrow.recheck_installation(&scope, &ctx).unwrap();

    assert_eq!(
        borrow.recheck_installation(&scope, &context()),
        Err(RuntimeKeyError::Conflict)
    );

    let cancelled = context();
    let borrow = owner.check(&cancelled).unwrap();
    cancelled.cancellation().cancel();
    assert_eq!(
        borrow.recheck_installation(&scope, &cancelled),
        Err(RuntimeKeyError::Cancelled)
    );
    let expired = SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    );
    let borrow = owner.check(&expired).unwrap();
    while Instant::now() < expired.deadline().unwrap() {
        std::thread::yield_now();
    }
    assert_eq!(
        borrow.recheck_installation(&scope, &expired),
        Err(RuntimeKeyError::Deadline)
    );
}

#[test]
fn installation_binding_denies_wrong_brand_root_and_full_realm_retarget() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let wrong_brand = realm(&root, "otherbrand");
    let (_other, other_root) = fixture();
    let wrong_root = realm(&other_root, "graphit");
    for scope in [&wrong_brand, &wrong_root] {
        let ctx = context();
        let borrow = owner.check(&ctx).unwrap();
        assert_eq!(
            borrow.recheck_installation(scope, &ctx),
            Err(RuntimeKeyError::Conflict)
        );
    }
    let runtime = root.join("runtime");
    drop(graphit_fs::create_private_directory(&runtime).unwrap());
    let scope = realm(&root, "graphit");
    let before = fs::metadata(&runtime).unwrap().modified().unwrap();
    fs::rename(&runtime, root.join("old-runtime")).unwrap();
    let replacement = graphit_fs::create_private_directory(&runtime).unwrap();
    #[cfg(windows)]
    let replacement = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
        };
        drop(replacement);
        // Timestamp spoofing is an explicit fixture operation; production
        // directory captures correctly do not request WRITE_ATTRIBUTES.
        fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&runtime)
            .unwrap()
    };
    replacement
        .set_times(std::fs::FileTimes::new().set_modified(before))
        .unwrap();
    assert_eq!(fs::metadata(&runtime).unwrap().modified().unwrap(), before);
    let ctx = context();
    let borrow = owner.check(&ctx).unwrap();
    assert_eq!(
        borrow.recheck_installation(&scope, &ctx),
        Err(RuntimeKeyError::Conflict)
    );

    assert_eq!(fs::read(key(&root)).unwrap().len(), 64);
}

#[cfg(unix)]
#[test]
fn installation_binding_preserves_compared_fd_through_equal_byte_replacement() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let scope = realm(&root, "graphit");
    let ctx = context();
    let borrow = owner.check(&ctx).unwrap();
    let original = fs::read(key(&root)).unwrap();
    let retained = root.join("retained-key");
    let changed_key = key(&root);
    let kept = retained.clone();
    let replacement = original.clone();
    CapturedPublicationScope::test_inspection_step(Box::new(move || {
        fs::rename(&changed_key, &kept).unwrap();
        private_write(&changed_key, &replacement);
    }));
    assert_eq!(
        borrow.recheck_installation(&scope, &ctx),
        Err(RuntimeKeyError::Conflict)
    );

    assert_eq!(fs::read(key(&root)).unwrap(), original);
    assert_eq!(fs::read(&retained).unwrap(), original);
    assert!(
        physical_id(&key(&root), &fs::metadata(key(&root)).unwrap()).unwrap()
            != physical_id(&retained, &fs::metadata(&retained).unwrap()).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn installation_binding_rechecks_same_fd_privacy_after_scope_inspection() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, root) = fixture();
    let owner = start(&root);
    let scope = realm(&root, "graphit");
    let ctx = context();
    let borrow = owner.check(&ctx).unwrap();
    let original = fs::read(key(&root)).unwrap();
    let changed_key = key(&root);
    CapturedPublicationScope::test_inspection_step(Box::new(move || {
        fs::set_permissions(&changed_key, fs::Permissions::from_mode(0o644)).unwrap();
    }));
    assert!(borrow.recheck_installation(&scope, &ctx).is_err());

    assert_eq!(fs::read(key(&root)).unwrap(), original);
    assert_eq!(
        fs::metadata(key(&root)).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[cfg(unix)]
#[test]
fn installation_binding_denies_late_privacy_change_and_symlinked_realm() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let (_temp, root) = fixture();
    let owner = start(&root);
    let runtime = root.join("runtime");
    drop(graphit_fs::create_private_directory(&runtime).unwrap());
    let scope = realm(&root, "graphit");
    let ctx = context();
    let borrow = owner.check(&ctx).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(borrow.recheck_installation(&scope, &ctx).is_err());

    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::rename(&runtime, root.join("old-runtime")).unwrap();
    symlink(root.join("old-runtime"), &runtime).unwrap();
    let ctx = context();
    let borrow = owner.check(&ctx).unwrap();
    assert_eq!(
        borrow.recheck_installation(&scope, &ctx),
        Err(RuntimeKeyError::Conflict)
    );
    assert!(
        fs::symlink_metadata(&runtime)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn private_start_check_maintain_remove_and_terminal_generation() {
    let (_temp, root) = fixture();
    let ctx = context();
    let installed = installation(&root, &ctx);
    assert_eq!(
        installed.revalidate(&context()),
        Err(AuthDocumentError::Conflict)
    );
    let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
    assert_eq!(out.effects().key, RuntimeKeyEffect::Applied);
    assert_eq!(out.effects().namespace.directory, RuntimeKeyEffect::Applied);
    assert_eq!(
        out.effects().namespace.owner_lock,
        RuntimeKeyEffect::Applied
    );
    assert_eq!(out.failure(), None);
    let owner = Arc::new(out.into_owner().ok().unwrap());
    let data = fs::read(key(&root)).unwrap();
    assert_eq!(data.len(), 64);
    assert!(
        data.iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    );

    let checked = context();
    let borrow = owner.check(&checked).unwrap();
    assert_eq!(&data[..], borrow.secret(&checked).unwrap());
    assert_eq!(borrow.recheck(&context()), Err(RuntimeKeyError::Conflict));
    let unchanged = context();
    let out = owner.maintain(&unchanged);
    assert_eq!(out.failure(), None);
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);

    let remove = context();
    let out = owner.remove(&remove);
    assert_eq!(out.failure(), None);
    assert_eq!(out.effects().key, RuntimeKeyEffect::Applied);
    assert_eq!(out.generation(), 2);

    assert!(!key(&root).exists());
    assert!(root.join("daemon/mcp.owner.lock").exists());
    assert_eq!(borrow.recheck(&checked), Err(RuntimeKeyError::Conflict));
    failure(
        &owner.maintain(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
    failure(
        &owner.remove(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
}
#[test]
fn missing_recreation_same_secret_new_receipt_and_stale_borrow() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let original = context();
    let borrow = owner.check(&original).unwrap();
    let body = borrow.secret(&original).unwrap().to_vec();
    fs::remove_file(key(&root)).unwrap();
    assert!(!stage(&root).exists());
    let ctx = context();
    let out = owner.maintain(&ctx);
    assert_eq!(out.failure(), None);
    assert_eq!(out.effects().key, RuntimeKeyEffect::Applied);
    assert_eq!(out.generation(), 2);
    assert_eq!(fs::read(key(&root)).unwrap(), body);

    assert_eq!(borrow.recheck(&original), Err(RuntimeKeyError::Conflict));
    assert!(owner.check(&context()).is_ok());
}
#[test]
fn restart_changes_secret_and_existing_publication_exact_io() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let old = fs::read(key(&root)).unwrap();
    drop(owner);
    assert_eq!(fs::read(key(&root)).unwrap(), old);
    let ctx = context();
    let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
    assert_eq!(out.failure(), None);
    let _owner = out.into_owner().ok().unwrap();
    assert_ne!(fs::read(key(&root)).unwrap(), old);
}

#[test]
fn concurrent_original_requests_recheck_the_same_real_key() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let expected = fs::read(key(&root)).unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        let run = || {
            let ctx = context();
            barrier.wait();
            for _ in 0..8 {
                let borrowed = owner.check(&ctx).unwrap();
                assert_eq!(borrowed.secret(&ctx).unwrap().as_slice(), expected);
                borrowed.recheck(&ctx).unwrap();
            }
        };
        let first = threads.spawn(run);
        let second = threads.spawn(run);
        first.join().unwrap();
        second.join().unwrap();
    });
}

#[test]
fn readonly_key_checks_share_state_and_removal_remains_exclusive() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let ctx = context();
    let borrowed = owner.check(&ctx).unwrap();
    let guard = owner.read_state(&ctx).unwrap();
    assert!(matches!(
        owner.state.try_write(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    std::thread::scope(|threads| {
        let (completed, completion) = std::sync::mpsc::channel();
        let owner = &owner;
        let reader = threads.spawn(move || {
            let result = owner.check(&context()).map(|_| ());
            completed.send(result).unwrap();
        });
        let result = completion.recv_timeout(Duration::from_secs(3));
        // Release before asserting so a regressed exclusive reader cannot hang the test.
        drop(guard);
        assert_eq!(result.unwrap(), Ok(()));
        reader.join().unwrap();
    });
    assert_eq!(owner.remove(&ctx).failure(), None);
    assert_eq!(borrowed.recheck(&ctx), Err(RuntimeKeyError::Conflict));
    assert!(!key(&root).exists());
}

#[test]
fn cancelled_request_cannot_consume_a_contended_owner() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let cancellation = Cancellation::default();
    let ctx = SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        cancellation.clone(),
    );
    let guard = owner.state.write().unwrap();
    std::thread::scope(|threads| {
        let (ready, waiting) = std::sync::mpsc::channel();
        let owner = &owner;
        let ctx = &ctx;
        let checked = threads.spawn(move || {
            ready.send(()).unwrap();
            owner.check(ctx).err()
        });
        waiting.recv().unwrap();
        cancellation.cancel();
        drop(guard);
        assert_eq!(checked.join().unwrap(), Some(RuntimeKeyError::Cancelled));
    });
    assert!(owner.check(&context()).is_ok());
}

#[test]
fn poisoned_owner_refuses_key_access_without_physical_effects() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let expected = fs::read(key(&root)).unwrap();
    std::thread::scope(|threads| {
        assert!(
            threads
                .spawn(|| {
                    let _guard = owner.state.write().unwrap();
                    panic!("test owner state failure");
                })
                .join()
                .is_err()
        );
    });
    assert_eq!(owner.check(&context()).err(), Some(RuntimeKeyError::Io));
    assert_eq!(fs::read(key(&root)).unwrap(), expected);
}

#[test]
fn fresh_start_context_cannot_consume_bootstrap_observation() {
    let (_temp, root) = fixture();
    let original = context();
    let installed = installation(&root, &original);
    let out = RuntimeKeyOwner::start(installed, limits(), &context());
    assert_eq!(out.failure(), Some(RuntimeKeyError::Conflict));
    assert_eq!(out.effects(), RuntimeKeyEffects::default());
    assert!(!root.join("daemon").exists());
}
#[test]
fn owner_lease_contender_has_bounded_deadline_and_no_key_effect() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let old = fs::read(key(&root)).unwrap();
    let ctx = SnapshotContext::new(
        Instant::now() + Duration::from_millis(30),
        Cancellation::default(),
    );
    let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Deadline));
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    assert_eq!(fs::read(key(&root)).unwrap(), old);
    drop(owner);
}
#[test]
fn replaced_key_equal_bytes_and_restored_mtime_is_foreign_and_preserved() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let path = key(&root);
    let old = fs::read(&path).unwrap();
    let time = fs::metadata(&path).unwrap().modified().unwrap();
    fs::rename(&path, root.join("old-key")).unwrap();
    private_write(&path, &old);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(time))
        .unwrap();
    assert_eq!(
        owner.check(&context()).err(),
        Some(RuntimeKeyError::Conflict)
    );
    failure(
        &owner.maintain(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
    failure(
        &owner.remove(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
    assert_eq!(fs::read(&path).unwrap(), old);
}
#[test]
fn changed_body_same_inode_and_restored_mtime_denied() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let path = key(&root);
    let time = fs::metadata(&path).unwrap().modified().unwrap();
    let mut file = File::options().write(true).open(&path).unwrap();
    file.write_all(&[b'a'; 64]).unwrap();
    file.set_times(fs::FileTimes::new().set_modified(time))
        .unwrap();
    // Validate the edited body after the mutation handle has released its sharing contract.
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(
        owner.check(&context()).err(),
        Some(RuntimeKeyError::Conflict)
    );
    failure(
        &owner.remove(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
    assert!(path.exists());
}
#[test]
fn replaced_lease_and_directory_denied_without_repair() {
    for directory in [false, true] {
        let (_temp, root) = fixture();
        let owner = start(&root);
        #[cfg(windows)]
        let before = (
            owner.namespace.directory.identity().clone(),
            crate::native_file_identity::handle_id(owner.namespace.lease.file()).unwrap(),
            fs::read(key(&root)).unwrap(),
            owner.namespace.lease.file().metadata().unwrap().len(),
        );
        let selected = if directory {
            root.join("daemon")
        } else {
            root.join("daemon/mcp.owner.lock")
        };
        let renamed = fs::rename(&selected, root.join("foreign-old"));
        #[cfg(unix)]
        {
            renamed.unwrap();
            if directory {
                drop(graphit_fs::create_private_directory(&selected).unwrap());
                private_write(&key(&root), &[b'a'; 64]);
            } else {
                drop(graphit_fs::create_private_lock_file(&selected).unwrap());
            }
            assert!(owner.check(&context()).is_err());
            assert!(owner.remove(&context()).failure().is_some());
            assert!(key(&root).exists());
        }
        #[cfg(windows)]
        {
            // Both the real lease and its held directory exclude delete sharing.
            assert_eq!(renamed.unwrap_err().raw_os_error(), Some(32));
            assert_eq!(before.3, 0);
            assert_eq!(
                fs::read(owner.namespace.lease.path())
                    .unwrap_err()
                    .raw_os_error(),
                Some(33)
            );
            let current = context();
            owner.namespace.check(&current).unwrap();
            assert!(
                crate::native_file_identity::physical_id(
                    &root.join("daemon"),
                    &fs::symlink_metadata(root.join("daemon")).unwrap(),
                )
                .unwrap()
                    == before.0
            );
            assert!(
                crate::native_file_identity::physical_id(
                    owner.namespace.lease.path(),
                    &fs::symlink_metadata(owner.namespace.lease.path()).unwrap(),
                )
                .unwrap()
                    == before.1
            );
            let checked = owner.check(&current).unwrap();
            assert_eq!(
                checked.secret(&current).unwrap().as_slice(),
                before.2.as_slice()
            );
            assert_eq!(fs::read(key(&root)).unwrap(), before.2);
            assert_eq!(
                fs::metadata(owner.namespace.lease.path()).unwrap().len(),
                before.3
            );
            assert!(!root.join("foreign-old").exists());
            assert!(!stage(&root).exists());
        }
    }
}
#[cfg(unix)]
#[test]
fn public_symlink_and_hardlink_keys_are_refused() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for case in 0..3 {
        let (_temp, root) = fixture();
        let owner = start(&root);
        let path = key(&root);
        match case {
            0 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            1 => {
                fs::rename(&path, root.join("target")).unwrap();
                symlink(root.join("target"), &path).unwrap();
            }
            _ => fs::hard_link(&path, root.join("alias")).unwrap(),
        };
        assert!(owner.check(&context()).is_err());
        assert!(owner.maintain(&context()).failure().is_some());
        assert!(owner.remove(&context()).failure().is_some());
        assert!(path.exists());
    }
}
#[test]
fn wrong_shape_and_oversized_existing_keys_never_adopted() {
    for body in [vec![], vec![b'a'; 63], vec![b'z'; 64], vec![b'a'; 65]] {
        let (_temp, root) = fixture();
        drop(graphit_fs::create_private_directory(&root.join("daemon")).unwrap());
        private_write(&key(&root), &body);
        let ctx = context();
        let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
        assert!(out.failure().is_some());
        assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
        assert_eq!(fs::read(key(&root)).unwrap(), body);
    }
}
#[test]
fn partial_stage_explicit_discard_owns_lease_and_ready_fresh_context() {
    for n in [0, 1, 31, 64] {
        let (_temp, root) = fixture();
        drop(graphit_fs::create_private_directory(&root.join("daemon")).unwrap());
        private_write(&stage(&root), &vec![0xff; n]);
        let ctx = context();
        let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
        assert_eq!(out.failure(), Some(RuntimeKeyError::Conflict));

        let recovery = out.into_recovery().unwrap();
        assert!(FileLock::is_locked(root.join("daemon/mcp.owner.lock")).unwrap());
        let fresh = context();
        let discarded = recovery.discard(&fresh);
        assert_eq!(discarded.failure(), None);
        assert_eq!(discarded.effects().key, RuntimeKeyEffect::NotApplied);
        assert_eq!(
            discarded.effects().namespace.discard,
            RuntimeKeyEffect::Applied
        );

        let ready = discarded.into_ready().ok().unwrap();
        let fresh = context();
        let out = ready.start(&fresh);
        assert_eq!(out.failure(), None);
        let _owner = out.into_owner().ok().unwrap();
        assert_eq!(fs::read(key(&root)).unwrap().len(), 64);
        assert!(!stage(&root).exists());
    }
}
#[test]
fn foreign_replaced_or_oversized_stage_preserved() {
    let (_temp, root) = fixture();
    drop(graphit_fs::create_private_directory(&root.join("daemon")).unwrap());
    private_write(&stage(&root), b"partial");
    let ctx = context();
    let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
    let recovery = out.into_recovery().unwrap();
    fs::rename(stage(&root), root.join("oldstage")).unwrap();
    private_write(&stage(&root), b"partial");
    let out = recovery.discard(&context());
    assert_eq!(out.failure(), Some(RuntimeKeyError::Conflict));
    assert_eq!(
        out.effects().namespace.discard,
        RuntimeKeyEffect::NotApplied
    );
    assert_eq!(fs::read(stage(&root)).unwrap(), b"partial");
    drop(out);
    fs::remove_file(stage(&root)).unwrap();
    private_write(&stage(&root), &[1; 65]);
    let ctx = context();
    let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Limit));
    assert!(out.into_recovery().is_none());
    assert_eq!(fs::read(stage(&root)).unwrap(), [1; 65]);
}
#[test]
fn entropy_failure_and_cancelled_or_deadline_before_birth_expose_no_key() {
    let (_temp, root) = fixture();
    let ctx = context();
    ENTROPY_FAIL.with(|v| v.set(true));
    let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Io));
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    assert!(!key(&root).exists());
    assert!(!stage(&root).exists());
    let original = context();
    let installed = installation(&root, &original);
    original.cancellation().cancel();
    let out = RuntimeKeyOwner::start(installed, limits(), &original);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Cancelled));
    assert_eq!(out.effects(), RuntimeKeyEffects::default());
    let original = context();
    let installed = installation(&root, &original);
    std::thread::sleep(Duration::from_millis(1));
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    let out = RuntimeKeyOwner::start(installed, limits(), &expired);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Deadline));
    assert_eq!(out.effects(), RuntimeKeyEffects::default());
}
#[test]
fn native_replace_errors_classified_and_success_observer_cancel_keeps_applied() {
    for case in 0..4 {
        let (_temp, root) = fixture();
        let ctx = context();
        let installed = installation(&root, &ctx);
        if case < 2 {
            REPLACE_FAULT.with(|v| *v.borrow_mut() = Some(case == 1));
        }
        if case >= 2 {
            let cancel = ctx.cancellation().clone();
            HOOK.with(|v| {
                *v.borrow_mut() = Some(Box::new(move |phase| {
                    if phase == "after_replace" {
                        cancel.cancel();
                    }
                }))
            });
            if case == 3 {
                REPLACE_FAULT.with(|v| *v.borrow_mut() = Some(false));
            }
        }
        let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
        HOOK.with(|v| *v.borrow_mut() = None);
        assert!(out.failure().is_some());
        let expected = match case {
            0 => RuntimeKeyEffect::NotApplied,
            1 | 2 => RuntimeKeyEffect::Applied,
            _ => RuntimeKeyEffect::Unknown,
        };
        assert_eq!(out.effects().key, expected);
        assert!(out.into_owner().is_err());
    }
}
#[test]
fn remove_success_late_cancel_is_applied_and_terminal_unknown_never_recreates() {
    for unknown in [false, true] {
        let (_temp, root) = fixture();
        let owner = start(&root);
        let ctx = context();
        let cancel = ctx.cancellation().clone();
        HOOK.with(|v| {
            *v.borrow_mut() = Some(Box::new(move |phase| {
                if phase == "after_remove" {
                    cancel.cancel();
                }
            }))
        });
        if unknown {
            REMOVE_FAULT.with(|v| *v.borrow_mut() = Some(true));
        }
        let out = owner.remove(&ctx);
        HOOK.with(|v| *v.borrow_mut() = None);
        failure(
            &out,
            if unknown {
                RuntimeKeyError::Io
            } else {
                RuntimeKeyError::Cancelled
            },
            if unknown {
                RuntimeKeyEffect::Unknown
            } else {
                RuntimeKeyEffect::Applied
            },
        );
        assert!(!key(&root).exists());
        failure(
            &owner.maintain(&context()),
            RuntimeKeyError::Conflict,
            RuntimeKeyEffect::NotApplied,
        );
        assert!(owner.check(&context()).is_err());
    }
}
#[test]
fn recovery_success_late_cancel_applied_without_ready() {
    let (_temp, root) = fixture();
    drop(graphit_fs::create_private_directory(&root.join("daemon")).unwrap());
    private_write(&stage(&root), b"partial");
    let ctx = context();
    let recovery = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx)
        .into_recovery()
        .unwrap();
    let ctx = context();
    let cancel = ctx.cancellation().clone();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |p| {
            if p == "after_discard" {
                cancel.cancel();
            }
        }))
    });
    let out = recovery.discard(&ctx);
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(out.effects().namespace.discard, RuntimeKeyEffect::Applied);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Cancelled));
    assert!(out.into_ready().is_err());
    assert!(!stage(&root).exists());
}
#[test]
fn positive_limits_and_overflow_reject_invalid_input() {
    for args in [
        (0, 1, 12),
        (1, 0, 12),
        (1, 1, 11),
        (usize::MAX, 1, 12),
        (1, usize::MAX, 12),
        (1, 1, usize::MAX),
    ] {
        assert!(RuntimeKeyLimits::new(args.0, args.1, args.2).is_err());
    }
}

#[test]
fn failed_start_cleanup_keeps_origin_lease_then_explicit_discard_ready() {
    let (_temp, root) = fixture();
    let ctx = context();
    let installed = installation(&root, &ctx);
    let cancel = ctx.cancellation().clone();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "stage_created" {
                cancel.cancel();
            }
        }))
    });
    let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Cancelled));
    assert_eq!(out.cleanup_failure(), Some(RuntimeKeyError::Cancelled));
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    assert_eq!(out.effects().namespace.stage, RuntimeKeyEffect::Applied);
    assert_eq!(fs::metadata(stage(&root)).unwrap().len(), 0);
    assert!(FileLock::is_locked(root.join("daemon/mcp.owner.lock")).unwrap());
    let recovery = out.into_recovery().unwrap();
    let fresh = context();
    let out = recovery.discard(&fresh);
    assert_eq!(out.failure(), None);
    assert_eq!(out.effects().namespace.discard, RuntimeKeyEffect::Applied);

    let ready = out.into_ready().ok().unwrap();
    let out = ready.start(&context());
    assert_eq!(out.failure(), None);
    let _owner = out.into_owner().ok().unwrap();
}
#[test]
fn pending_start_origin_foreign_replacement_preserved_and_absent_requalified() {
    let (_temp, root) = fixture();
    let ctx = context();
    let installed = installation(&root, &ctx);
    let cancel = ctx.cancellation().clone();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "written" {
                cancel.cancel();
            }
        }))
    });
    let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(fs::metadata(stage(&root)).unwrap().len(), 64);
    let recovery = out.into_recovery().unwrap();
    fs::rename(stage(&root), root.join("oldstage")).unwrap();
    private_write(&stage(&root), b"foreign");
    let out = recovery.discard(&context());
    assert_eq!(out.failure(), Some(RuntimeKeyError::Conflict));
    assert_eq!(
        out.effects().namespace.discard,
        RuntimeKeyEffect::NotApplied
    );
    assert_eq!(fs::read(stage(&root)).unwrap(), b"foreign");
    let recovery = out.into_recovery().unwrap();
    fs::remove_file(stage(&root)).unwrap();
    let out = recovery.discard(&context());
    assert_eq!(out.failure(), None);
    assert_eq!(
        out.effects().namespace.discard,
        RuntimeKeyEffect::NotApplied
    );
    let _ready = out.into_ready().ok().unwrap();
}
#[test]
fn failed_applied_start_has_no_owner_but_keeps_absent_origin_after_cancel() {
    let (_temp, root) = fixture();
    let ctx = context();
    let installed = installation(&root, &ctx);
    let cancel = ctx.cancellation().clone();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "after_replace" {
                cancel.cancel();
            }
        }))
    });
    let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(out.effects().key, RuntimeKeyEffect::Applied);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Cancelled));
    assert!(!stage(&root).exists());
    assert!(key(&root).exists());
    let recovery = out.into_recovery().unwrap();
    let out = recovery.discard(&context());
    assert_eq!(out.failure(), None);
    assert_eq!(
        out.effects().namespace.discard,
        RuntimeKeyEffect::NotApplied
    );
    let out = out.into_ready().ok().unwrap().start(&context());
    assert_eq!(out.failure(), None);
    let _owner = out.into_owner().ok().unwrap();
}
#[test]
fn maintain_pending_origin_explicit_cleanup_and_terminal_state_do_not_resurrect() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    fs::remove_file(key(&root)).unwrap();
    let ctx = context();
    let cancel = ctx.cancellation().clone();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "stage_created" {
                cancel.cancel();
            }
        }))
    });
    let out = owner.maintain(&ctx);
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(out.cleanup_failure(), Some(RuntimeKeyError::Cancelled));
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    failure(
        &owner.maintain(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
    let fresh = context();
    let out = owner.discard_pending(&fresh);
    assert_eq!(out.failure(), None);
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    assert_eq!(out.effects().namespace.discard, RuntimeKeyEffect::Applied);

    assert_eq!(owner.maintain(&context()).failure(), None);
    assert_eq!(owner.remove(&context()).failure(), None);
    assert_eq!(owner.discard_pending(&context()).failure(), None);
    failure(
        &owner.maintain(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
}
#[test]
fn pending_maintenance_unknown_after_replace_can_cleanup_but_never_reactivate() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    fs::remove_file(key(&root)).unwrap();
    let ctx = context();
    let cancel = ctx.cancellation().clone();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "after_replace" {
                cancel.cancel();
            }
        }))
    });
    REPLACE_FAULT.with(|v| *v.borrow_mut() = Some(true));
    let out = owner.maintain(&ctx);
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(out.effects().key, RuntimeKeyEffect::Unknown);
    assert!(owner.check(&context()).is_err());
    let out = owner.discard_pending(&context());
    assert_eq!(out.failure(), None);
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    failure(
        &owner.maintain(&context()),
        RuntimeKeyError::Conflict,
        RuntimeKeyEffect::NotApplied,
    );
}

#[test]
fn partial_actual_write_counters_origin_recovery_never_adopts_bytes() {
    let (_temp, root) = fixture();
    let ctx = context();
    let installed = installation(&root, &ctx);
    let cancel = ctx.cancellation().clone();
    WRITE_MAX.with(|v| v.set(32));
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "written" {
                cancel.cancel();
            }
        }))
    });
    let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
    WRITE_MAX.with(|v| v.set(usize::MAX));
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Cancelled));
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    assert_eq!(fs::metadata(stage(&root)).unwrap().len(), 32);

    let out = out.into_recovery().unwrap().discard(&context());
    assert_eq!(out.failure(), None);
    let out = out.into_ready().ok().unwrap().start(&context());
    assert_eq!(out.failure(), None);
    let _owner = out.into_owner().ok().unwrap();
    assert_eq!(fs::read(key(&root)).unwrap().len(), 64);
}
#[cfg(unix)]
#[test]
fn public_linked_stage_lock_and_directory_are_preserved() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for case in 0..3 {
        let (_temp, root) = fixture();
        drop(graphit_fs::create_private_directory(&root.join("daemon")).unwrap());
        let path = stage(&root);
        private_write(&path, b"partial");
        match case {
            0 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            1 => {
                fs::rename(&path, root.join("target")).unwrap();
                symlink(root.join("target"), &path).unwrap();
            }
            _ => fs::hard_link(&path, root.join("alias")).unwrap(),
        };
        let ctx = context();
        let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
        assert!(out.failure().is_some());
        assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
        assert!(out.into_recovery().is_none());
        assert_eq!(fs::read(&path).unwrap(), b"partial");
        assert!(!key(&root).exists());
    }
    for directory in [false, true] {
        let (_temp, root) = fixture();
        let owner = start(&root);
        let body = fs::read(key(&root)).unwrap();
        let path = if directory {
            root.join("daemon")
        } else {
            root.join("daemon/mcp.owner.lock")
        };
        fs::set_permissions(
            &path,
            fs::Permissions::from_mode(if directory { 0o755 } else { 0o644 }),
        )
        .unwrap();
        assert!(owner.check(&context()).is_err());
        assert!(owner.maintain(&context()).failure().is_some());
        assert!(owner.remove(&context()).failure().is_some());
        assert_eq!(fs::read(key(&root)).unwrap(), body);
    }
}
#[test]
fn path_and_ancestor_limits_and_late_namespace_cancel() {
    for policy in [
        RuntimeKeyLimits::new(1, 256, 65536).unwrap(),
        RuntimeKeyLimits::new(65536, 1, 65536).unwrap(),
    ] {
        let (_temp, root) = fixture();
        let ctx = context();
        let out = RuntimeKeyOwner::start(installation(&root, &ctx), policy, &ctx);
        assert_eq!(out.failure(), Some(RuntimeKeyError::Limit));
        assert_eq!(out.effects(), RuntimeKeyEffects::default());
        assert!(!root.join("daemon").exists());
    }
    let (_temp, root) = fixture();
    let ctx = context();
    let installed = installation(&root, &ctx);
    let cancel = ctx.cancellation().clone();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "directory_created" {
                cancel.cancel();
            }
        }))
    });
    let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
    HOOK.with(|v| *v.borrow_mut() = None);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Cancelled));
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    assert_eq!(out.effects().namespace.directory, RuntimeKeyEffect::Applied);
    assert_eq!(
        out.effects().namespace.owner_lock,
        RuntimeKeyEffect::NotApplied
    );
    assert!(root.join("daemon").is_dir());
    assert!(!key(&root).exists());
}
#[test]
fn directory_sync_errors_preserve_applied_key_and_cleanup_effects() {
    for after in [false, true] {
        let (_temp, root) = fixture();
        let ctx = context();
        let installed = installation(&root, &ctx);
        SYNC_FAULT.with(|v| *v.borrow_mut() = Some(after));
        let out = RuntimeKeyOwner::start(installed, limits(), &ctx);
        assert_eq!(out.effects().key, RuntimeKeyEffect::Applied);
        assert_eq!(out.failure(), Some(RuntimeKeyError::Io));
        assert!(out.into_owner().is_err());
        assert!(key(&root).exists());
        let owner = start(&root);
        SYNC_FAULT.with(|v| *v.borrow_mut() = Some(after));
        let out = owner.remove(&context());
        failure(&out, RuntimeKeyError::Io, RuntimeKeyEffect::Applied);
        failure(
            &owner.maintain(&context()),
            RuntimeKeyError::Conflict,
            RuntimeKeyEffect::NotApplied,
        );
        drop(owner);
        private_write(&stage(&root), b"partial");
        let ctx = context();
        let recovery = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx)
            .into_recovery()
            .unwrap();
        SYNC_FAULT.with(|v| *v.borrow_mut() = Some(after));
        let out = recovery.discard(&context());
        assert_eq!(out.effects().namespace.discard, RuntimeKeyEffect::Applied);
        assert_eq!(out.failure(), Some(RuntimeKeyError::Io));
        assert!(!stage(&root).exists());
        let recovery = out.into_recovery().unwrap();
        assert_eq!(recovery.discard(&context()).failure(), None);
    }
}

#[test]
#[ignore = "executed by different_process_cannot_take_live_owner_lease"]
fn lease_contender_child() {
    let root = PathBuf::from(std::env::var_os("GRAPHIT_RUNTIME_KEY_FIXTURE_ROOT").unwrap());
    let ctx = SnapshotContext::new(
        Instant::now() + Duration::from_millis(50),
        Cancellation::default(),
    );
    let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
    assert_eq!(out.failure(), Some(RuntimeKeyError::Deadline));
    assert_eq!(out.effects().key, RuntimeKeyEffect::NotApplied);
    std::process::exit(38);
}
#[cfg(unix)]
#[test]
fn different_process_cannot_take_live_owner_lease() {
    let (_temp, root) = fixture();
    let owner = start(&root);
    let body = fs::read(key(&root)).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime_key::tests::lease_contender_child",
            "--ignored",
            "--nocapture",
        ])
        .env("GRAPHIT_RUNTIME_KEY_FIXTURE_ROOT", &root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("bounded lease child timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(status.code(), Some(38));
    assert_eq!(fs::read(key(&root)).unwrap(), body);
    assert!(owner.check(&context()).is_ok());
}

#[test]
#[ignore = "executed with a real process exit by bounded_crash_and_lease_process_journeys"]
fn crash_child() {
    let root = PathBuf::from(std::env::var_os("GRAPHIT_RUNTIME_KEY_FIXTURE_ROOT").unwrap());
    let target = std::env::var("GRAPHIT_RUNTIME_KEY_FIXTURE_PHASE").unwrap();
    HOOK.with(|v| {
        *v.borrow_mut() = Some(Box::new(move |phase| {
            if phase == target {
                std::process::exit(37);
            }
        }))
    });
    let _owner = start(&root);
    panic!("crash phase not reached");
}
#[cfg(unix)]
#[test]
fn bounded_crash_and_lease_process_journeys() {
    for phase in ["stage_created", "written", "stage_synced", "after_replace"] {
        let (_temp, root) = fixture();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime_key::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("GRAPHIT_RUNTIME_KEY_FIXTURE_ROOT", &root)
            .env("GRAPHIT_RUNTIME_KEY_FIXTURE_PHASE", phase)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("bounded child timed out");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.code(), Some(37));
        let ctx = context();
        let out = RuntimeKeyOwner::start(installation(&root, &ctx), limits(), &ctx);
        if phase == "after_replace" {
            assert_eq!(out.failure(), None);
            let owner = out.into_owner().ok().unwrap();
            assert!(owner.check(&context()).is_ok());
        } else {
            assert_eq!(out.failure(), Some(RuntimeKeyError::Conflict));
            let discard = out.into_recovery().unwrap().discard(&context());
            let out = discard.into_ready().ok().unwrap().start(&context());
            assert_eq!(out.failure(), None);
            let owner = out.into_owner().ok().unwrap();
            assert!(owner.check(&context()).is_ok());
        }
        assert!(!stage(&root).exists());
        assert_eq!(fs::read(key(&root)).unwrap().len(), 64);
    }
}

fn native_capture_request() -> std::sync::Arc<crate::file_io::SnapshotContext> {
    std::sync::Arc::new(crate::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        crate::file_lock::Cancellation::default(),
    ))
}
