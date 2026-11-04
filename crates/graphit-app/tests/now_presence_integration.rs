#![cfg(target_os = "linux")]
//! Compiled Runtime consumers. Broker journeys use its public native fixture.
use graphit_app::foreground_source::ForegroundSource;
use graphit_auth::{
    profile::{
        AiService, BrokerService, LocalCredentials, OidcTokens, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    config::ConfigSnapshot,
    file_lock::Cancellation,
    installation_identity::InstallationIdentityStore,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::project_bootstrap::ProjectBootstrapInput;
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, ResourceSetting},
};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use secrecy::SecretString;
use serde_json::{Value, json};
use std::{
    fs, io,
    io::Read,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const WAIT: Duration = Duration::from_secs(60);
// External fixture control includes Docker/PostgreSQL startup and teardown.
// Product requests, leases and Runtime/CLI waits keep their existing limits.
const BROKER_CONTROL_WAIT: Duration = Duration::from_secs(300);

// Diagnostics remain bounded and never retain fixture/runtime bearer values.
fn sanitized_diagnostic(text: &str) -> String {
    let mut output = String::new();
    let mut start = 0;
    for (offset, character) in text
        .char_indices()
        .chain(std::iter::once((text.len(), '\0')))
    {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
            continue;
        }
        let token = &text[start..offset];
        output.push_str(
            if token.len() >= 32 || matches!(token, "fixture-access" | "fixture-service") {
                "<redacted>"
            } else {
                token
            },
        );
        if offset < text.len() {
            output.push(character);
        }
        start = offset + character.len_utf8();
    }
    output
}

struct SeededTasks {
    ids: Vec<String>,
    target: Value,
}

#[derive(Clone, Copy)]
enum BrokerMode {
    Native,
    Local,
}

struct Installation {
    temporary: tempfile::TempDir,
    paths: InstallationPaths,
    project: ProjectPaths,
    project_id: ProjectId,
    version: String,
}
impl Installation {
    fn new(name: &str, version: &str) -> Result<Self> {
        let temporary = tempfile::tempdir()?;
        let paths = InstallationPaths::resolve(
            Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?,
            temporary.path(),
            None,
            Some(&temporary.path().join("installation")),
        )?;
        let root = temporary.path().join("project");
        fs::create_dir(&root)?;
        // The stdio proxy maintains the actual checkout before selecting a
        // requested view. Seed the branch that the runtime is meant to serve.
        let git = |args: &[&str]| -> io::Result<()> {
            let output = Command::new("git")
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .env("HOME", temporary.path())
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .current_dir(&root)
                .args(args)
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other(format!(
                    "Git fixture {args:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            Ok(())
        };
        git(&["init", "-b", "main", "--template="])?;
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ])?;
        git(&["switch", "-c", version])?;
        let project = ProjectPaths::at(&root, &paths)?;
        let project_id = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &Cancellation::default())?
            .ensure(&ProjectMetadata::new(name)?)?
            .id();
        Ok(Self {
            temporary,
            paths,
            project,
            project_id,
            version: version.into(),
        })
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.temporary.path())
            .current_dir(self.project.root())
            .arg("--global-dir")
            .arg(self.paths.root())
            .args([
                "--project-version",
                &self.version,
                "-c",
                "agent=codex",
                "-c",
                "modules.ast=false",
                "-c",
                "broker.allow_loopback_http=true",
            ]);
        command
    }
    async fn daemon(&self) -> Result<Runtime> {
        let log = self.temporary.path().join("runtime.log");
        let child = self
            .command()
            .args(["daemon", "foreground"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log)?)
            .spawn()?;
        let mut runtime = Runtime { child, log };
        let until = Instant::now() + WAIT;
        loop {
            if let Some(status) = runtime.child.try_wait()? {
                return Err(format!(
                    "Runtime startup exited {status}: {}",
                    fs::read_to_string(&runtime.log)?
                )
                .into());
            }
            if self.paths.root().join("daemon/mcp.json").exists() {
                break;
            }
            if Instant::now() >= until {
                return Err("Runtime discovery not ready".into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok(runtime)
    }
    async fn call(&self, name: &str, mut arguments: Value) -> Result<Value> {
        if name.starts_with("graphit_task_") {
            arguments = json!({"selection":{"project_id":self.project_id,"project_version":self.version},"input":arguments});
        }
        let Value::Object(mut arguments) = arguments else {
            return Err("MCP arguments must be an object".into());
        };
        arguments.insert("ai_optimized".into(), Value::Bool(false));
        let mut command = tokio::process::Command::from(self.command());
        let diagnostic = self.temporary.path().join("mcp-stderr.log");
        let mut child = command
            .args(["mcp", "--stdio", "--non-interactive"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&diagnostic)?)
            .kill_on_drop(true)
            .spawn()?;
        let initialized = ()
            .serve((
                child.stdout.take().ok_or("MCP stdout missing")?,
                child.stdin.take().ok_or("MCP stdin missing")?,
            ))
            .await;
        let sdk = match initialized {
            Ok(sdk) => sdk,
            Err(error) => {
                let status = tokio::time::timeout(WAIT, child.wait()).await;
                let cleanup = if matches!(&status, Ok(Ok(_))) {
                    None
                } else {
                    let requested = child.start_kill();
                    Some((requested, child.wait().await))
                };
                let mut diagnostic_text = String::new();
                let _ = fs::File::open(&diagnostic)?
                    .take(8192)
                    .read_to_string(&mut diagnostic_text);
                let diagnostic_text =
                    diagnostic_text.replace(self.temporary.path().to_str().unwrap(), "<temporary>");
                return Err(format!(
                    "MCP {name} initialize: {error}; child: {status:?}; cleanup: {cleanup:?}; stderr: {}",
                    sanitized_diagnostic(&diagnostic_text)
                )
                .into());
            }
        };
        let result = tokio::time::timeout(
            WAIT,
            sdk.peer()
                .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments)),
        )
        .await
        .map_err(|e| format!("MCP {name} response: {e}"))?;
        let failed_schema = if result.as_ref().is_ok_and(|r| r.is_error == Some(true)) {
            sdk.peer()
                .list_all_tools()
                .await?
                .into_iter()
                .find(|tool| tool.name == name)
                .map(|tool| tool.input_schema)
        } else {
            None
        };
        sdk.cancel().await?;
        let status = tokio::time::timeout(WAIT, child.wait()).await??;
        assert!(status.success(), "MCP subprocess did not join cleanly");
        let result = result?;
        if result.is_error == Some(true) {
            let cause: String = result
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .map(|c| c.text.as_str())
                .collect();
            return Err(format!(
                "MCP operation failed: {name}, schema {failed_schema:?}: {}",
                cause.replace(self.temporary.path().to_str().unwrap(), "<temporary>")
            )
            .into());
        }
        let text: String = result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|c| c.text.as_str())
            .collect();
        Ok(serde_json::from_str(&text)?)
    }
    async fn seed_tasks(&self) -> Result<SeededTasks> {
        let session = self.call("graphit_task_session_create", json!({
            "title":"Private integration session", "description":"PRIVATE_SESSION_BODY",
            "strategy":"Exercise actual native records", "idempotency_key":"now-presence-session",
        })).await?;
        let session_id = session["result"]["id"]
            .as_str()
            .ok_or("session id missing")?;
        let mut ids = Vec::new();
        let mut target = None;
        for (key, title) in [("first", "Café α"), ("second", "Café β")] {
            let task = self
                .call(
                    "graphit_task_create",
                    json!({
                        "session_id":session_id, "title":title,"description":"PRIVATE_TASK_BODY",
                        "idempotency_key":key,"task_type":"task","priority":2,"checks":[
                            {"kind":"acceptance","text":"Observe this real native head"},
                            {"kind":"test","text":"Read the head through the Runtime Now consumer"}
                        ],
                    }),
                )
                .await?;
            let id = task["result"]["specification"]["id"]
                .as_str()
                .ok_or("task id missing")?
                .to_owned();
            let claim = self
                .call("graphit_task_claim", json!({"id":id,"lease_ms":600000}))
                .await?;
            if ids.is_empty() {
                target = Some(
                    json!({"id":id,"claim":{"token":claim["result"]["token"],"expected_revision":claim["result"]["record"]["specification"]["revision"]}}),
                );
            }
            ids.push(id);
        }
        Ok(SeededTasks {
            ids,
            target: target.ok_or("active claim missing")?,
        })
    }
    async fn local_now(&self) -> Result<Value> {
        self.call(
            "graphit_now",
            json!({"mode":"local","project_id":self.project_id,"version":self.version}),
        )
        .await
    }
    fn instance_id(&self) -> Result<String> {
        Ok(InstallationIdentityStore::new(&self.paths)?
            .load()?
            .unit_id()
            .ok_or("Runtime installation id missing")?
            .as_str()
            .into())
    }
    fn cleaned(&self) {
        assert!(!self.paths.root().join("daemon/mcp.json").exists());
        assert!(!self.paths.root().join("daemon/mcp.key").exists());
    }
}
struct Runtime {
    child: Child,
    log: std::path::PathBuf,
}
impl Runtime {
    async fn stop(&mut self, signal: &str) -> Result {
        let status = Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()?;
        if !status.success() {
            return Err(io::Error::other("Runtime signal delivery failed").into());
        }
        let until = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait()? {
                assert_eq!(
                    status.code(),
                    Some(130),
                    "Runtime failed: {}",
                    fs::read_to_string(&self.log)?
                );
                return Ok(());
            }
            if Instant::now() >= until {
                return Err("Runtime did not join after signal".into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Installation {
    async fn prepare(&self) -> Result {
        let config = ConfigSnapshot::new(self.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel)?;
        let owner = ForegroundSource::start(
            &self.paths,
            &[ProjectBootstrapInput {
                project: &self.project,
                explicit_version: &self.version,
                pin: None,
                detached_base: "",
            }],
            0,
            &config,
            NativeBinding::Codex,
            &original,
        )?;
        owner.shutdown().await?;
        self.cleaned();
        Ok(())
    }
    fn broker_profile(&self, broker: &Broker) -> Result {
        let config = ConfigSnapshot::new(self.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel)?;
        let number = |setting| usize::try_from(policy.limit(setting).value);
        let bytes = number(ResourceSetting::ContentInputBytes)?;
        let scalar = number(ResourceSetting::ContentScalarBytes)?;
        let depth = number(ResourceSetting::SnapshotDepth)?;
        let entries = number(ResourceSetting::ContentCollectionEntries)?;
        let identity = InstallationIdentityStore::new(&self.paths)?.load()?;
        let unit = identity.unit_id().ok_or("installation identity missing")?;
        let scope = CapturedPublicationScope::installation(
            &self.paths,
            unit.clone(),
            [self.paths.root().to_owned()],
            ScopeLimits::new(4, graphit_core::native_path::MAX_PATH_BYTES, depth * 4)?,
            original.snapshot(),
        )?;
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(
                    bytes,
                    graphit_core::native_path::MAX_PATH_BYTES,
                    depth,
                    scalar,
                )?,
                original.snapshot(),
            )?,
            ProfileLimits {
                bytes,
                nodes: number(ResourceSetting::ContentNodes)?,
                scalar_bytes: scalar,
                collection_entries: entries,
                providers: entries,
                profiles: entries,
            },
            &original,
        )?;
        let endpoint = broker.text("endpoint")?;
        let access = broker.text("access_bearer")?;
        let service = BrokerService {
            endpoint: endpoint.into(),
            mcp_resource: match broker.mode {
                BrokerMode::Native => Some(broker.text("resource")?.into()),
                BrokerMode::Local => None,
            },
            allow_anonymous: false,
        };
        let (kind, credentials, username, s3_source) = match broker.mode {
            BrokerMode::Native => (
                ProviderKind::Broker(service),
                ProfileCredentials::Broker {
                    issuer: broker.text("issuer")?.into(),
                    subject: broker.text("subject")?.into(),
                    tokens: OidcTokens {
                        access_token: Secret::new(SecretString::from(access)),
                        refresh_token: None,
                        id_token: Secret::new(SecretString::from("fixture-untrusted-id-data")),
                        expires_at: i64::try_from(
                            broker.ready["expires_at"]
                                .as_u64()
                                .ok_or("Native expiry missing")?,
                        )?,
                    },
                    s3_disabled: true,
                },
                broker.text("subject")?,
                S3Source::BrokerScope,
            ),
            BrokerMode::Local => (
                ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: Some(service),
                },
                ProfileCredentials::Local(LocalCredentials {
                    anonymous: false,
                    mcp_key: None,
                    broker_key: Some(Secret::new(SecretString::from(access))),
                    embedding_api_key: None,
                    rerank_api_key: None,
                    s3: None,
                }),
                broker.text("username")?,
                S3Source::LocalCredentials,
            ),
        };
        let outcome = store.mutate(store.load(&original)?, &original, |doc, ctx| {
            doc.add_provider(
                "inventory".into(),
                ProviderConfig {
                    kind,
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: s3_source,
                    },
                },
                doc.revision(),
                1,
                ctx,
            )?;
            doc.replace_profile_data(
                ProfileData {
                    name: "fixture".into(),
                    provider: "inventory".into(),
                    username: username.into(),
                    organization: None,
                    teams: vec![],
                    credentials,
                },
                ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc
                        .provider("inventory")
                        .expect("created provider")
                        .revision(),
                    profile_revision: None,
                },
                1,
                ctx,
            )?;
            doc.activate("fixture", doc.revision(), ctx)?;
            Ok(())
        })?;
        assert!(
            outcome.failure().is_none(),
            "private Auth publication failed"
        );
        Ok(())
    }
    async fn revise_task(&self, seeded: &SeededTasks, title: &str) -> Result {
        let response = self.call("graphit_task_revise", json!({
            "target":seeded.target,"lease_ms":600000,"reason":"Observe a fresh real publication","title":title,
        })).await?;
        assert_eq!(response["kind"], "task");
        Ok(())
    }
}

fn fixture_diagnostic(line: &str) -> bool {
    let fields: Vec<_> = line.split_ascii_whitespace().collect();
    if fields.len() < 3 || fields[0] != "broker-fixture" {
        return false;
    }
    if !matches!(
        fields[1],
        "phase=docker-create"
            | "phase=docker-start"
            | "phase=docker-copy-tls"
            | "phase=docker-configure-tls"
            | "phase=docker-mapped-port"
            | "phase=docker-control"
            | "phase=postgres-fixture"
            | "phase=postgres-startup"
    ) {
        return false;
    }
    match fields[2] {
        "event=begin" => fields.len() == 3,
        "event=end" if fields.len() == 5 => {
            matches!(
                fields[3],
                "status=success" | "status=failure" | "status=spawn-or-wait-failure"
            ) && fields[4].strip_prefix("elapsed_ms=").is_some_and(|value| {
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
            })
        }
        _ => false,
    }
}

struct Broker {
    child: tokio::process::Child,
    input: Option<tokio::process::ChildStdin>,
    output: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    stderr: Option<tokio::task::JoinHandle<io::Result<Vec<String>>>>,
    ready: Value,
    mode: BrokerMode,
}
impl Broker {
    async fn start(mode: BrokerMode, first: &Installation, second: &Installation) -> Result<Self> {
        let binary = std::env::var_os(match mode {
            BrokerMode::Native => "GRAPHIT_BROKER_NATIVE_INVENTORY_FIXTURE_BIN",
            BrokerMode::Local => "GRAPHIT_BROKER_INVENTORY_FIXTURE_BIN",
        })
        .ok_or("public Broker fixture executable input missing")?;
        let mut child = tokio::process::Command::new(binary)
            .env("GRAPHIT_BROKER_FIXTURE_DIAGNOSTICS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stderr = child.stderr.take().ok_or("Broker stderr missing")?;
        let stderr = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut diagnostics = Vec::new();
            while let Some(line) = lines.next_line().await? {
                if diagnostics.len() < 32 && line.len() <= 160 && fixture_diagnostic(&line) {
                    diagnostics.push(line);
                }
            }
            Ok(diagnostics)
        });
        let mut broker = Self {
            input: child.stdin.take(),
            output: BufReader::new(child.stdout.take().ok_or("Broker stdout missing")?).lines(),
            child,
            stderr: Some(stderr),
            ready: Value::Null,
            mode,
        };
        let mut start = match mode {
            BrokerMode::Local => {
                // The public Local fixture derives Task grants from these real
                // captured project/version scopes. Each Runtime still publishes
                // its own current inventory through the production registrar.
                json!({"command":"start","instance_id":first.instance_id()?,"projects":[
                    {"project_id":first.project_id,"versions":[first.version]},
                    {"project_id":second.project_id,"versions":[second.version]},
                ]})
            }
            BrokerMode::Native => json!({
                "command":"start","api_audience":"graphit-code-native-inventory-fixture",
                "resource":"https://resource.example.test/runtime/inventory","client_id":"graphit-code-runtime-fixture",
                "subject":"runtime-native-foreground-fixture","scope":"openid presence:read presence:publish",
                "redirect_uri_path":"/callback","access_ttl_seconds":600,"instance_id":first.instance_id()?,
                "projects":[],"seed_inventory":false,"grant_project_ids":["*"],
                "read_required_scopes":["presence:read"],"publish_required_scopes":["presence:publish"],
                "token_bytes":65536,"grants_json_bytes":65536,"inventory_json_bytes":65536,"body_bytes":65536,
                "request_timeout_ms":30000,
            }),
        };
        start["realtime"] = json!({
            "heartbeat_timeout_ms":60000,"broadcast_capacity":32,"required_scopes":["presence:read"],
            "command_bytes":65536,"outbound_bytes":65536,"operation_timeout_ms":30000,"task_read":true,
        });
        let started = async {
            broker.send(&start).await?;
            broker.receive("Start Ready").await
        }
        .await;
        broker.ready = match started {
            Ok(ready) => ready,
            Err(error) => {
                broker.input.take();
                let requested = broker.child.start_kill();
                let joined = broker.child.wait().await;
                let diagnostics = broker.diagnostics().await;
                return Err(format!("{error}; Broker startup cleanup kill={requested:?} join={joined:?}; diagnostics={diagnostics:?}").into());
            }
        };
        assert_eq!(broker.ready["event"], "ready");
        assert_eq!(broker.ready["instance_id"], first.instance_id()?);
        Ok(broker)
    }
    fn text(&self, field: &str) -> Result<&str> {
        self.ready[field]
            .as_str()
            .ok_or_else(|| format!("public Broker Ready missing {field}").into())
    }
    async fn send(&mut self, value: &Value) -> Result {
        let input = self.input.as_mut().ok_or("Broker input closed")?;
        input.write_all(&serde_json::to_vec(value)?).await?;
        input.write_all(b"\n").await?;
        input.flush().await?;
        Ok(())
    }
    async fn receive(&mut self, phase: &str) -> Result<Value> {
        let line = tokio::time::timeout(BROKER_CONTROL_WAIT, self.output.next_line())
            .await
            .map_err(|e| format!("Broker {phase}: {e}"))??
            .ok_or_else(|| format!("Broker {phase}: response EOF"))?;
        Ok(serde_json::from_str(&line)?)
    }
    async fn diagnostics(&mut self) -> Result<Vec<String>> {
        match self.stderr.take() {
            Some(stderr) => Ok(stderr.await??),
            None => Ok(Vec::new()),
        }
    }
    async fn stop(&mut self) -> Result {
        self.send(&json!({"command":"shutdown"})).await?;
        let ack = self.receive("Shutdown ACK").await?;
        assert_eq!(ack, json!({"event":"ack","command":"shutdown","ok":true}));
        self.input.take();
        assert!(
            tokio::time::timeout(BROKER_CONTROL_WAIT, self.child.wait())
                .await??
                .success()
        );
        for line in self.diagnostics().await? {
            eprintln!("{line}");
        }
        Ok(())
    }
}

struct NowPipe {
    child: tokio::process::Child,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    stderr: Option<tokio::task::JoinHandle<io::Result<Vec<String>>>>,
}
impl NowPipe {
    fn start(observer: &Installation, selected: &Installation) -> Result<Self> {
        let mut command = tokio::process::Command::from(observer.command());
        let mut child = command
            .args([
                "now",
                "--project",
                &selected.project_id.to_string(),
                "--provider",
                "inventory",
                "--version",
                &selected.version,
                "--instance",
                &selected.instance_id()?,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stderr = child.stderr.take().ok_or("Now stderr missing")?;
        let stderr = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut diagnostics = Vec::new();
            while let Some(line) = lines.next_line().await? {
                if diagnostics.len() < 32 {
                    diagnostics.push(sanitized_diagnostic(&line).chars().take(320).collect());
                }
            }
            Ok(diagnostics)
        });
        Ok(Self {
            lines: BufReader::new(child.stdout.take().ok_or("Now stdout missing")?).lines(),
            child,
            stderr: Some(stderr),
        })
    }
    async fn frame(&mut self) -> Result<Value> {
        let line = tokio::time::timeout(WAIT, self.lines.next_line())
            .await
            .map_err(|e| format!("Now stream frame: {e}"))??;
        let Some(line) = line else {
            let status = tokio::time::timeout(WAIT, self.child.wait()).await??;
            let diagnostics = self
                .stderr
                .take()
                .ok_or("Now stderr owner missing")?
                .await??;
            return Err(format!(
                "Now stream EOF; status {status}; stderr: {}",
                diagnostics.join("; ")
            )
            .into());
        };
        assert!(!line.contains('\u{001b}'), "pipe emitted terminal control");
        Ok(serde_json::from_str(&line)?)
    }
    async fn task(
        &mut self,
        selected: &Installation,
        ids: &[String],
        title: &str,
    ) -> Result<Value> {
        let until = Instant::now() + WAIT;
        loop {
            let frame = self.frame().await?;
            let instances = frame["instances"]
                .as_array()
                .ok_or("Now instances missing")?;
            if let Some(instance) = instances.first()
                && instance["now"]["sections"]["tasks"]["items"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item["title"] == title))
            {
                remote_heads(&frame, selected, ids);
                return Ok(frame);
            }
            if Instant::now() >= until {
                return Err("fresh Task head not published".into());
            }
        }
    }
    async fn empty(&mut self) -> Result {
        let until = Instant::now() + WAIT;
        loop {
            if self.frame().await?["instances"]
                .as_array()
                .is_some_and(Vec::is_empty)
            {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err("retired Runtime channel remained visible".into());
            }
        }
    }
    async fn stop(&mut self) -> Result {
        let pid = self.child.id().ok_or("Now process already exited")?;
        assert!(
            tokio::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status()
                .await?
                .success()
        );
        let status = tokio::time::timeout(WAIT, self.child.wait()).await??;
        // The same child has exited: retain its drain until actual EOF, also
        // when its status does not match the expected interruption.
        let diagnostics = self
            .stderr
            .take()
            .ok_or("Now stderr owner missing")?
            .await??;
        assert_eq!(status.code(), Some(130), "{}", diagnostics.join("; "));
        Ok(())
    }
}

fn remote_heads(frame: &Value, selected: &Installation, ids: &[String]) {
    let instances = frame["instances"]
        .as_array()
        .expect("full replacement instances");
    assert_eq!(instances.len(), 1);
    let instance = &instances[0];
    assert_eq!(instance["instance_id"], selected.instance_id().unwrap());
    assert_eq!(instance["version"], selected.version);
    let now = &instance["now"];
    private_heads(now, selected, ids);
    for section in ["memories", "knowledge"] {
        assert_eq!(now["sections"][section]["availability"], "failed");
        assert_eq!(now["sections"][section]["error"], "forbidden");
        assert_eq!(now["sections"][section]["items"], json!([]));
        assert!(now["sections"][section]["total"].is_null());
        assert!(now["sections"][section]["has_more"].is_null());
    }
}

async fn broker_journey(mode: BrokerMode) -> Result {
    let a = Installation::new("broker-integration-a", "feature/ação")?;
    let b = Installation::new("broker-integration-b", "release/β")?;
    a.prepare()
        .await
        .map_err(|e| format!("Installation A preparation: {e}"))?;
    b.prepare()
        .await
        .map_err(|e| format!("Installation B preparation: {e}"))?;
    assert_ne!(a.instance_id()?, b.instance_id()?);
    assert_ne!(a.project_id, b.project_id);
    let mut broker = Broker::start(mode, &a, &b).await?;
    let observed: Result = async {
        a.broker_profile(&broker)?;
        b.broker_profile(&broker)?;
        let mut first = a
            .daemon()
            .await
            .map_err(|e| format!("Runtime A startup: {e}"))?;
        let mut second = b
            .daemon()
            .await
            .map_err(|e| format!("Runtime B startup: {e}"))?;
        assert_ne!(first.child.id(), second.child.id());
        let a_tasks = a
            .seed_tasks()
            .await
            .map_err(|e| format!("Runtime A Task seed: {e}"))?;
        let b_tasks = b
            .seed_tasks()
            .await
            .map_err(|e| format!("Runtime B Task seed: {e}"))?;
        let mut pipe = NowPipe::start(&b, &a)?;
        let initial = pipe.task(&a, &a_tasks.ids, "Café α").await?;
        for id in &b_tasks.ids {
            assert!(!initial.to_string().contains(id));
        }
        a.revise_task(&a_tasks, "Updated café γ").await?;
        let updated = pipe.task(&a, &a_tasks.ids, "Updated café γ").await?;
        let generated = |frame: &Value| -> Result<u128> {
            Ok(frame["instances"][0]["now"]["generated_at"]
                .as_str()
                .ok_or("generated_at missing")?
                .parse()?)
        };
        assert!(
            generated(&updated)? > generated(&initial)?,
            "realtime views arrived out of order"
        );
        pipe.stop().await?;
        // Explicit new CLI admission and full current view, without replaying an old stream.
        let mut reconnected = NowPipe::start(&b, &a)?;
        let current = reconnected.task(&a, &a_tasks.ids, "Updated café γ").await?;
        assert!(generated(&current)? >= generated(&updated)?);
        let remote = b
            .call(
                "graphit_now",
                json!({
                    "mode":"broker","project_id":a.project_id,"version":a.version,
                    "instance_id":a.instance_id()?,"provider":"inventory",
                }),
            )
            .await?;
        remote_heads(&remote, &a, &a_tasks.ids);
        let listed = b.call("graphit_instances", json!({
            "project_id":a.project_id,"version":a.version,"instance_id":a.instance_id()?,"provider":"inventory",
        })).await?;
        assert!(listed.to_string().contains(&a.instance_id()?));
        first.stop("-TERM").await?;
        a.cleaned();
        reconnected.empty().await?;
        reconnected.stop().await?;
        assert!(second.child.try_wait()?.is_none());
        let sibling = b
            .call(
                "graphit_now",
                json!({
                    "mode":"broker","project_id":b.project_id,"version":b.version,
                    "instance_id":b.instance_id()?,"provider":"inventory",
                }),
            )
            .await?;
        remote_heads(&sibling, &b, &b_tasks.ids);
        second.stop("-INT").await?;
        b.cleaned();
        Ok(())
    }.await;
    if observed.is_err() {
        for (label, installation) in [("A", &a), ("B", &b)] {
            match fs::File::open(installation.temporary.path().join("runtime.log")) {
                Ok(file) => {
                    for line in std::io::BufRead::lines(std::io::BufReader::new(file)).take(32) {
                        match line {
                            Ok(line) => eprintln!(
                                "Runtime {label}: {}",
                                sanitized_diagnostic(&line)
                                    .chars()
                                    .take(320)
                                    .collect::<String>()
                            ),
                            Err(error) => {
                                eprintln!("Runtime {label} log read: {:?}", error.kind());
                                break;
                            }
                        }
                    }
                }
                Err(error) => eprintln!("Runtime {label} log open: {:?}", error.kind()),
            }
        }
    }
    let joined = broker.stop().await;
    match (observed, joined) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => {
            Err(format!("journey: {error}; Broker cleanup: {cleanup}").into())
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires real public Native Broker fixture with PostgreSQL TLS and realtime"]
async fn compiled_native_two_runtime_now_registration_reconnect_and_join() -> Result {
    broker_journey(BrokerMode::Native).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires real public Local Broker fixture with PostgreSQL TLS and realtime"]
async fn compiled_local_two_runtime_now_registration_reconnect_and_join() -> Result {
    broker_journey(BrokerMode::Local).await
}
fn private_heads(now: &Value, installation: &Installation, ids: &[String]) {
    assert_eq!(now["project_id"], json!(installation.project_id));
    let section = &now["sections"]["tasks"];
    assert_eq!(section["availability"], "available");
    assert_eq!(section["total"], ids.len());
    assert_eq!(section["has_more"], false);
    let observed: Vec<_> = section["items"]
        .as_array()
        .expect("task heads")
        .iter()
        .map(|v| v["id"].as_str().expect("task head id"))
        .collect();
    for id in ids {
        assert!(observed.contains(&id.as_str()));
    }
    let wire = now.to_string();
    for private in [
        "PRIVATE_TASK_BODY",
        "PRIVATE_SESSION_BODY",
        installation.temporary.path().to_str().unwrap(),
    ] {
        assert!(!wire.contains(private), "Now leaked private data");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_two_runtime_projects_versions_native_heads_and_owned_shutdown() -> Result {
    let a = Installation::new("integration-a", "feature/ação")?;
    let b = Installation::new("integration-b", "release/β")?;
    assert_ne!(a.project_id, b.project_id);
    let mut first = a.daemon().await?;
    let mut second = b.daemon().await?;
    assert_ne!(first.child.id(), second.child.id());
    assert_ne!(a.instance_id()?, b.instance_id()?);
    let a_ids = a.seed_tasks().await?.ids;
    let b_ids = b.seed_tasks().await?.ids;
    let now_a = a.local_now().await?;
    let now_b = b.local_now().await?;
    private_heads(&now_a, &a, &a_ids);
    private_heads(&now_b, &b, &b_ids);
    for id in &b_ids {
        assert!(!now_a.to_string().contains(id));
    }
    first.stop("-TERM").await?;
    a.cleaned();
    assert!(
        second.child.try_wait()?.is_none(),
        "sibling Runtime was stopped"
    );
    private_heads(&b.local_now().await?, &b, &b_ids);
    second.stop("-INT").await?;
    b.cleaned();
    Ok(())
}
