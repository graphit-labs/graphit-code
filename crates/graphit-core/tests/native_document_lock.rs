use graphit_core::{
    file_io::SnapshotContext,
    file_lock::Cancellation,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    native_document_lock::NativeDocumentLock,
};
use std::{
    ffi::OsStr,
    fs, io,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn context(timeout: Duration, cancellation: Cancellation) -> SnapshotContext {
    SnapshotContext::new(Instant::now() + timeout, cancellation)
}
fn capture(path: &Path, ctx: &SnapshotContext) -> CapturedDocumentDirectory {
    CapturedDocumentDirectory::capture(
        path,
        NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
        ctx,
    )
    .unwrap()
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("private");
    drop(graphit_fs::create_private_directory(&parent).unwrap());
    (tmp, parent)
}
fn acquire<'a>(
    directory: &'a CapturedDocumentDirectory,
    ctx: &'a SnapshotContext,
) -> io::Result<NativeDocumentLock<'a>> {
    NativeDocumentLock::acquire(directory, OsStr::new("document.lock"), ctx)
}

#[test]
fn private_birth_reopen_and_drop_preserve_empty_permanent_entry() {
    let (_tmp, root) = fixture();
    let ctx = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let directory = capture(&root, &ctx);
    let guard = acquire(&directory, &ctx).unwrap();
    guard.revalidate().unwrap();
    let before = directory.read(OsStr::new("document.lock"), &ctx).unwrap();
    assert_eq!(before.bytes(), Some([].as_slice()));
    drop(guard);
    let next = capture(&root, &ctx);
    drop(acquire(&next, &ctx).unwrap());
    directory.revalidate_observation(&before, &ctx).unwrap();
    assert_eq!(fs::metadata(root.join("document.lock")).unwrap().len(), 0);
}

#[test]
fn lock_content_io_and_reservations_are_zero() {
    let (_tmp, root) = fixture();
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(&root, &ctx);
    for name in [
        "document.lock",
        ".config.lock",
        "ação-日本語.lock",
        "COM10.lock",
        "NUL-safe.lock",
    ] {
        let guard = NativeDocumentLock::acquire(&directory, OsStr::new(name), &ctx).unwrap();
        guard.revalidate().unwrap();
        assert_eq!(fs::metadata(root.join(name)).unwrap().len(), 0);
    }
}

#[test]
fn prebirth_cancel_deadline_invalid_name_and_path_limit_create_nothing() {
    let (_tmp, root) = fixture();
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(&root, &ctx);
    let cancel = Cancellation::default();
    cancel.cancel();
    let cancelled = context(Duration::from_secs(30), cancel);
    assert_eq!(
        acquire(&directory, &cancelled).err().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    let expired = context(Duration::ZERO, Cancellation::default());
    assert_eq!(
        acquire(&directory, &expired).err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    for name in [
        "",
        ".",
        "..",
        "../escape",
        "/absolute",
        "nested/file",
        "null\0entry",
    ] {
        assert!(NativeDocumentLock::acquire(&directory, OsStr::new(name), &ctx).is_err());
    }
    #[cfg(windows)]
    for name in [
        "file:stream",
        "NUL",
        "nul.tar.gz",
        "CON",
        "prn.lock",
        "AUX",
        "COM1",
        "com9.lock",
        "COM¹",
        "com².lock",
        "COM³",
        "LPT1",
        "lpt9.lock",
        "LPT¹",
        "lpt².lock",
        "LPT³",
        "entry.",
        "entry ",
        "nested\\file",
        "entry*",
        "entry?",
        "entry\"",
        "entry<",
        "entry>",
        "entry|",
        "entry\u{1f}",
    ] {
        assert_eq!(
            NativeDocumentLock::acquire(&directory, OsStr::new(name), &ctx)
                .err()
                .unwrap_or_else(|| panic!("accepted invalid document name {name:?}"))
                .kind(),
            io::ErrorKind::InvalidInput,
            "{name:?}"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0, "{name:?}");
    }
    let limited = CapturedDocumentDirectory::capture(
        &root,
        NativeDocumentFileLimits::new(1, 4096, 256, 65536).unwrap(),
        &ctx,
    )
    .unwrap();
    assert_eq!(
        NativeDocumentLock::acquire(&limited, OsStr::new(&"x".repeat(4096)), &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
}

#[test]
fn nonempty_entry_is_preserved_without_a_usable_guard() {
    use std::io::Write;
    let (_tmp, root) = fixture();
    let path = root.join("document.lock");
    let mut file = graphit_fs::create_private_lock_file(&path).unwrap();
    file.write_all(b"foreign-content").unwrap();
    drop(file);
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(&root, &ctx);
    assert_eq!(
        acquire(&directory, &ctx).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(&path).unwrap(), b"foreign-content");
}

#[test]
fn contention_uses_one_request_deadline_and_cancellation_releases_no_other_lock() {
    let (_tmp, root) = fixture();
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(&root, &ctx);
    let held = acquire(&directory, &ctx).unwrap();
    let deadline = Instant::now() + Duration::from_millis(150);
    let waiter = SnapshotContext::new(deadline, Cancellation::default());
    let error = acquire(&directory, &waiter).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(Instant::now() >= deadline);
    let cancellation = Cancellation::default();
    let waiter = context(Duration::from_secs(30), cancellation.clone());
    thread::scope(|scope| {
        let waiting = scope.spawn(|| acquire(&directory, &waiter).err().unwrap().kind());
        cancellation.cancel();
        assert_eq!(waiting.join().unwrap(), io::ErrorKind::Interrupted);
    });
    held.revalidate().unwrap();
    drop(held);
    let fresh = context(Duration::from_secs(30), Cancellation::default());
    drop(acquire(&directory, &fresh).unwrap());
}

#[cfg(unix)]
#[test]
fn access_symlink_hardlink_directory_and_public_parent_are_rejected_without_repair() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_tmp, root) = fixture();
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(&root, &ctx);
    let path = root.join("document.lock");
    let target = root.join("target");
    drop(graphit_fs::create_private_lock_file(&target).unwrap());
    symlink(&target, &path).unwrap();
    assert!(acquire(&directory, &ctx).is_err());
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(&path).unwrap();
    fs::hard_link(&target, &path).unwrap();
    assert!(acquire(&directory, &ctx).is_err());
    fs::remove_file(&path).unwrap();
    fs::rename(&target, &path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(acquire(&directory, &ctx).is_err());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o640
    );
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(acquire(&directory, &ctx).is_err());
    assert!(path.is_dir());
    fs::remove_dir(&path).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(acquire(&directory, &ctx).is_err());
    assert!(!path.exists());
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o7777,
        0o755
    );
}

#[cfg(unix)]
#[test]
fn missing_recreated_and_retargeted_parents_never_create_locks() {
    use std::os::unix::fs::symlink;
    let (tmp, root) = fixture();
    let alias = tmp.path().join("alias");
    symlink(&root, &alias).unwrap();
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(&alias, &ctx);
    fs::remove_file(&alias).unwrap();
    symlink(tmp.path(), &alias).unwrap();
    assert!(acquire(&directory, &ctx).is_err());
    assert!(!root.join("document.lock").exists());
    assert!(!tmp.path().join("document.lock").exists());
    let directory = capture(&root, &ctx);
    fs::rename(&root, tmp.path().join("original")).unwrap();
    assert!(acquire(&directory, &ctx).is_err());
    assert!(!root.exists());
    drop(graphit_fs::create_private_directory(&root).unwrap());
    assert!(acquire(&directory, &ctx).is_err());
    assert!(!root.join("document.lock").exists());
}

#[cfg(unix)]
#[test]
fn revalidation_detects_replaced_entry_and_releases_only_its_original_inode() {
    let (_tmp, root) = fixture();
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(&root, &ctx);
    let held = acquire(&directory, &ctx).unwrap();
    let path = root.join("document.lock");
    fs::rename(&path, root.join("old.lock")).unwrap();
    drop(graphit_fs::create_private_lock_file(&path).unwrap());
    assert_eq!(
        held.revalidate().err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    let foreign = acquire(&directory, &ctx).unwrap();
    drop(held);
    foreign.revalidate().unwrap();
    drop(foreign);
    assert!(root.join("old.lock").exists());
    assert!(path.exists());
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn independent_process_exclusion_and_process_death_release_keep_lock_file() {
    let (_tmp, root) = fixture();
    let ready = root.join("ready");
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "private_lock_child", "--nocapture"])
        .env("GRAPHIT_PRIVATE_LOCK_CHILD_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline, "child failed to acquire lock");
        thread::sleep(Duration::from_millis(10));
    }
    let ctx = context(Duration::from_millis(100), Cancellation::default());
    let directory = capture(&root, &ctx);
    assert_eq!(
        acquire(&directory, &ctx).err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let fresh = context(Duration::from_secs(30), Cancellation::default());
    drop(acquire(&directory, &fresh).unwrap());
    assert_eq!(fs::metadata(root.join("document.lock")).unwrap().len(), 0);
}

#[test]
fn private_lock_child() {
    use std::io::Read;
    let Some(root) = std::env::var_os("GRAPHIT_PRIVATE_LOCK_CHILD_ROOT") else {
        return;
    };
    let ctx = context(Duration::from_secs(30), Cancellation::default());
    let directory = capture(Path::new(&root), &ctx);
    let _held = acquire(&directory, &ctx).unwrap();
    fs::write(Path::new(&root).join("ready"), b"ready").unwrap();
    let mut byte = [0];
    let _ = io::stdin().read(&mut byte).unwrap();
}
