//! Explicit best-effort priority reduction before starting background workers.

use std::io;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriorityStage {
    CpuRead,
    CpuWrite,
    IoWrite,
}

/// An operation and error category, without native diagnostic payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PriorityFailure {
    pub stage: PriorityStage,
    pub kind: io::ErrorKind,
}

impl std::fmt::Display for PriorityFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "background priority failed at {:?}: {:?}",
            self.stage, self.kind
        )
    }
}

impl std::error::Error for PriorityFailure {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriorityOutcome {
    /// The kernel accepted the request; this does not guarantee scheduling effect.
    Applied,
    /// The observed CPU priority was already at or below the requested priority.
    AlreadyLowered,
    Unsupported,
    Unavailable(PriorityFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PriorityReport {
    cpu: PriorityOutcome,
    io: PriorityOutcome,
}

impl PriorityReport {
    pub const fn cpu(&self) -> PriorityOutcome {
        self.cpu
    }

    pub const fn io(&self) -> PriorityOutcome {
        self.io
    }
}

/// Call on the startup thread before creating background worker runtimes.
///
/// Linux changes the calling thread's nice value and I/O class. macOS changes
/// process nice; Windows changes the process CPU class. Existing Linux threads
/// are not scanned or targeted; a shared Linux I/O context can propagate the
/// I/O setting to its sharers. The caller chooses diagnostics and continues
/// after failures. Effects persist; dropping the report does not restore priority.
#[must_use]
pub fn lower_background_priority() -> PriorityReport {
    apply(lower_cpu, lower_io)
}

fn apply(
    cpu: impl FnOnce() -> PriorityOutcome,
    io: impl FnOnce() -> PriorityOutcome,
) -> PriorityReport {
    PriorityReport {
        cpu: cpu(),
        io: io(),
    }
}

#[cfg(any(test, target_os = "linux", target_os = "macos", windows))]
fn failed(stage: PriorityStage, kind: io::ErrorKind) -> PriorityOutcome {
    PriorityOutcome::Unavailable(PriorityFailure { stage, kind })
}

#[cfg(any(test, target_os = "linux", target_os = "macos"))]
fn lower_nice(
    read: impl FnOnce() -> Result<i32, io::ErrorKind>,
    write: impl FnOnce(i32) -> Result<(), io::ErrorKind>,
) -> PriorityOutcome {
    match read() {
        Err(kind) => failed(PriorityStage::CpuRead, kind),
        Ok(current) if current >= 10 => PriorityOutcome::AlreadyLowered,
        Ok(_) => match write(10) {
            Ok(()) => PriorityOutcome::Applied,
            Err(kind) => failed(PriorityStage::CpuWrite, kind),
        },
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn lower_cpu() -> PriorityOutcome {
    lower_nice(
        || rustix::process::getpriority_process(None).map_err(|e| io::Error::from(e).kind()),
        |value| {
            rustix::process::setpriority_process(None, value).map_err(|e| io::Error::from(e).kind())
        },
    )
}

#[cfg(windows)]
fn lower_cpu() -> PriorityOutcome {
    use winsafe::{HPROCESS, co};
    let process = HPROCESS::GetCurrentProcess();
    match process.GetPriorityClass() {
        Err(error) => failed(
            PriorityStage::CpuRead,
            io::Error::from_raw_os_error(error.raw() as i32).kind(),
        ),
        Ok(class)
            if class == co::PRIORITY_CLASS::IDLE || class == co::PRIORITY_CLASS::BELOW_NORMAL =>
        {
            PriorityOutcome::AlreadyLowered
        }
        Ok(_) => match process.SetPriorityClass(co::PRIORITY_CLASS::BELOW_NORMAL) {
            Ok(()) => PriorityOutcome::Applied,
            Err(error) => failed(
                PriorityStage::CpuWrite,
                io::Error::from_raw_os_error(error.raw() as i32).kind(),
            ),
        },
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn lower_cpu() -> PriorityOutcome {
    PriorityOutcome::Unsupported
}

#[cfg(target_os = "linux")]
fn lower_io() -> PriorityOutcome {
    let target = ioprio::Target::Process(ioprio::Pid::from_raw(0));
    match ioprio::set_priority(target, ioprio::Priority::new(ioprio::Class::Idle)) {
        Ok(()) => PriorityOutcome::Applied,
        Err(error) => failed(
            PriorityStage::IoWrite,
            error.as_errno().map_or(io::ErrorKind::Other, |errno| {
                io::Error::from_raw_os_error(errno as i32).kind()
            }),
        ),
    }
}

#[cfg(not(target_os = "linux"))]
fn lower_io() -> PriorityOutcome {
    PriorityOutcome::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn nice_reduction_never_raises_an_already_lowered_priority() {
        for (current, expected_write, expected_outcome) in [
            (-1, Some(10), PriorityOutcome::Applied),
            (0, Some(10), PriorityOutcome::Applied),
            (9, Some(10), PriorityOutcome::Applied),
            (10, None, PriorityOutcome::AlreadyLowered),
            (19, None, PriorityOutcome::AlreadyLowered),
        ] {
            let written = Cell::new(None);
            let outcome = lower_nice(
                || Ok(current),
                |value| {
                    written.set(Some(value));
                    Ok(())
                },
            );
            assert_eq!(written.get(), expected_write);
            assert_eq!(outcome, expected_outcome);
        }
    }

    #[test]
    fn cpu_read_failure_does_not_write_and_write_failure_remains_distinct() {
        assert_eq!(
            lower_nice(
                || Err(io::ErrorKind::PermissionDenied),
                |_| panic!("must not write")
            ),
            failed(PriorityStage::CpuRead, io::ErrorKind::PermissionDenied)
        );
        assert_eq!(
            lower_nice(|| Ok(0), |_| Err(io::ErrorKind::PermissionDenied)),
            failed(PriorityStage::CpuWrite, io::ErrorKind::PermissionDenied)
        );
        let error = PriorityFailure {
            stage: PriorityStage::CpuWrite,
            kind: io::ErrorKind::PermissionDenied,
        };
        assert_eq!(
            error.to_string(),
            "background priority failed at CpuWrite: PermissionDenied"
        );
    }

    #[test]
    fn both_providers_run_once_and_each_outcome_is_retained() {
        let cpu_error = failed(PriorityStage::CpuRead, io::ErrorKind::PermissionDenied);
        let write_error = failed(PriorityStage::CpuWrite, io::ErrorKind::PermissionDenied);
        let io_error = failed(PriorityStage::IoWrite, io::ErrorKind::Unsupported);
        for cpu in [
            PriorityOutcome::Applied,
            PriorityOutcome::AlreadyLowered,
            PriorityOutcome::Unsupported,
            cpu_error,
            write_error,
        ] {
            for io in [
                PriorityOutcome::Applied,
                PriorityOutcome::Unsupported,
                io_error,
            ] {
                let cpu_calls = Cell::new(0);
                let io_calls = Cell::new(0);
                let report = apply(
                    || {
                        cpu_calls.set(cpu_calls.get() + 1);
                        cpu
                    },
                    || {
                        io_calls.set(io_calls.get() + 1);
                        io
                    },
                );
                assert_eq!((cpu_calls.get(), io_calls.get()), (1, 1));
                assert_eq!(report.cpu(), cpu);
                assert_eq!(report.io(), io);
            }
        }
    }
}
