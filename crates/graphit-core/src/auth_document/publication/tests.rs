use super::super::recovery::{self, AuthCleanupEffect};
use super::*;
use crate::{
    artifact_store::ScopeLimits, brand::Brand, file_lock::Cancellation,
    installation_identity::InstallationId, project_paths::InstallationPaths,
};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("installation");
        drop(graphit_fs::create_private_directory(&root).unwrap());
        Self {
            _temporary: temporary,
            root,
        }
    }
    fn store_at(
        root: &Path,
        context: &std::sync::Arc<SnapshotContext>,
        bytes: usize,
    ) -> AuthDocumentStore {
        let installation = InstallationPaths::resolve(
            Brand::new("graphit").unwrap(),
            root.parent().unwrap(),
            None,
            Some(root),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &installation,
            InstallationId::new("publisher-fixture").unwrap(),
            [root.to_owned()],
            ScopeLimits::new(8, 65536, 256).unwrap(),
            &std::sync::Arc::clone(context),
        )
        .unwrap();
        AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(bytes, 65536, 256, 65536).unwrap(),
            context,
        )
        .unwrap()
    }
    fn store(&self, context: &std::sync::Arc<SnapshotContext>) -> AuthDocumentStore {
        Self::store_at(&self.root, context, 512 * 1024)
    }
    fn write(&self, name: &str, bytes: &[u8]) {
        let mut file = graphit_fs::create_private_file(&self.root.join(name)).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
}
fn context() -> std::sync::Arc<SnapshotContext> {
    std::sync::Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
struct Hooks;
impl Drop for Hooks {
    fn drop(&mut self) {
        HOOK.with(|slot| *slot.borrow_mut() = None);
        NATIVE_FAULT.with(|slot| *slot.borrow_mut() = None);
        BIRTH_FAULT.with(|slot| slot.set(false));
        recovery::HOOK.with(|slot| *slot.borrow_mut() = None);
        recovery::REMOVE_FAULT.with(|slot| *slot.borrow_mut() = None);
    }
}
fn hook(action: impl FnMut(&'static str) + 'static) -> Hooks {
    HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(action)));
    Hooks
}
fn recovery_hook(action: impl FnMut(&'static str) + 'static) -> Hooks {
    recovery::HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(action)));
    Hooks
}

#[test]
fn first_publication_overwrite_unchanged_and_exact_io_are_real() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = fixture.store(&ctx);
    let result = store.publish(store.read(&ctx).unwrap(), b"first", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert_eq!(result.failure(), None);
    assert_eq!(result.cleanup_failure(), None);
    assert_eq!(result.cleanup_effect(), AuthCleanupEffect::Absent);
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"first");

    assert!(!fixture.root.join(STAGE).exists());

    let unchanged = store.publish(store.read(&ctx).unwrap(), b"first", &ctx);
    assert_eq!(unchanged.effect(), AuthPublicationEffect::Unchanged);

    let result = store.publish(store.read(&ctx).unwrap(), b"second", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"second");
    assert_eq!(AUTH_PRIVATE_NAMES, ["auth.json", STAGE, LOCK]);
}

#[test]
fn stale_request_store_and_same_bytes_new_inode_cannot_write() {
    let fixture = Fixture::new();
    fixture.write("auth.json", b"same");
    let ctx = context();
    let store = fixture.store(&ctx);
    let snapshot = store.read(&ctx).unwrap();
    let other = context();
    assert_eq!(
        store.publish(snapshot, b"next", &other).failure(),
        Some(AuthDocumentError::Conflict)
    );
    let snapshot = store.read(&ctx).unwrap();
    let other_store = fixture.store(&ctx);
    assert_eq!(
        other_store.publish(snapshot, b"next", &ctx).failure(),
        Some(AuthDocumentError::Conflict)
    );
    let snapshot = store.read(&ctx).unwrap();
    fs::rename(
        fixture.root.join("auth.json"),
        fixture.root.join("original"),
    )
    .unwrap();
    fixture.write("auth.json", b"same");
    let result = store.publish(snapshot, b"next", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));

    assert!(!fixture.root.join(STAGE).exists());
}

#[test]
fn shared_request_can_publish_read_snapshot() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = fixture.store(&ctx);
    let original = store.read(&ctx).unwrap();
    let shared = std::sync::Arc::clone(&ctx);
    let result = store.publish(original, b"new", &shared);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"new");
}

#[test]
fn occupied_stage_is_bounded_and_recovery_discards_without_parsing() {
    let fixture = Fixture::new();
    fixture.write(STAGE, b"not JSON and never an authentication proof");
    let ctx = context();
    let store = fixture.store(&ctx);
    for _ in 0..8 {
        let result = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
        assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
        assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
        assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 2);
    }

    assert_eq!(
        store.recover_stage(&ctx).effect(),
        AuthCleanupEffect::Applied
    );
    assert!(!fixture.root.join(STAGE).exists());

    assert_eq!(
        store.recover_stage(&ctx).effect(),
        AuthCleanupEffect::Absent
    );
    assert_eq!(
        store
            .publish(store.read(&ctx).unwrap(), b"new", &ctx)
            .effect(),
        AuthPublicationEffect::Applied
    );
}

#[test]
fn cancellation_at_each_stage_keeps_truthful_effect_and_cleanup_token() {
    for phase_name in [
        "created",
        "written",
        "synced",
        "before_replace",
        "after_replace",
        "before_cleanup",
    ] {
        let fixture = Fixture::new();
        fixture.write("auth.json", b"old");
        let cancellation = Cancellation::default();
        let ctx = std::sync::Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        ));
        let store = fixture.store(&ctx);
        let snapshot = store.read(&ctx).unwrap();
        let _hooks = hook(move |phase| {
            if phase == phase_name {
                cancellation.cancel();
            }
        });
        let result = store.publish(snapshot, b"new", &ctx);
        let applied = matches!(phase_name, "after_replace" | "before_cleanup");
        assert_eq!(
            result.effect(),
            if applied {
                AuthPublicationEffect::Applied
            } else {
                AuthPublicationEffect::NotApplied
            },
            "{phase_name}"
        );
        assert_eq!(
            fs::read(fixture.root.join("auth.json")).unwrap(),
            if applied { b"new" } else { b"old" }
        );
        assert_eq!(result.cleanup_failure(), Some(AuthDocumentError::Cancelled));
        let pending = result.into_pending_cleanup().unwrap();
        drop(_hooks);
        let retry = context();
        let result = store.cleanup_pending(pending, &retry);
        assert_eq!(
            result.effect(),
            if applied {
                AuthCleanupEffect::Absent
            } else {
                AuthCleanupEffect::Applied
            }
        );
        assert_eq!(result.failure(), None);
    }
}

#[test]
fn actual_deadline_after_creation_is_not_reset_and_recovers_automatically() {
    let fixture = Fixture::new();
    let deadline = Instant::now() + Duration::from_millis(100);
    let ctx = std::sync::Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
    let store = fixture.store(&ctx);
    let snapshot = store.read(&ctx).unwrap();
    let _hooks = hook(move |phase| {
        if phase == "created" {
            while Instant::now() < deadline {
                thread::sleep(Duration::from_millis(2));
            }
        }
    });
    let result = store.publish(snapshot, b"new", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Deadline));
    assert_eq!(ctx.deadline().unwrap(), deadline);

    drop(_hooks);
    assert_eq!(
        store.recover_stage(&context()).effect(),
        AuthCleanupEffect::Applied
    );
}

#[test]
fn native_failure_before_after_and_unobservable_effects_are_distinct() {
    for after in [false, true] {
        let fixture = Fixture::new();
        fixture.write("auth.json", b"old");
        let ctx = context();
        let store = fixture.store(&ctx);
        NATIVE_FAULT.with(|slot| {
            *slot.borrow_mut() = Some(if after {
                NativeFault::After
            } else {
                NativeFault::Before
            })
        });
        let _hooks = Hooks;
        let result = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
        assert_eq!(
            result.effect(),
            if after {
                AuthPublicationEffect::Applied
            } else {
                AuthPublicationEffect::NotApplied
            }
        );
        assert_eq!(result.failure(), Some(AuthDocumentError::Io));
        assert_eq!(result.cleanup_failure(), None);
    }
    let fixture = Fixture::new();
    fixture.write("auth.json", b"old");
    let ctx = context();
    let store = fixture.store(&ctx);
    let root = fixture.root.clone();
    let _hooks = hook(move |phase| {
        if phase == "after_replace" {
            fs::rename(root.join("auth.json"), root.join("native-original")).unwrap();
            let mut file = graphit_fs::create_private_file(&root.join("auth.json")).unwrap();
            file.write_all(b"unrelated").unwrap();
        }
    });
    NATIVE_FAULT.with(|slot| *slot.borrow_mut() = Some(NativeFault::After));
    let result = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::Unknown);
    assert_eq!(
        fs::read(fixture.root.join("auth.json")).unwrap(),
        b"unrelated"
    );
}

#[test]
fn old_and_stage_changes_before_replace_are_preserved() {
    for change_stage in [false, true] {
        let fixture = Fixture::new();
        fixture.write("auth.json", b"old");
        let ctx = context();
        let store = fixture.store(&ctx);
        let root = fixture.root.clone();
        let _hooks = hook(move |phase| {
            if phase == "before_replace" {
                let name = if change_stage { STAGE } else { "auth.json" };
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .open(root.join(name))
                    .unwrap();
                file.write_all(b"bad").unwrap();
            }
        });
        let result = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
        assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
        assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
        assert_eq!(
            fs::read(fixture.root.join("auth.json")).unwrap(),
            if change_stage { b"old" } else { b"bad" }
        );
    }
}

#[test]
fn pending_cleanup_never_deletes_a_replaced_stage_or_crosses_store() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let ctx = std::sync::Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    ));
    let store = fixture.store(&ctx);
    let _hooks = hook(move |phase| {
        if phase == "created" {
            cancellation.cancel();
        }
    });
    let pending = store
        .publish(store.read(&ctx).unwrap(), b"new", &ctx)
        .into_pending_cleanup()
        .unwrap();
    drop(_hooks);
    let retry = context();
    let other = fixture.store(&retry);
    let result = other.cleanup_pending(pending, &retry);
    assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
    let pending = result.into_pending_cleanup().unwrap();
    fs::rename(
        fixture.root.join(STAGE),
        fixture.root.join("original-stage"),
    )
    .unwrap();
    fixture.write(STAGE, b"foreign");
    let result = store.cleanup_pending(pending, &retry);
    assert_eq!(result.effect(), AuthCleanupEffect::NotApplied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
    assert_eq!(fs::read(fixture.root.join(STAGE)).unwrap(), b"foreign");
}

#[test]
fn recovery_native_effects_and_late_cancellation_are_explicit() {
    for after in [false, true] {
        let fixture = Fixture::new();
        fixture.write(STAGE, b"secret");
        let ctx = context();
        let store = fixture.store(&ctx);
        recovery::REMOVE_FAULT.with(|slot| *slot.borrow_mut() = Some(after));
        let _hooks = Hooks;
        let result = store.recover_stage(&ctx);
        assert_eq!(
            result.effect(),
            if after {
                AuthCleanupEffect::Applied
            } else {
                AuthCleanupEffect::NotApplied
            }
        );
        assert_eq!(result.failure(), Some(AuthDocumentError::Io));
    }
    let fixture = Fixture::new();
    fixture.write(STAGE, b"secret");
    let cancellation = Cancellation::default();
    let ctx = std::sync::Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    ));
    let store = fixture.store(&ctx);
    let _hooks = recovery_hook(move |phase| {
        if phase == "after_remove" {
            cancellation.cancel();
        }
    });
    let result = store.recover_stage(&ctx);
    assert_eq!(result.effect(), AuthCleanupEffect::Applied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Cancelled));
    assert!(!fixture.root.join(STAGE).exists());
}

#[cfg(unix)]
#[test]
fn recovery_rejects_symlinks_links_nonprivate_oversize_and_changed_identity() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for kind in [
        "symlink", "hardlink", "public", "oversize", "replaced", "privacy",
    ] {
        let fixture = Fixture::new();
        fixture.write("auth.json", b"untouched");
        if kind == "symlink" {
            symlink("auth.json", fixture.root.join(STAGE)).unwrap();
        } else {
            fixture.write(STAGE, b"stage");
        }
        if kind == "hardlink" {
            fs::hard_link(fixture.root.join(STAGE), fixture.root.join("alias")).unwrap();
        }
        if kind == "public" {
            fs::set_permissions(fixture.root.join(STAGE), fs::Permissions::from_mode(0o644))
                .unwrap();
        }
        let ctx = context();
        let store = Fixture::store_at(
            &fixture.root,
            &ctx,
            if kind == "oversize" { 1 } else { 1024 },
        );
        let root = fixture.root.clone();
        let _hooks = recovery_hook(move |phase| {
            if phase == "before_remove" {
                if kind == "replaced" {
                    fs::rename(root.join(STAGE), root.join("original-stage")).unwrap();
                    let file = graphit_fs::create_private_file(&root.join(STAGE)).unwrap();
                    drop(file);
                }
                if kind == "privacy" {
                    fs::set_permissions(root.join(STAGE), fs::Permissions::from_mode(0o644))
                        .unwrap();
                }
            }
        });
        let result = store.recover_stage(&ctx);
        assert_eq!(result.effect(), AuthCleanupEffect::NotApplied, "{kind}");
        assert!(result.failure().is_some());
        assert!(fs::symlink_metadata(fixture.root.join(STAGE)).is_ok());
        assert_eq!(
            fs::read(fixture.root.join("auth.json")).unwrap(),
            b"untouched"
        );
    }
}

#[test]
fn real_crashes_at_private_birth_write_and_fsync_recover_without_growth() {
    let fixture = Fixture::new();
    fixture.write("auth.json", b"old");
    for (iteration, crash_phase) in [
        "created", "written", "synced", "created", "written", "synced",
    ]
    .into_iter()
    .enumerate()
    {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "auth_document::publication::tests::crash_child",
                "--nocapture",
            ])
            .env("GRAPHIT_AUTH_CRASH_ROOT", &fixture.root)
            .env("GRAPHIT_AUTH_CRASH_PHASE", crash_phase)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        let exit = loop {
            if let Some(exit) = child.try_wait().unwrap() {
                break exit;
            }
            if Instant::now() >= until {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("child did not terminate at {crash_phase}");
            }
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(exit.code(), Some(23));
        assert!(fixture.root.join(STAGE).exists());
        assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 3);
        let ctx = context();
        let store = fixture.store(&ctx);
        assert_eq!(
            store.recover_stage(&ctx).effect(),
            AuthCleanupEffect::Applied
        );

        let committed = format!("committed-{iteration}");
        assert_eq!(
            store
                .publish(store.read(&ctx).unwrap(), committed.as_bytes(), &ctx)
                .effect(),
            AuthPublicationEffect::Applied
        );
        assert_eq!(
            fs::read(fixture.root.join("auth.json")).unwrap(),
            committed.as_bytes()
        );
        assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 2);
    }
}

#[cfg(unix)]
#[test]
fn changed_parent_lock_and_privacy_deny_before_replace() {
    use std::os::unix::fs::PermissionsExt;
    for change in ["parent", "lock", "privacy"] {
        let fixture = Fixture::new();
        fixture.write("auth.json", b"old");
        let ctx = context();
        let store = fixture.store(&ctx);
        let root = fixture.root.clone();
        let retired = root.with_file_name("retired");
        let read_back = retired.clone();
        let _hooks = hook(move |phase| {
            if phase == "before_replace" {
                match change {
                    "parent" => {
                        fs::rename(&root, &retired).unwrap();
                        drop(graphit_fs::create_private_directory(&root).unwrap());
                    }
                    "lock" => {
                        fs::rename(root.join(LOCK), root.join("retired-lock")).unwrap();
                        drop(graphit_fs::create_private_lock_file(&root.join(LOCK)).unwrap());
                    }
                    "privacy" => {
                        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap()
                    }
                    _ => unreachable!(),
                }
            }
        });
        let result = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
        assert_eq!(
            result.effect(),
            AuthPublicationEffect::NotApplied,
            "{change}"
        );
        assert!(result.failure().is_some());
        let old_path = if change == "parent" {
            read_back.join("auth.json")
        } else {
            fixture.root.join("auth.json")
        };
        assert_eq!(fs::read(old_path).unwrap(), b"old");
    }
}

#[test]
fn recovery_ambiguous_remove_and_cancelled_observation_are_unknown() {
    for replace in [false, true] {
        let fixture = Fixture::new();
        fixture.write(STAGE, b"old-stage");
        let cancellation = Cancellation::default();
        let ctx = std::sync::Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        ));
        let store = fixture.store(&ctx);
        let root = fixture.root.clone();
        let _hooks = recovery_hook(move |phase| {
            if phase == "after_remove" {
                if replace {
                    fs::rename(root.join(STAGE), root.join("original-stage")).unwrap();
                    drop(graphit_fs::create_private_file(&root.join(STAGE)).unwrap());
                } else {
                    cancellation.cancel();
                }
            }
        });
        recovery::REMOVE_FAULT.with(|slot| *slot.borrow_mut() = Some(false));
        let result = store.recover_stage(&ctx);
        assert_eq!(result.effect(), AuthCleanupEffect::Unknown);
        assert_eq!(
            result.failure(),
            Some(if replace {
                AuthDocumentError::Conflict
            } else {
                AuthDocumentError::Cancelled
            })
        );
        assert!(fixture.root.join(STAGE).exists());
    }
}

#[test]
fn failed_private_birth_empty_residue_is_recoverable_without_content_reads() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = fixture.store(&ctx);
    BIRTH_FAULT.with(|slot| slot.set(true));
    let _hooks = Hooks;
    // Inject the error after the real SDK's private CREATE_NEW. No origin token
    // is fabricated for the failed producer, and its actual empty file remains.
    let failed = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
    assert_eq!(failed.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(failed.failure(), Some(AuthDocumentError::Io));
    assert_eq!(failed.cleanup_effect(), AuthCleanupEffect::NotApplied);
    assert!(failed.into_pending_cleanup().is_none());
    assert_eq!(fs::metadata(fixture.root.join(STAGE)).unwrap().len(), 0);
    let result = store.recover_stage(&ctx);
    assert_eq!(result.effect(), AuthCleanupEffect::Applied);

    assert_eq!(
        store
            .publish(store.read(&ctx).unwrap(), b"new", &ctx)
            .effect(),
        AuthPublicationEffect::Applied
    );
}

#[test]
fn multi_chunk_write_stops_at_original_cancellation() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let ctx = std::sync::Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    ));
    let store = fixture.store(&ctx);
    let _hooks = hook(move |phase| {
        if phase == "written" {
            cancellation.cancel();
        }
    });
    let result = store.publish(store.read(&ctx).unwrap(), &[b'x'; 128 * 1024], &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);

    assert_eq!(
        fs::metadata(fixture.root.join(STAGE)).unwrap().len(),
        64 * 1024
    );
    assert!(!fixture.root.join("auth.json").exists());
    drop(_hooks);
    assert_eq!(
        store
            .cleanup_pending(result.into_pending_cleanup().unwrap(), &context())
            .effect(),
        AuthCleanupEffect::Applied
    );
}

#[cfg(unix)]
#[test]
fn stage_privacy_and_physical_origin_are_checked_before_secret_writes() {
    use std::os::unix::fs::PermissionsExt;
    for replace in [false, true] {
        let fixture = Fixture::new();
        let ctx = context();
        let store = fixture.store(&ctx);
        let root = fixture.root.clone();
        let _hooks = hook(move |phase| {
            if phase == "created" {
                if replace {
                    fs::rename(root.join(STAGE), root.join("original-stage")).unwrap();
                    drop(graphit_fs::create_private_file(&root.join(STAGE)).unwrap());
                } else {
                    fs::set_permissions(root.join(STAGE), fs::Permissions::from_mode(0o644))
                        .unwrap();
                }
            }
        });
        let result = store.publish(store.read(&ctx).unwrap(), b"secret", &ctx);
        assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
        assert_eq!(
            result.failure(),
            Some(if replace {
                AuthDocumentError::Conflict
            } else {
                AuthDocumentError::Denied
            })
        );

        assert_eq!(fs::metadata(fixture.root.join(STAGE)).unwrap().len(), 0);
    }
}

#[test]
#[ignore = "executed by bounded crash parent in isolated subprocesses"]
fn crash_child() {
    let root = PathBuf::from(std::env::var_os("GRAPHIT_AUTH_CRASH_ROOT").unwrap());
    let crash = std::env::var("GRAPHIT_AUTH_CRASH_PHASE").unwrap();
    let ctx = context();
    let store = Fixture::store_at(&root, &ctx, 1024);
    let _hooks = hook(move |phase| {
        if phase == crash {
            std::process::exit(23);
        }
    });
    let _ = store.publish(store.read(&ctx).unwrap(), &[b'x'; 512], &ctx);
    panic!("real publisher missed crash phase");
}

#[test]
fn review_stage_replacement_after_old_comparison_never_reaches_native_replace() {
    let fixture = Fixture::new();
    fixture.write("auth.json", b"old");
    let ctx = context();
    let store = fixture.store(&ctx);
    let root = fixture.root.clone();
    NATIVE_REPLACE_CALLS.with(|calls| calls.set(0));
    let _hooks = hook(move |phase| {
        if phase == "after_old_compare" {
            fs::rename(root.join(STAGE), root.join("original-stage")).unwrap();
            let mut foreign = graphit_fs::create_private_file(&root.join(STAGE)).unwrap();
            foreign.write_all(b"new").unwrap();
            foreign.sync_all().unwrap();
        }
    });
    let result = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
    assert_eq!(
        NATIVE_REPLACE_CALLS.with(std::cell::Cell::get),
        0,
        "foreign stage reached the actual native replacement wrapper"
    );
    assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"old");
    assert_eq!(fs::read(fixture.root.join(STAGE)).unwrap(), b"new");
    assert_eq!(
        fs::read(fixture.root.join("original-stage")).unwrap(),
        b"new"
    );
    assert!(result.into_pending_cleanup().is_some());
}

fn review_absent_guard(kind: &'static str) {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let deadline =
        Instant::now() + Duration::from_millis(if kind == "deadline" { 100 } else { 30000 });
    let ctx = std::sync::Arc::new(SnapshotContext::new(deadline, cancellation.clone()));
    let store = fixture.store(&ctx);
    let root = fixture.root.clone();
    recovery::NATIVE_REMOVE_CALLS.with(|calls| calls.set(0));
    let _hooks = recovery_hook(move |phase| {
        if phase == "before_remove" {
            match kind {
                "cancel" => cancellation.cancel(),
                "deadline" => {
                    while Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(2));
                    }
                }
                "parent" => {
                    fs::rename(&root, root.with_file_name("retired-parent")).unwrap();
                    drop(graphit_fs::create_private_directory(&root).unwrap());
                }
                "lock" => {
                    fs::rename(root.join(LOCK), root.join("retired-lock")).unwrap();
                    drop(graphit_fs::create_private_lock_file(&root.join(LOCK)).unwrap());
                }
                _ => unreachable!(),
            }
        }
    });
    let result = store.recover_stage(&ctx);
    assert_eq!(recovery::NATIVE_REMOVE_CALLS.with(std::cell::Cell::get), 0);
    assert_eq!(
        result.effect(),
        AuthCleanupEffect::NotApplied,
        "absence was minted after {kind}"
    );
    assert_eq!(
        result.failure(),
        Some(match kind {
            "cancel" => AuthDocumentError::Cancelled,
            "deadline" => AuthDocumentError::Deadline,
            _ => AuthDocumentError::Conflict,
        })
    );
    assert!(!fixture.root.join(STAGE).exists());
    assert!(!fixture.root.join("auth.json").exists());
}
#[test]
fn review_absent_recovery_rechecks_cancellation() {
    review_absent_guard("cancel");
}
#[test]
fn review_absent_recovery_rechecks_original_deadline() {
    review_absent_guard("deadline");
}
#[cfg(unix)]
#[test]
fn review_absent_recovery_rechecks_parent() {
    review_absent_guard("parent");
}
#[cfg(unix)]
#[test]
fn review_absent_recovery_rechecks_lock() {
    review_absent_guard("lock");
}

#[cfg(feature = "auth-test-support")]
mod cancellation_fixture;
