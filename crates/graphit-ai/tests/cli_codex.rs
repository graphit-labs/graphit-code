use graphit_ai::{
    cli_codex::{
        CodexClassification, CodexFailure, CodexReason, CodexReceipt, CodexSink, CodexStreamLimits,
        run_codex,
    },
    cli_codex_wire::{CodexEventView, CodexItemBodyView, CodexWireLimits},
    codex_completion::{
        AttemptPurpose, CodexCompletionRun, CompletionFailureKind, CompletionLimits,
        CompletionOutcome, UsageEvidence,
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, Limits, PipeEnd,
    ProcessHost, RepairObservation, Request, SinkFailure,
};
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::time::sleep;
const START: &[u8] = br#"{"type":"turn.started"}"#;
const END:&[u8]=br#"{"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}"#;
fn message(text: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"type":"item.completed","item":{"id":"fixture","type":"agent_message","text":text}})).unwrap()
}
fn line(bytes: &[u8]) -> Vec<u8> {
    let mut v = bytes.to_vec();
    v.push(b'\n');
    v
}
fn wire() -> CodexWireLimits {
    CodexWireLimits {
        frame_bytes: 8192,
        depth: 16,
        nodes: 1000,
        fields: 1000,
        array_items: 1000,
        string_bytes: 8192,
        decoded_string_bytes: 8192,
        id_bytes: 256,
    }
}
fn state() -> CompletionLimits {
    CompletionLimits {
        max_graphit_attempts: 4,
        max_response_bytes: 8192,
        max_model_id_bytes: 256,
    }
}
fn stream() -> CodexStreamLimits {
    CodexStreamLimits {
        stdout_bytes: 32768,
        stderr_bytes: 32768,
    }
}
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    ))
}
struct Host {
    host: ProcessHost,
    _service: Cancellation,
}
impl Host {
    async fn new() -> Self {
        let service = Cancellation::default();
        let host = ProcessHost::install(&service, HostLimits::new(1, 1).unwrap()).unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        Self {
            host,
            _service: service,
        }
    }
    async fn finish<'n>(self, result: &mut Result<CodexReceipt<'n>, CodexFailure<'n>>) {
        let receipt = match result {
            Ok(r) => r,
            Err(e) => e.receipt_mut(),
        };
        self.finish_process(receipt.process_mut()).await;
    }
    async fn finish_process(
        self,
        process: Option<&mut Result<graphit_process::Outcome, graphit_process::Failure>>,
    ) {
        let remaining = match process {
            Some(Ok(o)) => o.remaining.as_mut(),
            Some(Err(e)) => e.remaining.as_mut(),
            None => None,
        };
        if let Some(r) = remaining {
            assert!(matches!(
                r.repair(Instant::now() + Duration::from_secs(5)).await,
                RepairObservation::Observed {
                    cleanup: Cleanup::Complete,
                    ..
                }
            ));
        }
        let report = match self
            .host
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
        {
            HostShutdown::Joined(r) => r,
            HostShutdown::Pending(mut r) => {
                match r.repair(Instant::now() + Duration::from_secs(5)).await {
                    HostShutdownObservation::Joined(r) => r,
                    _ => panic!("host still pending"),
                }
            }
        };
        assert!(report.failure().is_none());
    }
}
fn socket() -> TcpListener {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    l
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
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
            stdout_bytes: 32768,
            stderr_bytes: 32768,
            termination_grace: Duration::from_millis(15),
            cleanup: Duration::from_secs(1),
        },
    }
}
fn raw(dir: &Path, l: &TcpListener, out: bool, b: &[u8]) -> Request {
    native(
        dir,
        vec![
            "raw".into(),
            l.local_addr().unwrap().to_string().into(),
            if out { "out".into() } else { "err".into() },
            hex(b).into(),
        ],
    )
}
fn sequence(dir: &Path, l: &TcpListener, exit: i32, writes: &[(bool, &[u8])]) -> Request {
    let mut a = vec![
        "sequence".into(),
        l.local_addr().unwrap().to_string().into(),
        exit.to_string().into(),
    ];
    for (out, b) in writes {
        a.push(if *out { "out".into() } else { "err".into() });
        a.push(hex(b).into());
    }
    native(dir, a)
}
async fn peer(l: &TcpListener, c: &SnapshotContext) -> TcpStream {
    loop {
        c.check().unwrap();
        match l.accept() {
            Ok((p, _)) => {
                p.set_nonblocking(true).unwrap();
                return p;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                sleep(Duration::from_millis(1)).await
            }
            Err(e) => panic!("accept {:?}", e.kind()),
        }
    }
}
async fn read_byte(p: &mut TcpStream, c: &SnapshotContext) -> Option<u8> {
    let mut b = [0];
    loop {
        c.check().unwrap();
        match p.read(&mut b) {
            Ok(0) => return None,
            Ok(_) => return Some(b[0]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                sleep(Duration::from_millis(1)).await
            }
            Err(e) => panic!("read {:?}", e.kind()),
        }
    }
}
async fn ack(p: &mut TcpStream, c: &SnapshotContext) {
    loop {
        c.check().unwrap();
        match p.write(b"A") {
            Ok(1) => return,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                sleep(Duration::from_millis(1)).await
            }
            _ => panic!("ack failed"),
        }
    }
}
// Test-control ACK per write, including writes that contain no complete frame.
async fn drive(l: &TcpListener, c: &SnapshotContext, n: usize) {
    let mut p = peer(l, c).await;
    for _ in 0..n {
        assert_eq!(read_byte(&mut p, c).await, Some(b'L'));
        // The child completes this pipe write before consuming the ACK.
        ack(&mut p, c).await;
    }
}
struct Sink<'a> {
    original: &'a SnapshotContext,
    events: usize,
    text: String,
    fail: bool,
    cancel: bool,
    listener: Option<TcpListener>,
    peer: Option<TcpStream>,
    pending_verified: bool,
    fail_usage: bool,
    cancel_usage: bool,
    pending: bool,
    pending_eof: bool,
    dropped: usize,
    started: Option<Arc<tokio::sync::Notify>>,
}
impl<'a> Sink<'a> {
    fn new(original: &'a SnapshotContext) -> Self {
        Self {
            original,
            events: 0,
            text: String::new(),
            fail: false,
            cancel: false,
            listener: None,
            peer: None,
            pending_verified: false,
            fail_usage: false,
            cancel_usage: false,
            pending: false,
            pending_eof: false,
            dropped: 0,
            started: None,
        }
    }
}
impl CodexSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        event: CodexEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(std::ptr::eq(original, self.original));
        self.events += 1;
        if self.pending
            || (self.pending_eof && matches!(event, CodexEventView::TurnCompleted { .. }))
        {
            struct PendingDrop<'a>(&'a mut usize);
            impl Drop for PendingDrop<'_> {
                fn drop(&mut self) {
                    *self.0 += 1;
                }
            }
            let _guard = PendingDrop(&mut self.dropped);
            if let Some(started) = &self.started {
                started.notify_one();
            }
            std::future::pending::<()>().await;
        }
        if let CodexEventView::Item { ref item, .. } = event
            && let CodexItemBodyView::AgentMessage { text } = item.body
        {
            self.text.push_str(text);
        }
        if let Some(l) = &self.listener {
            if self.peer.is_none() {
                self.peer = Some(peer(l, original).await);
            }
            let p = self.peer.as_mut().unwrap();
            assert_eq!(read_byte(p, original).await, Some(b'L'));
            if self.events == 1 {
                // The child cannot start its next write until this ACK.
                let mut b = [0];
                assert!(matches!(p.read(&mut b),Err(e) if e.kind()==io::ErrorKind::WouldBlock));
                self.pending_verified = true;
            }
            ack(p, original).await;
        }
        if self.cancel_usage && matches!(event, CodexEventView::TurnCompleted { .. }) {
            original.cancellation().cancel();
        }
        if self.fail_usage && matches!(event, CodexEventView::TurnCompleted { .. }) {
            return Err(SinkFailure::Closed);
        }
        if self.cancel {
            original.cancellation().cancel();
        }
        if self.fail {
            Err(SinkFailure::Closed)
        } else {
            Ok(())
        }
    }
}
fn answer<'a>(r: &'a CodexReceipt<'_>) -> &'a str {
    match r.completion().unwrap() {
        CompletionOutcome::Answer(a) => a.text(),
        _ => panic!("answer missing"),
    }
}
#[tokio::test]
async fn native_events_arrive_before_exit_with_serial_backpressure_and_original() {
    for requested_model in [None, Some("captured-model")] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, requested_model, &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        sink.listener = Some(socket());
        let start = line(START);
        let msg = line(&message("é😀"));
        let end = line(END);
        let request = sequence(
            dir.path(),
            sink.listener.as_ref().unwrap(),
            0,
            &[(true, &start), (true, &msg), (true, &end)],
        );
        let mut result = run_codex(
            &mut run,
            &host.host,
            request,
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink,
        )
        .await;
        assert!(sink.pending_verified);
        assert_eq!(sink.events, 3);
        let r = result.as_ref().unwrap();
        assert_eq!(answer(r), "é😀");
        assert!(
            matches!(r.completion(), Some(CompletionOutcome::Answer(answer)) if answer.end().requested_model() == requested_model)
        );
        assert_eq!(r.known_events(), 3);
        assert_eq!(r.frames(), 3);
        assert_eq!(r.classification(), CodexClassification::Structured);
        assert!(
            matches!(r.process(),Some(Ok(o)) if o.pipes.stdout==PipeEnd::Eof && o.status.success())
        );
        host.finish(&mut result).await;
    }
}

#[tokio::test]
async fn fragmented_unicode_crlf_multiple_frames_and_final_fragment_require_eof() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    let l = socket();
    let text = message("é😀");
    let at = text.windows(2).position(|s| s == "é".as_bytes()).unwrap() + 1;
    let mut prefix = line(START);
    prefix.extend_from_slice(&text[..at]);
    let mut middle = text[at..].to_vec();
    middle.extend_from_slice(b"\r\n");
    middle.extend_from_slice(b"{\"type\":\"unknown.future\"}\n");
    let request = sequence(
        dir.path(),
        &l,
        0,
        &[
            (true, &prefix),
            (false, b"not JSON \xff"),
            (true, &middle),
            (true, END),
        ],
    );
    let (mut result, ()) = tokio::join!(
        run_codex(
            &mut run,
            &host.host,
            request,
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink
        ),
        drive(&l, &c, 4)
    );
    let r = result.as_ref().unwrap();
    assert_eq!(answer(r), "é😀");
    assert_eq!(sink.events, 3);
    assert_eq!(r.frames(), 4);
    assert_eq!(r.accepted().stderr, 10);
    host.finish(&mut result).await;
}
#[tokio::test]
async fn unknown_and_stderr_never_establish_structured_or_answer() {
    for (out, bytes) in [
        (true, &b"{\"type\":\"future\",\"bounded\":true}\n"[..]),
        (false, &b"stderr is not an answer \xff"[..]),
    ] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        let l = socket();
        let mut result = run_codex(
            &mut run,
            &host.host,
            raw(dir.path(), &l, out, bytes),
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink,
        )
        .await;
        assert_eq!(
            result.as_ref().unwrap().classification(),
            CodexClassification::Unknown
        );
        assert_eq!(sink.events, 0);
        assert!(matches!(
            result.as_ref().unwrap().completion(),
            Some(CompletionOutcome::EvidenceUnavailable(_))
        ));
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn invalid_known_provider_and_sink_errors_keep_real_terminal_and_safe_causes() {
    for (bytes, kind, sink_fail) in [
        (b"not JSON\n".as_slice(), CompletionFailureKind::Wire, false),
        (
            b"{\"type\":\"thread.started\"}\n".as_slice(),
            CompletionFailureKind::Wire,
            false,
        ),
        (
            b"{\"type\":\"error\",\"message\":\"fixture-private-canary\"}\n".as_slice(),
            CompletionFailureKind::Provider,
            false,
        ),
        (&line(START), CompletionFailureKind::Phase, true),
    ] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        sink.fail = sink_fail;
        let l = socket();
        let mut result = run_codex(
            &mut run,
            &host.host,
            raw(dir.path(), &l, true, bytes),
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink,
        )
        .await;
        let e = result.as_ref().err().unwrap();
        assert!(e.receipt().process().is_some());
        assert!(!format!("{e:?} {e}").contains("fixture-private-canary"));
        if sink_fail {
            assert!(matches!(e.reason(), CodexReason::Sink(SinkFailure::Closed)));
        } else {
            assert!(
                matches!(e.receipt().completion(),Some(CompletionOutcome::Failed(f)) if f.kind()==kind)
            );
        }
        let mut cause: &dyn std::error::Error = e;
        loop {
            assert!(!format!("{cause:?} {cause}").contains("fixture-private-canary"));
            if let Some(next) = cause.source() {
                cause = next;
            } else {
                break;
            }
        }
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn nonzero_exit_never_publishes_successful_completion() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    let l = socket();
    let mut bytes = line(START);
    bytes.extend(line(&message("candidate")));
    bytes.extend_from_slice(END);
    let request = sequence(dir.path(), &l, 7, &[(true, &bytes)]);
    let (mut result, ()) = tokio::join!(
        run_codex(
            &mut run,
            &host.host,
            request,
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink
        ),
        drive(&l, &c, 1)
    );
    let e = result.as_ref().err().unwrap();
    assert!(matches!(e.reason(), CodexReason::NonzeroExit));
    assert!(e.receipt().completion().is_none());
    assert!(matches!(e.receipt().process(),Some(Ok(o)) if o.status.code()==Some(7)));
    assert_eq!(run.totals().observed_input_tokens, 8);
    host.finish(&mut result).await;
}
#[tokio::test]
async fn consuming_parts_keep_nonzero_terminal_usage_after_attempt_scope() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let (mut process, usage) = {
        let c = original();
        let model = String::from("captured-owned-parts-model");
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some(&model), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        let l = socket();
        let mut bytes = line(START);
        bytes.extend(line(&message("candidate")));
        bytes.extend_from_slice(END);
        let (result, ()) = tokio::join!(
            run_codex(
                &mut run,
                &host.host,
                sequence(dir.path(), &l, 7, &[(true, &bytes)]),
                AttemptPurpose::Request,
                None,
                stream(),
                &mut sink
            ),
            drive(&l, &c, 1)
        );
        let failure = result.err().unwrap();
        let before = failure.receipt();
        let observations = (
            before.usage(),
            before.accepted(),
            before.frames(),
            before.known_events(),
            before.classification(),
        );
        let failure = failure.into_parts();
        assert!(matches!(failure.reason, CodexReason::NonzeroExit));
        assert!(failure.cause.is_none());
        let parts = failure.receipt.into_parts();
        assert!(parts.completion.is_none());
        assert_eq!(parts.usage, observations.0);
        assert_eq!(parts.accepted.stdout, observations.1.stdout);
        assert_eq!(parts.accepted.stderr, observations.1.stderr);
        assert_eq!(parts.frames, observations.2);
        assert_eq!(parts.known_events, observations.3);
        assert_eq!(parts.classification, observations.4);
        assert_eq!(run.totals().observed_input_tokens, 8);
        assert_eq!(run.totals().observed_output_tokens, 3);
        (parts.process, parts.usage)
    };
    assert!(matches!(process, Some(Ok(ref outcome)) if outcome.status.code() == Some(7)));
    assert!(
        matches!(usage, UsageEvidence::Observed(v) if v.input_tokens == 8 && v.output_tokens == 3)
    );
    host.finish_process(process.as_mut()).await;
}
#[tokio::test]
async fn consuming_parts_keep_owned_state_cause_after_attempt_scope() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let (mut process, failure) = {
        let c = original();
        let model = String::from("captured-owned-parts-model");
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some(&model), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        let l = socket();
        let bytes = b"{\"type\":\"error\",\"message\":\"owned-state-private-canary\"}\n";
        let (result, ()) = tokio::join!(
            run_codex(
                &mut run,
                &host.host,
                raw(dir.path(), &l, true, bytes),
                AttemptPurpose::Request,
                None,
                stream(),
                &mut sink
            ),
            drive(&l, &c, 1)
        );
        let failure = result.err().unwrap().into_parts();
        assert!(matches!(failure.reason, CodexReason::State));
        assert!(failure.cause.is_none());
        let parts = failure.receipt.into_parts();
        let Some(CompletionOutcome::Failed(failure)) = parts.completion else {
            panic!("the original State cause was not retained");
        };
        assert_eq!(parts.usage, failure.usage());
        (parts.process, failure)
    };
    assert!(process.is_some());
    assert_eq!(failure.kind(), CompletionFailureKind::Provider);
    assert!(!format!("{failure:?} {failure}").contains("owned-state-private-canary"));
    host.finish_process(process.as_mut()).await;
}
#[tokio::test]
async fn consuming_parts_keep_owned_context_cause_without_process_placeholder() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let (mut process, cause, diagnostic) = {
        let cancellation = Cancellation::default();
        let c = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            cancellation.clone(),
        ));
        let model = String::from("captured-owned-parts-model");
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some(&model), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        cancellation.cancel();
        let failure = run_codex(
            &mut run,
            &host.host,
            native(dir.path(), Vec::new()),
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink,
        )
        .await
        .err()
        .unwrap();
        let diagnostic = std::error::Error::source(&failure).unwrap().to_string();
        let failure = failure.into_parts();
        assert!(matches!(
            failure.reason,
            CodexReason::Context(io::ErrorKind::Interrupted)
        ));
        let parts = failure.receipt.into_parts();
        assert!(parts.completion.is_none());
        assert!(parts.process.is_none());
        assert_eq!(parts.frames, 0);
        assert_eq!(parts.known_events, 0);
        (parts.process, failure.cause.unwrap(), diagnostic)
    };
    assert_eq!(cause.to_string(), diagnostic);
    assert_eq!(
        std::error::Error::source(&cause).unwrap().to_string(),
        diagnostic
    );
    host.finish_process(process.as_mut()).await;
}
#[tokio::test]
async fn consumer_byte_frame_and_response_caps_stop_without_plain_fallback() {
    for case in 0..4 {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let mut w = wire();
        let mut sl = stream();
        let mut st = state();
        if case == 0 {
            sl.stdout_bytes = 1;
        }
        if case == 1 {
            sl.stderr_bytes = 1;
        }
        if case == 2 {
            w.frame_bytes = 4;
        }
        if case == 3 {
            st.max_response_bytes = 1;
        }
        let mut run = CodexCompletionRun::new(&c, Some("m"), &w, st).unwrap();
        let mut sink = Sink::new(&c);
        let l = socket();
        let mut bytes = line(START);
        bytes.extend(line(&message("oversized")));
        let mut result = run_codex(
            &mut run,
            &host.host,
            raw(dir.path(), &l, case != 1, &bytes),
            AttemptPurpose::Request,
            None,
            sl,
            &mut sink,
        )
        .await;
        let e = result.as_ref().err().unwrap();
        match case {
            0 => assert!(matches!(e.reason(), CodexReason::StdoutLimit)),
            1 => assert!(matches!(e.reason(), CodexReason::StderrLimit)),
            2 => assert!(matches!(e.reason(), CodexReason::FrameLimit)),
            3 => assert!(
                matches!(e.receipt().completion(),Some(CompletionOutcome::Failed(f)) if f.kind()==CompletionFailureKind::ResponseLimit)
            ),
            _ => unreachable!(),
        };
        assert!(e.receipt().process().is_some());
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn cancellation_after_event_awaits_supervisor_cleanup_and_keeps_no_answer() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    sink.cancel = true;
    let l = socket();
    let request = sequence(dir.path(), &l, 0, &[(true, &line(START))]);
    let mut result = run_codex(
        &mut run,
        &host.host,
        request,
        AttemptPurpose::Request,
        None,
        stream(),
        &mut sink,
    )
    .await;
    let e = result.as_ref().err().unwrap();
    assert!(e.receipt().process().is_some());
    assert!(
        matches!(e.receipt().completion(),Some(CompletionOutcome::Failed(f)) if f.io_kind()==Some(io::ErrorKind::Interrupted))
    );
    assert_eq!(sink.events, 1);
    host.finish(&mut result).await;
}

#[tokio::test]
async fn exact_frame_cap_accepts_lf_crlf_and_split_crlf() {
    for case in 0..3 {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let mut w = wire();
        w.frame_bytes = START.len();
        let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        let l = socket();
        let mut bytes = START.to_vec();
        bytes.extend_from_slice(if case == 0 { b"\n" } else { b"\r\n" });
        let mut result = if case == 2 {
            let request = sequence(
                dir.path(),
                &l,
                0,
                &[(true, &bytes[..bytes.len() - 1]), (true, b"\n")],
            );
            let (result, ()) = tokio::join!(
                run_codex(
                    &mut run,
                    &host.host,
                    request,
                    AttemptPurpose::Request,
                    None,
                    stream(),
                    &mut sink
                ),
                drive(&l, &c, 2)
            );
            result
        } else {
            run_codex(
                &mut run,
                &host.host,
                raw(dir.path(), &l, true, &bytes),
                AttemptPurpose::Request,
                None,
                stream(),
                &mut sink,
            )
            .await
        };
        assert_eq!(result.as_ref().unwrap().frames(), 1);
        assert_eq!(sink.events, 1);
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn terminal_nonzero_remains_first_cause_when_final_fragment_is_invalid() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    let l = socket();
    let request = sequence(dir.path(), &l, 7, &[(true, b"invalid-final-fragment")]);
    let (mut result, ()) = tokio::join!(
        run_codex(
            &mut run,
            &host.host,
            request,
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink
        ),
        drive(&l, &c, 1)
    );
    let e = result.as_ref().err().unwrap();
    assert!(matches!(e.reason(), CodexReason::NonzeroExit));
    assert!(
        matches!(e.receipt().completion(),Some(CompletionOutcome::Failed(f)) if f.kind()==CompletionFailureKind::Wire)
    );
    assert!(matches!(e.receipt().process(),Some(Ok(o)) if o.status.code()==Some(7)));
    host.finish(&mut result).await;
}
#[tokio::test]
async fn decoded_usage_survives_sink_rejection_and_cancellation_on_turn_completed() {
    for cancel in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        sink.fail_usage = !cancel;
        sink.cancel_usage = cancel;
        let l = socket();
        let mut bytes = line(START);
        bytes.extend(line(&message("candidate")));
        bytes.extend_from_slice(b"{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":8,\"cached_input_tokens\":2,\"output_tokens\":3,\"reasoning_output_tokens\":1,\"cache_write_input_tokens\":9}}\n");
        let mut result = run_codex(
            &mut run,
            &host.host,
            raw(dir.path(), &l, true, &bytes),
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink,
        )
        .await;
        assert!(result.is_err());
        let totals = run.totals();
        assert_eq!(totals.observed_input_tokens, 8);
        assert_eq!(totals.observed_cached_input_tokens, 2);
        assert_eq!(totals.observed_output_tokens, 3);
        assert_eq!(totals.observed_reasoning_output_tokens, 1);
        assert_eq!(totals.observed_cache_write_input_tokens, 9);
        assert_eq!(totals.observed_attempts, 1);
        let e = result.as_ref().err().unwrap();
        assert!(e.receipt().process().is_some());
        assert!(!matches!(
            e.receipt().completion(),
            Some(CompletionOutcome::Answer(_))
        ));
        if !cancel {
            assert!(matches!(e.reason(), CodexReason::Sink(SinkFailure::Closed)));
        }
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn prelaunch_policy_and_context_have_no_fabricated_terminal() {
    for cancel in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        let l = socket();
        let mut sl = stream();
        sl.stdout_bytes = 0;
        if cancel {
            c.cancellation().cancel();
        }
        let mut result = run_codex(
            &mut run,
            &host.host,
            raw(dir.path(), &l, true, b""),
            AttemptPurpose::Request,
            None,
            sl,
            &mut sink,
        )
        .await;
        let e = result.as_ref().err().unwrap();
        assert!(e.receipt().process().is_none());
        assert_eq!(run.admitted_attempts(), 0);
        if cancel {
            assert!(matches!(
                e.reason(),
                CodexReason::Context(io::ErrorKind::Interrupted)
            ));
        } else {
            assert!(matches!(e.reason(), CodexReason::Policy));
        }
        host.finish(&mut result).await;
    }
}

#[tokio::test]
async fn real_spawn_failure_keeps_not_spawned_terminal_without_wire_eof_or_answer() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    let l = socket();
    let mut request = raw(dir.path(), &l, true, START);
    request.program = dir.path().join("missing-fixture.exe");
    let mut result = run_codex(
        &mut run,
        &host.host,
        request,
        AttemptPurpose::Request,
        None,
        stream(),
        &mut sink,
    )
    .await;
    let e = result.as_ref().err().unwrap();
    assert!(matches!(e.reason(), CodexReason::Process));
    assert!(e.receipt().completion().is_none());
    assert!(
        matches!(e.receipt().process(),Some(Err(f)) if f.status.is_none() && f.pipes.stdout==PipeEnd::NotSpawned && f.pipes.stderr==PipeEnd::NotSpawned)
    );
    assert_eq!(sink.events, 0);
    assert_eq!(run.admitted_attempts(), 1);
    assert_eq!(run.totals().unavailable_attempts, 1);
    host.finish(&mut result).await;
}

#[tokio::test]
async fn original_deadline_drops_pending_codex_callback_then_awaits_real_cleanup() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        Cancellation::default(),
    ));
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    sink.pending = true;
    let l = socket();
    let request = sequence(dir.path(), &l, 0, &[(true, &line(START))]);
    let mut result = run_codex(
        &mut run,
        &host.host,
        request,
        AttemptPurpose::Request,
        None,
        stream(),
        &mut sink,
    )
    .await;
    let e = result.as_ref().err().unwrap();
    assert_eq!(sink.events, 1);
    assert_eq!(sink.dropped, 1);
    assert_eq!(e.receipt().known_events(), 0);
    assert!(matches!(e.receipt().process(), Some(Err(_))));
    assert!(
        matches!(e.receipt().completion(),Some(CompletionOutcome::Failed(f)) if f.io_kind()==Some(io::ErrorKind::TimedOut))
    );
    assert_eq!(run.admitted_attempts(), 1);
    assert_eq!(run.totals().unavailable_attempts, 1);
    host.finish(&mut result).await;
}

#[tokio::test]
async fn host_cancellation_drops_pending_callback_without_eof_replay_or_new_context() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    sink.pending = true;
    let started = Arc::new(tokio::sync::Notify::new());
    sink.started = Some(started.clone());
    let service = host._service.clone();
    let l = socket();
    let request = sequence(dir.path(), &l, 0, &[(true, &line(START))]);
    let stop = async {
        started.notified().await;
        service.cancel();
    };
    let (mut result, ()) = tokio::join!(
        run_codex(
            &mut run,
            &host.host,
            request,
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink
        ),
        stop
    );
    let e = result.as_ref().err().unwrap();
    assert!(matches!(e.reason(), CodexReason::Process));
    assert_eq!(sink.events, 1);
    assert_eq!(sink.dropped, 1);
    assert_eq!(e.receipt().frames(), 1);
    assert_eq!(e.receipt().known_events(), 0);
    assert!(c.check().is_ok());
    assert!(matches!(e.receipt().process(), Some(Err(_))));
    assert!(e.receipt().completion().is_none());
    assert_eq!(run.totals().unavailable_attempts, 1);
    host.finish(&mut result).await;
}
async fn original_stops_pending_eof_callback(cancel: bool) {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = if cancel {
        original()
    } else {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_millis(200),
            Cancellation::default(),
        ))
    };
    let w = wire();
    let mut run = CodexCompletionRun::new(&c, Some("captured-model"), &w, state()).unwrap();
    let mut sink = Sink::new(&c);
    sink.pending_eof = true;
    let started = Arc::new(tokio::sync::Notify::new());
    sink.started = Some(started.clone());
    let l = socket();
    let mut bytes = line(START);
    bytes.extend_from_slice(END);
    let request = raw(dir.path(), &l, true, &bytes);
    let deadline = c.required_deadline().unwrap();
    let stop = async {
        tokio::select! {
            _=started.notified()=>{if cancel {c.cancellation().cancel();}},
            _=tokio::time::sleep_until(deadline.into())=>{}
        }
    };
    let (mut result, ()) = tokio::join!(
        run_codex(
            &mut run,
            &host.host,
            request,
            AttemptPurpose::Request,
            None,
            stream(),
            &mut sink
        ),
        stop
    );
    let e = result.as_ref().err().unwrap();
    let expected = if cancel {
        io::ErrorKind::Interrupted
    } else {
        io::ErrorKind::TimedOut
    };
    assert!(matches!(e.reason(),CodexReason::Context(kind) if kind==expected));
    assert_eq!(sink.events, 2);
    assert_eq!(sink.dropped, 1);
    assert_eq!(e.receipt().frames(), 2);
    assert_eq!(e.receipt().known_events(), 1);
    assert!(!matches!(
        e.receipt().completion(),
        Some(CompletionOutcome::Answer(_))
    ));
    let Some(Ok(process)) = e.receipt().process() else {
        panic!("the native process must report its terminal result");
    };
    assert!(process.status.success());
    assert_eq!(
        (process.pipes.stdout, process.pipes.stderr),
        (PipeEnd::Eof, PipeEnd::Eof)
    );
    #[cfg(unix)]
    assert_eq!(process.cleanup, Cleanup::Complete);
    #[cfg(windows)]
    assert_eq!(
        process.cleanup,
        Cleanup::Unconfirmed(graphit_process::UnconfirmedCleanup::TreeUnknownWindows {
            leader: graphit_process::LeaderCleanup::Reaped,
            control: graphit_process::ControlFailures::default(),
        })
    );
    assert_eq!(run.totals().observed_input_tokens, 8);
    assert_eq!(run.totals().observed_cached_input_tokens, 2);
    assert_eq!(run.totals().observed_output_tokens, 3);
    assert_eq!(run.totals().observed_reasoning_output_tokens, 1);
    assert_eq!(run.totals().observed_attempts, 1);
    host.finish(&mut result).await;
}
#[tokio::test]
async fn original_cancel_drops_pending_final_fragment_after_actual_process_eof() {
    original_stops_pending_eof_callback(true).await;
}
#[tokio::test]
async fn original_deadline_drops_pending_final_fragment_after_actual_process_eof() {
    original_stops_pending_eof_callback(false).await;
}

#[tokio::test]
async fn observed_usage_is_returned_for_actual_nonzero_and_late_wire_failure() {
    for malformed in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let w = wire();
        let mut run = CodexCompletionRun::new(&c, Some("m"), &w, state()).unwrap();
        let mut sink = Sink::new(&c);
        let l = socket();
        let mut bytes = line(START);
        bytes.extend(line(&message("candidate")));
        bytes.extend_from_slice(b"{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":8,\"cached_input_tokens\":2,\"output_tokens\":3,\"reasoning_output_tokens\":1,\"cache_write_input_tokens\":9}}\n");
        if malformed {
            bytes.extend_from_slice(b"{invalid-json\n");
        }
        let request = if malformed {
            raw(dir.path(), &l, true, &bytes)
        } else {
            sequence(dir.path(), &l, 7, &[(true, &bytes)])
        };
        let (mut result, ()) = tokio::join!(
            run_codex(
                &mut run,
                &host.host,
                request,
                AttemptPurpose::Request,
                None,
                stream(),
                &mut sink
            ),
            async {
                if !malformed {
                    drive(&l, &c, 1).await;
                }
            }
        );
        let error = result.as_ref().err().unwrap();
        let receipt = error.receipt();
        assert_eq!(receipt.known_events(), 3);
        let UsageEvidence::Observed(value) = receipt.usage() else {
            panic!("observed attempt usage required")
        };
        assert_eq!(
            (
                value.input_tokens,
                value.cached_input_tokens,
                value.output_tokens,
                value.reasoning_output_tokens,
                value.cache_write_input_tokens
            ),
            (8, 2, 3, 1, 9)
        );
        assert_eq!(run.totals().observed_attempts, 1);
        assert_eq!(run.totals().observed_input_tokens, 8);
        assert_eq!(run.totals().observed_output_tokens, 3);
        assert_eq!(run.totals().observed_cache_write_input_tokens, 9);
        if malformed {
            assert!(
                matches!(receipt.completion(),Some(CompletionOutcome::Failed(error)) if error.kind()==CompletionFailureKind::Wire)
            );
        } else {
            assert!(matches!(error.reason(), CodexReason::NonzeroExit));
            assert!(receipt.completion().is_none());
            assert!(
                matches!(receipt.process(),Some(Ok(process)) if process.status.code()==Some(7))
            );
        }
        host.finish(&mut result).await;
    }
}
