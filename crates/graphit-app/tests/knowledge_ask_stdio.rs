#![cfg(target_os = "linux")]
//! Real SDK STDIO over the compiled App, authenticated HTTP, FULL16 and native Process.
use crate::agent_fixture as native_agent_fixture;
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
        &pages,
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
    let mut arguments = arguments.as_object().unwrap().clone();
    arguments
        .entry("ai_optimized")
        .or_insert(Value::Bool(false));
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
fn peer(f: &fixture_support::Fixture, scenario: Value) -> std::path::PathBuf {
    let program = f.temp.path().join("codex");
    native_agent_fixture::install(&program, scenario);
    program
}
fn terminal() -> String {
    r#"{"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}"#.to_owned()
}
async fn attach(
    f: &mut fixture_support::Fixture,
    ctx: &RequestContext<'_>,
    extra: Vec<(OsString, OsString)>,
) {
    let mut env = vec![("PATH".into(), f.temp.path().as_os_str().to_owned())];
    env.extend(extra);
    f.server
        .attach_ask(
            &graphit_core::config::ConfigSnapshot::new(f.paths.brand(), []).unwrap(),
            env,
            ctx,
        )
        .await
        .unwrap();
}
#[test]
fn compiled_stdio_preserves_native_progress_raw_numbers_token_and_final_order() {
    run(async {
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let f = fixture_support::fixture(
            &[
                Permission::Read(Module::Ast),
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ],
            brand,
            NativeBinding::Codex,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        let gate = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = gate.local_addr().unwrap().port();
        peer(
            &f,
            json!({
                "kind":"wire","protocol":"codex", "checks":{
                    "args":["exec","--json","--model","synthetic-secret-model;$(literal)","-"],
                    "cwd_basename":"project", "question":"  synthetic-secret-query β\n  ", "slug":"Paris"
                },
                "frames":[
                    r#"{"type":"turn.started"}"#,
                    r#"{"type":"item.completed","item":{"id":"m","type":"mcp_tool_call","server":"server","tool":"tool","arguments":{"n":1e400,"z":-0,"big":123456789012345678901234567890},"status":"completed","result":{"content":[],"structured_content":{"n":1e400}}}}"#,
                    r#"{"type":"item.completed","item":{"id":"outer","type":"web_search","id":"inner","query":"synthetic-secret-web","action":{"type":"search","query":"q"},"results":[1e400,-0]}}"#,
                    r#"{"type":"item.completed","item":{"id":"a","type":"agent_message","text":"synthetic-secret-answer β [[Paris]]"}}"#,
                    terminal()
                ], "gate":{"before":3,"send":"waiting","expect":"x"}, "marker":null,"exit":0
            }),
        );
        let mut cmd = tokio::process::Command::from(command(&f));
        let mut daemon = cli_daemon::CliDaemon::start(
            f,
            NativeBinding::Codex,
            vec![("GATE_PORT".into(), port.to_string().into())],
            &ctx,
        )
        .await
        .unwrap();
        cmd.args(["mcp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
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
        let client = ().serve(transport).await.unwrap();
        let tools = client.peer().list_all_tools().await.unwrap();
        assert!(tools.iter().any(|t| t.name == "graphit_ast_source"));
        assert!(tools.iter().any(|t| t.name == "graphit_knowledge_ask"));
        let source = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source")
                    .with_arguments(json!({"path":"raw"}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            source.content[0].as_text().unwrap().text,
            "actual native payload"
        );
        events.lock().unwrap().clear();
        let mut call = ask(
            json!({"query":"  synthetic-secret-query β\n  ","selected_slugs":["Paris"],"model":"synthetic-secret-model;$(literal)"}),
        );
        call.meta = Some(serde_json::from_value(json!({"caller":"literal-metadata"})).unwrap());
        let handle = client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(call)),
                PeerRequestOptions::with_timeout(Duration::from_secs(20)),
            )
            .await
            .unwrap();
        let upstream = handle.progress_token.clone();
        let final_wait = tokio::spawn(handle.await_response());
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), gate.accept())
            .await
            .unwrap()
            .unwrap();
        progress_count(&events, &changed, 3).await;
        assert!(!final_wait.is_finished());
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, Seen::Final))
        );
        use tokio::io::AsyncWriteExt;
        socket.write_all(b"x").await.unwrap();
        let ServerResult::CallToolResult(result) = final_wait.await.unwrap().unwrap() else {
            panic!("native tool result required")
        };
        assert_ne!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["receipt"]["known_events"], 5);
        assert_eq!(
            payload["receipt"]["completion"]["answer"],
            "synthetic-secret-answer β [[Paris]]"
        );
        assert_eq!(
            payload["receipt"]["completion"]["usage"]["output_tokens"],
            3
        );
        {
            let observed = events.lock().unwrap();
            let progress: Vec<_> = observed
                .iter()
                .filter_map(|e| {
                    if let Seen::Progress(p) = e {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                progress.iter().map(|p| p.progress).collect::<Vec<_>>(),
                [1.0, 2.0, 3.0, 4.0, 5.0]
            );
            assert!(progress.iter().all(|p| p.progress_token == upstream));
            let message = progress[1].message.as_ref().unwrap();
            assert!(message.contains("1e400"));
            assert!(message.contains(":-0"));
            assert!(message.contains("123456789012345678901234567890"));
            assert!(progress[2].message.as_ref().unwrap().contains("1e400"));
            assert!(matches!(observed.last(), Some(Seen::Final)));
        }
        // A valid explicit version still receives the missing current root literally.
        let mut toon_call = ask(
            json!({"query":"  synthetic-secret-query β\n  ","selected_slugs":["Paris"],"model":"synthetic-secret-model;$(literal)","project_version":"main"}),
        );
        toon_call.arguments.as_mut().unwrap().remove("ai_optimized");
        let request = client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(toon_call)),
                PeerRequestOptions::with_timeout(Duration::from_secs(20)),
            )
            .await
            .unwrap();
        let response = tokio::spawn(request.await_response());
        let (mut selected, _) = tokio::time::timeout(Duration::from_secs(10), gate.accept())
            .await
            .unwrap()
            .unwrap();
        selected.write_all(b"x").await.unwrap();
        let ServerResult::CallToolResult(selected) = response.await.unwrap().unwrap() else {
            panic!("selected native tool response")
        };
        assert_ne!(selected.is_error, Some(true));
        assert!(selected.structured_content.is_none());
        let toon = &selected.content[0].as_text().unwrap().text;
        assert!(toon.contains("receipt:"));
        assert!(toon.contains("output_tokens: 3"));
        assert!(toon.contains("synthetic-secret-answer β [[Paris]]"));
        client.cancel().await.unwrap();
        let output = cli_daemon::child_output(child, Duration::from_secs(10)).await;
        assert!(output.status.success());
        for marker in [
            "synthetic-secret-query",
            "synthetic-secret-model",
            "synthetic-secret-web",
            "synthetic-secret-answer",
        ] {
            assert!(!String::from_utf8_lossy(&output.stderr).contains(marker));
        }
        daemon.stop().await.unwrap();
    });
}
fn kiro_stdio(binding: NativeBinding) {
    run(async move {
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let f = fixture_support::fixture(
            &[
                Permission::Read(Module::Ast),
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ],
            brand,
            binding,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        publish(&f, &ctx).await;
        std::os::unix::fs::symlink(
            env!("CARGO_BIN_EXE_kiro-knowledge-fixture"),
            f.temp.path().join("kiro-cli"),
        )
        .unwrap();
        let journal = f.temp.path().join("kiro-journal");
        let mut cmd = tokio::process::Command::from(command(&f));
        let mut daemon = cli_daemon::CliDaemon::start(
            f,
            binding,
            vec![
                ("KIRO_CAPTURED".into(), "literal-captured".into()),
                (
                    "KIRO_FIXTURE_MODEL".into(),
                    "literal model;$(native)".into(),
                ),
                ("KIRO_FIXTURE_MODE".into(), "stream_gate".into()),
                (
                    "KIRO_FIXTURE_JOURNAL".into(),
                    journal.clone().into_os_string(),
                ),
            ],
            &ctx,
        )
        .await
        .unwrap();
        cmd.args(["mcp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
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
        let tools = client.peer().list_all_tools().await.unwrap();
        assert!(tools.iter().any(|t| t.name == "graphit_ast_source"));
        assert!(tools.iter().any(|t| t.name == "graphit_knowledge_ask"));
        let handle = client
            .peer()
            .send_request_with_option(
                ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(ask(json!({
                    "query":"  literal β\n  ", "selected_slugs":["Paris"],
                    "model":"literal model;$(native)"
                })))),
                PeerRequestOptions::with_timeout(Duration::from_secs(20)),
            )
            .await
            .unwrap();
        let token = handle.progress_token.clone();
        let final_wait = tokio::spawn(handle.await_response());
        // Await the actual wire progress before releasing the native terminal gate.
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let waiting = changed.notified();
                tokio::pin!(waiting);
                waiting.as_mut().enable();
                let ready = events.lock().unwrap().iter().any(|event| {
                    matches!(event, Seen::Progress(p) if p.message.as_deref().is_some_and(|m| m.contains("fresh fixture answer")))
                });
                if ready { break; }
                assert!(!final_wait.is_finished(), "terminal before gated native progress");
                waiting.await;
            }
        }).await.unwrap();
        assert!(!final_wait.is_finished());
        assert!(
            !std::fs::read_to_string(&journal)
                .unwrap()
                .lines()
                .any(|line| line == "terminal")
        );
        {
            let seen = events.lock().unwrap();
            assert!(!seen.iter().any(|e| matches!(e, Seen::Final)));
            assert!(seen.iter().any(|e| matches!(e, Seen::Progress(p) if p.message.as_deref().is_some_and(|m| m.contains("literal-tool-id") && m.contains("native")))));
        }
        std::fs::write(journal.with_extension("finish"), b"finish").unwrap();
        let ServerResult::CallToolResult(result) = final_wait.await.unwrap().unwrap() else {
            panic!("native Kiro tool result required")
        };
        assert_ne!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["receipt"]["provider"], "kiro");
        assert_eq!(
            payload["receipt"]["completion"]["answer"],
            "fresh fixture answer"
        );
        assert_eq!(payload["receipt"]["process"]["joined"], true);
        assert!(payload["receipt"].get("known_events").is_none());
        assert!(!payload.to_string().contains("private-kiro-fixture-session"));
        {
            let seen = events.lock().unwrap();
            let progress: Vec<_> = seen
                .iter()
                .filter_map(|e| {
                    if let Seen::Progress(p) = e {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                payload["receipt"]["accepted_notifications"]
                    .as_u64()
                    .unwrap() as usize,
                progress.len()
            );
            assert!(
                progress
                    .iter()
                    .enumerate()
                    .all(|(i, p)| p.progress == (i + 1) as f64 && p.progress_token == token)
            );
            assert!(progress.iter().all(|p| {
                !p.message
                    .as_deref()
                    .unwrap()
                    .contains("private-kiro-fixture-session")
            }));
            assert!(matches!(seen.last(), Some(Seen::Final)));
        }
        client.cancel().await.unwrap();
        let output = cli_daemon::child_output(child, Duration::from_secs(10)).await;
        assert!(output.status.success());
        for marker in [
            "private-kiro-fixture-session",
            "fresh fixture answer",
            "literal model;$(native)",
        ] {
            assert!(!String::from_utf8_lossy(&output.stderr).contains(marker));
        }
        daemon.stop().await.unwrap();
    });
}

#[test]
fn compiled_stdio_kiro_v3_progress_precedes_terminal_and_preserves_real_receipt() {
    kiro_stdio(NativeBinding::KiroV3);
}
#[test]
fn unoffered_ask_preserves_source_and_never_launches_child() {
    run(async {
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let f = fixture_support::fixture(
            &[
                Permission::Read(Module::Ast),
                Permission::Read(Module::Knowledge),
            ],
            brand,
            NativeBinding::Codex,
        );
        let cancel = Cancellation::default();
        let ctx = original(&cancel);
        let marker = f.temp.path().join("child-ran");
        peer(
            &f,
            json!({"kind":"wire","protocol":"codex","frames":[],
                "checks":{},"gate":null,"marker":marker,"exit":0}),
        );
        // No attach means the actual service offers no Ask capability.
        let (served, record) = serve(&f, &ctx).await;
        let proxy = SourceProxy::new(
            f.paths.clone(),
            record.address(),
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
                Duration::from_secs(10),
            ),
            Some((f.projects[0].0.root(), Some("main"))),
            &ctx,
        )
        .await
        .unwrap();
        let (local, remote) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(remote);
        let owner = proxy.clone();
        let joined = tokio::spawn(async move { owner.serve_stream(read, write).await });
        let client = ().serve(local).await.unwrap();
        assert!(
            !client
                .peer()
                .list_all_tools()
                .await
                .unwrap()
                .iter()
                .any(|t| t.name == "graphit_knowledge_ask")
        );
        // Offered Ask selection denials are exercised separately. An unoffered
        // tool fails at the actual MCP catalogue boundary before input validation.
        let error = client
            .peer()
            .call_tool(ask(
                json!({"query":"q","selected_slugs":["Paris"],"model":"m"}),
            ))
            .await
            .unwrap_err();
        let rmcp::ServiceError::McpError(error) = error else {
            panic!("unoffered Ask must return a typed MCP error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::METHOD_NOT_FOUND);
        let source = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source")
                    .with_arguments(json!({"path":"raw"}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            source.content[0].as_text().unwrap().text,
            "actual native payload"
        );
        assert!(!marker.exists());
        client.cancel().await.unwrap();
        joined.await.unwrap().unwrap();
        proxy.shutdown().await.unwrap();
        stop(&f, served, &record, ctx.snapshot()).await;
    });
}

#[test]
fn offered_ask_auth_and_selection_denials_are_real_and_do_not_launch_child() {
    run(async {
        for denied in [false, true] {
            let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
            let mut permissions = vec![
                Permission::Read(Module::Ast),
                Permission::MutateModule(Module::Knowledge),
            ];
            if !denied {
                permissions.push(Permission::Read(Module::Knowledge));
            }
            let mut f = fixture_support::fixture(&permissions, brand, NativeBinding::Codex);
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            publish(&f, &ctx).await;
            let marker = f.temp.path().join("child-ran");
            peer(
                &f,
                json!({"kind":"wire","protocol":"codex","frames":[],
                "checks":{},"gate":null,"marker":marker,"exit":0}),
            );
            attach(&mut f, &ctx, vec![]).await;
            let (served, record) = serve(&f, &ctx).await;
            let proxy = SourceProxy::new(
                f.paths.clone(),
                record.address(),
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
                    Duration::from_secs(10),
                ),
                Some((f.projects[0].0.root(), None)),
                &ctx,
            )
            .await
            .unwrap();
            let (local, remote) = tokio::io::duplex(65536);
            let (read, write) = tokio::io::split(remote);
            let owner = proxy.clone();
            let joined = tokio::spawn(async move { owner.serve_stream(read, write).await });
            let client = ().serve(local).await.unwrap();
            assert!(
                client
                    .peer()
                    .list_all_tools()
                    .await
                    .unwrap()
                    .iter()
                    .any(|t| t.name == "graphit_knowledge_ask")
            );
            let cases = if denied {
                vec![json!({"query":"q","selected_slugs":["Paris"],"model":"m"})]
            } else {
                vec![
                    json!({"query":"q","selected_slugs":["missing"],"model":"m"}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","project_version":null}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","project_dir":"/foreign"}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","project_dir":null}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","extra":true}),
                ]
            };
            for args in cases {
                let result = client.peer().call_tool(ask(args)).await.unwrap();
                assert_eq!(result.is_error, Some(true));
                assert!(
                    result
                        .structured_content
                        .as_ref()
                        .is_none_or(|v| v.get("receipt").is_none())
                );
            }
            assert!(!marker.exists());
            client.cancel().await.unwrap();
            joined.await.unwrap().unwrap();
            proxy.shutdown().await.unwrap();
            stop(&f, served, &record, ctx.snapshot()).await;
        }
    });
}
#[test]
fn native_nonzero_and_malformed_stdout_preserve_error_usage_and_receipt() {
    run(async {
        for malformed in [false, true] {
            let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
            let mut f = fixture_support::fixture(
                &[
                    Permission::Read(Module::Ast),
                    Permission::Read(Module::Knowledge),
                    Permission::MutateModule(Module::Knowledge),
                ],
                brand,
                NativeBinding::Codex,
            );
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            publish(&f, &ctx).await;
            let mut frames = vec![
                r#"{"type":"turn.started"}"#.to_owned(),
                r#"{"type":"item.completed","item":{"id":"a","type":"agent_message","text":"must-not-be-an-answer"}}"#.to_owned(),
                terminal()
            ];
            if malformed {
                frames.push("{invalid-native-json".to_owned());
            }
            peer(
                &f,
                json!({"kind":"wire","protocol":"codex","frames":frames,
                "checks":{},"gate":null,"marker":null,"exit":if malformed {0} else {9}}),
            );
            attach(&mut f, &ctx, vec![]).await;
            let (served, record) = serve(&f, &ctx).await;
            let proxy = SourceProxy::new(
                f.paths.clone(),
                record.address(),
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
                    Duration::from_secs(10),
                ),
                Some((f.projects[0].0.root(), None)),
                &ctx,
            )
            .await
            .unwrap();
            let (local, remote) = tokio::io::duplex(65536);
            let (read, write) = tokio::io::split(remote);
            let owner = proxy.clone();
            let joined = tokio::spawn(async move { owner.serve_stream(read, write).await });
            let client = ().serve(local).await.unwrap();
            let result = client
                .peer()
                .call_tool(ask(
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m"}),
                ))
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            let payload = result.structured_content.unwrap();
            assert_eq!(payload["receipt"]["known_events"], 3);
            assert_eq!(payload["receipt"]["usage"]["provenance"], "observed");
            assert_eq!(payload["receipt"]["usage"]["output_tokens"], 3);
            if malformed {
                assert_eq!(
                    payload["receipt"]["completion"]["usage"],
                    payload["receipt"]["usage"]
                );
            } else {
                assert!(payload["receipt"].get("completion").is_none());
                assert_eq!(payload["execution_error"], "NonzeroExit");
            }
            assert!(payload["receipt"]["completion"].get("answer").is_none());
            assert!(payload["receipt"]["process"]["cleanup"].is_string());
            if !malformed {
                assert_eq!(payload["receipt"]["process"]["exit_code"], 9);
                assert_eq!(payload["receipt"]["process"]["status_success"], false);
            }
            client.cancel().await.unwrap();
            joined.await.unwrap().unwrap();
            proxy.shutdown().await.unwrap();
            stop(&f, served, &record, ctx.snapshot()).await;
        }
    });
}
#[test]
fn cancellation_eof_deadline_and_aborted_shutdown_keep_owned_cleanup() {
    run(async {
        for mode in ["cancel", "eof", "deadline", "shutdown"] {
            let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
            let mut f = fixture_support::fixture(
                &[
                    Permission::Read(Module::Ast),
                    Permission::Read(Module::Knowledge),
                    Permission::MutateModule(Module::Knowledge),
                ],
                brand,
                NativeBinding::Codex,
            );
            let cancel = Cancellation::default();
            let ctx = original(&cancel);
            publish(&f, &ctx).await;
            let gate = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = gate.local_addr().unwrap().port();
            peer(
                &f,
                json!({"kind":"wire","protocol":"codex",
                "frames":[r#"{"type":"turn.started"}"#,terminal()],
                "checks":{},"gate":{"before":1,"send":"w","expect":null},"marker":null,"exit":0}),
            );
            attach(
                &mut f,
                &ctx,
                vec![("GATE_PORT".into(), port.to_string().into())],
            )
            .await;
            let (served, record) = serve(&f, &ctx).await;
            let proxy = SourceProxy::new(
                f.paths.clone(),
                record.address(),
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
                    if mode == "deadline" {
                        Duration::from_secs(2)
                    } else {
                        Duration::from_secs(10)
                    },
                ),
                Some((f.projects[0].0.root(), None)),
                &ctx,
            )
            .await
            .unwrap();
            let (local, remote) = tokio::io::duplex(65536);
            let (read, write) = tokio::io::split(remote);
            let owner = proxy.clone();
            let joined = tokio::spawn(async move { owner.serve_stream(read, write).await });
            let mut client = Some(().serve(local).await.unwrap());
            let handle = client
                .as_ref()
                .unwrap()
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
                        panic!("native tool response")
                    };
                    assert_eq!(result.is_error, Some(true));
                    assert!(
                        !serde_json::to_string(&result)
                            .unwrap()
                            .contains("must-not-be-an-answer")
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
                            "shutdown waiter must actually be pending before drop"
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
            use tokio::io::AsyncReadExt;
            let mut bytes = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut bytes))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(bytes, b"w");
        }
    });
}
