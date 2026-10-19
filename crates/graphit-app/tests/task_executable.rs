#![cfg(target_os = "linux")]
//! Root Task product qualification using the compiled CLI and actual Foreground.
#[path = "task_executable/cli_flags.rs"]
mod cli_flags;
use cli_flags::task_flags;
#[path = "task_executable/git_hooks.rs"]
mod git_hooks;
#[path = "task_executable/parallel_claims.rs"]
mod parallel_claims;
#[path = "task_executable/session_revision.rs"]
mod session_revision;
use graphit_app::foreground_source::ForegroundSource;
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::project_bootstrap::ProjectBootstrapInput;
use graphit_hub::{NativeBinding, policy::HubResourcePolicy};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
};
use serde_json::{Value, json};
use std::{
    fs, io,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn json_arguments(
    mut arguments: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    arguments.insert("ai_optimized".into(), serde_json::Value::Bool(false));
    arguments
}

struct Fixture {
    _temp: tempfile::TempDir,
    paths: InstallationPaths,
    projects: Vec<ProjectPaths>,
}
impl Fixture {
    async fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
        let paths = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let cancel = Cancellation::default();
        let mut projects = Vec::new();
        for name in ["current", "sibling"] {
            let root = temp.path().join(name);
            fs::create_dir(&root)?;
            let project = ProjectPaths::at(&root, &paths)?;
            ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), &cancel)?
                .ensure(&ProjectMetadata::new(name)?)?;
            projects.push(project);
        }
        for args in [
            vec!["init", "-b", "main"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
            vec!["branch", "feature"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(projects[0].root())
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other(String::from_utf8_lossy(&output.stderr)).into());
            }
        }
        let config = ConfigSnapshot::new(&brand, [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let original = policy.request_context(Instant::now(), &cancel)?;
        let inputs = [
            (&projects[0], "main"),
            (&projects[0], "feature"),
            (&projects[1], ""),
        ]
        .map(|(project, explicit_version)| ProjectBootstrapInput {
            project,
            explicit_version,
            pin: None,
            detached_base: "",
        });
        let owner =
            ForegroundSource::start(&paths, &inputs, 0, &config, NativeBinding::Codex, &original)?;
        owner.shutdown().await?;
        ConfigStore::global(&paths)?.set(
            ConfigKey::new("agent")?,
            json!("codex"),
            Duration::from_secs(5),
            &cancel,
        )?;

        Ok(Self {
            _temp: temp,
            paths,
            projects,
        })
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(self.projects[0].root())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self._temp.path())
            .env("GRAPHIT_MODULES__AST", "false");
        command.env(
            self.paths
                .brand()
                .environment_variable("GLOBAL_DIR")
                .unwrap(),
            self.paths.root(),
        );
        command
    }

    fn task(&self, operation: &str, input: Option<Value>, selection: &[&str]) -> Result<Value> {
        let mut command = self.command();
        command.args(["task", operation]).args(selection);
        if let Some(input) = input {
            command.args(task_flags(operation, &input));
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "CLI {operation}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    async fn daemon(&self, args: &[&str]) -> Result<Daemon> {
        let mut command = self.command();
        command.args(["daemon", "foreground"]).args(args);
        self.daemon_command(command).await
    }
    async fn daemon_command(&self, mut command: Command) -> Result<Daemon> {
        let log = self._temp.path().join("daemon.log");
        let child = command
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log)?)
            .spawn()?;
        let mut daemon = Daemon { child, log };
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = daemon.child.try_wait()? {
                return Err(io::Error::other(format!(
                    "daemon {status}: {}",
                    fs::read_to_string(&daemon.log)?
                ))
                .into());
            }
            if self.paths.root().join("daemon/mcp.json").exists() {
                break;
            }
            if Instant::now() >= until {
                return Err(io::Error::other("daemon discovery did not become ready").into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(daemon)
    }
}
struct Daemon {
    child: Child,
    log: std::path::PathBuf,
}
impl Daemon {
    async fn stop(mut self, signal: &str) -> Result {
        signal_child(&self.child, signal)?;
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait()? {
                assert_eq!(
                    status.code(),
                    Some(130),
                    "{}",
                    fs::read_to_string(&self.log)?
                );
                break;
            }
            if Instant::now() >= until {
                return Err(io::Error::other("daemon did not join on signal").into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }
}
fn signal_child(child: &Child, signal: &str) -> io::Result<()> {
    let status = Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("owned child signal failed"))
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn wire(result: CallToolResult) -> Result<Value> {
    assert_ne!(result.is_error, Some(true), "MCP returned an error");
    if let Some(value) = result.structured_content {
        return Ok(value);
    }
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .ok_or("missing MCP text")?;
    Ok(serde_json::from_str(&text.text)?)
}
fn query_input(table: &str) -> Value {
    json!({"selection":{"table":table},"columns":["id","status"],"page_size":16,"top_k":16})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_root_task_cli_mcp_native_http_history_and_current_now() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let session = f.task("create-session", Some(json!({"idempotency_key":"root-session", "title":"Root session", "description":"Actual compiled Task", "strategy":"CLI MCP HTTP Now"})), &[])?;
    let session_id = session["result"]["id"].clone();
    f.task(
        "claim-session",
        Some(json!({"id":session_id,"lease_ms":120000})),
        &[],
    )?;
    let task = f.task("create-task", Some(json!({"idempotency_key":"root-task","session_id":session_id,"title":"Root task","description":"Explicit IDs and real scopes","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Root exposes Task"},{"kind":"test","text":"Use compiled CLI and real ingress"}]})), &[])?;
    let id = task["result"]["specification"]["id"].clone();
    let claimed = f.task("claim-task", Some(json!({"id":id,"lease_ms":120000})), &[])?;
    let token = claimed["result"]["token"].clone();
    let revision = claimed["result"]["record"]["specification"]["revision"].clone();
    let progress = f.task("progress-task", Some(json!({"target":{"id":id,"claim":{"token":token,"expected_revision":revision}},"lease_ms":120000,"summary":"Product ingress reached real SDK","next_step":"Read current heads"})), &[])?;
    assert!(
        progress["result"]["specification"]["revision"]
            .as_u64()
            .unwrap()
            > revision.as_u64().unwrap()
    );
    let mut comment_revision = progress["result"]["specification"]["revision"].clone();
    for index in 0..2 {
        let comment = f.task(
            "add-task-comment",
            Some(json!({
                "target":{"id":id,"claim":{"token":token,"expected_revision":comment_revision}},
                "lease_ms":120000,"idempotency_key":format!("root-comment-{index}"),
                "kind":"note","body":format!("Captured CLI comment {index}")
            })),
            &[],
        )?;
        comment_revision = comment["result"]["result"]["task"]["specification"]["revision"].clone();
    }
    let current_detail = f.task("get-task", Some(json!({"id":id})), &[])?;
    assert!(current_detail["result"]["history"].is_null());
    assert!(current_detail["result"]["comments"].is_null());
    // Publish after capture, then exercise the explicit history/comment cursor flags
    // with both initial cursors. Explicit pages retain the earlier SDK pin.
    f.task(
        "add-task-comment",
        Some(json!({
            "target":{"id":id,"claim":{"token":token,"expected_revision":comment_revision}},
            "lease_ms":120000,"idempotency_key":"root-comment-after-capture",
            "kind":"note","body":"Later CLI comment outside the captured audit"
        })),
        &[],
    )?;
    let detail = f.task(
        "get-task",
        Some(json!({"id":id,
            "history":{"page_size":100,"cursor":current_detail["result"]["history_start_cursor"]},
            "comments":{"page_size":100,"cursor":current_detail["result"]["comments_start_cursor"]}
        })),
        &[],
    )?;
    assert_eq!(detail["result"]["task"], current_detail["result"]["task"]);
    assert_eq!(
        detail["result"]["generation"],
        current_detail["result"]["generation"]
    );
    assert_eq!(
        detail["result"]["comments"]["comments"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!detail.to_string().contains("Later CLI comment outside"));
    assert!(!detail.to_string().contains(token.as_str().unwrap()));
    let mut cursor = current_detail["result"]["history_start_cursor"].clone();
    let mut before = detail["result"]["creation"]["record"].clone();
    let mut events = Vec::new();
    loop {
        let page = f.task(
            "get-task",
            Some(json!({"id":id,
                "history":{"page_size":1,"cursor":cursor}
            })),
            &[],
        )?;
        assert_eq!(
            page["result"]["generation"],
            current_detail["result"]["generation"]
        );
        assert_eq!(page["result"]["task"], current_detail["result"]["task"]);
        assert_eq!(page["result"]["history"]["before"], before);
        assert!(page["result"]["comments"].is_null());
        let selected = page["result"]["history"]["events"].as_array().unwrap();
        assert_eq!(selected.len(), 1);
        before = selected[0]["record"].clone();
        events.extend(selected.iter().cloned());
        cursor = page["result"]["history"]["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(
        events,
        *detail["result"]["history"]["events"].as_array().unwrap()
    );
    let mut cursor = current_detail["result"]["comments_start_cursor"].clone();
    let mut comments = Vec::new();
    loop {
        let page = f.task(
            "get-task",
            Some(json!({"id":id,
                "comments":{"page_size":1,"cursor":cursor}
            })),
            &[],
        )?;
        assert_eq!(
            page["result"]["generation"],
            current_detail["result"]["generation"]
        );
        assert!(page["result"]["history"].is_null());
        let selected = page["result"]["comments"]["comments"].as_array().unwrap();
        assert_eq!(selected.len(), 1);
        assert!(comments.len() < 2);
        assert_eq!(
            selected[0]["sequence"].as_u64(),
            Some(comments.len() as u64 + 1)
        );
        comments.extend(selected.iter().cloned());
        cursor = page["result"]["comments"]["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(
        comments,
        *detail["result"]["comments"]["comments"].as_array().unwrap()
    );
    let latest_detail = f.task("get-task", Some(json!({"id":id})), &[])?;
    assert!(
        latest_detail["result"]["task"]["specification"]["revision"]
            .as_u64()
            .unwrap()
            > current_detail["result"]["task"]["specification"]["revision"]
                .as_u64()
                .unwrap()
    );
    assert_eq!(
        f.task("get-session", Some(json!({"id":session_id})), &[])?["result"]["session"]["id"],
        session_id
    );
    assert!(f.task("session-heads", None, &[])?["result"].is_array());
    let cancel = Cancellation::default();
    let config = ConfigSnapshot::new(f.paths.brand(), [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let original = policy.request_context(Instant::now(), &cancel)?;
    let limits = NativeDocumentFileLimits::new(65536, 4096, 32, 4096)?;
    let address =
        graphit_daemon::source_discovery::read_discovery(&f.paths, limits, original.snapshot())?
            .ok_or("missing discovery")?
            .address();
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let headers = graphit_app::ast_source::runtime_headers(&f.paths, limits, &original).await?;
    let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
        http.clone(),
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
            format!("http://{address}/mcp"),
        )
        .custom_headers(headers.clone()),
    );
    let client = ().serve(transport).await?;
    let tools = client.peer().list_all_tools().await?;
    assert_eq!(
        tools
            .iter()
            .filter(|tool| tool.name.starts_with("graphit_task_"))
            .count(),
        28
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "graphit_task_session_revise")
    );
    let queried = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_task_query").with_arguments(json_arguments(
                    json!({"input":query_input("tasks")})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?,
    )?;
    assert!(queried.to_string().contains(id.as_str().unwrap()));
    let main_id = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .unwrap()
        .id();
    let now_name = f.paths.brand().mcp_tool_name(&["now"])?;
    assert!(
        tools.iter().any(|tool| tool.name.as_ref() == now_name),
        "Now local tool must be advertised"
    );
    let now = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new(now_name).with_arguments(json_arguments(
                    json!({"project_id":main_id,"mode":"local","version":"main"})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?,
    )?;
    assert_eq!(now["sections"]["tasks"]["availability"], "available");
    assert_eq!(now["sections"]["tasks"]["items"][0]["id"], id);
    assert_eq!(now["sections"]["sessions"]["items"][0]["id"], session_id);
    let native = format!("http://{address}/api/tasks/native");
    let header_map: reqwest::header::HeaderMap = headers.into_iter().collect();
    let listed: Value = http
        .post(format!("{native}/catalogue"))
        .headers(header_map.clone())
        .json(&json!({"selection":{"project_dir":graphit_core::native_path::NativePath::new(f.projects[0].root().to_owned())?,"project_version":"main"},"input":{"filter":{},"page_size":1}}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(listed["operation"], "catalogue");
    assert_eq!(listed["result"]["results"][0]["id"], id);
    let sibling = f.projects[1].root().to_str().unwrap();
    let sibling_session: Value = http.post(format!("{native}/create_session")).headers(header_map.clone()).json(&json!({"selection":{"project_dir":graphit_core::native_path::NativePath::new(f.projects[1].root().to_owned())?},"input":{"idempotency_key":"sibling-session","title":"Sibling","description":"Isolated scope","strategy":"native HTTP"}})).send().await?.error_for_status()?.json().await?;
    assert_eq!(sibling_session["kind"], "session");
    let sibling_id = sibling_session["result"]["id"].clone();
    assert_eq!(
        f.task(
            "get-session",
            Some(json!({"id":sibling_id})),
            &["--project-dir", sibling]
        )?["result"]["session"]["id"],
        sibling_id
    );
    let sibling_view = f.task(
        "query",
        Some(query_input("tasks")),
        &["--project-dir", sibling],
    )?;
    assert!(!sibling_view.to_string().contains(id.as_str().unwrap()));
    // A missing branch reads the captured default baseline without materializing it.
    let inherited = f.task(
        "query",
        Some(query_input("tasks")),
        &["--project-version", "feature"],
    )?;
    assert!(inherited.to_string().contains(id.as_str().unwrap()));
    let feature_task = f.task("create-task", Some(json!({"idempotency_key":"feature-task","session_id":session_id,"title":"Feature only","description":"SDK fork preserves baseline","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Isolate feature publication"},{"kind":"test","text":"Main must retain its own snapshot"}]})), &["--project-version","feature"])?;
    let feature_id = feature_task["result"]["specification"]["id"].clone();
    let feature = f.task(
        "query",
        Some(query_input("tasks")),
        &["--project-version", "feature"],
    )?;
    assert!(feature.to_string().contains(id.as_str().unwrap()));
    assert!(feature.to_string().contains(feature_id.as_str().unwrap()));
    let main_view = f.task("query", Some(query_input("tasks")), &[])?;
    assert!(!main_view.to_string().contains(feature_id.as_str().unwrap()));
    let later_main = f.task("create-task", Some(json!({"idempotency_key":"main-after-fork","session_id":session_id,"title":"Main after fork","description":"Do not advance the feature baseline","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Retain frozen fork history"},{"kind":"test","text":"Feature must exclude later Main publication"}]})), &[])?;
    let later_main_id = later_main["result"]["specification"]["id"].clone();
    let feature_after_main = f.task(
        "query",
        Some(query_input("tasks")),
        &["--project-version", "feature"],
    )?;
    assert!(
        !feature_after_main
            .to_string()
            .contains(later_main_id.as_str().unwrap())
    );
    assert_eq!(
        f.task(
            "get-task",
            Some(json!({"id":id})),
            &["--project-version", "feature"]
        )?["result"]["task"],
        latest_detail["result"]["task"]
    );

    assert_eq!(
        http.post(format!("{native}/get_task"))
            .json(&json!({"input":{"id":id}}))
            .send()
            .await?
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http.post(format!("http://{address}/api/tasks/get"))
            .headers(header_map.clone())
            .json(&json!({"input":{"id":id}}))
            .send()
            .await?
            .status(),
        reqwest::StatusCode::NOT_FOUND
    );
    let denied = f
        .command()
        .env("GRAPHIT_ROLE_BEARER", "owned-role-context")
        .args(["task", "query"])
        .args(task_flags("query", &query_input("tasks")))
        .output()?;
    assert_eq!(denied.status.code(), Some(1));
    assert!(denied.stdout.is_empty());
    let unknown = f
        .command()
        .args([
            "task",
            "get-task",
            "--id",
            id.as_str().unwrap(),
            "--project-version",
            "retired",
        ])
        .output()?;
    assert_eq!(unknown.status.code(), Some(1));
    assert!(unknown.stdout.is_empty());
    for version in [None, Some("feature")] {
        let mut command = tokio::process::Command::from(f.command());
        command
            .args(["mcp", "--stdio", "--non-interactive"])
            .args(
                version
                    .into_iter()
                    .flat_map(|version| ["--project-version", version]),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let sdk = ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await?;
        let default = wire(
            sdk.peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_task_get").with_arguments(json_arguments(
                        json!({"input":{"id":id}}).as_object().unwrap().clone(),
                    )),
                )
                .await?,
        )?;
        assert_eq!(default["result"]["task"], latest_detail["result"]["task"]);
        assert!(default["result"]["history"].is_null());
        assert!(default["result"]["comments"].is_null());
        let explicit = wire(
            sdk.peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_task_get").with_arguments(json_arguments(
                        json!({"selection":{"project_version":"main"},"input":{"id":id}})
                            .as_object()
                            .unwrap()
                            .clone(),
                    )),
                )
                .await?,
        )?;
        assert_eq!(explicit["result"]["task"], latest_detail["result"]["task"]);
        assert!(explicit["result"]["history"].is_null());
        assert!(explicit["result"]["comments"].is_null());
        let sibling_selected = wire(sdk.peer().call_tool(CallToolRequestParams::new("graphit_task_session_get")
            .with_arguments(json_arguments(json!({"selection":{"project_dir":graphit_core::native_path::NativePath::new(f.projects[1].root().to_owned())?},"input":{"id":sibling_id}}).as_object().unwrap().clone()))).await?)?;
        assert_eq!(sibling_selected["result"]["session"]["id"], sibling_id);
        let invalid = sdk
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_task_query").with_arguments(json_arguments(
                    json!({"selection":"invalid"}).as_object().unwrap().clone(),
                )),
            )
            .await?;
        assert_eq!(invalid.is_error, Some(true));
        sdk.cancel().await?;
        assert!(
            tokio::time::timeout(Duration::from_secs(10), child.wait())
                .await??
                .success()
        );
    }
    let current_version = "g1-live-version";
    let maintain =
        |version: &str| graphit_app::foreground_project_control::ProjectCommand::MaintainCheckout {
            project_id: main_id,
            project_dir: graphit_core::native_path::NativePath::new(
                f.projects[0].root().to_owned(),
            )
            .unwrap(),
            version: version.to_owned(),
            detached_base: String::new(),
            select: true,
        };
    let added = http
        .post(format!(
            "http://{address}{}",
            graphit_app::foreground_project_control::ROUTE
        ))
        .headers(header_map.clone())
        .json(&maintain(current_version))
        .send()
        .await?;
    assert!(
        added.status().is_success(),
        "live version add/switch failed"
    );
    let current_session: Value = http.post(format!("{native}/create_session"))
        .headers(header_map.clone()).json(&json!({"input":{"idempotency_key":"current-live-session","title":"Live current","description":"Current tuple comes from the owner","strategy":"Observe live version changes"}}))
        .send().await?.error_for_status()?.json().await?;
    let current_session_id = current_session["result"]["id"].clone();
    let stale_main = f
        .command()
        .args(["task", "query"])
        .args(task_flags("query", &query_input("tasks")))
        .output()?;
    assert!(!stale_main.status.success());
    assert!(stale_main.stdout.is_empty());
    assert!(String::from_utf8_lossy(&stale_main.stderr).contains("selection_unavailable"));
    let current_added = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_task_query").with_arguments(json_arguments(
                    json!({"input":query_input("sessions")})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?,
    )?;
    assert!(
        current_added
            .to_string()
            .contains(current_session_id.as_str().unwrap())
    );
    let added_now = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new(f.paths.brand().mcp_tool_name(&["now"])?)
                    .with_arguments(json_arguments(
                        json!({"project_id":main_id,"mode":"local","version":current_version})
                            .as_object()
                            .unwrap()
                            .clone(),
                    )),
            )
            .await?,
    )?;
    assert_eq!(added_now["sections"]["tasks"]["availability"], "available");
    assert_eq!(added_now["sections"]["tasks"]["items"][0]["id"], id);
    let switched = http
        .post(format!(
            "http://{address}{}",
            graphit_app::foreground_project_control::ROUTE
        ))
        .headers(header_map.clone())
        .json(&maintain("main"))
        .send()
        .await?;
    assert!(switched.status().is_success(), "live switch to Main failed");
    let current_main = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_task_query").with_arguments(json_arguments(
                    json!({"input":query_input("sessions")})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?,
    )?;
    assert!(
        !current_main
            .to_string()
            .contains(current_session_id.as_str().unwrap())
    );
    let retired = http
        .post(format!(
            "http://{address}{}",
            graphit_app::foreground_project_control::ROUTE
        ))
        .headers(header_map.clone())
        .json(
            &graphit_app::foreground_project_control::ProjectCommand::RetireCheckout {
                project_id: main_id,
                project_dir: graphit_core::native_path::NativePath::new(
                    f.projects[0].root().to_owned(),
                )?,
            },
        )
        .send()
        .await?;
    assert!(retired.status().is_success(), "retirement failed");
    assert!(f.projects[0].brand_directory().exists());
    assert_eq!(http.post(format!("{native}/get_task")).headers(header_map.clone())
        .json(&json!({"selection":{"project_dir":graphit_core::native_path::NativePath::new(f.projects[0].root().to_owned())?,"project_version":"main"},"input":{"id":id}}))
        .send().await?.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(
        f.task(
            "get-session",
            Some(json!({"id":sibling_id})),
            &["--project-dir", sibling]
        )?["result"]["session"]["id"],
        sibling_id
    );
    let current = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_task_query").with_arguments(json_arguments(
                    json!({"input":query_input("tasks")})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?,
    )?;
    assert!(!current.to_string().contains(id.as_str().unwrap()));
    let sibling_sessions = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_task_query").with_arguments(json_arguments(
                    json!({"input":query_input("sessions")})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?,
    )?;
    assert!(
        sibling_sessions
            .to_string()
            .contains(sibling_id.as_str().unwrap())
    );
    client.cancel().await?;
    daemon.stop("-TERM").await?;
    let reopened = f.daemon(&["--project-version", "main"]).await?;
    assert_eq!(
        f.task("get-task", Some(json!({"id":id})), &[])?["result"]["task"],
        latest_detail["result"]["task"]
    );
    reopened.stop("-INT").await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_task_search_pages_across_cli_mcp_http_and_tracks_current_text() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let query = |text: &str| json!({"text":text,"filter":{},"page_size":1,"top_k":3});
    assert_eq!(
        f.task("search", Some(query("paginationneedle")), &[])?["result"],
        "NoCorpus"
    );
    let session = f.task("create-session", Some(json!({"idempotency_key":"search-session","title":"Search session","description":"Real compiled search corpus","strategy":"CLI MCP HTTP continuation"})), &[])?;
    let session_id = session["result"]["id"].clone();
    let task_input = |key: &str, title: &str, session: &Value| {
        json!({
            "idempotency_key":key,"session_id":session,"title":title,
            "description":"Native Task lexical consumer","task_type":"TASK","priority":1,
            "checks":[{"kind":"acceptance","text":"Keep functional search"},{"kind":"test","text":"Exercise actual ingress"}]
        })
    };
    let mut expected = Vec::new();
    for key in ["search-a", "search-b", "search-c"] {
        let task = f.task(
            "create-task",
            Some(task_input(key, "paginationneedle", &session_id)),
            &[],
        )?;
        expected.push(task["result"]["specification"]["id"].clone());
    }
    let first = f.task("search", Some(query("paginationneedle")), &[])?;
    assert_eq!(first["operation"], "search");
    let first_page = &first["result"]["Published"];
    assert_eq!(first_page["total_hits"], 3);
    assert_eq!(first_page["results"].as_array().unwrap().len(), 1);
    assert!(first_page["next_cursor"].is_object());

    let cancel = Cancellation::default();
    let config = ConfigSnapshot::new(f.paths.brand(), [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let original = policy.request_context(Instant::now(), &cancel)?;
    let limits = NativeDocumentFileLimits::new(65536, 4096, 32, 4096)?;
    let address =
        graphit_daemon::source_discovery::read_discovery(&f.paths, limits, original.snapshot())?
            .ok_or("missing discovery")?
            .address();
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let headers = graphit_app::ast_source::runtime_headers(&f.paths, limits, &original).await?;
    let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
        http.clone(),
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
            format!("http://{address}/mcp"),
        )
        .custom_headers(headers.clone()),
    );
    let client = ().serve(transport).await?;
    assert!(
        client
            .peer()
            .list_all_tools()
            .await?
            .iter()
            .any(|tool| tool.name == "graphit_task_search")
    );
    let main_selection = json!({
        "project_dir":graphit_core::native_path::NativePath::new(f.projects[0].root().to_owned())?,
        "project_version":"main"
    });
    let mut second_query = query("paginationneedle");
    second_query["cursor"] = first_page["next_cursor"].clone();
    let second = wire(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_task_search").with_arguments(json_arguments(
                    json!({"selection":main_selection,"input":second_query})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?,
    )?;
    assert_eq!(second["operation"], "search");
    let second_page = &second["result"]["Published"];
    assert_eq!(second_page["generation"], first_page["generation"]);
    assert_eq!(second_page["total_hits"], 3);
    assert_eq!(second_page["results"].as_array().unwrap().len(), 1);
    assert!(second_page["next_cursor"].is_object());
    let mut third_query = query("paginationneedle");
    third_query["cursor"] = second_page["next_cursor"].clone();
    let header_map: reqwest::header::HeaderMap = headers.into_iter().collect();
    let native_search = format!("http://{address}/api/tasks/native/search");
    let third: Value = http
        .post(&native_search)
        .headers(header_map.clone())
        .json(&json!({"selection":main_selection,"input":third_query}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(third["operation"], "search");
    let third_page = &third["result"]["Published"];
    assert_eq!(third_page["generation"], first_page["generation"]);
    assert_eq!(third_page["total_hits"], 3);
    assert_eq!(third_page["results"].as_array().unwrap().len(), 1);
    assert!(third_page["next_cursor"].is_null());
    let mut actual = [first_page, second_page, third_page]
        .into_iter()
        .map(|page| page["results"][0]["task"]["id"].clone())
        .collect::<Vec<_>>();
    actual.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
    expected.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
    assert_eq!(
        actual, expected,
        "continuations must return each Task exactly once"
    );

    let feature = f.task(
        "create-task",
        Some(task_input(
            "feature-search",
            "selectionneedle feature",
            &session_id,
        )),
        &["--project-version", "feature"],
    )?;
    let feature_id = feature["result"]["specification"]["id"].clone();
    let sibling = f.projects[1].root().to_str().unwrap();
    let sibling_session = f.task("create-session", Some(json!({"idempotency_key":"sibling-search-session","title":"Sibling search","description":"Own corpus","strategy":"Keep selections distinct"})), &["--project-dir", sibling])?;
    let sibling_task = f.task(
        "create-task",
        Some(task_input(
            "sibling-search",
            "selectionneedle sibling",
            &sibling_session["result"]["id"],
        )),
        &["--project-dir", sibling],
    )?;
    let sibling_id = sibling_task["result"]["specification"]["id"].clone();
    assert_eq!(
        f.task("search", Some(query("selectionneedle")), &[])?["result"]["Published"]["total_hits"],
        0
    );
    let feature_search = f.task(
        "search",
        Some(query("selectionneedle")),
        &["--project-version", "feature"],
    )?;
    assert_eq!(feature_search["result"]["Published"]["total_hits"], 1);
    assert_eq!(
        feature_search["result"]["Published"]["results"][0]["task"]["id"],
        feature_id
    );
    let sibling_search = f.task(
        "search",
        Some(query("selectionneedle")),
        &["--project-dir", sibling],
    )?;
    assert_eq!(sibling_search["result"]["Published"]["total_hits"], 1);
    assert_eq!(
        sibling_search["result"]["Published"]["results"][0]["task"]["id"],
        sibling_id
    );
    let wrong_selection = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_task_search").with_arguments(json_arguments(
                json!({"selection":{"project_version":"feature"},"input":second_query})
                    .as_object()
                    .unwrap()
                    .clone(),
            )),
        )
        .await?;
    assert_eq!(
        wrong_selection.is_error,
        Some(true),
        "a cursor cannot grant another selection"
    );

    let id = expected[0].clone();
    let claimed = f.task("claim-task", Some(json!({"id":id,"lease_ms":120000})), &[])?;
    let token = claimed["result"]["token"].clone();
    let revision = claimed["result"]["record"]["specification"]["revision"].clone();
    let revised = f.task("revise-task", Some(json!({
        "target":{"id":id,"claim":{"token":token,"expected_revision":revision}},
        "lease_ms":120000,"reason":"Replace current searchable specification","title":"revisedneedle"
    })), &[])?;
    let revised_revision = revised["result"]["specification"]["revision"].clone();
    let revised_search = f.task("search", Some(query("revisedneedle")), &[])?;
    let revised_page = &revised_search["result"]["Published"];
    assert_eq!(revised_page["total_hits"], 1);
    assert_eq!(revised_page["results"][0]["task"]["id"], id);
    assert_eq!(
        revised_page["results"][0]["location"]["Specification"]["revision"],
        revised_revision
    );
    assert_eq!(
        f.task("search", Some(query("paginationneedle")), &[])?["result"]["Published"]["total_hits"],
        2
    );
    f.task("add-task-comment", Some(json!({
        "target":{"id":id,"claim":{"token":token,"expected_revision":revised_revision}},
        "lease_ms":120000,"idempotency_key":"search-comment","kind":"note","body":"commentneedle"
    })), &[])?;
    let comment_search = f.task("search", Some(query("commentneedle")), &[])?;
    let comment_page = &comment_search["result"]["Published"];
    assert_eq!(comment_page["total_hits"], 1);
    assert_eq!(comment_page["results"][0]["task"]["id"], id);
    assert!(
        comment_page["results"][0]["location"]["Comment"]["sequence"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(!comment_search.to_string().contains(token.as_str().unwrap()));
    assert_eq!(
        http.post(&native_search)
            .json(&json!({"selection":main_selection,"input":query("commentneedle")}))
            .send()
            .await?
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let denied = f
        .command()
        .env("GRAPHIT_ROLE_BEARER", "owned-role-context")
        .args(["task", "search"])
        .args(task_flags("search", &query("commentneedle")))
        .output()?;
    assert_eq!(denied.status.code(), Some(1));
    assert!(denied.stdout.is_empty());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("managed-role transport context"));
    client.cancel().await?;
    daemon.stop("-TERM").await?;
    Ok(())
}
