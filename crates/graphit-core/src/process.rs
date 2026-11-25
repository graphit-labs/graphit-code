//! Bounded child execution for finite Git builtins, not arbitrary process trees.

use crate::file_lock::Cancellation;
use std::io::{self, Read};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(crate) const OUTPUT_LIMIT: usize = 64 * 1024;

pub(crate) struct Budget<'a> {
    deadline: Instant,
    cancellation: &'a Cancellation,
}

impl<'a> Budget<'a> {
    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub(crate) fn new(timeout: Duration, cancellation: &'a Cancellation) -> io::Result<Self> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "timeout is too large"))?;
        let budget = Self {
            deadline,
            cancellation,
        };
        budget.check()?;
        Ok(budget)
    }

    pub(crate) fn check(&self) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Git inspection cancelled",
            ))
        } else if Instant::now() >= self.deadline {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Git inspection expired",
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn run(&self, command: &mut Command) -> io::Result<Output> {
        self.check()?;
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let overflow = AtomicBool::new(false);
        std::thread::scope(|scope| {
            // Keep the guard inside the scope: every early return kills/reaps before
            // the scope joins its pipe readers, including thread creation failures.
            let mut child = Reap(child);
            let overflow = &overflow;
            let stdout = child.0.stdout.take().expect("piped stdout");
            let stderr = child.0.stderr.take().expect("piped stderr");
            let out = std::thread::Builder::new()
                .spawn_scoped(scope, move || read_limited(stdout, overflow))?;
            let err = std::thread::Builder::new()
                .spawn_scoped(scope, move || read_limited(stderr, overflow))?;
            let status = loop {
                self.check()?;
                if overflow.load(Ordering::Acquire) {
                    return Err(output_limit());
                }
                if let Some(status) = child.0.try_wait()? {
                    break status;
                }
                std::thread::sleep(
                    self.deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(1)),
                );
            };
            let stdout = out
                .join()
                .map_err(|_| io::Error::other("stdout reader failed"))??;
            let stderr = err
                .join()
                .map_err(|_| io::Error::other("stderr reader failed"))??;
            self.check()?;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        })
    }
}

struct Reap(Child);

impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn read_limited(mut pipe: impl Read, overflow: &AtomicBool) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = pipe.read(&mut buffer)?;
        if count == 0 {
            return Ok(result);
        }
        if result.len() + count > OUTPUT_LIMIT {
            overflow.store(true, Ordering::Release);
            return Err(output_limit());
        }
        result.extend_from_slice(&buffer[..count]);
    }
}

fn output_limit() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Git output exceeds 64 KiB per stream",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn helper(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "process::tests::child", "--nocapture"])
            .env("GRAPHIT_TEST_CHILD", mode);
        command
    }

    #[test]
    fn child() {
        match std::env::var("GRAPHIT_TEST_CHILD").as_deref() {
            Ok("wait") => std::thread::sleep(Duration::from_secs(30)),
            Ok("flood") => {
                let bytes = [b'x'; 4096];
                loop {
                    let _ = io::stdout().write_all(&bytes);
                }
            }
            Ok("flood-stderr") => {
                let bytes = [b'x'; 4096];
                loop {
                    let _ = io::stderr().write_all(&bytes);
                }
            }
            Ok("both") => {
                for _ in 0..10 {
                    io::stdout().write_all(&[b'o'; 4096]).unwrap();
                    io::stderr().write_all(&[b'e'; 4096]).unwrap();
                }
            }
            _ => {}
        }
    }

    #[test]
    fn drains_both_streams_without_deadlock() {
        let cancel = Cancellation::default();
        let result = Budget::new(Duration::from_secs(5), &cancel)
            .unwrap()
            .run(&mut helper("both"))
            .unwrap();
        assert!(result.status.success());
        assert!(result.stdout.len() >= 40_960 && result.stdout.len() <= OUTPUT_LIMIT);
        assert_eq!(result.stderr.len(), 40_960);
    }

    #[test]
    fn enforces_shared_deadline_cancellation_and_limit() {
        let cancel = Cancellation::default();
        let start = Instant::now();
        let result = Budget::new(Duration::from_millis(80), &cancel)
            .unwrap()
            .run(&mut helper("wait"));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(2));
        let result = Budget::new(Duration::from_secs(5), &cancel)
            .unwrap()
            .run(&mut helper("flood-stderr"));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        let start = Instant::now();
        let result = Budget::new(Duration::from_secs(5), &cancel)
            .unwrap()
            .run(&mut helper("flood"));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(start.elapsed() < Duration::from_secs(2));
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(80));
                cancel.cancel();
            });
            let start = Instant::now();
            let result = Budget::new(Duration::from_secs(30), &cancel)
                .unwrap()
                .run(&mut helper("wait"));
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
            assert!(start.elapsed() < Duration::from_secs(2));
        });
    }

    #[test]
    fn rejects_invalid_budget_and_missing_executable() {
        let cancel = Cancellation::default();
        assert!(
            matches!(Budget::new(Duration::ZERO, &cancel), Err(e) if e.kind() == io::ErrorKind::TimedOut)
        );
        assert!(
            matches!(Budget::new(Duration::MAX, &cancel), Err(e) if e.kind() == io::ErrorKind::InvalidInput)
        );
        let result = Budget::new(Duration::from_secs(5), &cancel)
            .unwrap()
            .run(&mut Command::new("graphit-test-executable-does-not-exist"));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
        let budget = Budget::new(Duration::from_millis(30), &cancel).unwrap();
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(
            budget.run(&mut helper("both")).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        cancel.cancel();
        assert!(
            matches!(Budget::new(Duration::from_secs(1), &cancel), Err(e) if e.kind() == io::ErrorKind::Interrupted)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn timed_out_child_is_reaped() {
        let cancel = Cancellation::default();
        let before = child_pids();
        let _ = Budget::new(Duration::from_millis(80), &cancel)
            .unwrap()
            .run(&mut helper("wait"));
        // Only this thread's children are observed; parallel tests cannot distort it.
        assert_eq!(child_pids(), before);
    }

    #[cfg(target_os = "linux")]
    fn child_pids() -> String {
        std::fs::read_to_string("/proc/thread-self/children").unwrap()
    }
}
