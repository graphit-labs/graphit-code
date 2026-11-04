use super::codec::Kind;
use super::*;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Instant;

const ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

struct Fixture {
    store: ArtifactStore,
    source: PathBuf,
    destination: PathBuf,
    // Captured root handles close before the temporary directory is removed.
    _directory: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        Self::with_owner(false)
    }
    fn with_owner(global: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let source = root.join("source 🚀");
        let destination = root.join("adapter/link");
        fs::write(&source, b"source remains").unwrap();
        fs::create_dir(root.join("adapter")).unwrap();
        fs::create_dir(root.join("global")).unwrap();
        let installation = crate::project_paths::InstallationPaths::resolve(
            crate::brand::Brand::default(),
            &root,
            None,
            Some(&root.join("global")),
        )
        .unwrap();
        let project = ProjectPaths::at(&root, &installation).unwrap();
        let store = ArtifactStore::new(
            fixture_scope(&project, &installation, global),
            ArtifactLimits::new(
                1024 * 1024,
                1024,
                CopyLimits::new(64 * 1024 * 1024, 10_000, 1024 * 1024, 64).unwrap(),
            )
            .unwrap(),
        );
        Self {
            _directory: directory,
            store,
            source,
            destination,
        }
    }
    fn writer(&self) -> ArtifactWriter {
        self.writer_with_timeout(Duration::from_secs(5))
    }
    fn writer_with_timeout(&self, timeout: Duration) -> ArtifactWriter {
        self.store
            .writer(timeout, &Cancellation::default())
            .unwrap()
    }
    fn fresh_store(&self) -> ArtifactStore {
        let root = self.source.parent().unwrap();
        let installation = crate::project_paths::InstallationPaths::resolve(
            crate::brand::Brand::default(),
            root,
            None,
            Some(&root.join("global")),
        )
        .unwrap();
        let project = ProjectPaths::at(root, &installation).unwrap();
        ArtifactStore::new(
            fixture_scope(
                &project,
                &installation,
                matches!(self.store.owner(), StoreOwner::Installation { .. }),
            ),
            self.store.limits(),
        )
    }
    fn key(&self) -> ArtifactKey {
        ArtifactKey::new("codex", "command", "org/repo").unwrap()
    }
    fn native_link(&self, destination: &Path, kind: NodeKind) {
        LinkPlan::capture(&self.source, destination, kind)
            .unwrap()
            .create()
            .unwrap();
    }
}

fn fixture_scope(
    project: &ProjectPaths,
    installation: &crate::project_paths::InstallationPaths,
    global: bool,
) -> CapturedPublicationScope {
    let limits = ScopeLimits::new(1, 16 * 1024, 1024).unwrap();
    if global {
        CapturedPublicationScope::installation(
            installation,
            crate::installation_identity::InstallationId::new("fixture-installation").unwrap(),
            [project.root().join("adapter")],
            limits,
            &native_capture_request(),
        )
        .unwrap()
    } else {
        CapturedPublicationScope::project(
            project,
            ID.parse().unwrap(),
            limits,
            &native_capture_request(),
        )
        .unwrap()
    }
}

#[cfg(unix)]
#[test]
fn control_root_accepts_private_existing_runtime_under_public_parent_without_repair() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        let parent = f.store.runtime_root().parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
        let held = graphit_fs::create_private_directory(f.store.runtime_root()).unwrap();
        let identity = crate::native_file_identity::handle_id(&held).unwrap();
        // Public layout is established before the fresh physical factory capture.
        let store = f.fresh_store();
        assert!(store.load().unwrap().is_none());
        assert_eq!(fs::read_dir(f.store.runtime_root()).unwrap().count(), 0);
        let writer = store
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap();
        assert!(writer.load().unwrap().is_none());
        assert_eq!(fs::metadata(parent).unwrap().mode() & 0o7777, 0o755);
        assert!(crate::native_file_identity::handle_id(&held).unwrap() == identity);
        graphit_fs::verify_private_directory(&held).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn control_root_rejects_permissive_existing_runtime_for_load_and_writer_without_repair() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        fs::create_dir_all(f.store.runtime_root()).unwrap();
        fs::set_permissions(f.store.runtime_root(), fs::Permissions::from_mode(0o755)).unwrap();
        let sentinel = f.store.runtime_root().join("foreign");
        fs::write(&sentinel, b"preserve").unwrap();
        assert_eq!(
            f.store.load().unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            f.store
                .writer(Duration::from_secs(1), &Cancellation::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            fs::metadata(f.store.runtime_root()).unwrap().mode() & 0o7777,
            0o755
        );
        assert_eq!(fs::read(sentinel).unwrap(), b"preserve");
        assert!(!f.store.runtime_root().join("artifact-store.lock").exists());
        assert!(!f.store.manifest_path().exists());
    }
}

#[test]
fn control_root_missing_read_has_no_effect_and_writer_creates_private_boundary() {
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        assert!(f.store.load().unwrap().is_none());
        assert!(!f.store.runtime_root().exists());
        let writer = f.writer();
        let held =
            crate::native_document_file::open_access_directory(f.store.runtime_root()).unwrap();
        graphit_fs::verify_private_directory(&held).unwrap();
        assert!(writer.load().unwrap().is_none());
        assert!(!f.store.manifest_path().exists());
    }
}

#[cfg(unix)]
#[test]
fn control_root_observed_access_change_blocks_held_writer_without_repair() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let f = Fixture::new();
    let writer = f.writer();
    fs::set_permissions(f.store.runtime_root(), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(writer.load().unwrap_err().stage(), ArtifactStage::Read);
    assert_eq!(
        fs::metadata(f.store.runtime_root()).unwrap().mode() & 0o7777,
        0o755
    );
    assert!(!f.store.manifest_path().exists());
}

#[cfg(unix)]
#[test]
fn control_root_retained_writer_rejects_replaced_private_namespace() {
    let f = Fixture::new();
    let writer = f.writer();
    let moved = f.store.runtime_root().with_file_name("original-runtime");
    fs::rename(f.store.runtime_root(), &moved).unwrap();
    let _foreign = graphit_fs::create_private_directory(f.store.runtime_root()).unwrap();
    assert_eq!(writer.load().unwrap_err().stage(), ArtifactStage::Read);
    assert!(moved.join("artifact-store.lock").exists());
    assert_eq!(fs::read_dir(f.store.runtime_root()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn control_root_disappearance_before_lock_does_not_recreate_parent() {
    let f = Fixture::new();
    let runtime = f.store.runtime_root().to_owned();
    let moved = runtime.with_file_name("original-runtime");
    let original = runtime.clone();
    let retained = moved.clone();
    control::test_hooks::BEFORE_LOCK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || fs::rename(original, retained).unwrap()));
    });
    let error = f
        .store
        .writer(Duration::from_secs(1), &Cancellation::default())
        .unwrap_err();
    assert_eq!(error.stage(), ArtifactStage::Lock);
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(!runtime.exists());
    assert!(moved.is_dir());
    assert!(!moved.join("artifact-store.lock").exists());
}

#[cfg(unix)]
#[test]
fn control_root_revalidates_after_failed_read_and_preserves_primary_and_secondary_errors() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let writer = f.writer();
    fs::create_dir(f.store.manifest_path()).unwrap();
    let runtime = f.store.runtime_root().to_owned();
    control::test_hooks::AFTER_READ.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            fs::set_permissions(runtime, fs::Permissions::from_mode(0o755)).unwrap();
        }));
    });
    let error = writer.load().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(error.secondary_failure(), Some(io::ErrorKind::InvalidData));

    assert!(f.store.manifest_path().is_dir());
}

#[cfg(unix)]
#[test]
fn control_root_rejects_save_before_effect_when_privacy_changed() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let writer = f.writer();
    let document = Document::empty(&f.store).unwrap();
    fs::write(f.store.manifest_path(), b"preserve").unwrap();
    fs::set_permissions(f.store.runtime_root(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(writer.save(&document, ArtifactStage::RecordProof).is_err());
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), b"preserve");
}

#[cfg(unix)]
#[test]
fn control_root_revalidates_after_failed_save_without_losing_primary_error_or_counting_write() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let writer = f.writer();
    let document = Document::empty(&f.store).unwrap();
    fs::write(f.store.manifest_path(), b"preserve").unwrap();
    fs::set_permissions(f.store.manifest_path(), fs::Permissions::from_mode(0o400)).unwrap();
    let runtime = f.store.runtime_root().to_owned();
    control::test_hooks::AFTER_SAVE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            fs::set_permissions(runtime, fs::Permissions::from_mode(0o755)).unwrap();
        }));
    });
    let error = writer
        .save(&document, ArtifactStage::RecordProof)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(error.secondary_failure(), Some(io::ErrorKind::InvalidData));
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), b"preserve");
}

fn catalog_scratch_paths(store: &ArtifactStore) -> Vec<PathBuf> {
    fs::read_dir(store.runtime_root())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".artifact-store-")
        })
        .collect()
}

fn catalog_file_id(store: &ArtifactStore) -> crate::native_file_identity::PhysicalId {
    crate::native_file_identity::physical_id(
        &store.manifest_path(),
        &fs::symlink_metadata(store.manifest_path()).unwrap(),
    )
    .unwrap()
}
fn catalog_json(document: &Document) -> serde_json::Value {
    serde_json::from_slice(&codec::encode(document, 1024 * 1024).unwrap()).unwrap()
}

#[test]
fn catalog_reader_missing_and_invalid_caps_have_no_namespace_effects() {
    let mut f = Fixture::new();
    assert!(!f.store.runtime_root().exists());
    assert!(f.store.load().unwrap().is_none());
    assert!(!f.store.runtime_root().exists());
    for cap in [0, usize::MAX] {
        f.store.limits.max_document_bytes = cap;
        assert_eq!(
            f.store.load().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!f.store.runtime_root().exists());
    }
}

#[test]
fn catalog_reader_closed_document_rejects_missing_duplicate_and_unknown_fields() {
    let f = Fixture::new();
    let writer = f.writer();
    let document = Document::empty(&f.store).unwrap();
    writer.save(&document, ArtifactStage::RecordProof).unwrap();
    let good = fs::read(f.store.manifest_path()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&good).unwrap();
    assert!(value.get("document").is_none());
    assert!(value.get("physical_id").is_none());
    let mut unknown = value.clone();
    unknown["unknown"] = serde_json::json!(true);
    let mut missing_scope = value.clone();
    missing_scope.as_object_mut().unwrap().remove("scope");
    let mut missing_entries = value.clone();
    missing_entries.as_object_mut().unwrap().remove("entries");
    let mut missing_pending = value.clone();
    missing_pending.as_object_mut().unwrap().remove("pending");
    for malformed in [
        serde_json::to_vec(&unknown).unwrap(),
        serde_json::to_vec(&missing_scope).unwrap(),
        serde_json::to_vec(&missing_entries).unwrap(),
        serde_json::to_vec(&missing_pending).unwrap(),
        format!(r#"{{"scope":{},"scope":{},"entries":[],"pending":null}}"#, value["scope"], value["scope"]).into_bytes(),
        String::from_utf8(good.clone()).unwrap().replacen("\"entries\":", "\"entries\":[],\"entries\":", 1).into_bytes(),
        serde_json::to_vec(&serde_json::json!({"physical_id":crate::native_file_identity::PhysicalId::largest(),"document":value})).unwrap(),
        [good.as_slice(), b" false"].concat(),
    ] {
        fs::write(f.store.manifest_path(), &malformed).unwrap();
        assert_eq!(writer.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), malformed);
        assert!(catalog_scratch_paths(&f.store).is_empty());
    }
    fs::write(f.store.manifest_path(), good).unwrap();
    assert!(writer.load().unwrap().is_some());
}

#[test]
fn catalog_reader_accepts_complete_private_replacement_before_a_new_capture() {
    use std::io::Write;
    let f = Fixture::new();
    let writer = f.writer();
    writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap();
    let bytes = fs::read(f.store.manifest_path()).unwrap();
    let old_id = catalog_file_id(&f.store);
    let replacement = f.store.runtime_root().join("foreign-catalog");
    let mut file = graphit_fs::create_private_file(&replacement).unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
    drop(file);
    #[cfg(unix)]
    fs::rename(&replacement, f.store.manifest_path()).unwrap();
    #[cfg(windows)]
    graphit_fs::replace_existing_file(&replacement, &f.store.manifest_path()).unwrap();
    assert!(catalog_file_id(&f.store) != old_id);
    // The current private file is captured afresh. No stale live baseline is
    // being reused; scopes and publication receipts still validate normally.
    assert!(writer.load().unwrap().is_some());
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), bytes);
    writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap();
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), bytes);

    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[cfg(unix)]
#[test]
fn catalog_reader_permissive_readonly_and_hardlinked_files_are_not_repaired() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o644, 0o400] {
        let f = Fixture::new();
        let writer = f.writer();
        writer
            .save(
                &Document::empty(&f.store).unwrap(),
                ArtifactStage::RecordProof,
            )
            .unwrap();
        let bytes = fs::read(f.store.manifest_path()).unwrap();
        fs::set_permissions(f.store.manifest_path(), fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            writer.load().unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            fs::metadata(f.store.manifest_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            mode
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), bytes);
    }
    let f = Fixture::new();
    let writer = f.writer();
    writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap();
    let alias = f.store.runtime_root().join("foreign-hardlink");
    fs::hard_link(f.store.manifest_path(), &alias).unwrap();
    assert!(writer.load().is_err());
    assert_eq!(
        fs::read(&alias).unwrap(),
        fs::read(f.store.manifest_path()).unwrap()
    );
    assert!(alias.exists());
}

#[cfg(unix)]
#[test]
fn catalog_reader_detects_same_bytes_path_replacement_during_held_fd_read() {
    use std::io::Write;
    let f = Fixture::new();
    let writer = f.writer();
    writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap();
    let original = fs::read(f.store.manifest_path()).unwrap();
    let displaced = f.store.runtime_root().join("displaced-catalog");
    let moved = displaced.clone();
    catalog::test_hooks::AFTER_READ.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |path| {
            let bytes = fs::read(path).unwrap();
            fs::rename(path, &moved).unwrap();
            let mut foreign = graphit_fs::create_private_file(path).unwrap();
            foreign.write_all(&bytes).unwrap();
        }));
    });
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), original);
    assert_eq!(fs::read(displaced).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn catalog_reader_symlink_and_access_change_during_read_preserve_foreign_state() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let writer = f.writer();
    writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap();
    let before = fs::read(f.store.manifest_path()).unwrap();
    catalog::test_hooks::AFTER_READ.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(|path| {
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
        }));
    });
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        fs::metadata(f.store.manifest_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    let displaced = f.store.runtime_root().join("foreign-target");
    fs::rename(f.store.manifest_path(), &displaced).unwrap();
    symlink(&displaced, f.store.manifest_path()).unwrap();
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(
        fs::symlink_metadata(f.store.manifest_path())
            .unwrap()
            .is_symlink()
    );
    assert_eq!(fs::read(displaced).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn catalog_reader_invalid_cap_precedes_writer_root_io() {
    let f = Fixture::new();
    let mut writer = f.writer();
    writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap();
    let root = f.store.runtime_root();
    let displaced = root.with_extension("displaced");
    fs::rename(root, &displaced).unwrap();
    let original = fs::read(displaced.join("artifact-store.json")).unwrap();
    for limit in [0, usize::MAX] {
        writer.store.limits.max_document_bytes = limit;
        let error = writer.load().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.secondary_failure().is_none());
        assert_eq!(
            writer.document().err().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!root.exists());
        assert_eq!(
            fs::read(displaced.join("artifact-store.json")).unwrap(),
            original
        );
    }
}

#[test]
fn catalog_sdk_first_save_and_replacement_have_private_native_files() {
    use crate::native_file_identity::{handle_id, physical_id};
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        let writer = f.writer();
        let document = Document::empty(&f.store).unwrap();
        writer.save(&document, ArtifactStage::RecordProof).unwrap();
        let file = crate::native_document_file::open_file(&f.store.manifest_path()).unwrap();
        graphit_fs::verify_private_file(&file).unwrap();
        let old = handle_id(&file).unwrap();
        drop(file);
        let created = std::rc::Rc::new(std::cell::RefCell::new(None));
        let captured = created.clone();
        catalog::test_hooks::AFTER_BIRTH.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |path| {
                assert_eq!(fs::metadata(path).unwrap().len(), 0);
                *captured.borrow_mut() =
                    Some(physical_id(path, &fs::symlink_metadata(path).unwrap()).unwrap());
            }));
        });
        writer.save(&document, ArtifactStage::RecordProof).unwrap();
        let file = crate::native_document_file::open_file(&f.store.manifest_path()).unwrap();
        graphit_fs::verify_private_file(&file).unwrap();
        let actual = handle_id(&file).unwrap();
        let created = created.borrow();
        let created = created.as_ref().unwrap();
        #[cfg(unix)]
        assert!(actual == *created && actual != old);
        #[cfg(windows)]
        {
            use crate::native_file_identity::PhysicalId;
            let (
                PhysicalId::Windows {
                    volume,
                    index,
                    created: actual_time,
                },
                PhysicalId::Windows {
                    volume: stage_volume,
                    index: stage_index,
                    ..
                },
                PhysicalId::Windows {
                    created: old_time, ..
                },
            ) = (&actual, created, &old)
            else {
                panic!()
            };
            assert_eq!(volume, stage_volume);
            assert_eq!(index, stage_index);
            assert_eq!(actual_time, old_time);
            assert!(actual != old);
        }
        assert_eq!(
            fs::read(f.store.manifest_path()).unwrap(),
            codec::encode(&document, f.store.limits.max_document_bytes).unwrap()
        );

        assert!(catalog_scratch_paths(&f.store).is_empty());
        assert!(writer.load().unwrap().is_some());
    }
}

#[cfg(unix)]
#[test]
fn catalog_sdk_rejects_permissive_readonly_symlink_and_hardlinked_existing_without_repair() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    for kind in ["permissive", "readonly", "symlink", "hardlink"] {
        let f = Fixture::new();
        let writer = f.writer();
        let document = Document::empty(&f.store).unwrap();
        writer.save(&document, ArtifactStage::RecordProof).unwrap();
        let path = f.store.manifest_path();
        let old = fs::read(&path).unwrap();
        match kind {
            "permissive" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            "readonly" => fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap(),
            "symlink" => {
                fs::rename(&path, path.with_extension("retained")).unwrap();
                symlink(path.with_extension("retained"), &path).unwrap();
            }
            "hardlink" => fs::hard_link(&path, path.with_extension("alias")).unwrap(),
            _ => unreachable!(),
        }
        let before = fs::symlink_metadata(&path).unwrap();
        assert!(writer.save(&document, ArtifactStage::RecordProof).is_err());
        assert_eq!(fs::read(&path).unwrap(), old);
        let after = fs::symlink_metadata(&path).unwrap();
        assert_eq!(before.ino(), after.ino());
        assert_eq!(before.mode(), after.mode());
        assert_eq!(before.nlink(), after.nlink());

        assert!(catalog_scratch_paths(&f.store).is_empty());
    }
}

#[test]
fn catalog_sdk_missing_collision_preserves_foreign_bytes_and_removes_only_owned_scratch() {
    use std::io::Write;
    let f = Fixture::new();
    let writer = f.writer();
    let destination = f.store.manifest_path();
    catalog::test_hooks::BEFORE_NATIVE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |_| {
            graphit_fs::create_private_file(&destination)
                .unwrap()
                .write_all(b"foreign creator")
                .unwrap();
        }));
    });
    let error = writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(error.secondary_failure(), Some(io::ErrorKind::InvalidData));
    assert_eq!(
        fs::read(f.store.manifest_path()).unwrap(),
        b"foreign creator"
    );

    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[test]
fn catalog_sdk_create_return_error_and_collision_never_authorize_cleanup() {
    use std::io::Write;
    for collision in [false, true] {
        let f = Fixture::new();
        let writer = f.writer();
        catalog::test_hooks::CREATE.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |path| {
                let mut file = graphit_fs::create_private_file(path)?;
                file.write_all(b"preserve residue")?;
                drop(file);
                if collision {
                    graphit_fs::create_private_file(path)
                } else {
                    Err(io::ErrorKind::Other.into())
                }
            }));
        });
        let error = writer
            .save(
                &Document::empty(&f.store).unwrap(),
                ArtifactStage::RecordProof,
            )
            .unwrap_err();
        assert_eq!(
            error.kind(),
            if collision {
                io::ErrorKind::AlreadyExists
            } else {
                io::ErrorKind::Other
            }
        );
        let scratch = catalog_scratch_paths(&f.store);
        assert_eq!(scratch.len(), 1);
        assert_eq!(fs::read(&scratch[0]).unwrap(), b"preserve residue");
        assert!(!f.store.manifest_path().exists());
    }
}

#[test]
fn catalog_sdk_write_error_cleans_only_successfully_created_and_bound_scratch() {
    let f = Fixture::new();
    let writer = f.writer();
    catalog::test_hooks::CREATE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |path| {
            let file = graphit_fs::create_private_file(path)?;
            drop(file);
            // Deterministic descriptor fault: write_all must fail on this read-only FD.
            crate::native_document_file::open_file(path)
        }));
    });
    assert!(
        writer
            .save(
                &Document::empty(&f.store).unwrap(),
                ArtifactStage::RecordProof
            )
            .is_err()
    );

    assert!(!f.store.manifest_path().exists());
    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[cfg(unix)]
#[test]
fn catalog_sdk_scratch_substitution_before_bytes_has_no_foreign_cleanup_authority() {
    use std::io::Write;
    let f = Fixture::new();
    let writer = f.writer();
    catalog::test_hooks::AFTER_BIRTH.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |path| {
            fs::rename(path, path.with_extension("original")).unwrap();
            graphit_fs::create_private_file(path)
                .unwrap()
                .write_all(b"foreign")
                .unwrap();
        }));
    });
    let error = writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.secondary_failure(), Some(io::ErrorKind::InvalidData));
    let paths = catalog_scratch_paths(&f.store);
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().any(|p| fs::read(p).unwrap() == b"foreign"));
    assert!(paths.iter().any(|p| fs::read(p).unwrap().is_empty()));
    assert!(!f.store.manifest_path().exists());
}

#[cfg(unix)]
#[test]
fn catalog_sdk_root_retarget_after_birth_preserves_empty_residue_and_writes_no_secret_bytes() {
    let f = Fixture::new();
    let writer = f.writer();
    let runtime = f.store.runtime_root().to_owned();
    let retained = runtime.with_file_name("retained-runtime");
    let moved = retained.clone();
    catalog::test_hooks::AFTER_BIRTH.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |_| {
            fs::rename(&runtime, &moved).unwrap();
            graphit_fs::create_private_directory(&runtime).unwrap();
        }));
    });
    let error = writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.secondary_failure(), Some(io::ErrorKind::InvalidData));
    assert_eq!(fs::read_dir(f.store.runtime_root()).unwrap().count(), 0);
    let scratch = fs::read_dir(retained)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".artifact-store-")
        })
        .unwrap();
    assert_eq!(fs::metadata(scratch).unwrap().len(), 0);
}

#[cfg(unix)]
#[test]
fn catalog_sdk_native_success_counts_even_when_observer_and_root_fail() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let writer = f.writer();
    let runtime = f.store.runtime_root().to_owned();
    catalog::test_hooks::AFTER_NATIVE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |_| {
            fs::set_permissions(runtime, fs::Permissions::from_mode(0o755)).unwrap()
        }));
    });
    let error = writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.secondary_failure(), Some(io::ErrorKind::InvalidData));

    assert!(f.store.manifest_path().is_file());
}

#[test]
fn catalog_sdk_native_error_after_real_effect_is_observed_without_counting_as_success() {
    let f = Fixture::new();
    let writer = f.writer();
    catalog::test_hooks::NATIVE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |stage, destination, exists| {
            assert!(!exists);
            graphit_fs::rename_exclusive(stage, destination)?;
            Err(io::ErrorKind::Interrupted.into())
        }));
    });
    let error = writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(error.secondary_failure(), None);

    assert!(writer.load().unwrap().is_some());
    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[test]
fn catalog_sdk_native_error_without_effect_observes_old_state_and_cleans_scratch() {
    let f = Fixture::new();
    let writer = f.writer();
    let document = Document::empty(&f.store).unwrap();
    writer.save(&document, ArtifactStage::RecordProof).unwrap();
    let old = fs::read(f.store.manifest_path()).unwrap();
    catalog::test_hooks::NATIVE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |_, _, exists| {
            assert!(exists);
            Err(io::ErrorKind::PermissionDenied.into())
        }));
    });
    let error = writer
        .save(&document, ArtifactStage::RecordProof)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(error.secondary_failure(), None);

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), old);
    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[test]
fn catalog_sdk_cap_zero_overflow_and_exact_length_are_checked_before_effects() {
    let f = Fixture::new();
    let mut writer = f.writer();
    for invalid in [0, usize::MAX] {
        writer.store.limits.max_document_bytes = invalid;
        let error = catalog::save(
            &writer,
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(fs::read_dir(f.store.runtime_root()).unwrap().count(), 1);
    }
    let document = Document::empty(&f.store).unwrap();
    let bytes = codec::encode(&document, f.store.limits.max_document_bytes).unwrap();
    writer.store.limits.max_document_bytes = bytes.len();
    writer.save(&document, ArtifactStage::RecordProof).unwrap();
    writer.save(&document, ArtifactStage::RecordProof).unwrap();

    let actual_bytes = fs::read(f.store.manifest_path()).unwrap();
    let decoded = codec::decode(&actual_bytes, &writer.store).unwrap();
    assert!(decoded.scope == document.scope);
    assert!(decoded.entries.is_empty());
    assert!(decoded.pending.is_none());
    assert!(actual_bytes.len() <= bytes.len());
}

#[test]
fn catalog_sdk_sync_error_keeps_old_catalog_and_cleans_only_new_scratch() {
    let f = Fixture::new();
    let writer = f.writer();
    let document = Document::empty(&f.store).unwrap();
    writer.save(&document, ArtifactStage::RecordProof).unwrap();
    let old = fs::read(f.store.manifest_path()).unwrap();
    catalog::test_hooks::SYNC.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |file| {
            assert!(file.metadata()?.len() > 0);
            Err(io::ErrorKind::Other.into())
        }));
    });
    let error = writer
        .save(&document, ArtifactStage::RecordProof)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(error.secondary_failure(), None);

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), old);
    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[test]
fn catalog_sdk_existing_change_before_effect_preserves_external_bytes() {
    let f = Fixture::new();
    let writer = f.writer();
    let document = Document::empty(&f.store).unwrap();
    writer.save(&document, ArtifactStage::RecordProof).unwrap();
    let destination = f.store.manifest_path();
    catalog::test_hooks::AFTER_CLOSE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |_| {
            fs::write(destination, b"external replacement bytes").unwrap()
        }));
    });
    let error = writer
        .save(&document, ArtifactStage::RecordProof)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);

    assert_eq!(
        fs::read(f.store.manifest_path()).unwrap(),
        b"external replacement bytes"
    );
    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[test]
fn catalog_sdk_native_success_with_valid_root_still_reports_observer_bytes_conflict() {
    let f = Fixture::new();
    let writer = f.writer();
    let destination = f.store.manifest_path();
    catalog::test_hooks::AFTER_NATIVE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |_| {
            fs::write(destination, b"external post-effect bytes").unwrap()
        }));
    });
    let error = writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.secondary_failure(), None);

    writer
        .control
        .revalidate(&writer.store.scope, None)
        .unwrap();
    assert_eq!(
        fs::read(f.store.manifest_path()).unwrap(),
        b"external post-effect bytes"
    );
    assert!(catalog_scratch_paths(&f.store).is_empty());
}

#[cfg(unix)]
#[test]
fn catalog_sdk_same_bytes_scratch_swap_after_comparison_is_rejected_without_cleanup_grant() {
    use std::io::Write;
    let f = Fixture::new();
    let writer = f.writer();
    catalog::test_hooks::AFTER_COMPARE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |path| {
            let original = path.with_extension("original");
            fs::rename(path, &original).unwrap();
            let bytes = fs::read(&original).unwrap();
            graphit_fs::create_private_file(path)
                .unwrap()
                .write_all(&bytes)
                .unwrap();
        }));
    });
    let error = writer
        .save(
            &Document::empty(&f.store).unwrap(),
            ArtifactStage::RecordProof,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.secondary_failure(), Some(io::ErrorKind::InvalidData));

    assert!(!f.store.manifest_path().exists());
    let paths = catalog_scratch_paths(&f.store);
    assert_eq!(paths.len(), 2);
    assert_eq!(fs::read(&paths[0]).unwrap(), fs::read(&paths[1]).unwrap());
}

#[test]
fn scope_deadline_and_cancellation_after_capture_prevent_first_intent() {
    for global in [false, true] {
        for cancelled in [false, true] {
            let f = Fixture::with_owner(global);
            let mut writer = f.writer();
            let deadline = Instant::now() + Duration::from_secs(1);
            writer.scope_context = ScopeContext::new(deadline, writer.cancellation.clone());
            let cancellation = writer.cancellation.clone();
            *writer.before_prepare.borrow_mut() = Some(Box::new(move || {
                if cancelled {
                    cancellation.cancel();
                } else {
                    thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(1),
                    );
                }
            }));
            let error = copy_fixture(&mut writer, &f, NodeKind::File).unwrap_err();
            assert_eq!(error.stage(), ArtifactStage::Prepare);
            assert_eq!(
                error.kind(),
                if cancelled {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::TimedOut
                }
            );

            assert!(!f.store.manifest_path().exists());
            assert_eq!(
                fs::read_dir(f.destination.parent().unwrap())
                    .unwrap()
                    .count(),
                0
            );
            assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
        }
    }
}

#[test]
fn pending_installation_scope_change_blocks_recovery_and_preserves_foreign_content() {
    let f = Fixture::with_owner(true);
    let mut writer = f.writer();
    writer.fail_at.set(Some(ArtifactStage::Commit));
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::File)
            .unwrap_err()
            .stage(),
        ArtifactStage::Commit
    );
    let before = fs::read(f.store.manifest_path()).unwrap();
    drop(writer);
    let root = f.source.parent().unwrap();
    for (brand, id) in [
        ("graphit", "wrong-installation"),
        ("other", "fixture-installation"),
    ] {
        let installation = crate::project_paths::InstallationPaths::resolve(
            crate::brand::Brand::new(brand).unwrap(),
            root,
            None,
            Some(&root.join("global")),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &installation,
            crate::installation_identity::InstallationId::new(id).unwrap(),
            [f.destination.parent().unwrap().to_owned()],
            ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
            &native_capture_request(),
        )
        .unwrap();
        let foreign_owner = ArtifactStore::new(scope, f.store.limits());
        assert!(foreign_owner.load().is_err());
        assert!(
            foreign_owner
                .writer(Duration::from_secs(5), &Cancellation::default())
                .unwrap()
                .recover()
                .is_err()
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
    }
    let allowed = f.destination.parent().unwrap();
    let original = allowed.with_file_name("original-adapter");
    fs::rename(allowed, &original).unwrap();
    fs::create_dir(allowed).unwrap();
    fs::write(&f.destination, b"foreign destination").unwrap();
    assert!(f.store.load().is_err());
    let fresh = f.fresh_store();
    assert!(fresh.load().is_err());
    let mut reopened = fresh
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    assert!(reopened.recover().is_err());
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    assert_eq!(fs::read(&f.destination).unwrap(), b"foreign destination");
    assert_eq!(
        fs::read(original.join(f.destination.file_name().unwrap())).unwrap(),
        b"source remains"
    );
    assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
}

#[test]
fn scope_deadline_after_effect_does_not_interrupt_durable_reconciliation() {
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        let mut writer = f.writer();
        writer.fail_at.set(Some(ArtifactStage::Commit));
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::File)
                .unwrap_err()
                .stage(),
            ArtifactStage::Commit
        );
        assert!(writer.load().unwrap().unwrap().pending().is_some());
        writer.scope_context = ScopeContext::new(
            Instant::now() - Duration::from_secs(1),
            Cancellation::default(),
        );
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedCreate);
        assert!(writer.load().unwrap().unwrap().pending().is_none());
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        drop(writer);
        let fresh = f.fresh_store();
        fresh
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .remove(&f.key())
            .unwrap();
    }
}

#[test]
fn native_membership_reopen_idempotence_and_broken_source_unlink() {
    let f = Fixture::new();
    assert!(f.store.load().unwrap().is_none());
    assert!(!f.store.runtime().exists());
    let mut writer = f.writer();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap(),
        ArtifactChange::Created
    );
    let snapshot = writer.load().unwrap().unwrap();
    assert_eq!(snapshot.entries()[0].key().id(), "org/repo");
    assert_eq!(snapshot.entries()[0].source(), f.source);
    assert_eq!(snapshot.entries()[0].destination(), f.destination);
    assert_eq!(
        snapshot.owner(),
        &StoreOwner::Project {
            project_id: ID.parse().unwrap()
        }
    );
    assert!(snapshot.pending().is_none());
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
    let before = fs::read(f.store.manifest_path()).unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap(),
        ArtifactChange::Unchanged
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    assert!(
        !f.store
            .runtime_root()
            .parent()
            .unwrap()
            .join("project.json")
            .exists()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.store.manifest_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(f.store.runtime())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    drop(writer);
    let mut writer = f.writer();
    fs::remove_file(&f.source).unwrap();
    assert_eq!(writer.remove(&f.key()).unwrap(), ArtifactChange::Removed);
    assert!(fs::symlink_metadata(&f.destination).is_err());
    assert!(writer.load().unwrap().unwrap().entries().is_empty());
}

#[test]
fn idempotence_revalidates_source_kind_and_canonical_input_aliases() {
    let f = Fixture::new();
    let mut writer = f.writer();
    let aliased_source = f
        .source
        .parent()
        .unwrap()
        .join("adapter/..")
        .join(f.source.file_name().unwrap());
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&aliased_source),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&aliased_source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap(),
        ArtifactChange::Unchanged
    );
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::write(f.source.join("keep"), b"changed source type").unwrap();
    let before = fs::read(f.store.manifest_path()).unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Validate
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    writer.remove(&f.key()).unwrap();
    assert_eq!(
        fs::read(f.source.join("keep")).unwrap(),
        b"changed source type"
    );
}

#[cfg(windows)]
#[test]
fn windows_codec_rejects_unpaired_utf16_and_preserves_unicode_target_equivalence() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let invalid = PathBuf::from(std::ffi::OsString::from_wide(&[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0xd800,
    ]));
    assert!(NativePath::new(invalid.clone()).is_err());
    assert!(serde_json::to_vec(&NativePath(invalid)).is_err());
    let units = "C:\\界😀".encode_utf16().collect::<Vec<_>>();
    let native = NativePath(PathBuf::from(std::ffi::OsString::from_wide(&units)));
    let encoded = serde_json::to_vec(&native).unwrap();
    let decoded: NativePath = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        decoded.0.as_os_str().encode_wide().collect::<Vec<_>>(),
        units
    );
    let mut prefixed: Vec<_> = "\\\\?\\".encode_utf16().collect();
    prefixed.extend(&units);
    let candidate = Candidate {
        key: ArtifactKey::new("codex", "skill", "org/repo").unwrap(),
        slot: PublicationSlot::Scope,
        source: NativePath(PathBuf::from(std::ffi::OsString::from_wide(&prefixed))),
        generated_origin: None,
        destination: NativePath(PathBuf::from("C:\\destination")),
        kind: Kind::Directory,
    };
    let mut receipt = receipt::Receipt {
        stamp: receipt::Stamp::Windows {
            volume: 1,
            index: [1; 16],
            created: 1,
        },
        target: candidate.source.clone(),
    };
    receipt.target = native;
    receipt.validate(&candidate).unwrap();
}

#[cfg(unix)]
#[test]
fn non_unicode_source_rejected_without_publishing_or_modifying_input() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let f = Fixture::new();
    let source = f
        .source
        .with_file_name(OsString::from_vec(b"source-\xff".to_vec()));
    fs::rename(&f.source, &source).unwrap();
    let mut writer = f.writer();
    assert!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .is_err()
    );
    assert_eq!(fs::read(source).unwrap(), b"source remains");
    assert!(!f.destination.exists());
    assert!(
        writer
            .load()
            .unwrap()
            .is_none_or(|state| state.entries().is_empty())
    );
}

#[test]
fn directory_links_preserve_source_and_explicit_relink_preserves_conflicts() {
    let f = Fixture::new();
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::write(f.source.join("keep"), b"untouched").unwrap();
    let replacement = f.store.scope.owner_root().join("replacement");
    fs::create_dir(&replacement).unwrap();
    fs::write(replacement.join("keep"), b"new").unwrap();
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&f.source),
            &f.destination,
            NodeKind::Directory,
            ArtifactMode::Link,
        )
        .unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&replacement),
                &f.destination,
                NodeKind::Directory,
                ArtifactMode::Link
            )
            .unwrap(),
        ArtifactChange::Updated
    );
    assert_eq!(fs::read(f.destination.join("keep")).unwrap(), b"new");
    fs::write(replacement.join("keep"), b"live").unwrap();
    assert_eq!(fs::read(f.destination.join("keep")).unwrap(), b"live");
    fs::write(replacement.join("keep"), b"new").unwrap();
    writer.remove(&f.key()).unwrap();
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&replacement),
            &f.destination,
            NodeKind::Directory,
            ArtifactMode::Link,
        )
        .unwrap();
    assert_eq!(fs::read(f.destination.join("keep")).unwrap(), b"new");
    writer.remove(&f.key()).unwrap();
    assert_eq!(fs::read(f.source.join("keep")).unwrap(), b"untouched");
    assert_eq!(fs::read(replacement.join("keep")).unwrap(), b"new");
    fs::remove_dir_all(&replacement).unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&replacement),
                &f.destination,
                NodeKind::Directory,
                ArtifactMode::Link
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert!(writer.load().unwrap().unwrap().entries().is_empty());
}

#[test]
fn replacement_even_with_same_target_is_foreign_and_forget_never_touches_it() {
    for same_target in [false, true] {
        let f = Fixture::new();
        let mut writer = f.writer();
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link,
            )
            .unwrap();
        // Keep the old inode alive; the test never relies on allocator reuse.
        fs::rename(&f.destination, f.destination.with_extension("old")).unwrap();
        if same_target {
            f.native_link(&f.destination, NodeKind::File);
        } else {
            fs::write(&f.destination, b"user content").unwrap();
        }
        let bytes = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            writer.remove(&f.key()).unwrap_err().stage(),
            ArtifactStage::Conflict
        );
        assert_eq!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link
                )
                .unwrap_err()
                .stage(),
            ArtifactStage::Conflict
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), bytes);
        let content = fs::read(&f.destination).unwrap();
        let abandoned = writer.forget(&f.key()).unwrap();
        assert_eq!(abandoned.entry().unwrap().destination(), f.destination);
        assert_eq!(fs::read(&f.destination).unwrap(), content);
        assert_eq!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(writer.load().unwrap().unwrap().entries().is_empty());
    }
}

#[test]
fn phase_failures_reload_same_writer_and_reopen_without_inventing_membership() {
    for global in [false, true] {
        for stage in [
            ArtifactStage::Prepare,
            ArtifactStage::Create,
            ArtifactStage::Observe,
            ArtifactStage::RecordProof,
            ArtifactStage::Commit,
        ] {
            let f = Fixture::with_owner(global);
            let mut writer = f.writer();
            writer.fail_at.set(Some(stage));
            assert_eq!(
                writer
                    .publish(
                        &f.key(),
                        PhysicalSource::ExternalSource(&f.source),
                        &f.destination,
                        NodeKind::File,
                        ArtifactMode::Link
                    )
                    .unwrap_err()
                    .stage(),
                stage
            );
            let state = writer.load().unwrap();
            assert!(state.as_ref().is_none_or(|s| s.entries().is_empty()));
            if stage == ArtifactStage::Prepare {
                assert!(state.is_none());
            } else {
                let phase = if stage == ArtifactStage::Commit {
                    ArtifactPending::Ready
                } else {
                    ArtifactPending::Building
                };
                assert_eq!(
                    state.as_ref().unwrap().pending().map(ArtifactIntent::phase),
                    Some(phase)
                );
                assert_eq!(
                    writer
                        .publish(
                            &f.key(),
                            PhysicalSource::ExternalSource(&f.source),
                            &f.destination,
                            NodeKind::File,
                            ArtifactMode::Link
                        )
                        .unwrap_err()
                        .stage(),
                    ArtifactStage::RecoveryRequired
                );
            }
            drop(writer);
            let mut writer = f.writer();
            match stage {
                ArtifactStage::Prepare => {
                    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::NoPending)
                }
                ArtifactStage::Create => {
                    assert_eq!(
                        writer.recover().unwrap(),
                        ArtifactRecovery::AbortedPublication
                    )
                }
                ArtifactStage::Commit => {
                    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedCreate);
                    assert_eq!(writer.load().unwrap().unwrap().entries().len(), 1);
                }
                ArtifactStage::Observe | ArtifactStage::RecordProof => {
                    assert_eq!(
                        writer.recover().unwrap(),
                        ArtifactRecovery::AbortedPublication
                    );
                    let document = writer.document().unwrap();
                    assert!(document.pending.is_none());
                    assert_eq!(document.abandoned_publications.len(), 1);
                    assert!(
                        document.abandoned_publications[0]
                            .operation
                            .stage
                            .path
                            .0
                            .exists()
                    );
                    assert!(fs::symlink_metadata(&f.destination).is_err());
                }
                _ => unreachable!(),
            }
            assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
        }
    }
}

#[test]
fn remove_failures_preserve_committed_entry_and_recover_only_known_intent() {
    for global in [false, true] {
        for stage in [
            ArtifactStage::Prepare,
            ArtifactStage::Detach,
            ArtifactStage::Commit,
        ] {
            let f = Fixture::with_owner(global);
            let mut writer = f.writer();
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link,
                )
                .unwrap();
            writer.fail_at.set(Some(stage));
            assert_eq!(writer.remove(&f.key()).unwrap_err().stage(), stage);
            let same_writer = writer.load().unwrap().unwrap();
            assert_eq!(same_writer.entries().len(), 1);
            assert_eq!(
                same_writer.pending().map(ArtifactIntent::phase),
                if stage == ArtifactStage::Prepare {
                    None
                } else {
                    Some(ArtifactPending::Ready)
                }
            );
            drop(writer);
            let mut writer = f.writer();
            if stage == ArtifactStage::Prepare {
                writer.remove(&f.key()).unwrap();
            } else {
                assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedRemove);
            }
            assert!(writer.load().unwrap().unwrap().entries().is_empty());
            assert!(fs::symlink_metadata(&f.destination).is_err());
            assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
        }
    }
}

#[test]
fn recovery_receipt_conflicts_preserve_replacements_and_unknown_absence() {
    for pending_remove in [false, true] {
        let f = Fixture::new();
        let mut writer = f.writer();
        if pending_remove {
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link,
                )
                .unwrap();
            writer.fail_at.set(Some(ArtifactStage::Detach));
            writer.remove(&f.key()).unwrap_err();
        } else {
            writer.fail_at.set(Some(ArtifactStage::Commit));
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link,
                )
                .unwrap_err();
        }
        let physical = if pending_remove {
            writer
                .load()
                .unwrap()
                .unwrap()
                .pending()
                .unwrap()
                .publication()
                .unwrap()
                .backup()
                .unwrap()
        } else {
            f.destination.clone()
        };
        fs::rename(&physical, f.destination.with_extension("old")).unwrap();
        fs::write(&physical, b"foreign").unwrap();
        let before = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert_eq!(fs::read(&physical).unwrap(), b"foreign");
        fs::remove_file(&physical).unwrap();
        // A missing FROM and TO cannot be attributed to the recorded movement.
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        writer.forget(&f.key()).unwrap();
    }
}

#[test]
fn reopened_pending_intent_identifies_metadata_only_forget() {
    for pending_remove in [false, true] {
        let f = Fixture::new();
        let other_key = ArtifactKey::new("codex", "command", "other/repo").unwrap();
        let other_destination = f.destination.with_extension("other");
        let mut writer = f.writer();
        writer
            .publish(
                &other_key,
                PhysicalSource::ExternalSource(&f.source),
                &other_destination,
                NodeKind::File,
                ArtifactMode::Link,
            )
            .unwrap();
        if pending_remove {
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link,
                )
                .unwrap();
            writer.fail_at.set(Some(ArtifactStage::Detach));
            writer.remove(&f.key()).unwrap_err();
        } else {
            writer.fail_at.set(Some(ArtifactStage::Observe));
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link,
                )
                .unwrap_err();
        }
        drop(writer);
        let mut writer = f.writer();
        let snapshot = writer.load().unwrap().unwrap();
        let intent = snapshot.pending().unwrap().publication().unwrap();
        assert_eq!(intent.key().id(), "org/repo");
        assert_eq!(intent.source(), f.source);
        assert_eq!(intent.destination(), f.destination);
        assert_eq!(intent.kind(), NodeKind::File);
        assert_eq!(
            intent.phase(),
            if pending_remove {
                ArtifactPending::Ready
            } else {
                ArtifactPending::Building
            }
        );
        let debug = format!("{intent:?}");
        assert!(!debug.contains("org/repo"));
        assert!(!debug.contains("source"));
        assert!(!debug.contains("adapter"));
        let before = fs::read(f.store.manifest_path()).unwrap();
        if pending_remove {
            let backup = intent.backup().unwrap();
            fs::rename(&backup, f.destination.with_extension("old")).unwrap();
            fs::write(&backup, b"foreign").unwrap();
        }
        // For Building without a receipt, recover now archives and clears pending.
        // Exercise explicit metadata-only forget directly, before that resolution.
        if pending_remove {
            assert_eq!(
                writer.recover().unwrap_err().stage(),
                ArtifactStage::RecoveryConflict
            );
        }
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        let destination_before = fs::read(intent.destination()).ok();
        let backup_before = fs::read(intent.backup().unwrap()).ok();
        let abandoned = writer.forget(intent.key()).unwrap();
        assert_eq!(
            abandoned
                .intent()
                .unwrap()
                .publication()
                .unwrap()
                .destination(),
            intent.destination()
        );
        assert_eq!(fs::read(intent.destination()).ok(), destination_before);
        assert_eq!(fs::read(intent.backup().unwrap()).ok(), backup_before);
        let after = writer.load().unwrap().unwrap();
        assert!(after.pending().is_none());
        assert_eq!(after.entries().len(), 1);
        assert_eq!(after.entries()[0].key(), &other_key);
        assert_eq!(fs::read(other_destination).unwrap(), b"source remains");
        assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
    }
}

#[test]
fn strict_documents_bind_project_pending_entry_keys_paths_and_receipts() {
    let f = Fixture::new();
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&f.source),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    let original = writer.document().unwrap();
    let mut documents = Vec::new();
    let mut d = original.clone();
    d.entries.push(d.entries[0].clone());
    documents.push(d);
    let mut d = original.clone();
    d.pending = Some(Pending::Publication(Box::new(
        publication::Operation::new(
            d.entries[0].candidate.clone(),
            ArtifactMode::Link,
            None,
            publication::Goal::Create,
        )
        .unwrap(),
    )));
    documents.push(d);
    let mut d = original.clone();
    let mut entry = d.entries[0].clone();
    entry.candidate.key.id = "other".into();
    d.pending = Some(Pending::Publication(Box::new(
        publication::Operation::new(
            entry.candidate.clone(),
            entry.mode(),
            Some(entry),
            publication::Goal::Remove,
        )
        .unwrap(),
    )));
    documents.push(d);
    let mut d = original.clone();
    d.entries[0].candidate.destination = NativePath(f.store.runtime().join("forbidden"));
    documents.push(d);
    for d in documents {
        fs::write(
            f.store.manifest_path(),
            codec::encode(&d, 1024 * 1024).unwrap(),
        )
        .unwrap();
        assert_eq!(
            writer.load().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
    }
    let mut wrong_owner = catalog_json(&original);
    wrong_owner["scope"]["owner"]["project_id"] = serde_json::json!("01ARZ3NDEKTSV4RRFFQ69G5FAW");
    fs::write(
        f.store.manifest_path(),
        serde_json::to_vec(&wrong_owner).unwrap(),
    )
    .unwrap();
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let mut value = catalog_json(&original);
    value["unexpected"] = serde_json::json!(true);
    fs::write(f.store.manifest_path(), serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let mut value = catalog_json(&original);
    value["entries"][0]["candidate"]["source"] = serde_json::json!({"os":"Unix","units":[47,112]});
    fs::write(f.store.manifest_path(), serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let mut value = catalog_json(&original);
    value["entries"][0]["candidate"]["source"] = serde_json::json!("/nul\0source");
    fs::write(f.store.manifest_path(), serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(
        f.store.manifest_path(),
        codec::encode(&original, 1024 * 1024).unwrap(),
    )
    .unwrap();
    assert_eq!(writer.load().unwrap().unwrap().entries().len(), 1);
}

#[test]
fn scope_collisions_caps_and_redacted_errors_have_no_artifact_effects() {
    let f = Fixture::new();
    let mut writer = f.writer();
    for destination in [
        f.store.runtime().join("illegal"),
        f.store.scope.owner_root().parent().unwrap().join("outside"),
        f.destination.join(".."),
        PathBuf::from("relative"),
    ] {
        assert!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &destination,
                    NodeKind::File,
                    ArtifactMode::Link
                )
                .is_err()
        );
    }
    assert!(!f.store.manifest_path().exists());
    for directory in [false, true] {
        if directory {
            fs::create_dir(&f.destination).unwrap();
        } else {
            fs::write(&f.destination, b"foreign").unwrap();
        }
        assert_eq!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        if directory {
            fs::remove_dir(&f.destination).unwrap();
        } else {
            assert_eq!(fs::read(&f.destination).unwrap(), b"foreign");
            fs::remove_file(&f.destination).unwrap();
        }
    }
    let e = writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&f.source.with_extension("absent-private")),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap_err();
    assert!(!format!("{e:?} {e}").contains("absent-private"));
    assert!(!format!("{e:?} {e}").contains(f.store.scope.owner_root().to_str().unwrap()));
    writer.store.limits.max_key_bytes = 1;
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Validate
    );
    writer.store.limits.max_key_bytes = 1024;
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&f.source),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    let other = ArtifactKey::new("codex", "agent", "org/repo").unwrap();
    assert_eq!(
        writer
            .publish(
                &other,
                PhysicalSource::ExternalSource(&f.source),
                &f.destination.with_extension("other"),
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap(),
        ArtifactChange::Created
    );
    assert!(writer.load().unwrap().unwrap().pending().is_none());
    assert!(f.destination.with_extension("other").exists());
    assert!(ArtifactLimits::new(0, 1, CopyLimits::new(1, 1, 1, 1).unwrap()).is_err());
    assert!(ArtifactKey::new("codex", "command", "\n").is_err());
}

#[cfg(unix)]
#[test]
fn aliases_unicode_and_complete_ctime_are_preserved_conservatively() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let native = f.source.with_file_name("native-界");
    fs::rename(&f.source, &native).unwrap();
    let alias = f.source.with_extension("alias");
    symlink(&native, &alias).unwrap();
    let parent_alias = f.store.scope.owner_root().join("parent-alias");
    symlink(f.destination.parent().unwrap(), &parent_alias).unwrap();
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&alias),
            &parent_alias.join("link"),
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    assert_eq!(
        writer.load().unwrap().unwrap().entries()[0].source(),
        native
    );
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
    // Link metadata itself changes; source metadata does not establish ownership.
    let Proof::Link {
        receipt:
            receipt::Receipt {
                stamp:
                    receipt::Stamp::Unix {
                        change_seconds,
                        change_nanos,
                        ..
                    },
                ..
            },
    } = &writer.document().unwrap().entries[0].receipt
    else {
        unreachable!()
    };
    assert!(*change_seconds > 0);
    assert!(*change_nanos < 1_000_000_000);
    fs::remove_file(&f.destination).unwrap();
    symlink(&native, &f.destination).unwrap();
    // ctime seconds+nanos are part of the observation even if an inode is reused.
    assert!(writer.remove(&f.key()).is_err());
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
    fs::set_permissions(&native, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn writer_lock_serializes_threads_and_cancel_timeout_never_publish() {
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        let writer = f.writer();
        assert_eq!(
            f.store
                .writer(Duration::from_millis(10), &Cancellation::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        let cancel = Cancellation::default();
        cancel.cancel();
        assert_eq!(
            f.store
                .writer(Duration::from_secs(1), &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!f.store.manifest_path().exists());
        drop(writer);
        thread::scope(|scope| {
            let mut workers = Vec::new();
            for id in ["one", "two"] {
                let store = &f.store;
                let source = &f.source;
                let destination = f.destination.with_file_name(id);
                workers.push(scope.spawn(move || {
                    let mut writer = store
                        .writer(Duration::from_secs(5), &Cancellation::default())
                        .unwrap();
                    writer
                        .publish(
                            &ArtifactKey::new("codex", "command", id).unwrap(),
                            PhysicalSource::ExternalSource(source),
                            &destination,
                            NodeKind::File,
                            ArtifactMode::Link,
                        )
                        .unwrap();
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        assert_eq!(f.store.load().unwrap().unwrap().entries().len(), 2);
    }
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn child(root: &Path, id: &str, crash: bool) -> OwnedChild {
    child_with_owner(root, id, crash, false)
}
fn child_with_owner(root: &Path, id: &str, crash: bool, global: bool) -> OwnedChild {
    OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "artifact_store::tests::writer_child",
                "--nocapture",
            ])
            .env("GRAPHIT_LINK_TEST_ROOT", root)
            .env("GRAPHIT_LINK_TEST_ID", id)
            .env("GRAPHIT_LINK_TEST_CRASH", if crash { "1" } else { "0" })
            .env(
                "GRAPHIT_LINK_TEST_INSTALLATION",
                if global { "1" } else { "0" },
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
#[test]
fn writer_child() {
    let Some(root) = std::env::var_os("GRAPHIT_LINK_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let installation = crate::project_paths::InstallationPaths::resolve(
        crate::brand::Brand::default(),
        &root,
        None,
        Some(&root.join("global")),
    )
    .unwrap();
    let project = ProjectPaths::at(&root, &installation).unwrap();
    let store = ArtifactStore::new(
        fixture_scope(
            &project,
            &installation,
            std::env::var("GRAPHIT_LINK_TEST_INSTALLATION").unwrap() == "1",
        ),
        ArtifactLimits::new(
            1024 * 1024,
            1024,
            CopyLimits::new(64 * 1024 * 1024, 10_000, 1024 * 1024, 64).unwrap(),
        )
        .unwrap(),
    );
    let mut writer = store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    let id = std::env::var("GRAPHIT_LINK_TEST_ID").unwrap();
    let crash = std::env::var("GRAPHIT_LINK_TEST_CRASH").unwrap() == "1";
    if let Some(phase) = id.strip_prefix("native-stage-kill-") {
        native_stage_tests::crash_native_operation(writer, &root, phase);
        return;
    }
    if crash {
        writer.fail_at.set(Some(ArtifactStage::Commit));
    }
    let key = ArtifactKey::new("codex", "command", &id).unwrap();
    let source = root.join("source 🚀");
    let destination = root.join("adapter").join(&id);
    let result = if id.starts_with("copy-") {
        writer.publish(
            &key,
            PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
    } else {
        writer.publish(
            &key,
            PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
    };
    if crash {
        assert_eq!(result.unwrap_err().stage(), ArtifactStage::Commit);
        fs::write(root.join("ready"), b"locked").unwrap();
        loop {
            thread::park();
        }
    } else {
        result.unwrap();
    }
}
#[test]
fn independent_processes_do_not_lose_membership_and_death_releases_pending_writer() {
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        let root = f.source.parent().unwrap();
        let mut one = child_with_owner(root, "process-one", false, global);
        let mut two = child_with_owner(root, "process-two", false, global);
        assert!(one.0.wait().unwrap().success());
        assert!(two.0.wait().unwrap().success());
        assert_eq!(f.store.load().unwrap().unwrap().entries().len(), 2);
        let mut interrupted = child_with_owner(root, "interrupted", true, global);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("ready").exists() {
            assert!(
                Instant::now() < deadline,
                "child did not record its pending receipt"
            );
            assert!(
                interrupted.0.try_wait().unwrap().is_none(),
                "child failed before ready"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            f.store
                .writer(Duration::from_millis(10), &Cancellation::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        interrupted.0.kill().unwrap();
        interrupted.0.wait().unwrap();
        let mut writer = f.writer();
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedCreate);
        assert_eq!(writer.load().unwrap().unwrap().entries().len(), 3);
        assert_eq!(
            fs::read(root.join("adapter/interrupted")).unwrap(),
            b"source remains"
        );
    }
}

fn directory_source(f: &Fixture) {
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::create_dir(f.source.join("empty 🚀")).unwrap();
    fs::create_dir(f.source.join("nested")).unwrap();
    fs::write(f.source.join("nested/binary"), [0, 255, 1, 0, 37]).unwrap();
    fs::write(f.source.join("empty-file"), []).unwrap();
}
fn copy_fixture(
    writer: &mut ArtifactWriter,
    f: &Fixture,
    kind: NodeKind,
) -> ArtifactResult<ArtifactChange> {
    writer.publish(
        &f.key(),
        PhysicalSource::ExternalSource(&f.source),
        &f.destination,
        kind,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    )
}

#[test]
fn noncanonical_manifest_relatives_reject_recovery_without_effects_or_import() {
    fn corrupt(
        value: &mut serde_json::Value,
        canonical: &serde_json::Value,
        alias: &serde_json::Value,
    ) -> usize {
        match value {
            serde_json::Value::Object(fields) => {
                let mut changed = 0;
                if let Some(relative) = fields.get_mut("relative")
                    && relative == canonical
                {
                    *relative = alias.clone();
                    changed += 1;
                }
                changed
                    + fields
                        .values_mut()
                        .map(|v| corrupt(v, canonical, alias))
                        .sum::<usize>()
            }
            serde_json::Value::Array(items) => {
                items.iter_mut().map(|v| corrupt(v, canonical, alias)).sum()
            }
            _ => 0,
        }
    }
    for global in [false, true] {
        for pending in [false, true] {
            let f = Fixture::with_owner(global);
            directory_source(&f);
            let mut writer = f.writer();
            if pending {
                writer.fail_at.set(Some(ArtifactStage::Commit));
                assert_eq!(
                    copy_fixture(&mut writer, &f, NodeKind::Directory)
                        .unwrap_err()
                        .stage(),
                    ArtifactStage::Commit
                );
            } else {
                copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
            }
            let canonical_bytes = fs::read(f.store.manifest_path()).unwrap();
            let canonical =
                serde_json::to_value(tree::RelativePath(PathBuf::from("nested/binary"))).unwrap();
            for spelling in ["nested/./binary", "nested//binary", "nested/binary/"] {
                let alias = serde_json::json!(spelling);
                let mut malformed: serde_json::Value =
                    serde_json::from_slice(&canonical_bytes).unwrap();
                assert!(corrupt(&mut malformed, &canonical, &alias) > 0);
                let malformed_bytes = serde_json::to_vec(&malformed).unwrap();
                fs::write(f.store.manifest_path(), &malformed_bytes).unwrap();
                assert_eq!(
                    writer.load().unwrap_err().kind(),
                    io::ErrorKind::InvalidData
                );
                assert!(writer.recover().is_err());

                assert_eq!(fs::read(f.store.manifest_path()).unwrap(), malformed_bytes);
                assert_eq!(
                    fs::read(f.destination.join("nested/binary")).unwrap(),
                    [0, 255, 1, 0, 37]
                );
                assert_eq!(
                    fs::read(f.source.join("nested/binary")).unwrap(),
                    [0, 255, 1, 0, 37]
                );
                assert!(f.destination.join("empty 🚀").is_dir());
            }
            fs::write(f.store.manifest_path(), canonical_bytes).unwrap();
            writer.recover().unwrap();
            writer.remove(&f.key()).unwrap();
            assert!(!f.destination.exists());
            assert_eq!(
                fs::read(f.source.join("nested/binary")).unwrap(),
                [0, 255, 1, 0, 37]
            );
        }
    }
}

#[test]
fn copy_file_and_directory_reopen_true_noop_update_remove() {
    for directory in [false, true] {
        let f = Fixture::new();
        let kind = if directory {
            directory_source(&f);
            NodeKind::Directory
        } else {
            NodeKind::File
        };
        let mut writer = f.writer();
        assert_eq!(
            copy_fixture(&mut writer, &f, kind).unwrap(),
            ArtifactChange::Created
        );
        assert!(
            !fs::symlink_metadata(&f.destination)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let before = writer.document().unwrap();
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        let Proof::Copy { tree, .. } = &before.entries[0].receipt else {
            panic!("copy proof");
        };
        let root = &tree.nodes[0];
        assert!(
            root.id != tree::physical_id(&f.source, &fs::metadata(&f.source).unwrap()).unwrap()
        );
        if directory {
            assert!(f.destination.join("empty 🚀").is_dir());
            assert_eq!(
                fs::read(f.destination.join("nested/binary")).unwrap(),
                [0, 255, 1, 0, 37]
            );
        } else {
            assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        }
        assert_eq!(
            copy_fixture(&mut writer, &f, kind).unwrap(),
            ArtifactChange::Unchanged
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        assert!(writer.document().unwrap().entries[0] == before.entries[0]);
        let changed = if directory {
            f.source.join("nested/binary")
        } else {
            f.source.clone()
        };
        fs::write(&changed, b"new bytes").unwrap();
        assert_eq!(
            copy_fixture(&mut writer, &f, kind).unwrap(),
            ArtifactChange::Updated
        );
        let target = if directory {
            f.destination.join("nested/binary")
        } else {
            f.destination.clone()
        };
        assert_eq!(fs::read(target).unwrap(), b"new bytes");
        assert!(writer.load().unwrap().unwrap().pending().is_none());
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(writer.remove(&f.key()).unwrap(), ArtifactChange::Removed);
        assert!(!f.destination.exists());
        assert_eq!(fs::read(changed).unwrap(), b"new bytes");
        assert!(writer.load().unwrap().unwrap().entries().is_empty());
    }
}

#[test]
fn copy_rebind_only_same_owned_destination_without_content_rewrite() {
    let f = Fixture::new();
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    let old = writer.document().unwrap().entries.remove(0);
    let other = f.source.with_extension("other");
    fs::write(&other, b"source remains").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&other, fs::metadata(&f.source).unwrap().permissions()).unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&other),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::FollowRegularFiles)
            )
            .unwrap(),
        ArtifactChange::Updated
    );
    let new = writer.document().unwrap().entries.remove(0);
    let (Proof::Copy { tree: a, .. }, Proof::Copy { tree: b, .. }) = (&old.receipt, &new.receipt)
    else {
        panic!("proofs");
    };
    assert!(a == b);
    assert_eq!(new.source(), other);
    let foreign = f.destination.with_extension("foreign");
    fs::write(&foreign, b"source remains").unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&other),
                &foreign,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Conflict
    );
    assert_eq!(fs::read(foreign).unwrap(), b"source remains");
}

#[test]
fn copy_foreign_replacement_content_and_extra_nodes_never_adopted_or_removed() {
    for mutation in 0..3 {
        let f = Fixture::new();
        directory_source(&f);
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
        let path = f.destination.join("nested/binary");
        match mutation {
            0 => {
                fs::rename(&path, f.destination.with_extension("old-inode")).unwrap();
                fs::write(&path, [0, 255, 1, 0, 37]).unwrap();
            }
            1 => {
                fs::write(&path, b"changed").unwrap();
            }
            _ => {
                fs::write(f.destination.join("user-extra"), b"user").unwrap();
            }
        }
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            writer.remove(&f.key()).unwrap_err().stage(),
            ArtifactStage::Conflict
        );
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::Directory)
                .unwrap_err()
                .stage(),
            ArtifactStage::Conflict
        );
        let abandoned = writer.forget(&f.key()).unwrap();
        assert_eq!(abandoned.entry().unwrap().destination(), f.destination);
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(f.destination.join("empty 🚀").exists());
    }
}

#[test]
fn copy_caps_and_overlap_fail_before_staging_or_destination_effects() {
    assert!(CopyLimits::new(0, 1, 1, 1).is_err());
    assert!(CopyLimits::new(1, 0, 1, 1).is_err());
    assert!(CopyLimits::new(1, 1, 0, 1).is_err());
    assert!(CopyLimits::new(1, 1, 1, 257).is_err());
    for limits in [
        CopyLimits::new(1, 100, 1000, 64).unwrap(),
        CopyLimits::new(1000, 2, 1000, 64).unwrap(),
        CopyLimits::new(1000, 100, 1, 64).unwrap(),
        CopyLimits::new(1000, 100, 1000, 1).unwrap(),
    ] {
        let f = Fixture::new();
        directory_source(&f);
        let mut writer = f.writer();
        writer.store.limits.copy = limits;
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::Directory)
                .unwrap_err()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert!(!f.destination.exists());
        assert!(!f.store.manifest_path().exists());
        assert_eq!(
            fs::read_dir(f.destination.parent().unwrap())
                .unwrap()
                .count(),
            0
        );
    }
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    let nested = f.source.join("copy");
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &nested,
                NodeKind::Directory,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Validate
    );
    assert!(!nested.exists());
}

#[test]
fn copy_build_fault_cleans_proven_partial_and_preserves_old_update() {
    for directory in [false, true] {
        let f = Fixture::new();
        let kind = if directory {
            directory_source(&f);
            NodeKind::Directory
        } else {
            NodeKind::File
        };
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, kind).unwrap();
        let old = writer.document().unwrap().entries.remove(0);
        let changed = if directory {
            f.source.join("nested/binary")
        } else {
            f.source.clone()
        };
        fs::write(changed, b"larger new content").unwrap();
        writer.fail_at.set(Some(ArtifactStage::CopyChunk));
        assert!(copy_fixture(&mut writer, &f, kind).is_err());
        assert!(writer.document().unwrap().entries[0] == old);
        assert!(writer.document().unwrap().pending.is_none());
        writer.remove(&f.key()).unwrap();
    }
}

#[test]
fn copy_rename_before_journal_crashes_recover_same_writer_and_reopen() {
    for global in [false, true] {
        for stage in [
            ArtifactStage::Publish,
            ArtifactStage::Detach,
            ArtifactStage::Commit,
        ] {
            for reopen in [false, true] {
                let f = Fixture::with_owner(global);
                let mut writer = f.writer();
                if stage != ArtifactStage::Publish {
                    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
                    fs::write(&f.source, b"updated").unwrap();
                }
                writer.fail_at.set(Some(stage));
                assert!(copy_fixture(&mut writer, &f, NodeKind::File).is_err());
                assert!(writer.load().unwrap().unwrap().pending().is_some());
                if reopen {
                    drop(writer);
                    writer = f.writer();
                }
                let outcome = writer.recover().unwrap();
                assert!(matches!(
                    outcome,
                    ArtifactRecovery::CompletedCreate | ArtifactRecovery::CompletedUpdate
                ));
                assert!(writer.load().unwrap().unwrap().pending().is_none());
                assert_eq!(
                    fs::read(&f.destination).unwrap(),
                    fs::read(&f.source).unwrap()
                );
                writer.remove(&f.key()).unwrap();
            }
        }
    }
}

#[test]
fn copy_cleanup_fault_keeps_committed_new_membership_until_recovery() {
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    fs::write(f.source.join("nested/binary"), b"updated").unwrap();
    writer.fail_at.set(Some(ArtifactStage::Cleanup));
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::Directory)
            .unwrap_err()
            .stage(),
        ArtifactStage::CleanupRequired
    );
    let snapshot = writer.load().unwrap().unwrap();
    let intent = snapshot.pending().unwrap().publication().unwrap();
    assert_eq!(intent.phase(), ArtifactPending::Cleanup);
    assert_eq!(
        fs::read(f.destination.join("nested/binary")).unwrap(),
        b"updated"
    );
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::Directory)
            .unwrap_err()
            .stage(),
        ArtifactStage::RecoveryRequired
    );
    drop(writer);
    let mut writer = f.writer();
    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
    assert!(!intent.staging_directory().unwrap().exists());
    writer.remove(&f.key()).unwrap();
}

#[test]
fn copy_building_without_proof_abandons_intent_and_preserves_stage() {
    for stage in [
        ArtifactStage::Create,
        ArtifactStage::Observe,
        ArtifactStage::RecordProof,
    ] {
        let f = Fixture::new();
        let mut writer = f.writer();
        writer.fail_at.set(Some(stage));
        assert!(copy_fixture(&mut writer, &f, NodeKind::File).is_err());
        let snapshot = writer.load().unwrap().unwrap();
        let intent = snapshot.pending().unwrap().publication().unwrap();
        let staged = intent.staging_directory().unwrap().to_owned();
        drop(writer);
        let mut writer = f.writer();
        if stage == ArtifactStage::Create {
            assert_eq!(
                writer.recover().unwrap(),
                ArtifactRecovery::AbortedPublication
            );
            assert!(!staged.exists());
        } else {
            assert_eq!(
                writer.recover().unwrap(),
                ArtifactRecovery::AbortedPublication
            );
            let document = writer.document().unwrap();
            assert!(document.pending.is_none());
            assert_eq!(document.abandoned_publications.len(), 1);
            assert_eq!(
                document.abandoned_publications[0].operation.stage.path.0,
                staged
            );
            assert!(staged.is_dir());
        }
        assert!(!f.destination.exists());
        assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
    }
}

#[cfg(unix)]
#[test]
fn copy_modes_empty_directories_and_regular_link_policy_are_observable() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    directory_source(&f);
    fs::set_permissions(
        f.source.join("nested/binary"),
        fs::Permissions::from_mode(0o751),
    )
    .unwrap();
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    assert_eq!(
        fs::metadata(f.destination.join("nested/binary"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o751
    );
    fs::set_permissions(
        f.source.join("nested/binary"),
        fs::Permissions::from_mode(0o711),
    )
    .unwrap();
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap(),
        ArtifactChange::Updated
    );
    fs::create_dir(f.source.join("new-empty")).unwrap();
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap(),
        ArtifactChange::Updated
    );
    assert!(f.destination.join("new-empty").is_dir());
    symlink(f.source.join("nested/binary"), f.source.join("file-link")).unwrap();
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::Directory)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::Directory,
                ArtifactMode::Copy(CopyPolicy::FollowRegularFiles)
            )
            .unwrap(),
        ArtifactChange::Updated
    );
    assert!(
        !fs::symlink_metadata(f.destination.join("file-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    symlink(f.source.join("nested"), f.source.join("directory-link")).unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::Directory,
                ArtifactMode::Copy(CopyPolicy::FollowRegularFiles)
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    writer.remove(&f.key()).unwrap();
}

#[test]
fn copy_and_link_backend_changes_replace_owned_objects_without_false_noop() {
    for copy_first in [false, true] {
        let f = Fixture::new();
        let mut writer = f.writer();
        let initial = if copy_first {
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
        } else {
            ArtifactMode::Link
        };
        let next = if copy_first {
            ArtifactMode::Link
        } else {
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
        };
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                initial,
            )
            .unwrap();
        assert_eq!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    next
                )
                .unwrap(),
            ArtifactChange::Updated
        );
        assert_eq!(writer.load().unwrap().unwrap().entries()[0].mode(), next);
        assert_eq!(
            fs::symlink_metadata(&f.destination)
                .unwrap()
                .file_type()
                .is_symlink(),
            copy_first
        );
        fs::write(&f.source, b"live update").unwrap();
        assert_eq!(
            fs::read(&f.destination).unwrap(),
            if copy_first {
                b"live update".as_slice()
            } else {
                b"source remains".as_slice()
            }
        );
        writer.remove(&f.key()).unwrap();
        assert_eq!(fs::read(&f.source).unwrap(), b"live update");
    }
}

#[cfg(unix)]
struct PermissionReset(Vec<PathBuf>);
#[cfg(unix)]
impl Drop for PermissionReset {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        for path in &self.0 {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
        }
    }
}
#[cfg(unix)]
fn restricted_owned_tree(
    writer: &ArtifactWriter,
    f: &Fixture,
    inspect: bool,
) -> (tree::TreeReceipt, PermissionReset) {
    use std::os::unix::fs::PermissionsExt;
    let mut doc = writer.document().unwrap();
    let Proof::Copy { tree, .. } = &mut doc.entries[0].receipt else {
        panic!("proof");
    };
    for node in &mut tree.nodes {
        node.mode = tree::Mode::Unix(0);
        // The fixture seeds a different creation-time mode; it is not a
        // verified readable publication observation.
        node.verified = None;
    }
    let receipt = tree.clone();
    if inspect {
        doc.pending = Some(Pending::Inspect {
            entry: Box::new(doc.entries[0].clone()),
        });
    }
    // Private fixture seeds creation-time receipt before final restrictive chmod.
    writer.save(&doc, ArtifactStage::Commit).unwrap();
    let paths: Vec<_> = receipt
        .nodes
        .iter()
        .map(|n| {
            if n.relative.0.as_os_str().is_empty() {
                f.destination.clone()
            } else {
                f.destination.join(&n.relative.0)
            }
        })
        .collect();
    let reset = PermissionReset(paths.clone());
    for path in paths.iter().rev() {
        fs::set_permissions(path, fs::Permissions::from_mode(0o0)).unwrap();
    }
    (receipt, reset)
}

#[cfg(unix)]
#[test]
fn copy_restricted_owned_modes_use_durable_access_and_cleanup() {
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    let (_, _reset) = restricted_owned_tree(&writer, &f, false);
    assert_eq!(writer.remove(&f.key()).unwrap(), ArtifactChange::Removed);
    assert!(!f.destination.exists());
    assert_eq!(
        fs::read(f.source.join("nested/binary")).unwrap(),
        [0, 255, 1, 0, 37]
    );
    assert!(writer.document().unwrap().pending.is_none());
}

#[cfg(unix)]
#[test]
fn inspect_recovery_regrants_inaccessible_parent_then_restores_postorder() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    let (receipt, _reset) = restricted_owned_tree(&writer, &f, true);
    receipt
        .grant(&f.destination, tree::Access::Read, false, || Ok(()))
        .unwrap();
    // Simulate a child restored and its originally inaccessible parent restored.
    fs::set_permissions(
        f.destination.join("nested/binary"),
        fs::Permissions::from_mode(0o0),
    )
    .unwrap();
    fs::set_permissions(
        f.destination.join("nested"),
        fs::Permissions::from_mode(0o0),
    )
    .unwrap();
    fs::set_permissions(&f.destination, fs::Permissions::from_mode(0o0)).unwrap();
    drop(writer);
    let mut writer = f.writer();
    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::RestoredAccess);
    assert!(writer.document().unwrap().pending.is_none());
    assert_eq!(
        fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
        0
    );
    fs::set_permissions(&f.destination, fs::Permissions::from_mode(0o700)).unwrap();
    for node in &receipt.nodes[1..] {
        let path = f.destination.join(&node.relative.0);
        assert_eq!(
            fs::symlink_metadata(&path).unwrap().permissions().mode() & 0o777,
            0
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(
        fs::read(f.destination.join("nested/binary")).unwrap(),
        [0, 255, 1, 0, 37]
    );
    assert!(f.destination.join("empty 🚀").is_dir());
}

#[cfg(unix)]
#[test]
fn inspect_restore_failure_and_foreign_node_preserve_journal_and_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    let (receipt, _reset) = restricted_owned_tree(&writer, &f, true);
    receipt
        .grant(&f.destination, tree::Access::Read, false, || Ok(()))
        .unwrap();
    writer.fail_at.set(Some(ArtifactStage::RestoreAccess));
    assert_eq!(
        writer.recover().unwrap_err().stage(),
        ArtifactStage::RecoveryConflict
    );
    assert!(writer.document().unwrap().pending.is_some());
    // Make all fixture paths accessible, then replace one registered physical node.
    receipt
        .grant(&f.destination, tree::Access::Read, false, || Ok(()))
        .unwrap();
    fs::set_permissions(
        f.destination.join("nested"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let path = f.destination.join("nested/binary");
    fs::rename(&path, f.destination.join("nested/old-inode")).unwrap();
    fs::write(&path, b"foreign").unwrap();
    assert_eq!(
        writer.recover().unwrap_err().stage(),
        ArtifactStage::RecoveryConflict
    );
    assert!(writer.document().unwrap().pending.is_some());
    let abandoned = writer.forget(&f.key()).unwrap();
    assert_eq!(
        abandoned.intent().unwrap().publication().unwrap().phase(),
        ArtifactPending::Inspect
    );
    assert_eq!(fs::read(path).unwrap(), b"foreign");
}

#[test]
fn copy_recovery_conflicts_preserve_foreign_backup_or_destination() {
    for foreign_destination in [false, true] {
        let f = Fixture::new();
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        fs::write(&f.source, b"updated").unwrap();
        writer.fail_at.set(Some(ArtifactStage::Detach));
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::File)
                .unwrap_err()
                .stage(),
            ArtifactStage::Detach
        );
        let doc = writer.document().unwrap();
        let Pending::Publication(op) = doc.pending.as_ref().unwrap() else {
            panic!("copy intent");
        };
        let target = if foreign_destination {
            f.destination.clone()
        } else {
            op.stage.path.0.join("backup")
        };
        if !foreign_destination {
            fs::rename(&target, op.stage.path.0.join("kept-inode")).unwrap();
        }
        fs::write(&target, b"foreign").unwrap();
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        assert_eq!(fs::read(&target).unwrap(), b"foreign");
        let abandoned = writer.forget(&f.key()).unwrap();
        assert!(abandoned.entry().is_some());
        assert!(abandoned.intent().is_some());
        assert_eq!(fs::read(target).unwrap(), b"foreign");
        assert!(writer.document().unwrap().entries.is_empty());
    }
}

#[test]
fn copy_source_plan_detects_changes_without_partial_inventory_or_root_mtime_authority() {
    let f = Fixture::new();
    directory_source(&f);
    let plan = tree::SourcePlan::capture(
        &f.source,
        Kind::Directory,
        CopyPolicy::RejectFileLinks,
        f.store.limits.copy,
        &Cancellation::default(),
    )
    .unwrap();
    fs::write(f.source.join("nested/binary"), [0, 255, 2, 0, 37]).unwrap();
    assert_eq!(
        plan.recheck(f.store.limits.copy, &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let mut partial = tree::TreeReceipt { nodes: Vec::new() };
    let stage = f.destination.with_extension("stage");
    assert!(
        plan.build(&stage, &Cancellation::default(), None, &mut partial, || Ok(
            ()
        ))
        .is_err()
    );
    assert!(!f.destination.exists());
    if !partial.nodes.is_empty() {
        partial
            .cleanup(&stage, f.store.limits.copy, || Ok(()))
            .unwrap();
    }
}

#[test]
fn copy_manifest_and_lowered_limits_fail_before_owned_publication_effects() {
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    writer.store.limits.max_document_bytes = 700;
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::Directory)
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert!(!f.destination.exists());
    assert!(!f.store.manifest_path().exists());
    writer.store.limits.max_document_bytes = 1024 * 1024;
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    let before = fs::read(f.store.manifest_path()).unwrap();
    writer.store.limits.copy = CopyLimits::new(1, 1, 1, 1).unwrap();
    assert_eq!(
        writer.remove(&f.key()).unwrap_err().stage(),
        ArtifactStage::Read
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    assert_eq!(
        fs::read(f.destination.join("nested/binary")).unwrap(),
        [0, 255, 1, 0, 37]
    );
}

#[test]
fn mixed_link_copy_processes_share_writer_and_holder_death_recovers_copy() {
    let f = Fixture::new();
    let root = f.store.scope.owner_root();
    let mut link = child(root, "link-process", false);
    let mut copy = child(root, "copy-process", false);
    assert!(link.0.wait().unwrap().success());
    assert!(copy.0.wait().unwrap().success());
    let snapshot = f.store.load().unwrap().unwrap();
    assert_eq!(snapshot.entries().len(), 2);
    assert!(
        snapshot
            .entries()
            .iter()
            .any(|e| e.mode() == ArtifactMode::Link)
    );
    assert!(
        snapshot
            .entries()
            .iter()
            .any(|e| e.mode() == ArtifactMode::Copy(CopyPolicy::RejectFileLinks))
    );
    let mut interrupted = child(root, "copy-interrupted", true);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("ready").exists() {
        assert!(Instant::now() < deadline);
        assert!(interrupted.0.try_wait().unwrap().is_none());
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        f.store
            .writer(Duration::from_millis(10), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    interrupted.0.kill().unwrap();
    interrupted.0.wait().unwrap();
    let mut writer = f.writer();
    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedCreate);
    assert_eq!(writer.document().unwrap().entries.len(), 3);
    assert_eq!(
        fs::read(root.join("adapter/copy-interrupted")).unwrap(),
        b"source remains"
    );
}

#[cfg(unix)]
#[test]
fn restricted_directory_publish_update_remove_preserve_original_modes_and_source() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o500, 0o555] {
        let f = Fixture::new();
        directory_source(&f);
        let reset = PermissionReset(vec![f.source.clone()]);
        fs::set_permissions(&f.source, fs::Permissions::from_mode(mode)).unwrap();
        let mut writer = f.writer();
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap(),
            ArtifactChange::Created
        );
        assert_eq!(
            fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
            mode
        );
        let file = f.source.join("nested/binary");
        fs::write(&file, b"updated").unwrap();
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap(),
            ArtifactChange::Updated
        );
        assert_eq!(
            fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
            mode
        );
        assert_eq!(
            fs::metadata(&f.source).unwrap().permissions().mode() & 0o777,
            mode
        );
        assert_eq!(fs::read(&file).unwrap(), b"updated");
        assert_eq!(writer.remove(&f.key()).unwrap(), ArtifactChange::Removed);
        assert_eq!(
            fs::metadata(&f.source).unwrap().permissions().mode() & 0o777,
            mode
        );
        drop(reset);
    }
}

#[cfg(unix)]
#[test]
fn directory_move_grant_rename_restore_journal_crashes_recover_with_original_modes() {
    use std::os::unix::fs::PermissionsExt;
    for update in [false, true] {
        for stage in [
            ArtifactStage::GrantMove,
            ArtifactStage::Publish,
            ArtifactStage::RestoreMove,
            ArtifactStage::ClearMove,
        ] {
            for reopen in [false, true] {
                let f = Fixture::new();
                directory_source(&f);
                let _reset = PermissionReset(vec![f.source.clone()]);
                fs::set_permissions(&f.source, fs::Permissions::from_mode(0o555)).unwrap();
                let mut writer = f.writer();
                if update {
                    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
                    fs::write(f.source.join("nested/binary"), b"updated").unwrap();
                }
                writer.fail_at.set(Some(stage));
                assert!(
                    copy_fixture(&mut writer, &f, NodeKind::Directory).is_err(),
                    "stage {stage:?}"
                );
                let doc = writer.document().unwrap();
                let Pending::Publication(operation) = doc.pending.as_ref().unwrap() else {
                    panic!("copy journal");
                };
                assert!(operation.movement.is_some());
                if reopen {
                    drop(writer);
                    writer = f.writer();
                }
                let outcome = writer.recover().unwrap();
                assert!(matches!(
                    outcome,
                    ArtifactRecovery::CompletedCreate | ArtifactRecovery::CompletedUpdate
                ));
                assert!(writer.document().unwrap().pending.is_none());
                assert_eq!(
                    fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
                    0o555
                );
                assert_eq!(
                    fs::metadata(&f.source).unwrap().permissions().mode() & 0o777,
                    0o555
                );
                assert_eq!(
                    fs::read(f.destination.join("nested/binary")).unwrap(),
                    fs::read(f.source.join("nested/binary")).unwrap()
                );
                writer.remove(&f.key()).unwrap();
            }
        }
    }
}

#[test]
fn strict_copy_receipts_journal_modes_and_native_relative_paths_fail_closed() {
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    let original = writer.document().unwrap();
    let mut invalid = Vec::new();
    for alteration in 0..5 {
        let mut doc = original.clone();
        let Proof::Copy { tree, .. } = &mut doc.entries[0].receipt else {
            panic!("tree");
        };
        match alteration {
            0 => tree.nodes[1].id = tree.nodes[0].id.clone(),
            1 => tree.nodes[1].relative.0 = PathBuf::from("../escape"),
            2 => {
                tree.nodes[0].content = tree::Content::File {
                    bytes: 0,
                    digest: [0; 32],
                }
            }
            3 => {
                tree.nodes[0].mode = if cfg!(unix) {
                    tree::Mode::Windows(false)
                } else {
                    tree::Mode::Unix(0o777)
                }
            }
            _ => tree.nodes.swap(1, 2),
        }
        invalid.push(doc);
    }
    fs::write(f.source.join("nested/binary"), b"updated").unwrap();
    writer.fail_at.set(Some(ArtifactStage::Detach));
    assert!(copy_fixture(&mut writer, &f, NodeKind::Directory).is_err());
    let pending = writer.document().unwrap();
    for alteration in 0..6 {
        let mut doc = pending.clone();
        let Pending::Publication(op) = doc.pending.as_mut().unwrap() else {
            panic!("operation");
        };
        match alteration {
            0 => op.prepared = None,
            1 => op.stage.path.0 = f.source.join("foreign-stage"),
            2 => {
                op.movement = Some(publication::Movement::Detach);
                op.read_access = Some(publication::Location::Backup);
            }
            3 => {
                op.phase = publication::Phase::Cleanup;
                op.movement = Some(publication::Movement::Detach);
            }
            4 => {
                op.stage.receipt.as_mut().unwrap().id = match op.prepared.as_ref().unwrap() {
                    Proof::Copy { tree, .. } => tree.nodes[0].id.clone(),
                    _ => panic!("copy proof"),
                }
            }
            _ => op.goal = publication::Goal::Create,
        }
        invalid.push(doc);
    }
    for (index, doc) in invalid.into_iter().enumerate() {
        let bytes = if index == 1 {
            // The path-string serializer itself rejects an escaping relative
            // path. Inject the raw wire value to test the reader independently.
            assert!(codec::encode(&doc, 1024 * 1024).is_err());
            let mut value = catalog_json(&original);
            value["entries"][0]["receipt"]["tree"]["nodes"][1]["relative"] =
                serde_json::json!("../escape");
            serde_json::to_vec(&value).unwrap()
        } else {
            codec::encode(&doc, 1024 * 1024).unwrap()
        };
        fs::write(f.store.manifest_path(), bytes).unwrap();
        assert_eq!(
            writer.load().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            fs::read(f.source.join("nested/binary")).unwrap(),
            b"updated"
        );
    }
    let mut unknown = catalog_json(&pending);
    unknown["pending"]["surprise"] = serde_json::json!(true);
    fs::write(
        f.store.manifest_path(),
        serde_json::to_vec(&unknown).unwrap(),
    )
    .unwrap();
    assert_eq!(
        writer.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(
        f.store.manifest_path(),
        codec::encode(&pending, 1024 * 1024).unwrap(),
    )
    .unwrap();
    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
    writer.remove(&f.key()).unwrap();
}

#[cfg(unix)]
#[test]
fn copy_unicode_root_file_links_and_special_nodes_preserve_input() {
    use std::os::unix::{fs::symlink, net::UnixListener};
    let f = Fixture::new();
    let native = f.source.with_file_name("x-界");
    fs::rename(&f.source, &native).unwrap();
    symlink(&native, &f.source).unwrap();
    let mut writer = f.writer();
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::File)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::FollowRegularFiles)
            )
            .unwrap(),
        ArtifactChange::Created
    );
    assert_eq!(writer.document().unwrap().entries[0].source(), native);
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
    writer.remove(&f.key()).unwrap();
    fs::remove_file(&f.source).unwrap();
    symlink(f.source.with_extension("missing"), &f.source).unwrap();
    assert_eq!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::FollowRegularFiles)
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    fs::remove_file(&f.source).unwrap();
    let _socket = UnixListener::bind(&f.source).unwrap();
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::File)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(native).unwrap(), b"source remains");
}

#[cfg(unix)]
#[test]
fn movement_foreign_dual_missing_or_unexpected_modes_never_restore_unknown_nodes() {
    use std::os::unix::fs::PermissionsExt;
    for mutation in 0..4 {
        let f = Fixture::new();
        directory_source(&f);
        let _reset = PermissionReset(vec![f.source.clone()]);
        fs::set_permissions(&f.source, fs::Permissions::from_mode(0o555)).unwrap();
        let mut writer = f.writer();
        writer.fail_at.set(Some(ArtifactStage::GrantMove));
        assert!(copy_fixture(&mut writer, &f, NodeKind::Directory).is_err());
        let snapshot = writer.load().unwrap().unwrap();
        let intent = snapshot.pending().unwrap().publication().unwrap();
        assert!(intent.permissions_may_be_modified());
        let payload = intent.payload().unwrap();
        match mutation {
            0 => {
                fs::create_dir(&f.destination).unwrap();
                fs::write(f.destination.join("foreign"), b"foreign").unwrap();
            }
            1 => fs::rename(&payload, f.destination.with_extension("kept-root")).unwrap(),
            2 => {
                fs::rename(&payload, f.destination.with_extension("kept-root")).unwrap();
                fs::create_dir(&payload).unwrap();
                fs::write(payload.join("foreign"), b"foreign").unwrap();
            }
            _ => fs::set_permissions(&payload, fs::Permissions::from_mode(0o700)).unwrap(),
        }
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        assert!(writer.document().unwrap().pending.is_some());
        let abandoned = writer.forget(&f.key()).unwrap();
        assert!(
            abandoned
                .intent()
                .unwrap()
                .publication()
                .unwrap()
                .permissions_may_be_modified()
        );
        assert!(writer.document().unwrap().entries.is_empty());
        if mutation == 0 {
            assert_eq!(fs::read(f.destination.join("foreign")).unwrap(), b"foreign");
        }
        if mutation == 2 {
            assert_eq!(fs::read(payload.join("foreign")).unwrap(), b"foreign");
        }
        assert_eq!(
            fs::metadata(&f.source).unwrap().permissions().mode() & 0o777,
            0o555
        );
        assert_eq!(
            fs::read(f.source.join("nested/binary")).unwrap(),
            [0, 255, 1, 0, 37]
        );
    }
}

#[test]
fn copy_cancellation_cleans_known_build_preserves_pre_detach_and_finishes_after_detach() {
    for global in [false, true] {
        for stage in [
            ArtifactStage::CopyChunk,
            ArtifactStage::RecordProof,
            ArtifactStage::Detach,
        ] {
            let f = Fixture::with_owner(global);
            let mut writer = f.writer();
            copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
            let old = writer.document().unwrap().entries.remove(0);
            fs::write(&f.source, vec![7; 128 * 1024]).unwrap();
            // File creation is the first callback; the second follows a written chunk.
            writer.cancel_at.set(Some((
                stage,
                if stage == ArtifactStage::CopyChunk {
                    2
                } else {
                    1
                },
            )));
            let result = copy_fixture(&mut writer, &f, NodeKind::File);
            assert!(writer.cancellation.is_cancelled());
            let snapshot = writer.load().unwrap().unwrap();
            match stage {
                ArtifactStage::CopyChunk => {
                    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
                    assert!(snapshot.pending().is_none());
                    assert!(snapshot.entries()[0] == old);
                    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
                }
                ArtifactStage::RecordProof => {
                    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
                    assert_eq!(snapshot.pending().unwrap().phase(), ArtifactPending::Ready);
                    assert!(snapshot.entries()[0] == old);
                    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
                }
                ArtifactStage::Detach => {
                    assert_eq!(result.unwrap(), ArtifactChange::Updated);
                    assert!(snapshot.pending().is_none());
                    assert_eq!(fs::read(&f.destination).unwrap(), vec![7; 128 * 1024]);
                }
                _ => unreachable!(),
            }
            assert_eq!(fs::read(&f.source).unwrap(), vec![7; 128 * 1024]);
            drop(writer);
            let mut writer = f.writer();
            if stage == ArtifactStage::RecordProof {
                assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
            }
            writer.remove(&f.key()).unwrap();
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_copy_readonly_file_and_directory_modes_survive_noop_update_and_cleanup() {
    struct Reset(Vec<PathBuf>);
    impl Drop for Reset {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = tree::Mode::Windows(false).set(path);
            }
        }
    }
    fn readonly(path: &Path, value: bool) {
        tree::Mode::Windows(value).set(path).unwrap();
    }
    for directory in [false, true] {
        let f = Fixture::new();
        let (kind, source_file, target_file) = if directory {
            directory_source(&f);
            (
                NodeKind::Directory,
                f.source.join("nested/binary"),
                f.destination.join("nested/binary"),
            )
        } else {
            (NodeKind::File, f.source.clone(), f.destination.clone())
        };
        let _reset = Reset(vec![
            source_file.clone(),
            f.source.clone(),
            target_file.clone(),
            f.destination.clone(),
        ]);
        readonly(&source_file, true);
        readonly(&f.source, true);
        let mut writer = f.writer();
        assert_eq!(
            copy_fixture(&mut writer, &f, kind).unwrap(),
            ArtifactChange::Created
        );
        assert!(fs::metadata(&target_file).unwrap().permissions().readonly());
        assert!(
            fs::metadata(&f.destination)
                .unwrap()
                .permissions()
                .readonly()
        );
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            copy_fixture(&mut writer, &f, kind).unwrap(),
            ArtifactChange::Unchanged
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        readonly(&source_file, false);
        fs::write(&source_file, b"readonly updated").unwrap();
        readonly(&source_file, true);
        assert_eq!(
            copy_fixture(&mut writer, &f, kind).unwrap(),
            ArtifactChange::Updated
        );
        assert_eq!(fs::read(&target_file).unwrap(), b"readonly updated");
        assert!(fs::metadata(&target_file).unwrap().permissions().readonly());
        assert!(
            fs::metadata(&f.destination)
                .unwrap()
                .permissions()
                .readonly()
        );
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(writer.remove(&f.key()).unwrap(), ArtifactChange::Removed);
        assert!(!f.destination.exists());
        assert_eq!(fs::read(&source_file).unwrap(), b"readonly updated");
        assert!(fs::metadata(&source_file).unwrap().permissions().readonly());
        assert!(fs::metadata(&f.source).unwrap().permissions().readonly());
    }
}

type PhysicalTree = Vec<(PathBuf, tree::PhysicalId, tree::Mode, Option<Vec<u8>>)>;

fn physical_tree(root: &Path) -> PhysicalTree {
    if !root.exists() {
        return Vec::new();
    }
    let mut nodes = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .map(|item| {
            let item = item.unwrap();
            let path = item.path();
            let metadata = fs::symlink_metadata(path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            (
                path.strip_prefix(root).unwrap().to_owned(),
                tree::physical_id(path, &metadata).unwrap(),
                tree::Mode::observe(&metadata),
                metadata.is_file().then(|| fs::read(path).unwrap()),
            )
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.0.cmp(&b.0));
    nodes
}

#[test]
fn cancelled_stream_has_exact_partial_receipt_and_only_known_payload_is_cleaned() {
    use sha2::Digest;
    let f = Fixture::new();
    let bytes = vec![19; 128 * 1024];
    fs::write(&f.source, &bytes).unwrap();
    let plan = tree::SourcePlan::capture(
        &f.source,
        Kind::File,
        CopyPolicy::RejectFileLinks,
        f.store.limits.copy,
        &Cancellation::default(),
    )
    .unwrap();
    let source = physical_tree(&f.source);
    let payload = f.destination.with_extension("partial");
    fs::write(&f.destination, b"foreign preserved").unwrap();
    let cancellation = Cancellation::default();
    let mut receipt = tree::TreeReceipt { nodes: Vec::new() };
    let error = plan
        .build(&payload, &cancellation, None, &mut receipt, || {
            if fs::metadata(&payload)?.len() > 0 {
                cancellation.cancel();
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    let written = fs::read(&payload).unwrap();
    assert!(!written.is_empty() && written.len() < bytes.len());
    assert_eq!(written, bytes[..written.len()]);
    let tree::Content::File {
        bytes: count,
        digest,
    } = &receipt.nodes[0].content
    else {
        panic!("partial file receipt");
    };
    assert_eq!(*count, written.len() as u64);
    assert_eq!(*digest, <[u8; 32]>::from(sha2::Sha256::digest(&written)));
    receipt
        .cleanup(&payload, f.store.limits.copy, || Ok(()))
        .unwrap();
    assert!(!payload.exists());
    assert!(physical_tree(&f.source) == source);
    assert_eq!(fs::read(&f.destination).unwrap(), b"foreign preserved");
}

#[test]
fn cleanup_remaining_tree_rejects_extras_bytes_and_replacements_before_more_deletion() {
    for mutation in 0..3 {
        let f = Fixture::new();
        directory_source(&f);
        fs::write(f.source.join("a-survivor"), b"old surviving bytes").unwrap();
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
        fs::write(f.source.join("nested/binary"), b"updated").unwrap();
        writer.fail_at.set(Some(ArtifactStage::Cleanup));
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::Directory)
                .unwrap_err()
                .stage(),
            ArtifactStage::CleanupRequired
        );
        let snapshot = writer.load().unwrap().unwrap();
        let intent = snapshot.pending().unwrap().publication().unwrap();
        assert_eq!(intent.phase(), ArtifactPending::Cleanup);
        let backup = intent.backup().unwrap();
        assert!(!backup.join("nested/binary").exists());
        let surviving = backup.join("a-survivor");
        assert!(surviving.is_file());
        let displaced = f.destination.with_extension("displaced");
        match mutation {
            0 => fs::write(backup.join("extra"), b"external bytes").unwrap(),
            1 => fs::write(&surviving, b"changed external bytes").unwrap(),
            2 => {
                fs::rename(&surviving, &displaced).unwrap();
                fs::write(&surviving, b"old surviving bytes").unwrap();
            }
            _ => unreachable!(),
        }
        let remaining = physical_tree(&backup);
        let published = physical_tree(&f.destination);
        let source = physical_tree(&f.source);
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::CleanupRequired
        );
        assert!(physical_tree(&backup) == remaining);
        assert!(physical_tree(&f.destination) == published);
        assert!(physical_tree(&f.source) == source);
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        assert_eq!(
            writer.load().unwrap().unwrap().pending().unwrap().phase(),
            ArtifactPending::Cleanup
        );
        if mutation == 2 {
            assert_eq!(fs::read(&displaced).unwrap(), b"old surviving bytes");
        }
    }
}

#[test]
fn forget_committed_copy_cleanup_preserves_new_backup_residue_and_other_membership() {
    let f = Fixture::new();
    directory_source(&f);
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    let other_key = ArtifactKey::new("codex", "skill", "org/other").unwrap();
    let other_destination = f.destination.with_extension("other");
    writer
        .publish(
            &other_key,
            PhysicalSource::ExternalSource(&f.source),
            &other_destination,
            NodeKind::Directory,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    let other = writer
        .document()
        .unwrap()
        .entries
        .into_iter()
        .find(|e| e.key() == &other_key)
        .unwrap();
    fs::write(f.source.join("nested/binary"), b"updated").unwrap();
    writer.fail_at.set(Some(ArtifactStage::Cleanup));
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::Directory)
            .unwrap_err()
            .stage(),
        ArtifactStage::CleanupRequired
    );
    let snapshot = writer.load().unwrap().unwrap();
    let intent = snapshot.pending().unwrap().publication().unwrap();
    let new_entry = snapshot
        .entries()
        .iter()
        .find(|e| e.key() == &f.key())
        .unwrap()
        .clone();
    let stage = intent.staging_directory().unwrap().to_owned();
    let source_before = physical_tree(&f.source);
    let published_before = physical_tree(&f.destination);
    let stage_before = physical_tree(&stage);
    let other_before = physical_tree(&other_destination);
    assert!(!stage_before.is_empty());
    drop(writer);
    let mut writer = f.writer();
    let abandoned = writer.forget(&f.key()).unwrap();
    assert!(abandoned.entry().unwrap() == &new_entry);
    assert_eq!(
        abandoned.intent().unwrap().publication().unwrap().phase(),
        ArtifactPending::Cleanup
    );
    assert_eq!(
        abandoned
            .intent()
            .unwrap()
            .publication()
            .unwrap()
            .staging_directory()
            .unwrap(),
        stage
    );
    assert!(
        abandoned
            .intent()
            .unwrap()
            .publication()
            .unwrap()
            .permissions_may_be_modified()
    );
    let after = writer.load().unwrap().unwrap();
    assert!(after.pending().is_none());
    assert_eq!(after.entries().len(), 1);
    assert!(after.entries()[0] == other);
    assert!(physical_tree(&f.source) == source_before);
    assert!(physical_tree(&f.destination) == published_before);
    assert!(physical_tree(&stage) == stage_before);
    assert!(physical_tree(&other_destination) == other_before);
    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::NoPending);
}

#[test]
fn cancelled_partial_build_is_durably_abandoned_without_losing_previous_publication() {
    for global in [false, true] {
        let f = Fixture::with_owner(global);
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        let old = writer.document().unwrap().entries.remove(0);
        fs::write(&f.source, vec![29; 128 * 1024]).unwrap();
        writer.cancel_at.set(Some((ArtifactStage::CopyChunk, 2)));
        writer.fail_at.set(Some(ArtifactStage::Cleanup));
        let error = copy_fixture(&mut writer, &f, NodeKind::File).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(error.secondary_failure(), Some(io::ErrorKind::Other));
        let document = writer.document().unwrap();
        let Some(Pending::Publication(operation)) = &document.pending else {
            panic!("publication intent");
        };
        assert!(operation.phase == publication::Phase::Building);
        let stage = operation.stage.path.0.clone();
        let partial = physical_tree(&stage);
        let before = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        drop(writer);
        // A restarted operation has no surviving source tempfile to replay.
        fs::remove_file(&f.source).unwrap();
        let mut writer = f.writer();
        writer.cancellation.cancel();
        assert_eq!(
            writer.recover().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert!(physical_tree(&stage) == partial);
        drop(writer);
        let mut writer = f.writer();
        writer.fail_at.set(Some(ArtifactStage::Commit));
        assert_eq!(writer.recover().unwrap_err().stage(), ArtifactStage::Commit);
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert!(physical_tree(&stage) == partial);
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(
            writer.recover().unwrap(),
            ArtifactRecovery::AbortedPublication
        );
        let recovered = writer.document().unwrap();
        assert!(recovered.pending.is_none());
        assert!(recovered.entries[0] == old);
        assert_eq!(recovered.abandoned_publications.len(), 1);
        let abandoned = &recovered.abandoned_publications[0];
        assert_eq!(abandoned.operation.stage.path.0, stage);
        assert!(abandoned.scope.matches_scope(writer.store.scope()));
        assert!(abandoned.operation.prepared.is_none());
        assert!(physical_tree(&stage) == partial);
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::NoPending);
        // Native-document projections must retain the same archive as ordinary saves.
        let projection = codec::DocumentProjection::<native_document::Operation>::new(
            &recovered,
            &recovered.scope,
            None,
        );
        let projected = serde_json::to_value(projection).unwrap();
        assert_eq!(
            projected["abandoned_publications"],
            serde_json::to_value(&recovered).unwrap()["abandoned_publications"]
        );
        fs::write(&f.source, b"new complete graph").unwrap();
        assert_eq!(
            copy_fixture(&mut writer, &f, NodeKind::File).unwrap(),
            ArtifactChange::Updated
        );
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::NoPending);
        let updated = writer.document().unwrap();
        assert_eq!(updated.entries.len(), 1);
        assert_eq!(updated.abandoned_publications.len(), 1);
        assert!(physical_tree(&stage) == partial);
        assert_eq!(fs::read(&f.destination).unwrap(), b"new complete graph");
    }
}

#[cfg(unix)]
#[test]
fn building_abandonment_rejects_changed_receipted_namespace_without_effects() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for changed in ["identity", "symlink", "permissions", "unknown-child"] {
        let f = Fixture::new();
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        fs::write(&f.source, b"replacement").unwrap();
        writer.fail_at.set(Some(ArtifactStage::RecordProof));
        assert!(copy_fixture(&mut writer, &f, NodeKind::File).is_err());
        let document = writer.document().unwrap();
        let Some(Pending::Publication(operation)) = &document.pending else {
            panic!("publication intent");
        };
        let stage = operation.stage.path.0.clone();
        let before = fs::read(f.store.manifest_path()).unwrap();
        drop(writer);
        let held = stage.with_extension("held");
        match changed {
            "identity" => {
                fs::rename(&stage, &held).unwrap();
                graphit_fs::create_private_directory(&stage).unwrap();
                fs::write(stage.join("foreign"), b"preserve").unwrap();
            }
            "symlink" => {
                fs::rename(&stage, &held).unwrap();
                symlink(&held, &stage).unwrap();
            }
            "permissions" => {
                fs::set_permissions(&stage, fs::Permissions::from_mode(0o755)).unwrap()
            }
            "unknown-child" => fs::write(stage.join("foreign"), b"preserve").unwrap(),
            _ => unreachable!(),
        }
        let inspected = if changed == "symlink" { &held } else { &stage };
        let stage_before = physical_tree(inspected);
        let mut writer = f.writer();
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert!(physical_tree(inspected) == stage_before);
        if changed == "symlink" {
            assert_eq!(fs::read_link(&stage).unwrap(), held);
        }
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        assert!(writer.document().unwrap().abandoned_publications.is_empty());
    }
}

#[test]
fn owned_publication_transitions_destination_backend_and_kind() {
    for global in [false, true] {
        let modes = [
            ArtifactMode::Link,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        ];
        for old_mode in modes {
            for new_mode in modes {
                for old_kind in [NodeKind::File, NodeKind::Directory] {
                    for new_kind in [NodeKind::File, NodeKind::Directory] {
                        for relocation in 0..3 {
                            let f = Fixture::with_owner(global);
                            let directory = f.store.scope.owner_root().join("directory-source");
                            fs::create_dir(&directory).unwrap();
                            fs::create_dir(directory.join("empty")).unwrap();
                            fs::write(directory.join("keep"), b"directory bytes").unwrap();
                            let source_for = |kind| {
                                if kind == NodeKind::File {
                                    f.source.as_path()
                                } else {
                                    directory.as_path()
                                }
                            };
                            let destination = match relocation {
                                0 => f.destination.clone(),
                                1 => f.destination.with_file_name("new-alias"),
                                _ => {
                                    let parent =
                                        f.destination.parent().unwrap().join("other-adapter");
                                    fs::create_dir(&parent).unwrap();
                                    parent.join("new-alias")
                                }
                            };
                            let mut writer = f.writer_with_timeout(Duration::from_secs(30));
                            writer
                                .publish(
                                    &f.key(),
                                    PhysicalSource::ExternalSource(source_for(old_kind)),
                                    &f.destination,
                                    old_kind,
                                    old_mode,
                                )
                                .unwrap();
                            // Identical requests are no-op; all explicit transitions replace the owned payload.
                            let expected = if relocation == 0
                                && old_kind == new_kind
                                && old_mode == new_mode
                            {
                                ArtifactChange::Unchanged
                            } else {
                                ArtifactChange::Updated
                            };
                            assert_eq!(
                                writer
                                    .publish(
                                        &f.key(),
                                        PhysicalSource::ExternalSource(source_for(new_kind)),
                                        &destination,
                                        new_kind,
                                        new_mode
                                    )
                                    .unwrap(),
                                expected
                            );
                            let state = writer.load().unwrap().unwrap();
                            assert!(state.pending().is_none());
                            assert_eq!(state.entries().len(), 1);
                            assert_eq!(state.entries()[0].destination(), destination);
                            assert_eq!(state.entries()[0].mode(), new_mode);
                            assert_eq!(state.entries()[0].kind(), new_kind);
                            assert_eq!(
                                fs::symlink_metadata(&destination)
                                    .unwrap()
                                    .file_type()
                                    .is_symlink(),
                                new_mode == ArtifactMode::Link
                            );
                            if relocation != 0 {
                                assert!(fs::symlink_metadata(&f.destination).is_err());
                            }
                            if new_kind == NodeKind::Directory {
                                assert_eq!(
                                    fs::read(destination.join("keep")).unwrap(),
                                    b"directory bytes"
                                );
                                assert!(destination.join("empty").is_dir());
                            } else {
                                assert_eq!(fs::read(&destination).unwrap(), b"source remains");
                            }
                            assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
                            assert_eq!(
                                fs::read(directory.join("keep")).unwrap(),
                                b"directory bytes"
                            );
                            writer.remove(&f.key()).unwrap();
                            assert!(fs::symlink_metadata(&destination).is_err());
                            for parent in [
                                f.destination.parent().unwrap(),
                                destination.parent().unwrap(),
                            ] {
                                assert!(fs::read_dir(parent).unwrap().all(|e| {
                                    !e.unwrap()
                                        .file_name()
                                        .to_string_lossy()
                                        .starts_with(".graphit-artifact-")
                                }));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn relocated_link_movement_faults_recover_same_writer_and_reopen_with_full_receipts() {
    for stage in [
        ArtifactStage::RecordMove,
        ArtifactStage::Detach,
        ArtifactStage::Publish,
        ArtifactStage::RefreshProof,
        ArtifactStage::ClearMove,
        ArtifactStage::Commit,
        ArtifactStage::Cleanup,
    ] {
        for reopen in [false, true] {
            let f = Fixture::new();
            let parent = f.store.scope.owner_root().join("new-parent");
            fs::create_dir(&parent).unwrap();
            let new_destination = parent.join("new-alias");
            let mut writer = f.writer();
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link,
                )
                .unwrap();
            writer.fail_at.set(Some(stage));
            assert!(
                writer
                    .publish(
                        &f.key(),
                        PhysicalSource::ExternalSource(&f.source),
                        &new_destination,
                        NodeKind::File,
                        ArtifactMode::Link
                    )
                    .is_err()
            );
            if reopen {
                drop(writer);
                writer = f.writer();
            }
            assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
            let state = writer.load().unwrap().unwrap();
            assert!(state.pending().is_none());
            assert_eq!(state.entries()[0].destination(), new_destination);
            // A refreshed full receipt is required for subsequent ordinary verification.
            assert_eq!(
                writer
                    .publish(
                        &f.key(),
                        PhysicalSource::ExternalSource(&f.source),
                        &new_destination,
                        NodeKind::File,
                        ArtifactMode::Link
                    )
                    .unwrap(),
                ArtifactChange::Unchanged
            );
            assert!(fs::symlink_metadata(&f.destination).is_err());
            assert_eq!(fs::read(&new_destination).unwrap(), b"source remains");
            assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
        }
    }
}

#[test]
fn building_recovery_abandons_intent_without_claiming_unreceipted_wrappers() {
    for present_wrapper in 0..3 {
        let f = Fixture::new();
        let parent = f.store.scope.owner_root().join("new-parent");
        fs::create_dir(&parent).unwrap();
        let destination = parent.join("alias");
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        let mut document = writer.document().unwrap();
        let old = document.entries[0].clone();
        let candidate = Candidate {
            destination: NativePath(destination.clone()),
            ..old.candidate.clone()
        };
        let mut operation = publication::Operation::new(
            candidate,
            old.mode(),
            Some(old.clone()),
            publication::Goal::Update,
        )
        .unwrap();
        let primary = operation.stage.path.0.clone();
        let secondary = operation.old_stage.as_ref().unwrap().path.0.clone();
        // Simulate mkdir-before-proof, including the SECOND wrapper crash boundary.
        if present_wrapper != 0 {
            graphit_fs::create_private_directory(&primary).unwrap();
            operation.stage.receipt = Some(tree::TreeReceipt::directory(&primary).unwrap());
            if present_wrapper == 2 {
                fs::create_dir(&secondary).unwrap();
                fs::remove_dir(&primary).unwrap();
            }
        }
        document.pending = Some(Pending::Publication(Box::new(operation)));
        writer.save(&document, ArtifactStage::Prepare).unwrap();
        drop(writer);
        let mut writer = f.writer();
        if present_wrapper == 0 {
            assert_eq!(
                writer.recover().unwrap(),
                ArtifactRecovery::AbortedPublication
            );
        } else {
            assert_eq!(
                writer.recover().unwrap(),
                ArtifactRecovery::AbortedPublication
            );
            let document = writer.document().unwrap();
            assert!(document.pending.is_none());
            assert!(document.entries[0] == old);
            assert_eq!(document.abandoned_publications.len(), 1);
            assert_eq!(primary.exists(), present_wrapper == 1);
            assert_eq!(secondary.exists(), present_wrapper == 2);
            let abandoned = &document.abandoned_publications[0].operation;
            assert_eq!(abandoned.stage.path.0, primary);
            assert_eq!(abandoned.old_stage.as_ref().unwrap().path.0, secondary);
            assert!(abandoned.old_stage.as_ref().unwrap().receipt.is_none());
            assert_eq!(abandoned.candidate.destination.0, destination);
        }
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
    }
}

#[cfg(unix)]
#[test]
fn cancellation_preserves_primary_when_move_or_inspect_restoration_fails() {
    use std::os::unix::fs::PermissionsExt;
    for inspect in [false, true] {
        let f = Fixture::new();
        directory_source(&f);
        let _source_reset = PermissionReset(vec![f.source.clone()]);
        let mut writer = f.writer();
        let _owned_reset;
        if inspect {
            copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
            let (_, reset) = restricted_owned_tree(&writer, &f, false);
            _owned_reset = Some(reset);
            writer.cancel_at.set(Some((ArtifactStage::GrantAccess, 1)));
            writer.fail_at.set(Some(ArtifactStage::RestoreAccess));
        } else {
            _owned_reset = Some(PermissionReset(vec![f.destination.clone()]));
            fs::set_permissions(&f.source, fs::Permissions::from_mode(0o555)).unwrap();
            writer.cancel_at.set(Some((ArtifactStage::GrantMove, 1)));
            writer.fail_at.set(Some(ArtifactStage::RestoreMove));
        }
        let failure = copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::Interrupted);
        assert_eq!(failure.secondary_failure(), Some(io::ErrorKind::Other));
        let state = writer.load().unwrap().unwrap();
        assert!(state.pending().unwrap().permissions_may_be_modified());
        assert!(state.pending().is_some());
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(
            writer.recover().unwrap(),
            if inspect {
                ArtifactRecovery::RestoredAccess
            } else {
                ArtifactRecovery::CompletedCreate
            }
        );
        if inspect {
            assert_eq!(
                fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
                0
            );
        } else {
            assert_eq!(
                fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
                0o555
            );
        }
        assert!(writer.load().unwrap().unwrap().pending().is_none());
    }
}

#[test]
fn mixed_backend_relocation_faults_finish_forward_without_losing_sources() {
    let modes = [
        ArtifactMode::Link,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    ];
    for old_mode in modes {
        for new_mode in modes {
            for stage in [
                ArtifactStage::Detach,
                ArtifactStage::Publish,
                ArtifactStage::Commit,
                ArtifactStage::Cleanup,
            ] {
                for reopen in [false, true] {
                    let f = Fixture::new();
                    let parent = f.store.scope.owner_root().join("new-parent");
                    fs::create_dir(&parent).unwrap();
                    let new_destination = parent.join("alias");
                    let mut writer = f.writer();
                    writer
                        .publish(
                            &f.key(),
                            PhysicalSource::ExternalSource(&f.source),
                            &f.destination,
                            NodeKind::File,
                            old_mode,
                        )
                        .unwrap();
                    writer.fail_at.set(Some(stage));
                    assert!(
                        writer
                            .publish(
                                &f.key(),
                                PhysicalSource::ExternalSource(&f.source),
                                &new_destination,
                                NodeKind::File,
                                new_mode
                            )
                            .is_err()
                    );
                    if reopen {
                        drop(writer);
                        writer = f.writer();
                    }
                    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
                    let state = writer.load().unwrap().unwrap();
                    assert!(state.pending().is_none());
                    assert_eq!(state.entries()[0].mode(), new_mode);
                    assert_eq!(state.entries()[0].destination(), new_destination);
                    assert_eq!(fs::read(&new_destination).unwrap(), b"source remains");
                    assert!(fs::symlink_metadata(&f.destination).is_err());
                    assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
                    assert_eq!(
                        writer
                            .publish(
                                &f.key(),
                                PhysicalSource::ExternalSource(&f.source),
                                &new_destination,
                                NodeKind::File,
                                new_mode
                            )
                            .unwrap(),
                        ArtifactChange::Unchanged
                    );
                }
            }
        }
    }
}

#[test]
fn readable_copy_noop_at_manifest_cap_needs_no_hypothetical_inspect_journal() {
    let f = Fixture::new();
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    let document = writer.document().unwrap();
    let manifest = fs::read(f.store.manifest_path()).unwrap();
    let mut inspect = document.clone();
    inspect.pending = Some(Pending::Inspect {
        entry: Box::new(document.entries[0].clone()),
    });
    let bytes = codec::encode(&inspect, 1024 * 1024).unwrap().len();
    assert!(bytes > manifest.len());
    writer.store.limits.max_document_bytes = manifest.len();
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap(),
        ArtifactChange::Unchanged
    );

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "native cross-device qualification requires Linux mount namespaces and /dev/shm"]
fn publication_between_devices_uses_local_wrappers_and_recovers_relocations() {
    const CHILD: &str = "GRAPHIT_ARTIFACT_CROSS_DEVICE_CHILD";
    const CASE: &str = "artifact_store::tests::publication_between_devices_uses_local_wrappers_and_recovers_relocations";
    if std::env::var_os(CHILD).is_none() {
        // Mounts exist only in this child namespace, never in the host namespace.
        let output = Command::new("unshare")
            .args(["--user", "--map-root-user", "--mount"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture", "--include-ignored"])
            .env(CHILD, "1")
            .output()
            .expect("Linux cross-device suite requires unshare in PATH");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "cross-device child did not pass: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    let parent = f.store.scope.owner_root().join("mounted-adapter");
    fs::create_dir(&parent).unwrap();
    let device =
        tempfile::tempdir_in("/dev/shm").expect("Linux cross-device suite requires /dev/shm");
    let mounted = Command::new("mount")
        .arg("--bind")
        .arg(device.path())
        .arg(&parent)
        .status()
        .expect("mount in PATH");
    assert!(mounted.success());
    let result = std::panic::catch_unwind(|| {
        assert_ne!(
            fs::metadata(f.destination.parent().unwrap()).unwrap().dev(),
            fs::metadata(&parent).unwrap().dev()
        );
        let new_destination = parent.join("alias");
        let modes = [
            ArtifactMode::Link,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        ];
        for old_mode in modes {
            for new_mode in modes {
                for stage in [
                    None,
                    Some(ArtifactStage::Detach),
                    Some(ArtifactStage::Publish),
                ] {
                    let mut writer = f.writer_with_timeout(Duration::from_secs(30));
                    writer
                        .publish(
                            &f.key(),
                            PhysicalSource::ExternalSource(&f.source),
                            &f.destination,
                            NodeKind::File,
                            old_mode,
                        )
                        .unwrap();
                    writer.fail_at.set(stage);
                    let changed = writer.publish(
                        &f.key(),
                        PhysicalSource::ExternalSource(&f.source),
                        &new_destination,
                        NodeKind::File,
                        new_mode,
                    );
                    if let Some(expected_stage) = stage {
                        let error = changed.expect_err("injected publication failure must occur");
                        assert_eq!(error.stage(), expected_stage);
                        assert_eq!(error.kind(), io::ErrorKind::Other);
                        drop(writer);
                        writer = f.writer_with_timeout(Duration::from_secs(30));
                        assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedUpdate);
                    } else {
                        assert_eq!(changed.unwrap(), ArtifactChange::Updated);
                    }
                    let state = writer.load().unwrap().unwrap();
                    assert!(state.pending().is_none());
                    assert_eq!(state.entries()[0].destination(), new_destination);
                    assert_eq!(state.entries()[0].mode(), new_mode);
                    assert!(fs::symlink_metadata(&f.destination).is_err());
                    assert_eq!(fs::read(&new_destination).unwrap(), b"source remains");
                    assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
                    writer.remove(&f.key()).unwrap();
                    assert!(fs::read_dir(&parent).unwrap().next().is_none());
                    assert!(
                        fs::read_dir(f.destination.parent().unwrap())
                            .unwrap()
                            .next()
                            .is_none()
                    );
                }
            }
        }
    });
    assert!(
        Command::new("umount")
            .arg(&parent)
            .status()
            .expect("umount in PATH")
            .success()
    );
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn cleanup_checks_old_wrapper_when_new_wrapper_has_already_disappeared() {
    let f = Fixture::new();
    let parent = f.store.scope.owner_root().join("new-parent");
    fs::create_dir(&parent).unwrap();
    let destination = parent.join("alias");
    let mut writer = f.writer();
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    writer.fail_at.set(Some(ArtifactStage::Cleanup));
    assert!(
        writer
            .publish(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .is_err()
    );
    let snapshot = writer.load().unwrap().unwrap();
    let intent = snapshot.pending().unwrap().publication().unwrap();
    assert_eq!(intent.phase(), ArtifactPending::Cleanup);
    let primary = intent.staging_directory().unwrap();
    let secondary = intent.old_staging_directory().unwrap().to_owned();
    // Known empty primary wrapper is gone, but secondary is still pending cleanup.
    fs::remove_dir(primary).unwrap();
    fs::write(secondary.join("foreign"), b"preserve foreign").unwrap();
    let manifest = fs::read(f.store.manifest_path()).unwrap();
    assert_eq!(
        writer.recover().unwrap_err().stage(),
        ArtifactStage::CleanupRequired
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
    assert_eq!(
        fs::read(secondary.join("foreign")).unwrap(),
        b"preserve foreign"
    );
    assert_eq!(fs::read(&destination).unwrap(), b"source remains");
    let abandoned = writer.forget(&f.key()).unwrap();
    assert_eq!(
        abandoned
            .intent()
            .unwrap()
            .publication()
            .unwrap()
            .old_staging_directory()
            .unwrap(),
        secondary
    );
    assert_eq!(abandoned.entry().unwrap().destination(), destination);
    assert!(secondary.exists());
}

#[test]
fn link_movement_rejects_foreign_dual_missing_and_unauthorized_from_refresh() {
    for mutation in 0..6 {
        #[cfg(not(unix))]
        if mutation == 4 {
            continue;
        }
        let f = Fixture::new();
        let mut writer = f.writer();
        writer.fail_at.set(Some(ArtifactStage::BeforeRename));
        assert_eq!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    NodeKind::File,
                    ArtifactMode::Link
                )
                .unwrap_err()
                .stage(),
            ArtifactStage::BeforeRename
        );
        let snapshot = writer.load().unwrap().unwrap();
        let intent = snapshot.pending().unwrap().publication().unwrap();
        assert!(!intent.permissions_may_be_modified());
        let payload = intent.payload().unwrap();
        let kept = f.destination.with_file_name("kept-original-link");
        match mutation {
            0 => f.native_link(&f.destination, NodeKind::File), // BOTH positions exist.
            1 => fs::rename(&payload, &kept).unwrap(), // NEITHER authorized position exists.
            2 => {
                fs::rename(&payload, &kept).unwrap();
                f.native_link(&payload, NodeKind::File); // Same target, different FROM identity.
            }
            3 => {
                fs::rename(&payload, &kept).unwrap();
                f.native_link(&f.destination, NodeKind::File); // Foreign TO identity.
            }
            4 => {
                #[cfg(unix)]
                {
                    thread::sleep(Duration::from_millis(2));
                    fs::rename(&payload, &kept).unwrap();
                    fs::rename(&kept, &payload).unwrap(); // FROM full ctime changed outside authorized move.
                }
            }
            _ => fs::rename(&payload, &f.destination).unwrap(), // Known TO simulates own rename before refresh.
        }
        let before = fs::read(f.store.manifest_path()).unwrap();
        drop(writer);
        let mut writer = f.writer();
        if mutation == 5 {
            assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedCreate);
            assert_eq!(
                writer
                    .publish(
                        &f.key(),
                        PhysicalSource::ExternalSource(&f.source),
                        &f.destination,
                        NodeKind::File,
                        ArtifactMode::Link
                    )
                    .unwrap(),
                ArtifactChange::Unchanged
            );
        } else {
            assert_eq!(
                writer.recover().unwrap_err().stage(),
                ArtifactStage::RecoveryConflict
            );
            assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
            assert!(writer.load().unwrap().unwrap().pending().is_some());
            writer.forget(&f.key()).unwrap();
            assert_eq!(
                fs::read(if mutation == 0 || mutation == 3 {
                    &f.destination
                } else if mutation == 1 {
                    &kept
                } else {
                    &payload
                })
                .unwrap(),
                b"source remains"
            );
        }
        assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
    }
}

#[test]
fn recovery_rejects_redirected_old_or_new_parent_without_touching_foreign_data() {
    for redirect_old in [false, true] {
        let f = Fixture::new();
        let new_parent = f.store.scope.owner_root().join("new-parent");
        fs::create_dir(&new_parent).unwrap();
        let destination = new_parent.join("alias");
        let foreign = f.store.scope.owner_root().join("foreign-parent");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("link"), b"foreign link-name").unwrap();
        fs::write(foreign.join("alias"), b"foreign alias-name").unwrap();
        let kept = f.store.scope.owner_root().join("kept-original-parent");
        let parent = if redirect_old {
            f.destination.parent().unwrap()
        } else {
            new_parent.as_path()
        };
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        let old = writer.load().unwrap().unwrap().entries()[0].clone();
        writer.fail_at.set(Some(ArtifactStage::BeforeRename));
        assert!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &destination,
                    NodeKind::File,
                    ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
                )
                .is_err()
        );
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        fs::rename(parent, &kept).unwrap();
        LinkPlan::capture(&foreign, parent, NodeKind::Directory)
            .unwrap()
            .create()
            .unwrap();
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        let state = writer.load().unwrap().unwrap();
        assert!(state.pending().is_some());
        assert!(state.entries()[0] == old);
        assert_eq!(
            fs::read(if redirect_old {
                kept.join("link")
            } else {
                f.destination.clone()
            })
            .unwrap(),
            b"source remains"
        );
        assert_eq!(
            fs::read(foreign.join("link")).unwrap(),
            b"foreign link-name"
        );
        assert_eq!(
            fs::read(foreign.join("alias")).unwrap(),
            b"foreign alias-name"
        );
        assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
        writer.forget(&f.key()).unwrap();
        assert!(kept.is_dir());
    }
}

#[test]
fn prepared_copy_bytes_or_identity_changes_preserve_old_publication_and_journal() {
    for replace_identity in [false, true] {
        let f = Fixture::new();
        let parent = f.store.scope.owner_root().join("new-parent");
        fs::create_dir(&parent).unwrap();
        let destination = parent.join("alias");
        let mut writer = f.writer();
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
        let old = writer.load().unwrap().unwrap().entries()[0].clone();
        fs::write(&f.source, b"updated").unwrap();
        writer.fail_at.set(Some(ArtifactStage::BeforeRename));
        assert!(
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &destination,
                    NodeKind::File,
                    ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
                )
                .is_err()
        );
        let payload = writer
            .load()
            .unwrap()
            .unwrap()
            .pending()
            .unwrap()
            .publication()
            .unwrap()
            .payload()
            .unwrap();
        if replace_identity {
            fs::rename(&payload, f.store.scope.owner_root().join("kept-payload")).unwrap();
            fs::write(&payload, b"updated").unwrap(); // Identical bytes do not prove ownership.
        } else {
            fs::write(&payload, b"foreign changed bytes").unwrap();
        }
        let manifest = fs::read(f.store.manifest_path()).unwrap();
        let payload_before = fs::read(&payload).unwrap();
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(
            writer.recover().unwrap_err().stage(),
            ArtifactStage::RecoveryConflict
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
        let state = writer.load().unwrap().unwrap();
        assert!(state.pending().is_some());
        assert!(state.entries()[0] == old);
        assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        assert_eq!(fs::read(&payload).unwrap(), payload_before);
        assert_eq!(fs::read(&f.source).unwrap(), b"updated");
        assert!(fs::symlink_metadata(&destination).is_err());
        writer.forget(&f.key()).unwrap();
        assert_eq!(fs::read(&payload).unwrap(), payload_before);
    }
}

#[test]
fn cancellation_at_final_rename_boundary_preserves_unpublished_and_old_destinations() {
    for mode in [
        ArtifactMode::Link,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    ] {
        for update in [false, true] {
            let f = Fixture::new();
            let destination = f.destination.with_file_name("alias");
            let mut writer = f.writer();
            if update {
                writer
                    .publish(
                        &f.key(),
                        PhysicalSource::ExternalSource(&f.source),
                        &f.destination,
                        NodeKind::File,
                        mode,
                    )
                    .unwrap();
            }
            writer.cancel_at.set(Some((ArtifactStage::BeforeRename, 1)));
            let failure = writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &destination,
                    NodeKind::File,
                    mode,
                )
                .unwrap_err();
            assert_eq!(failure.kind(), io::ErrorKind::Interrupted);
            assert_eq!(failure.secondary_failure(), None);
            assert!(fs::symlink_metadata(&destination).is_err());
            if update {
                assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
            }
            assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
            let state = writer.load().unwrap().unwrap();
            assert!(state.pending().is_some());
            assert_eq!(state.entries().len(), usize::from(update));
            drop(writer);
            let mut writer = f.writer();
            assert_eq!(
                writer.recover().unwrap(),
                if update {
                    ArtifactRecovery::CompletedUpdate
                } else {
                    ArtifactRecovery::CompletedCreate
                }
            );
            assert!(writer.load().unwrap().unwrap().pending().is_none());
            assert_eq!(fs::read(&destination).unwrap(), b"source remains");
            assert_eq!(fs::read(&f.source).unwrap(), b"source remains");
        }
    }
}

#[test]
fn private_relative_codec_preserves_root_and_nested_names_without_public_bypass() {
    for path in [PathBuf::new(), PathBuf::from("nested/name")] {
        let value = tree::RelativePath(path.clone());
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            serde_json::from_slice::<tree::RelativePath>(&bytes)
                .unwrap()
                .0,
            path
        );
        assert!(serde_json::from_slice::<NativePath>(&bytes).is_err());
    }
    for path in [
        tempfile::tempdir().unwrap().path().to_path_buf(),
        PathBuf::from("../outside"),
    ] {
        let bytes = serde_json::to_vec(&path).unwrap();
        assert!(serde_json::from_slice::<tree::RelativePath>(&bytes).is_err());
    }
}

fn check_fixture(
    writer: &ArtifactWriter,
    f: &Fixture,
    kind: NodeKind,
    mode: ArtifactMode,
) -> ArtifactResult<PublicationCheck> {
    writer.check_publication(
        &f.key(),
        PhysicalSource::ExternalSource(&f.source),
        &f.destination,
        kind,
        mode,
    )
}

#[test]
fn publication_check_readable_proofs_are_read_only_at_exact_manifest_cap() {
    for mode in [
        ArtifactMode::Link,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    ] {
        for kind in [NodeKind::File, NodeKind::Directory] {
            let f = Fixture::new();
            if kind == NodeKind::Directory {
                fs::remove_file(&f.source).unwrap();
                fs::create_dir(&f.source).unwrap();
                fs::create_dir(f.source.join("empty")).unwrap();
                fs::write(f.source.join("empty-file"), b"").unwrap();
                for i in 0..16 {
                    fs::write(f.source.join(i.to_string()), vec![i as u8; 64 * 1024]).unwrap();
                }
            }
            let mut writer = f.writer();
            writer
                .publish(
                    &f.key(),
                    PhysicalSource::ExternalSource(&f.source),
                    &f.destination,
                    kind,
                    mode,
                )
                .unwrap();
            let manifest = fs::read(f.store.manifest_path()).unwrap();
            writer.store.limits.max_document_bytes = manifest.len();
            let before = fs::symlink_metadata(&f.destination).unwrap();
            let siblings = fs::read_dir(f.destination.parent().unwrap())
                .unwrap()
                .count();
            assert_eq!(
                check_fixture(&writer, &f, kind, mode).unwrap(),
                PublicationCheck::VerifiedUnchanged
            );

            assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
            let after = fs::symlink_metadata(&f.destination).unwrap();
            assert_eq!(before.modified().unwrap(), after.modified().unwrap());
            assert_eq!(before.permissions(), after.permissions());
            assert_eq!(
                fs::read_dir(f.destination.parent().unwrap())
                    .unwrap()
                    .count(),
                siblings
            );
            assert!(writer.load().unwrap().unwrap().pending().is_none());
        }
    }
}

#[test]
fn publication_check_reads_content_and_later_publish_revalidates() {
    let f = Fixture::new();
    let mut writer = f.writer();
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode).unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
    let modified = fs::metadata(&f.source).unwrap().modified().unwrap();
    fs::write(&f.source, b"source changes").unwrap(); // Same length; restored mtime is not proof.
    fs::File::options()
        .write(true)
        .open(&f.source)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let manifest = fs::read(f.store.manifest_path()).unwrap();
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode).unwrap(),
        PublicationCheck::NeedsPublication
    );

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
    assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
    assert_eq!(
        copy_fixture(&mut writer, &f, NodeKind::File).unwrap(),
        ArtifactChange::Updated
    );
    assert_eq!(fs::read(&f.destination).unwrap(), b"source changes");
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode).unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
}

#[test]
fn publication_check_new_candidates_and_modes_need_publication_without_creating_state() {
    let f = Fixture::new();
    let mut writer = f.writer();
    let copy = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, copy).unwrap(),
        PublicationCheck::NeedsPublication
    );

    assert!(!f.store.manifest_path().exists());
    assert!(!f.destination.exists());
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    let before = fs::read(f.store.manifest_path()).unwrap();
    for mode in [
        ArtifactMode::Link,
        ArtifactMode::Copy(CopyPolicy::FollowRegularFiles),
    ] {
        assert_eq!(
            check_fixture(&writer, &f, NodeKind::File, mode).unwrap(),
            PublicationCheck::NeedsPublication
        );
    }
    let other_source = f.source.with_file_name("other");
    fs::write(&other_source, b"source remains").unwrap();
    assert_eq!(
        writer
            .check_publication(
                &f.key(),
                PhysicalSource::ExternalSource(&other_source),
                &f.destination,
                NodeKind::File,
                copy
            )
            .unwrap(),
        PublicationCheck::NeedsPublication
    );
    let other_dest = f.destination.with_file_name("other");
    assert_eq!(
        writer
            .check_publication(
                &f.key(),
                PhysicalSource::ExternalSource(&f.source),
                &other_dest,
                NodeKind::File,
                copy
            )
            .unwrap(),
        PublicationCheck::NeedsPublication
    );

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    assert!(!other_dest.exists());
}

#[test]
fn publication_check_rejects_foreign_tamper_and_bad_inputs_without_effects() {
    let f = Fixture::new();
    let mut writer = f.writer();
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    fs::write(&f.destination, b"foreign").unwrap();
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode)
            .unwrap_err()
            .stage(),
        ArtifactStage::Conflict
    );
    fs::remove_file(&f.destination).unwrap();
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    let before = fs::read(f.store.manifest_path()).unwrap();
    fs::write(&f.destination, b"source changes").unwrap();
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode)
            .unwrap_err()
            .stage(),
        ArtifactStage::Conflict
    );
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::Directory, mode)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        writer
            .check_publication(
                &f.key(),
                PhysicalSource::ExternalSource(Path::new("relative")),
                &f.destination,
                NodeKind::File,
                mode
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Validate
    );
    let other_key = ArtifactKey::new("codex", "command", "other").unwrap();
    assert_eq!(
        writer
            .check_publication(
                &other_key,
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                mode
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Conflict
    );
    let oversized_key = ArtifactKey::new("codex", "command", &"x".repeat(1025)).unwrap();
    assert_eq!(
        writer
            .check_publication(
                &oversized_key,
                PhysicalSource::ExternalSource(&f.source),
                &f.destination,
                NodeKind::File,
                mode
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Validate
    );

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    assert_eq!(fs::read(&f.destination).unwrap(), b"source changes");
}

#[test]
fn publication_check_tree_composition_modes_and_copy_limits_are_observed() {
    let f = Fixture::new();
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::create_dir(f.source.join("empty")).unwrap();
    fs::write(f.source.join("file"), b"content").unwrap();
    let mut writer = f.writer();
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    let before = fs::read(f.store.manifest_path()).unwrap();
    fs::create_dir(f.source.join("new-empty")).unwrap();
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::Directory, mode).unwrap(),
        PublicationCheck::NeedsPublication
    );
    fs::remove_dir(f.source.join("new-empty")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(f.source.join("file"), fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(
            check_fixture(&writer, &f, NodeKind::Directory, mode).unwrap(),
            PublicationCheck::NeedsPublication
        );
    }
    writer.store.limits.copy.max_total_file_bytes = 1;
    assert!(check_fixture(&writer, &f, NodeKind::Directory, mode).is_err());

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn publication_check_private_restricted_receipt_needs_inspection_and_source_errors_propagate() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::write(f.source.join("file"), b"content").unwrap();
    let mut writer = f.writer();
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    copy_fixture(&mut writer, &f, NodeKind::Directory).unwrap();
    // This seeds conservative private persistent state, not a public creation fixture.
    let (_, _reset) = restricted_owned_tree(&writer, &f, false);
    let before = fs::read(f.store.manifest_path()).unwrap();
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::Directory, mode).unwrap(),
        PublicationCheck::NeedsPublication
    );
    assert_eq!(
        fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
        0
    );
    fs::set_permissions(f.source.join("file"), fs::Permissions::from_mode(0o0)).unwrap();
    let err = check_fixture(&writer, &f, NodeKind::Directory, mode).unwrap_err();
    fs::set_permissions(f.source.join("file"), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(err.stage(), ArtifactStage::SourceChanged);
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    writer.remove(&f.key()).unwrap(); // Existing normal Inspect handles the private receipt.

    assert!(!f.destination.exists());
}

#[test]
fn publication_check_pending_and_cancellation_never_mutate() {
    let f = Fixture::new();
    let mut writer = f.writer();
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
    let manifest = fs::read(f.store.manifest_path()).unwrap();
    writer.cancel_at.set(Some((ArtifactStage::Observe, 1)));
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
    drop(writer);
    let mut writer = f.writer();
    fs::write(&f.source, b"updated").unwrap();
    writer.fail_at.set(Some(ArtifactStage::Commit));
    assert!(copy_fixture(&mut writer, &f, NodeKind::File).is_err());
    let pending = fs::read(f.store.manifest_path()).unwrap();
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode)
            .unwrap_err()
            .stage(),
        ArtifactStage::RecoveryRequired
    );

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), pending);
    writer.recover().unwrap();
}

#[cfg(unix)]
#[test]
fn publication_check_source_alias_binding_and_link_content_contract() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let alias = f.source.with_file_name("alias");
    symlink(&f.source, &alias).unwrap();
    let mut writer = f.writer_with_timeout(Duration::from_secs(30));
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&alias),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    fs::write(&f.source, b"changed link source contents").unwrap();
    assert_eq!(
        writer
            .check_publication(
                &f.key(),
                PhysicalSource::ExternalSource(&alias),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
    let copy = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    assert_eq!(
        writer
            .check_publication(
                &f.key(),
                PhysicalSource::ExternalSource(&alias),
                &f.destination,
                NodeKind::File,
                copy
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::Validate
    );
    let other = f.source.with_file_name("other");
    fs::write(&other, b"other").unwrap();
    fs::remove_file(&alias).unwrap();
    symlink(&other, &alias).unwrap();
    assert_eq!(
        writer
            .check_publication(
                &f.key(),
                PhysicalSource::ExternalSource(&alias),
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap(),
        PublicationCheck::NeedsPublication
    );
}

#[cfg(unix)]
#[test]
fn publication_check_detects_external_alias_redirect_during_proof() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let alias = f.source.with_file_name("alias");
    let other = f.source.with_file_name("other");
    fs::write(&other, b"other").unwrap();
    symlink(&f.source, &alias).unwrap();
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&alias),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    let manifest = fs::read(f.store.manifest_path()).unwrap();
    let original_link = fs::read_link(&f.destination).unwrap();
    let changed_alias = alias.clone();
    // The test actor changes an external input after proof, not the owned publication.
    *writer.before_check_rebind.borrow_mut() = Some(Box::new(move || {
        fs::remove_file(&changed_alias).unwrap();
        symlink(other, changed_alias).unwrap();
    }));
    let error = writer
        .check_publication(
            &f.key(),
            PhysicalSource::ExternalSource(&alias),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap_err();
    assert_eq!(error.stage(), ArtifactStage::SourceChanged);

    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), manifest);
    assert_eq!(fs::read_link(&f.destination).unwrap(), original_link);
}

#[test]
fn publication_check_retains_writer_lock_and_needs_is_not_copy_preflight() {
    let f = Fixture::new();
    let mut writer = f.writer();
    // A new key does not promise future SourcePlan success or reserve a publication.
    writer.store.limits.copy.max_total_file_bytes = 1;
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    assert_eq!(
        check_fixture(&writer, &f, NodeKind::File, mode).unwrap(),
        PublicationCheck::NeedsPublication
    );
    let error = f
        .store
        .writer(Duration::from_millis(10), &Cancellation::default())
        .unwrap_err();
    assert_eq!(error.stage(), ArtifactStage::Lock);

    assert!(!f.store.manifest_path().exists());
    assert!(copy_fixture(&mut writer, &f, NodeKind::File).is_err());
    assert!(!f.destination.exists());
}

mod native_stage_tests {
    use super::super::native_document::{Operation, Outcome, State, test_hooks};
    use super::*;
    use crate::native_document_file::{
        CapturedDocumentDirectory, NativeDocumentFileLimits, NativeDocumentObservation,
        NativeDocumentReceipt,
    };
    #[cfg(target_os = "linux")]
    use crate::native_file_identity::handle_id;
    use crate::native_file_identity::physical_id;
    use std::{fs::File, sync::Arc};

    fn context() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ))
    }
    fn capture(
        f: &Fixture,
        ctx: &SnapshotContext,
    ) -> (CapturedDocumentDirectory, NativeDocumentObservation) {
        let limits = NativeDocumentFileLimits::new(1024 * 1024, 65536, 256, 65536).unwrap();
        let parent =
            CapturedDocumentDirectory::capture(f.destination.parent().unwrap(), limits, ctx)
                .unwrap();
        let old = parent
            .read(f.destination.file_name().unwrap(), ctx)
            .unwrap();
        (parent, old)
    }
    fn pending(writer: &ArtifactWriter) -> Operation {
        match writer.document().unwrap().pending.unwrap() {
            Pending::NativeDocument(operation) => *operation,
            _ => panic!("unexpected Pending"),
        }
    }
    fn prepared<'w>(
        writer: &'w mut ArtifactWriter,
        key: &ArtifactKey,
        parent: &'w CapturedDocumentDirectory,
        old: &'w NativeDocumentObservation,
        desired: &'w [u8],
    ) -> NativeDocumentStage<'w> {
        writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                key,
                parent,
                old,
                desired,
            )
            .unwrap_or_else(|failure| panic!("{:?}", failure.error()))
    }
    fn published(stage: NativeDocumentStage<'_>) -> NativeDocumentResolution<'_> {
        stage
            .publish()
            .unwrap_or_else(|failure| panic!("{:?}", failure.error()))
    }
    // Generic pre-existing fixtures resume their own current operation. The
    // correlation tests below build requests independently before Core birth.
    fn recover_fixture<'w>(
        writer: &'w mut ArtifactWriter,
        parent: &'w CapturedDocumentDirectory,
    ) -> Result<NativeDocumentRecovery<'w>, ArtifactError> {
        let op = pending(writer);
        let scope = writer.store.scope.binding().unwrap();
        let identity = parent.directory_identity();
        writer.recover_native_document(
            NativeDocumentRecoveryRequest {
                operation_id: op.id.parse().unwrap(),
                key: &op.key,
                destination: &op.destination.0,
                parent_identity: &identity,
                previous: op.previous.as_ref(),
                scope: &scope,
            },
            parent,
        )
    }
    fn recovered<'w>(
        writer: &'w mut ArtifactWriter,
        parent: &'w CapturedDocumentDirectory,
    ) -> NativeDocumentResolution<'w> {
        resolution_from_recovery(
            recover_fixture(writer, parent).unwrap_or_else(|e| panic!("{e:?}")),
        )
    }

    fn resolution_from_recovery(
        result: NativeDocumentRecovery<'_>,
    ) -> NativeDocumentResolution<'_> {
        match result {
            NativeDocumentRecovery::Resolution(resolution) => resolution,
            NativeDocumentRecovery::Completion(_) => panic!("expected unacknowledged resolution"),
        }
    }
    fn completion_from_recovery(
        result: NativeDocumentRecovery<'_>,
    ) -> NativeDocumentCompletion<'_> {
        match result {
            NativeDocumentRecovery::Completion(completion) => completion,
            NativeDocumentRecovery::Resolution(_) => panic!("expected retained terminal"),
        }
    }

    fn leave_birthless_intent(
        writer: &mut ArtifactWriter,
        f: &Fixture,
        parent: &CapturedDocumentDirectory,
        old: &NativeDocumentObservation,
    ) {
        test_hooks::CREATE_DIRECTORY.with(|h| {
            *h.borrow_mut() = Some(Box::new(|_| Err(io::ErrorKind::Other.into())));
        });
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                parent,
                old,
                b"new",
            )
            .err()
            .unwrap();
        assert!(failure.resolution().is_none());
        drop(failure);
        assert!(matches!(pending(writer).state, State::Intent {}));
    }
    #[test]
    fn terminal_handoff_birthless_abort_is_durable_and_drop_never_clears() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        leave_birthless_intent(&mut writer, &f, &parent, &old);
        let op = pending(&writer);
        let completion = completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap());
        assert_eq!(completion.outcome(), NativeDocumentOutcome::NotPublished);
        assert!(completion.destination_receipt().is_none());
        drop(completion);
        assert!(matches!(
            pending(&writer).state,
            State::AbortedBeforeBirth {}
        ));
        assert!(!op.directory_path().unwrap().exists());
        drop(writer);
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f
            .fresh_store()
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .with_io_context(ctx)
            .unwrap();
        completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
            .release()
            .unwrap();
        assert!(writer.document().unwrap().pending.is_none());
        assert!(!f.destination.exists());
    }
    #[test]
    fn terminal_handoff_late_unknown_birth_prevents_abort_release_without_cleanup() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        leave_birthless_intent(&mut writer, &f, &parent, &old);
        let op = pending(&writer);
        let completion = completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap());
        let directory = op.directory_path().unwrap();
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("foreign"), b"keep").unwrap();
        let before = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            completion.release().unwrap_err().stage(),
            ArtifactStage::IncompleteStage
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert_eq!(fs::read(directory.join("foreign")).unwrap(), b"keep");
        assert!(recover_fixture(&mut writer, &parent).is_err());
    }
    #[test]
    fn terminal_handoff_cancel_before_acknowledge_preserves_resolved_without_cleanup() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let resolution = published(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
        let directory = parent
            .canonical_path()
            .join(format!(".graphit-doc-{}", resolution.id()));
        let before = fs::read(f.store.manifest_path()).unwrap();
        ctx.cancellation().cancel();
        assert_eq!(
            resolution.acknowledge().err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert!(matches!(pending(&writer).state, State::Resolved { .. }));
        assert!(directory.exists());
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    }
    #[cfg(unix)]
    #[test]
    fn terminal_handoff_retarget_before_acknowledge_preserves_resolved_without_cleanup() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let ctx = context();
        let alias = f.source.parent().unwrap().join("terminal-alias");
        symlink(f.destination.parent().unwrap(), &alias).unwrap();
        let other = f.source.parent().unwrap().join("terminal-other");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("foreign"), b"keep").unwrap();
        let parent = CapturedDocumentDirectory::capture(
            &alias,
            NativeDocumentFileLimits::new(1024 * 1024, 65536, 256, 65536).unwrap(),
            &ctx,
        )
        .unwrap();
        let old = parent
            .read(f.destination.file_name().unwrap(), &ctx)
            .unwrap();
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let resolution = published(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
        let directory = parent
            .canonical_path()
            .join(format!(".graphit-doc-{}", resolution.id()));
        let before = fs::read(f.store.manifest_path()).unwrap();
        fs::remove_file(&alias).unwrap();
        symlink(&other, &alias).unwrap();
        assert!(resolution.acknowledge().is_err());
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
        assert!(matches!(pending(&writer).state, State::Resolved { .. }));
        assert!(directory.exists());
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
        assert_eq!(fs::read(other.join("foreign")).unwrap(), b"keep");
    }
    #[test]
    fn terminal_handoff_cancel_before_release_retains_both_terminal_states() {
        for applied in [false, true] {
            let f = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
            let completion = if applied {
                published(prepared(&mut writer, &f.key(), &parent, &old, b"new"))
                    .acknowledge()
                    .unwrap()
            } else {
                leave_birthless_intent(&mut writer, &f, &parent, &old);
                completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
            };
            let before = fs::read(f.store.manifest_path()).unwrap();
            ctx.cancellation().cancel();
            assert_eq!(
                completion.release().unwrap_err().kind(),
                io::ErrorKind::Interrupted
            );
            assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
            assert!(matches!(
                pending(&writer).state,
                State::Cleaned { .. } | State::AbortedBeforeBirth {}
            ));
            drop(writer);
            let ctx = context();
            let (parent, _) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
                .release()
                .unwrap();
            assert!(writer.document().unwrap().pending.is_none());
            if applied {
                assert_eq!(fs::read(&f.destination).unwrap(), b"new");
            } else {
                assert!(!f.destination.exists());
            }
        }
    }
    #[test]
    fn terminal_handoff_cancel_after_cleanup_retains_acknowledged_for_fresh_finish() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let resolution = published(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
        let directory = parent
            .canonical_path()
            .join(format!(".graphit-doc-{}", resolution.id()));
        test_hooks::AFTER_REMOVE_DIRECTORY.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                ctx.cancellation().cancel();
                Ok(())
            }));
        });
        assert_eq!(
            resolution.acknowledge().err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(matches!(pending(&writer).state, State::Acknowledged { .. }));
        assert!(!directory.exists());
        drop(writer);
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let completion = completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap());
        assert_eq!(completion.outcome(), NativeDocumentOutcome::Applied);
        completion.release().unwrap();
        assert_eq!(fs::read(f.destination).unwrap(), b"new");
    }
    #[test]
    fn terminal_handoff_cancel_after_terminal_save_recovers_actual_saved_abort() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        leave_birthless_intent(&mut writer, &f, &parent, &old);
        test_hooks::AFTER_TERMINAL_RECORD.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || ctx.cancellation().cancel()));
        });
        assert_eq!(
            recover_fixture(&mut writer, &parent).err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(matches!(
            pending(&writer).state,
            State::AbortedBeforeBirth {}
        ));
        drop(writer);
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
            .release()
            .unwrap();
        assert!(writer.document().unwrap().pending.is_none());
    }
    #[cfg(unix)]
    #[test]
    fn terminal_handoff_native_save_error_after_effect_keeps_primary_and_durable_terminal() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        leave_birthless_intent(&mut writer, &f, &parent, &old);
        super::super::catalog::test_hooks::NATIVE.with(|h| {
            *h.borrow_mut() = Some(Box::new(|stage, destination, _| {
                fs::rename(stage, destination)?;
                Err(io::ErrorKind::PermissionDenied.into())
            }));
        });
        assert_eq!(
            recover_fixture(&mut writer, &parent).err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(matches!(
            pending(&writer).state,
            State::AbortedBeforeBirth {}
        ));
        completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
            .release()
            .unwrap();
        assert!(writer.document().unwrap().pending.is_none());
    }
    #[test]
    fn terminal_handoff_release_rejects_same_id_foreign_key_or_changed_phase() {
        for changed_phase in [false, true] {
            let f = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            drop(published(prepared(
                &mut writer,
                &f.key(),
                &parent,
                &old,
                b"new",
            )));
            let mut altered = writer.document().unwrap();
            let completion = recovered(&mut writer, &parent).acknowledge().unwrap();
            let Some(Pending::NativeDocument(op)) = &mut altered.pending else {
                panic!("own operation")
            };
            if changed_phase {
                op.state = State::Intent {};
            } else {
                op.key = ArtifactKey::new("kiro", "agent", "foreign").unwrap();
            }
            let manifest = f.store.manifest_path();
            let encoded = codec::encode(&altered, f.store.limits.max_document_bytes).unwrap();
            fs::write(&manifest, &encoded).unwrap();
            assert!(f.store.load().is_ok());
            assert_eq!(
                completion.release().unwrap_err().stage(),
                ArtifactStage::RecoveryConflict
            );
            assert_eq!(fs::read(&manifest).unwrap(), encoded);
            assert_eq!(fs::read(&f.destination).unwrap(), b"new");
        }
    }

    #[test]
    fn native_stage_recovery_correlates_each_expected_operation_component() {
        let f = Fixture::new();
        let other = Fixture::new();
        fs::write(&f.destination, b"old").unwrap();
        fs::write(&other.destination, b"old").unwrap();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let (other_parent, other_old) = capture(&other, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        // These expected values exist before Core preparation, independently
        // of the Core pending diagnostics being tested.
        let id = NativeDocumentOperationId::generate();
        let key = f.key();
        let scope = writer.store.scope.binding().unwrap();
        let identity = parent.directory_identity();
        let other_identity = other_parent.directory_identity();
        let other_key = ArtifactKey::new("kiro-v2", "command", "org/other").unwrap();
        let other_destination = parent.canonical_path().join("other");
        drop(
            writer
                .prepare_native_document(id, &key, &parent, &old, b"new!")
                .unwrap_or_else(|failure| panic!("{:?}", failure.error())),
        );
        let op = pending(&writer);
        let stage_path = op.file_path().unwrap();
        let catalog = fs::read(f.store.manifest_path()).unwrap();
        let stage_id = physical_id(&stage_path, &fs::metadata(&stage_path).unwrap()).unwrap();

        let mut changed_receipts = Vec::new();
        for component in ["length", "content", "access"] {
            let mut value = serde_json::to_value(old.receipt().unwrap()).unwrap();
            if component == "length" {
                value[component] = serde_json::json!(4);
            } else {
                let byte = value[component][0].as_u64().unwrap() ^ 1;
                value[component][0] = serde_json::json!(byte);
            }
            changed_receipts.push(serde_json::from_value::<NativeDocumentReceipt>(value).unwrap());
        }
        for case in 0..9 {
            let mut expected = NativeDocumentRecoveryRequest {
                operation_id: id,
                key: &key,
                destination: &f.destination,
                parent_identity: &identity,
                previous: old.receipt(),
                scope: &scope,
            };
            match case {
                0 => expected.operation_id = NativeDocumentOperationId::generate(),
                1 => expected.key = &other_key,
                2 => expected.destination = &other_destination,
                3 => expected.parent_identity = &other_identity,
                4 => expected.previous = None,
                // Equal old bytes in a different physical file are not a match.
                5 => expected.previous = other_old.receipt(),
                6..=8 => expected.previous = Some(&changed_receipts[case - 6]),
                _ => unreachable!(),
            }
            let failure = writer
                .recover_native_document(expected, &parent)
                .err()
                .unwrap();
            assert_eq!(failure.stage(), ArtifactStage::RecoveryConflict);
            assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
            assert_eq!(fs::read(f.store.manifest_path()).unwrap(), catalog);
            assert_eq!(fs::read(&stage_path).unwrap(), b"new!");
            assert!(
                physical_id(&stage_path, &fs::metadata(&stage_path).unwrap()).unwrap() == stage_id
            );
            assert_eq!(fs::read(&f.destination).unwrap(), b"old");
        }
        let resolution = resolution_from_recovery(
            writer
                .recover_native_document(
                    NativeDocumentRecoveryRequest {
                        operation_id: id,
                        key: &key,
                        destination: &f.destination,
                        parent_identity: &identity,
                        previous: old.receipt(),
                        scope: &scope,
                    },
                    &parent,
                )
                .unwrap(),
        );
        assert_eq!(resolution.outcome(), NativeDocumentOutcome::NotPublished);
        resolution.acknowledge().unwrap().release().unwrap();
        assert!(writer.document().unwrap().pending.is_none());
        assert!(!op.directory_path().unwrap().exists());
        assert_eq!(fs::read(&f.destination).unwrap(), b"old");
    }

    #[test]
    fn native_stage_recovery_checks_expected_scope_without_replacing_captured_authority() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let id = NativeDocumentOperationId::generate();
        let key = f.key();
        let identity = parent.directory_identity();
        let scope = writer.store.scope.binding().unwrap();
        let root = f.source.parent().unwrap();
        let limits = ScopeLimits::new(1, 16 * 1024, 1024).unwrap();
        assert!(
            ScopeBindingRecord::pure_restore(
                &serde_json::to_vec(&scope).unwrap(),
                limits,
                256 * 1024
            )
            .is_ok(),
            "original scope roundtrip"
        );
        let mut wrong_scopes = Vec::new();
        for component in [
            "owner",
            "brand",
            "owner_root",
            "allowed_roots",
            "directories",
        ] {
            let mut value = serde_json::to_value(&scope).unwrap();
            // Keep a valid minimal closed binding so each mismatch reaches
            // recovery rather than being rejected by the data decoder.
            let owner_root: NativePath =
                serde_json::from_value(value["owner_root"].clone()).unwrap();
            value["directories"]
                .as_array_mut()
                .unwrap()
                .retain(|binding| {
                    serde_json::from_value::<NativePath>(binding["path"].clone()).unwrap()
                        == owner_root
                });
            assert_eq!(
                value["directories"].as_array().unwrap().len(),
                1,
                "owner observation retained"
            );
            match component {
                "owner" => {
                    value["owner"]["project_id"] = serde_json::json!("01ARZ3NDEKTSV4RRFFQ69G5FAW");
                    value["runtime_root"] = serde_json::to_value(NativePath(
                        serde_json::from_value::<NativePath>(value["storage_root"].clone())
                            .unwrap()
                            .0
                            .join("projects/01ARZ3NDEKTSV4RRFFQ69G5FAW"),
                    ))
                    .unwrap();
                }
                "brand" => {
                    value["brand"] = serde_json::json!("other");
                }
                "owner_root" => {
                    let elsewhere = root.join("elsewhere");
                    let encoded = serde_json::to_value(NativePath(elsewhere.clone())).unwrap();
                    value["owner_root"] = encoded.clone();
                    value["directories"][0]["path"] = encoded;
                }
                "allowed_roots" => {
                    value["allowed_roots"][0] =
                        serde_json::to_value(NativePath(root.join("other-adapter"))).unwrap()
                }
                "directories" => {
                    value["directories"][0]["id"] =
                        serde_json::to_value(crate::native_file_identity::PhysicalId::largest())
                            .unwrap()
                }
                _ => unreachable!(),
            }
            let encoded = serde_json::to_vec(&value).unwrap();
            wrong_scopes.push(
                ScopeBindingRecord::pure_restore(&encoded, limits, 256 * 1024)
                    .unwrap_or_else(|_| panic!("invalid {component} scope fixture")),
            );
        }
        drop(
            writer
                .prepare_native_document(id, &key, &parent, &old, b"new")
                .unwrap_or_else(|failure| panic!("{:?}", failure.error())),
        );
        let op = pending(&writer);
        let catalog = fs::read(f.store.manifest_path()).unwrap();

        for wrong in &wrong_scopes {
            let failure = writer
                .recover_native_document(
                    NativeDocumentRecoveryRequest {
                        operation_id: id,
                        key: &key,
                        destination: &f.destination,
                        parent_identity: &identity,
                        previous: old.receipt(),
                        scope: wrong,
                    },
                    &parent,
                )
                .err()
                .unwrap();
            assert_eq!(failure.stage(), ArtifactStage::RecoveryConflict);
            assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
            assert_eq!(fs::read(f.store.manifest_path()).unwrap(), catalog);
            assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"new");
            assert!(!f.destination.exists());
        }
        // The pre-birth binding permits legitimate additional observations,
        // while the current captured scope and catalogue remain authoritative.
        resolution_from_recovery(
            writer
                .recover_native_document(
                    NativeDocumentRecoveryRequest {
                        operation_id: id,
                        key: &key,
                        destination: &f.destination,
                        parent_identity: &identity,
                        previous: old.receipt(),
                        scope: &scope,
                    },
                    &parent,
                )
                .unwrap(),
        )
        .acknowledge()
        .unwrap()
        .release()
        .unwrap();
    }

    #[test]
    fn native_stage_recovery_mismatch_cannot_clear_intent_or_clean_terminal_operations() {
        for phase in ["intent", "aborted", "resolved", "acknowledged", "cleaned"] {
            let f = Fixture::new();
            let other = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
            let id = NativeDocumentOperationId::generate();
            let key = f.key();
            let identity = parent.directory_identity();
            let scope = writer.store.scope.binding().unwrap();
            let foreign_scope = other.store.scope.binding().unwrap();
            let directory = parent.canonical_path().join(format!(".graphit-doc-{id}"));
            let sentinel = directory.join("foreign");
            if matches!(phase, "intent" | "aborted") {
                test_hooks::CREATE_DIRECTORY.with(|hook| {
                    *hook.borrow_mut() =
                        Some(Box::new(|_| Err(io::ErrorKind::PermissionDenied.into())))
                });
                assert!(
                    writer
                        .prepare_native_document(id, &key, &parent, &old, b"new")
                        .is_err()
                );
                assert!(!directory.exists());
                assert!(matches!(pending(&writer).state, State::Intent {}));
                if phase == "aborted" {
                    drop(completion_from_recovery(
                        recover_fixture(&mut writer, &parent).unwrap(),
                    ));
                    assert!(matches!(
                        pending(&writer).state,
                        State::AbortedBeforeBirth {}
                    ));
                }
            } else {
                let stage = writer
                    .prepare_native_document(id, &key, &parent, &old, b"new")
                    .unwrap_or_else(|failure| panic!("{:?}", failure.error()));
                let resolution = stage
                    .discard()
                    .unwrap_or_else(|failure| panic!("{:?}", failure.error()));
                if phase == "acknowledged" {
                    fs::write(&sentinel, b"keep").unwrap();
                    assert!(resolution.acknowledge().is_err());
                    assert!(matches!(pending(&writer).state, State::Acknowledged { .. }));
                } else if phase == "cleaned" {
                    drop(resolution.acknowledge().unwrap());
                    assert!(matches!(pending(&writer).state, State::Cleaned { .. }));
                    assert!(!directory.exists());
                } else {
                    drop(resolution);
                    assert!(matches!(pending(&writer).state, State::Resolved { .. }));
                }
            }
            let op = pending(&writer);
            let catalog = fs::read(f.store.manifest_path()).unwrap();

            let other_key = ArtifactKey::new("kiro-v3-preview", "command", "org/other").unwrap();
            for case in 0..3 {
                let expected = NativeDocumentRecoveryRequest {
                    operation_id: if case == 0 {
                        NativeDocumentOperationId::generate()
                    } else {
                        id
                    },
                    key: if case == 1 { &other_key } else { &key },
                    destination: &f.destination,
                    parent_identity: &identity,
                    previous: old.receipt(),
                    scope: if case == 2 { &foreign_scope } else { &scope },
                };
                let failure = writer
                    .recover_native_document(expected, &parent)
                    .err()
                    .unwrap();
                assert_eq!(failure.stage(), ArtifactStage::RecoveryConflict);
                assert_eq!(fs::read(f.store.manifest_path()).unwrap(), catalog);
                assert!(pending(&writer) == op);
                assert!(!f.destination.exists());
                if matches!(phase, "resolved" | "acknowledged") {
                    assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"new");
                } else {
                    assert!(!op.directory_path().unwrap().exists());
                }
                if phase == "acknowledged" {
                    assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
                }
            }
            if phase == "acknowledged" {
                fs::remove_file(&sentinel).unwrap();
            }
            let result = writer
                .recover_native_document(
                    NativeDocumentRecoveryRequest {
                        operation_id: id,
                        key: &key,
                        destination: &f.destination,
                        parent_identity: &identity,
                        previous: old.receipt(),
                        scope: &scope,
                    },
                    &parent,
                )
                .unwrap();
            match result {
                NativeDocumentRecovery::Resolution(resolution) => {
                    resolution.acknowledge().unwrap().release().unwrap()
                }
                NativeDocumentRecovery::Completion(completion) => completion.release().unwrap(),
            }
            assert!(writer.document().unwrap().pending.is_none());
            assert!(!directory.exists());
        }
    }

    #[test]
    fn native_stage_missing_and_existing_publish_new_identity_without_membership() {
        for existing in [false, true] {
            let f = Fixture::new();
            if existing {
                fs::write(&f.destination, b"old").unwrap();
            }
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
            let key = f.key();
            let stage = prepared(&mut writer, &key, &parent, &old, b"new!");
            assert_eq!(
                fs::read(&f.destination).ok(),
                if existing {
                    Some(b"old".to_vec())
                } else {
                    None
                }
            );
            let resolved = published(stage);
            assert_eq!(resolved.outcome(), NativeDocumentOutcome::Applied);
            assert_eq!(fs::read(&f.destination).unwrap(), b"new!");
            let result_id =
                physical_id(&f.destination, &fs::metadata(&f.destination).unwrap()).unwrap();
            assert!(old.receipt().is_none_or(|old| old.file_id() != &result_id));
            drop(resolved);
            let op = pending(&writer);
            assert!(matches!(
                op.state,
                State::Resolved {
                    outcome: Outcome::Applied { .. },
                    ..
                }
            ));
            assert!(writer.document().unwrap().entries.is_empty());
            assert_eq!(
                writer.recover().unwrap_err().stage(),
                ArtifactStage::RecoveryRequired
            );
            assert_eq!(
                writer.forget(&key).unwrap_err().stage(),
                ArtifactStage::RecoveryRequired
            );
            recovered(&mut writer, &parent)
                .acknowledge()
                .unwrap()
                .release()
                .unwrap();
            assert!(writer.document().unwrap().pending.is_none());
            assert!(!op.directory_path().unwrap().exists());
            assert_eq!(fs::read(&f.destination).unwrap(), b"new!");
        }
    }

    #[test]
    fn native_stage_other_context_rejected_before_intent_or_birth() {
        let f = Fixture::new();
        let first = context();
        let second = context();
        let (parent, old) = capture(&f, &first);
        let mut writer = f.writer().with_io_context(second.clone()).unwrap();
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                b"new",
            )
            .err()
            .unwrap();
        assert_eq!(failure.error().stage(), ArtifactStage::Validate);
        assert!(failure.resolution().is_none());
        drop(failure);
        assert!(writer.load().unwrap().is_none());
        assert_eq!(
            fs::read_dir(f.destination.parent().unwrap())
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn native_stage_empty_document_uses_positive_limits_and_finite_eof_slots() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let resolved = published(prepared(&mut writer, &f.key(), &parent, &old, b""));
        assert_eq!(resolved.outcome(), NativeDocumentOutcome::Applied);
        assert_eq!(fs::read(&f.destination).unwrap(), b"");
    }
    #[test]
    fn native_stage_partial_cancel_records_actual_prefix_and_fresh_abort_cleanup() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let cancelled = ctx.cancellation().clone();
        test_hooks::AFTER_WRITE
            .with(|h| *h.borrow_mut() = Some(Box::new(move || cancelled.cancel())));
        let bytes = vec![91; 128 * 1024];
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                &bytes,
            )
            .err()
            .unwrap();
        assert_eq!(
            failure.resolution().unwrap().outcome(),
            NativeDocumentOutcome::NotPublished
        );
        assert_eq!(failure.error().kind(), io::ErrorKind::Interrupted);
        drop(failure);
        let op = pending(&writer);
        assert!(
            matches!(&op.state, State::Resolved { stage, outcome: Outcome::NotPublished {}, .. } if stage.length() == 65536)
        );
        assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), vec![91; 65536]);

        assert!(!f.destination.exists());
        drop(writer);
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let resolution = recovered(&mut writer, &parent);
        assert_eq!(resolution.outcome(), NativeDocumentOutcome::NotPublished);
        resolution.acknowledge().unwrap().release().unwrap();
        assert!(!op.directory_path().unwrap().exists());
    }
    #[test]
    fn native_stage_cancel_after_effect_records_applied_without_budget_reset() {
        let f = Fixture::new();
        let id = persist_operation_id(f.source.parent().unwrap());
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let cancelled = ctx.cancellation().clone();
        test_hooks::AFTER_RENAME
            .with(|h| *h.borrow_mut() = Some(Box::new(move || cancelled.cancel())));
        let stage = writer
            .prepare_native_document(id, &f.key(), &parent, &old, b"new")
            .unwrap_or_else(|failure| panic!("{:?}", failure.error()));
        let resolved = published(stage);
        assert_eq!(resolved.id(), id.to_string());
        assert_eq!(resolved.outcome(), NativeDocumentOutcome::Applied);

        let before_ack = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            resolved.acknowledge().err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(matches!(pending(&writer).state, State::Resolved { .. }));
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before_ack);
        assert_eq!(pending(&writer).id, id.to_string());
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    }
    #[test]
    fn native_stage_error_after_actual_rename_reports_error_and_applied_together() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        test_hooks::NATIVE.with(|h| {
            *h.borrow_mut() = Some(Box::new(|stage, dest, exists| {
                assert!(!exists);
                graphit_fs::rename_exclusive(stage, dest)?;
                Err(io::ErrorKind::PermissionDenied.into())
            }))
        });
        let failure = prepared(&mut writer, &f.key(), &parent, &old, b"new")
            .publish()
            .err()
            .unwrap();
        assert_eq!(failure.error().stage(), ArtifactStage::Publish);
        assert_eq!(failure.error().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            failure.resolution().unwrap().outcome(),
            NativeDocumentOutcome::Applied
        );
        failure
            .into_parts()
            .1
            .unwrap()
            .acknowledge()
            .unwrap()
            .release()
            .unwrap();
        assert!(writer.document().unwrap().pending.is_none());
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    }
    #[test]
    fn native_stage_native_error_without_effect_proves_unchanged_and_cleanup_preserves_old() {
        let f = Fixture::new();
        fs::write(&f.destination, b"old").unwrap();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        test_hooks::NATIVE.with(|h| {
            *h.borrow_mut() = Some(Box::new(|_, _, _| {
                Err(io::ErrorKind::PermissionDenied.into())
            }))
        });
        let failure = prepared(&mut writer, &f.key(), &parent, &old, b"new")
            .publish()
            .err()
            .unwrap();
        assert_eq!(
            failure.resolution().unwrap().outcome(),
            NativeDocumentOutcome::Unchanged
        );
        failure
            .into_parts()
            .1
            .unwrap()
            .acknowledge()
            .unwrap()
            .release()
            .unwrap();
        assert_eq!(fs::read(&f.destination).unwrap(), b"old");
        assert!(writer.document().unwrap().pending.is_none());
    }
    #[test]
    fn native_stage_concurrent_creator_is_preserved_and_outcome_stays_uncertain() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let destination = f.destination.clone();
        test_hooks::BEFORE_RENAME.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || fs::write(destination, b"user").unwrap()))
        });
        let failure = prepared(&mut writer, &f.key(), &parent, &old, b"new")
            .publish()
            .err()
            .unwrap();
        assert_eq!(failure.error().kind(), io::ErrorKind::AlreadyExists);
        assert!(failure.resolution().is_none());
        drop(failure);
        assert!(matches!(pending(&writer).state, State::Publishing { .. }));
        assert_eq!(fs::read(&f.destination).unwrap(), b"user");
        assert_eq!(
            recover_fixture(&mut writer, &parent).err().unwrap().stage(),
            ArtifactStage::RecoveryConflict
        );
    }
    #[test]
    fn native_stage_source_edit_before_compare_blocks_but_cooperative_window_is_explicit() {
        for after_compare in [false, true] {
            let f = Fixture::new();
            fs::write(&f.destination, b"old").unwrap();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            let stage = prepared(&mut writer, &f.key(), &parent, &old, b"new");
            if after_compare {
                let destination = f.destination.clone();
                test_hooks::BEFORE_RENAME.with(|h| {
                    *h.borrow_mut() =
                        Some(Box::new(move || fs::write(destination, b"user").unwrap()))
                });
                published(stage).acknowledge().unwrap().release().unwrap();
                assert_eq!(fs::read(&f.destination).unwrap(), b"new");
            } else {
                fs::write(&f.destination, b"user").unwrap();
                let failure = stage.publish().err().unwrap();
                assert_eq!(failure.error().stage(), ArtifactStage::Conflict);
                assert_eq!(
                    failure.resolution().unwrap().outcome(),
                    NativeDocumentOutcome::NotPublished
                );
                failure
                    .into_parts()
                    .1
                    .unwrap()
                    .acknowledge()
                    .unwrap()
                    .release()
                    .unwrap();
                assert_eq!(fs::read(&f.destination).unwrap(), b"user");
            }
        }
    }
    #[test]
    fn native_stage_prepared_fresh_recovery_aborts_without_replaying_native_effect() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        drop(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
        let op = pending(&writer);
        assert!(matches!(op.state, State::Prepared { .. }));
        drop(writer);
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let resolution = recovered(&mut writer, &parent);
        assert_eq!(resolution.outcome(), NativeDocumentOutcome::NotPublished);
        assert!(!f.destination.exists());
        resolution.acknowledge().unwrap().release().unwrap();
        assert!(!op.directory_path().unwrap().exists());
    }
    #[test]
    fn native_stage_partial_birth_error_never_adopts_residue() {
        for directory_birth in [true, false] {
            let f = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            if directory_birth {
                test_hooks::CREATE_DIRECTORY.with(|h| {
                    *h.borrow_mut() = Some(Box::new(|path| {
                        drop(graphit_fs::create_private_directory(path)?);
                        Err(io::ErrorKind::PermissionDenied.into())
                    }))
                });
            } else {
                test_hooks::CREATE_FILE.with(|h| {
                    *h.borrow_mut() = Some(Box::new(|path| {
                        let mut file = graphit_fs::create_private_file(path)?;
                        use std::io::Write;
                        file.write_all(b"residue")?;
                        Err(io::ErrorKind::PermissionDenied.into())
                    }))
                });
            }
            let failure = writer
                .prepare_native_document(
                    NativeDocumentOperationId::generate(),
                    &f.key(),
                    &parent,
                    &old,
                    b"new",
                )
                .err()
                .unwrap();
            assert!(failure.resolution().is_none());
            drop(failure);
            let op = pending(&writer);
            assert!(matches!(
                op.state,
                State::Intent {} | State::Directory { .. }
            ));
            assert_eq!(
                recover_fixture(&mut writer, &parent).err().unwrap().stage(),
                ArtifactStage::IncompleteStage
            );
            assert!(op.directory_path().unwrap().exists());
            if !directory_birth {
                assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"residue");
            }
        }
    }
    #[test]
    fn native_stage_foreign_child_blocks_ack_cleanup_and_fresh_ack_retry_is_exact() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let resolved = published(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
        let directory = parent
            .canonical_path()
            .join(format!(".graphit-doc-{}", resolved.id()));
        let foreign = directory.join("foreign");
        fs::write(&foreign, b"keep").unwrap();
        assert_eq!(
            resolved.acknowledge().err().unwrap().stage(),
            ArtifactStage::Conflict
        );
        assert!(matches!(pending(&writer).state, State::Acknowledged { .. }));
        assert_eq!(fs::read(&foreign).unwrap(), b"keep");
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
        drop(writer);
        fs::remove_file(&foreign).unwrap();
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
            .release()
            .unwrap();
        assert!(writer.document().unwrap().pending.is_none());
        assert!(!directory.exists());
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_stage_preserves_actual_mode_and_named_acl_on_same_stage_handle() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let f = Fixture::new();
        fs::write(&f.destination, b"old").unwrap();
        fs::set_permissions(&f.destination, fs::Permissions::from_mode(0o640)).unwrap();
        let mut acl = 2u32.to_le_bytes().to_vec();
        for (tag, permissions, id) in [
            (1u16, 6u16, u32::MAX),
            (2, 4, 12345),
            (4, 4, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            acl.extend(tag.to_le_bytes());
            acl.extend(permissions.to_le_bytes());
            acl.extend(id.to_le_bytes());
        }
        let before = File::open(&f.destination).unwrap();
        rustix::fs::fsetxattr(
            &before,
            "system.posix_acl_access",
            &acl,
            rustix::fs::XattrFlags::empty(),
        )
        .unwrap();
        drop(before);
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let old_id = old.receipt().unwrap().file_id().clone();
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        published(prepared(&mut writer, &f.key(), &parent, &old, b"new"))
            .acknowledge()
            .unwrap()
            .release()
            .unwrap();
        assert_eq!(fs::metadata(&f.destination).unwrap().mode() & 0o7777, 0o640);
        let actual = File::open(&f.destination).unwrap();
        assert!(handle_id(&actual).unwrap() != old_id);
        let mut bytes = [0u8; 256];
        let count =
            rustix::fs::fgetxattr(&actual, "system.posix_acl_access", &mut bytes[..]).unwrap();
        assert_eq!(&bytes[..count], acl.as_slice());
    }
    #[test]
    fn native_stage_failed_file_record_grants_no_resolution_and_preserves_sdk_residue() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        writer.fail_at.set(Some(ArtifactStage::RecordProof));
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                b"new",
            )
            .err()
            .unwrap();
        assert_eq!(failure.error().stage(), ArtifactStage::RecordProof);
        assert!(failure.resolution().is_none());
        drop(failure);
        let op = pending(&writer);
        assert!(matches!(op.state, State::Directory { .. }));
        assert!(op.file_path().unwrap().is_file());
        assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"");

        assert_eq!(
            recover_fixture(&mut writer, &parent).err().unwrap().stage(),
            ArtifactStage::IncompleteStage
        );
        assert!(op.file_path().unwrap().exists());
    }
    #[test]
    fn native_stage_stage_error_after_real_rename_still_inspects_destination() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        test_hooks::NATIVE.with(|h| {
            *h.borrow_mut() = Some(Box::new(|stage, dest, _| {
                graphit_fs::rename_exclusive(stage, dest)?;
                fs::write(stage, b"foreign")?;
                Err(io::ErrorKind::PermissionDenied.into())
            }))
        });
        let failure = prepared(&mut writer, &f.key(), &parent, &old, b"new")
            .publish()
            .err()
            .unwrap();
        assert_eq!(failure.error().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            failure.error().secondary_failure(),
            Some(io::ErrorKind::InvalidData)
        );
        assert!(failure.resolution().is_none());
        // Prepared read + final Stage read + destination completion despite Stage Err.

        drop(failure);
        let op = pending(&writer);
        assert!(matches!(op.state, State::Publishing { .. }));
        assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"foreign");
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    }
    #[test]
    fn native_stage_stream_error_still_revalidates_parent_and_preserves_secondary() {
        let f = Fixture::new();
        fs::write(&f.destination, b"real").unwrap();
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut file = File::open(&f.destination).unwrap();
        let moved = f.destination.parent().unwrap().with_extension("moved");
        let failure = crate::native_document_file::inspect_file(
            &f.destination,
            &mut file,
            parent.limits(),
            ctx.native_inspection(4).unwrap(),
            Some(b"fake"),
            |pass| {
                fs::rename(parent.canonical_path(), &moved)?;
                parent.revalidate_io(pass)
            },
        )
        .err()
        .unwrap();
        let failure = at(ArtifactStage::Observe, failure);
        assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
        assert_eq!(failure.secondary_failure(), Some(io::ErrorKind::NotFound));
        // Capture plus the failed streaming compare.
        assert_eq!(
            fs::read(moved.join(f.destination.file_name().unwrap())).unwrap(),
            b"real"
        );
    }
    #[test]
    fn native_stage_prepared_receipt_is_frozen_across_content_access_links_and_identity_changes() {
        for mutation in 0..5 {
            let f = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            let stage = prepared(&mut writer, &f.key(), &parent, &old, b"new");
            let path = parent
                .canonical_path()
                .join(format!(".graphit-doc-{}", stage.id()))
                .join("document");
            match mutation {
                0 => fs::write(&path, b"edit").unwrap(),
                1 => {
                    fs::rename(&path, path.with_extension("original")).unwrap();
                    fs::write(&path, b"new").unwrap();
                }
                2 => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
                    }
                    #[cfg(windows)]
                    {
                        fs::write(&path, b"edit").unwrap();
                    }
                }
                3 => fs::hard_link(&path, path.with_extension("other")).unwrap(),
                _ => {
                    #[cfg(unix)]
                    {
                        let link = path.with_extension("source");
                        fs::rename(&path, &link).unwrap();
                        std::os::unix::fs::symlink(link, &path).unwrap();
                    }
                    #[cfg(windows)]
                    {
                        fs::write(&path, b"edit").unwrap();
                    }
                }
            }
            let failure = stage.publish().err().unwrap();
            assert!(failure.resolution().is_none());
            drop(failure);
            assert!(matches!(pending(&writer).state, State::Publishing { .. }));
            assert!(!f.destination.exists());
            assert!(fs::symlink_metadata(&path).is_ok());
            assert!(recover_fixture(&mut writer, &parent).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_stage_parent_alias_retarget_and_readonly_partial_never_relax_reader() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let f = Fixture::new();
        let ctx = context();
        let alias = f.source.parent().unwrap().join("alias");
        symlink(f.destination.parent().unwrap(), &alias).unwrap();
        let other = f.source.parent().unwrap().join("other");
        fs::create_dir(&other).unwrap();
        let parent = CapturedDocumentDirectory::capture(
            &alias,
            NativeDocumentFileLimits::new(1024 * 1024, 65536, 256, 65536).unwrap(),
            &ctx,
        )
        .unwrap();
        let old = parent
            .read(f.destination.file_name().unwrap(), &ctx)
            .unwrap();
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let stage = prepared(&mut writer, &f.key(), &parent, &old, b"new");
        fs::remove_file(&alias).unwrap();
        symlink(&other, &alias).unwrap();
        assert!(stage.publish().err().unwrap().resolution().is_none());
        assert!(!f.destination.exists());
        drop(writer);
        // Independent request: partial SDK birth becomes unreadable, no cleanup capability.
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let root = parent.canonical_path().to_owned();
        test_hooks::AFTER_WRITE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                let directory = fs::read_dir(root).unwrap().next().unwrap().unwrap().path();
                fs::set_permissions(
                    directory.join("document"),
                    fs::Permissions::from_mode(0o400),
                )
                .unwrap();
            }))
        });
        let bytes = vec![7; 128 * 1024];
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                &bytes,
            )
            .err()
            .unwrap();
        assert!(failure.resolution().is_none());
        drop(failure);
        let op = pending(&writer);
        assert!(matches!(op.state, State::File { .. }));
        assert_eq!(fs::metadata(op.file_path().unwrap()).unwrap().len(), 65536);
        assert_eq!(
            recover_fixture(&mut writer, &parent).err().unwrap().stage(),
            ArtifactStage::IncompleteStage
        );
        fs::set_permissions(op.file_path().unwrap(), fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[test]
    fn native_stage_failed_resolved_or_ack_record_never_cleans_and_recovers_actual_effect() {
        for acknowledgement in [false, true] {
            let f = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            if acknowledgement {
                writer.fail_at.set(Some(ArtifactStage::Commit));
                let resolved = published(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
                assert_eq!(
                    resolved.acknowledge().err().unwrap().stage(),
                    ArtifactStage::Commit
                );
                assert!(matches!(pending(&writer).state, State::Resolved { .. }));
            } else {
                test_hooks::AFTER_RENAME.with(|h| {
                    *h.borrow_mut() = Some(Box::new(|| {
                        // Inject catalog sync failure only after the actual destination effect.
                        super::super::catalog::test_hooks::SYNC.with(|h| {
                            *h.borrow_mut() = Some(Box::new(|_| Err(io::ErrorKind::Other.into())))
                        });
                    }))
                });
                let failure = prepared(&mut writer, &f.key(), &parent, &old, b"new")
                    .publish()
                    .err()
                    .unwrap();
                assert!(failure.resolution().is_none());
                drop(failure);
                assert!(matches!(pending(&writer).state, State::Publishing { .. }));
            }
            let op = pending(&writer);
            assert!(op.directory_path().unwrap().exists());
            assert_eq!(fs::read(&f.destination).unwrap(), b"new");
            drop(writer);
            let ctx = context();
            let (parent, _) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            let resolved = recovered(&mut writer, &parent);
            assert_eq!(resolved.outcome(), NativeDocumentOutcome::Applied);
            resolved.acknowledge().unwrap().release().unwrap();
            assert!(!op.directory_path().unwrap().exists());
        }
    }
    #[test]
    fn native_stage_collision_preserves_sentinel_and_absent_intent_can_abort() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        test_hooks::CREATE_DIRECTORY.with(|h| {
            *h.borrow_mut() = Some(Box::new(|path| {
                fs::create_dir(path)?;
                fs::write(path.join("sentinel"), b"keep")?;
                graphit_fs::create_private_directory(path)
            }))
        });
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                b"new",
            )
            .err()
            .unwrap();
        assert_eq!(failure.error().kind(), io::ErrorKind::AlreadyExists);
        assert!(failure.resolution().is_none());
        drop(failure);
        let op = pending(&writer);
        assert_eq!(
            fs::read(op.directory_path().unwrap().join("sentinel")).unwrap(),
            b"keep"
        );
        assert_eq!(
            recover_fixture(&mut writer, &parent).err().unwrap().stage(),
            ArtifactStage::IncompleteStage
        );
        // Explicit fixture removes its own collision; no Graphit cleanup was granted.
        fs::remove_file(op.directory_path().unwrap().join("sentinel")).unwrap();
        fs::remove_dir(op.directory_path().unwrap()).unwrap();
        completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
            .release()
            .unwrap();
        assert!(writer.document().unwrap().pending.is_none());
    }
    #[test]
    fn native_stage_same_persisted_id_cannot_adopt_pending_with_any_key_or_destination() {
        let f = Fixture::new();
        fs::write(&f.destination, b"old").unwrap();
        let id = persist_operation_id(f.source.parent().unwrap());
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let other = parent.read(std::ffi::OsStr::new("other"), &ctx).unwrap();
        let other_root = f.source.parent().unwrap().join("another-parent");
        fs::create_dir(&other_root).unwrap();
        let other_parent =
            CapturedDocumentDirectory::capture(&other_root, parent.limits(), &ctx).unwrap();
        let other_old = other_parent
            .read(std::ffi::OsStr::new("link"), &ctx)
            .unwrap();
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let key = f.key();
        drop(
            writer
                .prepare_native_document(id, &key, &parent, &old, b"new")
                .unwrap_or_else(|e| panic!("{:?}", e.error())),
        );
        let op = pending(&writer);
        let catalog = fs::read(f.store.manifest_path()).unwrap();
        let other_key = ArtifactKey::new("kiro", "command", "org/other").unwrap();
        for (key, parent, expected) in [
            (&key, &parent, &old),
            (&other_key, &parent, &old),
            (&key, &parent, &other),
            (&key, &other_parent, &other_old),
        ] {
            let failure = writer
                .prepare_native_document(id, key, parent, expected, b"different")
                .err()
                .unwrap();
            assert_eq!(failure.error().stage(), ArtifactStage::RecoveryRequired);
            assert_eq!(failure.error().kind(), io::ErrorKind::WouldBlock);
            assert!(failure.resolution().is_none());
            drop(failure);
            assert_eq!(fs::read(f.store.manifest_path()).unwrap(), catalog);

            assert_eq!(fs::read(&f.destination).unwrap(), b"old");
            assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"new");
            assert!(!parent.canonical_path().join("other").exists());
        }
        let resolved = recovered(&mut writer, &parent);
        assert_eq!(resolved.id(), id.to_string());
        assert_eq!(resolved.outcome(), NativeDocumentOutcome::NotPublished);
        resolved.acknowledge().unwrap().release().unwrap();
    }
    #[test]
    fn native_stage_known_id_and_equal_bytes_never_adopt_foreign_birth_or_replacement() {
        for before_birth in [true, false] {
            let f = Fixture::new();
            let id = persist_operation_id(f.source.parent().unwrap());
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let directory = parent.canonical_path().join(format!(".graphit-doc-{id}"));
            let file_path = directory.join("document");
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            if before_birth {
                drop(graphit_fs::create_private_directory(&directory).unwrap());
                let mut file = graphit_fs::create_private_file(&file_path).unwrap();
                use std::io::Write;
                file.write_all(b"new").unwrap();
                file.sync_all().unwrap();
                let failure = writer
                    .prepare_native_document(id, &f.key(), &parent, &old, b"new")
                    .err()
                    .unwrap();
                assert_eq!(failure.error().kind(), io::ErrorKind::AlreadyExists);
                assert!(failure.resolution().is_none());
            } else {
                drop(
                    writer
                        .prepare_native_document(id, &f.key(), &parent, &old, b"new")
                        .unwrap_or_else(|e| panic!("{:?}", e.error())),
                );
                // Keep the original inode alive, so same bytes cannot mask a distinct birth.
                fs::rename(&file_path, directory.join("original")).unwrap();
                let mut file = graphit_fs::create_private_file(&file_path).unwrap();
                use std::io::Write;
                file.write_all(b"new").unwrap();
                file.sync_all().unwrap();
            }
            assert_eq!(pending(&writer).id, id.to_string());
            let catalog = fs::read(f.store.manifest_path()).unwrap();
            let identity = physical_id(&file_path, &fs::metadata(&file_path).unwrap()).unwrap();
            drop(writer);
            let ctx = context();
            let (parent, _) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            let failure = recover_fixture(&mut writer, &parent).err().unwrap();
            assert_eq!(
                failure.stage(),
                if before_birth {
                    ArtifactStage::IncompleteStage
                } else {
                    ArtifactStage::RecoveryConflict
                }
            );
            assert_eq!(fs::read(f.store.manifest_path()).unwrap(), catalog);
            assert_eq!(fs::read(&file_path).unwrap(), b"new");
            assert!(
                physical_id(&file_path, &fs::metadata(&file_path).unwrap()).unwrap() == identity
            );
            assert!(!f.destination.exists());
        }
    }
    fn persist_operation_id(root: &Path) -> NativeDocumentOperationId {
        let id = NativeDocumentOperationId::generate();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("logical-operation.json"))
            .unwrap();
        serde_json::to_writer(&mut file, &id).unwrap();
        file.sync_all().unwrap();
        id
    }
    fn pause_process(root: &Path) -> ! {
        fs::write(root.join("ready"), b"paused").unwrap();
        loop {
            thread::park();
        }
    }
    pub(super) fn crash_native_operation(mut writer: ArtifactWriter, root: &Path, phase: &str) {
        let id: NativeDocumentOperationId =
            serde_json::from_slice(&fs::read(root.join("logical-operation.json")).unwrap())
                .unwrap();
        let ctx = context();
        let parent = CapturedDocumentDirectory::capture(
            &root.join("adapter"),
            NativeDocumentFileLimits::new(1024 * 1024, 65536, 256, 65536).unwrap(),
            &ctx,
        )
        .unwrap();
        let old = parent.read(std::ffi::OsStr::new("link"), &ctx).unwrap();
        writer = writer.with_io_context(ctx).unwrap();
        let signal_root = root.to_owned();
        match phase {
            "terminal-aborted" => {
                test_hooks::CREATE_DIRECTORY.with(|h| {
                    *h.borrow_mut() = Some(Box::new(|_| Err(io::ErrorKind::Other.into())));
                });
                test_hooks::AFTER_TERMINAL_RECORD.with(|h| {
                    *h.borrow_mut() = Some(Box::new(move || pause_process(&signal_root)));
                });
            }
            "terminal-cleanup" => test_hooks::AFTER_REMOVE_DIRECTORY.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || pause_process(&signal_root)));
            }),
            "terminal-cleaned" => test_hooks::AFTER_TERMINAL_RECORD.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || pause_process(&signal_root)));
            }),
            "terminal-released" => test_hooks::AFTER_CLEAR_OPERATION.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || pause_process(&signal_root)));
            }),
            "intent" => test_hooks::CREATE_DIRECTORY.with(|h| {
                *h.borrow_mut() = Some(Box::new(move |_| pause_process(&signal_root)));
            }),
            "directory" => test_hooks::AFTER_DIRECTORY_RECORD.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || pause_process(&signal_root)));
            }),
            "file" => test_hooks::AFTER_WRITE.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || pause_process(&signal_root)));
            }),
            "prepared" => {}
            "publishing" => test_hooks::AFTER_RENAME.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || pause_process(&signal_root)));
            }),
            _ => panic!("unexpected crash fixture phase"),
        }
        let key = ArtifactKey::new("codex", "command", "org/repo").unwrap();
        if phase == "terminal-aborted" {
            assert!(
                writer
                    .prepare_native_document(id, &key, &parent, &old, b"after real rename")
                    .is_err()
            );
            drop(completion_from_recovery(
                recover_fixture(&mut writer, &parent).unwrap(),
            ));
            panic!("terminal crash hook did not pause");
        }
        let stage = writer
            .prepare_native_document(id, &key, &parent, &old, b"after real rename")
            .unwrap_or_else(|failure| panic!("{:?}", failure.error()));
        assert_eq!(stage.id(), id.to_string());
        if phase == "prepared" {
            pause_process(root);
        }
        let resolution = published(stage);
        if phase.starts_with("terminal-") {
            let completion = resolution.acknowledge().unwrap();
            // This is a typed caller marker, not the shared Hub application's codec.
            let mut marker = fs::File::create(root.join("logical-released.json")).unwrap();
            serde_json::to_writer(&mut marker, &id).unwrap();
            marker.sync_all().unwrap();
            completion.release().unwrap();
            panic!("terminal crash hook did not pause");
        }
        drop(resolution);
    }
    #[test]
    fn terminal_handoff_real_process_death_preserves_each_durable_boundary_without_replay() {
        for phase in [
            "terminal-aborted",
            "terminal-cleanup",
            "terminal-cleaned",
            "terminal-released",
        ] {
            let f = Fixture::new();
            let root = f.source.parent().unwrap();
            let id = persist_operation_id(root);
            let mut worker = child(root, &format!("native-stage-kill-{phase}"), false);
            let deadline = Instant::now() + Duration::from_secs(10);
            while !root.join("ready").exists() {
                assert!(Instant::now() < deadline, "{phase}");
                assert!(worker.0.try_wait().unwrap().is_none(), "{phase}");
                thread::sleep(Duration::from_millis(5));
            }
            let original = fs::read(&f.destination).ok();
            worker.0.kill().unwrap();
            worker.0.wait().unwrap();
            let ctx = context();
            let (parent, _) = capture(&f, &ctx);
            let mut writer = f
                .fresh_store()
                .writer(Duration::from_secs(5), &Cancellation::default())
                .unwrap()
                .with_io_context(ctx)
                .unwrap();
            if phase == "terminal-released" {
                assert!(writer.document().unwrap().pending.is_none());
                let marker: NativeDocumentOperationId =
                    serde_json::from_slice(&fs::read(root.join("logical-released.json")).unwrap())
                        .unwrap();
                assert!(marker == id);
            } else {
                let saved = pending(&writer);
                assert_eq!(saved.id, id.to_string());
                assert!(match phase {
                    "terminal-aborted" => matches!(saved.state, State::AbortedBeforeBirth {}),
                    "terminal-cleanup" => matches!(saved.state, State::Acknowledged { .. }),
                    "terminal-cleaned" => matches!(saved.state, State::Cleaned { .. }),
                    _ => false,
                });
                let completion =
                    completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap());
                assert_eq!(
                    completion.outcome(),
                    if phase == "terminal-aborted" {
                        NativeDocumentOutcome::NotPublished
                    } else {
                        NativeDocumentOutcome::Applied
                    }
                );
                drop(completion);
                assert!(writer.document().unwrap().pending.is_some());
                completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
                    .release()
                    .unwrap();
                assert!(writer.document().unwrap().pending.is_none());
            }
            assert_eq!(fs::read(&f.destination).ok(), original);
            assert_eq!(
                original,
                if phase == "terminal-aborted" {
                    None
                } else {
                    Some(b"after real rename".to_vec())
                }
            );
            assert!(
                !parent
                    .canonical_path()
                    .join(format!(".graphit-doc-{id}"))
                    .exists()
            );
        }
    }
    #[test]
    fn native_stage_real_process_kill_after_native_effect_recovers_without_replay() {
        let f = Fixture::new();
        let root = f.source.parent().unwrap();
        let id = persist_operation_id(root);
        let mut worker = child(root, "native-stage-kill-publishing", false);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("ready").exists() {
            assert!(Instant::now() < deadline);
            assert!(worker.0.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(fs::read(&f.destination).unwrap(), b"after real rename");
        worker.0.kill().unwrap();
        worker.0.wait().unwrap();
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        assert_eq!(pending(&writer).id, id.to_string());
        assert!(matches!(pending(&writer).state, State::Publishing { .. }));
        let resolved = recovered(&mut writer, &parent);
        assert_eq!(resolved.id(), id.to_string());
        assert_eq!(resolved.outcome(), NativeDocumentOutcome::Applied);
        resolved.acknowledge().unwrap().release().unwrap();
        assert!(writer.document().unwrap().pending.is_none());
        assert_eq!(fs::read(&f.destination).unwrap(), b"after real rename");
    }
    #[test]
    fn native_stage_persisted_id_survives_process_kill_at_every_prepublication_phase() {
        for phase in ["intent", "directory", "file", "prepared"] {
            let f = Fixture::new();
            let root = f.source.parent().unwrap();
            let id = persist_operation_id(root);
            assert!(!f.store.manifest_path().exists());
            let mut worker = child(root, &format!("native-stage-kill-{phase}"), false);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !root.join("ready").exists() {
                assert!(Instant::now() < deadline, "{phase}");
                assert!(worker.0.try_wait().unwrap().is_none(), "{phase}");
                thread::sleep(Duration::from_millis(5));
            }
            assert!(!f.destination.exists());
            worker.0.kill().unwrap();
            worker.0.wait().unwrap();
            let ctx = context();
            let (parent, _) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            let op = pending(&writer);
            assert_eq!(op.id, id.to_string());
            let catalog = fs::read(f.store.manifest_path()).unwrap();
            match phase {
                "intent" => {
                    assert!(matches!(op.state, State::Intent {}));
                    assert!(!op.directory_path().unwrap().exists());
                    completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
                        .release()
                        .unwrap();
                }
                "directory" => {
                    assert!(matches!(op.state, State::Directory { .. }));
                    assert!(!op.file_path().unwrap().exists());
                    assert_eq!(
                        recover_fixture(&mut writer, &parent).err().unwrap().stage(),
                        ArtifactStage::IncompleteStage
                    );
                    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), catalog);
                }
                "file" => {
                    assert!(matches!(op.state, State::File { .. }));
                    assert_eq!(
                        recover_fixture(&mut writer, &parent).err().unwrap().stage(),
                        ArtifactStage::IncompleteStage
                    );
                    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), catalog);
                    assert_eq!(
                        fs::read(op.file_path().unwrap()).unwrap(),
                        b"after real rename"
                    );
                }
                "prepared" => {
                    assert!(matches!(op.state, State::Prepared { .. }));
                    let resolved = recovered(&mut writer, &parent);
                    assert_eq!(resolved.id(), id.to_string());
                    assert_eq!(resolved.outcome(), NativeDocumentOutcome::NotPublished);
                    resolved.acknowledge().unwrap().release().unwrap();
                }
                _ => unreachable!(),
            }
            if matches!(phase, "directory" | "file") {
                assert_eq!(pending(&writer).id, id.to_string());
                assert!(op.directory_path().unwrap().exists());
            } else {
                assert!(writer.document().unwrap().pending.is_none());
                assert!(!op.directory_path().unwrap().exists());
            }
            assert!(!f.destination.exists());
            let restored: NativeDocumentOperationId =
                serde_json::from_slice(&fs::read(root.join("logical-operation.json")).unwrap())
                    .unwrap();
            assert!(restored == id);
        }
    }
    #[test]
    fn native_stage_all_phases_block_generic_dispatch_and_have_closed_wire() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        drop(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
        let mut document = writer.document().unwrap();
        let op = pending(&writer);
        let State::Prepared { directory, stage } = &op.state else {
            panic!("Prepared");
        };
        let states = [
            State::Intent {},
            State::AbortedBeforeBirth {},
            State::Directory {
                directory: directory.clone(),
            },
            State::File {
                directory: directory.clone(),
                file: stage.file_id().clone(),
            },
            State::Prepared {
                directory: directory.clone(),
                stage: stage.clone(),
            },
            State::Publishing {
                directory: directory.clone(),
                stage: stage.clone(),
            },
            State::Resolved {
                directory: directory.clone(),
                stage: stage.clone(),
                outcome: Outcome::NotPublished {},
            },
            State::Acknowledged {
                directory: directory.clone(),
                stage: stage.clone(),
                outcome: Outcome::NotPublished {},
            },
            State::Cleaned {
                directory: directory.clone(),
                stage: stage.clone(),
                outcome: Outcome::NotPublished {},
            },
        ];
        for state in states {
            let mut op = op.clone();
            op.state = state;
            document.pending = Some(Pending::NativeDocument(Box::new(op.clone())));
            let encoded = codec::encode(&document, f.store.limits.max_document_bytes).unwrap();
            let decoded = codec::decode(&encoded, &f.store).unwrap();
            assert!(
                matches!(decoded.pending, Some(Pending::NativeDocument(decoded)) if *decoded == op)
            );
            writer.save(&document, ArtifactStage::Commit).unwrap();
            assert_eq!(
                writer.recover().unwrap_err().stage(),
                ArtifactStage::RecoveryRequired
            );
            assert_eq!(
                writer.forget(&f.key()).unwrap_err().stage(),
                ArtifactStage::RecoveryRequired
            );
            assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"new");
            let value = serde_json::to_value(&op).unwrap();
            for field in ["id", "key", "destination", "parent", "previous", "state"] {
                let mut invalid = value.clone();
                invalid.as_object_mut().unwrap().remove(field);
                assert!(serde_json::from_value::<Operation>(invalid).is_err());
            }
            let mut invalid = value.clone();
            invalid["extra"] = true.into();
            assert!(serde_json::from_value::<Operation>(invalid).is_err());
            let mut invalid = value.clone();
            invalid["state"]["extra"] = true.into();
            assert!(serde_json::from_value::<Operation>(invalid).is_err());
            let mut invalid = value.clone();
            invalid["state"]["phase"] = "Unknown".into();
            assert!(serde_json::from_value::<Operation>(invalid).is_err());
            let raw = serde_json::to_string(&op).unwrap();
            let duplicate = raw.replacen("\"id\":", &format!("\"id\":\"{}\",\"id\":", op.id), 1);
            assert!(serde_json::from_str::<Operation>(&duplicate).is_err());
        }
        // Malformed canonical ULID is validated even though its data wire is closed.
        let mut op = op;
        op.id = op.id.to_lowercase();
        assert!(op.validate(&document, &f.store).is_err());
    }
    #[test]
    fn native_stage_existing_membership_key_and_path_hierarchies_are_preserved() {
        for same_key in [false, true] {
            let f = Fixture::new();
            let mut writer = f.writer();
            copy_fixture(&mut writer, &f, NodeKind::File).unwrap();
            let membership = writer.document().unwrap().entries[0].clone();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = writer.with_io_context(ctx.clone()).unwrap();
            let key = if same_key {
                f.key()
            } else {
                ArtifactKey::new("kiro", "shared", "other").unwrap()
            };
            let failure = writer
                .prepare_native_document(
                    NativeDocumentOperationId::generate(),
                    &key,
                    &parent,
                    &old,
                    b"new",
                )
                .err()
                .unwrap();
            assert_eq!(failure.error().stage(), ArtifactStage::Validate);
            assert!(failure.resolution().is_none());
            drop(failure);
            let document = writer.document().unwrap();
            assert!(document.entries[0] == membership);
            assert!(document.pending.is_none());
            assert_eq!(fs::read(&f.destination).unwrap(), b"source remains");
        }
    }
    #[test]
    fn native_stage_ack_cleanup_preserves_changed_stage_receipt() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let resolution = prepared(&mut writer, &f.key(), &parent, &old, b"new")
            .discard()
            .unwrap_or_else(|failure| panic!("{:?}", failure.error()));
        let path = parent
            .canonical_path()
            .join(format!(".graphit-doc-{}", resolution.id()))
            .join("document");
        fs::write(&path, b"edit").unwrap();
        assert!(resolution.acknowledge().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"edit");
        assert!(matches!(pending(&writer).state, State::Acknowledged { .. }));
        assert!(recover_fixture(&mut writer, &parent).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"edit");
    }
    #[test]
    fn native_stage_recorded_directory_is_revalidated_before_file_birth() {
        for cancel in [false, true] {
            let f = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
            let root = parent.canonical_path().to_owned();
            let cancellation = ctx.cancellation().clone();
            test_hooks::AFTER_DIRECTORY_RECORD.with(|h| {
                *h.borrow_mut() = Some(Box::new(move || {
                    if cancel {
                        cancellation.cancel();
                    } else {
                        let directory =
                            fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();
                        fs::rename(&directory, directory.with_extension("original")).unwrap();
                        fs::create_dir(&directory).unwrap();
                        fs::write(directory.join("sentinel"), b"keep").unwrap();
                    }
                }))
            });
            let failure = writer
                .prepare_native_document(
                    NativeDocumentOperationId::generate(),
                    &f.key(),
                    &parent,
                    &old,
                    b"new",
                )
                .err()
                .unwrap();
            assert!(failure.resolution().is_none());
            drop(failure);
            let op = pending(&writer);
            assert!(matches!(op.state, State::Directory { .. }));
            assert!(!op.file_path().unwrap().exists());

            if !cancel {
                assert_eq!(
                    fs::read(op.directory_path().unwrap().join("sentinel")).unwrap(),
                    b"keep"
                );
                assert!(
                    fs::read_dir(op.directory_path().unwrap().with_extension("original"))
                        .unwrap()
                        .next()
                        .is_none()
                );
            }
            assert!(!f.destination.exists());
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_stage_private_directory_is_revalidated_before_access_is_amplified() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let f = Fixture::new();
        fs::write(&f.destination, b"old").unwrap();
        fs::set_permissions(&f.destination, fs::Permissions::from_mode(0o640)).unwrap();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        let root = parent.canonical_path().to_owned();
        test_hooks::AFTER_WRITE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                let directory = fs::read_dir(root)
                    .unwrap()
                    .map(Result::unwrap)
                    .find(|e| e.file_name().to_string_lossy().starts_with(".graphit-doc-"))
                    .unwrap()
                    .path();
                fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();
            }))
        });
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                b"new",
            )
            .err()
            .unwrap();
        assert_eq!(failure.error().stage(), ArtifactStage::CopyChunk);
        assert!(failure.resolution().is_none());
        drop(failure);
        let op = pending(&writer);
        assert!(matches!(op.state, State::File { .. }));
        assert_eq!(
            fs::metadata(op.file_path().unwrap()).unwrap().mode() & 0o7777,
            0o600
        );
        assert_eq!(fs::read(&f.destination).unwrap(), b"old");
        fs::set_permissions(
            op.directory_path().unwrap(),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    #[test]
    fn native_stage_actual_expired_deadline_after_effect_does_not_skip_recording() {
        let f = Fixture::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        let ctx = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        test_hooks::AFTER_RENAME.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                while Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(5));
                }
            }))
        });
        let resolved = published(prepared(&mut writer, &f.key(), &parent, &old, b"new"));
        assert_eq!(resolved.outcome(), NativeDocumentOutcome::Applied);

        let before_ack = fs::read(f.store.manifest_path()).unwrap();
        assert_eq!(
            resolved.acknowledge().err().unwrap().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(matches!(pending(&writer).state, State::Resolved { .. }));
        assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before_ack);
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    }
    #[test]
    fn native_stage_sdk_birth_and_first_write_observe_durable_original_ids() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let store = f.store.clone();
        test_hooks::CREATE_DIRECTORY.with(|h| *h.borrow_mut() = Some(Box::new(move |path| {
            assert!(matches!(store.read().unwrap().unwrap().pending,Some(Pending::NativeDocument(op)) if matches!(op.state,State::Intent {})));
            graphit_fs::create_private_directory(path)
        })));
        let store = f.store.clone();
        test_hooks::CREATE_FILE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |path| {
                let document = store.read().unwrap().unwrap();
                let Some(Pending::NativeDocument(op)) = document.pending else {
                    panic!("native");
                };
                let State::Directory { directory } = op.state else {
                    panic!("durable Directory");
                };
                assert!(
                    directory
                        == physical_id(
                            path.parent().unwrap(),
                            &fs::symlink_metadata(path.parent().unwrap()).unwrap()
                        )
                        .unwrap()
                );
                graphit_fs::create_private_file(path)
            }))
        });
        let store = f.store.clone();
        test_hooks::AFTER_WRITE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                let Some(Pending::NativeDocument(op)) = store.read().unwrap().unwrap().pending
                else {
                    panic!("native");
                };
                let State::File { directory, file } = &op.state else {
                    panic!("durable File");
                };
                assert!(
                    directory
                        == &physical_id(
                            &op.directory_path().unwrap(),
                            &fs::symlink_metadata(op.directory_path().unwrap()).unwrap()
                        )
                        .unwrap()
                );
                assert!(
                    file == &physical_id(
                        &op.file_path().unwrap(),
                        &fs::symlink_metadata(op.file_path().unwrap()).unwrap()
                    )
                    .unwrap()
                );
            }))
        });
        published(prepared(&mut writer, &f.key(), &parent, &old, b"new"))
            .acknowledge()
            .unwrap()
            .release()
            .unwrap();
    }
    #[test]
    fn native_stage_short_write_rechecks_namespace_before_next_write() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx.clone()).unwrap();
        crate::file_io::native_io_tests::MAX_WRITE.with(|limit| limit.set(Some(8)));
        let root = parent.canonical_path().to_owned();
        test_hooks::AFTER_WRITE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                let stage = fs::read_dir(root)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path()
                    .join("document");
                fs::rename(&stage, stage.with_extension("original")).unwrap();
                fs::write(&stage, b"foreign").unwrap();
            }))
        });
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                b"0123456789abcdef",
            )
            .err()
            .unwrap();
        assert!(failure.resolution().is_none());
        drop(failure);
        let op = pending(&writer);

        assert_eq!(
            fs::read(op.file_path().unwrap().with_extension("original")).unwrap(),
            b"01234567"
        );
        assert_eq!(fs::read(op.file_path().unwrap()).unwrap(), b"foreign");
        assert!(!f.destination.exists());
    }
    #[test]
    fn native_stage_publishing_record_precedes_final_comparisons() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let stage = prepared(&mut writer, &f.key(), &parent, &old, b"new");
        let path = parent
            .canonical_path()
            .join(format!(".graphit-doc-{}", stage.id()))
            .join("document");
        test_hooks::AFTER_PUBLISHING_RECORD
            .with(|h| *h.borrow_mut() = Some(Box::new(move || fs::write(path, b"bad").unwrap())));
        let failure = stage.publish().err().unwrap();
        assert!(failure.resolution().is_none());
        drop(failure);
        assert!(!f.destination.exists());
        assert!(matches!(pending(&writer).state, State::Publishing { .. }));
    }
    #[test]
    fn native_stage_sync_error_records_actual_terminal_without_fabricating_prepared() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        test_hooks::AFTER_SYNC
            .with(|h| *h.borrow_mut() = Some(Box::new(|_| Err(io::ErrorKind::Other.into()))));
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                b"new",
            )
            .err()
            .unwrap();
        assert_eq!(failure.error().stage(), ArtifactStage::CopyChunk);
        assert_eq!(
            failure.resolution().unwrap().outcome(),
            NativeDocumentOutcome::NotPublished
        );
        failure
            .into_parts()
            .1
            .unwrap()
            .acknowledge()
            .unwrap()
            .release()
            .unwrap();
        assert!(!f.destination.exists());
        assert!(writer.document().unwrap().pending.is_none());
    }
    #[cfg(unix)]
    #[test]
    fn native_stage_access_error_after_actual_application_keeps_unreadable_stage_incomplete() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let f = Fixture::new();
        fs::write(&f.destination, b"old").unwrap();
        fs::set_permissions(&f.destination, fs::Permissions::from_mode(0o640)).unwrap();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        crate::native_document_file::test_hooks::AFTER_APPLY.with(|h| {
            *h.borrow_mut() = Some(Box::new(|file| {
                assert_eq!(file.metadata()?.mode() & 0o7777, 0o640);
                file.set_permissions(fs::Permissions::from_mode(0o400))?;
                Err(io::ErrorKind::Other.into())
            }))
        });
        let failure = writer
            .prepare_native_document(
                NativeDocumentOperationId::generate(),
                &f.key(),
                &parent,
                &old,
                b"new",
            )
            .err()
            .unwrap();
        assert_eq!(failure.error().stage(), ArtifactStage::GrantAccess);
        assert_eq!(failure.error().kind(), io::ErrorKind::Other);
        assert_eq!(
            failure.error().secondary_failure(),
            Some(io::ErrorKind::PermissionDenied)
        );
        assert!(failure.resolution().is_none());
        drop(failure);
        let op = pending(&writer);
        assert!(matches!(op.state, State::File { .. }));
        assert_eq!(
            fs::metadata(op.file_path().unwrap()).unwrap().mode() & 0o7777,
            0o400
        );
        assert_eq!(fs::read(&f.destination).unwrap(), b"old");
        assert_eq!(
            recover_fixture(&mut writer, &parent).err().unwrap().stage(),
            ArtifactStage::IncompleteStage
        );
        fs::set_permissions(op.file_path().unwrap(), fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[test]
    fn native_stage_partial_cleanup_error_retries_only_acknowledged_verified_state() {
        for removed_directory in [false, true] {
            let f = Fixture::new();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            let resolved = prepared(&mut writer, &f.key(), &parent, &old, b"new")
                .discard()
                .unwrap_or_else(|failure| panic!("{:?}", failure.error()));
            let directory = parent
                .canonical_path()
                .join(format!(".graphit-doc-{}", resolved.id()));
            if removed_directory {
                test_hooks::AFTER_REMOVE_DIRECTORY.with(|h| {
                    *h.borrow_mut() = Some(Box::new(|| Err(io::ErrorKind::Other.into())))
                });
            } else {
                test_hooks::AFTER_UNLINK.with(|h| {
                    *h.borrow_mut() = Some(Box::new(|| Err(io::ErrorKind::Other.into())))
                });
            }
            assert_eq!(
                resolved.acknowledge().err().unwrap().stage(),
                ArtifactStage::Cleanup
            );
            assert!(matches!(pending(&writer).state, State::Acknowledged { .. }));
            assert!(!directory.join("document").exists());
            assert_eq!(directory.exists(), !removed_directory);
            drop(writer);
            let ctx = context();
            let (parent, _) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            completion_from_recovery(recover_fixture(&mut writer, &parent).unwrap())
                .release()
                .unwrap();
            assert!(writer.document().unwrap().pending.is_none());
            assert!(!directory.exists());
            assert!(!f.destination.exists());
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_stage_control_root_failure_after_effect_grants_no_resolution_or_recreation() {
        let f = Fixture::new();
        let ctx = context();
        let (parent, old) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        let runtime = f.store.runtime_root().to_owned();
        let moved = runtime.with_extension("moved");
        let from = runtime.clone();
        let to = moved.clone();
        test_hooks::AFTER_RENAME
            .with(|h| *h.borrow_mut() = Some(Box::new(move || fs::rename(from, to).unwrap())));
        let failure = prepared(&mut writer, &f.key(), &parent, &old, b"new")
            .publish()
            .err()
            .unwrap();
        assert!(failure.resolution().is_none());
        drop(failure);
        assert!(!runtime.exists());
        assert!(moved.join("artifact-store.json").is_file());
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
        assert!(writer.load().is_err());
        drop(writer);
        // Restore this fixture's original held namespace, not an adoption/repair API.
        fs::rename(&moved, &runtime).unwrap();
        let ctx = context();
        let (parent, _) = capture(&f, &ctx);
        let mut writer = f.writer().with_io_context(ctx).unwrap();
        assert!(matches!(pending(&writer).state, State::Publishing { .. }));
        let resolved = recovered(&mut writer, &parent);
        assert_eq!(resolved.outcome(), NativeDocumentOutcome::Applied);
        resolved.acknowledge().unwrap().release().unwrap();
        assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    }
    #[test]
    fn native_stage_maximum_admitted_catalog_with_old_present_fits_every_terminal_outcome() {
        use crate::native_file_identity::PhysicalId;
        for outcome in 0..3 {
            let f = Fixture::new();
            fs::write(&f.destination, b"old").unwrap();
            let ctx = context();
            let (parent, old) = capture(&f, &ctx);
            let mut writer = f.writer().with_io_context(ctx).unwrap();
            let mut document = writer.document().unwrap();
            let stage = NativeDocumentReceipt::largest(4);
            let projected = Operation {
                id: ulid::Ulid::generate().to_string(),
                key: f.key(),
                destination: NativePath(f.destination.clone()),
                parent: PhysicalId::largest(),
                previous: Some(NativeDocumentReceipt::largest(3)),
                state: State::Acknowledged {
                    directory: PhysicalId::largest(),
                    stage: stage.clone(),
                    outcome: Outcome::Applied { destination: stage },
                },
            };
            document.pending = Some(Pending::NativeDocument(Box::new(projected)));
            let limit = codec::encode(&document, 65536).unwrap().len();
            writer.store.limits.max_document_bytes = limit;
            let stage = prepared(&mut writer, &f.key(), &parent, &old, b"new!");
            let resolved = match outcome {
                0 => published(stage),
                1 => {
                    test_hooks::NATIVE.with(|h| {
                        *h.borrow_mut() = Some(Box::new(|_, _, _| Err(io::ErrorKind::Other.into())))
                    });
                    stage.publish().err().unwrap().into_parts().1.unwrap()
                }
                _ => stage
                    .discard()
                    .unwrap_or_else(|failure| panic!("{:?}", failure.error())),
            };
            assert_eq!(
                resolved.outcome(),
                match outcome {
                    0 => NativeDocumentOutcome::Applied,
                    1 => NativeDocumentOutcome::Unchanged,
                    _ => NativeDocumentOutcome::NotPublished,
                }
            );
            resolved.acknowledge().unwrap().release().unwrap();
            assert!(writer.document().unwrap().pending.is_none());
            assert_eq!(
                fs::read(&f.destination).unwrap(),
                if outcome == 0 {
                    b"new!".as_slice()
                } else {
                    b"old".as_slice()
                }
            );
        }
    }
}

// Scope birth performs metadata work only; payload requests keep their own original context.
fn native_capture_request() -> std::sync::Arc<crate::file_io::SnapshotContext> {
    std::sync::Arc::new(crate::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        Cancellation::default(),
    ))
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod native_writer_acquisition;

mod native_catalog_projection;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod native_current_catalog_body;

#[cfg(target_os = "linux")]
mod user_scope;

mod owned_copy;

mod owned_missing_file;

mod project_worktrees;

mod generated_publication;

#[test]
fn catalog_persists_owner_once_in_scope() {
    let f = Fixture::new();
    let document = f.writer().document().unwrap();
    let value = catalog_json(&document);
    assert!(value.get("owner").is_none());
    assert_eq!(
        value["scope"]["owner"],
        serde_json::to_value(f.store.owner()).unwrap()
    );
}
