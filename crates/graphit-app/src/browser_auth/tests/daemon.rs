use super::*;

#[tokio::test]
async fn cookie_daemon_status_reads_source_state_without_runtime_credentials() {
    let (mut source, graphs, _destination) = crate::graph_server::tests::web_fixture().await;
    crate::source_server::replace_fixture_graph(&mut source, graphs);
    let fixture = Fixture::with_graphs(policy(), (source.server.clone(), source.raw.clone())).await;
    let url = format!("{}/api/daemon/status", fixture.origin);
    assert_eq!(
        fixture.client.get(&url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = fixture.login().await;
    let response = fixture
        .client
        .get(&url)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"pid": std::process::id(), "state": "running"})
    );
    assert_eq!(
        fixture
            .client
            .get(&url)
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, "https://foreign.example")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .client
            .post(&url)
            .header(header::COOKIE, &cookie)
            .json(&json!({"operation": "stop"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        source.server.daemon_status().state,
        crate::daemon_control::State::Running
    );
    source.server.shutdown().await.unwrap();
    let response = fixture
        .client
        .get(&url)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"pid": std::process::id(), "state": "stopping"})
    );
    fixture.service.shutdown().await.unwrap();
}
