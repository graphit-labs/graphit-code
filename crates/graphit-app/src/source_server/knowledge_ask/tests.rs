//! Real Auth/FULL16/Process/HTTP fixtures. No commercial Codex qualification.
use super::*;
use crate::knowledge_ask::{
    AuthenticatedKnowledgeQuestion, CodexQuestionExecution, KnowledgeCodexQuestion,
    ask_knowledge_codex_authenticated,
};
use graphit_auth::{Permission, ProjectWriteOperation};
use graphit_core::{
    artifact_store::{ArtifactLimits, CopyLimits},
    module_settings::Module,
};
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use rmcp::{
    ClientHandler, RoleClient,
    model::{NumberOrString, ProtocolVersion, RequestMetaObject},
    service::{ClientLifecycleMode, ClientServiceExt, NotificationContext},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::{future::Future, io::Write, num::NonZeroUsize, os::unix::fs::PermissionsExt};

fn run(test: impl Future<Output = ()> + Send + 'static) {
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
fn fixture(knowledge: bool) -> (tempfile::TempDir, SourceServer, String) {
    let mut permissions = vec![Permission::Read(Module::Ast)];
    if knowledge {
        permissions.extend([
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ]);
    }
    let (temp, mut server, raw) = super::super::tests::fixture_with_permissions(&permissions);
    let state = Arc::get_mut(&mut server.0).unwrap();
    state.policy.binding = NativeBinding::Codex;
    state.policy.content = ContentLimits {
        max_input_bytes: 1024 * 1024,
        max_output_bytes: 8 * 1024 * 1024,
        max_depth: 64,
        max_nodes: 65536,
        max_scalar_bytes: 1024 * 1024,
        max_collection_entries: 65536,
    };
    (temp, server, raw)
}
async fn publish(server: &SourceServer, raw: &str, empty: bool) -> KnowledgeFirstPublication {
    let cancel = Cancellation::default();
    let original = original(&cancel);
    let snapshot = server.0.live_projects.snapshot().unwrap();
    let (paths, id, version) = &snapshot.projects[0];
    let live =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let record = live.binding().unwrap();
    let scope = ScopeInput {
        installation: server.0.auth.installation(),
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
    };
    let caller = server
        .0
        .auth
        .issuer()
        .authenticate_runtime(raw, &original)
        .unwrap();
    let authority = ProjectRecordAuthority::new(server.0.auth.issuer());
    let admission = authority
        .admit_authenticated_write(&caller, &original, scope, ProjectWriteOperation::Knowledge)
        .unwrap();
    let n = |value| NonZeroUsize::new(value).unwrap();
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
        original.snapshot(),
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
        original.snapshot(),
        &admission,
        &rows,
        KnowledgeIndexWriteLimits {
            content: KnowledgeIndexContentLimits {
                max_identifier_bytes: n(256),
            },
            native: server.0.policy.index,
            artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
        },
    )
    .await
    .unwrap()
}
async fn launch(
    temp: &Path,
    server: &mut SourceServer,
    script: &str,
    environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
) {
    let program = temp.join("codex");
    std::fs::write(&program, format!("#!/usr/bin/python3\n{script}")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut environment = environment;
    environment.push(("PATH".into(), temp.as_os_str().to_owned()));
    server
        .attach_ask(
            &graphit_core::config::ConfigSnapshot::new(server.0.policy.installation.brand(), [])
                .unwrap(),
            environment,
            &original(&Cancellation::default()),
        )
        .await
        .unwrap();
}
struct Progress(tokio::sync::mpsc::UnboundedSender<ProgressNotificationParam>);
impl ClientHandler for Progress {
    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _: NotificationContext<RoleClient>,
    ) {
        let _ = self.0.send(params);
    }
}
async fn client<H: ClientHandler>(
    server: &SourceServer,
    raw: &str,
    handler: H,
) -> (
    rmcp::service::RunningService<RoleClient, H>,
    tokio::task::JoinHandle<io::Result<()>>,
) {
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let uri = format!("http://{}/mcp", listener.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve(listener).await });
    let mut headers = std::collections::HashMap::new();
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(axum::http::header::AUTHORIZATION, bearer);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("runtime"),
    );
    let client = handler
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    (client, served)
}
fn arguments(query: &str) -> serde_json::Map<String, Value> {
    json!({"query":query,"selected_slugs":["Paris"],"model":"literal model;$(native)"})
        .as_object()
        .unwrap()
        .clone()
}
fn params(query: &str) -> CallToolRequestParams {
    CallToolRequestParams::new(TOOL).with_arguments(arguments(query))
}
async fn stop(
    server: &SourceServer,
    served: tokio::task::JoinHandle<io::Result<()>>,
    client: rmcp::service::RunningService<RoleClient, impl ClientHandler>,
) {
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[test]
fn raw_projection_preserves_large_numbers_and_every_known_item_family() {
    let cancel = Cancellation::default();
    let original = original(&cancel);
    let wire = crate::knowledge_ask_service::AskAvailability::capture(
        graphit_ai::cli_provider::CliCompletionProvider::KiroV3,
        std::ffi::OsStr::new("kiro-cli"),
        Vec::new(),
        Path::new("/"),
        NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
    )
    .unwrap();
    assert!(wire.service().is_none());
    let limits = CodexWireLimits {
        frame_bytes: 1024 * 1024,
        depth: 64,
        nodes: 65536,
        fields: 65536,
        array_items: 65536,
        string_bytes: 1024 * 1024,
        decoded_string_bytes: 1024 * 1024,
        id_bytes: 4096,
    };
    let input = r#"{"type":"item.completed","item":{"id":"outer","type":"mcp_tool_call","server":"fixture","tool":"fixture","arguments":{"big":1234567890123456789012345678901234567890,"huge":1e400,"negative":-0},"result":{"content":[1e400,-0,1234567890123456789012345678901234567890],"structured_content":{"v":-0},"_meta":{"huge":1e400}},"status":"completed"}}"#;
    let frame = decode_codex_frame(input.as_bytes(), &limits, original.snapshot()).unwrap();
    let CodexDecodedFrame::Known(event) = frame else {
        panic!("known")
    };
    let text = serde_json::to_string(&NativeEvent(&event.view())).unwrap();
    for literal in ["1234567890123456789012345678901234567890", "1e400", "-0"] {
        assert!(text.contains(literal));
    }
    assert!(text.contains("\"outer_id\":\"outer\""));
    assert!(text.contains("\"structured_content\":{\"v\":-0}"));
    for input in [
        r#"{"type":"thread.started","thread_id":"thread"}"#,
        r#"{"type":"turn.started"}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":4,"cache_write_input_tokens":5}}"#,
        r#"{"type":"turn.failed","error":{"message":"actual"}}"#,
        r#"{"type":"error","message":"actual"}"#,
        r#"{"type":"item.started","item":{"id":"i","type":"agent_message","text":"text"}}"#,
        r#"{"type":"item.updated","item":{"id":"i","type":"reasoning","text":"text"}}"#,
        r#"{"type":"item.completed","item":{"id":"i","type":"command_execution","command":"literal","aggregated_output":"out","exit_code":7,"status":"failed"}}"#,
        r#"{"type":"item.completed","item":{"id":"i","type":"file_change","changes":[{"path":"p","kind":"add"}],"status":"completed"}}"#,
        r#"{"type":"item.completed","item":{"id":"i","type":"collab_tool_call","tool":"spawn_agent","sender_thread_id":"s","receiver_thread_ids":["r"],"prompt":"p","agents_states":{"r":{"status":"completed","message":"m"}},"status":"completed"}}"#,
        r#"{"type":"item.completed","item":{"id":"outer","type":"web_search","id":"inner","query":"q","action":{"type":"search","query":"q","queries":["q"]},"results":[1e400,-0]}}"#,
        r#"{"type":"item.completed","item":{"id":"i","type":"todo_list","items":[{"text":"t","completed":true}]}}"#,
        r#"{"type":"item.completed","item":{"id":"i","type":"error","message":"m"}}"#,
    ] {
        let frame = decode_codex_frame(input.as_bytes(), &limits, original.snapshot()).unwrap();
        let CodexDecodedFrame::Known(event) = frame else {
            panic!("known")
        };
        assert!(
            !serde_json::to_string(&NativeEvent(&event.view()))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn actual_http_progress_reaches_client_before_child_eof_and_preserves_launch_literals() {
    run(async {
        let (temp, mut server, raw) = fixture(true);
        publish(&server, &raw, false).await;
        let gate = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = gate.local_addr().unwrap().port();
        let script = format!(
            r#"import sys,os,json,socket
assert sys.argv[1:] == ['exec','--json','--model','literal model;$(native)','-']
assert os.environ['CAPTURED'] == 'literal;$(env)'
prompt=json.load(sys.stdin)
assert prompt['question'] == '  literal β\n  '
assert prompt['pages'][0]['slug'] == 'Paris'
assert os.path.basename(os.getcwd()) == 'project'
print('{{"type":"turn.started"}}',flush=True)
print('{{"type":"item.completed","item":{{"id":"i","type":"agent_message","text":"actual β [[Paris]]"}}}}',flush=True)
print('{{"type":"item.completed","item":{{"id":"outer","type":"mcp_tool_call","server":"fixture","tool":"fixture","arguments":{{"big":1234567890123456789012345678901234567890,"huge":1e400,"negative":-0}},"result":{{"content":[1e400,-0,1234567890123456789012345678901234567890],"structured_content":{{"v":-0}},"_meta":{{"huge":1e400}}}},"status":"completed"}}}}',flush=True)
print('{{"type":"item.completed","item":{{"id":"outer","type":"web_search","id":"inner","query":"q","action":{{"type":"search","query":"q"}},"results":[1e400,-0,1234567890123456789012345678901234567890]}}}}',flush=True)
s=socket.create_connection(('127.0.0.1',{port}));s.recv(1);s.close()
print('{{"type":"turn.completed","usage":{{"input_tokens":8,"cached_input_tokens":1,"output_tokens":3,"reasoning_output_tokens":2,"cache_write_input_tokens":0}}}}',flush=True)
"#
        );
        launch(
            temp.path(),
            &mut server,
            &script,
            vec![("CAPTURED".into(), "literal;$(env)".into())],
        )
        .await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (client, served) = client(&server, &raw, Progress(tx)).await;
        let mut params = params("  literal β\n  ");
        params.meta = Some(RequestMetaObject::with_progress_token(ProgressToken(
            NumberOrString::Number(17),
        )));
        {
            let request = client.call_tool(params);
            tokio::pin!(request);
            let started = tokio::select! { event=rx.recv()=>event.unwrap(), result=&mut request=>panic!("final before native turn progress: {:?}", result.as_ref().ok().and_then(|v|v.structured_content.as_ref()).map(|v|&v["execution_error"])) };
            assert_eq!(started.progress, 1.0);
            assert!(started.message.unwrap().contains("turn.started"));
            let notification = tokio::select! { event=rx.recv()=>event.unwrap(), result=&mut request=>panic!("final before client progress category: {:?}",result.as_ref().ok().and_then(|value|value.structured_content.as_ref()).map(|value|(&value["execution_error"],&value["category"]))) };
            assert_eq!(notification.progress, 2.0);
            assert!(notification.total.is_none());
            assert!(notification.message.unwrap().contains("actual β [[Paris]]"));
            for ordinal in [3.0, 4.0] {
                let raw = tokio::select! { event=rx.recv()=>event.unwrap(), result=&mut request=>panic!("final before raw progress: {:?}",result.as_ref().ok().and_then(|v|v.structured_content.as_ref()).map(|v|(&v["execution_error"], &v["receipt"]["completion"]))) };
                assert_eq!(raw.progress, ordinal);
                let message = raw.message.unwrap();
                for literal in ["1234567890123456789012345678901234567890", "1e400", "-0"] {
                    assert!(message.contains(literal));
                }
                assert!(message.contains("\"outer_id\":\"outer\""));
                if ordinal == 3.0 {
                    assert!(message.contains("\"structured_content\":{\"v\":-0}"));
                } else {
                    assert!(message.contains("\"inner_id\":\"inner\""));
                }
            }
            let mut stream = tokio::task::spawn_blocking(move || gate.accept().unwrap().0)
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(40), &mut request)
                    .await
                    .is_err()
            );
            stream.write_all(b"x").unwrap();
            let result = request.await.unwrap();
            assert_eq!(result.is_error, Some(false));
            let final_result = result.structured_content.unwrap();
            assert_eq!(final_result["status"], "answer");
            assert_eq!(
                final_result["receipt"]["completion"]["usage"]["provenance"],
                "observed"
            );
            assert_eq!(
                final_result["receipt"]["completion"]["usage"]["input_tokens"],
                8
            );
            assert_eq!(final_result["receipt"]["process"]["cleanup"], "Complete");
        }
        stop(&server, served, client).await;
    });
}

#[test]
fn missing_captured_path_and_other_binding_keep_source_active_and_ask_unavailable() {
    run(async {
        for binding in [NativeBinding::Codex, NativeBinding::KiroV3] {
            let (temp, mut server, raw) = fixture(true);
            Arc::get_mut(&mut server.0).unwrap().policy.binding = binding;
            server
                .attach_ask(
                    &graphit_core::config::ConfigSnapshot::new(
                        server.0.policy.installation.brand(),
                        [],
                    )
                    .unwrap(),
                    Vec::new(),
                    &original(&Cancellation::default()),
                )
                .await
                .unwrap();
            assert!(!available(&server.0));
            assert!(server.0.ask.as_ref().unwrap().service().is_none());
            // Installation after capture does not change optional availability.
            let late = temp.path().join("codex");
            std::fs::write(
                &late,
                "#!/usr/bin/python3\nraise AssertionError('uncaptured executable')\n",
            )
            .unwrap();
            std::fs::set_permissions(&late, std::fs::Permissions::from_mode(0o700)).unwrap();
            let (client, served) = client(&server, &raw, ()).await;
            let tools = client.list_all_tools().await.unwrap();
            assert!(tools.iter().any(|tool| tool.name == super::super::TOOL));
            assert!(!tools.iter().any(|tool| tool.name == TOOL));
            let source = client
                .call_tool(
                    CallToolRequestParams::new(super::super::TOOL)
                        .with_arguments(json!({"path":"raw"}).as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(source.is_error, Some(false));
            let result = client.call_tool(params("question")).await.unwrap();
            assert_eq!(result.is_error, Some(true));
            let denied = delivery_denied(result.clone(), "Cancelled")
                .structured_content
                .unwrap();
            assert_eq!(
                denied["error_content"][0]["text"],
                result.content[0].as_text().unwrap().text
            );
            assert_eq!(denied["delivery_error"], "Cancelled");
            assert!(
                result.content[0]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("restart")
            );
            stop(&server, served, client).await;
        }
    });
}

#[test]
fn invalid_unauthorized_empty_and_oversize_http_calls_spawn_no_child() {
    run(async {
        for knowledge in [false, true] {
            let (temp, mut server, raw) = fixture(knowledge);
            if knowledge {
                publish(&server, &raw, false).await;
            }
            let marker = temp.path().join("child-started");
            launch(
                temp.path(),
                &mut server,
                &format!(
                    "import pathlib\npathlib.Path({:?}).write_text('started')\n",
                    marker.to_str().unwrap()
                ),
                Vec::new(),
            )
            .await;
            let (client, served) = client(&server, &raw, ()).await;
            if !knowledge {
                assert_eq!(
                    client.call_tool(params("question")).await.unwrap().is_error,
                    Some(true)
                );
            } else {
                for input in [
                    json!({"query":"","selected_slugs":["Paris"],"model":"m"}),
                    json!({"query":"  \n ","selected_slugs":["Paris"],"model":"m"}),
                    json!({"query":"q","selected_slugs":["Paris","Paris"],"model":"m"}),
                    json!({"query":"q","selected_slugs":["missing"],"model":"m"}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":""}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","program":"forbidden"}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","project_version":null}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","project_dir":"/unretained"}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"m","project_version":"unretained"}),
                    json!({"query":"x".repeat(64*1024+1),"selected_slugs":["Paris"],"model":"m"}),
                    json!({"query":"q","selected_slugs":["Paris"],"model":"x".repeat(4097)}),
                ] {
                    let result = client
                        .call_tool(
                            CallToolRequestParams::new(TOOL)
                                .with_arguments(input.as_object().unwrap().clone()),
                        )
                        .await
                        .unwrap();
                    assert_eq!(result.is_error, Some(true));
                    assert!(!marker.exists());
                }
                let result = client
                    .call_tool(
                        CallToolRequestParams::new(TOOL).with_arguments(
                            json!({"query":"q","selected_slugs":[],"model":"m"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    result.structured_content.unwrap()["status"],
                    "empty_selection"
                );
            }
            assert!(!marker.exists());
            stop(&server, served, client).await;
        }
        for empty in [false, true] {
            let (temp, mut server, raw) = fixture(true);
            if empty {
                publish(&server, &raw, true).await;
            }
            let marker = temp.path().join("child-started");
            launch(
                temp.path(),
                &mut server,
                &format!(
                    "import pathlib\npathlib.Path({:?}).write_text('started')\n",
                    marker.to_str().unwrap()
                ),
                Vec::new(),
            )
            .await;
            let (client, served) = client(&server, &raw, ()).await;
            let result = client.call_tool(params("question")).await.unwrap();
            assert_eq!(result.is_error, Some(false));
            assert_eq!(
                result.structured_content.unwrap()["status"],
                if empty {
                    "published_empty"
                } else {
                    "no_corpus"
                }
            );
            assert!(!marker.exists());
            stop(&server, served, client).await;
        }
    });
}

#[test]
fn actual_native_failures_keep_first_cause_and_per_attempt_usage() {
    run(async {
        let (temp, mut server, raw) = fixture(true);
        publish(&server, &raw, false).await;
        launch(temp.path(),&mut server,r#"import json,sys
q=json.load(sys.stdin)['question']
print('{"type":"turn.started"}',flush=True)
if q == 'malformed': print('{malformed',flush=True)
elif q == 'provider': print('{"type":"turn.failed","error":{"message":"actual provider failure"}}',flush=True)
elif q == 'negative': print('{"type":"turn.completed","usage":{"input_tokens":-1,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}',flush=True)
elif q == 'frame': print('x' * (1024*1024+1),flush=True)
elif q == 'nonzero': sys.exit(7)
else:
 print('{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"  "}}',flush=True)
 print('{"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}',flush=True)
"#,Vec::new()).await;
        let (client, served) = client(&server, &raw, ()).await;
        for (query, expected) in [
            ("malformed", "Wire"),
            ("provider", "Provider"),
            ("negative", "InvalidUsage"),
            ("frame", "FrameLimit"),
            ("nonzero", "NonzeroExit"),
        ] {
            let result = client.call_tool(params(query)).await.unwrap();
            assert_eq!(result.is_error, Some(true));
            let denied = delivery_denied(result.clone(), "Cancelled");
            let payload = result.structured_content.unwrap();
            let denied = denied.structured_content.unwrap();
            assert_eq!(denied["category"], payload["category"]);
            assert_eq!(denied["execution_error"], payload["execution_error"]);
            assert_eq!(denied["receipt"], payload["receipt"]);
            assert_eq!(denied["delivery_error"], "Cancelled");
            assert!(
                payload.to_string().contains(expected),
                "expected native category {expected}; observed execution={:?} completion={:?} category={:?}",
                payload["execution_error"],
                payload["receipt"]["completion"],
                payload["category"]
            );
            assert_eq!(payload["receipt"]["process"]["cleanup"], "Complete");
        }
        let result = client.call_tool(params("empty-answer")).await.unwrap();
        assert_eq!(result.is_error, Some(false));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["status"], "empty_answer");
        assert_eq!(payload["receipt"]["completion"]["answer"], "  ");
        stop(&server, served, client).await;
    });
}

#[test]
fn denied_delivery_omits_native_answer_and_preserves_actual_observations() {
    struct Ignore;
    impl CodexSink for Ignore {
        async fn push<'a>(
            &'a mut self,
            _: CodexEventView<'a>,
            _: &'a SnapshotContext,
        ) -> Result<(), SinkFailure> {
            Ok(())
        }
    }
    run(async {
        let (temp, mut server, raw) = fixture(true);
        publish(&server, &raw, false).await;
        launch(temp.path(), &mut server, r#"print('{"type":"turn.started"}',flush=True)
print('{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"observed answer"}}',flush=True)
print('{"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}',flush=True)
"#, Vec::new()).await;
        let cancel = Cancellation::default();
        let original = original(&cancel);
        let state = &server.0;
        let snapshot = state.live_projects.snapshot().unwrap();
        let (paths, id, version) = &snapshot.projects[0];
        let live =
            CapturedPublicationScope::project(paths, *id, state.policy.scope, original.snapshot())
                .unwrap();
        let record = live.binding().unwrap();
        let scope = ScopeInput {
            installation: state.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &live,
                    record: &record,
                },
                version,
            }),
            providers: &[],
            binding: state.policy.binding,
            global_version: "",
        };
        let caller = state
            .auth
            .issuer()
            .authenticate_runtime(&raw, &original)
            .unwrap();
        let authority = ProjectRecordAuthority::new(state.auth.issuer());
        let service = state.ask.as_ref().unwrap().service().unwrap();
        let host = service.host().unwrap();
        let result = ask_knowledge_codex_authenticated(
            AuthenticatedKnowledgeQuestion {
                authority: &authority,
                caller: &caller,
                scope,
                original: &original,
            },
            KnowledgeCodexQuestion {
                query: "q",
                selected_slugs: &["Paris"],
                model: KnowledgeQuestionModel {
                    requested_model: "m",
                    wire_limits: &service.policy.wire,
                    completion_limits: service.policy.completion,
                },
                limits: service.policy.question,
            },
            CodexQuestionExecution {
                host: &host,
                request: service.request(paths.root(), "m"),
                stream_limits: CodexStreamLimits {
                    stdout_bytes: service.policy.process.stdout_bytes,
                    stderr_bytes: service.policy.process.stderr_bytes,
                },
                sink: &mut Ignore,
            },
        )
        .await
        .unwrap();
        let KnowledgeQuestionResult::Executed { receipt, .. } = &result else {
            panic!("native receipt")
        };
        let internal = receipt_value(receipt.as_ref(), true);
        assert_eq!(internal["completion"]["answer"], "observed answer");
        let denied = receipt_value(receipt.as_ref(), false);
        assert!(denied["completion"].get("answer").is_none());
        assert_eq!(
            denied["completion"]["usage"],
            internal["completion"]["usage"]
        );
        assert_eq!(denied["process"], internal["process"]);
        cancel.cancel();
        assert!(original.snapshot().check().is_err());
        let denied = delivery_denied(terminal(result), "Cancelled")
            .structured_content
            .unwrap();
        assert!(denied["receipt"]["completion"].get("answer").is_none());
        assert_eq!(denied["receipt"]["completion"]["usage"]["input_tokens"], 8);
        assert_eq!(denied["receipt"]["process"]["cleanup"], "Complete");
        drop(host);
        server.shutdown().await.unwrap();
    });
}

#[test]
fn actual_http_cancel_and_deadline_join_native_child_and_cancelled_shutdown_waiter() {
    run(async {
        use rmcp::model::{CallToolRequest, ClientRequest};
        for deadline in [false, true] {
            let (temp, mut server, raw) = fixture(true);
            publish(&server, &raw, false).await;
            if deadline {
                Arc::get_mut(&mut server.0).unwrap().policy.timeout = Duration::from_secs(3);
            }
            let gate = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = gate.local_addr().unwrap().port();
            launch(temp.path(),&mut server,&format!("import json,sys,socket\njson.load(sys.stdin)\ns=socket.create_connection(('127.0.0.1',{port}))\ns.recv(1)\n"),Vec::new()).await;
            let (client, served) = client(&server, &raw, ()).await;
            let request = client
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(params("question"))),
                    Default::default(),
                )
                .await
                .unwrap();
            let mut stream = tokio::task::spawn_blocking(move || {
                let stream = gate.accept().unwrap().0;
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                stream
            })
            .await
            .unwrap();
            if deadline {
                let result = request.await_response().await.unwrap();
                assert!(format!("{result:?}").contains("deadline"));
            } else {
                request.cancel(None).await.unwrap();
            }
            let waiting = server.clone();
            let waiter = tokio::spawn(async move { waiting.shutdown().await });
            waiter.abort();
            let _ = waiter.await;
            server.shutdown().await.unwrap();
            let eof = tokio::task::spawn_blocking(move || {
                let mut byte = [0];
                std::io::Read::read(&mut stream, &mut byte)
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(eof, 0);
            stop(&server, served, client).await;
        }
    });
}

#[test]
fn native_receipt_exposes_usage_independently_of_completion() {
    run(async {
        let (temp, mut server, raw) = fixture(true);
        publish(&server, &raw, false).await;
        launch(temp.path(),&mut server,r#"import json,sys
query=json.load(sys.stdin)['question']
print('{"type":"turn.started"}',flush=True)
print('{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"actual answer"}}',flush=True)
print('{"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1,"cache_write_input_tokens":9}}',flush=True)
if query == 'nonzero': sys.exit(7)
"#,Vec::new()).await;
        let (client, served) = client(&server, &raw, ()).await;
        for query in ["nonzero", "answer"] {
            let result = client.call_tool(params(query)).await.unwrap();
            let payload = result.structured_content.unwrap();
            assert_eq!(payload["receipt"]["usage"]["provenance"], "observed");
            for (name, value) in [
                ("input_tokens", 8),
                ("cached_input_tokens", 2),
                ("output_tokens", 3),
                ("reasoning_output_tokens", 1),
                ("cache_write_input_tokens", 9),
            ] {
                assert_eq!(payload["receipt"]["usage"][name], value);
            }
            assert_eq!(payload["receipt"]["known_events"], 3);
            assert_eq!(payload["receipt"]["process"]["cleanup"], "Complete");
            if query == "nonzero" {
                assert_eq!(result.is_error, Some(true));
                assert_eq!(payload["execution_error"], "NonzeroExit");
                assert!(payload["receipt"].get("completion").is_none());
                assert_eq!(payload["receipt"]["process"]["exit_code"], 7);
            } else {
                assert_ne!(result.is_error, Some(true));
                assert_eq!(payload["receipt"]["completion"]["answer"], "actual answer");
                assert_eq!(
                    payload["receipt"]["completion"]["usage"],
                    payload["receipt"]["usage"]
                );
            }
        }
        stop(&server, served, client).await;
    });
}

async fn launch_kiro_fixture(
    temp: &Path,
    server: &mut SourceServer,
    binding: NativeBinding,
    mode: &str,
) {
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("kiro-knowledge-fixture");
    assert!(
        binary.is_file(),
        "build the real SDK fixture bin before this suite"
    );
    std::os::unix::fs::symlink(binary, temp.join("kiro-cli")).unwrap();
    Arc::get_mut(&mut server.0).unwrap().policy.binding = binding;
    server
        .attach_ask(
            &graphit_core::config::ConfigSnapshot::new(server.0.policy.installation.brand(), [])
                .unwrap(),
            vec![
                ("PATH".into(), temp.as_os_str().to_owned()),
                ("KIRO_CAPTURED".into(), "literal-captured".into()),
                (
                    "KIRO_FIXTURE_MODEL".into(),
                    "literal model;$(native)".into(),
                ),
                ("KIRO_FIXTURE_MODE".into(), mode.into()),
                (
                    "KIRO_FIXTURE_JOURNAL".into(),
                    temp.join("kiro-journal").into_os_string(),
                ),
            ],
            &original(&Cancellation::default()),
        )
        .await
        .unwrap();
}

#[test]
fn kiro_stateless_v3_stream_is_private_and_precedes_real_terminal() {
    run(async {
        for binding in [NativeBinding::KiroV3] {
            let (temp, mut server, raw) = fixture(true);
            publish(&server, &raw, false).await;
            launch_kiro_fixture(temp.path(), &mut server, binding, "stream_gate").await;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let (client, served) = client(&server, &raw, Progress(tx)).await;
            let mut request = params("  literal β\n  ");
            request.meta = Some(RequestMetaObject::with_progress_token(ProgressToken(
                NumberOrString::Number(29),
            )));
            {
                let request = client.call_tool(request);
                tokio::pin!(request);
                let mut ordinal = 0.0;
                loop {
                    let event = tokio::select! {
                        event = rx.recv() => event.unwrap(),
                        _ = &mut request => panic!("terminal before gated native progress"),
                        _ = tokio::time::sleep(Duration::from_secs(10)) => panic!("no actual progress"),
                    };
                    assert!(event.progress > ordinal);
                    ordinal = event.progress;
                    let message = event.message.unwrap();
                    assert!(!message.contains("private-kiro-fixture-session"));
                    assert!(!message.contains("session_id"));
                    if message.contains("literal-tool-id") {
                        assert!(message.contains("native"));
                        break;
                    }
                }
                assert!(
                    tokio::time::timeout(Duration::from_millis(40), &mut request)
                        .await
                        .is_err()
                );
                assert!(
                    !std::fs::read_to_string(temp.path().join("kiro-journal"))
                        .unwrap()
                        .lines()
                        .any(|e| e == "terminal")
                );
                std::fs::write(
                    temp.path().join("kiro-journal").with_extension("finish"),
                    b"finish",
                )
                .unwrap();
                let result = request.await.unwrap();
                assert_eq!(result.is_error, Some(false));
                let payload = result.structured_content.unwrap();
                assert_eq!(
                    payload["receipt"]["completion"]["answer"],
                    "fresh fixture answer"
                );
                assert_eq!(payload["receipt"]["process"]["joined"], true);
                assert!(!payload.to_string().contains("private-kiro-fixture-session"));
            }
            stop(&server, served, client).await;
        }
    });
}

#[test]
fn kiro_stateless_denied_empty_invalid_and_actual_offer_mismatch_never_prompt() {
    run(async {
        for scenario in ["denied", "empty", "invalid", "offer_mismatch", "id_limit"] {
            let (temp, mut server, raw) = fixture(scenario != "denied");
            if scenario != "denied" {
                publish(&server, &raw, false).await;
            }
            launch_kiro_fixture(temp.path(), &mut server, NativeBinding::KiroV3, scenario).await;
            let (client, served) = client(&server, &raw, ()).await;
            let mut input = arguments("question");
            if scenario == "empty" {
                input.insert("selected_slugs".into(), json!([]));
            }
            if scenario == "invalid" {
                input.insert("model".into(), json!("x".repeat(4097)));
            }
            let result = client
                .call_tool(CallToolRequestParams::new(TOOL).with_arguments(input))
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(scenario != "empty"));
            if scenario == "empty" {
                assert_eq!(
                    result.structured_content.as_ref().unwrap()["status"],
                    "empty_selection"
                );
            }
            let events =
                std::fs::read_to_string(temp.path().join("kiro-journal")).unwrap_or_default();
            assert!(!events.lines().any(|e| e == "prompt"));
            if ["denied", "empty", "invalid"].contains(&scenario) {
                assert!(events.is_empty());
            }
            assert!(
                !serde_json::to_string(&result)
                    .unwrap()
                    .contains("private-kiro-fixture-session")
            );
            stop(&server, served, client).await;
        }
    });
}

#[test]
fn knowledge_lexical_search_mcp_uses_published_reader_and_read_grant_without_model() {
    run(async {
        for permitted in [true, false] {
            let (_temp, server, raw) = fixture(permitted);
            if permitted {
                publish(&server, &raw, false).await;
            }
            let (client, served) = client(&server, &raw, ()).await;
            let result = client.call_tool(
                CallToolRequestParams::new(super::super::knowledge_search::TOOL)
                    .with_arguments(json!({"search":{"query":"Paris","mode":"fts","preview":true,"page_size":1}})
                        .as_object().unwrap().clone()),
            ).await.unwrap();
            assert_eq!(result.is_error, Some(!permitted));
            let payload = result.structured_content.unwrap();
            if permitted {
                assert_eq!(payload["source"], json!({"kind":"project_knowledge"}));
                assert_eq!(payload["native"]["state"], "hits");
                assert_eq!(payload["native"]["total"], 1);
                assert!(payload["native"]["generation"]["version"].as_u64().unwrap() > 0);
                assert!(payload["native"]["next_cursor"].is_null());
                let hits = payload["native"]["hits"].as_array().unwrap();
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0]["focused_source"]["slug"], "Paris");
                assert_eq!(hits[0]["focused_source"]["title"], "Capital β");
                assert!(hits[0]["focused_source"]["namespace"].is_null());
                assert!(hits[0]["focused_source"]["witness"].is_null());
                assert!(hits[0]["lexical_score"].as_f64().unwrap().is_finite());
                assert_eq!(hits[0]["preview"]["source"], "summary");
                assert_eq!(hits[0]["preview"]["text"], "literal");
                assert!(hits[0].get("document").is_none());
                assert!(hits[0].get("body").is_none());
            } else {
                assert_eq!(payload["code"], "authentication_or_permission_denied");
                assert!(payload.get("native").is_none());
            }
            stop(&server, served, client).await;
        }
    });
}

#[test]
fn knowledge_imported_lexical_search_mcp_reads_registered_copy_with_native_admission() {
    run(async {
        use graphit_hub::{
            ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
            registry::{Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
        };
        let (_temp, mut server, raw) = fixture(true);
        // This shared fixture overrides Source content bounds for other tests.
        // Compose this configured owner from the same captured Hub policy used
        // by RoleToolPolicy and TaskPolicy, as foreground startup does.
        {
            let state = Arc::get_mut(&mut server.0).unwrap();
            state.policy.content = state.policy.native_install.hub.content();
        }
        // Foreground startup attaches the actual configuration owner before MCP
        // serves Imported requests, including the optional rerank decision.
        let config =
            graphit_core::config::ConfigSnapshot::new(server.0.policy.installation.brand(), [])
                .unwrap();
        server.attach_role_tools(config).unwrap();
        let published = publish(&server, &raw, false).await;
        let cancel = Cancellation::default();
        let request = original(&cancel);
        let snapshot = server.0.live_projects.snapshot().unwrap();
        let projection = server.0.policy.registry.limits().projection();
        let id = ArtifactId::new("vendor/search", projection).unwrap();
        let version = ConcreteVersion::new("exact-version", projection).unwrap();
        let source = SourceRef::hub_cache(
            &server.0.policy.installation,
            snapshot.projects[0].1,
            ArtifactType::Knowledge,
            id.clone(),
            None,
            version.clone(),
            projection,
        )
        .unwrap();
        let corpus = url::Url::parse(&published.default.corpus_uri)
            .unwrap()
            .to_file_path()
            .unwrap();
        let target = source.directory().as_path().join("index.lance");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        crate::task_server::browser::records_fixture::copy_tree(&corpus, &target);
        {
            let mut writer = server
                .0
                .policy
                .registry
                .writer(
                    request.remaining().unwrap().unwrap(),
                    request.cancellation(),
                )
                .unwrap();
            writer
                .put_metadata(InstalledArtifact {
                    consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                    artifact_type: ArtifactType::Knowledge,
                    id: id.clone(),
                    installations: vec![Installation {
                        target: InstallationTarget::GlobalNative,
                        original_source: source,
                        alias: None,
                        entry_name: None,
                        display: None,
                        backend: InstalledBackend::Native,
                        payload: InstalledPayload::Original {
                            selection_binding: None,
                        },
                    }],
                })
                .unwrap();
        }
        let selector = json!({"kind":"imported_knowledge", "artifact":{"id":id.as_str(), "version":version.as_str()}});
        let (client, served) = client(&server, &raw, ()).await;
        let params = |source: Value| {
            CallToolRequestParams::new(super::super::knowledge_search::TOOL).with_arguments(
                json!({"source":source,"search":{"query":"Paris","mode":"fts","preview":true}})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
        };
        let result = client.call_tool(params(selector.clone())).await.unwrap();
        assert_eq!(result.is_error, Some(false));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["source"], selector);
        assert!(payload.get("selection").is_none());
        assert_eq!(payload["native"]["state"], "hits");
        assert_eq!(payload["native"]["total"], 1);
        assert_eq!(
            payload["native"]["hits"][0]["focused_source"]["slug"],
            "Paris"
        );
        assert!(payload["native"]["hits"][0]["focused_source"]["namespace"].is_null());
        assert!(payload["native"]["hits"][0]["focused_source"]["witness"].is_null());
        assert_eq!(payload["native"]["hits"][0]["preview"]["text"], "literal");
        let mut missing = selector;
        missing["artifact"]["version"] = json!("not-registered");
        let absent = client.call_tool(params(missing)).await.unwrap();
        assert_eq!(absent.is_error, Some(false));
        assert_eq!(
            absent.structured_content.unwrap()["native"]["state"],
            "registry_absent"
        );
        stop(&server, served, client).await;
    });
}

#[test]
fn knowledge_lexical_search_retains_previous_worker_panic_in_service_shutdown() {
    run(async {
        let (_temp, server, raw) = fixture(true);
        publish(&server, &raw, false).await;
        let (client, served) = client(&server, &raw, ()).await;
        let worker = server.0.jobs.lock().await.tasks.spawn(async {
            panic!("fixture native worker failure");
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !worker.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let response = client
            .call_tool(
                CallToolRequestParams::new(super::super::knowledge_search::TOOL).with_arguments(
                    json!({"search":{"query":"Paris","mode":"fts"}})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(response.is_error, Some(false));
        assert_eq!(
            response.structured_content.unwrap()["native"]["state"],
            "hits"
        );
        client.cancel().await.unwrap();
        server.0.stop.cancel();
        let error = served.await.unwrap().unwrap_err();
        assert!(
            error
                .get_ref()
                .unwrap()
                .downcast_ref::<tokio::task::JoinError>()
                .unwrap()
                .is_panic()
        );
    });
}

#[test]
fn knowledge_vector_search_native_mcp_uses_existing_cpu_query_and_selected_module_gate() {
    run(async {
        let (_storage,mut foreground,tasks,server,raw,selected) = crate::foreground_source::foreground_memory_jobs_tests::browser_knowledge_search_fixture().await;
        let (client, served) = client(&server, &raw, ()).await;
        for mode in ["semantic", "hybrid"] {
            let result = client.call_tool(CallToolRequestParams::new(super::super::knowledge_search::TOOL).with_arguments(json!({"search":{"query":"Knowledge CPU search","mode":mode,"preview":true}}).as_object().unwrap().clone())).await.unwrap();
            assert_eq!(result.is_error, Some(false));
            let p = result.structured_content.unwrap();
            assert_eq!(p["native"]["state"], "hits");
            assert_eq!(
                p["native"]["hits"][0]["focused_source"]["slug"],
                "knowledge-cpu"
            );
            assert!(
                p["native"]["hits"][0]["semantic_distance"]
                    .as_f64()
                    .is_some_and(f64::is_finite)
            );
        }
        crate::foreground_source::foreground_memory_jobs_tests::stop_browser_model(&mut foreground)
            .await;
        graphit_core::config::ConfigStore::project(&selected.0)
            .unwrap()
            .set(
                graphit_core::config::ConfigKey::new("modules.embedding").unwrap(),
                json!(false),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
        let result = client.call_tool(CallToolRequestParams::new(super::super::knowledge_search::TOOL).with_arguments(json!({"search":{"query":"Knowledge CPU search","mode":"hybrid","preview":true}}).as_object().unwrap().clone())).await.unwrap();
        assert_eq!(result.is_error, Some(false));
        let p = result.structured_content.unwrap();
        assert_eq!(p["native"]["state"], "partial");
        assert_eq!(p["native"]["reason"], "model_unavailable");
        assert_eq!(
            p["native"]["hits"][0]["focused_source"]["slug"],
            "knowledge-cpu"
        );
        assert!(p["native"]["hits"][0].get("semantic_distance").is_none());
        assert!(p["native"]["hits"][0].get("rrf_score").is_none());
        stop(&server, served, client).await;
        tasks.shutdown().await.unwrap();
        foreground.shutdown().await.unwrap();
    });
}

#[test]
fn imported_knowledge_cpu_native_mcp_preserves_corpus_cursor_and_cancels_query() {
    run(async {
        let (_storage, mut foreground, tasks, server, raw, pair) =
            crate::foreground_source::foreground_memory_jobs_tests::imported_knowledge_cpu_search_fixture(None).await;
        let source = json!({"kind":"imported_knowledge","artifact":{"id":"vendor/mount-a","version":"exact-release"}});
        let params = |mode: &str, cursor: Option<Value>| {
            let mut search = json!({"query":"Knowledge CPU search","mode":mode,"top_k":2,"page_size":1,"preview":true});
            if let Some(cursor) = cursor {
                search["cursor"] = cursor;
            }
            CallToolRequestParams::new(super::super::knowledge_search::TOOL).with_arguments(
                json!({"source":source,"search":search})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
        };
        let (client, served) = client(&server, &raw, ()).await;
        let mut ready_cursor = None;
        let mut source_generation = None;
        let mut semantic_generation = None;
        for mode in ["semantic", "hybrid"] {
            let mut cursor = None;
            let mut seen = std::collections::BTreeSet::new();
            for page in 0..2 {
                let result = client.call_tool(params(mode, cursor)).await.unwrap();
                assert_eq!(result.is_error, Some(false), "{result:?}");
                let payload = result.structured_content.unwrap();
                assert_eq!(payload["source"], source);
                assert!(payload.get("selection").is_none());
                let native = &payload["native"];
                assert_eq!(native["state"], "hits");
                assert_eq!(native["total"], 2);
                assert_eq!(native["generation"]["version"].as_u64(), Some(pair));
                assert_eq!(
                    native["semantic_generation"]["version"].as_u64(),
                    Some(pair)
                );
                if let Some(expected) = &source_generation {
                    assert_eq!(&native["generation"], expected);
                } else {
                    source_generation = Some(native["generation"].clone());
                }
                if let Some(expected) = &semantic_generation {
                    assert_eq!(&native["semantic_generation"], expected);
                } else {
                    semantic_generation = Some(native["semantic_generation"].clone());
                }
                let corpus = url::Url::parse(native["generation"]["corpus_uri"].as_str().unwrap())
                    .unwrap()
                    .to_file_path()
                    .unwrap();
                assert_eq!(corpus.file_name().unwrap(), "index.lance");
                assert_eq!(
                    corpus.parent().unwrap().file_name().unwrap(),
                    "i1-mounted-a"
                );
                let hits = native["hits"].as_array().unwrap();
                assert_eq!(hits.len(), 1);
                let hit = &hits[0];
                let slug = hit["focused_source"]["slug"].as_str().unwrap();
                assert!(matches!(slug, "B" | "B-second"));
                assert!(seen.insert(slug.to_owned()));
                let title = if slug == "B" {
                    "Knowledge CPU search B first"
                } else {
                    "Knowledge CPU search B second"
                };
                assert_eq!(hit["focused_source"]["title"], title);
                assert!(hit["focused_source"]["namespace"].is_null());
                assert!(hit["focused_source"]["witness"].is_null());
                assert_eq!(hit["preview"]["text"], title);
                assert_eq!(hit["preview"]["source"], "summary");
                assert!(
                    hit["semantic_distance"]
                        .as_f64()
                        .is_some_and(f64::is_finite)
                );
                assert!(hit.get("body").is_none());
                assert!(hit.get("document").is_none());
                if mode == "hybrid" {
                    assert!(hit["lexical_score"].as_f64().is_some_and(f64::is_finite));
                    assert!(hit["rrf_score"].as_f64().is_some_and(f64::is_finite));
                } else {
                    assert!(hit.get("lexical_score").is_none());
                    assert!(hit.get("rrf_score").is_none());
                }
                cursor = Some(native["next_cursor"].clone());
                if page == 0 {
                    assert!(cursor.as_ref().unwrap().is_object());
                    if mode == "semantic" {
                        ready_cursor = cursor.clone();
                    }
                } else {
                    assert!(cursor.as_ref().unwrap().is_null());
                }
            }
            assert_eq!(
                seen,
                std::collections::BTreeSet::from(["B".to_owned(), "B-second".to_owned()])
            );
        }
        let changed = client.call_tool(CallToolRequestParams::new(super::super::knowledge_search::TOOL)
            .with_arguments(json!({"source":source,"search":{"query":"changed query","mode":"semantic","top_k":2,"page_size":1,"cursor":ready_cursor}}).as_object().unwrap().clone())).await.unwrap();
        assert_eq!(changed.is_error, Some(true));
        assert!(changed.structured_content.unwrap().get("native").is_none());
        let model = server.0.semantic_model().unwrap().unwrap();
        crate::foreground_source::foreground_memory_jobs_tests::stop_browser_model(&mut foreground)
            .await;
        *server.0.semantic.write().unwrap() = None;
        for (mode, state) in [("semantic", "unavailable"), ("hybrid", "partial")] {
            let result = client.call_tool(params(mode, None)).await.unwrap();
            assert_eq!(result.is_error, Some(false), "{result:?}");
            let payload = result.structured_content.unwrap();
            assert_eq!(payload["source"], source);
            let native = &payload["native"];
            assert_eq!(native["state"], state);
            assert_eq!(native["reason"], "model_unavailable");
            assert_eq!(&native["generation"], source_generation.as_ref().unwrap());
            assert_eq!(
                &native["semantic_generation"],
                semantic_generation.as_ref().unwrap()
            );
            if mode == "semantic" {
                assert!(native["hits"].as_array().unwrap().is_empty());
            } else {
                assert_eq!(native["hits"].as_array().unwrap().len(), 1);
                let hit = &native["hits"][0];
                assert!(matches!(
                    hit["focused_source"]["slug"].as_str(),
                    Some("B" | "B-second")
                ));
                assert!(hit["lexical_score"].as_f64().is_some_and(f64::is_finite));
                assert!(hit.get("semantic_distance").is_none());
                assert!(hit.get("rrf_score").is_none());
            }
        }
        let missing_source = json!({"kind":"imported_knowledge","artifact":{"id":"vendor/mount-a","version":"not-registered"}});
        let missing = client.call_tool(CallToolRequestParams::new(super::super::knowledge_search::TOOL)
            .with_arguments(json!({"source":missing_source,"search":{"query":"Knowledge CPU search","mode":"semantic"}}).as_object().unwrap().clone())).await.unwrap();
        assert_eq!(missing.is_error, Some(false));
        let missing = missing.structured_content.unwrap();
        assert_eq!(missing["source"], missing_source);
        assert_eq!(missing["native"]["state"], "registry_absent");
        assert!(missing.get("selection").is_none());
        *server.0.semantic.write().unwrap() = Some(Arc::clone(&model));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut jobs = server.0.jobs.lock().await;
                jobs.reap();
                if jobs.tasks.is_empty() {
                    break;
                }
                drop(jobs);
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // Queue ingress on the real retained-job registry, then cancel over MCP.
        let held = server.0.jobs.lock().await;
        let pending = client
            .send_cancellable_request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    params("semantic", None),
                )),
                Default::default(),
            )
            .await
            .unwrap();
        pending.cancel(None).await.unwrap();
        drop(held);
        tokio::time::timeout(Duration::from_secs(5), server.shutdown())
            .await
            .unwrap()
            .unwrap();
        client.cancel().await.unwrap();
        served.await.unwrap().unwrap();
        tasks.shutdown().await.unwrap();
        foreground.shutdown().await.unwrap();
    });
}

#[test]
fn automatic_knowledge_question_mcp_retrieves_same_pin_without_model_or_provider_on_no_hits() {
    run(async {
        let (temp, mut server, raw) = fixture(true);
        let published = publish(&server, &raw, false).await;
        let prompt_path = temp.path().join("automatic-prompt.json");
        let script = format!(
            r#"import sys,json,pathlib
p=json.load(sys.stdin)
pathlib.Path({:?}).write_text(json.dumps(p))
assert p['pages'][0]['slug']=='Paris'
print('{{"type":"turn.started"}}',flush=True)
print('{{"type":"item.completed","item":{{"id":"answer","type":"agent_message","text":"native retrieved answer"}}}}',flush=True)
print('{{"type":"turn.completed","usage":{{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}}}',flush=True)
"#,
            prompt_path.to_str().unwrap()
        );
        launch(temp.path(), &mut server, &script, vec![]).await;
        let (client, served) = client(&server, &raw, ()).await;
        for (mode, expected) in [("fts", "hits"), ("hybrid", "partial")] {
            let mut call = params("Paris");
            call.arguments = Some(
                json!({"query":"Paris","model":"m","retrieval":{"mode":mode,"top_k":1}})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
            let result = client.call_tool(call).await.unwrap();
            assert_eq!(result.is_error, Some(false));
            let value = result.structured_content.unwrap();
            assert_eq!(value["status"], "answer");
            assert_eq!(value["provided_slugs"], json!(["Paris"]));
            assert_eq!(value["generation"]["version"], published.default.version);
            assert_eq!(
                value["retrieval"]["generation"]["version"],
                published.default.version
            );
            assert_eq!(value["retrieval"]["state"], expected);
            let prompt: Value =
                serde_json::from_slice(&std::fs::read(&prompt_path).unwrap()).unwrap();
            assert_eq!(prompt["pages"].as_array().unwrap().len(), 1);
            std::fs::remove_file(&prompt_path).unwrap();
        }
        let mut call = params("zzzzabsentterm");
        call.arguments = Some(
            json!({"query":"zzzzabsentterm","model":"m","retrieval":{"mode":"fts","top_k":1}})
                .as_object()
                .unwrap()
                .clone(),
        );
        let result = client.call_tool(call).await.unwrap();
        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.structured_content.unwrap()["status"], "no_hits");
        assert!(!prompt_path.exists());
        stop(&server, served, client).await;
    });
}
