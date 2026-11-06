use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Chunk, Cleanup, HostLimits, HostReadiness, HostShutdown, Limits, OperationAdmissionFailure,
    PipeEnd, ProcessHost, Reason, RepairObservation, Request, Retention, ServiceSink, SinkFailure,
    Stream, operation, run_service, spawn_service,
};
use secrecy::zeroize::Zeroizing;
use std::{
    ffi::OsString,
    io::{Read, Write},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{sleep, timeout},
};

fn original(cancel: &Cancellation, duration: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + duration,
        cancel.clone(),
    ))
}
async fn install_host(service: &Cancellation, slots: usize) -> ProcessHost {
    let host = ProcessHost::install(service, HostLimits::new(slots, 1).unwrap()).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    host
}
fn request(mode: &str, address: &str) -> Request {
    let mut environment: Vec<_> = std::env::vars_os()
        .filter(|(k, _)| k.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT"))
        .collect();
    environment.push((
        OsString::from("GRAPHIT_SERVICE_TEST_MODE"),
        OsString::from(mode),
    ));
    environment.push((
        OsString::from("GRAPHIT_SERVICE_TEST_ADDRESS"),
        OsString::from(address),
    ));
    Request {
        program: std::env::current_exe().unwrap(),
        args: [
            "--exact",
            "native_service_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        cwd: std::env::current_dir().unwrap(),
        environment,
        stdin: Some(Zeroizing::new(vec![37; 64])),
        limits: Limits {
            stdin_bytes: 64,
            stdout_bytes: 64 * 1024,
            stderr_bytes: 64 * 1024,
            termination_grace: Duration::from_millis(20),
            cleanup: Duration::from_secs(2),
        },
    }
}
async fn connected(listener: &TcpListener) -> TcpStream {
    let (mut peer, _) = timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut byte = [0];
    peer.read_exact(&mut byte).await.unwrap();
    assert_eq!(byte, [b'L']);
    peer
}
async fn disconnected_after_native_stop(peer: &mut TcpStream) {
    let mut byte = [0];
    let result = timeout(Duration::from_secs(2), peer.read(&mut byte))
        .await
        .unwrap();
    // The caller has already observed readiness and requested/observed native
    // teardown. This socket is a closure check, separate from the Job receipt.
    assert!(
        matches!(result, Ok(0))
            || cfg!(windows)
                && result.is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionReset)
    );
}
async fn joined(host: ProcessHost) {
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}
fn cleanup(cleanup: Cleanup) {
    assert_eq!(cleanup, Cleanup::Complete);
}
#[derive(Default)]
struct Collect {
    bytes: Vec<u8>,
    reject: bool,
}
impl ServiceSink for Collect {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        service: &'a Cancellation,
    ) -> Result<(), SinkFailure> {
        assert!(!service.is_cancelled());
        if self.reject {
            return Err(SinkFailure::ConsumerFailed);
        }
        if chunk.stream() == Stream::Stdout {
            self.bytes.extend_from_slice(chunk.bytes());
        }
        Ok(())
    }
}

#[test]
#[ignore = "real native child entry exercised by parent tests"]
fn native_service_child() {
    let mode = std::env::var("GRAPHIT_SERVICE_TEST_MODE").unwrap();
    let mut peer =
        std::net::TcpStream::connect(std::env::var("GRAPHIT_SERVICE_TEST_ADDRESS").unwrap())
            .unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    peer.write_all(b"L").unwrap();
    if mode == "broken" {
        let mut ack = [0];
        peer.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [b'A']);
        std::process::exit(7);
    }
    if mode != "blocked" {
        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input).unwrap();
        assert_eq!(input, vec![37; 64]);
        std::io::stdout().write_all(b"bootstrap-eof").unwrap();
        std::io::stdout().flush().unwrap();
        if mode == "flood" {
            let _ = std::io::stdout().write_all(&[97; 8192]);
            std::process::exit(7);
        }
    }
    let mut ack = [0];
    peer.read_exact(&mut ack).unwrap();
    assert_eq!(ack, [b'A']);
    peer.write_all(b"K").unwrap();
    std::io::stdout().write_all(b"after-first-rpc").unwrap();
    std::process::exit(7);
}

#[tokio::test(flavor = "current_thread")]
async fn bootstrap_owner_survives_first_rpc_and_releases_its_arc() {
    let service = Cancellation::default();
    let host = install_host(&service, 1).await;
    let startup_cancel = Cancellation::default();
    let startup = original(&startup_cancel, Duration::from_millis(350));
    let weak = Arc::downgrade(&startup);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let process = spawn_service(
        request("alive", &listener.local_addr().unwrap().to_string()),
        &startup,
        &host,
    )
    .await
    .unwrap();
    let mut peer = connected(&listener).await;
    assert_eq!(Arc::strong_count(&startup), 1);
    sleep(Duration::from_millis(400)).await;
    startup_cancel.cancel();
    drop(startup);
    assert!(weak.upgrade().is_none());
    assert!(!service.is_cancelled());
    peer.write_all(b"A").await.unwrap();
    let mut byte = [0];
    peer.read_exact(&mut byte).await.unwrap();
    assert_eq!(byte, [b'K']);
    let mut sink = Collect::default();
    let result = run_service(process, Retention::None, &mut sink)
        .await
        .unwrap();
    assert_eq!(result.status.code(), Some(7));
    cleanup(result.cleanup);
    assert!(result.transcript.is_none());
    assert_eq!(result.pipes.stdout, PipeEnd::Eof);
    assert!(sink.bytes.windows(13).any(|v| v == b"bootstrap-eof"));
    assert!(sink.bytes.windows(15).any(|v| v == b"after-first-rpc"));
    joined(host).await;
}

#[tokio::test(flavor = "current_thread")]
async fn real_service_local_and_host_stop_do_not_cancel_other_signals() {
    for mode in ["service", "local", "host"] {
        let service = Cancellation::default();
        let host = install_host(&service, 1).await;
        let startup_cancel = Cancellation::default();
        let startup = original(&startup_cancel, Duration::from_secs(5));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let process = spawn_service(
            request("alive", &listener.local_addr().unwrap().to_string()),
            &startup,
            &host,
        )
        .await
        .unwrap();
        let mut peer = connected(&listener).await;
        let stop = process.stop_handle();
        let mut sink = Collect::default();
        let run = run_service(process, Retention::None, &mut sink);
        let shutdown = async {
            sleep(Duration::from_millis(20)).await;
            match mode {
                "service" => service.cancel(),
                "local" => stop.stop(),
                "host" => (),
                _ => unreachable!(),
            }
            if mode == "host" {
                joined(host).await;
                None
            } else {
                Some(host)
            }
        };
        let (result, host) = tokio::join!(run, shutdown);
        let error = result.unwrap_err();
        assert_eq!(
            error.reason,
            if mode == "service" {
                Reason::Cancelled
            } else {
                Reason::Stopped
            }
        );
        cleanup(error.cleanup);
        assert!(error.status.is_some());
        assert!(!startup_cancel.is_cancelled());
        if mode != "service" {
            assert!(!service.is_cancelled());
        }
        disconnected_after_native_stop(&mut peer).await;
        drop(stop);
        if let Some(host) = host {
            joined(host).await;
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn admission_and_bootstrap_failures_are_real() {
    let service = Cancellation::default();
    let host = install_host(&service, 1).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let unbounded = Arc::new(SnapshotContext::service(Cancellation::default()));
    let error = spawn_service(request("alive", &address), &unbounded, &host)
        .await
        .unwrap_err();
    assert!(
        matches!(error.admission, Some(OperationAdmissionFailure::Core(error))
        if error.kind() == std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(
        error.reason,
        Reason::Invalid(graphit_process::InvalidRequest::InvalidRequestContext)
    );
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    assert_eq!(error.pipes.stdout, PipeEnd::NotSpawned);
    assert_eq!(error.pipes.stderr, PipeEnd::NotSpawned);
    assert!(error.status.is_none());
    assert_eq!(Arc::strong_count(&unbounded), 1);
    let expired = original(&Cancellation::default(), Duration::ZERO);
    let error = spawn_service(request("alive", &address), &expired, &host)
        .await
        .unwrap_err();
    assert_eq!(error.reason, Reason::TimedOut);
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    let startup = original(&Cancellation::default(), Duration::from_secs(5));
    let mut oversized = request("alive", &address);
    oversized.stdin.as_mut().unwrap().push(37);
    let error = spawn_service(oversized, &startup, &host).await.unwrap_err();
    assert_eq!(
        error.reason,
        Reason::Invalid(graphit_process::InvalidRequest::StdinLimit)
    );
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    let process = spawn_service(request("alive", &address), &startup, &host)
        .await
        .unwrap();
    let _peer = connected(&listener).await;
    let error = spawn_service(request("alive", &address), &startup, &host)
        .await
        .unwrap_err();
    assert!(matches!(
        error.admission,
        Some(OperationAdmissionFailure::CapacityFull)
    ));
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    assert!(matches!(
        operation(&startup, &host),
        Err(OperationAdmissionFailure::CapacityFull)
    ));
    drop(process);
    joined(host).await;

    for mode in ["blocked", "broken"] {
        let service = Cancellation::default();
        let host = install_host(&service, 1).await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let startup = original(
            &Cancellation::default(),
            if mode == "blocked" {
                Duration::from_millis(250)
            } else {
                Duration::from_secs(5)
            },
        );
        let mut req = request(mode, &address);
        req.stdin = Some(Zeroizing::new(vec![37; 8 * 1024 * 1024]));
        req.limits.stdin_bytes = 8 * 1024 * 1024;
        let pending = spawn_service(req, &startup, &host);
        let release = async {
            let mut peer = connected(&listener).await;
            if mode == "broken" {
                peer.write_all(b"A").await.unwrap();
            }
            peer
        };
        let (result, _peer) = tokio::join!(pending, release);
        let error = result.unwrap_err();
        if mode == "blocked" {
            assert_eq!(error.reason, Reason::TimedOut);
        } else {
            assert!(
                matches!(error.reason, Reason::Io(graphit_process::Stage::Input, _)),
                "broken bootstrap cause: {:?}",
                error.reason
            );
        }
        cleanup(error.cleanup);
        assert!(error.status.is_some());
        assert_eq!(Arc::strong_count(&startup), 1);
        joined(host).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn sink_and_output_limits_stop_actual_child_and_keep_private_debug() {
    for reject in [true, false] {
        let service = Cancellation::default();
        let host = install_host(&service, 1).await;
        let startup = original(&Cancellation::default(), Duration::from_secs(5));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let mut req = request(if reject { "alive" } else { "flood" }, &address);
        if !reject {
            req.limits.stdout_bytes = 1;
        }
        let debug = format!("{req:?}");
        assert!(!debug.contains(&address));
        assert!(!debug.contains("GRAPHIT_SERVICE_TEST"));
        let process = spawn_service(req, &startup, &host).await.unwrap();
        let _peer = connected(&listener).await;
        assert_eq!(format!("{process:?}"), "ServiceProcess");
        let error = run_service(
            process,
            Retention::Full,
            &mut Collect {
                bytes: Vec::new(),
                reject,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.reason,
            if reject {
                Reason::Sink(SinkFailure::ConsumerFailed)
            } else {
                Reason::OutputLimit(Stream::Stdout)
            }
        );
        cleanup(error.cleanup);
        assert!(!format!("{error:?}").contains("bootstrap-eof"));
        joined(host).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn local_a_stop_preserves_b_and_same_host_capacity() {
    let service = Cancellation::default();
    let host = install_host(&service, 2).await;
    let startup = original(&Cancellation::default(), Duration::from_secs(5));
    let a = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pa = spawn_service(
        request("alive", &a.local_addr().unwrap().to_string()),
        &startup,
        &host,
    )
    .await
    .unwrap();
    let _a = connected(&a).await;
    let pb = spawn_service(
        request("alive", &b.local_addr().unwrap().to_string()),
        &startup,
        &host,
    )
    .await
    .unwrap();
    let mut b = connected(&b).await;
    pa.stop();
    let error = run_service(pa, Retention::None, &mut Collect::default())
        .await
        .unwrap_err();
    assert_eq!(error.reason, Reason::Stopped);
    cleanup(error.cleanup);
    assert!(!service.is_cancelled());
    b.write_all(b"A").await.unwrap();
    let mut byte = [0];
    b.read_exact(&mut byte).await.unwrap();
    assert_eq!(byte, [b'K']);
    let result = run_service(pb, Retention::None, &mut Collect::default())
        .await
        .unwrap();
    assert_eq!(result.status.code(), Some(7));
    cleanup(result.cleanup);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match operation(&startup, &host) {
            Ok((stop, receiver)) => {
                drop(receiver);
                drop(stop);
                break;
            }
            Err(OperationAdmissionFailure::CapacityFull) => {
                assert!(Instant::now() < deadline);
                sleep(Duration::from_millis(1)).await
            }
            Err(e) => panic!("unexpected admission {e:?}"),
        }
    }
    joined(host).await;
}

#[tokio::test(flavor = "current_thread")]
async fn tiny_cleanup_returns_actual_same_record_for_repair() {
    let service = Cancellation::default();
    let host = install_host(&service, 1).await;
    let startup = original(&Cancellation::default(), Duration::from_secs(5));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let mut req = request("alive", &address);
    req.limits.cleanup = Duration::from_nanos(1);
    req.limits.termination_grace = Duration::ZERO;
    let process = spawn_service(req, &startup, &host).await.unwrap();
    let _peer = connected(&listener).await;
    process.stop();
    let mut error = run_service(process, Retention::None, &mut Collect::default())
        .await
        .unwrap_err();
    if let Some(remaining) = error.remaining.as_mut() {
        eprintln!("actual service Remaining returned; observing same Host record");
        let observation = remaining
            .repair(Instant::now() + Duration::from_secs(5))
            .await;
        assert!(matches!(
            observation,
            RepairObservation::Observed {
                status: Some(_),
                ..
            }
        ));
    } else {
        assert!(error.status.is_some());
    }
    joined(host).await;
}

#[tokio::test(flavor = "current_thread")]
async fn aborting_bootstrap_closes_same_native_record() {
    let service = Cancellation::default();
    let host = install_host(&service, 1).await;
    let startup = original(&Cancellation::default(), Duration::from_secs(5));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let mut req = request("blocked", &address);
    req.stdin = Some(Zeroizing::new(vec![37; 8 * 1024 * 1024]));
    req.limits.stdin_bytes = 8 * 1024 * 1024;
    let mut peer = {
        let pending = spawn_service(req, &startup, &host);
        tokio::pin!(pending);
        tokio::select! { peer=connected(&listener)=>peer, result=&mut pending=>panic!("unexpected bootstrap result {result:?}") }
    };
    disconnected_after_native_stop(&mut peer).await;
    joined(host).await;
    assert_eq!(Arc::strong_count(&startup), 1);
    assert!(!service.is_cancelled());
}

#[tokio::test(flavor = "current_thread")]
async fn local_stop_finalizes_borrowed_callback_before_return() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct CallbackGuard(Arc<AtomicBool>);
    impl Drop for CallbackGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    struct BlockSink {
        entered: Arc<tokio::sync::Notify>,
        dropped: Arc<AtomicBool>,
    }
    impl ServiceSink for BlockSink {
        async fn push<'a>(
            &'a mut self,
            _chunk: Chunk<'a>,
            service: &'a Cancellation,
        ) -> Result<(), SinkFailure> {
            let _guard = CallbackGuard(Arc::clone(&self.dropped));
            self.entered.notify_one();
            service.cancelled().await;
            Ok(())
        }
    }
    let service = Cancellation::default();
    let host = install_host(&service, 1).await;
    let startup = original(&Cancellation::default(), Duration::from_secs(5));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let process = spawn_service(request("alive", &address), &startup, &host)
        .await
        .unwrap();
    let _peer = connected(&listener).await;
    let stop = process.stop_handle();
    let entered = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let mut sink = BlockSink {
        entered: Arc::clone(&entered),
        dropped: Arc::clone(&dropped),
    };
    let control = async {
        entered.notified().await;
        stop.stop();
    };
    let (result, ()) = tokio::join!(run_service(process, Retention::None, &mut sink), control);
    let error = result.unwrap_err();
    assert_eq!(error.reason, Reason::Stopped);
    cleanup(error.cleanup);
    assert!(dropped.load(Ordering::Acquire));
    assert_eq!(error.delivered.stdout, 0);
    assert!(!service.is_cancelled());
    drop(stop);
    joined(host).await;
}
