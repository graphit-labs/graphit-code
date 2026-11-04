//! Ask-owned native child custody around the mature ACP byte-stream transport.
#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectTo};
use futures_util::io::{AsyncRead, AsyncWrite};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::Limits;
#[cfg(unix)]
use std::time::Duration;
use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    pin::Pin,
    process::{ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::{Mutex, OwnedSemaphorePermit},
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[cfg(unix)]
type OwnedChild = tokio::process::Child;
#[cfg(windows)]
use crate::acp_process::windows_child as windows;
#[cfg(windows)]
type OwnedChild = windows::Child;

async fn wait_leader(child: &mut OwnedChild) -> io::Result<ExitStatus> {
    #[cfg(unix)]
    {
        child.wait().await
    }
    #[cfg(windows)]
    {
        child.wait_leader().await
    }
}

pub(crate) struct ChildObservation {
    pub status: Option<ExitStatus>,
    pub wait_error: Option<io::Error>,
    pub signal_error: Option<io::Error>,
    pub stderr_bytes: usize,
    pub stderr_eof: bool,
    pub stderr_error: Option<io::Error>,
}
struct ChildReturn {
    child: Option<OwnedChild>,
    observation: ChildObservation,
    // A failed wait must not return its admission before the retained child joins.
    permit: Option<OwnedSemaphorePermit>,
}
enum OwnerState {
    Idle,
    Joining(JoinHandle<ChildReturn>),
    Retained(Box<ChildReturn>),
    #[cfg(windows)]
    BirthUnobserved {
        _permit: OwnedSemaphorePermit,
        reason: graphit_process::Reason,
    },
    Joined(ChildObservation),
    JoinFailed(tokio::task::JoinError),
}
pub(crate) struct KiroProcessOwner {
    stop: Cancellation,
    spawned: AtomicBool,
    state: Mutex<OwnerState>,
    custody: Option<Arc<crate::knowledge_conversation::ConversationCustodyEntry>>,
}
impl KiroProcessOwner {
    pub(crate) fn new(
        custody: Option<Arc<crate::knowledge_conversation::ConversationCustodyEntry>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            stop: Cancellation::default(),
            spawned: AtomicBool::new(false),
            state: Mutex::new(OwnerState::Idle),
            custody,
        })
    }
    pub(crate) fn spawned(&self) -> bool {
        self.spawned.load(Ordering::Acquire)
    }
    pub(crate) fn stop(&self) {
        self.stop.cancel();
    }
    /// Borrow the actual join. A cancelled observer leaves it in this owner.
    pub(crate) async fn observe(&self, deadline: Instant) -> bool {
        if !self.spawned() {
            return self.stop.is_cancelled();
        }
        let Ok(mut state) = tokio::time::timeout_at(deadline.into(), self.state.lock()).await
        else {
            return false;
        };
        if let OwnerState::Joining(join) = &mut *state
            && let Ok(result) = tokio::time::timeout_at(deadline.into(), join).await
        {
            *state = match result {
                Ok(result) if result.child.is_none() => OwnerState::Joined(result.observation),
                Ok(result) => OwnerState::Retained(Box::new(result)),
                Err(error) => OwnerState::JoinFailed(error),
            };
        }
        // A Windows Job wait error cannot be repaired by the SDK's cached leader status.
        // Keep that native owner and admission retained rather than publish a false join.
        #[cfg(unix)]
        if let OwnerState::Retained(retained) = &mut *state {
            if let Some(child) = retained.child.as_mut()
                && let Ok(Ok(status)) = tokio::time::timeout_at(deadline.into(), child.wait()).await
            {
                retained.observation.status = Some(status);
                retained.child = None;
                retained.permit = None;
            }
            if retained.child.is_none() {
                let OwnerState::Retained(result) = std::mem::replace(&mut *state, OwnerState::Idle)
                else {
                    unreachable!()
                };
                *state = OwnerState::Joined(result.observation);
            }
        }
        let joined = matches!(*state, OwnerState::Joined(_));
        if joined && let Some(custody) = &self.custody {
            custody.observed_terminal();
        }
        joined
    }
    pub(crate) async fn public_observation(&self) -> serde_json::Value {
        let Ok(state) = self.state.try_lock() else {
            return serde_json::json!({"joined":false,"observation_pending":true});
        };
        match &*state {
            OwnerState::Joined(o) => {
                serde_json::json!({"joined":true,"exit_code":o.status.and_then(|s|s.code()),"status_success":o.status.map(|s|s.success()),"wait_error":o.wait_error.as_ref().map(|e|format!("{:?}",e.kind())),"signal_error":o.signal_error.as_ref().map(|e|format!("{:?}",e.kind())),"stderr_bytes":o.stderr_bytes,"stderr_eof":o.stderr_eof,"stderr_error":o.stderr_error.as_ref().map(|e|format!("{:?}",e.kind()))})
            }
            OwnerState::Retained(retained) => {
                let o = &retained.observation;
                serde_json::json!({"joined":false,"native_owner_retained":retained.child.is_some(),"admission_retained":retained.permit.is_some(),"leader_exit_code":o.status.and_then(|status|status.code()),"wait_error":o.wait_error.as_ref().map(|e|format!("{:?}",e.kind())),"signal_error":o.signal_error.as_ref().map(|e|format!("{:?}",e.kind())),"stderr_bytes":o.stderr_bytes,"stderr_eof":o.stderr_eof,"stderr_error":o.stderr_error.as_ref().map(|e|format!("{:?}",e.kind()))})
            }
            #[cfg(windows)]
            OwnerState::BirthUnobserved { reason, .. } => {
                serde_json::json!({"joined":false,"birth_unobserved":true,"spawn_error":format!("{reason:?}")})
            }
            OwnerState::JoinFailed(e) => {
                serde_json::json!({"joined":false,"join_error":{"panic":e.is_panic(),"cancelled":e.is_cancelled()}})
            }
            _ => serde_json::json!({"joined":false,"spawned":self.spawned()}),
        }
    }
    pub(crate) async fn clean_joined(&self) -> bool {
        self.state.try_lock().is_ok_and(|state|matches!(&*state,OwnerState::Joined(o) if o.status.is_some() && o.wait_error.is_none() && o.signal_error.is_none() && o.stderr_eof && o.stderr_error.is_none()))
    }
    fn dropped_transport(&self) {
        self.stop();
        if !self.spawned()
            && let Some(custody) = &self.custody
        {
            custody.release_unsubmitted();
        }
    }
}

pub(crate) struct KiroProcess {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
    pub original: Arc<SnapshotContext>,
    pub service_stop: Cancellation,
    pub limits: Limits,
    pub frame_bytes: usize,
    pub permit: Option<OwnedSemaphorePermit>,
    pub owner: Arc<KiroProcessOwner>,
}
impl Drop for KiroProcess {
    fn drop(&mut self) {
        self.owner.dropped_transport();
    }
}
impl ConnectTo<Client> for KiroProcess {
    async fn connect_to(
        mut self,
        client: impl ConnectTo<Agent>,
    ) -> Result<(), agent_client_protocol::Error> {
        let deadline = self
            .original
            .required_deadline()
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        let permit = self.permit.take().expect("admitted native execution slot");
        self.original
            .check()
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        if self.service_stop.is_cancelled() {
            return Err(agent_client_protocol::Error::into_internal_error(
                io::Error::from(io::ErrorKind::Interrupted),
            ));
        }
        let mut command = Command::new(&self.program);
        // This child is explicitly made its group leader. We never signal the parent's group.
        command
            .args(&self.args)
            .current_dir(&self.cwd)
            .env_clear()
            .envs(self.environment.iter().cloned())
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        let mut child = command
            .process_group(0)
            .spawn()
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        #[cfg(windows)]
        let windows::Spawn {
            mut child,
            failure: spawn_failure,
        } = match windows::spawn(command) {
            Ok(spawned) => spawned,
            Err(failure) => {
                let error = windows::reason_error(failure.reason);
                if !matches!(failure.phase, graphit_process::SpawnPhase::BeforeNative) {
                    *self.owner.state.try_lock().expect("unpolled child owner") =
                        OwnerState::BirthUnobserved {
                            _permit: permit,
                            reason: failure.reason,
                        };
                    self.owner.spawned.store(true, Ordering::Release);
                }
                return Err(agent_client_protocol::Error::into_internal_error(error));
            }
        };
        #[cfg(windows)]
        if spawn_failure.is_some() {
            self.owner.stop();
        }
        #[cfg(unix)]
        let pid = child
            .id()
            .and_then(|p| i32::try_from(p).ok())
            .and_then(rustix::process::Pid::from_raw);
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let stop = self.owner.stop.clone();
        let service_stop = self.service_stop.clone();
        let original = Arc::clone(&self.original);
        let limits = self.limits;
        let stderr_join = tokio::spawn(async move {
            let mut bytes = 0usize;
            let mut buffer = [0; 4096];
            loop {
                let result = stderr.read(&mut buffer).await;
                match result {
                    Ok(0) => return (bytes, true, None),
                    Ok(n) => {
                        bytes = bytes.saturating_add(n);
                        if bytes > limits.stderr_bytes {
                            stop.cancel();
                            return (
                                bytes,
                                false,
                                Some(io::Error::from(io::ErrorKind::FileTooLarge)),
                            );
                        }
                    }
                    Err(error) => {
                        stop.cancel();
                        return (bytes, false, Some(error));
                    }
                }
            }
        });
        let stop = self.owner.stop.clone();
        let child_join = tokio::spawn(async move {
            let mut signal_error = None;
            let first = tokio::select! {
                result=wait_leader(&mut child)=>Some(result),
                ()=stop.cancelled()=>None,
                ()=service_stop.cancelled()=>None,
                ()=original.cancellation().cancelled()=>None,
                ()=tokio::time::sleep_until(deadline.into())=>None,
            };
            #[cfg(unix)]
            let waited = terminate_group(
                &mut child,
                pid,
                first,
                limits.termination_grace,
                &mut signal_error,
            )
            .await;
            #[cfg(unix)]
            let initial_wait_error = None;
            #[cfg(windows)]
            let initial_wait_error = first.and_then(Result::err);
            #[cfg(windows)]
            let waited = child.terminate_and_join(&mut signal_error).await;
            // Keep the real drain owned until EOF or an observed IO/size failure.
            let (stderr_bytes, stderr_eof, stderr_error) = match stderr_join.await {
                Ok(result) => result,
                Err(error) => (0, false, Some(io::Error::other(error))),
            };
            #[cfg(unix)]
            let tree_joined = waited.is_ok();
            #[cfg(windows)]
            let tree_joined = child.tree_joined();
            let (status, wait_error, child, permit) = match waited {
                Ok(status) if tree_joined => (Some(status), initial_wait_error, None, None),
                Ok(status) => (Some(status), initial_wait_error, Some(child), Some(permit)),
                Err(error) => {
                    #[cfg(windows)]
                    let error = initial_wait_error.unwrap_or(error);
                    (None, Some(error), Some(child), Some(permit))
                }
            };
            ChildReturn {
                child,
                permit,
                observation: ChildObservation {
                    status,
                    wait_error,
                    signal_error,
                    stderr_bytes,
                    stderr_eof,
                    stderr_error,
                },
            }
        });
        // No await separates spawn, service ownership and registration of the real join.
        *self.owner.state.try_lock().expect("unpolled child owner") =
            OwnerState::Joining(child_join);
        self.owner.spawned.store(true, Ordering::Release);
        #[cfg(windows)]
        if let Some(error) = spawn_failure {
            return Err(agent_client_protocol::Error::into_internal_error(
                windows::reason_error(error),
            ));
        }
        let outgoing = BoundedWrite {
            inner: stdin.compat_write(),
            remaining: self.limits.stdin_bytes,
        };
        let incoming = BoundedRead {
            inner: stdout.compat(),
            remaining: self.limits.stdout_bytes,
            frame: 0,
            frame_limit: self.frame_bytes,
        };
        ConnectTo::<Client>::connect_to(ByteStreams::new(outgoing, incoming), client).await
    }
}
// The leader may already have exited while its owned group still uses the workspace.
// As in ProcessHost, grace belongs to the group, followed by KILL regardless of
// cached leader status. This observes native signaling and the same child's wait;
// it does not claim to reap descendants owned by another parent.
#[cfg(unix)]
async fn terminate_group(
    child: &mut OwnedChild,
    pid: Option<rustix::process::Pid>,
    first: Option<io::Result<ExitStatus>>,
    grace: Duration,
    signal_error: &mut Option<io::Error>,
) -> io::Result<ExitStatus> {
    if let Some(pid) = pid {
        // The final group signal below establishes the outcome even if
        // the graceful signal raced with the leader's exit.
        if rustix::process::kill_process_group(pid, rustix::process::Signal::TERM).is_ok() {
            tokio::time::sleep(grace).await;
        }
        let killed = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        // Match ProcessHost: a Darwin group can temporarily report EPERM
        // while its leader exits. Reap and retry without accepting a
        // persistent permission failure as a successful termination.
        #[cfg(target_os = "macos")]
        let killed = {
            let mut result = killed;
            let until = Instant::now() + Duration::from_millis(25);
            while result == Err(rustix::io::Errno::PERM) && Instant::now() < until {
                if child.try_wait().is_err() {
                    break;
                }
                tokio::time::sleep_until(
                    until.min(Instant::now() + Duration::from_millis(1)).into(),
                )
                .await;
                result = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            result
        };
        if let Err(error) = killed
            && error != rustix::io::Errno::SRCH
        {
            signal_error.get_or_insert(error.into());
        }
    } else {
        signal_error.get_or_insert_with(|| io::ErrorKind::InvalidData.into());
        if let Err(error) = child.start_kill() {
            signal_error.get_or_insert(error);
        }
    }
    match first {
        Some(result) => result,
        None => child.wait().await,
    }
}
struct BoundedRead<R> {
    inner: R,
    remaining: usize,
    frame: usize,
    frame_limit: usize,
}
impl<R: AsyncRead + Unpin> AsyncRead for BoundedRead<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let size = buffer.len().min(self.remaining.saturating_add(1));
        let n = match Pin::new(&mut self.inner).poll_read(cx, &mut buffer[..size]) {
            Poll::Ready(Ok(n)) => n,
            other => return other,
        };
        if n > self.remaining {
            return Poll::Ready(Err(io::ErrorKind::FileTooLarge.into()));
        }
        for b in &buffer[..n] {
            if *b == b'\n' {
                self.frame = 0;
            } else {
                self.frame = self.frame.saturating_add(1);
                if self.frame > self.frame_limit {
                    return Poll::Ready(Err(io::ErrorKind::FileTooLarge.into()));
                }
            }
        }
        self.remaining -= n;
        Poll::Ready(Ok(n))
    }
}
struct BoundedWrite<W> {
    inner: W,
    remaining: usize,
}
impl<W: AsyncWrite + Unpin> AsyncWrite for BoundedWrite<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if bytes.len() > self.remaining {
            return Poll::Ready(Err(io::ErrorKind::FileTooLarge.into()));
        }
        let n = match Pin::new(&mut self.inner).poll_write(cx, bytes) {
            Poll::Ready(Ok(n)) => n,
            other => return other,
        };
        self.remaining -= n;
        Poll::Ready(Ok(n))
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::io::{AsyncReadExt, AsyncWriteExt, Cursor};

    #[cfg(target_os = "linux")]
    struct GroupCleanup(rustix::process::Pid);
    #[cfg(target_os = "linux")]
    impl Drop for GroupCleanup {
        fn drop(&mut self) {
            let _ = rustix::process::kill_process_group(self.0, rustix::process::Signal::KILL);
        }
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn kiro_exited_leader_still_terminates_term_ignoring_descendant() {
        owned_group_cleanup(false).await;
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn kiro_term_leader_exit_still_terminates_term_ignoring_descendant() {
        owned_group_cleanup(true).await;
    }
    #[cfg(target_os = "linux")]
    async fn owned_group_cleanup(cancel: bool) {
        let temp = tempfile::tempdir().unwrap();
        let ready = temp.path().join("descendant");
        let leader_ready = temp.path().join("leader-ready");
        let end = if cancel { "wait; exit 8" } else { "exit 7" };
        let script = format!(
            r#"trap 'exit 7' TERM; /bin/sh -c 'trap "" TERM; echo $$ > "$1"; exec /bin/sleep 30' child "$1" & while ! test -s "$1"; do /bin/sleep 0.01; done; printf "owned fixture stderr\n" >&2; printf ready > "$2"; {end}"#
        );
        let mut child = Command::new("/bin/sh")
            .args(["-c", &script, "leader"])
            .arg(&ready)
            .arg(&leader_ready)
            .process_group(0)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid =
            rustix::process::Pid::from_raw(i32::try_from(child.id().unwrap()).unwrap()).unwrap();
        let _cleanup = GroupCleanup(pid);
        let mut stderr = child.stderr.take().unwrap();
        let drain = tokio::spawn(async move {
            let mut output = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut output)
                .await
                .unwrap();
            output
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !std::fs::metadata(&leader_ready).is_ok_and(|metadata| metadata.len() != 0) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let descendant = std::fs::read_to_string(&ready)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        let first = if cancel {
            None
        } else {
            Some(child.wait().await)
        };
        let mut signal_error = None;
        let status = tokio::time::timeout(
            Duration::from_secs(5),
            terminate_group(
                &mut child,
                Some(pid),
                first,
                Duration::from_millis(40),
                &mut signal_error,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let output = tokio::time::timeout(Duration::from_secs(5), drain)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.code(), Some(7));
        assert!(signal_error.is_none(), "{signal_error:?}");
        assert_eq!(output, b"owned fixture stderr\n");
        match std::fs::read_to_string(format!("/proc/{descendant}/stat")) {
            Ok(stat) => assert_eq!(stat.rsplit_once(") ").unwrap().1.chars().next(), Some('Z')),
            Err(error) => assert_eq!(error.kind(), io::ErrorKind::NotFound),
        }
        let owner = KiroProcessOwner::new(None);
        *owner.state.lock().await = OwnerState::Joined(ChildObservation {
            status: Some(status),
            wait_error: None,
            signal_error: None,
            stderr_bytes: 0,
            stderr_eof: false,
            stderr_error: None,
        });
        assert!(
            !owner.clean_joined().await,
            "leader status without EOF is not clean cleanup"
        );
        if let OwnerState::Joined(observation) = &mut *owner.state.lock().await {
            observation.stderr_eof = true;
        }
        assert!(owner.clean_joined().await);
    }
    #[tokio::test]
    async fn kiro_transport_byte_limits_reject_actual_excess_and_preserve_frame_boundaries() {
        let mut input = BoundedRead {
            inner: Cursor::new(b"ab\ncd\nX".to_vec()),
            remaining: 6,
            frame: 0,
            frame_limit: 2,
        };
        let mut buffer = [0; 3];
        assert_eq!(input.read(&mut buffer).await.unwrap(), 3);
        assert_eq!(&buffer, b"ab\n");
        assert_eq!(input.read(&mut buffer).await.unwrap(), 3);
        assert_eq!(&buffer, b"cd\n");
        assert_eq!(
            input.read(&mut buffer).await.unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
        let mut frame = BoundedRead {
            inner: Cursor::new(b"abcd\n".to_vec()),
            remaining: 10,
            frame: 0,
            frame_limit: 3,
        };
        assert_eq!(frame.read(&mut buffer).await.unwrap(), 3);
        assert_eq!(
            frame.read(&mut buffer).await.unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
        let mut output = BoundedWrite {
            inner: Cursor::new(Vec::new()),
            remaining: 4,
        };
        output.write_all(b"abc").await.unwrap();
        assert_eq!(
            output.write_all(b"de").await.unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(output.inner.get_ref(), b"abc");
        output.write_all(b"d").await.unwrap();
        assert_eq!(output.inner.get_ref(), b"abcd");
        assert_eq!(output.remaining, 0);
    }
}
