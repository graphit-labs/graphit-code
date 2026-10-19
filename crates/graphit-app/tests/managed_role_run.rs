//! Actual executable → daemon → native process → run-owned stdio MCP → Role HTTP.
#![cfg(unix)]
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
    os::unix::fs::{PermissionsExt, symlink},
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
        .env(
            installation
                .brand()
                .environment_variable("GLOBAL_DIR")
                .unwrap(),
            installation.root(),
        );
    if let Some(home) = std::env::var_os("HOME") {
        command.env("HOME", home);
    }
    command
}
fn child_fixture(bin: &Path) -> io::Result<()> {
    let script = r#"#!/usr/bin/env python3
import json,os,sys,subprocess,re,base64
cwd=None
role=json.loads(os.environ["GRAPHIT_ROLE_REFERENCE"])["role"]
stage="startup"
proxy_failure=None
source_failure=None
def send(message): print(json.dumps(message),flush=True)
def receive(child,ident):
    global proxy_failure
    for line in child.stdout:
        message=json.loads(line)
        if message.get("id")==ident: return message
    diagnostic=child.stderr.read()
    known=["Cannot start a runtime from within a runtime","Cannot drop a runtime in a context where blocking is not allowed","Cannot block the current thread from within a runtime","there is no reactor running","Role MCP requires the active daemon","Role catalogue request failed","Role catalogue transport failed to close","Role HTTP negotiation failed","Role stdio negotiation failed","permission denied","invalid input","No such file","connection refused"]
    site=re.search(r"panicked at .*[/\\]([^/\\\n]+:\d+:\d+)",diagnostic)
    proxy_failure={"exit":child.wait(),"category":next((value for value in known if value.lower() in diagnostic.lower()),"unclassified"),"panic_site":site.group(1) if site else None}
    raise RuntimeError("stdio MCP closed before its response")
def request(child,method,params,ident):
    message={"jsonrpc":"2.0","method":method,"params":params}
    if ident is not None: message["id"]=ident
    child.stdin.write(json.dumps(message)+"\n");child.stdin.flush()
    if ident is not None: return receive(child,ident)
for line in sys.stdin:
    message=json.loads(line);method=message.get("method");ident=message.get("id");params=message.get("params",{})
    stage=method
    if method=="initialize":
        send({"jsonrpc":"2.0","id":ident,"result":{"protocolVersion":1,"agentCapabilities":{}}})
    elif method=="session/new":
        cwd=params["cwd"]
        assert sys.argv[1:] == ["acp", "--agent-engine=v3"]
        assert params["_meta"]["kiro"]["modeId"].endswith("-"+role)
        send({"jsonrpc":"2.0","id":ident,"result":{"sessionId":"native-role"}})
    elif method=="session/prompt":
        text="".join(block["text"] for block in params["prompt"] if block["type"]=="text")
        if role=="tracker": assert text=="Read the actual source"
        else:
            prefix="[Standing context]\nPROJECT mandatory Memory:\n"
            assert text.startswith(prefix)
            payload,prompt=text[len(prefix):].split("\n\n",1)
            assert json.loads(payload)=={"project_version":"main","memory_state":"no_corpus","memories":[]}
            assert prompt=="Read the actual source"
        assert os.environ.get("GRAPHIT_ROLE_BEARER")
        assert os.environ.get("GRAPHIT_ROLE_REFERENCE")
        with subprocess.Popen(["graphit","mcp","--stdio","--role"],cwd=cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) as proxy:
            stage="stdio-initialize"
            initial=request(proxy,"initialize",{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"role-child","version":"1"}},1)
            assert "result" in initial
            request(proxy,"notifications/initialized",{},None)
            stage="stdio-source"
            answer=request(proxy,"tools/call",{"name":"graphit_ast_source","arguments":{"input":{"path":"raw"}}},2)
            source_failure={"is_error":answer.get("result",{}).get("isError",False),"rpc_error":answer.get("error",{}).get("code"),"tool_code":None,"category":None,"storage_kind":None,"io_kind":None}
            allowed=["authentication_or_permission_denied","project_required","invalid_path","source_operation_failed","source_index_failed","indexed_source_failed","source_inspection_failed","request_cancelled_or_expired","selection_unavailable","tool_not_available"]
            kinds=["Interrupted","TimedOut","NotFound","PermissionDenied","InvalidInput","InvalidData","Other"]
            categories=["invalid_scope","request_mismatch","invalid_locator","unexpected_entry","operation","storage","source"]
            storage_kinds=["schema_mismatch","sdk_dataset_not_found","sdk_version_not_found","sdk_schema_mismatch","sdk_schema","sdk_io_not_found","sdk_io","sdk_corrupt_file","sdk_invalid_input","sdk_not_found","sdk_ref_not_found","sdk_other","unsupported_commit_scheme","invalid_version","branch_state"]
            for content in answer.get("result",{}).get("content",[]):
                try: detail=json.loads(content.get("text",""))
                except (ValueError,TypeError):continue
                if isinstance(detail,dict):
                    if detail.get("code") in allowed:source_failure["tool_code"]=detail["code"]
                    if detail.get("category") in categories:source_failure["category"]=detail["category"]
                    if detail.get("storage_kind") in storage_kinds:source_failure["storage_kind"]=detail["storage_kind"]
                    if detail.get("io_kind") in kinds:source_failure["io_kind"]=detail["io_kind"]
            assert "result" in answer and not answer["result"].get("isError",False)
            assert "actual executable Role payload" in json.dumps(answer)
            if role=="tracker":
                stage="stdio-git-status"
                status=request(proxy,"tools/call",{"name":"graphit_git_status","arguments":{"input":{}}},3)
                assert not status["result"].get("isError",False)
                assert json.loads(status["result"]["content"][0]["text"])["object_format"]=="sha1"
                stage="stdio-git-diff"
                diff=request(proxy,"tools/call",{"name":"graphit_git_diff","arguments":{"input":{}}},4)
                assert not diff["result"].get("isError",False)
                patches=json.loads(diff["result"]["content"][0]["text"])["patches"]
                tracked=next(p for p in patches if base64.b64decode(p["path_base64"])==b"tracked")
                assert b"+bravo" in base64.b64decode(tracked["hunks_base64"])
            stage="stdio-close"
            proxy.stdin.close()
            assert proxy.wait(timeout=10)==0
        send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"native-role","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"actual executable Role payload"}}}})
        send({"jsonrpc":"2.0","id":ident,"result":{"stopReason":"end_turn"}})
"#;
    let path = bin.join("kiro-cli");
    let (prefix, body) = script.split_once("for line in sys.stdin:").unwrap();
    let body = format!("for line in sys.stdin:{body}");
    let guarded = format!(
        "{prefix}\ntry:\n{}\nexcept BaseException as error:\n    with open({},'w') as failure:\n        json.dump({{\"stage\":stage,\"class\":type(error).__name__,\"proxy\":proxy_failure,\"source\":source_failure}},failure)\n    sys.exit(1)\n",
        body.lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>(),
        serde_json::to_string(&bin.join("fixture-phase.json").to_str().unwrap()).unwrap()
    );
    fs::write(&path, guarded)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
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
