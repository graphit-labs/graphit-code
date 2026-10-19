#[path = "support/host_fixture.rs"]
mod host_fixture;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Chunk, Cleanup, Counts, IgnoreChunks, IncrementalSink, InvalidRequest, Limits, Reason, Request,
    Retention, SinkFailure, operation, run,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};
#[path = "support/lifecycle_fixture.rs"]
mod fixture;

fn original(budget: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + budget,
        Cancellation::default(),
    ))
}

fn request(address: Option<std::net::SocketAddr>) -> Request {
    let mut environment: Vec<_> = std::env::vars_os()
        .filter(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT"))
        .collect();
    if let Some(address) = address {
        environment.push((
            "GRAPHIT_LIFECYCLE_FIXTURE".into(),
            address.to_string().into(),
        ));
    }
    Request {
        program: std::env::current_exe().unwrap(),
        args: vec![
            "--exact".into(),
            "fixture::native_fixture".into(),
            "--nocapture".into(),
        ],
        cwd: std::env::current_dir().unwrap(),
        environment,
        stdin: None,
        limits: Limits {
            stdin_bytes: 0,
            stdout_bytes: 4096,
            stderr_bytes: 4096,
            termination_grace: Duration::ZERO,
            cleanup: Duration::from_millis(400),
        },
    }
}

#[tokio::test(flavor = "current_thread")]
async fn closed_and_fresh_context_are_rejected_before_native_birth() {
    let host = host_fixture::install(8).await;
    let context = original(Duration::from_secs(5));

    let (stop, receiver) = operation(&context, &host).unwrap();
    stop.stop();
    let failure = run(
        request(None),
        &context,
        Retention::None,
        &mut IgnoreChunks,
        receiver,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.reason, Reason::Stopped);
    assert_eq!(failure.cleanup, Cleanup::NotSpawned);
    assert_eq!(failure.observed, Counts::default());
    assert!(failure.remaining.is_none());
    let (stop, receiver) = operation(&context, &host).unwrap();
    let foreign = original(Duration::from_secs(5));
    let failure = run(
        request(None),
        &foreign,
        Retention::None,
        &mut IgnoreChunks,
        receiver,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure.reason,
        Reason::Invalid(InvalidRequest::InvalidRequestContext)
    );
    assert_eq!(failure.cleanup, Cleanup::NotSpawned);
    drop(stop);
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
}

struct HeldSink {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}
impl IncrementalSink for HeldSink {
    async fn push<'a>(
        &'a mut self,
        _: Chunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(())
    }
}
struct AckSink {
    socket: TcpListener,
    ready: Arc<Notify>,
    release: Arc<Notify>,
    acknowledged: bool,
}
impl IncrementalSink for AckSink {
    async fn push<'a>(
        &'a mut self,
        _: Chunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if !self.acknowledged {
            let (mut peer, _) = self.socket.accept().await.unwrap();
            let mut ready = [0];
            peer.read_exact(&mut ready).await.unwrap();
            assert_eq!(&ready, b"R");
            self.ready.notify_one();
            self.release.notified().await;
            peer.write_all(b"A").await.unwrap();
            self.acknowledged = true;
        }
        Ok(())
    }
}
#[tokio::test(flavor = "current_thread")]
async fn stop_a_drops_pending_delivery_and_b_acknowledges_after_a_cleanup_on_same_original() {
    let host = host_fixture::install(8).await;
    let context = original(Duration::from_secs(8));

    let listener_a = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listener_b = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let request_a = request(Some(listener_a.local_addr().unwrap()));
    let request_b = request(Some(listener_b.local_addr().unwrap()));
    let a_entered = Arc::new(Notify::new());
    let a_done = Arc::new(Notify::new());
    let b_ready = Arc::new(Notify::new());
    let b_release = Arc::new(Notify::new());
    let mut sink_a = HeldSink {
        entered: a_entered.clone(),
        release: Arc::new(Notify::new()),
    };
    let mut sink_b = AckSink {
        socket: listener_b,
        ready: b_ready.clone(),
        release: b_release.clone(),
        acknowledged: false,
    };
    let (stop_a, receiver_a) = operation(&context, &host).unwrap();
    let (stop_b, receiver_b) = operation(&context, &host).unwrap();
    let future_a = async {
        let result = run(
            request_a,
            &context,
            Retention::None,
            &mut sink_a,
            receiver_a,
        )
        .await;
        a_done.notify_one();
        result
    };
    let future_b = run(
        request_b,
        &context,
        Retention::None,
        &mut sink_b,
        receiver_b,
    );
    let steer = async {
        let (mut peer, _) = listener_a.accept().await.unwrap();
        let mut ready = [0];
        peer.read_exact(&mut ready).await.unwrap();
        assert_eq!(&ready, b"R");
        a_entered.notified().await;
        b_ready.notified().await;
        stop_a.stop();
        a_done.notified().await;
        assert!(context.check().is_ok());
        b_release.notify_one();
    };
    let (a, b, ()) = tokio::time::timeout(Duration::from_secs(7), async {
        tokio::join!(future_a, future_b, steer)
    })
    .await
    .unwrap();
    let a = a.unwrap_err();
    assert_eq!(a.reason, Reason::Stopped);
    assert_eq!(a.delivered, Counts::default());
    #[cfg(unix)]
    {
        assert_eq!(a.cleanup, Cleanup::Complete);
        assert!(a.remaining.is_none());
    }
    let b = b.unwrap();
    assert_eq!(b.status.code(), Some(7));
    assert!(b.delivered.stdout > 0);
    drop(stop_b);
    assert!(context.check().is_ok());
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_and_deadline_of_original_stop_both_actual_operations() {
    let host = host_fixture::install(8).await;
    for cancelled in [true, false] {
        let context = original(if cancelled {
            Duration::from_secs(6)
        } else {
            Duration::from_secs(1)
        });

        let a = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let b = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let request_a = request(Some(a.local_addr().unwrap()));
        let request_b = request(Some(b.local_addr().unwrap()));
        let (stop_a, receiver_a) = operation(&context, &host).unwrap();
        let (stop_b, receiver_b) = operation(&context, &host).unwrap();
        let mut sink_a = IgnoreChunks;
        let mut sink_b = IgnoreChunks;
        let steer = async {
            let (mut a, _) = a.accept().await.unwrap();
            let (mut b, _) = b.accept().await.unwrap();
            let mut ready = [0];
            a.read_exact(&mut ready).await.unwrap();
            assert_eq!(&ready, b"R");
            b.read_exact(&mut ready).await.unwrap();
            assert_eq!(&ready, b"R");
            if cancelled {
                context.cancellation().cancel();
            } else {
                tokio::time::sleep_until(tokio::time::Instant::from_std(
                    context.required_deadline().unwrap(),
                ))
                .await;
            }
        };
        let (a, b, ()) = tokio::join!(
            run(
                request_a,
                &context,
                Retention::None,
                &mut sink_a,
                receiver_a
            ),
            run(
                request_b,
                &context,
                Retention::None,
                &mut sink_b,
                receiver_b
            ),
            steer,
        );
        let expected = if cancelled {
            Reason::Cancelled
        } else {
            Reason::TimedOut
        };
        assert_eq!(a.unwrap_err().reason, expected);
        assert_eq!(b.unwrap_err().reason, expected);
        drop((stop_a, stop_b));
    }
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
}
