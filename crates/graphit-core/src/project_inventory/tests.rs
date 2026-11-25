use super::*;
use crate::{brand::Brand, file_lock::Cancellation, project_identity::ProjectMetadata};
use std::{
    process::Command,
    time::{Duration, Instant},
};

fn request() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
struct Fixture {
    _temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let root = temp.path().join("checkout");
        fs::create_dir(&root).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("global")),
        )
        .unwrap();
        let project = ProjectPaths::at(&root, &installation).unwrap();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .ensure(&ProjectMetadata::new("example").unwrap())
            .unwrap();
        Self {
            _temp: temp,
            installation,
            project,
        }
    }
    fn store(&self) -> GlobalProjectInventory {
        GlobalProjectInventory::at(&self.installation, 65536).unwrap()
    }
    fn writer(&self, req: &Arc<SnapshotContext>) -> ProjectInventoryWriter {
        fs::create_dir_all(self.installation.root()).unwrap();
        self.store().writer(req).unwrap()
    }

    fn capture(&self, req: &Arc<SnapshotContext>, selected: &str) -> ProjectInventoryObservation {
        capture(&self.project, req, selected)
    }
}
fn capture(
    project: &ProjectPaths,
    req: &Arc<SnapshotContext>,
    explicit: &str,
) -> ProjectInventoryObservation {
    let git =
        GitCheckout::inspect(project.root(), Duration::from_secs(5), req.cancellation()).unwrap();
    let version = VersionContext::select(project, git.as_checkout(), explicit, None, "").unwrap();
    let expected = ProjectIdentityStore::at(project)
        .load()
        .unwrap()
        .unwrap()
        .id();
    ProjectInventoryObservation::observe(project, expected, &git, &version, req).unwrap()
}
fn git(root: &Path, args: &[&str]) {
    let result = crate::git_checkout::git_command(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ],
    )
    .args(args)
    .output()
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn write_document(store: &GlobalProjectInventory, bytes: &[u8]) {
    fs::write(store.path(), bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o600)).unwrap();
    }
}
#[test]
fn absent_read_creates_no_root_lock_or_identity() {
    let f = Fixture::new();
    let req = request();
    let store = f.store();
    assert_eq!(store.load(&req).unwrap().projects().len(), 0);
    assert!(!f.installation.root().exists());
    assert!(GlobalProjectInventory::at(&f.installation, 0).is_err());
}
#[test]
fn register_reload_noop_and_versions_lifecycle() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let id = cap.identity.id();
    {
        let mut writer = f.writer(&req);
        assert!(writer.register_checkout(&cap).unwrap());
        let bytes = fs::read(f.store().path()).unwrap();
        assert!(!writer.register_checkout(&cap).unwrap());
        assert_eq!(fs::read(f.store().path()).unwrap(), bytes);
        assert!(writer.retain_version(&cap, "release/β").unwrap());
        assert!(!writer.retain_version(&cap, "release/β").unwrap());
        assert_eq!(
            writer.remove_version(id, "main").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(writer.remove_checkout(id, f.project.root()).unwrap());
        assert!(!writer.remove_checkout(id, f.project.root()).unwrap());
    }
    let reopened = GlobalProjectInventory::at(&f.installation, 65536).unwrap();
    let snapshot = reopened.load(&req).unwrap();
    assert_eq!(
        snapshot.get(id).unwrap().versions().collect::<Vec<_>>(),
        ["main", "release/β"]
    );
    assert_eq!(snapshot.get(id).unwrap().checkouts().len(), 0);
    let mut writer = f.writer(&req);
    assert!(writer.remove_version(id, "main").unwrap());
    assert!(writer.remove_version(id, "release/β").unwrap());
    assert!(f.store().load(&req).unwrap().projects().next().is_none());
    assert!(f.project.identity_file().exists());
}
#[test]
fn real_git_worktrees_and_switch_preserve_versions_and_physical_selection() {
    let f = Fixture::new();
    git(f.project.root(), &["init", "-b", "main"]);
    git(f.project.root(), &["add", ".graphit/project.json"]);
    git(f.project.root(), &["commit", "-m", "initial"]);
    let second = f._temp.path().join("second");
    git(
        f.project.root(),
        &["worktree", "add", "-b", "feature", "../second"],
    );
    let other = ProjectPaths::at(&second, &f.installation).unwrap();
    let req = request();
    let main = f.capture(&req, "");
    let feature = capture(&other, &req, "pinned");
    let id = main.identity.id();
    assert_eq!(id, feature.identity.id());
    let mut writer = f.writer(&req);
    writer.register_checkout(&main).unwrap();
    writer.register_checkout(&feature).unwrap();
    git(f.project.root(), &["switch", "-c", "next"]);
    let next = f.capture(&req, "");
    writer.register_checkout(&next).unwrap();
    let snapshot = f.store().load(&req).unwrap();
    let entry = snapshot.get(id).unwrap();
    assert_eq!(entry.checkouts().len(), 2);
    assert_eq!(
        entry.versions().collect::<Vec<_>>(),
        ["main", "next", "pinned"]
    );
    let observed = entry
        .checkouts()
        .find(|c| c.path() == other.root())
        .unwrap();
    assert_eq!(observed.selected_version(), "pinned");
    assert_eq!(observed.source(), VersionSource::Explicit);
    assert_eq!(
        observed.checkout(),
        &CheckoutState::Branch {
            name: "feature".into()
        }
    );
    git(other.root(), &["checkout", "--detach"]);
    let detached = capture(&other, &req, "");
    writer.register_checkout(&detached).unwrap();
    let detached_snapshot = f.store().load(&req).unwrap();
    let detached_checkout = detached_snapshot
        .get(id)
        .unwrap()
        .checkouts()
        .find(|checkout| checkout.path() == other.root())
        .unwrap();
    assert_eq!(detached_checkout.source(), VersionSource::DetachedCommit);
    assert!(detached_checkout.selected_version().starts_with("git:"));
    assert_ne!(
        detached_checkout.selected_version(),
        detached_checkout.default_version()
    );
    assert_eq!(
        f.store()
            .load(&req)
            .unwrap()
            .get(id)
            .unwrap()
            .checkouts()
            .find(|c| c.path() == other.root())
            .unwrap()
            .checkout(),
        &CheckoutState::Detached
    );
}
#[test]
fn rename_keeps_id_and_ephemeral_is_not_registered() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let id = cap.identity.id();
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let identity = ProjectIdentityStore::at(&f.project);
    let mut changed = ProjectMetadata::new("renamed").unwrap();
    identity
        .writer(Duration::from_secs(5), req.cancellation())
        .unwrap()
        .update(id, &changed)
        .unwrap();
    assert!(!writer.register_checkout(&cap).unwrap());
    writer.register_checkout(&f.capture(&req, "")).unwrap();
    assert_eq!(
        f.store()
            .load(&req)
            .unwrap()
            .get(id)
            .unwrap()
            .identity()
            .metadata()
            .name,
        "renamed"
    );
    changed.ephemeral = true;
    identity
        .writer(Duration::from_secs(5), req.cancellation())
        .unwrap()
        .update(id, &changed)
        .unwrap();
    let git = GitCheckout::NonGit;
    let version = VersionContext::select(&f.project, git.as_checkout(), "", None, "").unwrap();
    assert_eq!(
        ProjectInventoryObservation::observe(&f.project, id, &git, &version, &req)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}
#[test]
fn maintenance_and_retention_cannot_overwrite_a_confirmed_label_edit_with_stale_metadata() {
    let f = Fixture::new();
    let req = request();
    let stale = f.capture(&req, "");
    let id = stale.identity.id();
    let mut inventory = f.writer(&req);
    assert!(inventory.register_checkout(&stale).unwrap());

    let store = ProjectIdentityStore::at(&f.project);
    let mut writer = store
        .writer(Duration::from_secs(5), req.cancellation())
        .unwrap();
    let mut metadata = stale.identity.metadata().clone();
    metadata
        .cluster
        .insert("service".into(), vec!["api".into()]);
    let updated = writer.update(id, &metadata).unwrap();
    assert!(
        inventory
            .refresh_project_metadata(&f.project, &updated)
            .unwrap()
    );
    assert!(!inventory.register_checkout(&stale).unwrap());
    assert!(inventory.retain_version(&stale, "release").unwrap());
    assert!(
        !inventory
            .refresh_project_metadata(&f.project, &updated)
            .unwrap()
    );

    let snapshot = f.store().load(&req).unwrap();
    let project = snapshot.get(id).unwrap();
    assert_eq!(project.identity(), &updated);
    assert_eq!(project.versions().collect::<Vec<_>>(), ["main", "release"]);
    assert_eq!(
        project.checkouts().next().unwrap().selected_version(),
        "main"
    );
}
#[test]
fn cancellation_and_deadline_preserve_inventory() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let bytes = fs::read(f.store().path()).unwrap();
    req.cancellation().cancel();
    assert_eq!(
        writer.retain_version(&cap, "new").unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read(f.store().path()).unwrap(), bytes);
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        f.store().load(&expired).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    drop(writer);
}
#[test]
fn decoder_rejects_corruption_duplicates_unknown_and_oversize() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let bytes = fs::read(f.store().path()).unwrap();
    drop(writer);
    let mut cases = vec![
        b"{".to_vec(),
        br#"{"schema":1,"schema":1,"projects":[]}"#.to_vec(),
        br#"{"schema":1,"projects":[],"extra":0}"#.to_vec(),
        br#"{"schema":2,"projects":[]}"#.to_vec(),
    ];
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let project = value["projects"][0].clone();
    value["projects"].as_array_mut().unwrap().push(project);
    cases.push(serde_json::to_vec(&value).unwrap());
    for field in ["versions", "checkouts"] {
        let mut duplicate: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let extra = duplicate["projects"][0][field][0].clone();
        duplicate["projects"][0][field]
            .as_array_mut()
            .unwrap()
            .push(extra);
        cases.push(serde_json::to_vec(&duplicate).unwrap());
    }
    for bad in cases {
        write_document(&f.store(), &bad);
        assert_eq!(
            f.store().load(&req).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    write_document(&f.store(), &bytes);
    let small = GlobalProjectInventory::at(&f.installation, bytes.len() - 1).unwrap();
    assert_eq!(
        small.load(&req).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn payload_limit_failure_preserves_original_document() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let bytes = fs::read(f.store().path()).unwrap();
    drop(writer);
    let store = GlobalProjectInventory::at(&f.installation, bytes.len() + 8).unwrap();
    let mut small = store.writer(&req).unwrap();
    assert_eq!(
        small
            .retain_version(&cap, &"x".repeat(1000))
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert_eq!(fs::read(store.path()).unwrap(), bytes);
    drop(small);
}
#[test]
fn independent_writers_converge_and_lock_wait_is_cancellable() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let id = cap.identity.id();
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let installation = f.installation.clone();
    let project = f.project.clone();
    let worker = std::thread::spawn(move || {
        let req = request();
        let cap = capture(&project, &req, "");

        let store = GlobalProjectInventory::at(&installation, 65536).unwrap();
        store
            .writer(&req)
            .unwrap()
            .retain_version(&cap, "worker")
            .unwrap()
    });
    writer.retain_version(&cap, "first").unwrap();
    drop(writer);
    assert!(worker.join().unwrap());
    assert_eq!(
        f.store()
            .load(&req)
            .unwrap()
            .get(id)
            .unwrap()
            .versions()
            .collect::<Vec<_>>(),
        ["first", "main", "worker"]
    );
    let _held = f.writer(&req);
    let exp = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    ));

    assert_eq!(
        f.store().writer(&exp).err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
}
#[cfg(unix)]
#[test]
fn inventory_reads_configured_link_and_public_json() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let req = request();
    fs::create_dir_all(f.installation.root()).unwrap();
    let target = f._temp.path().join("inventory.json");
    let snapshot = ProjectInventorySnapshot::empty(f.installation.brand().key());
    fs::write(&target, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    symlink(&target, f.store().path()).unwrap();
    assert_eq!(f.store().load(&req).unwrap(), snapshot);
    assert!(
        fs::symlink_metadata(f.store().path())
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn non_unicode_checkout_is_rejected_without_identity_or_inventory_publication() {
    use std::os::unix::ffi::OsStringExt;
    let f = Fixture::new();
    let name = std::ffi::OsString::from_vec(b"checkout-\xff".to_vec());
    let other = f._temp.path().join(name);
    #[cfg(target_os = "linux")]
    fs::create_dir(&other).unwrap();
    let error = ProjectPaths::at(&other, &f.installation).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("Unicode"));
    assert!(!other.join(".graphit").exists());
    assert!(!f.store().path().exists());
}

#[test]
fn independent_process_writer_preserves_parent_update() {
    use std::io::BufRead;
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let id = cap.identity.id();
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "project_inventory::tests::child_process_writer",
            "--ignored",
            "--nocapture",
        ])
        .env("GRAPHIT_N5_FIXTURE", f._temp.path())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert_ne!(
            output.read_line(&mut line).unwrap(),
            0,
            "child did not reach lock"
        );
        if line.trim() == "N5_READY" {
            break;
        }
    }
    writer.retain_version(&cap, "parent").unwrap();
    drop(writer);
    assert!(child.wait().unwrap().success());
    assert_eq!(
        f.store()
            .load(&req)
            .unwrap()
            .get(id)
            .unwrap()
            .versions()
            .collect::<Vec<_>>(),
        ["child", "main", "parent"]
    );
}
#[test]
#[ignore = "subprocess fixture invoked by independent_process_writer_preserves_parent_update"]
fn child_process_writer() {
    let root = PathBuf::from(std::env::var_os("GRAPHIT_N5_FIXTURE").expect("fixture root"));
    let installation =
        InstallationPaths::resolve(Brand::default(), &root, None, Some(&root.join("global")))
            .unwrap();
    let project = ProjectPaths::at(root.join("checkout"), &installation).unwrap();
    let req = request();
    let capture = capture(&project, &req, "");

    println!("N5_READY");
    std::io::stdout().flush().unwrap();
    GlobalProjectInventory::at(&installation, 65536)
        .unwrap()
        .writer(&req)
        .unwrap()
        .retain_version(&capture, "child")
        .unwrap();
}
#[test]
fn selection_brand_and_identity_conflicts_do_not_rekey() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let original = cap.identity.id();
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(f.project.identity_file()).unwrap()).unwrap();
    value["id"] = "01ARZ3NDEKTSV4RRFFQ69G5FAW".into();
    fs::write(
        f.project.identity_file(),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let replacement = f.capture(&req, "");
    assert_eq!(
        writer.register_checkout(&replacement).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(f.store().load(&req).unwrap().get(original).is_some());
    let other = f._temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let paths = ProjectPaths::at(&other, &f.installation).unwrap();
    let selection = VersionContext::select(&f.project, Checkout::NonGit, "", None, "").unwrap();
    assert_eq!(
        ProjectInventoryObservation::observe(
            &paths,
            original,
            &GitCheckout::NonGit,
            &selection,
            &req
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
    let acme_installation = InstallationPaths::resolve(
        Brand::new("acme").unwrap(),
        f._temp.path(),
        None,
        Some(f.installation.root()),
    )
    .unwrap();
    let acme_project = ProjectPaths::at(f.project.root(), &acme_installation).unwrap();
    ProjectIdentityStore::at(&acme_project)
        .writer(Duration::from_secs(5), req.cancellation())
        .unwrap()
        .ensure(&ProjectMetadata::new("acme").unwrap())
        .unwrap();
    assert_eq!(
        ProjectInventoryObservation::observe(
            &acme_project,
            original,
            &GitCheckout::NonGit,
            &selection,
            &req
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
    let acme_capture = capture(&acme_project, &req, "");
    assert_eq!(
        writer.register_checkout(&acme_capture).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        GlobalProjectInventory::at(&acme_installation, 65536)
            .unwrap()
            .load(&req)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn distinct_projects_keep_independent_versions_and_removal() {
    let f = Fixture::new();
    let root = f._temp.path().join("second-project");
    fs::create_dir(&root).unwrap();
    let second = ProjectPaths::at(&root, &f.installation).unwrap();
    ProjectIdentityStore::at(&second)
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("second").unwrap())
        .unwrap();
    let req = request();
    let first = f.capture(&req, "");
    let second_capture = capture(&second, &req, "topic");
    let first_id = first.identity.id();
    let second_id = second_capture.identity.id();
    assert_ne!(first_id, second_id);
    let mut writer = f.writer(&req);
    writer.register_checkout(&first).unwrap();
    writer.register_checkout(&second_capture).unwrap();
    writer.retain_version(&first, "first-data").unwrap();
    let snapshot = f.store().load(&req).unwrap();
    assert_eq!(snapshot.projects().len(), 2);
    assert_eq!(
        snapshot
            .get(first_id)
            .unwrap()
            .versions()
            .collect::<Vec<_>>(),
        ["first-data", "main"]
    );
    assert_eq!(
        snapshot
            .get(second_id)
            .unwrap()
            .versions()
            .collect::<Vec<_>>(),
        ["topic"]
    );
    writer.remove_checkout(first_id, f.project.root()).unwrap();
    writer.remove_version(first_id, "main").unwrap();
    writer.remove_version(first_id, "first-data").unwrap();
    let snapshot = f.store().load(&req).unwrap();
    assert_eq!(snapshot.projects().len(), 1);
    assert!(snapshot.get(second_id).is_some());
}
#[test]
fn expired_writer_cannot_publish_and_fresh_writer_can_resume() {
    let f = Fixture::new();
    let req = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    ));
    let cap = f.capture(&req, "");
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let bytes = fs::read(f.store().path()).unwrap();
    std::thread::sleep(
        req.deadline()
            .unwrap()
            .saturating_duration_since(Instant::now())
            + Duration::from_millis(1),
    );
    assert_eq!(
        writer
            .retain_version(&cap, "after-deadline")
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(fs::read(f.store().path()).unwrap(), bytes);
    drop(writer);
    let fresh = request();
    let mut new_writer = f.writer(&fresh);
    assert!(!new_writer.register_checkout(&cap).unwrap());
}

#[test]
fn observation_rejects_wrong_project_id_and_stale_id_before_mutation() {
    let f = Fixture::new();
    let other = Fixture::new();
    let req = request();
    let observed = f.capture(&req, "");
    let expected = observed.identity.id();
    let mut writer = f.writer(&req);
    writer.register_checkout(&observed).unwrap();
    let bytes = fs::read(f.store().path()).unwrap();
    fs::copy(other.project.identity_file(), f.project.identity_file()).unwrap();
    let checkout = GitCheckout::NonGit;
    let selected =
        VersionContext::select(&f.project, checkout.as_checkout(), "", None, "").unwrap();
    assert_eq!(
        ProjectInventoryObservation::observe(&f.project, expected, &checkout, &selected, &req)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData,
    );
    assert_eq!(
        writer.retain_version(&observed, "new").unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(f.store().path()).unwrap(), bytes);
}

#[test]
fn observation_preserves_missing_ephemeral_corrupt_and_cancel_errors() {
    let f = Fixture::new();
    let req = request();
    let identity = ProjectIdentityStore::at(&f.project)
        .load()
        .unwrap()
        .unwrap();
    let expected = identity.id();
    let original = fs::read(f.project.identity_file()).unwrap();
    let checkout = GitCheckout::NonGit;
    let selected =
        VersionContext::select(&f.project, checkout.as_checkout(), "", None, "").unwrap();
    fs::remove_file(f.project.identity_file()).unwrap();
    assert_eq!(
        ProjectInventoryObservation::observe(&f.project, expected, &checkout, &selected, &req)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound,
    );
    fs::write(f.project.identity_file(), original).unwrap();
    let mut metadata = identity.metadata().clone();
    metadata.ephemeral = true;
    ProjectIdentityStore::at(&f.project)
        .writer(Duration::from_secs(5), req.cancellation())
        .unwrap()
        .update(expected, &metadata)
        .unwrap();
    assert_eq!(
        ProjectInventoryObservation::observe(&f.project, expected, &checkout, &selected, &req)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput,
    );
    fs::write(f.project.identity_file(), b"{").unwrap();
    assert_eq!(
        ProjectInventoryObservation::observe(&f.project, expected, &checkout, &selected, &req)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData,
    );
    req.cancellation().cancel();
    assert_eq!(
        ProjectInventoryObservation::observe(&f.project, expected, &checkout, &selected, &req)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted,
    );
    assert!(!f.store().path().exists());
}

#[test]
fn unicode_checkout_string_roundtrip_preserves_inventory_identity() {
    let f = Fixture::new();
    let root = f._temp.path().join("checkout ação 日本語 with spaces");
    fs::rename(f.project.root(), &root).unwrap();
    let project = ProjectPaths::at(&root, &f.installation).unwrap();
    let canonical_root = fs::canonicalize(&root).unwrap();
    assert_eq!(project.root(), canonical_root);
    assert_eq!(canonical_root.file_name(), root.file_name());
    let req = request();
    let captured = capture(&project, &req, "release/β");
    let id = captured.identity.id();
    {
        let mut writer = f.writer(&req);
        assert!(writer.register_checkout(&captured).unwrap());
        assert!(writer.retain_version(&captured, "main").unwrap());
    }
    let bytes = fs::read(f.store().path()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["projects"][0]["checkouts"][0]["path"],
        canonical_root.to_str().unwrap()
    );
    let snapshot = f.store().load(&req).unwrap();
    let entry = snapshot.get(id).unwrap();
    assert_eq!(entry.versions().collect::<Vec<_>>(), ["main", "release/β"]);
    assert_eq!(entry.checkouts().next().unwrap().path(), canonical_root);
    assert_eq!(entry.identity.id(), id);
    let mut writer = f.writer(&req);
    assert!(!writer.register_checkout(&captured).unwrap());
    assert_eq!(fs::read(f.store().path()).unwrap(), bytes);
}

#[test]
fn detached_inventory_requires_a_valid_isolated_commit_version() {
    let f = Fixture::new();
    let req = request();
    let capture = f.capture(&req, "");
    f.writer(&req).register_checkout(&capture).unwrap();
    let baseline: serde_json::Value =
        serde_json::from_slice(&fs::read(f.store().path()).unwrap()).unwrap();
    for (selected, valid) in [
        (format!("git:{}", "a".repeat(40)), true),
        (format!("git:{}", "b".repeat(64)), true),
        (format!("git/commit/{}", "a".repeat(40)), false),
        (format!("git:{}", "g".repeat(40)), false),
        ("git:short".to_owned(), false),
        ("main".to_owned(), false),
    ] {
        let mut value = baseline.clone();
        value["projects"][0]["versions"] = serde_json::json!([selected]);
        let checkout = &mut value["projects"][0]["checkouts"][0];
        checkout["source"] = serde_json::json!("DetachedCommit");
        checkout["selected_version"] = serde_json::json!(selected);
        checkout["checkout"] = serde_json::json!({"kind":"Detached"});
        let snapshot: ProjectInventorySnapshot = serde_json::from_value(value).unwrap();
        assert_eq!(snapshot.validate().is_ok(), valid, "{selected}");
    }
}
