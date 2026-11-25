//! Actual live owner updates, current SQL inventory and realtime consumers.
use super::*;
use graphit_app::foreground_project_control::{ProjectCommand, ROUTE};
use graphit_core::native_path::NativePath;
use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::Value;

async fn task_call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    input: Value,
    error: bool,
) -> Value {
    let response = client
        .peer()
        .call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(input.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let value: Value = serde_json::from_str(&response.content[0].as_text().unwrap().text).unwrap();
    assert_eq!(response.is_error, Some(error), "{name}: {value}");
    value
}
async fn inventory(client: &reqwest::Client, origin: &str, instance: &str, bearer: &str) -> Value {
    let response = client
        .get(format!("{origin}/v1/instances/{instance}/inventory"))
        .bearer_auth(bearer)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    response.json().await.unwrap()
}
async fn maintain(
    client: &reqwest::Client,
    address: std::net::SocketAddr,
    headers: &std::collections::HashMap<axum::http::HeaderName, axum::http::HeaderValue>,
    command: &ProjectCommand,
    connection: &mut PgConnection,
) -> Value {
    let response = client
        .post(format!("http://{address}{ROUTE}"))
        .headers(
            headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
        .json(command)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let value = response.json::<Value>().await.unwrap();
    if !status.is_success() {
        // Keep the diagnosis in this failing journey; never emit SQL values or credentials.
        let states = sqlx_core::query_as::query_as::<_, (i32, String, Option<String>, String)>(
            "SELECT pid, COALESCE(state,'unknown'), wait_event_type, CASE
                WHEN query ILIKE 'COMMIT%' THEN 'commit'
                WHEN query ILIKE 'ROLLBACK%' THEN 'rollback'
                WHEN query ILIKE 'BEGIN%' OR query ILIKE 'START TRANSACTION%' THEN 'begin'
                WHEN query ILIKE '%instance_inventory%' THEN 'inventory'
                WHEN query ILIKE '%instance_presence%' OR query ILIKE '%instance_now%' THEN 'presence'
                WHEN query ILIKE '%resource_acl%' OR query ILIKE '%grant_capabilities%'
                    OR query ILIKE '%grant_projects%' OR query ILIKE '%resource_grants%' THEN 'acl'
                WHEN query ILIKE 'SELECT%' THEN 'read'
                WHEN query ILIKE 'INSERT%' OR query ILIKE 'UPDATE%' OR query ILIKE 'DELETE%' THEN 'write'
                ELSE 'other' END
             FROM pg_stat_activity WHERE datname=current_database()
                AND backend_type='client backend' AND pid<>pg_backend_pid()",
        )
        .fetch_all(&mut *connection)
        .await;
        match states {
            Ok(states) => eprintln!(
                "Broker failure SQL states (pid/state/wait/last-query class; not correlated with the request phase): {states:?}"
            ),
            Err(error) => eprintln!(
                "Broker SQL diagnosis unavailable: {}",
                error
                    .as_database_error()
                    .and_then(|error| error.code())
                    .as_deref()
                    .unwrap_or("driver")
            ),
        }
    }
    assert_eq!(status, axum::http::StatusCode::OK, "{value}");
    assert_eq!(value["broker_effect"], "confirmed");
    value
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real live Foreground/Broker TCP WebSocket and PostgreSQL TLS"]
async fn live_add_switch_retire_publish_complete_inventory_and_source_selection() {
    let fixture = pg::Fixture::new(true).await;
    let mut connection = fixture.connection().await;
    connection.execute("CREATE ROLE runtime_now_owner LOGIN NOSUPERUSER PASSWORD 'ephemeral-owner'; DROP SCHEMA public CASCADE; CREATE SCHEMA public AUTHORIZATION runtime_now_owner; GRANT CONNECT,TEMP ON DATABASE postgres TO runtime_now_owner").await.unwrap();
    let store = Arc::new(
        PostgresControlStore::open(pg_config(&fixture), pg::digester(), pg::deadline())
            .await
            .unwrap(),
    );
    assert!(
        !sqlx_core::query_scalar::query_scalar::<_, bool>(
            "SELECT rolsuper FROM pg_roles WHERE rolname='runtime_now_owner'"
        )
        .fetch_one(&mut connection)
        .await
        .unwrap()
    );
    let capabilities = [
        "presence:read",
        "presence:publish",
        "task:read",
        "memory:read",
        "knowledge:read",
    ];

    let key = Arc::new(
        Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),"kid":"actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]});
    let jwks = serde_json::from_value(keys.clone()).unwrap();
    let verifier =
        Arc::new(BrokerAccessVerifier::new(&origin, "broker-api", None, &jwks, n(8192)).unwrap());
    let claims = json!({"iss":origin,"sub":"fixture-sub","aud":["broker-api"],"client_id":"graphit-cli","jti":"runtime-now","token_use":"access","scope":"presence-sdk","iat":seconds()-1,"exp":seconds()+3600});
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let raw = Zeroizing::new(format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(message.as_bytes()).as_ref())
    ));
    let metadata = json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[]}});
    let discovery = json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]});
    // Match the qualified normal subscription fixture; expiry has separate cases.
    let runtime = RealtimeServerRuntime::new(Duration::from_secs(60), n(8)).unwrap();
    let routes = broker_realtime_router(store.clone(), verifier.clone(), realtime(), &runtime)
        .unwrap()
        .merge(
            broker_instance_inventory_router(
                store.clone(),
                verifier,
                BrokerPublishHttpConfig {
                    required_scopes: vec!["presence-sdk".into()],
                    grants_json_bytes: n(65536),
                    body_bytes: n(65536),
                    // Normal functional journey, matching the native Broker integration.
                    request_timeout: Duration::from_secs(30),
                },
                BrokerOwnReadHttpConfig {
                    required_scopes: vec!["presence-sdk".into()],
                    grants_json_bytes: n(65536),
                    inventory_json_bytes: n(65536),
                    request_timeout: Duration::from_secs(30),
                },
            )
            .unwrap(),
        )
        .route(
            "/.well-known/graphit-broker",
            axum::routing::get(move || async move { axum::Json(metadata) }),
        )
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(move || async move { axum::Json(discovery) }),
        )
        .route(
            "/keys",
            axum::routing::get(move || async move { axum::Json(keys) }),
        );
    let native_server = Server::start(listener, routes);

    let cancellation = Cancellation::default();
    let original = request(&cancellation, Duration::from_secs(600));
    let first = AuthFixture::with_embedding(
        &origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        AiService::Disabled,
        &original,
    );
    let second = AuthFixture::new(&origin, Credential::Local, None, &ordinary(), &original);
    let observer = AuthFixture::new(
        &origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        &original,
    );
    let first_id = first.project_id();
    let second_id = second.project_id();
    let third_root = first._directory.path().join("added-checkout");
    std::fs::create_dir(&third_root).unwrap();
    let third = ProjectPaths::at(&third_root, &first.paths).unwrap();
    let third_id = ProjectIdentityStore::at(&third)
        .writer(Duration::from_secs(5), &cancellation)
        .unwrap()
        .ensure(&ProjectMetadata::new("added-live").unwrap())
        .unwrap()
        .id();
    acl(
        &mut connection,
        &capabilities,
        &[first_id, second_id, third_id],
    )
    .await;
    for project in [&first.project_paths, &second.project_paths, &third] {
        ConfigStore::project(project)
            .unwrap()
            .set(
                ConfigKey::new("modules.ast").unwrap(),
                json!(false),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    let first = first.release(&original);
    let config = ConfigSnapshot::new(
        first.paths.brand(),
        [ConfigDocument::new(
            first.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [
        ProjectBootstrapInput {
            project: &first.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
        ProjectBootstrapInput {
            project: &second.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
    ];
    let owner = ForegroundSource::start_and_publish(
        &first.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        &original,
    )
    .await
    .unwrap();
    let address = owner.address().unwrap();
    let service = Cancellation::default();
    let operational = service.clone();
    let running = tokio::spawn(async move { owner.run(&operational).await });
    let caller = observer
        .auth
        .issuer()
        .authenticate_runtime(&observer.runtime_key(&original), &original)
        .unwrap();
    let mut selected = observer
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let cache = cache();
    let verifier = cache.get(&mut selected).unwrap();
    let lease = selected.native_credential(&verifier).await.unwrap();
    let transport = websocket(&origin);
    let subscription = |id| NowSubscriptionConfig {
        project_id: id,
        instance_id: Some(first.instance_id()),
        version: None,
        command_bytes: n(65536),
        snapshot_bytes: n(65536),
    };
    let mut a = start_now_subscription(&transport, &lease, &original, subscription(first_id))
        .await
        .unwrap();
    let mut b = start_now_subscription(&transport, &lease, &original, subscription(second_id))
        .await
        .unwrap();
    loop {
        if a.next()
            .await
            .unwrap()
            .instances()
            .iter()
            .any(|i| i.version() == "main")
        {
            break;
        }
    }
    loop {
        if b.next()
            .await
            .unwrap()
            .instances()
            .iter()
            .any(|i| i.version() == "main")
        {
            break;
        }
    }
    let http = reqwest::Client::new();
    let headers =
        graphit_app::ast_source::runtime_headers(&first.paths, native_limits(), &original)
            .await
            .unwrap();
    let client = ()
        .serve(StreamableHttpClientTransport::with_client(
            http.clone(),
            StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
                .custom_headers(headers.clone()),
        ))
        .await
        .unwrap();
    let role_runtime = ()
        .serve(StreamableHttpClientTransport::with_client(
            http.clone(),
            StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/roles/mcp"))
                .custom_headers(headers.clone()),
        ))
        .await
        .unwrap();
    let denied = task_call(
        &role_runtime,
        "graphit_task_query",
        json!({"input":{"selection":{"table":"tasks"},"columns":["id"],"page_size":1,"top_k":1}}),
        true,
    )
    .await;
    assert_eq!(denied["code"], "role_credential_required");
    role_runtime.cancel().await.unwrap();
    for (project, source) in [
        (&first.project_paths, "first live\n"),
        (&second.project_paths, "sibling live\n"),
        (&third, "added live\n"),
    ] {
        std::fs::write(project.root().join("selection.source"), source).unwrap();
    }
    let current = |expected: &'static str| {
        let client = &client;
        async move {
            let answer = client
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_ast_source").with_arguments(
                        json!({"path":"selection.source"})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_ne!(answer.is_error, Some(true), "{answer:?}");
            assert_eq!(
                answer
                    .content
                    .iter()
                    .map(|c| c.as_text().unwrap().text.as_str())
                    .collect::<String>(),
                expected
            );
        }
    };
    current("first live\n").await;
    let added = maintain(
        &http,
        address,
        &headers,
        &ProjectCommand::MaintainCheckout {
            project_id: third_id,
            project_dir: NativePath::new(third.root().to_owned()).unwrap(),
            version: "main".into(),
            detached_base: String::new(),
            select: true,
        },
        &mut connection,
    )
    .await;
    assert_eq!(added["phase"], "complete");
    assert_eq!(added["trusted_root"], true);
    let own = inventory(&http, &origin, first.instance_id(), &raw).await;
    assert_eq!(own["projects"].as_array().unwrap().len(), 3);
    assert!(
        own["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["project_id"] == third_id.to_string() && p["versions"] == json!(["main"]))
    );
    let mut c = start_now_subscription(&transport, &lease, &original, subscription(third_id))
        .await
        .unwrap();
    loop {
        if c.next()
            .await
            .unwrap()
            .instances()
            .iter()
            .any(|i| i.version() == "main")
        {
            break;
        }
    }
    current("added live\n").await;
    let switched = maintain(
        &http,
        address,
        &headers,
        &ProjectCommand::MaintainCheckout {
            project_id: first_id,
            project_dir: NativePath::new(first.project_paths.root().to_owned()).unwrap(),
            version: "feature/live".into(),
            detached_base: String::new(),
            select: true,
        },
        &mut connection,
    )
    .await;
    assert_eq!(switched["phase"], "complete");
    loop {
        let s = a.next().await.unwrap();
        if s.instances().iter().any(|i| i.version() == "feature/live") {
            assert!(s.instances().iter().all(|i| i.version() != "main"));
            break;
        }
    }
    let own = inventory(&http, &origin, first.instance_id(), &raw).await;
    let versions = own["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["project_id"] == first_id.to_string())
        .unwrap()["versions"]
        .as_array()
        .unwrap();
    assert!(
        versions.contains(&json!("main"))
            && versions.contains(&json!("feature/live"))
            && versions.contains(&json!("branch/ação"))
    );
    current("first live\n").await;
    let retired = maintain(
        &http,
        address,
        &headers,
        &ProjectCommand::RetireCheckout {
            project_id: first_id,
            project_dir: NativePath::new(first.project_paths.root().to_owned()).unwrap(),
        },
        &mut connection,
    )
    .await;
    assert_eq!(retired["checkout_removed"], true);
    loop {
        if a.next().await.unwrap().instances().is_empty() {
            break;
        }
    }
    current("sibling live\n").await;
    let removed = client.peer().call_tool(CallToolRequestParams::new("graphit_ast_source").with_arguments(json!({"path":"selection.source","project_dir":first.project_paths.root().to_str().unwrap()}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(
        removed.is_error,
        Some(true),
        "retired checkout must reject new Source requests"
    );
    let own = inventory(&http, &origin, first.instance_id(), &raw).await;
    assert_eq!(
        own["projects"].as_array().unwrap().len(),
        3,
        "Broker retains maintained versions after checkout retirement"
    );
    let local = GlobalProjectInventory::at(&first.paths, 65536)
        .unwrap()
        .load(original.snapshot())
        .unwrap();
    let retired_entry = local.get(first_id).unwrap();
    assert_eq!(retired_entry.checkouts().len(), 0);
    assert!(retired_entry.versions().any(|v| v == "feature/live"));
    assert_eq!(local.get(second_id).unwrap().checkouts().len(), 1);
    assert_eq!(local.get(third_id).unwrap().checkouts().len(), 1);
    assert!(!running.is_finished());
    loop {
        if b.next()
            .await
            .unwrap()
            .instances()
            .iter()
            .any(|i| i.version() == "main")
        {
            break;
        }
    }
    loop {
        if c.next()
            .await
            .unwrap()
            .instances()
            .iter()
            .any(|i| i.version() == "main")
        {
            break;
        }
    }
    client.cancel().await.unwrap();
    service.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    for sub in [&mut b, &mut c] {
        loop {
            if sub.next().await.unwrap().instances().is_empty() {
                break;
            }
        }
    }
    for sub in [a, b, c] {
        sub.shutdown().await.unwrap();
    }
    native_server.shutdown().await;
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}
