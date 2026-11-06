use super::*;
use graphit_core::brand::Brand;
use graphit_hub::NativeBinding;
use std::{fs, time::Duration};

#[tokio::test]
async fn cancelled_shutdown_waiter_keeps_jobs_and_reports_terminal_failure_once() {
    for panic in [false, true] {
        let fixture = crate::source_server::test_fixture::fixture(
            &[],
            Brand::default(),
            NativeBinding::Codex,
        );
        let service = ManagedRolesHttp::new(
            Arc::clone(&fixture.auth),
            Arc::clone(&fixture.projects),
            0,
            fixture.server.policy().clone(),
            CancellationToken::new(),
        );
        let release = Arc::new(tokio::sync::Notify::new());
        let released = Arc::clone(&release);
        service.0.jobs.lock().await.spawn(async move {
            released.notified().await;
            assert!(!panic, "controlled managed role job panic");
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "controlled managed role job failure",
            ))
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(20), service.shutdown())
                .await
                .is_err()
        );
        assert!(service.0.stop.is_cancelled());
        assert_eq!(service.0.jobs.lock().await.len(), 1);
        release.notify_one();
        let failure = service.shutdown().await.unwrap_err();
        let original = failure
            .get_ref()
            .unwrap()
            .downcast_ref::<Arc<io::Error>>()
            .unwrap();
        if panic {
            let join = original
                .get_ref()
                .unwrap()
                .downcast_ref::<tokio::task::JoinError>()
                .unwrap();
            assert!(join.is_panic());
        } else {
            assert_eq!(original.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(original.to_string(), "controlled managed role job failure");
        }
        assert!(service.0.jobs.lock().await.is_empty());
        assert!(service.0.retained_runs.lock().await.is_empty());
        assert!(service.0.first_failure.lock().unwrap().is_none());
        service.shutdown().await.unwrap();
    }
}

async fn stopped_run_before_capture(mode: &str) {
    let fixture =
        crate::source_server::test_fixture::fixture(&[], Brand::default(), NativeBinding::Codex);
    let service = ManagedRolesHttp::new(
        Arc::clone(&fixture.auth),
        Arc::clone(&fixture.projects),
        0,
        fixture.server.policy().clone(),
        CancellationToken::new(),
    );
    let content = fixture.server.policy().content;
    let cancel = Cancellation::default();
    if mode == "cancelled" {
        cancel.cancel();
    }
    let deadline = if mode == "expired" {
        Instant::now() - Duration::from_secs(1)
    } else {
        Instant::now() + Duration::from_secs(10)
    };
    let worker_cancel = cancel.clone();
    let mode = mode.to_owned();
    let work = tokio::task::spawn_blocking(move || {
        let result = (|| {
            let expected = if mode == "expired" {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::Interrupted
            };
            let original =
                RequestContext::new(content, &worker_cancel, deadline).map_err(|cause| {
                    assert!(
                        matches!(
                            (&cause, expected),
                            (
                                RequestError::Content(ContentError::Cancelled),
                                io::ErrorKind::Interrupted
                            ) | (
                                RequestError::Content(ContentError::TimedOut),
                                io::ErrorKind::TimedOut
                            )
                        ),
                        "unexpected Original rejection: {cause:?}"
                    );
                    ContextError::Request(cause)
                })?;
            if mode == "capture" {
                worker_cancel.cancel();
            }
            let cause = check_context(&original).expect_err("stopped Original entered capture");
            assert!(matches!(&cause, ContextError::Original(error) if error.kind() == expected));
            Err(cause)
        })();
        finish_run_work(result, content.max_output_bytes)
    });
    let (send, mut receive) = mpsc::channel(1);
    service
        .0
        .jobs
        .lock()
        .await
        .spawn(finish_run_job(work, cancel, service.0.stop.clone(), send));
    let joined = service.0.jobs.lock().await.join_next().await.unwrap();
    service.0.record_job_result(joined);
    service
        .shutdown()
        .await
        .expect("Original refusal poisoned Source shutdown");
    let encoded = receive.recv().await.unwrap().unwrap();
    let outcome: crate::managed_role_run::RunOutcome = serde_json::from_slice(&encoded).unwrap();
    assert!(outcome.answer.is_none());
    assert!(!outcome.run_closed);
    assert!(
        outcome
            .failure
            .unwrap()
            .contains("managed role operation failed:")
    );
    assert!(service.0.jobs.lock().await.is_empty());
    assert!(service.0.retained_runs.lock().await.is_empty());
    assert!(service.0.first_failure.lock().unwrap().is_none());
}

#[tokio::test]
async fn pre_cancelled_run_worker_joins_without_poisoning_source() {
    stopped_run_before_capture("cancelled").await;
}
#[tokio::test]
async fn pre_expired_run_worker_joins_without_poisoning_source() {
    stopped_run_before_capture("expired").await;
}
#[tokio::test]
async fn run_worker_cancelled_before_capture_joins_without_poisoning_source() {
    stopped_run_before_capture("capture").await;
}

#[tokio::test]
async fn unrelated_worker_errors_remain_failures_when_original_is_also_stopped() {
    let missing = tempfile::tempdir().unwrap();
    let not_found = fs::File::open(missing.path().join("missing")).unwrap_err();
    for error in [
        io::Error::from(io::ErrorKind::Interrupted),
        io::Error::from(io::ErrorKind::TimedOut),
        io::Error::new(io::ErrorKind::Interrupted, "unrelated I/O interrupt"),
        io::Error::new(io::ErrorKind::TimedOut, "unrelated I/O timeout"),
        io::Error::new(io::ErrorKind::PermissionDenied, "authorization refused"),
        io::Error::other(RequestError::DeadlineOverflow),
        not_found,
    ] {
        let expected = (error.kind(), error.raw_os_error(), error.to_string());
        let fixture = crate::source_server::test_fixture::fixture(
            &[],
            Brand::default(),
            NativeBinding::Codex,
        );
        let service = ManagedRolesHttp::new(
            Arc::clone(&fixture.auth),
            Arc::clone(&fixture.projects),
            0,
            fixture.server.policy().clone(),
            CancellationToken::new(),
        );
        let cancel = Cancellation::default();
        cancel.cancel();
        let work = tokio::task::spawn_blocking(move || {
            finish_run_work(Err(ContextError::Io(error)), usize::MAX)
        });
        let (send, _receive) = mpsc::channel(1);
        service.0.jobs.lock().await.spawn(finish_run_job(
            work,
            cancel,
            service.0.stop.clone(),
            send,
        ));
        let joined = service.0.jobs.lock().await.join_next().await.unwrap();
        service.0.record_job_result(joined);
        let failure = service.shutdown().await.unwrap_err();
        let original = failure
            .get_ref()
            .unwrap()
            .downcast_ref::<Arc<io::Error>>()
            .unwrap();
        assert_eq!(
            (
                original.kind(),
                original.raw_os_error(),
                original.to_string()
            ),
            expected
        );
        assert!(service.0.jobs.lock().await.is_empty());
        assert!(service.0.retained_runs.lock().await.is_empty());
        service.shutdown().await.unwrap();
    }
}

#[test]
fn early_original_outcome_retains_output_bound_and_other_request_errors() {
    let cancelled = ContextError::Request(RequestError::Content(ContentError::Cancelled));
    assert_eq!(
        finish_run_work(Err(cancelled), 0).unwrap_err().kind(),
        io::ErrorKind::FileTooLarge
    );
    let error = finish_run_work(
        Err(ContextError::Request(RequestError::DeadlineOverflow)),
        usize::MAX,
    )
    .unwrap_err();
    assert!(matches!(
        error.get_ref().unwrap().downcast_ref::<RequestError>(),
        Some(RequestError::DeadlineOverflow)
    ));
}
