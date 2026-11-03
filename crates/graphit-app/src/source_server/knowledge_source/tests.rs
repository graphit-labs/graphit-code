use super::*;
use crate::task_server::{
    TaskServer,
    browser::{knowledge_fixture, records_fixture as native},
};
use graphit_auth::Permission;
use graphit_core::{brand::Brand, module_settings::Module};
use graphit_data::knowledge_references::{
    KnowledgeDocumentState, KnowledgeReferenceFields, KnowledgeReferenceNamespace,
    KnowledgeReferenceScope,
};
use rmcp::{
    RoleClient,
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::collections::HashMap;

type Fixture = super::super::test_fixture::Fixture;
type Client = rmcp::service::RunningService<RoleClient, ()>;

fn fixture(read: bool) -> Fixture {
    let mut permissions = vec![Permission::MutateModule(Module::Knowledge)];
    if read {
        permissions.push(Permission::Read(Module::Knowledge));
    }
    super::super::test_fixture::fixture(&permissions, Brand::default(), NativeBinding::KiroV3)
}
fn task_fixture(f: &Fixture) -> TaskServer {
    let mut policy = crate::foreground_tasks::task_policy(
        &f.server.0.policy.native_install.hub,
        f.server.0.policy.scope,
        f.server.0.policy.index,
        f.server.0.policy.binding,
    )
    .unwrap();
    policy.content = f.server.0.policy.content;
    native::with_import_registry(
        &TaskServer::new(
            Arc::clone(&f.auth),
            Arc::clone(&f.projects),
            0,
            policy,
            CancellationToken::new(),
        )
        .unwrap(),
    )
}
async fn publish(
    f: &Fixture,
    tasks: &TaskServer,
) -> graphit_data::knowledge_index::KnowledgeFirstPublication {
    let body = format!(
        "# Complete é\n{}\nEND-OF-COMPLETE-MARKDOWN\n",
        "Literal Markdown **strong** with Unicode é.\n".repeat(90)
    );
    assert!(body.chars().count() > 1000);
    let mut page = native::page("alpha", "Full page", &[]);
    page.body = Some(&body);
    let tail = native::page("z-tail", "Late page", &[]);
    let namespace = KnowledgeReferenceNamespace {
        scope: KnowledgeReferenceScope::Project(f.projects[0].1),
        context: None,
    };
    let refs = ["alpha", "z-tail"].map(|slug| KnowledgeReferenceFields {
        slug,
        state: KnowledgeDocumentState::Active,
        namespace,
        authored: &[],
    });
    native::publish(tasks, &f.raw, 0, &[page, tail], Some(&refs)).await
}
async fn connect(f: &Fixture) -> (Client, tokio::task::JoinHandle<io::Result<()>>) {
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
    let owner = f.server.clone();
    let serving = tokio::spawn(async move { owner.serve(listener).await });
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {}", f.raw)).unwrap();
    bearer.set_sensitive(true);
    let headers = HashMap::from([
        (axum::http::header::AUTHORIZATION, bearer),
        (
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("runtime"),
        ),
    ]);
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    (client, serving)
}
fn params(mut input: Value) -> CallToolRequestParams {
    input["ai_optimized"] = json!(false);
    CallToolRequestParams::new(TOOL).with_arguments(input.as_object().unwrap().clone())
}
async fn read(client: &Client, input: Value, error: bool) -> Value {
    let result = client.call_tool(params(input)).await.unwrap();
    assert_eq!(result.is_error.unwrap_or(false), error, "{result:?}");
    result.structured_content.expect("native response")
}
async fn close(f: &Fixture, client: Client, serving: tokio::task::JoinHandle<io::Result<()>>) {
    client.cancel().await.unwrap();
    f.stop.cancel();
    f.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_native_project_and_imported_source_full_markdown_slices_and_namespace() {
    let mut f = fixture(true);
    let config = graphit_core::config::ConfigSnapshot::new(f.paths.brand(), []).unwrap();
    f.server.attach_project_hook(&config).unwrap();
    let tasks = task_fixture(&f);
    let publication = publish(&f, &tasks).await;
    let imported = serde_json::to_value(
        native::register_copy(&tasks, &publication, "reader/library", "exact-1", 0).await,
    )
    .unwrap();
    let registry = native::registry_bytes(&tasks);
    let (client, serving) = connect(&f).await;
    let announced = client.list_tools(Default::default()).await.unwrap();
    assert_eq!(announced.tools.iter().filter(|t| t.name == TOOL).count(), 1);
    let guide = client.call_tool(CallToolRequestParams::new("graphit_module_skill").with_arguments(
        json!({"ai_optimized":false,"module":"knowledge","project_dir":NativePath::new(f.projects[0].0.root().to_owned()).unwrap()}).as_object().unwrap().clone(),
    )).await.unwrap();
    assert_ne!(guide.is_error, Some(true), "{guide:?}");
    let guide: Value = serde_json::from_str(&guide.content[0].as_text().unwrap().text).unwrap();
    let text = guide["content"].as_str().unwrap();
    assert!(text.contains("`graphit_wiki_source`"));
    let example = text
        .split("```json\n")
        .skip(1)
        .map(|part| part.split("```").next().unwrap())
        .find(|example| example.contains("\"slug\""))
        .expect("generated Native reader example");
    assert_eq!(
        read(&client, serde_json::from_str(example).unwrap(), false).await["native"]["kind"],
        "found"
    );
    for source in [json!({"kind":"project_knowledge"}), imported.clone()] {
        let result = read(&client, json!({"slug":"alpha","source":source}), false).await;
        assert_eq!(result["native"]["kind"], "found");
        assert_eq!(
            result["native"]["generation"]["version"],
            publication.default.version
        );
        let markdown = result["native"]["selection"]["source"].as_str().unwrap();
        assert!(markdown.starts_with("---\n"));
        assert!(markdown.len() > 1000);
        assert!(markdown.ends_with("END-OF-COMPLETE-MARKDOWN\n"));
        assert!(markdown.contains("Literal Markdown **strong** with Unicode é."));
        assert_eq!(
            result["focused_source"]["namespace"]["scope"]["project"],
            f.projects[0].1.to_string()
        );
        assert_eq!(result["source"], source);
        let sliced = read(
            &client,
            json!({"slug":"alpha","source":source,"selectors":{"tail":2,"line_numbers":true}}),
            false,
        )
        .await;
        assert!(
            sliced["native"]["selection"]["source"]
                .as_str()
                .unwrap()
                .contains("END-OF-COMPLETE-MARKDOWN")
        );
        assert!(
            sliced["native"]["selection"]["source"]
                .as_str()
                .unwrap()
                .len()
                < 120
        );
        assert_eq!(
            sliced["native"]["selection"]["total_lines"],
            result["native"]["selection"]["total_lines"]
        );
        let range = read(
            &client,
            json!({"slug":"alpha","source":source,"selectors":{"start_line":1,"end_line":1}}),
            false,
        )
        .await;
        assert_eq!(range["native"]["selection"]["source"], "---");
        let absent = read(&client, json!({"slug":"missing","source":source}), false).await;
        assert_eq!(absent["native"]["kind"], "page_absent");
    }
    assert_eq!(native::registry_bytes(&tasks), registry);
    let mut wrong = imported.clone();
    wrong["artifact"]["version"] = json!("other");
    assert_eq!(
        read(&client, json!({"slug":"alpha","source":wrong}), false).await["native"]["kind"],
        "registry_absent"
    );
    close(&f, client, serving).await;
    // An installed page has installation authority and remains readable without a PROJECT selection.
    let policy = f.server.0.policy.clone();
    f.stop = CancellationToken::new();
    f.server = SourceServer::new(
        Arc::clone(&f.auth),
        Arc::new(Vec::new()),
        0,
        policy,
        f.stop.clone(),
    )
    .unwrap();
    let (global, served) = connect(&f).await;
    assert_eq!(
        read(&global, json!({"slug":"alpha","source":imported}), false).await["native"]["kind"],
        "found"
    );
    close(&f, global, served).await;
    tasks.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_native_source_no_corpus_empty_denial_and_output_limits() {
    let f = fixture(true);
    let tasks = task_fixture(&f);
    let (client, serving) = connect(&f).await;
    assert_eq!(
        read(&client, json!({"slug":"alpha"}), false).await["native"]["kind"],
        "no_corpus"
    );
    native::publish(&tasks, &f.raw, 0, &[], None).await;
    assert_eq!(
        read(&client, json!({"slug":"alpha"}), false).await["native"]["kind"],
        "published_empty"
    );
    for input in [
        json!({"slug":"alpha","selectors":{"head":-1}}),
        json!({"slug":"alpha","selectors":{"pattern":"[","is_regex":true}}),
        json!({"path":"alpha"}),
        json!({"slug":"alpha","selectors":{"unknown":1}}),
        json!({"slug":"alpha","context":"old@v1"}),
    ] {
        assert!(read(&client, input, true).await.get("code").is_some());
    }
    close(&f, client, serving).await;
    tasks.shutdown().await.unwrap();

    let f = fixture(false);
    let tasks = task_fixture(&f);
    let publication = publish(&f, &tasks).await;
    let imported = serde_json::to_value(
        native::register_copy(&tasks, &publication, "reader/denied", "exact-1", 0).await,
    )
    .unwrap();
    let (client, serving) = connect(&f).await;
    assert_eq!(
        read(&client, json!({"slug":"alpha"}), true).await["code"],
        "authentication_or_permission_denied"
    );
    assert_eq!(
        read(&client, json!({"slug":"alpha","source":imported}), true).await["code"],
        "authentication_or_permission_denied"
    );
    close(&f, client, serving).await;
    tasks.shutdown().await.unwrap();

    let mut f = fixture(true);
    let tasks = task_fixture(&f);
    publish(&f, &tasks).await;
    Arc::get_mut(&mut f.server.0)
        .unwrap()
        .policy
        .content
        .max_output_bytes = 2048;
    let (client, serving) = connect(&f).await;
    assert_eq!(
        read(&client, json!({"slug":"alpha"}), true).await["code"],
        "knowledge_source_limit"
    );
    assert_eq!(
        read(
            &client,
            json!({"slug":"alpha","selectors":{"head":1}}),
            false
        )
        .await["native"]["selection"]["source"],
        "---"
    );
    close(&f, client, serving).await;
    tasks.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_native_source_selected_integrity_is_independent_of_other_pages() {
    let f = fixture(true);
    let tasks = task_fixture(&f);
    publish(&f, &tasks).await;
    let (client, serving) = connect(&f).await;
    assert_eq!(
        read(&client, json!({"slug":"alpha"}), false).await["native"]["kind"],
        "found"
    );
    knowledge_fixture::corrupt_unselected_source_path(&tasks).await;
    assert_eq!(
        read(&client, json!({"slug":"alpha"}), false).await["native"]["kind"],
        "found"
    );
    let failed = read(&client, json!({"slug":"z-tail"}), true).await;
    assert_eq!(failed["code"], "knowledge_index_failure");
    assert!(failed.get("native").is_none());
    close(&f, client, serving).await;
    tasks.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_native_source_queued_cancel_and_deadline_join_existing_owner() {
    use rmcp::model::{CallToolRequest, ClientRequest};
    for deadline in [false, true] {
        let mut f = fixture(true);
        Arc::get_mut(&mut f.server.0).unwrap().policy.timeout = Duration::from_millis(300);
        let (client, serving) = connect(&f).await;
        let jobs = f.server.0.jobs.lock().await;
        let request = client
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(params(
                    json!({"slug":"alpha"}),
                ))),
                Default::default(),
            )
            .await
            .unwrap();
        if deadline {
            let response = request.await_response().await.unwrap();
            assert!(format!("{response:?}").contains("request_cancelled_or_expired"));
        } else {
            request.cancel(None).await.unwrap();
        }
        drop(jobs);
        close(&f, client, serving).await;
    }
}
