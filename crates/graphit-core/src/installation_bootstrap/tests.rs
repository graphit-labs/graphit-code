use super::*;
use crate::{brand::Brand, file_lock::Cancellation};
use std::time::{Duration, Instant};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> BootstrapLimits {
    BootstrapLimits::new(65536, 256).unwrap()
}
fn paths(root: &Path) -> InstallationPaths {
    InstallationPaths::resolve(
        Brand::new("graphit").unwrap(),
        root.parent().unwrap(),
        None,
        Some(root),
    )
    .unwrap()
}
#[test]
fn existing_private_root_is_read_only_and_request_bound() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    drop(graphit_fs::create_private_directory(&root).unwrap());
    let ctx = context();
    let outcome = prepare(&paths(&root), limits(), &ctx);
    assert_eq!(outcome.effects(), BootstrapEffects::default());
    assert_eq!(outcome.failure(), None);
    let installed = outcome.into_installation().unwrap();
    installed.revalidate(&ctx).unwrap();
    installed.revalidate(&ctx).unwrap();
    assert_eq!(
        installed.revalidate(&context()).unwrap_err(),
        AuthDocumentError::Conflict
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
}
#[test]
fn nested_roots_are_private_at_birth_and_create_no_auth_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("first/second/root");
    let resolved = InstallationPaths::resolve(
        Brand::new("graphit").unwrap(),
        temp.path(),
        None,
        Some(&root),
    )
    .unwrap();
    let ctx = context();
    let result = prepare(&resolved, limits(), &ctx);
    assert_eq!(result.failure(), None);
    assert_eq!(result.effects().created_directories, 3);
    assert!(!result.effects().uncertain_creation);
    let installed = result.into_installation().unwrap();
    installed.revalidate(&ctx).unwrap();
    assert_eq!(installed.paths().root(), root);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);

    for path in root.ancestors().take(3) {
        let captured = CapturedDocumentDirectory::capture(path, limits().native, &ctx).unwrap();
        drop(captured.private_handle(&ctx).unwrap());
    }
}
#[test]
fn invalid_and_short_policies_fail_before_creation() {
    for policy in [(0, 256), (65536, 0)] {
        assert!(BootstrapLimits::new(policy.0, policy.1).is_err());
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("first/second/root");
    let resolved = InstallationPaths::resolve(
        Brand::new("graphit").unwrap(),
        temp.path(),
        None,
        Some(&root),
    )
    .unwrap();
    let ctx = context();
    for policy in [
        BootstrapLimits::new(1, 256).unwrap(),
        BootstrapLimits::new(65536, root.ancestors().count() - 1).unwrap(),
    ] {
        let result = prepare(&resolved, policy, &ctx);
        assert_eq!(result.failure(), Some(AuthDocumentError::Limit));
        assert_eq!(result.effects(), BootstrapEffects::default());
        assert!(!temp.path().join("first").exists());
    }
    let exact = BootstrapLimits::new(65536, 256).unwrap();
    assert_eq!(
        prepare(&resolved, exact, &ctx)
            .effects()
            .created_directories,
        3
    );
}
#[test]
fn failed_native_birth_and_late_cancel_preserve_visible_effects() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let ctx = context();
    let failed = prepare_inner(
        &paths(&root),
        limits(),
        &ctx,
        |p| {
            drop(graphit_fs::create_private_directory(p)?);
            Err(io::Error::other("injected after actual birth"))
        },
        |_, _| {},
    );
    assert!(failed.effects().uncertain_creation);
    assert_eq!(failed.failure(), Some(AuthDocumentError::Io));
    assert!(root.is_dir());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    assert!(
        prepare(&paths(&root), limits(), &ctx)
            .into_installation()
            .is_ok()
    );
    let second = temp.path().join("second");
    let cancelled = context();
    let result = prepare_inner(
        &paths(&second),
        limits(),
        &cancelled,
        graphit_fs::create_private_directory,
        |phase, _| {
            if matches!(phase, Phase::AfterCreate) {
                cancelled.cancellation().cancel();
            }
        },
    );
    assert_eq!(result.effects().created_directories, 1);
    assert_eq!(result.failure(), Some(AuthDocumentError::Cancelled));
    assert!(second.is_dir());
    assert_eq!(fs::read_dir(&second).unwrap().count(), 0);
}
#[test]
fn cancelled_or_expired_request_cannot_create_a_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let cancelled = context();
    cancelled.cancellation().cancel();
    assert_eq!(
        prepare(&paths(&root), limits(), &cancelled).failure(),
        Some(AuthDocumentError::Cancelled)
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        prepare(&paths(&root), limits(), &expired).failure(),
        Some(AuthDocumentError::Deadline)
    );
    assert!(!root.exists());
}
#[test]
fn concurrent_private_birth_is_qualified_without_claiming_our_creation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let ctx = context();
    let result = prepare_inner(
        &paths(&root),
        limits(),
        &ctx,
        graphit_fs::create_private_directory,
        |phase, p| {
            if matches!(phase, Phase::BeforeCreate) {
                drop(graphit_fs::create_private_directory(p).unwrap());
            }
        },
    );
    assert_eq!(result.effects(), BootstrapEffects::default());
    result
        .into_installation()
        .unwrap()
        .revalidate(&ctx)
        .unwrap();
}

#[test]
fn exact_native_path_and_depth_are_admitted_before_creation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let ctx = context();
    let native = limits().native;
    let canonical = fs::canonicalize(temp.path()).unwrap().join("root");
    let path_cost = native
        .path_cost(&root)
        .unwrap()
        .max(native.path_cost(&canonical).unwrap());
    {
        let policy = BootstrapLimits::new(path_cost - 1, 256).unwrap();
        let result = prepare(&paths(&root), policy, &ctx);
        assert_eq!(result.failure(), Some(AuthDocumentError::Limit));
        assert_eq!(result.effects(), BootstrapEffects::default());
        assert!(!root.exists());
    }
    let exact = BootstrapLimits::new(path_cost, canonical.ancestors().count()).unwrap();
    let result = prepare(&paths(&root), exact, &ctx);
    assert_eq!(result.failure(), None);
    assert_eq!(result.effects().created_directories, 1);
}

#[cfg(unix)]
#[test]
fn retained_root_revalidation_rechecks_privacy_and_physical_identity() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let ctx = context();
    let installed = prepare(&paths(&root), limits(), &ctx)
        .into_installation()
        .unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(installed.revalidate(&ctx).is_err());
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o755
    );
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::rename(&root, temp.path().join("old-root")).unwrap();
    drop(graphit_fs::create_private_directory(&root).unwrap());
    assert_eq!(
        installed.revalidate(&ctx).unwrap_err(),
        AuthDocumentError::Conflict
    );
}
#[cfg(unix)]
#[test]
fn foreign_entries_and_nonprivate_roots_are_never_repaired() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let ctx = context();
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(prepare(&paths(&root), limits(), &ctx).failure().is_some());
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let file = temp.path().join("file");
    fs::write(&file, b"data").unwrap();
    assert_eq!(
        prepare(&paths(&file), limits(), &ctx).failure(),
        Some(AuthDocumentError::Denied)
    );
    let link = temp.path().join("link");
    symlink(&root, &link).unwrap();
    assert_eq!(
        prepare(&paths(&link), limits(), &ctx).failure(),
        Some(AuthDocumentError::Denied)
    );
}
#[cfg(unix)]
#[test]
fn parent_or_created_root_replacements_cannot_return_a_proof() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    drop(graphit_fs::create_private_directory(&parent).unwrap());
    let root = parent.join("root");
    let ctx = context();
    let result = prepare_inner(
        &paths(&root),
        limits(),
        &ctx,
        graphit_fs::create_private_directory,
        |phase, _| {
            if matches!(phase, Phase::BeforeCreate) {
                fs::rename(&parent, temp.path().join("old-parent")).unwrap();
                drop(graphit_fs::create_private_directory(&parent).unwrap());
            }
        },
    );
    assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
    assert_eq!(result.effects(), BootstrapEffects::default());
    assert!(!root.exists());
    let second = temp.path().join("second");
    let result = prepare_inner(
        &paths(&second),
        limits(),
        &ctx,
        graphit_fs::create_private_directory,
        |phase, p| {
            if matches!(phase, Phase::AfterCreate) {
                fs::rename(p, temp.path().join("old-second")).unwrap();
                drop(graphit_fs::create_private_directory(p).unwrap());
            }
        },
    );
    assert_eq!(result.failure(), Some(AuthDocumentError::Conflict));
    assert_eq!(result.effects().created_directories, 1);
}

#[test]
fn independent_start_path_is_admitted_before_any_birth() {
    let temp = tempfile::tempdir().unwrap();
    let start = temp.path().join("long-start".repeat(24));
    fs::create_dir(&start).unwrap();
    let root = temp.path().join("root");
    let resolved =
        InstallationPaths::resolve(Brand::new("graphit").unwrap(), &start, None, Some(&root))
            .unwrap();
    let ctx = context();
    let native = limits().native;
    let canonical = fs::canonicalize(temp.path()).unwrap().join("root");
    let short_cost = native
        .path_cost(&root)
        .unwrap()
        .max(native.path_cost(&canonical).unwrap());
    let mut births = 0;
    let result = prepare_inner(
        &resolved,
        BootstrapLimits::new(short_cost, 256).unwrap(),
        &ctx,
        |p| {
            births += 1;
            graphit_fs::create_private_directory(p)
        },
        |_, _| {},
    );
    assert_eq!(result.failure(), Some(AuthDocumentError::Limit));
    assert_eq!(births, 0);
    assert_eq!(result.effects(), BootstrapEffects::default());
    assert!(!root.exists());
}
