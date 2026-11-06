#[path = "support/host_fixture.rs"]
mod host_fixture;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Cleanup, Failure, IgnoreChunks, InvalidRequest, Limits, Outcome, Reason, Request, Retention,
    Stage, Stream, run,
};
use tempfile::TempDir;
use tokio::time::{Instant, sleep, timeout};

fn assert_owned_cleanup(cleanup: Cleanup) {
    assert_eq!(cleanup, Cleanup::Complete);
}

#[tokio::test(flavor = "current_thread")]
async fn zero_and_nonzero_exit_preserve_status_and_cleanup() {
    let dir = TempDir::new().unwrap();
    let token = Cancellation::default();
    let context = original(&token, Duration::from_secs(5));
    for (mode, code) in [("close-input", 0), ("bytes", 7)] {
        let result = execute(request(dir.path(), &[mode]), &context)
            .await
            .unwrap();
        assert_eq!(result.status.code(), Some(code));
        assert_owned_cleanup(result.cleanup);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn spawn_failure_does_not_create_child() {
    let dir = TempDir::new().unwrap();
    let token = Cancellation::default();
    let context = original(&token, Duration::from_secs(5));
    let mut spec = request(dir.path(), &[]);
    spec.program = dir.path().join("absent-native-program.exe");
    let failure = execute(spec, &context).await.unwrap_err();
    assert!(matches!(failure.reason, Reason::Io(Stage::Spawn, _)));
    assert_eq!(failure.cleanup, Cleanup::NotSpawned);
    assert!(failure.status.is_none());
    assert_eq!(failure.pipes.stdout, graphit_process::PipeEnd::NotSpawned);
    assert_eq!(failure.pipes.stderr, graphit_process::PipeEnd::NotSpawned);
    assert_eq!(failure.observed, graphit_process::Counts::default());
}

fn request(dir: &Path, args: &[&str]) -> Request {
    Request {
        program: PathBuf::from(env!("CARGO_BIN_EXE_subprocess-fixture")),
        args: args.iter().map(OsString::from).collect(),
        cwd: dir.to_path_buf(),
        // Kernel DLL discovery requires captured SystemRoot on Windows. No
        // implicit inheritance inside the runner; HOME is deliberately absent.
        environment: std::env::vars_os()
            .filter(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT"))
            .collect(),
        stdin: None,
        limits: Limits {
            stdin_bytes: 1024 * 1024,
            stdout_bytes: 1024 * 1024,
            stderr_bytes: 1024 * 1024,
            termination_grace: Duration::from_millis(30),
            cleanup: Duration::from_secs(1),
        },
    }
}

fn original(cancellation: &Cancellation, budget: Duration) -> std::sync::Arc<SnapshotContext> {
    std::sync::Arc::new(SnapshotContext::new(
        std::time::Instant::now().checked_add(budget).unwrap(),
        cancellation.clone(),
    ))
}
async fn execute(
    spec: Request,
    original: &std::sync::Arc<SnapshotContext>,
) -> Result<Outcome, Failure> {
    execute_operation(spec, original, Retention::Full, &mut IgnoreChunks).await
}

async fn ready(path: &Path) {
    timeout(Duration::from_secs(2), async {
        while !path.exists() {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("fixture never became ready");
}
async fn stopped(path: &Path) {
    let before = fs::read(path).unwrap();
    sleep(Duration::from_millis(100)).await;
    assert_eq!(before, fs::read(path).unwrap(), "descendant still alive");
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_context_binary_streams_and_nonzero_exit() {
    let root = TempDir::new().unwrap();
    let cwd = root.path().join("cwd with spaces");
    fs::create_dir(&cwd).unwrap();
    let literal = "secret-canary; $(touch unwanted) & | *";
    let mut spec = request(&cwd, &["probe", literal]);
    spec.environment.push((
        "GRAPHIT_PROCESS_EXPLICIT".into(),
        "secret-env-canary".into(),
    ));
    let debug = format!("{spec:?}");
    assert!(!debug.contains(literal));
    assert!(!debug.contains("secret-env-canary"));
    assert!(!debug.contains(&cwd.to_string_lossy().to_string()));
    let result = execute(
        spec,
        &original(&Cancellation::default(), Duration::from_secs(3)),
    )
    .await
    .unwrap();
    assert!(result.status.success());
    let output = String::from_utf8_lossy(result.transcript.as_ref().unwrap().stdout());
    #[cfg(unix)]
    let expected_cwd = fs::canonicalize(&cwd).unwrap();
    #[cfg(windows)]
    let expected_cwd = cwd.clone();
    assert!(output.contains(&format!("cwd={}", expected_cwd.display())));
    assert!(output.contains("ambient=absent"));
    assert!(output.contains("explicit=secret-env-canary"));
    assert!(output.contains(&format!("arg={literal}")));
    assert!(!format!("{result:?}").contains("secret-"));
    assert!(!cwd.join("unwanted").exists());
    let result = execute(
        request(&cwd, &["bytes"]),
        &original(&Cancellation::default(), Duration::from_secs(3)),
    )
    .await
    .unwrap();
    assert_eq!(result.status.code(), Some(7));
    assert_eq!(
        result.transcript.as_ref().unwrap().stdout(),
        &[0, 255, 128, b'x']
    );
    assert_eq!(
        result.transcript.as_ref().unwrap().stderr(),
        &[254, 0, b'e']
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stdin_eof_null_early_close_and_concurrent_duplex() {
    let dir = TempDir::new().unwrap();
    let cancellation = Cancellation::default();
    let context = original(&cancellation, Duration::from_secs(3));
    let result = execute(request(dir.path(), &["echo"]), &context)
        .await
        .unwrap();
    assert!(result.transcript.as_ref().unwrap().stdout().is_empty());
    let mut spec = request(dir.path(), &["echo"]);
    spec.stdin = Some(vec![0, 255, b'x'].into());
    let result = execute(spec, &context).await.unwrap();
    assert_eq!(
        result.transcript.as_ref().unwrap().stdout(),
        &[0, 255, b'x']
    );
    let mut spec = request(dir.path(), &["close-input"]);
    spec.stdin = Some(vec![b'x'; 512 * 1024].into());
    let result = execute(spec, &context).await.unwrap();
    assert!(result.status.success());
    assert_eq!(result.transcript.as_ref().unwrap().stdout(), b"closed\n");
    let mut spec = request(dir.path(), &["duplex"]);
    spec.stdin = Some(vec![b'x'; 512 * 1024].into());
    let result = execute(spec, &context).await.unwrap();
    assert_eq!(
        result.transcript.as_ref().unwrap().stdout(),
        vec![b'o'; 256 * 1024]
    );
    assert_eq!(
        result.transcript.as_ref().unwrap().stderr(),
        vec![b'e'; 256 * 1024]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn exact_output_limits_zero_and_unaccepted_violation() {
    let dir = TempDir::new().unwrap();
    let token = Cancellation::default();
    let context = original(&token, Duration::from_secs(3));
    let mut spec = request(dir.path(), &["flood", "8192"]);
    spec.limits.stdout_bytes = 8192;
    spec.limits.stderr_bytes = 8192;
    let result = execute(spec, &context).await.unwrap();
    assert_eq!(result.transcript.as_ref().unwrap().stdout().len(), 8192);
    assert_eq!(result.transcript.as_ref().unwrap().stderr().len(), 8192);
    for (stdout, stderr) in [(8191, 8192), (8192, 8191), (0, 0)] {
        let mut spec = request(dir.path(), &["flood", "8192"]);
        spec.limits.stdout_bytes = stdout;
        spec.limits.stderr_bytes = stderr;
        let error = execute(spec, &context).await.unwrap_err();
        assert!(matches!(
            error.reason,
            Reason::OutputLimit(Stream::Stdout | Stream::Stderr)
        ));
        assert_owned_cleanup(error.cleanup);
        match error.reason {
            Reason::OutputLimit(Stream::Stdout) => {
                assert_eq!(error.observed.stdout, stdout as u64 + 1)
            }
            Reason::OutputLimit(Stream::Stderr) => {
                assert_eq!(error.observed.stderr, stderr as u64 + 1)
            }
            _ => unreachable!(),
        }
        assert!(error.delivered.stdout <= stdout as u64);
        assert!(error.delivered.stderr <= stderr as u64);
        assert!(error.transcript.as_ref().unwrap().stdout().len() <= stdout);
        assert!(error.transcript.as_ref().unwrap().stderr().len() <= stderr);
        assert!(!format!("{error:?} {error}").contains("oooo"));
    }
    let mut spec = request(dir.path(), &["echo"]);
    spec.limits.stdout_bytes = 0;
    spec.limits.stderr_bytes = 0;
    assert!(execute(spec, &context).await.unwrap().status.success());
}

#[tokio::test(flavor = "current_thread")]
async fn validation_precancel_and_spawn_failure_never_run_fixture() {
    let host = host_fixture::install(8).await;
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("spawned");
    let args = ["ready", marker.to_str().unwrap()];
    let token = Cancellation::default();
    let context = original(&token, Duration::from_secs(3));
    let (_stop, receiver) = graphit_process::operation(&context, &host).unwrap();
    token.cancel();
    let error = run(
        request(dir.path(), &args),
        &context,
        Retention::Full,
        &mut IgnoreChunks,
        receiver,
    )
    .await
    .unwrap_err();
    assert_eq!(error.reason, Reason::Cancelled);
    assert_eq!(error.cleanup, Cleanup::NotSpawned);
    let token = Cancellation::default();
    let context = original(&token, Duration::from_secs(3));
    for invalid in [
        InvalidRequest::Program,
        InvalidRequest::Cwd,
        InvalidRequest::Budget,
        InvalidRequest::StdinLimit,
    ] {
        let mut spec = request(dir.path(), &args);
        match invalid {
            InvalidRequest::Program => spec.program = "relative".into(),
            InvalidRequest::WindowsExecutable => unreachable!("separate Windows fixture"),
            InvalidRequest::InvalidRequestContext => unreachable!("separate identity fixture"),
            InvalidRequest::Cwd => spec.cwd = "relative".into(),
            InvalidRequest::Budget => spec.limits.cleanup = Duration::ZERO,
            InvalidRequest::StdinLimit => {
                spec.limits.stdin_bytes = 1;
                spec.stdin = Some(vec![0; 2].into());
            }
        }
        let error = execute(spec, &context).await.unwrap_err();
        assert_eq!(error.reason, Reason::Invalid(invalid));
        assert_eq!(error.cleanup, Cleanup::NotSpawned);
    }
    for duration in [Duration::MAX, Duration::ZERO] {
        let mut spec = request(dir.path(), &args);
        spec.limits.cleanup = duration;
        assert_eq!(
            execute(spec, &context).await.unwrap_err().reason,
            Reason::Invalid(InvalidRequest::Budget)
        );
    }
    let mut spec = request(dir.path(), &args);
    spec.limits.stdout_bytes = usize::MAX;
    spec.limits.stderr_bytes = usize::MAX;
    assert_eq!(
        execute(spec, &context).await.unwrap_err().reason,
        Reason::Invalid(InvalidRequest::Budget)
    );
    let mut spec = request(dir.path(), &args);
    spec.program = dir.path().join("secret-missing-program.exe");
    let error = execute(spec, &context).await.unwrap_err();
    assert!(matches!(error.reason, Reason::Io(Stage::Spawn, _)));
    assert!(!format!("{error} {error:?}").contains("secret-missing"));
    assert!(!marker.exists());
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
}

#[cfg(windows)]
#[tokio::test(flavor = "current_thread")]
async fn windows_batch_paths_fail_before_spawn() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("shell-ran");
    for suffix in ["bat", "CMD", "exe."] {
        let path = dir.path().join(format!("wrapper.{suffix}"));
        fs::write(&path, format!("@echo shell > \"{}\"", marker.display())).unwrap();
        let mut spec = request(dir.path(), &["probe"]);
        spec.program = path;
        let error = execute(
            spec,
            &original(&Cancellation::default(), Duration::from_secs(3)),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.reason,
            Reason::Invalid(InvalidRequest::WindowsExecutable)
        );
        assert_eq!(error.cleanup, Cleanup::NotSpawned);
    }
    assert!(!marker.exists());
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_deadline_and_normal_exit_clean_descendants() {
    let dir = TempDir::new().unwrap();
    for mode in ["cancel", "deadline", "normal"] {
        let heartbeat = dir.path().join(mode);
        let child_mode = if mode == "normal" {
            "tree-exit"
        } else {
            "tree"
        };
        let spec = request(dir.path(), &[child_mode, heartbeat.to_str().unwrap()]);
        let token = Cancellation::default();
        let context = original(&token, Duration::from_secs(3));
        let cancel = token.clone();
        let task = tokio::spawn(async move { execute(spec, &context).await });
        ready(&heartbeat).await;
        if mode == "cancel" {
            cancel.cancel();
        }
        let start = Instant::now();
        let result = timeout(Duration::from_secs(6), task)
            .await
            .unwrap()
            .unwrap();
        assert!(start.elapsed() < Duration::from_secs(6));
        match mode {
            "normal" => assert_eq!(result.unwrap().status.code(), Some(7)),
            "cancel" => {
                let e = result.unwrap_err();
                assert_eq!(e.reason, Reason::Cancelled);
                assert_owned_cleanup(e.cleanup);
            }
            _ => {
                let e = result.unwrap_err();
                assert_eq!(e.reason, Reason::TimedOut);
                assert_owned_cleanup(e.cleanup);
            }
        }
        stopped(&heartbeat).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn aborting_future_requests_tree_kill() {
    let dir = TempDir::new().unwrap();
    let heartbeat = dir.path().join("aborted");
    let spec = request(dir.path(), &["tree", heartbeat.to_str().unwrap()]);
    let context = original(&Cancellation::default(), Duration::from_secs(3));
    let host = std::sync::Arc::new(host_fixture::install(8).await);
    let task_host = std::sync::Arc::clone(&host);
    let task = tokio::spawn(async move {
        let (_stop, receiver) = graphit_process::operation(&context, &task_host).unwrap();
        run(spec, &context, Retention::Full, &mut IgnoreChunks, receiver).await
    });
    ready(&heartbeat).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let host =
        std::sync::Arc::try_unwrap(host).unwrap_or_else(|_| panic!("aborted task retained host"));
    let report = host_fixture::joined(host).await;
    assert!(report.failure().is_none());
    stopped(&heartbeat).await;
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn graceful_term_is_delivered_and_escalation_survives_leader_exit() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("graceful");
    let spec = request(dir.path(), &["term", marker.to_str().unwrap()]);
    let token = Cancellation::default();
    let context = original(&token, Duration::from_secs(3));
    let cancel = token.clone();
    let task = tokio::spawn(async move { execute(spec, &context).await });
    ready(&marker).await;
    cancel.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.reason, Reason::Cancelled);
    assert_owned_cleanup(error.cleanup);
    assert!(marker.with_extension("term").exists());
    // The shared tree cases use a TERM-blocking descendant. On cancel the
    // leader exits immediately; its descendant must still receive SIGKILL.
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_overflow_and_cancellation_recover() {
    let dir = TempDir::new().unwrap();
    for attempt in 0..20 {
        let mut spec = request(dir.path(), &["flood", "65536"]);
        spec.limits.stdout_bytes = 1024;
        spec.limits.stderr_bytes = 1024;
        let error = execute(
            spec,
            &original(&Cancellation::default(), Duration::from_secs(3)),
        )
        .await
        .unwrap_err();
        assert!(matches!(error.reason, Reason::OutputLimit(_)));
        assert_owned_cleanup(error.cleanup);
        let marker = dir.path().join(format!("cancel-{attempt}"));
        let spec = request(dir.path(), &["ready", marker.to_str().unwrap()]);
        let token = Cancellation::default();
        let context = original(&token, Duration::from_secs(3));
        let cancel = token.clone();
        let task = tokio::spawn(async move { execute(spec, &context).await });
        ready(&marker).await;
        cancel.cancel();
        let error = timeout(Duration::from_secs(6), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.reason, Reason::Cancelled);
        assert_owned_cleanup(error.cleanup);
    }
    let result = execute(
        request(dir.path(), &["bytes"]),
        &original(&Cancellation::default(), Duration::from_secs(3)),
    )
    .await
    .unwrap();
    assert_eq!(result.status.code(), Some(7));
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
