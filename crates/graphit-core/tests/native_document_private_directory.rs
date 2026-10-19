use graphit_core::{
    file_io::SnapshotContext,
    file_lock::Cancellation,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
};
use std::{
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn capture(path: &Path, context: &SnapshotContext) -> CapturedDocumentDirectory {
    let limits = NativeDocumentFileLimits::new(64, 65536, 256, 65536).unwrap();
    CapturedDocumentDirectory::capture(path, limits, context).unwrap()
}

#[test]
fn external_caller_qualifies_private_directory_without_mutating_it() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("private");
    let born = graphit_fs::create_private_directory(&path).unwrap();
    let context = context();
    let captured = capture(&path, &context);
    let identity = captured.directory_identity();

    let qualified = captured.private_handle(&context).unwrap();
    assert!(qualified.metadata().unwrap().is_dir());
    graphit_fs::verify_private_directory(&qualified).unwrap();
    assert!(identity == capture(&path, &context).directory_identity());
    captured.revalidate(&context).unwrap();
    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
    drop((qualified, captured, born));
}

#[test]
fn qualification_observes_cancellation_and_expiration() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("private");
    let born = graphit_fs::create_private_directory(&path).unwrap();
    let cancellation = Cancellation::default();
    let context = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    let captured = capture(&path, &context);
    cancellation.cancel();
    assert_eq!(
        captured.private_handle(&context).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        captured.private_handle(&expired).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
    drop((captured, born));
}

#[cfg(unix)]
#[test]
fn permissive_directory_is_rejected_without_repair() {
    use std::os::unix::fs::PermissionsExt;

    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("permissive");
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();
    let context = context();
    let captured = capture(&path, &context);
    assert_eq!(
        captured.private_handle(&context).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o777
    );
    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn replacement_does_not_qualify_the_new_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("private");
    let born = graphit_fs::create_private_directory(&path).unwrap();
    let context = context();
    let captured = capture(&path, &context);
    let identity = captured.directory_identity();
    fs::rename(&path, temporary.path().join("previous")).unwrap();
    let replacement = graphit_fs::create_private_directory(&path).unwrap();
    assert_eq!(
        captured.private_handle(&context).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(identity != capture(&path, &context).directory_identity());
    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
    drop((replacement, captured, born));
}
