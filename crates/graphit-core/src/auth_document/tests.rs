use super::*;
use crate::file_lock::Cancellation;
use publication::AuthPublicationEffect;
use std::{
    fs,
    io::Write,
    time::{Duration, Instant},
};

fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    )
}
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    drop(graphit_fs::create_private_directory(&root).unwrap());
    (temp, root)
}
fn store(root: &Path, bytes: usize, original: &SnapshotContext) -> AuthDocumentStore {
    AuthDocumentStore::new(
        root.to_owned(),
        AuthDocumentLimits::new(bytes).unwrap(),
        original,
    )
    .unwrap()
}
fn failure<T>(result: Result<T, AuthDocumentError>) -> AuthDocumentError {
    result.err().expect("must fail")
}

#[test]
fn absent_and_exact_content_limit_use_current_bytes() {
    let (_temp, root) = fixture();
    let request = original();
    let store = store(&root, 3, &request);
    assert!(store.read(&request).unwrap().bytes().is_none());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    assert_eq!(
        store.publish(store.read(&request).unwrap(), b"old", &request),
        Ok(AuthPublicationEffect::Applied)
    );
    assert_eq!(
        store.read(&request).unwrap().bytes(),
        Some(b"old".as_slice())
    );
    assert_eq!(
        store.publish(store.read(&request).unwrap(), b"old", &request),
        Ok(AuthPublicationEffect::Unchanged)
    );
    assert_eq!(
        store.publish(store.read(&request).unwrap(), b"long", &request),
        Err(AuthDocumentError::Limit)
    );
    assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"old");
}

#[test]
fn stale_bytes_conflict_but_equal_bytes_replacement_is_accepted() {
    let (_temp, root) = fixture();
    let request = original();
    let store = store(&root, 3, &request);
    store
        .lock_latest(&request)
        .unwrap()
        .publish(b"old")
        .unwrap();
    let old = store.read(&request).unwrap();
    store
        .lock_latest(&request)
        .unwrap()
        .publish(b"new")
        .unwrap();
    assert_eq!(
        store.publish(old, b"bad", &request),
        Err(AuthDocumentError::Conflict)
    );
    let same = store.read(&request).unwrap();
    let mut replacement = tempfile::Builder::new()
        .make_in(&root, graphit_fs::create_private_file)
        .unwrap();
    replacement.write_all(b"new").unwrap();
    replacement
        .into_temp_path()
        .persist(root.join("auth.json"))
        .unwrap();
    assert_eq!(
        store.publish(same, b"end", &request),
        Ok(AuthPublicationEffect::Applied)
    );
}

#[test]
fn latest_transaction_holds_one_global_lock_and_preserves_drop_without_write() {
    let (_temp, root) = fixture();
    let request = original();
    let first = store(&root, 8, &request);
    let second = store(&root, 8, &request);
    first
        .lock_latest(&request)
        .unwrap()
        .publish(b"current")
        .unwrap();
    let mutation = second.lock_latest(&request).unwrap();
    assert_eq!(mutation.bytes(), Some(b"current".as_slice()));
    assert!(
        private_document::try_lock(&root.join(publication::LOCK), &request)
            .unwrap()
            .is_none()
    );
    mutation.publish(b"next").unwrap();
    let mutation = first.lock_latest(&request).unwrap();
    assert_eq!(mutation.bytes(), Some(b"next".as_slice()));
    drop(mutation);
    assert_eq!(
        second.lock_latest(&request).unwrap().bytes(),
        Some(b"next".as_slice())
    );
}

#[test]
fn independent_stores_serialize_one_profile_without_blocking_other_profiles_or_publication() {
    fn assert_send<T: Send>(_: &T) {}
    let (_temp, root) = fixture();
    let request = original();
    let first = store(&root, 8, &request);
    let second = store(&root, 8, &request);
    let name = OsStr::new(".auth-refresh-a.lock");
    let held = first
        .try_profile_refresh_lock(name, &request)
        .unwrap()
        .unwrap();
    assert_send(&held);
    assert!(
        second
            .try_profile_refresh_lock(name, &request)
            .unwrap()
            .is_none()
    );
    let _other = second
        .try_profile_refresh_lock(OsStr::new(".auth-refresh-b.lock"), &request)
        .unwrap()
        .unwrap();
    second
        .lock_latest(&request)
        .unwrap()
        .publish(b"data")
        .unwrap();
    drop(held);
    assert!(
        second
            .try_profile_refresh_lock(name, &request)
            .unwrap()
            .is_some()
    );
}

#[test]
fn cancellation_expiration_and_limits_preserve_existing_content() {
    let (_temp, root) = fixture();
    let request = original();
    let store = store(&root, 3, &request);
    store
        .lock_latest(&request)
        .unwrap()
        .publish(b"old")
        .unwrap();
    let mutation = store.lock_latest(&request).unwrap();
    request.cancellation().cancel();
    assert_eq!(mutation.publish(b"new"), Err(AuthDocumentError::Cancelled));
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(failure(store.read(&expired)), AuthDocumentError::Deadline);
    assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"old");
    let fresh = original();
    assert_eq!(
        store.lock_latest(&fresh).unwrap().publish(b"long"),
        Err(AuthDocumentError::Limit)
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn configured_directory_symlink_and_private_hardlinks_are_supported() {
    use std::os::unix::fs::symlink;
    let (temp, root) = fixture();
    let alias = temp.path().join("selected");
    symlink(&root, &alias).unwrap();
    let request = original();
    let store = store(&alias, 3, &request);
    store
        .lock_latest(&request)
        .unwrap()
        .publish(b"old")
        .unwrap();
    fs::hard_link(root.join("auth.json"), root.join("kept")).unwrap();
    let snapshot = store.read(&request).unwrap();
    assert_eq!(
        store.publish(snapshot, b"new", &request),
        Ok(AuthPublicationEffect::Applied)
    );
    assert_eq!(fs::read(root.join("kept")).unwrap(), b"old");
    assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"new");
}

#[cfg(unix)]
#[test]
fn public_credential_bytes_are_denied_without_permission_repair() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, root) = fixture();
    let request = original();
    let store = store(&root, 3, &request);
    store
        .lock_latest(&request)
        .unwrap()
        .publish(b"old")
        .unwrap();
    fs::set_permissions(root.join("auth.json"), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(failure(store.read(&request)), AuthDocumentError::Denied);
    assert_eq!(
        fs::metadata(root.join("auth.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
}
