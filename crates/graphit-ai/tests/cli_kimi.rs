use graphit_ai::cli_kimi::{KimiEndpoint, KimiReason, spawn_kimi_server};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_http::client::{
    BoundedClient, ClientPolicy, Limits as NetworkLimits, NetworkRequest, NetworkRequestContext,
};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, Limits, ProcessHost, Request,
};
use http::{HeaderMap, HeaderValue, Method, header};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
#[path = "support/kimi_peer.rs"]
mod peer;

#[test]
#[ignore = "native subprocess peer; explicitly invoked by the focused suites"]
fn native_kimi_peer() {
    peer::run();
}

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    ))
}
fn network() -> NetworkLimits {
    NetworkLimits {
        request_body_bytes: 4096,
        response_body_bytes: 4096,
        url_bytes: 4096,
        header_bytes: 4096,
        header_count: 16,
        max_in_flight: 1,
        max_idle_per_host: 1,
        idle_timeout: Duration::from_secs(1),
        max_certificates: 1,
        max_certificate_bytes: 4096,
    }
}
fn request(dir: &std::path::Path, mode: &str) -> Request {
    let mut environment = vec![
        ("KIMI_PEER".into(), "1".into()),
        ("KIMI_MODE".into(), mode.into()),
        ("KIMI_PEER_PORT".into(), dir.join("port").into_os_string()),
    ];
    environment.extend(
        std::env::vars_os().filter(|(key, _)| key == "LD_LIBRARY_PATH" || key == "SYSTEMROOT"),
    );
    Request {
        program: std::env::current_exe().unwrap(),
        args: [
            "--ignored",
            "--exact",
            "native_kimi_peer",
            "--nocapture",
            "--format",
            "terse",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
        cwd: dir.to_owned(),
        environment,
        stdin: None,
        limits: Limits {
            stdin_bytes: 4096,
            stdout_bytes: 32768,
            stderr_bytes: 32768,
            termination_grace: Duration::from_millis(20),
            cleanup: Duration::from_secs(2),
        },
    }
}
async fn host() -> ProcessHost {
    let host =
        ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1).unwrap()).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    host
}
async fn finish(host: ProcessHost) {
    match host.shutdown(Instant::now() + Duration::from_secs(5)).await {
        HostShutdown::Joined(report) => assert!(report.failure().is_none()),
        HostShutdown::Pending(_) => panic!("native Host did not join"),
    }
}
#[test]
fn readiness_rejects_remote_userinfo_query_missing_or_extra_credentials() {
    for value in [
        "http://192.0.2.1:1234/#token=fixture",
        "http://2130706433:1234/#token=fixture",
        "http://0x7f000001:1234/#token=fixture",
        "http://127.0.0.1:1234/a/../#token=fixture",
        "http://127.0.0.1:1234/%2e/#token=fixture",
        "https://127.0.0.1:1234/#token=fixture",
        "http://user@127.0.0.1:1234/#token=fixture",
        "http://127.0.0.1:1234/?query=1#token=fixture",
        "http://127.0.0.1:1234/",
        "http://127.0.0.1:1234/#token=fixture&other=credential",
    ] {
        let line = format!("Kimi server: {value}\n");
        assert!(matches!(
            KimiEndpoint::from_ready_line(line.as_bytes(), 4096),
            Err(KimiReason::Endpoint)
        ));
    }
    assert!(
        KimiEndpoint::from_ready_line(
            b"Kimi server: http://127.0.0.1:1234/#token=fixture-token\n",
            4096
        )
        .is_ok()
    );
}
#[tokio::test]
async fn official_readiness_then_native_shutdown_observes_status_eof_and_join() {
    let dir = tempfile::tempdir().unwrap();
    let original = original();
    let host = host().await;
    let owner = spawn_kimi_server(&original, &host, request(dir.path(), "answer"), 4096).await;
    assert!(owner.ready(&original).await.is_ok());
    let port = std::fs::read_to_string(dir.path().join("port")).unwrap();
    let origin = format!("http://127.0.0.1:{port}");
    let client = BoundedClient::new(
        ClientPolicy {
            origin: &origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer fixture-kimi-token"),
    );
    let url = format!("{origin}/api/v1/shutdown");
    let response = client
        .execute(
            NetworkRequest {
                method: Method::POST,
                url: &url,
                headers: &headers,
                body: b"{}",
            },
            NetworkRequestContext::new(
                original.required_deadline().unwrap(),
                original.cancellation(),
            ),
        )
        .await
        .unwrap();
    assert!(response.status().is_success());
    let observation = owner.observe(Instant::now() + Duration::from_secs(5)).await;
    assert!(
        observation.joined && observation.clean && observation.stdout_eof && observation.stderr_eof
    );
    assert_eq!(observation.status_code, Some(0));
    drop(owner);
    finish(host).await;
}
#[tokio::test]
async fn startup_eof_and_invalid_endpoint_do_not_become_readiness_and_original_cancel_joins() {
    for mode in ["startup_eof", "endpoint", "answer"] {
        let dir = tempfile::tempdir().unwrap();
        let original = original();
        let host = host().await;
        let owner = spawn_kimi_server(&original, &host, request(dir.path(), mode), 4096).await;
        let ready = owner.ready(&original).await;
        match mode {
            "startup_eof" => assert!(matches!(ready, Err(KimiReason::Startup))),
            "endpoint" => assert!(matches!(ready, Err(KimiReason::Endpoint))),
            _ => {
                assert!(ready.is_ok());
                original.cancellation().cancel();
            }
        }
        if mode != "startup_eof" {
            owner.stop();
        }
        let observation = owner.observe(Instant::now() + Duration::from_secs(5)).await;
        assert!(observation.joined);
        assert_eq!(observation.cleanup, Some(Cleanup::Complete));
        if mode != "startup_eof" {
            assert!(!observation.clean);
        }
        drop(owner);
        finish(host).await;
    }
}

#[tokio::test]
async fn readiness_limit_and_failed_spawn_keep_real_process_observations() {
    let dir = tempfile::tempdir().unwrap();
    let original = original();
    let host = host().await;
    let owner = spawn_kimi_server(&original, &host, request(dir.path(), "answer"), 16).await;
    assert!(matches!(
        owner.ready(&original).await,
        Err(KimiReason::Limit)
    ));
    let observation = owner.observe(Instant::now() + Duration::from_secs(5)).await;
    assert!(observation.joined);
    assert!(!observation.clean);
    assert!(observation.process_reason.is_some());
    drop(owner);
    let mut missing = request(dir.path(), "answer");
    missing.program = dir.path().join("absent-native-kimi");
    let owner = spawn_kimi_server(&original, &host, missing, 4096).await;
    assert!(matches!(
        owner.ready(&original).await,
        Err(KimiReason::Startup)
    ));
    let observation = owner.observe(Instant::now() + Duration::from_secs(5)).await;
    assert!(observation.joined);
    assert!(!observation.clean);
    assert_eq!(observation.cleanup, Some(Cleanup::NotSpawned));
    assert_eq!(observation.status_code, None);
    assert!(observation.process_reason.is_some());
    drop(owner);
    finish(host).await;
}
#[tokio::test]
async fn original_deadline_and_cancelled_observation_keep_one_owned_native_child() {
    let dir = tempfile::tempdir().unwrap();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(2),
        Cancellation::default(),
    ));
    let host = host().await;
    let owner = spawn_kimi_server(&original, &host, request(dir.path(), "answer"), 4096).await;
    assert!(owner.ready(&original).await.is_ok());
    {
        let held = owner.observe(Instant::now() + Duration::from_secs(5));
        tokio::pin!(held);
        let first = std::future::poll_fn(|cx| std::task::Poll::Ready(held.as_mut().poll(cx))).await;
        assert!(first.is_pending());
        let second = tokio::time::timeout(
            Duration::from_millis(500),
            owner.observe(Instant::now() + Duration::from_millis(20)),
        )
        .await
        .expect("observer deadline must include mutex acquisition");
        assert!(!second.joined);
        assert_eq!(second.status_code, None);
    }
    // A timed-out observer did not consume the handle. The same Original deadline stops the child.
    let observed = owner.observe(Instant::now() + Duration::from_secs(5)).await;
    assert!(observed.joined);
    assert!(!observed.clean);
    assert_eq!(observed.cleanup, Some(Cleanup::Complete));
    assert!(observed.process_reason.is_some());
    drop(owner);
    finish(host).await;
}

fn completion_limits() -> graphit_ai::cli_kimi::KimiLimits {
    graphit_ai::cli_kimi::KimiLimits {
        wire: graphit_ai::cli_codex_wire::CodexWireLimits {
            frame_bytes: 8192,
            depth: 16,
            nodes: 2048,
            fields: 2048,
            array_items: 2048,
            string_bytes: 4096,
            decoded_string_bytes: 8192,
            id_bytes: 128,
        },
        network: network(),
        websocket: graphit_http::websocket::WebSocketLimits {
            frame_bytes: std::num::NonZeroUsize::new(8192).unwrap(),
            message_bytes: std::num::NonZeroUsize::new(8192).unwrap(),
            send_bytes: std::num::NonZeroUsize::new(8192).unwrap(),
        },
        prompt_text_bytes: 4096,
        response_text_bytes: 4096,
    }
}
struct NeutralSink {
    original: Arc<SnapshotContext>,
    denied: Arc<std::sync::atomic::AtomicBool>,
    deny_after_progress: bool,
    events: usize,
}
impl graphit_ai::cli_kimi::KimiSink for NeutralSink {
    async fn push<'a>(
        &'a mut self,
        _: &'a serde_json::value::RawValue,
        original: &'a SnapshotContext,
    ) -> std::io::Result<()> {
        assert!(std::ptr::eq(original, self.original.as_ref()));
        original.check()?;
        self.events += 1;
        if self.deny_after_progress {
            self.denied.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }
}
#[tokio::test]
async fn neutral_recheck_runs_real_protocol_without_knowledge_admission() {
    for requested_model in [None, Some(String::from("explicit/fixture-model"))] {
        let dir = tempfile::tempdir().unwrap();
        let original = original();
        let host = host().await;
        let journal = dir.path().join("journal.jsonl");
        let mut native = request(dir.path(), "answer");
        native
            .environment
            .push(("KIMI_JOURNAL".into(), journal.as_os_str().to_owned()));
        let owner = spawn_kimi_server(&original, &host, native, 4096).await;
        let endpoint = owner.ready(&original).await.unwrap();
        let denied = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let before = || {
            checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        };
        let mut sink = NeutralSink {
            original: Arc::clone(&original),
            denied,
            deny_after_progress: false,
            events: 0,
        };
        let receipt = graphit_ai::cli_kimi::run_kimi_guarded(
            &original,
            &before,
            &endpoint,
            graphit_ai::cli_kimi::KimiRequest {
                session: graphit_ai::cli_kimi::KimiSessionStart::Create,
                cwd: dir.path().to_owned(),
                model: requested_model.clone(),
                prompt: "literal controlled question".into(),
            },
            completion_limits(),
            &mut sink,
        )
        .await
        .unwrap();
        assert!(receipt.eligible_text().is_some());
        let entries: Vec<serde_json::Value> = std::fs::read_to_string(&journal)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let submitted = &entries
            .iter()
            .find(|entry| entry["stage"] == "prompt")
            .unwrap()["body"];
        match requested_model.as_deref() {
            Some(model) => assert_eq!(submitted["model"].as_str(), Some(model)),
            None => assert!(!submitted.as_object().unwrap().contains_key("model")),
        }
        assert!(sink.events > 0);
        assert!(checks.load(std::sync::atomic::Ordering::SeqCst) > 0);
        let observation = owner.observe(Instant::now() + Duration::from_secs(5)).await;
        assert!(
            observation.joined
                && observation.clean
                && observation.stdout_eof
                && observation.stderr_eof
        );
        assert_eq!(observation.status_code, Some(0));
        drop(owner);
        finish(host).await;
    }
}

#[tokio::test]
async fn neutral_recheck_deny_before_io_and_after_progress_preserves_process_owner() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for deny_before in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let original = original();
        let host = host().await;
        let journal = dir.path().join("journal.jsonl");
        let mut native = request(dir.path(), "answer");
        native
            .environment
            .push(("KIMI_JOURNAL".into(), journal.as_os_str().to_owned()));
        let owner = spawn_kimi_server(&original, &host, native, 4096).await;
        let endpoint = owner.ready(&original).await.unwrap();
        let denied = Arc::new(AtomicBool::new(deny_before));
        let before = || {
            if denied.load(Ordering::SeqCst) {
                Err(std::io::ErrorKind::PermissionDenied.into())
            } else {
                Ok(())
            }
        };
        let mut sink = NeutralSink {
            original: Arc::clone(&original),
            denied: Arc::clone(&denied),
            deny_after_progress: !deny_before,
            events: 0,
        };
        let failure = graphit_ai::cli_kimi::run_kimi_guarded(
            &original,
            &before,
            &endpoint,
            graphit_ai::cli_kimi::KimiRequest {
                session: graphit_ai::cli_kimi::KimiSessionStart::Create,
                cwd: dir.path().to_owned(),
                model: Some("explicit/fixture-model".into()),
                prompt: "literal controlled question".into(),
            },
            completion_limits(),
            &mut sink,
        )
        .await
        .unwrap_err();
        assert_eq!(failure.reason(), KimiReason::Admission);
        assert!(failure.receipt().eligible_text().is_none());
        assert_eq!(
            failure
                .native_cause()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let entries: Vec<serde_json::Value> = std::fs::read_to_string(&journal)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(entries.iter().all(|entry| entry["stage"] != "shutdown"));
        if deny_before {
            assert!(entries.iter().all(|entry| entry["stage"] != "meta"));
            assert_eq!(sink.events, 0);
        } else {
            assert!(entries.iter().any(|entry| entry["stage"] == "prompt"));
            assert_eq!(sink.events, 1);
        }
        owner.stop();
        let observation = owner.observe(Instant::now() + Duration::from_secs(5)).await;
        assert!(observation.joined);
        assert_eq!(observation.cleanup, Some(Cleanup::Complete));
        drop(owner);
        finish(host).await;
    }
}

#[tokio::test]
async fn explicitly_empty_kimi_model_is_invalid_before_rest_io() {
    let dir = tempfile::tempdir().unwrap();
    let original = original();
    let host = host().await;
    let journal = dir.path().join("journal.jsonl");
    let mut native = request(dir.path(), "answer");
    native
        .environment
        .push(("KIMI_JOURNAL".into(), journal.as_os_str().to_owned()));
    let owner = spawn_kimi_server(&original, &host, native, 4096).await;
    let endpoint = owner.ready(&original).await.unwrap();
    let mut sink = NeutralSink {
        original: Arc::clone(&original),
        denied: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        deny_after_progress: false,
        events: 0,
    };
    let failure = graphit_ai::cli_kimi::run_kimi_guarded(
        &original,
        &|| Ok(()),
        &endpoint,
        graphit_ai::cli_kimi::KimiRequest {
            session: graphit_ai::cli_kimi::KimiSessionStart::Create,
            cwd: dir.path().to_owned(),
            model: Some(String::new()),
            prompt: "literal controlled question".into(),
        },
        completion_limits(),
        &mut sink,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.reason(), KimiReason::Limit);
    assert!(failure.receipt().eligible_text().is_none());
    let entries: Vec<serde_json::Value> = std::fs::read_to_string(&journal)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(entries.iter().all(|entry| entry["stage"] != "meta"));
    assert_eq!(sink.events, 0);
    owner.stop();
    let observed = owner.observe(Instant::now() + Duration::from_secs(5)).await;
    assert!(observed.joined);
    assert_eq!(observed.cleanup, Some(Cleanup::Complete));
    drop(owner);
    finish(host).await;
}
