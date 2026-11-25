//! Real ForegroundSource trusted-provider policy consumed over signed Web HTTP.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_factory_reads_own_user_without_project_grant_or_userinfo_and_without_birth() {
    let broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    broker.issuer.allowed.store(false, Ordering::SeqCst);
    let owner = fixture.start(Some(config(&broker.issuer))).unwrap();
    let origin = format!("http://{}", owner.address().unwrap());
    let cancellation = Cancellation::default();
    let stop = cancellation.clone();
    let running = tokio::spawn(async move { owner.run(&stop).await });
    let client = client();
    let cookie = login(&client, &origin, &broker.issuer).await;
    let users = fixture.paths.root().join("users");
    assert!(!users.exists());
    let response = client
        .post(format!("{origin}/api/user-memory/list"))
        .header(header::ORIGIN, &origin)
        .header(header::COOKIE, &cookie)
        .json(&json!({"input":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["native"], json!({"state":"no_corpus"}));
    assert_eq!(
        body["currentOwner"]["identity"]["issuer"],
        broker.issuer.origin
    );
    assert_eq!(body["currentOwner"]["identity"]["subject"], "web-subject");
    let identity: Value = serde_json::from_str(body["scope_id"].as_str().unwrap()).unwrap();
    assert_eq!(identity[0], body["currentOwner"]["installation_id"]);
    assert_eq!(identity[1], broker.issuer.origin);
    assert_eq!(identity[2], "web-subject");
    assert!(
        !users.exists(),
        "readonly absent USER must not create storage"
    );
    assert_eq!(broker.issuer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(broker.issuer.userinfo.load(Ordering::SeqCst), 0);
    cancellation.cancel();
    running.await.unwrap().unwrap();
    fixture.assert_clean();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_and_project_configuration_do_not_enable_user_browser_reads() {
    let broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    for source in [None, Some(ConfigSource::Project)] {
        let documents = source.map(|source| {
            ConfigDocument::new(
                fixture.paths.brand(),
                source,
                [(
                    ConfigKey::new("task.browser").unwrap(),
                    config(&broker.issuer),
                )],
            )
            .unwrap()
        });
        let snapshot = ConfigSnapshot::new(fixture.paths.brand(), documents).unwrap();
        let policy = HubResourcePolicy::resolve(&snapshot).unwrap();
        let cancellation = Cancellation::default();
        let original = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let owner = ForegroundSource::start(
            &fixture.paths,
            &[ProjectBootstrapInput {
                project: &fixture.project,
                explicit_version: "",
                pin: None,
                detached_base: "",
            }],
            0,
            &snapshot,
            NativeBinding::Codex,
            &original,
        )
        .unwrap();
        let origin = format!("http://{}", owner.address().unwrap());
        let stop = cancellation.clone();
        let running = tokio::spawn(async move { owner.run(&stop).await });
        let response = client()
            .post(format!("{origin}/api/user-memory/list"))
            .header(header::ORIGIN, &origin)
            .json(&json!({"input":{}}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(!fixture.paths.root().join("users").exists());
        cancellation.cancel();
        running.await.unwrap().unwrap();
        fixture.assert_clean();
    }
    assert_eq!(broker.issuer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(broker.issuer.userinfo.load(Ordering::SeqCst), 0);
}
