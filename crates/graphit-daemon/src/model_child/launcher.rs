use super::Readiness;
use crate::model_service::{ModelErrorKind, ModelResult, ModelServiceClient};
use graphit_core::{
    artifact_store::SnapshotContext, file_lock::Cancellation, runtime_key::RuntimeKeyOwner,
};
use graphit_process::{
    Chunk, Limits, ProcessHost, Request, Retention, ServiceFailure, ServiceOutcome, ServiceSink,
    ServiceStop, SinkFailure, Stream, run_service, spawn_service,
};
use std::{error::Error, ffi::OsString, fmt, io, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{
    net::TcpStream,
    sync::oneshot,
    task::{JoinError, JoinHandle},
};
use zeroize::Zeroizing;

pub type ChildProcessResult = Result<Result<ServiceOutcome, ServiceFailure>, JoinError>;
/// No inferred executable, inherited environment or shell interpolation.
pub struct ChildLaunch {
    pub executable: PathBuf,
    pub installation_root: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
    pub limits: Limits,
}
pub struct ChildStartError {
    pub cause: io::Error,
    /// Preserves real cleanup/remaining ownership after any spawned child.
    pub process: Option<Box<ChildProcessResult>>,
}
impl fmt::Debug for ChildStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildStartError")
            .field("kind", &self.cause.kind())
            .field("process", &self.process)
            .finish()
    }
}
impl fmt::Display for ChildStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "model child startup failed: {:?}", self.cause.kind())?;
        if let Some(diagnostic) = self
            .cause
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<StartupDiagnostic>())
        {
            write!(f, " ({diagnostic})")?;
        }
        if let Some(process) = self.process.as_deref() {
            let status = match process {
                Ok(Ok(outcome)) => Some(&outcome.status),
                Ok(Err(failure)) => failure.status.as_ref(),
                Err(_) => None,
            };
            if let Some(status) = status {
                write!(f, " (child {status})")?;
            }
        }
        Ok(())
    }
}
impl Error for ChildStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
impl From<io::Error> for ChildStartError {
    fn from(cause: io::Error) -> Self {
        Self {
            cause,
            process: None,
        }
    }
}
pub struct ChildShutdown {
    pub drain: ModelResult<()>,
    pub process: ChildProcessResult,
}
/// Drop requests native stop. Explicit shutdown additionally waits for the runner.
pub struct ModelChild {
    address: SocketAddr,
    stop: ServiceStop,
    runner: JoinHandle<Result<ServiceOutcome, ServiceFailure>>,
    completed: Option<ChildProcessResult>,
}
impl ModelChild {
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub async fn start(
        launch: ChildLaunch,
        runtime: &RuntimeKeyOwner,
        original: &Arc<SnapshotContext>,
        host: &ProcessHost,
    ) -> Result<Self, ChildStartError> {
        let startup_deadline = original.required_deadline()?;
        if !launch.installation_root.is_absolute() || launch.limits.stdin_bytes < 64 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let borrow = runtime.check(original).map_err(io::Error::other)?;
        let stdin = Zeroizing::new(borrow.secret(original).map_err(io::Error::other)?.to_vec());
        let stderr_limit = launch.limits.stderr_bytes;
        let environment = launch.environment;
        let request = Request {
            program: launch.executable,
            args: vec![launch.installation_root.clone().into_os_string()],
            cwd: launch.installation_root,
            environment,
            stdin: Some(stdin),
            limits: launch.limits,
        };
        let process = spawn_service(request, original, host)
            .await
            .map_err(|failure| ChildStartError {
                cause: io::ErrorKind::Other.into(),
                process: Some(Box::new(Ok(Err(failure)))),
            })?;
        let stop = process.stop_handle();
        let (send, receive) = oneshot::channel();
        let runner = tokio::spawn(async move {
            let mut sink = ReadySink {
                bytes: Vec::new(),
                ready: Some(send),
                ready_accepted: false,
                stderr: Zeroizing::new(Vec::new()),
                stderr_limit,
                stderr_truncated: false,
                diagnostics: tokio::io::stderr(),
            };
            run_service(process, Retention::None, &mut sink).await
        });
        let readiness = tokio::select! {
            biased;
            _ = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
            _ = tokio::time::sleep_until(startup_deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
            ready = receive => ready.unwrap_or_else(|_| Err(io::ErrorKind::UnexpectedEof.into())),
        };
        let address = match readiness.and_then(|address| {
            original.check()?;
            Ok(address)
        }) {
            Ok(address) => address,
            Err(cause) => {
                stop.stop();
                return Err(ChildStartError {
                    cause,
                    process: Some(Box::new(runner.await)),
                });
            }
        };
        Ok(Self {
            address,
            stop,
            runner,
            completed: None,
        })
    }
    /// Observe the existing runner once; dropping this waiter never detaches
    /// ownership or requests stop. Repeated observations use the same outcome.
    pub async fn completed(&mut self) -> &ChildProcessResult {
        if self.completed.is_none() {
            self.completed = Some((&mut self.runner).await);
        }
        self.completed
            .as_ref()
            .expect("observed model child runner")
    }

    /// Transfer the actual runner/cleanup result, without sending an RPC to a
    /// process that has already exited.
    pub async fn join(mut self) -> ChildProcessResult {
        self.completed().await;
        self.completed.take().expect("observed model child runner")
    }

    /// Drain success is separate from the SDK's typed stop/cleanup observation.
    pub async fn shutdown(
        self,
        client: &ModelServiceClient,
        runtime: &RuntimeKeyOwner,
        original: &SnapshotContext,
    ) -> ChildShutdown {
        let drain = client
            .shutdown(runtime, original, TcpStream::connect(self.address))
            .await;
        self.stop.stop();
        let process = self.join().await;
        ChildShutdown { drain, process }
    }
}

struct ReadySink<W = tokio::io::Stderr> {
    bytes: Vec<u8>,
    ready: Option<oneshot::Sender<io::Result<SocketAddr>>>,
    ready_accepted: bool,
    stderr: Zeroizing<Vec<u8>>,
    stderr_limit: usize,
    stderr_truncated: bool,
    diagnostics: W,
}
impl<W> ReadySink<W> {
    fn publish_readiness(&mut self) {
        let result = serde_json::from_slice::<Readiness>(&self.bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            .and_then(|ready| {
                if !ready.address.ip().is_loopback() || ready.address.port() == 0 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Ok(ready.address)
            });
        let valid = result.is_ok();
        self.bytes.clear();
        self.stderr = Zeroizing::new(Vec::new());
        if let Some(send) = self.ready.take() {
            let accepted = send.send(result).is_ok();
            self.ready_accepted = valid && accepted;
        }
    }
    fn capture_stderr(&mut self, bytes: &[u8]) {
        if self.ready.is_some() {
            let remaining = self.stderr_limit.saturating_sub(self.stderr.len());
            let retained = remaining.min(bytes.len());
            self.stderr.extend_from_slice(&bytes[..retained]);
            self.stderr_truncated |= retained != bytes.len();
        }
    }
}

/// Retain bounded private diagnostics; only typed categories can reach logs.
struct StartupDiagnostic {
    stderr: Zeroizing<Vec<u8>>,
    truncated: bool,
}
impl StartupDiagnostic {
    fn line(&self) -> Option<&str> {
        if self.truncated {
            return None;
        }
        let line = std::str::from_utf8(&self.stderr).ok()?.strip_suffix('\n')?;
        if line.contains(['\n', '\r']) {
            return None;
        }
        line.strip_prefix("model child startup/service failed: ")
    }
    fn model_kind(&self) -> Option<ModelErrorKind> {
        self.io_kind()?;
        let (_, model) = self.line()?.split_once("; model=")?;
        serde_json::from_str(model).ok()
    }
    fn io_kind(&self) -> Option<io::ErrorKind> {
        let kind = self.line()?.split("; model=").next()?;
        Some(match kind {
            "Other" => io::ErrorKind::Other,
            "NotFound" => io::ErrorKind::NotFound,
            "PermissionDenied" => io::ErrorKind::PermissionDenied,
            "InvalidInput" => io::ErrorKind::InvalidInput,
            "InvalidData" => io::ErrorKind::InvalidData,
            "Interrupted" => io::ErrorKind::Interrupted,
            "TimedOut" => io::ErrorKind::TimedOut,
            "AddrInUse" => io::ErrorKind::AddrInUse,
            "AddrNotAvailable" => io::ErrorKind::AddrNotAvailable,
            "AlreadyExists" => io::ErrorKind::AlreadyExists,
            "NotADirectory" => io::ErrorKind::NotADirectory,
            "OutOfMemory" => io::ErrorKind::OutOfMemory,
            "FileTooLarge" => io::ErrorKind::FileTooLarge,
            "Unsupported" => io::ErrorKind::Unsupported,
            _ => return None,
        })
    }
}

impl fmt::Debug for StartupDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StartupDiagnostic")
            .field("io_kind", &self.io_kind())
            .field("model_kind", &self.model_kind())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for StartupDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("readiness unavailable")?;
        if let Some(kind) = self.io_kind() {
            write!(f, "; child_io={kind:?}")?;
        }
        if let Some(kind) = self.model_kind() {
            write!(f, "; child_model={kind:?}")?;
        }
        Ok(())
    }
}
impl Error for StartupDiagnostic {}
impl<W> Drop for ReadySink<W> {
    fn drop(&mut self) {
        if let Some(send) = self.ready.take() {
            let diagnostic = StartupDiagnostic {
                stderr: std::mem::replace(&mut self.stderr, Zeroizing::new(Vec::new())),
                truncated: self.stderr_truncated,
            };
            let _ = send.send(Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                diagnostic,
            )));
        }
    }
}
impl<W: tokio::io::AsyncWrite + Unpin + Send> ReadySink<W> {
    async fn accept_stderr(&mut self, bytes: &[u8]) -> Result<(), SinkFailure> {
        if !self.ready_accepted {
            self.capture_stderr(bytes);
            Ok(())
        } else {
            tokio::io::AsyncWriteExt::write_all(&mut self.diagnostics, bytes)
                .await
                .map_err(|_| SinkFailure::ConsumerFailed)?;
            tokio::io::AsyncWriteExt::flush(&mut self.diagnostics)
                .await
                .map_err(|_| SinkFailure::ConsumerFailed)
        }
    }
}
impl<W: tokio::io::AsyncWrite + Unpin + Send> ServiceSink for ReadySink<W> {
    async fn push<'a>(
        &'a mut self,
        chunk: Chunk<'a>,
        _: &'a Cancellation,
    ) -> Result<(), SinkFailure> {
        if chunk.stream() == Stream::Stderr {
            return self.accept_stderr(chunk.bytes()).await;
        }
        for byte in chunk.bytes() {
            if self.ready.is_none() {
                return Err(SinkFailure::ConsumerFailed);
            }
            if self.bytes.len() >= 512 {
                if let Some(send) = self.ready.take() {
                    let _ = send.send(Err(io::ErrorKind::InvalidData.into()));
                }
                return Err(SinkFailure::ConsumerFailed);
            }
            if *byte == b'\n' {
                self.publish_readiness();
            } else {
                self.bytes.push(*byte);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    #[tokio::test]
    async fn failed_readiness_preserves_typed_cause_without_printing_stderr() {
        let (send, receive) = oneshot::channel();
        let sink = ReadySink {
            bytes: Vec::new(),
            ready: Some(send),
            ready_accepted: false,
            stderr_limit: 4096,
            stderr_truncated: false,
            stderr: Zeroizing::new(
                b"model child startup/service failed: Other; model=\"Deadline\"\n".to_vec(),
            ),
            diagnostics: Vec::<u8>::new(),
        };
        drop(sink);
        let cause = receive.await.unwrap().unwrap_err();
        assert_eq!(cause.kind(), io::ErrorKind::UnexpectedEof);
        let error = ChildStartError::from(cause);
        assert!(
            error
                .to_string()
                .contains("child_io=Other; child_model=Deadline")
        );
    }

    #[tokio::test]
    async fn stderr_overflow_after_a_complete_line_cannot_classify_failure() {
        let line = b"model child startup/service failed: Other; model=\"Native\"\n";
        let (send, receive) = oneshot::channel();
        let mut sink = ReadySink {
            bytes: Vec::new(),
            ready: Some(send),
            ready_accepted: false,
            stderr: Zeroizing::new(Vec::new()),
            stderr_limit: line.len(),
            stderr_truncated: false,
            diagnostics: Vec::<u8>::new(),
        };
        sink.capture_stderr(line);
        sink.capture_stderr(b"private-bootstrap-material");
        assert_eq!(sink.stderr.len(), line.len());
        drop(sink);
        let error = ChildStartError::from(receive.await.unwrap().unwrap_err());
        assert!(!error.to_string().contains("child_model="));
        assert!(!error.to_string().contains("private-bootstrap-material"));
    }

    #[tokio::test]
    async fn post_ready_stderr_is_forwarded_without_retaining_bootstrap() {
        let (send, receive) = oneshot::channel();
        let mut sink = ReadySink {
            bytes: Vec::new(),
            ready: Some(send),
            ready_accepted: false,
            stderr: Zeroizing::new(Vec::new()),
            stderr_limit: 4096,
            stderr_truncated: false,
            diagnostics: Vec::<u8>::new(),
        };
        sink.accept_stderr(b"private-bootstrap-material")
            .await
            .unwrap();
        assert!(sink.diagnostics.is_empty());
        sink.bytes
            .extend_from_slice(br#"{"address":"127.0.0.1:12345"}"#);
        sink.publish_readiness();
        assert!(sink.ready_accepted);
        assert!(receive.await.unwrap().is_ok());
        for part in [
            b"model service deadline: ".as_slice(),
            b"operation=embedding phase=admission\n",
            &[0xff, 0],
        ] {
            sink.accept_stderr(part).await.unwrap();
        }
        assert_eq!(
            sink.diagnostics,
            b"model service deadline: operation=embedding phase=admission\n\xff\0"
        );
        assert!(sink.stderr.is_empty());
    }

    #[tokio::test]
    async fn invalid_or_unaccepted_readiness_never_forwards_later_stderr() {
        for readiness in [br#"{}"#.as_slice(), br#"{"address":"192.0.2.1:12345"}"#] {
            let (send, receive) = oneshot::channel();
            let mut sink = ReadySink {
                bytes: readiness.to_vec(),
                ready: Some(send),
                ready_accepted: false,
                stderr: Zeroizing::new(Vec::new()),
                stderr_limit: 4096,
                stderr_truncated: false,
                diagnostics: Vec::<u8>::new(),
            };
            sink.publish_readiness();
            assert_eq!(
                receive.await.unwrap().unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert!(sink.ready.is_none());
            assert!(!sink.ready_accepted);
            sink.accept_stderr(b"private-bootstrap-material\n")
                .await
                .unwrap();
            assert!(sink.diagnostics.is_empty());
        }
        let (send, receive) = oneshot::channel();
        drop(receive);
        let mut sink = ReadySink {
            bytes: br#"{"address":"127.0.0.1:12345"}"#.to_vec(),
            ready: Some(send),
            ready_accepted: false,
            stderr: Zeroizing::new(Vec::new()),
            stderr_limit: 4096,
            stderr_truncated: false,
            diagnostics: Vec::<u8>::new(),
        };
        sink.publish_readiness();
        assert!(!sink.ready_accepted);
        sink.accept_stderr(b"private-bootstrap-material\n")
            .await
            .unwrap();
        assert!(sink.diagnostics.is_empty());
    }

    #[tokio::test]
    async fn post_ready_stderr_write_is_borrowed_and_reports_closed_consumer() {
        use tokio::io::AsyncReadExt;
        let (writer, mut reader) = tokio::io::duplex(1);
        let mut sink = ReadySink {
            bytes: Vec::new(),
            ready: None,
            ready_accepted: true,
            stderr: Zeroizing::new(Vec::new()),
            stderr_limit: 4096,
            stderr_truncated: false,
            diagnostics: writer,
        };
        let mut write = Box::pin(sink.accept_stderr(b"ab"));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(write.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(write);
        let mut byte = [0];
        reader.read_exact(&mut byte).await.unwrap();
        assert_eq!(byte, [b'a']);
        drop(reader);
        assert_eq!(
            sink.accept_stderr(b"c").await,
            Err(SinkFailure::ConsumerFailed)
        );
    }

    #[test]
    fn arbitrary_truncated_and_multiline_diagnostics_are_redacted() {
        for stderr in [
            "private-bootstrap-material",
            "model child startup/service failed: Other; model=\"private-bootstrap-material\"\n",
            "model child startup/service failed: private-bootstrap-material\n",
            "model child startup/service failed: Other; model=\"Native\"",
            "model child startup/service failed: Other; model=\"Native\"\nprivate-bootstrap-material\n",
        ] {
            let diagnostic = StartupDiagnostic {
                stderr: Zeroizing::new(stderr.as_bytes().to_vec()),
                truncated: false,
            };
            assert!(
                !diagnostic
                    .to_string()
                    .contains("private-bootstrap-material")
            );
            assert!(!format!("{diagnostic:?}").contains("private-bootstrap-material"));
            if !stderr.ends_with('\n') || stderr.lines().count() != 1 {
                assert!(diagnostic.model_kind().is_none());
            }
        }
    }
}
