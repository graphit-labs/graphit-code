#[path = "support/finite_cli.rs"]
mod finite_cli;
use finite_cli::FiniteOutput;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::TempDir;

fn compiled_line() -> Vec<u8> {
    format!(
        "{} {}\n",
        env!("GRAPHIT_COMPILED_BUILD_BRAND"),
        env!("GRAPHIT_COMPILED_BUILD_VERSION")
    )
    .into_bytes()
}

fn command(cwd: &Path, args: &[OsString]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
    command.args(args).current_dir(cwd).env_clear();
    #[cfg(windows)]
    if let Some(value) = std::env::var_os("SYSTEMROOT") {
        command.env("SYSTEMROOT", value);
    }
    command
}

fn assert_output(command: &mut Command, code: i32, stdout: &[u8], stderr: &[u8]) {
    let output = command.finite_output().expect("run Graphit binary");
    assert_eq!(output.status.code(), Some(code));
    assert_eq!(output.stdout, stdout);
    assert_eq!(output.stderr, stderr);
}

type Tree = BTreeMap<PathBuf, Option<Vec<u8>>>;

fn snapshot(root: &Path) -> Tree {
    fn visit(root: &Path, dir: &Path, tree: &mut Tree) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                tree.insert(relative, None);
                visit(root, &path, tree);
            } else {
                tree.insert(relative, Some(fs::read(path).unwrap()));
            }
        }
    }
    let mut tree = Tree::new();
    visit(root, root, &mut tree);
    tree
}

#[test]
fn version_runs_without_setup_and_preserves_empty_cwd() {
    let root = TempDir::new().unwrap();
    let before = snapshot(root.path());
    let expected = compiled_line();
    if env!("GRAPHIT_COMPILED_BUILD_BRAND") == "graphit"
        && env!("GRAPHIT_COMPILED_BUILD_VERSION") == "dev"
    {
        assert_eq!(expected, b"graphit dev\n");
    }
    assert_output(
        &mut command(root.path(), &["--version".into()]),
        0,
        &expected,
        b"",
    );
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn compiled_version_ignores_child_environment_and_invalid_owned_configs() {
    let root = TempDir::new().unwrap();
    let home = root.path().join("fixture home");
    let project = root.path().join("fixture project");
    let global = home.join(".graphit");
    fs::create_dir_all(&global).unwrap();
    fs::create_dir(&project).unwrap();
    fs::write(global.join("config.json"), b"invalid global fixture").unwrap();
    fs::write(project.join("graphit.json"), b"invalid project fixture").unwrap();
    fs::write(project.join("graphit.lock.json"), b"invalid lock fixture").unwrap();
    let before = snapshot(root.path());
    let expected = compiled_line();
    let mut command = command(&project, &["--version".into()]);
    for key in ["HOME", "USERPROFILE", "XDG_CONFIG_HOME"] {
        command.env(key, &home);
    }
    for key in [
        "GRAPHIT_BUILD_BRAND",
        "GRAPHIT_BUILD_VERSION",
        "GRAPHIT_COMPILED_BUILD_BRAND",
        "GRAPHIT_COMPILED_BUILD_VERSION",
    ] {
        command.env(key, "runtime-override-is-not-build-data");
    }
    assert_output(&mut command, 0, &expected, b"");
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn unsupported_arguments_fail_with_clap_diagnostic_and_no_output() {
    let mut cases: Vec<Vec<OsString>> = vec![
        vec![],
        vec!["setup".into()],
        vec!["--versionx".into()],
        vec!["--version=true".into()],
        vec!["--version".into(), "extra".into()],
        vec!["--version".into(), "--version".into()],
        vec!["x".repeat(4096).into()],
    ];
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        cases.push(vec![OsString::from_vec(vec![0xff])]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        cases.push(vec![OsString::from_wide(&[0xd800])]);
    }
    for args in cases {
        let root = TempDir::new().unwrap();
        let before = snapshot(root.path());
        let output = command(root.path(), &args).finite_output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert_eq!(snapshot(root.path()), before);
    }
}
