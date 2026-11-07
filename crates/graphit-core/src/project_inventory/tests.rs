use super::*;
use crate::{
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    project_identity::ProjectMetadata,
};
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
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
fn scope_limits() -> ScopeLimits {
    ScopeLimits::new(4, 4096, 256).unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
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
        let root = installation_bootstrap::prepare(
            &self.installation,
            BootstrapLimits::new(4096, 128).unwrap(),
            req,
        )
        .into_installation()
        .unwrap();
        self.store().writer(root, req).unwrap()
    }
    fn capture(&self, req: &Arc<SnapshotContext>, selected: &str) -> CapturedProjectInventory {
        capture(&self.project, req, selected)
    }
}
fn capture(
    project: &ProjectPaths,
    req: &Arc<SnapshotContext>,
    explicit: &str,
) -> CapturedProjectInventory {
    let git =
        GitCheckout::inspect(project.root(), Duration::from_secs(5), req.cancellation()).unwrap();
    let version = VersionContext::select(project, git.as_checkout(), explicit, None, "").unwrap();
    CapturedProjectInventory::capture(project, &git, &version, scope_limits(), native(), req)
        .unwrap()
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
        &["worktree", "add", "-b", "feature", second.to_str().unwrap()],
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
    assert_eq!(
        writer.register_checkout(&cap).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
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
        CapturedProjectInventory::capture(
            &f.project,
            &git,
            &version,
            scope_limits(),
            native(),
            &req
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
}
#[test]
fn original_request_cancel_deadline_and_root_change_deny_publication() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let foreign = Arc::new(SnapshotContext::new(
        req.deadline().unwrap(),
        req.cancellation().clone(),
    ));
    let mut foreign_writer = f.writer(&foreign);
    assert_eq!(
        foreign_writer.register_checkout(&cap).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    drop(foreign_writer);
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
    let fresh = request();
    let cap = f.capture(&fresh, "");
    let mut writer = f.writer(&fresh);
    let moved = f._temp.path().join("moved");
    let renamed = fs::rename(f.project.root(), &moved);
    #[cfg(unix)]
    {
        renamed.unwrap();
        fs::create_dir(f.project.root()).unwrap();
        assert!(writer.register_checkout(&cap).is_err());
    }
    #[cfg(windows)]
    {
        // The admitted native checkout handle prevents the replacement itself.
        assert_eq!(renamed.unwrap_err().raw_os_error(), Some(32));
        assert!(!moved.exists());
        assert!(f.project.root().is_dir());
        assert!(!writer.register_checkout(&cap).unwrap());
    }
    assert_eq!(fs::read(f.store().path()).unwrap(), bytes);
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
fn cap_and_readonly_failures_preserve_original_document() {
    let f = Fixture::new();
    let req = request();
    let cap = f.capture(&req, "");
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let bytes = fs::read(f.store().path()).unwrap();
    drop(writer);
    let store = GlobalProjectInventory::at(&f.installation, bytes.len() + 8).unwrap();
    let private = installation_bootstrap::prepare(
        &f.installation,
        BootstrapLimits::new(4096, 128).unwrap(),
        &req,
    )
    .into_installation()
    .unwrap();
    let mut small = store.writer(private, &req).unwrap();
    assert_eq!(
        small
            .retain_version(&cap, &"x".repeat(1000))
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert_eq!(fs::read(store.path()).unwrap(), bytes);
    drop(small);
    let mut permissions = fs::metadata(store.path()).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(store.path(), permissions).unwrap();
    let mut writer = f.writer(&req);
    assert_eq!(
        writer.retain_version(&cap, "new").unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(fs::read(store.path()).unwrap(), bytes);
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
        let private = installation_bootstrap::prepare(
            &installation,
            BootstrapLimits::new(4096, 128).unwrap(),
            &req,
        )
        .into_installation()
        .unwrap();
        let store = GlobalProjectInventory::at(&installation, 65536).unwrap();
        store
            .writer(private, &req)
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
    let private = installation_bootstrap::prepare(
        &f.installation,
        BootstrapLimits::new(4096, 128).unwrap(),
        &exp,
    )
    .into_installation()
    .unwrap();
    assert_eq!(
        f.store().writer(private, &exp).err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
}
#[cfg(unix)]
#[test]
fn links_and_public_documents_do_not_become_empty_snapshots() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let req = request();
    drop(f.writer(&req));
    symlink("missing", f.store().path()).unwrap();
    assert!(f.store().load(&req).is_err());
    fs::remove_file(f.store().path()).unwrap();
    fs::create_dir(f.store().path()).unwrap();
    assert!(f.store().load(&req).is_err());
    fs::remove_dir(f.store().path()).unwrap();
    fs::write(f.store().path(), br#"{"schema":1,"projects":[]}"#).unwrap();
    fs::set_permissions(f.store().path(), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        f.store().load(&req).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
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
    let private = installation_bootstrap::prepare(
        &installation,
        BootstrapLimits::new(4096, 128).unwrap(),
        &req,
    )
    .into_installation()
    .unwrap();
    println!("N5_READY");
    std::io::stdout().flush().unwrap();
    GlobalProjectInventory::at(&installation, 65536)
        .unwrap()
        .writer(private, &req)
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
        CapturedProjectInventory::capture(
            &paths,
            &GitCheckout::NonGit,
            &selection,
            scope_limits(),
            native(),
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
        CapturedProjectInventory::capture(
            &acme_project,
            &GitCheckout::NonGit,
            &selection,
            scope_limits(),
            native(),
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
fn expired_original_writer_does_not_publish_with_a_fresh_request() {
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
    assert_eq!(
        new_writer.register_checkout(&cap).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
}

fn scoped_capture(
    project: &ProjectPaths,
    req: &Arc<SnapshotContext>,
    scope: &CapturedPublicationScope,
    explicit: &str,
) -> io::Result<CapturedProjectInventory> {
    let checkout =
        GitCheckout::inspect(project.root(), Duration::from_secs(5), req.cancellation())?;
    let selection = VersionContext::select(project, checkout.as_checkout(), explicit, None, "")?;
    CapturedProjectInventory::capture_in_scope(project, &checkout, &selection, scope, native(), req)
}
fn project_scope(f: &Fixture, req: &Arc<SnapshotContext>) -> CapturedPublicationScope {
    let identity = ProjectIdentityStore::at(&f.project)
        .load()
        .unwrap()
        .unwrap();
    CapturedPublicationScope::project(&f.project, identity.id(), scope_limits(), req).unwrap()
}
#[test]
fn scoped_capture_retains_original_owner_and_logical_selection() {
    let f = Fixture::new();
    let req = request();
    let scope = project_scope(&f, &req);
    let cap = scoped_capture(&f.project, &req, &scope, "pinned").unwrap();
    let id = cap.identity.id();
    assert_eq!(scope.owner(), &StoreOwner::Project { project_id: id });
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    writer.retain_version(&cap, "main").unwrap();
    let snapshot = f.store().load(&req).unwrap();
    let entry = snapshot.get(id).unwrap();
    assert_eq!(entry.versions().collect::<Vec<_>>(), ["main", "pinned"]);
    assert_eq!(
        entry.checkouts().next().unwrap().source(),
        VersionSource::Explicit
    );
    assert_eq!(
        entry.checkouts().next().unwrap().checkout(),
        &CheckoutState::NonGit
    );
}
#[test]
fn scoped_capture_compares_loaded_identity_to_original_owner() {
    let f = Fixture::new();
    let other = Fixture::new();
    let req = request();
    let scope = project_scope(&f, &req);
    let original = fs::read(f.project.identity_file()).unwrap();
    fs::copy(other.project.identity_file(), f.project.identity_file()).unwrap();
    assert_eq!(
        scoped_capture(&f.project, &req, &scope, "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::write(f.project.identity_file(), original).unwrap();
    let cap = scoped_capture(&f.project, &req, &scope, "").unwrap();
    assert_eq!(
        scope.owner(),
        &StoreOwner::Project {
            project_id: cap.identity.id()
        }
    );
    assert!(!f.store().path().exists());
}
#[test]
fn scoped_capture_rejects_other_root_brand_and_selection() {
    let f = Fixture::new();
    let other = Fixture::new();
    let req = request();
    let scope = project_scope(&f, &req);
    let other_scope = project_scope(&other, &req);
    assert_eq!(
        scoped_capture(&f.project, &req, &other_scope, "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let installation = InstallationPaths::resolve(
        Brand::new("other").unwrap(),
        f._temp.path(),
        None,
        Some(&f._temp.path().join("other-global")),
    )
    .unwrap();
    let project = ProjectPaths::at(f.project.root(), &installation).unwrap();
    let brand_scope = CapturedPublicationScope::project(
        &project,
        ProjectIdentityStore::at(&f.project)
            .load()
            .unwrap()
            .unwrap()
            .id(),
        scope_limits(),
        &req,
    )
    .unwrap();
    assert_eq!(
        scoped_capture(&f.project, &req, &brand_scope, "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let checkout =
        GitCheckout::inspect(f.project.root(), Duration::from_secs(5), req.cancellation()).unwrap();
    let selection =
        VersionContext::select(&other.project, checkout.as_checkout(), "", None, "").unwrap();
    assert_eq!(
        CapturedProjectInventory::capture_in_scope(
            &f.project,
            &checkout,
            &selection,
            &scope,
            native(),
            &req,
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
}
#[test]
fn scoped_capture_rejects_installation_owner() {
    let f = Fixture::new();
    let req = request();
    let _root = installation_bootstrap::prepare(
        &f.installation,
        BootstrapLimits::new(4096, 128).unwrap(),
        &req,
    )
    .into_installation()
    .unwrap();
    let identity = crate::installation_identity::InstallationIdentityStore::new(&f.installation)
        .unwrap()
        .ensure(Duration::from_secs(5), req.cancellation())
        .unwrap();
    let scope = CapturedPublicationScope::installation(
        &f.installation,
        identity.unit_id().clone(),
        [f.project.root().to_owned()],
        scope_limits(),
        &req,
    )
    .unwrap();
    assert_eq!(
        scoped_capture(&f.project, &req, &scope, "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}
#[test]
fn scoped_capture_rejects_foreign_request_and_original_cancellation() {
    let f = Fixture::new();
    let req = request();
    let scope = project_scope(&f, &req);
    let foreign = Arc::new(SnapshotContext::new(
        req.deadline().unwrap(),
        req.cancellation().clone(),
    ));
    let capture = scoped_capture(&f.project, &req, &scope, "").unwrap();
    let mut writer = f.writer(&foreign);
    assert_eq!(
        writer.register_checkout(&capture).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!f.store().path().exists());
    drop(writer);
    let checkout =
        GitCheckout::inspect(f.project.root(), Duration::from_secs(5), req.cancellation()).unwrap();
    let selection =
        VersionContext::select(&f.project, checkout.as_checkout(), "", None, "").unwrap();
    req.cancellation().cancel();
    assert_eq!(
        CapturedProjectInventory::capture_in_scope(
            &f.project,
            &checkout,
            &selection,
            &scope,
            native(),
            &req,
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::Interrupted
    );
}
#[test]
fn scoped_capture_checks_expired_original_before_loading_identity() {
    let f = Fixture::new();
    let req = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    ));
    let scope = project_scope(&f, &req);
    let checkout =
        GitCheckout::inspect(f.project.root(), Duration::from_secs(5), req.cancellation()).unwrap();
    let selection =
        VersionContext::select(&f.project, checkout.as_checkout(), "", None, "").unwrap();
    std::thread::sleep(
        req.deadline()
            .unwrap()
            .saturating_duration_since(Instant::now()),
    );
    assert_eq!(
        CapturedProjectInventory::capture_in_scope(
            &f.project,
            &checkout,
            &selection,
            &scope,
            native(),
            &req,
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::TimedOut
    );
}
#[test]
fn scoped_capture_propagates_missing_ephemeral_and_corrupt_identity() {
    let f = Fixture::new();
    let req = request();
    let scope = project_scope(&f, &req);
    let original = fs::read(f.project.identity_file()).unwrap();
    fs::remove_file(f.project.identity_file()).unwrap();
    assert_eq!(
        scoped_capture(&f.project, &req, &scope, "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    fs::write(f.project.identity_file(), &original).unwrap();
    let identity = ProjectIdentityStore::at(&f.project)
        .load()
        .unwrap()
        .unwrap();
    let mut metadata = identity.metadata().clone();
    metadata.ephemeral = true;
    ProjectIdentityStore::at(&f.project)
        .writer(Duration::from_secs(5), req.cancellation())
        .unwrap()
        .update(identity.id(), &metadata)
        .unwrap();
    assert_eq!(
        scoped_capture(&f.project, &req, &scope, "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    fs::write(f.project.identity_file(), b"{").unwrap();
    assert_eq!(
        scoped_capture(&f.project, &req, &scope, "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert!(!f.store().path().exists());
}
#[test]
fn scoped_capture_identity_change_before_writer_preserves_inventory() {
    let f = Fixture::new();
    let other = Fixture::new();
    let req = request();
    let scope = project_scope(&f, &req);
    let cap = scoped_capture(&f.project, &req, &scope, "").unwrap();
    let mut writer = f.writer(&req);
    writer.register_checkout(&cap).unwrap();
    let bytes = fs::read(f.store().path()).unwrap();
    fs::copy(other.project.identity_file(), f.project.identity_file()).unwrap();
    assert_eq!(
        writer.retain_version(&cap, "published").unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(f.store().path()).unwrap(), bytes);
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
