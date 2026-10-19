use super::*;
use graphit_core::brand::Brand;
use graphit_hub::NativeBinding;
use std::time::Duration;

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
