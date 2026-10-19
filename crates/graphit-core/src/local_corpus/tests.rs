use super::*;
use crate::artifact_store::ScopeLimits;
use crate::brand::Brand;
use crate::file_lock::Cancellation;
use crate::project_paths::{InstallationPaths, ProjectPaths};
use std::fs;
use std::time::{Duration, Instant};

const PROJECT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn request() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}

fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}

fn fixture() -> (tempfile::TempDir, CapturedPublicationScope) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("checkout");
    fs::create_dir(&root).unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("global")),
    )
    .unwrap();
    let paths = ProjectPaths::at(&root, &installation).unwrap();
    let scope = CapturedPublicationScope::project(
        &paths,
        PROJECT.parse().unwrap(),
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &request(),
    )
    .unwrap();
    (temp, scope)
}

#[test]
fn absent_runtime_selects_normal_dataset_without_creating_state() {
    let (temp, scope) = fixture();
    for version in ["main", "feat/x"] {
        assert_eq!(
            resolve(
                &scope,
                version,
                "knowledge",
                "index.lance",
                limits(),
                &request()
            )
            .unwrap(),
            Path::new("knowledge/index.lance")
        );
    }
    assert!(!temp.path().join("global").exists());
    assert!(!scope.owner_root().join(".graphit").exists());
}

#[test]
fn invalid_caller_selection_fails_before_filesystem_birth() {
    let (temp, scope) = fixture();
    for (version, module, dataset) in [
        ("", "knowledge", "index.lance"),
        (" main", "knowledge", "index.lance"),
        ("main\u{1}", "knowledge", "index.lance"),
        ("main", "unknown", "index.lance"),
        ("main", "../knowledge", "index.lance"),
        ("main", "knowledge", "../index.lance"),
        ("main", "knowledge", "dir/index.lance"),
        ("main", "knowledge", "dir\\index.lance"),
        ("main", "knowledge", "/index.lance"),
        ("main", "knowledge", ""),
    ] {
        assert_eq!(
            resolve(&scope, version, module, dataset, limits(), &request())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(!temp.path().join("global").exists());
}

#[test]
fn original_cancellation_and_deadline_remain_errors() {
    let (_temp, scope) = fixture();
    let current = request();
    current.cancellation().cancel();
    assert_eq!(
        resolve(
            &scope,
            "main",
            "knowledge",
            "index.lance",
            limits(),
            &current
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert_eq!(
        resolve(
            &scope,
            "main",
            "knowledge",
            "index.lance",
            limits(),
            &expired
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn versions_and_worktrees_resolve_one_dataset_under_the_stable_project() {
    let (temp, scope) = fixture();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("global")),
    )
    .unwrap();
    let checkout = temp.path().join("worktree");
    fs::create_dir(&checkout).unwrap();
    let paths = ProjectPaths::at(&checkout, &installation).unwrap();
    let other = CapturedPublicationScope::project(
        &paths,
        PROJECT.parse().unwrap(),
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &request(),
    )
    .unwrap();
    assert_eq!(scope.runtime_root(), other.runtime_root());
    for module in ["ast", "knowledge", "memory", "task"] {
        for version in ["main", "feat/ação"] {
            for selected in [&scope, &other] {
                assert_eq!(
                    resolve(
                        selected,
                        version,
                        module,
                        "index.lance",
                        limits(),
                        &request()
                    )
                    .unwrap(),
                    Path::new(module).join("index.lance")
                );
            }
        }
    }
    let foreign = CapturedPublicationScope::project(
        &paths,
        "01ARZ3NDEKTSV4RRFFQ69G5FAX".parse().unwrap(),
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &request(),
    )
    .unwrap();
    assert_ne!(scope.runtime_root(), foreign.runtime_root());
}

#[test]
fn path_and_ancestor_limits_remain_enforced() {
    let (_temp, scope) = fixture();
    for small in [
        NativeDocumentFileLimits::new(65536, 1, 128, 65536).unwrap(),
        NativeDocumentFileLimits::new(65536, 4096, 1, 65536).unwrap(),
    ] {
        assert_eq!(
            resolve(
                &scope,
                "main",
                "knowledge",
                "index.lance",
                small,
                &request()
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
}

#[cfg(unix)]
#[test]
fn canonical_dataset_capture_cannot_escape_through_symlinks() {
    let (temp, scope) = fixture();
    fs::create_dir_all(scope.runtime_root()).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, scope.runtime_root().join("knowledge")).unwrap();
    let original = request();
    let relative = resolve(
        &scope,
        "main",
        "knowledge",
        "index.lance",
        limits(),
        &original,
    )
    .unwrap();
    // Resolution is a lexical observation; the existing SDK capture owns the
    // physical-directory fence and must reject a linked ancestor.
    assert!(
        CapturedProjectDocumentDirectory::probe_runtime(&scope, &relative, limits(), &original)
            .is_err()
    );
}
