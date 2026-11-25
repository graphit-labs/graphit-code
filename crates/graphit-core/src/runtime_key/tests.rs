use super::*;
use crate::file_lock::Cancellation;
use std::time::{Duration, Instant};
fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    )
}
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installation");
    drop(graphit_fs::create_private_directory(&root).unwrap());
    (temp, root)
}
fn failure<T>(result: Result<T, RuntimeKeyError>) -> RuntimeKeyError {
    result.err().expect("must fail")
}

#[test]
fn real_owner_has_private_random_secret_and_excludes_another_owner() {
    let (_temp, root) = fixture();
    let request = original();
    let owner = RuntimeKeyOwner::start(root.clone(), &request).unwrap();
    let key = owner.check(&request).unwrap();
    assert!(
        key.secret(&request)
            .unwrap()
            .iter()
            .all(u8::is_ascii_hexdigit)
    );
    assert_eq!(
        fs::read(root.join("daemon/mcp.key")).unwrap(),
        key.secret(&request).unwrap().as_slice()
    );
    assert_eq!(
        failure(RuntimeKeyOwner::start(root.clone(), &request)),
        RuntimeKeyError::Busy
    );
    assert_eq!(owner.maintain(&request), Ok(RuntimeKeyEffect::NotApplied));
    let file = fs::File::open(root.join("daemon/mcp.key")).unwrap();
    graphit_fs::verify_private_file(&file).unwrap();
}

#[test]
fn missing_key_republication_changes_generation_and_removal_allows_explicit_restart() {
    let (_temp, root) = fixture();
    let request = original();
    let owner = RuntimeKeyOwner::start(root.clone(), &request).unwrap();
    let old = owner.check(&request).unwrap();
    fs::remove_file(root.join("daemon/mcp.key")).unwrap();
    assert_eq!(owner.maintain(&request), Ok(RuntimeKeyEffect::Applied));
    assert_eq!(old.recheck(&request), Err(RuntimeKeyError::Conflict));
    assert_eq!(owner.check(&request).unwrap().generation(), 2);
    assert_eq!(owner.remove(&request), Ok(RuntimeKeyEffect::Applied));
    assert_eq!(failure(owner.check(&request)), RuntimeKeyError::Conflict);
    assert!(!root.join("daemon/mcp.key").exists());
    drop(old);
    drop(owner);
    let restarted = RuntimeKeyOwner::start(root, &request).unwrap();
    restarted.check(&request).unwrap();
}

#[test]
fn foreign_content_is_neither_overwritten_nor_removed() {
    let (_temp, root) = fixture();
    let request = original();
    let owner = RuntimeKeyOwner::start(root.clone(), &request).unwrap();
    fs::write(root.join("daemon/mcp.key"), [0; KEY]).unwrap();
    assert_eq!(owner.maintain(&request), Err(RuntimeKeyError::Conflict));
    assert_eq!(owner.remove(&request), Err(RuntimeKeyError::Conflict));
    assert_eq!(fs::read(root.join("daemon/mcp.key")).unwrap(), [0; KEY]);
}

#[test]
fn original_cancellation_and_borrow_identity_are_preserved() {
    let (_temp, root) = fixture();
    let request = original();
    let owner = RuntimeKeyOwner::start(root.clone(), &request).unwrap();
    let borrow = owner.check(&request).unwrap();
    assert_eq!(borrow.recheck(&original()), Err(RuntimeKeyError::Conflict));
    request.cancellation().cancel();
    assert_eq!(owner.remove(&request), Err(RuntimeKeyError::Cancelled));
    assert!(root.join("daemon/mcp.key").exists());
    owner.check(&original()).unwrap();
}

#[cfg(unix)]
#[test]
fn configured_installation_symlink_is_supported() {
    use std::os::unix::fs::symlink;
    let (temp, root) = fixture();
    let alias = temp.path().join("selected");
    symlink(&root, &alias).unwrap();
    let request = original();
    let owner = RuntimeKeyOwner::start(alias, &request).unwrap();
    owner.check(&request).unwrap();
    assert_eq!(owner.remove(&request), Ok(RuntimeKeyEffect::Applied));
    assert!(!root.join("daemon/mcp.key").exists());
}
