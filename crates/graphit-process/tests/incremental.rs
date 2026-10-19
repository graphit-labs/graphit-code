#[path = "support/host_fixture.rs"]
mod host_fixture;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Chunk, Cleanup, Counts, IgnoreChunks, IncrementalSink, Limits, PipeEnd, Reason, Request,
    Retention, SinkFailure, Stream, run,
};
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    sync::Notify,
    time::{sleep, timeout},
};

fn assert_owned_cleanup(cleanup: Cleanup) {
    #[cfg(unix)]
    assert_eq!(cleanup, Cleanup::Complete);
    #[cfg(windows)]
    assert_eq!(
        cleanup,
        Cleanup::Unconfirmed(graphit_process::UnconfirmedCleanup::TreeUnknownWindows {
            leader: graphit_process::LeaderCleanup::Reaped,
            control: graphit_process::ControlFailures::default(),
        })
    );
}

const FIRST: &[u8] = &[0, 255, 128, b'1'];
const SECOND: &[u8] = &[254, 0, b'2', 129];
fn original(cancellation: &Cancellation, budget: Duration) -> std::sync::Arc<SnapshotContext> {
    std::sync::Arc::new(SnapshotContext::new(
        std::time::Instant::now() + budget,
        cancellation.clone(),
    ))
}
fn request(dir: &Path, args: &[&str]) -> Request {
    Request {
        program: PathBuf::from(env!("CARGO_BIN_EXE_subprocess-fixture")),
        args: args.iter().map(OsString::from).collect(),
        cwd: dir.to_path_buf(),
        environment: std::env::vars_os()
            .filter(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT"))
            .collect(),
        stdin: None,
        limits: Limits {
            stdin_bytes: 1024 * 1024,
            stdout_bytes: 128 * 1024,
            stderr_bytes: 128 * 1024,
            termination_grace: Duration::from_millis(30),
            cleanup: Duration::from_secs(2),
        },
    }
}
fn listener() -> TcpListener {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    socket.set_nonblocking(true).unwrap();
    socket
}
async fn peer(socket: &TcpListener) -> TcpStream {
    loop {
        match socket.accept() {
            Ok((peer, _)) => {
                peer.set_nonblocking(true).unwrap();
                return peer;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                sleep(Duration::from_millis(5)).await
            }
            Err(e) => panic!("fixture accept kind {:?}", e.kind()),
        }
    }
}
async fn byte(peer: &mut TcpStream) -> Option<u8> {
    let mut bytes = [0];
    loop {
        match peer.read(&mut bytes) {
            Ok(0) => return None,
            Ok(_) => return Some(bytes[0]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                sleep(Duration::from_millis(5)).await
            }
            Err(e) => panic!("fixture read kind {:?}", e.kind()),
        }
    }
}
async fn ack(peer: &mut TcpStream) {
    loop {
        match peer.write(b"A") {
            Ok(1) => return,
            Ok(_) => panic!("fixture closed acknowledgement"),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                sleep(Duration::from_millis(5)).await
            }
            Err(e) => panic!("fixture write kind {:?}", e.kind()),
        }
    }
}

struct AckSink<'a> {
    socket: TcpListener,
    peer: Option<TcpStream>,
    observed: Vec<u8>,
    first_alive: bool,
    acknowledged: bool,
    reject_second: Option<SinkFailure>,
    original: &'a SnapshotContext,
    successful: u64,
}
impl IncrementalSink for AckSink<'_> {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        context: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(std::ptr::eq(context, self.original));
        assert_eq!(chunk.stream(), Stream::Stdout);
        let debug = format!("{chunk:?}");
        assert!(
            !debug.contains(&format!("{:?}", chunk.bytes())),
            "private byte representation leaked"
        );
        if self.peer.is_none() {
            let mut p = peer(&self.socket).await;
            assert_eq!(byte(&mut p).await, Some(b'L'));
            self.peer = Some(p);
        }
        self.observed.extend_from_slice(chunk.bytes());
        assert!(self.observed.len() <= FIRST.len() + SECOND.len()); // Sink-owned cap.
        if !self.acknowledged && self.observed.len() >= FIRST.len() {
            assert_eq!(&self.observed[..FIRST.len()], FIRST);
            // Child cannot produce SECOND/exit without this ACK. Completion7
            // therefore proves the first binary delivery occurred while alive.
            self.first_alive = true;
            ack(self.peer.as_mut().unwrap()).await;
            self.acknowledged = true;
        } else if let Some(error) = self.reject_second {
            return Err(error);
        }
        self.successful += chunk.bytes().len() as u64;
        Ok(())
    }
}
#[tokio::test(flavor = "current_thread")]
async fn tcp_ack_proves_binary_delivery_before_exit_without_retention() {
    let dir = TempDir::new().unwrap();
    let cancel = Cancellation::default();
    let context = original(&cancel, Duration::from_secs(5));
    let socket = listener();
    let address = socket.local_addr().unwrap().to_string();
    let mut sink = AckSink {
        socket,
        peer: None,
        observed: vec![],
        first_alive: false,
        acknowledged: false,
        reject_second: None,
        original: &context,
        successful: 0,
    };
    let result = execute_operation(
        request(dir.path(), &["incremental", &address]),
        &context,
        Retention::None,
        &mut sink,
    )
    .await
    .unwrap();
    assert_eq!(result.status.code(), Some(7));
    assert_owned_cleanup(result.cleanup);
    assert!(result.transcript.is_none());
    assert!(sink.first_alive);
    assert_eq!(sink.observed, [FIRST, SECOND].concat());
    assert_eq!(
        result.observed,
        Counts {
            stdout: 8,
            stderr: 0
        }
    );
    assert_eq!(
        result.delivered,
        Counts {
            stdout: 8,
            stderr: 0
        }
    );
    assert_eq!(result.pipes.stdout, PipeEnd::Eof);
}
#[tokio::test(flavor = "current_thread")]
async fn rejecting_after_real_prior_delivery_preserves_counts_and_accepted_transcript() {
    for failure in [SinkFailure::Closed, SinkFailure::ConsumerFailed] {
        for retention in [Retention::None, Retention::Full] {
            let dir = TempDir::new().unwrap();
            let context = original(&Cancellation::default(), Duration::from_secs(5));
            let socket = listener();
            let address = socket.local_addr().unwrap().to_string();
            let mut sink = AckSink {
                socket,
                peer: None,
                observed: vec![],
                first_alive: false,
                acknowledged: false,
                reject_second: Some(failure),
                original: &context,
                successful: 0,
            };
            let error = execute_operation(
                request(dir.path(), &["incremental", &address]),
                &context,
                retention,
                &mut sink,
            )
            .await
            .unwrap_err();
            assert_eq!(error.reason, Reason::Sink(failure));
            assert_owned_cleanup(error.cleanup);
            assert!(sink.first_alive);
            assert_eq!(error.delivered.stdout, sink.successful);
            assert_eq!(error.delivered.stdout, 4);
            assert!(error.observed.stdout >= 5 && error.observed.stdout <= 8);
            match retention {
                Retention::None => assert!(error.transcript.is_none()),
                Retention::Full => {
                    let bytes = error.transcript.as_ref().unwrap().stdout();
                    assert!(bytes.len() > 4 && bytes.len() <= 8);
                    assert_eq!(&bytes[..4], FIRST);
                    assert_eq!(&bytes[4..], &SECOND[..bytes.len() - 4]);
                }
            }
        }
    }
}
struct Active {
    active: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}
impl Drop for Active {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
struct OrderedSink {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}
impl IncrementalSink for OrderedSink {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        context: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        context.check().unwrap();
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        let _scope = Active {
            active: Arc::clone(&self.active),
            dropped: Arc::clone(&self.dropped),
        };
        tokio::task::yield_now().await;
        let bytes = match chunk.stream() {
            Stream::Stdout => &mut self.stdout,
            Stream::Stderr => &mut self.stderr,
        };
        assert!(bytes.len() + chunk.bytes().len() <= 32768);
        bytes.extend_from_slice(chunk.bytes());
        Ok(())
    }
}
#[tokio::test(flavor = "current_thread")]
async fn concurrent_binary_streams_preserve_order_with_one_serial_callback() {
    for retention in [Retention::None, Retention::Full] {
        let dir = TempDir::new().unwrap();
        let context = original(&Cancellation::default(), Duration::from_secs(5));
        let mut sink = OrderedSink {
            stdout: vec![],
            stderr: vec![],
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(AtomicUsize::new(0)),
        };
        let result = execute_operation(
            request(dir.path(), &["sequence", "32768"]),
            &context,
            retention,
            &mut sink,
        )
        .await
        .unwrap();
        let expected: Vec<_> = (0..32768).map(|index| (index % 251) as u8).collect();
        assert_eq!(sink.stdout, expected);
        assert_eq!(sink.stderr, expected);
        assert_eq!(
            result.observed,
            Counts {
                stdout: 32768,
                stderr: 32768
            }
        );
        assert_eq!(result.delivered, result.observed);
        assert_eq!(sink.peak.load(Ordering::SeqCst), 1);
        assert_eq!(sink.active.load(Ordering::SeqCst), 0);
        assert!(sink.dropped.load(Ordering::SeqCst) > 1);
        if let Some(transcript) = result.transcript {
            assert_eq!(transcript.stdout(), sink.stdout);
            assert_eq!(transcript.stderr(), sink.stderr);
        } else {
            assert_eq!(retention, Retention::None);
        }
    }
}
#[tokio::test(flavor = "current_thread")]
async fn exact_caps_cap_plus_one_and_zero_apply_even_with_no_transcript() {
    for retention in [Retention::None, Retention::Full] {
        for stream in [Stream::Stdout, Stream::Stderr] {
            for cap in [0, 8191, 8192] {
                let dir = TempDir::new().unwrap();
                let context = original(&Cancellation::default(), Duration::from_secs(5));
                let mut spec = request(dir.path(), &["flood", "8192"]);
                match stream {
                    Stream::Stdout => spec.limits.stdout_bytes = cap,
                    Stream::Stderr => spec.limits.stderr_bytes = cap,
                }
                let result = execute_operation(spec, &context, retention, &mut IgnoreChunks).await;
                if cap == 8192 {
                    let result = result.unwrap();
                    assert_eq!(
                        result.observed,
                        Counts {
                            stdout: 8192,
                            stderr: 8192
                        }
                    );
                    assert_eq!(result.delivered, result.observed);
                    assert_eq!(result.transcript.is_some(), retention == Retention::Full);
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.reason, Reason::OutputLimit(stream));
                    let observed = match stream {
                        Stream::Stdout => error.observed.stdout,
                        Stream::Stderr => error.observed.stderr,
                    };
                    let delivered = match stream {
                        Stream::Stdout => error.delivered.stdout,
                        Stream::Stderr => error.delivered.stderr,
                    };
                    assert_eq!(observed, cap as u64 + 1);
                    assert!(delivered <= cap as u64);
                    assert_owned_cleanup(error.cleanup);
                    if let Some(transcript) = error.transcript {
                        let bytes = match stream {
                            Stream::Stdout => transcript.stdout(),
                            Stream::Stderr => transcript.stderr(),
                        };
                        assert!(bytes.len() <= cap);
                        if cap == 0 {
                            assert!(bytes.is_empty());
                        }
                    } else {
                        assert_eq!(retention, Retention::None);
                    }
                }
            }
        }
    }
}
struct PendingSink {
    entered: Arc<Notify>,
    active: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}
impl IncrementalSink for PendingSink {
    async fn push<'a>(
        &'a mut self,
        _: Chunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        self.active.fetch_add(1, Ordering::SeqCst);
        let _scope = Active {
            active: Arc::clone(&self.active),
            dropped: Arc::clone(&self.dropped),
        };
        self.entered.notify_one();
        std::future::pending().await
    }
}
#[tokio::test(flavor = "current_thread")]
async fn original_cancel_drops_only_pending_sink_then_awaits_cleanup() {
    let dir = TempDir::new().unwrap();
    let cancel = Cancellation::default();
    let context = original(&cancel, Duration::from_secs(5));
    let socket = listener();
    let address = socket.local_addr().unwrap().to_string();
    let entered = Arc::new(Notify::new());
    let active = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut sink = PendingSink {
        entered: Arc::clone(&entered),
        active: Arc::clone(&active),
        dropped: Arc::clone(&dropped),
    };
    let run = execute_operation(
        request(dir.path(), &["incremental", &address]),
        &context,
        Retention::Full,
        &mut sink,
    );
    let cancellation = async {
        let mut child = peer(&socket).await;
        assert_eq!(byte(&mut child).await, Some(b'L'));
        entered.notified().await;
        assert_eq!(active.load(Ordering::SeqCst), 1);
        cancel.cancel();
    };
    let (result, ()) = timeout(Duration::from_secs(7), async {
        tokio::join!(run, cancellation)
    })
    .await
    .unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.reason, Reason::Cancelled);
    assert_owned_cleanup(error.cleanup);
    assert_eq!(error.delivered, Counts::default());
    assert!(error.observed.stdout > 0);
    assert!(!error.transcript.as_ref().unwrap().stdout().is_empty());
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}
#[tokio::test(flavor = "current_thread")]
async fn actual_read_readiness_precedes_shared_cancellation_without_callback() {
    let dir = TempDir::new().unwrap();
    let cancel = Cancellation::default();
    let context = original(&cancel, Duration::from_secs(5));
    let socket = listener();
    let address = socket.local_addr().unwrap().to_string();
    let mut sink = IgnoreChunks;
    let run = execute_operation(
        request(dir.path(), &["read-ready", &address]),
        &context,
        Retention::None,
        &mut sink,
    );
    let signal = async {
        let mut child = peer(&socket).await;
        assert_eq!(byte(&mut child).await, Some(b'L'));
        cancel.cancel();
        child
    };
    // Keep the control peer alive until native cancellation cleanup has finished.
    // Early EOF would panic the fixture's read_exact and create stderr payload.
    let (result, _peer) = tokio::join!(run, signal);
    let error = result.unwrap_err();
    assert_eq!(error.reason, Reason::Cancelled);
    assert_owned_cleanup(error.cleanup);
    assert_eq!(error.observed, Counts::default());
    assert_eq!(error.delivered, Counts::default());
    assert!(error.transcript.is_none());
}
struct ExitPendingSink {
    socket: TcpListener,
    peer_closed: bool,
    delay: Duration,
    entered: Arc<Notify>,
    dropped: Arc<AtomicUsize>,
}
impl IncrementalSink for ExitPendingSink {
    async fn push<'a>(
        &'a mut self,
        _: Chunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        let active = Arc::new(AtomicUsize::new(1));
        let _scope = Active {
            active,
            dropped: Arc::clone(&self.dropped),
        };
        if !self.peer_closed {
            let mut child = peer(&self.socket).await;
            assert_eq!(byte(&mut child).await, Some(b'L'));
            assert_eq!(byte(&mut child).await, None);
            self.peer_closed = true;
            self.entered.notify_one();
            sleep(self.delay).await;
        }
        Ok(())
    }
}
#[tokio::test(flavor = "current_thread")]
async fn leader_exit_keeps_pending_callback_and_delivers_remaining_output() {
    let dir = TempDir::new().unwrap();
    let context = original(&Cancellation::default(), Duration::from_secs(5));
    let socket = listener();
    let address = socket.local_addr().unwrap().to_string();
    let mut sink = ExitPendingSink {
        socket,
        peer_closed: false,
        delay: Duration::from_millis(100),
        entered: Arc::new(Notify::new()),
        dropped: Arc::new(AtomicUsize::new(0)),
    };
    let result = execute_operation(
        request(dir.path(), &["exit-output", &address]),
        &context,
        Retention::Full,
        &mut sink,
    )
    .await
    .unwrap();
    assert_eq!(result.status.code(), Some(7));
    assert_owned_cleanup(result.cleanup);
    assert_eq!(
        result.observed,
        Counts {
            stdout: 8,
            stderr: 0
        }
    );
    assert_eq!(result.delivered, result.observed);
    assert_eq!(
        result.transcript.as_ref().unwrap().stdout(),
        [FIRST, SECOND].concat()
    );
    assert_eq!(result.pipes.stdout, PipeEnd::Eof);
    assert!(sink.dropped.load(Ordering::SeqCst) > 0);
}
#[tokio::test(flavor = "current_thread")]
async fn normal_exit_drain_respects_original_deadline_and_keeps_observed_status() {
    let dir = TempDir::new().unwrap();
    let context = original(&Cancellation::default(), Duration::from_secs(2));
    let socket = listener();
    let address = socket.local_addr().unwrap().to_string();
    let mut spec = request(dir.path(), &["exit-output", &address]);
    spec.limits.cleanup = Duration::from_secs(5);
    let mut sink = ExitPendingSink {
        socket,
        peer_closed: false,
        delay: Duration::from_secs(20),
        entered: Arc::new(Notify::new()),
        dropped: Arc::new(AtomicUsize::new(0)),
    };
    let error = execute_operation(spec, &context, Retention::Full, &mut sink)
        .await
        .unwrap_err();
    assert_eq!(error.reason, Reason::TimedOut);
    assert_owned_cleanup(error.cleanup);
    assert_eq!(error.status.unwrap().code(), Some(7));
    assert_eq!(error.delivered, Counts::default());
    assert!(error.observed.stdout > 0);
    assert!(sink.dropped.load(Ordering::SeqCst) > 0);
}
#[tokio::test(flavor = "current_thread")]
async fn normal_exit_drain_cleanup_horizon_is_not_renewed_for_pending_sink() {
    let dir = TempDir::new().unwrap();
    let context = original(&Cancellation::default(), Duration::from_secs(5));
    let socket = listener();
    let address = socket.local_addr().unwrap().to_string();
    let mut spec = request(dir.path(), &["exit-output", &address]);
    spec.limits.cleanup = Duration::from_secs(1);
    let mut sink = ExitPendingSink {
        socket,
        peer_closed: false,
        delay: Duration::from_secs(20),
        entered: Arc::new(Notify::new()),
        dropped: Arc::new(AtomicUsize::new(0)),
    };
    let error = execute_operation(spec, &context, Retention::None, &mut sink)
        .await
        .unwrap_err();
    assert_eq!(error.reason, Reason::Cleanup);
    assert_eq!(error.cleanup, Cleanup::TimedOut);
    assert_eq!(error.status.unwrap().code(), Some(7));
    assert_eq!(error.delivered, Counts::default());
    assert!(error.observed.stdout > 0);
    assert!(error.transcript.is_none());
    assert!(context.check().is_ok());
    assert!(sink.dropped.load(Ordering::SeqCst) > 0);
}
#[tokio::test(flavor = "current_thread")]
async fn original_queued_deadline_and_precancel_deny_before_spawn() {
    let host = host_fixture::install(8).await;
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("must-not-spawn");
    let marker = marker.to_str().unwrap();
    let cancel = Cancellation::default();
    let context = original(&cancel, Duration::from_secs(1));
    let (_stop, receiver) = graphit_process::operation(&context, &host).unwrap();
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        context.required_deadline().unwrap(),
    ))
    .await;
    let error = run(
        request(dir.path(), &["ready", marker]),
        &context,
        Retention::None,
        &mut IgnoreChunks,
        receiver,
    )
    .await
    .unwrap_err();
    assert_eq!(error.reason, Reason::TimedOut);
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    assert_eq!(error.observed, Counts::default());
    assert_eq!(error.pipes.stdout, PipeEnd::NotSpawned);
    assert!(!Path::new(marker).exists());
    let cancel = Cancellation::default();
    let context = original(&cancel, Duration::from_secs(5));
    let (_stop, receiver) = graphit_process::operation(&context, &host).unwrap();
    cancel.cancel();
    let error = run(
        request(dir.path(), &["ready", marker]),
        &context,
        Retention::Full,
        &mut IgnoreChunks,
        receiver,
    )
    .await
    .unwrap_err();
    assert_eq!(error.reason, Reason::Cancelled);
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    assert!(error.transcript.as_ref().unwrap().stdout().is_empty());
    assert!(!Path::new(marker).exists());
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn finite_binary_stdin_null_early_close_and_duplex_share_none_full_engine() {
    for retention in [Retention::None, Retention::Full] {
        for mode in ["null", "echo", "close-input", "duplex"] {
            let dir = TempDir::new().unwrap();
            let context = original(&Cancellation::default(), Duration::from_secs(5));
            let fixture = if mode == "null" { "echo" } else { mode };
            let mut spec = request(dir.path(), &[fixture]);
            spec.limits.stdout_bytes = 256 * 1024;
            spec.limits.stderr_bytes = 256 * 1024;
            match mode {
                "echo" => spec.stdin = Some(vec![0, 255, 128, b'x'].into()),
                "close-input" | "duplex" => spec.stdin = Some(vec![b'x'; 512 * 1024].into()),
                _ => {
                    spec.limits.stdout_bytes = 0;
                    spec.limits.stderr_bytes = 0;
                }
            }
            let result = execute_operation(spec, &context, retention, &mut IgnoreChunks)
                .await
                .unwrap();
            let expected = match mode {
                "null" => Counts::default(),
                "echo" => Counts {
                    stdout: 4,
                    stderr: 0,
                },
                "close-input" => Counts {
                    stdout: 7,
                    stderr: 0,
                },
                _ => Counts {
                    stdout: 256 * 1024,
                    stderr: 256 * 1024,
                },
            };
            assert!(result.status.success());
            assert_eq!(result.observed, expected);
            assert_eq!(result.delivered, expected);
            assert_eq!(result.transcript.is_some(), retention == Retention::Full);
            if let Some(transcript) = result.transcript {
                assert_eq!(transcript.stdout().len() as u64, expected.stdout);
                assert_eq!(transcript.stderr().len() as u64, expected.stderr);
                if mode == "echo" {
                    assert_eq!(transcript.stdout(), &[0, 255, 128, b'x']);
                }
            }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn normal_leader_exit_terminates_term_ignoring_descendant_and_finalizes_pipes() {
    let dir = TempDir::new().unwrap();
    let heartbeat = dir.path().join("descendant-after-leader");
    let context = original(&Cancellation::default(), Duration::from_secs(5));
    let result = execute_operation(
        request(dir.path(), &["tree-exit", heartbeat.to_str().unwrap()]),
        &context,
        Retention::None,
        &mut IgnoreChunks,
    )
    .await
    .unwrap();
    assert_eq!(result.status.code(), Some(7));
    assert_owned_cleanup(result.cleanup);
    assert_eq!(result.pipes.stdout, PipeEnd::Eof);
    assert_eq!(result.pipes.stderr, PipeEnd::Eof);
    assert!(result.observed.stdout > 0);
    let before = std::fs::read(&heartbeat).unwrap();
    sleep(Duration::from_millis(100)).await;
    assert_eq!(
        std::fs::read(&heartbeat).unwrap(),
        before,
        "owned descendant still updates heartbeat"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cleanup_requires_positive_remainder_but_zero_grace_remains_valid() {
    let dir = TempDir::new().unwrap();
    let context = original(&Cancellation::default(), Duration::from_secs(5));
    let mut spec = request(dir.path(), &["echo"]);
    spec.limits.termination_grace = Duration::ZERO;
    assert!(
        execute_operation(spec, &context, Retention::None, &mut IgnoreChunks)
            .await
            .unwrap()
            .status
            .success()
    );
    let marker = dir.path().join("invalid-grace-must-not-spawn");
    let mut spec = request(dir.path(), &["ready", marker.to_str().unwrap()]);
    spec.limits.termination_grace = spec.limits.cleanup;
    let error = execute_operation(spec, &context, Retention::None, &mut IgnoreChunks)
        .await
        .unwrap_err();
    assert_eq!(
        error.reason,
        Reason::Invalid(graphit_process::InvalidRequest::Budget)
    );
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    assert_eq!(error.observed, Counts::default());
    assert!(!marker.exists());
}

async fn execute_operation<S: graphit_process::IncrementalSink>(
    request: Request,
    original: &std::sync::Arc<SnapshotContext>,
    retention: Retention,
    sink: &mut S,
) -> Result<graphit_process::Outcome, graphit_process::Failure> {
    let host = host_fixture::install(8).await;
    let (stop, receiver) = graphit_process::operation(original, &host).unwrap();
    let result = run(request, original, retention, sink, receiver).await;
    drop(stop);
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
    result
}
