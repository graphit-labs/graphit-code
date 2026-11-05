use super::*;
use crate::{
    artifact_store::ScopeLimits, brand::Brand, file_lock::Cancellation,
    installation_identity::InstallationId, project_paths::InstallationPaths,
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    installation: InstallationPaths,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("installation");
        drop(graphit_fs::create_private_directory(&root).unwrap());
        let installation = InstallationPaths::resolve(
            Brand::new("graphit").unwrap(),
            temporary.path(),
            None,
            Some(&root),
        )
        .unwrap();
        Self {
            _temporary: temporary,
            root,
            installation,
        }
    }
    fn scope(&self, context: &std::sync::Arc<SnapshotContext>) -> CapturedPublicationScope {
        self.scope_roots(context, [self.root.clone()])
    }
    fn scope_roots(
        &self,
        context: &std::sync::Arc<SnapshotContext>,
        roots: impl IntoIterator<Item = PathBuf>,
    ) -> CapturedPublicationScope {
        CapturedPublicationScope::installation(
            &self.installation,
            InstallationId::new("auth-test-installation").unwrap(),
            roots,
            ScopeLimits::new(8, 65536, 256).unwrap(),
            &native_capture_request(std::sync::Arc::clone(context)),
        )
        .unwrap()
    }
    fn store(&self, context: &std::sync::Arc<SnapshotContext>, bytes: usize) -> AuthDocumentStore {
        AuthDocumentStore::capture(self.scope(context), limits(bytes), context).unwrap()
    }
    fn write(&self, body: &[u8]) {
        let mut file = graphit_fs::create_private_file(&self.root.join("auth.json")).unwrap();
        file.write_all(body).unwrap();
        file.sync_all().unwrap();
    }
}
fn context() -> std::sync::Arc<SnapshotContext> {
    std::sync::Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
fn limits(bytes: usize) -> AuthDocumentLimits {
    AuthDocumentLimits::new(bytes, 65536, 256, 65536).unwrap()
}
fn err<T>(result: Result<T, AuthDocumentError>) -> AuthDocumentError {
    result.err().expect("request must fail")
}

fn selected_scope(
    paths: &InstallationPaths,
    id: &str,
    context: &std::sync::Arc<SnapshotContext>,
) -> CapturedPublicationScope {
    CapturedPublicationScope::installation(
        paths,
        InstallationId::new(id).unwrap(),
        [paths.root().to_owned()],
        ScopeLimits::new(8, 65536, 256).unwrap(),
        &native_capture_request(std::sync::Arc::clone(context)),
    )
    .unwrap()
}
fn during_candidate(action: impl FnOnce() + 'static) {
    BEFORE_BINDING_CANDIDATE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            CapturedPublicationScope::test_inspection_step(Box::new(action))
        }))
    });
}
fn occupy_scope(
    scope: CapturedPublicationScope,
) -> (std::sync::mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        CapturedPublicationScope::test_inspection_step(Box::new(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }));
        scope
            .revalidate_bounded(&scope_context(&context()))
            .unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    (release_tx, handle)
}
fn after_scope_wait<T: Send>(
    scope: CapturedPublicationScope,
    action: impl FnOnce() -> T + Send,
) -> T {
    let (release, holder) = occupy_scope(scope);
    let result = std::thread::scope(|threads| {
        let (entered, waiting) = std::sync::mpsc::channel();
        let worker = threads.spawn(move || {
            CapturedPublicationScope::test_before_lock(Box::new(move || entered.send(()).unwrap()));
            action()
        });
        waiting.recv().unwrap();
        release.send(()).unwrap();
        worker.join().unwrap()
    });
    holder.join().unwrap();
    result
}

#[test]
fn existing_read_lock_preserves_absence_and_requires_publisher_integrity() {
    let fixture = Fixture::new();
    let original = context();
    let store = fixture.store(&original, 1024);
    let path = fixture.root.join(publication::LOCK);
    let (guard, absent) = store.read_locked(&original).unwrap();
    assert!(guard.is_none());
    assert!(absent.bytes().is_none());
    assert!(!path.exists());
    assert!(!fixture.root.join("auth.json").exists());
    let publication = store.publish(store.read(&original).unwrap(), b"{}", &original);
    assert_eq!(
        publication.effect(),
        publication::AuthPublicationEffect::Applied
    );
    assert_eq!(publication.failure(), None);
    let (guard, present) = store.read_locked(&original).unwrap();
    let guard = guard.unwrap();
    assert_eq!(present.bytes(), Some(b"{}".as_slice()));
    let (other_guard, other) = store.read_locked(&original).unwrap();
    assert_eq!(other.bytes(), present.bytes());
    let other_guard = other_guard.unwrap();
    other_guard.revalidate().unwrap();
    assert!(crate::file_lock::FileLock::is_locked(&path).unwrap());
    guard.revalidate().unwrap();
    drop(guard);
    assert!(crate::file_lock::FileLock::is_locked(&path).unwrap());
    drop(other_guard);
    assert!(!crate::file_lock::FileLock::is_locked(&path).unwrap());
    fs::remove_file(&path).unwrap();
    assert_eq!(
        err(store.read_locked(&original)),
        AuthDocumentError::Conflict
    );
    assert!(!path.exists());
    let mut malformed = graphit_fs::create_private_file(&path).unwrap();
    malformed.write_all(b"invalid lock content").unwrap();
    drop(malformed);
    assert_eq!(
        err(store.read_locked(&original)),
        AuthDocumentError::Conflict
    );
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"{}");
}
fn poison_scope(scope: CapturedPublicationScope) {
    assert!(
        std::thread::spawn(move || {
            CapturedPublicationScope::test_inspection_step(Box::new(|| {
                panic!("private scope poison fixture")
            }));
            let _ = scope.revalidate_bounded(&scope_context(&context()));
        })
        .join()
        .is_err()
    );
}

#[test]
fn installation_binding_real_bootstrap_independent_capture_and_metadata_context() {
    let fixture = Fixture::new();
    fixture.write(b"data");
    let original = context();
    let prepared = crate::installation_bootstrap::prepare(
        &fixture.installation,
        crate::installation_bootstrap::BootstrapLimits::new(65536, 256).unwrap(),
        &original,
    )
    .into_installation()
    .unwrap();
    prepared.revalidate(&original).unwrap();
    let store = fixture.store(&original, 4);
    let candidate = fixture.scope(&original);
    let metadata = context();
    store.recheck_installation(&candidate, &metadata).unwrap();

    let mut snapshot = store.read(&original).unwrap();
    assert_eq!(
        err(store.revalidate(&mut snapshot, &metadata)),
        AuthDocumentError::Conflict
    );

    store.recheck_installation(&candidate, &original).unwrap();

    store.revalidate(&mut snapshot, &original).unwrap();

    assert_eq!(snapshot.bytes(), Some(b"data".as_slice()));
    assert!(!fixture.root.join(".auth.json.next").exists());
}

#[test]
fn installation_binding_denies_different_captured_owner_id_brand_root_and_project() {
    let fixture = Fixture::new();
    fixture.write(b"data");
    let metadata = context();
    let store = fixture.store(&metadata, 4);
    let wrong_id = selected_scope(&fixture.installation, "other-id", &metadata);
    let other_brand = InstallationPaths::resolve(
        Brand::new("otherbrand").unwrap(),
        fixture._temporary.path(),
        None,
        Some(&fixture.root),
    )
    .unwrap();
    let wrong_brand = selected_scope(&other_brand, "auth-test-installation", &metadata);
    let other = Fixture::new();
    let wrong_root = other.scope(&metadata);
    let project_root = fixture._temporary.path().join("project");
    drop(graphit_fs::create_private_directory(&project_root).unwrap());
    drop(graphit_fs::create_private_directory(&project_root.join(".graphit")).unwrap());
    let project =
        crate::project_paths::ProjectPaths::at(&project_root, &fixture.installation).unwrap();
    let project_scope = CapturedPublicationScope::project(
        &project,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        ScopeLimits::new(8, 65536, 256).unwrap(),
        &native_capture_request(std::sync::Arc::clone(&metadata)),
    )
    .unwrap();
    for candidate in [&wrong_id, &wrong_brand, &wrong_root, &project_scope] {
        assert_eq!(
            err(store.recheck_installation(candidate, &metadata)),
            AuthDocumentError::Conflict
        );
    }
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"data");
}

#[cfg(unix)]
#[test]
fn installation_binding_denies_parent_replacement_during_candidate_inspection() {
    let fixture = Fixture::new();
    fixture.write(b"original");
    let metadata = context();
    let store = fixture.store(&metadata, 8);
    let candidate = fixture.scope(&metadata);
    let root = fixture.root.clone();
    let kept = fixture._temporary.path().join("kept-root");
    let retained = kept.clone();
    let modified = fs::metadata(&root).unwrap().modified().unwrap();
    during_candidate(move || {
        fs::rename(&root, &kept).unwrap();
        let handle = graphit_fs::create_private_directory(&root).unwrap();
        let mut foreign = graphit_fs::create_private_file(&root.join("auth.json")).unwrap();
        foreign.write_all(b"foreign!").unwrap();
        foreign.sync_all().unwrap();
        handle
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    });
    assert_eq!(
        err(store.recheck_installation(&candidate, &metadata)),
        AuthDocumentError::Conflict
    );

    assert_eq!(
        fs::metadata(&fixture.root).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(fs::read(retained.join("auth.json")).unwrap(), b"original");
    assert_eq!(
        fs::read(fixture.root.join("auth.json")).unwrap(),
        b"foreign!"
    );
}

#[cfg(unix)]
#[test]
fn installation_binding_denies_parent_privacy_and_symlink_during_inspection() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for replace in [false, true] {
        let fixture = Fixture::new();
        fixture.write(b"data");
        let metadata = context();
        let store = fixture.store(&metadata, 4);
        let candidate = fixture.scope(&metadata);
        let root = fixture.root.clone();
        let kept = fixture._temporary.path().join("kept-root");
        let retained = kept.clone();
        during_candidate(move || {
            if replace {
                fs::rename(&root, &kept).unwrap();
                symlink(&kept, &root).unwrap();
            } else {
                fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
            }
        });
        assert!(store.recheck_installation(&candidate, &metadata).is_err());

        if replace {
            assert_eq!(fs::read(retained.join("auth.json")).unwrap(), b"data");
            assert!(
                fs::symlink_metadata(&fixture.root)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        } else {
            assert_eq!(
                fs::metadata(&fixture.root).unwrap().permissions().mode() & 0o777,
                0o755
            );
            assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"data");
        }
    }
}

#[test]
fn installation_binding_cancel_and_deadline_during_candidate_inspection_have_zero_io() {
    for deadline in [false, true] {
        let fixture = Fixture::new();
        fixture.write(b"data");
        let metadata = std::sync::Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(2),
            Cancellation::default(),
        ));
        let store = fixture.store(&metadata, 4);
        let candidate = fixture.scope(&metadata);
        let cancellation = metadata.cancellation().clone();
        let end = metadata.deadline().unwrap();
        during_candidate(move || {
            if deadline {
                while Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(1));
                }
            } else {
                cancellation.cancel();
            }
        });
        assert_eq!(
            err(store.recheck_installation(&candidate, &metadata)),
            if deadline {
                AuthDocumentError::Deadline
            } else {
                AuthDocumentError::Cancelled
            }
        );

        assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"data");
    }
}

#[test]
fn installation_capture_and_store_wait_for_scope_and_deny_poison() {
    let fixture = Fixture::new();
    fixture.write(b"data");
    let metadata = context();
    let scope = fixture.scope(&metadata);
    let store = AuthDocumentStore::capture(scope.clone(), limits(4), &metadata).unwrap();
    let candidate = fixture.scope(&metadata);
    after_scope_wait(scope.clone(), || {
        AuthDocumentStore::capture(scope.clone(), limits(4), &metadata)
    })
    .unwrap();
    after_scope_wait(scope.clone(), || {
        store.recheck_installation(&candidate, &metadata)
    })
    .unwrap();
    let snapshot = after_scope_wait(scope.clone(), || store.read(&metadata)).unwrap();
    assert_eq!(snapshot.bytes(), Some(b"data".as_slice()));
    poison_scope(scope.clone());
    assert_eq!(
        err(AuthDocumentStore::capture(scope, limits(4), &metadata)),
        AuthDocumentError::Io
    );
    assert_eq!(
        err(store.recheck_installation(&candidate, &metadata)),
        AuthDocumentError::Io
    );
    assert_eq!(err(store.read(&metadata)), AuthDocumentError::Io);
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"data");
}

#[test]
fn installation_binding_candidate_scope_waits_and_denies_poison() {
    let fixture = Fixture::new();
    fixture.write(b"data");
    let metadata = context();
    let store = fixture.store(&metadata, 4);
    let candidate = fixture.scope(&metadata);
    after_scope_wait(candidate.clone(), || {
        store.recheck_installation(&candidate, &metadata)
    })
    .unwrap();
    poison_scope(candidate.clone());
    assert_eq!(
        err(store.recheck_installation(&candidate, &metadata)),
        AuthDocumentError::Io
    );
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"data");
}

#[cfg(unix)]
#[test]
fn installation_binding_preserves_exact_native_path_and_directory_input_limit() {
    let fixture = Fixture::new();
    fixture.write(b"data");
    let metadata = context();
    let directories = fixture
        .scope(&metadata)
        .binding()
        .unwrap()
        .directory_count();
    let entry_depth_bound = directories.max(fixture.root.join("runtime").components().count());
    let capture = |cap| {
        CapturedPublicationScope::installation(
            &fixture.installation,
            InstallationId::new("auth-test-installation").unwrap(),
            [fixture.root.clone()],
            ScopeLimits::new(1, 65536, cap).unwrap(),
            &native_capture_request(std::sync::Arc::clone(&metadata)),
        )
    };
    let candidate = capture(entry_depth_bound).unwrap();
    assert_eq!(
        capture(entry_depth_bound - 1).err().unwrap().kind(),
        io::ErrorKind::FileTooLarge
    );
    let native_path = fixture
        .root
        .join("auth.json")
        .as_os_str()
        .as_encoded_bytes()
        .len();
    let store = AuthDocumentStore::capture(
        fixture.scope(&metadata),
        AuthDocumentLimits::new(4, native_path, 256, 65536).unwrap(),
        &metadata,
    )
    .unwrap();
    store.recheck_installation(&candidate, &metadata).unwrap();
    assert_eq!(
        err(AuthDocumentStore::capture(
            fixture.scope(&metadata),
            AuthDocumentLimits::new(4, native_path - 1, 256, 65536).unwrap(),
            &metadata
        )),
        AuthDocumentError::Limit
    );
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"data");
}

#[test]
fn absence_creates_nothing_and_revalidation_detects_first_publication() {
    let fixture = Fixture::new();
    let context = context();
    let store = fixture.store(&context, 8);
    let mut snapshot = store.read(&context).unwrap();
    assert!(snapshot.bytes().is_none());
    store.revalidate(&mut snapshot, &context).unwrap();
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);

    fixture.write(b"new");
    assert_eq!(
        err(store.revalidate(&mut snapshot, &context)),
        AuthDocumentError::Conflict
    );
}

#[test]
fn exact_byte_limit_precedes_content_read() {
    let fixture = Fixture::new();
    fixture.write(b"12345678");
    let exact = context();
    let store = fixture.store(&exact, 8);
    let snapshot = store.read(&exact).unwrap();
    assert_eq!(snapshot.bytes(), Some(b"12345678".as_slice()));

    let short = context();
    let store = fixture.store(&short, 7);
    assert_eq!(err(store.read(&short)), AuthDocumentError::Limit);
}

#[test]
fn original_snapshot_cannot_be_rebound_to_another_store_or_fresh_request() {
    let fixture = Fixture::new();
    fixture.write(b"original");
    let original = context();
    let store = fixture.store(&original, 8);
    let mut snapshot = store.read(&original).unwrap();
    let fresh = context();
    assert_eq!(
        err(store.revalidate(&mut snapshot, &fresh)),
        AuthDocumentError::Conflict
    );

    let other = fixture.store(&original, 8);
    assert_eq!(
        err(other.revalidate(&mut snapshot, &original)),
        AuthDocumentError::Conflict
    );

    store.revalidate(&mut snapshot, &original).unwrap();
}

#[test]
fn same_size_mutation_with_restored_mtime_is_not_accepted_as_same_content() {
    let fixture = Fixture::new();
    fixture.write(b"old");
    let context = context();
    let store = fixture.store(&context, 3);
    let mut snapshot = store.read(&context).unwrap();
    let path = fixture.root.join("auth.json");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let mut changed = fs::OpenOptions::new().write(true).open(&path).unwrap();
    changed.write_all(b"new").unwrap();
    changed.sync_all().unwrap();
    changed
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(
        err(store.revalidate(&mut snapshot, &context)),
        AuthDocumentError::Conflict
    );
    assert_eq!(snapshot.bytes(), Some(b"old".as_slice()));
}

#[test]
fn atomic_publication_preserves_retained_readers_and_rejects_stale_state() {
    let fixture = Fixture::new();
    fixture.write(b"old");
    let original = context();
    let store = fixture.store(&original, 4);
    let mut reader = store.read(&original).unwrap();
    let update = store.publish(store.read(&original).unwrap(), b"new", &original);
    assert_eq!(update.effect(), publication::AuthPublicationEffect::Applied);
    assert_eq!(update.failure(), None);
    assert_eq!(update.cleanup_failure(), None);
    assert_eq!(reader.bytes(), Some(b"old".as_slice()));
    assert_eq!(
        err(store.revalidate(&mut reader, &original)),
        AuthDocumentError::Conflict
    );
    assert_eq!(
        store.read(&original).unwrap().bytes(),
        Some(b"new".as_slice())
    );
}

#[test]
fn same_bytes_on_a_replaced_physical_file_are_a_conflict() {
    let fixture = Fixture::new();
    fixture.write(b"same");
    let context = context();
    let store = fixture.store(&context, 4);
    let mut snapshot = store.read(&context).unwrap();
    fs::rename(
        fixture.root.join("auth.json"),
        fixture.root.join("original.json"),
    )
    .unwrap();
    fixture.write(b"same");
    assert_eq!(
        err(store.revalidate(&mut snapshot, &context)),
        AuthDocumentError::Conflict
    );
    assert_eq!(snapshot.bytes(), Some(b"same".as_slice()));
}

#[test]
fn changes_during_the_actual_read_cannot_return_a_snapshot() {
    let fixture = Fixture::new();
    fixture.write(b"read");
    let context = context();
    let store = fixture.store(&context, 4);
    let result = store.read_inner(&context, || {
        fs::rename(
            fixture.root.join("auth.json"),
            fixture.root.join("original.json"),
        )
        .unwrap();
        fixture.write(b"read");
    });
    assert_eq!(err(result), AuthDocumentError::Conflict);
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"read");
}

#[cfg(unix)]
#[test]
fn atomic_replacement_of_retained_auth_observation_is_conflict() {
    use std::os::unix::fs::PermissionsExt;
    for (mode, linked) in [(0o600, false), (0o644, false), (0o600, true)] {
        let fixture = Fixture::new();
        fixture.write(b"same");
        let original = context();
        let store = fixture.store(&original, 4);
        let mut snapshot = store.read(&original).unwrap();
        let next = fixture.root.join("auth.next");
        let mut stage = graphit_fs::create_private_file(&next).unwrap();
        stage.write_all(b"same").unwrap();
        stage.sync_all().unwrap();
        stage
            .set_permissions(fs::Permissions::from_mode(mode))
            .unwrap();
        drop(stage);
        fs::rename(&next, fixture.root.join("auth.json")).unwrap();
        if linked {
            fs::hard_link(
                fixture.root.join("auth.json"),
                fixture.root.join("current-link"),
            )
            .unwrap();
        }

        assert_eq!(
            err(store.revalidate(&mut snapshot, &original)),
            AuthDocumentError::Conflict
        );
        assert_eq!(snapshot.bytes(), Some(b"same".as_slice()));
        if mode == 0o600 && !linked {
            assert_eq!(
                store.read(&original).unwrap().bytes(),
                Some(b"same".as_slice())
            );
        } else {
            assert_eq!(err(store.read(&original)), AuthDocumentError::Denied);
        }
    }
}

#[cfg(unix)]
#[test]
fn atomic_replacement_during_auth_read_cannot_return_snapshot() {
    let fixture = Fixture::new();
    fixture.write(b"read");
    let original = context();
    let store = fixture.store(&original, 4);
    let result = store.read_inner(&original, || {
        let next = fixture.root.join("auth.next");
        let mut stage = graphit_fs::create_private_file(&next).unwrap();
        stage.write_all(b"read").unwrap();
        stage.sync_all().unwrap();
        drop(stage);
        fs::rename(&next, fixture.root.join("auth.json")).unwrap();
    });
    assert_eq!(err(result), AuthDocumentError::Conflict);
    assert_eq!(
        store.read(&original).unwrap().bytes(),
        Some(b"read".as_slice())
    );
}

#[test]
fn cancellation_and_deadline_are_original_and_checked_after_content_read() {
    let fixture = Fixture::new();
    fixture.write(b"data");
    let context = context();
    let store = fixture.store(&context, 4);
    let result = store.read_inner(&context, || context.cancellation().cancel());
    assert_eq!(err(result), AuthDocumentError::Cancelled);

    assert_eq!(err(store.read(&context)), AuthDocumentError::Cancelled);

    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(err(store.read(&expired)), AuthDocumentError::Deadline);
}

#[test]
fn finite_policy_rejects_zero_overflow_and_path_limits() {
    for values in [
        [0, 65536, 256, 65536],
        [8, 0, 256, 65536],
        [8, 65536, 0, 65536],
        [8, 65536, 256, 0],
        [usize::MAX, 65536, 256, 65536],
    ] {
        assert_eq!(
            err(AuthDocumentLimits::new(
                values[0], values[1], values[2], values[3],
            )),
            AuthDocumentError::InvalidPolicy
        );
    }
    let fixture = Fixture::new();
    let context = context();
    let limits = AuthDocumentLimits::new(8, 1, 256, 65536).unwrap();
    assert_eq!(
        err(AuthDocumentStore::capture(
            fixture.scope(&context),
            limits,
            &context
        )),
        AuthDocumentError::Limit
    );
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
}

#[test]
fn installation_scope_must_explicitly_include_its_exact_owner_root() {
    let fixture = Fixture::new();
    let context = context();
    let child = fixture.root.join("destination");
    fs::create_dir(&child).unwrap();
    let scope = fixture.scope_roots(&context, [child]);
    assert_eq!(
        err(AuthDocumentStore::capture(scope, limits(8), &context)),
        AuthDocumentError::InvalidPolicy
    );
    assert!(!fixture.root.join("auth.json").exists());
}

#[test]
fn project_scope_is_rejected_before_auth_io() {
    let fixture = Fixture::new();
    let context = context();
    let project_root = fixture._temporary.path().join("project");
    fs::create_dir_all(project_root.join(".graphit")).unwrap();
    let paths =
        crate::project_paths::ProjectPaths::at(&project_root, &fixture.installation).unwrap();
    let scope = CapturedPublicationScope::project(
        &paths,
        "01M38BCCWFHWK1KNE1E8N3TJPJ".parse().unwrap(),
        ScopeLimits::new(8, 65536, 256).unwrap(),
        &native_capture_request(std::sync::Arc::clone(&context)),
    )
    .unwrap();
    assert_eq!(
        err(AuthDocumentStore::capture(scope, limits(8), &context)),
        AuthDocumentError::InvalidPolicy
    );

    assert!(!project_root.join("auth.json").exists());
}

#[test]
fn failed_content_bound_still_rechecks_context_and_parent() {
    let fixture = Fixture::new();
    fixture.write(b"data");
    let cancelled = context();
    let store = fixture.store(&cancelled, 3);
    let result = store.read_attempt(
        &cancelled,
        || panic!("content was not read"),
        || {
            cancelled.cancellation().cancel();
        },
    );
    assert_eq!(err(result), AuthDocumentError::Cancelled);

    let changed = context();
    let store = fixture.store(&changed, 3);
    let result = store.read_attempt(
        &changed,
        || panic!("content was not read"),
        || {
            fs::rename(
                &fixture.root,
                fixture._temporary.path().join("original-root"),
            )
            .unwrap();
            drop(graphit_fs::create_private_directory(&fixture.root).unwrap());
        },
    );
    assert_eq!(err(result), AuthDocumentError::Conflict);
}

#[cfg(unix)]
#[test]
fn parent_and_file_privacy_are_rechecked_without_permission_repair() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fixture.write(b"data");
    let context = context();
    let store = fixture.store(&context, 4);
    let mut snapshot = store.read(&context).unwrap();
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(store.read(&context).is_err());
    assert!(store.revalidate(&mut snapshot, &context).is_err());
    assert_eq!(
        fs::metadata(&fixture.root).unwrap().permissions().mode() & 0o777,
        0o755
    );
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(
        fixture.root.join("auth.json"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(store.read(&context).is_err());
    assert_eq!(
        fs::metadata(fixture.root.join("auth.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
}

#[cfg(unix)]
#[test]
fn foreign_symlink_and_replaced_parent_never_return_auth_body() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let context = context();
    let store = fixture.store(&context, 8);
    let foreign = fixture._temporary.path().join("foreign");
    fs::write(&foreign, b"private").unwrap();
    symlink(&foreign, fixture.root.join("auth.json")).unwrap();
    assert!(store.read(&context).is_err());

    fs::remove_file(fixture.root.join("auth.json")).unwrap();
    fs::rename(
        &fixture.root,
        fixture._temporary.path().join("original-root"),
    )
    .unwrap();
    drop(graphit_fs::create_private_directory(&fixture.root).unwrap());
    assert!(store.read(&context).is_err());
    assert!(AuthDocumentStore::capture(store.scope.clone(), limits(8), &context).is_err());
}

#[test]
fn public_errors_have_no_source_paths_or_body() {
    use std::error::Error;
    let marker = "AUTH-DOCUMENT-MARKER";
    let error = AuthDocumentError::from(io::Error::other(marker));
    assert!(!format!("{error:?}: {error}").contains(marker));
    assert!(error.source().is_none());
}

fn native_capture_request(
    original: std::sync::Arc<crate::file_io::SnapshotContext>,
) -> std::sync::Arc<crate::file_io::SnapshotContext> {
    original
}
