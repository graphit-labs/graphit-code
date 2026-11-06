//! Actual compiled CLI, Role factory/issuer and PROJECT Memory over controlled ACP.
//! This does not qualify installed Kiro or model perception.
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
use serde_json::json;
use std::{
    fs, io,
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
    async fn stop(&mut self) -> io::Result<i32> {
        let sent = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .status()?;
        if !sent.success() {
            return Err(io::Error::other("temporary daemon stop failed"));
        }
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.0.try_wait()? {
                return status.code().ok_or_else(|| {
                    io::Error::other("temporary daemon ended without an exit code")
                });
            }
            if Instant::now() >= until {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
fn command(
    project: &Path,
    installation: &InstallationPaths,
    path: &std::ffi::OsStr,
    home: &Path,
) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_graphit"));
    cmd.current_dir(project)
        .env_clear()
        .env("PATH", path)
        .env("HOME", home)
        .arg("--global-dir")
        .arg(installation.root())
        .arg("--project-version")
        .arg("main");
    cmd
}
fn role_peer(bin: &Path, brand: &Brand) -> io::Result<()> {
    runtime_peer::install(
        bin,
        Some(serde_json::json!({
            "fixture":"role-hooks", "brand":brand.key(), "program":brand.binary_name(),
            "graphit":env!("CARGO_BIN_EXE_graphit"), "inspector":env!("CARGO_BIN_EXE_kiro-knowledge-fixture"),
            "journal":bin.parent().ok_or(io::ErrorKind::InvalidInput)?.join("peer-stage.json")
        })),
        &["kiro-cli"],
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hook_v3_compiled_profile_commands_real_factory_memory_and_role_transport()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let installation = InstallationPaths::resolve(
        brand.clone(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let root = temp.path().join("project 'quoted' $literal α");
    let home = temp.path().join("home");
    fs::create_dir(&root)?;
    fs::create_dir(&home)?;
    let project = ProjectPaths::at(&root, &installation)?;
    let cancel = Cancellation::default();
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancel)?
        .ensure(&ProjectMetadata::new("role-hook-qualification")?)?;
    ConfigStore::global(&installation)?.set(
        ConfigKey::new("agent")?,
        json!(graphit_hub::NativeBinding::KiroV3.as_str()),
        Duration::from_secs(5),
        &cancel,
    )?;
    let bin = temp.path().join("bin");
    fs::create_dir(&bin)?;
    role_peer(&bin, &brand)?;
    let mut search = vec![bin];
    search.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(search)?;
    let daemon_log = temp.path().join("daemon.log");
    let mut daemon = Daemon(
        command(&root, &installation, &path, &home)
            .args(["daemon", "foreground"])
            .stdout(Stdio::null())
            .stderr(fs::File::create(&daemon_log)?)
            .spawn()?,
    );
    let until = Instant::now() + Duration::from_secs(30);
    while !installation.root().join("daemon/mcp.json").exists() {
        if daemon.0.try_wait()?.is_some() {
            return Err(io::Error::other("temporary foreground ended before discovery").into());
        }
        if Instant::now() >= until {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let discovery: graphit_daemon::source_discovery::DiscoveryRecord =
        serde_json::from_slice(&fs::read(installation.root().join("daemon/mcp.json"))?)?;
    assert!(discovery.address().ip().is_loopback());
    let readiness = reqwest::Client::new()
        .get(format!("http://{}/admin/roles/run", discovery.address()))
        .timeout(Duration::from_secs(30))
        .send()
        .await;
    assert!(
        matches!(&readiness, Ok(response) if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED),
        "real Source HTTP Role route was not serving after discovery: {:?}",
        readiness
            .map(|response| response.status())
            .map_err(|error| error.without_url()),
    );
    for role in ["scribe", "scout", "tracker"] {
        let mut run = command(&root, &installation, &path, &home);
        run.args([
            "roles",
            "run",
            "--role",
            role,
            "--prompt",
            "Verify both published V3 hook commands",
        ]);
        let output = tokio::task::spawn_blocking(move || run.output()).await??;
        let diagnostic = String::from_utf8_lossy(&output.stderr)
            .replace(temp.path().to_str().unwrap(), "<fixture>");
        let stage = fs::read_to_string(temp.path().join("peer-stage.json"))
            .unwrap_or_else(|_| "peer not started".into());
        let profile_published = root
            .join(".kiro/agents")
            .join(format!("{}-{}.md", brand.key(), role))
            .exists();
        let daemon_diagnostic = if output.status.success() {
            String::new()
        } else {
            let mut text = fs::read_to_string(&daemon_log)?;
            if let Ok(key) = fs::read_to_string(installation.root().join("daemon/mcp.key")) {
                text = text.replace(&key, "<credential>");
            }
            text.replace(temp.path().to_str().unwrap(), "<fixture>")
        };
        assert!(
            output.status.success(),
            "Role CLI status {:?}, diagnostic: {}, stage: {}, profile_published: {}, daemon: {}",
            output.status.code(),
            diagnostic,
            stage,
            profile_published,
            daemon_diagnostic,
        );
        let outcome: RunOutcome = serde_json::from_slice(&output.stdout)?;
        assert!(
            output.status.success(),
            "Role journey failed: {:?}",
            outcome.failure
        );
        assert!(outcome.run_closed);
        assert_eq!(outcome.answer.as_deref(), Some("Role hooks verified"));
    }
    assert_eq!(daemon.stop().await?, 130);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hook_v3_run_worker_eof_stop_and_deadline_while_core_writer_is_held()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_core::{
        artifact_store::{ArtifactStore, CapturedPublicationScope, ScopeLimits},
        config::ConfigSnapshot,
    };
    use graphit_hub::policy::{HubResourcePolicy, ResourceSetting};
    for mode in ["eof", "stop", "deadline"] {
        let temp = tempfile::tempdir()?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let installation = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let root = temp.path().join("project");
        let home = temp.path().join("home");
        let bin = temp.path().join("bin");
        fs::create_dir(&root)?;
        fs::create_dir(&home)?;
        fs::create_dir(&bin)?;
        let project = ProjectPaths::at(&root, &installation)?;
        let cancel = Cancellation::default();
        let id = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("run-owner-lock-qualification")?)?
            .id();
        let global = ConfigStore::global(&installation)?;
        global.set(
            ConfigKey::new("agent")?,
            json!(graphit_hub::NativeBinding::KiroV3.as_str()),
            Duration::from_secs(5),
            &cancel,
        )?;
        // This scenario exercises Role publication and request custody only.
        // Unrelated indexing jobs have separate producer qualification.
        for module in ["modules.ast", "modules.knowledge"] {
            global.set(
                ConfigKey::new(module)?,
                json!(false),
                Duration::from_secs(5),
                &cancel,
            )?;
        }
        global.set(
            ConfigKey::new(ResourceSetting::RequestDeadlineMs.spec().key)?,
            json!(5000),
            Duration::from_secs(5),
            &cancel,
        )?;
        role_peer(&bin, &brand)?;
        let mut search = vec![bin];
        search.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(search)?;
        let lifecycle_log = temp.path().join("lifecycle.log");
        let mut daemon = Daemon(
            command(&root, &installation, &path, &home)
                .args(["daemon", "foreground"])
                .stdout(Stdio::null())
                .stderr(fs::File::create(&lifecycle_log)?)
                .spawn()?,
        );
        let until = Instant::now() + Duration::from_secs(30);
        while !installation.root().join("daemon/mcp.json").exists() {
            if daemon.0.try_wait()?.is_some() || Instant::now() >= until {
                return Err(io::Error::other("temporary worker fixture did not start").into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let discovery: graphit_daemon::source_discovery::DiscoveryRecord =
            serde_json::from_slice(&fs::read(installation.root().join("daemon/mcp.json"))?)?;
        assert!(discovery.address().ip().is_loopback());
        let client = reqwest::Client::builder().no_proxy().build()?;
        let url = format!("http://{}/admin/roles/run", discovery.address());
        let ready = client
            .get(&url)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        assert_eq!(ready.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);
        let hub = HubResourcePolicy::resolve(&ConfigSnapshot::new(&brand, [])?)?;
        let holder_cancel = Cancellation::default();
        let holder = hub.request_context(Instant::now(), &holder_cancel)?;
        let depth = usize::try_from(hub.limit(ResourceSetting::SnapshotDepth).value)?;
        // This private fixture contains one PROJECT and no trusted peer roots.
        let scope = CapturedPublicationScope::project(
            &project,
            id,
            ScopeLimits::new(1, graphit_core::native_path::MAX_PATH_BYTES, depth)?,
            holder.snapshot(),
        )?;
        let core = ArtifactStore::new(scope, hub.artifacts());
        let held_writer = core.writer_with_context(holder.snapshot())?;
        let key = zeroize::Zeroizing::new(fs::read(installation.root().join("daemon/mcp.key"))?);
        let response = client
            .post(&url)
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(std::str::from_utf8(&key)?)
            .json(&graphit_app::managed_role_run::RunRequest {
                install: graphit_app::managed_roles::InstallRequest {
                    destination: graphit_app::managed_roles::InstallDestination::Project,
                    project_dir: Some(root.clone()),
                    project_version: Some("main".into()),
                    binding: None,
                    roles: vec![graphit_app::managed_roles::ManagedRoleName::Scout],
                    skills_only: false,
                    rules: Vec::new(),
                    skill_aliases: std::collections::BTreeMap::new(),
                    skill_bundles: Default::default(),
                    module_skill: None,
                },
                role: graphit_app::managed_roles::ManagedRoleName::Scout,
                model: None,
                prompt: "Verify retained worker cancellation".into(),
            })
            .timeout(Duration::from_secs(12))
            .send()
            .await?;
        assert!(response.status().is_success());
        assert!(!temp.path().join("peer-stage.json").exists());
        assert!(
            !root
                .join(".kiro/agents")
                .join(format!("{}-scout.md", brand.key()))
                .exists()
        );
        if mode == "deadline" {
            let started = Instant::now();
            let outcome: RunOutcome =
                tokio::time::timeout(Duration::from_secs(10), response.json()).await??;
            assert!(outcome.failure.is_some());
            assert!(!outcome.run_closed && outcome.answer.is_none());
            assert!(started.elapsed() >= Duration::from_secs(3));
            assert!(started.elapsed() < Duration::from_secs(8));
            assert!(!temp.path().join("peer-stage.json").exists());
            // The native failure was delivered as a complete outcome. It is not
            // an undelivered HTTP job failure and does not poison service shutdown.
            let exit = daemon.stop().await?;
            let diagnostic = fs::read_to_string(&lifecycle_log)?
                .replace(std::str::from_utf8(&key)?, "<credential>")
                .replace(temp.path().to_str().unwrap(), "<fixture>");
            assert_eq!(exit, 130, "deadline daemon: {diagnostic}");
        } else if mode == "eof" {
            drop(response);
            let ready = client
                .get(&url)
                .timeout(Duration::from_secs(15))
                .send()
                .await?;
            assert_eq!(ready.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);
            assert!(installation.root().join("daemon/mcp.key").exists());
            let exit = daemon.stop().await?;
            let diagnostic = fs::read_to_string(&lifecycle_log)?
                .replace(std::str::from_utf8(&key)?, "<credential>")
                .replace(temp.path().to_str().unwrap(), "<fixture>");
            assert_eq!(exit, 130, "EOF daemon: {diagnostic}");
        } else {
            let stop = daemon.stop();
            let body = async {
                let mut response = response;
                while let Some(chunk) = response.chunk().await? {
                    assert!(chunk.is_empty(), "stopped Role returned a partial payload");
                }
                Ok::<_, reqwest::Error>(())
            };
            let (exit, outcome) = tokio::join!(stop, body);
            let diagnostic = fs::read_to_string(&lifecycle_log)?
                .replace(std::str::from_utf8(&key)?, "<credential>")
                .replace(temp.path().to_str().unwrap(), "<fixture>");
            assert_eq!(exit?, 130, "stop daemon: {diagnostic}");
            assert!(
                diagnostic.contains("selected_branch=service_stop error_kind=None"),
                "stop daemon: {diagnostic}"
            );
            // Service stop closes the recipient before this worker can deliver.
            // No payload may arrive before either empty completion or the actual
            // truncated-body I/O error from that closed chunked response.
            if let Err(error) = outcome {
                let mut cause = std::error::Error::source(&error);
                loop {
                    let current = cause.unwrap_or_else(|| {
                        panic!("stop body has no underlying I/O cause: {error:?}")
                    });
                    if let Some(cause) = current.downcast_ref::<io::Error>() {
                        assert_eq!(cause.kind(), io::ErrorKind::UnexpectedEof);
                        break;
                    }
                    cause = current.source();
                }
            }
        }
        // BeforeCommit remains observable for every cancellation mode while the
        // actual SDK writer is still held through worker and daemon join.
        assert!(!temp.path().join("peer-stage.json").exists());
        assert!(
            !root
                .join(".kiro/agents")
                .join(format!("{}-scout.md", brand.key()))
                .exists()
        );
        assert!(holder.snapshot().check().is_ok());
        drop(held_writer);
        assert!(!installation.root().join("daemon/mcp.key").exists());
        assert!(!installation.root().join("daemon/mcp.json").exists());
    }
    Ok(())
}
