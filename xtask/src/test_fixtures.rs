//! Build subprocess peers through Cargo; tests receive its exact executable paths.
use cargo_metadata::{Message, MetadataCommand};
use std::{
    collections::BTreeSet,
    io::BufReader,
    path::Path,
    process::{Command, Stdio},
};

fn app_tests(args: &[&str]) -> bool {
    if args.first() != Some(&"test") {
        return false;
    }
    let mut packages = Vec::new();
    let mut args = args.iter().copied().take_while(|s| *s != "--");
    while let Some(arg) = args.next() {
        if arg == "--doc" {
            return false;
        }
        if arg == "--exclude" && args.next() == Some("graphit-app") {
            return false;
        }
        if arg == "--exclude=graphit-app" {
            return false;
        }
        if arg == "-p" || arg == "--package" {
            if let Some(package) = args.next() {
                packages.push(package);
            }
        } else if let Some(package) = arg
            .strip_prefix("--package=")
            .or_else(|| arg.strip_prefix("-p").filter(|s| !s.is_empty()))
        {
            packages.push(package);
        }
    }
    packages.is_empty()
        || packages.iter().any(|p| {
            p.split('@').next() == Some("graphit-app")
                // Let Cargo resolve package patterns; conservatively prepare
                // peers rather than silently omit them for a matching glob.
                || p.contains(['*', '?', '['])
        })
}

fn build_arguments(args: &[&str]) -> Vec<String> {
    let mut result: Vec<String> = vec![
        "test".into(),
        "--no-run".into(),
        "--locked".into(),
        "--message-format=json-render-diagnostics".into(),
    ];
    let mut selected_targets = false;
    let mut args = args.iter().copied().skip(1).take_while(|s| *s != "--");
    while let Some(arg) = args.next() {
        if matches!(
            arg,
            "--offline"
                | "--frozen"
                | "--release"
                | "--all-features"
                | "--no-default-features"
                | "--workspace"
                | "--lib"
                | "--tests"
                | "--bins"
                | "--all-targets"
        ) {
            selected_targets |= matches!(arg, "--lib" | "--tests" | "--bins" | "--all-targets");
            result.push(arg.into());
        } else if matches!(
            arg,
            "--target"
                | "--target-dir"
                | "--profile"
                | "--features"
                | "-F"
                | "--jobs"
                | "-j"
                | "--package"
                | "-p"
                | "--exclude"
                | "--test"
                | "--bin"
        ) {
            selected_targets |= matches!(arg, "--test" | "--bin");
            result.push(arg.into());
            if let Some(value) = args.next() {
                result.push(value.into());
            }
        } else if [
            "--target=",
            "--target-dir=",
            "--profile=",
            "--features=",
            "--jobs=",
            "--package=",
            "--exclude=",
            "--test=",
            "--bin=",
            "-j",
            "-F",
            "-p",
        ]
        .iter()
        .any(|prefix| arg.starts_with(prefix))
        {
            selected_targets |= arg.starts_with("--test=") || arg.starts_with("--bin=");
            result.push(arg.into());
        }
    }
    // Selected integration tests already need this compilation and emit the CLI.
    if !selected_targets {
        result.push("--tests".into());
    }
    // Library/bin-only selections need executables, not an unrelated test suite.
    if !result.iter().any(|arg| {
        matches!(arg.as_str(), "--tests" | "--all-targets" | "--test") || arg.starts_with("--test=")
    }) {
        result[0] = "build".into();
        result.retain(|arg| !matches!(arg.as_str(), "--no-run" | "--lib"));
        result.extend(["--bin".into(), "graphit".into()]);
    }
    if !result
        .iter()
        .any(|arg| arg == "--release" || arg == "--profile" || arg.starts_with("--profile="))
    {
        result.extend(["--profile".into(), "test".into()]);
    }
    result
}

pub(crate) fn prepare(root: &Path, args: &[&str], test: &mut Command) -> Result<(), String> {
    if !app_tests(args) {
        return Ok(());
    }
    let mut metadata = MetadataCommand::new();
    metadata.manifest_path(root.join("Cargo.toml")).no_deps();
    let mut options = vec!["--locked".into()];
    if args.iter().any(|s| matches!(*s, "--offline" | "--frozen")) {
        options.push("--offline".into());
    }
    let metadata = metadata
        .other_options(options)
        .exec()
        .map_err(|e| e.to_string())?;
    let app = metadata
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == "graphit-app")
        .ok_or("graphit-app package missing")?;
    let peers: BTreeSet<String> = app
        .targets
        .iter()
        .filter(|t| t.is_bin() && t.name.ends_with("-fixture"))
        .map(|t| t.name.clone())
        .collect();
    if peers.is_empty() {
        return Ok(());
    }
    let mut pending = peers.clone();
    pending.insert("graphit".into());
    let arguments = build_arguments(args);
    let mut build = Command::new(test.get_program());
    build
        .args(&arguments)
        .current_dir(root)
        .stdout(Stdio::piped());
    // Keep native preparation, compiler and Cargo metadata environment identical.
    for (name, value) in test.get_envs() {
        if let Some(value) = value {
            build.env(name, value);
        } else {
            build.env_remove(name);
        }
    }
    for name in &peers {
        build.args(["--bin", name]);
    }
    eprintln!(
        "Preparing {} App subprocess fixtures and the normal CLI through Cargo",
        peers.len()
    );
    let mut child = build.spawn().map_err(|e| e.to_string())?;
    let output = child
        .stdout
        .take()
        .ok_or("fixture Cargo stdout unavailable")?;
    let mut error = None;
    for message in Message::parse_stream(BufReader::new(output)) {
        match message {
            Ok(Message::CompilerArtifact(artifact))
                if artifact.package_id == app.id
                    && artifact.target.is_bin()
                    && (artifact.target.name != "graphit" || !artifact.profile.test) =>
            {
                if let Some(path) = artifact
                    .executable
                    .filter(|_| pending.remove(&artifact.target.name))
                {
                    let name = format!(
                        "GRAPHIT_TEST_{}",
                        artifact.target.name.replace('-', "_").to_ascii_uppercase()
                    );
                    test.env(name, path.as_std_path());
                }
            }
            Ok(Message::CompilerMessage(message)) => {
                if let Some(rendered) = message.message.rendered {
                    eprint!("{rendered}");
                }
            }
            Ok(Message::TextLine(line)) => eprintln!("{line}"),
            Err(e) => {
                error = Some(e.to_string());
            }
            _ => {}
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("App fixture build failed: {status}"));
    }
    if let Some(error) = error {
        return Err(error);
    }
    if !pending.is_empty() {
        return Err(format!(
            "Cargo did not emit App fixture executables: {pending:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixtures_follow_selected_test_packages() {
        assert!(app_tests(&["test", "--workspace"]));
        assert!(app_tests(&["test", "-pgraphit-app", "--lib"]));
        assert!(app_tests(&["test", "--package=graphit-app", "filter"]));
        assert!(!app_tests(&["test", "-p", "graphit-core"]));
        assert!(!app_tests(&[
            "test",
            "--workspace",
            "--exclude",
            "graphit-app"
        ]));
        assert!(!app_tests(&["test", "--doc"]));
        assert!(!app_tests(&["build", "-p", "graphit-app"]));
    }
    #[test]
    fn fixture_build_preserves_compilation_options_without_test_filters() {
        let args = build_arguments(&[
            "test",
            "-p",
            "graphit-app",
            "--offline",
            "--target-dir",
            "a path é",
            "--profile=release",
            "-j4",
            "--features",
            "foo",
            "--lib",
            "filter",
            "--",
            "--ignored",
        ]);
        assert!(args.windows(2).any(|a| a == ["--target-dir", "a path é"]));
        for expected in ["--offline", "--profile=release", "-j4", "foo"] {
            assert!(args.iter().any(|a| a == expected));
        }
        assert_eq!(args[0], "build");
        assert!(!args.iter().any(|arg| arg == "--lib"));
        assert!(args.windows(2).any(|a| a == ["-p", "graphit-app"]));
        for absent in ["filter", "--ignored"] {
            assert!(!args.iter().any(|a| a == absent));
        }
    }

    #[test]
    fn peer_preparation_keeps_workspace_and_selected_test_feature_graph() {
        let args = build_arguments(&["test", "--workspace", "--offline"]);
        assert!(args.iter().any(|arg| arg == "--workspace"));
        assert!(args.iter().any(|arg| arg == "--tests"));
        let args = build_arguments(&[
            "test",
            "-p",
            "graphit-app",
            "--test",
            "integration",
            "source_executable::",
        ]);
        assert!(args.windows(2).any(|a| a == ["--test", "integration"]));
        assert!(!args.iter().any(|arg| arg == "--tests"));
        assert!(!args.iter().any(|arg| arg == "source_executable::"));
    }

    #[test]
    fn library_preparation_builds_the_normal_cli_without_linking_integration_tests() {
        let args = build_arguments(&["test", "-p", "graphit-app", "--lib", "library_filter"]);
        assert_eq!(args[0], "build");
        assert!(args.windows(2).any(|args| args == ["--bin", "graphit"]));
        assert!(!args.iter().any(|arg| arg == "library_filter"));
        for absent in [
            "--no-run",
            "--lib",
            "--test",
            "--tests",
            "--all-targets",
            "integration",
        ] {
            assert!(!args.iter().any(|arg| arg == absent));
        }
    }
}
