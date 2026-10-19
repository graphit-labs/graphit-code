//! Linux startup custody for an explicitly transferred native daemon.
//! Application readiness and authority remain the launcher's responsibility.
use graphit_core::file_io::SnapshotContext;
use process_wrap::std::{ChildWrapper, CommandWrap, ProcessSession};
use std::{
    ffi::OsString,
    fmt, io,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

type Child = Box<dyn ChildWrapper>;

pub struct DetachedRequest {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
    pub termination_grace: Duration,
    pub cleanup: Duration,
}
impl fmt::Debug for DetachedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DetachedRequest")
    }
}

#[derive(Debug)]
pub struct DetachedFailure {
    pub cause: io::Error,
    pub status: Option<ExitStatus>,
    pub remaining: Option<DetachedRemaining>,
}
#[derive(Clone, Copy, Debug)]
pub struct DetachedCommit {
    pub pid: u32,
}

struct State {
    child: Option<Child>,
    status: Option<ExitStatus>,
    transferred: bool,
}
impl Drop for State {
    fn drop(&mut self) {
        if !self.transferred
            && let Some(child) = self.child.as_mut()
        {
            // A dead reaper still cannot make dropping provisional custody leave a live child.
            let _ = child.start_kill();
        }
    }
}
type Shared = Arc<Mutex<State>>;
struct Report {
    status: Option<ExitStatus>,
    error: Option<io::Error>,
}
enum Reap {
    Commit,
    Cancel(Option<oneshot::Sender<Report>>),
}

pub struct DetachedStartup {
    pid: u32,
    original: Arc<SnapshotContext>,
    state: Shared,
    sender: Option<mpsc::Sender<Reap>>,
    cleanup: Duration,
    grace: Duration,
}
impl fmt::Debug for DetachedStartup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DetachedStartup")
            .field("pid", &self.pid)
            .finish()
    }
}
impl Drop for DetachedStartup {
    fn drop(&mut self) {
        if self.sender.is_some() {
            let _ = queue_cancel(&mut self.sender, &self.state, self.grace, None);
            self.sender.take();
        }
    }
}
impl DetachedStartup {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let mut state = self.state.lock().map_err(|_| io::ErrorKind::Other)?;
        if let Some(status) = state.status {
            return Ok(Some(status));
        }
        let status = state
            .child
            .as_mut()
            .ok_or(io::ErrorKind::BrokenPipe)?
            .try_wait()?;
        if let Some(status) = status {
            state.status = Some(status);
        }
        Ok(status)
    }
    /// Transfer only after the caller authenticated readiness and matched the native PID.
    /// This synchronous transition does not attest application readiness or mint authority.
    pub fn commit(mut self) -> Result<DetachedCommit, DetachedFailure> {
        if let Err(cause) = self.original.check() {
            return Err(self.failure(cause));
        }
        match self.try_wait() {
            Ok(None) => (),
            Ok(Some(_)) => {
                return Err(self.failure(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "child exited before transfer",
                )));
            }
            Err(cause) => return Err(self.failure(cause)),
        }
        if let Err(cause) = self.original.check() {
            return Err(self.failure(cause));
        }
        let transfer = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let sender = self.sender.as_ref().expect("owned startup decision");
            if sender.send(Reap::Commit).is_ok() {
                state.transferred = true;
                Ok(())
            } else {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
        };
        if let Err(cause) = transfer {
            return Err(self.failure(cause));
        }
        self.sender.take();
        Ok(DetachedCommit { pid: self.pid })
    }
    fn failure(mut self, cause: io::Error) -> DetachedFailure {
        let status = self.state.lock().ok().and_then(|s| s.status);
        let remaining = self.begin_cancel();
        DetachedFailure {
            cause,
            status,
            remaining: Some(remaining),
        }
    }
    fn begin_cancel(&mut self) -> DetachedRemaining {
        let (reply, receiver) = oneshot::channel();
        let mut sender = self.sender.take();
        let receiver = queue_cancel(&mut sender, &self.state, self.grace, Some(reply))
            .ok()
            .map(|()| receiver);
        DetachedRemaining {
            pid: self.pid,
            state: Arc::clone(&self.state),
            sender,
            receiver,
            grace: self.grace,
        }
    }
    /// Teardown does not consult the expired startup context.
    pub async fn cancel_join(mut self, deadline: Instant) -> Result<ExitStatus, DetachedFailure> {
        let mut remaining = self.begin_cancel();
        match remaining.repair(deadline).await {
            Ok(Some(status)) => Ok(status),
            Ok(None) => Err(DetachedFailure {
                cause: io::ErrorKind::TimedOut.into(),
                status: remaining.status(),
                remaining: Some(remaining),
            }),
            Err(cause) => {
                let status = remaining.status();
                Err(DetachedFailure {
                    cause,
                    status,
                    remaining: Some(remaining),
                })
            }
        }
    }
    pub fn cleanup_deadline(&self) -> io::Result<Instant> {
        Instant::now()
            .checked_add(self.cleanup)
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }
}

/// Owns the same native child/reaper after an incomplete cleanup observation.
pub struct DetachedRemaining {
    pid: u32,
    state: Shared,
    sender: Option<mpsc::Sender<Reap>>,
    receiver: Option<oneshot::Receiver<Report>>,
    grace: Duration,
}
impl fmt::Debug for DetachedRemaining {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DetachedRemaining")
            .field("pid", &self.pid)
            .finish()
    }
}
impl Drop for DetachedRemaining {
    fn drop(&mut self) {
        if self.sender.is_some() {
            let _ = queue_cancel(&mut self.sender, &self.state, self.grace, None);
            self.sender.take();
        }
    }
}
impl DetachedRemaining {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn status(&self) -> Option<ExitStatus> {
        self.state.try_lock().ok().and_then(|s| s.status)
    }
    /// Cancelling this observation retains its receiver and the real reaper's child.
    pub async fn repair(&mut self, deadline: Instant) -> io::Result<Option<ExitStatus>> {
        if self.receiver.is_none() {
            let joined = self.state.try_lock().ok().and_then(|state| {
                if state.child.is_none() {
                    state.status
                } else {
                    None
                }
            });
            if let Some(status) = joined {
                return Ok(Some(status));
            }
            let (reply, receiver) = oneshot::channel();
            queue_cancel(&mut self.sender, &self.state, self.grace, Some(reply))?;
            self.receiver = Some(receiver);
        }
        let receiver = self.receiver.as_mut().expect("cleanup observation");
        let report = match tokio::time::timeout_at(deadline.into(), receiver).await {
            Err(_) => return Ok(None),
            Ok(Err(_)) => {
                self.receiver.take();
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            Ok(Ok(report)) => report,
        };
        self.receiver.take();
        if let Some(error) = report.error {
            return Err(error);
        }
        Ok(report.status)
    }
}

/// Creates the reaper before native birth, so thread creation cannot lose a spawned child.
pub async fn spawn_detached(
    request: DetachedRequest,
    startup: &Arc<SnapshotContext>,
) -> Result<DetachedStartup, DetachedFailure> {
    let rejected = |cause| DetachedFailure {
        cause,
        status: None,
        remaining: None,
    };
    let deadline = startup.required_deadline().map_err(rejected)?;
    if deadline.checked_add(request.cleanup).is_none()
        || Instant::now()
            .checked_add(request.termination_grace)
            .is_none()
    {
        return Err(rejected(io::ErrorKind::InvalidInput.into()));
    }
    if !request.program.is_absolute() || !request.cwd.is_absolute() || request.cleanup.is_zero() {
        return Err(rejected(io::ErrorKind::InvalidInput.into()));
    }
    let state = Arc::new(Mutex::new(State {
        child: None,
        status: None,
        transferred: false,
    }));
    let sender = start_reaper(&state, request.termination_grace).map_err(rejected)?;
    startup.check().map_err(rejected)?;
    let mut command = CommandWrap::with_new(&request.program, |command| {
        command
            .args(&request.args)
            .current_dir(&request.cwd)
            .env_clear()
            .envs(request.environment.iter().cloned())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    });
    command.wrap(ProcessSession);
    let child = command.spawn().map_err(rejected)?;
    let pid = child.id();
    state.lock().unwrap_or_else(|e| e.into_inner()).child = Some(child);
    let pending = DetachedStartup {
        pid,
        original: Arc::clone(startup),
        state,
        sender: Some(sender),
        cleanup: request.cleanup,
        grace: request.termination_grace,
    };
    if let Err(cause) = startup.check() {
        return Err(pending.failure(cause));
    }
    Ok(pending)
}

fn start_reaper(state: &Shared, grace: Duration) -> io::Result<mpsc::Sender<Reap>> {
    let (sender, receiver) = mpsc::channel();
    let owned = Arc::clone(state);
    thread::Builder::new()
        .name("graphit-detached-reap".into())
        .spawn(move || reap(owned, receiver, grace))?;
    Ok(sender)
}
fn queue_cancel(
    sender: &mut Option<mpsc::Sender<Reap>>,
    state: &Shared,
    grace: Duration,
    reply: Option<oneshot::Sender<Report>>,
) -> io::Result<()> {
    let action = Reap::Cancel(reply);
    let action = if let Some(current) = sender.as_ref() {
        match current.send(action) {
            Ok(()) => return Ok(()),
            Err(error) => error.0,
        }
    } else {
        action
    };
    // A disconnected reaper does not turn the same retained Child into a new spawn.
    *sender = Some(start_reaper(state, grace)?);
    sender
        .as_ref()
        .expect("created native reaper")
        .send(action)
        .map_err(|_| io::ErrorKind::BrokenPipe.into())
}

fn reap(state: Shared, receiver: mpsc::Receiver<Reap>, grace: Duration) {
    let mut committed = false;
    loop {
        let action = if committed {
            Reap::Commit
        } else {
            receiver.recv().unwrap_or(Reap::Cancel(None))
        };
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(child) = state.child.as_mut() else {
            return;
        };
        let reply = match action {
            Reap::Commit => {
                committed = true;
                None
            }
            Reap::Cancel(reply) => reply,
        };
        let mut error = None;
        let mut cancellation_delivered = committed;
        if !committed {
            // Leader exit does not establish that its owned group has terminated.
            let group_absent = match child.signal(15) {
                Ok(()) => false,
                Err(cause) if cause.raw_os_error() == Some(3) => true,
                Err(cause) => {
                    error = Some(cause);
                    false
                }
            };
            if group_absent {
                cancellation_delivered = true;
            } else {
                let endpoint = Instant::now().checked_add(grace).unwrap_or_else(|| {
                    error.get_or_insert_with(|| io::ErrorKind::InvalidInput.into());
                    Instant::now()
                });
                while Instant::now() < endpoint {
                    if let Err(cause) = child.try_wait() {
                        error.get_or_insert(cause);
                    }
                    thread::sleep(
                        endpoint
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(10)),
                    );
                }
                // Always kill the owned group after grace, including when the leader exited.
                match child.start_kill() {
                    Ok(()) => cancellation_delivered = true,
                    Err(cause) if cause.raw_os_error() == Some(3) => cancellation_delivered = true,
                    Err(cause) => {
                        error.get_or_insert(cause);
                    }
                }
            }
        }
        match child.wait() {
            Ok(status) => {
                state.status = Some(status);
                if cancellation_delivered {
                    state.child.take();
                }
            }
            Err(cause) => {
                error.get_or_insert(cause);
            }
        }
        let status = state.status;
        let reaped = state.child.is_none();
        drop(state);
        if let Some(reply) = reply {
            let _ = reply.send(Report { status, error });
        }
        if reaped {
            return;
        }
        // A failed native wait never discards the owned child or kills a transferred daemon.
        thread::sleep(Duration::from_millis(10));
    }
}
