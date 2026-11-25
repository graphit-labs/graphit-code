//! Native peers exercise the real Run owner and Role HTTP authority.
use super::*;
use serde_json::{Value, json};
use std::{ffi::OsString, io::Read, path::PathBuf};

const PROMPT: &str = "Read the actual source";
const ANSWER: &str = "actual executable Role payload";
const MODEL: &str = "provider/literal;$(model)";

struct Fixture {
    temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: PathBuf,
    path: OsString,
    daemon: Daemon,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.daemon.0.try_wait().ok().flatten().is_some() {
            return;
        }
        // Failure containment lets the daemon join an in-flight native child.
        // The normal path still requires stop() and its explicit assertions.
        let _ = Command::new("kill")
            .args(["-TERM", &self.daemon.0.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until {
            if self.daemon.0.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
impl Fixture {
    async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let installation = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let project = temp.path().join("project");
        fs::create_dir(&project)?;
        fs::write(project.join("raw"), ANSWER)?;
        // Cursor's Tracker uses the actual Git tool surface, not Scout grants.
        for args in [vec!["init", "-q", "-b", "main"], vec!["add", "tracked"]] {
            if args[0] == "add" {
                fs::write(project.join("tracked"), "alpha\n")?;
            }
            let output = Command::new("git")
                .args(args)
                .current_dir(&project)
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .env("HOME", temp.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other("temporary Git setup failed").into());
            }
        }
        fs::write(project.join("tracked"), "bravo\n")?;
        let paths = ProjectPaths::at(&project, &installation)?;
        let cancel = Cancellation::default();
        ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("native-role-bindings")?)?;
        ConfigStore::global(&installation)?.set(
            ConfigKey::new("agent")?,
            json!(NativeBinding::KiroV3.as_str()),
            Duration::from_secs(5),
            &cancel,
        )?;
        let bin = temp.path().join("bin");
        fs::create_dir(&bin)?;
        native_peer(&bin)?;
        symlink(env!("CARGO_BIN_EXE_graphit"), bin.join(brand.binary_name()))?;
        let mut search = vec![bin];
        search.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(search)?;
        let mut daemon = Daemon(
            command(&project, &installation, &path)
                .env("GRAPHIT_ROLE_BINDING", "inherited-invalid-binding")
                .args(["daemon", "foreground", "--project-version", "main"])
                .stdout(Stdio::null())
                .stderr(fs::File::create(temp.path().join("daemon.log"))?)
                .spawn()?,
        );
        let until = Instant::now() + Duration::from_secs(30);
        while !installation.root().join("daemon/mcp.json").exists() {
            if daemon.0.try_wait()?.is_some() {
                return Err(io::Error::other("native role daemon exited during startup").into());
            }
            if Instant::now() >= until {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "native role daemon discovery missing",
                )
                .into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(Self {
            temp,
            installation,
            project,
            path,
            daemon,
        })
    }
    async fn run(
        &self,
        binding: NativeBinding,
        role: &str,
        model: Option<&str>,
    ) -> Result<RunOutcome, Box<dyn std::error::Error>> {
        for name in ["proof.json", "phase.json"] {
            if let Err(error) = fs::remove_file(self.temp.path().join("bin").join(name))
                && error.kind() != io::ErrorKind::NotFound
            {
                return Err(error.into());
            }
        }
        let mut case = json!({
            "binding": binding.as_str(), "role":role, "model":model,
            "agent_directory": Path::new(binding.adapter().layout().root)
                .join(binding.adapter().layout().agents),
            "mcp_server": self.installation.brand().key(),
        });
        if binding == NativeBinding::OpenCode {
            let plan = graphit_hub::native::mcp_config::NativeServerSpec::graphit(
                self.installation.brand().key(),
            )
            .plan(binding)?;
            let relative = plan.project_config_path().ok_or_else(|| {
                io::Error::other("OpenCode PROJECT MCP configuration path missing")
            })?;
            let config = self.project.join(relative);
            assert!(
                config.is_file(),
                "OpenCode requires its installed native MCP server"
            );
            case["mcp_config"] = json!(config);
            case["mcp_entry"] = json!(plan.entry());
            case["graphit_executable"] = json!(env!("CARGO_BIN_EXE_graphit"));
            case["graphit_program"] = json!(self.installation.brand().binary_name());
        }
        fs::write(
            self.temp.path().join("bin/case.json"),
            serde_json::to_vec(&case)?,
        )?;
        let mut command = command(&self.project, &self.installation, &self.path);
        command.args([
            "roles",
            "run",
            "--agent",
            binding.as_str(),
            "--role",
            role,
            "--prompt",
            PROMPT,
        ]);
        if let Some(model) = model {
            command.args(["--model", model]);
        }
        let output = tokio::task::spawn_blocking(move || command.output()).await??;
        let decoded = serde_json::from_slice::<RunOutcome>(&output.stdout);
        // Never print raw native stderr, argv, environment or credentials on failure.
        assert!(
            output.status.success(),
            "Run {} failed: status={}; outcome={:?}; peer={:?}",
            binding.as_str(),
            output.status,
            decoded
                .as_ref()
                .ok()
                .map(|result| (&result.failure, result.run_closed)),
            fs::read_to_string(self.temp.path().join("bin/phase.json")).ok()
        );
        assert!(output.stderr.is_empty());
        let result = decoded?;
        assert!(
            result.failure.is_none(),
            "public Run failure: {:?}",
            result.failure
        );
        assert!(result.run_closed);
        let proof: Value =
            serde_json::from_slice(&fs::read(self.temp.path().join("bin/proof.json"))?)?;
        assert_eq!(proof["binding"], binding.as_str());
        assert_eq!(proof["role_mcp"], true);
        assert_eq!(proof["tampered_binding_denied"], true);
        assert_eq!(proof["model"], json!(model));
        if binding == NativeBinding::OpenCode {
            assert_eq!(proof["configured_mcp"], true);
        }
        Ok(result)
    }
    async fn install_opencode_coordinator(&self) -> Result<(), Box<dyn std::error::Error>> {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/agent-instructions/coordinator");
        let mut command = command(&self.project, &self.installation, &self.path);
        command
            .arg("--global-dir")
            .arg(self.installation.root())
            .args([
                "--project-version",
                "main",
                "coordinator",
                "install",
                "--binding",
                "opencode",
                "--source-dir",
            ])
            .arg(source);
        let output = tokio::task::spawn_blocking(move || command.output()).await??;
        let decoded = serde_json::from_slice::<
            graphit_app::project_coordinator_install::CoordinatorOutcome,
        >(&output.stdout);
        assert!(
            output.status.success(),
            "real OpenCode coordinator installation failed: status={}; public failure present={:?}",
            output.status,
            decoded.as_ref().ok().map(|result| result.failure.is_some())
        );
        assert!(output.stderr.is_empty());
        let result = decoded?;
        assert!(result.failure.is_none());
        assert!(result.mcp.is_some());
        Ok(())
    }
    async fn stop(&mut self) -> io::Result<()> {
        if let Err(error) = self.daemon.stop().await {
            let log = fs::read_to_string(self.temp.path().join("daemon.log")).unwrap_or_default();
            let markers: Vec<_> = [
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
            ]
            .into_iter()
            .filter(|marker| log.contains(marker))
            .collect();
            let diagnostic = json!({
                "stage":"fixture-daemon-shutdown",
                "exit":self.daemon.0.try_wait()?.and_then(|status| status.code()),
                "kind":format!("{:?}",error.kind()),
                "discovery_exists":self.installation.root().join("daemon/mcp.json").exists(),
                "key_exists":self.installation.root().join("daemon/mcp.key").exists(),
                "markers":markers,
            });
            return Err(io::Error::new(error.kind(), diagnostic.to_string()));
        }
        assert!(!self.installation.root().join("daemon/mcp.json").exists());
        assert!(!self.installation.root().join("daemon/mcp.key").exists());
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_native_json_roles_select_binding_model_and_real_role_mcp()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::start().await?;
    // One native journey per recipe; existing adapter suites own wire permutations.
    for (binding, role, model) in [
        (NativeBinding::Codex, "scout", Some(MODEL)),
        (NativeBinding::Claude, "scout", None),
        (NativeBinding::Cursor, "tracker", Some(MODEL)),
        (NativeBinding::Qwen, "scout", None),
        (NativeBinding::Antigravity, "scout", Some(MODEL)),
    ] {
        let result = fixture.run(binding, role, model).await?;
        assert_eq!(result.answer.as_deref(), Some(ANSWER));
    }
    fixture.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_role_native_eof_reports_failure_closes_run_and_allows_next_run()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::start().await?;
    fs::write(
        fixture.temp.path().join("bin/case.json"),
        serde_json::to_vec(&json!({
            "binding":NativeBinding::Codex.as_str(), "role":"scout", "model":MODEL, "eof":true,
        }))?,
    )?;
    let mut command = command(&fixture.project, &fixture.installation, &fixture.path);
    command.args([
        "roles",
        "run",
        "--agent",
        NativeBinding::Codex.as_str(),
        "--role",
        "scout",
        "--model",
        MODEL,
        "--prompt",
        PROMPT,
    ]);
    let output = tokio::task::spawn_blocking(move || command.output()).await??;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let result: RunOutcome = serde_json::from_slice(&output.stdout)?;
    assert!(result.failure.is_some());
    assert!(result.run_closed);
    assert!(result.answer.is_none());
    let proof: Value =
        serde_json::from_slice(&fs::read(fixture.temp.path().join("bin/proof.json"))?)?;
    assert_eq!(
        proof["role_mcp"], true,
        "EOF must follow real Role admission and source read"
    );
    let next = fixture
        .run(NativeBinding::Codex, "scout", Some(MODEL))
        .await?;
    assert_eq!(next.answer.as_deref(), Some(ANSWER));
    fixture.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_role_cancel_joins_native_child_and_preserves_independent_runs()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::start().await?;
    fs::write(
        fixture.temp.path().join("bin/case.json"),
        serde_json::to_vec(&json!({
            "binding":NativeBinding::Codex.as_str(), "role":"scout", "model":MODEL, "hold":true,
        }))?,
    )?;
    let mut held = Daemon(
        command(&fixture.project, &fixture.installation, &fixture.path)
            .args([
                "roles",
                "run",
                "--agent",
                NativeBinding::Codex.as_str(),
                "--role",
                "scout",
                "--model",
                MODEL,
                "--prompt",
                PROMPT,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let marker = fixture.temp.path().join("bin/holding.pid");
    let until = Instant::now() + Duration::from_secs(30);
    while !marker.exists() {
        assert!(
            held.0.try_wait()?.is_none(),
            "held Role exited before native prompt/MCP"
        );
        if Instant::now() >= until {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "native prompt/MCP marker missing",
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let pid = fs::read_to_string(marker)?;
    assert!(
        Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()?
            .success()
    );
    let sibling = fixture.run(NativeBinding::Claude, "scout", None).await?;
    assert_eq!(sibling.answer.as_deref(), Some(ANSWER));
    if let Some(status) = held.0.try_wait()? {
        return Err(io::Error::other(format!(
            "held Role client ended before cancellation: exit={:?}; outcome={}",
            status.code(),
            closed_client_outcome(&mut held.0)?
        ))
        .into());
    }
    assert!(
        Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()?
            .success(),
        "held native child disappeared before client cancellation"
    );
    if let Err(error) = held.stop().await {
        return Err(io::Error::new(
            error.kind(),
            format!(
                "Role client cancellation failed: exit={:?}; outcome={}",
                held.0.try_wait()?.and_then(|status| status.code()),
                closed_client_outcome(&mut held.0)?
            ),
        )
        .into());
    }
    // Client cancellation closes HTTP; native cleanup finishes in the daemon owner.
    let until = Instant::now() + Duration::from_secs(20);
    while Command::new("kill")
        .args(["-0", &pid])
        .stderr(Stdio::null())
        .status()?
        .success()
    {
        if Instant::now() >= until {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "native Role child remained after client cancellation",
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(fixture.daemon.0.try_wait()?.is_none());
    assert!(fixture.installation.root().join("daemon/mcp.key").exists());
    // A fresh genuine run still works after cancellation of the independent owner.
    let subsequent = fixture.run(NativeBinding::Claude, "scout", None).await?;
    assert_eq!(subsequent.answer.as_deref(), Some(ANSWER));
    fixture.stop().await?;
    Ok(())
}

fn closed_client_outcome(child: &mut Child) -> io::Result<Value> {
    if child.try_wait()?.is_none() {
        return Ok(json!({"closed":false}));
    }
    let mut bytes = Vec::new();
    if let Some(mut output) = child.stdout.take() {
        output.read_to_end(&mut bytes)?;
    }
    Ok(match serde_json::from_slice::<RunOutcome>(&bytes) {
        Ok(result) => json!({
            "closed":true, "decoded":true, "failure_present":result.failure.is_some(),
            "run_closed":result.run_closed, "answer_present":result.answer.is_some(),
        }),
        Err(_) => json!({"closed":true,"decoded":false}),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_opencode_role_acknowledges_published_mode_before_prompt()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::start().await?;
    fixture.install_opencode_coordinator().await?;
    let result = fixture
        .run(NativeBinding::OpenCode, "scout", Some(MODEL))
        .await?;
    assert_eq!(result.answer.as_deref(), Some(ANSWER));
    fixture.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_kimi_print_role_selects_published_agent_and_joins_after_eof()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::start().await?;
    for model in [None, Some(MODEL)] {
        let result = fixture.run(NativeBinding::Kimi, "scout", model).await?;
        assert_eq!(result.answer.as_deref(), Some(ANSWER));
        let pid = fs::read_to_string(fixture.temp.path().join("bin/native.pid"))?;
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()?
                .success(),
            "Kimi Run returned before the print process was joined"
        );
    }
    fixture.stop().await?;
    Ok(())
}

fn native_peer(bin: &Path) -> io::Result<()> {
    runtime_peer::install(
        bin,
        None,
        &[
            "codex",
            "claude",
            "cursor-agent",
            "qwen",
            "agy",
            "opencode",
            "kimi",
        ],
    )
}
