use super::*;
use graphit_auth::Permission;
use graphit_core::{brand::Brand, module_settings::Module};

fn fixture() -> super::super::test_fixture::Fixture {
    let mut f = super::super::test_fixture::fixture(
        &[
            Permission::Read(Module::Task),
            Permission::Read(Module::Memory),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Ast),
            Permission::CoordinateTask,
            Permission::WriteTask,
            Permission::WriteMemory,
        ],
        Brand::default(),
        graphit_hub::NativeBinding::Codex,
    );
    let config = graphit_core::config::ConfigSnapshot::new(f.paths.brand(), []).unwrap();
    let hub = graphit_hub::policy::HubResourcePolicy::resolve(&config).unwrap();
    let policy =
        crate::foreground_tasks::task_policy(&hub, graphit_hub::NativeBinding::Codex).unwrap();
    f.server.attach_tasks(policy).unwrap();
    f
}
fn app(f: &super::super::test_fixture::Fixture) -> Router {
    Router::new()
        .fallback(|| async { StatusCode::UNAUTHORIZED })
        .layer(axum::middleware::from_fn_with_state(
            LocalUi::new(f.server.clone(), "127.0.0.1:9876".parse().unwrap(), false),
            dispatch,
        ))
}
fn request(path: &str, body: Option<Value>) -> Request {
    let builder = Request::builder()
        .uri(path)
        .header(header::HOST, "127.0.0.1:9876");
    match body {
        Some(body) => builder
            .method(Method::POST)
            .header(header::ORIGIN, "http://127.0.0.1:9876")
            .header("x-graphit-request", "ui")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}
async fn value(response: Response) -> Value {
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}
#[tokio::test]
async fn d0_identity_and_native_records_need_no_broker_or_login() {
    let f = fixture();
    assert!(!f.paths.auth_file().exists());
    let router = app(&f);
    let identity = value(
        router
            .clone()
            .oneshot(request("/auth/session", None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(identity["mode"], "local");
    assert_eq!(identity["anonymous"], true);
    assert!(identity["subject"].is_null());
    assert_eq!(
        value(
            router
                .clone()
                .oneshot(request("/auth/providers", None))
                .await
                .unwrap()
        )
        .await,
        json!({"providers":[]})
    );
    let tasks = f.server.task_server().unwrap();
    let seed=tasks.call(serde_json::from_value(json!({"input":{"idempotency_key":"local-ui-seed","title":"Local session","description":"Real persisted task data","strategy":"Local UI regression"}})).unwrap(),"create_session",true,crate::task_server::request::Credential{role_binding:None,bearer:Zeroizing::new(f.raw.clone()),origin:crate::task_server::request::Origin::Runtime},CancellationToken::new()).await.unwrap_or_else(|_|panic!("native session seed failed"));
    let session_id = seed["result"]["id"].clone();
    assert!(session_id.is_string());
    for (path, body) in [
        ("/api/tasks/contexts", json!({"selection":{}})),
        ("/api/memory/contexts", json!({"input":{}})),
        ("/api/knowledge/contexts", json!({"input":{}})),
        ("/api/ast/contexts", json!({})),
    ] {
        let reply = value(
            router
                .clone()
                .oneshot(request(path, Some(body)))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            reply["contexts"].as_array().unwrap().len(),
            1,
            "{path}: {reply}"
        );
        assert!(!reply.to_string().contains(&f.raw));
    }
    let sessions = value(
        router
            .clone()
            .oneshot(request(
                "/api/tasks/session-heads",
                Some(json!({"selection":{}})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(sessions.to_string().contains("Local session"), "{sessions}");
    assert!(!sessions.to_string().contains(&f.raw));
    let selected = &f.projects[0];
    let selection = json!({"project_dir":selected.0.root(),"project_id":selected.1,"project_version":selected.2.version()});
    let snapshot = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let checkout = graphit_core::git_checkout::GitCheckout::inspect(
        selected.0.root(),
        Duration::from_secs(5),
        snapshot.cancellation(),
    )
    .unwrap();
    let observation = graphit_core::project_inventory::ProjectInventoryObservation::observe(
        &selected.0,
        selected.1,
        &checkout,
        &selected.2,
        &snapshot,
    )
    .unwrap();
    graphit_core::project_inventory::GlobalProjectInventory::at(&f.paths, 65536)
        .unwrap()
        .writer(&snapshot)
        .unwrap()
        .register_checkout(&observation)
        .unwrap();
    let inventory = value(
        router
            .clone()
            .oneshot(request("/api/projects/local/list", Some(json!({}))))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        inventory["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| { project["project_id"] == selection["project_id"] })
    );
    let activity = value(
        router
            .clone()
            .oneshot(request(
                "/api/now/web/snapshot",
                Some(json!({"selection":selection})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(activity["selection"], selection);
    assert_eq!(activity["snapshot"]["project_id"], selection["project_id"]);
    assert!(activity["snapshot"]["sections"]["sessions"].is_object());
    let partial = router
        .clone()
        .oneshot(request(
            "/api/now/web/snapshot",
            Some(json!({"selection":{"project_id":selection["project_id"]}})),
        ))
        .await
        .unwrap();
    assert_eq!(partial.status(), StatusCode::BAD_REQUEST);
    let links = value(router.clone().oneshot(request("/api/records/references", Some(json!({
        "selection":selection,"input":{"focus":{"kind":"session","scope":{"project":selection["project_id"]},"id":session_id},
        "outgoing_source":{"kind":"project_task"},"page_size":20,"include_user":false,"contexts":[],
        "pages":[{"source":{"kind":"project_task"},"direction":"outgoing"}]}
    })))).await.unwrap()).await;
    assert_eq!(links["sources"][0]["source"]["kind"], "project_task");
    let dream = value(
        router
            .clone()
            .oneshot(request(
                "/api/dream/local/status",
                Some(json!({"selection":selection})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(dream["enabled"], false);
    assert_eq!(dream["worker_attached"], false);
    let created=value(router.clone().oneshot(request("/api/memory/create",Some(json!({"selection":selection,"input":{"idempotency_key":"local-ui-memory","title":"Local memory","body":"Native persisted body","attributes":{"classification":"fact","mandatory":false,"important":false},"tags":[],"references":[]}})))).await.unwrap()).await;
    assert_eq!(created["native"]["state"], "completed", "{created}");
    let memories = value(
        router
            .clone()
            .oneshot(request(
                "/api/memory/list",
                Some(json!({"selection":selection,"input":{}})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(memories.to_string().contains("Native persisted body"));
    let knowledge = value(
        router
            .clone()
            .oneshot(request(
                "/api/knowledge/heads",
                Some(json!({"selection":selection,"input":{}})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(knowledge["native"]["state"], "no_corpus");
    let source = value(
        router
            .clone()
            .oneshot(request(
                "/api/ast/source",
                Some(json!({"selection":selection,"source":{"path":"raw"}})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(source.to_string().contains("actual native payload"));
    assert!(!source.to_string().contains(&f.raw));
    f.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn local_ui_rejects_foreign_origins_hosts_and_simple_posts() {
    let f = fixture();
    let router = app(&f);
    let mut requests = Vec::new();
    let mut foreign = request("/api/tasks/contexts", Some(json!({"selection":{}})));
    foreign.headers_mut().insert(
        header::ORIGIN,
        header::HeaderValue::from_static("https://foreign.example"),
    );
    requests.push(foreign);
    let mut foreign = request("/auth/session", None);
    foreign.headers_mut().insert(
        header::HOST,
        header::HeaderValue::from_static("foreign.example:9876"),
    );
    requests.push(foreign);
    let mut simple = request("/api/tasks/contexts", Some(json!({"selection":{}})));
    simple.headers_mut().remove("x-graphit-request");
    requests.push(simple);
    let mut no_origin = request("/api/tasks/contexts", Some(json!({"selection":{}})));
    no_origin.headers_mut().remove(header::ORIGIN);
    requests.push(no_origin);
    let mut cross_site = request("/auth/session", None);
    cross_site.headers_mut().insert(
        "sec-fetch-site",
        header::HeaderValue::from_static("cross-site"),
    );
    requests.push(cross_site);
    for request in requests {
        assert_eq!(
            router.clone().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        router
            .clone()
            .oneshot(request("/api/tasks/native/catalogue", Some(json!({}))))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    f.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn explicit_broker_configuration_keeps_the_broker_boundary() {
    let f = fixture();
    graphit_core::config::ConfigStore::global(&f.paths).unwrap().set(graphit_core::config::ConfigKey::new("task.browser").unwrap(),json!({"providers":["company"],"project_endpoint":"https://broker.example/v1/project/access/authorize"}),Duration::from_secs(5),&Cancellation::default()).unwrap();
    let router = app(&f);
    assert_eq!(
        router
            .oneshot(request(
                "/api/tasks/contexts",
                Some(json!({"selection":{}}))
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    f.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn local_deadline_and_stop_preserve_their_failure_categories() {
    let mut expired = fixture();
    Arc::get_mut(&mut expired.server.0).unwrap().policy.timeout = Duration::from_nanos(1);
    let response = app(&expired)
        .oneshot(request(
            "/api/tasks/contexts",
            Some(json!({"selection":{}})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    expired.server.shutdown().await.unwrap();
    let stopped = fixture();
    stopped.stop.cancel();
    let response = app(&stopped)
        .oneshot(request(
            "/api/tasks/contexts",
            Some(json!({"selection":{}})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    stopped.server.shutdown().await.unwrap();
}
