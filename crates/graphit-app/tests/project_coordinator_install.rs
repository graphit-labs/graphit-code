//! Real Auth/Core/Hub publication and compiled CLI journeys. ACP peers test transport,
//! not installed Kiro model perception.
#![cfg(unix)]
use graphit_app::{
    managed_roles::InstallResources,
    project_coordinator_install::{self, CoordinatorOutcome, CoordinatorRequest},
    project_hook_install,
    source_server::SourcePolicy,
};
use graphit_auth::{
    AdminSessionPolicy, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits, Permission,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy,
        SourceReadLimits, StoreOwner,
    },
    auth_document::AuthDocumentLimits,
    brand::Brand,
    config::ConfigSnapshot,
    file_lock::Cancellation,
    installation_bootstrap::BootstrapLimits,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    version_context::{Checkout, VersionContext},
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
use graphit_hub::{
    NativeBinding, native::ContentLimits, policy::RequestContext, registry::HubRegistry,
};
use graphit_records::memory_store::MemoryStoreReadLimits;
use serde_json::{Value, json};
use std::{
    fs, io,
    num::{NonZeroU32, NonZeroUsize},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn assert_portable_artifact(text: &str, project: &Path, installation: &InstallationPaths) {
    let test_executable = std::env::current_exe().unwrap();
    for local in [
        project,
        installation.root(),
        Path::new(env!("CARGO_BIN_EXE_graphit")),
        test_executable.as_path(),
    ] {
        assert!(
            !text.contains(local.to_str().unwrap()),
            "local path in artifact"
        );
    }
}

fn content_limits() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 1048576,
        max_output_bytes: 1048576,
        max_depth: 64,
        max_nodes: 65536,
        max_scalar_bytes: 65536,
        max_collection_entries: 65536,
    }
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        content_limits(),
        cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    auth: DaemonAuth,
    policy: SourcePolicy,
    projects: Vec<(ProjectPaths, ProjectId, VersionContext)>,
    raw: String,
    memory: MemoryStoreReadLimits,
}
impl Fixture {
    fn new() -> Self {
        Self::at(None, Brand::default(), true)
    }
    fn at(existing: Option<&Path>, brand: Brand, publish: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            brand,
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let root = existing
            .map(Path::to_owned)
            .unwrap_or_else(|| temp.path().join("project"));
        fs::create_dir_all(&root).unwrap();
        let peer = temp.path().join("peer-project");
        fs::create_dir(&peer).unwrap();
        let projects = [
            ProjectPaths::at(&root, &paths).unwrap(),
            ProjectPaths::at(&peer, &paths).unwrap(),
        ];
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let scope = ScopeLimits::new(16, 65536, 256).unwrap();
        let mut permissions = vec![Permission::Configure];
        if publish {
            permissions.push(Permission::External(ExternalEffect::PublishedFilesystem));
        }
        let source = || SourceAuthorityPolicy {
            ordinary: &permissions,
            administrative: &permissions,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
            },
        };
        let authority = IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 4096,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        let auth = DaemonAuth::start(
            &paths,
            &projects,
            StartupPolicy {
                bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
                runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                scope,
                document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
                profiles: ProfileLimits {
                    bytes: 65536,
                    nodes: 4096,
                    scalar_bytes: 4096,
                    collection_entries: 64,
                    providers: 16,
                    profiles: 16,
                },
                authority,
                ttl: TtlPolicy {
                    ttl: Duration::from_secs(900),
                    maximum: Duration::from_secs(3600),
                    refresh_margin: Duration::from_secs(120),
                },
            },
            &ctx,
        )
        .ok()
        .unwrap();
        let key = auth.runtime().check(ctx.snapshot()).unwrap();
        let raw = String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap();
        drop(key);
        let projects = projects
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let id = ProjectIdentityStore::at(&p)
                    .writer(Duration::from_secs(5), &cancel)
                    .unwrap()
                    .ensure(&ProjectMetadata::new(&format!("project-{i}")).unwrap())
                    .unwrap()
                    .id();
                let version =
                    VersionContext::select(&p, Checkout::NonGit, "main", None, "").unwrap();
                (p, id, version)
            })
            .collect();
        let config = ConfigSnapshot::new(paths.brand(), []).unwrap();
        let native_install =
            graphit_app::native_install_http::NativeInstallPolicy::capture(&config, ctx.snapshot())
                .unwrap();
        let hub = &native_install.hub;
        let copy = CopyLimits::new(1048576, 128, 65536, 16).unwrap();
        let retained = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
        let index = NativeDocumentFileLimits::new(1048576, 65536, 256, 8388608).unwrap();
        let StoreOwner::Installation { installation_id } = auth.installation().live.owner() else {
            unreachable!()
        };
        let policy = SourcePolicy {
            graph_query: graphit_ast::icebug_query::QueryOptions {
                buffer_pool_bytes: 134217728,
                query_threads: 2,
                max_rows: hub.content().max_collection_entries,
                max_bytes: hub.content().max_output_bytes,
                max_depth: hub.content().max_depth,
            },
            installation: paths.clone(),
            content: hub.content(),
            timeout: native_install.timeout,
            scope,
            file: SnapshotPolicy::new(
                CopyPolicy::RejectFileLinks,
                copy,
                RetainedFileSet::new([], retained).unwrap(),
                retained,
            )
            .unwrap(),
            read: SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
            path: retained,
            index,
            binding: NativeBinding::KiroV3,
            registry: HubRegistry::global(&paths, installation_id, hub.hub()).unwrap(),
            native_install,
        };
        Self {
            _temp: temp,
            auth,
            policy,
            projects,
            raw,
            memory: MemoryStoreReadLimits {
                max_query_bytes: NonZeroUsize::new(4194304).unwrap(),
                batch_rows: NonZeroU32::new(16).unwrap(),
                native: index,
            },
        }
    }
    fn input(&self, selected: usize) -> CoordinatorRequest {
        CoordinatorRequest {
            operation: project_hook_install::HookOperation::Install,
            binding: NativeBinding::KiroV3,
            kimi_config_dir: None,
            source_dir: Some(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../templates/agent-instructions/coordinator"),
            ),
            project_dir: self.projects[selected].0.root().into(),
            project_version: Some("main".into()),
        }
    }
    fn install(
        &self,
        input: &CoordinatorRequest,
        raw: &str,
        ctx: &RequestContext<'_>,
    ) -> project_coordinator_install::CoordinatorExecution {
        project_coordinator_install::execute(
            InstallResources {
                auth: &self.auth,
                global_paths: None,
                projects: &self.projects,
                current: 0,
                policy: &self.policy,
                modules: &[],
                memory_read: self.memory,
            },
            raw,
            input,
            ctx,
        )
    }
}
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
        .env(
            installation
                .brand()
                .environment_variable("GLOBAL_DIR")
                .unwrap(),
            installation.root(),
        )
        .arg("--project-version")
        .arg("main");
    cmd
}
fn acp_fixture(bin: &Path, brand: &Brand, mandatory_body: &str) -> io::Result<()> {
    let prefix = format!(
        "BRAND={}\nMANDATORY_BODY={}\nMCP_ENV_VARS={}\n",
        serde_json::to_string(brand.key())?,
        serde_json::to_string(mandatory_body)?,
        serde_json::to_string(&project_hook_install::client::mcp_environment_variables(
            brand,
            NativeBinding::Codex
        )?)?
    );
    let body = r#"import json,os,sys,subprocess
role=json.loads(os.environ["GRAPHIT_ROLE_REFERENCE"])["role"]
cwd=None
def send(message): print(json.dumps(message),flush=True)
def response(child,ident):
    for line in child.stdout:
        message=json.loads(line)
        if message.get("id")==ident:return message
    raise RuntimeError("Role stdio closed")
def request(child,method,params,ident):
    message={"jsonrpc":"2.0","method":method,"params":params}
    if ident is not None:message["id"]=ident
    child.stdin.write(json.dumps(message)+"\n");child.stdin.flush()
    if ident is not None:return response(child,ident)
for line in sys.stdin:
    message=json.loads(line);method=message.get("method");ident=message.get("id");params=message.get("params",{})
    if method=="initialize":
        send({"jsonrpc":"2.0","id":ident,"result":{"protocolVersion":1,"agentCapabilities":{}}})
    elif method=="session/new":
        cwd=params["cwd"]
        assert sys.argv[1:]==["acp","--agent-engine=v3"]
        assert params["_meta"]["kiro"]["modeId"]==BRAND+"-"+role
        send({"jsonrpc":"2.0","id":ident,"result":{"sessionId":"hook-boundary"}})
    elif method=="session/prompt":
        assert os.environ.get("GRAPHIT_ROLE_BEARER") and os.environ.get("GRAPHIT_ROLE_REFERENCE") and os.environ.get("GRAPHIT_ROLE_SELECTION")
        if role=="scribe":
            with open(os.path.join(cwd,".kiro","settings","mcp.json")) as document:
                server=json.load(document)["mcpServers"][BRAND]
            assert server["command"]==BRAND
            assert not any(arg in ["--project-dir","--project-version","--role"] for arg in server["args"])
            with subprocess.Popen([server["command"],*server["args"]],cwd=cwd,env={key:value for key,value in os.environ.items() if key in ["PATH","HOME",*MCP_ENV_VARS]},stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) as child:
                assert "result" in request(child,"initialize",{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"hook-qualification","version":"1"}},1)
                request(child,"notifications/initialized",{},None)
                answer=request(child,"tools/call",{"name":BRAND+"_memory_insert","arguments":{"input":{"idempotency_key":"coordinator-native-mandatory-be6e","title":"PROJECT mandatory hook qualification","body":MANDATORY_BODY,"classification":"convention","mandatory":True,"important":False,"tags":[]}}},2)
                assert "result" in answer and not answer["result"].get("isError",False)
                child.stdin.close();assert child.wait(timeout=15)==0
        send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"hook-boundary","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Role hook boundary verified"}}}})
        send({"jsonrpc":"2.0","id":ident,"result":{"stopReason":"end_turn"}})
"#;
    let script = format!("#!/usr/bin/env python3\n{prefix}{body}");
    let file = bin.join("kiro-cli");
    fs::write(&file, script)?;
    fs::set_permissions(file, fs::Permissions::from_mode(0o700))
}

#[test]
fn coordinator_real_sdk_publication_two_projects_current_binding_repeat_and_denials() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    for selected in 0..2 {
        let root = fixture.projects[selected].0.root();

        let input = fixture.input(selected);
        let installed = fixture.install(&input, &fixture.raw, &ctx);
        assert!(installed.cause.is_none(), "{:?}", installed.cause);
        let profile = root.join(installed.outcome.profile.as_ref().unwrap());
        let bytes = fs::read(&profile).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert_portable_artifact(&text, root, &fixture.policy.installation);
        assert!(!text.contains(&fixture.raw));
        assert!(!text.contains("Standing context"));
        assert!(!text.contains("GRAPHIT_ROLE_BEARER"));
        assert!(!text.contains("MANDATORY-be6e"));
        assert!(!text.contains("allowedTools"));
        let mcp = root.join(".kiro/settings/mcp.json");
        let config_text = fs::read_to_string(mcp).unwrap();
        assert_portable_artifact(&config_text, root, &fixture.policy.installation);
        let config: Value = serde_json::from_str(&config_text).unwrap();
        let server = &config["mcpServers"][fixture.policy.installation.brand().key()];
        assert_eq!(
            server["command"],
            fixture.policy.installation.brand().binary_name()
        );
        assert!(
            !config
                .to_string()
                .contains(fixture.policy.installation.root().to_str().unwrap())
        );
        assert!(!config.to_string().contains(root.to_str().unwrap()));
        assert_eq!(server["args"], json!(["mcp", "--stdio"]));
        let header = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap()
            .0;
        let document: Value = serde_saphyr::from_str(header).unwrap();
        assert_eq!(document["permissions"]["rules"], json!([]));
        let hooks = document["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 4);
        assert_eq!(hooks[0]["trigger"], "SessionStart");
        assert_eq!(hooks[1]["trigger"], "UserPromptSubmit");

        let repeated = fixture.install(&input, &fixture.raw, &ctx);
        assert!(repeated.cause.is_none(), "{:?}", repeated.cause);
        assert_eq!(fs::read(profile).unwrap(), bytes);
    }
    let denied = fixture.install(&fixture.input(0), "invalid-runtime-credential", &ctx);
    assert_eq!(
        denied.cause.unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(denied.outcome.profile.is_none());
    let cancelled = Cancellation::default();
    let cancelled_request = request(&cancelled);
    cancelled.cancel();
    let stopped = fixture.install(&fixture.input(0), &fixture.raw, &cancelled_request);
    assert_eq!(stopped.cause.unwrap().kind(), io::ErrorKind::Interrupted);
    assert!(stopped.outcome.profile.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn coordinator_compiled_cli_publishes_root_mcp_and_delivers_current_project_context()
-> Result<(), Box<dyn std::error::Error>> {
    use graphit_app::managed_role_run::RunOutcome;
    use graphit_core::config::{ConfigKey, ConfigStore};
    {
        let binding = NativeBinding::KiroV3;
        let temp = tempfile::tempdir()?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let installation = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let root = temp.path().join("project with 'quotes' $meta α");
        fs::create_dir(&root)?;
        let home = temp.path().join("home");
        fs::create_dir(&home)?;
        let project = ProjectPaths::at(&root, &installation)?;
        let cancel = Cancellation::default();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("coordinator-qualification")?)?;
        let peer_root = temp.path().join("second maintained project");
        fs::create_dir(&peer_root)?;
        let peer_project = ProjectPaths::at(&peer_root, &installation)?;
        ProjectIdentityStore::at(&peer_project)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("coordinator-peer")?)?;
        fs::write(root.join("coordinator-cwd.txt"), "coordinator root CWD")?;
        fs::write(
            peer_root.join("coordinator-cwd.txt"),
            "coordinator peer CWD",
        )?;
        // Both physical projects are registered before daemon authority is captured.
        let original = request(&cancel);
        let private = graphit_core::installation_bootstrap::prepare(
            &installation,
            BootstrapLimits::new(65536, 256)?,
            original.snapshot(),
        )
        .into_installation()
        .map_err(io::Error::other)?;
        let inventory = graphit_core::project_inventory::GlobalProjectInventory::at(
            &installation,
            content_limits().max_input_bytes,
        )?;
        let mut writer = inventory.writer(private, original.snapshot())?;
        for selected in [&project, &peer_project] {
            let git = graphit_core::git_checkout::GitCheckout::inspect(
                selected.root(),
                Duration::from_secs(5),
                &cancel,
            )?;
            let version = VersionContext::select(selected, git.as_checkout(), "main", None, "")?;
            let captured = graphit_core::project_inventory::CapturedProjectInventory::capture(
                selected,
                &git,
                &version,
                ScopeLimits::new(16, 65536, 256)?,
                NativeDocumentFileLimits::new(1048576, 65536, 256, 8388608)?,
                original.snapshot(),
            )?;
            writer.register_checkout(&captured)?;
        }
        drop(writer);
        ConfigStore::global(&installation)?.set(
            ConfigKey::new("agent")?,
            json!(binding.as_str()),
            Duration::from_secs(5),
            &cancel,
        )?;
        let bin = temp.path().join("bin");
        fs::create_dir(&bin)?;
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_graphit"), bin.join(brand.binary_name()))?;
        acp_fixture(&bin, &brand, "MANDATORY-be6e literal {{ untrusted }}")?;
        let mut search = vec![bin];
        search.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(search)?;
        let log = temp.path().join("daemon.log");
        let mut daemon = Daemon(
            command(&root, &installation, &path, &home)
                .args(["daemon", "foreground"])
                .stdout(Stdio::null())
                .stderr(fs::File::create(&log)?)
                .spawn()?,
        );
        let until = Instant::now() + Duration::from_secs(30);
        while !installation.root().join("daemon/mcp.json").exists() {
            if daemon.0.try_wait()?.is_some() {
                return Err(io::Error::other(format!(
                    "temporary foreground failed: {}",
                    fs::read_to_string(&log)?
                ))
                .into());
            }
            if Instant::now() >= until {
                return Err(io::Error::from(io::ErrorKind::TimedOut).into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/agent-instructions/coordinator");
        let mut install = command(&root, &installation, &path, &home);
        install
            .args([
                "coordinator",
                "install",
                "--binding",
                "kiro-v3",
                "--source-dir",
            ])
            .arg(source);
        let result = tokio::task::spawn_blocking(move || install.output()).await??;
        let outcome: CoordinatorOutcome = serde_json::from_slice(&result.stdout)?;
        assert!(
            result.status.success(),
            "{:?}: {}",
            outcome.failure,
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        let profile = fs::read_to_string(root.join(outcome.profile.unwrap()))?;
        assert_portable_artifact(&profile, &root, &installation);
        assert!(!profile.contains("MANDATORY-be6e"));
        // Execute the same unpinned SDK-published argv from each maintained PROJECT.
        use rmcp::ServiceExt;
        let mcp_text = fs::read_to_string(root.join(".kiro/settings/mcp.json"))?;
        assert_portable_artifact(&mcp_text, &root, &installation);
        let mcp_document: Value = serde_json::from_str(&mcp_text)?;
        let server = &mcp_document["mcpServers"][brand.key()];
        assert_eq!(server["args"], json!(["mcp", "--stdio"]));
        for (selected_root, expected_source) in [
            (&root, "coordinator root CWD"),
            (&peer_root, "coordinator peer CWD"),
        ] {
            let mut root_command =
                tokio::process::Command::new(server["command"].as_str().unwrap());
            root_command.args(
                server["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|arg| arg.as_str().unwrap()),
            );
            root_command
                .kill_on_drop(true)
                .current_dir(selected_root)
                .env_clear()
                .env("PATH", &path)
                .env("HOME", &home)
                .env(
                    brand.environment_variable("GLOBAL_DIR")?,
                    installation.root(),
                )
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let mut root_mcp = root_command.spawn()?;
            let client = ()
                .serve((
                    root_mcp.stdout.take().unwrap(),
                    root_mcp.stdin.take().unwrap(),
                ))
                .await?;
            let catalog = client.peer().list_all_tools().await?;
            assert!(
                catalog
                    .iter()
                    .any(|tool| tool.name == format!("{}_ast_source", brand.key()))
            );
            let source = client
                .peer()
                .call_tool(
                    rmcp::model::CallToolRequestParams::new(format!("{}_ast_source", brand.key()))
                        .with_arguments(
                            json!({"path": "coordinator-cwd.txt"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                )
                .await?;
            assert_ne!(source.is_error, Some(true), "{source:?}");
            let actual: String = source
                .content
                .iter()
                .filter_map(|content| content.as_text())
                .map(|text| text.text.as_str())
                .collect();
            assert_eq!(actual, expected_source);
            client.cancel().await?;
            assert!(
                tokio::time::timeout(Duration::from_secs(15), root_mcp.wait())
                    .await??
                    .success()
            );
        }

        let mut run = command(&root, &installation, &path, &home);
        run.args([
            "roles",
            "run",
            "--role",
            "scribe",
            "--prompt",
            "Write current PROJECT mandatory through the real admitted MCP",
        ]);
        let result = tokio::task::spawn_blocking(move || run.output()).await??;
        let outcome: RunOutcome = serde_json::from_slice(&result.stdout)?;
        assert!(result.status.success(), "{:?}", outcome.failure);
        assert!(outcome.run_closed);
        let header = profile
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap()
            .0;
        let document: Value = serde_saphyr::from_str(header)?;
        let hooks: Vec<String> = document["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hook| hook["action"]["command"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(hooks.len(), 4);
        for hook in hooks {
            let mut shell = Command::new("/bin/sh");
            shell
                .args(["-c", &hook])
                .current_dir(&root)
                .env_clear()
                .env("PATH", &path)
                .env("HOME", &home)
                .env(
                    brand.environment_variable("GLOBAL_DIR")?,
                    installation.root(),
                );
            let result = tokio::task::spawn_blocking(move || shell.output()).await??;
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stderr.is_empty());
            let context = String::from_utf8(result.stdout)?;
            let args = shlex::split(&hook).unwrap();
            let context_event = args.windows(2).any(|args| {
                args[0] == "--event"
                    && matches!(args[1].as_str(), "SessionStart" | "UserPromptSubmit")
            });
            if context_event {
                assert!(context.contains("MANDATORY-be6e literal {{ untrusted }}"));
            } else {
                assert!(context.is_empty());
            }
        }
        // Controlled peers consume the actual published native documents and invoke
        // the compiled Graphit CLI against the same authorized Source.
        use graphit_app::session_hook_entry::{Engine, Event};

        let mut peer_install = command(&peer_root, &installation, &path, &home);
        peer_install
            .args([
                "coordinator",
                "install",
                "--binding",
                "kiro-v3",
                "--source-dir",
            ])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../templates/agent-instructions/coordinator"),
            );
        let result = tokio::task::spawn_blocking(move || peer_install.output()).await??;
        let peer_outcome: CoordinatorOutcome = serde_json::from_slice(&result.stdout)?;
        assert!(
            result.status.success(),
            "peer Coordinator status={:?}, failure={:?}",
            result.status.code(),
            peer_outcome.failure
        );
        assert!(peer_outcome.failure.is_none());
        for (engine, event, selected_root) in [
            (Engine::Codex, Event::SessionStart, &root),
            (Engine::Claude, Event::SessionStart, &root),
            (Engine::Cursor, Event::SessionStart, &root),
            (Engine::Qwen, Event::SessionStart, &root),
            (Engine::Kimi, Event::UserPromptSubmit, &root),
            (Engine::Kimi, Event::UserPromptSubmit, &peer_root),
            (Engine::Antigravity, Event::PreInvocation, &root),
        ] {
            let mut install = command(selected_root, &installation, &path, &home);
            install
                .args([
                    "coordinator",
                    "install",
                    "--binding",
                    engine.binding().as_str(),
                    "--source-dir",
                ])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../templates/agent-instructions/coordinator"),
                );
            let result = tokio::task::spawn_blocking(move || install.output()).await??;
            assert!(
                result.status.success(),
                "{engine:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let published: CoordinatorOutcome = serde_json::from_slice(&result.stdout)?;
            assert!(published.failure.is_none());
            assert!(published.mcp.is_some());
            if matches!(engine, Engine::Codex) {
                let config = fs::read_to_string(selected_root.join(".codex/config.toml"))?;
                let names = project_hook_install::client::mcp_environment_variables(
                    &brand,
                    NativeBinding::Codex,
                )?;
                assert_eq!(names.len(), 8);
                for name in names {
                    assert!(config.contains(&format!("\"{name}\"")));
                }
                assert_portable_artifact(&config, selected_root, &installation);
            }
            if matches!(engine, Engine::Antigravity) {
                assert!(published.profile.is_none());
            } else {
                assert_portable_artifact(
                    &fs::read_to_string(selected_root.join(published.profile.as_ref().unwrap()))?,
                    selected_root,
                    &installation,
                );
            }
            let invocation =
                published_hook_command(selected_root, &home, brand.key(), engine, event)?;
            let native_event = event.semantic().native_name(engine.binding())?;
            let input = match engine {
                Engine::Cursor => {
                    json!({"hook_event_name":native_event, "workspace_roots":[selected_root], "conversation_id":"native-only-not-a-graphit-session"})
                }
                Engine::Antigravity => {
                    json!({"workspacePaths":[selected_root], "conversationId":"native-only-not-a-graphit-session"})
                }
                _ => {
                    json!({"hook_event_name":native_event,"cwd":selected_root,"session_id":"native-only-not-a-graphit-session"})
                }
            };
            let input_path = temp.path().join("native-hook-input.json");
            fs::write(&input_path, serde_json::to_vec(&input)?)?;
            let mut shell = Command::new("/bin/sh");
            shell
                .args(["-c", &invocation])
                .current_dir(if matches!(engine, Engine::Kimi) {
                    &home
                } else {
                    selected_root
                })
                .env_clear()
                .env("PATH", &path)
                .env("HOME", &home)
                .env(
                    brand.environment_variable("GLOBAL_DIR")?,
                    installation.root(),
                )
                .stdin(Stdio::from(fs::File::open(&input_path)?));
            let result = tokio::task::spawn_blocking(move || shell.output()).await??;
            assert!(
                result.status.success(),
                "{engine:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stderr.is_empty());
            let context = String::from_utf8(result.stdout)?;
            if selected_root == &root {
                assert!(
                    context.contains("MANDATORY-be6e literal {{ untrusted }}"),
                    "{engine:?}: context missing"
                );
            } else {
                assert!(!context.contains("MANDATORY-be6e literal {{ untrusted }}"));
                assert!(context.contains("PROJECT Memory state: no_corpus."));
            }
            assert!(!context.contains("native-only-not-a-graphit-session"));
            if matches!(engine, Engine::Codex) {
                let mismatch = json!({"hook_event_name": native_event, "cwd": peer_root});
                fs::write(&input_path, serde_json::to_vec(&mismatch)?)?;
                let mut denied = Command::new("/bin/sh");
                denied
                    .args(["-c", &invocation])
                    .current_dir(selected_root)
                    .env_clear()
                    .env("PATH", &path)
                    .env("HOME", &home)
                    .env(
                        brand.environment_variable("GLOBAL_DIR")?,
                        installation.root(),
                    )
                    .stdin(Stdio::from(fs::File::open(&input_path)?));
                let result = tokio::task::spawn_blocking(move || denied.output()).await??;
                assert_eq!(result.status.code(), Some(1));
                assert!(result.stdout.is_empty());
                assert!(
                    String::from_utf8_lossy(&result.stderr).contains("Native hook input invalid")
                );
            }
            if !matches!(engine, Engine::Kimi) {
                let output: Value = serde_json::from_str(&context)?;
                assert!(output.is_object());
            }
            for event in [Event::Stop, Event::SessionEnd] {
                let Ok(native_event) = event.semantic().native_name(engine.binding()) else {
                    continue;
                };
                let invocation =
                    published_hook_command(selected_root, &home, brand.key(), engine, event)?;
                let mut native_input = input.clone();
                if !matches!(engine, Engine::Antigravity) {
                    native_input["hook_event_name"] = json!(native_event);
                }
                fs::write(&input_path, serde_json::to_vec(&native_input)?)?;
                let mut shell = Command::new("/bin/sh");
                shell
                    .args(["-c", &invocation])
                    .current_dir(if matches!(engine, Engine::Kimi) {
                        &home
                    } else {
                        selected_root
                    })
                    .env_clear()
                    .env("PATH", &path)
                    .env("HOME", &home)
                    .env(
                        brand.environment_variable("GLOBAL_DIR")?,
                        installation.root(),
                    )
                    .stdin(Stdio::from(fs::File::open(&input_path)?));
                let result = tokio::task::spawn_blocking(move || shell.output()).await??;
                assert!(
                    result.status.success(),
                    "{engine:?}/{event:?}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                if matches!(engine, Engine::Antigravity) {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&result.stdout)?,
                        json!({"decision":"stop"})
                    );
                } else {
                    assert!(result.stdout.is_empty());
                }
            }
        }
        let mut install = command(&root, &installation, &path, &home);
        install
            .args([
                "coordinator",
                "install",
                "--binding",
                "opencode",
                "--source-dir",
            ])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../templates/agent-instructions/coordinator"),
            );
        let result = tokio::task::spawn_blocking(move || install.output()).await??;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let published: CoordinatorOutcome = serde_json::from_slice(&result.stdout)?;
        assert!(published.failure.is_none());
        assert!(published.mcp.is_some());
        assert!(root.join(published.profile.unwrap()).is_file());
        let plugin = root.join(format!(
            ".opencode/plugins/{}-runtime-context.js",
            brand.key()
        ));
        assert!(!fs::read_to_string(&plugin)?.contains("MANDATORY-be6e"));
        let harness = temp.path().join("opencode-peer.mjs");
        fs::write(&harness, OPENCODE_PEER)?;
        let mut node = Command::new("node");
        node.arg(&harness)
            .arg(&plugin)
            .arg(&root)
            .current_dir(&home)
            .env_clear()
            .env("PATH", &path)
            .env("HOME", &home)
            .env(
                brand.environment_variable("GLOBAL_DIR")?,
                installation.root(),
            );
        let result = tokio::task::spawn_blocking(move || node.output()).await??;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let applied: Value = serde_json::from_slice(&result.stdout)?;
        assert_eq!(
            applied["system"][0],
            json!({"type":"text","text":"foreign-system"})
        );
        assert_eq!(
            applied["context"][0],
            json!({"type":"text","text":"foreign-context"})
        );
        for field in ["system", "context"] {
            assert!(
                applied[field][1]["text"]
                    .as_str()
                    .unwrap()
                    .contains("MANDATORY-be6e literal {{ untrusted }}")
            );
        }
        // Withdraw using the actual CLI, preserving a shared Kimi transport until the peer leaves.
        for (binding, selected_root) in NativeBinding::ALL
            .into_iter()
            .map(|binding| (binding, &root))
            .chain([(NativeBinding::Kimi, &peer_root)])
        {
            let mut remove = command(selected_root, &installation, &path, &home);
            remove.args(["coordinator", "remove", "--binding", binding.as_str()]);
            let result = tokio::task::spawn_blocking(move || remove.output()).await??;
            assert!(
                result.status.success(),
                "remove {binding:?} (daemon exit {:?}, Source terminal {:?}): {}",
                daemon.0.try_wait(),
                fs::read_to_string(&log).ok().and_then(|log| log
                    .lines()
                    .find(|line| line.starts_with("Graphit Source terminal: selected_branch="))
                    .map(str::to_owned)),
                String::from_utf8_lossy(&result.stderr)
            );
            let removed: CoordinatorOutcome = serde_json::from_slice(&result.stdout)?;
            assert!(removed.failure.is_none());
            if binding == NativeBinding::Kimi {
                let global: Value =
                    serde_json::from_slice(&fs::read(home.join(".kimi-code/mcp.json"))?)?;
                if selected_root == &root {
                    assert_eq!(removed.mcp.as_deref(), Some("RetainedReferences"));
                    assert!(global["mcpServers"][brand.key()].is_object());
                } else {
                    assert!(global["mcpServers"].get(brand.key()).is_none());
                }
            }
        }
        // Native SIGTERM preserves the original Interrupted cause after owned joins.
        assert_eq!(daemon.stop().await?, 130);
    }
    Ok(())
}

#[test]
fn coordinator_real_sdk_preserves_foreign_document_and_retains_collision_effect() {
    for collide in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.projects[0].0.root();
        let settings = root.join(".kiro/settings");
        fs::create_dir_all(&settings).unwrap();
        let name = if collide {
            fixture.policy.installation.brand().key()
        } else {
            "foreign"
        };
        let foreign = json!({"mcpServers":{name:{"command":"foreign-command","args":["foreign-argument",root]}},"extension":{"preserved":true,"user_path":fixture.policy.installation.root()}});
        let path = settings.join("mcp.json");
        fs::write(&path, serde_json::to_vec_pretty(&foreign).unwrap()).unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let installed = fixture.install(&fixture.input(0), &fixture.raw, &ctx);
        assert!(installed.outcome.profile.is_some());
        assert_eq!(installed.outcome.hooks.len(), 4);
        assert_eq!(installed.outcome.hooks[0].event, "SessionStart");
        assert_eq!(installed.outcome.hooks[1].event, "UserPromptSubmit");
        assert!(
            installed
                .outcome
                .hooks
                .iter()
                .all(|hook| hook.change == "PublishedInProfile")
        );
        assert!(root.join(installed.outcome.profile.unwrap()).is_file());
        let after: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(after["mcpServers"][name], foreign["mcpServers"][name]);
        assert_eq!(after["extension"], foreign["extension"]);
        if collide {
            assert!(installed.cause.is_some());
            assert!(installed.outcome.failure.is_some());
            assert!(installed.outcome.mcp.is_none());
        } else {
            assert!(installed.cause.is_none(), "{:?}", installed.cause);
        }
    }
}

#[test]
fn coordinator_real_source_rejects_changed_extra_and_links_before_effect() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let stock = fixture.input(0).source_dir.unwrap();
    for variant in ["changed", "extra", "link"] {
        let source = tempfile::tempdir().unwrap();
        for name in ["AGENT.md", "prompt.md"] {
            fs::copy(stock.join(name), source.path().join(name)).unwrap();
        }
        match variant {
            "changed" => fs::write(source.path().join("prompt.md"), "foreign prompt").unwrap(),
            "extra" => fs::write(source.path().join("foreign"), "extra file").unwrap(),
            "link" => {
                fs::remove_file(source.path().join("prompt.md")).unwrap();
                std::os::unix::fs::symlink(
                    stock.join("prompt.md"),
                    source.path().join("prompt.md"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let mut input = fixture.input(0);
        input.source_dir = Some(source.path().into());
        let installed = fixture.install(&input, &fixture.raw, &ctx);
        assert!(installed.cause.is_some());
        assert!(installed.outcome.profile.is_none());
        assert!(installed.outcome.mcp.is_none());
        assert!(!fixture.projects[0].0.root().join(".kiro").exists());
    }
    let restricted = Fixture::at(None, Brand::new("helios").unwrap(), false);
    let denied = restricted.install(&restricted.input(0), &restricted.raw, &ctx);
    assert_eq!(
        denied.cause.unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(denied.outcome.profile.is_none());
}

#[tokio::test]
async fn native_hook_cli_deadline_joins_when_host_keeps_stdin_open()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let installation = InstallationPaths::resolve(
        Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?,
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let root = temp.path().join("project");
    let home = temp.path().join("home");
    fs::create_dir(&root)?;
    fs::create_dir(&home)?;
    let project = ProjectPaths::at(&root, &installation)?;
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &Cancellation::default())?
        .ensure(&ProjectMetadata::new("native-hook-deadline")?)?;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
    child
        .current_dir(&root)
        .env_clear()
        .env("HOME", &home)
        .arg("--global-dir")
        .arg(installation.root())
        .args([
            "-c",
            "hub.limits.request.deadline_ms=50",
            "_session-hook",
            "--engine",
            "kiro",
            "--event",
            "SessionStart",
            "--project-dir",
        ])
        .arg(&root)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = child.spawn()?;
    // Keep the native host's writer open through wait: EOF cannot satisfy read.
    let input = child.stdin.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let exited = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    if exited.is_err() {
        child.kill().await?;
    }
    drop(input);
    let status = exited.map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "native hook retained stdin worker after its request expired",
        )
    })??;
    use tokio::io::AsyncReadExt;
    let mut error = String::new();
    stderr.read_to_string(&mut error).await?;
    assert_eq!(status.code(), Some(1));
    assert!(
        error.contains("timed out"),
        "unexpected hook failure: {error}"
    );
    Ok(())
}

fn published_hook_command(
    root: &Path,
    home: &Path,
    brand: &str,
    engine: graphit_app::session_hook_entry::Engine,
    event: graphit_app::session_hook_entry::Event,
) -> Result<String, Box<dyn std::error::Error>> {
    use clap::ValueEnum;
    use graphit_app::session_hook_entry::Engine;
    use graphit_hub::native::{
        DocumentFormat, FieldValue, NativeDocument,
        hooks::{CommandHookInput, plan_command_hook},
    };
    let cancel = Cancellation::default();
    let original = request(&cancel);
    let hub = graphit_hub::policy::HubResourcePolicy::resolve(&ConfigSnapshot::new(
        &Brand::new(brand)?,
        [],
    )?)?;
    let native_event = event.semantic().native_name(engine.binding())?;
    let identifier = format!("{brand}-{}-context", native_event.to_ascii_lowercase());
    let plan = plan_command_hook(
        engine.binding(),
        CommandHookInput {
            identifier: &identifier,
            native_event,
            command: "true",
            matcher: None,
            timeout: None,
        },
        hub.shared(),
        &original,
    )?;
    let path = if matches!(engine, Engine::Kimi) {
        home.join(".kimi-code/config.toml")
    } else {
        root.join(
            plan.project_config_path()
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?,
        )
    };
    let bytes = fs::read(&path)?;
    let document = NativeDocument::parse(
        if matches!(engine, Engine::Kimi) {
            DocumentFormat::Toml
        } else {
            DocumentFormat::Jsonc
        },
        &bytes,
        hub.shared(),
        &original,
    )?;
    fn collect(value: &FieldValue, event: &str, commands: &mut Vec<String>) {
        match value {
            FieldValue::Object(fields) => {
                if let Some(FieldValue::String(command)) = fields.get("command") {
                    let args = shlex::split(command).unwrap();
                    if args.iter().any(|arg| arg == "_session-hook")
                        && args
                            .windows(2)
                            .any(|args| args[0] == "--event" && args[1] == event)
                    {
                        commands.push(command.clone());
                    }
                }
                for value in fields.values() {
                    collect(value, event, commands);
                }
            }
            FieldValue::Array(values) => {
                for value in values {
                    collect(value, event, commands);
                }
            }
            _ => {}
        }
    }
    let hooks = match engine {
        Engine::Antigravity => document.get(&[plan.identifier(), native_event]),
        Engine::Kimi => document.get(&["hooks"]),
        _ => document.get(&["hooks", native_event]),
    }
    .map_err(|cause| io::Error::other(format!("published {engine:?}/{event:?} hooks: {cause}")))?
    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
    let mut commands = Vec::new();
    collect(
        &hooks,
        event.to_possible_value().unwrap().get_name(),
        &mut commands,
    );
    assert_eq!(
        commands.len(),
        1,
        "published {engine:?}/{event:?} command must be unique"
    );
    if matches!(engine, Engine::Kimi) {
        assert!(!commands[0].contains("--project-dir"));
        assert!(!commands[0].contains("--project-version"));
    }
    Ok(commands.remove(0))
}

const OPENCODE_PEER: &str = r#"
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const module = await import(pathToFileURL(process.argv[2]).href);
const directory = process.argv[3];
const hooks = new Map();
let eventSignal, pending, acknowledged;
const events = [];
const deliver = () => {
  if (!pending || !events.length) return;
  const { event, ack } = events.shift();
  const resolve = pending;
  pending = undefined;
  acknowledged = ack;
  resolve({ value: event, done: false });
};
const emit = event => new Promise(resolve => { events.push({ event, ack: resolve }); deliver(); });
const ctx = {
  location: { directory },
  session: {
    hook: async (name, callback) => {
      assert.ok(['context', 'compaction'].includes(name));
      assert.ok(!hooks.has(name));
      hooks.set(name, callback);
      return { async dispose() { hooks.delete(name); } };
    },
    get: async ({ sessionID }, options) => {
      assert.equal(sessionID, 'native-only');
      assert.ok(options?.signal instanceof AbortSignal);
      assert.equal(options.signal.aborted, false);
      return { location: { directory } };
    },
  },
  event: {
    subscribe: ({ signal }) => {
      eventSignal = signal;
      let closed = false;
      const close = () => {
        closed = true;
        pending?.({ done: true });
        pending = undefined;
        acknowledged?.();
        acknowledged = undefined;
      };
      signal.addEventListener('abort', close, { once: true });
      return {
        [Symbol.asyncIterator]() { return this; },
        next() {
          acknowledged?.();
          acknowledged = undefined;
          if (closed || signal.aborted) return Promise.resolve({ done: true });
          return new Promise(resolve => { pending = resolve; deliver(); });
        },
        return() { close(); return Promise.resolve({ done: true }); },
      };
    },
  },
};
assert.equal(typeof module.default.setup, 'function');
const cleanup = await module.default.setup(ctx);
assert.equal(typeof cleanup, 'function');
const system = { sessionID: 'native-only', agent: 'build', model: { providerID: 'fixture', id: 'literal' }, system: [{ type: 'text', text: 'foreign-system' }], messages: [{ role: 'user', content: 'foreign message' }], tools: { foreign: { description: 'Foreign tool', input: { type: 'object', properties: {} } } }, options: { literal: true } };
const compacting = { sessionID: 'native-only', agent: 'build', model: { providerID: 'fixture', id: 'literal' }, system: [{ type: 'text', text: 'foreign-context' }], messages: [{ role: 'user', content: 'foreign message' }], tools: { foreign: { description: 'Foreign tool', input: { type: 'object', properties: {} } } }, options: { literal: true } };
try {
  for (const [name, event] of [['context', system], ['compaction', compacting]]) {
    const before = JSON.stringify([event.messages, event.tools, event.options, event.result]);
    await hooks.get(name)(event);
    assert.equal(JSON.stringify([event.messages, event.tools, event.options, event.result]), before);
  }
  await emit({ type: 'session.idle', data: { sessionID: 'native-only' }, location: { directory } });
  await emit({ type: 'session.deleted', data: { sessionID: 'native-only' }, location: { directory } });
} finally {
  await cleanup();
}
assert.equal(eventSignal.aborted, true);
assert.equal(hooks.size, 0);
assert.equal(Object.hasOwn(compacting, 'result'), false);
console.log(JSON.stringify({ system: system.system, context: compacting.system }));
"#;
#[test]
fn native_coordinator_reports_hooks_when_workspace_mcp_collides() {
    let fixture = Fixture::new();
    let root = fixture.projects[0].0.root();
    let parent = root.join(".agents");
    fs::create_dir(&parent).unwrap();
    let path = parent.join("mcp_config.json");
    let foreign = serde_json::to_vec(&json!({"mcpServers":{fixture.policy.installation.brand().key():{"command":"foreign-command","args":[]}}})).unwrap();
    fs::write(&path, &foreign).unwrap();
    let mut input = fixture.input(0);
    input.binding = NativeBinding::Antigravity;
    let cancel = Cancellation::default();
    let original = request(&cancel);
    let installed = fixture.install(&input, &fixture.raw, &original);
    assert_eq!(
        installed.cause.unwrap().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(installed.outcome.profile.is_none());
    assert!(installed.outcome.failure.is_some());
    assert_eq!(installed.outcome.hooks.len(), 2);
    assert_eq!(installed.outcome.hooks[0].event, "PreInvocation");
    assert_eq!(installed.outcome.hooks[1].event, "Stop");
    assert_eq!(fs::read(&path).unwrap(), foreign);
    let hooks = fs::read_to_string(parent.join("hooks.json")).unwrap();
    assert!(hooks.contains("_session-hook"));
    assert!(hooks.contains("PreInvocation"));
}

#[test]
fn coordinator_removal_keeps_kimi_mcp_until_the_last_project_reference() {
    use graphit_hub::native::{DocumentFormat, NativeDocument};
    let fixture = Fixture::new();
    let global = fixture._temp.path().join("native-kimi");
    fs::create_dir(&global).unwrap();
    fs::write(global.join("config.toml"), "theme = \"foreign\"\n").unwrap();
    fs::write(
        global.join("mcp.json"),
        br#"{"mcpServers":{"foreign":{"command":"foreign","args":[]}},"extension":true}"#,
    )
    .unwrap();
    let cancel = Cancellation::default();
    let original = request(&cancel);
    for selected in 0..2 {
        let mut input = fixture.input(selected);
        input.binding = NativeBinding::Kimi;
        input.kimi_config_dir = Some(global.clone());
        let installed = fixture.install(&input, &fixture.raw, &original);
        assert!(installed.cause.is_none(), "{:?}", installed.cause);
        assert!(installed.outcome.profile.is_some());
    }
    let mut input = fixture.input(0);
    input.operation = project_hook_install::HookOperation::Remove;
    input.binding = NativeBinding::Kimi;
    input.source_dir = None;
    input.kimi_config_dir = Some(global.clone());
    let removed = fixture.install(&input, &fixture.raw, &original);
    assert!(removed.cause.is_none(), "{:?}", removed.cause);
    assert_eq!(removed.outcome.mcp.as_deref(), Some("RetainedReferences"));
    let mcp: Value = serde_json::from_slice(&fs::read(global.join("mcp.json")).unwrap()).unwrap();
    assert!(mcp["mcpServers"][fixture.policy.installation.brand().key()].is_object());
    input.project_dir = fixture.projects[1].0.root().to_owned();
    let removed = fixture.install(&input, &fixture.raw, &original);
    assert!(removed.cause.is_none(), "{:?}", removed.cause);
    let mcp: Value = serde_json::from_slice(&fs::read(global.join("mcp.json")).unwrap()).unwrap();
    assert!(
        mcp["mcpServers"]
            .get(fixture.policy.installation.brand().key())
            .is_none()
    );
    assert_eq!(mcp["mcpServers"]["foreign"]["command"], "foreign");
    assert_eq!(mcp["extension"], true);
    let config = fs::read(global.join("config.toml")).unwrap();
    let document = NativeDocument::parse(
        DocumentFormat::Toml,
        &config,
        fixture.policy.native_install.hub.shared(),
        &original,
    )
    .unwrap();
    assert!(document.get(&["hooks"]).unwrap().is_none_or(
        |value| matches!(value, graphit_hub::native::FieldValue::Array(values) if values.is_empty())
    ));
    assert!(String::from_utf8(config).unwrap().contains("foreign"));
    for (paths, _, _) in &fixture.projects {
        assert!(
            !paths
                .root()
                .join(format!(
                    ".kimi-code/agents/{}-coordinator.md",
                    paths.brand().key()
                ))
                .exists()
        );
    }
}

#[test]
fn coordinator_removal_uses_hub_ownership_for_inline_kiro_hooks() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let original = request(&cancel);
    let mut input = fixture.input(0);
    let installed = fixture.install(&input, &fixture.raw, &original);
    assert!(installed.cause.is_none(), "{:?}", installed.cause);
    let relative = installed.outcome.profile.unwrap();
    let profile = fixture.projects[0].0.root().join(&relative);
    let before = fs::read(&profile).unwrap();
    let hook_input = project_hook_install::HookRequest {
        operation: project_hook_install::HookOperation::Remove,
        binding: NativeBinding::KiroV3,
        profile: Some(profile.file_name().unwrap().to_str().unwrap().into()),
        project_dir: input.project_dir.clone(),
        project_version: input.project_version.clone(),
        kimi_config_dir: None,
    };
    let denied = project_hook_install::execute(
        InstallResources {
            auth: &fixture.auth,
            global_paths: None,
            projects: &fixture.projects,
            current: 0,
            policy: &fixture.policy,
            modules: &[],
            memory_read: fixture.memory,
        },
        &fixture.raw,
        &hook_input,
        &original,
    );
    assert_eq!(
        denied.cause.unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(fs::read(&profile).unwrap(), before);
    input.operation = project_hook_install::HookOperation::Remove;
    input.source_dir = None;
    let removed = fixture.install(&input, &fixture.raw, &original);
    assert!(removed.cause.is_none(), "{:?}", removed.cause);
    assert!(removed.outcome.publication.is_some());
    assert!(!profile.exists());
}
