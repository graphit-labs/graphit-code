//! Optional real browser observer against the same-origin productive routers.
use super::*;
use crate::{
    knowledge_live::{HistoryLimits, LiveSessionService},
    task_server::{browser::records_fixture as native, tests::fixture_with_reference_permissions},
};
use graphit_auth::Permission;
use graphit_core::module_settings::Module;
use graphit_data::knowledge_references::{
    KnowledgeDocumentState, KnowledgeReferenceFields, KnowledgeReferenceNamespace,
    KnowledgeReferenceScope,
};

fn diagnostic(path: &std::path::Path, value: &Value) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .expect("private browser diagnostic file");
    serde_json::to_writer(&mut file, value).expect("browser diagnostic JSON");
    file.write_all(b"\n").expect("browser diagnostic write");
}

fn ordinal(value: Option<&Value>) -> Option<Value> {
    value
        .filter(|v| {
            v.as_u64().is_some()
                || v.as_str()
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        })
        .cloned()
}

struct StreamDiagnostic {
    file: std::path::PathBuf,
    endpoint: String,
    last_frame: &'static str,
    eof: bool,
}
impl Drop for StreamDiagnostic {
    fn drop(&mut self) {
        if !self.eof {
            diagnostic(
                &self.file,
                &json!({"stage":"stream_end","path":self.endpoint,
                "reason":"consumer_dropped_before_eof","last_frame":self.last_frame}),
            );
        }
    }
}

/// Probe-only response observation. Never records request bodies, frame contents or headers.
pub(super) async fn diagnostics(
    axum::extract::State(probe): axum::extract::State<std::path::PathBuf>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path().to_owned();
    if !path.starts_with("/api/live/sessions") {
        return next.run(request).await;
    }
    let file = probe.with_extension("trace.jsonl");
    diagnostic(
        &file,
        &json!({"stage":"request","method":request.method().as_str(),"path":path}),
    );
    let response = next.run(request).await;
    diagnostic(
        &file,
        &json!({"stage":"response","path":path,"status":response.status().as_u16()}),
    );
    if !response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"text/event-stream"))
    {
        return response;
    }
    let (parts, body) = response.into_parts();
    let mut input = Box::pin(body.into_data_stream());
    let mut trace = StreamDiagnostic {
        file,
        endpoint: path,
        last_frame: "none",
        eof: false,
    };
    let output = futures_util::stream::poll_fn(move |cx| {
        match futures_util::Stream::poll_next(input.as_mut(), cx) {
            std::task::Poll::Ready(Some(Ok(bytes))) => {
                let frame = String::from_utf8_lossy(&bytes);
                let event = frame
                    .lines()
                    .find_map(|line| line.strip_prefix("event:"))
                    .map(str::trim);
                let name = match event {
                    Some("opened") => "opened",
                    Some("event") => "event",
                    Some("terminal") => "terminal",
                    Some("closed") => "closed",
                    Some("error") => "error",
                    _ => "other",
                };
                trace.last_frame = name;
                let data = frame
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                    .collect::<Vec<_>>()
                    .join("\n");
                let value = serde_json::from_str::<Value>(&data).ok();
                let cursor = value.as_ref().and_then(|v| {
                    v.get("cursor")
                        .or_else(|| v.get("session").and_then(|s| s.get("cursor")))
                });
                let kind = value
                    .as_ref()
                    .and_then(|v| v.get("data"))
                    .and_then(|v| v.get("type"))
                    .and_then(Value::as_str);
                let kind = match kind {
                    Some("state") => Some("state"),
                    Some("prep") => Some("prep"),
                    Some("prompt") => Some("prompt"),
                    Some("text") => Some("text"),
                    Some("thought") => Some("thought"),
                    Some("tool_use") => Some("tool_use"),
                    Some("tool_result") => Some("tool_result"),
                    Some("error") => Some("error"),
                    Some("turn_done") => Some("turn_done"),
                    _ => None,
                };
                let code = value
                    .as_ref()
                    .and_then(|v| v.get("error"))
                    .and_then(|v| v.get("code"))
                    .and_then(Value::as_str)
                    .filter(|s| {
                        s.len() <= 96 && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                    });
                diagnostic(
                    &trace.file,
                    &json!({"stage":"frame","path":trace.endpoint,
                    "name":name,"kind":kind,"payload_json":value.is_some(),
                    "sequence":ordinal(value.as_ref().and_then(|v|v.get("sequence"))),
                    "cursor_sequence":ordinal(cursor.and_then(|v|v.get("sequence"))),
                    "generation":cursor.and_then(|v|v.get("generation")).and_then(Value::as_str)
                        .filter(|s|s.len()<=96 && s.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-' || b==b'_')),
                    "error_code":code}),
                );
                std::task::Poll::Ready(Some(Ok(bytes)))
            }
            std::task::Poll::Ready(Some(Err(error))) => {
                diagnostic(
                    &trace.file,
                    &json!({"stage":"body_error","path":trace.endpoint,
                    "reason":"upstream_body_error","last_frame":trace.last_frame}),
                );
                std::task::Poll::Ready(Some(Err(error)))
            }
            std::task::Poll::Ready(None) => {
                trace.eof = true;
                diagnostic(
                    &trace.file,
                    &json!({"stage":"stream_end","path":trace.endpoint,
                    "reason":"upstream_eof","last_frame":trace.last_frame}),
                );
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    });
    Response::from_parts(parts, axum::body::Body::from_stream(output))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a real browser observer and GRAPHIT_UI_BROWSER_PROBE_FILE"]
async fn web_live_browser_probe_signed_oauth_assets_and_citations() {
    let probe = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_UI_BROWSER_PROBE_FILE")
            .expect("set the browser probe path for this explicit observer scenario"),
    );
    assert!(!probe.exists(), "use a fresh browser probe path");
    let completion = probe.with_extension("done");
    assert!(!completion.exists(), "use a fresh completion path");
    let (temp, tasks, raw) = fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Hub),
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        Some(&[
            Permission::ReadUser,
            Permission::MutateUser,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ]),
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let limits = HistoryLimits::capture(
        graphit_core::native_document_file::NativeDocumentFileLimits::new(
            2 * 1024 * 1024,
            4096,
            128,
            8192,
        )
        .unwrap(),
        tasks.browser_policy().0,
    )
    .unwrap();
    let tasks = tasks
        .with_live_sessions(Arc::new(LiveSessionService::new(limits)))
        .unwrap();
    live_execute::configure(&tasks);
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let project_id = snapshot.projects[0].1;
    let project_version = snapshot.projects[0].2.version().to_owned();
    native::publish(
        &tasks,
        &raw,
        0,
        &[
            native::page("same", "Live productive Knowledge", &["other"]),
            native::page("other", "Referenced Knowledge", &[]),
        ],
        Some(&[
            KnowledgeReferenceFields {
                slug: "same",
                state: KnowledgeDocumentState::Active,
                namespace: KnowledgeReferenceNamespace {
                    scope: KnowledgeReferenceScope::Project(project_id),
                    context: None,
                },
                authored: &[],
            },
            KnowledgeReferenceFields {
                slug: "other",
                state: KnowledgeDocumentState::Active,
                namespace: KnowledgeReferenceNamespace {
                    scope: KnowledgeReferenceScope::Project(project_id),
                    context: None,
                },
                authored: &[],
            },
        ]),
    )
    .await;
    let selections = live_execute::installed_texts(&tasks);
    let (available, journal) =
        live_execute::attach_peer_with_mode(temp.path(), &tasks, "live_browser", None);
    let mut browser_policy = policy();
    // This optional observer fixture needs time for browser interaction; product TTLs are unchanged.
    browser_policy.session_lifetime = Duration::from_secs(900);
    browser_policy.login_timeout = Duration::from_secs(60);
    let mut fixture = Fixture::with_live_tasks(browser_policy, (tasks.clone(), raw)).await;
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let response = fixture
        .client
        .get(format!("{}/live", fixture.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.text().await.unwrap().contains("<html"));
    std::fs::write(
        &probe,
        serde_json::to_vec(&json!({
            "scenario":"live_execution",
            "origin":fixture.origin,
            "path":"/live",
            "project":{"project_id":project_id.to_string(),"project_version":project_version},
            "agent":selections[0],
            "artifacts":[selections[1],selections[2]],
            "prompt":"Read the selected Knowledge and show the reference.",
            "citation_slug":"same",
            "citation_title":"Live productive Knowledge",
        "reference_slug":"other",
        "reference_title":"Referenced Knowledge",
            "page_body":"Complete native body é",
            "binding":graphit_hub::NativeBinding::KiroV3,
            "model":"fixture-model",
            "controlled_peer":true,
            "vendor_native_qualified":false,
            "browser_qualified":false,
        }))
        .unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(900);
    let mut finished = false;
    while Instant::now() < deadline {
        if completion.exists() {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let observed = std::fs::read_to_string(&journal).unwrap_or_default();
    let assets_checked = observed.lines().any(|line| line == "live_assets_checked");
    let mcp_joined = observed
        .lines()
        .any(|line| line == "live_mcp_four_tools_joined");
    let prompt_checked = observed
        .lines()
        .any(|line| line == "live_browser_prompt_checked");
    let completed = observed.lines().any(|line| line == "terminal");
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    fixture.service.shutdown().await.unwrap();
    fixture.stop.cancel();
    for server in fixture.servers.drain(..) {
        // Issuer and host listeners belong only to this fixture and have no native children.
        server.abort();
        let _ = server.await;
    }
    let userinfo_calls = fixture.issuer.userinfo.load(Ordering::SeqCst);
    std::fs::write(
        probe.with_extension("result.json"),
        serde_json::to_vec(&json!({
            "finished":finished,
            "assets_checked":assets_checked,
            "mcp_four_tools_joined":mcp_joined,
            "prompt_checked":prompt_checked,
            "completed":completed,
            "native_owners_joined":true,
            "userinfo_calls":userinfo_calls,
            "browser_qualified":false,
            "vendor_native_qualified":false,
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(finished, "browser probe expired after native owner cleanup");
    assert!(
        assets_checked && mcp_joined && prompt_checked && completed,
        "the browser must run the actual prepared peer and private MCP before completing the probe"
    );
    assert_eq!(userinfo_calls, 0);
}
