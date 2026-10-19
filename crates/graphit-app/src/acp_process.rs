//! OpenCode Ask's concrete child, bounded SDK pipes and retained native retirement.
#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use agent_client_protocol::{Agent, ByteStreams, Channel, Client, ConnectTo, ConnectionDriver};
use graphit_ai::{cli_codex_wire::CodexWireLimits, cli_opencode_wire::audit_frame};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::Limits;
use std::{
    ffi::OsString,
    io,
    path::Path,
    pin::Pin,
    process::{ExitStatus, Stdio},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf},
    process::{ChildStderr, ChildStdin, ChildStdout, Command},
    sync::{Mutex, OwnedSemaphorePermit},
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) mod windows_child;

struct Input {
    pipe: Option<ChildStdout>,
    pending: Vec<u8>,
    ready: Vec<u8>,
    position: usize,
    // Private native prefix. Never serialize it or include it in Debug/errors.
    native: Vec<u8>,
    observed: usize,
    eof: bool,
    error: Option<io::ErrorKind>,
    wire: CodexWireLimits,
    original: Arc<SnapshotContext>,
    maximum: usize,
}
impl Input {
    fn new(
        pipe: Option<ChildStdout>,
        original: Arc<SnapshotContext>,
        wire: CodexWireLimits,
        maximum: usize,
    ) -> Self {
        Self {
            pipe,
            pending: Vec::new(),
            ready: Vec::new(),
            position: 0,
            native: Vec::new(),
            observed: 0,
            eof: false,
            error: None,
            wire,
            original,
            maximum,
        }
    }
    fn error(&mut self, kind: io::ErrorKind) -> io::Error {
        self.error.get_or_insert(kind);
        io::Error::from(kind)
    }
    fn poll_bytes(
        &mut self,
        cx: &mut Context<'_>,
        destination: &mut ReadBuf<'_>,
        audit: bool,
    ) -> Poll<io::Result<()>> {
        if destination.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        loop {
            if self.position < self.ready.len() {
                let count = destination
                    .remaining()
                    .min(self.ready.len() - self.position);
                destination.put_slice(&self.ready[self.position..self.position + count]);
                self.position += count;
                return Poll::Ready(Ok(()));
            }
            self.ready.clear();
            self.position = 0;
            if !audit {
                self.pending.clear();
            } else if let Some(end) = self
                .pending
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|n| n + 1)
                .or_else(|| (self.eof && !self.pending.is_empty()).then_some(self.pending.len()))
            {
                if end > self.wire.frame_bytes {
                    return Poll::Ready(Err(self.error(io::ErrorKind::FileTooLarge)));
                }
                let frame = &self.pending[..end];
                if let Err(error) = self.original.check() {
                    return Poll::Ready(Err(self.error(error.kind())));
                }
                if audit_frame(frame, &self.wire, &self.original).is_err() {
                    return Poll::Ready(Err(self.error(io::ErrorKind::InvalidData)));
                }
                self.ready = self.pending.drain(..end).collect();
                continue;
            } else if self.pending.len() > self.wire.frame_bytes {
                return Poll::Ready(Err(self.error(io::ErrorKind::FileTooLarge)));
            }
            if self.eof || self.pipe.is_none() {
                return Poll::Ready(Ok(()));
            }
            let mut bytes = [0; 4096];
            let mut read = ReadBuf::new(&mut bytes);
            match Pin::new(self.pipe.as_mut().expect("retained stdout")).poll_read(cx, &mut read) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    self.error.get_or_insert(error.kind());
                    self.pipe = None;
                    return Poll::Ready(Err(error));
                }
                Poll::Ready(Ok(())) => {}
            }
            let bytes = read.filled();
            if bytes.is_empty() {
                self.eof = true;
                self.pipe = None;
                continue;
            }
            let Some(total) = self.observed.checked_add(bytes.len()) else {
                self.pipe = None;
                return Poll::Ready(Err(self.error(io::ErrorKind::FileTooLarge)));
            };
            self.observed = total;
            let retained = self
                .maximum
                .saturating_sub(self.native.len())
                .min(bytes.len());
            if self.native.try_reserve(retained).is_err() {
                self.pipe = None;
                return Poll::Ready(Err(self.error(io::ErrorKind::OutOfMemory)));
            }
            self.native.extend_from_slice(&bytes[..retained]);
            if audit {
                if total > self.maximum {
                    return Poll::Ready(Err(self.error(io::ErrorKind::FileTooLarge)));
                }
                if self.pending.try_reserve(bytes.len()).is_err() {
                    return Poll::Ready(Err(self.error(io::ErrorKind::OutOfMemory)));
                }
                self.pending.extend_from_slice(bytes);
            } else {
                let count = destination.remaining().min(bytes.len());
                destination.put_slice(&bytes[..count]);
                return Poll::Ready(Ok(()));
            }
        }
    }
    fn closed(&self) -> bool {
        self.pipe.is_none() && self.pending.is_empty() && self.position == self.ready.len()
    }
}
struct Incoming {
    input: Arc<StdMutex<Input>>,
    dropped: Arc<AtomicBool>,
    stop: Cancellation,
    sdk: bool,
}
impl AsyncRead for Incoming {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        destination: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut input = self.input.lock().unwrap_or_else(|error| error.into_inner());
        if self.sdk
            && let Some(kind) = input.error
        {
            return Poll::Ready(Err(io::Error::from(kind)));
        }
        let audit = input.error.is_none();
        let result = input.poll_bytes(cx, destination, audit);
        if matches!(&result, Poll::Ready(Err(_))) {
            self.stop.cancel();
        }
        result
    }
}
impl Drop for Incoming {
    fn drop(&mut self) {
        if self.sdk {
            self.dropped.store(true, Ordering::Release);
        }
    }
}
struct Outgoing {
    pipe: ChildStdin,
    written: usize,
    maximum: usize,
    stop: Cancellation,
}
impl AsyncWrite for Outgoing {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this
            .written
            .checked_add(bytes.len())
            .is_none_or(|n| n > this.maximum)
        {
            this.stop.cancel();
            return Poll::Ready(Err(io::ErrorKind::FileTooLarge.into()));
        }
        let result = Pin::new(&mut this.pipe).poll_write(cx, bytes);
        if let Poll::Ready(Ok(count)) = result {
            this.written += count;
        }
        result
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().pipe).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().pipe).poll_shutdown(cx)
    }
}
#[cfg(unix)]
struct Group(Option<rustix::process::Pid>);
#[cfg(unix)]
impl Group {
    fn absent(&mut self) -> io::Result<bool> {
        let Some(pid) = self.0 else { return Ok(true) };
        match rustix::process::test_kill_process_group(pid) {
            Err(rustix::io::Errno::SRCH) => {
                self.0 = None;
                Ok(true)
            }
            Ok(()) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    fn signal(&self, signal: rustix::process::Signal) -> io::Result<()> {
        if let Some(pid) = self.0 {
            match rustix::process::kill_process_group(pid, signal) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
                Err(error) => Err(error.into()),
            }
        } else {
            Ok(())
        }
    }
}
#[cfg(unix)]
impl Drop for Group {
    fn drop(&mut self) {
        let _ = self.signal(rustix::process::Signal::KILL);
    }
}
struct Observation {
    status: ExitStatus,
    #[cfg(unix)]
    group_absent: bool,
    group_kill_completed: bool,
    wait_error: Option<io::ErrorKind>,
    signal_error: Option<io::ErrorKind>,
    stderr_bytes: usize,
    stderr_eof: bool,
    stderr_error: Option<io::ErrorKind>,
}
enum Retirement {
    Joined(Observation),
    #[cfg(windows)]
    Retained(windows::Retained),
}
enum OwnerState {
    Joining(JoinHandle<Retirement>),
    Joined(Observation),
    #[cfg(windows)]
    Retained(windows::Retained),
    Failed(tokio::task::JoinError),
}
pub(crate) struct AcpProcessOwner {
    state: Mutex<OwnerState>,
    permit: Arc<StdMutex<Option<OwnedSemaphorePermit>>>,
    input: Arc<StdMutex<Input>>,
    stop: Cancellation,
}
impl AcpProcessOwner {
    pub(crate) fn stop(&self) {
        self.stop.cancel();
    }
    /// Await the same retained task by mutable borrow. Abandonment leaves custody intact.
    pub(crate) async fn observe(&self, deadline: Instant) -> bool {
        let Ok(mut state) = tokio::time::timeout_at(deadline.into(), self.state.lock()).await
        else {
            return false;
        };
        if let OwnerState::Joining(task) = &mut *state
            && let Ok(result) = tokio::time::timeout_at(deadline.into(), task).await
        {
            *state = match result {
                Ok(Retirement::Joined(observation)) => OwnerState::Joined(observation),
                #[cfg(windows)]
                Ok(Retirement::Retained(retained)) => OwnerState::Retained(retained),
                Err(error) => OwnerState::Failed(error),
            };
        }
        let joined = matches!(*state, OwnerState::Joined(_));
        if joined {
            self.permit
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take();
        }
        joined
    }
    pub(crate) async fn clean(&self) -> bool {
        let state = self.state.lock().await;
        let input = self.input.lock().unwrap_or_else(|error| error.into_inner());
        matches!(&*state,OwnerState::Joined(observation) if observation.status.success()&&observation.wait_error.is_none()&&observation.signal_error.is_none()&&observation.stderr_eof&&observation.stderr_error.is_none()&&input.eof&&input.error.is_none()&&input.closed())
    }
    pub(crate) async fn public_observation(&self) -> serde_json::Value {
        let Ok(state) = self.state.try_lock() else {
            return serde_json::json!({"joined":false,"observation_pending":true});
        };
        let input = self.input.lock().unwrap_or_else(|error| error.into_inner());
        match &*state {
            OwnerState::Joined(o) => {
                #[cfg(unix)]
                {
                    serde_json::json!({"joined":true,"group_absent":o.group_absent,"group_kill_completed":o.group_kill_completed,"exit_code":o.status.code(),"status_success":o.status.success(),"wait_error":o.wait_error.map(|e|format!("{e:?}")),"signal_error":o.signal_error.map(|e|format!("{e:?}")),"stdout_bytes":input.observed,"stdout_eof":input.eof,"stdout_error":input.error.map(|e|format!("{e:?}")),"stderr_bytes":o.stderr_bytes,"stderr_eof":o.stderr_eof,"stderr_error":o.stderr_error.map(|e|format!("{e:?}"))})
                }
                #[cfg(windows)]
                {
                    serde_json::json!({"joined":true,"job_wait_completed":true,"job_termination_requested":o.group_kill_completed,"exit_code":o.status.code(),"status_success":o.status.success(),"wait_error":o.wait_error.map(|e|format!("{e:?}")),"signal_error":o.signal_error.map(|e|format!("{e:?}")),"stdout_bytes":input.observed,"stdout_eof":input.eof,"stdout_error":input.error.map(|e|format!("{e:?}")),"stderr_bytes":o.stderr_bytes,"stderr_eof":o.stderr_eof,"stderr_error":o.stderr_error.map(|e|format!("{e:?}"))})
                }
            }
            #[cfg(windows)]
            OwnerState::Retained(retained) => retained.observation(),
            OwnerState::Failed(error) => {
                serde_json::json!({"joined":false,"task_failed":true,"panic":error.is_panic(),"cancelled":error.is_cancelled()})
            }
            OwnerState::Joining(_) => serde_json::json!({"joined":false}),
        }
    }
}
pub(crate) struct AcpProcess {
    incoming: Option<Incoming>,
    outgoing: Option<Outgoing>,
    owner: Arc<AcpProcessOwner>,
    completed: bool,
}
impl Drop for AcpProcess {
    fn drop(&mut self) {
        if !self.completed {
            self.owner.stop();
        }
    }
}
impl ConnectTo<Client> for AcpProcess {
    fn into_channel_and_future(mut self) -> (Channel, Option<ConnectionDriver>) {
        let incoming = self.incoming.take().expect("single ACP input");
        let outgoing = self.outgoing.take().expect("single ACP output");
        let (channel, driver) = ConnectTo::<Client>::into_channel_and_future(ByteStreams::new(
            outgoing.compat_write(),
            incoming.compat(),
        ));
        // Keep the SDK's physical transport finish metadata. An opaque replacement
        // driver can be dropped when the foreground callback ends, before native EOF.
        let driver = driver.map(|driver| {
            driver.map_future(|work| async move {
                let mut process = self;
                let result = work.await;
                process.completed = result.is_ok();
                result
            })
        });
        (channel, driver)
    }

    async fn connect_to(
        mut self,
        client: impl ConnectTo<Agent>,
    ) -> Result<(), agent_client_protocol::Error> {
        let incoming = self.incoming.take().expect("single ACP input");
        let outgoing = self.outgoing.take().expect("single ACP output");
        let result = ConnectTo::<Client>::connect_to(
            ByteStreams::new(outgoing.compat_write(), incoming.compat()),
            client,
        )
        .await;
        self.completed = true;
        if result.is_err() {
            self.owner.stop();
        }
        result
    }
}

/// The caller checks the actual admission immediately before this synchronous spawn,
/// then registers the returned owner before its next await.
pub(crate) struct AcpLaunch<'a> {
    pub program: &'a Path,
    pub cwd: &'a Path,
    pub environment: &'a [(OsString, OsString)],
    pub original: Arc<SnapshotContext>,
    pub service_stop: Cancellation,
    pub limits: Limits,
    pub wire: CodexWireLimits,
    pub permit: OwnedSemaphorePermit,
}
pub(crate) struct SpawnFailure {
    pub error: io::Error,
    pub owner: Option<Arc<AcpProcessOwner>>,
}
impl std::fmt::Debug for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpSpawnFailure")
            .field("kind", &self.error.kind())
            .field("owner_retained", &self.owner.is_some())
            .finish()
    }
}
impl From<io::Error> for SpawnFailure {
    fn from(error: io::Error) -> Self {
        Self { error, owner: None }
    }
}
impl From<io::ErrorKind> for SpawnFailure {
    fn from(kind: io::ErrorKind) -> Self {
        io::Error::from(kind).into()
    }
}
pub(crate) fn spawn(
    launch: AcpLaunch<'_>,
) -> Result<(AcpProcess, Arc<AcpProcessOwner>), SpawnFailure> {
    let AcpLaunch {
        program,
        cwd,
        environment,
        original,
        service_stop,
        limits,
        wire,
        permit,
    } = launch;
    original.required_deadline()?;
    original.check()?;
    if limits.stdin_bytes == 0 || limits.stdout_bytes == 0 || limits.stderr_bytes == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if service_stop.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let mut command = Command::new(program);
    command
        .arg("acp")
        .current_dir(cwd)
        .env_clear()
        .envs(environment.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    let mut child = command.process_group(0).spawn()?;
    #[cfg(windows)]
    let (mut child, permit) = windows::spawn(command, permit, &original, limits, &wire)?;
    #[cfg(unix)]
    let group = Group(Some(
        rustix::process::Pid::from_raw(child.id().expect("spawned child") as i32)
            .expect("positive child id"),
    ));
    #[cfg(unix)]
    let (outgoing, incoming, stderr) = (
        child.stdin.take().expect("piped stdin"),
        child.stdout.take().expect("piped stdout"),
        child.stderr.take().expect("piped stderr"),
    );
    #[cfg(windows)]
    let (outgoing, incoming, stderr) = (
        child.stdin.take().expect("piped stdin"),
        child.stdout.take().expect("piped stdout"),
        child.stderr.take().expect("piped stderr"),
    );
    let stop = Cancellation::default();
    let dropped = Arc::new(AtomicBool::new(false));
    let input = Arc::new(StdMutex::new(Input::new(
        Some(incoming),
        Arc::clone(&original),
        wire,
        limits.stdout_bytes,
    )));
    let task_input = Arc::clone(&input);
    let task_stop = stop.clone();
    let task_dropped = Arc::clone(&dropped);
    let permit = Arc::new(StdMutex::new(Some(permit)));
    let task_permit = Arc::clone(&permit);
    #[cfg(unix)]
    let task = tokio::spawn(async move {
        let _retained_permit = task_permit;
        let mut group = group;
        let mut stderr: Option<ChildStderr> = Some(stderr);
        let mut status = None;
        let mut wait_error = None;
        let mut signal_error = None;
        let mut stderr_bytes = 0usize;
        let mut stderr_eof = false;
        let mut stderr_error = None;
        let mut stopping = None;
        let mut killed = false;
        let mut group_kill_completed = false;
        let mut eof_requested = None;
        let mut bytes = [0; 4096];
        let mut drain = [0; 4096];
        let mut incoming = Incoming {
            input: Arc::clone(&task_input),
            dropped: Arc::clone(&task_dropped),
            stop: task_stop.clone(),
            sdk: false,
        };
        loop {
            let absent = match group.absent() {
                Ok(value) => value,
                Err(error) => {
                    signal_error.get_or_insert(error.kind());
                    false
                }
            };
            let dropped = task_dropped.load(Ordering::Acquire);
            if dropped && eof_requested.is_none() {
                eof_requested = Some(Instant::now());
            }
            let closed = task_input
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .closed();
            if let Some(status) = status
                && (absent || group_kill_completed)
                && dropped
                && closed
                && stderr.is_none()
            {
                // KILL + same-child wait + pipe EOF is the ProcessHost contract.
                // A stopped descendant awaiting its parent's reap need not be ESRCH.
                group.0 = None;
                return Retirement::Joined(Observation {
                    status,
                    group_absent: absent,
                    group_kill_completed,
                    wait_error,
                    signal_error,
                    stderr_bytes,
                    stderr_eof,
                    stderr_error,
                });
            }
            if stopping.is_none()
                && (status.is_some()
                    || task_stop.is_cancelled()
                    || service_stop.is_cancelled()
                    || original.check().is_err()
                    || eof_requested
                        .is_some_and(|start| start.elapsed() >= limits.termination_grace))
            {
                stopping = Some(Instant::now());
                if let Err(error) = group.signal(rustix::process::Signal::TERM) {
                    signal_error.get_or_insert(error.kind());
                }
            }
            if !killed && stopping.is_some_and(|start| start.elapsed() >= limits.termination_grace)
            {
                killed = true;
                match group.signal(rustix::process::Signal::KILL) {
                    Ok(()) => group_kill_completed = true,
                    Err(error) => {
                        signal_error.get_or_insert(error.kind());
                    }
                }
            }
            tokio::select! {
                result=child.wait(),if status.is_none()=>match result {Ok(value)=>status=Some(value),Err(error)=>{wait_error.get_or_insert(error.kind());tokio::time::sleep(Duration::from_millis(10)).await;}},
                result=async {stderr.as_mut().expect("retained stderr").read(&mut bytes).await},if stderr.is_some()=>match result {
                    Ok(0)=>{stderr_eof=true;stderr=None;},
                    Ok(count)=>{stderr_bytes=stderr_bytes.saturating_add(count);if stderr_bytes>limits.stderr_bytes {stderr_error.get_or_insert(io::ErrorKind::FileTooLarge);task_stop.cancel();}},
                    Err(error)=>{stderr_error.get_or_insert(error.kind());stderr=None;task_stop.cancel();},
                },
                result=incoming.read(&mut drain),if dropped&&!closed=>{if result.is_err(){task_stop.cancel();}},
                ()=task_stop.cancelled(),if stopping.is_none()=>{},
                ()=service_stop.cancelled(),if stopping.is_none()=>{},
                ()=original.cancellation().cancelled(),if stopping.is_none()=>{},
                ()=tokio::time::sleep(Duration::from_millis(10))=>{},
            }
        }
    });
    #[cfg(windows)]
    let task = tokio::spawn(
        windows::Run {
            child,
            stderr: Some(stderr),
            input: task_input,
            stop: task_stop,
            dropped: task_dropped,
            service_stop,
            original,
            limits,
            permit: task_permit,
            unconfirmed: None,
        }
        .retire(),
    );
    let owner = Arc::new(AcpProcessOwner {
        state: Mutex::new(OwnerState::Joining(task)),
        permit,
        input: Arc::clone(&input),
        stop: stop.clone(),
    });
    let transport = AcpProcess {
        incoming: Some(Incoming {
            input,
            dropped,
            stop: stop.clone(),
            sdk: true,
        }),
        outgoing: Some(Outgoing {
            pipe: outgoing,
            written: 0,
            maximum: limits.stdin_bytes,
            stop,
        }),
        owner: Arc::clone(&owner),
        completed: false,
    };
    Ok((transport, owner))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn wire_limits() -> CodexWireLimits {
        CodexWireLimits {
            frame_bytes: 4096,
            depth: 16,
            nodes: 128,
            fields: 64,
            array_items: 64,
            string_bytes: 4096,
            decoded_string_bytes: 4096,
            id_bytes: 256,
        }
    }
    fn process_limits() -> Limits {
        Limits {
            stdin_bytes: 4096,
            stdout_bytes: 8192,
            stderr_bytes: 1024,
            termination_grace: Duration::from_millis(40),
            cleanup: Duration::from_secs(2),
        }
    }
    fn program(root: &Path, body: &str) -> io::Result<std::path::PathBuf> {
        let python = which::which("python3").map_err(io::Error::other)?;
        let program = root.join("opencode");
        std::fs::write(
            &program,
            format!(
                "#!{}\nimport os,sys\nassert sys.argv[1:]==['acp']\n{body}\n",
                python.display()
            ),
        )?;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))?;
        Ok(program)
    }
    #[tokio::test]
    async fn sdk_foreground_completion_preserves_native_eof_and_normal_exit() {
        let temp = tempfile::tempdir().unwrap();
        let program = program(
            temp.path(),
            "import time\nwith open('ready', 'w') as marker: marker.write('ready')\nsys.stdin.buffer.read()\nwith open('stdin.eof', 'w') as marker: marker.write('EOF')\ntime.sleep(0.05)",
        )
        .unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        ));
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let mut limits = process_limits();
        // Separate normal EOF retirement from the faulty immediate TERM path.
        limits.termination_grace = Duration::from_secs(1);
        let (transport, owner) = spawn(AcpLaunch {
            program: &program,
            cwd: temp.path(),
            environment: &[],
            original,
            service_stop: Cancellation::default(),
            limits,
            wire: wire_limits(),
            permit: slots.clone().acquire_owned().await.unwrap(),
        })
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !temp.path().join("ready").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(slots.available_permits(), 0);
        Client
            .builder()
            .connect_with(transport, async |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(slots.available_permits(), 0);
        assert!(owner.observe(Instant::now() + Duration::from_secs(2)).await);
        assert!(owner.clean().await);
        let observed = owner.public_observation().await;
        assert_eq!(observed["exit_code"], 0);
        assert_eq!(observed["stdout_eof"], true);
        assert_eq!(observed["stderr_eof"], true);
        assert_eq!(
            std::fs::read(temp.path().join("stdin.eof")).unwrap(),
            b"EOF"
        );
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test]
    async fn sdk_owner_retires_real_descendant_pipes_after_leader_exit_cancel_deadline_and_service_stop()
     {
        for mode in ["leader_exit", "original_cancel", "deadline", "service_stop"] {
            let temp = tempfile::tempdir().unwrap();
            let leader = if mode == "leader_exit" {
                "while not os.path.exists('release.leader'): time.sleep(0.005)\nos._exit(7)"
            } else {
                "signal.pause()"
            };
            let body = format!(
                r#"import signal,time
pid=os.fork()
if pid==0:
 def term(signum,frame):
  with open('descendant.term','w') as marker: marker.write('TERM')
 signal.signal(signal.SIGTERM,term)
 with open('descendant.pid','w') as marker: marker.write(str(os.getpid()))
 while True:
  with open('heartbeat','ab') as beat: beat.write(b'.')
  time.sleep(0.01)
def term(signum,frame):
 with open('leader.term','w') as marker: marker.write('TERM')
 os._exit(31)
signal.signal(signal.SIGTERM,term)
while not os.path.exists('descendant.pid'): time.sleep(0.005)
with open('leader.ready','w') as marker: marker.write('ready')
{leader}"#
            );
            let program = program(temp.path(), &body).unwrap();
            let cancel = Cancellation::default();
            let service_stop = Cancellation::default();
            let foreground = Cancellation::default();
            let original = Arc::new(SnapshotContext::new(
                Instant::now()
                    + if mode == "deadline" {
                        Duration::from_secs(1)
                    } else {
                        Duration::from_secs(6)
                    },
                cancel.clone(),
            ));
            let slots = Arc::new(tokio::sync::Semaphore::new(1));
            let mut limits = process_limits();
            limits.termination_grace = Duration::from_millis(150);
            let (transport, owner) = spawn(AcpLaunch {
                program: &program,
                cwd: temp.path(),
                environment: &[],
                original: Arc::clone(&original),
                service_stop: service_stop.clone(),
                limits,
                wire: wire_limits(),
                permit: slots.clone().acquire_owned().await.unwrap(),
            })
            .unwrap();
            let connection = Client.builder().connect_with(transport, async |_| {
                foreground.cancelled().await;
                Ok(())
            });
            let exercise = async {
                tokio::time::timeout(Duration::from_secs(2), async {
                    while !temp.path().join("leader.ready").exists()
                        || !std::fs::metadata(temp.path().join("heartbeat"))
                            .is_ok_and(|m| m.len() > 0)
                    {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                assert!(
                    tokio::time::timeout(
                        Duration::from_millis(20),
                        owner.observe(Instant::now() + Duration::from_secs(4))
                    )
                    .await
                    .is_err()
                );
                assert!(matches!(*owner.state.lock().await, OwnerState::Joining(_)));
                assert_eq!(slots.available_permits(), 0);
                assert!(!temp.path().join("descendant.term").exists());
                assert!(!temp.path().join("leader.term").exists());
                match mode {
                    "leader_exit" => {
                        std::fs::write(temp.path().join("release.leader"), b"exit").unwrap()
                    }
                    "original_cancel" => cancel.cancel(),
                    "deadline" => {
                        tokio::time::sleep_until(original.required_deadline().unwrap().into()).await
                    }
                    "service_stop" => service_stop.cancel(),
                    _ => unreachable!(),
                }
                tokio::time::timeout(Duration::from_secs(2), async {
                    while !temp.path().join("descendant.term").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                assert_eq!(slots.available_permits(), 0);
                foreground.cancel();
                assert!(
                    owner.observe(Instant::now() + Duration::from_secs(3)).await,
                    "{mode}"
                );
                let observed = owner.public_observation().await;
                assert_eq!(observed["joined"], true);
                assert_eq!(observed["group_kill_completed"], true);
                assert_eq!(observed["stdout_eof"], true);
                assert_eq!(observed["stderr_eof"], true);
                assert_eq!(
                    observed["exit_code"],
                    if mode == "leader_exit" { 7 } else { 31 }
                );
                assert_eq!(slots.available_permits(), 1);
                let descendant = std::fs::read_to_string(temp.path().join("descendant.pid"))
                    .unwrap()
                    .parse::<u32>()
                    .unwrap();
                match std::fs::read_to_string(format!("/proc/{descendant}/stat")) {
                    Ok(stat) => {
                        assert_eq!(stat.rsplit_once(") ").unwrap().1.chars().next(), Some('Z'))
                    }
                    // procfs can report ESRCH after opening a now-dead process.
                    Err(error) => assert!(
                        error.kind() == io::ErrorKind::NotFound
                            || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()),
                        "{mode}: {error:?}"
                    ),
                }
                let heartbeat = std::fs::read(temp.path().join("heartbeat")).unwrap();
                tokio::time::sleep(Duration::from_millis(60)).await;
                assert_eq!(
                    std::fs::read(temp.path().join("heartbeat")).unwrap(),
                    heartbeat
                );
                assert!(
                    !owner.clean().await,
                    "a nonzero native exit never becomes an answer"
                );
            };
            let (connection_result, ()) = tokio::time::timeout(Duration::from_secs(8), async {
                tokio::join!(connection, exercise)
            })
            .await
            .unwrap();
            assert!(
                connection_result.is_ok(),
                "SDK foreground must finish successfully"
            );
            assert!(matches!(*owner.state.lock().await, OwnerState::Joined(_)));
        }
    }

    #[tokio::test]
    async fn native_frames_are_audited_before_delivery_and_keep_private_lexemes() {
        let temp = tempfile::tempdir().unwrap();
        let mut frame = br#"{"extension":1.2300e+02,"literal":"\u0061"}"#.to_vec();
        frame.push(b'\n');
        let program = program(temp.path(), &format!("os.write(1,bytes({frame:?}))")).unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        ));
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let (mut transport, owner) = spawn(AcpLaunch {
            program: &program,
            cwd: temp.path(),
            environment: &[],
            original,
            service_stop: Cancellation::default(),
            limits: process_limits(),
            wire: wire_limits(),
            permit: slots.clone().acquire_owned().await.unwrap(),
        })
        .unwrap();
        let mut output = Vec::new();
        transport
            .incoming
            .as_mut()
            .unwrap()
            .read_to_end(&mut output)
            .await
            .unwrap();
        assert_eq!(output, frame);
        assert_eq!(owner.input.lock().unwrap().native, frame);
        assert_eq!(slots.available_permits(), 0);
        drop(transport);
        assert!(owner.observe(Instant::now() + Duration::from_secs(2)).await);
        assert!(owner.clean().await);
        assert_eq!(slots.available_permits(), 1);
    }
    #[tokio::test]
    async fn abandoned_observer_keeps_the_same_task_and_slot_until_owned_retirement() {
        let temp = tempfile::tempdir().unwrap();
        let program = program(temp.path(), "os.read(0,1)").unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        ));
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let (transport, owner) = spawn(AcpLaunch {
            program: &program,
            cwd: temp.path(),
            environment: &[],
            original,
            service_stop: Cancellation::default(),
            limits: process_limits(),
            wire: wire_limits(),
            permit: slots.clone().acquire_owned().await.unwrap(),
        })
        .unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                owner.observe(Instant::now() + Duration::from_secs(4))
            )
            .await
            .is_err()
        );
        assert!(matches!(*owner.state.lock().await, OwnerState::Joining(_)));
        assert_eq!(slots.available_permits(), 0);
        drop(transport);
        assert!(owner.observe(Instant::now() + Duration::from_secs(2)).await);
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test]
    async fn real_original_cancel_deadline_and_service_stop_keep_custody_until_stream_drop_and_join()
     {
        for mode in ["original_cancel", "deadline", "service_stop"] {
            let temp = tempfile::tempdir().unwrap();
            let program = program(
                temp.path(),
                "import signal\ndef term(signum,frame):\n with open('term.observed','w') as marker: marker.write('TERM')\n os._exit(31)\nsignal.signal(signal.SIGTERM,term)\nos.write(1,b'{}\\n')\nsignal.pause()",
            )
            .unwrap();
            let cancel = Cancellation::default();
            let stop = Cancellation::default();
            let original = Arc::new(SnapshotContext::new(
                Instant::now()
                    + if mode == "deadline" {
                        Duration::from_secs(1)
                    } else {
                        Duration::from_secs(5)
                    },
                cancel.clone(),
            ));
            let slots = Arc::new(tokio::sync::Semaphore::new(1));
            let (mut transport, owner) = spawn(AcpLaunch {
                program: &program,
                cwd: temp.path(),
                environment: &[],
                original: Arc::clone(&original),
                service_stop: stop.clone(),
                limits: process_limits(),
                wire: wire_limits(),
                permit: slots.clone().acquire_owned().await.unwrap(),
            })
            .unwrap();
            let mut frame = [0; 3];
            tokio::time::timeout(
                Duration::from_secs(2),
                transport.incoming.as_mut().unwrap().read_exact(&mut frame),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(&frame, b"{}\n");
            match mode {
                "original_cancel" => cancel.cancel(),
                "service_stop" => stop.cancel(),
                "deadline" => {
                    tokio::time::sleep_until(original.required_deadline().unwrap().into()).await
                }
                _ => unreachable!(),
            }
            if mode != "service_stop" {
                assert!(original.check().is_err());
            }
            // Prove this trigger delivered TERM while SDK custody is still retained.
            // Dropping the transport also requests stop, so it cannot be the oracle.
            tokio::time::timeout(Duration::from_secs(2), async {
                while !std::fs::read(temp.path().join("term.observed"))
                    .is_ok_and(|bytes| bytes == b"TERM")
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(
                !owner
                    .observe(Instant::now() + Duration::from_millis(20))
                    .await,
                "a retained SDK input must keep native retirement pending"
            );
            assert_eq!(slots.available_permits(), 0);
            drop(transport);
            assert!(
                owner.observe(Instant::now() + Duration::from_secs(2)).await,
                "{mode}"
            );
            assert_eq!(slots.available_permits(), 1);
            let observed = owner.public_observation().await;
            assert_eq!(observed["group_absent"], true);
            assert_eq!(observed["stdout_eof"], true);
            assert_eq!(observed["stderr_eof"], true);
            assert_eq!(observed["status_success"], false);
            assert_eq!(observed["exit_code"], 31);
        }
    }
    #[tokio::test]
    async fn native_frame_limit_stops_delivery_and_retains_first_error_through_real_retirement() {
        let temp = tempfile::tempdir().unwrap();
        let program=program(temp.path(),"import signal\nos.write(1,b'{\"extension\":\"over the native frame limit\"}\\n')\nsignal.pause()").unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        ));
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let mut wire = wire_limits();
        wire.frame_bytes = 16;
        let (mut transport, owner) = spawn(AcpLaunch {
            program: &program,
            cwd: temp.path(),
            environment: &[],
            original,
            service_stop: Cancellation::default(),
            limits: process_limits(),
            wire,
            permit: slots.clone().acquire_owned().await.unwrap(),
        })
        .unwrap();
        let mut output = Vec::new();
        let failure = tokio::time::timeout(
            Duration::from_secs(2),
            transport
                .incoming
                .as_mut()
                .unwrap()
                .read_to_end(&mut output),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::FileTooLarge);
        assert!(
            output.is_empty(),
            "an unaudited frame must not reach the SDK"
        );
        assert_eq!(
            owner.input.lock().unwrap().error,
            Some(io::ErrorKind::FileTooLarge)
        );
        assert_eq!(slots.available_permits(), 0);
        drop(transport);
        assert!(owner.observe(Instant::now() + Duration::from_secs(2)).await);
        assert!(!owner.clean().await);
        assert_eq!(
            owner.input.lock().unwrap().error,
            Some(io::ErrorKind::FileTooLarge)
        );
        assert_eq!(slots.available_permits(), 1);
    }
}
