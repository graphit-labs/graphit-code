#![cfg(target_os = "linux")]

use graphit_core::startup_priority::{PriorityOutcome, lower_background_priority};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn kernel_state() -> (i32, ioprio::Priority) {
    (
        rustix::process::getpriority_process(None).unwrap(),
        ioprio::get_priority(ioprio::Target::Process(ioprio::Pid::from_raw(0))).unwrap(),
    )
}

struct Reap(Child);

impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn child(case: &str) {
    let before = kernel_state();
    let mut process = Reap(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "priority_effect_child", "--test-threads=1"])
            .env("GRAPHIT_TEST_PRIORITY_CASE", case)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "priority child deadline exceeded"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success(), "priority child failed: {status}");
    assert_eq!(kernel_state(), before, "child must not change the parent");
}

#[test]
fn native_reduction_is_isolated_and_new_worker_inherits() {
    child("normal");
}

#[test]
fn native_already_lowered_cpu_priority_is_preserved() {
    child("already-lowered");
}

#[test]
fn priority_effect_child() {
    let Some(case) = std::env::var_os("GRAPHIT_TEST_PRIORITY_CASE") else {
        return;
    };
    let was_lowered = case == "already-lowered";
    assert!(was_lowered || case == "normal");
    if was_lowered {
        rustix::process::setpriority_process(None, 19).unwrap();
    }
    let original = kernel_state().0;
    let report = lower_background_priority();
    assert_eq!(
        report.cpu(),
        if original >= 10 {
            PriorityOutcome::AlreadyLowered
        } else {
            PriorityOutcome::Applied
        },
        "native CPU effect must be established, not assumed after denial"
    );
    assert_eq!(report.io(), PriorityOutcome::Applied);
    let (nice, io) = kernel_state();
    assert_eq!(nice, original.max(10));
    assert_eq!(io.class(), Some(ioprio::Class::Idle));
    let worker = std::thread::spawn(kernel_state).join().unwrap();
    assert_eq!(worker, (nice, io), "new worker must inherit the priority");
    let again = lower_background_priority();
    assert_eq!(again.cpu(), PriorityOutcome::AlreadyLowered);
    assert_eq!(again.io(), PriorityOutcome::Applied);
    assert_eq!(kernel_state(), (nice, io));
}
