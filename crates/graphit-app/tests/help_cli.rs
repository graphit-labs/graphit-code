use std::{fs, process::Command};

fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
    command.current_dir(root).env_clear();
    #[cfg(windows)]
    if let Some(value) = std::env::var_os("SYSTEMROOT") {
        command.env("SYSTEMROOT", value);
    }
    command
}

#[test]
fn public_help_lists_current_commands_without_setup() {
    let root = tempfile::tempdir().unwrap();
    for args in [vec!["--help"], vec!["-h"], vec!["help"]] {
        let output = command(root.path()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Graphit Code: A Powerful Agent Harness"));
        assert!(help.contains(env!("GRAPHIT_COMPILED_BUILD_BRAND")));
        assert!(help.contains("package"));
        assert!(help.contains("--version"));
        assert!(help.contains("--help"));
    }
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn help_ignores_invalid_runtime_configuration_and_metadata() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("graphit.json");
    fs::write(&config, b"invalid fixture configuration").unwrap();
    let expected = command(root.path()).arg("--help").output().unwrap();
    let output = command(root.path())
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .env("GRAPHIT_BUILD_BRAND", "runtime override")
        .env("GRAPHIT_BUILD_VERSION", "runtime override")
        .arg("--help")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, expected.stdout);
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(config).unwrap(), b"invalid fixture configuration");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn nested_package_help_and_version_conflicts_use_the_central_parser() {
    let root = tempfile::tempdir().unwrap();
    let output = command(root.path())
        .args(["package", "--help"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    for name in ["export", "inspect", "import"] {
        assert!(help.contains(name));
    }
    for args in [
        vec!["--version", "package"],
        vec!["--version", "package", "inspect", "--input", "absent.zip"],
    ] {
        let output = command(root.path()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}
