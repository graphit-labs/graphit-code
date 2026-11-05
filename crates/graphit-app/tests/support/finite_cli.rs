//! Finite CLI captures through the real Process host, for explicitly cleared environments.
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_process::{
    HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, IgnoreChunks, Limits,
    ProcessHost, Request, Retention, operation, run,
};
use std::{
    io,
    process::Command,
    process::Output,
    sync::Arc,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(8);

pub(super) fn request(command: &Command) -> io::Result<Request> {
    Ok(Request {
        program: command.get_program().into(),
        args: command.get_args().map(Into::into).collect(),
        cwd: command
            .get_current_dir()
            .ok_or(io::ErrorKind::InvalidInput)?
            .into(),
        environment: command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key.into(), value.into())))
            .collect(),
        stdin: None,
        limits: Limits {
            stdin_bytes: 0,
            stdout_bytes: 1024 * 1024,
            stderr_bytes: 1024 * 1024,
            termination_grace: Duration::from_millis(20),
            cleanup: Duration::from_secs(2),
        },
    })
}

pub(super) fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

pub(super) async fn host(slots: usize) -> io::Result<ProcessHost> {
    let host = ProcessHost::install(&Cancellation::default(), HostLimits::new(slots, 1)?)?;
    assert!(matches!(
        host.ready(Instant::now() + WAIT).await,
        HostReadiness::Ready
    ));
    Ok(host)
}

pub(super) async fn capture(host: &ProcessHost, request: Request) -> io::Result<Output> {
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + WAIT,
        Cancellation::default(),
    ));
    let (stop, receiver) = operation(&original, host)
        .map_err(|error| io::Error::other(format!("CLI admission {error:?}")))?;
    let result = run(
        request,
        &original,
        Retention::Full,
        &mut IgnoreChunks,
        receiver,
    )
    .await;
    drop(stop);
    let result = result.map_err(|error| io::Error::other(format!("CLI supervision {error:?}")))?;
    let transcript = result.transcript.expect("full CLI capture");
    Ok(Output {
        status: result.status,
        stdout: transcript.stdout().to_vec(),
        stderr: transcript.stderr().to_vec(),
    })
}

pub(super) async fn joined(host: ProcessHost) -> io::Result<()> {
    let report = match host.shutdown(Instant::now() + WAIT).await {
        HostShutdown::Joined(report) => report,
        HostShutdown::Pending(mut same) => match same.repair(Instant::now() + WAIT).await {
            HostShutdownObservation::Joined(report) => report,
            HostShutdownObservation::Pending => return Err(io::ErrorKind::TimedOut.into()),
        },
    };
    match report.failure() {
        None => Ok(()),
        Some(error) => Err(io::Error::other(format!("CLI host {error:?}"))),
    }
}

pub trait FiniteOutput {
    /// The command factory must clear its environment and use finite, null stdin.
    fn finite_output(&self) -> io::Result<Output>;
}

impl FiniteOutput for Command {
    fn finite_output(&self) -> io::Result<Output> {
        let request = request(self)?;
        runtime()?.block_on(async {
            let host = host(1).await?;
            let result = capture(&host, request).await;
            joined(host).await?;
            result
        })
    }
}
