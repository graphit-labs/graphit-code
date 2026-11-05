#![cfg(target_os = "linux")]
//! Copilot native CLI Ask through the real Graphit CLI, HTTP and STDIO ingress.
use graphit_app::{
    native_install_http::NativeInstallPolicy,
    source_server::{SourcePolicy, SourceServer},
};
#[path = "support/knowledge_cli_daemon.rs"]
mod cli_daemon;
#[path = "support/knowledge_ask_server.rs"]
mod fixture_support;
use graphit_auth::{
    Permission, ProjectCapture, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::{ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeLimits},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
};
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use std::{
    ffi::OsString,
    io,
    num::NonZeroUsize,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn original(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1024 * 1024,
            max_output_bytes: 8 * 1024 * 1024,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 1024 * 1024,
            max_collection_entries: 65536,
        },
        cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap()
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
async fn publish(f: &fixture_support::Fixture, ctx: &RequestContext<'_>) {
    publish_corpus(f, ctx, false).await;
}
async fn publish_corpus(f: &fixture_support::Fixture, ctx: &RequestContext<'_>, empty: bool) {
    let (paths, id, version) = &f.projects[0];
    let live = CapturedPublicationScope::project(
        paths,
        *id,
        ScopeLimits::new(16, 4096, 256).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let record = live.binding().unwrap();
    let caller = f.auth.issuer().authenticate_runtime(&f.raw, ctx).unwrap();
    let authority = ProjectRecordAuthority::new(f.auth.issuer());
    let admission = authority
        .admit_authenticated_write(
            &caller,
            ctx,
            ScopeInput {
                installation: f.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &live,
                        record: &record,
                    },
                    version,
                }),
                providers: &[],
                binding: NativeBinding::Codex,
                global_version: "",
            },
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let pages = [h::AuthoredPageFields {
        slug: Some("Paris"),
        title: Some("Capital β"),
        body: Some("Paris é a capital.\n"),
        kind: Some("page"),
        summary: Some("literal"),
        description: None,
        summary_truncated: false,
        source_path: "docs/fixture.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([2; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(16),
            }),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    }];
    let rows = h::prepare(
        ctx.snapshot(),
        if empty { &[] } else { &pages },
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(8),
            max_observations: n(8),
            max_rows: n(64),
            max_field_bytes: n(65536),
            max_text_bytes: n(131072),
        },
    )
    .unwrap();
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 16).unwrap();
    publish_native_authored(
        ctx.snapshot(),
        &admission,
        &rows,
        KnowledgeIndexWriteLimits {
            content: KnowledgeIndexContentLimits {
                max_identifier_bytes: n(256),
            },
            native: native(),
            artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
        },
    )
    .await
    .unwrap();
}
fn command(f: &fixture_support::Fixture) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
    command
        .current_dir(f.projects[0].0.root())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", f.temp.path());
    command.env(
        f.paths.brand().environment_variable("GLOBAL_DIR").unwrap(),
        f.paths.root(),
    );
    command
}
fn run(test: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
                .block_on(test)
        })
        .unwrap()
        .join()
        .unwrap();
}
async fn serve(
    f: &fixture_support::Fixture,
    ctx: &RequestContext<'_>,
) -> (
    tokio::task::JoinHandle<io::Result<()>>,
    graphit_daemon::source_discovery::DiscoveryRecord,
) {
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        ctx.cancellation(),
    )
    .unwrap();
    let discovery = graphit_daemon::source_discovery::publish_discovery(
        &f.auth,
        &listener,
        native(),
        ctx.snapshot(),
    )
    .unwrap();
    let server = f.server.clone();
    (
        tokio::spawn(async move { SourceServer::serve(&server, listener).await }),
        discovery,
    )
}
async fn stop(
    f: &fixture_support::Fixture,
    served: tokio::task::JoinHandle<io::Result<()>>,
    record: &graphit_daemon::source_discovery::DiscoveryRecord,
    ctx: &SnapshotContext,
) {
    f.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    graphit_daemon::source_discovery::remove_discovery(&f.auth, record, native(), ctx).unwrap();
}

use graphit_app::mcp_source::SourceProxy;
use rmcp::{
    RoleClient, ServiceExt,
    model::{
        CallToolRequestParams, ClientRequest, JsonRpcMessage, ProgressNotificationParam,
        ServerNotification, ServerResult,
    },
    service::{PeerRequestOptions, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{Transport, async_rw::AsyncRwTransport},
};
use serde_json::{Value, json};

#[derive(Clone)]
enum Seen {
    Progress(ProgressNotificationParam),
    Final,
}
struct Observe<T> {
    inner: T,
    events: Arc<Mutex<Vec<Seen>>>,
    changed: Arc<tokio::sync::Notify>,
}
impl<T: Transport<RoleClient>> Transport<RoleClient> for Observe<T> {
    type Error = T::Error;
    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        let item = self.inner.receive().await;
        let seen = match &item {
            Some(JsonRpcMessage::Notification(notification)) => match &notification.notification {
                ServerNotification::ProgressNotification(event) => {
                    Some(Seen::Progress(event.params.clone()))
                }
                _ => None,
            },
            Some(JsonRpcMessage::Response(response))
                if matches!(response.result, ServerResult::CallToolResult(_)) =>
            {
                Some(Seen::Final)
            }
            _ => None,
        };
        if let Some(seen) = seen {
            self.events.lock().unwrap().push(seen);
            self.changed.notify_waiters();
        }
        item
    }
    async fn close(&mut self) -> Result<(), Self::Error> {
        self.inner.close().await
    }
}
fn ask(arguments: Value) -> CallToolRequestParams {
    let Value::Object(mut arguments) = arguments else {
        panic!("MCP arguments must be an object");
    };
    arguments.insert("ai_optimized".into(), Value::Bool(false));
    CallToolRequestParams::new("graphit_knowledge_ask").with_arguments(arguments)
}
async fn progress_delta(events: &Mutex<Vec<Seen>>, changed: &tokio::sync::Notify) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting = changed.notified();
            tokio::pin!(waiting);
            waiting.as_mut().enable();
            if events.lock().unwrap().iter().any(|value|matches!(value, Seen::Progress(progress) if progress.message.as_deref().is_some_and(|message|message.contains("assistant.message_delta"))))
            {
                return;
            }
            waiting.await;
        }
    })
    .await
    .unwrap();
}

fn copilot_config(brand: &Brand) -> graphit_core::config::ConfigSnapshot {
    use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource};
    ConfigSnapshot::new(
        brand,
        [ConfigDocument::new(
            brand,
            ConfigSource::Global,
            [(ConfigKey::new("ai.cli").unwrap(), json!("copilot"))],
        )
        .unwrap()],
    )
    .unwrap()
}
async fn attach(
    f: &mut fixture_support::Fixture,
    ctx: &RequestContext<'_>,
    extra: Vec<(OsString, OsString)>,
) {
    let mut environment = vec![("PATH".into(), f.temp.path().as_os_str().to_owned())];
    environment.extend(extra);
    f.server
        .attach_ask(&copilot_config(f.paths.brand()), environment, ctx)
        .await
        .unwrap();
}
fn fixture() -> fixture_support::Fixture {
    fixture_support::fixture(
        &[
            Permission::Read(Module::Ast),
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
        NativeBinding::Codex,
    )
}
struct McpHarness {
    proxy: SourceProxy,
    joined: tokio::task::JoinHandle<io::Result<()>>,
    client: rmcp::service::RunningService<RoleClient, ()>,
    events: Arc<Mutex<Vec<Seen>>>,
    changed: Arc<tokio::sync::Notify>,
}
impl McpHarness {
    async fn connect(
        f: &fixture_support::Fixture,
        ctx: &RequestContext<'_>,
        address: std::net::SocketAddr,
        deadline: Duration,
    ) -> Self {
        let proxy = SourceProxy::new(
            f.paths.clone(),
            address,
            reqwest::Client::new(),
            (
                native(),
                ContentLimits {
                    max_input_bytes: 1048576,
                    max_output_bytes: 8388608,
                    max_depth: 64,
                    max_nodes: 65536,
                    max_scalar_bytes: 1048576,
                    max_collection_entries: 65536,
                },
                deadline,
            ),
            Some((f.projects[0].0.root(), None)),
            ctx,
        )
        .await
        .unwrap();
        let (local, remote) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(remote);
        let owner = proxy.clone();
        let joined = tokio::spawn(async move { owner.serve_stream(read, write).await });
        let (read, write) = tokio::io::split(local);
        let events = Arc::new(Mutex::new(Vec::new()));
        let changed = Arc::new(tokio::sync::Notify::new());
        let client = ()
            .serve(Observe {
                inner: AsyncRwTransport::new_client(read, write),
                events: Arc::clone(&events),
                changed: Arc::clone(&changed),
            })
            .await
            .unwrap();
        Self {
            proxy,
            joined,
            client,
            events,
            changed,
        }
    }
    async fn stop(self) {
        self.client.cancel().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), self.joined)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        self.proxy.shutdown().await.unwrap();
    }
}

const ANSWER: &str = "Native Copilot documentary answer 🦀\n";
fn native_peer(f: &fixture_support::Fixture) {
    let program = f.temp.path().join("copilot");
    std::fs::copy(env!("CARGO_BIN_EXE_app-copilot-fixture"), program).unwrap();
}
fn captured(f: &fixture_support::Fixture, mode: &str) -> Vec<(OsString, OsString)> {
    let mut environment = vec![
        (
            graphit_core::config::ConfigKey::new("ai.cli")
                .unwrap()
                .environment_variable(f.paths.brand())
                .into(),
            "copilot".into(),
        ),
        (
            "COPILOT_CAPTURED_ENV".into(),
            "startup literal;$(value)".into(),
        ),
        ("COPILOT_MODE".into(), mode.into()),
        (
            "COPILOT_CONVERSATION_JOURNAL".into(),
            f.temp.path().join("native.journal").into_os_string(),
        ),
        (
            "COPILOT_GATE_PORT".into(),
            f.temp.path().join("gate.port").into_os_string(),
        ),
        (
            "COPILOT_NATIVE_ARGS".into(),
            f.temp.path().join("argv").into_os_string(),
        ),
    ];
    environment.extend(
        std::env::vars_os().filter(|(key, _)| key == "LD_LIBRARY_PATH" || key == "SYSTEMROOT"),
    );
    environment
}
fn arguments() -> Value {
    json!({"query":"  literal β\n  ","selected_slugs":["Paris"],"model":"provider/literal;$(copilot)"})
}
async fn release(f: &fixture_support::Fixture) -> Result<(), reqwest::Error> {
    let port = gate_port(&f.temp.path().join("gate.port")).await;
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/fixture/release"))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

async fn gate_port(path: &std::path::Path) -> u16 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Ok(port) = text.parse::<u16>()
            {
                break port;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
fn assert_joined(payload: &Value) {
    let process = &payload["receipt"]["process"];
    assert_eq!(process["cleanup"], "Complete", "{payload}");
    assert!(process["stdout_end"].is_string(), "{payload}");
    assert!(process["stderr_end"].is_string(), "{payload}");
}
fn assert_success_process(payload: &Value) {
    assert_joined(payload);
    let process = &payload["receipt"]["process"];
    assert_eq!(process["exit_code"], 0);
    assert_eq!(process["status_success"], true);
    assert_eq!(process["stdout_end"], "Eof");
    assert_eq!(process["stderr_end"], "Eof");
}
#[test]
fn compiled_cli_uses_captured_copilot_environment_literal_model_and_documentary_prompt() {
    run(async {
        let f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let environment = captured(&f, "answer");
        let journal = f.temp.path().join("native.journal");
        let argv = f.temp.path().join("argv");
        let mut cmd = tokio::process::Command::from(command(&f));
        let mut daemon = cli_daemon::CliDaemon::start(f, NativeBinding::Codex, environment, &ctx)
            .await
            .unwrap();
        cmd.env("PATH", "/unavailable-client-path")
            .env("COPILOT_CAPTURED_ENV", "different caller")
            .args([
                "knowledge",
                "ask",
                "--query",
                "  literal β\n  ",
                "--slug",
                "Paris",
                "--model",
                "provider/literal;$(copilot)",
            ])
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, format!("{}\n", ANSWER).as_bytes());
        assert!(output.stderr.is_empty());
        let journal: Vec<Value> = std::fs::read_to_string(journal)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let start = journal.iter().find(|v| v["op"] == "start").unwrap();
        assert_eq!(start["environment"], "startup literal;$(value)");
        assert_eq!(start["model"], "provider/literal;$(copilot)");
        let prompt = journal.iter().find(|v| v["op"] == "prompt").unwrap();
        let documentary: Value = serde_json::from_str(prompt["prompt"].as_str().unwrap()).unwrap();
        assert_eq!(documentary["question"], "  literal β\n  ");
        assert!(documentary.to_string().contains("Paris é a capital."));
        assert_eq!(
            std::fs::read_to_string(argv).unwrap(),
            "--output-format=json\n--no-ask-user\n--available-tools=\n--model=provider/literal;$(copilot)\n"
        );
        daemon.stop().await.unwrap();
    });
}
#[test]
fn http_mcp_requires_native_main_terminal_and_clean_process_and_preserves_raw_progress() {
    run(async {
        for mode in [
            "answer",
            "transient",
            "fragmented",
            "final_fragment",
            "nonzero_after_result",
            "startup_error",
            "native_error",
            "malformed",
            "duplicate_keys",
            "truncated",
            "no_result",
            "result_error",
            "duplicate_result",
        ] {
            let mut f = fixture();
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            publish(&f, &ctx).await;
            native_peer(&f);
            let environment = captured(&f, mode);
            attach(&mut f, &ctx, environment).await;
            let (served, record) = serve(&f, &ctx).await;
            let h = McpHarness::connect(&f, &ctx, record.address(), Duration::from_secs(15)).await;
            let result = h.client.peer().call_tool(ask(arguments())).await.unwrap();
            let payload = result.structured_content.unwrap();
            assert_eq!(payload["receipt"]["provider"], "copilot");
            assert_joined(&payload);
            {
                let seen = h.events.lock().unwrap();
                assert!(!seen.iter().any(|seen|matches!(seen,Seen::Progress(progress) if progress.message.as_deref().is_some_and(|message|message.contains("private-native-error-canary")))));
            }
            if matches!(
                mode,
                "answer" | "transient" | "fragmented" | "final_fragment"
            ) {
                assert_ne!(result.is_error, Some(true));
                assert_eq!(payload["status"], "answer");
                assert_eq!(payload["receipt"]["completion"]["answer"], ANSWER);
                assert!(
                    uuid::Uuid::parse_str(payload["receipt"]["session_id"].as_str().unwrap())
                        .is_ok()
                );
                assert_eq!(payload["receipt"]["prompt_id"], "native-prompt");
                assert_eq!(payload["receipt"]["turn_id"], "main-turn");
                assert_success_process(&payload);
                let native: Value = serde_json::from_str(
                    payload["receipt"]["native_result_json"].as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(native["type"], "result");
                assert_eq!(native["exitCode"], 0);
                assert_eq!(native["sessionId"], payload["receipt"]["session_id"]);
                let seen = h.events.lock().unwrap();
                let messages: Vec<_> = seen
                    .iter()
                    .filter_map(|seen| match seen {
                        Seen::Progress(value) => value.message.as_deref(),
                        _ => None,
                    })
                    .collect();
                assert_eq!(payload["receipt"]["accepted_notifications"], messages.len());
                assert!(
                    messages
                        .iter()
                        .any(|message| message.contains("assistant.message_delta"))
                );
                assert!(
                    messages
                        .iter()
                        .any(|message| message.contains("tool-canary"))
                );
                assert!(matches!(seen.last(), Some(Seen::Final)));
            } else {
                assert_eq!(result.is_error, Some(true), "mode={mode}");
                assert!(payload["receipt"]["completion"].get("answer").is_none());
                assert!(!payload.to_string().contains("private-native-error-canary"));
                if mode == "nonzero_after_result" {
                    assert_eq!(payload["receipt"]["process"]["exit_code"], 8);
                    assert_eq!(payload["receipt"]["process"]["status_success"], false);
                }
            }
            h.stop().await;
            stop(&f, served, &record, ctx.snapshot()).await;
        }
    });
}
#[test]
fn compiled_global_stdio_streams_copilot_before_native_terminal_and_process_retirement() {
    run(async {
        let f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let environment = captured(&f, "hold");
        let project = f.projects[0].0.root().to_str().unwrap().to_owned();
        let port_path = f.temp.path().join("gate.port");
        let mut cmd = tokio::process::Command::from(command(&f));
        cmd.current_dir(f.paths.root())
            .args(["mcp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut daemon = cli_daemon::CliDaemon::start(f, NativeBinding::Codex, environment, &ctx)
            .await
            .unwrap();
        let mut child = cmd.spawn().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let changed = Arc::new(tokio::sync::Notify::new());
        let client = ()
            .serve(Observe {
                inner: AsyncRwTransport::new_client(
                    child.stdout.take().unwrap(),
                    child.stdin.take().unwrap(),
                ),
                events: Arc::clone(&events),
                changed: Arc::clone(&changed),
            })
            .await
            .unwrap();
        let mut input = arguments();
        input["project_dir"] = json!(project);
        let handle = client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(ask(input))),
                PeerRequestOptions::with_timeout(Duration::from_secs(30)),
            )
            .await
            .unwrap();
        let completion = tokio::spawn(handle.await_response());
        progress_delta(&events, &changed).await;
        assert!(!completion.is_finished());
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|value| matches!(value, Seen::Final))
        );
        let port = gate_port(&port_path).await;
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/fixture/release"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let ServerResult::CallToolResult(result) =
            tokio::time::timeout(Duration::from_secs(15), completion)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
        else {
            panic!("call tool result")
        };
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["receipt"]["completion"]["answer"], ANSWER);
        assert_joined(&payload);
        assert!(matches!(events.lock().unwrap().last(), Some(Seen::Final)));
        client.cancel().await.unwrap();
        let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        daemon.stop().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
    });
}
#[test]
fn actual_read_denial_prevents_copilot_spawn() {
    run(async {
        let mut f = fixture_support::fixture(
            &[Permission::Read(Module::Ast)],
            Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
            NativeBinding::Codex,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        native_peer(&f);
        let journal = f.temp.path().join("native.journal");
        let environment = captured(&f, "answer");
        attach(&mut f, &ctx, environment).await;
        let (served, record) = serve(&f, &ctx).await;
        let h = McpHarness::connect(&f, &ctx, record.address(), Duration::from_secs(15)).await;
        let result = h.client.peer().call_tool(ask(arguments())).await;
        match result {
            Ok(value) => assert_eq!(value.is_error, Some(true)),
            Err(error) => assert!(error.to_string().contains("unknown tool")),
        };
        assert!(!journal.exists());
        h.stop().await;
        stop(&f, served, &record, ctx.snapshot()).await;
    });
}
#[test]
fn invalidating_real_project_after_copilot_delta_suppresses_delivery() {
    run(async {
        let mut f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let environment = captured(&f, "hold");
        attach(&mut f, &ctx, environment).await;
        let (served, record) = serve(&f, &ctx).await;
        let h = McpHarness::connect(&f, &ctx, record.address(), Duration::from_secs(15)).await;
        let handle = h
            .client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(ask(arguments()))),
                PeerRequestOptions::with_timeout(Duration::from_secs(30)),
            )
            .await
            .unwrap();
        let completion = tokio::spawn(handle.await_response());
        progress_delta(&h.events, &h.changed).await;
        assert!(!completion.is_finished());
        let displaced = f.temp.path().join("displaced-project");
        std::fs::rename(f.projects[0].0.root(), &displaced).unwrap();
        let gate_release = release(&f).await;
        let result = tokio::time::timeout(Duration::from_secs(15), completion)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        std::fs::rename(&displaced, f.projects[0].0.root()).unwrap();
        let ServerResult::CallToolResult(result) = result else {
            panic!("call tool result")
        };
        let payload = result.structured_content.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert!(payload["receipt"].get("native_result_json").is_none());
        assert_joined(&payload);
        // Admission may have retired the child before the control request.
        // Accept a refused/closed gate only after denial and actual joined custody above.
        if let Err(error) = gate_release {
            assert!(error.is_connect() || error.is_request());
        }
        h.stop().await;
        stop(&f, served, &record, ctx.snapshot()).await;
    });
}
#[test]
fn actual_source_shutdown_joins_active_copilot_after_abandoned_request_waiter() {
    run(async {
        let mut f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let environment = captured(&f, "hold");
        attach(&mut f, &ctx, environment).await;
        let (served, record) = serve(&f, &ctx).await;
        let h = McpHarness::connect(&f, &ctx, record.address(), Duration::from_secs(30)).await;
        let request = h
            .client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(ask(arguments()))),
                PeerRequestOptions::with_timeout(Duration::from_secs(30)),
            )
            .await
            .unwrap();
        let completion = tokio::spawn(request.await_response());
        progress_delta(&h.events, &h.changed).await;
        assert!(!completion.is_finished());
        completion.abort();
        let _ = completion.await;
        tokio::time::timeout(Duration::from_secs(15), f.server.shutdown())
            .await
            .unwrap()
            .unwrap();
        h.stop().await;
        tokio::time::timeout(Duration::from_secs(5), served)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        graphit_daemon::source_discovery::remove_discovery(
            &f.auth,
            &record,
            native(),
            ctx.snapshot(),
        )
        .unwrap();
    });
}
