#![cfg(target_os = "linux")]
//! Ordinary CLI startup in a fresh private installation, without a user manager.

use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    native_path::MAX_PATH_BYTES,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_inventory::GlobalProjectInventory,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::policy::{HubResourcePolicy, ResourceSetting};
use std::{
    fs, io,
    process::Command,
    time::{Duration, Instant},
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct PrivateInstallation {
    root: Option<tempfile::TempDir>,
    paths: InstallationPaths,
    project: ProjectPaths,
}

impl PrivateInstallation {
    fn command(&self) -> Command {
        let root = self.root.as_ref().unwrap().path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", root)
            .env("XDG_RUNTIME_DIR", root.join("absent-bus"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .current_dir(self.project.root())
            .arg("--global-dir")
            .arg(self.paths.root());
        command
    }

    async fn run(&self, args: &[&str]) -> Result<std::process::Output> {
        let mut command = tokio::process::Command::from(self.command());
        command.args(args).kill_on_drop(true);
        Ok(tokio::time::timeout(Duration::from_secs(90), command.output()).await??)
    }

    async fn success(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = self.run(args).await?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "{args:?}: status={} stdout={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ))
            .into());
        }
        Ok(output.stdout)
    }

    fn assert_not_started(&self) {
        for path in ["daemon.pid", "daemon/mcp.json", "daemon/mcp.key"] {
            assert!(!self.paths.root().join(path).exists(), "unexpected {path}");
        }
    }
}

impl Drop for PrivateInstallation {
    fn drop(&mut self) {
        let stop = self.command().args(["daemon", "stop"]).output();
        let released = match fs::File::open(self.paths.root().join("daemon.pid")) {
            Ok(file) => file.try_lock().is_ok(),
            Err(error) => error.kind() == io::ErrorKind::NotFound,
        };
        if !released
            || ["daemon/mcp.json", "daemon/mcp.key"]
                .iter()
                .any(|path| self.paths.root().join(path).exists())
        {
            let retained = self.root.take().unwrap().keep();
            eprintln!(
                "private daemon cleanup failed; retained {}: {stop:?}",
                retained.display()
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ordinary_source_starts_maintains_reuses_and_stops_private_daemon() -> Result {
    let root = tempfile::tempdir()?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let paths = InstallationPaths::resolve(
        brand.clone(),
        root.path(),
        None,
        Some(&root.path().join("installation")),
    )?;
    let config = ConfigSnapshot::new(&brand, [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancellation)?;
    installation_bootstrap::prepare(
        &paths,
        BootstrapLimits::new(
            MAX_PATH_BYTES,
            usize::try_from(policy.limit(ResourceSetting::SnapshotDepth).value)?,
        )?,
        original.snapshot(),
    )
    .into_installation()?;
    for (key, value) in [
        ("agent", serde_json::json!("codex")),
        ("modules.ast", serde_json::json!(false)),
    ] {
        ConfigStore::global(&paths)?.set(
            ConfigKey::new(key)?,
            value,
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    let project_root = root.path().join("project");
    fs::create_dir(&project_root)?;
    let project = ProjectPaths::at(&project_root, &paths)?;
    let project_id = ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancellation)?
        .ensure(&ProjectMetadata::new("ordinary-autostart")?)?
        .id();
    fs::write(
        project.root().join("notes.unsupported"),
        "alpha\nbeta\ngamma",
    )?;
    let fixture = PrivateInstallation {
        root: Some(root),
        paths,
        project,
    };
    fixture.assert_not_started();

    // Invalid typed CLI arguments cannot start a daemon.
    let invalid = fixture
        .run(&[
            "ast",
            "query",
            "MATCH (n) RETURN n",
            "--param",
            "missing-separator",
        ])
        .await?;
    assert_eq!(
        invalid.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&invalid.stderr)
    );
    fixture.assert_not_started();
    let mut role = tokio::process::Command::from(fixture.command());
    role.env("GRAPHIT_ROLE_BEARER", "not-a-root-credential")
        .args(["ast", "source", "notes.unsupported"])
        .kill_on_drop(true);
    let denied = tokio::time::timeout(Duration::from_secs(90), role.output()).await??;
    assert_eq!(
        denied.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&denied.stderr)
    );
    fixture.assert_not_started();
    let _ = fixture.run(&["daemon", "status"]).await?;
    let _ = fixture
        .command()
        .args([
            "projects",
            "retire-checkout",
            "--project-id",
            &project_id.to_string(),
            "--project-dir",
            fixture.project.root().to_str().unwrap(),
        ])
        .output()?;
    fixture.assert_not_started();

    // A project without Git has the retained version main; selecting an
    // invented version cannot authorize reading files from its checkout.
    assert_eq!(
        fixture
            .success(&[
                "--project-version",
                "main",
                "ast",
                "source",
                "notes.unsupported",
                "--head",
                "1",
            ])
            .await?,
        b"alpha"
    );
    let first: serde_json::Value =
        serde_json::from_slice(&fixture.success(&["daemon", "status"]).await?)?;
    assert_eq!(first["state"], "running");
    let pid = first["pid"]
        .as_u64()
        .ok_or("authenticated daemon PID missing")?;
    assert!(fixture.paths.root().join("daemon/mcp.key").exists());
    assert_eq!(
        fs::read_link(format!("/proc/{pid}/cwd"))?,
        fixture.paths.root()
    );
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let inventory = GlobalProjectInventory::at(&fixture.paths, policy.content().max_input_bytes)?
        .load(original.snapshot())?;
    assert!(
        inventory
            .get(project_id)
            .ok_or("CLI did not maintain project")?
            .checkouts()
            .any(|checkout| std::path::Path::new(checkout.path()) == fixture.project.root())
    );
    assert!(
        !fixture
            .project
            .project_runtime(project_id, &["ast", "source.lance"])?
            .exists()
    );

    assert_eq!(
        fixture
            .success(&[
                "--project-version",
                "main",
                "ast",
                "source",
                "notes.unsupported",
                "--tail",
                "1",
            ])
            .await?,
        b"gamma"
    );
    let second: serde_json::Value =
        serde_json::from_slice(&fixture.success(&["daemon", "status"]).await?)?;
    assert_eq!(
        second["pid"], pid,
        "ordinary CLI must reuse its live daemon"
    );
    let stopped: serde_json::Value =
        serde_json::from_slice(&fixture.success(&["daemon", "stop"]).await?)?;
    assert_eq!(stopped["state"], "stopped");
    assert!(!fixture.paths.root().join("daemon/mcp.json").exists());
    assert!(!fixture.paths.root().join("daemon/mcp.key").exists());
    let lock = fs::File::open(fixture.paths.root().join("daemon.pid"))?;
    lock.try_lock()?;
    lock.unlock()?;
    Ok(())
}
