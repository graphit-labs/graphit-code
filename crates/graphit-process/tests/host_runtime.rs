#[path = "support/host_fixture.rs"]
mod host_fixture;

use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    HostLimits, HostReadiness, OperationAdmissionFailure, ProcessHost, operation,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    ))
}

#[tokio::test(flavor = "current_thread")]
async fn unbounded_original_is_rejected_before_host_reservation() {
    let host = host_fixture::install(1).await;
    let unbounded = Arc::new(SnapshotContext::service(Cancellation::default()));
    let rejected = operation(&unbounded, &host).unwrap_err();
    assert!(matches!(rejected, OperationAdmissionFailure::Core(error)
        if error.kind() == std::io::ErrorKind::InvalidInput));
    let finite = original();
    let (stop, receiver) = operation(&finite, &host).unwrap();
    // Missing deadline wins even when reservation would fail with CapacityFull.
    let rejected = operation(&unbounded, &host).unwrap_err();
    assert!(matches!(rejected, OperationAdmissionFailure::Core(error)
        if error.kind() == std::io::ErrorKind::InvalidInput));
    assert_eq!(Arc::strong_count(&unbounded), 1);
    drop((stop, receiver));
    assert!(host_fixture::joined(host).await.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_host_limits_are_rejected_and_valid_host_starts() {
    assert!(HostLimits::new(0, 1).is_err());
    assert!(HostLimits::new(1, 0).is_err());
    let host =
        ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1).unwrap()).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    assert!(host_fixture::joined(host).await.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn one_host_slot_survives_one_endpoint_and_reuses_after_last_owner() {
    let host = host_fixture::install(1).await;
    // The service signal and operation original are independent.
    let context = original();
    let (stop, receiver) = operation(&context, &host).unwrap();
    assert!(matches!(
        operation(&context, &host),
        Err(OperationAdmissionFailure::CapacityFull)
    ));
    drop(stop);
    assert!(matches!(
        operation(&context, &host),
        Err(OperationAdmissionFailure::CapacityFull)
    ));
    drop(receiver);
    let (stop, receiver) = operation(&context, &host).unwrap();
    drop((stop, receiver));
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn sealed_host_closes_unsubmitted_receiver_and_joins_same_thread() {
    let host = host_fixture::install(1).await;
    let context = original();
    let (stop, receiver) = operation(&context, &host).unwrap();
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
    drop((stop, receiver));
}

#[path = "support/lifecycle_fixture.rs"]
mod fixture;

use graphit_process::{
    Cleanup, IgnoreChunks, InvalidRequest, Limits, Reason, Request, Retention, run,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};

fn bounded_original(budget: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + budget,
        Cancellation::default(),
    ))
}
fn limits(slots: usize) -> HostLimits {
    HostLimits::new(slots, 1).unwrap()
}
async fn service_host(service: &Cancellation, slots: usize) -> ProcessHost {
    let host = ProcessHost::install(service, limits(slots)).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    host
}
async fn after_record_owners_close<'a>(
    original: &'a Arc<SnapshotContext>,
    host: &ProcessHost,
) -> (
    graphit_process::OperationStop,
    graphit_process::OperationReceiver<'a>,
) {
    // Unconfirmed cleanup conservatively retains capacity until its actual
    // owners close. Never fabricate release, renew the context, or accept another
    // error. The Complete regression below requires immediate admission without
    // this helper.
    tokio::time::timeout_at(original.required_deadline().unwrap().into(), async {
        loop {
            match operation(original, host) {
                Ok(pair) => break pair,
                Err(OperationAdmissionFailure::CapacityFull) => tokio::task::yield_now().await,
                Err(error) => panic!("closed record admission failed: {error:?}"),
            }
        }
    })
    .await
    .expect("physical record owners must close within the original deadline")
}

fn native_request(address: Option<std::net::SocketAddr>) -> Request {
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
fn finite_request() -> Request {
    let mut request = native_request(None);
    request.program = env!("CARGO_BIN_EXE_subprocess-fixture").into();
    request.args = vec!["bytes".into()];
    request
}
async fn native_ready(listener: &TcpListener) -> TcpStream {
    let (mut peer, _) = listener.accept().await.unwrap();
    let mut ready = [0];
    peer.read_exact(&mut ready).await.unwrap();
    assert_eq!(ready, *b"R");
    peer
}

#[derive(Clone, Copy)]
enum EndA {
    Cancel,
    Deadline,
    Stop,
    Drop,
}

#[tokio::test(flavor = "current_thread")]
async fn distinct_native_requests_keep_b_and_service_live_after_a_ends() {
    for cause in [EndA::Cancel, EndA::Deadline, EndA::Stop, EndA::Drop] {
        let service = Cancellation::default();
        let host = service_host(&service, 2).await;
        let a = bounded_original(if matches!(cause, EndA::Deadline) {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(8)
        });
        let b = bounded_original(Duration::from_secs(8));
        let listener_a = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_b = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (stop_a, receiver_a) = operation(&a, &host).unwrap();
        let (stop_b, receiver_b) = operation(&b, &host).unwrap();
        let done_a = Notify::new();
        let future_a = async {
            let result = run(
                native_request(Some(listener_a.local_addr().unwrap())),
                &a,
                Retention::None,
                &mut IgnoreChunks,
                receiver_a,
            )
            .await;
            done_a.notify_one();
            result
        };
        let mut sink_b = IgnoreChunks;
        let future_b = run(
            native_request(Some(listener_b.local_addr().unwrap())),
            &b,
            Retention::None,
            &mut sink_b,
            receiver_b,
        );
        let steer = async {
            let _peer_a = native_ready(&listener_a).await;
            let mut peer_b = native_ready(&listener_b).await;
            let mut retained_a = Some(stop_a);
            match cause {
                EndA::Cancel => a.cancellation().cancel(),
                EndA::Deadline => (),
                EndA::Stop => retained_a.take().unwrap().stop(),
                EndA::Drop => drop(retained_a.take()),
            }
            done_a.notified().await;
            drop(retained_a);
            assert!(b.check().is_ok());
            assert_eq!(
                a.cancellation().is_cancelled(),
                matches!(cause, EndA::Cancel)
            );
            if matches!(cause, EndA::Stop | EndA::Drop) {
                assert!(a.check().is_ok());
            }
            assert!(!service.is_cancelled());
            assert!(matches!(
                host.ready(Instant::now()).await,
                HostReadiness::Ready
            ));
            let c = bounded_original(Duration::from_secs(5));
            let (stop_c, receiver_c) = after_record_owners_close(&c, &host).await;
            let c = run(
                finite_request(),
                &c,
                Retention::Full,
                &mut IgnoreChunks,
                receiver_c,
            )
            .await
            .unwrap();
            assert_eq!(c.status.code(), Some(7));
            drop(stop_c);
            peer_b.write_all(b"A").await.unwrap();
        };
        let (a_result, b_result, ()) = tokio::time::timeout(Duration::from_secs(7), async {
            tokio::join!(future_a, future_b, steer)
        })
        .await
        .unwrap();
        let failure = a_result.unwrap_err();
        assert_eq!(
            failure.reason,
            match cause {
                EndA::Cancel => Reason::Cancelled,
                EndA::Deadline => Reason::TimedOut,
                EndA::Stop | EndA::Drop => Reason::Stopped,
            }
        );
        #[cfg(unix)]
        {
            assert_eq!(failure.cleanup, Cleanup::Complete);
            assert!(failure.remaining.is_none());
        }
        let b_result = b_result.unwrap();
        assert_eq!(b_result.status.code(), Some(7));
        assert!(b_result.delivered.stdout > 0);
        drop(stop_b);
        assert!(host_fixture::joined(host).await.failure().is_none());
        assert!(!service.is_cancelled());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn foreign_request_is_denied_without_native_birth_or_service_cancel() {
    let service = Cancellation::default();
    let host = service_host(&service, 1).await;
    let a = original();
    let b = original();
    let (stop, receiver) = operation(&a, &host).unwrap();
    let failure = run(
        finite_request(),
        &b,
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
    assert!(failure.remaining.is_none());
    assert!(a.check().is_ok() && b.check().is_ok());
    assert!(!service.is_cancelled());
    drop(stop);
    assert!(host_fixture::joined(host).await.failure().is_none());
    assert!(!service.is_cancelled());
}

#[tokio::test(flavor = "current_thread")]
async fn service_cancel_stops_distinct_actual_children_without_cancelling_requests() {
    let service = Cancellation::default();
    let host = service_host(&service, 2).await;
    let a = original();
    let b = original();
    let listener_a = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listener_b = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (stop_a, receiver_a) = operation(&a, &host).unwrap();
    let (stop_b, receiver_b) = operation(&b, &host).unwrap();
    let mut sink_a = IgnoreChunks;
    let future_a = run(
        native_request(Some(listener_a.local_addr().unwrap())),
        &a,
        Retention::None,
        &mut sink_a,
        receiver_a,
    );
    let mut sink_b = IgnoreChunks;
    let future_b = run(
        native_request(Some(listener_b.local_addr().unwrap())),
        &b,
        Retention::None,
        &mut sink_b,
        receiver_b,
    );
    let steer = async {
        let _a = native_ready(&listener_a).await;
        let _b = native_ready(&listener_b).await;
        service.cancel();
        assert!(matches!(
            operation(&a, &host),
            Err(OperationAdmissionFailure::HostSealed)
        ));
        assert!(matches!(
            host.ready(Instant::now()).await,
            HostReadiness::Sealed
        ));
        assert!(a.check().is_ok() && b.check().is_ok());
    };
    let (a_result, b_result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(future_a, future_b, steer)
    })
    .await
    .unwrap();
    for failure in [a_result.unwrap_err(), b_result.unwrap_err()] {
        assert_eq!(failure.reason, Reason::Stopped);
        #[cfg(unix)]
        {
            assert_eq!(failure.cleanup, Cleanup::Complete);
            assert!(failure.remaining.is_none());
        }
    }
    drop((stop_a, stop_b));
    assert!(a.check().is_ok() && b.check().is_ok());
    assert!(host_fixture::joined(host).await.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn terminal_native_cleanup_controls_slot_reuse_independent_of_observer_drop() {
    let service = Cancellation::default();
    let host = service_host(&service, 1).await;
    let a = original();
    let (stop_a, receiver_a) = operation(&a, &host).unwrap();
    let result = run(
        finite_request(),
        &a,
        Retention::None,
        &mut IgnoreChunks,
        receiver_a,
    )
    .await
    .unwrap();
    assert_eq!(result.status.code(), Some(7));
    #[cfg(unix)]
    assert_eq!(result.cleanup, Cleanup::Complete);
    if matches!(result.cleanup, Cleanup::Complete | Cleanup::NotSpawned) {
        let b = original();
        let (stop_b, receiver_b) = operation(&b, &host).unwrap();
        drop(stop_a);
        // A retired observer cannot release the newly reserved operation.
        assert!(matches!(
            operation(&b, &host),
            Err(OperationAdmissionFailure::CapacityFull)
        ));
        let result = run(
            finite_request(),
            &b,
            Retention::None,
            &mut IgnoreChunks,
            receiver_b,
        )
        .await
        .unwrap();
        assert_eq!(result.status.code(), Some(7));
        drop(stop_b);
    } else {
        // Unconfirmed tree cleanup retains the existing conservative ownership.
        assert!(matches!(
            operation(&a, &host),
            Err(OperationAdmissionFailure::CapacityFull)
        ));
        drop(stop_a);
    }
    assert!(host_fixture::joined(host).await.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn precancelled_service_install_is_interrupted() {
    let service = Cancellation::default();
    service.cancel();
    let error = ProcessHost::install(&service, limits(1)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
}

#[test]
fn inherited_service_native_fixture() {
    use std::io::{Read, Write};
    let Ok(address) = std::env::var("GRAPHIT_INHERITED_SERVICE_FIXTURE") else {
        return;
    };
    assert_eq!(
        std::env::current_dir().unwrap(),
        std::path::PathBuf::from(std::env::var_os("GRAPHIT_INHERITED_CWD").unwrap())
    );
    assert_eq!(
        std::env::var("GRAPHIT_INHERITED_LITERAL").unwrap(),
        "literal spaces; dollar$ parentheses() "
    );
    assert!(std::env::var_os("PATH").is_none());
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    assert_eq!(
        input,
        std::env::var("GRAPHIT_INHERITED_INPUT")
            .unwrap_or_default()
            .as_bytes()
    );
    let mut peer = std::net::TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
    std::io::stdout()
        .write_all(b"inherited stdout without captured quota\n")
        .unwrap();
    std::io::stderr()
        .write_all(b"inherited stderr without captured quota\n")
        .unwrap();
    peer.write_all(b"R").unwrap();
    let mut ack = [0];
    peer.read_exact(&mut ack).unwrap();
    assert_eq!(&ack, b"A");
    std::process::exit(7);
}

fn inherited_service_request(
    address: std::net::SocketAddr,
    cwd: &std::path::Path,
) -> graphit_process::InheritedServiceRequest {
    let mut request = native_request(None);
    request.environment.extend([
        (
            "GRAPHIT_INHERITED_SERVICE_FIXTURE".into(),
            address.to_string().into(),
        ),
        ("GRAPHIT_INHERITED_CWD".into(), cwd.as_os_str().to_owned()),
        (
            "GRAPHIT_INHERITED_LITERAL".into(),
            "literal spaces; dollar$ parentheses() ".into(),
        ),
    ]);
    graphit_process::InheritedServiceRequest {
        program: request.program,
        args: vec![
            "--exact".into(),
            "inherited_service_native_fixture".into(),
            "--nocapture".into(),
        ],
        cwd: cwd.to_owned(),
        environment: request.environment,
        stdin: std::process::Stdio::null(),
        stdout: std::process::Stdio::inherit(),
        termination_grace: request.limits.termination_grace,
        cleanup: request.limits.cleanup,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn inherited_service_releases_startup_and_keeps_native_child_past_its_deadline() {
    use graphit_process::{PipeEnd, join_inherited_service, spawn_inherited_service};
    let host = host_fixture::install(1).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let startup = bounded_original(Duration::from_secs(2));
    let weak = Arc::downgrade(&startup);
    let deadline = startup.required_deadline().unwrap();
    let process = spawn_inherited_service(
        inherited_service_request(listener.local_addr().unwrap(), cwd.path()),
        &startup,
        &host,
    )
    .await
    .unwrap();
    assert!(process.pid().is_some_and(|pid| pid != std::process::id()));
    let mut peer = native_ready(&listener).await;
    startup.cancellation().cancel();
    drop(startup);
    assert!(
        weak.upgrade().is_none(),
        "service retains no startup Original"
    );
    tokio::time::sleep_until((deadline + Duration::from_millis(10)).into()).await;
    peer.write_all(b"A").await.unwrap();
    let outcome = join_inherited_service(process).await.unwrap();
    assert_eq!(outcome.status.code(), Some(7));
    assert_eq!(outcome.cleanup, Cleanup::Complete);
    assert_eq!(outcome.pipes.stdout, PipeEnd::NotSpawned);
    assert_eq!(outcome.pipes.stderr, PipeEnd::NotSpawned);
    assert_eq!(outcome.observed.stdout + outcome.observed.stderr, 0);
    assert_eq!(outcome.delivered.stdout + outcome.delivered.stderr, 0);
    assert!(outcome.transcript.is_none() && outcome.remaining.is_none());
    assert!(host_fixture::joined(host).await.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn inherited_service_accepts_native_stdin_without_capturing_output() {
    use graphit_process::{join_inherited_service, spawn_inherited_service};
    use std::io::{Seek, Write};
    let host = host_fixture::install(1).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let mut input = tempfile::tempfile().unwrap();
    input
        .write_all("native stdin, spaces and ç\n".as_bytes())
        .unwrap();
    input.rewind().unwrap();
    let mut request = inherited_service_request(listener.local_addr().unwrap(), cwd.path());
    request.environment.push((
        "GRAPHIT_INHERITED_INPUT".into(),
        "native stdin, spaces and ç\n".into(),
    ));
    request.stdin = std::process::Stdio::from(input);
    let process =
        spawn_inherited_service(request, &bounded_original(Duration::from_secs(2)), &host)
            .await
            .unwrap();
    let mut peer = native_ready(&listener).await;
    peer.write_all(b"A").await.unwrap();
    let outcome = join_inherited_service(process).await.unwrap();
    assert_eq!(outcome.status.code(), Some(7));
    assert!(outcome.transcript.is_none());
    assert!(host_fixture::joined(host).await.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn inherited_service_denies_expired_startup_before_native_birth_and_joins_stop() {
    use graphit_process::{PipeEnd, join_inherited_service, spawn_inherited_service};
    let host = host_fixture::install(1).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cwd = tempfile::tempdir().unwrap();
    for cancelled in [false, true] {
        let original = Arc::new(SnapshotContext::new(
            if cancelled {
                Instant::now() + Duration::from_secs(5)
            } else {
                Instant::now()
            },
            Cancellation::default(),
        ));
        if cancelled {
            original.cancellation().cancel();
        }
        let failure = spawn_inherited_service(
            inherited_service_request(listener.local_addr().unwrap(), cwd.path()),
            &original,
            &host,
        )
        .await
        .unwrap_err();
        assert_eq!(
            failure.reason,
            if cancelled {
                Reason::Cancelled
            } else {
                Reason::TimedOut
            }
        );
        assert_eq!(failure.cleanup, Cleanup::NotSpawned);
        assert_eq!(failure.pipes.stdout, PipeEnd::NotSpawned);
        assert_eq!(failure.pipes.stderr, PipeEnd::NotSpawned);
        assert!(failure.remaining.is_none());
    }
    let process = spawn_inherited_service(
        inherited_service_request(listener.local_addr().unwrap(), cwd.path()),
        &original(),
        &host,
    )
    .await
    .unwrap();
    let _peer = native_ready(&listener).await;
    process.stop();
    let failure = join_inherited_service(process).await.unwrap_err();
    assert_eq!(failure.reason, Reason::Stopped);
    assert_eq!(failure.cleanup, Cleanup::Complete);
    assert!(failure.remaining.is_none());
    assert_eq!(failure.pipes.stdout, PipeEnd::NotSpawned);
    assert_eq!(failure.pipes.stderr, PipeEnd::NotSpawned);
    assert!(host_fixture::joined(host).await.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn inherited_service_host_shutdown_observes_the_same_actual_child() {
    use graphit_process::{PipeEnd, join_inherited_service, spawn_inherited_service};
    let host = host_fixture::install(1).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let process = spawn_inherited_service(
        inherited_service_request(listener.local_addr().unwrap(), cwd.path()),
        &original(),
        &host,
    )
    .await
    .unwrap();
    let _peer = native_ready(&listener).await;
    let (result, report) =
        tokio::join!(join_inherited_service(process), host_fixture::joined(host));
    let failure = result.unwrap_err();
    assert_eq!(failure.reason, Reason::Stopped);
    assert_eq!(failure.cleanup, Cleanup::Complete);
    assert!(failure.remaining.is_none());
    assert_eq!(failure.pipes.stdout, PipeEnd::NotSpawned);
    assert_eq!(failure.pipes.stderr, PipeEnd::NotSpawned);
    assert!(report.failure().is_none());
}
