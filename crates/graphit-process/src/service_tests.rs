use super::*;
use tokio::io::AsyncReadExt;

struct GatedSink {
    entered: Option<oneshot::Sender<()>>,
    release: Option<oneshot::Receiver<()>>,
}
impl ServiceSink for GatedSink {
    async fn push<'a>(&'a mut self, _: Chunk<'a>, _: &'a Cancellation) -> Result<(), SinkFailure> {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
            self.release.take().unwrap().await.unwrap();
        }
        Ok(())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn late_observation_preserves_service_control_and_real_cleanup() {
    for mode in ["ready", "cancelled", "stopped", "pending"] {
        let service = Cancellation::default();
        let host = ProcessHost::install(&service, HostLimits::new(1, 1).unwrap()).unwrap();
        assert!(matches!(
            host.ready(std::time::Instant::now() + Duration::from_secs(5))
                .await,
            HostReadiness::Ready
        ));
        let startup = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        // Reuse the actual native child already exercised by supervisor tests.
        let mut environment: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT"))
            .collect();
        environment.extend([
            (
                "GRAPHIT_PROCESS_RECOVERY_FIXTURE".into(),
                listener.local_addr().unwrap().to_string().into(),
            ),
            ("GRAPHIT_PROCESS_RECOVERY_MODE".into(), "held".into()),
        ]);
        let process = spawn_service(
            Request {
                program: std::env::current_exe().unwrap(),
                args: [
                    "--exact",
                    "supervisor::tests::native_recovery_fixture",
                    "--nocapture",
                ]
                .into_iter()
                .map(OsString::from)
                .collect(),
                cwd: std::env::current_dir().unwrap(),
                environment,
                stdin: None,
                limits: Limits {
                    stdin_bytes: 0,
                    stdout_bytes: 4096,
                    stderr_bytes: 4096,
                    termination_grace: Duration::ZERO,
                    cleanup: Duration::from_millis(200),
                },
            },
            &startup,
            &host,
        )
        .await
        .unwrap();
        let slot = Arc::clone(&process.control.slot);
        let stop = process.stop_handle();
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let mut sink = GatedSink {
            entered: Some(entered_tx),
            release: Some(release_rx),
        };
        let result = {
            let call = run_service(process, Retention::Full, &mut sink);
            tokio::pin!(call);
            tokio::select! {
                result = &mut call => panic!("native service ended before callback gate: {result:?}"),
                entered = entered_rx => entered.unwrap(),
            }
            let (mut peer, _) = listener.accept().await.unwrap();
            let mut ready = [0];
            peer.read_exact(&mut ready).await.unwrap();
            assert_eq!(ready, *b"R");
            peer.write_all(b"A").await.unwrap();
            let terminal = slot.completed().await;
            let endpoint = terminal.supervision.observation.cleanup_end.unwrap();
            if mode != "pending" {
                release_tx.send(()).unwrap();
            }
            assert!(Instant::now() < endpoint);
            match mode {
                "cancelled" => service.cancel(),
                "stopped" => stop.stop(),
                _ => (),
            }
            // The ready callback and real native terminal await only observation.
            // The pending branch deliberately leaves its callback unfinished.
            sleep_until(endpoint + Duration::from_millis(1)).await;
            call.await
        };
        let report = crate::host_fixture::joined(host).await;
        if mode != "cancelled" {
            assert!(report.failure().is_none());
        }
        let (cleanup, status, pipes) = if mode == "ready" {
            let outcome = result.unwrap();
            assert_eq!(outcome.delivered, outcome.observed);
            assert!(outcome.observed.stdout > 0);
            assert!(outcome.remaining.is_none());
            (outcome.cleanup, Some(outcome.status), outcome.pipes)
        } else {
            let failure = result.unwrap_err();
            assert_eq!(
                failure.reason,
                match mode {
                    "cancelled" => Reason::Cancelled,
                    "stopped" => Reason::Stopped,
                    _ => Reason::Cleanup,
                }
            );
            assert_eq!(failure.delivered, Counts::default());
            assert!(failure.remaining.is_none());
            (failure.cleanup, failure.status, failure.pipes)
        };
        assert_eq!(cleanup, Cleanup::Complete);
        assert_eq!(status.unwrap().code(), Some(7));
        if mode == "cancelled" {
            // Cancelling the Host service can close its I/O driver before the
            // caller observes EOF. The failed operation delivers no callback.
            assert!(matches!(pipes.stdout, PipeEnd::Eof | PipeEnd::Closed));
            assert!(matches!(pipes.stderr, PipeEnd::Eof | PipeEnd::Closed));
        } else if mode == "pending" {
            assert_eq!(pipes.stdout, PipeEnd::Closed);
        } else {
            assert_eq!(pipes.stdout, PipeEnd::Eof);
            assert_eq!(pipes.stderr, PipeEnd::Eof);
        }
    }
}
