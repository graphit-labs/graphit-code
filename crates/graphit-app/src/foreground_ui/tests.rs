use super::*;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn shipped_assets_have_exact_bytes_mime_head_and_finite_routes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router())
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    for (path, bytes, mime) in [
        ("/workspace", INDEX, "text/html; charset=utf-8"),
        ("/task/gantt", INDEX, "text/html; charset=utf-8"),
        ("/task/explorer", INDEX, "text/html; charset=utf-8"),
        ("/task/explorer/task-id", INDEX, "text/html; charset=utf-8"),
        ("/task/sessions", INDEX, "text/html; charset=utf-8"),
        ("/memory/explorer", INDEX, "text/html; charset=utf-8"),
        (
            "/memory/explorer/project",
            INDEX,
            "text/html; charset=utf-8",
        ),
        (
            "/memory/explorer/project/memory-id",
            INDEX,
            "text/html; charset=utf-8",
        ),
        ("/memory/explorer/user", INDEX, "text/html; charset=utf-8"),
        (
            "/memory/explorer/user/memory-id",
            INDEX,
            "text/html; charset=utf-8",
        ),
        ("/live", INDEX, "text/html; charset=utf-8"),
        ("/hub/installed", INDEX, "text/html; charset=utf-8"),
        ("/hub/registry", INDEX, "text/html; charset=utf-8"),
        ("/hub/local", INDEX, "text/html; charset=utf-8"),
        ("/hub/upload", INDEX, "text/html; charset=utf-8"),
        ("/knowledge/explorer", INDEX, "text/html; charset=utf-8"),
        ("/knowledge/contexts", INDEX, "text/html; charset=utf-8"),
        (
            "/knowledge/contexts?library=imported",
            INDEX,
            "text/html; charset=utf-8",
        ),
        ("/ast/contexts", INDEX, "text/html; charset=utf-8"),
        ("/ast/explorer", INDEX, "text/html; charset=utf-8"),
        ("/ast/explorer/project", INDEX, "text/html; charset=utf-8"),
        (
            "/ast/explorer/imported@version",
            INDEX,
            "text/html; charset=utf-8",
        ),
        (
            "/task/sessions/session-id",
            INDEX,
            "text/html; charset=utf-8",
        ),
        ("/logo.svg", LOGO, "image/svg+xml"),
        ("/favicon.svg", FAVICON, "image/svg+xml"),
    ] {
        let response = client.get(format!("{origin}{path}")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(response.bytes().await.unwrap().as_ref(), bytes);
        let response = client.head(format!("{origin}{path}")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
        assert!(response.bytes().await.unwrap().is_empty());
    }
    for path in ["/", "/task", "/task/"] {
        let response = client.get(format!("{origin}{path}")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(response.headers()[header::LOCATION], "/task/gantt");
    }
    for path in [
        "/api/missing",
        "/auth/missing",
        "/mcp/missing",
        "/task/missing.js",
        "/task/rota-inexistente",
        "/task/gantt/extra",
        "/memory/explorer/unknown",
        "/memory/explorer/project/memory-id/extra",
        "/live/session-id",
        "/hub/upload/extra",
        "/hub/registry/extra",
        "/hub/local/extra",
        "/knowledge/explorer/unknown",
        "/knowledge/contexts/unknown",
        "/ast/unknown",
        "/ast/explorer/project/extra",
        "/api/ast/missing",
        "/assets/missing.css",
    ] {
        let response = client.get(format!("{origin}{path}")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
        assert!(
            !response
                .headers()
                .get(header::CONTENT_TYPE)
                .is_some_and(|value| value.to_str().unwrap().contains("text/html"))
        );
    }
    assert_eq!(
        client
            .post(format!("{origin}/task/gantt"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::METHOD_NOT_ALLOWED
    );
    stop.cancel();
    server.await.unwrap();
}
