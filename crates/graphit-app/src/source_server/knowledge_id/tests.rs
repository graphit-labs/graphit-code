use super::*;

const TOOL: &str = "graphit_knowledge_id";
use graphit_core::{brand::Brand, project_identity::ProjectId};
use rmcp::{
    RoleClient, ServiceExt,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};

async fn connect(uri: &str, raw: &str, origin: &str) -> RunningService<RoleClient, ()> {
    let mut headers = std::collections::HashMap::new();
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(axum::http::header::AUTHORIZATION, bearer);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_str(origin).unwrap(),
    );
    ().serve(StreamableHttpClientTransport::with_client(
        super::super::test_http_client(),
        StreamableHttpClientTransportConfig::with_uri(uri.to_owned()).custom_headers(headers),
    ))
    .await
    .unwrap()
}

async fn serve(server: &SourceServer) -> (String, tokio::task::JoinHandle<io::Result<()>>) {
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
    let service = server.clone();
    let serving = tokio::spawn(async move { SourceServer::serve(&service, listener).await });
    (uri, serving)
}

async fn call(client: &RunningService<RoleClient, ()>, input: Value) -> CallToolResult {
    tokio::time::timeout(
        Duration::from_secs(10),
        client.call_tool(
            CallToolRequestParams::new(TOOL).with_arguments(input.as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn knowledge_id_root_catalogue_calls_default_formatter_and_pure_producer() {
    let fixture = super::super::test_fixture::fixture(
        &[Permission::Read(Module::Knowledge)],
        Brand::default(),
        NativeBinding::KiroV3,
    );
    let (uri, serving) = serve(&fixture.server).await;
    let client = connect(&uri, &fixture.raw, "runtime").await;
    let tools = client.list_all_tools().await.unwrap();
    let selected: Vec<_> = tools.iter().filter(|tool| tool.name == TOOL).collect();
    assert_eq!(selected.len(), 1);
    let tool = selected[0];
    let documented: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/reference/mcp-input-schemas.json"
    )))
    .unwrap();
    let documented = documented["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == TOOL)
        .unwrap();
    assert_eq!(*documented, serde_json::to_value(tool).unwrap());
    assert_eq!(
        serde_json::to_value(tool).unwrap()["inputSchema"]["required"],
        json!(["path"])
    );
    assert!(tool.output_schema.is_none());
    assert_eq!(
        tool.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    let schema = tool.input_schema.as_ref();
    assert_eq!(schema["properties"]["ai_optimized"]["default"], true);
    assert_eq!(
        fixture.server.get_tool(TOOL).unwrap().input_schema,
        tool.input_schema
    );
    let path = Path::new("docs")
        .join("pasta com espaços")
        .join("ação 日本語.md");
    assert!(!fixture.temp.path().join(&path).exists());
    let expected =
        graphit_data::knowledge_document_lifecycle::KnowledgeSourceIdentity::from_relative_path(
            &path,
            None,
            std::num::NonZeroUsize::new(fixture.server.0.policy.content.max_scalar_bytes).unwrap(),
        )
        .unwrap();
    let input = json!({"path":path,"ai_optimized":false});
    let first = call(&client, input.clone()).await;
    assert_eq!(first.is_error, Some(false));
    let value = first.structured_content.unwrap();
    assert_eq!(value, json!({"id":expected.slug()}));
    let id = value["id"].as_str().unwrap();
    assert_eq!(id.len(), 26);
    assert_eq!(id.parse::<ProjectId>().unwrap().to_string(), id);
    assert_eq!(
        call(&client, input).await.structured_content,
        Some(value.clone())
    );
    for options in [
        json!({"path":path}),
        json!({"path":path,"ai_optimized":true}),
    ] {
        let result = call(&client, options).await;
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_none());
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0].as_text().unwrap().text,
            format!("id: {id}")
        );
    }
    let other = call(&client, json!({"path":"other.md","ai_optimized":false})).await;
    assert_ne!(other.structured_content.unwrap(), value);
    assert!(!fixture.temp.path().join(path).exists());
    client.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn knowledge_id_root_rejects_invalid_arguments_paths_and_limits() {
    let fixture = super::super::test_fixture::fixture(
        &[Permission::Read(Module::Knowledge)],
        Brand::default(),
        NativeBinding::KiroV3,
    );
    let (uri, serving) = serve(&fixture.server).await;
    let client = connect(&uri, &fixture.raw, "runtime").await;
    for input in [
        json!({}),
        json!({"path":null}),
        json!({"path":23}),
        json!({"path":"doc.md","authored_id":"extra"}),
        json!({"path":""}),
        json!({"path":"../escape.md"}),
        json!({"path":"docs/../escape.md"}),
        json!({"path":"/absolute.md"}),
        json!({"path":"bad\u{0000}.md"}),
        json!({"path":"x".repeat(fixture.server.0.policy.content.max_scalar_bytes)}),
    ] {
        let mut input = input;
        input["ai_optimized"] = json!(false);
        let result = call(&client, input).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.unwrap()["code"].is_string());
    }
    assert!(
        client
            .call_tool(
                CallToolRequestParams::new(TOOL).with_arguments(
                    json!({"path":"doc.md","ai_optimized":null})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
            )
            .await
            .is_err()
    );
    client.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn knowledge_id_root_requires_native_caller_and_knowledge_permission() {
    for (permissions, valid_bearer, origin) in [
        (vec![Permission::Read(Module::Ast)], true, "runtime"),
        (vec![Permission::Read(Module::Knowledge)], false, "runtime"),
        (vec![Permission::Read(Module::Knowledge)], true, "role"),
    ] {
        let fixture = super::super::test_fixture::fixture(
            &permissions,
            Brand::default(),
            NativeBinding::KiroV3,
        );
        let (uri, serving) = serve(&fixture.server).await;
        let raw = if valid_bearer {
            fixture.raw.as_str()
        } else {
            "invalid-fixture-bearer"
        };
        let client = connect(&uri, raw, origin).await;
        let result = call(&client, json!({"path":"doc.md","ai_optimized":false})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.unwrap().get("id").is_none());
        client.cancel().await.unwrap();
        fixture.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn knowledge_id_root_validated_brand_matches_dynamic_template_name() {
    let brand = Brand::new("orbit").unwrap();
    let name = brand.mcp_tool_name(&["knowledge", "id"]).unwrap();
    let fixture = super::super::test_fixture::fixture(
        &[Permission::Read(Module::Knowledge)],
        brand,
        NativeBinding::KiroV3,
    );
    assert!(fixture.server.get_tool(TOOL).is_none());
    assert!(fixture.server.get_tool(&name).is_some());
    let (uri, serving) = serve(&fixture.server).await;
    let client = connect(&uri, &fixture.raw, "runtime").await;
    assert!(
        client
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == name)
    );
    let result = client
        .call_tool(
            CallToolRequestParams::new(name).with_arguments(
                json!({"path":"docs/page.md","ai_optimized":false})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    assert_eq!(
        result.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .len(),
        26
    );
    client.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
