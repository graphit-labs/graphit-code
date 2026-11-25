use graphit_core::file_lock::{Cancellation, FileLock};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

mod support;
use support::Fixture;

#[test]
fn independent_handles_exclude_and_owned_release_unlocks() {
    let fixture = Fixture::new();
    let path = fixture.path("same.lock");
    let first = FileLock::try_acquire(&path).unwrap();
    assert_eq!(first.path(), path);
    assert!(FileLock::is_locked(&path).unwrap());
    assert_eq!(
        FileLock::try_acquire(&path).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let independent = FileLock::try_acquire(fixture.path("other.lock")).unwrap();
    first.release().unwrap();
    let next = FileLock::try_acquire(&path).unwrap();
    drop(next);
    assert!(!FileLock::is_locked(&path).unwrap());
    assert!(path.exists(), "release must not unlink the lock inode");
    drop(independent);
}

#[test]
fn probes_and_acquisition_preserve_existing_contents() {
    let fixture = Fixture::new();
    let missing = fixture.path("missing/lock");
    assert!(!FileLock::is_locked(&missing).unwrap());
    assert!(!missing.parent().unwrap().exists());
    let path = fixture.path("diagnostic.lock");
    fs::write(&path, b"existing diagnostic contents").unwrap();
    assert!(!FileLock::is_locked(&path).unwrap());
    let held = FileLock::try_acquire(&path).unwrap();
    assert!(FileLock::is_locked(&path).unwrap());
    drop(held);
    assert_eq!(fs::read(&path).unwrap(), b"existing diagnostic contents");
}

#[test]
fn unwinding_releases_ownership() {
    let fixture = Fixture::new();
    let path = fixture.path("unwind.lock");
    let result = std::panic::catch_unwind(|| {
        let _held = FileLock::try_acquire(&path).unwrap();
        panic!("simulated failure after acquisition");
    });
    assert!(result.is_err());
    let recovered = FileLock::try_acquire(&path).unwrap();
    drop(recovered);
}

#[test]
fn nested_creation_and_io_failure_are_distinct_from_contention() {
    let fixture = Fixture::new();
    let path = fixture.path("nested/child/lock");
    let held = FileLock::try_acquire(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let blocker = fixture.path("ordinary-file");
    fs::write(&blocker, b"not a directory").unwrap();
    let error = FileLock::try_acquire(blocker.join("lock")).unwrap_err();
    assert_ne!(error.kind(), io::ErrorKind::WouldBlock);
    assert_ne!(error.kind(), io::ErrorKind::TimedOut);
    let error = FileLock::is_locked(fixture.root()).unwrap_err();
    assert_ne!(error.kind(), io::ErrorKind::WouldBlock);
    drop(held);
}

#[test]
fn deadlines_are_bounded_and_zero_attempts_once() {
    let fixture = Fixture::new();
    let path = fixture.path("deadline.lock");
    let token = Cancellation::default();
    let held = FileLock::acquire(&path, Duration::ZERO, &token).unwrap();
    for timeout in [Duration::ZERO, Duration::from_millis(80)] {
        let start = Instant::now();
        let error = FileLock::acquire(&path, timeout, &token).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() >= timeout);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    assert_eq!(
        FileLock::acquire(&path, Duration::MAX, &token)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    drop(held);
}

#[test]
fn cancellation_before_acquisition_creates_nothing() {
    let fixture = Fixture::new();
    let path = fixture.path("absent/lock");
    let token = Cancellation::default();
    token.cancel();
    assert!(token.clone().is_cancelled());
    assert_eq!(
        FileLock::acquire(&path, Duration::from_secs(30), &token)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!path.parent().unwrap().exists());
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn process_death_releases_the_kernel_lock_without_cleanup() {
    let fixture = Fixture::new();
    let path = fixture.path("process.lock");
    let ready = fixture.path("ready");
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_child", "--nocapture"])
        .env("GRAPHIT_RUST_LOCK_CHILD_PATH", &path)
        .env("GRAPHIT_RUST_LOCK_CHILD_READY", &ready)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let start = Instant::now();
    while !ready.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child exited before holding lock"
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "child did not become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(FileLock::is_locked(&path).unwrap());
    assert_eq!(
        FileLock::try_acquire(&path).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let recovered = FileLock::try_acquire(&path).unwrap();
    assert!(path.exists());
    drop(recovered);
}

#[test]
fn lock_child() {
    let Some(path) = std::env::var_os("GRAPHIT_RUST_LOCK_CHILD_PATH") else {
        return;
    };
    let _held = FileLock::try_acquire(Path::new(&path)).unwrap();
    fs::write(
        std::env::var_os("GRAPHIT_RUST_LOCK_CHILD_READY").unwrap(),
        b"ready",
    )
    .unwrap();
    io::stdin().read_to_end(&mut Vec::new()).unwrap();
    std::process::exit(0);
}
