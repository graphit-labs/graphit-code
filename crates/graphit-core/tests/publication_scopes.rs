use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactKey, ArtifactLimits, ArtifactMode, ArtifactRecovery, ArtifactStore,
        CapturedPublicationScope, CopyLimits, CopyPolicy, ScopeBindingRecord, ScopeContext,
        ScopeLimits, StoreOwner,
    },
    brand::Brand,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_fs::NodeKind;
use std::{
    fs, io,
    path::PathBuf,
    time::{Duration, Instant},
};

fn context() -> ScopeContext {
    ScopeContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    )
}
fn limits() -> ScopeLimits {
    ScopeLimits::new(4, 16 * 1024, 1024).unwrap()
}
fn artifact_limits() -> ArtifactLimits {
    ArtifactLimits::new(
        128 * 1024,
        1024,
        CopyLimits::new(1024 * 1024, 128, 16 * 1024, 8).unwrap(),
    )
    .unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
    allowed: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let owner = root.join("installation");
        let project_root = root.join("project");
        let allowed = root.join("adapter");
        for path in [&owner, &project_root, &allowed] {
            fs::create_dir(path).unwrap();
        }
        let installation =
            InstallationPaths::resolve(Brand::default(), &root, None, Some(&owner)).unwrap();
        let project = ProjectPaths::at(&project_root, &installation).unwrap();
        let source = root.join("external-source");
        fs::write(&source, b"source").unwrap();
        Self {
            _temp: temp,
            installation,
            project,
            allowed,
            source,
        }
    }
    fn scope(&self, installation: bool) -> CapturedPublicationScope {
        if installation {
            CapturedPublicationScope::installation(
                &self.installation,
                InstallationId::new("fixture-installation").unwrap(),
                [self.allowed.clone()],
                limits(),
                &native_capture_request(),
            )
            .unwrap()
        } else {
            CapturedPublicationScope::project(
                &self.project,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                limits(),
                &native_capture_request(),
            )
            .unwrap()
        }
    }
    fn destination(&self, installation: bool) -> PathBuf {
        (if installation {
            &self.allowed
        } else {
            self.project.root()
        })
        .join("published")
    }
}

#[test]
fn factory_directory_entry_limit_accepts_exact_and_rejects_one_under() {
    let f = Fixture::new();
    let scope = f.scope(true);
    let input_bound = scope
        .binding()
        .unwrap()
        .directory_count()
        .max(scope.runtime_root().components().count())
        .max(f.allowed.components().count());
    let capture = |cap| {
        CapturedPublicationScope::installation(
            &f.installation,
            InstallationId::new("fixture-installation").unwrap(),
            [f.allowed.clone()],
            ScopeLimits::new(1, 16 * 1024, cap).unwrap(),
            &native_capture_request(),
        )
    };
    assert!(capture(input_bound).is_ok());
    assert_eq!(
        capture(input_bound - 1).err().unwrap().kind(),
        io::ErrorKind::FileTooLarge
    );
    assert!(!scope.runtime_root().exists());
}

#[test]
fn factories_are_read_only_and_installation_runtime_is_stable_and_explicit() {
    let f = Fixture::new();
    for installation in [false, true] {
        let scope = f.scope(installation);
        let expected = if installation {
            f.installation.global_path(&["runtime"]).unwrap()
        } else {
            f.project
                .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[])
                .unwrap()
        };
        assert_eq!(scope.runtime_root(), expected);
        assert!(!expected.exists());
        let record = scope.binding().unwrap();
        let bytes = serde_json::to_vec(&record).unwrap();
        let restored = ScopeBindingRecord::pure_restore(&bytes, limits(), bytes.len()).unwrap();
        assert!(restored == record);
        assert!(restored.matches_scope(&scope));
        scope.revalidate(&context()).unwrap();
        let store = ArtifactStore::new(scope, artifact_limits());
        assert!(store.load().unwrap().is_none());
        assert!(!expected.exists());
        if installation {
            assert_eq!(
                store.owner(),
                &StoreOwner::Installation {
                    installation_id: InstallationId::new("fixture-installation").unwrap()
                }
            );
            assert_ne!(
                store.runtime_root(),
                f.installation.framework_runtime("dev-version").unwrap()
            );
        }
    }
    assert_eq!(fs::read(&f.source).unwrap(), b"source");
}

#[test]
fn both_owners_use_one_publisher_for_copy_link_noop_update_and_fresh_reopen() {
    for installation in [false, true] {
        let f = Fixture::new();
        let store = ArtifactStore::new(f.scope(installation), artifact_limits());
        let destination = f.destination(installation);
        let key = ArtifactKey::new("adapter", "command", "fixture").unwrap();
        let copy = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
        {
            let mut writer = store
                .writer(Duration::from_secs(5), &Cancellation::default())
                .unwrap();
            assert_eq!(
                writer
                    .publish(
                        &key,
                        graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                        &destination,
                        NodeKind::File,
                        copy
                    )
                    .unwrap(),
                ArtifactChange::Created
            );
            assert_eq!(
                writer
                    .publish(
                        &key,
                        graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                        &destination,
                        NodeKind::File,
                        copy
                    )
                    .unwrap(),
                ArtifactChange::Unchanged
            );

            assert_eq!(
                writer
                    .publish(
                        &key,
                        graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                        &destination,
                        NodeKind::File,
                        ArtifactMode::Link
                    )
                    .unwrap(),
                ArtifactChange::Updated
            );
            assert_eq!(fs::read(&destination).unwrap(), b"source");
        }
        let reopened = ArtifactStore::new(f.scope(installation), artifact_limits());
        let snapshot = reopened.load().unwrap().unwrap();
        assert_eq!(snapshot.owner(), store.owner());
        assert_eq!(snapshot.entries().len(), 1);
        assert!(snapshot.pending().is_none());
        let mut writer = reopened
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap();
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::NoPending);
        assert_eq!(writer.remove(&key).unwrap(), ArtifactChange::Removed);
        assert!(!destination.exists());
        assert_eq!(fs::read(&f.source).unwrap(), b"source");
    }
}

#[test]
fn outside_roots_runtime_and_owner_root_fail_before_operation_intent() {
    let f = Fixture::new();
    let store = ArtifactStore::new(f.scope(true), artifact_limits());
    let key = ArtifactKey::new("adapter", "command", "fixture").unwrap();
    let mut writer = store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    for destination in [
        f.project.root().join("outside"),
        store.runtime_root().join("illegal"),
        f.installation.root().to_owned(),
        f.allowed.clone(),
    ] {
        assert_eq!(
            writer
                .publish(
                    &key,
                    graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                    &destination,
                    NodeKind::File,
                    ArtifactMode::Link
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );

        assert!(!store.manifest_path().exists());
    }
    assert_eq!(fs::read(&f.source).unwrap(), b"source");
}

#[test]
fn missing_nested_root_cannot_publish_its_ancestor_and_break_future_recovery() {
    let f = Fixture::new();
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::write(f.source.join("asset"), b"preserve source").unwrap();
    let ancestor = f.allowed.join("nested");
    let nested_root = ancestor.join("bound");
    let scope = CapturedPublicationScope::installation(
        &f.installation,
        InstallationId::new("fixture-installation").unwrap(),
        [f.allowed.clone(), nested_root.clone()],
        limits(),
        &native_capture_request(),
    )
    .unwrap();
    let store = ArtifactStore::new(scope, artifact_limits());
    let key = ArtifactKey::new("adapter", "skill", "nested").unwrap();
    let mut writer = store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    for mode in [
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        ArtifactMode::Link,
    ] {
        assert_eq!(
            writer
                .check_publication(
                    &key,
                    graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                    &ancestor,
                    NodeKind::Directory,
                    mode
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            writer
                .publish(
                    &key,
                    graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                    &ancestor,
                    NodeKind::Directory,
                    mode
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );

        assert!(!store.manifest_path().exists());
        assert!(!ancestor.exists());
        assert_eq!(fs::read_dir(&f.allowed).unwrap().count(), 0);
    }
    fs::create_dir_all(&nested_root).unwrap();
    let leaf = nested_root.join("leaf");
    assert_eq!(
        writer
            .publish(
                &key,
                graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                &leaf,
                NodeKind::Directory,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .unwrap(),
        ArtifactChange::Created
    );
    assert_eq!(fs::read(leaf.join("asset")).unwrap(), b"preserve source");
    writer.remove(&key).unwrap();
    store.scope().revalidate(&context()).unwrap();
    assert_eq!(
        fs::read(f.source.join("asset")).unwrap(),
        b"preserve source"
    );
    assert!(nested_root.is_dir());
}

#[test]
fn nested_roots_are_boundaries_and_only_descendants_can_be_published() {
    let f = Fixture::new();
    let nested = f.allowed.join("nested");
    fs::create_dir(&nested).unwrap();
    let capture = |cap| {
        CapturedPublicationScope::installation(
            &f.installation,
            InstallationId::new("fixture-installation").unwrap(),
            [f.allowed.clone(), nested.clone()],
            cap,
            &native_capture_request(),
        )
    };
    assert!(capture(ScopeLimits::new(1, 16 * 1024, 1024).unwrap()).is_err());
    let scope = capture(ScopeLimits::new(2, 16 * 1024, 1024).unwrap()).unwrap();
    let bytes = serde_json::to_vec(&scope.binding().unwrap()).unwrap();
    assert!(ScopeBindingRecord::pure_restore(&bytes, limits(), bytes.len()).is_ok());
    assert!(
        ScopeBindingRecord::pure_restore(
            &bytes,
            ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
            bytes.len()
        )
        .is_err()
    );
    let store = ArtifactStore::new(scope, artifact_limits());
    let key = ArtifactKey::new("adapter", "command", "nested").unwrap();
    let mut writer = store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    for root in [&f.allowed, &nested] {
        assert_eq!(
            writer
                .publish(
                    &key,
                    graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                    root,
                    NodeKind::File,
                    ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );

        assert!(!store.manifest_path().exists());
    }
    let leaf = nested.join("leaf");
    assert_eq!(
        writer
            .publish(
                &key,
                graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
                &leaf,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .unwrap(),
        ArtifactChange::Created
    );
    assert_eq!(fs::read(&leaf).unwrap(), b"source");
    writer.remove(&key).unwrap();
    store.scope().revalidate(&context()).unwrap();
    assert!(nested.is_dir());
}

#[test]
fn no_roots_bad_paths_zero_limits_cancel_and_expired_capture_create_nothing() {
    let f = Fixture::new();
    let capture = |roots: Vec<PathBuf>,
                   ctx: &std::sync::Arc<graphit_core::file_io::SnapshotContext>| {
        CapturedPublicationScope::installation(
            &f.installation,
            InstallationId::new("fixture").unwrap(),
            roots,
            limits(),
            ctx,
        )
    };
    assert_eq!(
        capture(vec![], &native_capture_request())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(capture(vec![PathBuf::from("relative")], &native_capture_request()).is_err());
    assert!(capture(vec![f.allowed.join("../escape")], &native_capture_request()).is_err());
    assert!(
        capture(
            vec![f.allowed.clone(), f.allowed.clone()],
            &native_capture_request()
        )
        .is_err()
    );
    for values in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
        assert!(ScopeLimits::new(values.0, values.1, values.2).is_err());
    }
    let cancelled = native_capture_request();
    cancelled.cancellation().cancel();
    assert_eq!(
        capture(vec![f.allowed.clone()], &cancelled)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let original = std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    ));
    let expired = original;
    std::thread::sleep(
        expired
            .deadline()
            .unwrap()
            .saturating_duration_since(Instant::now()),
    );
    assert_eq!(
        capture(vec![f.allowed.clone()], &expired)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(!f.installation.global_path(&["runtime"]).unwrap().exists());
}

#[test]
fn binding_restore_has_exact_limits_and_rejects_strict_owner_native_and_path_data() {
    let f = Fixture::new();
    let scope = f.scope(true);
    let record = scope.binding().unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let path_bytes = |v: &serde_json::Value| {
        let text = v.as_str().unwrap();
        if cfg!(windows) {
            text.encode_utf16().count() * 2
        } else {
            text.len()
        }
    };
    let paths: Vec<_> = [
        &value["owner_root"],
        &value["storage_root"],
        &value["runtime_root"],
    ]
    .into_iter()
    .chain(value["allowed_roots"].as_array().unwrap().iter())
    .chain(
        value["directories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| &d["path"]),
    )
    .collect();
    let per_path = paths.iter().map(|p| path_bytes(p)).max().unwrap();
    let exact = ScopeLimits::new(1, per_path, record.directory_count()).unwrap();
    assert!(ScopeBindingRecord::pure_restore(&bytes, exact, bytes.len()).is_ok());
    assert!(ScopeBindingRecord::pure_restore(&bytes, exact, bytes.len() - 1).is_err());
    for cap in [
        ScopeLimits::new(1, per_path - 1, record.directory_count()).unwrap(),
        ScopeLimits::new(1, per_path, record.directory_count() - 1).unwrap(),
    ] {
        assert!(ScopeBindingRecord::pure_restore(&bytes, cap, bytes.len()).is_err());
    }
    let mut bad = Vec::new();
    let mut unknown = value.clone();
    unknown["unexpected"] = serde_json::json!(true);
    bad.push(unknown);
    let mut null = value.clone();
    null["owner"] = serde_json::Value::Null;
    bad.push(null);
    let mut id = value.clone();
    id["owner"]["installation_id"] = serde_json::json!("../invalid");
    bad.push(id);
    let mut non_string = value.clone();
    non_string["owner_root"] = serde_json::json!({"os":"Unix","units":[47,112]});
    bad.push(non_string);
    let mut duplicates = value.clone();
    let first = duplicates["allowed_roots"][0].clone();
    duplicates["allowed_roots"]
        .as_array_mut()
        .unwrap()
        .push(first);
    bad.push(duplicates);
    let mut brand = value.clone();
    brand["brand"] = serde_json::json!("other");
    let other_brand = ScopeBindingRecord::pure_restore(
        &serde_json::to_vec(&brand).unwrap(),
        limits(),
        128 * 1024,
    )
    .unwrap();
    assert!(!other_brand.matches_scope(&scope));
    for value in bad {
        assert!(
            ScopeBindingRecord::pure_restore(
                &serde_json::to_vec(&value).unwrap(),
                limits(),
                128 * 1024
            )
            .is_err()
        );
    }
    let text = std::str::from_utf8(&bytes).unwrap();
    let duplicate = text.replace("\"owner\":", "\"own\\u0065r\":null,\"owner\":");
    assert!(ScopeBindingRecord::pure_restore(duplicate.as_bytes(), limits(), 128 * 1024).is_err());
    assert!(
        ScopeBindingRecord::pure_restore(format!("{text} {{}}").as_bytes(), limits(), 128 * 1024)
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn captured_allowed_root_parent_alias_and_runtime_replacements_preserve_foreign_state() {
    use std::os::unix::fs::symlink;
    for runtime in [false, true] {
        let f = Fixture::new();
        let scope = f.scope(true);
        let store = ArtifactStore::new(scope.clone(), artifact_limits());
        if runtime {
            drop(
                store
                    .writer(Duration::from_secs(5), &Cancellation::default())
                    .unwrap(),
            );
        }
        let original = if runtime {
            store.runtime_root().to_owned()
        } else {
            f.allowed.clone()
        };
        let kept = original.with_file_name("kept-original");
        fs::rename(&original, &kept).unwrap();
        let foreign = f.project.root().join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("marker"), b"foreign").unwrap();
        symlink(&foreign, &original).unwrap();
        assert!(scope.revalidate(&context()).is_err());
        assert!(
            store
                .writer(Duration::from_secs(5), &Cancellation::default())
                .is_err()
        );
        assert_eq!(fs::read(foreign.join("marker")).unwrap(), b"foreign");
        assert!(
            fs::symlink_metadata(&original)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!foreign.join("artifact-store.json").exists());
    }
    let f = Fixture::new();
    let parent = f.allowed.parent().unwrap();
    let alias = parent.join("root-alias");
    symlink(&f.allowed, &alias).unwrap();
    assert!(
        CapturedPublicationScope::installation(
            &f.installation,
            InstallationId::new("fixture").unwrap(),
            [alias.join("missing")],
            limits(),
            &native_capture_request()
        )
        .is_err()
    );
}

#[test]
fn missing_allowed_suffix_is_observed_without_creation_and_bound_when_it_appears() {
    let f = Fixture::new();
    let missing = f.allowed.join("missing");
    let scope = CapturedPublicationScope::installation(
        &f.installation,
        InstallationId::new("fixture").unwrap(),
        [missing.clone()],
        limits(),
        &native_capture_request(),
    )
    .unwrap();
    let before = scope.binding().unwrap();
    assert!(!missing.exists());
    fs::create_dir(&missing).unwrap();
    scope.revalidate(&context()).unwrap();
    assert!(before.matches_scope(&scope));
    assert_eq!(
        scope.binding().unwrap().directory_count(),
        before.directory_count() + 1
    );
    let kept = f.allowed.join("kept");
    fs::rename(&missing, &kept).unwrap();
    fs::create_dir(&missing).unwrap();
    assert_eq!(
        scope.revalidate(&context()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(kept.is_dir());
}

#[cfg(unix)]
#[test]
fn unicode_allowed_root_survives_scope_binding_and_real_publication() {
    let f = Fixture::new();
    let root = f.allowed.join("n-界");
    fs::create_dir(&root).unwrap();
    let scope = CapturedPublicationScope::installation(
        &f.installation,
        InstallationId::new("fixture").unwrap(),
        [root.clone()],
        limits(),
        &native_capture_request(),
    )
    .unwrap();
    assert_eq!(scope.allowed_roots().next(), Some(root.as_path()));
    let record = scope.binding().unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    assert!(ScopeBindingRecord::pure_restore(&bytes, limits(), bytes.len()).unwrap() == record);
    let store = ArtifactStore::new(scope, artifact_limits());
    let mut writer = store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    let target = root.join("file");
    writer
        .publish(
            &ArtifactKey::new("adapter", "command", "fixture").unwrap(),
            graphit_core::artifact_store::PhysicalSource::ExternalSource(&f.source),
            &target,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    assert_eq!(fs::read(target).unwrap(), b"source");
}

#[cfg(unix)]
#[test]
fn non_unicode_allowed_root_is_rejected_without_creating_runtime_state() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let f = Fixture::new();
    let root = f.allowed.join(OsString::from_vec(vec![b'n', 0xff]));
    fs::create_dir(&root).unwrap();
    assert!(
        CapturedPublicationScope::installation(
            &f.installation,
            InstallationId::new("fixture").unwrap(),
            [root.clone()],
            limits(),
            &native_capture_request(),
        )
        .is_err()
    );
    assert!(root.is_dir());
    assert!(!f.installation.global_path(&["runtime"]).unwrap().exists());
}

// Scope birth performs metadata work only; payload requests keep their own original context.
fn native_capture_request() -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    ))
}
