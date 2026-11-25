use crate::cli_control as control;
use graphit_ai::{cli_grok::*, cli_grok_wire::*};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, Limits, PipeEnd,
    ProcessHost, RepairObservation, Request, SinkFailure,
};
use std::{
    ffi::OsString,
    io,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::net::TcpStream;
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
    async fn finish(self, result: &mut Result<GrokReceipt, GrokFailure>) {
        let receipt = match result {
            Ok(r) => r,
            Err(e) => e.receipt_mut(),
        };
        let remaining = match receipt.process_mut() {
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
    control::peer(l, c).await.unwrap()
}
async fn read_byte(p: &mut TcpStream, c: &SnapshotContext) -> Option<u8> {
    control::byte(p, c).await.unwrap()
}
async fn ack(p: &mut TcpStream, c: &SnapshotContext) {
    control::acknowledge(p, c).await.unwrap();
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

const THOUGHT: &[u8] = br#"{"type":"thought","data":"native thought, never answer"}"#;
const USAGE: &[u8] = br#"{"type":"usage","messageId":"native-message","stopReason":"tool_use","usage":{"input_tokens":9}}"#;
const MESSAGE: &[u8] = br#"{"type":"text","data":"FINAL"}"#;
const TOOL: &[u8] = br#"{"type":"tool_call","toolCallId":"native-tool","toolName":"read_file","title":"Read","kind":"read","status":"in_progress","rawInput":{"big":9007199254740993,"big":9007199254740995,"ratio":1e-4,"unicode":"\u0061"},"content":[],"locations":[]}"#;
const TOOL_END: &[u8] = br#"{"type":"tool_call_update","toolCallId":"native-tool","status":"completed","rawOutput":{"text":"injected tool response"},"content":[],"locations":[]}"#;
const END: &[u8] = br#"{"type":"end","stopReason":"end_turn","sessionId":"session","requestId":"","usage":{"input_tokens":9,"output_tokens":2},"num_turns":1}"#;
fn line(bytes: &[u8]) -> Vec<u8> {
    let mut b = bytes.to_vec();
    b.push(b'\n');
    b
}
fn transcript(parts: &[&[u8]]) -> Vec<u8> {
    parts.iter().flat_map(|b| line(b)).collect()
}
fn limits() -> GrokLimits {
    GrokLimits {
        wire: GrokWireLimits {
            frame_bytes: 8192,
            depth: 16,
            nodes: 1000,
            fields: 1000,
            array_items: 1000,
            string_bytes: 8192,
            decoded_string_bytes: 8192,
            id_bytes: 256,
        },
        stdout_bytes: 32768,
        stderr_bytes: 32768,
        frames: 32,
        response_bytes: 8192,
    }
}
struct Sink<'n> {
    original: &'n SnapshotContext,
    seen: Vec<String>,
    listener: Option<TcpListener>,
    peer: Option<TcpStream>,
    backpressure: bool,
    pending_on: Option<&'static str>,
    started: Option<Arc<tokio::sync::Notify>>,
    dropped: usize,
    cancel_result: bool,
}
impl<'n> Sink<'n> {
    fn new(original: &'n SnapshotContext) -> Self {
        Self {
            original,
            seen: Vec::new(),
            listener: None,
            peer: None,
            backpressure: false,
            pending_on: None,
            started: None,
            dropped: 0,
            cancel_result: false,
        }
    }
}
impl GrokSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        event: GrokEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(std::ptr::eq(original, self.original));
        let result = matches!(event.data, GrokEventData::End(_));
        if self.pending_on == Some("any") || (self.pending_on == Some("result") && result) {
            struct OnDrop<'a>(&'a mut usize);
            impl Drop for OnDrop<'_> {
                fn drop(&mut self) {
                    *self.0 += 1;
                }
            }
            let _guard = OnDrop(&mut self.dropped);
            if let Some(n) = &self.started {
                n.notify_one();
            }
            std::future::pending::<()>().await;
        }
        self.seen.push(event.native.get().to_owned());
        if let Some(l) = &self.listener {
            if self.peer.is_none() {
                self.peer = Some(peer(l, original).await);
            }
            let p = self.peer.as_mut().unwrap();
            assert_eq!(read_byte(p, original).await, Some(b'L'));
            if self.seen.len() == 1 {
                // Confirm this completed pipe write is still held by its ACK.
                control::command(p, original, b'P').await.unwrap();
                assert_eq!(read_byte(p, original).await, Some(b'W'));
                self.backpressure = true;
            }
            ack(p, original).await;
        }
        if result && self.cancel_result {
            original.cancellation().cancel();
        }
        Ok(())
    }
}
// Test-control ACK, tolerant of the real child being terminated by an invalid frame.
async fn release(l: &TcpListener, writes: usize) {
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(3),
        Cancellation::default(),
    );
    let mut p = control::peer(l, &original).await.unwrap();
    for _ in 0..writes {
        match control::byte(&mut p, &original).await {
            Ok(None) => return,
            Ok(Some(byte)) => {
                assert_eq!(byte, b'L');
                if let Err(error) = control::acknowledge(&mut p, &original).await {
                    assert!(matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                    ));
                    return;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                ) =>
            {
                return;
            }
            Err(error) => panic!("test control {:?}", error.kind()),
        }
    }
}
#[tokio::test]
async fn native_complete_messages_before_exit_preserve_tool_lexemes_and_backpressure() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let mut sink = Sink::new(&c);
    sink.listener = Some(socket());
    let writes = [
        line(THOUGHT),
        line(USAGE),
        line(MESSAGE),
        line(TOOL),
        line(TOOL_END),
        line(END),
    ];
    let req = sequence(
        dir.path(),
        sink.listener.as_ref().unwrap(),
        0,
        &writes
            .iter()
            .map(|v| (true, v.as_slice()))
            .collect::<Vec<_>>(),
    );
    let mut result = run_grok(&c, &host.host, req, limits(), &mut sink).await;
    let r = result.as_ref().unwrap();
    assert!(sink.backpressure);
    assert_eq!(sink.seen.len(), 6);
    assert_eq!(r.known_events(), 6);
    assert_eq!(r.answer(), Some("FINAL"));
    assert_eq!(sink.seen[3], std::str::from_utf8(TOOL).unwrap());
    assert!(
        sink.seen[3].contains("9007199254740993")
            && sink.seen[3].contains("1e-4")
            && sink.seen[3].contains("\\u0061")
    );
    assert_eq!(
        r.native_result_json(),
        Some(std::str::from_utf8(END).unwrap())
    );
    let native: serde_json::Value = serde_json::from_str(r.native_result_json().unwrap()).unwrap();
    assert_eq!(native["usage"]["input_tokens"], 9);
    let debug = format!("{r:?}");
    assert!(!debug.contains("FINAL") && !debug.contains("session"));
    host.finish(&mut result).await;
}
#[tokio::test]
async fn split_crlf_unknown_json_and_final_unterminated_frame_wait_for_native_eof() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let l = socket();
    let mut bytes = br#"{"type":"future","payload":{"big":9007199254740993,"n":1e-4}}"#.to_vec();
    bytes.extend(b"\r\n");
    bytes.extend(THOUGHT);
    bytes.extend(b"\r\n");
    bytes.extend(MESSAGE);
    bytes.extend(b"\r\n");
    bytes.extend(END);
    // The existing native fixture accepts at most 32 controlled writes.
    // Force a split between CR and LF, then split the remaining JSON bytes.
    let first_cr = bytes.iter().position(|&b| b == b'\r').unwrap() + 1;
    let mut writes = vec![(true, &bytes[..first_cr])];
    writes.extend(bytes[first_cr..].chunks(31).map(|b| (true, b)));
    assert!(writes.len() <= 32);
    let req = sequence(dir.path(), &l, 0, &writes);
    let mut sink = Sink::new(&c);
    let (mut result, ()) = tokio::join!(
        run_grok(&c, &host.host, req, limits(), &mut sink),
        drive(&l, &c, writes.len())
    );
    let r = result.as_ref().unwrap();
    assert_eq!(r.frames(), 4);
    assert_eq!(r.known_events(), 3);
    assert_eq!(r.answer(), Some("FINAL"));
    let p = r.process().unwrap().as_ref().unwrap();
    assert_eq!(p.pipes.stdout, PipeEnd::Eof);
    assert_eq!(p.pipes.stderr, PipeEnd::Eof);
    host.finish(&mut result).await;
}
#[tokio::test]
async fn missing_error_stop_duplicate_and_postterminal_never_answer() {
    let cancelled = String::from_utf8(END.to_vec())
        .unwrap()
        .replace("end_turn", "cancelled")
        .into_bytes();
    let cases = [
        (transcript(&[MESSAGE]), "MissingResult"),
        (transcript(&[END, END]), "State"),
        (
            transcript(&[END, br#"{"type":"future","extra":{"lex":1e-4}}"#]),
            "State",
        ),
        (transcript(&[END, MESSAGE]), "State"),
        (transcript(&[&cancelled]), "Provider"),
        (
            transcript(&[br#"{"type":"error","message":"private native failure"}"#]),
            "Provider",
        ),
        (
            transcript(&[MESSAGE, br#"{"type":"error","message":"late failure"}"#]),
            "Provider",
        ),
        (
            transcript(&[br#"{"type":"max_turns_reached"}"#]),
            "Provider",
        ),
    ];
    for (bytes, reason) in cases {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let l = socket();
        let mut sink = Sink::new(&c);
        let req = raw(dir.path(), &l, true, &bytes);
        let (mut result, ()) = tokio::join!(
            run_grok(&c, &host.host, req, limits(), &mut sink),
            release(&l, 1)
        );
        let e = result.as_ref().unwrap_err();
        assert_eq!(format!("{:?}", e.reason()), reason);
        assert_eq!(e.receipt().answer(), None);
        assert_eq!(e.receipt().native_result_json(), None);
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn schema_and_concrete_output_response_json_limits_preserve_native_custody() {
    let mut response = limits();
    response.response_bytes = 2;
    let mut frames = limits();
    frames.frames = 1;
    let mut stdout = limits();
    stdout.stdout_bytes = 1;
    let mut stderr = limits();
    stderr.stderr_bytes = 1;
    let mut frame = limits();
    frame.wire.frame_bytes = 20;
    let mut nodes = limits();
    nodes.wire.nodes = 1;
    let mut fields = limits();
    fields.wire.fields = 1;
    let bytes = transcript(&[THOUGHT, MESSAGE, END]);
    for (out, data, cap, reason) in [
        (true, bytes.as_slice(), response, "ResponseLimit"),
        (true, bytes.as_slice(), frames, "FrameLimit"),
        (true, bytes.as_slice(), stdout, "StdoutLimit"),
        (false, b"native stderr".as_slice(), stderr, "StderrLimit"),
        (true, bytes.as_slice(), frame, "FrameLimit"),
        (true, bytes.as_slice(), nodes, "Wire"),
        (true, bytes.as_slice(), fields, "Wire"),
        (
            true,
            br#"{"type":"text","data":null}"#.as_slice(),
            limits(),
            "Wire",
        ),
        (
            true,
            br#"{"type":"text","type":"text","data":"ignored"}"#.as_slice(),
            limits(),
            "Wire",
        ),
        (
            true,
            br#"{"type":"end","stopReason":"end_turn","sessionId":"","requestId":""}"#.as_slice(),
            limits(),
            "Wire",
        ),
    ] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let l = socket();
        let mut sink = Sink::new(&c);
        let req = raw(dir.path(), &l, out, data);
        let (mut result, ()) = tokio::join!(
            run_grok(&c, &host.host, req, cap, &mut sink),
            release(&l, 1)
        );
        let e = result.as_ref().unwrap_err();
        assert_eq!(format!("{:?}", e.reason()), reason);
        assert!(e.receipt().answer().is_none());
        assert!(e.receipt().process().is_some());
        host.finish(&mut result).await;
    }
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let l = socket();
    let mut sink = Sink::new(&c);
    let mut policy = limits();
    policy.frames = 0;
    let mut result = run_grok(
        &c,
        &host.host,
        raw(dir.path(), &l, true, THOUGHT),
        policy,
        &mut sink,
    )
    .await;
    assert!(matches!(
        result.as_ref().unwrap_err().reason(),
        GrokReason::Policy
    ));
    assert!(result.as_ref().unwrap_err().receipt().process().is_none());
    host.finish(&mut result).await;
}
#[tokio::test]
async fn pending_native_progress_cancel_and_real_deadline_keep_cleanup_and_original() {
    for deadline in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = Arc::new(SnapshotContext::new(
            Instant::now()
                + if deadline {
                    Duration::from_millis(500)
                } else {
                    Duration::from_secs(3)
                },
            Cancellation::default(),
        ));
        let l = socket();
        let mut sink = Sink::new(&c);
        sink.pending_on = Some("any");
        let started = Arc::new(tokio::sync::Notify::new());
        sink.started = Some(Arc::clone(&started));
        let req = raw(dir.path(), &l, true, &line(THOUGHT));
        let cancel = async {
            started.notified().await;
            if !deadline {
                c.cancellation().cancel();
            }
        };
        let (mut result, ()) =
            tokio::join!(run_grok(&c, &host.host, req, limits(), &mut sink), cancel);
        assert!(result.is_err());
        assert_eq!(sink.dropped, 1);
        assert!(sink.seen.is_empty());
        assert!(result.as_ref().unwrap_err().receipt().answer().is_none());
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn final_eof_callback_cancel_is_observed_once_and_never_becomes_answer() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let l = socket();
    let mut sink = Sink::new(&c);
    sink.pending_on = Some("result");
    let started = Arc::new(tokio::sync::Notify::new());
    sink.started = Some(Arc::clone(&started));
    let mut bytes = line(THOUGHT);
    bytes.extend(END);
    let req = raw(dir.path(), &l, true, &bytes);
    let cancel = async {
        started.notified().await;
        c.cancellation().cancel();
    };
    let (mut result, (), ()) = tokio::join!(
        run_grok(&c, &host.host, req, limits(), &mut sink),
        release(&l, 1),
        cancel
    );
    assert!(result.is_err());
    assert_eq!(sink.dropped, 1);
    assert_eq!(sink.seen.len(), 1);
    let r = result.as_ref().unwrap_err().receipt();
    assert!(r.answer().is_none() && r.native_result_json().is_none());
    host.finish(&mut result).await;
}
#[tokio::test]
async fn native_nonzero_and_cancellation_at_terminal_cannot_promote_result() {
    for cancelled in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let l = socket();
        let mut sink = Sink::new(&c);
        sink.cancel_result = cancelled;
        let bytes = transcript(&[THOUGHT, END]);
        let req = sequence(
            dir.path(),
            &l,
            if cancelled { 0 } else { 13 },
            &[(true, bytes.as_slice())],
        );
        let (mut result, ()) = tokio::join!(
            run_grok(&c, &host.host, req, limits(), &mut sink),
            release(&l, 1)
        );
        assert!(result.is_err());
        let r = result.as_ref().unwrap_err().receipt();
        assert!(r.answer().is_none() && r.native_result_json().is_none());
        if !cancelled {
            assert!(matches!(
                result.as_ref().unwrap_err().reason(),
                GrokReason::NonzeroExit
            ));
        }
        host.finish(&mut result).await;
    }
}
