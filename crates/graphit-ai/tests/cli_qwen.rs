use crate::cli_control as control;
use graphit_ai::{cli_qwen::*, cli_qwen_wire::*};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, Limits, PipeEnd,
    ProcessHost, RepairObservation, Request, SinkFailure,
};
use std::{
    ffi::OsString,
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
    async fn finish(self, result: &mut Result<QwenReceipt, QwenFailure>) {
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

const INIT: &[u8] = br#"{"type":"system","subtype":"init","uuid":"session","session_id":"session","model":"observed"}"#;
const END: &[u8] = br#"{"type":"result","subtype":"success","uuid":"end","is_error":false,"duration_ms":8,"duration_api_ms":5,"num_turns":1,"session_id":"session","result":"FINAL","usage":{"input_tokens":5,"output_tokens":3,"cache_read_input_tokens":2,"total_tokens":10},"modelUsage":{"observed":{"inputTokens":5,"outputTokens":3,"cacheReadInputTokens":2,"cacheCreationInputTokens":0,"webSearchRequests":0,"contextWindow":128}},"permission_denials":[]}"#;
const PARTIAL: &[u8] = br#"{"type":"stream_event","uuid":"delta","session_id":"session","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"partial \u00e9\ud83d\ude00"}}}"#;
const TOOLS: &[u8] = br#"{"type":"assistant","uuid":"tool-message","session_id":"session","parent_tool_use_id":"agent-tool","message":{"id":"tool-message","type":"message","role":"assistant","model":"observed","usage":{"input_tokens":5,"output_tokens":3},"content":[{"type":"tool_use","id":"native-tool","name":"Read","input":{"big":9007199254740993,"ratio":1e-4,"unicode":"\u0061"}}]}}"#;
fn line(bytes: &[u8]) -> Vec<u8> {
    let mut b = bytes.to_vec();
    b.push(b'\n');
    b
}
fn limits() -> QwenLimits {
    QwenLimits {
        wire: QwenWireLimits {
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
    pending: bool,
    cancel_result: bool,
    fail_result: bool,
    started: Option<Arc<tokio::sync::Notify>>,
    dropped: usize,
}
impl<'n> Sink<'n> {
    fn new(original: &'n SnapshotContext) -> Self {
        Self {
            original,
            seen: Vec::new(),
            listener: None,
            peer: None,
            backpressure: false,
            pending: false,
            cancel_result: false,
            fail_result: false,
            started: None,
            dropped: 0,
        }
    }
}
impl QwenSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        event: QwenEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(std::ptr::eq(original, self.original));
        if self.pending {
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
        if matches!(event.data, QwenEventData::Result(_)) {
            if self.cancel_result {
                original.cancellation().cancel();
            }
            if self.fail_result {
                return Err(SinkFailure::Closed);
            }
        }
        Ok(())
    }
}
#[tokio::test]
async fn primary_jsonl_arrives_before_exit_and_preserves_raw_tools_with_backpressure() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let mut sink = Sink::new(&c);
    sink.listener = Some(socket());
    let writes = [line(INIT), line(PARTIAL), line(TOOLS), line(END)];
    let request = sequence(
        dir.path(),
        sink.listener.as_ref().unwrap(),
        0,
        &writes
            .iter()
            .map(|v| (true, v.as_slice()))
            .collect::<Vec<_>>(),
    );
    let mut result = run_qwen(&c, &host.host, request, limits(), &mut sink).await;
    let receipt = result.as_ref().unwrap();
    assert!(sink.backpressure);
    assert_eq!(sink.seen.len(), 4);
    assert_eq!(receipt.known_events(), 4);
    assert_eq!(receipt.answer(), Some("FINAL"));
    assert_eq!(sink.seen[2], std::str::from_utf8(TOOLS).unwrap());
    assert!(sink.seen[2].contains("9007199254740993"));
    assert!(sink.seen[2].contains("1e-4"));
    assert_eq!(receipt.usage().unwrap().cache_creation_input_tokens, None);
    assert_eq!(
        receipt.native_result_json(),
        Some(std::str::from_utf8(END).unwrap())
    );
    let debug = format!("{receipt:?}");
    assert!(!debug.contains("FINAL") && !debug.contains("session"));
    assert!(
        receipt
            .process()
            .unwrap()
            .as_ref()
            .unwrap()
            .status
            .success()
    );
    host.finish(&mut result).await;
}
#[tokio::test]
async fn subagent_messages_do_not_compose_answer_and_unterminated_result_requires_exit() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let l = socket();
    let injected=br#"{"type":"assistant","uuid":"subagent","session_id":"session","parent_tool_use_id":"agent-tool","message":{"id":"subagent","type":"message","role":"assistant","model":"observed","usage":{"input_tokens":0,"output_tokens":1},"content":[{"type":"text","text":"INJECTED"}]}}"#;
    let mut bytes = line(br#"{"type":"future-native","data":{"number":1e-4}}"#);
    bytes.extend(line(INIT));
    bytes.extend(line(injected));
    bytes.extend_from_slice(END);
    let request = raw(dir.path(), &l, true, &bytes);
    let mut sink = Sink::new(&c);
    let (mut result, ()) = tokio::join!(
        run_qwen(&c, &host.host, request, limits(), &mut sink),
        drive(&l, &c, 1)
    );
    let receipt = result.as_ref().unwrap();
    assert_eq!(receipt.frames(), 4);
    assert_eq!(receipt.known_events(), 3);
    assert_eq!(receipt.answer(), Some("FINAL"));
    assert_eq!(sink.seen.len(), 3);
    assert_eq!(
        receipt.process().unwrap().as_ref().unwrap().pipes.stdout,
        PipeEnd::Eof
    );
    host.finish(&mut result).await;
}
#[tokio::test]
async fn unattributed_native_error_is_first_cause_and_stops_process_conservatively() {
    for success_first in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let l = socket();
        let error=br#"{"type":"result","subtype":"error_during_execution","uuid":"subagent-error","is_error":true,"duration_ms":0,"duration_api_ms":0,"num_turns":1,"session_id":"session","usage":{"input_tokens":0,"output_tokens":0},"permission_denials":[],"error":{"message":"native-error"}}"#;
        let mut bytes = line(INIT);
        if success_first {
            bytes.extend(line(END));
        }
        bytes.extend(line(error));
        let mut sink = Sink::new(&c);
        let mut result = run_qwen(
            &c,
            &host.host,
            raw(dir.path(), &l, true, &bytes),
            limits(),
            &mut sink,
        )
        .await;
        let e = result.as_ref().err().unwrap();
        assert!(matches!(e.reason(), QwenReason::Provider));
        assert_eq!(
            e.receipt().known_events(),
            if success_first { 3 } else { 2 }
        );
        assert_eq!(e.receipt().result(), Some(QwenStatus::Error));
        assert_eq!(e.receipt().candidate_text(), None);
        let native: serde_json::Value =
            serde_json::from_str(e.receipt().native_result_json().unwrap()).unwrap();
        assert_eq!(native["uuid"], "subagent-error");
        assert_eq!(e.receipt().answer(), None);
        assert!(e.receipt().process().is_some());
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn exit_and_missing_result_do_not_finalize_observed_candidates() {
    for (exit, writes, expected) in [
        (7, vec![line(INIT), line(END)], "exit"),
        (0, vec![line(INIT), line(PARTIAL)], "missing"),
    ] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let l = socket();
        let mut sink = Sink::new(&c);
        let request = sequence(
            dir.path(),
            &l,
            exit,
            &writes
                .iter()
                .map(|v| (true, v.as_slice()))
                .collect::<Vec<_>>(),
        );
        let (mut result, ()) = tokio::join!(
            run_qwen(&c, &host.host, request, limits(), &mut sink),
            drive(&l, &c, writes.len())
        );
        let e = result.as_ref().err().unwrap();
        match expected {
            "exit" => assert!(matches!(e.reason(), QwenReason::NonzeroExit)),
            _ => assert!(matches!(e.reason(), QwenReason::MissingResult)),
        }
        assert_eq!(e.receipt().answer(), None);
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn pending_callback_observes_same_original_cancel_and_deadline_and_is_dropped() {
    for cancel in [true, false] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_millis(400),
            Cancellation::default(),
        ));
        let l = socket();
        let mut sink = Sink::new(&c);
        sink.pending = true;
        let started = Arc::new(tokio::sync::Notify::new());
        sink.started = Some(started.clone());
        let request = raw(dir.path(), &l, true, &line(INIT));
        let control = async {
            started.notified().await;
            if cancel {
                c.cancellation().cancel();
            }
        };
        let (mut result, ()) = tokio::join!(
            run_qwen(&c, &host.host, request, limits(), &mut sink),
            control
        );
        let e = result.as_ref().err().unwrap();
        assert_eq!(e.receipt().answer(), None);
        assert_eq!(e.receipt().known_events(), 0);
        assert_eq!(sink.dropped, 1);
        assert!(matches!(
            e.reason(),
            QwenReason::Process | QwenReason::Context(_)
        ));
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn wire_and_process_limits_reject_without_authoritative_answer() {
    for variant in 0..4 {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let l = socket();
        let mut sink = Sink::new(&c);
        let mut policy = limits();
        let mut bytes = line(INIT);
        bytes.extend(line(END));
        match variant {
            0 => policy.wire.frame_bytes = 16,
            1 => policy.response_bytes = 2,
            2 => policy.stdout_bytes = 16,
            _ => policy.frames = 1,
        }
        let mut result = run_qwen(
            &c,
            &host.host,
            raw(dir.path(), &l, true, &bytes),
            policy,
            &mut sink,
        )
        .await;
        let e = result.as_ref().err().unwrap();
        assert_eq!(e.receipt().answer(), None);
        match variant {
            0 => assert!(matches!(
                e.reason(),
                QwenReason::FrameLimit | QwenReason::Wire
            )),
            1 => assert!(matches!(e.reason(), QwenReason::ResponseLimit)),
            2 => assert!(matches!(e.reason(), QwenReason::StdoutLimit)),
            _ => assert!(matches!(e.reason(), QwenReason::FrameLimit)),
        }
        host.finish(&mut result).await;
    }
}
#[tokio::test]
async fn native_usage_observations_keep_u64_validation_without_artificial_cap() {
    // Assistant usage remains validated by the wire decoder after removing the duplicate parse.
    let c = original();
    for value in ["18446744073709551615", "18446744073709551616", "-1", "1.5"] {
        let native = std::str::from_utf8(TOOLS)
            .unwrap()
            .replace("\"input_tokens\":5", &format!("\"input_tokens\":{value}"));
        let decoded = decode_qwen_frame(native.as_bytes(), &limits().wire, &c);
        if value == "18446744073709551615" {
            assert!(matches!(decoded, Ok(QwenDecodedFrame::Known(_))));
        } else {
            assert!(decoded.is_err());
        }
    }
    // Native observations are parsed as u64; Graphit imposes no execution quota on them.
    for value in ["18446744073709551615", "18446744073709551616", "-1", "1.5"] {
        for model_usage in [false, true] {
            let dir = TempDir::new().unwrap();
            let host = Host::new().await;
            let c = original();
            let l = socket();
            let mut sink = Sink::new(&c);
            let field = if model_usage {
                "\"inputTokens\":5"
            } else {
                "\"input_tokens\":5"
            };
            let replacement = format!("{}:{value}", field.split_once(':').unwrap().0);
            let native = std::str::from_utf8(END)
                .unwrap()
                .replace(field, &replacement);
            let mut bytes = line(INIT);
            bytes.extend_from_slice(native.as_bytes());
            let mut result = run_qwen(
                &c,
                &host.host,
                raw(dir.path(), &l, true, &bytes),
                limits(),
                &mut sink,
            )
            .await;
            if value == "18446744073709551615" {
                let receipt = result.as_ref().unwrap();
                assert_eq!(receipt.answer(), Some("FINAL"));
                assert_eq!(receipt.native_result_json(), Some(native.as_str()));
                if !model_usage {
                    assert_eq!(
                        receipt.usage().unwrap().input_tokens,
                        value.parse::<u64>().unwrap()
                    );
                }
            } else {
                let error = result.as_ref().err().unwrap();
                assert!(matches!(error.reason(), QwenReason::Wire));
                assert!(error.receipt().answer().is_none());
            }
            host.finish(&mut result).await;
        }
    }
}
#[test]
fn canonical_schema_requires_native_parent_ids_usage_and_result_without_flat_aliases() {
    let c = original();
    let policy = limits();
    for bytes in [
        br#"{"type":"stream_event","uuid":"u","session_id":"session","parent_tool_use_id":null,"delta":{"text":"flat"}}"#.as_slice(),
        br#"{"type":"assistant","uuid":"a","session_id":"session","message":{"id":"a","type":"message","role":"assistant","model":"m","usage":{"input_tokens":0,"output_tokens":0},"content":[]}}"#,
        br#"{"type":"assistant","uuid":"a","session_id":"session","parent_tool_use_id":null,"message":{"id":"a","type":"message","role":"assistant","model":"m","usage":{"input_tokens":0,"output_tokens":0},"content":[{"type":"tool_use","id":"","name":"Read","input":{}}]}}"#,
        br#"{"type":"assistant","uuid":"a","session_id":"session","parent_tool_use_id":null,"message":{"id":"a","type":"message","role":"assistant","model":"m","usage":{"input_tokens":0,"output_tokens":0},"content":[{"type":"tool_use","id":"i","name":"Read"}]}}"#,
        br#"{"type":"result","subtype":"success","uuid":"end","is_error":false,"duration_ms":1,"duration_api_ms":1,"num_turns":1,"session_id":"session","usage":{"input_tokens":0,"output_tokens":0},"permission_denials":[]}"#,
        br#"{"type":"stream_event","uuid":"u","session_id":"session","parent_tool_use_id":null,"event":{"type":"tool_progress","content":{}}}"#,
        br#"{"type":"result","subtype":"success","uuid":"end","is_error":false,"duration_ms":1,"duration_api_ms":1,"num_turns":1,"session_id":"session","result":"x","usage":{"output_tokens":0},"permission_denials":[]}"#,
    ] {
        let e=decode_qwen_frame(bytes,&policy.wire,&c).err().unwrap();
        assert!(!format!("{e:?}").contains("flat"));
    }
    assert!(matches!(
        decode_qwen_frame(br#"{"type":"new-kind","native":1e-4}"#, &policy.wire, &c).unwrap(),
        QwenDecodedFrame::Ignored
    ));
    let QwenDecodedFrame::Known(e) = decode_qwen_frame(TOOLS, &policy.wire, &c).unwrap() else {
        panic!("known")
    };
    assert_eq!(e.native.get().as_bytes(), TOOLS);
}
#[test]
fn qwen_unknown_tool_inputs_optional_thinking_and_goal_progress_controls_stay_native() {
    let c = original();
    let policy = limits();
    for raw in [
        br#"{"type":"assistant","uuid":"a","session_id":"session","parent_tool_use_id":"p","message":{"id":"a","type":"message","role":"assistant","model":"m","usage":{"input_tokens":0,"output_tokens":0},"content":[{"type":"tool_use","id":"i","name":"Read","input":null},{"type":"tool_use","id":"j","name":"Read","input":[9007199254740993,1e-4]},{"type":"thinking","thinking":"native"}]}}"#.as_slice(),
        br#"{"type":"stream_event","uuid":"u","session_id":"session","parent_tool_use_id":null,"event":{"type":"goal_state","goal_state":{"version":2,"native":1e-4}}}"#,
        br#"{"type":"stream_event","uuid":"u","session_id":"session","parent_tool_use_id":null,"event":{"type":"tool_progress","tool_use_id":"native","content":{"type":"shell","native":1e-4}}}"#,
        br#"{"type":"control_request","request_id":"control","request":{"subtype":"can_use_tool","tool_name":"Read","input":{"native":1e-4}}}"#,
        br#"{"type":"control_response","response":{"subtype":"success","request_id":"control","response":{"allowed":false}}}"#,
    ] {
        let QwenDecodedFrame::Known(e)=decode_qwen_frame(raw,&policy.wire,&c).unwrap() else {panic!("known")};
        assert_eq!(e.native.get().as_bytes(),raw);
    }
}
