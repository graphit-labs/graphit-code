use super::*;
use graphit_core::{
    brand::Brand,
    file_lock::Cancellation,
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
        fs::create_dir_all(installation.root()).unwrap();
        let project = ProjectPaths::at(root, &installation).unwrap();
        ensure(&project, false);
        Self {
            temp,
            installation,
            project,
        }
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
    ) -> Result<VersionContext, ProjectBootstrapError> {
        bootstrap_project(input(&self.project, version), &self.store(), req)
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
fn failure(result: Result<VersionContext, ProjectBootstrapError>) -> ProjectBootstrapError {
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
    assert!(f.installation.root().exists());
    let result = f.run(&req, "").unwrap();
    assert_eq!(result.version(), "main");
    let bytes = fs::read(f.document()).unwrap();
    let metadata = fs::metadata(f.document()).unwrap().modified().unwrap();
    f.run(&req, "").unwrap();
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
    assert_eq!(
        fs::metadata(f.document()).unwrap().modified().unwrap(),
        metadata
    );
    assert_eq!(
        f.run(&req, "release/β").unwrap().source(),
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
    assert_eq!(result.version(), "main");
}

#[test]
fn branch_pin_explicit_foreign_pin_and_detached_use_core_policy() {
    let f = Fixture::new();
    f.init_git();
    let req = request();
    git(f.project.root(), &["switch", "-c", "feature"]);
    assert_eq!(f.run(&req, "").unwrap().version(), "feature");
    let pin = ScopedPin::new(&f.project, "pinned").unwrap();
    let mut selected = input(&f.project, "");
    selected.pin = Some(&pin);
    let result = bootstrap_project(selected, &f.store(), &req).unwrap();
    assert_eq!(result.source(), VersionSource::Pin);
    let mut selected = input(&f.project, "explicit");
    selected.pin = Some(&pin);
    assert_eq!(
        bootstrap_project(selected, &f.store(), &req)
            .unwrap()
            .source(),
        VersionSource::Explicit
    );
    let foreign = Fixture::new();
    let foreign_pin = ScopedPin::new(&foreign.project, "foreign").unwrap();
    let mut selected = input(&f.project, "");
    selected.pin = Some(&foreign_pin);
    assert_eq!(
        bootstrap_project(selected, &f.store(), &req)
            .unwrap()
            .version(),
        "feature"
    );
    git(f.project.root(), &["checkout", "--detach", "HEAD"]);
    let mut selected = input(&f.project, "");
    selected.detached_base = "refs/heads/main";
    let result = bootstrap_project(selected, &f.store(), &req).unwrap();
    assert!(result.is_detached());
    assert_eq!(result.source(), VersionSource::DetachedCommit);
    assert_eq!(
        f.run(&req, "").unwrap().source(),
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
        bootstrap_project(input(&project, ""), &f.store(), &req).unwrap();
    }
    let b_root = f.temp.path().join("project-b");
    fs::create_dir(&b_root).unwrap();
    let b = ProjectPaths::at(b_root, &f.installation).unwrap();
    ensure(&b, false);
    bootstrap_project(input(&b, "b-version"), &f.store(), &req).unwrap();
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
fn cancelled_and_expired_requests_preserve_inventory() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    let service = Arc::new(SnapshotContext::service(Cancellation::default()));
    assert!(matches!(
        failure(bootstrap_project(input(&f.project, "after"), &f.store(), &service)),
        ProjectBootstrapError::Request(ref e) if e.kind() == io::ErrorKind::InvalidInput
    ));
    req.cancellation().cancel();
    assert!(matches!(
        failure(bootstrap_project(input(&f.project, "after"), &f.store(), &req)),
        ProjectBootstrapError::Request(ref e) if e.kind() == io::ErrorKind::Interrupted
    ));
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert!(matches!(
        failure(bootstrap_project(input(&f.project, "after"), &f.store(), &expired)),
        ProjectBootstrapError::Request(ref e) if e.kind() == io::ErrorKind::TimedOut
    ));
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
    // A separate Original can perform a later operation with the same plain paths.
    assert!(f.run(&request(), "after").is_ok());
}

#[test]
fn missing_ephemeral_identity_and_invalid_selection_create_no_inventory() {
    let f = Fixture::new();
    let req = request();
    fs::remove_file(f.project.identity_file()).unwrap();
    assert!(
        matches!(failure(f.run(&req, "")), ProjectBootstrapError::Identity(ref e) if e.kind()==io::ErrorKind::NotFound)
    );
    assert!(!f.project.identity_file().exists());
    assert!(!f.document().exists());
    ensure(&f.project, true);
    assert!(matches!(
        failure(f.run(&req, "")),
        ProjectBootstrapError::Identity(_)
    ));
    assert!(!f.document().exists());
    assert!(
        matches!(failure(f.run(&req, "invalid\u{0000}version")), ProjectBootstrapError::Selection(ref e) if e.kind()==io::ErrorKind::InvalidInput)
    );
}

#[test]
fn wrong_brand_and_bad_git_preserve_old_state() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    let other = InstallationPaths::resolve(
        Brand::new("other").unwrap(),
        f.temp.path(),
        None,
        Some(&f.temp.path().join("other-global")),
    )
    .unwrap();
    let project = ProjectPaths::at(f.project.root(), &other).unwrap();
    ensure(&project, false);
    assert!(matches!(
        failure(bootstrap_project(input(&project, "after"), &f.store(), &req)),
        ProjectBootstrapError::Inventory(ref e) if e.kind() == io::ErrorKind::InvalidInput
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
fn ordinary_inventory_permissions_are_accepted() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    fs::set_permissions(f.document(), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(f.run(&req, "after").unwrap().version(), "after");
    assert_eq!(
        fs::metadata(f.document()).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
fn bounded_inventory_decoder_preserves_existing_document() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    let bounded = GlobalProjectInventory::at(&f.installation, 1).unwrap();
    assert!(
        matches!(failure(bootstrap_project(input(&f.project, "after"), &bounded, &req)), ProjectBootstrapError::Inventory(ref e) if e.kind()==io::ErrorKind::InvalidData)
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}

#[test]
fn missing_inventory_parent_returns_filesystem_error() {
    let f = Fixture::new();
    let req = request();
    fs::remove_dir(f.installation.root()).unwrap();
    let error = failure(bootstrap_project(input(&f.project, ""), &f.store(), &req));
    assert!(
        matches!(error, ProjectBootstrapError::Inventory(ref e) if e.kind() == io::ErrorKind::NotFound)
    );
    assert!(error.source().is_some());
    assert!(!f.installation.root().exists());
}

#[test]
fn corrupt_identity_preserves_inventory() {
    let f = Fixture::new();
    let req = request();
    f.run(&req, "before").unwrap();
    let bytes = fs::read(f.document()).unwrap();
    fs::write(f.project.identity_file(), b"{}").unwrap();
    assert!(
        matches!(failure(f.run(&req, "after")), ProjectBootstrapError::Identity(ref e) if e.kind() == io::ErrorKind::InvalidData)
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}
