//! The actual Host-owned Kimi child. No polling, discovery or detached supervisor.
use super::{KimiEndpoint, KimiReason};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    Chunk, Cleanup, PipeEnd, ProcessHost, Request, Retention, ServiceFailure, ServiceOutcome,
    ServiceSink, ServiceStop, SinkFailure, Stream, run_service, spawn_service,
};
use secrecy::zeroize::{Zeroize, Zeroizing};
use std::{fmt, sync::Arc, time::Instant};
use tokio::{
    sync::{Mutex, watch},
    task::JoinHandle,
};

pub struct KimiServer {
    stop: Option<Arc<ServiceStop>>,
    ready: watch::Receiver<Option<Result<Arc<KimiEndpoint>, KimiReason>>>,
    state: Mutex<State>,
}
enum State {
    Running(JoinHandle<Result<ServiceOutcome, ServiceFailure>>),
    Finished(Result<ServiceOutcome, ServiceFailure>),
    JoinFailed(tokio::task::JoinError),
}
#[derive(Clone, Copy, Debug)]
pub struct KimiServerObservation {
    pub joined: bool,
    pub clean: bool,
    pub status_code: Option<i32>,
    pub cleanup: Option<Cleanup>,
    pub stdout_eof: bool,
    pub stderr_eof: bool,
    pub driver_cancelled: Option<bool>,
    pub process_reason: Option<graphit_process::Reason>,
}
impl KimiServerObservation {
    fn pending() -> Self {
        Self {
            joined: false,
            clean: false,
            status_code: None,
            cleanup: None,
            stdout_eof: false,
            stderr_eof: false,
            driver_cancelled: None,
            process_reason: None,
        }
    }
}

impl KimiServer {
    pub fn stop(&self) {
        if let Some(stop) = &self.stop {
            stop.stop();
        }
    }
    pub async fn ready(&self, original: &SnapshotContext) -> Result<Arc<KimiEndpoint>, KimiReason> {
        let deadline = original
            .required_deadline()
            .map_err(|error| KimiReason::Context(error.kind()))?;
        let mut receiver = self.ready.clone();
        loop {
            original
                .check()
                .map_err(|error| KimiReason::Context(error.kind()))?;
            if let Some(value) = receiver.borrow_and_update().clone() {
                return value;
            }
            tokio::select! {
                biased;
                _ = original.cancellation().cancelled() => return Err(KimiReason::Context(std::io::ErrorKind::Interrupted)),
                _ = tokio::time::sleep_until(deadline.into()) => return Err(KimiReason::Context(std::io::ErrorKind::TimedOut)),
                result = receiver.changed() => if result.is_err() { return Err(KimiReason::Startup); },
            }
        }
    }
    /// A cancelled observer keeps the same handle and remaining custody in this owner.
    pub async fn observe(&self, deadline: Instant) -> KimiServerObservation {
        let Ok(mut state) = tokio::time::timeout_at(deadline.into(), self.state.lock()).await
        else {
            // A contended observer has no new status/EOF/join observation. Custody stays here.
            return KimiServerObservation::pending();
        };
        if let State::Running(job) = &mut *state
            && let Ok(result) = tokio::time::timeout_at(deadline.into(), job).await
        {
            *state = match result {
                Ok(result) => State::Finished(result),
                Err(error) => State::JoinFailed(error),
            };
        }
        if let State::Finished(result) = &mut *state {
            let owner = match result {
                Ok(outcome) => &mut outcome.remaining,
                Err(failure) => &mut failure.remaining,
            };
            if let Some(owner) = owner
                && let graphit_process::RepairObservation::Observed { cleanup, status } =
                    owner.repair(deadline).await
            {
                match result {
                    Ok(outcome) => {
                        outcome.cleanup = cleanup;
                        if let Some(status) = status {
                            outcome.status = status;
                        }
                        outcome.remaining = None;
                    }
                    Err(failure) => {
                        failure.cleanup = cleanup;
                        failure.status = status;
                        failure.remaining = None;
                    }
                }
            }
        }
        observation(&state)
    }
}
impl Drop for KimiServer {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            stop.stop();
        }
        if let State::Running(job) = self.state.get_mut() {
            job.abort();
        }
    }
}
impl fmt::Debug for KimiServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KimiServer")
    }
}
fn observation(state: &State) -> KimiServerObservation {
    let result = match state {
        State::Running(_) => {
            return KimiServerObservation::pending();
        }
        State::JoinFailed(error) => {
            return KimiServerObservation {
                joined: false,
                clean: false,
                status_code: None,
                cleanup: None,
                stdout_eof: false,
                stderr_eof: false,
                driver_cancelled: Some(error.is_cancelled()),
                process_reason: None,
            };
        }
        State::Finished(result) => result,
    };
    let (status, cleanup, pipes, remaining) = match result {
        Ok(outcome) => (
            Some(outcome.status),
            outcome.cleanup,
            outcome.pipes,
            outcome.remaining.is_some(),
        ),
        Err(failure) => (
            failure.status,
            failure.cleanup,
            failure.pipes,
            failure.remaining.is_some(),
        ),
    };
    let leader_reaped = status.is_some() && cleanup.leader_reaped_without_control_failure();
    let joined = !remaining
        && match cleanup {
            Cleanup::TimedOut => false,
            Cleanup::Unconfirmed(_) => leader_reaped,
            _ => true,
        };
    KimiServerObservation {
        joined,
        clean: result.is_ok()
            && joined
            && leader_reaped
            && status.is_some_and(|status| status.success())
            && pipes.stdout == PipeEnd::Eof
            && pipes.stderr == PipeEnd::Eof,
        status_code: status.and_then(|status| status.code()),
        cleanup: Some(cleanup),
        process_reason: result.as_ref().err().map(|failure| failure.reason),
        stdout_eof: pipes.stdout == PipeEnd::Eof,
        stderr_eof: pipes.stderr == PipeEnd::Eof,
        driver_cancelled: None,
    }
}
struct Startup {
    buffer: Zeroizing<Vec<u8>>,
    maximum: usize,
    published: bool,
    ready: watch::Sender<Option<Result<Arc<KimiEndpoint>, KimiReason>>>,
}
impl ServiceSink for Startup {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        _: &'a Cancellation,
    ) -> Result<(), SinkFailure> {
        if chunk.stream() != Stream::Stdout || self.published {
            return Ok(());
        }
        for byte in chunk.bytes() {
            if self.buffer.len() >= self.maximum {
                self.ready.send_replace(Some(Err(KimiReason::Limit)));
                return Err(SinkFailure::ConsumerFailed);
            }
            if self.buffer.try_reserve(1).is_err() {
                self.ready.send_replace(Some(Err(KimiReason::Allocation)));
                return Err(SinkFailure::ConsumerFailed);
            }
            self.buffer.push(*byte);
            if *byte == b'\n' {
                if self.buffer.starts_with(b"Kimi server: ") {
                    let endpoint =
                        KimiEndpoint::from_ready_line(&self.buffer, self.maximum).map(Arc::new);
                    let valid = endpoint.is_ok();
                    self.ready.send_replace(Some(endpoint));
                    self.buffer.zeroize();
                    self.published = true;
                    return if valid {
                        Ok(())
                    } else {
                        Err(SinkFailure::ConsumerFailed)
                    };
                }
                self.buffer.zeroize();
            }
        }
        Ok(())
    }
}
/// The caller registers this owner before awaiting readiness or any network effect.
pub async fn spawn_kimi_server(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    startup_line_bytes: usize,
) -> Arc<KimiServer> {
    let process = match spawn_service(request, original, host).await {
        Ok(process) => process,
        Err(failure) => {
            let (_, ready) = watch::channel(Some(Err(KimiReason::Startup)));
            return Arc::new(KimiServer {
                stop: None,
                ready,
                state: Mutex::new(State::Finished(Err(failure))),
            });
        }
    };
    let stop = Arc::new(process.stop_handle());
    let task_stop = Arc::clone(&stop);
    let context = Arc::clone(original);
    let (ready, receiver) = watch::channel(None);
    let job = tokio::spawn(async move {
        let mut sink = Startup {
            buffer: Zeroizing::new(Vec::new()),
            maximum: startup_line_bytes,
            published: false,
            ready,
        };
        let result = {
            let execution = run_service(process, Retention::None, &mut sink);
            tokio::pin!(execution);
            tokio::select! {
                biased;
                result = &mut execution => result,
                _ = context.cancellation().cancelled() => { task_stop.stop(); execution.await },
                _ = async { if let Ok(deadline) = context.required_deadline() { tokio::time::sleep_until(deadline.into()).await } } => { task_stop.stop(); execution.await },
            }
        };
        let unpublished = sink.ready.borrow().is_none();
        if unpublished {
            sink.ready.send_replace(Some(Err(KimiReason::Startup)));
        }
        result
    });
    Arc::new(KimiServer {
        stop: Some(stop),
        ready: receiver,
        state: Mutex::new(State::Running(job)),
    })
}
