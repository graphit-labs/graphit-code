#![cfg(target_os = "linux")]
//! New App surfaces over real Source authentication; subscriber internals have their own receipt.
use graphit_app::{
    mcp_source::SourceProxy,
    native_install_http::NativeInstallPolicy,
    source_server::{SourcePolicy, SourceServer},
};
#[path = "support/knowledge_ask_server.rs"]
mod support;
use graphit_auth::{ExternalEffect, Permission};
use graphit_core::{
    brand::Brand, config::ConfigSnapshot, file_lock::Cancellation, module_settings::Module,
};
use graphit_hub::{NativeBinding, policy::RequestContext};
use rmcp::{
    RoleClient, ServerHandler, ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr, time::Duration};

#[path = "support/now_fixture.rs"]
mod wire;
use wire::{ProtocolServer, content, original, remote_fixture};

async fn serve(
    f: &support::Fixture,
) -> (
    SocketAddr,
    tokio::task::JoinHandle<std::io::Result<()>>,
    graphit_daemon::source_discovery::DiscoveryRecord,
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
    let address = listener.address();
    let cancel = Cancellation::default();
    let original = original(&cancel);
    let discovery = graphit_daemon::source_discovery::publish_discovery(
        &f.auth,
        &listener,
        graphit_core::native_document_file::NativeDocumentFileLimits::new(65536, 4096, 128, 65536)
            .unwrap(),
        original.snapshot(),
    )
    .unwrap();
    let server = f.server.clone();
    (
        address,
        tokio::spawn(async move { SourceServer::serve(&server, listener).await }),
        discovery,
    )
}
async fn client(address: SocketAddr, raw: &str, origin: &str) -> RunningService<RoleClient, ()> {
    let mut headers = HashMap::new();
    let mut bearer = reqwest::header::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(reqwest::header::AUTHORIZATION, bearer);
    headers.insert(
        reqwest::header::HeaderName::from_static("x-graphit-auth-origin"),
        reqwest::header::HeaderValue::from_str(origin).unwrap(),
    );
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
            .custom_headers(headers),
    );
    ().serve(transport).await.unwrap()
}
fn params(name: &str, value: Value) -> CallToolRequestParams {
    let Value::Object(mut arguments) = value else {
        panic!("MCP arguments must be an object");
    };
    arguments.insert("ai_optimized".into(), Value::Bool(false));
    CallToolRequestParams::new(name.to_owned()).with_arguments(arguments)
}
fn value(result: CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true));
    serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
}
fn local(f: &support::Fixture) -> Value {
    json!({"mode":"local","project_id":f.projects[0].1,"version":f.projects[0].2.version()})
}
fn attach(f: &mut support::Fixture) {
    f.server
        .attach_now(None, &ConfigSnapshot::new(f.paths.brand(), []).unwrap())
        .unwrap();
}
async fn stop(
    f: &support::Fixture,
    served: tokio::task::JoinHandle<std::io::Result<()>>,
    discovery: &graphit_daemon::source_discovery::DiscoveryRecord,
) {
    f.stop.cancel();
    served.await.unwrap().unwrap();
    let cancel = Cancellation::default();
    let original = original(&cancel);
    assert!(
        graphit_daemon::source_discovery::remove_discovery(
            &f.auth,
            discovery,
            graphit_core::native_document_file::NativeDocumentFileLimits::new(
                65536, 4096, 128, 65536
            )
            .unwrap(),
            original.snapshot(),
        )
        .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_http_catalog_local_heads_and_independent_module_denials() {
    let mut f = support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    wire::install_local_profile(&f, "http://127.0.0.1:9");
    attach(&mut f);
    let before = f.server.get_tool("graphit_now").unwrap();
    assert_eq!(
        before.input_schema.get("additionalProperties"),
        Some(&json!(false))
    );
    let (address, served, discovery) = serve(&f).await;
    let caller = client(address, &f.raw, "runtime").await;
    let tools = caller.peer().list_all_tools().await.unwrap();
    for name in ["graphit_now", "graphit_instances", "graphit_ast_source"] {
        assert!(tools.iter().any(|tool| tool.name == name));
    }
    let now = value(
        caller
            .peer()
            .call_tool(params("graphit_now", local(&f)))
            .await
            .unwrap(),
    );
    assert_eq!(now["project_id"], json!(f.projects[0].1));
    assert_eq!(now["sections"]["tasks"]["availability"], "no_corpus");
    for section in ["memories", "knowledge"] {
        assert_eq!(now["sections"][section]["availability"], "failed");
        assert_eq!(now["sections"][section]["error"], "forbidden");
        assert!(now["sections"][section]["total"].is_null());
    }
    let serialized = now.to_string();
    assert!(!serialized.contains(f.temp.path().to_str().unwrap()));
    assert!(!serialized.contains(&f.raw));
    assert!(!serialized.contains("actual native payload"));
    let local_caller = client(address, "now-local-client", "local").await;
    let local_now = value(
        local_caller
            .peer()
            .call_tool(params("graphit_now", local(&f)))
            .await
            .unwrap(),
    );
    assert_eq!(local_now["sections"]["tasks"]["availability"], "no_corpus");
    local_caller.cancel().await.unwrap();
    let root = f.projects[0].0.root();
    std::fs::rename(root, f.temp.path().join("old-project")).unwrap();
    std::fs::create_dir(root).unwrap();
    assert_eq!(
        caller
            .peer()
            .call_tool(params("graphit_now", local(&f)))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    caller.cancel().await.unwrap();
    stop(&f, served, &discovery).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_selectors_invalid_credentials_and_remote_permission_remain_failures() {
    let mut f = support::fixture(
        &[Permission::Read(Module::Memory)],
        Brand::default(),
        NativeBinding::Codex,
    );
    attach(&mut f);
    let (address, served, discovery) = serve(&f).await;
    let caller = client(address, &f.raw, "runtime").await;
    let mut cases = vec![
        json!({"mode":"local","project_id":f.projects[0].1}),
        json!({"mode":"local","project_id":f.projects[0].1,"version":null}),
        json!({"mode":"local","project_id":f.projects[0].1,"version":"absent"}),
        json!({"mode":"broker","project_id":f.projects[0].1}),
        json!({"mode":"local","project_id":f.projects[0].1,"version":"main","provider":"now"}),
    ];
    let mut unknown = local(&f);
    unknown["project_dir"] = json!("untrusted");
    cases.push(unknown);
    for arguments in cases {
        assert_eq!(
            caller
                .peer()
                .call_tool(params("graphit_now", arguments))
                .await
                .unwrap()
                .is_error,
            Some(true)
        );
    }
    let instances = caller
        .peer()
        .call_tool(params(
            "graphit_instances",
            json!({"project_id":f.projects[0].1,"provider":"now"}),
        ))
        .await
        .unwrap();
    assert_eq!(instances.is_error, Some(true));
    let fake = client(address, "invalid-source-proof", "runtime").await;
    assert_eq!(
        fake.peer()
            .call_tool(params("graphit_now", local(&f)))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    let wrong_origin = client(address, &f.raw, "local").await;
    assert_eq!(
        wrong_origin
            .peer()
            .call_tool(params("graphit_now", local(&f)))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    wrong_origin.cancel().await.unwrap();
    fake.cancel().await.unwrap();
    caller.cancel().await.unwrap();
    stop(&f, served, &discovery).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sdk_stdio_forwards_now_and_instances_without_source_argument_rewrites() {
    let mut f = support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    attach(&mut f);
    let (address, served, discovery) = serve(&f).await;
    let cancel = Cancellation::default();
    let original = original(&cancel);
    let proxy = SourceProxy::new(
        f.paths.clone(),
        address,
        reqwest::Client::new(),
        (
            graphit_core::native_document_file::NativeDocumentFileLimits::new(
                65536, 4096, 128, 65536,
            )
            .unwrap(),
            content(),
            Duration::from_secs(30),
        ),
        None,
        &original,
    )
    .await
    .unwrap();
    let (local_io, remote_io) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote_io);
    let task_proxy = proxy.clone();
    let joined = tokio::spawn(async move { task_proxy.serve_stream(read, write).await });
    let caller = ().serve(local_io).await.unwrap();
    let tools = caller.peer().list_all_tools().await.unwrap();
    for name in ["graphit_now", "graphit_instances", "graphit_ast_source"] {
        assert!(tools.iter().any(|t| t.name == name));
    }
    let result = value(
        caller
            .peer()
            .call_tool(params("graphit_now", local(&f)))
            .await
            .unwrap(),
    );
    assert_eq!(result["project_id"], json!(f.projects[0].1));
    assert_eq!(
        caller
            .peer()
            .call_tool(params(
                "graphit_instances",
                json!({"project_id":f.projects[0].1,"provider":"now"})
            ))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    caller.cancel().await.unwrap();
    joined.await.unwrap().unwrap();
    proxy.shutdown().await.unwrap();
    stop(&f, served, &discovery).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_now_shutdown_cancels_a_pending_http_handshake_and_joins() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut f = support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    attach(&mut f);
    let (address, served, discovery) = serve(&f).await;
    let cancel = Cancellation::default();
    let original = original(&cancel);
    let proxy = SourceProxy::new(
        f.paths.clone(),
        address,
        reqwest::Client::new(),
        (
            graphit_core::native_document_file::NativeDocumentFileLimits::new(
                65536, 4096, 128, 65536,
            )
            .unwrap(),
            content(),
            Duration::from_secs(30),
        ),
        None,
        &original,
    )
    .await
    .unwrap();
    stop(&f, served, &discovery).await;
    // Publish the replacement listener through the real discovery producer.
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &cancel,
    )
    .unwrap();
    let discovery = graphit_daemon::source_discovery::publish_discovery(
        &f.auth,
        &listener,
        graphit_core::native_document_file::NativeDocumentFileLimits::new(65536, 4096, 128, 65536)
            .unwrap(),
        original.snapshot(),
    )
    .unwrap();
    let listener = listener.into_listener();
    listener.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let (entered, observed) = tokio::sync::oneshot::channel();
    let stalled = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(socket.read(&mut bytes).await.unwrap() > 0);
        entered.send(()).unwrap();
        let mut trailing = Vec::new();
        let _ = socket.read_to_end(&mut trailing).await;
        let _ = socket.shutdown().await;
    });
    let (local_io, remote_io) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote_io);
    let served_proxy = proxy.clone();
    let joined = tokio::spawn(async move { served_proxy.serve_stream(read, write).await });
    let caller = ().serve(local_io).await.unwrap();
    let peer = caller.peer().clone();
    let arguments = local(&f);
    let call = tokio::spawn(async move { peer.call_tool(params("graphit_now", arguments)).await });
    tokio::time::timeout(Duration::from_secs(5), observed)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), proxy.shutdown())
        .await
        .unwrap()
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), call)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err() || result.unwrap().is_error == Some(true));
    tokio::time::timeout(Duration::from_secs(2), stalled)
        .await
        .unwrap()
        .unwrap();
    let _ = caller.cancel().await;
    joined.await.unwrap().unwrap();
    assert!(
        graphit_daemon::source_discovery::remove_discovery(
            &f.auth,
            &discovery,
            graphit_core::native_document_file::NativeDocumentFileLimits::new(
                65536, 4096, 128, 65536
            )
            .unwrap(),
            original.snapshot(),
        )
        .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subscribe_refuses_unauthenticated_input_and_untrusted_http_origin() {
    let mut f = support::fixture(
        &[Permission::External(ExternalEffect::RemoteRequest)],
        Brand::default(),
        NativeBinding::Codex,
    );
    attach(&mut f);
    let (address, served, discovery) = serve(&f).await;
    let client = reqwest::Client::new();
    let url = format!("http://{address}/api/now/subscribe");
    let request = json!({"project_id":f.projects[0].1,"provider":"now"});
    assert_eq!(
        client
            .post(&url)
            .body(request.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(&url)
            .header("origin", "http://foreign.invalid")
            .bearer_auth(&f.raw)
            .header("x-graphit-auth-origin", "runtime")
            .body(request.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    let invalid = json!({"project_id":f.projects[0].1,"provider":"now","version":null});
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&f.raw)
            .header("x-graphit-auth-origin", "runtime")
            .body(invalid.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    stop(&f, served, &discovery).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_app_tools_use_subscriber_first_snapshot_filters_and_finite_close() {
    let mut protocol = ProtocolServer::start().await;
    let f = remote_fixture(&protocol.endpoint);
    let (address, served, discovery) = serve(&f).await;
    let caller = client(address, &f.raw, "runtime").await;
    let full = value(
        caller
            .peer()
            .call_tool(params(
                "graphit_instances",
                json!({"project_id":f.projects[0].1,"provider":"now"}),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(full["type"], "snapshot");
    assert_eq!(full["instances"].as_array().unwrap().len(), 2);
    assert_eq!(
        full["instances"][0]["now"]["generated_at"],
        "9007199254740993001"
    );
    protocol.closed().await;
    let filtered=value(caller.peer().call_tool(params("graphit_now",json!({"mode":"broker","project_id":f.projects[0].1,"provider":"now","instance_id":"b","version":"feature/β"}))).await.unwrap());
    assert_eq!(filtered["instances"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["instances"][0]["instance_id"], "b");
    protocol.closed().await;
    caller.cancel().await.unwrap();
    stop(&f, served, &discovery).await;
    protocol.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_app_ndjson_full_empty_replacements_and_body_drop_close_the_owner() {
    let mut protocol = ProtocolServer::start().await;
    let f = remote_fixture(&protocol.endpoint);
    let (address, served, discovery) = serve(&f).await;
    let cancel = Cancellation::default();
    let original = RequestContext::service(content(), &cancel).unwrap();
    let request = graphit_app::source_server::BrokerRequest {
        project_id: f.projects[0].1,
        provider: "now".into(),
        version: None,
        instance_id: None,
    };
    let client = reqwest::Client::new();
    let mut stream = graphit_app::now_transport::connect_now(
        &f.paths,
        address,
        &client,
        (
            graphit_core::native_document_file::NativeDocumentFileLimits::new(
                65536, 4096, 128, 65536,
            )
            .unwrap(),
            content(),
        ),
        &request,
        &original,
    )
    .await
    .unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap().instances().len(), 2);
    assert!(stream.next().await.unwrap().unwrap().instances().is_empty());
    drop(stream);
    protocol.closed().await;
    stop(&f, served, &discovery).await;
    protocol.shutdown().await;
}
