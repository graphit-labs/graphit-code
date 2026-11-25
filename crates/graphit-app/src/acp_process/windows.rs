//! Windows ACP custody using the existing Process spawn boundary and pinned Job Object SDK.
use super::windows_child::{self, Child, reason_error as spawn_error};
use super::*;
use graphit_process::{Reason, SpawnPhase};

pub(super) struct Retained {
    child: Option<Child>,
    spawn_failure: Option<Box<graphit_process::SpawnFailure>>,
    phase: Option<SpawnPhase>,
    reason: Option<Reason>,
    status: Option<ExitStatus>,
    wait_error: Option<io::ErrorKind>,
    signal_error: Option<io::ErrorKind>,
}
impl Retained {
    pub(super) fn observation(&self) -> serde_json::Value {
        serde_json::json!({"joined":false,"native_owner_retained":self.child.is_some() || self.spawn_failure.as_ref().is_some_and(|failure|failure.child.is_some()),"tree_unconfirmed":true,"spawn_phase":self.phase.map(|phase|match phase {SpawnPhase::BeforeNative=>"before_native",SpawnPhase::NativeAttempted=>"native_attempted",SpawnPhase::ChildObserved=>"child_observed"}),"spawn_cause":self.reason.map(|reason|format!("{reason:?}")),"exit_code":self.status.and_then(|status|status.code()),"wait_error":self.wait_error.map(|kind|format!("{kind:?}")),"signal_error":self.signal_error.map(|kind|format!("{kind:?}"))})
    }
}
pub(super) fn spawn(
    command: Command,
    permit: OwnedSemaphorePermit,
    original: &Arc<SnapshotContext>,
    limits: Limits,
    wire: &CodexWireLimits,
) -> Result<(Child, OwnedSemaphorePermit), SpawnFailure> {
    match windows_child::spawn(command) {
        Ok(windows_child::Spawn {
            child,
            failure: None,
        }) => Ok((child, permit)),
        Ok(windows_child::Spawn {
            mut child,
            failure: Some(reason),
        }) => {
            let error = spawn_error(reason);
            child.stdin.take();
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let mut input = Input::new(stdout, Arc::clone(original), *wire, limits.stdout_bytes);
            input.eof = input.pipe.is_none();
            input.error = Some(error.kind());
            let input = Arc::new(StdMutex::new(input));
            let permit = Arc::new(StdMutex::new(Some(permit)));
            let stop = Cancellation::default();
            stop.cancel();
            // The helper preserves a RAW recovered child. Its real wait and
            // pipe drain run in the existing ACP owner, without claiming Job ownership.
            let state = OwnerState::Joining(tokio::spawn(
                Run {
                    child,
                    stderr,
                    input: Arc::clone(&input),
                    stop: stop.clone(),
                    dropped: Arc::new(AtomicBool::new(true)),
                    service_stop: stop.clone(),
                    original: Arc::clone(original),
                    limits,
                    permit: Arc::clone(&permit),
                    unconfirmed: Some((SpawnPhase::ChildObserved, reason)),
                }
                .retire(),
            ));
            let owner = Arc::new(AcpProcessOwner {
                state: Mutex::new(state),
                permit,
                input,
                stop,
            });
            Err(SpawnFailure {
                error,
                owner: Some(owner),
            })
        }
        Err(failure) => {
            let error = spawn_error(failure.reason);
            if matches!(failure.phase, SpawnPhase::BeforeNative) && failure.child.is_none() {
                return Err(error.into());
            }
            // NativeAttempted without a handle is unknown birth, not NotSpawned.
            // Keep the original failure and admission in the caller's registered owner.
            let mut input = Input::new(None, Arc::clone(original), *wire, limits.stdout_bytes);
            input.eof = true;
            input.error = Some(error.kind());
            let input = Arc::new(StdMutex::new(input));
            let permit = Arc::new(StdMutex::new(Some(permit)));
            let stop = Cancellation::default();
            stop.cancel();
            let state = OwnerState::Retained(Retained {
                child: None,
                phase: Some(failure.phase),
                reason: Some(failure.reason),
                spawn_failure: Some(failure),
                status: None,
                wait_error: None,
                signal_error: None,
            });
            let owner = Arc::new(AcpProcessOwner {
                state: Mutex::new(state),
                permit,
                input,
                stop,
            });
            Err(SpawnFailure {
                error,
                owner: Some(owner),
            })
        }
    }
}
pub(super) struct Run {
    pub child: Child,
    pub stderr: Option<ChildStderr>,
    pub input: Arc<StdMutex<Input>>,
    pub stop: Cancellation,
    pub dropped: Arc<AtomicBool>,
    pub service_stop: Cancellation,
    pub original: Arc<SnapshotContext>,
    pub limits: Limits,
    pub permit: Arc<StdMutex<Option<OwnedSemaphorePermit>>>,
    pub unconfirmed: Option<(SpawnPhase, Reason)>,
}
impl Run {
    pub(super) async fn retire(self) -> Retirement {
        let Self {
            mut child,
            mut stderr,
            input,
            stop,
            dropped,
            service_stop,
            original,
            limits,
            permit,
            unconfirmed,
        } = self;
        let _retained_permit = permit;
        let mut incoming = Incoming {
            input: Arc::clone(&input),
            dropped: Arc::clone(&dropped),
            stop: stop.clone(),
            sdk: false,
        };
        let mut stderr_bytes = 0usize;
        let mut stderr_eof = false;
        let mut stderr_error = None;
        let mut bytes = [0; 4096];
        let mut drain = [0; 4096];
        let mut eof_requested = None;
        // Observe only the native leader here. A cancelled leader wait cannot
        // start, cancel or recreate the SDK's whole-Job completion wait.
        let first = loop {
            let sdk_dropped = dropped.load(Ordering::Acquire);
            if sdk_dropped && eof_requested.is_none() {
                eof_requested = Some(Instant::now());
            }
            let closed = input
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .closed();
            if stop.is_cancelled()
                || service_stop.is_cancelled()
                || original.check().is_err()
                || eof_requested.is_some_and(|start| start.elapsed() >= limits.termination_grace)
            {
                break None;
            }
            tokio::select! {
                result=child.wait_leader()=>break Some(result),
                result=async {stderr.as_mut().expect("retained stderr").read(&mut bytes).await},if stderr.is_some()=>match result {
                    Ok(0)=>{stderr_eof=true;stderr=None;},
                    Ok(count)=>{stderr_bytes=stderr_bytes.saturating_add(count);if stderr_bytes>limits.stderr_bytes {stderr_error.get_or_insert(io::ErrorKind::FileTooLarge);stop.cancel();}},
                    Err(error)=>{stderr_error.get_or_insert(error.kind());stderr=None;stop.cancel();},
                },
                result=incoming.read(&mut drain),if sdk_dropped&&!closed=>{if result.is_err(){stop.cancel();}},
                ()=stop.cancelled()=>break None,
                ()=service_stop.cancelled()=>break None,
                ()=original.cancellation().cancelled()=>break None,
                ()=tokio::time::sleep(Duration::from_millis(10))=>{},
            }
        };
        // Windows has no Unix TERM phase. Terminate the owned Job, even after
        // the leader exits; for a failed birth this controls only its RAW child.
        let mut signal_error = None;
        let prior_wait_error = first
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .map(io::Error::kind);
        let waited = {
            // Create this SDK wait exactly once and retain it across every pipe
            // drain. Only the enclosing owner task awaits it; observer timeouts
            // borrow that task's handle and cannot destroy this native wait.
            let native_wait = child.terminate_and_join(&mut signal_error);
            tokio::pin!(native_wait);
            let mut result = None;
            loop {
                let sdk_dropped = dropped.load(Ordering::Acquire);
                let closed = input
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .closed();
                if sdk_dropped
                    && closed
                    && stderr.is_none()
                    && let Some(observed) = result.take()
                {
                    break observed;
                }
                tokio::select! {
                    observed=&mut native_wait,if result.is_none()=>result=Some(observed),
                    observed=async {stderr.as_mut().expect("retained stderr").read(&mut bytes).await},if stderr.is_some()=>match observed {
                        Ok(0)=>{stderr_eof=true;stderr=None;},
                        Ok(count)=>{stderr_bytes=stderr_bytes.saturating_add(count);if stderr_bytes>limits.stderr_bytes {stderr_error.get_or_insert(io::ErrorKind::FileTooLarge);}},
                        Err(error)=>{stderr_error.get_or_insert(error.kind());stderr=None;},
                    },
                    observed=incoming.read(&mut drain),if sdk_dropped&&!closed=>{if observed.is_err(){stop.cancel();}},
                    ()=tokio::time::sleep(Duration::from_millis(10))=>{},
                }
            }
        };
        let signal_error = signal_error.map(|error| error.kind());
        let wait_error = prior_wait_error.or_else(|| waited.as_ref().err().map(io::Error::kind));
        let status = waited.ok();
        if unconfirmed.is_some() || status.is_none() || !child.tree_joined() {
            return Retirement::Retained(Retained {
                child: Some(child),
                spawn_failure: None,
                phase: unconfirmed.map(|value| value.0),
                reason: unconfirmed.map(|value| value.1),
                status,
                wait_error,
                signal_error,
            });
        }
        Retirement::Joined(Observation {
            status: status.expect("successful whole-Job wait"),
            group_kill_completed: signal_error.is_none(),
            wait_error,
            signal_error,
            stderr_bytes,
            stderr_eof,
            stderr_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    fn limits() -> Limits {
        Limits {
            stdin_bytes: 4096,
            stdout_bytes: 8192,
            stderr_bytes: 4096,
            termination_grace: Duration::from_millis(150),
            cleanup: Duration::from_secs(5),
        }
    }
    fn wire() -> CodexWireLimits {
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
    fn program(root: &Path, mut process: serde_json::Value) -> std::path::PathBuf {
        let program = root.join("opencode.exe");
        process["args"] = serde_json::json!(["acp"]);
        crate::test_agent_fixture::install(
            &program,
            serde_json::json!({"kind":"raw","process":process}),
        );
        program
    }
    fn original(seconds: u64, cancel: Cancellation) -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(seconds),
            cancel,
        ))
    }
    async fn ready(path: &Path) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !path.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn assert_retirement_pending(owner: &AcpProcessOwner) {
        {
            let observation = owner.observe(Instant::now() + Duration::from_secs(4));
            tokio::pin!(observation);
            assert!(futures_util::poll!(observation.as_mut()).is_pending());
        }
        assert!(matches!(*owner.state.lock().await, OwnerState::Joining(_)));
    }
    #[tokio::test]
    async fn duplex_frame_bytes_and_normal_eof_join_the_owned_job() {
        let root = tempfile::tempdir().unwrap();
        let frame = br#"{"extension":1.2300e+02,"literal":"\u0061"}"#;
        let program = program(
            root.path(),
            serde_json::json!({"expect_stdin":"{\"ask\":\"duplex\"}\n","stdout":"{\"extension\":1.2300e+02,\"literal\":\"\\u0061\"}\n","wait_stdin_eof":true,"stderr":"closed"}),
        );
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let environment: Vec<_> = std::env::vars_os().collect();
        let (mut transport, owner) = super::super::spawn(AcpLaunch {
            program: &program,
            cwd: root.path(),
            environment: &environment,
            original: original(15, Cancellation::default()),
            service_stop: Cancellation::default(),
            limits: limits(),
            wire: wire(),
            permit: slots.clone().acquire_owned().await.unwrap(),
        })
        .unwrap();
        transport
            .outgoing
            .as_mut()
            .unwrap()
            .write_all(b"{\"ask\":\"duplex\"}\n")
            .await
            .unwrap();
        let mut received = vec![0; frame.len() + 1];
        transport
            .incoming
            .as_mut()
            .unwrap()
            .read_exact(&mut received)
            .await
            .unwrap();
        assert_eq!(&received[..frame.len()], frame);
        assert_eq!(received[frame.len()], b'\n');
        drop(transport.outgoing.take());
        assert_eq!(
            transport
                .incoming
                .as_mut()
                .unwrap()
                .read(&mut [0; 1])
                .await
                .unwrap(),
            0
        );
        transport.completed = true;
        drop(transport);
        assert!(owner.observe(Instant::now() + Duration::from_secs(5)).await);
        assert!(owner.clean().await);
        let observation = owner.public_observation().await;
        assert_eq!(observation["job_wait_completed"], true);
        assert_eq!(observation["stdout_eof"], true);
        assert_eq!(observation["stderr_eof"], true);
        assert_eq!(slots.available_permits(), 1);
    }
    #[tokio::test]
    async fn descendant_custody_survives_leader_exit_original_cancel_deadline_and_service_stop() {
        for mode in ["leader_exit", "original_cancel", "deadline", "service_stop"] {
            let root = tempfile::tempdir().unwrap();
            let mut process = serde_json::json!({"ready":"leader.ready","child":{"pid":"descendant.pid","hold":true}});
            if mode == "leader_exit" {
                process["release"] = "release.leader".into();
                process["exit"] = 7.into();
            } else {
                process["hold"] = true.into();
            }
            let program = program(root.path(), process);
            let cancel = Cancellation::default();
            let service_stop = Cancellation::default();
            let original = original(if mode == "deadline" { 8 } else { 20 }, cancel.clone());
            let slots = Arc::new(tokio::sync::Semaphore::new(1));
            let environment: Vec<_> = std::env::vars_os().collect();
            let (transport, owner) = super::super::spawn(AcpLaunch {
                program: &program,
                cwd: root.path(),
                environment: &environment,
                original: Arc::clone(&original),
                service_stop: service_stop.clone(),
                limits: limits(),
                wire: wire(),
                permit: slots.clone().acquire_owned().await.unwrap(),
            })
            .unwrap();
            ready(&root.path().join("leader.ready")).await;
            assert_retirement_pending(&owner).await;
            assert_eq!(slots.available_permits(), 0);
            match mode {
                "leader_exit" => {
                    std::fs::write(root.path().join("release.leader"), b"exit").unwrap()
                }
                "original_cancel" => cancel.cancel(),
                "deadline" => {
                    tokio::time::sleep_until(original.required_deadline().unwrap().into()).await
                }
                "service_stop" => service_stop.cancel(),
                _ => unreachable!(),
            }
            // Retain the SDK stream until its actual drop, even after native cancellation.
            assert!(
                !owner
                    .observe(Instant::now() + Duration::from_millis(30))
                    .await
            );
            assert_eq!(slots.available_permits(), 0);
            drop(transport);
            assert!(
                owner.observe(Instant::now() + Duration::from_secs(8)).await,
                "{mode}"
            );
            let observation = owner.public_observation().await;
            assert_eq!(observation["job_wait_completed"], true);
            assert_eq!(observation["stdout_eof"], true);
            assert_eq!(observation["stderr_eof"], true);
            assert_eq!(slots.available_permits(), 1);
            assert!(!owner.clean().await);
        }
    }
    #[tokio::test]
    async fn batch_shell_is_rejected_before_native_birth_and_returns_the_slot() {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("untrusted.cmd");
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let result = super::super::spawn(AcpLaunch {
            program: &program,
            cwd: root.path(),
            environment: &[],
            original: original(5, Cancellation::default()),
            service_stop: Cancellation::default(),
            limits: limits(),
            wire: wire(),
            permit: slots.clone().acquire_owned().await.unwrap(),
        });
        let failure = result.err().expect("batch execution must be rejected");
        assert_eq!(failure.error.kind(), io::ErrorKind::InvalidInput);
        assert!(failure.owner.is_none());
        assert_eq!(slots.available_permits(), 1);
    }
}
