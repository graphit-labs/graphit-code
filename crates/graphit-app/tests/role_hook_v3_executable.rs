//! Actual compiled CLI, Role factory/issuer and PROJECT Memory over controlled ACP.
//! This does not qualify installed Kiro or model perception.
#![cfg(unix)]
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
    os::unix::fs::PermissionsExt,
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
    let prefix = format!(
        "EXE={}\nINSPECTOR={}\nBRAND={}\nPROGRAM={}\nSTAGE={}\nHOME={}\n",
        serde_json::to_string(env!("CARGO_BIN_EXE_graphit"))?,
        serde_json::to_string(env!("CARGO_BIN_EXE_kiro-knowledge-fixture"))?,
        serde_json::to_string(brand.key())?,
        serde_json::to_string(brand.binary_name())?,
        serde_json::to_string(
            bin.parent()
                .unwrap()
                .join("peer-stage.json")
                .to_str()
                .unwrap()
        )?,
        serde_json::to_string(bin.parent().unwrap().join("home").to_str().unwrap())?,
    );
    let body = r#"import json,os,sys,subprocess,shlex,shutil
role="unknown"
cwd=None
def stage(name):
    with open(STAGE,"w") as file:json.dump({"stage":name,"role":role},file)
stage("started")
role=json.loads(os.environ["GRAPHIT_ROLE_REFERENCE"])["role"]
os.environ["HOME"]=HOME
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
    stage(method)
    if method=="initialize":
        send({"jsonrpc":"2.0","id":ident,"result":{"protocolVersion":1,"agentCapabilities":{}}})
    elif method=="session/new":
        cwd=params["cwd"]
        assert sys.argv[1:]==["acp","--agent-engine=v3"]
        assert params["_meta"]["kiro"]["modeId"]==BRAND+"-"+role
        send({"jsonrpc":"2.0","id":ident,"result":{"sessionId":"role-hooks-actual-transport"}})
    elif method=="session/prompt":
        assert all(os.environ.get(key) for key in ["GRAPHIT_ROLE_BEARER","GRAPHIT_ROLE_REFERENCE","GRAPHIT_ROLE_SELECTION"])
        if role=="scribe":
            initial={"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"role-mcp-denial","version":"1"}}}
            for case in ["invalid-bearer","bearer-only","reference-only","explicit-missing","version-pin","explicit-version-pin"]:
                private={key:value for key,value in os.environ.items() if not key.startswith("GRAPHIT_ROLE_")}
                argv=[EXE,"mcp","--stdio"]
                if case=="invalid-bearer":
                    private["GRAPHIT_ROLE_BEARER"]="invalid-role-credential"
                    private["GRAPHIT_ROLE_REFERENCE"]=os.environ["GRAPHIT_ROLE_REFERENCE"]
                elif case=="bearer-only":private["GRAPHIT_ROLE_BEARER"]=os.environ["GRAPHIT_ROLE_BEARER"]
                elif case=="reference-only":private["GRAPHIT_ROLE_REFERENCE"]=os.environ["GRAPHIT_ROLE_REFERENCE"]
                elif case in ["version-pin","explicit-version-pin"]:
                    private.update({key:value for key,value in os.environ.items() if key.startswith("GRAPHIT_ROLE_")})
                    argv=[EXE,"--project-version","foreign-role-version","mcp","--stdio"]
                    if case=="explicit-version-pin":argv.append("--role")
                else:argv.append("--role")
                rejected=subprocess.run(argv,cwd=cwd,env=private,input=json.dumps(initial)+"\n",capture_output=True,text=True,timeout=15)
                assert rejected.returncode!=0,case
                assert rejected.stdout=="",case
                assert "Root Source startup is unavailable" not in rejected.stderr,case
                if case in ["version-pin","explicit-version-pin"]:
                    assert "managed-role MCP cannot override its captured transport or project" in rejected.stderr,case
                for key in ["GRAPHIT_ROLE_BEARER","GRAPHIT_ROLE_REFERENCE"]:
                    for secret in [os.environ[key],private.get(key)]:
                        if secret:assert secret not in rejected.stdout and secret not in rejected.stderr,case
            with subprocess.Popen([EXE,"mcp","--stdio"],cwd=cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) as child:
                stage("mcp-initialize")
                assert "result" in request(child,"initialize",{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"role-hook-proof","version":"1"}},1)
                request(child,"notifications/initialized",{},None)
                stage("mcp-write")
                for number,required in [(1,True),(2,True),(3,False)]:
                    answer=request(child,"tools/call",{"name":BRAND+"_memory_insert","arguments":{"input":{"idempotency_key":"role-hooks-"+str(number),"title":"Hook PROJECT "+str(number),"body":"MANDATORY-b9fb literal {{ data }} "+str(number),"classification":"convention","mandatory":required,"important":False,"tags":[]}}},number+1)
                    assert "result" in answer and not answer["result"].get("isError",False)
                stage("mcp-close")
                child.stdin.close();assert child.wait(timeout=15)==0
                stage("mcp-joined")
        profile_path=os.path.join(cwd,".kiro","agents",BRAND+"-"+role+".md")
        with open(profile_path) as f:profile=f.read()
        assert "Standing context" not in profile
        assert EXE not in profile and os.path.dirname(STAGE) not in profile
        inspected=subprocess.run([INSPECTOR,"--profile-hook-commands",profile_path],capture_output=True,text=True,check=True)
        commands=json.loads(inspected.stdout)
        assert [entry["event"] for entry in commands]==["SessionStart","UserPromptSubmit"]
        for entry in commands:
            event=entry["event"];hook=entry["command"]
            argv=shlex.split(hook)
            assert argv[0]==PROGRAM and "--recipient" in argv and argv[argv.index("--recipient")+1]=="role"
            assert "--global-dir" not in argv and argv[argv.index("--project-dir")+1]=="."
            assert os.path.samefile(shutil.which(argv[0]),EXE)
            stage("hook-"+event)
            result=subprocess.run(["/bin/sh","-c",hook],cwd=cwd,input="",capture_output=True,text=True,timeout=20)
            assert result.returncode==0 and result.stderr==""
            if role=="tracker":assert result.stdout==""
            else:
                prefix="[Standing context]\nPROJECT mandatory Memory:\n"
                assert result.stdout.startswith(prefix)
                context=json.loads(result.stdout[len(prefix):])
                assert context["memory_state"]=="published" and len(context["memories"])==2
                assert {row["body"] for row in context["memories"]}=={"MANDATORY-b9fb literal {{ data }} 1","MANDATORY-b9fb literal {{ data }} 2"}
                assert all(row["mandatory"] for row in context["memories"])
                assert "task" not in context and "user" not in context
            for key in ["GRAPHIT_ROLE_BEARER","GRAPHIT_ROLE_REFERENCE","GRAPHIT_ROLE_SELECTION"]:
                missing=os.environ.copy();missing.pop(key)
                rejected=subprocess.run(argv,cwd=cwd,env=missing,input="",capture_output=True,text=True,timeout=20)
                assert rejected.returncode!=0 and rejected.stdout==""
                invalid=os.environ.copy();invalid[key]=""
                rejected=subprocess.run(argv,cwd=cwd,env=invalid,input="",capture_output=True,text=True,timeout=20)
                assert rejected.returncode!=0 and rejected.stdout==""
            coordinator=argv[:];at=coordinator.index("--recipient");del coordinator[at:at+2]
            rejected=subprocess.run(coordinator,cwd=cwd,input="",capture_output=True,text=True,timeout=20)
            assert rejected.returncode!=0 and rejected.stdout==""
        send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"role-hooks-actual-transport","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Role hooks verified"}}}})
        send({"jsonrpc":"2.0","id":ident,"result":{"stopReason":"end_turn"}})
"#;
    let file = bin.join("kiro-cli");
    fs::write(&file, format!("#!/usr/bin/env python3\n{prefix}{body}"))?;
    fs::set_permissions(file, fs::Permissions::from_mode(0o700))
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
        tokio::time::sleep(Duration::from_millis(150)).await;
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
            assert_eq!(daemon.stop().await?, 130);
        } else if mode == "eof" {
            drop(response);
            let ready = client
                .get(&url)
                .timeout(Duration::from_secs(15))
                .send()
                .await?;
            assert_eq!(ready.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);
            assert!(installation.root().join("daemon/mcp.key").exists());
            assert_eq!(daemon.stop().await?, 130);
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
