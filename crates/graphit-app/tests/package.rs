//! Real CLI physical fixtures; these files are not semantic native datasets.
use crate::finite_cli;
use finite_cli::FiniteOutput;
use std::{
    fs,
    process::{Command, Output},
};

fn cli(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
    command.env_clear().env("HOME", root).current_dir(root);
    #[cfg(windows)]
    if let Some(value) = std::env::var_os("SYSTEMROOT") {
        command.env("SYSTEMROOT", value);
    }
    command
}
fn assert_success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
#[test]
fn package_binary_roundtrips_each_payload_family() {
    for (kind, bundle, file) in [
        ("ast", "graph.icebug", "schema.cypher"),
        ("knowledge", "index.lance", "fragment.bin"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join(bundle).join("empty")).unwrap();
        fs::write(source.join(bundle).join(file), b"physical fixture bytes").unwrap();
        let archive = root.path().join("payload.zip");
        assert_success(
            cli(root.path())
                .args(["package", "export", "--kind", kind, "--source"])
                .arg(&source)
                .arg("--output")
                .arg(&archive)
                .finite_output()
                .unwrap(),
        );
        let result = assert_success(
            cli(root.path())
                .args(["package", "inspect", "--kind", kind, "--input"])
                .arg(&archive)
                .finite_output()
                .unwrap(),
        );
        assert_eq!(String::from_utf8(result.stdout).unwrap().trim(), kind);
        let destination = root.path().join("installed");
        assert_success(
            cli(root.path())
                .args(["package", "import", "--kind", kind, "--input"])
                .arg(&archive)
                .arg("--output")
                .arg(&destination)
                .finite_output()
                .unwrap(),
        );
        assert_eq!(
            fs::read(destination.join(bundle).join(file)).unwrap(),
            b"physical fixture bytes"
        );
        assert!(destination.join(bundle).join("empty").is_dir());
        assert!(
            !cli(root.path())
                .args(["package", "import", "--input"])
                .arg(&archive)
                .arg("--output")
                .arg(&destination)
                .finite_output()
                .unwrap()
                .status
                .success()
        );
    }
}
#[test]
fn package_usage_source_members_and_self_capture_fail_causally() {
    let root = tempfile::tempdir().unwrap();
    assert!(
        cli(root.path())
            .args(["package", "--help"])
            .finite_output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(
        cli(root.path())
            .args(["package", "export", "--kind", "ast"])
            .finite_output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let output = root.path().join("output.zip");
    for kind in ["ast", "knowledge"] {
        let missing = cli(root.path())
            .args(["package", "export", "--kind", kind, "--source"])
            .arg(&source)
            .arg("--output")
            .arg(&output)
            .finite_output()
            .unwrap();
        assert!(!missing.status.success());
        assert!(!missing.stderr.is_empty());
        assert!(!output.exists());
    }
    fs::create_dir(source.join("graph.icebug")).unwrap();
    let schema_missing = cli(root.path())
        .args(["package", "export", "--kind", "ast", "--source"])
        .arg(&source)
        .arg("--output")
        .arg(&output)
        .finite_output()
        .unwrap();
    assert!(!schema_missing.status.success());
    assert!(!output.exists());
    fs::write(source.join("graph.icebug/schema.cypher"), b"schema").unwrap();
    let inside = source.join("output.zip");
    assert!(
        !cli(root.path())
            .args(["package", "export", "--kind", "ast", "--source"])
            .arg(&source)
            .arg("--output")
            .arg(&inside)
            .finite_output()
            .unwrap()
            .status
            .success()
    );
    assert!(!inside.exists());
    fs::create_dir(source.join("search.lance")).unwrap();
    assert!(
        !cli(root.path())
            .args(["package", "export", "--kind", "ast", "--source"])
            .arg(&source)
            .arg("--output")
            .arg(&output)
            .finite_output()
            .unwrap()
            .status
            .success()
    );
    assert!(!output.exists());
}
#[test]
fn version_remains_available_without_package_startup() {
    let root = tempfile::tempdir().unwrap();
    let output = assert_success(
        cli(root.path())
            .arg("--version")
            .current_dir(root.path())
            .finite_output()
            .unwrap(),
    );
    assert!(!output.stdout.is_empty());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}
#[cfg(unix)]
#[test]
fn non_utf8_cli_path_is_rejected_before_package_io() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(source.join("index.lance")).unwrap();
    fs::write(source.join("index.lance/fragment.bin"), b"data").unwrap();
    let archive = root
        .path()
        .join(std::ffi::OsString::from_vec(vec![b'p', 0xff]));
    let output = cli(root.path())
        .args(["package", "export", "--kind", "knowledge", "--source"])
        .arg(&source)
        .arg("--output")
        .arg(&archive)
        .finite_output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("valid Unicode")
    );
    assert!(!archive.exists());
    assert_eq!(
        fs::read(source.join("index.lance/fragment.bin")).unwrap(),
        b"data"
    );
}
