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
            case["installation"] = json!(self.installation.root());
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
    let script = r#"#!/usr/bin/env python3
import json,os,sys,subprocess,pathlib,tomllib,select,time,re
directory=pathlib.Path(__file__).parent
case=json.loads((directory/'case.json').read_text())
binding,role,model=case['binding'],case['role'],case['model']
stage='startup'
answer='actual executable Role payload'
prompt='Read the actual source'
def send(value): print(json.dumps(value),flush=True)
def receive(child,ident):
    while select.select([child.stdout],[],[],10)[0]:
        line=child.stdout.readline()
        if not line: raise EOFError('Role MCP closed')
        value=json.loads(line)
        if value.get('id')==ident: return value
    raise TimeoutError('Role MCP did not respond')
def request(child,method,params,ident):
    value={'jsonrpc':'2.0','method':method,'params':params}
    if ident is not None: value['id']=ident
    child.stdin.write(json.dumps(value)+'\n');child.stdin.flush()
    if ident is not None: return receive(child,ident)
def proxy(environment,denied=False):
    argv=['graphit','mcp','--stdio']
    cwd=str(pathlib.Path.cwd())
    child_environment=dict(environment)
    if binding=='opencode':
        server=json.loads(pathlib.Path(case['mcp_config']).read_text())
        for key in case['mcp_entry']: server=server[key]
        assert server['type']=='local'
        argv=server['command']
        assert pathlib.Path(argv[0]).resolve()==pathlib.Path(case['graphit_executable']).resolve()
        assert argv[1:]==['--global-dir',case['installation'],'mcp','--stdio']
        configured_environment=server.get('environment',{})
        assert not any(key.startswith('GRAPHIT_ROLE_') for key in configured_environment)
        child_environment.update(configured_environment)
        cwd=server.get('cwd',cwd)
        assert pathlib.Path(cwd).resolve()==pathlib.Path.cwd()
    with subprocess.Popen(argv,cwd=cwd,env=child_environment,
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) as child:
        try:
            initial=request(child,'initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'native-role-peer','version':'1'}},1)
            if denied and 'error' in initial:
                child.stdin.close();child.wait(timeout=10);return
            assert 'result' in initial
            request(child,'notifications/initialized',{},None)
            value=request(child,'tools/call',{'name':'graphit_ast_source','arguments':{'input':{'path':'raw'}}},2)
            rejected='error' in value or value.get('result',{}).get('isError',False)
            if denied:
                assert rejected
                assert answer not in json.dumps(value)
            else:
                assert not rejected and answer in json.dumps(value)
                if role=='tracker':
                    git=request(child,'tools/call',{'name':'graphit_git_status','arguments':{'input':{}}},3)
                    assert not git['result'].get('isError',False)
                    assert json.loads(git['result']['content'][0]['text'])['object_format']=='sha1'
            if not denied and role=='scout':
                shape=request(child,'tools/call',{'name':'graphit_hub_type_path','arguments':{'input':{'artifact_type':'agent','name':'observer'}}},4)
                assert not shape['result'].get('isError',False)
                metadata=json.loads(shape['result']['content'][0]['text'])
                assert metadata['binding']==binding
                assert metadata['target']=={'kind':'native_binding','binding':binding}
            child.stdin.close()
            assert child.wait(timeout=10)==0 or denied
        except (BrokenPipeError,EOFError):
            if not denied: raise
            assert child.wait(timeout=10)!=0
        finally:
            if child.poll() is None:
                child.kill();child.wait(timeout=10)
def role_mcp():
    global stage
    stage='role-authority'
    assert os.environ['GRAPHIT_ROLE_BINDING']==binding
    assert json.loads(os.environ['GRAPHIT_ROLE_REFERENCE'])['role']==role
    assert os.environ['GRAPHIT_ROLE_BEARER']
    for wrong in ['not-a-binding','kiro-v3']:
        altered=dict(os.environ,GRAPHIT_ROLE_BINDING=wrong)
        proxy(altered,True)
    proxy(os.environ)
    (directory/'proof.json').write_text(json.dumps({'binding':binding,'model':model,'role_mcp':True,'tampered_binding_denied':True,'configured_mcp':binding=='opencode'}))
def validate_prompt(text):
    if role=='tracker': assert text==prompt
    else:
        prefix='[Standing context]\nPROJECT mandatory Memory:\n'
        assert text.startswith(prefix)
        memory,body=text[len(prefix):].split('\n\n',1)
        assert json.loads(memory)=={'project_version':'main','memory_state':'no_corpus','memories':[]}
        assert body==prompt
def native_profile(name):
    global stage
    stage='profile-name'
    assert name.endswith('-'+role)
    stage='profile-path'
    path=pathlib.Path.cwd()/case['agent_directory']/(name+'.md')
    assert path.is_file()
    body=path.read_text()
    stage='profile-body'
    assert role in body
    assert os.environ['GRAPHIT_ROLE_BEARER'] not in body
    assert os.environ['GRAPHIT_ROLE_REFERENCE'] not in body
    return body
def cli():
    global stage
    stage='native-arguments'
    args=sys.argv[1:]
    expected_model=[] if model is None else ['--model',model]
    if binding=='codex':
        assert args[:3]==['exec','--json','--skip-git-repo-check'] and args[-1]=='-'
        settings={}
        rest=args[3:-1]
        while rest and rest[0]=='-c':
            settings.update(tomllib.loads(rest[1]));rest=rest[2:]
        assert rest==expected_model
        assert role in settings['developer_instructions']
        assert settings['features']['shell_tool'] is False
        assert 'sandbox_mode' not in settings
        text=sys.stdin.read()
    elif binding=='claude':
        prefix=['--print','--output-format','stream-json','--verbose','--include-partial-messages','--agent']
        assert args[:len(prefix)]==prefix
        native_profile(args[len(prefix)])
        assert args[len(prefix)+1:]==expected_model
        text=sys.stdin.read()
    elif binding=='cursor':
        prefix=['--print','--output-format','stream-json']+expected_model
        assert args[:-1]==prefix
        assert '--mode' not in args and '--agent' not in args
        assert args[-1].endswith(prompt)
        body,text=args[-1][:-len(prompt)],prompt
        assert role in body
    elif binding=='qwen':
        prefix=['--input-format','text','--output-format','stream-json','--include-partial-messages','--system-prompt']
        assert args[:len(prefix)]==prefix
        assert role in args[len(prefix)] and args[len(prefix)+1:]==expected_model
        text=sys.stdin.read()
    elif binding=='antigravity':
        prefix=['--print','--input-format','stream-json','--output-format','stream-json','--agent']
        assert args[:len(prefix)]==prefix
        native_profile(args[len(prefix)])
        assert args[len(prefix)+1:]==expected_model
        record=json.loads(sys.stdin.read())
        assert record['event']=='user'
        text=record['message']['content']
    elif binding=='kimi':
        assert args[0]=='--agent-file'
        profile=pathlib.Path(args[1])
        assert profile.is_absolute() and profile.is_file()
        assert profile.parent==pathlib.Path.cwd()/'.kimi-code'/'agents'
        assert profile.suffix=='.md'
        body=native_profile(profile.stem)
        assert body.startswith('---\n')
        assert all(selector in body for selector in ['Read','Grep','Glob','mcp__'])
        kimi_model=[] if model is None else ['-m',model]
        assert args[2:-1]==['--output-format','stream-json']+kimi_model+['-p']
        assert sys.stdin.read()==''
        text=args[-1]
        (directory/'native.pid').write_text(str(os.getpid()))
    else: raise AssertionError('unexpected CLI binding')
    stage='prompt';validate_prompt(text);role_mcp()
    if case.get('eof'): return
    if case.get('hold'):
        (directory/'holding.pid').write_text(str(os.getpid()))
        while True: time.sleep(1)
    observed=model or 'observed'
    if binding=='codex':
        send({'type':'turn.started'})
        send({'type':'item.completed','item':{'id':'native-role','type':'agent_message','text':answer}})
        send({'type':'turn.completed','usage':{'input_tokens':8,'cached_input_tokens':2,'output_tokens':3,'reasoning_output_tokens':1}})
    elif binding=='antigravity':
        send({'event':'init','conversation_id':'native-role','init':{'model':observed}})
        send({'event':'result','result':{'conversation_id':'native-role','status':'SUCCESS','response':answer}})
    elif binding=='kimi':
        send({'role':'tool','tool_call_id':'native-source','content':'excluded tool answer'})
        send({'role':'assistant','content':'actual executable '})
        send({'role':'assistant','content':'Role payload'})
        # The official stream has no success terminal; exit and both EOFs matter.
    else:
        send({'type':'system','subtype':'init','uuid':'native-role','session_id':'native-role','model':observed})
        result={'type':'result','subtype':'success','is_error':False,'duration_ms':8,'duration_api_ms':5,'num_turns':1,'session_id':'native-role','result':answer}
        if binding=='qwen': result.update(uuid='end',usage={'input_tokens':5,'output_tokens':3,'cache_read_input_tokens':2,'total_tokens':10},modelUsage={observed:{'inputTokens':5,'outputTokens':3,'cacheReadInputTokens':2,'cacheCreationInputTokens':0,'webSearchRequests':0,'contextWindow':128}},permission_denials=[])
        if binding=='claude': result.update(total_cost_usd=0.0001,usage={'input_tokens':5,'output_tokens':3,'cache_read_input_tokens':2},modelUsage={observed:{'inputTokens':5,'outputTokens':3}})
        if binding=='cursor': result['request_id']='native-request'
        send(result)
def acp():
    global stage
    stage='opencode-argv'
    assert sys.argv[1:]==['acp']
    stage='opencode-profile-path'
    paths=list(pathlib.Path.cwd().glob('.opencode/agents/*-'+role+'.md'))
    assert len(paths)==1
    name=paths[0].stem
    body=native_profile(name)
    stage='opencode-profile-mode'
    frontmatter=body.split('---\n',2)[1]
    assert re.search(r'^mode:\s*(?:all|\'all\'|"all")\s*$',frontmatter,re.MULTILINE)
    stage='opencode-profile-permissions'
    server=''.join(c if c.isascii() and (c.isalnum() or c in '_-') else '_' for c in case['mcp_server'])
    assert 'permissions:' in frontmatter and 'execute' in frontmatter and server+'_*' in frontmatter
    selected={'mode':False,'model':False}
    values={'mode':'build','model':'provider/default'}
    def options():
        return [{'id':key,'name':key.title(),'category':key,'type':'select','currentValue':values[key],'options':[{'value':value,'name':value}]} for key,value in [('mode',name),('model',model)]]
    for line in sys.stdin:
        message=json.loads(line);method=message.get('method');ident=message.get('id');params=message.get('params',{})
        stage=method
        if method=='initialize':
            send({'jsonrpc':'2.0','id':ident,'result':{'protocolVersion':1,'agentInfo':{'name':'OpenCode','version':'2.0.0'},'agentCapabilities':{'mcpCapabilities':{'http':True,'sse':False}}}})
        elif method=='session/new':
            assert pathlib.Path(params['cwd'])==pathlib.Path.cwd()
            send({'jsonrpc':'2.0','id':ident,'result':{'sessionId':'native-role','configOptions':options()}})
        elif method=='session/set_config_option':
            key=params['configId'];assert params['value']==({'mode':name,'model':model}[key])
            values[key]=params['value'];selected[key]=True
            send({'jsonrpc':'2.0','id':ident,'result':{'configOptions':options()}})
        elif method=='session/prompt':
            assert selected=={'mode':True,'model':True}
            validate_prompt(''.join(item['text'] for item in params['prompt'] if item['type']=='text'))
            role_mcp()
            send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'native-role','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':answer}}}})
            send({'jsonrpc':'2.0','id':ident,'result':{'stopReason':'end_turn'}})
try:
    if binding=='opencode': acp()
    else: cli()
except BaseException as error:
    (directory/'phase.json').write_text(json.dumps({'stage':stage,'class':type(error).__name__}))
    sys.exit(1)
"#;
    for name in [
        "codex",
        "claude",
        "cursor-agent",
        "qwen",
        "agy",
        "opencode",
        "kimi",
    ] {
        let target = bin.join(name);
        fs::write(&target, script)?;
        fs::set_permissions(target, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
