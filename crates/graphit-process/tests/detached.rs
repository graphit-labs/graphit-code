#![cfg(target_os = "linux")]
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{DetachedRequest, spawn_detached};
use std::{
    fs,
    io::Read,
    path::Path,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

fn original(duration: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + duration,
        Cancellation::default(),
    ))
}
fn request(root: &Path, mode: &str) -> DetachedRequest {
    DetachedRequest {
        program: std::env::current_exe().unwrap(),
        args: vec![
            "--exact".into(),
            "native_child".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ],
        cwd: root.to_owned(),
        environment: vec![
            ("DETACHED_FIXTURE".into(), root.as_os_str().to_owned()),
            ("DETACHED_MODE".into(), mode.into()),
            ("LITERAL".into(), "a % quoted \" Ω".into()),
        ],
        termination_grace: Duration::from_millis(50),
        cleanup: Duration::from_secs(3),
    }
}
async fn capture(root: &Path) -> String {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(text) = fs::read_to_string(root.join("capture")) {
            return text;
        }
        assert!(Instant::now() < end, "native child did not publish capture");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

// Controlled native SDK child, not a Graphit daemon or commercial-host proof.
#[test]
#[ignore]
fn native_child() {
    let root = std::env::var_os("DETACHED_FIXTURE").expect("private native SDK fixture");
    let root = Path::new(&root);
    let signal_runtime = (std::env::var("DETACHED_MODE").unwrap() == "ignore").then(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    });
    let _entered = signal_runtime.as_ref().map(|runtime| runtime.enter());
    let _term = signal_runtime.as_ref().map(|_| {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap()
    });
    let stat = fs::read_to_string("/proc/self/stat").unwrap();
    let tail = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    let capture = format!(
        "pid={}\npgid={}\nsid={}\ncwd={}\nliteral={}\nstdin={}\nfd0={}\nfd1={}\nfd2={}\n",
        std::process::id(),
        tail[2],
        tail[3],
        std::env::current_dir().unwrap().display(),
        std::env::var("LITERAL").unwrap(),
        input.len(),
        fs::read_link("/proc/self/fd/0").unwrap().display(),
        fs::read_link("/proc/self/fd/1").unwrap().display(),
        fs::read_link("/proc/self/fd/2").unwrap().display()
    );
    fs::write(root.join("capture"), capture).unwrap();
    if std::env::var("DETACHED_MODE").unwrap() == "group" {
        #[expect(
            clippy::zombie_processes,
            reason = "The native leader deliberately exits before its inherited group member; the SDK cancellation regression owns termination."
        )]
        let _member = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "native_member", "--ignored", "--nocapture"])
            .env_clear()
            .env("DETACHED_MEMBER", root)
            .spawn()
            .unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while !root.join("member").exists() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        return;
    }
    if std::env::var("DETACHED_MODE").unwrap() == "exit" {
        return;
    }
    while !root.join("stop").exists() {
        fs::write(
            root.join("heartbeat"),
            format!("{:?}", std::time::SystemTime::now()),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(10));
    }
    fs::write(root.join("finished"), "joined native fixture").unwrap();
}

#[tokio::test]
async fn native_session_null_stdio_and_cancel_join_are_actual() {
    let root = tempfile::Builder::new()
        .prefix("detached Ω ")
        .tempdir()
        .unwrap();
    let context = original(Duration::from_secs(4));
    let mut child = spawn_detached(request(root.path(), "wait"), &context)
        .await
        .unwrap();
    let actual = capture(root.path()).await;
    for key in ["pid", "pgid", "sid"] {
        assert!(
            actual.contains(&format!("{key}={}\n", child.pid())),
            "{actual}"
        );
    }
    assert!(actual.contains(&format!("cwd={}\n", root.path().display())));
    assert!(actual.contains("literal=a % quoted \" Ω\nstdin=0\n"));
    for fd in 0..3 {
        assert!(actual.contains(&format!("fd{fd}=/dev/null\n")));
    }
    assert!(child.try_wait().unwrap().is_none());
    context.cancellation().cancel();
    let status = child
        .cancel_join(Instant::now() + Duration::from_secs(3))
        .await
        .unwrap();
    assert!(!status.success());
}

#[tokio::test]
async fn cancelled_and_expired_startup_cannot_transfer_actual_child() {
    for expired in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let context = original(if expired {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(3)
        });
        let child = spawn_detached(request(root.path(), "wait"), &context)
            .await
            .unwrap();
        capture(root.path()).await;
        if expired {
            tokio::time::sleep(Duration::from_millis(1010)).await;
        } else {
            context.cancellation().cancel();
        }
        let mut failure = child.commit().unwrap_err();
        assert_eq!(
            failure.cause.kind(),
            if expired {
                std::io::ErrorKind::TimedOut
            } else {
                std::io::ErrorKind::Interrupted
            }
        );
        let remaining = failure
            .remaining
            .as_mut()
            .expect("same child cleanup owner");
        let pid = remaining.pid();
        let status = remaining
            .repair(Instant::now() + Duration::from_secs(3))
            .await
            .unwrap()
            .unwrap();
        assert!(!status.success());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[tokio::test]
async fn actual_exit_and_invalid_startup_are_not_detached_success() {
    let root = tempfile::tempdir().unwrap();
    let service = Arc::new(SnapshotContext::service(Cancellation::default()));
    assert_eq!(
        spawn_detached(request(root.path(), "wait"), &service)
            .await
            .unwrap_err()
            .cause
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    let mut relative = request(root.path(), "wait");
    relative.program = "relative-program".into();
    assert_eq!(
        spawn_detached(relative, &original(Duration::from_secs(3)))
            .await
            .unwrap_err()
            .cause
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    for cleanup in [true, false] {
        let mut invalid = request(root.path(), "wait");
        if cleanup {
            invalid.cleanup = Duration::from_secs(u64::MAX);
        } else {
            invalid.termination_grace = Duration::from_secs(u64::MAX);
        }
        let failure = spawn_detached(invalid, &original(Duration::from_secs(3)))
            .await
            .unwrap_err();
        assert_eq!(failure.cause.kind(), std::io::ErrorKind::InvalidInput);
        assert!(failure.remaining.is_none());
    }
    assert!(!root.path().join("capture").exists());
    let mut child = spawn_detached(
        request(root.path(), "exit"),
        &original(Duration::from_secs(3)),
    )
    .await
    .unwrap();
    capture(root.path()).await;
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut failure = child.commit().unwrap_err();
    assert_eq!(failure.cause.kind(), std::io::ErrorKind::BrokenPipe);
    let status = failure.status.unwrap();
    assert!(status.success());
    let observed = failure
        .remaining
        .as_mut()
        .unwrap()
        .repair(Instant::now() + Duration::from_secs(3))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observed, status);
    let mut input = request(root.path(), "wait");
    input.program = "/usr/bin/false".into();
    input.args.clear();
    let mut exited = spawn_detached(input, &original(Duration::from_secs(3)))
        .await
        .unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = exited.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert_eq!(status.code(), Some(1));
    assert_eq!(
        exited
            .cancel_join(Instant::now() + Duration::from_secs(3))
            .await
            .unwrap(),
        status
    );
}

#[tokio::test]
async fn dropped_startup_kills_and_reaps_without_a_join_claim() {
    let root = tempfile::tempdir().unwrap();
    let child = spawn_detached(
        request(root.path(), "wait"),
        &original(Duration::from_secs(3)),
    )
    .await
    .unwrap();
    capture(root.path()).await;
    let pid = child.pid();
    drop(child);
    let end = Instant::now() + Duration::from_secs(3);
    while Path::new(&format!("/proc/{pid}")).exists() {
        assert!(Instant::now() < end, "owned startup child remains");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn cancelled_cleanup_observer_retains_actual_kill_wait_and_remaining() {
    use std::os::unix::process::ExitStatusExt;
    let root = tempfile::tempdir().unwrap();
    let context = original(Duration::from_secs(3));
    let mut input = request(root.path(), "ignore");
    input.termination_grace = Duration::from_millis(400);
    let child = spawn_detached(input, &context).await.unwrap();
    capture(root.path()).await;
    let pid = child.pid();
    context.cancellation().cancel();
    let mut failure = child.cancel_join(Instant::now()).await.unwrap_err();
    let remaining = failure.remaining.as_mut().expect("same unfinished cleanup");
    {
        let observation = remaining.repair(Instant::now() + Duration::from_secs(3));
        tokio::pin!(observation);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(10)) => (),
            result = &mut observation => panic!("SIGTERM ignored; observer unexpectedly ended: {result:?}"),
        }
    }
    let status = remaining
        .repair(Instant::now() + Duration::from_secs(3))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.signal(), Some(9));
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
}

#[test]
#[ignore]
fn native_parent() {
    let root = std::env::var_os("DETACHED_PARENT").expect("private parent fixture");
    let root = Path::new(&root);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let context = original(Duration::from_secs(3));
        let child = spawn_detached(request(root, "wait"), &context)
            .await
            .unwrap();
        capture(root).await;
        let pid = child.commit().unwrap().pid;
        context.cancellation().cancel();
        fs::write(root.join("committed"), pid.to_string()).unwrap();
    });
}

#[tokio::test]
async fn explicit_transfer_survives_real_launcher_exit_and_context_cancel() {
    let root = tempfile::tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "native_parent", "--ignored", "--nocapture"])
        .env("DETACHED_PARENT", root.path())
        .status()
        .unwrap();
    assert!(status.success());
    let pid: u32 = fs::read_to_string(root.path().join("committed"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(Path::new(&format!("/proc/{pid}")).exists());
    let prior = fs::read(root.path().join("heartbeat")).unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_ne!(prior, fs::read(root.path().join("heartbeat")).unwrap());
    fs::write(root.path().join("stop"), "native fixture stop").unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while !root.path().join("finished").exists() {
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[test]
#[ignore]
fn native_member() {
    let root = std::env::var_os("DETACHED_MEMBER").expect("private native group member");
    let root = Path::new(&root);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let _term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    let stat = fs::read_to_string("/proc/self/stat").unwrap();
    let tail = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    fs::write(
        root.join("member"),
        format!("{} {} {}", std::process::id(), tail[2], tail[3]),
    )
    .unwrap();
    while !root.join("member_stop").exists() {
        fs::write(
            root.join("member_heartbeat"),
            format!("{:?}", std::time::SystemTime::now()),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::write(root.join("member_finished"), "native member stopped").unwrap();
}

#[tokio::test]
async fn exited_leader_does_not_finish_group_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let mut leader = spawn_detached(
        request(root.path(), "group"),
        &original(Duration::from_secs(3)),
    )
    .await
    .unwrap();
    capture(root.path()).await;
    let end = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = leader.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert!(status.success());
    let member = fs::read_to_string(root.path().join("member")).unwrap();
    let fields = member.split_whitespace().collect::<Vec<_>>();
    let pid: u32 = fields[0].parse().unwrap();
    assert_eq!(fields[1], leader.pid().to_string());
    assert_eq!(fields[2], leader.pid().to_string());
    assert_eq!(
        leader
            .cancel_join(Instant::now() + Duration::from_secs(3))
            .await
            .unwrap(),
        status
    );
    let end = Instant::now() + Duration::from_millis(250);
    let terminated = loop {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"));
        if stat
            .as_ref()
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            || stat
                .as_ref()
                .is_ok_and(|s| s.rsplit_once(") ").unwrap().1.starts_with("Z "))
        {
            break true;
        }
        if Instant::now() >= end {
            break false;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    if !terminated {
        // Stop only this private fixture after reproducing a cleanup failure.
        fs::write(root.path().join("member_stop"), "fixture recovery").unwrap();
        let end = Instant::now() + Duration::from_secs(1);
        while !root.path().join("member_finished").exists() {
            assert!(Instant::now() < end);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    assert!(
        terminated,
        "native member still running after leader-only cleanup receipt"
    );
}
