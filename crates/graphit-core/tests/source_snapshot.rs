use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotContext,
        SnapshotPolicy, SourceSnapshot,
    },
    file_lock::Cancellation,
};
use graphit_fs::NodeKind;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
fn policy(paths: &[&str], bytes: usize) -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(bytes, 8).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::FollowLinks,
        CopyLimits::new(8 * 1024 * 1024, 64).unwrap(),
        RetainedFileSet::new(paths.iter().map(PathBuf::from), retention).unwrap(),
        retention,
    )
    .unwrap()
}
#[test]
fn retains_only_selected_bytes_and_shares_content_manifest() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("document.md"), b"hello").unwrap();
    fs::write(temp.path().join("asset.bin"), vec![42; 1024 * 1024]).unwrap();
    fs::create_dir(temp.path().join("empty")).unwrap();
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&["document.md"], 5),
        &context(),
    )
    .unwrap();
    assert_eq!(snapshot.retained_bytes(), 5);
    assert_eq!(
        snapshot.retained(Path::new("document.md")),
        Some(b"hello".as_slice())
    );
    assert!(snapshot.retained(Path::new("asset.bin")).is_none());
    assert_eq!(snapshot.nodes().len(), 4);
    let clone = snapshot.clone();
    assert_eq!(snapshot.fingerprint(), clone.fingerprint());
    assert_eq!(
        snapshot
            .retained(Path::new("document.md"))
            .unwrap()
            .as_ptr(),
        clone.retained(Path::new("document.md")).unwrap().as_ptr()
    );
    assert_eq!(
        snapshot
            .nodes()
            .find(|n| n.relative() == Path::new("empty"))
            .unwrap()
            .kind(),
        NodeKind::Directory
    );
}
#[test]
fn content_change_is_a_new_fingerprint_and_does_not_mutate_previous_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("doc");
    fs::write(&path, b"before").unwrap();
    let ctx = context();
    let before = SourceSnapshot::capture(&path, NodeKind::File, &policy(&[""], 64), &ctx).unwrap();
    fs::write(&path, b"after").unwrap();
    let after = SourceSnapshot::capture(&path, NodeKind::File, &policy(&[""], 64), &ctx).unwrap();
    assert_ne!(before.fingerprint(), after.fingerprint());
    assert_eq!(before.retained(Path::new("")), Some(b"before".as_slice()));
    assert_eq!(after.retained(Path::new("")), Some(b"after".as_slice()));
}
#[test]
fn retention_limit_and_invalid_selections_are_explicit_errors() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("doc"), b"hello").unwrap();
    let ctx = context();
    assert_eq!(
        SourceSnapshot::capture(temp.path(), NodeKind::Directory, &policy(&["doc"], 4), &ctx)
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::FileTooLarge
    );
    fs::create_dir(temp.path().join("directory")).unwrap();
    for (relative, kind) in [
        ("missing", std::io::ErrorKind::NotFound),
        ("directory", std::io::ErrorKind::InvalidInput),
    ] {
        assert_eq!(
            SourceSnapshot::capture(
                temp.path(),
                NodeKind::Directory,
                &policy(&["doc", relative], 64),
                &ctx,
            )
            .err()
            .unwrap()
            .kind(),
            kind,
        );
    }
}
#[test]
fn cancelled_and_expired_operations_do_not_return_partial_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("doc"), b"hello").unwrap();
    let cancelled = context();
    cancelled.cancellation().cancel();
    assert_eq!(
        SourceSnapshot::capture(
            temp.path(),
            NodeKind::Directory,
            &policy(&[], 64),
            &cancelled
        )
        .err()
        .unwrap()
        .kind(),
        std::io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        SourceSnapshot::capture(temp.path(), NodeKind::Directory, &policy(&[], 64), &expired)
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
}
#[cfg(unix)]
#[test]
fn configured_root_and_nested_directory_links_are_supported() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let external = temp.path().join("external");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&external).unwrap();
    fs::write(external.join("doc.md"), b"linked").unwrap();
    symlink(&external, source.join("docs")).unwrap();
    let alias = temp.path().join("alias");
    symlink(&source, &alias).unwrap();
    let snapshot = SourceSnapshot::capture(
        &alias,
        NodeKind::Directory,
        &policy(&["docs/doc.md"], 64),
        &context(),
    )
    .unwrap();
    assert_eq!(
        snapshot.retained(Path::new("docs/doc.md")),
        Some(b"linked".as_slice())
    );
}

#[test]
fn an_explicitly_selected_empty_file_is_retained() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("empty.md");
    fs::write(&file, []).unwrap();
    let snapshot =
        SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 1), &context()).unwrap();
    assert_eq!(snapshot.retained(Path::new("")), Some(&b""[..]));
    assert_eq!(snapshot.retained_bytes(), 0);
}
