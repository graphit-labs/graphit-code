#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]
//! Subprocess supervision through an installed host and its shared driver runtime.
//! Operations borrow the caller's original Core context. Services use that context for
//! startup, then follow host and local cancellation without retaining its deadline.
//! Binary callbacks are serial and borrowed; see [`spawn_service`] and [`run_service`].

use graphit_core::file_io::SnapshotContext;
use secrecy::zeroize::Zeroizing;
use std::{
    ffi::OsString, fmt, future::Future, io, path::PathBuf, process::ExitStatus, time::Duration,
};

#[cfg(test)]
extern crate self as graphit_process;
#[cfg(test)]
#[path = "../tests/support/host_fixture.rs"]
mod host_fixture;

#[cfg(windows)]
mod windows_job;

#[cfg(target_os = "linux")]
mod detached;
#[cfg(target_os = "linux")]
pub use detached::{
    DetachedCommit, DetachedFailure, DetachedRemaining, DetachedRequest, DetachedStartup,
    spawn_detached,
};
mod host;
mod operation;
mod service;
mod supervisor;
pub use host::{
    HostFailure, HostLimits, HostPhase, HostReadiness, HostReport, HostShutdown,
    HostShutdownObservation, PendingHost, ProcessHost,
};
pub use operation::{OperationAdmissionFailure, OperationReceiver, OperationStop, operation};
pub use service::{
    InheritedServiceProcess, InheritedServiceRequest, ServiceFailure, ServiceOutcome,
    ServiceProcess, ServiceRemaining, ServiceSink, ServiceStop, join_inherited_service,
    run_service, spawn_inherited_service, spawn_service,
};
pub use supervisor::run;
pub use supervisor::{RemainingProcess, RepairObservation, SpawnFailure, SpawnPhase, spawn_owned};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub stdin_bytes: usize,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub termination_grace: Duration,
    pub cleanup: Duration,
}

/// Complete explicit child environment; inputs never appear in Debug or errors.
pub struct Request {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
    pub stdin: Option<Zeroizing<Vec<u8>>>,
    pub limits: Limits,
}
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retention {
    None,
    Full,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// A transient borrow of one scratch buffer, never an owned event queue.
/// Fields cannot mint a runner observation:
/// ```compile_fail
/// use graphit_process::{Chunk, Stream};
/// let chunk = Chunk { stream: Stream::Stdout, bytes: b"forged" };
/// ```
/// The byte lifetime cannot become a detached static payload:
/// ```compile_fail
/// use graphit_process::Chunk;
/// fn detach(chunk: Chunk<'_>) -> &'static [u8] { chunk.bytes() }
/// ```
pub struct Chunk<'a> {
    stream: Stream,
    bytes: &'a [u8],
}
impl<'a> Chunk<'a> {
    pub fn stream(&self) -> Stream {
        self.stream
    }
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}
impl fmt::Debug for Chunk<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chunk")
            .field("stream", &self.stream)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinkFailure {
    Closed,
    ConsumerFailed,
}

/// Each future must cooperatively poll, be cancellation-safe on drop and leave
/// no detached work. Arbitrary blocking or detached consumers need an owned,
/// joinable adapter. The runner never spawns callback tasks.
pub trait IncrementalSink {
    fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        original: &'a SnapshotContext,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}
/// Finite retention and streaming use the same supervisor even when ignored.
pub struct IgnoreChunks;
impl IncrementalSink for IgnoreChunks {
    async fn push<'a>(
        &'a mut self,
        _: Chunk<'a>,
        _: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        Ok(())
    }
}

#[derive(Default)]
pub struct Transcript {
    storage: Box<TranscriptStorage>,
}
#[derive(Default)]
struct TranscriptStorage {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
impl Transcript {
    pub fn stdout(&self) -> &[u8] {
        &self.storage.stdout
    }
    pub fn stderr(&self) -> &[u8] {
        &self.storage.stderr
    }
}
impl fmt::Debug for Transcript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transcript")
            .field("stdout_bytes", &self.storage.stdout.len())
            .field("stderr_bytes", &self.storage.stderr.len())
            .finish()
    }
}
/// Observed includes bounded cleanup discard; delivered counts only push Ok.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub stdout: u64,
    pub stderr: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeEnd {
    NotSpawned,
    Unobserved,
    Eof,
    Closed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeFinalization {
    pub stdout: PipeEnd,
    pub stderr: PipeEnd,
}
#[derive(Debug)]
pub struct Outcome {
    pub status: ExitStatus,
    pub cleanup: Cleanup,
    pub pipes: PipeFinalization,
    pub observed: Counts,
    pub delivered: Counts,
    pub transcript: Option<Transcript>,
    pub remaining: Option<RemainingProcess>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Spawn,
    Input,
    Stdout,
    Stderr,
    Wait,
    Terminate,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidRequest {
    Program,
    WindowsExecutable,
    Cwd,
    Budget,
    StdinLimit,
    InvalidRequestContext,
}
/// Only the SDK error kind and exact optional native code are retained.
/// Read the observations through the getters; their storage is private:
/// ```compile_fail
/// use graphit_process::IoFailure;
/// fn inspect(error: &IoFailure) -> std::io::ErrorKind { error.kind }
/// ```
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct IoFailure {
    kind: io::ErrorKind,
    os_code: Option<std::num::NonZeroI32>,
    zero_code: bool,
}
impl IoFailure {
    pub fn kind(&self) -> io::ErrorKind {
        self.kind
    }
    pub fn os_code(&self) -> Option<i32> {
        if self.zero_code {
            Some(0)
        } else {
            self.os_code.map(std::num::NonZeroI32::get)
        }
    }
}
impl fmt::Debug for IoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IoFailure")
            .field("kind", &self.kind())
            .field("os_code", &self.os_code())
            .finish()
    }
}
impl From<io::Error> for IoFailure {
    fn from(error: io::Error) -> Self {
        let os_code = error.raw_os_error();
        Self {
            kind: error.kind(),
            os_code: os_code.and_then(std::num::NonZeroI32::new),
            zero_code: os_code == Some(0),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    Invalid(InvalidRequest),
    Cancelled,
    Stopped,
    TimedOut,
    OutputLimit(Stream),
    Io(Stage, IoFailure),
    Panic(Stage),
    Sink(SinkFailure),
    Cleanup,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cleanup {
    Complete,
    NotSpawned,
    TimedOut,
    Failed(IoFailure),
    ControlFailed {
        leader: LeaderCleanup,
        control: ControlFailures,
    },
    Unconfirmed(UnconfirmedCleanup),
}
impl Cleanup {
    /// The process leader was reaped without a reported control failure.
    /// On Windows this can be true while cleanup of descendants remains unconfirmed.
    pub fn leader_reaped_without_control_failure(self) -> bool {
        self == Self::Complete
            || cfg!(windows)
                && matches!(
                    self,
                    Self::Unconfirmed(UnconfirmedCleanup::TreeUnknownWindows {
                        leader: LeaderCleanup::Reaped,
                        control: ControlFailures {
                            group: None,
                            terminate: None,
                            kill: None,
                        },
                    })
                )
    }
}
/// The result of an actual cleanup wait; status is reported separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaderCleanup {
    Reaped,
    TimedOut,
    Failed(IoFailure),
}
/// Bounded observations of failed control operations, never permissions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControlFailures {
    pub group: Option<GroupFailure>,
    pub terminate: Option<IoFailure>,
    pub kill: Option<IoFailure>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupFailure {
    Io(IoFailure),
    Panic,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnconfirmedCleanup {
    BirthUnobserved,
    DriverPanic,
    HostPumpPanic,
    TreeUnknownWindows {
        leader: LeaderCleanup,
        control: ControlFailures,
    },
    GroupUnknownUnix {
        leader: LeaderCleanup,
        control: ControlFailures,
    },
}
#[derive(Debug)]
pub struct Failure {
    pub reason: Reason,
    pub cleanup: Cleanup,
    pub status: Option<ExitStatus>,
    pub pipes: PipeFinalization,
    pub observed: Counts,
    pub delivered: Counts,
    pub transcript: Option<Transcript>,
    pub remaining: Option<RemainingProcess>,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "subprocess {:?}; cleanup {:?}",
            self.reason, self.cleanup
        )
    }
}
impl std::error::Error for Failure {}

#[cfg(test)]
mod io_failure_tests {
    use super::*;

    #[test]
    fn actual_std_errors_preserve_optional_native_codes_and_kind() {
        for code in [i32::MIN, -1, 0, 1, i32::MAX] {
            let error = io::Error::from_raw_os_error(code);
            let kind = error.kind();
            let observed = IoFailure::from(error);
            assert_eq!(observed.kind(), kind);
            assert_eq!(observed.os_code(), Some(code));
        }
        let observed = IoFailure::from(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(observed.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(observed.os_code(), None);
    }

    #[test]
    fn native_error_text_is_absent_from_observation_debug() {
        let observed = IoFailure::from(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "private-error-canary",
        ));
        assert_eq!(observed.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(observed.os_code(), None);
        assert_eq!(
            format!("{observed:?}"),
            "IoFailure { kind: PermissionDenied, os_code: None }"
        );
    }
}
