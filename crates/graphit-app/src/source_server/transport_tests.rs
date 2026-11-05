use super::{serve_http_connection, test_fixture as support};
use crate::daemon_control::{self, Reply, State};
use graphit_auth::Permission;
use graphit_core::{brand::Brand, file_lock::Cancellation};
use graphit_hub::{NativeBinding, policy::RequestContext};
use std::time::{Duration, Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    task::JoinHandle,
};

async fn stopping_connection(
    timeout: Duration,
) -> (support::Fixture, DuplexStream, JoinHandle<()>) {
    let fixture = support::fixture(
        &[Permission::Configure],
        Brand::default(),
        NativeBinding::Codex,
    );
    let stop = fixture.server.0.stop.clone();
    let router = daemon_control::router(
        fixture.auth.clone(),
        fixture.server.0.policy.content,
        timeout,
        stop.clone(),
        None,
    );
    let (server, mut client) = tokio::io::duplex(32);
    let running = tokio::spawn(serve_http_connection(server, router, stop.clone()));
    let body = r#"{"operation":"stop"}"#;
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nX-Graphit-Auth-Origin: runtime\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        daemon_control::ROUTE,
        fixture.raw,
        body.len(),
        body,
    );
    client.write_all(request.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), stop.cancelled())
        .await
        .unwrap();
    (fixture, client, running)
}

async fn cleanup(fixture: support::Fixture) {
    fixture.server.shutdown().await.unwrap();
    let cancellation = Cancellation::default();
    let request = RequestContext::new(
        fixture.server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let removed = fixture.auth.remove_runtime(request.snapshot());
    assert!(removed.failure().is_none());
    assert!(removed.cleanup_failure().is_none());
}

#[tokio::test]
async fn stop_acknowledgement_drains_after_backpressure() {
    let (fixture, mut client, mut running) = stopping_connection(Duration::from_secs(30)).await;
    assert!(
        futures_util::poll!(&mut running).is_pending(),
        "stop must retain the connection while its acknowledgement cannot be written",
    );
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    running.await.unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
    let (_, body) = response.split_once("\r\n\r\n").unwrap();
    let reply: Reply = serde_json::from_str(body).unwrap();
    assert_eq!(reply.state, State::Stopping);
    assert_eq!(reply.pid, std::process::id());
    cleanup(fixture).await;
}

#[tokio::test]
async fn stop_acknowledgement_expires_at_original_deadline() {
    let (fixture, mut client, running) = stopping_connection(Duration::from_secs(1)).await;
    tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .unwrap()
        .unwrap();
    let mut response = Vec::new();
    client.read_to_end(&mut response).await.unwrap();
    assert!(!response.is_empty());
    let response = String::from_utf8(response).unwrap();
    let body = response.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    assert!(serde_json::from_str::<Reply>(body).is_err());
    cleanup(fixture).await;
}
