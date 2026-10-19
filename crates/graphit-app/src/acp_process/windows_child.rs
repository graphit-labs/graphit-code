//! Concrete duplex ACP custody using the same pinned native producer as ProcessHost.
use graphit_process::{Reason, SpawnFailure, SpawnPhase, Stage, spawn_owned};
use process_wrap::tokio::{ChildWrapper, CommandWrap, JobObject, KillOnDrop};
use std::{io, path::Path, process::ExitStatus};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};

pub(crate) struct Child {
    native: Box<dyn ChildWrapper>,
    job_owned: bool,
    tree_joined: bool,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}
pub(crate) struct Spawn {
    pub child: Child,
    pub failure: Option<Reason>,
}
pub(crate) fn reason_error(reason: Reason) -> io::Error {
    match reason {
        Reason::Io(_, failure) => failure.os_code().map_or_else(
            || io::Error::from(failure.kind()),
            io::Error::from_raw_os_error,
        ),
        _ => io::Error::other(format!("{reason:?}")),
    }
}
pub(crate) fn spawn(command: Command) -> Result<Spawn, Box<SpawnFailure>> {
    // Native Windows EXE resolution accepts an omitted suffix. Explicit batch/script
    // extensions are rejected before spawn; the ACP transport never adds a shell.
    if Path::new(command.as_std().get_program())
        .extension()
        .is_some_and(|extension| !extension.as_encoded_bytes().eq_ignore_ascii_case(b"exe"))
    {
        return Err(Box::new(SpawnFailure {
            reason: Reason::Io(
                Stage::Spawn,
                io::Error::from(io::ErrorKind::InvalidInput).into(),
            ),
            phase: SpawnPhase::BeforeNative,
            child: None,
        }));
    }
    let mut command = CommandWrap::from(command);
    command.wrap(KillOnDrop).wrap(JobObject);
    let (mut native, failure, job_owned) = match spawn_owned(&mut command) {
        Ok(native) => (native, None, true),
        Err(mut failure) => {
            let Some(child) = failure.child.take() else {
                return Err(failure);
            };
            // The producer recovered the actual raw child, not a completed Job guard.
            (
                Box::new(child) as Box<dyn ChildWrapper>,
                Some(failure.reason),
                false,
            )
        }
    };
    let stdin = native.stdin().take();
    let stdout = native.stdout().take();
    let stderr = native.stderr().take();
    Ok(Spawn {
        child: Child {
            native,
            job_owned,
            tree_joined: false,
            stdin,
            stdout,
            stderr,
        },
        failure,
    })
}
impl Child {
    pub(crate) async fn wait_leader(&mut self) -> io::Result<ExitStatus> {
        // Cancelling this native wait never cancels the SDK's whole-Job completion wait.
        // spawn_owned constructs only this native Tokio chain, optionally wrapped
        // by the pinned Job Object and recovery guards. Borrow its terminal layer
        // through the SDK's safe API without consuming or replacing any guard.
        let mut leader = self.native.as_mut();
        while std::any::Any::type_id(&*leader) != std::any::TypeId::of::<tokio::process::Child>() {
            leader = leader.inner_mut();
        }
        leader.wait().await
    }
    pub(crate) fn tree_joined(&self) -> bool {
        self.tree_joined
    }
    pub(crate) async fn terminate_and_join(
        &mut self,
        signal_error: &mut Option<io::Error>,
    ) -> io::Result<ExitStatus> {
        // Terminate the owned Job even after its leader exited; never target another PID.
        if let Err(error) = self.native.start_kill() {
            signal_error.get_or_insert(error);
        }
        // Await exactly once inside the retained task: SDK leader caching does not prove
        // descendant completion after cancellation or a failed completion-port wait.
        let joined = self.native.wait().await;
        self.tree_joined = self.job_owned && joined.is_ok();
        joined
    }
}
