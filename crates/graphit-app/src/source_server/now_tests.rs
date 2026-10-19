use super::*;
use crate::source_server::test_fixture as support;
#[path = "../../tests/support/now_fixture.rs"]
mod wire;
use graphit_auth::{IssuerLimits, Permission, RoleDefinition};
use graphit_core::{brand::Brand, module_settings::Module};
use graphit_hub::{NativeBinding, native::Role};
use rmcp::{
    RoleClient,
    service::{ClientServiceExt, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::collections::HashMap;
use tokio_stream::StreamExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queue_wait_observes_request_deadline_without_admitting_a_worker() {
    let mut f = support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    f.server
        .attach_now(None, &ConfigSnapshot::new(f.paths.brand(), []).unwrap())
        .unwrap();
    Arc::get_mut(
        &mut Arc::get_mut(&mut f.server.0)
            .unwrap()
            .now
            .as_mut()
            .unwrap()
            .0,
    )
    .unwrap()
    .source
    .timeout = Duration::from_millis(20);
    let service = f.server.0.now.as_ref().unwrap().clone();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HOST,
        axum::http::HeaderValue::from_static("127.0.0.1"),
    );
    headers.insert(
        header::AUTHORIZATION,
        axum::http::HeaderValue::from_str(&format!("Bearer {}", f.raw)).unwrap(),
    );
    headers.insert(
        "x-graphit-auth-origin",
        axum::http::HeaderValue::from_static("runtime"),
    );
    let jobs = service.0.jobs.lock().await;
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        service.call(
            CallToolRequestParams::new(service.0.tools[0].name.clone()).with_arguments(
                json!({"project_id":f.projects[0].1,"version":f.projects[0].2.version(),"mode":"local"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            service.credential(&headers).unwrap(),
            CancellationToken::new(),
            None,
        ),
    )
    .await
    .expect("a blocked admission must finish at its own request deadline");
    assert_eq!(result.is_error, Some(true));
    assert!(
        serde_json::to_string(&result)
            .unwrap()
            .contains("now_deadline")
    );
    assert!(jobs.tasks.is_empty());
    drop(jobs);
    service.shutdown().await.unwrap();
    f.server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_unread_http_body_closes_actual_subscriber() {
    let mut protocol = wire::ProtocolServer::start().await;
    let f = wire::remote_fixture(&protocol.endpoint);
    let service = f.server.0.now.as_ref().unwrap().clone();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HOST,
        axum::http::HeaderValue::from_static("127.0.0.1"),
    );
    headers.insert(
        header::AUTHORIZATION,
        axum::http::HeaderValue::from_str(&format!("Bearer {}", f.raw)).unwrap(),
    );
    headers.insert(
        "x-graphit-auth-origin",
        axum::http::HeaderValue::from_static("runtime"),
    );
    let response = subscribe(
        State(service.clone()),
        headers,
        Bytes::from(json!({"project_id":f.projects[0].1,"provider":"now"}).to_string()),
    )
    .await;
    drop(response);
    protocol.closed().await;
    service.shutdown().await.unwrap();
    f.server.shutdown().await.unwrap();
    protocol.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_auth_notice_closes_subscriber_while_http_body_is_not_read() {
    let mut protocol = wire::ProtocolServer::start_with_terminal(true).await;
    let f = wire::remote_fixture(&protocol.endpoint);
    let service = f.server.0.now.as_ref().unwrap().clone();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HOST,
        axum::http::HeaderValue::from_static("127.0.0.1"),
    );
    headers.insert(
        header::AUTHORIZATION,
        axum::http::HeaderValue::from_str(&format!("Bearer {}", f.raw)).unwrap(),
    );
    headers.insert(
        "x-graphit-auth-origin",
        axum::http::HeaderValue::from_static("runtime"),
    );
    let response = subscribe(
        State(service.clone()),
        headers,
        Bytes::from(json!({"project_id":f.projects[0].1,"provider":"now"}).to_string()),
    )
    .await;
    // Keep the HTTP body completely unpolled: first snapshot in body, second fills queue(1).
    protocol.closed().await;
    let body = axum::body::to_bytes(response.into_body(), wire::content().max_output_bytes).await;
    assert!(body.is_err());
    tokio::time::timeout(Duration::from_secs(5), service.shutdown())
        .await
        .unwrap()
        .unwrap();
    f.server.shutdown().await.unwrap();
    protocol.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_shutdown_waiter_keeps_the_owned_join_set_until_next_shutdown() {
    let mut f = support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    f.server
        .attach_now(None, &ConfigSnapshot::new(f.paths.brand(), []).unwrap())
        .unwrap();
    let service = f.server.0.now.as_ref().unwrap().clone();
    let (release, receive) = oneshot::channel();
    service.0.jobs.lock().await.tasks.spawn(async move {
        receive.await.unwrap();
    });
    let waiting = service.clone();
    let waiter = tokio::spawn(async move { waiting.shutdown().await });
    service.0.stop.cancelled().await;
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert_eq!(service.0.jobs.lock().await.tasks.len(), 1);
    release.send(()).unwrap();
    service.shutdown().await.unwrap();
    assert!(service.0.jobs.lock().await.tasks.is_empty());
    f.server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reaped_panic_survives_new_call_and_cancelled_shutdown_until_all_joins() {
    let mut f = support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    f.server
        .attach_now(None, &ConfigSnapshot::new(f.paths.brand(), []).unwrap())
        .unwrap();
    let service = f.server.0.now.as_ref().unwrap().clone();
    let panic = service.0.jobs.lock().await.tasks.spawn(async {
        panic!("previous Now worker panic");
    });
    while !panic.is_finished() {
        tokio::task::yield_now().await;
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        axum::http::HeaderValue::from_str(&format!("Bearer {}", f.raw)).unwrap(),
    );
    headers.insert(
        "x-graphit-auth-origin",
        axum::http::HeaderValue::from_static("runtime"),
    );
    let result = service
        .call(
            CallToolRequestParams::new(service.0.tools[0].name.clone()).with_arguments(
                json!({"project_id":f.projects[0].1,"version":f.projects[0].2.version(),"mode":"local"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            service.credential(&headers).unwrap(),
            CancellationToken::new(),
            None,
        )
        .await;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let (release, receive) = oneshot::channel();
    service.0.jobs.lock().await.tasks.spawn(async move {
        receive.await.unwrap();
    });
    let waiting = service.clone();
    let waiter = tokio::spawn(async move { waiting.shutdown().await });
    service.0.stop.cancelled().await;
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    {
        let jobs = service.0.jobs.lock().await;
        assert!(jobs.first_failure.as_ref().unwrap().is_panic());
        assert!(!jobs.tasks.is_empty());
    }
    release.send(()).unwrap();
    let failure = service.shutdown().await.unwrap_err();
    assert!(
        failure
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<JoinError>())
            .is_some_and(JoinError::is_panic)
    );
    assert!(service.0.jobs.lock().await.tasks.is_empty());
    f.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn terminal_cancellation_discards_first_and_queued_snapshots() {
    let cancel = Cancellation::default();
    let (send, receive) = mpsc::channel(1);
    send.send(Ok(Bytes::from_static(b"queued\n")))
        .await
        .unwrap();
    let mut stream = ResponseStream {
        first: Some(Bytes::from_static(b"first\n")),
        lines: ReceiverStream::new(receive),
        _guard: CancelOnDrop(cancel.clone()),
        ended: false,
    };
    assert!(matches!(
        send.try_send(Err(Failure::Authentication)),
        Err(mpsc::error::TrySendError::Full(_))
    ));
    cancel.cancel();
    assert_eq!(
        stream.next().await.unwrap().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(stream.next().await.is_none());
    assert!(send.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn genuine_role_endpoint_lists_local_now_and_preserves_grant_scope_and_revocation() {
    let mut f = support::fixture_with_projects_and_authority(
        &[
            Permission::Read(Module::Task),
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ],
        &[
            Permission::Read(Module::Task),
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ],
        Brand::default(),
        NativeBinding::KiroV3,
        &["project"],
        &[],
    );
    let config = ConfigSnapshot::new(f.paths.brand(), []).unwrap();
    let content = f.server.0.policy.native_install.hub.content();
    Arc::get_mut(&mut f.server.0).unwrap().policy.content = content;
    f.server.attach_role_tools(config.clone()).unwrap();
    f.server.attach_now(None, &config).unwrap();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        f.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let issuer = f.auth.issuer();
    let caller = issuer.authenticate_runtime(&f.raw, &original).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let permissions = [Permission::Read(Module::Task)];
    let reference = issuer
        .configure_tool_owned_definition(
            &parent,
            RoleDefinition::tool_owned(
                Role::Tracker,
                f.paths.brand(),
                NativeBinding::KiroV3,
                &permissions,
                [37; 32],
                IssuerLimits {
                    max_permissions: 64,
                    max_scope_bytes: 65536,
                    max_parent_bearer_bytes: 4096,
                },
            )
            .unwrap(),
        )
        .unwrap();
    let (paths, id, version) = &f.projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, f.server.0.policy.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let scope = ScopeInput {
        installation: f.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let run = issuer
        .start_run(&parent, &scope, Duration::from_secs(60))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &scope)
        .unwrap();
    let mut headers = HashMap::new();
    let mut auth =
        axum::http::HeaderValue::from_str(&format!("Bearer {}", bearer.expose_for_transport()))
            .unwrap();
    auth.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, auth);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("role"),
    );
    let mut reference_header = axum::http::HeaderValue::from_bytes(
        &reference
            .encode(f.server.0.policy.content.max_input_bytes, &original)
            .unwrap(),
    )
    .unwrap();
    reference_header.set_sensitive(true);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-role-reference"),
        reference_header,
    );
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &cancel,
    )
    .unwrap();
    let address = listener.address();
    let server = f.server.clone();
    let served =
        tokio::spawn(
            async move { crate::source_server::SourceServer::serve(&server, listener).await },
        );
    let client: RunningService<RoleClient, ()> = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(format!(
                    "http://{address}/roles/mcp"
                ))
                .custom_headers(headers),
            ),
            rmcp::service::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    let tools = client.peer().list_all_tools().await.unwrap();
    let now = tools.iter().find(|t| t.name == "graphit_now").unwrap();
    // Hosts must see the complete selection in the advertised object rather
    // than dropping shared properties while projecting a oneOf branch.
    assert!(!now.input_schema.contains_key("oneOf"));
    assert_eq!(now.input_schema["required"], json!(["project_id", "mode"]));
    assert_eq!(now.input_schema["additionalProperties"], false);
    let properties = now.input_schema["properties"].as_object().unwrap();
    for name in ["project_id", "mode", "version", "provider", "instance_id"] {
        assert_eq!(properties[name]["type"], "string");
    }
    assert_eq!(properties["mode"]["enum"], json!(["local", "broker"]));
    assert!(!tools.iter().any(|t| t.name == "graphit_instances"));
    assert!(tools.iter().any(|t| t.name == "graphit_task_list"));
    let local = json!({"mode":"local", "project_id":id, "version":version.version()});
    let params = |name: &str, arguments: Value| {
        let mut arguments = arguments.as_object().unwrap().clone();
        arguments.insert("ai_optimized".into(), Value::Bool(false));
        CallToolRequestParams::new(name.to_owned()).with_arguments(arguments)
    };
    for arguments in [
        json!({"mode":"local", "version":version.version()}),
        json!({"project_id":id, "version":version.version()}),
        json!({"mode":"local", "project_id":id}),
        json!({"mode":"local", "project_id":id, "version":null}),
        json!({"mode":"local", "project_id":id, "version":""}),
        json!({"mode":"broker", "project_id":id}),
        json!({"mode":"local", "project_id":id, "version":version.version(), "provider":"now"}),
        json!({"mode":"local", "project_id":id, "version":version.version(), "instance_id":"instance"}),
    ] {
        let result = client
            .peer()
            .call_tool(params("graphit_now", arguments))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.content[0].as_text().unwrap().text,
            "now_invalid_arguments"
        );
    }
    let result = client
        .peer()
        .call_tool(params("graphit_now", local.clone()))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    let data: Value = serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap();
    assert!(!data.to_string().contains(f.temp.path().to_str().unwrap()));
    assert!(!data.to_string().contains(&f.raw));
    assert_eq!(data["sections"]["tasks"]["availability"], "no_corpus");
    assert_eq!(data["sections"]["memories"]["error"], "forbidden");
    assert_eq!(data["sections"]["knowledge"]["error"], "forbidden");
    let toon = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_now")
                .with_arguments(local.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(toon.is_error, Some(true));
    assert!(toon.structured_content.is_none());
    assert!(
        toon.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("availability: no_corpus")
    );
    assert!(!toon.content[0].as_text().unwrap().text.contains(&f.raw));
    let mut foreign = local.clone();
    foreign["version"] = json!("foreign");
    assert_eq!(
        client
            .peer()
            .call_tool(params("graphit_now", foreign))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    assert_eq!(
        client
            .peer()
            .call_tool(params(
                "graphit_instances",
                json!({"project_id":id,"provider":"now"})
            ))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    issuer.revoke(&parent, &bearer).unwrap();
    assert_eq!(
        client
            .peer()
            .call_tool(params("graphit_now", local))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    client.cancel().await.unwrap();
    f.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}
