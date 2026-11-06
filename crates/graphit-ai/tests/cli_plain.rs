use crate::cli_control as control;
use graphit_ai::cli_plain::{
    PlainFailure, PlainLimit, PlainLimits, PlainReason, PlainReceipt, TextChunk, TextSink,
    run_plain,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Cleanup, Failure, HostLimits, HostReadiness, HostReport, HostShutdown, HostShutdownObservation,
    Limits, OperationAdmissionFailure, Outcome, PipeEnd, ProcessHost, Reason, RepairObservation,
    Request, SinkFailure, Stream, operation,
};
use secrecy::zeroize::Zeroizing;
use std::{
    ffi::OsString,
    io,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio::{io::AsyncReadExt, sync::Notify, time::sleep};

struct OwnedFixture {
    root: Option<TempDir>,
}

impl OwnedFixture {
    fn new() -> Self {
        Self {
            root: Some(TempDir::new().unwrap()),
        }
    }

    fn path(&self) -> &Path {
        self.root.as_ref().unwrap().path()
    }

    fn finish(mut self, receipt: &PlainReceipt, closure: &FixtureClosure) {
        let historical = match receipt.process() {
            Some(Ok(outcome)) => Some(outcome.cleanup),
            Some(Err(failure)) => Some(failure.cleanup),
            None => None,
        };
        let cleanup = match closure.remaining {
            Some(RepairObservation::Observed { cleanup, .. }) => Some(cleanup),
            Some(RepairObservation::Pending { .. }) => None,
            None => historical,
        };
        let root = self.root.take().unwrap();
        if cleanup == Some(Cleanup::Complete) && closure.host.failure().is_none() {
            let path = root.path().to_path_buf();
            assert!(root.close().is_ok(), "owned fixture removal failed");
            assert!(
                !path.exists(),
                "owned fixture remains after complete cleanup"
            );
        } else {
            let path = root.keep();
            assert!(path.is_dir(), "owned fixture retention failed");
        }
    }
}

impl Drop for OwnedFixture {
    fn drop(&mut self) {
        if let Some(root) = self.root.take() {
            let _path = root.keep();
        }
    }
}

#[test]
fn owned_fixture_retains_on_unwind_and_unclassified_future_drop() {
    let fixture = OwnedFixture::new();
    let path = fixture.path().to_path_buf();
    let result = std::panic::catch_unwind(move || {
        let _fixture = fixture;
        panic!("fixture-only unwind");
    });
    assert!(result.is_err());
    assert!(path.is_dir(), "fixture lost during unwind");

    let fixture = OwnedFixture::new();
    let path = fixture.path().to_path_buf();
    let unclassified = async move {
        let _fixture = fixture;
        std::future::pending::<()>().await;
    };
    drop(unclassified);
    assert!(path.is_dir(), "fixture lost without terminal evidence");
}

fn original(cancellation: &Cancellation, budget: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + budget,
        cancellation.clone(),
    ))
}

struct FixtureClosure {
    host: HostReport,
    remaining: Option<RepairObservation>,
}

struct FixtureHost {
    service: Cancellation,
    host: ProcessHost,
    readiness: HostReadiness,
}

impl FixtureHost {
    async fn new() -> Self {
        let service = Cancellation::default();
        let host_limits = HostLimits::new(1, 1).unwrap();
        let host = ProcessHost::install(&service, host_limits).unwrap();
        let readiness = host.ready(Instant::now() + Duration::from_secs(5)).await;
        Self {
            service,
            host,
            readiness,
        }
    }

    async fn finish(self, result: &mut Result<PlainReceipt, PlainFailure>) -> FixtureClosure {
        let receipt = match result {
            Ok(receipt) => receipt,
            Err(failure) => failure.receipt_mut(),
        };
        let remaining = match receipt.process_mut() {
            Some(Ok(outcome)) => outcome.remaining.as_mut(),
            Some(Err(failure)) => failure.remaining.as_mut(),
            None => None,
        };
        let observation = if let Some(same) = remaining {
            Some(same.repair(Instant::now() + Duration::from_secs(5)).await)
        } else {
            None
        };
        let mut late = matches!(observation, Some(RepairObservation::Pending { .. }));
        let report = match self
            .host
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
        {
            HostShutdown::Joined(report) => report,
            HostShutdown::Pending(mut same) => {
                late = true;
                match same.repair(Instant::now() + Duration::from_secs(5)).await {
                    HostShutdownObservation::Joined(report) => report,
                    HostShutdownObservation::Pending => panic!("process host did not shut down"),
                }
            }
        };
        assert!(matches!(self.readiness, HostReadiness::Ready));
        assert!(report.failure().is_none());
        assert!(
            !late,
            "native fixture closure exceeded its observation deadline"
        );
        FixtureClosure {
            host: report,
            remaining: observation,
        }
    }
}

fn policy() -> PlainLimits {
    PlainLimits {
        stdout_text_bytes: 32_768,
        stderr_text_bytes: 32_768,
        events: 64,
    }
}

fn native(dir: &Path, args: Vec<OsString>) -> Request {
    Request {
        program: PathBuf::from(env!("CARGO_BIN_EXE_ai-cli-plain-fixture")),
        args,
        cwd: dir.to_path_buf(),
        environment: std::env::vars_os()
            .filter(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT"))
            .collect(),
        stdin: None,
        limits: Limits {
            stdin_bytes: 1024,
            stdout_bytes: 32_768,
            stderr_bytes: 32_768,
            termination_grace: Duration::from_millis(15),
            cleanup: Duration::from_secs(1),
        },
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sequence(dir: &Path, socket: &TcpListener, exit: i32, writes: &[(Stream, &[u8])]) -> Request {
    let mut args = vec![
        "sequence".into(),
        socket.local_addr().unwrap().to_string().into(),
        exit.to_string().into(),
    ];
    for (stream, bytes) in writes {
        args.push(
            match stream {
                Stream::Stdout => "out",
                Stream::Stderr => "err",
            }
            .into(),
        );
        args.push(hex(bytes).into());
    }
    native(dir, args)
}

fn mode(dir: &Path, socket: &TcpListener, name: &str, stream: Stream, bytes: &[u8]) -> Request {
    native(
        dir,
        vec![
            name.into(),
            socket.local_addr().unwrap().to_string().into(),
            match stream {
                Stream::Stdout => "out",
                Stream::Stderr => "err",
            }
            .into(),
            hex(bytes).into(),
        ],
    )
}

fn socket() -> TcpListener {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    socket.set_nonblocking(true).unwrap();
    socket
}

async fn peer(socket: &TcpListener, context: &SnapshotContext) -> Result<TcpStream, SinkFailure> {
    control::peer(socket, context)
        .await
        .map_err(|_| SinkFailure::ConsumerFailed)
}

async fn byte(peer: &mut TcpStream, context: &SnapshotContext) -> Result<Option<u8>, SinkFailure> {
    control::byte(peer, context)
        .await
        .map_err(|_| SinkFailure::ConsumerFailed)
}

async fn acknowledge(peer: &mut TcpStream, context: &SnapshotContext) -> Result<(), SinkFailure> {
    control::acknowledge(peer, context).await.map_err(|error| {
        if matches!(
            error.kind(),
            io::ErrorKind::WriteZero | io::ErrorKind::BrokenPipe
        ) {
            SinkFailure::Closed
        } else {
            SinkFailure::ConsumerFailed
        }
    })
}

struct Collector<'a> {
    listener: TcpListener,
    peer: Option<TcpStream>,
    context: &'a SnapshotContext,
    stdout: String,
    stderr: String,
    events: u64,
    ack_remaining: u64,
    first_alive: bool,
}

impl<'a> Collector<'a> {
    fn new(listener: TcpListener, context: &'a SnapshotContext, ack: bool) -> Self {
        Self {
            listener,
            peer: None,
            context,
            stdout: String::new(),
            stderr: String::new(),
            events: 0,
            ack_remaining: if ack { 64 } else { 0 },
            first_alive: false,
        }
    }
}

impl TextSink for Collector<'_> {
    async fn push<'a>(
        &'a mut self,
        chunk: TextChunk<'a>,
        context: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(std::ptr::eq(context, self.context));
        let next = self
            .stdout
            .len()
            .checked_add(self.stderr.len())
            .and_then(|n| n.checked_add(chunk.text().len()))
            .ok_or(SinkFailure::ConsumerFailed)?;
        if next > 65_536 || self.events >= 64 {
            return Err(SinkFailure::ConsumerFailed);
        }
        if self.peer.is_none() {
            self.peer = Some(peer(&self.listener, context).await?);
        }
        let connection = self.peer.as_mut().ok_or(SinkFailure::Closed)?;
        if self.ack_remaining != 0 {
            // One-byte ASCII writes make this handshake deterministic for caps.
            if byte(connection, context).await? != Some(b'L') {
                return Err(SinkFailure::ConsumerFailed);
            }
            self.first_alive = true;
        }
        match chunk.stream() {
            Stream::Stdout => self.stdout.push_str(chunk.text()),
            Stream::Stderr => self.stderr.push_str(chunk.text()),
        }
        self.events += 1;
        if self.ack_remaining != 0 {
            acknowledge(connection, context).await?;
            self.ack_remaining -= 1;
        }
        Ok(())
    }
}

fn assert_owned_cleanup(cleanup: Cleanup) {
    assert_eq!(cleanup, Cleanup::Complete);
}

fn finished(receipt: &PlainReceipt) -> &Outcome {
    let outcome = receipt.process().unwrap().as_ref().unwrap();
    assert_owned_cleanup(outcome.cleanup);
    assert_eq!(
        (outcome.pipes.stdout, outcome.pipes.stderr),
        (PipeEnd::Eof, PipeEnd::Eof)
    );
    assert!(outcome.transcript.is_none());
    outcome
}

fn failed(receipt: &PlainReceipt) -> &Failure {
    let failure = receipt.process().unwrap().as_ref().unwrap_err();
    assert_owned_cleanup(failure.cleanup);
    assert!(failure.transcript.is_none());
    failure
}

#[tokio::test]
async fn text_without_newline_arrives_before_native_exit_and_stderr_stays_separate() {
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let listener = socket();
    let dir = OwnedFixture::new();
    let request = sequence(
        dir.path(),
        &listener,
        0,
        &[
            (Stream::Stdout, b"A"),
            (Stream::Stderr, b"E"),
            (Stream::Stdout, b"B"),
        ],
    );
    let mut collector = Collector::new(listener, &context, true);

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let receipt = result.unwrap();
    assert!(collector.first_alive);
    assert_eq!(collector.stdout, "AB");
    assert_eq!(collector.stderr, "E");
    assert_eq!(receipt.text(), collector.stdout);
    assert_eq!(
        (
            receipt.accepted().stdout,
            receipt.accepted().stderr,
            receipt.events()
        ),
        (2, 1, 3)
    );
    let outcome = finished(&receipt);
    assert!(outcome.status.success());
    assert_eq!((outcome.delivered.stdout, outcome.delivered.stderr), (2, 1));
    dir.finish(&receipt, &closure);
}

#[tokio::test]
async fn real_utf8_output_is_exact_without_claiming_os_read_boundaries() {
    for scalar in ["¢", "€", "🦀"] {
        for offset in 1..scalar.len() {
            let context = original(&Cancellation::default(), Duration::from_secs(30));
            let listener = socket();
            let first = [b"A".as_slice(), &scalar.as_bytes()[..offset]].concat();
            let second = [&scalar.as_bytes()[offset..], b"Z".as_slice()].concat();
            let dir = OwnedFixture::new();
            let request = native(
                dir.path(),
                vec![
                    "scalar".into(),
                    listener.local_addr().unwrap().to_string().into(),
                    hex(&first).into(),
                    hex(&second).into(),
                ],
            );
            let mut collector = Collector::new(listener, &context, true);
            collector.ack_remaining = 1;

            let fixture_host = FixtureHost::new().await;
            let mut result = run_plain(
                &context,
                &fixture_host.host,
                request,
                policy(),
                &mut collector,
            )
            .await;
            let closure = fixture_host.finish(&mut result).await;
            let receipt = result.unwrap();
            assert!(collector.first_alive);
            assert_eq!(receipt.text(), format!("A{scalar}Z"));
            assert_eq!(receipt.text(), collector.stdout);
            assert!(finished(&receipt).status.success());
            dir.finish(&receipt, &closure);
        }
    }
}

#[tokio::test]
async fn whitespace_crlf_and_final_text_without_newline_are_not_normalized() {
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let listener = socket();
    let expected = " \tA\n\n  B\r\nZ ";
    let dir = OwnedFixture::new();
    let request = mode(
        dir.path(),
        &listener,
        "raw",
        Stream::Stdout,
        expected.as_bytes(),
    );
    let mut collector = Collector::new(listener, &context, false);

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let receipt = result.unwrap();
    assert_eq!(receipt.text(), expected);
    assert_eq!(receipt.text(), collector.stdout);
    assert!(finished(&receipt).status.success());
    dir.finish(&receipt, &closure);
}

#[tokio::test]
async fn an_invalid_later_chunk_keeps_only_previously_accepted_text() {
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let listener = socket();
    let dir = OwnedFixture::new();
    let request = sequence(
        dir.path(),
        &listener,
        0,
        &[(Stream::Stdout, b"A"), (Stream::Stdout, &[0xff])],
    );
    let mut collector = Collector::new(listener, &context, true);

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let error = result.unwrap_err();
    assert_eq!(error.reason(), PlainReason::InvalidUtf8(Stream::Stdout));
    assert_eq!((error.receipt().text(), error.receipt().events()), ("A", 1));
    assert_eq!(failed(error.receipt()).delivered.stdout, 1);
    dir.finish(error.receipt(), &closure);
}

#[tokio::test]
async fn invalid_utf8_awaits_cleanup_and_incomplete_eof_preserves_real_outcome() {
    for stream in [Stream::Stdout, Stream::Stderr] {
        let context = original(&Cancellation::default(), Duration::from_secs(30));
        let listener = socket();
        let dir = OwnedFixture::new();
        let request = mode(dir.path(), &listener, "raw", stream, &[0xff]);
        let mut collector = Collector::new(listener, &context, false);

        let fixture_host = FixtureHost::new().await;
        let mut result = run_plain(
            &context,
            &fixture_host.host,
            request,
            policy(),
            &mut collector,
        )
        .await;
        let closure = fixture_host.finish(&mut result).await;
        let error = result.unwrap_err();
        assert_eq!(error.reason(), PlainReason::InvalidUtf8(stream));
        assert_eq!(
            failed(error.receipt()).reason,
            Reason::Sink(SinkFailure::ConsumerFailed)
        );
        assert_eq!(error.receipt().events(), 0);
        dir.finish(error.receipt(), &closure);

        let context = original(&Cancellation::default(), Duration::from_secs(30));
        let listener = socket();
        let dir = OwnedFixture::new();
        let request = mode(dir.path(), &listener, "raw", stream, &[0xe2, 0x82]);
        let mut collector = Collector::new(listener, &context, false);

        let fixture_host = FixtureHost::new().await;
        let mut result = run_plain(
            &context,
            &fixture_host.host,
            request,
            policy(),
            &mut collector,
        )
        .await;
        let closure = fixture_host.finish(&mut result).await;
        let error = result.unwrap_err();
        assert_eq!(error.reason(), PlainReason::IncompleteUtf8(stream));
        let outcome = finished(error.receipt());
        assert!(outcome.status.success());
        let raw = match stream {
            Stream::Stdout => outcome.delivered.stdout,
            Stream::Stderr => outcome.delivered.stderr,
        };
        assert_eq!(raw, 2);
        assert_eq!(
            (
                error.receipt().accepted().stdout,
                error.receipt().accepted().stderr,
                error.receipt().events()
            ),
            (0, 0, 0)
        );
        dir.finish(error.receipt(), &closure);
    }
}

#[tokio::test]
async fn exact_text_and_event_caps_pass_and_the_next_byte_is_rejected() {
    for stream in [Stream::Stdout, Stream::Stderr] {
        for limit_events in [false, true] {
            for exact in [true, false] {
                let context = original(&Cancellation::default(), Duration::from_secs(30));
                let listener = socket();
                let dir = OwnedFixture::new();
                let request = sequence(dir.path(), &listener, 0, &[(stream, b"A"), (stream, b"B")]);
                let mut collector = Collector::new(listener, &context, true);
                let mut limits = policy();
                if limit_events {
                    limits.events = if exact { 2 } else { 1 };
                } else {
                    let cap = if exact { 2 } else { 1 };
                    match stream {
                        Stream::Stdout => limits.stdout_text_bytes = cap,
                        Stream::Stderr => limits.stderr_text_bytes = cap,
                    }
                }

                let fixture_host = FixtureHost::new().await;
                let mut result = run_plain(
                    &context,
                    &fixture_host.host,
                    request,
                    limits,
                    &mut collector,
                )
                .await;
                let closure = fixture_host.finish(&mut result).await;
                if exact {
                    let receipt = result.unwrap();
                    assert_eq!(receipt.events(), 2);
                    assert!(finished(&receipt).status.success());
                    dir.finish(&receipt, &closure);
                } else {
                    let error = result.unwrap_err();
                    let bound = if limit_events {
                        PlainLimit::Events
                    } else {
                        match stream {
                            Stream::Stdout => PlainLimit::Stdout,
                            Stream::Stderr => PlainLimit::Stderr,
                        }
                    };
                    assert_eq!(error.reason(), PlainReason::Limit(bound));
                    assert_eq!(error.receipt().events(), 1);
                    assert_eq!(
                        failed(error.receipt()).reason,
                        Reason::Sink(SinkFailure::ConsumerFailed)
                    );
                    assert_eq!(collector.events, 1);
                    dir.finish(error.receipt(), &closure);
                }
            }
        }
    }
}

#[tokio::test]
async fn invalid_policy_precancel_preexpiry_and_native_preflight_spawn_no_child() {
    for invalid in [
        PlainLimits {
            stdout_text_bytes: 0,
            ..policy()
        },
        PlainLimits {
            stderr_text_bytes: 0,
            ..policy()
        },
        PlainLimits {
            events: 0,
            ..policy()
        },
        PlainLimits {
            stdout_text_bytes: usize::MAX,
            ..policy()
        },
    ] {
        let context = original(&Cancellation::default(), Duration::from_secs(30));
        let listener = socket();
        let dir = OwnedFixture::new();
        let request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
        let mut collector = Collector::new(listener, &context, false);

        let fixture_host = FixtureHost::new().await;
        let mut result = run_plain(
            &context,
            &fixture_host.host,
            request,
            invalid,
            &mut collector,
        )
        .await;
        let closure = fixture_host.finish(&mut result).await;
        let error = result.unwrap_err();
        assert_eq!(error.reason(), PlainReason::Policy);
        assert!(error.receipt().process().is_none());
        assert_eq!(collector.events, 0);
        assert!(
            matches!(collector.listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock)
        );
        dir.finish(error.receipt(), &closure);
    }
    for canceled in [true, false] {
        let cancellation = Cancellation::default();
        // Capture the real native domain while this SAME original is still valid.
        let context = original(
            &cancellation,
            if canceled {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(200)
            },
        );
        let listener = socket();
        let dir = OwnedFixture::new();
        let request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
        let mut collector = Collector::new(listener, &context, false);

        let fixture_host = FixtureHost::new().await;
        if canceled {
            cancellation.cancel();
        } else {
            tokio::time::sleep_until(context.deadline().expect("finite fixture deadline").into())
                .await;
        }
        let mut result = run_plain(
            &context,
            &fixture_host.host,
            request,
            policy(),
            &mut collector,
        )
        .await;
        let closure = fixture_host.finish(&mut result).await;
        let error = result.unwrap_err();
        assert!(matches!(error.reason(), PlainReason::Context(_)));
        assert!(error.receipt().process().is_none());
        assert_eq!(collector.events, 0);
        assert!(
            matches!(collector.listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock)
        );
        dir.finish(error.receipt(), &closure);
    }
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let listener = socket();
    let dir = OwnedFixture::new();
    let mut request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
    request.cwd = PathBuf::from("relative-denied");
    let mut collector = Collector::new(listener, &context, false);

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let error = result.unwrap_err();
    let actual = error.receipt().process().unwrap().as_ref().unwrap_err();
    assert_eq!(actual.cleanup, Cleanup::NotSpawned);
    assert!(matches!(actual.reason, Reason::Invalid(_)));
    assert!(actual.status.is_none());
    dir.finish(error.receipt(), &closure);
}

#[tokio::test]
async fn real_service_cancellation_seals_host_before_plain_admission() {
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let fixture_host = FixtureHost::new().await;
    let listener = socket();
    let dir = OwnedFixture::new();
    let request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
    let mut collector = Collector::new(listener, &context, false);
    fixture_host.service.cancel();
    // Wait for actual host sealing after requesting service cancellation.
    let mut observed = fixture_host.host.ready(Instant::now()).await;
    while matches!(observed, HostReadiness::Ready | HostReadiness::Pending)
        && Instant::now() < context.deadline().expect("finite fixture deadline")
    {
        sleep(Duration::from_millis(1)).await;
        observed = fixture_host.host.ready(Instant::now()).await;
    }
    let original_valid = context.check();
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let error = result.unwrap_err();
    assert!(matches!(observed, HostReadiness::Sealed));
    assert!(
        original_valid.is_ok(),
        "service cancellation changed the operation original"
    );
    assert_eq!(
        error.reason(),
        PlainReason::Admission(OperationAdmissionFailure::HostSealed)
    );
    assert!(error.receipt().process().is_none());
    assert_eq!(collector.events, 0);
    assert!(matches!(collector.listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock));
    dir.finish(error.receipt(), &closure);
}

#[tokio::test]
async fn real_host_capacity_denial_keeps_typed_admission_and_no_process_terminal() {
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let fixture_host = FixtureHost::new().await;
    let held = operation(&context, &fixture_host.host).unwrap();
    let listener = socket();
    let dir = OwnedFixture::new();
    let request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
    let mut collector = Collector::new(listener, &context, false);
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    drop(held); // The actual reserved pair had no submitted native driver.
    let closure = fixture_host.finish(&mut result).await;
    let error = result.unwrap_err();
    assert_eq!(
        error.reason(),
        PlainReason::Admission(OperationAdmissionFailure::CapacityFull)
    );
    assert!(error.receipt().process().is_none());
    assert_eq!(collector.events, 0);
    assert!(matches!(collector.listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock));
    dir.finish(error.receipt(), &closure);
}

#[tokio::test]
async fn captured_native_cwd_literal_argv_env_input_and_nonzero_status_are_real() {
    let dir = OwnedFixture::new();
    let cwd = std::fs::canonicalize(dir.path()).unwrap();
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let listener = socket();
    let mut request = native(
        &cwd,
        vec![
            "capture".into(),
            listener.local_addr().unwrap().to_string().into(),
            cwd.clone().into_os_string(),
            "literal;$(fixture)".into(),
        ],
    );
    request.environment.push((
        "AI_PLAIN_FIXTURE_VALUE".into(),
        "CANARY-CLI-PLAIN-PRIVATE".into(),
    ));
    request.stdin = Some(Zeroizing::new(b"CANARY-CLI-PLAIN-PRIVATE".to_vec()));
    let request_debug = format!("{request:?}");
    assert!(!request_debug.contains("CANARY-CLI-PLAIN-PRIVATE"));
    let mut collector = Collector::new(listener, &context, true);

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let receipt = result.unwrap();
    assert_eq!(receipt.text(), "A");
    assert!(finished(&receipt).status.success());
    assert!(!format!("{receipt:?}").contains("CANARY-CLI-PLAIN-PRIVATE"));
    dir.finish(&receipt, &closure);

    let listener = socket();
    let dir = OwnedFixture::new();
    let request = sequence(dir.path(), &listener, 7, &[(Stream::Stdout, b"A")]);
    let mut collector = Collector::new(listener, &context, true);

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let receipt = result.unwrap();
    assert_eq!(finished(&receipt).status.code(), Some(7));
    assert_eq!(receipt.text(), "A");
    dir.finish(&receipt, &closure);
}

struct Reject {
    category: SinkFailure,
    calls: u64,
}
impl TextSink for Reject {
    async fn push<'a>(
        &'a mut self,
        chunk: TextChunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(!format!("{chunk:?}").contains("CANARY-CLI-PLAIN-PRIVATE"));
        self.calls += 1;
        Err(self.category)
    }
}

#[tokio::test]
async fn closed_and_failed_consumers_preserve_process_failure_and_safe_diagnostics() {
    for category in [SinkFailure::Closed, SinkFailure::ConsumerFailed] {
        let context = original(&Cancellation::default(), Duration::from_secs(30));
        let listener = socket();
        let dir = OwnedFixture::new();
        let request = mode(
            dir.path(),
            &listener,
            "raw",
            Stream::Stdout,
            b"CANARY-CLI-PLAIN-PRIVATE",
        );
        let mut sink = Reject { category, calls: 0 };

        let fixture_host = FixtureHost::new().await;
        let mut result =
            run_plain(&context, &fixture_host.host, request, policy(), &mut sink).await;
        let closure = fixture_host.finish(&mut result).await;
        let error = result.unwrap_err();
        assert_eq!(error.reason(), PlainReason::Sink(category));
        assert_eq!(failed(error.receipt()).reason, Reason::Sink(category));
        assert_eq!((sink.calls, error.receipt().events()), (1, 0));
        assert!(!format!("{error:?} {error}").contains("CANARY-CLI-PLAIN-PRIVATE"));
        dir.finish(error.receipt(), &closure);
    }
}

struct DropObservation(Arc<AtomicUsize>);
impl Drop for DropObservation {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct Pending {
    entered: Arc<Notify>,
    dropped: Arc<AtomicUsize>,
    calls: u64,
}
impl TextSink for Pending {
    async fn push<'a>(
        &'a mut self,
        _: TextChunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        self.calls += 1;
        let _observation = DropObservation(self.dropped.clone());
        self.entered.notify_one();
        std::future::pending::<Result<(), SinkFailure>>().await
    }
}

#[tokio::test]
async fn original_cancel_and_deadline_drop_pending_callback_then_await_real_cleanup() {
    for cancel in [true, false] {
        let cancellation = Cancellation::default();
        let context = original(
            &cancellation,
            if cancel {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(300)
            },
        );
        let listener = socket();
        let dir = OwnedFixture::new();
        let request = mode(dir.path(), &listener, "hold", Stream::Stdout, b"");
        let entered = Arc::new(Notify::new());
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut sink = Pending {
            entered: entered.clone(),
            dropped: dropped.clone(),
            calls: 0,
        };
        // The readiness observer and invocation are joined; no control work detaches.
        let observe = async {
            let deadline = tokio::time::Instant::from_std(
                context.deadline().expect("finite fixture deadline"),
            );
            tokio::select! {
                () = entered.notified() => {},
                () = tokio::time::sleep_until(deadline) => return None,
            }
            let mut connection = peer(&listener, &context).await.ok()?;
            let alive = byte(&mut connection, &context).await == Ok(Some(b'L'));
            if cancel && alive {
                cancellation.cancel();
            }
            Some((connection, alive))
        };
        let fixture_host = FixtureHost::new().await;
        let (mut result, observation) = tokio::join!(
            run_plain(&context, &fixture_host.host, request, policy(), &mut sink),
            observe
        );
        let closure = fixture_host.finish(&mut result).await;
        let error = result.unwrap_err();
        let (mut connection, alive) =
            observation.expect("native callback readiness was not observed");
        assert!(alive);
        let actual = failed(error.receipt());
        assert_eq!(
            actual.reason,
            if cancel {
                Reason::Cancelled
            } else {
                Reason::TimedOut
            }
        );
        assert_eq!(
            (
                sink.calls,
                dropped.load(Ordering::SeqCst),
                error.receipt().events()
            ),
            (1, 1, 0)
        );
        assert!(actual.status.is_some());
        let closed = tokio::time::timeout(Duration::from_secs(2), connection.read(&mut [0]))
            .await
            .expect("native fixture connection must close after cleanup");
        match closed {
            Ok(0) => (),
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => (),
            other => panic!("expected EOF or reset from the terminated fixture, got {other:?}"),
        }
        dir.finish(error.receipt(), &closure);
    }
}

struct AcceptThenCancel<'a> {
    cancellation: &'a Cancellation,
}

struct ExitBeforeAccept<'a> {
    listener: TcpListener,
    context: &'a SnapshotContext,
    accepted: bool,
    observed_control_eof: bool,
}
impl TextSink for ExitBeforeAccept<'_> {
    async fn push<'a>(
        &'a mut self,
        chunk: TextChunk<'a>,
        context: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(std::ptr::eq(context, self.context));
        assert_eq!((chunk.stream(), chunk.text()), (Stream::Stdout, "A"));
        let mut connection = peer(&self.listener, context).await?;
        assert_eq!(byte(&mut connection, context).await?, Some(b'L'));
        assert_eq!(byte(&mut connection, context).await?, None);
        self.observed_control_eof = true;
        // Control EOF is the fixture's exit signal. The receipt below still
        // requires the real native terminal after the callback accepts.
        self.accepted = true;
        Ok(())
    }
}

#[tokio::test]
async fn callback_accepts_after_control_eof_and_awaits_the_native_terminal() {
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let listener = socket();
    let dir = OwnedFixture::new();
    let request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
    let mut sink = ExitBeforeAccept {
        listener,
        context: &context,
        accepted: false,
        observed_control_eof: false,
    };

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(&context, &fixture_host.host, request, policy(), &mut sink).await;
    let closure = fixture_host.finish(&mut result).await;
    let receipt = result.unwrap();
    assert!(sink.accepted && sink.observed_control_eof);
    assert_eq!(
        (receipt.text(), receipt.events(), receipt.accepted().stdout),
        ("A", 1, 1)
    );
    assert_eq!(finished(&receipt).delivered.stdout, 1);
    assert!(finished(&receipt).status.success());
    dir.finish(&receipt, &closure);
}
impl TextSink for AcceptThenCancel<'_> {
    async fn push<'a>(
        &'a mut self,
        _: TextChunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        self.cancellation.cancel();
        Ok(())
    }
}

#[tokio::test]
async fn accepted_text_before_postcallback_context_failure_remains_truthful() {
    let cancellation = Cancellation::default();
    let context = original(&cancellation, Duration::from_secs(5));
    let listener = socket();
    let dir = OwnedFixture::new();
    let request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
    let mut sink = AcceptThenCancel {
        cancellation: &cancellation,
    };

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(&context, &fixture_host.host, request, policy(), &mut sink).await;
    let closure = fixture_host.finish(&mut result).await;
    let error = result.unwrap_err();
    assert!(matches!(error.reason(), PlainReason::Context(_)));
    assert_eq!(
        (
            error.receipt().text(),
            error.receipt().accepted().stdout,
            error.receipt().events()
        ),
        ("A", 1, 1)
    );
    let actual = failed(error.receipt());
    assert_eq!(actual.delivered.stdout, 0);
    assert!(matches!(
        actual.reason,
        Reason::Cancelled | Reason::Sink(SinkFailure::ConsumerFailed)
    ));
    dir.finish(error.receipt(), &closure);
}

#[tokio::test]
async fn process_output_limit_is_not_an_ai_cap_or_synthetic_success() {
    let context = original(&Cancellation::default(), Duration::from_secs(30));
    let listener = socket();
    let dir = OwnedFixture::new();
    let mut request = mode(dir.path(), &listener, "raw", Stream::Stdout, b"A");
    request.limits.stdout_bytes = 0;
    let mut collector = Collector::new(listener, &context, false);

    let fixture_host = FixtureHost::new().await;
    let mut result = run_plain(
        &context,
        &fixture_host.host,
        request,
        policy(),
        &mut collector,
    )
    .await;
    let closure = fixture_host.finish(&mut result).await;
    let error = result.unwrap_err();
    assert_eq!(error.reason(), PlainReason::Process);
    let actual = failed(error.receipt());
    assert_eq!(actual.reason, Reason::OutputLimit(Stream::Stdout));
    assert_eq!(
        (
            actual.observed.stdout,
            actual.delivered.stdout,
            collector.events
        ),
        (1, 0, 0)
    );
    assert_eq!(actual.pipes.stdout, PipeEnd::Closed);
    dir.finish(error.receipt(), &closure);
}
