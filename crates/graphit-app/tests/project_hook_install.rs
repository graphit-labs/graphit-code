//! Real Auth/Core/Hub publication and compiled CLI journeys. ACP peers test transport,
//! not installed Kiro model perception.
#![cfg(unix)]
use graphit_app::{
    managed_roles::InstallResources,
    project_hook_install::{self, HookOutcome, HookRequest},
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
    NativeBinding,
    native::{AgentDescriptor, ContentLimits, RenderContext, ToolGrants, render_agent},
    policy::RequestContext,
    registry::HubRegistry,
};
use graphit_records::memory_store::MemoryStoreReadLimits;
use serde_json::{Value, json};
use std::{
    fs, io,
    num::{NonZeroU32, NonZeroUsize},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

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
fn profile_document(bytes: &[u8]) -> (Value, &str) {
    let text = std::str::from_utf8(bytes).unwrap();
    let (header, body) = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap();
    (serde_saphyr::from_str(header).unwrap(), body)
}
fn read_profile(file: &Path) -> Value {
    profile_document(&fs::read(file).unwrap()).0
}
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
fn write_profile(file: &Path, metadata: &Value, body: &str) -> io::Result<()> {
    let header = serde_saphyr::to_string(metadata).map_err(io::Error::other)?;
    fs::write(file, format!("---\n{header}---\n{body}"))
}
fn coordinator(root: &Path, name: &str) -> io::Result<PathBuf> {
    let cancel = Cancellation::default();
    let mut render = RenderContext::new(
        content_limits(),
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .map_err(io::Error::other)?;
    let descriptor = AgentDescriptor::new(
        name,
        "PROJECT Coordinator",
        "Read the PROJECT policy at runtime.",
        false,
        true,
        &mut render,
    )
    .map_err(io::Error::other)?;
    let native = render_agent(
        NativeBinding::KiroV3,
        &descriptor,
        None,
        &ToolGrants {
            binding: NativeBinding::KiroV3,
            read_only: false,
            allows_shell: true,
            tools: Vec::new(),
        },
        &mut render,
    )
    .map_err(io::Error::other)?;
    let (mut profile, body) = profile_document(&native.bytes);
    profile["extension"] = json!({"preserved":"foreign profile data"});
    profile["hooks"] = json!([{"name":"foreign","trigger":"Stop","action":{"type":"command","command":"printf foreign"},"timeout":1500,"foreign":true}]);
    let parent = root.join(".kiro/agents");
    fs::create_dir_all(&parent)?;
    let file = parent.join(format!("{name}.md"));
    write_profile(&file, &profile, body)?;
    Ok(file)
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
    fn input(&self, selected: usize) -> HookRequest {
        HookRequest {
            operation: project_hook_install::HookOperation::Install,
            binding: NativeBinding::KiroV3,
            kimi_config_dir: None,
            profile: Some("coordinator.md".into()),
            project_dir: self.projects[selected].0.root().into(),
            project_version: Some("main".into()),
        }
    }
    fn install(
        &self,
        input: &HookRequest,
        raw: &str,
        ctx: &RequestContext<'_>,
    ) -> project_hook_install::HookExecution {
        project_hook_install::execute(
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
#[test]
fn project_hook_native_two_projects_preserve_profiles_repeat_and_cross_brand_state() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    for selected in 0..2 {
        let root = fixture.projects[selected].0.root();
        let file = coordinator(root, "coordinator").unwrap();
        let before_bytes = fs::read(&file).unwrap();
        let (before, before_body) = profile_document(&before_bytes);
        let input = fixture.input(selected);
        let installed = fixture.install(&input, &fixture.raw, &ctx);
        assert!(installed.cause.is_none(), "{:?}", installed.cause);
        assert_eq!(installed.outcome.hooks.len(), 4);
        let installed_bytes = fs::read(&file).unwrap();
        assert_portable_artifact(
            std::str::from_utf8(&installed_bytes).unwrap(),
            root,
            &fixture.policy.installation,
        );
        let (after, after_body) = profile_document(&installed_bytes);
        assert_eq!(after_body.as_bytes(), before_body.as_bytes());
        for field in ["name", "description", "tools", "extension"] {
            assert_eq!(after[field], before[field]);
        }
        assert_eq!(after["hooks"][0], before["hooks"][0]);
        assert_eq!(after["hooks"][1]["trigger"], "SessionStart");
        assert_eq!(after["hooks"][2]["trigger"], "UserPromptSubmit");
        assert!(after.get("version").is_none());
        let repeat = fixture.install(&input, &fixture.raw, &ctx);
        assert!(repeat.cause.is_none());
        assert!(
            repeat
                .outcome
                .hooks
                .iter()
                .all(|effect| effect.change == "Unchanged")
        );
        assert_eq!(fs::read(&file).unwrap(), installed_bytes);
        assert!(!root.join(".kiro/hooks").exists());
    }
    let root = fixture.projects[0].0.root();
    let other = Fixture::at(Some(root), Brand::new("qualifier").unwrap(), true);
    let input = other.input(0);
    assert!(other.install(&input, &other.raw, &ctx).cause.is_none());
    let native = read_profile(&root.join(".kiro/agents/coordinator.md"));
    assert_eq!(native["hooks"].as_array().unwrap().len(), 9);
    assert!(
        fixture
            .install(&fixture.input(0), &fixture.raw, &ctx)
            .cause
            .is_none()
    );
    assert!(root.join(".kiro/.graphit-shared-documents").is_dir());
    assert!(!root.join(".kiro/.qualifier-shared-documents").exists());
}

#[test]
fn project_hook_native_denies_authority_inputs_roles_aliases_and_preserves_partial_effects() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let root = fixture.projects[0].0.root();
    let file = coordinator(root, "coordinator").unwrap();
    let original = fs::read(&file).unwrap();
    let input = fixture.input(0);
    let denied = fixture.install(&input, "invalid-runtime-credential", &ctx);
    assert_eq!(
        denied.cause.unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        denied.outcome.failure.as_deref(),
        Some("hook installation denied")
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    let mut bad = fixture.input(0);
    bad.profile = Some("../coordinator.md".into());
    assert!(fixture.install(&bad, &fixture.raw, &ctx).cause.is_some());
    bad.profile = Some("missing.md".into());
    assert_eq!(
        fixture
            .install(&bad, &fixture.raw, &ctx)
            .cause
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert!(!root.join(".kiro/agents/missing.md").exists());
    bad = fixture.input(0);
    bad.project_dir = fixture._temp.path().join("untrusted");
    assert!(fixture.install(&bad, &fixture.raw, &ctx).cause.is_some());
    // Removed machine-path inputs are rejected rather than retained as dead API fields.
    let mut obsolete = serde_json::to_value(fixture.input(0)).unwrap();
    obsolete["executable"] = json!("/untrusted/graphit");
    assert!(serde_json::from_value::<HookRequest>(obsolete).is_err());
    for name in [
        "graphit-scout",
        "graphit-tracker",
        "graphit-scribe",
        "graphit-dream",
    ] {
        let role = coordinator(root, name).unwrap();
        let before = fs::read(&role).unwrap();
        bad = fixture.input(0);
        bad.profile = Some(format!("{name}.md"));
        assert_eq!(
            fixture
                .install(&bad, &fixture.raw, &ctx)
                .cause
                .unwrap()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read(role).unwrap(), before);
    }
    let (mut proxy, original_body) = profile_document(&original);
    proxy["mcpServers"] = json!({"native":{"command":"graphit","args":["mcp","--stdio","--role"]}});
    write_profile(&file, &proxy, original_body).unwrap();
    assert_eq!(
        fixture
            .install(&input, &fixture.raw, &ctx)
            .cause
            .unwrap()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::write(&file, &original).unwrap();
    // The second effect collides; the real SDK retains the first, never rolls it back.
    let (mut collision, original_body) = profile_document(&original);
    collision["hooks"].as_array_mut().unwrap().push(json!({"name":format!("{}-prompt-context", fixture.policy.installation.brand().key()),"trigger":"UserPromptSubmit","action":{"type":"command","command":project_hook_install::client::native_command(&fixture.policy.installation, Some(&fixture.projects[0].0), input.project_version.as_deref(), graphit_app::session_hook_entry::Engine::Kiro, graphit_app::session_hook_entry::Event::UserPromptSubmit, graphit_app::session_hook_entry::Recipient::Coordinator).unwrap()},"foreign":true}));
    write_profile(&file, &collision, original_body).unwrap();
    let partial = fixture.install(&input, &fixture.raw, &ctx);
    assert_eq!(partial.cause.unwrap().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(partial.outcome.hooks.len(), 1);
    let after = read_profile(&file);
    assert_eq!(after["hooks"][1], collision["hooks"][1]);
    assert_eq!(after["hooks"].as_array().unwrap().len(), 3);
    assert_eq!(after["hooks"][2]["trigger"], "SessionStart");
    let removed = fixture.auth.remove_runtime(ctx.snapshot());
    assert!(removed.failure().is_none());
    assert!(fixture.install(&input, &fixture.raw, &ctx).cause.is_some());
    let no_permission = Fixture::at(None, Brand::default(), false);
    coordinator(no_permission.projects[0].0.root(), "coordinator").unwrap();
    assert_eq!(
        no_permission
            .install(&no_permission.input(0), &no_permission.raw, &ctx)
            .cause
            .unwrap()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    let cancelled = Cancellation::default();
    let cancelled_ctx = request(&cancelled);
    cancelled.cancel();
    assert_eq!(
        no_permission
            .install(&no_permission.input(0), &no_permission.raw, &cancelled_ctx)
            .cause
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = RequestContext::new(
        content_limits(),
        &cancel,
        Instant::now() + Duration::from_millis(100),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(110));
    assert_eq!(
        no_permission
            .install(&no_permission.input(0), &no_permission.raw, &expired)
            .cause
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    let mut fractional = Fixture::new();
    fractional.policy.timeout = Duration::from_millis(1);
    assert_eq!(
        fractional
            .install(&fractional.input(0), &fractional.raw, &ctx)
            .cause
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!fractional.projects[0].0.root().join(".kiro").exists());
    for component in [".kiro", "agents"] {
        let alias = Fixture::new();
        let project = alias.projects[0].0.root();
        let outside = alias._temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let external_profile = if component == ".kiro" {
            coordinator(&outside, "coordinator").unwrap()
        } else {
            let profile = coordinator(&outside, "coordinator").unwrap();
            fs::create_dir(project.join(".kiro")).unwrap();
            profile
        };
        let destination = if component == ".kiro" {
            outside.join(".kiro")
        } else {
            outside.join(".kiro/agents")
        };
        let link = if component == ".kiro" {
            project.join(".kiro")
        } else {
            project.join(".kiro/agents")
        };
        symlink(destination, link).unwrap();
        let before = fs::read(&external_profile).unwrap();
        assert!(
            alias
                .install(&alias.input(0), &alias.raw, &ctx)
                .cause
                .is_some()
        );
        assert_eq!(fs::read(&external_profile).unwrap(), before);
        assert!(!outside.join(".kiro/.graphit-shared-documents").exists());
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
fn acp_fixture(bin: &Path, brand: &Brand) -> io::Result<()> {
    let prefix = format!(
        "EXE={}\nBRAND={}\nHOOKS_FILE={}\n",
        serde_json::to_string(env!("CARGO_BIN_EXE_graphit"))?,
        serde_json::to_string(brand.key())?,
        serde_json::to_string(
            &bin.parent()
                .ok_or(io::ErrorKind::InvalidInput)?
                .join("hook-commands.json")
        )?
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
            with subprocess.Popen([EXE,"mcp","--stdio","--role"],cwd=cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) as child:
                assert "result" in request(child,"initialize",{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"hook-qualification","version":"1"}},1)
                request(child,"notifications/initialized",{},None)
                answer=request(child,"tools/call",{"name":BRAND+"_memory_insert","arguments":{"input":{"idempotency_key":"hook-native-mandatory-3180","title":"PROJECT mandatory hook qualification","body":"MANDATORY-3180 literal {{ untrusted }}","classification":"convention","mandatory":True,"important":False,"tags":[]}}},2)
                assert "result" in answer and not answer["result"].get("isError",False)
                child.stdin.close();assert child.wait(timeout=15)==0
        else:
            # The private ENV selects Role even without --role; verify its real scope and write denial.
            with subprocess.Popen([EXE,"mcp","--stdio"],cwd=cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) as child:
                assert "result" in request(child,"initialize",{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"hook-qualification","version":"1"}},1)
                request(child,"notifications/initialized",{},None)
                answer=request(child,"tools/call",{"name":BRAND+"_module_skill","arguments":{"input":{"module":"task"}}},2)
                assert "result" in answer and not answer["result"].get("isError",False)
                assert BRAND+"_task" in json.dumps(answer["result"])
                denied=request(child,"tools/call",{"name":BRAND+"_memory_list","arguments":{"input":{}}},3)
                assert "result" in denied and denied["result"].get("isError",False)
                denied=request(child,"tools/call",{"name":BRAND+"_memory_insert","arguments":{"input":{"idempotency_key":"hook-native-tracker-denied","title":"Tracker must not write Memory","body":"must remain denied","classification":"convention","mandatory":False,"important":False,"tags":[]}}},4)
                assert "result" in denied and denied["result"].get("isError",False)
                child.stdin.close();assert child.wait(timeout=15)==0
                stderr=child.stderr.read()
                assert "MANDATORY-3180" not in stderr and os.environ["GRAPHIT_ROLE_BEARER"] not in stderr
            with open(HOOKS_FILE) as file:hooks=json.load(file)
            for hook in hooks:
                result=subprocess.run(["/bin/sh","-c",hook],cwd=cwd,input="",capture_output=True,text=True)
                assert result.returncode!=0 and not result.stdout
                assert "Root session hook is unavailable in a managed-role transport context" in result.stderr
                assert "MANDATORY-3180" not in result.stderr and os.environ["GRAPHIT_ROLE_BEARER"] not in result.stderr
        send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"hook-boundary","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Role hook boundary verified"}}}})
        send({"jsonrpc":"2.0","id":ident,"result":{"stopReason":"end_turn"}})
"#;
    let script = format!("#!/usr/bin/env python3\n{prefix}{body}");
    let file = bin.join("kiro-cli");
    fs::write(&file, script)?;
    fs::set_permissions(file, fs::Permissions::from_mode(0o700))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_hook_compiled_cli_real_writer_mandatory_and_role_factory_v3()
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
        // Shell metacharacters and Unicode stay data through shlex and the real shell.
        let root = temp.path().join("project with 'quotes' $meta α");
        fs::create_dir(&root)?;
        let home = temp.path().join("home");
        fs::create_dir(&home)?;
        let project = ProjectPaths::at(&root, &installation)?;
        let cancel = Cancellation::default();
        let project_id = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("hook-qualification")?)?
            .id();
        ConfigStore::global(&installation)?.set(
            ConfigKey::new("agent")?,
            json!(binding.as_str()),
            Duration::from_secs(5),
            &cancel,
        )?;
        let profile = coordinator(&root, "coordinator")?;
        let bin = temp.path().join("bin");
        fs::create_dir(&bin)?;
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_graphit"), bin.join(brand.binary_name()))?;
        acp_fixture(&bin, &brand)?;
        let mut search = vec![bin];
        search.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(search)?;
        // Source has neither the installation variable nor Graphit on PATH.
        // Its managed child must receive both from the captured native launch.
        let source_bin = temp.path().join("source-bin");
        fs::create_dir(&source_bin)?;
        let python = Command::new(which::which("python3")?)
            .args(["-c", "import sys; print(sys.executable)"])
            .output()?;
        assert!(
            python.status.success(),
            "Python interpreter discovery failed"
        );
        let executable = String::from_utf8(python.stdout)?;
        let executable = Path::new(executable.trim_end_matches(['\r', '\n']));
        assert!(executable.is_absolute() && executable.is_file());
        // A Python version-manager shim can require a shell absent from this private PATH.
        std::os::unix::fs::symlink(executable, source_bin.join("python3"))?;
        std::os::unix::fs::symlink(which::which("git")?, source_bin.join("git"))?;
        std::os::unix::fs::symlink(
            temp.path().join("bin/kiro-cli"),
            source_bin.join("kiro-cli"),
        )?;
        let log = temp.path().join("daemon.log");
        let mut daemon = Daemon(
            command(&root, &installation, &path, &home)
                .env("PATH", &source_bin)
                .env_remove(brand.environment_variable("GLOBAL_DIR")?)
                .arg("--global-dir")
                .arg(installation.root())
                .args(["daemon", "foreground"])
                .stdout(Stdio::null())
                .stderr(fs::File::create(&log)?)
                .spawn()?,
        );
        let until = Instant::now() + Duration::from_secs(30);
        while !installation.root().join("daemon/mcp.json").exists() {
            if daemon.0.try_wait()?.is_some() {
                return Err(io::Error::other("temporary foreground startup failed").into());
            }
            if Instant::now() >= until {
                return Err(io::Error::from(io::ErrorKind::TimedOut).into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let key = fs::read(installation.root().join("daemon/mcp.key"))?;
        let mut installation_command = command(&root, &installation, &path, &home);
        installation_command.args([
            "hooks",
            "install",
            "--binding",
            "kiro-v3",
            "--profile",
            "coordinator.md",
        ]);
        let installed =
            tokio::task::spawn_blocking(move || installation_command.output()).await??;
        let result: HookOutcome = serde_json::from_slice(&installed.stdout)?;
        assert!(
            installed.status.success(),
            "install category: {:?}",
            result.failure
        );
        assert!(installed.stderr.is_empty());
        assert_eq!(result.hooks.len(), 4);
        let installed_profile = fs::read(&profile)?;
        assert_portable_artifact(
            std::str::from_utf8(&installed_profile)?,
            &root,
            &installation,
        );
        let (profile_document, _) = profile_document(&installed_profile);
        let hooks: Vec<String> = profile_document["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|hook| {
                hook["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(&format!("{}-", brand.key())))
            })
            .map(|hook| hook["action"]["command"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(hooks.len(), 4);
        // Controlled peer input comes from the actual published profile, decoded by Rust.
        // This exercises authority boundaries, not installed Kiro profile perception.
        fs::write(
            temp.path().join("hook-commands.json"),
            serde_json::to_vec(&hooks)?,
        )?;
        for hook in &hooks {
            let argv = shlex::split(hook).unwrap();
            assert_eq!(argv[0], brand.binary_name());
            assert!(argv.windows(2).any(|args| args == ["--project-dir", "."]));
            assert!(!argv.iter().any(|arg| arg == "--global-dir"));
            assert!(!hook.contains(root.to_str().unwrap()));
            assert!(!hook.contains(installation.root().to_str().unwrap()));
            assert!(!hook.contains("MANDATORY-3180"));
            assert!(!hook.contains("GRAPHIT_ROLE_BEARER"));
        }
        // Scribe creates mandatory Memory via the real run-owned MCP write admission.
        for role in ["scribe", "tracker"] {
            let mut run = command(&root, &installation, &path, &home);
            run.args([
                "roles",
                "run",
                "--role",
                role,
                "--prompt",
                "Check the actual hook boundary",
            ]);
            let result = tokio::task::spawn_blocking(move || run.output()).await??;
            let outcome: RunOutcome = serde_json::from_slice(&result.stdout)?;
            assert!(
                result.status.success(),
                "Role {role} category: {:?}; closed: {}",
                outcome.failure,
                outcome.run_closed
            );
            assert!(outcome.run_closed);
            let role_profile = root.join(format!(".kiro/agents/{}-{role}.md", brand.key()));
            assert_portable_artifact(&fs::read_to_string(role_profile)?, &root, &installation);
            assert_eq!(
                outcome.answer.as_deref(),
                Some("Role hook boundary verified")
            );
        }
        // Native shell runs the installed commands: Rust CLI -> Source -> PROJECT Memory -> maintained templates.
        for hook in &hooks {
            let mut shell = Command::new("/bin/sh");
            shell
                .args(["-c", hook])
                .current_dir(&root)
                .env_clear()
                .env("PATH", &path)
                .env("HOME", &home)
                .env(
                    brand.environment_variable("GLOBAL_DIR")?,
                    installation.root(),
                );
            let result = tokio::task::spawn_blocking(move || shell.output()).await??;
            assert!(result.status.success(), "Coordinator hook failed");
            assert!(result.stderr.is_empty());
            let text = String::from_utf8(result.stdout)?;
            let args = shlex::split(hook).unwrap();
            let context = args.windows(2).any(|args| {
                args[0] == "--event"
                    && matches!(args[1].as_str(), "SessionStart" | "UserPromptSubmit")
            });
            if context {
                assert!(text.contains("MANDATORY-3180 literal {{ untrusted }}"));
                assert!(text.contains("PROJECT"));
            } else {
                assert!(text.is_empty());
            }
            assert!(
                !text
                    .as_bytes()
                    .windows(key.len())
                    .any(|window| window == key)
            );
        }
        // Closed HTTP DTOs and unauthenticated requests fail before publication.
        let ctx = request(&cancel);
        let native = NativeDocumentFileLimits::new(1048576, 65536, 256, 8388608)?;
        let discovery = graphit_daemon::source_discovery::read_discovery(
            &installation,
            native,
            ctx.snapshot(),
        )?
        .unwrap();
        let url = format!(
            "http://{}{}",
            discovery.address(),
            project_hook_install::ROUTE
        );
        let client = reqwest::Client::new();
        assert_eq!(
            client.post(&url).json(&json!({})).send().await?.status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
        for body in [
            json!({"binding":"unknown-host"}),
            json!({"binding":null}),
            json!({"binding":"kiro-v3","unknown":true}),
        ] {
            let response = client
                .post(&url)
                .header("x-graphit-auth-origin", "runtime")
                .bearer_auth(std::str::from_utf8(&key)?)
                .json(&body)
                .send()
                .await?;
            assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        }
        let mut bounded = command(&root, &installation, &path, &home);
        // Keep the policy coherent and leave room for native file access metadata.
        assert!(installed.stdout.len() > 128);
        bounded.args([
            "-c",
            "hub.limits.content.scalar_bytes=128",
            "-c",
            "hub.limits.content.output_bytes=128",
            "hooks",
            "install",
            "--binding",
            "kiro-v3",
            "--profile",
            "coordinator.md",
        ]);
        let bounded = tokio::task::spawn_blocking(move || bounded.output()).await??;
        assert!(!bounded.status.success());
        assert!(bounded.stdout.is_empty());
        let diagnostic = String::from_utf8(bounded.stderr)?;
        assert!(
            !diagnostic
                .as_bytes()
                .windows(key.len())
                .any(|window| window == key)
        );
        assert_eq!(diagnostic.trim(), "graphit: file too large");
        // Hold the SDK writer so an uncancelled removal would be observable later.
        // Recipient EOF must remain request-local; service stop still joins before key withdrawal.
        let hub =
            graphit_hub::policy::HubResourcePolicy::resolve(&ConfigSnapshot::new(&brand, [])?)?;
        let scope = graphit_core::artifact_store::CapturedPublicationScope::project(
            &project,
            project_id,
            ScopeLimits::new(16, 65536, 256)?,
            ctx.snapshot(),
        )?;
        let core = graphit_core::artifact_store::ArtifactStore::new(scope, hub.artifacts());
        let parent = graphit_core::native_document_file::CapturedDocumentDirectory::capture(
            &root.join(".kiro/agents"),
            native,
            ctx.snapshot(),
        )?;
        let shared = graphit_hub::native::shared_document::SharedDocumentStore::open(
            &root.join(".kiro/.graphit-shared-documents"),
        )?;
        let held_writer = shared.writer(&parent, "coordinator.md", &core, hub.shared(), &ctx)?;
        let pending = HookRequest {
            operation: project_hook_install::HookOperation::Remove,
            binding: NativeBinding::KiroV3,
            kimi_config_dir: None,
            profile: Some("coordinator.md".into()),
            project_dir: root.clone(),
            project_version: Some("main".into()),
        };
        let response = tokio::time::timeout(
            Duration::from_secs(3),
            client
                .post(&url)
                .header("x-graphit-auth-origin", "runtime")
                .bearer_auth(std::str::from_utf8(&key)?)
                .json(&pending)
                .send(),
        )
        .await??;
        assert!(response.status().is_success());
        drop(response);
        // A disconnected request must not stop the service or revoke its caller.
        let mut serving = command(&root, &installation, &path, &home);
        serving
            .args([
                "_session-hook",
                "--engine",
                "kiro",
                "--event",
                "SessionStart",
                "--project-dir",
                ".",
            ])
            .stdin(Stdio::null());
        let serving = tokio::task::spawn_blocking(move || serving.output()).await??;
        assert!(
            serving.status.success(),
            "Source must serve after recipient EOF"
        );
        assert!(serving.stderr.is_empty());
        assert!(
            String::from_utf8(serving.stdout)?.contains("MANDATORY-3180 literal {{ untrusted }}")
        );
        let snapshot = fs::read(root.join(".kiro/agents/coordinator.md"))?;
        assert_eq!(snapshot, installed_profile);
        assert_eq!(fs::read(installation.root().join("daemon/mcp.key"))?, key);
        let exit = daemon.stop().await?;
        let diagnostic = fs::read_to_string(&log)?;
        assert!(!diagnostic.is_empty());
        assert!(
            !diagnostic
                .as_bytes()
                .windows(key.len())
                .any(|window| window == key)
        );
        drop(held_writer);
        assert_eq!(
            fs::read(root.join(".kiro/agents/coordinator.md"))?,
            installed_profile
        );
        assert!(!installation.root().join("daemon/mcp.key").exists());
        assert!(!installation.root().join("daemon/mcp.json").exists());
        let terminal = diagnostic
            .lines()
            .find(|line| line.starts_with("Graphit Source terminal: selected_branch="));
        assert_eq!(
            exit, 130,
            "foreground SIGTERM after request-owned EOF must remain cancellation ({terminal:?})"
        );
    }
    Ok(())
}

#[test]
fn project_hook_native_agents_namespace_swaps_never_read_or_mutate_external_profile() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let fixture = Fixture::new();
    let root = fixture.projects[0].0.root();
    coordinator(root, "coordinator").unwrap();
    let outside = fixture._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let external = coordinator(&outside, "coordinator").unwrap();
    let before = fs::read(&external).unwrap();
    let parent = root.join(".kiro");
    let target = outside.join(".kiro/agents");
    let stop = Arc::new(AtomicBool::new(false));
    let swaps = Arc::new(AtomicUsize::new(0));
    let thread_stop = Arc::clone(&stop);
    let thread_swaps = Arc::clone(&swaps);
    let swapper = std::thread::spawn(move || {
        let active = parent.join("agents");
        let held = parent.join("held-agents");
        while !thread_stop.load(Ordering::Acquire) {
            fs::rename(&active, &held).unwrap();
            symlink(&target, &active).unwrap();
            thread_swaps.fetch_add(1, Ordering::Release);
            std::thread::yield_now();
            fs::remove_file(&active).unwrap();
            fs::rename(&held, &active).unwrap();
        }
    });
    while swaps.load(Ordering::Acquire) == 0 {
        std::thread::yield_now();
    }
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    for _ in 0..32 {
        let _outcome = fixture.install(&fixture.input(0), &fixture.raw, &ctx);
    }
    stop.store(true, Ordering::Release);
    swapper.join().unwrap();
    assert!(swaps.load(Ordering::Acquire) > 0);
    assert_eq!(fs::read(&external).unwrap(), before);
    assert!(!outside.join(".kiro/.graphit-shared-documents").exists());
}

#[test]
fn native_hooks_publish_current_host_documents_and_single_global_kimi_hook() {
    use graphit_hub::native::{
        DocumentFormat, FieldValue, NativeDocument,
        hooks::{CommandHookInput, plan_command_hook},
    };
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    for binding in NativeBinding::ALL
        .into_iter()
        .filter(|binding| *binding != NativeBinding::KiroV3)
    {
        let mut input = fixture.input(0);
        input.binding = binding;
        input.profile = None;
        input.kimi_config_dir =
            (binding == NativeBinding::Kimi).then(|| fixture._temp.path().join("global-kimi"));
        let installed = fixture.install(&input, &fixture.raw, &ctx);
        assert!(
            installed.cause.is_none(),
            "{binding:?}: {:?}",
            installed.cause
        );
        if binding == NativeBinding::OpenCode {
            let plugin = fixture.projects[0].0.root().join(format!(
                ".opencode/plugins/{}-runtime-context.js",
                fixture.policy.installation.brand().key()
            ));
            let bytes = fs::read(&plugin).unwrap();
            assert!(!bytes.is_empty());
            assert_eq!(installed.outcome.hooks.len(), 1);
            let repeat = fixture.install(&input, &fixture.raw, &ctx);
            assert!(repeat.cause.is_none());
            assert_eq!(fs::read(plugin).unwrap(), bytes);
            continue;
        }
        let event = match binding {
            NativeBinding::Cursor => "sessionStart",
            NativeBinding::Antigravity => "PreInvocation",
            NativeBinding::Kimi => "UserPromptSubmit",
            _ => "SessionStart",
        };
        let plan = plan_command_hook(
            binding,
            CommandHookInput {
                identifier: "path-selector",
                native_event: event,
                command: "true",
                matcher: None,
                timeout: None,
            },
            fixture.policy.native_install.hub.shared(),
            &ctx,
        )
        .unwrap();
        let path = if binding == NativeBinding::Kimi {
            input.kimi_config_dir.as_ref().unwrap().join("config.toml")
        } else {
            fixture.projects[0]
                .0
                .root()
                .join(plan.project_config_path().unwrap())
        };
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.is_empty());
        let repeat = fixture.install(&input, &fixture.raw, &ctx);
        assert!(repeat.cause.is_none(), "{binding:?}: {:?}", repeat.cause);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        if binding == NativeBinding::Kimi {
            input.project_dir = fixture.projects[1].0.root().to_owned();
            assert!(fixture.install(&input, &fixture.raw, &ctx).cause.is_none());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            let document = NativeDocument::parse(
                DocumentFormat::Toml,
                &bytes,
                fixture.policy.native_install.hub.shared(),
                &ctx,
            )
            .unwrap();
            let Some(FieldValue::Array(hooks)) = document.get(&["hooks"]).unwrap() else {
                panic!("missing native Kimi hooks");
            };
            assert_eq!(hooks.len(), 3);
            let FieldValue::Object(fields) = &hooks[0] else {
                panic!("invalid native Kimi hook");
            };
            let Some(FieldValue::String(command)) = fields.get("command") else {
                panic!("missing Kimi command");
            };
            assert!(!command.contains("--project-dir"));
            assert!(!command.contains("--project-version"));
        }
    }
}
