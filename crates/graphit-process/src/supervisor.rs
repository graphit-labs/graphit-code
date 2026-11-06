use super::*;
use crate::operation::ControlLease;
#[cfg(unix)]
use process_wrap::tokio::CommandWrapper;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use std::task::{Context, Poll};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    sync::{oneshot, watch},
    time::{Instant, sleep_until, timeout_at},
};

/// One engine for retained capture and true incremental output. Original context
/// is borrowed across the entire future, including callback and final fences.
/// The installed host retains the same owned driver when the borrowed caller is dropped.
/// The original context and sink cannot escape their borrowed lifetime:
/// ```compile_fail
/// use graphit_core::file_io::SnapshotContext;
/// use graphit_process::{Failure, IgnoreChunks, Outcome, OperationReceiver, Request, Retention, run};
/// fn detach<'a>(request: Request, context: &'a SnapshotContext, sink: &'a mut IgnoreChunks, operation: OperationReceiver<'a>)
///     -> impl std::future::Future<Output = Result<Outcome, Failure>> + 'static {
///     run(request, context, Retention::None, sink, operation)
/// }
/// ```
pub async fn run<S: IncrementalSink>(
    request: Request,
    original: &SnapshotContext,
    retention: Retention,
    sink: &mut S,
    operation: OperationReceiver<'_>,
) -> Result<Outcome, Failure> {
    if !operation.original.same_request(original) {
        return Err(not_spawned(
            Reason::Invalid(InvalidRequest::InvalidRequestContext),
            retention,
        ));
    }
    check(original).map_err(|reason| not_spawned(reason, retention))?;
    if operation.lease.is_closed() {
        return Err(not_spawned(Reason::Stopped, retention));
    }
    validate(&request, operation.lease.deadline)
        .map_err(|error| not_spawned(Reason::Invalid(error), retention))?;
    run_command(request, original, retention, sink, None, operation).await
}

fn stopped_reason(lease: &ControlLease) -> Reason {
    if matches!(
        lease.slot.observe().supervision.cleanup,
        Cleanup::Unconfirmed(UnconfirmedCleanup::DriverPanic | UnconfirmedCleanup::HostPumpPanic)
    ) {
        Reason::Panic(Stage::Wait)
    } else {
        Reason::Stopped
    }
}

fn stopped_startup(started: &mut oneshot::Receiver<Started>, lease: &ControlLease) -> Started {
    // An actual rejected startup is published before its internal close. Both
    // can be ready when the caller resumes; retain that earlier typed cause.
    // A queued success has no primary error and its pipes close as before.
    match started.try_recv() {
        Ok(initial) if initial.reason.is_some() => initial,
        Ok(_) | Err(_) => Started::rejected(stopped_reason(lease), PipeEnd::Closed),
    }
}

async fn started_closed(lease: &ControlLease, original: &SnapshotContext) -> Started {
    // Channel closure is not a primary cause. An unwinding F can drop its
    // startup sender before the host catch publishes the real panic snapshot.
    // Observe that SAME record or a real local/original limit instead.
    tokio::select! {
        biased;
        _ = lease.closed() => Started::rejected(stopped_reason(lease), PipeEnd::Closed),
        _ = original.cancellation().cancelled() => Started::rejected(Reason::Cancelled, PipeEnd::Closed),
        _ = sleep_until(lease.deadline.into()) => Started::rejected(Reason::TimedOut, PipeEnd::Closed),
        snapshot = lease.slot.completed() => Started::rejected(
            cleanup_failure(snapshot.supervision.cleanup)
                .unwrap_or(Reason::Stopped), PipeEnd::Closed),
    }
}

pub(crate) enum OutputMode {
    Piped,
    Inherited(std::process::Stdio),
}

pub(crate) fn command_for(request: &Request) -> CommandWrap {
    command_for_output(request, OutputMode::Piped, None)
}

pub(crate) fn command_for_output(
    request: &Request,
    output: OutputMode,
    stdin: Option<std::process::Stdio>,
) -> CommandWrap {
    let stderr = match &output {
        OutputMode::Piped => std::process::Stdio::piped(),
        OutputMode::Inherited(_) => std::process::Stdio::inherit(),
    };
    let mut command = CommandWrap::with_new(&request.program, |command| {
        command
            .args(&request.args)
            .current_dir(&request.cwd)
            .env_clear()
            .envs(request.environment.iter().cloned())
            .stdin(stdin.unwrap_or_else(|| {
                if request.stdin.is_some() {
                    std::process::Stdio::piped()
                } else {
                    std::process::Stdio::null()
                }
            }))
            .stdout(match output {
                OutputMode::Piped => std::process::Stdio::piped(),
                OutputMode::Inherited(stdout) => stdout,
            })
            .stderr(stderr);
    });
    command.wrap(KillOnDrop);
    #[cfg(unix)]
    command.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    {
        command.wrap(crate::windows_job::JobObservation::default());
        command.wrap(process_wrap::tokio::JobObject);
    }
    command
}

async fn run_command<S: IncrementalSink>(
    request: Request,
    original: &SnapshotContext,
    retention: Retention,
    sink: &mut S,
    command: Option<CommandWrap>,
    mut operation: OperationReceiver<'_>,
) -> Result<Outcome, Failure> {
    check(original).map_err(|reason| not_spawned(reason, retention))?;
    if !operation.original.same_request(original) {
        return Err(not_spawned(
            Reason::Invalid(InvalidRequest::InvalidRequestContext),
            retention,
        ));
    }
    if operation.lease.is_closed() {
        return Err(not_spawned(Reason::Stopped, retention));
    }
    let limits = request.limits;
    let lease = Arc::clone(&operation.lease);
    let receiver = operation
        .receiver
        .take()
        .expect("one caller consumes the admitted receiver");
    let sender = operation
        .sender
        .take()
        .expect("one caller submits its unpolled record");
    let (startup, mut started) = oneshot::channel();
    let (observations, mut observer) = watch::channel(Observation::default());
    let resources = Arc::new(tokio::sync::Mutex::new(DriverResources {
        owner: None,
        command,
        #[cfg(unix)]
        signal: None,
    }));
    let future = drive(
        request,
        Arc::clone(&resources),
        Arc::clone(&lease),
        receiver,
        startup,
        observations,
    );
    let record = Box::new(HostRecord::Operation(OperationRecord {
        driver: Box::pin(future),
        _resources: resources,
        lease: Arc::clone(&lease),
        observer: observer.clone(),
    }));
    if let Err(error) = sender.try_send(record) {
        // The exact returned record was never polled and has no native child.
        drop(error.into_inner());
        return Err(not_spawned(Reason::Stopped, retention));
    }
    drop(sender);
    let initial = tokio::select! {
        biased;
        _ = lease.closed() => stopped_startup(&mut started, &lease),
        _ = original.cancellation().cancelled() => Started::rejected(Reason::Cancelled, PipeEnd::Closed),
        _ = sleep_until(lease.deadline.into()) => Started::rejected(Reason::TimedOut, PipeEnd::Closed),
        result = &mut started => match result {
            Ok(started) => started,
            Err(_) => started_closed(&lease, original).await,
        },
    };
    let mut stdout = Reader::from_start(
        initial.stdout,
        limits.stdout_bytes,
        Stream::Stdout,
        initial.end,
    );
    let mut stderr = Reader::from_start(
        initial.stderr,
        limits.stderr_bytes,
        Stream::Stderr,
        initial.end,
    );
    let mut transcript = retained(retention);
    let mut delivered = Counts::default();
    let mut input = if initial.reason.is_none() {
        Some(Box::pin(write_input(initial.stdin, initial.input)))
    } else {
        None
    };
    let mut state = State::default();
    if let Some(reason) = initial.reason {
        state.reject(reason);
    }
    let snapshot = lease.slot.observe();
    if snapshot.finished() {
        state.control(Event::Supervised(snapshot.supervision), &mut input);
    }
    {
        loop {
            state.observe(*observer.borrow_and_update());
            if state.reason.is_none()
                && let Err(reason) = check(original)
            {
                state.reject(reason);
            }
            if state.reason.is_none() && lease.is_closed() {
                state.reject(stopped_reason(&lease));
            }
            if state.reason.is_some() {
                input.take(); // Close finite stdin before awaited tree cleanup.
                lease.close();
                stdout.close_if_exhausted();
                stderr.close_if_exhausted();
            }
            if state.cleanup_expired {
                stdout.close();
                stderr.close();
                input.take();
                break;
            }
            if state.supervision_done && !stdout.open() && !stderr.open() && input.is_none() {
                break;
            }
            let event = tokio::select! {
                biased;
                _ = lease.closed(), if state.reason.is_none() => Event::Stopped(stopped_reason(&lease)),
                _ = original.cancellation().cancelled(), if state.reason.is_none() => Event::Cancelled,
                _ = sleep_until(Instant::from_std(lease.deadline)), if state.reason.is_none() => Event::Deadline,
                result = lease.slot.completed(), if !state.supervision_done => Event::Supervised(result.supervision),
                result = input_result(&mut input), if input.is_some() => Event::Input(result),
                _ = observer.changed(), if !state.supervision_done => Event::Observation,
                result = stdout.read(state.reason.is_some()), if stdout.open() => Event::Read(Stream::Stdout, result),
                result = stderr.read(state.reason.is_some()), if stderr.open() => Event::Read(Stream::Stderr, result),
                // A ready native completion or EOF is an observation, not a
                // new wait. Only pending work competes with the cleanup clock.
                _ = cleanup_clock(state.cleanup_end), if !state.cleanup_expired => Event::CleanupDeadline,
            };
            let (stream, bytes) = match event {
                Event::Read(stream, Ok(bytes)) => (stream, bytes),
                Event::Read(stream, Err(reason)) => {
                    match stream {
                        Stream::Stdout => stdout.close(),
                        Stream::Stderr => stderr.close(),
                    }
                    state.reject(reason);
                    continue;
                }
                other => {
                    state.control(other, &mut input);
                    continue;
                }
            };
            if bytes == 0 {
                continue;
            }
            if state.reason.is_some() {
                // Ready bytes are still unfinished drain work. Do not let a
                // continuously writable descendant starve the cleanup clock.
                if state.cleanup_end.is_some_and(|end| Instant::now() >= end) {
                    state.expire_cleanup();
                }
                continue;
            }
            let scratch = match stream {
                Stream::Stdout => &stdout.scratch[..bytes],
                Stream::Stderr => &stderr.scratch[..bytes],
            };
            let cap = match stream {
                Stream::Stdout => limits.stdout_bytes,
                Stream::Stderr => limits.stderr_bytes,
            };
            if let Err(reason) = retain(&mut transcript, stream, scratch, cap) {
                state.reject(reason);
                continue;
            }
            // Allocation/acceptance does not renew permission to start delivery.
            if let Err(reason) = check(original) {
                state.reject(reason);
                continue;
            }
            if state.cleanup_end.is_some_and(|end| Instant::now() >= end) {
                state.expire_cleanup();
                continue;
            }
            if lease.is_closed() {
                state.reject(stopped_reason(&lease));
                continue;
            }
            // No pipe read occurs until this one borrowed callback is finalized.
            let pending = sink.push(
                Chunk {
                    stream,
                    bytes: scratch,
                },
                original,
            );
            tokio::pin!(pending);
            loop {
                state.observe(*observer.borrow_and_update());
                if let Err(reason) = check(original) {
                    state.reject(reason);
                }
                if lease.is_closed() {
                    state.reject(stopped_reason(&lease));
                }
                if state.reason.is_some() {
                    break;
                }
                let event = tokio::select! {
                    biased;
                    _ = lease.closed() => Event::Stopped(stopped_reason(&lease)),
                    _ = original.cancellation().cancelled() => Event::Cancelled,
                    _ = sleep_until(Instant::from_std(lease.deadline)) => Event::Deadline,
                    result = lease.slot.completed(), if !state.supervision_done => Event::Supervised(result.supervision),
                    result = input_result(&mut input), if input.is_some() => Event::Input(result),
                    _ = observer.changed(), if !state.supervision_done => Event::Observation,
                    result = &mut pending => Event::Pushed(result),
                    _ = cleanup_clock(state.cleanup_end), if !state.cleanup_expired => Event::CleanupDeadline,
                };
                match event {
                    Event::Pushed(Ok(())) => {
                        let count = match stream {
                            Stream::Stdout => &mut delivered.stdout,
                            Stream::Stderr => &mut delivered.stderr,
                        };
                        match count.checked_add(bytes as u64) {
                            Some(total) => *count = total,
                            None => state.reject(Reason::OutputLimit(stream)),
                        }
                        break;
                    }
                    Event::Pushed(Err(error)) => {
                        state.reject(Reason::Sink(error));
                        break;
                    }
                    other => state.control(other, &mut input),
                }
            }
            // Cancellation drops only this cancellation-safe callback. The owned
            // supervisor is still polled until its explicit terminal cleanup.
        }
    }
    let observed = Counts {
        stdout: stdout.observed,
        stderr: stderr.observed,
    };
    let pipes = PipeFinalization {
        stdout: stdout.end.expect("finalized stdout"),
        stderr: stderr.end.expect("finalized stderr"),
    };
    // A delivery/observation timeout is the operation's reason. Preserve the
    // cleanup actually consumed from the SAME driver, including uncertainty.
    let cleanup = state
        .cleanup
        .unwrap_or_else(|| pending_cleanup(*observer.borrow()));
    // Keep the terminal observation available when this caller stopped before
    // consuming it, even if the same driver has finished in the meantime.
    let remaining = if !state.supervision_done {
        Some(RemainingProcess::for_slot(Arc::clone(&lease.slot)))
    } else {
        None
    };
    if let Some(reason) = state.reason {
        Err(Failure {
            reason,
            cleanup,
            status: state.status,
            pipes,
            observed,
            delivered,
            transcript,
            remaining,
        })
    } else {
        Ok(Outcome {
            status: state.status.expect("normal leader observed"),
            cleanup,
            pipes,
            observed,
            delivered,
            transcript,
            remaining,
        })
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct Observation {
    pub(crate) status: Option<ExitStatus>,
    pub(crate) cleanup_end: Option<Instant>,
    pub(crate) wait_error: Option<IoFailure>,
    pub(crate) control: ControlFailures,
    pub(crate) group_error: Option<GroupFailure>,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Supervision {
    pub(crate) observation: Observation,
    pub(crate) cleanup: Cleanup,
}
pub(crate) enum Event {
    Stopped(Reason),
    Cancelled,
    Deadline,
    CleanupDeadline,
    Observation,
    Input(Result<(), Reason>),
    Supervised(Supervision),
    Read(Stream, Result<usize, Reason>),
    Pushed(Result<(), SinkFailure>),
}
#[derive(Default)]
pub(crate) struct State {
    pub(crate) reason: Option<Reason>,
    pub(crate) status: Option<ExitStatus>,
    pub(crate) cleanup_end: Option<Instant>,
    pub(crate) cleanup: Option<Cleanup>,
    pub(crate) cleanup_expired: bool,
    pub(crate) supervision_done: bool,
}
impl State {
    pub(crate) fn reject(&mut self, reason: Reason) {
        self.reason.get_or_insert(reason);
    }
    pub(crate) fn expire_cleanup(&mut self) {
        self.cleanup_expired = true;
        self.reject(Reason::Cleanup);
    }
    pub(crate) fn observe(&mut self, observation: Observation) {
        if observation.status.is_some() {
            self.status = observation.status;
        }
        if observation.cleanup_end.is_some() {
            self.cleanup_end = observation.cleanup_end;
        }
        if let Some(error) = observation.wait_error {
            self.reject(Reason::Io(Stage::Wait, error));
        }
    }
    pub(crate) fn control<I>(&mut self, event: Event, input: &mut Option<Pin<Box<I>>>) {
        match event {
            Event::Stopped(reason) => self.reject(reason),
            Event::Cancelled => self.reject(Reason::Cancelled),
            Event::Deadline => self.reject(Reason::TimedOut),
            Event::CleanupDeadline => self.expire_cleanup(),
            Event::Input(result) => {
                input.take();
                if let Err(reason) = result {
                    self.reject(reason);
                }
            }
            Event::Supervised(result) => {
                self.observe(result.observation);
                self.cleanup = Some(result.cleanup);
                self.supervision_done = true;
                if let Some(reason) = cleanup_failure(result.cleanup) {
                    self.reject(reason);
                }
            }
            Event::Observation => (),
            Event::Read(_, _) | Event::Pushed(_) => unreachable!("handled by delivery phase"),
        }
    }
}

pub(crate) fn cleanup_failure(cleanup: Cleanup) -> Option<Reason> {
    match cleanup {
        Cleanup::Complete => None,
        Cleanup::Unconfirmed(
            UnconfirmedCleanup::DriverPanic | UnconfirmedCleanup::HostPumpPanic,
        ) => Some(Reason::Panic(Stage::Wait)),
        _ => Some(Reason::Cleanup),
    }
}
async fn input_result<I: Future<Output = Result<(), Reason>>>(
    input: &mut Option<Pin<Box<I>>>,
) -> Result<(), Reason> {
    input.as_mut().expect("enabled input future").as_mut().await
}
pub(crate) async fn cleanup_clock(endpoint: Option<Instant>) {
    match endpoint {
        Some(end) => sleep_until(end).await,
        None => std::future::pending().await,
    }
}
pub(crate) type Driver = Pin<Box<dyn Future<Output = DriverFinal> + Send + 'static>>;

pub(crate) struct OwnedGuard {
    pub(crate) guard: Guard,
    pub(crate) _command: Option<CommandWrap>,
    #[cfg(unix)]
    pub(crate) _signal: Option<tokio::signal::unix::Signal>,
    pub(crate) control: OwnerControl,
    pub(crate) group_error: Option<GroupFailure>,
}
pub(crate) struct DriverResources {
    pub(crate) owner: Option<OwnedGuard>,
    pub(crate) command: Option<CommandWrap>,
    #[cfg(unix)]
    pub(crate) signal: Option<tokio::signal::unix::Signal>,
}
pub(crate) struct DriverFinal {
    pub(crate) supervision: Supervision,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum RecordPhase {
    Reserved,
    Running,
    Unknown,
    Closed,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RecordSnapshot {
    pub(crate) phase: RecordPhase,
    pub(crate) supervision: Supervision,
}
impl RecordSnapshot {
    pub(crate) fn reserved() -> Self {
        Self {
            phase: RecordPhase::Reserved,
            supervision: Supervision {
                observation: Observation::default(),
                cleanup: Cleanup::NotSpawned,
            },
        }
    }
    pub(crate) fn closed(supervision: Supervision) -> Self {
        Self {
            phase: RecordPhase::Closed,
            supervision,
        }
    }
    pub(crate) fn unknown(supervision: Supervision) -> Self {
        Self {
            phase: RecordPhase::Unknown,
            supervision,
        }
    }
    pub(crate) fn finished(&self) -> bool {
        matches!(self.phase, RecordPhase::Unknown | RecordPhase::Closed)
    }
}
pub(crate) enum HostRecord {
    Operation(OperationRecord),
    Service(crate::service::ServiceRecord),
}
impl HostRecord {
    pub(crate) fn infrastructure_unknown(&self) {
        match self {
            Self::Operation(v) => v.infrastructure_unknown(),
            Self::Service(v) => v.infrastructure_unknown(),
        }
    }
    pub(crate) fn slot_index(&self) -> usize {
        match self {
            Self::Operation(v) => v.slot_index(),
            Self::Service(v) => v.slot_index(),
        }
    }
    pub(crate) fn close(&self) {
        match self {
            Self::Operation(v) => v.close(),
            Self::Service(v) => v.close(),
        }
    }
    pub(crate) fn reject_unpolled(&self) {
        match self {
            Self::Operation(v) => v.reject_unpolled(),
            Self::Service(v) => v.reject_unpolled(),
        }
    }
    pub(crate) fn poll_once(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self {
            Self::Operation(v) => v.poll_once(cx),
            Self::Service(v) => v.poll_once(cx),
        }
    }
}
pub(crate) enum OwnerControl {
    Operation(Arc<ControlLease>),
    Service(Arc<crate::service::ServiceControl>),
}
impl OwnerControl {
    pub(crate) fn close(&self) {
        match self {
            Self::Operation(v) => v.close(),
            Self::Service(v) => v.close(),
        }
    }
    async fn closed(&self) {
        match self {
            Self::Operation(v) => v.closed().await,
            Self::Service(v) => v.closed().await,
        }
    }
    async fn cancelled(&self) {
        match self {
            Self::Operation(v) => v.original.cancellation().cancelled().await,
            Self::Service(v) => v.service.cancelled().await,
        }
    }
    async fn deadline(&self) {
        match self {
            Self::Operation(v) => sleep_until(Instant::from_std(v.deadline)).await,
            Self::Service(_) => std::future::pending::<()>().await,
        }
    }
}
pub(crate) struct OperationRecord {
    driver: Driver,
    _resources: Arc<tokio::sync::Mutex<DriverResources>>,
    lease: Arc<ControlLease>,
    observer: watch::Receiver<Observation>,
}
impl OperationRecord {
    pub(crate) fn infrastructure_unknown(&self) {
        let observation = *self.observer.borrow();
        self.lease
            .slot
            .publish(RecordSnapshot::unknown(Supervision {
                observation,
                cleanup: Cleanup::Unconfirmed(UnconfirmedCleanup::HostPumpPanic),
            }));
        self.lease.close();
    }
    pub(crate) fn slot_index(&self) -> usize {
        self.lease.slot.index
    }
    pub(crate) fn close(&self) {
        self.lease.close();
    }
    pub(crate) fn reject_unpolled(&self) {
        // Queue values are constructed once before the host's sole first poll.
        self.lease.close();
        self.lease.slot.publish(RecordSnapshot::closed(Supervision {
            observation: Observation::default(),
            cleanup: Cleanup::NotSpawned,
        }));
    }
    pub(crate) fn poll_once(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match catch_unwind(AssertUnwindSafe(|| self.driver.as_mut().poll(cx))) {
            Ok(Poll::Ready(result)) => {
                self.lease
                    .slot
                    .publish(RecordSnapshot::closed(result.supervision));
                Poll::Ready(())
            }
            Ok(Poll::Pending) => {
                if !self.lease.slot.observe().finished() {
                    let observation = *self.observer.borrow_and_update();
                    self.lease.slot.publish(RecordSnapshot {
                        phase: RecordPhase::Running,
                        supervision: Supervision {
                            observation,
                            cleanup: pending_cleanup(observation),
                        },
                    });
                }
                Poll::Pending
            }
            Err(_) => {
                let observation = *self.observer.borrow();
                self.lease
                    .slot
                    .publish(RecordSnapshot::unknown(Supervision {
                        observation,
                        cleanup: Cleanup::Unconfirmed(UnconfirmedCleanup::DriverPanic),
                    }));
                self.lease.close();
                // Dropping the failed driver releases the owned SDK guard via RAII.
                Poll::Ready(())
            }
        }
    }
}
struct Started {
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    stdin: Option<ChildStdin>,
    input: Option<Zeroizing<Vec<u8>>>,
    reason: Option<Reason>,
    end: PipeEnd,
}
impl Started {
    fn rejected(reason: Reason, end: PipeEnd) -> Self {
        Self {
            stdout: None,
            stderr: None,
            stdin: None,
            input: None,
            reason: Some(reason),
            end,
        }
    }
}

/// Observes the same host record; it never owns or polls the native future.
pub struct RemainingProcess {
    slot: Arc<crate::host::HostSlot>,
}
impl fmt::Debug for RemainingProcess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RemainingProcess")
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairObservation {
    Pending {
        cleanup: Cleanup,
        status: Option<ExitStatus>,
    },
    Observed {
        cleanup: Cleanup,
        status: Option<ExitStatus>,
    },
}
impl RemainingProcess {
    pub(crate) fn for_slot(slot: Arc<crate::host::HostSlot>) -> Self {
        Self { slot }
    }
    pub async fn repair(&mut self, repair_deadline: std::time::Instant) -> RepairObservation {
        let snapshot = match timeout_at(repair_deadline.into(), self.slot.completed()).await {
            Ok(snapshot) => snapshot,
            Err(_) => self.slot.observe(),
        };
        let cleanup = snapshot.supervision.cleanup;
        let status = snapshot.supervision.observation.status;
        if snapshot.finished() {
            RepairObservation::Observed { cleanup, status }
        } else {
            RepairObservation::Pending { cleanup, status }
        }
    }
}

pub(crate) fn pending_cleanup(observation: Observation) -> Cleanup {
    let leader = if observation.status.is_some() {
        LeaderCleanup::Reaped
    } else if let Some(error) = observation.wait_error {
        LeaderCleanup::Failed(error)
    } else {
        LeaderCleanup::TimedOut
    };
    let mut control = observation.control;
    if let Some(error) = observation.group_error {
        control.group = Some(error);
        return Cleanup::Unconfirmed(UnconfirmedCleanup::GroupUnknownUnix { leader, control });
    }
    if control != ControlFailures::default() {
        Cleanup::ControlFailed { leader, control }
    } else {
        Cleanup::TimedOut
    }
}
pub(crate) fn driver_without_child(cleanup: Cleanup) -> DriverFinal {
    DriverFinal {
        supervision: Supervision {
            observation: Observation::default(),
            cleanup,
        },
    }
}

async fn drive(
    request: Request,
    resources: Arc<tokio::sync::Mutex<DriverResources>>,
    lease: Arc<ControlLease>,
    stop: oneshot::Receiver<()>,
    startup: oneshot::Sender<Started>,
    observations: watch::Sender<Observation>,
) -> DriverFinal {
    let mut resources = resources.lock_owned().await;
    if let Err(reason) = check(&lease.original) {
        let _ = startup.send(Started::rejected(reason, PipeEnd::NotSpawned));
        return driver_without_child(Cleanup::NotSpawned);
    }
    if lease.is_closed() {
        let _ = startup.send(Started::rejected(Reason::Stopped, PipeEnd::NotSpawned));
        return driver_without_child(Cleanup::NotSpawned);
    }
    // Actual production Command allocations start only under the admitted
    // host-owned frame/resource cell. SDK internals remain their own domain.
    if resources.command.is_none() {
        resources.command = Some(command_for(&request));
    }
    #[cfg(unix)]
    {
        let signal = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child()) {
            Ok(signal) => signal,
            Err(error) => {
                let _ = startup.send(Started::rejected(
                    Reason::Io(Stage::Spawn, error.into()),
                    PipeEnd::NotSpawned,
                ));
                return driver_without_child(Cleanup::NotSpawned);
            }
        };
        // The host's resource cell owns this actual SDK registration before birth.
        resources.signal = Some(signal);
    }
    let rejected = check(&lease.original)
        .err()
        .or_else(|| lease.is_closed().then_some(Reason::Stopped));
    if let Some(reason) = rejected {
        let _ = startup.send(Started::rejected(reason, PipeEnd::NotSpawned));
        return driver_without_child(Cleanup::NotSpawned);
    }
    let limits = request.limits;
    let Some(command) = resources.command.as_mut() else {
        let _ = startup.send(Started::rejected(
            Reason::Invalid(InvalidRequest::Budget),
            PipeEnd::NotSpawned,
        ));
        return driver_without_child(Cleanup::NotSpawned);
    };
    let (guard, group_error, setup_reason) = match spawn_owned(command) {
        Ok(child) => (Guard { child, armed: true }, None, None),
        Err(failure) => {
            let failure = *failure;
            let reason = failure.reason;
            match failure.phase {
                SpawnPhase::BeforeNative => {
                    let _ = startup.send(Started::rejected(reason, PipeEnd::NotSpawned));
                    return driver_without_child(Cleanup::NotSpawned);
                }
                SpawnPhase::NativeAttempted => {
                    let _ = startup.send(Started::rejected(reason, PipeEnd::Unobserved));
                    let supervision = Supervision {
                        observation: Observation::default(),
                        cleanup: Cleanup::Unconfirmed(UnconfirmedCleanup::BirthUnobserved),
                    };
                    lease.slot.publish(RecordSnapshot::unknown(supervision));
                    // No actual child was returned; do not fabricate or reacquire one.
                    return DriverFinal { supervision };
                }
                SpawnPhase::ChildObserved => {
                    let (guard, group_error) = recover_guard(failure, command);
                    (guard, group_error, Some(reason))
                }
            }
        }
    };
    // Commit actual custody to the external cell before any postbirth await.
    let command = resources.command.take();
    #[cfg(unix)]
    let signal = resources.signal.take();
    resources.owner = Some(OwnedGuard {
        guard,
        _command: command,
        #[cfg(unix)]
        _signal: signal,
        control: OwnerControl::Operation(Arc::clone(&lease)),
        group_error,
    });
    let owner = resources
        .owner
        .as_mut()
        .expect("committed actual child custody");
    let initial = if let Some(reason) = setup_reason {
        drop(owner.guard.child.stdin().take());
        drop(owner.guard.child.stdout().take());
        drop(owner.guard.child.stderr().take());
        Started::rejected(reason, PipeEnd::Closed)
    } else {
        Started {
            stdout: owner.guard.child.stdout().take(),
            stderr: owner.guard.child.stderr().take(),
            stdin: owner.guard.child.stdin().take(),
            input: request.stdin,
            reason: None,
            end: PipeEnd::Closed,
        }
    };
    if startup.send(initial).is_err() || setup_reason.is_some() {
        owner.control.close();
    }
    let supervision = supervise(owner, limits, Some(stop), observations).await;
    #[cfg(unix)]
    let closure_observed = owner.group_error.is_none()
        && matches!(
            supervision.cleanup,
            Cleanup::Complete
                | Cleanup::ControlFailed {
                    leader: LeaderCleanup::Reaped,
                    ..
                }
        );
    #[cfg(windows)]
    let closure_observed = matches!(
        supervision.cleanup,
        Cleanup::Complete
            | Cleanup::ControlFailed {
                leader: LeaderCleanup::Reaped,
                ..
            }
    );
    if closure_observed {
        owner.guard.armed = false;
    }
    drop(resources.owner.take());
    DriverFinal { supervision }
}

async fn stop_requested(stop: &mut Option<oneshot::Receiver<()>>) {
    match stop {
        Some(receiver) => {
            let _ = receiver.await;
        }
        None => std::future::pending::<()>().await,
    }
}

pub(crate) async fn supervise(
    owner: &mut OwnedGuard,
    limits: Limits,
    mut stop: Option<oneshot::Receiver<()>>,
    observations: watch::Sender<Observation>,
) -> Supervision {
    let (status, wait_error) = {
        let leader = async {
            let target = native_wait_target(owner.guard.child.as_mut(), 3)
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
            target.wait().await
        };
        tokio::pin!(leader);
        tokio::select! {
            biased;
            _ = owner.control.closed() => (None, None),
            _ = stop_requested(&mut stop) => (None, None),
            _ = owner.control.cancelled() => (None, None),
            _ = owner.control.deadline() => (None, None),
            result = &mut leader => match result { Ok(status) => (Some(status), None), Err(error) => (None, Some(error.into())) },
        }
    };
    let (endpoint, endpoint_error) = match Instant::now().checked_add(limits.cleanup) {
        Some(endpoint) => (endpoint, None),
        None => (
            Instant::now(),
            Some(io::Error::from(io::ErrorKind::InvalidInput).into()),
        ),
    };
    let mut observation = Observation {
        status,
        wait_error: wait_error.or(endpoint_error),
        cleanup_end: Some(endpoint),
        control: ControlFailures::default(),
        group_error: owner.group_error,
    };
    observations.send_replace(observation);
    // The SDK owns the process-group or job control independently of leader exit.
    #[cfg(unix)]
    match control_tree(owner.guard.child.as_mut(), |child| {
        child.signal(nix::sys::signal::Signal::SIGTERM as i32)
    })
    .await
    {
        Ok(()) => {
            let end = Instant::now()
                .checked_add(limits.termination_grace)
                .map_or(endpoint, |end| end.min(endpoint));
            sleep_until(end).await;
        }
        Err(error) if already_gone(&error) => (),
        Err(error) => observation.control.terminate = Some(error.into()),
    }
    if let Err(error) = control_tree(owner.guard.child.as_mut(), |child| child.start_kill()).await
        && !already_gone(&error)
    {
        observation.control.kill = Some(error.into());
    }
    observations.send_replace(observation);
    // No timeout wraps this wait. Expiration belongs to the borrowing observer.
    let (leader, status) = match owner.guard.child.wait().await {
        Ok(status) => (LeaderCleanup::Reaped, Some(status)),
        Err(error) => (LeaderCleanup::Failed(error.into()), None),
    };
    observation.status = observation.status.or(status);
    let control = observation.control;
    #[cfg(windows)]
    let cleanup = crate::windows_job::cleanup(owner._command.as_ref(), leader, control).await;
    #[cfg(unix)]
    let cleanup = if let Some(error) = owner.group_error {
        let mut control = control;
        control.group = Some(error);
        Cleanup::Unconfirmed(UnconfirmedCleanup::GroupUnknownUnix { leader, control })
    } else if control != ControlFailures::default() {
        Cleanup::ControlFailed { leader, control }
    } else {
        match leader {
            LeaderCleanup::Reaped => Cleanup::Complete,
            LeaderCleanup::Failed(error) => Cleanup::Failed(error),
            LeaderCleanup::TimedOut => unreachable!("no timeout destroys native wait"),
        }
    };
    observations.send_replace(observation);
    Supervision {
        observation,
        cleanup,
    }
}

pub(crate) struct Reader<P> {
    pub(crate) pipe: Option<P>,
    pub(crate) scratch: [u8; 8192],
    pub(crate) cap: u64,
    pub(crate) stream: Stream,
    pub(crate) observed: u64,
    pub(crate) end: Option<PipeEnd>,
}
impl<P: AsyncRead + Unpin> Reader<P> {
    fn new(pipe: P, cap: usize, stream: Stream) -> Self {
        Self {
            pipe: Some(pipe),
            scratch: [0; 8192],
            cap: cap as u64,
            stream,
            observed: 0,
            end: None,
        }
    }
    pub(crate) fn from_start(pipe: Option<P>, cap: usize, stream: Stream, end: PipeEnd) -> Self {
        match pipe {
            Some(pipe) => Self::new(pipe, cap, stream),
            None => Self {
                pipe: None,
                scratch: [0; 8192],
                cap: cap as u64,
                stream,
                observed: 0,
                end: Some(end),
            },
        }
    }
    pub(crate) fn open(&self) -> bool {
        self.pipe.is_some()
    }
    pub(crate) fn close(&mut self) {
        if self.pipe.take().is_some() {
            self.end = Some(PipeEnd::Closed);
        }
    }
    pub(crate) fn close_if_exhausted(&mut self) {
        if self.observed >= self.cap {
            self.close();
        }
    }
    pub(crate) async fn read(&mut self, discard: bool) -> Result<usize, Reason> {
        let remaining = self.cap.saturating_sub(self.observed);
        let allowed = if discard { remaining } else { remaining + 1 };
        let count = allowed.min(self.scratch.len() as u64) as usize;
        debug_assert!(count != 0, "exhausted discard is closed before reading");
        let bytes = self
            .pipe
            .as_mut()
            .expect("enabled pipe")
            .read(&mut self.scratch[..count])
            .await
            .map_err(|error| Reason::Io(stage(self.stream), error.into()))?;
        if bytes == 0 {
            self.pipe.take();
            self.end = Some(PipeEnd::Eof);
            return Ok(0);
        }
        self.observed = self
            .observed
            .checked_add(bytes as u64)
            .ok_or(Reason::OutputLimit(self.stream))?;
        if self.observed > self.cap {
            return Err(Reason::OutputLimit(self.stream));
        }
        Ok(bytes)
    }
}
fn stage(stream: Stream) -> Stage {
    match stream {
        Stream::Stdout => Stage::Stdout,
        Stream::Stderr => Stage::Stderr,
    }
}
pub(crate) fn retain(
    transcript: &mut Option<Transcript>,
    stream: Stream,
    bytes: &[u8],
    cap: usize,
) -> Result<(), Reason> {
    let Some(transcript) = transcript else {
        return Ok(());
    };
    let output = match stream {
        Stream::Stdout => &mut transcript.storage.stdout,
        Stream::Stderr => &mut transcript.storage.stderr,
    };
    let needed = output
        .len()
        .checked_add(bytes.len())
        .ok_or(Reason::OutputLimit(stream))?;
    if needed > output.capacity() {
        let target = needed.max(output.capacity().saturating_mul(2)).min(cap);
        output
            .try_reserve_exact(target - output.len())
            .map_err(|_| {
                Reason::Io(
                    stage(stream),
                    io::Error::from(io::ErrorKind::OutOfMemory).into(),
                )
            })?;
    }
    output.extend_from_slice(bytes);
    Ok(())
}
pub(crate) fn retained(retention: Retention) -> Option<Transcript> {
    match retention {
        Retention::None => None,
        Retention::Full => Some(Transcript::default()),
    }
}

#[derive(Clone, Copy)]
pub enum SpawnPhase {
    /// Native creation did not acquire a child.
    BeforeNative,
    /// Native creation began, but its outcome was not observed.
    NativeAttempted,
    /// The native child was acquired and remains recoverable.
    ChildObserved,
}
struct RecoveryState {
    phase: SpawnPhase,
    child: Option<Child>,
    armed: bool,
}
type RecoverySlot = Arc<Mutex<RecoveryState>>;
fn recovery_slot(phase: SpawnPhase) -> RecoverySlot {
    Arc::new(Mutex::new(RecoveryState {
        phase,
        child: None,
        armed: true,
    }))
}
struct RecoveryChild {
    child: Option<Child>,
    slot: RecoverySlot,
}
impl std::fmt::Debug for RecoveryChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryChild")
    }
}
impl ChildWrapper for RecoveryChild {
    fn inner(&self) -> &dyn ChildWrapper {
        self.child.as_ref().expect("owned native child")
    }
    fn inner_mut(&mut self) -> &mut dyn ChildWrapper {
        self.child.as_mut().expect("owned native child")
    }
    fn into_inner(mut self: Box<Self>) -> Box<dyn ChildWrapper> {
        Box::new(self.child.take().expect("owned native child"))
    }
    #[cfg(windows)]
    fn process_handle(&self) -> Option<std::os::windows::io::BorrowedHandle<'_>> {
        self.child
            .as_ref()
            .expect("owned native child")
            .process_handle()
    }
}
impl Drop for RecoveryChild {
    fn drop(&mut self) {
        let Some(child) = self.child.take() else {
            return;
        };
        let mut state = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if state.armed {
            // spawn_with_child calls its spawner once: only this Child can arrive.
            // Transfer ownership; no request or Drop is an awaited cleanup proof.
            state.child = Some(child);
        } else {
            drop(state);
            drop(child);
        }
    }
}
/// A failed spawn together with any native child that still needs an owner.
///
/// A recovered child has not retained the command's wrappers. In particular,
/// waiting for it does not prove termination of a Windows Job's descendants.
pub struct SpawnFailure {
    pub reason: Reason,
    pub phase: SpawnPhase,
    pub child: Option<Child>,
}
/// Spawn a prepared command without losing a child to wrapper setup failure.
///
/// The caller supplies executable validation, process-group or Job wrappers,
/// and cancellation. On failure it must retain any recovered child in its
/// teardown owner before awaiting cleanup, preserving the failure's phase and
/// cause. `NativeAttempted` without a child is not confirmed absence of birth.
pub fn spawn_owned(command: &mut CommandWrap) -> Result<Box<dyn ChildWrapper>, Box<SpawnFailure>> {
    let slot = recovery_slot(SpawnPhase::BeforeNative);
    let result = catch_unwind(AssertUnwindSafe(|| {
        command.spawn_with_child(|native| {
            slot.lock().unwrap_or_else(|error| error.into_inner()).phase =
                SpawnPhase::NativeAttempted;
            let child = match native.spawn() {
                Ok(child) => child,
                Err(error) => {
                    slot.lock().unwrap_or_else(|error| error.into_inner()).phase =
                        SpawnPhase::BeforeNative;
                    return Err(error);
                }
            };
            slot.lock().unwrap_or_else(|error| error.into_inner()).phase =
                SpawnPhase::ChildObserved;
            Ok(Box::new(RecoveryChild {
                child: Some(child),
                slot: slot.clone(),
            }) as Box<dyn ChildWrapper>)
        })
    }));
    let reason = match result {
        Ok(Ok(child)) => {
            slot.lock().unwrap_or_else(|error| error.into_inner()).armed = false;
            return Ok(child);
        }
        Ok(Err(error)) => Reason::Io(Stage::Spawn, error.into()),
        Err(_) => Reason::Panic(Stage::Spawn),
    };
    let mut state = slot.lock().unwrap_or_else(|error| error.into_inner());
    Err(Box::new(SpawnFailure {
        reason,
        phase: state.phase,
        child: state.child.take(),
    }))
}
pub(crate) fn recover_guard(
    failure: SpawnFailure,
    command: &CommandWrap,
) -> (Guard, Option<GroupFailure>) {
    let native = failure
        .child
        .expect("closed SDK wrappers return owned child on setup failure");
    #[cfg(unix)]
    let (child, group_error) = {
        let slot = recovery_slot(SpawnPhase::ChildObserved);
        let recovering = Box::new(RecoveryChild {
            child: Some(native),
            slot: slot.clone(),
        });
        let mut group = process_wrap::tokio::ProcessGroup::leader();
        match catch_unwind(AssertUnwindSafe(|| group.wrap_child(recovering, command))) {
            Ok(Ok(child)) => {
                slot.lock().unwrap_or_else(|error| error.into_inner()).armed = false;
                (child, None)
            }
            result => {
                let error = match result {
                    Ok(Err(error)) => GroupFailure::Io(error.into()),
                    Err(_) => GroupFailure::Panic,
                    Ok(Ok(_)) => unreachable!("successful group wrap handled"),
                };
                let child = slot
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .child
                    .take()
                    .expect("owned child survives group wrap failure");
                (Box::new(child) as Box<dyn ChildWrapper>, Some(error))
            }
        }
    };
    #[cfg(windows)]
    let child = {
        let _ = command;
        Box::new(native) as Box<dyn ChildWrapper>
    };
    #[cfg(windows)]
    let group_error = None;
    (Guard { child, armed: true }, group_error)
}

fn not_spawned(reason: Reason, retention: Retention) -> Failure {
    Failure {
        reason,
        cleanup: Cleanup::NotSpawned,
        status: None,
        pipes: PipeFinalization {
            stdout: PipeEnd::NotSpawned,
            stderr: PipeEnd::NotSpawned,
        },
        observed: Counts::default(),
        delivered: Counts::default(),
        transcript: retained(retention),
        remaining: None,
    }
}
pub(crate) fn check(original: &SnapshotContext) -> Result<(), Reason> {
    original.check().map_err(|_| {
        if original.cancellation().is_cancelled() {
            Reason::Cancelled
        } else {
            Reason::TimedOut
        }
    })
}
pub(crate) fn validate(
    request: &Request,
    deadline: std::time::Instant,
) -> Result<(), InvalidRequest> {
    if !request.program.is_absolute() {
        return Err(InvalidRequest::Program);
    }
    #[cfg(windows)]
    if !windows_executable(&request.program) {
        return Err(InvalidRequest::WindowsExecutable);
    }
    if !request.cwd.is_absolute() {
        return Err(InvalidRequest::Cwd);
    }
    let limits = request.limits;
    if limits.cleanup.is_zero()
        || limits.termination_grace >= limits.cleanup
        || deadline.checked_add(limits.cleanup).is_none()
        || std::time::Instant::now()
            .checked_add(limits.cleanup)
            .is_none()
        || limits
            .stdout_bytes
            .checked_add(limits.stderr_bytes)
            .is_none()
        || [limits.stdout_bytes, limits.stderr_bytes]
            .into_iter()
            .any(|cap| {
                u64::try_from(cap)
                    .ok()
                    .and_then(|cap| cap.checked_add(1))
                    .is_none()
            })
    {
        return Err(InvalidRequest::Budget);
    }
    if request
        .stdin
        .as_ref()
        .is_some_and(|input| input.len() > limits.stdin_bytes)
    {
        return Err(InvalidRequest::StdinLimit);
    }
    Ok(())
}
#[cfg(any(windows, test))]
fn windows_executable(program: &std::path::Path) -> bool {
    program
        .extension()
        .is_some_and(|extension| extension.as_encoded_bytes().eq_ignore_ascii_case(b"exe"))
}
pub(crate) async fn write_input(
    mut pipe: Option<ChildStdin>,
    input: Option<Zeroizing<Vec<u8>>>,
) -> Result<(), Reason> {
    if let (Some(pipe), Some(input)) = (&mut pipe, input)
        && let Err(error) = pipe.write_all(&input).await
        && error.kind() != io::ErrorKind::BrokenPipe
    {
        return Err(Reason::Io(Stage::Input, error.into()));
    }
    Ok(())
}
pub(crate) struct Guard {
    pub(crate) child: Box<dyn ChildWrapper>,
    pub(crate) armed: bool,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.child.start_kill();
        }
    }
}
fn already_gone(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(nix::errno::Errno::ESRCH as i32)
    }
    #[cfg(windows)]
    {
        error.kind() == io::ErrorKind::NotFound
    }
}
fn native_wait_target(
    child: &mut dyn ChildWrapper,
    remaining: usize,
) -> Option<&mut dyn ChildWrapper> {
    if std::any::Any::type_id(&*child) == std::any::TypeId::of::<Child>() {
        return Some(child);
    }
    if remaining == 0 {
        return None;
    }
    native_wait_target(child.inner_mut(), remaining - 1)
}
async fn control_tree(
    child: &mut dyn ChildWrapper,
    operation: impl Fn(&mut dyn ChildWrapper) -> io::Result<()>,
) -> io::Result<()> {
    let result = operation(child);
    // Darwin can report EPERM while an exited process is leaving its group.
    // Poll and reap the owned leader during the retry window. An initial
    // nonterminal wait does not rule out a concurrent exit. A persistent
    // permission error remains an unconfirmed cleanup.
    #[cfg(target_os = "macos")]
    let result = {
        let mut result = result;
        // The observer may already have timed out. The owner still reaps the
        // child and resolves transient group errors before reporting cleanup.
        let retry_until = Instant::now() + Duration::from_millis(25);
        while result
            .as_ref()
            .is_err_and(|error| error.raw_os_error() == Some(nix::errno::Errno::EPERM as i32))
            && Instant::now() < retry_until
        {
            let Some(leader) = native_wait_target(child, 3) else {
                break;
            };
            if leader.try_wait().is_err() {
                break;
            }
            sleep_until(retry_until.min(Instant::now() + Duration::from_millis(1))).await;
            result = operation(child);
        }
        result
    };
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_fixture;
    use process_wrap::tokio::CommandWrapper;
    use std::{
        pin::Pin,
        sync::Arc,
        task::{Context, Poll},
    };

    struct ScheduledHostGates {
        start: Option<std::sync::mpsc::SyncSender<()>>,
        close_queue: Option<std::sync::mpsc::SyncSender<()>>,
        pump: Option<std::sync::mpsc::SyncSender<()>>,
    }
    impl ScheduledHostGates {
        fn release(endpoint: &mut Option<std::sync::mpsc::SyncSender<()>>) {
            if let Some(endpoint) = endpoint.take() {
                let _ = endpoint.send(());
            }
        }
    }
    impl Drop for ScheduledHostGates {
        fn drop(&mut self) {
            Self::release(&mut self.start);
            Self::release(&mut self.close_queue);
            Self::release(&mut self.pump);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn installed_starting_and_closed_queue_reject_before_native_birth() {
        use graphit_core::file_lock::Cancellation;
        let limits = crate::HostLimits::new(1, 1).unwrap();
        let service = Cancellation::default();

        let (start_tx, start_rx) = std::sync::mpsc::sync_channel(1);
        let (close_tx, close_rx) = std::sync::mpsc::sync_channel(1);
        let (pump_tx, pump_rx) = std::sync::mpsc::sync_channel(1);
        let (closed_tx, closed_rx) = oneshot::channel();
        let mut gates = ScheduledHostGates {
            start: Some(start_tx),
            close_queue: Some(close_tx),
            pump: Some(pump_tx),
        };
        let host = crate::ProcessHost::install_scheduled(
            &service,
            limits,
            crate::host::FixtureSchedule {
                start: start_rx,
                before_ready: None,
                close_queue: close_rx,
                queue_closed: closed_tx,
                pump: pump_rx,
            },
        )
        .unwrap();
        let original = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));

        let starting = host.ready(std::time::Instant::now()).await;
        let before_ready = operation(&original, &host);
        let not_ready = matches!(
            before_ready,
            Err(crate::OperationAdmissionFailure::HostNotReady)
        );
        drop(before_ready); // A defective admitted pair has no submitted native F.

        ScheduledHostGates::release(&mut gates.start);
        let ready = host
            .ready(std::time::Instant::now() + Duration::from_secs(5))
            .await;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let admitted = operation(&original, &host);
        ScheduledHostGates::release(&mut gates.close_queue);
        let queue_closed = closed_rx.await.is_ok();
        let mut stop = None;
        let mut rejected = None;
        let mut immediate = false;
        if let Ok((endpoint, receiver)) = admitted {
            stop = Some(endpoint);
            let mut sink = IgnoreChunks;
            let future = run(
                native_setup_request(&listener, "finite"),
                &original,
                Retention::None,
                &mut sink,
                receiver,
            );
            tokio::pin!(future);
            // Closed must return the exact unpolled record synchronously.
            // On a defect, keep this SAME caller F and release the actual pump
            // before awaiting it, so cleanup is still possible before asserts.
            let first = std::future::poll_fn(|cx| match future.as_mut().poll(cx) {
                Poll::Ready(result) => Poll::Ready(Some(result)),
                Poll::Pending => Poll::Ready(None),
            })
            .await;
            immediate = first.is_some();
            ScheduledHostGates::release(&mut gates.pump);
            rejected = Some(match first {
                Some(result) => result,
                None => future.await,
            });
        }
        drop(gates); // Releases every actual gate on either result path.
        let report = host_fixture::joined(host).await;
        // Actual host join precedes every assertion, including defective paths.
        let no_birth = listener
            .accept()
            .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock);

        let stopped_before_birth = matches!(
            &rejected,
            Some(Err(Failure {
                reason: Reason::Stopped,
                cleanup: Cleanup::NotSpawned,
                remaining: None,
                ..
            }))
        );
        drop(rejected);
        drop(stop);

        assert!(matches!(starting, crate::HostReadiness::Pending));
        assert!(not_ready);
        assert!(matches!(ready, crate::HostReadiness::Ready));
        assert!(queue_closed && immediate && no_birth && stopped_before_birth);
        assert!(report.failure().is_none());
    }

    fn scheduled_service_host(
        service: &graphit_core::file_lock::Cancellation,
    ) -> (
        crate::ProcessHost,
        ScheduledHostGates,
        oneshot::Receiver<()>,
    ) {
        let (start_tx, start_rx) = std::sync::mpsc::sync_channel(1);
        let (close_tx, close_rx) = std::sync::mpsc::sync_channel(1);
        let (pump_tx, pump_rx) = std::sync::mpsc::sync_channel(1);
        let (closed_tx, _closed_rx) = oneshot::channel();
        let (arrived_tx, arrived_rx) = oneshot::channel();
        let host = crate::ProcessHost::install_scheduled(
            service,
            crate::HostLimits::new(1, 1).unwrap(),
            crate::host::FixtureSchedule {
                start: start_rx,
                before_ready: Some(arrived_tx),
                close_queue: close_rx,
                queue_closed: closed_tx,
                pump: pump_rx,
            },
        )
        .unwrap();
        (
            host,
            ScheduledHostGates {
                start: Some(start_tx),
                close_queue: Some(close_tx),
                pump: Some(pump_tx),
            },
            arrived_rx,
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn service_cancel_at_actual_pre_ready_gate_rejects_admission() {
        let service = graphit_core::file_lock::Cancellation::default();
        let (host, gates, arrived) = scheduled_service_host(&service);
        tokio::time::timeout(Duration::from_secs(5), arrived)
            .await
            .unwrap()
            .unwrap();
        let original = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + Duration::from_secs(10),
            graphit_core::file_lock::Cancellation::default(),
        ));
        service.cancel();
        // The actual Host thread remains behind its Ready gate: this observes
        // cancellation before its select can seal/drain the pump.
        assert!(matches!(
            host.ready(std::time::Instant::now()).await,
            crate::HostReadiness::Sealed
        ));
        assert!(matches!(
            operation(&original, &host),
            Err(crate::OperationAdmissionFailure::HostSealed)
        ));
        drop(gates);
        let report = host_fixture::joined(host).await;
        assert!(
            matches!(report.failure(), Some(crate::HostFailure::Io(error)) if error.kind() == io::ErrorKind::Interrupted)
        );
        assert!(original.check().is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_pending_at_physical_gate_repairs_only_after_same_thread_exits() {
        let service = graphit_core::file_lock::Cancellation::default();
        let (host, gates, arrived) = scheduled_service_host(&service);
        tokio::time::timeout(Duration::from_secs(5), arrived)
            .await
            .unwrap()
            .unwrap();
        // This proves runtime/thread ownership and Pending at a physical gate.
        // Active native drain/Remaining is separately exercised by the SDK tests.
        let mut pending = match host.shutdown(std::time::Instant::now()).await {
            crate::HostShutdown::Pending(pending) => pending,
            crate::HostShutdown::Joined(_) => panic!("blocked actual thread cannot be joined"),
        };
        assert!(matches!(
            pending
                .repair(std::time::Instant::now() + Duration::from_millis(10))
                .await,
            crate::HostShutdownObservation::Pending
        ));
        assert!(!service.is_cancelled());
        drop(gates);
        let report = match pending
            .repair(std::time::Instant::now() + Duration::from_secs(5))
            .await
        {
            crate::HostShutdownObservation::Joined(report) => report,
            crate::HostShutdownObservation::Pending => panic!("released thread did not finish"),
        };
        assert!(report.failure().is_none());
        assert!(matches!(
            pending.repair(std::time::Instant::now()).await,
            crate::HostShutdownObservation::Joined(_)
        ));
        assert!(!service.is_cancelled());
    }

    // This entry selects only a native fixture body, never authority/context.
    #[test]
    fn native_recovery_fixture() {
        use std::{
            io::{Read, Write},
            net::TcpStream,
        };
        let Ok(address) = std::env::var("GRAPHIT_PROCESS_RECOVERY_FIXTURE") else {
            return;
        };
        let mut peer = TcpStream::connect(address).expect("native fixture connects");
        peer.write_all(b"R").expect("native fixture readiness");
        let mode = std::env::var("GRAPHIT_PROCESS_RECOVERY_MODE").unwrap();
        if mode == "held" || mode == "control-finite" || mode == "control-alive" {
            peer.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
            let mut ack = [0];
            peer.read_exact(&mut ack).unwrap();
            assert_eq!(ack, *b"A");
            std::process::exit(7);
        }
        if mode == "immediate-zero" {
            std::process::exit(0);
        }
        if mode == "immediate-seven" {
            std::process::exit(7);
        }
        let finite = mode == "finite" || mode == "zero";
        std::thread::sleep(if finite {
            Duration::from_millis(50)
        } else {
            Duration::from_secs(2)
        });
        // Peer lives until actual process exit; the harness awaits the Child.
        std::process::exit(if mode == "zero" { 0 } else { 7 });
    }

    #[cfg(target_os = "linux")]
    enum NativeProbeFailure {
        Unobserved(IoFailure),
        ObservedAfterUnknown(IoFailure),
        ObservedFailure,
    }
    #[cfg(target_os = "linux")]
    impl fmt::Debug for NativeProbeFailure {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Unobserved(error) => {
                    f.debug_tuple("NativeProbeUnobserved").field(error).finish()
                }
                Self::ObservedAfterUnknown(error) => f
                    .debug_tuple("NativeProbeObservedAfterUnknown")
                    .field(error)
                    .finish(),
                Self::ObservedFailure => f.write_str("NativeProbeObservedFailure"),
            }
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_ignored_sigchld_is_handled_in_a_fresh_process() -> Result<(), NativeProbeFailure> {
        let executable = std::env::current_exe()
            .map_err(|error| NativeProbeFailure::Unobserved(error.into()))?;
        let mut child = std::process::Command::new(env!("GRAPHIT_TEST_SIGCHLD_FIXTURE"))
            .env_clear()
            .env("GRAPHIT_SIGCHLD_FIRST_INSTALL", "1")
            .arg(executable)
            .args([
                "--exact",
                "supervisor::tests::native_first_sigchld_registration",
                "--nocapture",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|error| NativeProbeFailure::Unobserved(error.into()))?;
        let endpoint = std::time::Instant::now() + Duration::from_secs(12);
        let mut failed_observation = None;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if let Some(error) = failed_observation {
                        return Err(NativeProbeFailure::ObservedAfterUnknown(error));
                    }
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(NativeProbeFailure::ObservedFailure)
                    };
                }
                Ok(None) => {
                    if failed_observation.is_none() && std::time::Instant::now() >= endpoint {
                        failed_observation = Some(io::Error::from(io::ErrorKind::TimedOut).into());
                        fixture_unknown();
                    }
                }
                Err(error) => {
                    if failed_observation.is_none() {
                        failed_observation = Some(error.into());
                        fixture_unknown();
                    }
                }
            }
            // This scope retains the actual Child on observation failure.
            // Returning an owned Err to libtest would drop it before repair.
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_first_sigchld_registration() {
        if std::env::var_os("GRAPHIT_SIGCHLD_FIRST_INSTALL").is_none() {
            return;
        }
        // Read the real inherited process disposition before any Tokio runtime.
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let ignored = status
            .lines()
            .find_map(|line| line.strip_prefix("SigIgn:\t"))
            .unwrap();
        let mask = u64::from_str_radix(ignored.trim(), 16).unwrap();
        let signal = nix::sys::signal::Signal::SIGCHLD as u32;
        assert_ne!(
            mask & (1u64 << (signal - 1)),
            0,
            "parent actually inherits SIG_IGN"
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let host = host_fixture::install(8).await;
            use graphit_core::file_lock::Cancellation;
            let original = Arc::new(SnapshotContext::new(
                std::time::Instant::now() + Duration::from_secs(5),
                Cancellation::default(),
            ));

            for (mode, code) in [
                ("zero", 0),
                ("finite", 7),
                ("immediate-zero", 0),
                ("immediate-seven", 7),
            ] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let request = native_setup_request(&listener, mode);
                let (stop, receiver) = operation(&original, &host).unwrap();
                let mut result = run(
                    request,
                    &original,
                    Retention::None,
                    &mut IgnoreChunks,
                    receiver,
                )
                .await;
                drop(stop);
                let remaining = match &mut result {
                    Ok(outcome) => &mut outcome.remaining,
                    Err(failure) => &mut failure.remaining,
                };
                let mut late = false;
                if let Some(owner) = remaining.as_mut() {
                    late =
                        fixture_terminal(owner, std::time::Instant::now() + Duration::from_secs(3))
                            .await
                            .1;
                }
                assert!(
                    !late,
                    "probe deadline expired before the actual driver join"
                );
                let result = result.unwrap();
                assert_eq!(result.status.code(), Some(code));
                assert_eq!(result.cleanup, Cleanup::Complete);
                assert!(result.remaining.is_none());
            }
            let report = host_fixture::joined(host).await;
            assert!(report.failure().is_none());
        });
    }
    #[derive(Clone, Copy, Debug)]
    enum SetupFault {
        BeforeError,
        BeforePanic,
        AfterError,
        AfterPanic,
        ControlError,
        WaitPanic,
    }
    #[derive(Debug)]
    struct SetupFaultWrapper {
        fault: SetupFault,
        listener: std::net::TcpListener,
        endpoint: std::time::Instant,
        release: Arc<Mutex<Option<std::net::TcpStream>>>,
        release_on_kill: bool,
    }
    impl CommandWrapper for SetupFaultWrapper {
        fn pre_spawn(
            &mut self,
            _: &mut tokio::process::Command,
            _: &CommandWrap,
        ) -> io::Result<()> {
            match self.fault {
                SetupFault::BeforeError => Err(io::ErrorKind::PermissionDenied.into()),
                SetupFault::BeforePanic => panic!("static pre-spawn fixture panic"),
                _ => Ok(()),
            }
        }
        fn wrap_child(
            &mut self,
            child: Box<dyn ChildWrapper>,
            _: &CommandWrap,
        ) -> io::Result<Box<dyn ChildWrapper>> {
            // The real child is already owned by the production recovery wrapper.
            // Readiness proves it executed before the SDK fault drops ownership.
            let peer = loop {
                match self.listener.accept() {
                    Ok((peer, _)) => break peer,
                    Err(error)
                        if error.kind() == io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < self.endpoint =>
                    {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(error) => return Err(error),
                }
            };
            let peer = ready_peer(peer, self.endpoint)?;
            match self.fault {
                SetupFault::AfterError => Err(io::ErrorKind::PermissionDenied.into()),
                SetupFault::AfterPanic => panic!("static post-birth fixture panic"),
                SetupFault::ControlError | SetupFault::WaitPanic => {
                    *self.release.lock().unwrap() = Some(peer);
                    Ok(Box::new(FaultChild {
                        child,
                        fault: self.fault,
                        release: Arc::clone(&self.release),
                        release_on_kill: self.release_on_kill,
                    }))
                }
                SetupFault::BeforeError | SetupFault::BeforePanic => {
                    unreachable!("pre-spawn failed")
                }
            }
        }
    }
    struct FaultChild {
        child: Box<dyn ChildWrapper>,
        fault: SetupFault,
        release: Arc<Mutex<Option<std::net::TcpStream>>>,
        release_on_kill: bool,
    }
    impl std::fmt::Debug for FaultChild {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("FaultChild")
        }
    }
    impl ChildWrapper for FaultChild {
        fn inner(&self) -> &dyn ChildWrapper {
            self.child.as_ref()
        }
        fn inner_mut(&mut self) -> &mut dyn ChildWrapper {
            self.child.as_mut()
        }
        fn into_inner(self: Box<Self>) -> Box<dyn ChildWrapper> {
            self.child
        }
        #[cfg(windows)]
        fn process_handle(&self) -> Option<std::os::windows::io::BorrowedHandle<'_>> {
            self.child.process_handle()
        }
        fn start_kill(&mut self) -> io::Result<()> {
            if matches!(self.fault, SetupFault::ControlError) {
                if self.release_on_kill {
                    release_native_fixture(&self.release)?;
                }
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                self.child.start_kill()
            }
        }
        #[cfg(unix)]
        fn signal(&self, signal: i32) -> io::Result<()> {
            if matches!(self.fault, SetupFault::ControlError) {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                self.child.signal(signal)
            }
        }
        fn wait(
            &mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = io::Result<ExitStatus>> + Send + '_>>
        {
            if matches!(self.fault, SetupFault::WaitPanic) {
                Box::pin(async { panic!("static SDK wait fixture panic") })
            } else {
                self.child.wait()
            }
        }
    }
    fn native_setup_request(listener: &std::net::TcpListener, finite: &str) -> Request {
        use std::ffi::OsString;
        let mut environment: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT"))
            .collect();
        environment.push((
            OsString::from("GRAPHIT_PROCESS_RECOVERY_FIXTURE"),
            OsString::from(listener.local_addr().unwrap().to_string()),
        ));
        environment.push((
            OsString::from("GRAPHIT_PROCESS_RECOVERY_MODE"),
            OsString::from(finite),
        ));
        Request {
            program: std::env::current_exe().unwrap(),
            args: [
                "--exact",
                "supervisor::tests::native_recovery_fixture",
                "--nocapture",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: std::env::current_dir().unwrap(),
            environment,
            stdin: None,
            limits: Limits {
                stdin_bytes: 0,
                stdout_bytes: 4096,
                stderr_bytes: 4096,
                termination_grace: Duration::ZERO,
                cleanup: Duration::from_secs(1),
            },
        }
    }
    fn fault_command(
        request: &Request,
        fault: SetupFault,
        original: &SnapshotContext,
        listener: std::net::TcpListener,
    ) -> (CommandWrap, Arc<Mutex<Option<std::net::TcpStream>>>) {
        listener.set_nonblocking(true).unwrap();
        let mut command = command_for(request);
        let release = Arc::new(Mutex::new(None));
        command.wrap(SetupFaultWrapper {
            fault,
            listener,
            endpoint: original.required_deadline().unwrap(),
            release: Arc::clone(&release),
            release_on_kill: request.environment.iter().any(|(key, value)| {
                key == "GRAPHIT_PROCESS_RECOVERY_MODE" && value == "control-finite"
            }),
        });
        (command, release)
    }
    fn release_native_fixture(release: &Mutex<Option<std::net::TcpStream>>) -> io::Result<()> {
        use std::io::Write;
        if let Some(mut peer) = release.lock().unwrap().take() {
            peer.write_all(b"A")?;
        }
        Ok(())
    }
    #[tokio::test(flavor = "current_thread")]
    async fn sdk_wait_panic_preserves_terminal_cause_and_releases_host_slot() {
        use graphit_core::file_lock::Cancellation;
        let host = host_fixture::install(1).await;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let request = native_setup_request(&listener, "finite");
        let original = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        ));
        let (command, _release) =
            fault_command(&request, SetupFault::WaitPanic, &original, listener);
        let (stop, receiver) = operation(&original, &host).unwrap();
        let failure = tokio::time::timeout(
            Duration::from_secs(5),
            run_command(
                request,
                &original,
                Retention::None,
                &mut IgnoreChunks,
                Some(command),
                receiver,
            ),
        )
        .await
        .expect("terminal SDK wait panic must return")
        .unwrap_err();
        assert_eq!(failure.reason, Reason::Panic(Stage::Wait));
        assert_eq!(
            failure.cleanup,
            Cleanup::Unconfirmed(UnconfirmedCleanup::DriverPanic)
        );
        assert_eq!(failure.status.and_then(|status| status.code()), Some(7));
        assert!(failure.remaining.is_none());
        drop(failure);
        drop(stop);
        // Publication wakes the caller before the host drops its terminal record.
        // Observe a real reservation after that physical last owner closes; a
        // retained/leaked record still fails under the SAME original deadline.
        let (stop, receiver) =
            tokio::time::timeout_at(original.required_deadline().unwrap().into(), async {
                loop {
                    match operation(&original, &host) {
                        Ok(pair) => break pair,
                        Err(crate::OperationAdmissionFailure::CapacityFull) => {
                            tokio::task::yield_now().await;
                        }
                        Err(error) => panic!("terminal slot admission failed: {error:?}"),
                    }
                }
            })
            .await
            .expect("terminal record must release the actual host slot");
        drop(receiver);
        drop(stop);
        let report = host_fixture::joined(host).await;
        assert!(report.failure().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn native_setup_errors_and_panics_recover_the_actual_child() {
        let host = host_fixture::install(8).await;
        use graphit_core::file_lock::Cancellation;
        let mut observed = Vec::with_capacity(4);
        for fault in [
            SetupFault::BeforeError,
            SetupFault::BeforePanic,
            SetupFault::AfterError,
            SetupFault::AfterPanic,
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let request = native_setup_request(&listener, "alive");
            let original = Arc::new(SnapshotContext::new(
                std::time::Instant::now() + Duration::from_secs(5),
                Cancellation::default(),
            ));
            let (command, _release) = fault_command(&request, fault, &original, listener);

            let (_stop, operation) = operation(&original, &host).unwrap();
            let mut result = run_command(
                request,
                &original,
                Retention::None,
                &mut IgnoreChunks,
                Some(command),
                operation,
            )
            .await;
            let remaining = match &mut result {
                Ok(outcome) => &mut outcome.remaining,
                Err(failure) => &mut failure.remaining,
            };
            let remaining_before = remaining.is_some();
            #[cfg(unix)]
            if let Some(same) = remaining.as_mut() {
                let _ = fixture_terminal(same, std::time::Instant::now() + Duration::from_secs(5))
                    .await;
            }
            observed.push((fault, result, remaining_before));
        }
        // Every actual hosted child has closed before the assertions below.
        // A pending SDK wait retains its same owner until repair finishes.
        let report = host_fixture::joined(host).await;
        for (fault, result, remaining_before) in &observed {
            match result {
                Ok(outcome) => eprintln!(
                    "setup observation: {fault:?} outcome {:?} remaining={remaining_before}",
                    outcome.cleanup
                ),
                Err(failure) => eprintln!(
                    "setup observation: {fault:?} {:?} {:?} remaining={remaining_before}",
                    failure.reason, failure.cleanup
                ),
            }
        }
        for (fault, result, _) in observed {
            let failure = result.unwrap_err();
            match fault {
                SetupFault::BeforeError | SetupFault::AfterError => assert!(matches!(
                    failure.reason,
                    Reason::Io(Stage::Spawn, error) if error.kind() == io::ErrorKind::PermissionDenied
                )),
                SetupFault::BeforePanic | SetupFault::AfterPanic => {
                    assert_eq!(failure.reason, Reason::Panic(Stage::Spawn))
                }
                SetupFault::ControlError | SetupFault::WaitPanic => unreachable!(),
            }
            if matches!(fault, SetupFault::BeforeError | SetupFault::BeforePanic) {
                assert_eq!(failure.cleanup, Cleanup::NotSpawned);
                assert!(failure.status.is_none());
                assert_eq!(failure.pipes.stdout, PipeEnd::NotSpawned);
            } else {
                assert!(failure.status.is_some());
                assert_eq!(failure.pipes.stdout, PipeEnd::Closed);
                assert_eq!(failure.pipes.stderr, PipeEnd::Closed);
                #[cfg(unix)]
                assert_eq!(failure.cleanup, Cleanup::Complete);
                #[cfg(windows)]
                {
                    // The parent Job remains in CommandWrap after SDK wrapper
                    // failure. Its observed zero is required; real kill errors
                    // remain visible when the SDK's Drop terminated first.
                    match failure.cleanup {
                        Cleanup::Complete => (),
                        Cleanup::ControlFailed { leader, control } => {
                            assert_eq!(leader, LeaderCleanup::Reaped);
                            assert!(control.group.is_none());
                            assert!(control.terminate.is_none());
                            assert!(control.kill.is_some());
                        }
                        other => panic!("retained Job was not observed empty: {other:?}"),
                    }
                }
            }
            assert_eq!(failure.observed, Counts::default());
            assert_eq!(failure.delivered, Counts::default());
        }
        assert!(report.failure().is_none());
    }
    #[test]
    fn windows_executable_policy_excludes_implicit_shell_dispatch() {
        for name in ["tool.exe", "tool.EXE", "space name.ExE"] {
            assert!(windows_executable(std::path::Path::new(name)));
        }
        for name in [
            "tool.bat",
            "tool.BAT",
            "tool.cmd",
            "tool.CMD",
            "tool",
            "tool.exe.",
            "tool.exe ",
        ] {
            assert!(!windows_executable(std::path::Path::new(name)));
        }
    }

    struct StopSink {
        stop: Option<OperationStop>,
    }
    #[cfg(unix)]
    struct Companion {
        _peer: Option<std::net::TcpStream>,
        _child: Box<dyn process_wrap::std::ChildWrapper>,
    }
    #[cfg(unix)]
    impl fmt::Debug for Companion {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Companion")
        }
    }
    #[cfg(unix)]
    fn fixture_unknown() {
        panic!("process fixture did not complete cleanup");
    }
    async fn fixture_terminal(
        owner: &mut RemainingProcess,
        endpoint: std::time::Instant,
    ) -> (RepairObservation, bool) {
        let observed = owner.repair(endpoint).await;
        let late = matches!(observed, RepairObservation::Pending { .. });
        (observed, late)
    }
    fn ready_peer(
        mut peer: std::net::TcpStream,
        endpoint: std::time::Instant,
    ) -> io::Result<std::net::TcpStream> {
        use std::io::Read;
        // Darwin inherits O_NONBLOCK from the listener. The readiness read
        // needs a blocking peer so SO_RCVTIMEO governs the same endpoint.
        peer.set_nonblocking(false)?;
        let budget = endpoint.saturating_duration_since(std::time::Instant::now());
        if budget.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        peer.set_read_timeout(Some(budget))?;
        let mut byte = [0];
        peer.read_exact(&mut byte)?;
        if &byte != b"R" {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(peer)
    }
    #[cfg(unix)]
    #[test]
    fn native_ready_peer_waits_on_nonblocking_peer_until_its_actual_socket_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let _silent_client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        // Reproduce the actual BSD accepted-socket mode on every Unix host.
        peer.set_nonblocking(true).unwrap();
        let endpoint = std::time::Instant::now() + Duration::from_millis(30);
        let error = ready_peer(peer, endpoint).unwrap_err();
        assert!(
            std::time::Instant::now() >= endpoint,
            "readiness returned {:?} before the native read timeout",
            error.kind()
        );
        assert!(matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ));
    }
    #[cfg(unix)]
    #[test]
    fn native_ready_peer_accepts_readiness_and_preserves_protocol_eof_and_expiry_errors() {
        use std::io::Write;
        for (bytes, expired, expected) in [
            (&b"R"[..], false, None),
            (&b"X"[..], false, Some(io::ErrorKind::InvalidData)),
            (&b""[..], false, Some(io::ErrorKind::UnexpectedEof)),
            (&b"R"[..], true, Some(io::ErrorKind::TimedOut)),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (peer, _) = listener.accept().unwrap();
            peer.set_nonblocking(true).unwrap();
            client.write_all(bytes).unwrap();
            client.shutdown(std::net::Shutdown::Write).unwrap();
            let endpoint = if expired {
                std::time::Instant::now()
            } else {
                std::time::Instant::now() + Duration::from_secs(1)
            };
            match (ready_peer(peer, endpoint), expected) {
                (Ok(_), None) => (),
                (Err(error), Some(kind)) => assert_eq!(error.kind(), kind),
                (result, expected) => panic!("readiness result {result:?}, expected {expected:?}"),
            }
        }
    }
    #[cfg(unix)]
    #[derive(Debug)]
    struct GroupCompanionWrapper {
        leader: std::net::TcpListener,
        companion: std::net::TcpListener,
        owned: Arc<Mutex<Option<Companion>>>,
        endpoint: std::time::Instant,
    }
    #[cfg(unix)]
    impl CommandWrapper for GroupCompanionWrapper {
        fn wrap_child(
            &mut self,
            child: Box<dyn ChildWrapper>,
            _: &CommandWrap,
        ) -> io::Result<Box<dyn ChildWrapper>> {
            use std::io::Write;
            fn ready(
                socket: &std::net::TcpListener,
                endpoint: std::time::Instant,
            ) -> io::Result<std::net::TcpStream> {
                socket.set_nonblocking(true)?;
                let peer = loop {
                    match socket.accept() {
                        Ok((peer, _)) => break peer,
                        Err(error)
                            if error.kind() == io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < endpoint =>
                        {
                            std::thread::sleep(Duration::from_millis(1))
                        }
                        Err(error) => return Err(error),
                    }
                };
                ready_peer(peer, endpoint)
            }
            let pid = child
                .try_inner_child()
                .and_then(Child::id)
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
            if pid == 0 || i32::try_from(pid).is_err() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut leader = ready(&self.leader, self.endpoint)?;
            let request = native_setup_request(&self.companion, "held");
            // Keep the companion alive through TERM so the test exercises the
            // owned group's KILL and wait. Darwin skips zombies in killpg.
            let mut command = process_wrap::std::CommandWrap::with_new("/bin/sh", |command| {
                command
                    .args(["-c", "trap '' TERM; exec \"$@\"", "graphit-companion"])
                    .arg(&request.program)
                    .args(&request.args)
                    .current_dir(&request.cwd)
                    .env_clear()
                    .envs(request.environment)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
            });
            command.wrap(process_wrap::std::ProcessGroup::attach_to(pid));
            // The companion Group records its own PID, not the attached PGID.
            // Consume it immediately; never signal/wait through that wrapper.
            let companion = command.spawn()?.into_inner();
            *self.owned.lock().unwrap() = Some(Companion {
                _peer: None,
                _child: companion,
            });
            let peer = ready(&self.companion, self.endpoint)?;
            self.owned
                .lock()
                .unwrap()
                .as_mut()
                .expect("actual companion retained")
                ._peer = Some(peer);
            leader.write_all(b"A")?; // Leader can reap only after companion birth.
            Ok(child)
        }
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn native_caller_abort_result_drop_and_last_host_drop_retain_same_group_wait() {
        use graphit_core::file_lock::Cancellation;
        // Each branch owns a real group companion before any assertion. The
        // leader exits only after that birth; only SAME Group wait observes it.
        for (abort_caller, detach_host) in [(true, false), (true, true), (false, false)] {
            let host = Arc::new(host_fixture::install(1).await);
            let leader = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let companion = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let mut request = native_setup_request(&leader, "held");
            request.limits.cleanup = if abort_caller {
                Duration::from_secs(2)
            } else {
                Duration::from_millis(30)
            };
            let original = Arc::new(SnapshotContext::new(
                std::time::Instant::now() + Duration::from_secs(10),
                Cancellation::default(),
            ));

            let owned = Arc::new(Mutex::new(None));
            let mut command = command_for(&request);
            command.wrap(GroupCompanionWrapper {
                leader,
                companion,
                owned: owned.clone(),
                endpoint: original.required_deadline().unwrap(),
            });
            let (slot_tx, slot_rx) = oneshot::channel();
            let task_host = Arc::clone(&host);
            let task_original = Arc::clone(&original);
            let caller = tokio::spawn(async move {
                let (stop, receiver) = operation(&task_original, &task_host).unwrap();
                let _ = slot_tx.send(Arc::clone(&receiver.lease.slot));
                let result = run_command(
                    request,
                    &task_original,
                    Retention::None,
                    &mut IgnoreChunks,
                    Some(command),
                    receiver,
                )
                .await;
                drop(stop);
                result
            });
            let slot = slot_rx.await.unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while slot.observe().supervision.observation.status.is_none() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("leader status was not observed");
            let leader_code = slot
                .observe()
                .supervision
                .observation
                .status
                .and_then(|s| s.code());
            if abort_caller {
                caller.abort();
            }
            let caller_observation = match caller.await {
                Ok(result) => {
                    let observation = match &result {
                        Ok(outcome) => (None, Some(outcome.status), outcome.cleanup),
                        Err(failure) => (Some(failure.reason), failure.status, failure.cleanup),
                    };
                    // Drop the actual result before shutdown, but assert its
                    // categories only after the same group wait has completed.
                    drop(result);
                    Ok(observation)
                }
                Err(error) => Err(error),
            };

            let host = Arc::try_unwrap(host).expect("caller host owner closed");
            let mut retained_join = None;
            let mut joined_report = None;
            if detach_host {
                drop(host); // Closure retains owners; this branch never claims Joined.
            } else {
                match host.shutdown(std::time::Instant::now()).await {
                    crate::HostShutdown::Joined(report) => joined_report = Some(report),
                    crate::HostShutdown::Pending(mut same) => {
                        for _ in 0..2 {
                            match same
                                .repair(std::time::Instant::now() + Duration::from_millis(5))
                                .await
                            {
                                crate::HostShutdownObservation::Joined(report) => {
                                    joined_report = Some(report);
                                    break;
                                }
                                crate::HostShutdownObservation::Pending => (),
                            }
                        }
                        if joined_report.is_none() {
                            retained_join = Some(same);
                        }
                    }
                }
            }

            // Observe the existing host's closure after its own group kill.
            // No fresh Child::wait or numeric signal occurs.
            if let Some(mut same) = retained_join {
                match same
                    .repair(std::time::Instant::now() + Duration::from_secs(5))
                    .await
                {
                    crate::HostShutdownObservation::Joined(report) => joined_report = Some(report),
                    _ => fixture_unknown(),
                }
            }
            tokio::time::timeout(Duration::from_secs(5), async {
                while slot.observe().phase != RecordPhase::Closed {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("aborted operation closes after companion exit");
            drop(owned.lock().unwrap().take()); // Group's same wait already consumed native owners.
            match caller_observation {
                Ok((reason, status, cleanup)) => {
                    assert!(
                        reason.is_none() || reason == Some(Reason::Cleanup),
                        "abort_caller={abort_caller}, detach_host={detach_host}: reason={reason:?}, cleanup={cleanup:?}"
                    );
                    assert_eq!(status.and_then(|status| status.code()), Some(7));
                }
                Err(error) => assert!(abort_caller && error.is_cancelled(), "{error}"),
            }
            assert_eq!(leader_code, Some(7));
            assert_eq!(slot.observe().supervision.cleanup, Cleanup::Complete);
            assert!(original.check().is_ok());
            if !detach_host {
                assert!(joined_report.is_some_and(|report| report.failure().is_none()));
            }
        }
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn group_cleanup_and_remaining_repairs_preserve_leader_status() {
        let host = host_fixture::install(8).await;

        use graphit_core::file_lock::Cancellation;
        let leader = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let companion = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut request = native_setup_request(&leader, "held");
        request.limits.cleanup = Duration::from_millis(30);
        let original = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + Duration::from_secs(8),
            Cancellation::default(),
        ));

        let owned = Arc::new(Mutex::new(None));
        let mut command = command_for(&request);
        command.wrap(GroupCompanionWrapper {
            leader,
            companion,
            owned: owned.clone(),
            endpoint: original.required_deadline().unwrap(),
        });
        let (stop, receiver) = operation(&original, &host).unwrap();
        let mut result = run_command(
            request,
            &original,
            Retention::None,
            &mut IgnoreChunks,
            Some(command),
            receiver,
        )
        .await;
        drop(stop);
        let (cleanup, status) = match &result {
            Ok(outcome) => (outcome.cleanup, Some(outcome.status)),
            Err(failure) => (failure.cleanup, failure.status),
        };
        let remaining = match &mut result {
            Ok(outcome) => &mut outcome.remaining,
            Err(failure) => &mut failure.remaining,
        };

        let mut observations = Vec::new();
        if let Some(owner) = remaining.as_mut() {
            for _ in 0..2 {
                observations.push(
                    owner
                        .repair(std::time::Instant::now() + Duration::from_millis(5))
                        .await,
                );
            }
        }
        let (terminal, late) = if let Some(owner) = remaining.as_mut() {
            let (observed, late) =
                fixture_terminal(owner, std::time::Instant::now() + Duration::from_secs(6)).await;
            (Some(observed), late)
        } else {
            (None, false)
        };
        let joined = matches!(
            terminal,
            Some(RepairObservation::Observed {
                cleanup: Cleanup::Complete,
                ..
            })
        ) || (terminal.is_none() && cleanup == Cleanup::Complete);
        let companion = owned.lock().unwrap().take();
        let final_cleanup = match terminal.as_ref() {
            Some(RepairObservation::Observed { cleanup, .. }) => *cleanup,
            _ => cleanup,
        };
        // Join the same host before assertions, including on a late observation.
        // Its Group driver alone waits for the companion; a second Child wait
        // could race it or report ECHILD after the actual group was consumed.
        let report = host_fixture::joined(host).await;
        drop(companion);
        // Assertions occur only after the actual join/teardown observation.
        assert!(
            !late,
            "probe deadline expired before the actual driver join"
        );
        assert_eq!(status.unwrap().code(), Some(7));
        if terminal.is_none() {
            assert!(result.is_ok());
            assert_eq!(cleanup, Cleanup::Complete);
            assert!(observations.is_empty());
        } else {
            assert!(matches!(&result, Err(failure) if matches!(failure.reason, Reason::Cleanup)));
            assert_eq!(cleanup, Cleanup::TimedOut);
            assert_eq!(observations.len(), 2);
            for observation in observations {
                assert!(matches!(
                    observation,
                    RepairObservation::Pending { status: Some(status), .. }
                    | RepairObservation::Observed { cleanup: Cleanup::Complete, status: Some(status) }
                    if status.code() == Some(7)
                ));
            }
            assert!(
                matches!(terminal,Some(RepairObservation::Observed {cleanup:Cleanup::Complete,status:Some(status)}) if status.code()==Some(7))
            );
        }
        assert!(joined, "same group wait returned {final_cleanup:?}");
        drop(result);
        assert!(original.check().is_ok());
        assert!(report.failure().is_none());
    }

    impl IncrementalSink for StopSink {
        async fn push<'a>(
            &'a mut self,
            _: Chunk<'a>,
            _: &'a SnapshotContext,
        ) -> Result<(), SinkFailure> {
            if let Some(stop) = self.stop.take() {
                stop.stop();
            }
            Ok(())
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn actual_control_errors_and_pending_wait_stay_in_the_same_remaining_driver() {
        let host = host_fixture::install(8).await;
        use graphit_core::file_lock::Cancellation;
        for mode in ["finite", "alive"] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let native_mode = if mode == "finite" {
                "control-finite"
            } else {
                "control-alive"
            };
            let mut request = native_setup_request(&listener, native_mode);
            request.limits.cleanup = Duration::from_millis(30);
            let original = Arc::new(SnapshotContext::new(
                std::time::Instant::now() + Duration::from_secs(5),
                Cancellation::default(),
            ));
            let (command, release) =
                fault_command(&request, SetupFault::ControlError, &original, listener);

            let (stop, receiver) = operation(&original, &host).unwrap();
            let mut sink = StopSink { stop: Some(stop) };
            let mut result = run_command(
                request,
                &original,
                Retention::None,
                &mut sink,
                Some(command),
                receiver,
            )
            .await;
            let remaining = match &mut result {
                Ok(outcome) => &mut outcome.remaining,
                Err(failure) => &mut failure.remaining,
            };
            let mut short = None;
            let mut observation = None;
            let mut late = false;
            if let Some(owner) = remaining.as_mut() {
                if mode == "alive" {
                    short = Some(
                        owner
                            .repair(std::time::Instant::now() + Duration::from_millis(5))
                            .await,
                    );
                    release_native_fixture(&release).unwrap();
                }
                let (terminal, expired) =
                    fixture_terminal(owner, std::time::Instant::now() + Duration::from_secs(3))
                        .await;
                observation = Some(terminal);
                late = expired;
            }
            // All actual owners are now terminal before any assertion can panic.
            assert!(
                !late,
                "probe deadline expired before the actual driver join"
            );
            let failure = result.unwrap_err();
            assert_eq!(failure.reason, Reason::Stopped);
            let Cleanup::ControlFailed { control, .. } = failure.cleanup else {
                panic!("actual control errors survive outer deadline")
            };
            assert_eq!(
                control.kill.unwrap().kind(),
                io::ErrorKind::PermissionDenied
            );
            #[cfg(unix)]
            assert_eq!(
                control.terminate.unwrap().kind(),
                io::ErrorKind::PermissionDenied
            );
            if mode == "alive" {
                assert!(
                    failure.remaining.is_some(),
                    "active native wait retains its owner"
                );
                assert!(matches!(
                    short,
                    Some(RepairObservation::Pending {
                        cleanup: Cleanup::ControlFailed { .. },
                        ..
                    })
                ));
            }
            let (terminal_cleanup, terminal_status) = match observation {
                Some(RepairObservation::Observed { cleanup, status }) => (cleanup, status),
                Some(RepairObservation::Pending { .. }) => panic!("native wait remained active"),
                None => {
                    assert!(
                        failure.remaining.is_none(),
                        "retained native wait was not observed"
                    );
                    (failure.cleanup, failure.status)
                }
            };
            let Cleanup::ControlFailed {
                leader: LeaderCleanup::Reaped,
                control,
            } = terminal_cleanup
            else {
                panic!("same actual wait completes with control errors")
            };
            let status = terminal_status.expect("actual native exit status");
            assert_eq!(status.code(), Some(7));
            assert!(control.kill.is_some());
        }
        let report = host_fixture::joined(host).await;
        assert!(report.failure().is_none());
    }

    struct ReadyCallbackSink {
        entered: Option<oneshot::Sender<()>>,
        release: Option<oneshot::Receiver<()>>,
    }
    impl IncrementalSink for ReadyCallbackSink {
        async fn push<'a>(
            &'a mut self,
            _: Chunk<'a>,
            _: &'a SnapshotContext,
        ) -> Result<(), SinkFailure> {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
                self.release.take().unwrap().await.unwrap();
            }
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn late_observation_keeps_ready_callback_eof_and_real_cleanup() {
        use graphit_core::file_lock::Cancellation;
        for mode in ["ready", "cancelled", "deadline"] {
            let host = host_fixture::install(1).await;
            let signal = Cancellation::default();
            let original = Arc::new(SnapshotContext::new(
                std::time::Instant::now() + Duration::from_secs(3),
                signal.clone(),
            ));
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let mut request = native_setup_request(&listener, "held");
            request.limits.cleanup = Duration::from_millis(200);
            listener.set_nonblocking(true).unwrap();
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (stop, receiver) = operation(&original, &host).unwrap();
            let slot = Arc::clone(&receiver.lease.slot);
            let (entered_tx, entered_rx) = oneshot::channel();
            let (release_tx, release_rx) = oneshot::channel();
            let mut sink = ReadyCallbackSink {
                entered: Some(entered_tx),
                release: Some(release_rx),
            };
            let mut result = {
                let call = run(request, &original, Retention::Full, &mut sink, receiver);
                tokio::pin!(call);
                tokio::select! {
                    result = &mut call => panic!("native fixture ended before callback gate: {result:?}"),
                    entered = entered_rx => entered.unwrap(),
                }
                // The real child remains alive at this gate. Release it, then
                // observe the SAME Host record without polling the consumer.
                let (mut peer, _) = listener.accept().await.unwrap();
                let mut ready = [0];
                peer.read_exact(&mut ready).await.unwrap();
                assert_eq!(ready, *b"R");
                peer.write_all(b"A").await.unwrap();
                let terminal = slot.completed().await;
                let endpoint = terminal.supervision.observation.cleanup_end.unwrap();
                // The callback is ready before cleanup expires. Only its
                // consumer's next poll is deliberately postponed past it.
                release_tx.send(()).unwrap();
                assert!(Instant::now() < endpoint);
                if mode == "cancelled" {
                    signal.cancel();
                }
                let resume = if mode == "deadline" {
                    Instant::from_std(original.required_deadline().unwrap())
                } else {
                    endpoint
                };
                sleep_until(resume + Duration::from_millis(1)).await;
                call.await
            };
            if let Err(failure) = &mut result
                && let Some(remaining) = failure.remaining.as_mut()
            {
                remaining
                    .repair(original.required_deadline().unwrap())
                    .await;
            }
            drop(stop);
            let report = host_fixture::joined(host).await;
            assert!(report.failure().is_none());
            let (cleanup, status, pipes) = if mode == "ready" {
                let outcome = result.unwrap();
                assert_eq!(outcome.delivered, outcome.observed);
                assert!(outcome.observed.stdout > 0);
                assert!(outcome.remaining.is_none());
                (outcome.cleanup, Some(outcome.status), outcome.pipes)
            } else {
                let failure = result.unwrap_err();
                assert_eq!(
                    failure.reason,
                    if mode == "cancelled" {
                        Reason::Cancelled
                    } else {
                        Reason::TimedOut
                    }
                );
                assert!(failure.remaining.is_none());
                (failure.cleanup, failure.status, failure.pipes)
            };
            assert_eq!(cleanup, Cleanup::Complete);
            assert_eq!(status.unwrap().code(), Some(7));
            assert_eq!(pipes.stdout, PipeEnd::Eof);
            assert_eq!(pipes.stderr, PipeEnd::Eof);
        }
    }

    struct BrokenReader;
    impl AsyncRead for BrokenReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "secret-error-canary",
            )))
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn pipe_failure_preserves_stage_without_os_message() {
        for (stream, stage) in [
            (Stream::Stdout, Stage::Stdout),
            (Stream::Stderr, Stage::Stderr),
        ] {
            let mut reader = Reader::new(BrokenReader, 128, stream);
            let reason = reader.read(false).await.unwrap_err();
            assert!(
                matches!(reason, Reason::Io(s, error) if s == stage && error.kind() == io::ErrorKind::PermissionDenied)
            );
            assert!(!format!("{reason:?}").contains("secret-"));
            assert_eq!(reader.observed, 0);
        }
    }
}
