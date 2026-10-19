use super::*;
use graphit_core::brand::Brand;
use graphit_hub::NativeBinding;

#[tokio::test]
async fn open_ui_uses_runtime_authority_and_a_bound_browser_capability() {
    let fixture = crate::source_server::test_fixture::fixture(
        &[Permission::Configure],
        Brand::default(),
        NativeBinding::Codex,
    );
    let content = ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}/task/gantt");
    let stop = CancellationToken::new();
    let transport = CancellationToken::new();
    let app = router(
        Arc::clone(&fixture.auth),
        content,
        Duration::from_secs(5),
        stop.clone(),
        Some(url.clone()),
    )
    .merge(crate::foreground_ui::router());
    let signal = transport.clone();
    let served = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(signal.cancelled_owned())
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let headers = [
        ("x-graphit-auth-origin", "runtime".to_owned()),
        ("authorization", format!("Bearer {}", fixture.raw)),
    ];
    let response = client
        .post(format!("http://{address}{ROUTE}"))
        .headers(
            headers
                .iter()
                .map(|(k, v)| {
                    (
                        header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                        header::HeaderValue::from_str(v).unwrap(),
                    )
                })
                .collect(),
        )
        .json(&serde_json::json!({"operation":"open_ui"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.bytes().await.unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(&fixture.raw));
    let reply: Reply = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reply.ui_url.as_deref(), Some(url.as_str()));
    assert_eq!(reply.state, State::Running);
    assert!(client.get(&url).send().await.unwrap().status().is_success());
    for (origin, bearer, expected) in [
        (None, "incorrect", StatusCode::UNAUTHORIZED),
        (
            Some("http://untrusted.example"),
            fixture.raw.as_str(),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let mut request = client
            .post(format!("http://{address}{ROUTE}"))
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(bearer)
            .json(&serde_json::json!({"operation":"open_ui"}));
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        assert_eq!(request.send().await.unwrap().status(), expected);
    }
    stop.cancel();
    assert_eq!(
        client
            .post(format!("http://{address}{ROUTE}"))
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(&fixture.raw)
            .json(&serde_json::json!({"operation":"open_ui"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    transport.cancel();
    served.await.unwrap();
    fixture.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn open_ui_without_browser_or_configure_is_explicit_and_keeps_control_live() {
    for configure in [true, false] {
        let fixture = crate::source_server::test_fixture::fixture(
            if configure {
                &[Permission::Configure]
            } else {
                &[]
            },
            Brand::default(),
            NativeBinding::Codex,
        );
        let content = ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let transport = CancellationToken::new();
        let signal = transport.clone();
        let app = router(
            Arc::clone(&fixture.auth),
            content,
            Duration::from_secs(5),
            CancellationToken::new(),
            None,
        );
        let served = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(signal.cancelled_owned())
                .await
                .unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let request = || {
            client
                .post(format!("http://{address}{ROUTE}"))
                .header("x-graphit-auth-origin", "runtime")
                .bearer_auth(&fixture.raw)
        };
        let reply = request()
            .json(&serde_json::json!({"operation":"open_ui"}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            reply.status(),
            if configure {
                StatusCode::CONFLICT
            } else {
                StatusCode::FORBIDDEN
            }
        );
        if configure {
            assert_eq!(
                reply.json::<serde_json::Value>().await.unwrap()["error"],
                "browser_not_configured"
            );
            let status = request()
                .json(&serde_json::json!({"operation":"status"}))
                .send()
                .await
                .unwrap();
            assert_eq!(status.status(), StatusCode::OK);
            assert_eq!(status.json::<Reply>().await.unwrap().state, State::Running);
        }
        transport.cancel();
        served.await.unwrap();
        fixture.server.shutdown().await.unwrap();
    }
}

#[test]
fn browser_entry_urls_allow_only_the_validated_browser_surface() {
    for raw in [
        "http://127.0.0.1:1234/task/gantt",
        "http://[::1]:1234/task/gantt",
        "https://graphit.example/task/gantt",
    ] {
        assert!(validate_ui_url(raw).is_ok());
    }
    for raw in [
        "http://0.0.0.0:1234/task/gantt",
        "http://remote.example/task/gantt",
        "https://user:secret@graphit.example/task/gantt",
        "https://graphit.example/task/gantt?token=secret",
        "https://graphit.example/task/gantt#secret",
        "file:///task/gantt",
        "https://graphit.example/admin/daemon",
    ] {
        assert!(validate_ui_url(raw).is_err(), "{raw}");
    }
}

#[tokio::test]
async fn process_guard_does_not_mark_stop_until_an_authenticated_target_is_accepted() {
    let fixture = crate::source_server::test_fixture::fixture(
        &[Permission::Configure],
        Brand::default(),
        NativeBinding::Codex,
    );
    let content = ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    };
    let stop = CancellationToken::new();
    let transport = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(
        Arc::clone(&fixture.auth),
        content,
        Duration::from_secs(5),
        stop.clone(),
        None,
    );
    let signal = transport.clone();
    let served = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(signal.cancelled_owned())
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let pid = std::process::id();
    let wrong = if pid == 1 { 2 } else { 1 };
    for (credential, target, expected) in [
        ("incorrect", pid, StatusCode::UNAUTHORIZED),
        (fixture.raw.as_str(), wrong, StatusCode::CONFLICT),
        (fixture.raw.as_str(), 0, StatusCode::BAD_REQUEST),
    ] {
        let response = client
            .post(format!("http://{address}{ROUTE}"))
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(credential)
            .json(&serde_json::json!({"operation":"stop", "expected_pid":target}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert!(!stop.is_cancelled());
    }
    let response = client
        .post(format!("http://{address}{ROUTE}"))
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(&fixture.raw)
        .json(&serde_json::json!({"operation":"stop", "expected_pid":pid}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(stop.is_cancelled());
    assert_eq!(response.json::<Reply>().await.unwrap().pid, pid);
    transport.cancel();
    served.await.unwrap();
    fixture.server.shutdown().await.unwrap();
}
