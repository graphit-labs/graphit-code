#![cfg(target_os = "linux")]
//! Antigravity one-shot through the real CLI/SDK, authorized corpus and native Process.
use crate::agent_fixture;
use crate::knowledge_ask_server as fixture_support;
use crate::knowledge_cli_daemon as cli_daemon;
use graphit_app::source_server::SourceServer;
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
                binding: NativeBinding::Antigravity,
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
async fn progress_count(events: &Mutex<Vec<Seen>>, changed: &tokio::sync::Notify, count: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting = changed.notified();
            tokio::pin!(waiting);
            waiting.as_mut().enable();
            if events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, Seen::Progress(_)))
                .count()
                >= count
            {
                return;
            }
            waiting.await;
        }
    })
    .await
    .unwrap();
}

fn scenario(f: &fixture_support::Fixture, body: &str) {
    let mut data: serde_json::Value = serde_json::from_str(body).unwrap();
    data["kind"] = json!("wire");
    data["protocol"] = json!("antigravity");
    agent_fixture::install(&f.temp.path().join("agy"), data);
}
async fn attach(
    f: &mut fixture_support::Fixture,
    ctx: &RequestContext<'_>,
    extra: Vec<(OsString, OsString)>,
) {
    let mut environment = vec![("PATH".into(), f.temp.path().as_os_str().to_owned())];
    environment.extend(extra);
    f.server
        .attach_ask(
            &graphit_core::config::ConfigSnapshot::new(f.paths.brand(), []).unwrap(),
            environment,
            ctx,
        )
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
        NativeBinding::Antigravity,
    )
}

const SIMPLE: &str = r#"{
  "frames": [
    "{\"event\":\"init\",\"conversation_id\":\"fixture-session\",\"init\":{\"cwd\":\"/captured\",\"tools\":[\"view_file\"],\"permission_mode\":\"request-review\",\"model\":\"observed-model\"}}",
    "{\"event\":\"result\",\"result\":{\"conversation_id\":\"fixture-session\",\"status\":\"SUCCESS\",\"response\":\"actual β [[Paris]]\",\"duration_seconds\":0.001,\"num_turns\":1,\"usage\":{\"input_tokens\":0,\"output_tokens\":0}}}"
  ],
  "checks": {
    "model": "literal model;$(antigravity)",
    "captured": "startup only;$(literal)",
    "cwd_basename": "project",
    "question": "  literal β\n  ",
    "slug": "Paris"
  }
}"#;
#[test]
fn compiled_cli_antigravity_keeps_captured_launcher_and_literal_model_context() {
    run(async {
        let f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        scenario(&f, SIMPLE);
        let mut cmd = tokio::process::Command::from(command(&f));
        let mut daemon = cli_daemon::CliDaemon::start(
            f,
            NativeBinding::Antigravity,
            vec![("CAPTURED".into(), "startup only;$(literal)".into())],
            &ctx,
        )
        .await
        .unwrap();
        cmd.env("PATH", "/unavailable-client-path")
            .env("CAPTURED", "different caller")
            .args([
                "knowledge",
                "ask",
                "--query",
                "  literal β\n  ",
                "--slug",
                "Paris",
                "--model",
                "literal model;$(antigravity)",
            ]);
        let output = cmd.output().await.unwrap();
        assert!(
            output.status.success(),
            "CLI failure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, "actual β [[Paris]]\n".as_bytes());
        assert!(output.stderr.is_empty());
        daemon.stop().await.unwrap();
    });
}

#[test]
fn compiled_global_stdio_antigravity_streams_raw_native_events_before_real_exit() {
    run(async {
        use tokio::io::AsyncWriteExt;
        let f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        let gate = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        scenario(
            &f,
            r#"{
  "frames": [
    "{\"event\":\"init\",\"conversation_id\":\"fixture-session\",\"init\":{\"cwd\":\"/captured\",\"tools\":[\"view_file\"],\"permission_mode\":\"request-review\",\"model\":\"observed-model\"}}",
    "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"fixture-session\",\"step_index\":1,\"state\":\"ACTIVE\",\"step_type\":\"tool\",\"tool_name\":\"view_file\",\"tool_info\":{\"parameters\":{\"big\":9007199254740993,\"big\":9007199254740995,\"ratio\":1e-4}}}}",
    "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"fixture-session\",\"step_index\":1,\"state\":\"DONE\",\"step_type\":\"tool\",\"tool_name\":\"view_file\",\"tool_info\":{\"output\":\"native output\"}}}",
    "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"fixture-session\",\"step_index\":2,\"state\":\"DONE\",\"step_type\":\"tool\",\"tool_name\":\"delegate\",\"subagent_info\":{\"subagents\":[{\"type_name\":\"research\",\"role\":\"read\",\"conversation_id\":\"native-subagent\"}]},\"text_delta\":\"injected response\"}}",
    "{\"event\":\"future\",\"extra\":null}",
    "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"fixture-session\",\"step_index\":4,\"state\":\"ACTIVE\",\"step_type\":\"agent_response\",\"text_delta\":\"partial answer β\"}}",
    "{\"event\":\"result\",\"result\":{\"conversation_id\":\"fixture-session\",\"status\":\"SUCCESS\",\"response\":\"native answer β\",\"duration_seconds\":0.001,\"num_turns\":1,\"usage\":{\"input_tokens\":5,\"output_tokens\":3}}}"
  ],
  "gate": {
    "before": 6,
    "send": "",
    "expect": "1"
  },
  "checks": {
    "model": "model fixture",
    "body": "Paris é a capital.\n"
  }
}"#,
        );
        let mut cmd = tokio::process::Command::from(command(&f));
        cmd.current_dir(f.paths.root());
        let mut daemon = cli_daemon::CliDaemon::start(
            f,
            NativeBinding::Antigravity,
            vec![(
                "GATE_PORT".into(),
                gate.local_addr().unwrap().port().to_string().into(),
            )],
            &ctx,
        )
        .await
        .unwrap();
        // Global transport uses the selection supplied by Source discovery.
        // PROJECT startup/MaintainCheckout is covered by its own journey.
        cmd.args(["mcp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let changed = Arc::new(tokio::sync::Notify::new());
        let transport = Observe {
            inner: AsyncRwTransport::new_client(
                child.stdout.take().unwrap(),
                child.stdin.take().unwrap(),
            ),
            events: Arc::clone(&events),
            changed: Arc::clone(&changed),
        };
        let client = match ().serve(transport).await {
            Ok(client) => client,
            Err(error) => {
                let output =
                    tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
                        .await
                        .expect("failed CLI exits after initialize closure")
                        .unwrap();
                panic!(
                    "Root MCP initialize: {error}; status: {}; stderr: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        };
        let handle = client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(ask(
                    json!({"query":"question","selected_slugs":["Paris"],"model":"model fixture"}),
                ))),
                PeerRequestOptions::with_timeout(Duration::from_secs(30)),
            )
            .await
            .unwrap();
        let token = handle.progress_token.clone();
        let final_wait = tokio::spawn(handle.await_response());
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), gate.accept())
            .await
            .unwrap()
            .unwrap();
        progress_count(&events, &changed, 5).await;
        assert!(
            !final_wait.is_finished(),
            "terminal delivered before gated child exit"
        );
        {
            let seen = events.lock().unwrap();
            let messages: Vec<_> = seen
                .iter()
                .filter_map(|event| match event {
                    Seen::Progress(p) => Some(p),
                    _ => None,
                })
                .collect();
            assert!(messages.iter().all(|p| p.progress_token == token));
            let raw = messages[1].message.as_ref().unwrap();
            assert!(raw.contains(
                "\"tool_info\":{\"parameters\":{\"big\":9007199254740993,\"big\":9007199254740995,\"ratio\":1e-4}}"
            ));
            assert!(raw.contains("\"provider\":\"antigravity\""));
        }
        socket.write_all(b"1").await.unwrap();
        let ServerResult::CallToolResult(result) = final_wait.await.unwrap().unwrap() else {
            panic!("wrong native response")
        };
        assert_eq!(result.is_error, Some(false));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["receipt"]["provider"], "antigravity");
        assert_eq!(payload["receipt"]["known_events"], 6);
        assert_eq!(payload["receipt"]["frames"], 7);
        assert_eq!(
            payload["receipt"]["completion"]["answer"],
            "native answer β"
        );
        let native: serde_json::Value =
            serde_json::from_str(payload["receipt"]["native_result_json"].as_str().unwrap())
                .unwrap();
        assert_eq!(native["result"]["usage"]["output_tokens"], 3);
        assert_eq!(payload["receipt"]["process"]["cleanup"], "Complete");
        assert_eq!(
            payload["receipt"]["observed_init"]["model"],
            "observed-model"
        );
        assert!(matches!(events.lock().unwrap().last(), Some(Seen::Final)));
        client.cancel().await.unwrap();
        let output = child.wait_with_output().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        daemon.stop().await.unwrap();
    });
}

struct SdkHarness {
    proxy: SourceProxy,
    joined: tokio::task::JoinHandle<io::Result<()>>,
    client: rmcp::service::RunningService<RoleClient, ()>,
    events: Arc<Mutex<Vec<Seen>>>,
    changed: Arc<tokio::sync::Notify>,
}
impl SdkHarness {
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

#[test]
fn antigravity_http_receipts_use_native_result_and_retain_real_failures() {
    run(async {
        for mode in [
            "answer",
            "empty",
            "missing_text",
            "native_error",
            "late_error",
            "nonzero",
            "malformed",
            "missing",
            "wrong_usage",
        ] {
            let mut f = fixture();
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            publish(&f, &ctx).await;
            let mut frames = vec![String::from(
                "{\"event\":\"init\",\"conversation_id\":\"fixture\",\"init\":{\"model\":\"observed\"}}",
            )];
            if mode == "malformed" {
                frames.push(String::from("{bad-json}"));
            } else if mode != "missing" {
                let mut result = json!({"conversation_id":"fixture", "status":"SUCCESS", "duration_seconds":0.001,"num_turns":1,"usage":{"input_tokens":2,"output_tokens":1}});
                if mode == "native_error" {
                    result["status"] = json!("ERROR");
                    result["error"] = json!("native failure");
                }
                if mode == "wrong_usage" {
                    result["usage"] = json!([]);
                }
                if !matches!(mode, "missing_text" | "native_error") {
                    result["response"] = json!(if mode == "empty" { "" } else { "native answer" });
                }
                let envelope = json!({"event":"result", "result":result});
                frames.push(envelope.to_string());
            }
            if mode == "late_error" {
                let native_error = json!({"event":"result","result":{"conversation_id":"fixture","status":"ERROR","error":"failure after terminal"}});
                frames.push(native_error.to_string());
            }
            scenario(
                &f,
                &json!({
                    "frames":frames,
                    "exit":if mode == "nonzero" {9} else {0}
                })
                .to_string(),
            );
            attach(&mut f, &ctx, vec![]).await;
            let (served, record) = serve(&f, &ctx).await;
            let h = SdkHarness::connect(&f, &ctx, record.address(), Duration::from_secs(10)).await;
            let result = h
                .client
                .peer()
                .call_tool(ask(
                    serde_json::json!({"query":"q","selected_slugs":["Paris"],"model":"literal"}),
                ))
                .await
                .unwrap();
            let payload = result.structured_content.unwrap();
            let receipt = &payload["receipt"];
            assert_eq!(receipt["provider"], "antigravity");
            assert_eq!(receipt["process"]["cleanup"], "Complete");
            if matches!(mode, "answer" | "empty") {
                assert_ne!(result.is_error, Some(true), "{mode}: {payload}");
                assert_eq!(
                    payload["status"],
                    match mode {
                        "answer" => "answer",
                        "empty" => "empty_answer",
                        _ => "evidence_unavailable",
                    }
                );
                let native: serde_json::Value =
                    serde_json::from_str(receipt["native_result_json"].as_str().unwrap()).unwrap();
                assert_eq!(native["result"]["usage"]["input_tokens"], 2);
            } else {
                assert_eq!(result.is_error, Some(true), "{mode}: {payload}");
                assert!(receipt["completion"].get("answer").is_none());
            }
            if mode == "nonzero" {
                assert_eq!(receipt["process"]["exit_code"], 9);
                assert_eq!(payload["execution_error"], "NonzeroExit");
            }
            if matches!(mode, "missing_text" | "wrong_usage") {
                assert_eq!(payload["execution_error"], "Wire");
            }
            if mode == "late_error" {
                assert_eq!(receipt["known_events"], 2);
                assert!(receipt["native_result_json"].is_null());
                assert_eq!(payload["execution_error"], "State");
            }
            if mode == "native_error" {
                assert_eq!(payload["execution_error"], "Provider");
            }
            h.stop().await;
            stop(&f, served, &record, ctx.snapshot()).await;
        }
    });
}
#[test]
fn antigravity_prechild_validation_denial_empty_and_unavailable_never_execute() {
    run(async {
        for mode in [
            "validations",
            "denied",
            "no_corpus",
            "published_empty",
            "empty_selection",
            "unavailable",
        ] {
            let mut f = if mode == "denied" {
                fixture_support::fixture(
                    &[Permission::Read(Module::Ast)],
                    Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
                    NativeBinding::Antigravity,
                )
            } else {
                fixture()
            };
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            if !matches!(mode, "no_corpus" | "denied") {
                publish_corpus(&f, &ctx, mode == "published_empty").await;
            }
            let marker = f.temp.path().join("unexpected-child");
            if mode != "unavailable" {
                scenario(&f, &json!({"frames":[],"marker":marker}).to_string());
            }
            attach(&mut f, &ctx, vec![]).await;
            let (served, record) = serve(&f, &ctx).await;
            let h = SdkHarness::connect(&f, &ctx, record.address(), Duration::from_secs(10)).await;
            let listed = h
                .client
                .peer()
                .list_all_tools()
                .await
                .unwrap()
                .iter()
                .any(|t| t.name == "graphit_knowledge_ask");
            assert_eq!(listed, mode != "unavailable");
            let mut cases = vec![json!({"query":"q","selected_slugs":["Paris"],"model":"m"})];
            if mode == "empty_selection" {
                cases = vec![json!({"query":"q","selected_slugs":[],"model":"m"})];
            }
            if mode == "validations" {
                cases = vec![
                    json!({"query":"q","selected_slugs":["missing"],"model":"m"}),
                    json!({"query":"q","selected_slugs":["Paris","Paris"],"model":"m"}),
                    json!({"query":" ","selected_slugs":["Paris"],"model":"m"}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":""}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","extra":true}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","project_dir":"/foreign"}),
                ];
            }
            for arguments in cases {
                let result = match h.client.peer().call_tool(ask(arguments)).await {
                    Ok(result) => result,
                    Err(error) => {
                        assert!(
                            matches!(mode, "validations" | "denied" | "unavailable"),
                            "{mode}: {error}"
                        );
                        assert!(
                            error.to_string().contains("unknown tool"),
                            "{mode}: {error}"
                        );
                        continue;
                    }
                };
                if matches!(mode, "no_corpus" | "published_empty" | "empty_selection") {
                    assert_ne!(result.is_error, Some(true));
                    assert_eq!(result.structured_content.as_ref().unwrap()["status"], mode);
                } else {
                    assert_eq!(result.is_error, Some(true), "{mode}");
                }
                assert!(
                    result
                        .structured_content
                        .as_ref()
                        .is_none_or(|v| v.get("receipt").is_none())
                );
            }
            assert!(!marker.exists());
            h.stop().await;
            stop(&f, served, &record, ctx.snapshot()).await;
        }
    });
}
#[test]
fn antigravity_http_cancel_drop_deadline_and_dropped_shutdown_waiter_join_child() {
    run(async {
        use tokio::io::AsyncReadExt;
        for mode in ["cancel", "eof", "deadline", "shutdown"] {
            let mut f = fixture();
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            publish(&f, &ctx).await;
            let gate = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            scenario(
                &f,
                r#"{
  "frames": [
    "{\"event\":\"init\",\"conversation_id\":\"fixture\",\"init\":{\"cwd\":\"/captured\",\"tools\":[\"view_file\"],\"permission_mode\":\"request-review\",\"model\":\"observed\"}}",
    "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"fixture\",\"step_index\":4,\"state\":\"ACTIVE\",\"step_type\":\"agent_response\",\"text_delta\":\"must-not-be-answer\"}}",
    "{\"event\":\"result\",\"result\":{\"conversation_id\":\"fixture\",\"status\":\"SUCCESS\",\"response\":\"must-not-be-answer\",\"duration_seconds\":0.001,\"num_turns\":1,\"usage\":{\"input_tokens\":0,\"output_tokens\":0}}}"
  ],
  "gate": {
    "before": 2,
    "send": "w",
    "expect": null
  }
}"#,
            );
            attach(
                &mut f,
                &ctx,
                vec![(
                    "GATE_PORT".into(),
                    gate.local_addr().unwrap().port().to_string().into(),
                )],
            )
            .await;
            let (served, record) = serve(&f, &ctx).await;
            let h = SdkHarness::connect(
                &f,
                &ctx,
                record.address(),
                if mode == "deadline" {
                    Duration::from_secs(2)
                } else {
                    Duration::from_secs(10)
                },
            )
            .await;
            let handle = h
                .client
                .peer()
                .send_request_with_option(
                    ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(ask(
                        json!({"query":"q","selected_slugs":["Paris"],"model":"m"}),
                    ))),
                    PeerRequestOptions::with_timeout(Duration::from_secs(15)),
                )
                .await
                .unwrap();
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), gate.accept())
                .await
                .unwrap()
                .unwrap();
            progress_count(&h.events, &h.changed, 2).await;
            let SdkHarness {
                proxy,
                joined,
                client,
                ..
            } = h;
            let mut client = Some(client);
            match mode {
                "cancel" => {
                    handle
                        .cancel(Some("fixture cancellation".into()))
                        .await
                        .unwrap();
                }
                "eof" => {
                    client.take().unwrap().cancel().await.unwrap();
                    drop(handle);
                }
                "deadline" => {
                    let ServerResult::CallToolResult(result) =
                        handle.await_response().await.unwrap()
                    else {
                        panic!("tool response")
                    };
                    assert_eq!(result.is_error, Some(true));
                    assert!(
                        result
                            .structured_content
                            .as_ref()
                            .is_none_or(|v| v["receipt"]["completion"].get("answer").is_none())
                    );
                }
                "shutdown" => {
                    {
                        let waiter = proxy.shutdown();
                        tokio::pin!(waiter);
                        let first = std::future::poll_fn(|cx| {
                            std::task::Poll::Ready(waiter.as_mut().poll(cx))
                        })
                        .await;
                        assert!(
                            first.is_pending(),
                            "drop only an actual pending shutdown waiter"
                        );
                    }
                    proxy.shutdown().await.unwrap();
                    drop(handle);
                }
                _ => unreachable!(),
            }
            if let Some(client) = client {
                client.cancel().await.unwrap();
            }
            tokio::time::timeout(Duration::from_secs(10), joined)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            proxy.shutdown().await.unwrap();
            stop(&f, served, &record, ctx.snapshot()).await;
            let mut bytes = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut bytes))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                bytes, b"w",
                "child exited without receiving fixture release"
            );
        }
    });
}

#[test]
fn antigravity_project_invalidation_after_native_effect_denies_delivery_keeps_receipt() {
    run(async {
        use tokio::io::AsyncWriteExt;
        let mut f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        let gate = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        scenario(
            &f,
            r#"{
  "frames": [
    "{\"event\":\"init\",\"conversation_id\":\"fixture\",\"init\":{\"cwd\":\"/captured\",\"tools\":[\"view_file\"],\"permission_mode\":\"request-review\",\"model\":\"observed\"}}",
    "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"fixture\",\"step_index\":4,\"state\":\"ACTIVE\",\"step_type\":\"agent_response\",\"text_delta\":\"native effect before invalidation\"}}",
    "{\"event\":\"step_update\",\"step_update\":{\"conversation_id\":\"fixture\",\"step_index\":5,\"state\":\"ACTIVE\",\"step_type\":\"agent_response\",\"text_delta\":\"fixture progress boundary\"}}",
    "{\"event\":\"result\",\"result\":{\"conversation_id\":\"fixture\",\"status\":\"SUCCESS\",\"response\":\"must-not-be-answer\",\"duration_seconds\":0.001,\"num_turns\":1,\"usage\":{\"input_tokens\":5,\"output_tokens\":3}}}"
  ],
  "gate": {
    "before": 3,
    "send": "",
    "expect": "1"
  }
}"#,
        );
        attach(
            &mut f,
            &ctx,
            vec![(
                "GATE_PORT".into(),
                gate.local_addr().unwrap().port().to_string().into(),
            )],
        )
        .await;
        let (served, record) = serve(&f, &ctx).await;
        let h = SdkHarness::connect(&f, &ctx, record.address(), Duration::from_secs(10)).await;
        let handle = h
            .client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(ask(
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m"}),
                ))),
                PeerRequestOptions::with_timeout(Duration::from_secs(15)),
            )
            .await
            .unwrap();
        let final_wait = tokio::spawn(handle.await_response());
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), gate.accept())
            .await
            .unwrap()
            .unwrap();
        // Progress delivery precedes the sink's second authorization check.
        // Seeing the third callback proves that the first two were accepted.
        progress_count(&h.events, &h.changed, 3).await;
        assert!(!final_wait.is_finished());
        let displaced = f.temp.path().join("displaced-project");
        std::fs::rename(f.projects[0].0.root(), &displaced).unwrap();
        socket.write_all(b"1").await.unwrap();
        let ServerResult::CallToolResult(result) = final_wait.await.unwrap().unwrap() else {
            panic!("tool response")
        };
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert_eq!(payload["receipt"]["provider"], "antigravity");
        let accepted = payload["receipt"]["known_events"].as_u64().unwrap();
        assert!((2..=3).contains(&accepted), "receipt: {payload}");
        assert_eq!(payload["receipt"]["frames"], accepted + 1);
        assert!(payload["receipt"].get("native_result_json").is_none());
        assert_eq!(payload["receipt"]["process"]["cleanup"], "Complete");
        assert_eq!(payload["execution_error"], "Sink(Closed)");
        let denial = if accepted == 2 {
            // The proxy also denies the forwarded third event if its post-check failed.
            "Knowledge Ask observed progress exceeds accepted prefix"
        } else {
            "ProjectDeliveryDenied"
        };
        assert_eq!(payload["delivery_error"], denial);
        std::fs::rename(&displaced, f.projects[0].0.root()).unwrap();
        h.stop().await;
        stop(&f, served, &record, ctx.snapshot()).await;
    });
}
