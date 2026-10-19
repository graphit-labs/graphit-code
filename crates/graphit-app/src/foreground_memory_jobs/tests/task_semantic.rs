//! Real Task producer, background embedding client and authenticated MCP/HTTP consumers.
use super::*;
#[allow(dead_code)]
use crate::rerank_test_http as wire;
use graphit_auth::profile::DirectProtocol;
use graphit_daemon::supervisor::ProjectContext;
use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};

#[tokio::test]
async fn task_direct_background_reuses_vectors_and_mcp_http_search() {
    let cancel = Cancellation::default();
    let config = ConfigSnapshot::new(
        &Brand::default(),
        [ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Global,
            [
                (
                    ConfigKey::new("broker.allow_loopback_http").unwrap(),
                    json!(true),
                ),
                (Module::Memory.key(), json!(false)),
                (Module::Ast.key(), json!(false)),
                (Module::Knowledge.key(), json!(false)),
                (Module::Task.key(), json!(true)),
                (Module::Embedding.key(), json!(true)),
            ],
        )
        .unwrap()],
    )
    .unwrap();
    let policy = HubResourcePolicy::resolve(&config).unwrap();
    let request = policy.request_context(Instant::now(), &cancel).unwrap();
    let response = || {
        wire::reply(json!({"object":"list","model":"task-direct","data":[
        {"object":"embedding","index":0,"embedding":[1.,0.,0.,0.]}
    ],"usage":{"prompt_tokens":2}}))
    };
    let wire = wire::fixture_with_deadline(
        vec![response(), response(), response(), wire::reply(json!({"object":"list","model":"task-direct","data":[{"object":"embedding","index":0,"embedding":[1.,0.]}],"usage":{"prompt_tokens":2}}))],
        request.snapshot().required_deadline().unwrap(),
    )
    .await;
    let mut fixture = Fixture::with_ai_services(
        AiService::Direct {
            protocol: DirectProtocol::OpenAi,
            endpoint: format!("{}/v1/embeddings", wire.origin),
            model: "task-direct".into(),
            model_revision: "r1".into(),
            recipe: "retrieval-r1".into(),
            dimensions: 4,
            request_dimensions: None,
            batch_endpoint: None,
        },
        AiService::Disabled,
    );
    fixture.projects.truncate(1);
    fixture.config = config;
    fixture.policy = policy;
    let provider = active_provider(&fixture.installation, &fixture.config, &request)
        .unwrap()
        .unwrap();
    let foreground = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        &request,
        Some(&provider),
    )
    .unwrap();
    let modules = foreground
        .server
        .as_ref()
        .unwrap()
        .project_modules(&request)
        .unwrap();
    let mut jobs = foreground
        .create_memory_jobs(
            &fixture.installation,
            &fixture.config,
            Some(&provider),
            &request,
            &modules,
        )
        .unwrap()
        .expect("Task + embedding enables background resources");
    let resources = jobs
        .start(
            &fixture.installation,
            Vec::new(),
            foreground.scope,
            foreground.native,
            NativeBinding::KiroV3,
            &request,
        )
        .await
        .unwrap()
        .unwrap();
    assert!(resources.local.is_none());
    assert!(jobs.child.is_none());
    let server = foreground.server.as_ref().unwrap().task_server().unwrap();
    server
        .attach_memory_model(Some(Arc::clone(&resources)))
        .unwrap();
    let runtime = foreground.auth.runtime().check(request.snapshot()).unwrap();
    let raw = std::str::from_utf8(runtime.secret(request.snapshot()).unwrap())
        .unwrap()
        .to_owned();
    let credential = || crate::task_server::request::Credential {
        role_binding: None,
        bearer: zeroize::Zeroizing::new(raw.clone()),
        origin: crate::task_server::request::Origin::Runtime,
    };
    let invoke = |operation, input| {
        server.call(
            serde_json::from_value(json!({"input":input})).unwrap(),
            operation,
            true,
            credential(),
            tokio_util::sync::CancellationToken::new(),
        )
    };
    let session = invoke("create_session", json!({"idempotency_key":"task-semantic-session","title":"Session","description":"Task search","strategy":"Publish progressively"})).await.unwrap_or_else(|_| panic!("create session"));
    let task = invoke("create_task", json!({"idempotency_key":"task-semantic-record","session_id":session["result"]["id"],"title":"Recover publication","description":"Resume an interrupted publication","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Preserve committed records"},{"kind":"test","text":"Search from independent clients"}]})).await.unwrap_or_else(|_| panic!("create task"));
    let task_id = task["result"]["specification"]["id"].clone();
    let lexical = server.call(serde_json::from_value(json!({"input":{"text":"Recover publication","filter":{},"page_size":10,"top_k":10}})).unwrap(), "search", false, credential(), tokio_util::sync::CancellationToken::new()).await.unwrap_or_else(|_| panic!("model-free textual search before embedding"));
    assert_eq!(
        lexical["result"]["Published"]["results"][0]["task"]["id"], task_id,
        "{lexical}"
    );

    let (project, id, version) = &foreground.projects.as_ref().unwrap()[0];
    let selected = ProjectContext {
        id: *id,
        version: version.clone(),
    };
    resources
        .index_tasks(project, &selected, &cancel)
        .await
        .unwrap();
    resources
        .index_tasks(project, &selected, &cancel)
        .await
        .unwrap();
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &cancel,
    )
    .unwrap();
    let base = format!("http://{}", listener.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(listener).await });
    let headers = std::collections::HashMap::from([
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap(),
        ),
        (
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("runtime"),
        ),
    ]);
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(format!("{base}/mcp"))
            .custom_headers(headers),
    );
    let client = ().serve(transport).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    let search = tools
        .iter()
        .find(|tool| tool.name == "graphit_task_search")
        .unwrap();
    assert!(
        serde_json::to_string(&search.input_schema)
            .unwrap()
            .contains("hybrid")
    );
    let invalid = client.call_tool(CallToolRequestParams::new("graphit_task_search".to_owned()).with_arguments(
        json!({"input":{"mode":"semantic","text":" ","filter":{},"page_size":10,"top_k":10}}).as_object().unwrap().clone()
    )).await.unwrap();
    assert_eq!(invalid.is_error, Some(true));
    assert!(
        invalid.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("invalid_arguments")
    );
    let denied = reqwest::Client::new().post(format!("{base}/api/tasks/native/search"))
        .json(&json!({"input":{"mode":"semantic","text":"Recover publication","filter":{},"page_size":10,"top_k":10}}))
        .send().await.unwrap();
    assert_eq!(denied.status(), axum::http::StatusCode::UNAUTHORIZED);
    let semantic = client.call_tool(CallToolRequestParams::new("graphit_task_search".to_owned()).with_arguments(
        json!({"input":{"mode":"semantic","text":"Recover publication","filter":{},"page_size":10,"top_k":10}}).as_object().unwrap().clone()
    )).await.unwrap();
    assert_ne!(semantic.is_error, Some(true), "{semantic:?}");
    let response: serde_json::Value =
        serde_json::from_str(&semantic.content[0].as_text().unwrap().text).unwrap();
    assert_eq!(
        response["result"]["Published"]["results"][0]["task"]["id"], task_id,
        "{response}"
    );
    let response:serde_json::Value = reqwest::Client::new().post(format!("{base}/api/tasks/native/search"))
        .bearer_auth(&raw).header("x-graphit-auth-origin","runtime")
        .json(&json!({"input":{"mode":"hybrid","text":"Recover publication","filter":{},"page_size":10,"top_k":10}}))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_eq!(response["operation"], "search");
    assert_eq!(
        response["result"]["Published"]["results"][0]["task"]["id"], task_id,
        "{response}"
    );
    let malformed = client.call_tool(CallToolRequestParams::new("graphit_task_search".to_owned()).with_arguments(
        json!({"input":{"mode":"semantic","text":"Recover publication","filter":{},"page_size":10,"top_k":10}}).as_object().unwrap().clone()
    )).await.unwrap();
    assert_eq!(
        malformed.is_error,
        Some(true),
        "a configured service failure cannot become empty success"
    );
    client.cancel().await.unwrap();
    let requests = wire.server.await.unwrap();
    assert_eq!(
        requests.len(),
        4,
        "repeated indexing reused the document vector"
    );
    assert!(
        requests[0].body["input"][0]
            .as_str()
            .unwrap()
            .contains("Recover publication")
    );
    assert_eq!(requests[1].body["input"], json!(["Recover publication"]));
    assert_eq!(requests[2].body["input"], json!(["Recover publication"]));
    for request in requests {
        assert!(request.head.contains("Bearer fixture-direct-embedding"));
    }
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    jobs.shutdown().await.unwrap();
    foreground.shutdown().await.unwrap();
}
