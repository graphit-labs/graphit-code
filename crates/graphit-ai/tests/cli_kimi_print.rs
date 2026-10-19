use graphit_ai::cli_kimi_print::{
    KimiPrintEvent, KimiPrintEventData, KimiPrintFailure, KimiPrintLimits, KimiPrintReason,
    KimiPrintReceipt, KimiPrintSink, run_kimi_print,
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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::{sync::Notify, time::sleep};

#[tokio::test]
async fn invalid_print_policy_and_cancelled_original_never_spawn() {
    for cancel in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let listener = socket();
        let mut sink = Sink::new(&c);
        let mut limits = wire();
        limits.depth = 0;
        if cancel {
            c.cancellation().cancel();
        }
        let native = request(dir.path(), &listener, "raw", &[(true, b"{}")], 0);
        let mut result =
            run_kimi_print(&c, &host.host, native, &|| Ok(()), limits, &mut sink).await;
        let failure = result.as_ref().err().unwrap();
        if cancel {
            assert!(matches!(
                failure.reason(),
                KimiPrintReason::Context(io::ErrorKind::Interrupted)
            ));
        } else {
            assert!(matches!(failure.reason(), KimiPrintReason::Policy));
        }
        assert!(failure.receipt().process().is_none());
        assert!(matches!(listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock));
        assert!(sink.frames.is_empty());
        host.finish(&mut result).await;
    }
}

fn wire() -> KimiPrintLimits {
    KimiPrintLimits {
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
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    ))
}
struct Host {
    host: ProcessHost,
    service: Cancellation,
}
impl Host {
    async fn new() -> Self {
        let service = Cancellation::default();
        let host = ProcessHost::install(&service, HostLimits::new(1, 1).unwrap()).unwrap();
        assert!(matches!(
            host.ready(Instant::now() + Duration::from_secs(5)).await,
            HostReadiness::Ready
        ));
        Self { host, service }
    }
    async fn finish(self, result: &mut Result<KimiPrintReceipt, KimiPrintFailure>) {
        let receipt = match result {
            Ok(r) => r,
            Err(e) => e.receipt_mut(),
        };
        let remaining = match receipt.process_mut() {
            Some(Ok(o)) => o.remaining.as_mut(),
            Some(Err(e)) => e.remaining.as_mut(),
            None => None,
        };
        if let Some(remaining) = remaining {
            assert!(matches!(
                remaining
                    .repair(Instant::now() + Duration::from_secs(5))
                    .await,
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
                    _ => panic!("pending host"),
                }
            }
        };
        assert!(report.failure().is_none());
    }
}
fn socket() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}
fn request(
    dir: &Path,
    listener: &TcpListener,
    mode: &str,
    writes: &[(bool, &[u8])],
    exit: i32,
) -> Request {
    let mut args: Vec<OsString> = vec![
        mode.into(),
        listener.local_addr().unwrap().to_string().into(),
    ];
    if mode == "sequence" {
        args.push(exit.to_string().into());
    }
    for (out, bytes) in writes {
        args.push(if *out { "out".into() } else { "err".into() });
        args.push(
            bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into(),
        );
    }
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
async fn byte(p: &mut TcpStream, c: &SnapshotContext) -> Option<u8> {
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
            _ => panic!("ack"),
        }
    }
}
struct Sink<'a> {
    original: &'a SnapshotContext,
    frames: Vec<String>,
    kinds: Vec<&'static str>,
    deny: Option<&'a AtomicBool>,
    fail: bool,
    pending: bool,
    started: Arc<Notify>,
    dropped: usize,
}
impl<'a> Sink<'a> {
    fn new(original: &'a SnapshotContext) -> Self {
        Self {
            original,
            frames: Vec::new(),
            kinds: Vec::new(),
            deny: None,
            fail: false,
            pending: false,
            started: Arc::new(Notify::new()),
            dropped: 0,
        }
    }
}
impl KimiPrintSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        event: KimiPrintEvent<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        assert!(std::ptr::eq(self.original, original));
        self.kinds.push(match &event.data {
            KimiPrintEventData::Assistant {
                content,
                tool_calls,
            } => {
                assert!(content.is_some() || tool_calls.is_some());
                "assistant"
            }
            KimiPrintEventData::Tool {
                tool_call_id,
                content,
            } => {
                assert!(!tool_call_id.is_empty());
                let _ = content;
                "tool"
            }
            KimiPrintEventData::Meta { kind } => {
                assert!(!kind.is_empty());
                "meta"
            }
            KimiPrintEventData::Unknown => "unknown",
        });
        self.frames.push(event.native.get().to_owned());
        self.started.notify_one();
        if let Some(deny) = self.deny {
            deny.store(true, Ordering::SeqCst);
        }
        if self.pending {
            struct PendingDrop<'a>(&'a mut usize);
            impl Drop for PendingDrop<'_> {
                fn drop(&mut self) {
                    *self.0 += 1;
                }
            }
            let _drop = PendingDrop(&mut self.dropped);
            std::future::pending::<()>().await;
        }
        if self.fail {
            Err(SinkFailure::Closed)
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn official_print_messages_stream_over_existing_process_without_retention() {
    let dir = TempDir::new().unwrap();
    let host = Host::new().await;
    let c = original();
    let listener = socket();
    let mut sink = Sink::new(&c);
    let started = sink.started.clone();
    let first = b"{\"role\":\"meta\",\"type\":\"system.version\",\"version\":\"2.2.0\"}\r\n";
    let later = "{\"role\":\"assistant\",\"content\":\"hook or model 尾\",\"tool_calls\":[{\"type\":\"function\",\"id\":\"call\",\"function\":{\"name\":\"mcp_role\",\"arguments\":\"{}\"}}]}\n{\"role\":\"tool\",\"tool_call_id\":\"call\",\"content\":\"private tool result\"}\n{\"role\":\"meta\",\"type\":\"turn.step.retrying\",\"failed_attempt\":1,\"next_attempt\":2,\"max_attempts\":3,\"delay_ms\":2,\"error_name\":\"private\",\"error_message\":\"private\"}\n{\"role\":\"future\",\"opaque\":[1,2]}\n{\"role\":\"meta\",\"type\":\"session.resume_hint\",\"session_id\":\"private-id\",\"command\":\"kimi -r private-id\",\"content\":\"private hint\"}";
    let split = later.find('尾').unwrap() + 1;
    let writes = [
        (true, &first[..first.len() - 1]),
        (true, &first[first.len() - 1..]),
        (true, &later.as_bytes()[..split]),
        (true, &later.as_bytes()[split..]),
    ];
    let native = request(dir.path(), &listener, "sequence", &writes, 0);
    let control = async {
        let mut p = peer(&listener, &c).await;
        assert_eq!(byte(&mut p, &c).await, Some(b'L'));
        ack(&mut p, &c).await;
        assert_eq!(byte(&mut p, &c).await, Some(b'L'));
        started.notified().await;
        // Native is still awaiting this ACK: the callback occurred before exit/EOF.
        assert!(matches!(p.read(&mut [0]),Err(e) if e.kind()==io::ErrorKind::WouldBlock));
        ack(&mut p, &c).await;
        for _ in 0..2 {
            assert_eq!(byte(&mut p, &c).await, Some(b'L'));
            ack(&mut p, &c).await;
        }
    };
    let (mut result, ()) = tokio::join!(
        run_kimi_print(&c, &host.host, native, &|| Ok(()), wire(), &mut sink),
        control
    );
    let receipt = result.as_ref().unwrap();
    let outcome = receipt.process().unwrap().as_ref().unwrap();
    assert!(outcome.status.success());
    assert_eq!(outcome.cleanup, Cleanup::Complete);
    assert_eq!(outcome.pipes.stdout, PipeEnd::Eof);
    assert_eq!(outcome.pipes.stderr, PipeEnd::Eof);
    assert!(outcome.remaining.is_none());
    assert!(outcome.transcript.is_none());
    assert_eq!(
        sink.kinds,
        vec!["meta", "assistant", "tool", "meta", "unknown", "meta"]
    );
    assert!(sink.frames[1].contains("hook or model 尾"));
    assert!(!format!("{receipt:?}").contains("private"));
    host.finish(&mut result).await;
}

#[tokio::test]
async fn caller_recheck_denies_before_spawn_and_after_observed_message() {
    for after in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let listener = socket();
        let denied = AtomicBool::new(!after);
        let mut sink = Sink::new(&c);
        sink.deny = Some(&denied);
        let recheck = || {
            if denied.load(Ordering::SeqCst) {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "PRIVATE-RECHECK",
                ))
            } else {
                Ok(())
            }
        };
        let native = request(
            dir.path(),
            &listener,
            "raw",
            &[(true, b"{\"role\":\"assistant\",\"content\":\"observed\"}\n")],
            0,
        );
        let mut result = run_kimi_print(&c, &host.host, native, &recheck, wire(), &mut sink).await;
        let error = result.as_ref().err().unwrap();
        assert!(matches!(
            error.reason(),
            KimiPrintReason::Recheck(io::ErrorKind::PermissionDenied)
        ));
        assert_eq!(sink.frames.len(), usize::from(after));
        assert_eq!(error.receipt().process().is_some(), after);
        assert!(!format!("{error:?} {error}").contains("PRIVATE-RECHECK"));
        assert!(
            !std::error::Error::source(error)
                .unwrap()
                .to_string()
                .contains("PRIVATE-RECHECK")
        );
        if !after {
            assert!(matches!(listener.accept(),Err(e) if e.kind()==io::ErrorKind::WouldBlock));
        }
        host.finish(&mut result).await;
    }
}

#[tokio::test]
async fn print_schema_message_and_process_limits_preserve_real_failure() {
    for case in 0..7 {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let listener = socket();
        let mut sink = Sink::new(&c);
        let mut limits = wire();
        let bytes:&[u8]=match case{0=>b"{\"role\":\"assistant\",\"content\":false}\n",1=>b"{\"role\":\"assistant\",\"content\":\"a\",\"content\":\"b\"}\n",2=>b"{\"role\":\"tool\",\"tool_call_id\":\"x\",\"content\":\"tool\"}",3=>b"{\"role\":\"meta\",\"type\":\"system.version\"}\n",4=>b"private stderr bytes",5=>b"{\"role\":\"assistant\",\"tool_calls\":[{\"type\":\"bad\",\"id\":\"x\",\"function\":{\"name\":\"x\",\"arguments\":\"{}\"}}]}\n",_=>b"{invalid"};
        let mut native = request(dir.path(), &listener, "raw", &[(case != 4, bytes)], 0);
        match case {
            2 => limits.frame_bytes = 8,
            3 => native.limits.stdout_bytes = 8,
            4 => native.limits.stderr_bytes = 8,
            _ => {}
        }
        let mut result =
            run_kimi_print(&c, &host.host, native, &|| Ok(()), limits, &mut sink).await;
        let error = result.as_ref().err().unwrap();
        match case {
            0 | 1 | 5 => assert!(matches!(error.reason(), KimiPrintReason::Schema)),
            6 => assert!(matches!(error.reason(), KimiPrintReason::Wire)),
            2 => assert!(matches!(error.reason(), KimiPrintReason::FrameLimit)),
            3 | 4 => assert!(matches!(error.reason(), KimiPrintReason::Process)),
            _ => unreachable!(),
        }
        assert!(error.receipt().process().is_some());
        assert!(sink.frames.is_empty());
        host.finish(&mut result).await;
    }
}

#[tokio::test]
async fn print_nonzero_exit_and_sink_rejection_are_not_success() {
    for fail_sink in [false, true] {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = original();
        let listener = socket();
        let mut sink = Sink::new(&c);
        sink.fail = fail_sink;
        let bytes = b"{\"role\":\"assistant\",\"content\":\"private candidate\"}\n";
        let native = request(dir.path(), &listener, "sequence", &[(true, bytes)], 7);
        let started = sink.started.clone();
        let control = async {
            let mut p = peer(&listener, &c).await;
            assert_eq!(byte(&mut p, &c).await, Some(b'L'));
            if fail_sink {
                started.notified().await;
            } else {
                ack(&mut p, &c).await;
            }
        };
        let (mut result, ()) = tokio::join!(
            run_kimi_print(&c, &host.host, native, &|| Ok(()), wire(), &mut sink),
            control
        );
        let error = result.as_ref().err().unwrap();
        if fail_sink {
            assert!(matches!(
                error.reason(),
                KimiPrintReason::Sink(SinkFailure::Closed)
            ));
        } else {
            assert!(matches!(error.reason(), KimiPrintReason::NonzeroExit));
            assert!(matches!(error.receipt().process(),Some(Ok(o)) if o.status.code()==Some(7)));
        }
        assert_eq!(sink.frames.len(), 1);
        host.finish(&mut result).await;
    }
}

#[tokio::test]
async fn original_and_host_stop_pending_callbacks_with_same_process_owner() {
    for stop in 0..3 {
        let dir = TempDir::new().unwrap();
        let host = Host::new().await;
        let c = if stop == 1 {
            Arc::new(SnapshotContext::new(
                Instant::now() + Duration::from_secs(1),
                Cancellation::default(),
            ))
        } else {
            original()
        };
        let listener = socket();
        let mut sink = Sink::new(&c);
        sink.pending = true;
        let started = sink.started.clone();
        let service = host.service.clone();
        // Without LF, the callback runs only after Process has actually exited and reached EOF.
        // The host-stop case instead holds a live child in the existing sequence fixture.
        let bytes = if stop == 2 {
            &b"{\"role\":\"assistant\",\"content\":\"pending\"}\n"[..]
        } else {
            &b"{\"role\":\"assistant\",\"content\":\"pending\"}"[..]
        };
        let native = request(
            dir.path(),
            &listener,
            if stop == 2 { "sequence" } else { "raw" },
            &[(true, bytes)],
            0,
        );
        let control = async {
            started.notified().await;
            if stop == 0 {
                c.cancellation().cancel();
            } else if stop == 2 {
                service.cancel();
            }
        };
        let (mut result, ()) = tokio::join!(
            run_kimi_print(&c, &host.host, native, &|| Ok(()), wire(), &mut sink),
            control
        );
        let error = result.as_ref().err().unwrap();
        if stop == 2 {
            assert!(matches!(error.reason(), KimiPrintReason::Process));
            assert!(c.check().is_ok());
        } else {
            let expected = if stop == 0 {
                io::ErrorKind::Interrupted
            } else {
                io::ErrorKind::TimedOut
            };
            assert!(matches!(error.reason(),KimiPrintReason::Context(kind) if kind==expected));
            assert!(
                matches!(error.receipt().process(),Some(Ok(o)) if o.pipes.stdout==PipeEnd::Eof&&o.pipes.stderr==PipeEnd::Eof&&o.cleanup==Cleanup::Complete)
            );
        }
        assert_eq!(sink.frames.len(), 1);
        assert_eq!(sink.dropped, 1);
        host.finish(&mut result).await;
    }
}
