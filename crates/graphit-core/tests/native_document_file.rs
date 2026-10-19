use graphit_core::{
    file_io::SnapshotContext,
    file_lock::Cancellation,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentDirectoryIdentity, NativeDocumentFileLimits,
    },
};
use std::{
    ffi::OsStr,
    fs, io,
    time::{Duration, Instant},
};

fn limits(bytes: usize) -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(bytes, 65536, 256, 65536).unwrap()
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn capture(
    root: &std::path::Path,
    ctx: &SnapshotContext,
    bytes: usize,
) -> CapturedDocumentDirectory {
    CapturedDocumentDirectory::capture(root, limits(bytes), ctx).unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn captures_retain_selected_directories_without_duplicate_ancestor_descriptors() {
    fn descriptors_for(path: &std::path::Path) -> usize {
        fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_link(entry.path()).ok())
            .filter(|target| target == path)
            .count()
    }
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("a/b/c/d/e");
    let root = parent.join("documents");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("config"), b"value").unwrap();
    let ctx = context();
    let captures = (0..8).map(|_| capture(&root, &ctx, 64)).collect::<Vec<_>>();
    assert_eq!(descriptors_for(&root), 8);
    assert_eq!(descriptors_for(&parent), 0);
    assert_eq!(descriptors_for(tmp.path()), 0);
    for directory in &captures {
        let observation = directory.read(OsStr::new("config"), &ctx).unwrap();
        assert_eq!(observation.bytes(), Some(b"value".as_slice()));
        directory
            .revalidate_observation(&observation, &ctx)
            .unwrap();
    }
    drop(captures);
    assert_eq!(descriptors_for(&root), 0);
}

#[cfg(unix)]
#[test]
fn replaced_ancestor_is_rejected_even_when_the_selected_directory_is_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let root = parent.join("documents");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("config"), b"value").unwrap();
    let ctx = context();
    let directory = capture(&root, &ctx, 64);
    let identity = directory.directory_identity();
    let moved = tmp.path().join("original-parent");
    fs::rename(&parent, &moved).unwrap();
    fs::create_dir(&parent).unwrap();
    fs::rename(moved.join("documents"), &root).unwrap();
    let recaptured = capture(&root, &ctx, 64);
    assert!(identity == recaptured.directory_identity());
    assert_eq!(
        directory.revalidate(&ctx).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(directory.read(OsStr::new("config"), &ctx).is_err());
    assert_eq!(fs::read(root.join("config")).unwrap(), b"value");
}

#[test]
fn missing_observation_creates_nothing_and_detects_later_creator() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context();
    let directory = capture(tmp.path(), &ctx, 64);
    let missing = directory.read(OsStr::new("config.json"), &ctx).unwrap();
    assert!(missing.bytes().is_none());
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);

    fs::write(tmp.path().join("config.json"), b"{}").unwrap();
    assert_eq!(
        directory
            .revalidate_observation(&missing, &ctx)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(tmp.path().join("config.json")).unwrap(), b"{}");
}

#[test]
fn exact_content_bound_allows_repeat_reads() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("config"), b"abcd").unwrap();
    let ctx = context();
    let directory = capture(tmp.path(), &ctx, 4);
    let observation = directory.read(OsStr::new("config"), &ctx).unwrap();
    assert_eq!(observation.bytes(), Some(b"abcd".as_slice()));
    directory
        .revalidate_observation(&observation, &ctx)
        .unwrap();
    assert_eq!(
        directory.read(OsStr::new("config"), &ctx).unwrap().bytes(),
        observation.bytes()
    );
    let short = capture(tmp.path(), &ctx, 3);
    assert_eq!(
        short.read(OsStr::new("config"), &ctx).err().unwrap().kind(),
        io::ErrorKind::FileTooLarge
    );
}

#[test]
fn oversized_file_wrong_kind_and_names_fail_without_reading_or_creation() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("large"), b"12345").unwrap();
    fs::create_dir(tmp.path().join("folder")).unwrap();
    let ctx = context();
    let directory = capture(tmp.path(), &ctx, 4);
    for name in [
        "",
        ".",
        "..",
        "../outside",
        "two/components",
        "large",
        "folder",
        "nul\0name",
    ] {
        assert!(directory.read(OsStr::new(name), &ctx).is_err(), "{name:?}");
    }

    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 2);
    assert!(NativeDocumentFileLimits::new(usize::MAX, 65536, 1, 12).is_err());
    assert!(NativeDocumentFileLimits::new(4, 65537, 1, 12).is_err());
    assert!(NativeDocumentFileLimits::new(4, 65536, 0, 12).is_err());
    let shallow = NativeDocumentFileLimits::new(4, 65536, 1, 12).unwrap();
    assert!(CapturedDocumentDirectory::capture(tmp.path(), shallow, &ctx).is_err());
    assert!(
        CapturedDocumentDirectory::capture(std::path::Path::new("relative"), limits(4), &ctx)
            .is_err()
    );
}

#[test]
fn cancellation_and_deadline_remain_captured_between_passes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("config"), b"old").unwrap();
    let cancel = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
    let directory = capture(tmp.path(), &ctx, 64);
    let observation = directory.read(OsStr::new("config"), &ctx).unwrap();
    cancel.cancel();
    assert_eq!(
        directory
            .revalidate_observation(&observation, &ctx)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );

    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        directory
            .read(OsStr::new("config"), &expired)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );

    assert_eq!(fs::read(tmp.path().join("config")).unwrap(), b"old");
}

#[test]
fn observation_belongs_to_its_physical_directory_and_detects_in_place_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("config"), b"old").unwrap();
    fs::write(other.path().join("config"), b"old").unwrap();
    let ctx = context();
    let directory = capture(tmp.path(), &ctx, 64);
    let observation = directory.read(OsStr::new("config"), &ctx).unwrap();
    let foreign = capture(other.path(), &ctx, 64);
    assert!(!directory.same_directory(&foreign));
    assert!(foreign.revalidate_observation(&observation, &ctx).is_err());
    fs::write(tmp.path().join("config"), b"new").unwrap();
    assert!(
        directory
            .revalidate_observation(&observation, &ctx)
            .is_err()
    );
}

#[test]
fn directory_identity_is_stable_data_without_io_or_capabilities() {
    let first = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let ctx = context();
    let captured = capture(first.path(), &ctx, 1);
    let recaptured = capture(first.path(), &ctx, 1);
    let foreign = capture(other.path(), &ctx, 1);
    let identity = captured.directory_identity();
    assert!(identity == recaptured.directory_identity());
    assert!(identity != foreign.directory_identity());
    assert_eq!(
        identity.routing_key(),
        recaptured.directory_identity().routing_key()
    );
    assert_ne!(
        identity.routing_key(),
        foreign.directory_identity().routing_key()
    );
    let encoded = serde_json::to_vec(&identity).unwrap();
    let restored: NativeDocumentDirectoryIdentity = serde_json::from_slice(&encoded).unwrap();
    assert!(restored == identity);
    let mut identities = std::collections::HashSet::new();
    identities.insert(identity.clone());
    identities.insert(restored);
    assert_eq!(identities.len(), 1);
    assert!(captured.same_directory(&recaptured));
    captured.revalidate(&ctx).unwrap();

    assert_eq!(fs::read_dir(first.path()).unwrap().count(), 0);
    assert_eq!(fs::read_dir(other.path()).unwrap().count(), 0);
}

#[test]
fn directory_identity_rejects_invalid_closed_schema_and_other_os() {
    let invalid = [
        r#"{}"#,
        r#"{"identity":null}"#,
        r#"{"identity":{"device":1,"inode":0}}"#,
        r#"{"identity":{"volume":1,"index":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"created":1}}"#,
        r#"{"identity":{"volume":1,"index":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"created":0}}"#,
        r#"{"identity":{"device":1,"inode":2,"extra":0}}"#,
        r#"{"identity":{"device":1,"inode":2},"extra":0}"#,
        r#"{"identity":{"device":1,"inode":2},"identity":{"device":1,"inode":2}}"#,
        r#"{"identity":{"device":1,"inode":2,"inode":2}}"#,
        r#"{"identity":{"device":"1","inode":2}}"#,
        r#"{"identity":{"device":-1,"inode":2}}"#,
        r#"{"identity":{"device":18446744073709551616,"inode":2}}"#,
        r#"{"identity":{"os":"Other","device":1,"inode":2}}"#,
        r#"{"identity":{"device":1}}"#,
    ];
    for encoded in invalid {
        assert!(serde_json::from_str::<NativeDocumentDirectoryIdentity>(encoded).is_err());
    }
    #[cfg(unix)]
    let foreign =
        r#"{"identity":{"volume":1,"index":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],"created":3}}"#;
    #[cfg(windows)]
    let foreign = r#"{"identity":{"device":1,"inode":2}}"#;
    #[cfg(any(unix, windows))]
    assert!(serde_json::from_str::<NativeDocumentDirectoryIdentity>(foreign).is_err());
}

#[test]
fn directory_identity_routing_is_fixed_injective_and_matches_total_order() {
    let tuples = [
        (0u64, 1u64, 1u64),
        (0, 1, u64::MAX),
        (0, 2, 1),
        (1, 1, 1),
        (1, 256, 1),
        (u64::MAX, u64::MAX, u64::MAX),
    ];
    let mut records = Vec::new();
    for (first, second, native_third) in tuples {
        #[cfg(unix)]
        let (tag, third, encoded) = (
            1u8,
            {
                let _ = native_third;
                0u64
            },
            format!(r#"{{"identity":{{"device":{first},"inode":{second}}}}}"#),
        );
        #[cfg(windows)]
        let index = {
            let mut index = [0u8; 16];
            index[8..16].copy_from_slice(&second.to_be_bytes());
            index
        };
        #[cfg(windows)]
        let (tag, third, encoded) = (
            2u8,
            native_third,
            serde_json::json!({"identity": { "volume":first, "index": index, "created":native_third}}).to_string(),
        );
        // The Unix identity has two integers; Windows also distinguishes creation.
        let record: NativeDocumentDirectoryIdentity = serde_json::from_str(&encoded).unwrap();
        let mut expected = [0u8; 33];
        expected[0] = tag;
        expected[1..9].copy_from_slice(&first.to_be_bytes());
        #[cfg(unix)]
        {
            expected[9..17].copy_from_slice(&second.to_be_bytes());
            let _ = third;
        }
        #[cfg(windows)]
        {
            expected[9..25].copy_from_slice(&index);
            expected[25..33].copy_from_slice(&third.to_be_bytes());
        }
        assert_eq!(record.routing_key(), expected);
        for previous in &records {
            let previous: &NativeDocumentDirectoryIdentity = previous;
            assert_eq!(
                previous.cmp(&record),
                previous.routing_key().cmp(&record.routing_key())
            );
            assert_eq!(
                previous == &record,
                previous.routing_key() == record.routing_key()
            );
        }
        records.push(record);
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::{
        ffi::OsStringExt,
        fs::{PermissionsExt, symlink},
    };

    #[test]
    fn foreign_equal_bytes_and_changed_access_are_not_revalidated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config");
        fs::write(&path, b"same").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let ctx = context();
        let directory = capture(tmp.path(), &ctx, 64);
        let observation = directory.read(OsStr::new("config"), &ctx).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(
            directory
                .revalidate_observation(&observation, &ctx)
                .is_err()
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let retained = fs::File::open(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"same").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            directory
                .revalidate_observation(&observation, &ctx)
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"same");
        drop(retained);
    }

    #[test]
    fn hardlink_symlink_and_readonly_fail_without_reading() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config");
        fs::write(&path, b"value").unwrap();
        fs::hard_link(&path, tmp.path().join("second")).unwrap();
        symlink(&path, tmp.path().join("link")).unwrap();
        let ctx = context();
        let directory = capture(tmp.path(), &ctx, 64);
        assert!(directory.read(OsStr::new("config"), &ctx).is_err());
        assert!(directory.read(OsStr::new("link"), &ctx).is_err());
        fs::remove_file(tmp.path().join("second")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(directory.read(OsStr::new("config"), &ctx).is_err());
    }

    #[test]
    fn alias_retarget_and_replaced_parent_conflict_without_effects() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        fs::write(first.join("config"), b"same").unwrap();
        fs::write(second.join("config"), b"same").unwrap();
        let alias = tmp.path().join("alias");
        symlink(&first, &alias).unwrap();
        let ctx = context();
        let directory = capture(&alias, &ctx, 64);
        let direct = capture(&first, &ctx, 64);
        assert!(directory.same_directory(&direct));
        let original_identity = directory.directory_identity();
        assert!(original_identity == direct.directory_identity());
        assert_eq!(
            original_identity.routing_key(),
            direct.directory_identity().routing_key()
        );
        fs::remove_file(&alias).unwrap();
        symlink(&second, &alias).unwrap();
        let retargeted = capture(&alias, &ctx, 64);
        assert!(original_identity != retargeted.directory_identity());
        assert_ne!(
            original_identity.routing_key(),
            retargeted.directory_identity().routing_key()
        );
        assert!(directory.read(OsStr::new("config"), &ctx).is_err());
        fs::rename(&first, tmp.path().join("moved")).unwrap();
        fs::create_dir(&first).unwrap();
        fs::write(first.join("config"), b"same").unwrap();
        assert!(direct.read(OsStr::new("config"), &ctx).is_err());
        let replaced = capture(&first, &ctx, 64);
        assert!(original_identity != replaced.directory_identity());
        assert_ne!(
            original_identity.routing_key(),
            replaced.directory_identity().routing_key()
        );
        assert!(original_identity == direct.directory_identity());

        assert_eq!(fs::read(second.join("config")).unwrap(), b"same");
    }

    #[test]
    fn non_unicode_directory_and_filename_return_explicit_errors_without_effects() {
        let tmp = tempfile::tempdir().unwrap();
        let name = std::ffi::OsString::from_vec(b"config-\xff".to_vec());
        let root = tmp
            .path()
            .join(std::ffi::OsString::from_vec(b"root-\xfe".to_vec()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join(&name), b"native").unwrap();
        let ctx = context();
        assert!(
            matches!(CapturedDocumentDirectory::capture(&root, limits(64), &ctx), Err(error) if error.kind()==io::ErrorKind::InvalidInput)
        );
        let valid = tmp.path().join("valid");
        fs::create_dir(&valid).unwrap();
        fs::write(valid.join(&name), b"native").unwrap();
        let directory = capture(&valid, &ctx, 64);
        assert!(
            matches!(directory.read(&name, &ctx), Err(error) if error.kind()==io::ErrorKind::InvalidInput)
        );
        assert_eq!(fs::read(root.join(&name)).unwrap(), b"native");
        assert_eq!(fs::read(valid.join(&name)).unwrap(), b"native");
    }
}
