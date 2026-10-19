use super::*;
use graphit_core::{
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::InstallationPaths,
    version_context::VersionSource,
};
use std::{fs, path::Path, process::Command, time::Duration};

fn request() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
fn limits() -> ProjectBootstrapLimits {
    ProjectBootstrapLimits {
        scope: ScopeLimits::new(4, 4096, 256).unwrap(),
        native: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
    }
}
struct Fixture {
    temp: tempfile::TempDir,
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
        let project = ProjectPaths::at(root, &installation).unwrap();
        ensure(&project, false);
        Self {
            temp,
            installation,
            project,
        }
    }
    fn prepared(&self, req: &SnapshotContext) -> PrivateInstallation {
        installation_bootstrap::prepare(
            &self.installation,
            BootstrapLimits::new(4096, 128).unwrap(),
            req,
        )
        .into_installation()
        .unwrap()
    }
    fn store(&self) -> GlobalProjectInventory {
        GlobalProjectInventory::at(&self.installation, 65536).unwrap()
    }
    fn document(&self) -> std::path::PathBuf {
        self.installation.root().join("project-inventory.json")
    }
    fn run(
        &self,
        req: &Arc<SnapshotContext>,
        version: &str,
    ) -> Result<BootstrappedProject, ProjectBootstrapError> {
        bootstrap_project(
            input(&self.project, version),
            limits(),
            &self.store(),
            self.prepared(req),
            req,
        )
    }
    fn init_git(&self) {
        git(self.project.root(), &["init", "-b", "main"]);
        git(self.project.root(), &["add", "."]);
        git(self.project.root(), &["commit", "-m", "fixture"]);
    }
}
fn input<'a>(project: &'a ProjectPaths, version: &'a str) -> ProjectBootstrapInput<'a> {
    ProjectBootstrapInput {
        project,
        explicit_version: version,
        pin: None,
        detached_base: "",
    }
}
fn ensure(project: &ProjectPaths, ephemeral: bool) {
    let mut metadata = ProjectMetadata::new("fixture").unwrap();
    metadata.ephemeral = ephemeral;
    ProjectIdentityStore::at(project)
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .ensure(&metadata)
        .unwrap();
}
fn git(root: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    let result = command
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            if cfg!(windows) {
                "core.hooksPath=NUL"
            } else {
                "core.hooksPath=/dev/null"
            },
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "gc.auto=0",
        ])
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn failure(result: Result<BootstrappedProject, ProjectBootstrapError>) -> ProjectBootstrapError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected failure"),
    }
}

#[test]
fn non_git_register_reload_noop_and_switch_keep_history() {
    let f = Fixture::new();
    let req = request();
    assert_eq!(f.store().load(&req).unwrap().projects().len(), 0);
    assert!(!f.installation.root().exists());
    let result = f.run(&req, "").unwrap();
    assert_eq!(result.checkout(), &GitCheckout::NonGit);
    assert_eq!(result.version().version(), "main");
    assert_eq!(result.disposition(), InventoryRegistration::Registered);
    let bytes = fs::read(f.document()).unwrap();
    let metadata = fs::metadata(f.document()).unwrap().modified().unwrap();
    assert_eq!(
        f.run(&req, "").unwrap().disposition(),
        InventoryRegistration::Unchanged
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
    assert_eq!(
        fs::metadata(f.document()).unwrap().modified().unwrap(),
        metadata
    );
    assert_eq!(
        f.run(&req, "release/β").unwrap().version().source(),
        VersionSource::Explicit
    );
    let snapshot = f.store().load(&req).unwrap();
    let project = snapshot.projects().next().unwrap();
    assert_eq!(
        project.versions().collect::<Vec<_>>(),
        ["main", "release/β"]
    );
    assert_eq!(
        project.checkouts().next().unwrap().selected_version(),
        "release/β"
    );
    // Owned data survives request drop; a second write also demonstrates writer RAII release.
    drop(req);
    assert_eq!(result.version().version(), "main");
}

#[test]
fn branch_pin_explicit_foreign_pin_and_detached_use_core_policy() {
    let f = Fixture::new();
    f.init_git();
    let req = request();
    git(f.project.root(), &["switch", "-c", "feature"]);
    assert_eq!(f.run(&req, "").unwrap().version().version(), "feature");
    let pin = ScopedPin::new(&f.project, "pinned").unwrap();
    let mut selected = input(&f.project, "");
    selected.pin = Some(&pin);
    let result = bootstrap_project(selected, limits(), &f.store(), f.prepared(&req), &req).unwrap();
    assert_eq!(result.version().source(), VersionSource::Pin);
    let mut selected = input(&f.project, "explicit");
    selected.pin = Some(&pin);
    assert_eq!(
        bootstrap_project(selected, limits(), &f.store(), f.prepared(&req), &req)
            .unwrap()
            .version()
            .source(),
        VersionSource::Explicit
    );
    let foreign = Fixture::new();
    let foreign_pin = ScopedPin::new(&foreign.project, "foreign").unwrap();
    let mut selected = input(&f.project, "");
    selected.pin = Some(&foreign_pin);
    assert_eq!(
        bootstrap_project(selected, limits(), &f.store(), f.prepared(&req), &req)
            .unwrap()
            .version()
            .version(),
        "feature"
    );
    git(f.project.root(), &["checkout", "--detach", "HEAD"]);
    let mut selected = input(&f.project, "");
    selected.detached_base = "refs/heads/main";
    let result = bootstrap_project(selected, limits(), &f.store(), f.prepared(&req), &req).unwrap();
    assert!(result.version().is_detached());
    assert_eq!(result.version().source(), VersionSource::DetachedCommit);
    assert_eq!(
        f.run(&req, "").unwrap().version().source(),
        VersionSource::DetachedCommit
    );
}

#[test]
fn projects_and_real_worktrees_are_isolated_and_share_persistent_identity() {
    let f = Fixture::new();
    f.init_git();
    let req = request();
    f.run(&req, "").unwrap();
    let id = ProjectIdentityStore::at(&f.project)
        .load()
        .unwrap()
        .unwrap()
        .id();
    for (directory, branch) in [("work-a", "branch-a"), ("work-b", "branch-b")] {
        let root = f.temp.path().join(directory);
        git(
            f.project.root(),
            &["worktree", "add", "-b", branch, root.to_str().unwrap()],
        );
        let project = ProjectPaths::at(root, &f.installation).unwrap();
        bootstrap_project(
            input(&project, ""),
            limits(),
            &f.store(),
            f.prepared(&req),
            &req,
        )
        .unwrap();
    }
    let b_root = f.temp.path().join("project-b");
    fs::create_dir(&b_root).unwrap();
    let b = ProjectPaths::at(b_root, &f.installation).unwrap();
    ensure(&b, false);
    bootstrap_project(
        input(&b, "b-version"),
        limits(),
        &f.store(),
        f.prepared(&req),
        &req,
    )
    .unwrap();
    let snapshot = f.store().load(&req).unwrap();
    assert_eq!(snapshot.projects().len(), 2);
    let a = snapshot.get(id).unwrap();
    assert_eq!(a.checkouts().len(), 3);
    assert_eq!(
        a.versions().collect::<Vec<_>>(),
        ["branch-a", "branch-b", "main"]
    );
}

#[test]
fn cancelled_expired_and_foreign_requests_preserve_inventory() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    // Preparing this fixture is separate from the bootstrap operation under test.
    let service = Arc::new(SnapshotContext::service(Cancellation::default()));
    let prepared = f.prepared(&service);
    assert!(matches!(
        failure(bootstrap_project(input(&f.project, "after"), limits(), &f.store(), prepared, &service)),
        ProjectBootstrapError::Request(ref e) if e.kind() == io::ErrorKind::InvalidInput
    ));
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
    let prepared = f.prepared(&req);
    req.cancellation().cancel();
    assert!(
        matches!(failure(bootstrap_project(input(&f.project, "after"), limits(), &f.store(), prepared, &req)),
        ProjectBootstrapError::Request(ref e) if e.kind()==io::ErrorKind::Interrupted)
    );
    let original = request();
    let prepared = f.prepared(&original);
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert!(
        matches!(failure(bootstrap_project(input(&f.project, "after"), limits(), &f.store(), prepared, &expired)),
        ProjectBootstrapError::Request(ref e) if e.kind()==io::ErrorKind::TimedOut)
    );
    let prepared = f.prepared(&original);
    let foreign = request();
    assert!(matches!(
        failure(bootstrap_project(
            input(&f.project, "after"),
            limits(),
            &f.store(),
            prepared,
            &foreign
        )),
        ProjectBootstrapError::Installation(AuthDocumentError::Conflict)
    ));
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}

#[test]
fn missing_ephemeral_identity_and_invalid_selection_create_no_inventory() {
    let f = Fixture::new();
    let req = request();
    fs::remove_file(f.project.identity_file()).unwrap();
    assert!(
        matches!(failure(f.run(&req, "")), ProjectBootstrapError::Capture(ref e) if e.kind()==io::ErrorKind::NotFound)
    );
    assert!(!f.project.identity_file().exists());
    assert!(!f.document().exists());
    ensure(&f.project, true);
    assert!(matches!(
        failure(f.run(&req, "")),
        ProjectBootstrapError::Capture(_)
    ));
    assert!(!f.document().exists());
    assert!(
        matches!(failure(f.run(&req, "invalid\u{0000}version")), ProjectBootstrapError::Selection(ref e) if e.kind()==io::ErrorKind::InvalidInput)
    );
}

#[test]
fn wrong_brand_foreign_installation_bad_git_and_caps_preserve_old_state() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    let foreign = Fixture::new();
    assert!(matches!(
        failure(bootstrap_project(
            input(&f.project, "after"),
            limits(),
            &f.store(),
            foreign.prepared(&req),
            &req
        )),
        ProjectBootstrapError::Inventory(_)
    ));
    let other = InstallationPaths::resolve(
        Brand::new("other").unwrap(),
        f.temp.path(),
        None,
        Some(&f.temp.path().join("other-global")),
    )
    .unwrap();
    let project = ProjectPaths::at(f.project.root(), &other).unwrap();
    assert!(
        matches!(failure(bootstrap_project(input(&project, "after"), limits(), &f.store(), f.prepared(&req), &req)), ProjectBootstrapError::Capture(ref e) if e.kind()==io::ErrorKind::InvalidInput)
    );
    let mut capped = limits();
    capped.scope = ScopeLimits::new(4, 1, 256).unwrap();
    assert!(matches!(
        failure(bootstrap_project(
            input(&f.project, "after"),
            capped,
            &f.store(),
            f.prepared(&req),
            &req
        )),
        ProjectBootstrapError::Capture(_)
    ));
    fs::create_dir(f.project.root().join(".git")).unwrap();
    assert!(matches!(
        failure(f.run(&req, "after")),
        ProjectBootstrapError::Checkout(_)
    ));
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn unsafe_inventory_permissions_return_real_inventory_error() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    fs::set_permissions(f.document(), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        matches!(failure(f.run(&req, "after")), ProjectBootstrapError::Inventory(ref e) if e.kind()==io::ErrorKind::PermissionDenied)
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}

#[test]
fn bounded_inventory_decoder_preserves_existing_document() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    let bounded = GlobalProjectInventory::at(&f.installation, 1).unwrap();
    assert!(
        matches!(failure(bootstrap_project(input(&f.project, "after"), limits(), &bounded,
        f.prepared(&req), &req)), ProjectBootstrapError::Inventory(ref e) if e.kind()==io::ErrorKind::InvalidData)
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}

#[test]
fn missing_physical_installation_is_not_repaired_and_source_is_retained() {
    let f = Fixture::new();
    let req = request();
    let prepared = f.prepared(&req);
    fs::remove_dir(f.installation.root()).unwrap();
    let error = failure(bootstrap_project(
        input(&f.project, ""),
        limits(),
        &f.store(),
        prepared,
        &req,
    ));
    assert!(matches!(error, ProjectBootstrapError::Installation(_)));
    assert!(error.source().is_some());
    assert!(error.to_string().contains("installation"));
    assert!(!f.installation.root().exists());
}

#[cfg(unix)]
#[test]
fn readonly_installation_is_rejected_without_permission_repair() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    let prepared = f.prepared(&req);
    fs::set_permissions(f.installation.root(), fs::Permissions::from_mode(0o500)).unwrap();
    let error = failure(bootstrap_project(
        input(&f.project, "after"),
        limits(),
        &f.store(),
        prepared,
        &req,
    ));
    assert!(matches!(error, ProjectBootstrapError::Installation(_)));
    assert_eq!(
        fs::metadata(f.installation.root())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o500
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
    // Fixture cleanup alone restores its own temporary directory.
    fs::set_permissions(f.installation.root(), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn replaced_installation_and_corrupt_identity_preserve_inventory() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    let prepared = f.prepared(&req);
    let old = f.temp.path().join("old-installation");
    fs::rename(f.installation.root(), &old).unwrap();
    // Actual SDK creates another physical directory at the same pathname.
    drop(f.prepared(&req));
    assert!(matches!(
        failure(bootstrap_project(
            input(&f.project, "after"),
            limits(),
            &f.store(),
            prepared,
            &req
        )),
        ProjectBootstrapError::Installation(_)
    ));
    assert!(!f.document().exists());
    assert_eq!(fs::read(old.join("project-inventory.json")).unwrap(), bytes);
    fs::remove_dir_all(f.installation.root()).unwrap();
    fs::rename(old, f.installation.root()).unwrap();
    fs::write(f.project.identity_file(), b"{}").unwrap();
    assert!(
        matches!(failure(f.run(&req, "after")), ProjectBootstrapError::Capture(ref e) if e.kind()==io::ErrorKind::InvalidData)
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}
