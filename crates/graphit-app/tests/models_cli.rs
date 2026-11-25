#![cfg(target_os = "linux")]

use graphit_core::brand::Brand;
use std::{fs, path::Path, process::Command};

fn brand() -> Brand {
    Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap()
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
    command.current_dir(root).env_clear();
    command
}

#[test]
fn models_help_is_parsed_before_installation_setup() {
    let root = tempfile::tempdir().unwrap();
    let invalid = root.path().join("invalid-global");
    fs::write(&invalid, b"not a directory").unwrap();
    for args in [
        vec!["models", "--help"],
        vec!["models", "install", "--help"],
        vec!["models", "select", "--help"],
    ] {
        let output = command(root.path())
            .env(
                brand().environment_variable("GLOBAL_DIR").unwrap(),
                &invalid,
            )
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
    }
    assert_eq!(fs::read(&invalid).unwrap(), b"not a directory");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn invalid_models_arguments_never_bootstrap() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["models", "install", "unknown-model"],
        vec!["models", "install"],
        vec!["models", "select"],
        vec!["models", "--timeout", "0", "select", "missing.json"],
        vec!["models", "--timeout", "invalid", "select", "missing.json"],
        vec!["--version", "models", "select", "missing.json"],
    ] {
        let output = command(root.path())
            .env(
                brand().environment_variable("GLOBAL_DIR").unwrap(),
                root.path().join("installation"),
            )
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert!(fs::read_dir(root.path()).unwrap().next().is_none());
    }
}

fn assert_missing_selection(output: std::process::Output, installation: &Path) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("models:")
    );
    assert!(installation.join("models").is_dir());
    assert!(!installation.join("auth.json").exists());
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(installation.join("config.json")).unwrap()).unwrap();
    let settings = config["settings"].as_object().unwrap();
    assert!(settings.contains_key("unit.id"));
    assert!(settings.contains_key("client.secret"));
    assert!(!settings.contains_key("daemon.models"));
}

#[test]
fn explicit_global_directory_precedes_environment_and_relative_paths_work() {
    let root = tempfile::tempdir().unwrap();
    let output = command(root.path())
        .env(
            brand().environment_variable("GLOBAL_DIR").unwrap(),
            root.path().join("unused"),
        )
        .args([
            "models",
            "select",
            "missing.json",
            "--global-dir",
            "installation",
            "--timeout",
            "10",
        ])
        .output()
        .unwrap();
    assert_missing_selection(output, &root.path().join("installation"));
    assert!(!root.path().join("unused").exists());
}

#[test]
fn environment_and_home_select_the_real_installation() {
    for use_environment in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let mut command = command(root.path());
        command.env("HOME", root.path());
        let installation = if use_environment {
            let path = root.path().join("from-env");
            command.env(brand().environment_variable("GLOBAL_DIR").unwrap(), &path);
            path
        } else {
            root.path().join(brand().directory())
        };
        assert_missing_selection(
            command
                .args(["models", "select", "missing.json"])
                .output()
                .unwrap(),
            &installation,
        );
        if use_environment {
            assert!(!root.path().join(brand().directory()).exists());
        }
    }
}

#[test]
fn rerank_file_install_is_parsed_and_integrity_failure_never_selects() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("invalid.gguf"), b"not the pinned model").unwrap();
    let installation = root.path().join("installation");
    let output = command(root.path())
        .args([
            "models",
            "install",
            "bge-reranker-v2-m3",
            "--file",
            "invalid.gguf",
            "--global-dir",
            "installation",
            "--timeout",
            "10",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("pinned model integrity mismatch")
    );
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(installation.join("config.json")).unwrap()).unwrap();
    assert!(config["settings"].get("daemon.models").is_none());
    assert!(!installation.join("auth.json").exists());
    assert!(
        fs::read_dir(installation.join("models"))
            .unwrap()
            .next()
            .is_none()
    );
}
