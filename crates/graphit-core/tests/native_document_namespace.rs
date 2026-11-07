use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentFileLimits, NativeDocumentNamespace,
    },
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const NS: &str = ".graphit-provider-documents.state";
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(1024, 65536, 256, 65536).unwrap()
}
fn scope(
    root: &Path,
    brand: &str,
    allowed: &Path,
    observations: usize,
) -> CapturedPublicationScope {
    let owner = root.join("installation");
    if !owner.exists() {
        fs::create_dir(&owner).unwrap();
    }
    let installation =
        InstallationPaths::resolve(Brand::new(brand).unwrap(), root, None, Some(&owner)).unwrap();
    CapturedPublicationScope::installation(
        &installation,
        InstallationId::new("namespace-fixture").unwrap(),
        [allowed.to_owned()],
        ScopeLimits::new(4, 65536, observations).unwrap(),
        &native_capture_request(),
    )
    .unwrap()
}
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    (tmp, root)
}
fn slot(
    scope: &CapturedPublicationScope,
    path: &Path,
    ctx: &SnapshotContext,
) -> NativeDocumentNamespace {
    NativeDocumentNamespace::capture(scope, path, limits(), ctx).unwrap()
}
fn private(path: &Path) {
    let mut options = fs::OpenOptions::new();
    #[cfg(unix)]
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        options
            .access_mode(0x0002_0000)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    graphit_fs::verify_private_directory(&options.open(path).unwrap()).unwrap();
}

#[test]
fn public_provider_parent_private_namespace_and_permanent_lock_without_document_io() {
    let (_tmp, root) = fixture();
    let provider = root.join("provider");
    fs::create_dir(&provider).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&provider, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let ctx = context();
    let scope = scope(&root, "graphit", &provider, 1024);
    let ns = slot(&scope, &provider, &ctx);
    let id = ns.directory_identity();
    assert!(id == ns.parent().directory_identity());
    private(&provider.join(NS));
    let held = ns.lock(&ctx).unwrap();
    held.revalidate().unwrap();
    assert_eq!(fs::read_dir(provider.join(NS)).unwrap().count(), 1);
    assert!(!provider.join(NS).join("documents.json").exists());
    assert_eq!(
        fs::metadata(provider.join(NS).join("lock")).unwrap().len(),
        0
    );
    let before = CapturedDocumentDirectory::capture(&provider.join(NS), limits(), &ctx).unwrap();
    #[cfg(windows)]
    assert_eq!(
        before
            .read(OsStr::new("lock"), &ctx)
            .err()
            .unwrap()
            .raw_os_error(),
        Some(32)
    );
    drop(held);
    let observed = before.read(OsStr::new("lock"), &ctx).unwrap();
    assert_eq!(observed.bytes(), Some([].as_slice()));
    let reopened = slot(&scope, &provider, &ctx);
    assert!(id == reopened.directory_identity());
    drop(reopened.lock(&ctx).unwrap());
    before.revalidate_observation(&observed, &ctx).unwrap();
    // The read above is a separate fixture observation, not namespace content I/O.

    let untouched = context();
    ns.revalidate(&untouched).unwrap();
    drop(ns.lock(&untouched).unwrap());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&provider).unwrap().permissions().mode() & 0o7777,
            0o755
        );
    }
}

#[test]
fn missing_allowed_root_and_finite_descendants_are_private_at_birth() {
    let (_tmp, root) = fixture();
    let allowed = root.join("provider");
    let scope = scope(&root, "graphit", &allowed, 1024);
    let ctx = context();
    let target = allowed.join("agents").join("workspace");
    let ns = slot(&scope, &target, &ctx);
    for path in [&allowed, &allowed.join("agents"), &target, &target.join(NS)] {
        private(path);
    }
    ns.revalidate(&ctx).unwrap();
    assert_eq!(fs::read_dir(target.join(NS)).unwrap().count(), 0);
}

#[test]
fn existing_project_owner_above_runtime_is_an_authorized_provider_parent() {
    let (_tmp, root) = fixture();
    let installation =
        InstallationPaths::resolve(Brand::default(), &root, None, Some(&root.join("global")))
            .unwrap();
    let project = ProjectPaths::at(&root, &installation).unwrap();
    let ctx = std::sync::Arc::new(context());
    let scope = CapturedPublicationScope::project(
        &project,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        ScopeLimits::new(1, 65536, 1024).unwrap(),
        &project_birth_request(std::sync::Arc::clone(&ctx)),
    )
    .unwrap();
    let ns = slot(&scope, &root, &ctx);
    ns.lock(&ctx).unwrap().revalidate().unwrap();
    assert!(!scope.runtime_root().exists());
}

#[test]
fn invalid_cancelled_expired_and_bounded_layout_fail_before_any_birth() {
    let (_tmp, root) = fixture();
    let allowed = root.join("provider");
    let scope = scope(&root, "graphit", &allowed, 1024);
    let ctx = context();
    for target in [
        root.join("outside"),
        allowed.join("../escape"),
        PathBuf::from("relative"),
        allowed.join("null\0segment"),
        scope.runtime_root().to_owned(),
    ] {
        assert!(NativeDocumentNamespace::capture(&scope, &target, limits(), &ctx).is_err());
    }
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancelled);
    assert_eq!(
        NativeDocumentNamespace::capture(&scope, &allowed, limits(), &cancelled)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        NativeDocumentNamespace::capture(&scope, &allowed, limits(), &expired)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    for limited in [
        NativeDocumentFileLimits::new(1, 65536, 1, 65536).unwrap(),
        NativeDocumentFileLimits::new(1, 1, 256, 65536).unwrap(),
    ] {
        assert_eq!(
            NativeDocumentNamespace::capture(&scope, &allowed, limited, &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
    assert!(!allowed.exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}

#[test]
fn missing_ancestors_outside_allowed_root_are_not_created() {
    let (_tmp, root) = fixture();
    let outside = root.join("outside");
    let allowed = outside.join("provider");
    let scope = scope(&root, "graphit", &allowed, 1024);
    assert_eq!(
        NativeDocumentNamespace::capture(&scope, &allowed, limits(), &context())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!outside.exists());
}

#[cfg(unix)]
#[test]
fn public_file_and_symlink_namespaces_are_preserved_without_repair() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for case in 0..3 {
        let (_tmp, root) = fixture();
        let provider = root.join("provider");
        fs::create_dir(&provider).unwrap();
        let scope = scope(&root, "graphit", &provider, 1024);
        let ns = provider.join(NS);
        if case == 0 {
            fs::create_dir(&ns).unwrap();
            fs::set_permissions(&ns, fs::Permissions::from_mode(0o755)).unwrap();
            fs::write(ns.join("sentinel"), b"foreign").unwrap();
        }
        if case == 1 {
            fs::write(&ns, b"foreign").unwrap();
        }
        if case == 2 {
            symlink(&root, &ns).unwrap();
        }
        assert!(NativeDocumentNamespace::capture(&scope, &provider, limits(), &context()).is_err());
        match case {
            0 => {
                assert_eq!(
                    fs::metadata(&ns).unwrap().permissions().mode() & 0o7777,
                    0o755
                );
                assert_eq!(fs::read(ns.join("sentinel")).unwrap(), b"foreign");
            }
            1 => assert_eq!(fs::read(&ns).unwrap(), b"foreign"),
            _ => assert!(fs::symlink_metadata(&ns).unwrap().file_type().is_symlink()),
        }
    }
}

#[cfg(unix)]
#[test]
fn aliases_share_parent_identity_but_all_routes_must_be_revalidated_after_dedup() {
    use std::os::unix::fs::symlink;
    let (_tmp, root) = fixture();
    let provider = root.join("provider");
    fs::create_dir(&provider).unwrap();
    let alias = root.join("alias");
    symlink(&provider, &alias).unwrap();
    let scope = scope(&root, "graphit", &provider, 1024);
    let ctx = context();
    let direct = slot(&scope, &provider, &ctx);
    let route = slot(&scope, &alias, &ctx);
    assert!(direct.directory_identity() == route.directory_identity());
    let held = direct.lock(&ctx).unwrap();
    route.revalidate(&ctx).unwrap();
    fs::remove_file(&alias).unwrap();
    symlink(&root, &alias).unwrap();
    assert_eq!(
        route.revalidate(&ctx).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    held.revalidate().unwrap();
    assert!(!root.join(NS).exists());
}

#[cfg(unix)]
#[test]
fn scope_parent_namespace_and_lock_replacements_are_detected_without_cleanup() {
    for case in 0..4 {
        let (_tmp, root) = fixture();
        let provider = root.join("provider");
        fs::create_dir(&provider).unwrap();
        let scope = scope(&root, "graphit", &provider, 1024);
        let ctx = context();
        let ns = slot(&scope, &provider, &ctx);
        let held = ns.lock(&ctx).unwrap();
        let path = match case {
            0 => root.join("installation"),
            1 => provider.clone(),
            2 => provider.join(NS),
            _ => provider.join(NS).join("lock"),
        };
        let backup = root.join("original");
        fs::rename(&path, &backup).unwrap();
        if case == 3 {
            drop(graphit_fs::create_private_lock_file(&path).unwrap());
        } else {
            drop(graphit_fs::create_private_directory(&path).unwrap());
        }
        assert!(held.revalidate().is_err());
        drop(held);
        drop(ns);
        assert!(backup.exists());
        assert!(path.exists());
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn brands_and_filenames_share_one_actual_process_lock_and_original_deadline() {
    let (_tmp, root) = fixture();
    let provider = root.join("provider");
    fs::create_dir(&provider).unwrap();
    fs::write(provider.join("mcp.json"), b"mcp").unwrap();
    fs::write(provider.join("agent.json"), b"agent").unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "namespace_lock_child", "--nocapture"])
        .env("GRAPHIT_NAMESPACE_CHILD_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let ready = provider.join("ready");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let scope = scope(&root, "atlas", &provider, 1024);
    let ctx = context();
    let ns = slot(&scope, &provider, &ctx);
    let deadline = Instant::now() + Duration::from_millis(100);
    let wait = SnapshotContext::new(deadline, Cancellation::default());
    assert_eq!(
        ns.lock(&wait).err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(Instant::now() >= deadline);
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let held = ns.lock(&ctx).unwrap();
    held.revalidate().unwrap();
    assert_eq!(
        ns.parent()
            .read(OsStr::new("agent.json"), &ctx)
            .unwrap()
            .bytes(),
        Some(b"agent".as_slice())
    );
    assert_eq!(
        fs::metadata(provider.join(NS).join("lock")).unwrap().len(),
        0
    );
    assert_eq!(fs::read_dir(provider.join(NS)).unwrap().count(), 1);
}

#[test]
fn namespace_lock_child() {
    use std::io::Read;
    let Some(root) = std::env::var_os("GRAPHIT_NAMESPACE_CHILD_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let provider = root.join("provider");
    let scope = scope(root, "graphit", &provider, 1024);
    let ctx = context();
    let ns = slot(&scope, &provider, &ctx);
    let held = ns.lock(&ctx).unwrap();
    held.revalidate().unwrap();
    assert_eq!(
        ns.parent()
            .read(OsStr::new("mcp.json"), &ctx)
            .unwrap()
            .bytes(),
        Some(b"mcp".as_slice())
    );
    fs::write(provider.join("ready"), b"ready").unwrap();
    let mut byte = [0];
    let _ = io::stdin().read(&mut byte).unwrap();
}

#[test]
fn valid_private_brand_directory_cannot_collide_with_the_common_namespace() {
    let (_tmp, root) = fixture();
    let brand = Brand::new("graphit-provider-documents").unwrap();
    assert!(Brand::new("graphit-provider-documents.state").is_err());
    let brand_directory = root.join(brand.directory());
    fs::create_dir(&brand_directory).unwrap();
    fs::write(brand_directory.join("sentinel"), b"brand-content").unwrap();
    let installation =
        InstallationPaths::resolve(brand, &root, None, Some(&root.join("global"))).unwrap();
    let project = ProjectPaths::at(&root, &installation).unwrap();
    let ctx = std::sync::Arc::new(context());
    let scope = CapturedPublicationScope::project(
        &project,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        ScopeLimits::new(1, 65536, 1024).unwrap(),
        &project_birth_request(std::sync::Arc::clone(&ctx)),
    )
    .unwrap();
    let ns = slot(&scope, &root, &ctx);
    let held = ns.lock(&ctx).unwrap();
    held.revalidate().unwrap();
    private(&root.join(NS));
    assert_ne!(root.join(NS), brand_directory);
    assert_eq!(
        fs::read(brand_directory.join("sentinel")).unwrap(),
        b"brand-content"
    );
    assert!(!scope.runtime_root().exists());
    assert_eq!(fs::read_dir(root.join(NS)).unwrap().count(), 1);
}

fn project_birth_request(
    original: std::sync::Arc<graphit_core::file_io::SnapshotContext>,
) -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    original
}

fn native_capture_request() -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        graphit_core::file_lock::Cancellation::default(),
    ))
}
