//! Actual executable → daemon → native process → run-owned stdio MCP → Role HTTP.
#![cfg(unix)]
use crate::runtime_peer;
use graphit_app::managed_role_run::RunOutcome;
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigStore},
    file_lock::Cancellation,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::NativeBinding;
use std::{
    fs, io,
    os::unix::fs::symlink,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Daemon {
    async fn stop(&mut self) -> io::Result<()> {
        let sent = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .status()?;
        if !sent.success() {
            return Err(io::Error::other("daemon termination failed"));
        }
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.0.try_wait()? {
                return if status.code() == Some(130) {
                    Ok(())
                } else {
                    Err(io::Error::other(format!(
                        "daemon returned {status} instead of joined cancellation status 130"
                    )))
                };
            }
            if Instant::now() >= until {
                return Err(io::ErrorKind::TimedOut.into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
fn command(project: &Path, installation: &InstallationPaths, path: &std::ffi::OsStr) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
    command
        .current_dir(project)
        .env_clear()
        .env("PATH", path)
        .env("HOME", installation.root())
        .env(
            installation
                .brand()
                .environment_variable("GLOBAL_DIR")
                .unwrap(),
            installation.root(),
        );
    command
}
fn child_fixture(bin: &Path) -> io::Result<()> {
    runtime_peer::install(
        bin,
        Some(serde_json::json!({
            "fixture":"kiro-run", "brand":env!("GRAPHIT_COMPILED_BUILD_BRAND"),
            "graphit":env!("CARGO_BIN_EXE_graphit"),
            "failure_file":bin.join("fixture-phase.json")
        })),
        &["kiro-cli"],
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_role_run_v3_uses_only_run_owned_stdio_credentials()
-> Result<(), Box<dyn std::error::Error>> {
    for (binding, role) in [
        (NativeBinding::KiroV3, "scout"),
        (NativeBinding::KiroV3, "tracker"),
    ] {
        let temp = tempfile::tempdir()?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let installation = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let root = temp.path().join("project");
        fs::create_dir(&root)?;
        fs::write(root.join("raw"), "actual executable Role payload")?;
        if role == "tracker" {
            let run_git = |args: &[&str]| -> io::Result<()> {
                let output = Command::new("git")
                    .args(args)
                    .current_dir(&root)
                    .env_clear()
                    .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                    .env("HOME", temp.path())
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .output()?;
                if output.status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other("temporary Git fixture setup failed"))
                }
            };
            run_git(&["init", "-q", "-b", "main"])?;
            fs::write(root.join(".gitignore"), ".graphit/\n")?;
            fs::write(root.join("tracked"), "alpha\n")?;
            run_git(&["add", "tracked"])?;
            fs::write(root.join("tracked"), "bravo\n")?;
        }
        let project = ProjectPaths::at(&root, &installation)?;
        let cancel = Cancellation::default();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("owned-role")?)?;
        ConfigStore::global(&installation)?.set(
            ConfigKey::new("agent")?,
            serde_json::json!(binding.as_str()),
            Duration::from_secs(5),
            &cancel,
        )?;
        let bin = temp.path().join("bin");
        fs::create_dir(&bin)?;
        child_fixture(&bin)?;
        symlink(env!("CARGO_BIN_EXE_graphit"), bin.join(brand.binary_name()))?;
        let mut search = vec![bin];
        search.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(search)?;
        let log = temp.path().join("daemon.log");
        let mut daemon = Daemon(
            command(&root, &installation, &path)
                .args(["daemon", "foreground", "--project-version", "main"])
                .stdout(Stdio::null())
                .stderr(fs::File::create(&log)?)
                .spawn()?,
        );
        let until = Instant::now() + Duration::from_secs(30);
        while !installation.root().join("daemon/mcp.json").exists() {
            if let Some(status) = daemon.0.try_wait()? {
                return Err(io::Error::other(format!(
                    "daemon startup {status}: {}",
                    fs::read_to_string(&log)?
                ))
                .into());
            }
            if Instant::now() >= until {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "daemon discovery was not published",
                )
                .into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let key = fs::read(installation.root().join("daemon/mcp.key"))?;
        let root_run = root.clone();
        let paths_run = installation.clone();
        let path_run = path.clone();
        let output = tokio::task::spawn_blocking(move || {
            command(&root_run, &paths_run, &path_run)
                .args([
                    "roles",
                    "run",
                    "--role",
                    role,
                    "--prompt",
                    "Read the actual source",
                ])
                .output()
        })
        .await??;
        let result: RunOutcome = serde_json::from_slice(&output.stdout)?;
        assert!(
            output.status.success(),
            "actual roles run status: {}; failure class: {:?}; run closed: {}; public fixture phase: {:?}",
            output.status,
            result.failure,
            result.run_closed,
            fs::read_to_string(temp.path().join("bin/fixture-phase.json")).ok()
        );
        assert!(output.stderr.is_empty());
        assert!(result.failure.is_none(), "run failed: {:?}", result.failure);
        assert!(result.run_closed);
        assert_eq!(
            result.answer.as_deref(),
            Some("actual executable Role payload")
        );
        let profile = root.join(format!(".kiro/agents/{}-{role}.md", brand.key()));
        let native = fs::read_to_string(profile)?;
        assert!(native.contains("--role"));
        if role == "tracker" {
            assert!(
                native.contains("fs_write") && native.contains("shell") && native.contains("deny")
            );
        }
        assert!(!native.contains("GRAPHIT_ROLE_BEARER"));
        assert!(
            !native
                .as_bytes()
                .windows(key.len())
                .any(|window| window == key)
        );
        let denied = command(&root, &installation, &path)
            .args(["mcp", "--stdio", "--role"])
            .stdin(Stdio::null())
            .output()?;
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
        assert_eq!(fs::read(installation.root().join("daemon/mcp.key"))?, key);
        if let Err(error) = daemon.stop().await {
            let stderr = fs::read_to_string(&log);
            let markers = [
                "Graphit Knowledge job failed",
                "Graphit Ast job failed",
                "desired reconciliation",
                "SemanticPublicationFailure",
                "PublishedGraphGuardFailure",
                "RawPublicationFailure",
                "AuthRequest",
                "KnowledgeSourceFirstPublicationError",
                "BeforeCommit",
                "AfterCommit",
                "Artifact Lock Interrupted",
                "Artifact Lock TimedOut",
                "Interrupted",
                "TimedOut",
                "Request",
                "Created",
            ];
            let cause_markers: Vec<_> = markers
                .into_iter()
                .filter(|marker| {
                    stderr.as_ref().is_ok_and(|text| {
                        if marker.contains(' ') {
                            text.contains(marker)
                        } else {
                            text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                                .any(|token| token == *marker)
                        }
                    })
                })
                .collect();
            let diagnostic = serde_json::json!({
                "binding": binding.as_str(),
                "role": role,
                "expected_exit": 130,
                "daemon_exit": daemon.0.try_wait().ok().flatten().and_then(|status| status.code()),
                "shutdown_error_kind": format!("{:?}", error.kind()),
                "stderr_available": stderr.is_ok(),
                "cause_markers": cause_markers,
            });
            return Err(io::Error::other(format!(
                "managed role daemon shutdown diagnostic: {diagnostic}"
            ))
            .into());
        }
        assert!(!installation.root().join("daemon/mcp.key").exists());
        assert!(!installation.root().join("daemon/mcp.json").exists());
    }
    Ok(())
}

#[path = "managed_role_run/native_bindings.rs"]
mod native_bindings;
