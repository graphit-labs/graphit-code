#![cfg(target_os = "linux")]
//! OpenCode Ask through real Graphit ingress and a controlled native SDK peer.
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
    os::unix::fs::PermissionsExt,
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
                binding: NativeBinding::OpenCode,
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
        NativeBinding::OpenCode,
    )
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
fn native_peer(f: &fixture_support::Fixture) {
    let program = f.temp.path().join("opencode");
    std::fs::copy(env!("CARGO_BIN_EXE_opencode-knowledge-fixture"), &program).unwrap();
    std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o700)).unwrap();
}
fn captured(f: &fixture_support::Fixture, mode: &str) -> Vec<(OsString, OsString)> {
    vec![
        (
            "OPENCODE_CAPTURED".into(),
            "startup literal;$(value)".into(),
        ),
        ("OPENCODE_MODE".into(), mode.into()),
        (
            "OPENCODE_JOURNAL".into(),
            f.temp.path().join("native.journal").into_os_string(),
        ),
    ]
}
fn arguments() -> Value {
    json!({"query":"  literal β\n  ","selected_slugs":["Paris"],"model":"provider/literal;$(opencode)"})
}
async fn journal_event(path: &std::path::Path, event: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if std::fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .any(|line| line == event)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
fn assert_clean(payload: &Value) {
    let process = &payload["receipt"]["process"];
    assert_eq!(process["joined"], true, "{payload}");
    assert_eq!(process["group_absent"], true, "{payload}");
    assert_eq!(process["stdout_eof"], true, "{payload}");
    assert_eq!(process["stderr_eof"], true, "{payload}");
}
#[test]
fn compiled_cli_uses_captured_opencode_program_environment_model_and_documentary_prompt() {
    run(async {
        let f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let environment = captured(&f, "answer");
        let mut cmd = tokio::process::Command::from(command(&f));
        let mut daemon =
            cli_daemon::CliDaemon::start(f, NativeBinding::OpenCode, environment, &ctx)
                .await
                .unwrap();
        cmd.env("PATH", "/unavailable-client-path")
            .env("OPENCODE_CAPTURED", "different caller")
            .args([
                "knowledge",
                "ask",
                "--query",
                "  literal β\n  ",
                "--slug",
                "Paris",
                "--model",
                "provider/literal;$(opencode)",
            ])
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
            .await
            .unwrap()
            .unwrap();
        daemon.stop().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, "  actual β [[Paris]]\n\t\n".as_bytes());
        assert!(output.stderr.is_empty());
    });
}
#[test]
fn http_mcp_uses_observed_opencode_receipts_and_suppresses_nonterminal_or_failed_answers() {
    run(async {
        for mode in [
            "answer",
            "child",
            "nonzero",
            "foreign",
            "native_error",
            "cancelled",
        ] {
            let mut f = fixture();
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            publish(&f, &ctx).await;
            native_peer(&f);
            let environment = captured(&f, mode);
            attach(&mut f, &ctx, environment).await;
            let (served, record) = serve(&f, &ctx).await;
            let h = SdkHarness::connect(&f, &ctx, record.address(), Duration::from_secs(15)).await;
            let result = h.client.peer().call_tool(ask(arguments())).await.unwrap();
            let payload = result.structured_content.unwrap();
            h.stop().await;
            stop(&f, served, &record, ctx.snapshot()).await;
            assert_eq!(
                payload["receipt"]["provider"], "opencode",
                "{mode}: {payload}"
            );
            assert_clean(&payload);
            if matches!(mode, "answer" | "child") {
                assert_ne!(result.is_error, Some(true), "{payload}");
                assert_eq!(payload["status"], "answer");
                assert_eq!(
                    payload["receipt"]["completion"]["answer"],
                    "  actual β [[Paris]]\n\t"
                );
                assert_eq!(
                    payload["receipt"]["accepted_notifications"],
                    if mode == "child" { 2 } else { 1 }
                );
                let native: Value = serde_json::from_str(
                    payload["receipt"]["native_result_json"].as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(native["initialize"]["agentInfo"]["version"], "2.0.0");
                assert_eq!(native["prompt_response"]["stopReason"], "end_turn");
                assert_eq!(payload["receipt"]["process"]["status_success"], true);
            } else {
                assert_eq!(result.is_error, Some(true), "{mode}: {payload}");
                assert!(
                    payload["receipt"]["completion"].get("answer").is_none(),
                    "{mode}: {payload}"
                );
                if mode == "nonzero" {
                    assert_eq!(payload["receipt"]["process"]["exit_code"], 9);
                }
            }
        }
    });
}
#[test]
fn compiled_global_stdio_streams_actual_sdk_progress_before_terminal_and_native_exit() {
    run(async {
        let f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let journal = f.temp.path().join("native.journal");
        let environment = captured(&f, "gate");
        let project = f.projects[0].0.root().to_str().unwrap().to_owned();
        let mut cmd = tokio::process::Command::from(command(&f));
        cmd.current_dir(f.paths.root())
            .args(["mcp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut daemon =
            cli_daemon::CliDaemon::start(f, NativeBinding::OpenCode, environment, &ctx)
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
        journal_event(&journal, "chunk").await;
        progress_count(&events, &changed, 1).await;
        assert!(
            !completion.is_finished(),
            "terminal must wait for fixture gate"
        );
        {
            let seen = events.lock().unwrap();
            let Seen::Progress(progress) = seen
                .iter()
                .find(|event| matches!(event, Seen::Progress(_)))
                .unwrap()
            else {
                unreachable!()
            };
            let text = progress.message.as_deref().unwrap();
            let value: Value = serde_json::from_str(text).unwrap();
            assert_eq!(value["provider"], "opencode");
            assert!(!seen.iter().any(|event| matches!(event, Seen::Final)));
        }
        std::fs::write(journal.with_extension("finish"), b"finish").unwrap();
        let result = tokio::time::timeout(Duration::from_secs(15), completion)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let ServerResult::CallToolResult(result) = result else {
            panic!("tool result")
        };
        let payload = result.structured_content.unwrap();
        client.cancel().await.unwrap();
        let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        daemon.stop().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(payload["receipt"]["provider"], "opencode");
        assert_eq!(payload["receipt"]["accepted_notifications"], 1);
        assert_clean(&payload);
        assert_eq!(
            payload["receipt"]["completion"]["answer"],
            "  actual β [[Paris]]\n\t"
        );
        assert!(matches!(events.lock().unwrap().last(), Some(Seen::Final)));
    });
}
#[test]
fn actual_read_denial_prevents_native_spawn() {
    run(async {
        let mut f = fixture_support::fixture(
            &[Permission::Read(Module::Ast)],
            Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
            NativeBinding::OpenCode,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        native_peer(&f);
        let journal = f.temp.path().join("native.journal");
        let environment = captured(&f, "answer");
        attach(&mut f, &ctx, environment).await;
        let (served, record) = serve(&f, &ctx).await;
        let h = SdkHarness::connect(&f, &ctx, record.address(), Duration::from_secs(15)).await;
        let result = h.client.peer().call_tool(ask(arguments())).await;
        h.stop().await;
        stop(&f, served, &record, ctx.snapshot()).await;
        match result {
            Ok(value) => {
                assert_eq!(value.is_error, Some(true));
                let result = serde_json::to_value(value).unwrap();
                assert!(
                    result["content"].as_array().unwrap().iter().any(|entry| {
                        entry["text"].as_str().is_some_and(|text| {
                            text == "Knowledge Ask authentication or permission denied"
                                || text.starts_with("Knowledge Ask authorization: ")
                        })
                    }),
                    "expected the Source authorization classification: {result}"
                );
            }
            Err(error) => assert!(error.to_string().contains("unknown tool"), "{error}"),
        }
        assert!(
            !journal.exists(),
            "read denial must happen before native spawn"
        );
    });
}
#[test]
fn invalidating_actual_project_after_native_chunk_suppresses_answer_and_native_result() {
    run(async {
        let mut f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let journal = f.temp.path().join("native.journal");
        let environment = captured(&f, "gate");
        attach(&mut f, &ctx, environment).await;
        let (served, record) = serve(&f, &ctx).await;
        let h = SdkHarness::connect(&f, &ctx, record.address(), Duration::from_secs(15)).await;
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
        journal_event(&journal, "chunk").await;
        progress_count(&h.events, &h.changed, 1).await;
        assert!(!completion.is_finished());
        let displaced = f.temp.path().join("displaced-project");
        std::fs::rename(f.projects[0].0.root(), &displaced).unwrap();
        std::fs::write(journal.with_extension("finish"), b"finish").unwrap();
        let result = tokio::time::timeout(Duration::from_secs(15), completion)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        std::fs::rename(&displaced, f.projects[0].0.root()).unwrap();
        let ServerResult::CallToolResult(result) = result else {
            panic!("tool result")
        };
        let payload = result.structured_content.unwrap();
        h.stop().await;
        stop(&f, served, &record, ctx.snapshot()).await;
        assert_eq!(result.is_error, Some(true), "{payload}");
        assert!(
            payload["receipt"]["completion"].get("answer").is_none(),
            "{payload}"
        );
        assert!(
            payload["receipt"].get("native_result_json").is_none(),
            "{payload}"
        );
        assert_eq!(payload["receipt"]["provider"], "opencode");
        assert_clean(&payload);
    });
}

#[test]
fn real_source_shutdown_retires_active_opencode_tree_after_abandoned_shutdown_waiter() {
    run(async {
        let mut f = fixture();
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        native_peer(&f);
        let journal = f.temp.path().join("native.journal");
        let environment = captured(&f, "shutdown");
        attach(&mut f, &ctx, environment).await;
        let (served, record) = serve(&f, &ctx).await;
        let h = SdkHarness::connect(&f, &ctx, record.address(), Duration::from_secs(30)).await;
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
        journal_event(&journal, "descendant_ready").await;
        journal_event(&journal, "chunk").await;
        progress_count(&h.events, &h.changed, 1).await;
        let pid = std::fs::read_to_string(journal.with_extension("descendant.pid"))
            .unwrap()
            .parse::<u32>()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !std::fs::metadata(journal.with_extension("descendant.heartbeat"))
                .is_ok_and(|m| m.len() > 0)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            !completion.is_finished(),
            "Ask must be active before actual Source shutdown"
        );
        assert!(!journal.with_extension("descendant.term").exists());
        assert!(
            !std::fs::read_to_string(&journal)
                .unwrap()
                .lines()
                .any(|line| line == "terminal")
        );
        // This is the production Source -> jobs -> AskService shutdown, not only
        // cancellation of a token. Abandon its first waiter while cleanup is owned.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), f.server.shutdown())
                .await
                .is_err()
        );
        tokio::time::timeout(Duration::from_secs(15), f.server.shutdown())
            .await
            .unwrap()
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(5), completion)
            .await
            .unwrap()
            .unwrap();
        match response {
            Ok(ServerResult::CallToolResult(result)) => {
                assert_eq!(result.is_error, Some(true));
                if let Some(payload) = result.structured_content {
                    assert!(payload["receipt"]["completion"].get("answer").is_none());
                    if payload["receipt"]["process"]["joined"] == true {
                        assert_eq!(payload["receipt"]["process"]["stdout_eof"], true);
                        assert_eq!(payload["receipt"]["process"]["stderr_eof"], true);
                        assert!(
                            payload["receipt"]["process"]["group_absent"] == true
                                || payload["receipt"]["process"]["group_kill_completed"] == true
                        );
                    }
                }
            }
            Ok(_) => panic!("call tool response required"),
            Err(_) => {} // Source shutdown may close the active request without an answer.
        }
        assert_eq!(
            std::fs::read(journal.with_extension("descendant.term")).unwrap(),
            b"TERM"
        );
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => assert_eq!(stat.rsplit_once(") ").unwrap().1.chars().next(), Some('Z')),
            Err(error) => assert!(
                error.kind() == io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()),
                "{error:?}"
            ),
        }
        let heartbeat = std::fs::read(journal.with_extension("descendant.heartbeat")).unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(
            std::fs::read(journal.with_extension("descendant.heartbeat")).unwrap(),
            heartbeat
        );
        assert!(
            !std::fs::read_to_string(&journal)
                .unwrap()
                .lines()
                .any(|line| line == "terminal")
        );
        // Join the actual MCP connection and Source server before retiring discovery.
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
