use crate::finite_cli;
use finite_cli::FiniteOutput;
use std::{fs, process::Command};

fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
    command.current_dir(root).env_clear();
    // Cargo-built binaries use the prepared native DLL loader environment.
    #[cfg(windows)]
    for name in ["SYSTEMROOT", "PATH"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

#[test]
fn public_help_lists_current_commands_without_setup() {
    let root = tempfile::tempdir().unwrap();
    for args in [vec!["--help"], vec!["-h"], vec!["help"]] {
        let output = command(root.path())
            .args(args)
            .finite_output()
            .expect("capture Graphit CLI output");
        assert_eq!(
            output.status.code(),
            Some(0),
            "Graphit CLI {:?}: status={:?}",
            env!("CARGO_BIN_EXE_graphit"),
            output.status,
        );
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Graphit Code: Manage project context"));
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
    let expected = command(root.path())
        .arg("--help")
        .finite_output()
        .expect("capture Graphit CLI output");
    let output = command(root.path())
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .env("GRAPHIT_BUILD_BRAND", "runtime override")
        .env("GRAPHIT_BUILD_VERSION", "runtime override")
        .arg("--help")
        .finite_output()
        .expect("capture Graphit CLI output");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Graphit CLI {:?}: status={:?}",
        env!("CARGO_BIN_EXE_graphit"),
        output.status,
    );
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
        .finite_output()
        .expect("capture Graphit CLI output");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Graphit CLI {:?}: status={:?}",
        env!("CARGO_BIN_EXE_graphit"),
        output.status,
    );
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    for name in ["export", "inspect", "import"] {
        assert!(help.contains(name));
    }
    for args in [
        vec!["--version", "package"],
        vec!["--version", "package", "inspect", "--input", "absent.zip"],
    ] {
        let output = command(root.path())
            .args(args)
            .finite_output()
            .expect("capture Graphit CLI output");
        assert_eq!(
            output.status.code(),
            Some(2),
            "Graphit CLI {:?}: status={:?}",
            env!("CARGO_BIN_EXE_graphit"),
            output.status,
        );
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn human_config_is_default_and_json_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    let installation = root.path().join("installation");
    for (json, expected) in [
        (false, "Configuration saved."),
        (true, "{\"changed\":false}"),
    ] {
        let mut cli = command(root.path());
        cli.args(["--global-dir"]).arg(&installation).args([
            "config",
            "--global",
            "set",
            "custom.test",
            "hello",
        ]);
        if json {
            cli.arg("--json");
        }
        let output = cli.finite_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
    }
    let values = command(root.path())
        .arg("--global-dir")
        .arg(&installation)
        .args(["config", "--global", "get", "custom.test"])
        .finite_output()
        .unwrap();
    let text = String::from_utf8(values.stdout).unwrap();
    assert!(text.contains("Value: hello"), "{text}");
    assert!(text.contains("Source: global"), "{text}");
    assert!(!text.contains("{\"kind\""));
}

#[test]
fn functional_commands_have_legible_help_and_reject_usage_without_startup() {
    let root = tempfile::tempdir().unwrap();
    for path in [
        vec!["projects", "list"],
        vec!["projects", "show"],
        vec!["projects", "label-set"],
        vec!["projects", "label-unset"],
        vec!["index", "status"],
        vec!["index", "run"],
        vec!["knowledge", "list"],
        vec!["knowledge", "lint"],
        vec!["memory", "list"],
        vec!["memory", "search"],
        vec!["memory", "get"],
        vec!["memory", "history"],
        vec!["memory", "create"],
        vec!["memory", "update"],
        vec!["memory", "delete"],
        vec!["memory", "mandatory"],
        vec!["memory", "important"],
        vec!["task", "cancel-session"],
        vec!["task", "export"],
        vec!["task", "export-session"],
        vec!["references", "task"],
        vec!["references", "task-backlinks"],
        vec!["references", "knowledge"],
    ] {
        if !cfg!(target_os = "linux") && path == ["task", "cancel-session"] {
            continue;
        }
        let output = command(root.path())
            .args(&path)
            .args(["--json", "--help"])
            .finite_output()
            .unwrap();
        assert!(
            output.status.success(),
            "{path:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Usage:"));
        assert!(!help.trim_start().starts_with('{'));
    }
    for args in [
        vec!["projects", "show"],
        vec!["projects", "list", "--label-value", "api"],
        vec![
            "projects",
            "list",
            "--version",
            "other",
            "--project-version",
            "main",
        ],
        vec![
            "projects",
            "retire-checkout",
            "--project-id",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "--project-version",
            "main",
        ],
        vec![
            "projects",
            "label-set",
            "--project-id",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "--key",
            "service",
        ],
        vec![
            "projects",
            "label-unset",
            "--project-id",
            "not-an-id",
            "--key",
            "service",
        ],
        vec![
            "memory",
            "create",
            "--title",
            "Missing body",
            "--idempotency-key",
            "missing-body",
        ],
        vec!["memory", "list", "--scope", "user", "--project-dir", "."],
        vec![
            "--project-version",
            "main",
            "memory",
            "list",
            "--scope",
            "user",
        ],
        vec![
            "references",
            "knowledge",
            "--target-id",
            "id",
            "--target-kind",
            "memory",
        ],
        vec!["task", "export", "--id", "bad", "--page-size", "0"],
        vec!["memory", "mandatory", "--value", "maybe"],
        vec!["references", "task", "--task", "one", "--session", "two"],
    ] {
        let output = command(root.path()).args(&args).finite_output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert!(std::fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn functional_help_explains_input_pagination_and_index_waiting() {
    let root = tempfile::tempdir().unwrap();
    for (path, expected) in [
        (
            vec!["memory", "create"],
            vec![
                "--body <BODY>",
                "--body-file <FILE>",
                "standard input",
                "--tag <TAG>",
            ],
        ),
        (
            vec!["memory", "list"],
            vec![
                "--mandatory[=<MANDATORY>]",
                "--mandatory=false",
                "local installation",
            ],
        ),
        (
            vec!["index", "run"],
            vec![
                "--wait-ms <MILLISECONDS>",
                "Timeout exits 3",
                "Ctrl+C",
                "--module",
            ],
        ),
        (
            vec!["knowledge", "list"],
            vec!["--cursor <CURSOR>", "JSON continuation", "--stale=false"],
        ),
        (
            vec!["task", "export"],
            vec!["--output <FILE>", "every page is exported"],
        ),
        (
            vec!["references", "knowledge"],
            vec![
                "--target-project-id <ID>",
                "--target-user-id <ID>",
                "Exact JSON cursor",
            ],
        ),
    ] {
        let output = command(root.path())
            .args(&path)
            .args(["--json", "--help"])
            .finite_output()
            .unwrap();
        assert!(output.status.success(), "{path:?}: {output:?}");
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in expected {
            assert!(
                help.contains(expected),
                "{path:?}: missing {expected}: {help}"
            );
        }
    }
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}
