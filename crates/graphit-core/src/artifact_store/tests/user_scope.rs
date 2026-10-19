use super::*;
use crate::installation_identity::InstallationId;
use crate::native_document_file::{
    CapturedUserDocumentDirectory, DirectoryPresence, NativeDocumentFileLimits,
};
use std::sync::Arc;

fn scope_limits() -> ScopeLimits {
    ScopeLimits::new(1, 16 * 1024, 1024).unwrap()
}
fn read_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
fn request(cancellation: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        cancellation,
    ))
}
fn user_store(
    f: &Fixture,
    original: &Arc<SnapshotContext>,
    issuer: &str,
    sub: &str,
) -> ArtifactStore {
    let root = f.source.parent().unwrap();
    let installation = crate::project_paths::InstallationPaths::resolve(
        crate::brand::Brand::default(),
        root,
        None,
        Some(&root.join("global")),
    )
    .unwrap();
    let scope = CapturedPublicationScope::user(
        &installation,
        InstallationId::new("user-fixture").unwrap(),
        UserIdentity::new(issuer.into(), sub.into()).unwrap(),
        scope_limits(),
        original,
    )
    .unwrap();
    ArtifactStore::new(scope, f.store.limits())
}

#[test]
fn literal_pair_is_portable_isolated_data_and_restoration_validates_layout() {
    let f = Fixture::new();
    let original = request(Cancellation::default());
    let a = user_store(&f, &original, "https://issuer/a", "same");
    let b = user_store(&f, &original, "https://issuer/b", "same");
    let c = user_store(&f, &original, "https://issuer/A", "same");
    assert_ne!(a.runtime_root(), b.runtime_root());
    assert_ne!(a.runtime_root(), c.runtime_root());
    let StoreOwner::User { identity, .. } = a.owner() else {
        panic!("USER owner")
    };
    assert_eq!(identity.issuer(), "https://issuer/a");
    assert_eq!(identity.subject(), "same");
    assert!(
        identity
            .namespace_key()
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_ne!(
        UserIdentity::new("ab".into(), "c".into())
            .unwrap()
            .namespace_key(),
        UserIdentity::new("a".into(), "bc".into())
            .unwrap()
            .namespace_key()
    );
    assert!(UserIdentity::new("".into(), "s".into()).is_err());
    let binding = a.scope().binding().unwrap();
    let bytes = serde_json::to_vec(&binding).unwrap();
    let restored = ScopeBindingRecord::pure_restore(&bytes, scope_limits(), 65536).unwrap();
    let context = ScopeContext::from_snapshot(&original);
    assert!(restored.matches_scope_bounded(a.scope(), &context).unwrap());
    assert!(!restored.matches_scope_bounded(b.scope(), &context).unwrap());
    let mut wire = serde_json::to_value(binding).unwrap();
    wire["owner"]["identity"]["subject"] = "other".into();
    assert!(
        ScopeBindingRecord::pure_restore(
            &serde_json::to_vec(&wire).unwrap(),
            scope_limits(),
            65536
        )
        .is_err()
    );
}

#[test]
fn absent_read_never_births_any_namespace_and_project_api_stays_restricted() {
    let f = Fixture::new();
    let original = request(Cancellation::default());
    let store = user_store(&f, &original, "issuer", "sub");
    assert_eq!(
        CapturedUserDocumentDirectory::probe_memory(store.scope(), read_limits(), &original)
            .unwrap(),
        DirectoryPresence::Absent
    );
    assert!(!store.scope().owner_root().join("users").exists());
    assert!(
        crate::native_document_file::CapturedProjectDocumentDirectory::capture(
            store.scope(),
            Path::new(""),
            read_limits(),
            &original
        )
        .is_err()
    );
    assert!(
        CapturedUserDocumentDirectory::capture(
            f.store.scope(),
            Path::new(""),
            read_limits(),
            &original
        )
        .is_err()
    );
    assert!(
        CapturedUserDocumentDirectory::capture(
            store.scope(),
            Path::new("../other"),
            read_limits(),
            &original
        )
        .is_err()
    );
}

#[test]
fn fixed_private_birth_reopens_and_retains_user_original() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let original = request(Cancellation::default());
    let store = user_store(&f, &original, "issuer", "sub");
    let mut writer = store.writer_with_context(&original).unwrap();
    assert!(
        writer
            .create_record_directory(ProjectRecordDirectory::Memory, read_limits())
            .is_err()
    );
    let directory = writer.create_user_memory_directory(read_limits()).unwrap();
    let memory = store.runtime_root().join("memory");
    assert_eq!(
        fs::metadata(&memory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(writer.create_user_memory_directory(read_limits()).is_err());
    fs::write(memory.join("page.json"), b"USER document").unwrap();
    let observation = directory
        .read(std::ffi::OsStr::new("page.json"), &original)
        .unwrap();
    assert_eq!(observation.bytes(), Some(b"USER document".as_slice()));
    directory
        .revalidate_observation(&observation, &original)
        .unwrap();
    assert!(
        directory
            .revalidate(&request(Cancellation::default()))
            .is_err()
    );
    let reopened = CapturedUserDocumentDirectory::capture(
        store.scope(),
        Path::new("runtime/memory"),
        read_limits(),
        &original,
    )
    .unwrap();
    reopened.revalidate(&original).unwrap();
    assert_eq!(
        CapturedUserDocumentDirectory::probe_memory(store.scope(), read_limits(), &original)
            .unwrap(),
        DirectoryPresence::PresentDirectory
    );
    assert!(FileLock::is_locked(store.runtime_root().join("artifact-store.lock")).unwrap());
    drop(writer);
    assert!(!FileLock::is_locked(store.runtime_root().join("artifact-store.lock")).unwrap());
    assert!(!store.runtime_root().join("task").exists());
}

#[test]
fn other_user_is_unreadable_and_replaced_namespace_is_not_absence() {
    let f = Fixture::new();
    let original = request(Cancellation::default());
    let a = user_store(&f, &original, "issuer/a", "same");
    let b = user_store(&f, &original, "issuer/b", "same");
    let mut writer = b.writer_with_context(&original).unwrap();
    let _directory = writer.create_user_memory_directory(read_limits()).unwrap();
    assert_eq!(
        CapturedUserDocumentDirectory::probe_memory(a.scope(), read_limits(), &original).unwrap(),
        DirectoryPresence::Absent
    );
    let fresh = user_store(&f, &original, "issuer/b", "same");
    let root = fresh.scope().user_root().unwrap();
    let retained = root.with_extension("retained");
    fs::rename(root, &retained).unwrap();
    fs::create_dir(root).unwrap();
    assert!(
        CapturedUserDocumentDirectory::probe_memory(fresh.scope(), read_limits(), &original)
            .is_err()
    );
    assert!(fresh.writer_with_context(&original).is_err());
}

#[test]
fn links_and_file_in_namespace_are_errors_without_adoption() {
    use std::os::unix::fs::symlink;
    for link in [false, true] {
        let f = Fixture::new();
        let original = request(Cancellation::default());
        let store = user_store(&f, &original, "issuer", "sub");
        fs::create_dir(store.scope().owner_root().join("users")).unwrap();
        let user = store.scope().user_root().unwrap();
        if link {
            symlink(f.source.parent().unwrap(), user).unwrap();
        } else {
            fs::write(user, b"not a directory").unwrap();
        }
        assert!(
            CapturedUserDocumentDirectory::probe_memory(store.scope(), read_limits(), &original)
                .is_err()
        );
        assert!(store.writer_with_context(&original).is_err());
        assert!(!store.runtime_root().exists());
    }
}

#[test]
fn cancellation_deadline_and_user_lock_contention_preserve_causes() {
    let f = Fixture::new();
    let signal = Cancellation::default();
    let original = request(signal.clone());
    let store = user_store(&f, &original, "issuer", "sub");
    signal.cancel();
    assert_eq!(
        CapturedUserDocumentDirectory::probe_memory(store.scope(), read_limits(), &original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        store.writer_with_context(&original).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!store.runtime_root().exists());
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert_eq!(
        CapturedUserDocumentDirectory::probe_memory(store.scope(), read_limits(), &expired)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    let first = request(Cancellation::default());
    let held = store.writer_with_context(&first).unwrap();
    let cancel = Cancellation::default();
    let second = request(cancel.clone());
    crate::file_lock::BEFORE_WAIT
        .with(|hook| *hook.borrow_mut() = Some(Box::new(move || cancel.cancel())));
    assert_eq!(
        store.writer_with_context(&second).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(FileLock::is_locked(store.runtime_root().join("artifact-store.lock")).unwrap());
    drop(held);
}

#[test]
fn empty_root_capture_reads_without_birth_and_preserves_original_validation() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let original = request(Cancellation::default());
    let store = user_store(&f, &original, "issuer", "sub");
    let error = CapturedUserDocumentDirectory::capture(
        store.scope(),
        Path::new(""),
        read_limits(),
        &original,
    )
    .err()
    .expect("missing USER root");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(!store.scope().owner_root().join("users").exists());

    let installation = crate::project_paths::InstallationPaths::resolve(
        crate::brand::Brand::default(),
        f.source.parent().unwrap(),
        None,
        Some(store.scope().owner_root()),
    )
    .unwrap();
    let installation_scope = CapturedPublicationScope::installation(
        &installation,
        InstallationId::new("user-fixture").unwrap(),
        [f.source.parent().unwrap().to_path_buf()],
        scope_limits(),
        &original,
    )
    .unwrap();
    assert_eq!(
        CapturedUserDocumentDirectory::capture(
            &installation_scope,
            Path::new(""),
            read_limits(),
            &original,
        )
        .err()
        .expect("installation is not USER")
        .kind(),
        io::ErrorKind::InvalidInput
    );

    let writer = store.writer_with_context(&original).unwrap();
    let root = CapturedUserDocumentDirectory::capture(
        store.scope(),
        Path::new(""),
        read_limits(),
        &original,
    )
    .unwrap();
    let absent = root
        .read(std::ffi::OsStr::new("absent.json"), &original)
        .unwrap();
    assert!(absent.bytes().is_none());
    root.revalidate_observation(&absent, &original).unwrap();
    let file = store.scope().user_root().unwrap().join("record.json");
    fs::write(&file, b"private USER root record").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let observed = root
        .read(std::ffi::OsStr::new("record.json"), &original)
        .unwrap();
    assert_eq!(
        observed.bytes(),
        Some(b"private USER root record".as_slice())
    );
    root.revalidate_observation(&observed, &original).unwrap();
    root.revalidate(&original).unwrap();
    let foreign = request(Cancellation::default());
    assert!(
        root.read(std::ffi::OsStr::new("record.json"), &foreign)
            .is_err()
    );
    assert!(root.revalidate(&foreign).is_err());
    for invalid in [
        ".",
        "..",
        "runtime/../memory",
        "runtime//memory",
        "runtime/",
        "/absolute",
        "nul\0name",
    ] {
        assert_eq!(
            CapturedUserDocumentDirectory::capture(
                store.scope(),
                Path::new(invalid),
                read_limits(),
                &original,
            )
            .err()
            .expect("invalid relative path")
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(
        crate::file_io::validate_relative(Path::new("runtime/"), true)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(
        !store
            .scope()
            .user_root()
            .unwrap()
            .join("absent.json")
            .exists()
    );
    assert!(FileLock::is_locked(store.runtime_root().join("artifact-store.lock")).unwrap());
    drop(writer);
}
