use graphit_core::brand::Brand;
use graphit_core::file_lock::Cancellation;
use graphit_core::project_identity::{ProjectIdentityStore, ProjectMetadata};
use graphit_core::project_paths::{InstallationPaths, ProjectPaths};
use graphit_core::version_context::{Checkout, ScopedPin, VersionContext};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

mod support;
use support::Fixture;

fn installation(fixture: &Fixture, brand: &str) -> InstallationPaths {
    InstallationPaths::resolve(
        Brand::new(brand).unwrap(),
        fixture.root(),
        None,
        Some(&fixture.path("global")),
    )
    .unwrap()
}

#[test]
fn default_and_private_namespaces_have_one_validated_naming_contract() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Brand>();
    assert_send_sync::<InstallationPaths>();
    assert_send_sync::<ProjectPaths>();
    let brand = Brand::new(" Acme-Code ").unwrap();
    assert_eq!(brand.key(), "acme-code");
    assert_eq!(brand.directory(), ".acme-code");
    assert_eq!(brand.environment_prefix(), "ACME_CODE");
    assert_eq!(
        brand.environment_variable("GLOBAL_DIR").unwrap(),
        "ACME_CODE_GLOBAL_DIR"
    );
    assert_eq!(brand.binary_name(), "acme-code");
    assert_eq!(brand.windows_binary_name(), "acme-code.exe");
    assert_eq!(brand.mcp_server_name("ast").unwrap(), "acme-code-ast-mcp");
    assert_eq!(brand.skill_name("task").unwrap(), "acme-code-task");
    assert_eq!(
        brand.mcp_tool_name(&["task", "session_get"]).unwrap(),
        "acme-code_task_session_get"
    );
    assert_eq!(Brand::default().key(), "graphit");
    for invalid in [
        "",
        "a_b",
        ".a",
        "a-",
        "a/b",
        "a\\b",
        "á",
        "NUL",
        "con",
        "lpt9",
        "git",
        " GIT ",
        "gitignore",
        "gitattributes",
        "gitmodules",
        &"a".repeat(65),
    ] {
        assert!(Brand::new(invalid).is_err(), "{invalid:?}");
    }
    for invalid in ["", "../escape", "lowercase", "A\0B"] {
        assert!(brand.environment_variable(invalid).is_err());
    }
    assert!(brand.mcp_server_name("../escape").is_err());
    assert!(brand.mcp_tool_name(&[]).is_err());
    assert!(brand.mcp_tool_name(&["x"; 65]).is_err());
}

#[test]
fn bootstrap_override_and_home_resolution_are_explicit_and_side_effect_free() {
    let fixture = Fixture::new();
    assert!(Brand::new("git").is_err());
    assert!(!fixture.path(".git").exists());
    let home = fixture.path("home");
    let standard = InstallationPaths::resolve(
        Brand::new("acme-code").unwrap(),
        fixture.root(),
        Some(&home),
        None,
    )
    .unwrap();
    assert_eq!(standard.root(), home.join(".acme-code"));
    let blank = InstallationPaths::resolve(
        Brand::new("acme-code").unwrap(),
        fixture.root(),
        Some(&home),
        Some(Path::new(" \t ")),
    )
    .unwrap();
    assert_eq!(blank.root(), standard.root());
    let override_path = fixture.path("elsewhere");
    let absolute =
        InstallationPaths::resolve(Brand::default(), fixture.root(), None, Some(&override_path))
            .unwrap();
    assert_eq!(absolute.root(), override_path);
    let relative = InstallationPaths::resolve(
        Brand::default(),
        fixture.root(),
        None,
        Some(Path::new(" custom/global ")),
    )
    .unwrap();
    assert_eq!(
        relative.root(),
        fixture.root().canonicalize().unwrap().join("custom/global")
    );
    assert!(InstallationPaths::resolve(Brand::default(), fixture.root(), None, None).is_err());
    assert!(
        InstallationPaths::resolve(
            Brand::default(),
            fixture.root(),
            Some(Path::new("relative-home")),
            None
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
    #[cfg(windows)]
    assert!(
        InstallationPaths::resolve(
            Brand::default(),
            fixture.root(),
            None,
            Some(Path::new("C:relative"))
        )
        .is_err()
    );
}

#[test]
fn derived_paths_remain_under_the_chosen_root_and_runtime_versions_are_separate() {
    let fixture = Fixture::new();
    let installation = installation(&fixture, "acme-code");
    let project = ProjectPaths::at(fixture.root(), &installation).unwrap();
    assert_eq!(
        installation.config_file(),
        installation.root().join("config.json")
    );
    assert_eq!(
        installation.auth_file(),
        installation.root().join("auth.json")
    );
    assert_eq!(
        installation.models_directory(),
        installation.root().join("models")
    );
    for parts in [
        &["ast", "project", "id"][..],
        &["rules"][..],
        &["hub", "cache"][..],
        &["grammars", "antlr"][..],
    ] {
        assert!(
            installation
                .global_path(parts)
                .unwrap()
                .starts_with(installation.root())
        );
    }
    assert_eq!(
        installation.framework_runtime("v1.2.3").unwrap(),
        installation.root().join("runtime/v1.2.3")
    );
    assert_eq!(
        project
            .project_runtime(
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                &["daemon", "daemon.log"]
            )
            .unwrap(),
        installation
            .root()
            .join("projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/daemon/daemon.log")
    );
    assert_ne!(
        installation.framework_runtime("dev").unwrap(),
        project
            .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[])
            .unwrap()
    );
    assert!(
        installation
            .framework_runtime("feature/git-branch")
            .is_err()
    );
    for part in [
        "",
        ".",
        "..",
        "/escape",
        "a/b",
        "a\\b",
        "C:escape",
        "a?",
        "a*",
        "a|",
        "a\0b",
        "trailing.",
        "trailing ",
        "NUL",
        "con.txt",
        "COM1",
        "lpt9.log",
        "COM¹",
        "com².log",
        "COM³",
        "LPT¹",
        "lpt².log",
        "LPT³",
        &"a".repeat(129),
    ] {
        assert!(installation.global_path(&[part]).is_err(), "{part:?}");
        assert!(
            project
                .project_runtime("01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(), &[part])
                .is_err(),
            "{part:?}"
        );
    }
    assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
}

#[test]
fn project_storage_uses_captured_identity_across_checkouts_and_isolates_installations() {
    let fixture = Fixture::new();
    let installation = installation(&fixture, "graphit");
    let first_root = fixture.path("first");
    let second_root = fixture.path("second");
    fs::create_dir(&first_root).unwrap();
    fs::create_dir(&second_root).unwrap();
    let first = ProjectPaths::at(&first_root, &installation).unwrap();
    let second = ProjectPaths::at(&second_root, &installation).unwrap();
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
    let other = "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap();
    assert_eq!(
        first.project_runtime(id, &["memory"]).unwrap(),
        second.project_runtime(id, &["memory"]).unwrap()
    );
    assert_ne!(
        first.project_runtime(id, &["memory"]).unwrap(),
        first.project_runtime(other, &["memory"]).unwrap()
    );
    let isolated = InstallationPaths::resolve(
        Brand::default(),
        fixture.root(),
        None,
        Some(&fixture.path("other-global")),
    )
    .unwrap();
    let isolated = ProjectPaths::at(&first_root, &isolated).unwrap();
    assert_ne!(
        first.project_runtime(id, &["memory"]).unwrap(),
        isolated.project_runtime(id, &["memory"]).unwrap()
    );
    assert_eq!(
        first.identity_file(),
        first_root.join(".graphit/project.json")
    );
    assert!(!installation.root().exists());
    assert!(!first.brand_directory().exists());
    assert!(!second.brand_directory().exists());
}

#[test]
fn snapshots_own_their_inputs_and_do_not_share_later_configuration() {
    let fixture = Fixture::new();
    let mut override_path = fixture.path("first");
    let first = InstallationPaths::resolve(
        Brand::new("first").unwrap(),
        fixture.root(),
        None,
        Some(&override_path),
    )
    .unwrap();
    override_path.push("later");
    let second = InstallationPaths::resolve(
        Brand::new("second").unwrap(),
        fixture.root(),
        None,
        Some(&override_path),
    )
    .unwrap();
    assert_eq!(first.root(), fixture.path("first"));
    assert_eq!(first.brand().directory(), ".first");
    assert_eq!(second.root(), fixture.path("first/later"));
    assert_eq!(second.brand().directory(), ".second");
    assert_eq!(
        first.clone().global_path(&["config.json"]).unwrap(),
        fixture.path("first/config.json")
    );
}

const CHILD_ROOT: &str = "GRAPHIT_PATHS_TEST_ROOT";

#[test]
#[ignore = "private entry point started by the working-directory test"]
fn bootstrap_working_directory_child() {
    let root = std::path::PathBuf::from(std::env::var_os(CHILD_ROOT).unwrap());
    std::env::set_current_dir(&root).unwrap();
    let paths = InstallationPaths::resolve(
        Brand::new("acme-code").unwrap(),
        Path::new("."),
        None,
        Some(Path::new("relative-global")),
    )
    .unwrap();
    let project = ProjectPaths::at(&root, &paths).unwrap();
    std::env::set_current_dir(root.join("later")).unwrap();
    assert_eq!(
        paths.root(),
        root.canonicalize().unwrap().join("relative-global")
    );
    assert_eq!(
        paths.config_file(),
        root.join("relative-global/config.json")
    );
    assert_eq!(project.root(), root.canonicalize().unwrap());
    assert!(!paths.root().exists());
}

#[test]
fn captured_bootstrap_paths_do_not_drift_after_a_process_changes_directory() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.path("later")).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "bootstrap_working_directory_child"])
        .env(CHILD_ROOT, fixture.root().canonicalize().unwrap())
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stdout)
    );
}

#[test]
fn brands_have_independent_identities_and_version_pins_on_the_same_physical_root() {
    let fixture = Fixture::new();
    let graphit = ProjectPaths::at(fixture.root(), &installation(&fixture, "graphit")).unwrap();
    let acme = ProjectPaths::at(fixture.root(), &installation(&fixture, "acme-code")).unwrap();
    let graphit_store = ProjectIdentityStore::at(&graphit);
    let acme_store = ProjectIdentityStore::at(&acme);
    let g = graphit_store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("graphit-project").unwrap())
        .unwrap();
    let a = acme_store
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("acme-project").unwrap())
        .unwrap();
    assert_ne!(g.id(), a.id());
    assert_eq!(graphit_store.load().unwrap().unwrap(), g);
    assert_eq!(acme_store.load().unwrap().unwrap(), a);
    assert!(fixture.path(".graphit/project.json").is_file());
    assert!(fixture.path(".acme-code/project.json").is_file());
    let pin = ScopedPin::new(&graphit, "pin").unwrap();
    let facts = Checkout::Branch {
        name: "feature/x",
        default: "main",
    };
    assert_eq!(
        VersionContext::select(&graphit, facts, "", Some(&pin), "")
            .unwrap()
            .version(),
        "pin"
    );
    assert_eq!(
        VersionContext::select(&acme, facts, "", Some(&pin), "")
            .unwrap()
            .version(),
        "feature/x"
    );
    // Selection uses the captured scope even when source files cease to exist.
    fs::remove_file(graphit.identity_file()).unwrap();
    assert_eq!(
        VersionContext::select(&graphit, facts, "", Some(&pin), "")
            .unwrap()
            .version(),
        "pin"
    );
}

#[test]
fn private_discovery_and_global_guards_respect_namespace_and_aliases() {
    let fixture = Fixture::new();
    let acme_installation = installation(&fixture, "acme-code");
    let global = fixture.path("global");
    fs::create_dir(&global).unwrap();
    assert!(ProjectPaths::at(&global, &acme_installation).is_err());
    let nested = fixture.path("nested/deep");
    fs::create_dir_all(&nested).unwrap();
    let parent = ProjectPaths::at(fixture.root(), &acme_installation).unwrap();
    ProjectIdentityStore::at(&parent)
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("parent").unwrap())
        .unwrap();
    assert_eq!(
        ProjectPaths::discover(&global, &acme_installation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let child = ProjectPaths::at(fixture.path("nested"), &acme_installation).unwrap();
    ProjectIdentityStore::at(&child)
        .writer(Duration::ZERO, &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("child").unwrap())
        .unwrap();
    assert_eq!(
        ProjectPaths::discover(&nested, &acme_installation)
            .unwrap()
            .root(),
        child.root()
    );
    assert_eq!(
        ProjectPaths::discover(&nested, &installation(&fixture, "graphit"))
            .unwrap()
            .root(),
        nested.canonicalize().unwrap()
    );
    #[cfg(unix)]
    {
        let alias = fixture.path("alias");
        std::os::unix::fs::symlink(&global, &alias).unwrap();
        assert!(ProjectPaths::at(&alias, &acme_installation).is_err());
        assert!(ProjectPaths::discover(&alias, &acme_installation).is_err());
    }
}

#[cfg(unix)]
#[test]
fn non_unicode_bootstrap_paths_fail_without_creating_state() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = Fixture::new();
    let invalid = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![b'x', 0xff]));
    for result in [
        InstallationPaths::resolve(Brand::default(), fixture.root(), None, Some(&invalid)),
        InstallationPaths::resolve(
            Brand::default(),
            fixture.root(),
            Some(&fixture.root().join(&invalid)),
            None,
        ),
    ] {
        let error = result.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("Unicode"));
    }
    let directory = fixture.root().join(invalid);
    #[cfg(target_os = "linux")]
    fs::create_dir(&directory).unwrap();
    let error = InstallationPaths::resolve(
        Brand::default(),
        &directory,
        None,
        Some(Path::new("global")),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("Unicode"));
    #[cfg(target_os = "linux")]
    {
        let alias = fixture.root().join("unicode-alias");
        std::os::unix::fs::symlink(&directory, &alias).unwrap();
        assert!(
            InstallationPaths::resolve(Brand::default(), &alias, None, Some(Path::new("global")))
                .is_err()
        );
        assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 2);
    }
    assert!(!directory.join("global").exists());
}
