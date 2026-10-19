use super::*;
use tokio_util::sync::CancellationToken;

#[test]
fn shipped_assets_compile_without_dist_or_node_in_an_isolated_source_tree() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("src");
    let assets = fixture.path().join("assets/ui");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&assets).unwrap();
    for (name, bytes) in [
        ("index.html", INDEX),
        ("logo.svg", LOGO),
        ("favicon.svg", FAVICON),
    ] {
        std::fs::write(assets.join(name), bytes).unwrap();
    }
    // Compile the production include declarations, so a dependency on dist would fail.
    let declarations = include_str!("../foreground_ui.rs")
        .lines()
        .filter(|line| line.starts_with("const "))
        .collect::<Vec<_>>()
        .join("\n");
    let input = source.join("foreground_ui.rs");
    std::fs::write(
        &input,
        format!("{declarations}\nfn main() {{ std::hint::black_box((INDEX, LOGO, FAVICON)); }}"),
    )
    .unwrap();
    let sysroot = std::process::Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .unwrap();
    assert!(sysroot.status.success());
    let compiler = std::path::PathBuf::from(std::str::from_utf8(&sysroot.stdout).unwrap().trim())
        .join("bin")
        .join(if cfg!(windows) { "rustc.exe" } else { "rustc" });
    let result = std::process::Command::new(compiler)
        .env("PATH", "")
        .args(["--edition=2024", "--emit=metadata", "-Dwarnings"])
        .arg(&input)
        .arg("-o")
        .arg(fixture.path().join("assets.rmeta"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(fixture.path().join("assets.rmeta").is_file());
    assert!(!fixture.path().join("ui/dist").exists());
}

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
