use graphit_core::brand::Brand;
use graphit_core::file_lock::Cancellation;
use graphit_core::project_identity::{
    ProjectId, ProjectIdentity, ProjectIdentityStore, ProjectMetadata,
};
use graphit_core::project_paths::{InstallationPaths, ProjectPaths};
use graphit_core::project_root::resolve_project_root;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

mod support;
use support::Fixture;

const ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const DOCUMENT: &str =
    r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","name":"project","ephemeral":false,"settings":{}}"#;

#[test]
fn requested_identity_is_created_reused_or_rejected_without_replacement() {
    let fixture = Fixture::new();
    let store = store(&fixture);
    let mut writer = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let requested: ProjectId = ID.parse().unwrap();
    let first = writer
        .ensure_with_id(&ProjectMetadata::new("initial").unwrap(), requested)
        .unwrap();
    assert_eq!(first.id(), requested);
    let file = fixture.path(".graphit/project.json");
    let bytes = fs::read(&file).unwrap();
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    let changed = ProjectMetadata::new("other-name").unwrap();
    assert_eq!(writer.ensure_with_id(&changed, requested).unwrap(), first);
    let conflict = "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap();
    assert_eq!(
        writer
            .ensure_with_id(&changed, conflict)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    assert_eq!(store.load().unwrap().unwrap(), first);
}

#[cfg(unix)]
#[test]
fn identity_writer_keeps_private_adjacent_lock_under_public_parent_without_runtime() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let fixture = Fixture::new();
    let brand = fixture.path(".graphit");
    fs::create_dir(&brand).unwrap();
    fs::set_permissions(&brand, fs::Permissions::from_mode(0o755)).unwrap();
    let store = store(&fixture);
    let lock = brand.join(".project.json.lock");
    let mut writer = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let original = fs::metadata(&lock).unwrap();
    let identity = writer
        .ensure(&ProjectMetadata::new("private").unwrap())
        .unwrap();
    drop(writer);
    let mut reopened = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert_eq!(reopened.ensure(identity.metadata()).unwrap(), identity);
    let observed = fs::metadata(&lock).unwrap();
    assert_eq!(
        (original.dev(), original.ino()),
        (observed.dev(), observed.ino())
    );
    assert_eq!(observed.permissions().mode() & 0o7777, 0o600);
    assert_eq!(
        fs::metadata(&brand).unwrap().permissions().mode() & 0o7777,
        0o755
    );
    assert!(!brand.join("runtime").exists());
}

#[test]
fn identity_writer_rejects_nonregular_adjacent_lock_without_replacing_it() {
    let fixture = Fixture::new();
    let brand = fixture.path(".graphit");
    fs::create_dir(&brand).unwrap();
    let lock = brand.join(".project.json.lock");
    fs::create_dir(&lock).unwrap();
    assert!(
        store(&fixture)
            .writer(Duration::ZERO, &Cancellation::default())
            .is_err()
    );
    assert!(lock.is_dir());
    assert!(!brand.join("runtime").exists());
    assert!(!brand.join("project.json").exists());
}

fn project_at(root: impl AsRef<Path>, global: &Path) -> io::Result<ProjectPaths> {
    let root = root.as_ref();
    let installation = InstallationPaths::resolve(Brand::default(), root, None, Some(global))?;
    ProjectPaths::at(root, &installation)
}

fn store(fixture: &Fixture) -> ProjectIdentityStore {
    ProjectIdentityStore::at(
        &project_at(fixture.root(), &fixture.path("absent-installation")).unwrap(),
    )
}

fn manifest(root: &Path, bytes: &[u8]) {
    fs::create_dir_all(root.join(".graphit")).unwrap();
    fs::write(root.join(".graphit/project.json"), bytes).unwrap();
}

#[test]
fn absent_reads_are_side_effect_free_and_installation_is_not_a_project() {
    let fixture = Fixture::new();
    assert!(store(&fixture).load().unwrap().is_none());
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
    let installation = fixture.path("global");
    fs::create_dir(&installation).unwrap();
    manifest(&installation, DOCUMENT.as_bytes());
    assert_eq!(
        project_at(&installation, &installation).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        project_at(fixture.path("absent"), &installation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    let child = installation.join("real-project");
    fs::create_dir(&child).unwrap();
    assert!(
        ProjectIdentityStore::at(&project_at(&child, &installation).unwrap())
            .load()
            .unwrap()
            .is_none()
    );
    assert!(!child.join(".graphit").exists());
    #[cfg(unix)]
    {
        let alias = fixture.path("alias");
        std::os::unix::fs::symlink(&installation, &alias).unwrap();
        assert!(project_at(&alias, &installation).is_err());
        assert!(project_at(&installation, &alias).is_err());
    }
}

#[test]
fn identity_is_lazy_immutable_and_metadata_updates_are_explicit() {
    let fixture = Fixture::new();
    let store = store(&fixture);
    let mut writer = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert!(
        store.load().unwrap().is_none(),
        "acquiring a writer must not create identity"
    );
    let first = writer
        .ensure(&ProjectMetadata::new(" First.Project ").unwrap())
        .unwrap();
    assert_eq!(
        first.id().to_string().parse::<ProjectId>().unwrap(),
        first.id()
    );
    assert_eq!(first.metadata().name, "first.project");
    assert!(first.allows_persistent_stores());
    let before = fs::read(fixture.path(".graphit/project.json")).unwrap();
    let stamp = fs::metadata(fixture.path(".graphit/project.json"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        writer
            .ensure(&ProjectMetadata::new("second").unwrap())
            .unwrap(),
        first
    );
    assert_eq!(writer.update(first.id(), first.metadata()).unwrap(), first);
    assert_eq!(
        fs::metadata(fixture.path(".graphit/project.json"))
            .unwrap()
            .modified()
            .unwrap(),
        stamp
    );
    let conflict: ProjectId = ID.parse().unwrap();
    assert_ne!(conflict, first.id());
    assert_eq!(
        writer
            .update(conflict, &ProjectMetadata::new("conflict").unwrap())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        fs::read(fixture.path(".graphit/project.json")).unwrap(),
        before
    );
    let mut metadata = ProjectMetadata::new(" RENAMED ").unwrap();
    metadata.description = " Unicode Ω\n preserved ".into();
    metadata.ephemeral = true;
    metadata.cluster.insert(
        " team ".into(),
        vec![" beta ".into(), "alpha".into(), "beta".into(), " ".into()],
    );
    metadata.cluster.insert(" ".into(), vec!["ignored".into()]);
    metadata.cluster.insert("empty".into(), vec![" ".into()]);
    let updated = writer.update(first.id(), &metadata).unwrap();
    assert_eq!(updated.id(), first.id());
    assert_eq!(
        updated.metadata().cluster.get("team").unwrap(),
        &vec!["alpha".to_owned(), "beta".to_owned()]
    );
    assert_eq!(updated.metadata().cluster.len(), 1);
    assert_eq!(updated.metadata().description, metadata.description);
    assert!(!updated.allows_persistent_stores());
    assert_eq!(store.load().unwrap().unwrap(), updated);
    drop(writer);
    let reopened = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    drop(reopened);
    assert!(fixture.path(".graphit/.project.json.lock").is_file());
}

#[test]
fn explicit_names_are_strict_and_automatic_names_are_local() {
    for valid in ["a", "A", "A.B_C-D", " a ", &"a".repeat(64)] {
        assert!(ProjectMetadata::new(valid).is_ok(), "{valid:?}");
    }
    for invalid in [
        "",
        " ",
        "a/",
        "/a",
        "a\\b",
        ".a",
        "a-",
        "_a",
        "é",
        "a b",
        "a\0b",
        &"a".repeat(65),
    ] {
        assert!(ProjectMetadata::new(invalid).is_err(), "{invalid:?}");
    }
    let fixture = Fixture::new();
    for (directory, expected) in [
        (" Project Ω Test ", "project-test"),
        ("___", "project"),
        ("My.Project", "my.project"),
    ] {
        let root = fixture.path(directory);
        fs::create_dir(&root).unwrap();
        assert_eq!(ProjectMetadata::for_directory(root).unwrap().name, expected);
    }
    for invalid in [
        "00000000000000000000000000",
        "01arz3ndektsv4rrffq69g5fav",
        "81ARZ3NDEKTSV4RRFFQ69G5FAV",
        "01FIRST",
        "",
        " 01ARZ3NDEKTSV4RRFFQ69G5FAV",
    ] {
        assert!(invalid.parse::<ProjectId>().is_err(), "{invalid:?}");
    }
    assert_eq!(ID.parse::<ProjectId>().unwrap().to_string(), ID);
}

#[test]
fn malformed_documents_fail_closed_without_becoming_absence() {
    let fixture = Fixture::new();
    let store = store(&fixture);
    let invalid = [
        "{}".to_owned(),
        DOCUMENT.replace(ID, "00000000000000000000000000"),
        DOCUMENT.replace(ID, &ID.to_ascii_lowercase()),
        DOCUMENT.replace("\"project\"", "\" Project \""),
        DOCUMENT.replace(
            "\"ephemeral\":false",
            "\"ephemeral\":false,\"ephemeral\":true",
        ),
        DOCUMENT.replace("\"ephemeral\":false", "\"ephemeral\":false,\"unknown\":1"),
        DOCUMENT.replace(
            "\"ephemeral\":false",
            "\"ephemeral\":false,\"cluster\":{\"x\":[\"a\"],\"x\":[\"b\"]}",
        ),
        DOCUMENT.replace(
            "\"ephemeral\":false",
            "\"ephemeral\":false,\"cluster\":{\"x\":[\"b\",\"a\"]}",
        ),
        DOCUMENT.replace(
            "\"ephemeral\":false",
            "\"ephemeral\":false,\"cluster\":{\" x\":[\"a\"],\"x\":[\"b\"]}",
        ),
        DOCUMENT.replace(
            "\"ephemeral\":false",
            "\"ephemeral\":false,\"cluster\":{\"x\":[\"\"]}",
        ),
        DOCUMENT.replace("\"id\":", "\"unknown\":1,\"id\":"),
        format!("{DOCUMENT} {DOCUMENT}"),
    ];
    for text in invalid {
        manifest(fixture.root(), text.as_bytes());
        assert_eq!(
            store.load().unwrap_err().kind(),
            io::ErrorKind::InvalidData,
            "{text}"
        );
        assert!(resolve_project_root(fixture.root(), &Brand::default()).is_err());
        let mut writer = store
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap();
        assert!(
            writer
                .ensure(&ProjectMetadata::new("safe").unwrap())
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(fixture.path(".graphit/project.json")).unwrap(),
            text
        );
    }
    for bytes in [vec![0xff], vec![b' '; 64 * 1024 + 1]] {
        manifest(fixture.root(), &bytes);
        assert_eq!(store.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read(fixture.path(".graphit/project.json")).unwrap(),
            bytes
        );
    }
}

#[test]
fn invalid_metadata_and_byte_budgets_preserve_destination() {
    let fixture = Fixture::new();
    let store = store(&fixture);
    let mut writer = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    assert_eq!(
        writer
            .update(
                ID.parse().unwrap(),
                &ProjectMetadata::new("absent").unwrap()
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    let first = writer
        .ensure(&ProjectMetadata::new("safe").unwrap())
        .unwrap();
    let before = fs::read(fixture.path(".graphit/project.json")).unwrap();
    let mut invalid = first.metadata().clone();
    invalid.name = "../../outside".into();
    assert!(writer.update(first.id(), &invalid).is_err());
    let mut invalid = first.metadata().clone();
    invalid.cluster.insert("a".into(), vec!["one".into()]);
    invalid.cluster.insert(" a ".into(), vec!["two".into()]);
    assert!(writer.update(first.id(), &invalid).is_err());
    invalid.cluster.insert("a".into(), Vec::new());
    assert!(
        writer.update(first.id(), &invalid).is_err(),
        "empty values cannot hide a normalized key collision"
    );
    for description in ["x".repeat(64 * 1024 + 1), "\0".repeat(12 * 1024)] {
        let mut invalid = first.metadata().clone();
        invalid.description = description;
        assert_eq!(
            writer.update(first.id(), &invalid).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(
        fs::read(fixture.path(".graphit/project.json")).unwrap(),
        before
    );
    assert_eq!(fs::read_dir(fixture.path(".graphit")).unwrap().count(), 2);
}

#[test]
fn cancellation_deadline_and_readonly_files_are_distinct_failures() {
    let fixture = Fixture::new();
    let store = store(&fixture);
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        store
            .writer(Duration::ZERO, &cancellation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!fixture.path(".graphit").exists());
    let token = Cancellation::default();
    let mut writer = store.writer(Duration::ZERO, &token).unwrap();
    assert_eq!(
        store
            .writer(Duration::from_millis(5), &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    let first = writer
        .ensure(&ProjectMetadata::new("safe").unwrap())
        .unwrap();
    let path = fixture.path(".graphit/project.json");
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    assert_eq!(writer.update(first.id(), first.metadata()).unwrap(), first);
    assert_eq!(
        writer
            .update(first.id(), &ProjectMetadata::new("changed").unwrap())
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::set_permissions(&path, original).unwrap();
    token.cancel();
    assert_eq!(
        writer
            .update(first.id(), &ProjectMetadata::new("changed").unwrap())
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(store.load().unwrap().unwrap(), first);
}

#[test]
fn root_discovery_uses_new_identity_and_nearest_git_without_legacy_fallback() {
    let fixture = Fixture::new();
    let root = fixture.path("outer");
    let nested = root.join("pkg/deep");
    fs::create_dir_all(&nested).unwrap();
    fs::write(root.join(".git"), "gitdir: shared").unwrap();
    fs::write(root.join("pkg/graphit.lock.json"), "{}").unwrap();
    assert_eq!(
        resolve_project_root(&nested, &Brand::default()).unwrap(),
        root.canonicalize().unwrap()
    );
    manifest(&root.join("pkg"), DOCUMENT.as_bytes());
    assert_eq!(
        resolve_project_root(&nested, &Brand::default()).unwrap(),
        root.join("pkg").canonicalize().unwrap()
    );
    manifest(&nested, b"{}");
    assert!(
        resolve_project_root(&nested, &Brand::default()).is_err(),
        "invalid nearest identity cannot fall through to parent"
    );
    fs::remove_file(nested.join(".graphit/project.json")).unwrap();
    assert_eq!(
        resolve_project_root(&nested, &Brand::default()).unwrap(),
        root.join("pkg").canonicalize().unwrap()
    );
}

#[cfg(unix)]
#[test]
fn invalid_observed_git_markers_cannot_select_an_enclosing_project() {
    use std::os::unix::{fs::symlink, net::UnixListener};
    let fixture = Fixture::new();
    manifest(fixture.root(), DOCUMENT.as_bytes());
    let nested = fixture.path("nested");
    fs::create_dir(&nested).unwrap();
    let marker = nested.join(".git");
    for target in [fixture.path("missing"), fixture.path(".graphit")] {
        symlink(target, &marker).unwrap();
        assert_eq!(
            resolve_project_root(&nested, &Brand::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        fs::remove_file(&marker).unwrap();
    }
    let socket = UnixListener::bind(&marker).unwrap();
    assert!(resolve_project_root(&nested, &Brand::default()).is_err());
    drop(socket);
    fs::remove_file(&marker).unwrap();
    assert_eq!(
        resolve_project_root(&nested, &Brand::default()).unwrap(),
        fixture.root().canonicalize().unwrap()
    );
}

#[cfg(unix)]
#[test]
fn observed_links_and_special_files_are_rejected_and_modes_are_preserved() {
    use std::os::unix::{
        fs::{PermissionsExt, symlink},
        net::UnixListener,
    };
    let fixture = Fixture::new();
    let store = store(&fixture);
    let outside = fixture.path("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("file"), DOCUMENT).unwrap();
    let brand = fixture.path(".graphit");
    symlink(&outside, &brand).unwrap();
    assert!(store.load().is_err());
    assert!(
        store
            .writer(Duration::ZERO, &Cancellation::default())
            .is_err()
    );
    fs::remove_file(&brand).unwrap();
    fs::create_dir(&brand).unwrap();
    let path = brand.join("project.json");
    for target in [outside.join("file"), outside.join("missing")] {
        symlink(&target, &path).unwrap();
        assert!(store.load().is_err());
        let mut writer = store
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap();
        assert!(
            writer
                .ensure(&ProjectMetadata::new("safe").unwrap())
                .is_err()
        );
        drop(writer);
        fs::remove_file(&path).unwrap();
    }
    let socket = UnixListener::bind(&path).unwrap();
    assert!(store.load().is_err());
    drop(socket);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(store.load().is_err());
    fs::remove_dir(&path).unwrap();
    let lock = brand.join(".project.json.lock");
    fs::remove_file(&lock).unwrap();
    symlink(outside.join("file"), &lock).unwrap();
    assert!(
        store
            .writer(Duration::ZERO, &Cancellation::default())
            .is_err()
    );
    fs::remove_file(&lock).unwrap();
    let mut writer = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let first = writer
        .ensure(&ProjectMetadata::new("safe").unwrap())
        .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    writer
        .update(first.id(), &ProjectMetadata::new("changed").unwrap())
        .unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_to_string(outside.join("file")).unwrap(), DOCUMENT);
}

#[test]
fn readers_observe_complete_identity_snapshots_during_atomic_updates() {
    let fixture = Fixture::new();
    let store = store(&fixture);
    let mut writer = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    let first = writer
        .ensure(&ProjectMetadata::new("initial").unwrap())
        .unwrap();
    let path = fixture.path(".graphit/project.json");
    let original_bytes = fs::read(&path).unwrap();
    let mut original_reader = fs::File::open(&path).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let snapshots: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|name| {
            let mut metadata = ProjectMetadata::new(name).unwrap();
            metadata.description = name.repeat(4000);
            metadata
        })
        .collect();
    thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut reads = 0;
            let start = Instant::now();
            while !done.load(Ordering::Acquire) || reads < 100 {
                assert!(start.elapsed() < Duration::from_secs(10));
                let identity = store.load().unwrap().unwrap();
                assert_eq!(identity.id(), first.id());
                assert!(
                    identity.metadata() == first.metadata()
                        || snapshots.contains(identity.metadata())
                );
                reads += 1;
            }
            reads
        });
        let updated =
            (0..20).try_for_each(|i| writer.update(first.id(), &snapshots[i % 2]).map(|_| ()));
        done.store(true, Ordering::Release);
        let reads = reader.join();
        updated.unwrap();
        assert!(reads.unwrap() >= 100);
    });
    let mut retained_bytes = Vec::new();
    original_reader.read_to_end(&mut retained_bytes).unwrap();
    assert_eq!(retained_bytes, original_bytes);
    assert_eq!(store.load().unwrap().unwrap().metadata(), &snapshots[1]);
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_ready(child: &mut ChildGuard, ready: &Path) {
    let start = Instant::now();
    while !ready.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child exited before ready"
        );
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(5));
    }
}

fn child(root: &Path, ready: &Path, result: &Path, mode: &str) -> ChildGuard {
    ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "identity_child", "--nocapture"])
            .env("GRAPHIT_IDENTITY_CHILD_ROOT", root)
            .env("GRAPHIT_IDENTITY_CHILD_READY", ready)
            .env("GRAPHIT_IDENTITY_CHILD_RESULT", result)
            .env("GRAPHIT_IDENTITY_CHILD_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

#[test]
fn competing_processes_ensure_exactly_one_identity() {
    let fixture = Fixture::new();
    let root = fixture.path("project");
    fs::create_dir(&root).unwrap();
    let mut children: Vec<_> = (0..8)
        .map(|i| {
            let ready = fixture.path(&format!("ready-{i}"));
            let result = fixture.path(&format!("result-{i}"));
            let mut child = child(&root, &ready, &result, "ensure");
            wait_ready(&mut child, &ready);
            (child, result)
        })
        .collect();
    assert!(!root.join(".graphit").exists());
    for (child, _) in &mut children {
        child.0.stdin.take().unwrap().write_all(b"go").unwrap();
    }
    let mut ids = Vec::new();
    for (child, result) in &mut children {
        let start = Instant::now();
        while child.0.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < Duration::from_secs(15));
            thread::sleep(Duration::from_millis(5));
        }
        assert!(child.0.wait().unwrap().success());
        ids.push(fs::read_to_string(result).unwrap());
    }
    assert!(ids.iter().all(|id| id == &ids[0]));
    let identity =
        ProjectIdentityStore::at(&project_at(&root, &fixture.path("installation")).unwrap())
            .load()
            .unwrap()
            .unwrap();
    assert_eq!(identity.id().to_string(), ids[0]);
}

#[test]
fn killed_writer_releases_identity_lock_without_recreating_it() {
    let fixture = Fixture::new();
    let root = fixture.path("project");
    fs::create_dir(&root).unwrap();
    let ready = fixture.path("ready");
    let mut child = child(&root, &ready, &fixture.path("unused"), "hold");
    wait_ready(&mut child, &ready);
    let store =
        ProjectIdentityStore::at(&project_at(&root, &fixture.path("installation")).unwrap());
    assert!(store.load().unwrap().is_none());
    assert_eq!(
        store
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    let lock = root.join(".graphit/.project.json.lock");
    let stamp = fs::metadata(&lock).unwrap().modified().unwrap();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let mut recovered = store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap();
    recovered
        .ensure(&ProjectMetadata::new("recovered").unwrap())
        .unwrap();
    assert_eq!(fs::metadata(lock).unwrap().modified().unwrap(), stamp);
}

#[test]
fn identity_child() {
    let Some(root) = std::env::var_os("GRAPHIT_IDENTITY_CHILD_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let ready = std::env::var_os("GRAPHIT_IDENTITY_CHILD_READY").unwrap();
    let store =
        ProjectIdentityStore::at(&project_at(root, &root.join("absent-installation")).unwrap());
    if std::env::var("GRAPHIT_IDENTITY_CHILD_MODE").unwrap() == "hold" {
        let _writer = store
            .writer(Duration::ZERO, &Cancellation::default())
            .unwrap();
        fs::write(ready, "ready").unwrap();
        io::stdin().read_to_end(&mut Vec::new()).unwrap();
    } else {
        fs::write(ready, "ready").unwrap();
        io::stdin().read_to_end(&mut Vec::new()).unwrap();
        let mut writer = store
            .writer(Duration::from_secs(10), &Cancellation::default())
            .unwrap();
        let identity = writer
            .ensure(&ProjectMetadata::new("project").unwrap())
            .unwrap();
        fs::write(
            std::env::var_os("GRAPHIT_IDENTITY_CHILD_RESULT").unwrap(),
            identity.id().to_string(),
        )
        .unwrap();
    }
}

#[test]
fn immutable_snapshots_are_send_sync_and_serde_preserves_contract() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ProjectIdentity>();
    assert_send_sync::<ProjectIdentityStore>();
    let fixture = Fixture::new();
    manifest(fixture.root(), DOCUMENT.as_bytes());
    let identity = store(&fixture).load().unwrap().unwrap();
    assert_eq!(identity.id(), ID.parse().unwrap());
    assert_eq!(
        serde_json::from_str::<ProjectIdentity>(&serde_json::to_string(&identity).unwrap())
            .unwrap(),
        identity
    );
}
