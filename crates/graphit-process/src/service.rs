//! Native service children share the installed Host but do not inherit an RPC lifetime.
use crate::{
    host::HostSlot,
    supervisor::{
        self, Driver, DriverFinal, DriverResources, Event, Guard, HostRecord, Observation,
        OwnedGuard, OwnerControl, Reader, RecordPhase, RecordSnapshot, SpawnPhase, State,
        Supervision, cleanup_clock, pending_cleanup, retain, retained,
    },
    *,
};
use graphit_core::file_lock::Cancellation;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{
    io::AsyncWriteExt,
    process::{ChildStderr, ChildStdout},
    sync::{Notify, oneshot, watch},
    time::{Instant, sleep_until},
};

/// One cancellation-safe, borrowed delivery at a time. The signal is the actual Host service.
pub trait ServiceSink {
    fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        service: &'a Cancellation,
    ) -> impl Future<Output = Result<(), SinkFailure>> + Send + 'a;
}

pub(crate) struct ServiceControl {
    pub(crate) service: Cancellation,
    pub(crate) slot: Arc<HostSlot>,
    closed: AtomicBool,
    changed: Notify,
}
impl ServiceControl {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.changed.notify_waiters();
        self.slot.wake();
    }
    fn reason(&self) -> Option<Reason> {
        if self.service.is_cancelled() {
            return Some(Reason::Cancelled);
        }
        if self.closed.load(Ordering::Acquire) || self.slot.sealed() {
            return Some(
                if matches!(
                    self.slot.observe().supervision.cleanup,
                    Cleanup::Unconfirmed(
                        UnconfirmedCleanup::DriverPanic | UnconfirmedCleanup::HostPumpPanic
                    )
                ) {
                    Reason::Panic(Stage::Wait)
                } else {
                    Reason::Stopped
                },
            );
        }
        None
    }
    pub(crate) async fn closed(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.reason().is_some() {
                return;
            }
            tokio::select! { _ = changed => (), _ = self.slot.host_closed() => return, _ = self.service.cancelled() => return }
        }
    }
}

/// A service launch with terminal output inherited directly, without capture byte limits.
/// Program, arguments and environment are deliberately absent from Debug.
pub struct InheritedServiceRequest {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
    /// Native input handle. Output is always inherited without buffering.
    pub stdin: std::process::Stdio,
    /// Inherited handle, including a file; never retained as a transcript.
    pub stdout: std::process::Stdio,
    pub termination_grace: Duration,
    pub cleanup: Duration,
}
impl fmt::Debug for InheritedServiceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InheritedServiceRequest")
            .field("termination_grace", &self.termination_grace)
            .field("cleanup", &self.cleanup)
            .finish_non_exhaustive()
    }
}

/// The same Host-owned native child, with no owned stdout or stderr pipes.
/// Drop requests stop; only join supplies the observed native result.
pub struct InheritedServiceProcess {
    inner: ServiceProcess,
}
impl fmt::Debug for InheritedServiceProcess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InheritedServiceProcess")
    }
}
impl InheritedServiceProcess {
    /// PID observed on the actual child handle at native startup, never discovery.
    pub fn pid(&self) -> Option<u32> {
        self.inner.pid
    }
    pub fn stop(&self) {
        self.inner.stop();
    }
    pub fn stop_handle(&self) -> ServiceStop {
        self.inner.stop_handle()
    }
}

/// Actual bootstrapped child and pipe owners. Drop requests local stop; it is not a join receipt.
/// Custody cannot be duplicated:
/// ```compile_fail
/// use graphit_process::ServiceProcess;
/// fn duplicate(process: &ServiceProcess) -> ServiceProcess { process.clone() }
/// ```
pub struct ServiceProcess {
    pid: Option<u32>,
    control: Arc<ServiceControl>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    limits: Limits,
    observer: watch::Receiver<Observation>,
}
impl fmt::Debug for ServiceProcess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ServiceProcess")
    }
}
impl Drop for ServiceProcess {
    fn drop(&mut self) {
        self.control.close();
    }
}
impl ServiceProcess {
    pub fn stop(&self) {
        self.control.close();
    }
    pub fn stop_handle(&self) -> ServiceStop {
        ServiceStop {
            control: Arc::clone(&self.control),
        }
    }
}
pub struct ServiceStop {
    control: Arc<ServiceControl>,
}
impl ServiceStop {
    pub fn stop(&self) {
        self.control.close();
    }
}
impl Drop for ServiceStop {
    fn drop(&mut self) {
        self.control.close();
    }
}
impl fmt::Debug for ServiceStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ServiceStop")
    }
}

/// Observes the SAME Host-owned SDK driver; it cannot reconstruct or poll a second child.
pub struct ServiceRemaining {
    inner: RemainingProcess,
}
impl fmt::Debug for ServiceRemaining {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ServiceRemaining")
    }
}
impl ServiceRemaining {
    pub async fn repair(&mut self, deadline: std::time::Instant) -> RepairObservation {
        self.inner.repair(deadline).await
    }
}
#[derive(Debug)]
pub struct ServiceOutcome {
    pub status: ExitStatus,
    pub cleanup: Cleanup,
    pub pipes: PipeFinalization,
    pub observed: Counts,
    pub delivered: Counts,
    pub transcript: Option<Transcript>,
    pub remaining: Option<ServiceRemaining>,
}
#[derive(Debug)]
pub struct ServiceFailure {
    pub reason: Reason,
    /// Distinct real Host admission result, before native submission.
    pub admission: Option<OperationAdmissionFailure>,
    pub cleanup: Cleanup,
    pub status: Option<ExitStatus>,
    pub pipes: PipeFinalization,
    pub observed: Counts,
    pub delivered: Counts,
    pub transcript: Option<Transcript>,
    pub remaining: Option<ServiceRemaining>,
}
fn rejected(reason: Reason) -> ServiceFailure {
    ServiceFailure {
        reason,
        admission: None,
        cleanup: Cleanup::NotSpawned,
        status: None,
        pipes: PipeFinalization {
            stdout: PipeEnd::NotSpawned,
            stderr: PipeEnd::NotSpawned,
        },
        observed: Counts::default(),
        delivered: Counts::default(),
        transcript: None,
        remaining: None,
    }
}
fn remaining(control: &ServiceControl, snapshot: RecordSnapshot) -> Option<ServiceRemaining> {
    (!snapshot.finished()).then(|| ServiceRemaining {
        inner: RemainingProcess::for_slot(Arc::clone(&control.slot)),
    })
}

/// Spawn and close finite stdin under the original startup request. Successful return has
/// no retained startup context and does not certify application/model readiness.
pub async fn spawn_service(
    request: Request,
    startup: &Arc<SnapshotContext>,
    host: &ProcessHost,
) -> Result<ServiceProcess, ServiceFailure> {
    spawn_service_inner(request, supervisor::OutputMode::Piped, None, startup, host).await
}

/// Startup is fenced by the finite Original; service lifetime follows the Host.
/// Inherited output is not captured, counted as produced, or reported as pipe EOF.
pub async fn spawn_inherited_service(
    request: InheritedServiceRequest,
    startup: &Arc<SnapshotContext>,
    host: &ProcessHost,
) -> Result<InheritedServiceProcess, ServiceFailure> {
    let stdin = request.stdin;
    let stdout = request.stdout;
    let request = Request {
        program: request.program,
        args: request.args,
        cwd: request.cwd,
        environment: request.environment,
        stdin: None,
        limits: Limits {
            stdin_bytes: 0,
            stdout_bytes: 0,
            stderr_bytes: 0,
            termination_grace: request.termination_grace,
            cleanup: request.cleanup,
        },
    };
    match spawn_service_inner(
        request,
        supervisor::OutputMode::Inherited(stdout),
        Some(stdin),
        startup,
        host,
    )
    .await
    {
        Ok(inner) => Ok(InheritedServiceProcess { inner }),
        Err(mut failure) => {
            failure.pipes = PipeFinalization {
                stdout: PipeEnd::NotSpawned,
                stderr: PipeEnd::NotSpawned,
            };
            Err(failure)
        }
    }
}

struct ServiceLaunch {
    request: Request,
    output: supervisor::OutputMode,
    stdin: Option<std::process::Stdio>,
}

async fn spawn_service_inner(
    request: Request,
    output: supervisor::OutputMode,
    stdin: Option<std::process::Stdio>,
    startup: &Arc<SnapshotContext>,
    host: &ProcessHost,
) -> Result<ServiceProcess, ServiceFailure> {
    supervisor::check(startup).map_err(rejected)?;
    let deadline = startup.required_deadline().map_err(|error| {
        let reason = match error.kind() {
            io::ErrorKind::Interrupted => Reason::Cancelled,
            io::ErrorKind::TimedOut => Reason::TimedOut,
            _ => Reason::Invalid(crate::InvalidRequest::InvalidRequestContext),
        };
        let mut failure = rejected(reason);
        failure.admission = Some(error.into());
        failure
    })?;
    supervisor::validate(&request, deadline).map_err(|e| rejected(Reason::Invalid(e)))?;
    let slot = host.reserve().map_err(|e| {
        let mut failure = rejected(Reason::Stopped);
        failure.admission = Some(e);
        failure
    })?;
    let sender = host.sender().ok_or_else(|| rejected(Reason::Stopped))?;
    let control = Arc::new(ServiceControl {
        service: host.service_signal(),
        slot,
        closed: AtomicBool::new(false),
        changed: Notify::new(),
    });
    let limits = request.limits;
    let (startup_sender, mut receiver) = oneshot::channel();
    let (observations, observer) = watch::channel(Observation::default());
    let resources = Arc::new(tokio::sync::Mutex::new(DriverResources {
        owner: None,
        command: None,
        #[cfg(unix)]
        signal: None,
    }));
    // This owner is installed before awaiting so cancellation of the caller closes its record.
    let mut process = ServiceProcess {
        pid: None,
        control: Arc::clone(&control),
        stdout: None,
        stderr: None,
        limits,
        observer: observer.clone(),
    };
    let record = ServiceRecord {
        driver: Box::pin(drive_service(
            ServiceLaunch {
                request,
                output,
                stdin,
            },
            Arc::clone(startup),
            deadline,
            Arc::clone(&resources),
            Arc::clone(&control),
            startup_sender,
            observations,
        )),
        _resources: resources,
        control: Arc::clone(&control),
        observer,
    };
    sender
        .try_send(Box::new(HostRecord::Service(record)))
        .map_err(|e| {
            drop(e.into_inner());
            rejected(Reason::Stopped)
        })?;
    drop(sender);
    let initial = tokio::select! {
        biased;
        result = &mut receiver => match result { Ok(initial) => initial, Err(_) => {
            control.close();
            let snapshot = control.slot.completed().await;
            Err(supervisor::cleanup_failure(snapshot.supervision.cleanup, snapshot.supervision.observation.status).unwrap_or(Reason::Stopped))
        } },
        _ = startup.cancellation().cancelled() => Err(Reason::Cancelled),
        _ = sleep_until(deadline.into()) => Err(Reason::TimedOut),
        _ = control.closed() => Err(control.reason().unwrap_or(Reason::Stopped)),
    };
    let initial = initial.and_then(|pipes| {
        supervisor::check(startup)?;
        if let Some(reason) = control.reason() {
            return Err(reason);
        }
        Ok(pipes)
    });
    match initial {
        Ok((pid, stdout, stderr)) => {
            process.pid = pid;
            process.stdout = stdout;
            process.stderr = stderr;
            Ok(process)
        }
        Err(reason) => {
            control.close();
            let endpoint = Instant::now()
                .checked_add(limits.cleanup)
                .unwrap_or_else(Instant::now);
            let snapshot = match tokio::time::timeout_at(endpoint, control.slot.completed()).await {
                Ok(snapshot) => snapshot,
                Err(_) => control.slot.observe(),
            };
            let mut failure = rejected(reason);
            failure.cleanup = snapshot.supervision.cleanup;
            failure.status = snapshot.supervision.observation.status;
            if failure.cleanup != Cleanup::NotSpawned {
                let end = if matches!(
                    failure.cleanup,
                    Cleanup::Unconfirmed(UnconfirmedCleanup::BirthUnobserved)
                ) {
                    PipeEnd::Unobserved
                } else {
                    PipeEnd::Closed
                };
                failure.pipes = PipeFinalization {
                    stdout: end,
                    stderr: end,
                };
            }
            failure.remaining = remaining(&control, snapshot);
            Err(failure)
        }
    }
}

type ServiceStarted = Result<(Option<u32>, Option<ChildStdout>, Option<ChildStderr>), Reason>;
async fn drive_service(
    launch: ServiceLaunch,
    startup: Arc<SnapshotContext>,
    deadline: std::time::Instant,
    resources: Arc<tokio::sync::Mutex<DriverResources>>,
    control: Arc<ServiceControl>,
    ready: oneshot::Sender<ServiceStarted>,
    observations: watch::Sender<Observation>,
) -> DriverFinal {
    let ServiceLaunch {
        request,
        output,
        stdin,
    } = launch;
    let mut resources = resources.lock_owned().await;
    let prebirth = supervisor::check(&startup)
        .err()
        .or_else(|| control.reason());
    if let Some(reason) = prebirth {
        let _ = ready.send(Err(reason));
        return supervisor::driver_without_child(Cleanup::NotSpawned);
    }
    resources.command = Some(supervisor::command_for_output(&request, output, stdin));
    #[cfg(unix)]
    {
        resources.signal =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child()) {
                Ok(signal) => Some(signal),
                Err(error) => {
                    let _ = ready.send(Err(Reason::Io(Stage::Spawn, error.into())));
                    return supervisor::driver_without_child(Cleanup::NotSpawned);
                }
            };
    }
    if let Some(reason) = supervisor::check(&startup)
        .err()
        .or_else(|| control.reason())
    {
        let _ = ready.send(Err(reason));
        return supervisor::driver_without_child(Cleanup::NotSpawned);
    }
    let command = resources
        .command
        .as_mut()
        .expect("owned command before native birth");
    let (guard, group_error, setup_reason) = match supervisor::spawn_owned(command) {
        Ok(child) => (Guard { child, armed: true }, None, None),
        Err(failure) => {
            let failure = *failure;
            let reason = failure.reason;
            match failure.phase {
                SpawnPhase::BeforeNative => {
                    let _ = ready.send(Err(reason));
                    return supervisor::driver_without_child(Cleanup::NotSpawned);
                }
                SpawnPhase::NativeAttempted => {
                    let _ = ready.send(Err(reason));
                    return supervisor::driver_without_child(Cleanup::Unconfirmed(
                        UnconfirmedCleanup::BirthUnobserved,
                    ));
                }
                SpawnPhase::ChildObserved => {
                    let (guard, group_error) = supervisor::recover_guard(failure, command);
                    (guard, group_error, Some(reason))
                }
            }
        }
    };
    let command = resources.command.take();
    #[cfg(unix)]
    let signal = resources.signal.take();
    resources.owner = Some(OwnedGuard {
        guard,
        _command: command,
        #[cfg(unix)]
        _signal: signal,
        control: OwnerControl::Service(Arc::clone(&control)),
        group_error,
    });
    let owner = resources
        .owner
        .as_mut()
        .expect("committed actual child custody");
    let limits = request.limits;
    let pid = owner.guard.child.id();
    let mut stdin = owner.guard.child.stdin().take();
    let stdout = owner.guard.child.stdout().take();
    let stderr = owner.guard.child.stderr().take();
    let bootstrap = async {
        supervisor::check(&startup)?;
        if let Some(reason) = control.reason() {
            return Err(reason);
        }
        if let Some(reason) = setup_reason {
            return Err(reason);
        }
        if let Some(bytes) = request.stdin {
            let pipe = stdin.as_mut().ok_or_else(|| {
                Reason::Io(
                    Stage::Input,
                    io::Error::from(io::ErrorKind::BrokenPipe).into(),
                )
            })?;
            pipe.write_all(&bytes)
                .await
                .map_err(|e| Reason::Io(Stage::Input, e.into()))?;
            supervisor::check(&startup)?;
            if let Some(reason) = control.reason() {
                return Err(reason);
            }
            pipe.shutdown()
                .await
                .map_err(|e| Reason::Io(Stage::Input, e.into()))?;
        }
        supervisor::check(&startup)?;
        if let Some(reason) = control.reason() {
            return Err(reason);
        }
        Ok(())
    };
    let result = tokio::select! { biased;
        _ = control.closed() => Err(control.reason().unwrap_or(Reason::Stopped)),
        _ = startup.cancellation().cancelled() => Err(Reason::Cancelled),
        _ = sleep_until(deadline.into()) => Err(Reason::TimedOut),
        result = bootstrap => result,
    };
    drop(stdin);
    // No record, native owner or driver keeps the first RPC's Arc after handover.
    drop(startup);
    let initial = result.map(|()| (pid, stdout, stderr));
    if initial.is_err() {
        control.close();
    }
    if ready.send(initial).is_err() {
        control.close();
    }
    let supervision = supervisor::supervise(owner, limits, None, observations).await;
    #[cfg(unix)]
    if owner.group_error.is_none()
        && matches!(
            supervision.cleanup,
            Cleanup::Complete
                | Cleanup::ControlFailed {
                    leader: LeaderCleanup::Reaped,
                    ..
                }
        )
    {
        owner.guard.armed = false;
    }
    drop(resources.owner.take());
    DriverFinal { supervision }
}

pub(crate) struct ServiceRecord {
    driver: Driver,
    _resources: Arc<tokio::sync::Mutex<DriverResources>>,
    control: Arc<ServiceControl>,
    observer: watch::Receiver<Observation>,
}
impl ServiceRecord {
    pub(crate) fn infrastructure_unknown(&self) {
        self.unknown(UnconfirmedCleanup::HostPumpPanic);
    }
    fn unknown(&self, cause: UnconfirmedCleanup) {
        self.control
            .slot
            .publish(RecordSnapshot::unknown(Supervision {
                observation: *self.observer.borrow(),
                cleanup: Cleanup::Unconfirmed(cause),
            }));
        self.control.close();
    }
    pub(crate) fn slot_index(&self) -> usize {
        self.control.slot.index
    }
    pub(crate) fn close(&self) {
        self.control.close();
    }
    pub(crate) fn reject_unpolled(&self) {
        self.control.close();
        self.control
            .slot
            .publish(RecordSnapshot::closed(Supervision {
                observation: Observation::default(),
                cleanup: Cleanup::NotSpawned,
            }));
    }
    pub(crate) fn poll_once(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match catch_unwind(AssertUnwindSafe(|| self.driver.as_mut().poll(cx))) {
            Ok(Poll::Ready(result)) => {
                self.control
                    .slot
                    .publish(RecordSnapshot::closed(result.supervision));
                Poll::Ready(())
            }
            Ok(Poll::Pending) => {
                if !self.control.slot.observe().finished() {
                    let observation = *self.observer.borrow_and_update();
                    self.control.slot.publish(RecordSnapshot {
                        phase: RecordPhase::Running,
                        supervision: Supervision {
                            observation,
                            cleanup: supervisor::pending_cleanup(observation),
                        },
                    });
                }
                Poll::Pending
            }
            Err(_) => {
                self.unknown(UnconfirmedCleanup::DriverPanic);
                Poll::Ready(())
            }
        }
    }
}

/// Await the existing native service driver. No stream sink or replacement driver is created.
/// Dropping this future requests stop through the retained service owner.
pub async fn join_inherited_service(
    process: InheritedServiceProcess,
) -> Result<ServiceOutcome, ServiceFailure> {
    let process = process.inner;
    let control = &process.control;
    let first = tokio::select! {
        biased;
        () = control.closed() => control.reason(),
        _ = control.slot.completed() => None,
    };
    let snapshot = control.slot.completed().await;
    let observation = snapshot.supervision.observation;
    let cleanup = snapshot.supervision.cleanup;
    let reason = first
        .or_else(|| {
            observation
                .wait_error
                .map(|error| Reason::Io(Stage::Wait, error))
        })
        .or_else(|| supervisor::cleanup_failure(cleanup, observation.status));
    let pipes = PipeFinalization {
        stdout: PipeEnd::NotSpawned,
        stderr: PipeEnd::NotSpawned,
    };
    if let Some(reason) = reason {
        return Err(ServiceFailure {
            reason,
            admission: None,
            cleanup,
            status: observation.status,
            pipes,
            observed: Counts::default(),
            delivered: Counts::default(),
            transcript: None,
            remaining: remaining(control, snapshot),
        });
    }
    match observation.status {
        Some(status) => Ok(ServiceOutcome {
            status,
            cleanup,
            pipes,
            observed: Counts::default(),
            delivered: Counts::default(),
            transcript: None,
            remaining: remaining(control, snapshot),
        }),
        None => Err(ServiceFailure {
            reason: Reason::Cleanup,
            admission: None,
            cleanup,
            status: None,
            pipes,
            observed: Counts::default(),
            delivered: Counts::default(),
            transcript: None,
            remaining: remaining(control, snapshot),
        }),
    }
}

/// Deliver real service output; dropping this borrowed future requests local stop.
/// The borrowed sink cannot escape its lifetime:
/// ```compile_fail
/// use graphit_process::{run_service, Retention, ServiceProcess, ServiceSink, ServiceOutcome, ServiceFailure};
/// fn detach<'a, S: ServiceSink + 'a>(process: ServiceProcess, sink: &'a mut S)
///     -> impl std::future::Future<Output = Result<ServiceOutcome, ServiceFailure>> + 'static {
///     run_service(process, Retention::None, sink)
/// }
/// ```
pub async fn run_service<S: ServiceSink>(
    mut process: ServiceProcess,
    retention: Retention,
    sink: &mut S,
) -> Result<ServiceOutcome, ServiceFailure> {
    let control = Arc::clone(&process.control);
    let limits = process.limits;
    let mut stdout = Reader::from_start(
        process.stdout.take(),
        limits.stdout_bytes,
        Stream::Stdout,
        PipeEnd::Closed,
    );
    let mut stderr = Reader::from_start(
        process.stderr.take(),
        limits.stderr_bytes,
        Stream::Stderr,
        PipeEnd::Closed,
    );
    let mut transcript = retained(retention);
    let mut delivered = Counts::default();
    let mut state = State::default();
    let snapshot = control.slot.observe();
    if snapshot.finished() {
        service_event(&mut state, Event::Supervised(snapshot.supervision));
    }
    {
        loop {
            state.observe(*process.observer.borrow_and_update());
            if state.reason.is_none()
                && let Some(reason) = control.reason()
            {
                state.reject(reason);
            }
            if !state.cleanup_expired && state.cleanup_end.is_some_and(|end| Instant::now() >= end)
            {
                state.expire_cleanup();
            }
            if state.reason.is_none() && control.reason().is_some() {
                state.reject(control.reason().unwrap_or(Reason::Stopped));
            }
            if state.reason.is_some() {
                control.close();
                stdout.close_if_exhausted();
                stderr.close_if_exhausted();
            }
            if state.cleanup_expired {
                stdout.close();
                stderr.close();
                break;
            }
            if state.supervision_done && !stdout.open() && !stderr.open() {
                break;
            }
            let event = tokio::select! {
                biased;
                _ = control.closed(), if state.reason.is_none() => Event::Stopped(control.reason().unwrap_or(Reason::Stopped)),
                _ = control.service.cancelled(), if state.reason.is_none() => Event::Cancelled,
                _ = cleanup_clock(state.cleanup_end), if !state.cleanup_expired => Event::CleanupDeadline,
                result = control.slot.completed(), if !state.supervision_done => Event::Supervised(result.supervision),
                _ = process.observer.changed(), if !state.supervision_done => Event::Observation,
                result = stdout.read(state.reason.is_some()), if stdout.open() => Event::Read(Stream::Stdout, result),
                result = stderr.read(state.reason.is_some()), if stderr.open() => Event::Read(Stream::Stderr, result),
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
                    service_event(&mut state, other);
                    continue;
                }
            };
            if bytes == 0 || state.reason.is_some() {
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
            if let Some(reason) = control.reason() {
                state.reject(reason);
                continue;
            }
            if state.cleanup_end.is_some_and(|end| Instant::now() >= end) {
                state.expire_cleanup();
                continue;
            }
            if control.reason().is_some() {
                state.reject(control.reason().unwrap_or(Reason::Stopped));
                continue;
            }
            // No pipe read occurs until this one borrowed callback is finalized.
            let pending = sink.push(
                Chunk {
                    stream,
                    bytes: scratch,
                },
                &control.service,
            );
            tokio::pin!(pending);
            loop {
                state.observe(*process.observer.borrow_and_update());
                if control.reason().is_some() {
                    state.reject(control.reason().unwrap_or(Reason::Stopped));
                }
                if state.reason.is_some() {
                    break;
                }
                let event = tokio::select! {
                    biased;
                    _ = control.closed() => Event::Stopped(control.reason().unwrap_or(Reason::Stopped)),
                    _ = control.service.cancelled() => Event::Cancelled,
                    _ = cleanup_clock(state.cleanup_end), if !state.cleanup_expired => Event::CleanupDeadline,
                    result = control.slot.completed(), if !state.supervision_done => Event::Supervised(result.supervision),
                    _ = process.observer.changed(), if !state.supervision_done => Event::Observation,
                    result = &mut pending => Event::Pushed(result),
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
                    other => service_event(&mut state, other),
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
    let cleanup = match state
        .cleanup
        .unwrap_or_else(|| pending_cleanup(*process.observer.borrow()))
    {
        failed @ Cleanup::Failed(_) => failed,
        failed @ Cleanup::ControlFailed { .. } => failed,
        unknown @ Cleanup::Unconfirmed(
            UnconfirmedCleanup::DriverPanic | UnconfirmedCleanup::HostPumpPanic,
        ) => unknown,
        uncertain @ Cleanup::Unconfirmed(
            UnconfirmedCleanup::TreeUnknownWindows { control, .. }
            | UnconfirmedCleanup::GroupUnknownUnix { control, .. },
        ) if control != ControlFailures::default() => uncertain,
        _ if state.cleanup_expired => Cleanup::TimedOut,
        cleanup => cleanup,
    };
    // Match the cleanup observation consumed above rather than a later driver
    // snapshot, which could suppress the caller's only terminal observation.
    let remaining = (!state.supervision_done).then(|| ServiceRemaining {
        inner: RemainingProcess::for_slot(Arc::clone(&control.slot)),
    });
    if let Some(reason) = state.reason {
        Err(ServiceFailure {
            admission: None,
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
        Ok(ServiceOutcome {
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

fn service_event(state: &mut State, event: supervisor::Event) {
    use supervisor::Event;
    match event {
        Event::Stopped(reason) => state.reject(reason),
        Event::Cancelled => state.reject(Reason::Cancelled),
        Event::CleanupDeadline => state.expire_cleanup(),
        Event::Supervised(result) => {
            state.observe(result.observation);
            state.cleanup = Some(result.cleanup);
            state.supervision_done = true;
            if let Some(reason) = supervisor::cleanup_failure(result.cleanup, state.status) {
                state.reject(reason);
            }
        }
        Event::Observation => (),
        Event::Deadline | Event::Input(_) | Event::Read(_, _) | Event::Pushed(_) => {
            unreachable!("handled by service delivery")
        }
    }
}
