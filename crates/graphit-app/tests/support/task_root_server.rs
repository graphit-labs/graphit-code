#![cfg(target_os = "linux")]
use super::*;
use graphit_auth::Permission;
use graphit_core::{
    brand::Brand,
    module_settings::Module,
    tcp_listener::{self, Options, PortSelection},
};
use rmcp::ServiceExt;

#[tokio::test]
async fn root_task_dispatch_preserves_real_permissions_and_rejects_role_origin() {
    let mut fixture = super::test_fixture::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    let hub = graphit_hub::policy::HubResourcePolicy::resolve(
        &graphit_core::config::ConfigSnapshot::new(fixture.paths.brand(), []).unwrap(),
    )
    .unwrap();
    let policy = crate::foreground_tasks::task_policy(
        &hub,
        fixture.server.policy().scope,
        graphit_core::native_document_file::NativeDocumentFileLimits::new(65536, 4096, 32, 4096)
            .unwrap(),
        NativeBinding::Codex,
    )
    .unwrap();
    fixture.server.attach_tasks(policy).unwrap();
    let cancel = Cancellation::default();
    let original = hub.request_context(Instant::now(), &cancel).unwrap();
    let listener = tcp_listener::reserve(
        Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &cancel,
    )
    .unwrap();
    let address = listener.address();
    let serving = fixture.server.clone();
    let served = tokio::spawn(async move { SourceServer::serve(&serving, listener).await });
    let http = reqwest::Client::new();
    let limits =
        graphit_core::native_document_file::NativeDocumentFileLimits::new(65536, 4096, 32, 4096)
            .unwrap();
    let headers = crate::ast_source::runtime_headers(&fixture.paths, limits, &original)
        .await
        .unwrap();
    let connect = |headers| {
        rmcp::transport::StreamableHttpClientTransport::with_client(
            http.clone(),
            rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
                format!("http://{address}/mcp"),
            )
            .custom_headers(headers),
        )
    };
    let client = ().serve(connect(headers.clone())).await.unwrap();
    assert!(
        client
            .peer()
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == "graphit_task_session_create")
    );
    let tools = client.peer().list_all_tools().await.unwrap();
    let query_tool = tools
        .iter()
        .find(|tool| tool.name == "graphit_task_query")
        .unwrap();
    assert!(
        query_tool.input_schema["properties"]["input"]["properties"]
            .get("columns")
            .is_some()
    );
    assert!(tools.iter().any(|tool| tool.name == "graphit_task_schema"));
    let query = json!({"ai_optimized":false,"input":{"selection":{"table":"tasks"},"columns":["id","status"],"page_size":1,"top_k":1}});
    let read = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_task_query")
                .with_arguments(query.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(read.is_error, Some(true));
    let read: Value =
        serde_json::from_str(&read.content.first().unwrap().as_text().unwrap().text).unwrap();
    assert_eq!(read["operation"], "query");
    assert_eq!(read["result"]["state"], "no_corpus");
    let body = json!({"input":{"idempotency_key":"denied-root-create","title":"Denied","description":"ReadTask does not grant write","strategy":"Actual issuer policy"}});
    let denied = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_task_session_create")
                .with_arguments(body.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert!(
        denied
            .content
            .first()
            .unwrap()
            .as_text()
            .unwrap()
            .text
            .contains("authentication_or_permission_denied")
    );
    let header_map: reqwest::header::HeaderMap = headers.clone().into_iter().collect();
    let response = http
        .post(format!("http://{address}/api/tasks/native/create_session"))
        .headers(header_map)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    let mut role_headers = headers;
    role_headers.insert(
        reqwest::header::HeaderName::from_static("x-graphit-auth-origin"),
        reqwest::header::HeaderValue::from_static("role"),
    );
    let role = ().serve(connect(role_headers)).await.unwrap();
    let denied = role
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_task_query")
                .with_arguments(query.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert!(
        denied
            .content
            .first()
            .unwrap()
            .as_text()
            .unwrap()
            .text
            .contains("Root Task requires native Runtime or Local credentials")
    );
    role.cancel().await.unwrap();
    client.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}
