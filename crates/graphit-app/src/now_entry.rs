//! Continuous Now uses the existing Source discovery and admitted transport.
use crate::{
    now_renderer::Renderer,
    now_transport::{NowTransportError, connect_now},
    source_entry,
    source_server::BrokerRequest,
};
use graphit_core::{
    native_document_file::NativeDocumentFileLimits, project_identity::ProjectId,
    project_paths::InstallationPaths,
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::{
    fmt,
    io::{self, Write},
};

#[derive(clap::Args)]
pub struct Arguments {
    /// Canonical project ID; authorization remains with the daemon and Broker.
    #[arg(long, required = true)]
    pub project: ProjectId,
    /// Explicit configured Broker provider.
    #[arg(long, required = true, value_parser = literal)]
    pub provider: String,
    /// Filter the Broker view by its literal project version.
    #[arg(long, value_parser = literal)]
    pub version: Option<String>,
    /// Filter the Broker view by its instance ID.
    #[arg(long, value_parser = literal)]
    pub instance: Option<String>,
}
fn literal(value: &str) -> Result<String, &'static str> {
    if value.is_empty() || value.contains('\0') {
        Err("value must be nonempty and contain no NUL")
    } else {
        Ok(value.to_owned())
    }
}

/// Diagnostic boundary: underlying errors remain private and are never printed.
pub enum Failure {
    Discovery(io::Error),
    Transport(NowTransportError),
    Output(io::Error),
    Closed,
}
impl Failure {
    pub fn broken_pipe(&self) -> bool {
        matches!(self, Self::Output(error) if error.kind() == io::ErrorKind::BrokenPipe)
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::Discovery(_) => "Now service unavailable; start the daemon for this installation",
            Self::Transport(NowTransportError::Http(401)) => {
                "Now authentication failed; refresh or log in again"
            }
            Self::Transport(NowTransportError::Http(403)) => "Now project access denied",
            Self::Transport(NowTransportError::Http(400) | NowTransportError::Input) => {
                "Now provider or filters rejected"
            }
            Self::Transport(NowTransportError::Request(error))
                if error.kind() == io::ErrorKind::TimedOut =>
            {
                "Now request expired"
            }
            Self::Transport(NowTransportError::Request(error))
                if error.kind() == io::ErrorKind::Interrupted =>
            {
                "Now cancelled"
            }
            Self::Transport(
                NowTransportError::Protocol
                | NowTransportError::Frame(_)
                | NowTransportError::Snapshot(_),
            ) => "Now stream ended with invalid data or authorization failure",
            Self::Transport(_) => "Now connection unavailable or closed",
            Self::Output(_) => "Now output unavailable",
            Self::Closed => "Now connection closed; run the command again to reconnect",
        })
    }
}
impl fmt::Debug for Failure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, out)
    }
}
impl std::error::Error for Failure {}

impl Arguments {
    fn request(self) -> BrokerRequest {
        BrokerRequest {
            project_id: self.project,
            provider: self.provider,
            version: self.version,
            instance_id: self.instance,
        }
    }
}
async fn open<'a>(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    limits: (NativeDocumentFileLimits, ContentLimits),
    request: &'a BrokerRequest,
    original: &'a RequestContext<'a>,
) -> Result<crate::now_transport::NowStream<'a>, Failure> {
    let address = source_entry::address(installation, limits.0, original)
        .await
        .map_err(Failure::Discovery)?;
    connect_now(installation, address, client, limits, request, original)
        .await
        .map_err(Failure::Transport)
}
/// Replace the terminal view using the existing admitted stream.
pub async fn run_terminal<W: Write>(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    arguments: Arguments,
    limits: (NativeDocumentFileLimits, ContentLimits),
    original: &RequestContext<'_>,
    output: &mut W,
) -> Result<(), Failure> {
    let request = arguments.request();
    let mut stream = open(installation, client, limits, &request, original).await?;
    let mut renderer = Renderer::new(output, &request).map_err(Failure::Output)?;
    while let Some(snapshot) = stream.next().await.map_err(Failure::Transport)? {
        renderer.render(&snapshot).map_err(Failure::Output)?;
    }
    renderer.finish().map_err(Failure::Output)?;
    Err(Failure::Closed)
}

#[cfg(unix)]
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
#[cfg(unix)]
use std::os::fd::{AsFd as OutputHandle, AsRawFd, OwnedFd, RawFd};
#[cfg(unix)]
use tokio::io::unix::AsyncFd;

#[cfg(unix)]
struct NonblockingFd {
    fd: OwnedFd,
    previous: Option<OFlags>,
}
#[cfg(unix)]
impl AsRawFd for NonblockingFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
#[cfg(unix)]
impl Drop for NonblockingFd {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            let _ = fcntl_setfl(&self.fd, previous);
        }
    }
}
#[cfg(unix)]
enum JsonOutput {
    Pipe(AsyncFd<NonblockingFd>),
    File(std::fs::File),
}
#[cfg(unix)]
impl JsonOutput {
    fn capture(output: &impl OutputHandle) -> io::Result<Self> {
        let fd = rustix::io::fcntl_dupfd_cloexec(output.as_fd(), 0)?;
        let file = std::fs::File::from(fd);
        if file.metadata()?.is_file() {
            return Ok(Self::File(file));
        }
        let fd: OwnedFd = file.into();
        let previous = fcntl_getfl(&fd)?;
        let changed = !previous.contains(OFlags::NONBLOCK);
        if changed {
            fcntl_setfl(&fd, previous | OFlags::NONBLOCK)?;
        }
        // The owner restores changed flags even if reactor registration fails.
        Ok(Self::Pipe(AsyncFd::new(NonblockingFd {
            fd,
            previous: changed.then_some(previous),
        })?))
    }
    async fn render(
        &mut self,
        snapshot: &graphit_daemon::now_subscription::RemoteNowSnapshot,
        original: &RequestContext<'_>,
    ) -> Result<(), Failure> {
        original
            .check()
            .map_err(|e| Failure::Transport(NowTransportError::Request(e)))?;
        match self {
            Self::File(file) => {
                serde_json::to_writer(&mut *file, snapshot).map_err(json_failure)?;
                file.write_all(b"\n").map_err(Failure::Output)?;
                file.flush().map_err(Failure::Output)?;
            }
            Self::Pipe(fd) => {
                // One serialization buffer keeps partial writes resumable; no cloned DTO/history.
                let mut frame = serde_json::to_vec(snapshot).map_err(json_failure)?;
                frame.push(b'\n');
                let mut remaining = frame.as_slice();
                while !remaining.is_empty() {
                    original
                        .check()
                        .map_err(|e| Failure::Transport(NowTransportError::Request(e)))?;
                    let mut ready = tokio::select! {
                        ready = fd.writable() => ready.map_err(Failure::Output)?,
                        () = original.cancellation().cancelled() => return Err(Failure::Transport(NowTransportError::Request(io::ErrorKind::Interrupted.into()))),
                        () = output_deadline(original) => return Err(Failure::Transport(NowTransportError::Request(io::ErrorKind::TimedOut.into()))),
                    };
                    match ready.try_io(|inner| {
                        rustix::io::write(&inner.get_ref().fd, remaining).map_err(io::Error::from)
                    }) {
                        Ok(Ok(0)) => return Err(Failure::Output(io::ErrorKind::WriteZero.into())),
                        Ok(Ok(written)) => remaining = &remaining[written..],
                        Ok(Err(error)) => return Err(Failure::Output(error)),
                        Err(_) => continue,
                    }
                }
            }
        }
        original
            .check()
            .map_err(|e| Failure::Transport(NowTransportError::Request(e)))
    }
}

#[cfg(windows)]
use std::os::windows::io::AsHandle as OutputHandle;

#[cfg(windows)]
struct JsonOutput {
    sender: Option<std::sync::mpsc::Sender<OutputFrame>>,
    worker: Option<std::thread::JoinHandle<()>>,
    stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[cfg(windows)]
struct OutputFrame {
    bytes: Vec<u8>,
    completed: tokio::sync::oneshot::Sender<io::Result<()>>,
}
#[cfg(windows)]
impl JsonOutput {
    fn capture(output: &impl OutputHandle) -> io::Result<Self> {
        let mut file = std::fs::File::from(output.as_handle().try_clone_to_owned()?);
        let (sender, frames) = std::sync::mpsc::channel::<OutputFrame>();
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stopped = std::sync::Arc::clone(&stopped);
        let worker = std::thread::Builder::new()
            .name("graphit-now-output".into())
            .spawn(move || {
                while let Ok(frame) = frames.recv() {
                    let result = write_frame(&mut file, &frame.bytes, &worker_stopped);
                    drop(frame.bytes);
                    let failed = result.is_err();
                    if frame.completed.send(result).is_err() || failed {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            stopped,
        })
    }
    async fn render(
        &mut self,
        snapshot: &graphit_daemon::now_subscription::RemoteNowSnapshot,
        original: &RequestContext<'_>,
    ) -> Result<(), Failure> {
        original
            .check()
            .map_err(|e| Failure::Transport(NowTransportError::Request(e)))?;
        let mut bytes = serde_json::to_vec(snapshot).map_err(json_failure)?;
        bytes.push(b'\n');
        let (completed, completion) = tokio::sync::oneshot::channel();
        // The borrowed sink waits for this frame before accepting another; no snapshot queue.
        self.sender
            .as_ref()
            .ok_or_else(|| Failure::Output(io::ErrorKind::BrokenPipe.into()))?
            .send(OutputFrame { bytes, completed })
            .map_err(|_| Failure::Output(io::ErrorKind::BrokenPipe.into()))?;
        tokio::select! {
            result = completion => result
                .map_err(|_| Failure::Output(io::Error::other("Now output worker ended")))?
                .map_err(Failure::Output)?,
            () = original.cancellation().cancelled() => {
                self.stop_and_join().map_err(Failure::Output)?;
                return Err(Failure::Transport(NowTransportError::Request(io::ErrorKind::Interrupted.into())));
            },
            () = output_deadline(original) => {
                self.stop_and_join().map_err(Failure::Output)?;
                return Err(Failure::Transport(NowTransportError::Request(io::ErrorKind::TimedOut.into())));
            },
        }
        original
            .check()
            .map_err(|e| Failure::Transport(NowTransportError::Request(e)))
    }
    #[allow(
        unsafe_code,
        reason = "Cancel only this owned stdout worker and wait on its live thread handle"
    )]
    fn stop_and_join(&mut self) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::{ERROR_NOT_FOUND, WAIT_FAILED, WAIT_OBJECT_0},
            System::{IO::CancelSynchronousIo, Threading::WaitForSingleObject},
        };
        self.stopped
            .store(true, std::sync::atomic::Ordering::Release);
        // Closing the sole sender also wakes a worker waiting for its next frame.
        self.sender.take();
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        let mut failure = None;
        while !worker.is_finished() {
            // SAFETY: JoinHandle owns a live native thread handle until join below.
            // The thread does only this sink's writes; no unrelated IO can be canceled.
            if unsafe { CancelSynchronousIo(worker.as_raw_handle()) } != 0 {
                // The accepted cancellation still needs the owned join below.
                break;
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NOT_FOUND as i32) {
                failure = Some(error);
                break;
            }
            // A stop can race between the worker's flag check and entering WriteFile.
            // Wait on the actual thread before retrying that physical cancellation;
            // this is not a snapshot timer, output deadline or detached worker.
            // SAFETY: The borrowed native handle stays owned by worker across this wait.
            let wait = unsafe { WaitForSingleObject(worker.as_raw_handle(), 1) };
            if wait == WAIT_OBJECT_0 {
                break;
            }
            if wait == WAIT_FAILED {
                failure.get_or_insert_with(io::Error::last_os_error);
                break;
            }
        }
        worker
            .join()
            .map_err(|_| io::Error::other("Now output worker ended"))?;
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
#[cfg(windows)]
impl Drop for JsonOutput {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[cfg(any(windows, test))]
fn write_frame(
    output: &mut impl Write,
    mut remaining: &[u8],
    stopped: &std::sync::atomic::AtomicBool,
) -> io::Result<()> {
    use std::sync::atomic::Ordering;
    while !remaining.is_empty() {
        if stopped.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        match output.write(remaining) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => remaining = &remaining[written..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    if stopped.load(Ordering::Acquire) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    output.flush()
}

fn json_failure(error: serde_json::Error) -> Failure {
    Failure::Output(io::Error::new(
        error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData),
        error,
    ))
}
async fn output_deadline(original: &RequestContext<'_>) {
    match original.deadline() {
        Some(end) => tokio::time::sleep_until(end.into()).await,
        None => std::future::pending::<()>().await,
    }
}
/// JSON pipe readiness and partial writes honor the same Original. Regular file redirection is supported.
pub async fn run_pipe(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    arguments: Arguments,
    limits: (NativeDocumentFileLimits, ContentLimits),
    original: &RequestContext<'_>,
    output: &impl OutputHandle,
) -> Result<(), Failure> {
    original
        .check()
        .map_err(|e| Failure::Transport(NowTransportError::Request(e)))?;
    let mut output = JsonOutput::capture(output).map_err(Failure::Output)?;
    let request = arguments.request();
    let mut stream = open(installation, client, limits, &request, original).await?;
    while let Some(snapshot) = stream.next().await.map_err(Failure::Transport)? {
        output.render(&snapshot, original).await?;
    }
    Err(Failure::Closed)
}

#[cfg(test)]
mod output_tests {
    use super::write_frame;
    use std::{
        io::{self, Write},
        sync::atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn partial_writes_and_interruption_preserve_exact_frame() {
        struct Partial {
            bytes: Vec<u8>,
            interrupted: bool,
            flushed: bool,
        }
        impl Write for Partial {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(io::ErrorKind::Interrupted.into());
                }
                let length = bytes.len().min(2);
                self.bytes.extend_from_slice(&bytes[..length]);
                Ok(length)
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushed = true;
                Ok(())
            }
        }
        let mut output = Partial {
            bytes: Vec::new(),
            interrupted: false,
            flushed: false,
        };
        let frame = b"{\"instances\":[]}\n";
        write_frame(&mut output, frame, &AtomicBool::new(false)).unwrap();
        assert_eq!(output.bytes, frame);
        assert!(output.flushed);
    }

    #[test]
    fn already_stopped_frame_never_starts_io() {
        struct NoIo;
        impl Write for NoIo {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                panic!("write after stop");
            }
            fn flush(&mut self) -> io::Result<()> {
                panic!("flush after stop");
            }
        }
        let error = write_frame(&mut NoIo, b"frame\n", &AtomicBool::new(true)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn stop_after_partial_write_keeps_prefix_without_retrying_or_flushing() {
        struct StopAfterPrefix<'a> {
            stopped: &'a AtomicBool,
            bytes: Vec<u8>,
        }
        impl Write for StopAfterPrefix<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                assert!(self.bytes.is_empty(), "retried after stop");
                self.bytes.extend_from_slice(&bytes[..2]);
                self.stopped.store(true, Ordering::Release);
                Ok(2)
            }
            fn flush(&mut self) -> io::Result<()> {
                panic!("flush after stop");
            }
        }
        let stopped = AtomicBool::new(false);
        let mut output = StopAfterPrefix {
            stopped: &stopped,
            bytes: Vec::new(),
        };
        let error = write_frame(&mut output, b"frame\n", &stopped).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(output.bytes, b"fr");
    }

    #[test]
    fn broken_pipe_and_zero_write_remain_errors() {
        struct Failed(bool);
        impl Write for Failed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                if self.0 {
                    Err(io::ErrorKind::BrokenPipe.into())
                } else {
                    Ok(0)
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                panic!("flush after failed write");
            }
        }
        let stopped = AtomicBool::new(false);
        assert_eq!(
            write_frame(&mut Failed(true), b"frame\n", &stopped)
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(
            write_frame(&mut Failed(false), b"frame\n", &stopped)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WriteZero
        );
    }
}

#[cfg(all(test, windows))]
mod windows_output_tests {
    use super::{JsonOutput, OutputFrame};
    use std::{
        io::{self, Read},
        os::windows::io::AsRawHandle,
        time::{Duration, Instant},
    };

    fn send(output: &JsonOutput, bytes: Vec<u8>) -> tokio::sync::oneshot::Receiver<io::Result<()>> {
        let (completed, completion) = tokio::sync::oneshot::channel();
        assert!(
            output
                .sender
                .as_ref()
                .unwrap()
                .send(OutputFrame { bytes, completed })
                .is_ok()
        );
        completion
    }

    #[allow(
        unsafe_code,
        reason = "Observe only the owned fixture writer thread to prove blocked physical IO"
    )]
    fn wait_for_pending_write(output: &JsonOutput) {
        use windows_sys::Win32::{
            Foundation::WAIT_TIMEOUT,
            System::Threading::{GetThreadIOPendingFlag, WaitForSingleObject},
        };
        let worker = output.worker.as_ref().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut pending = 0;
            // SAFETY: worker retains its thread handle; pending is a writable local BOOL.
            assert_ne!(
                unsafe { GetThreadIOPendingFlag(worker.as_raw_handle(), &mut pending) },
                0
            );
            if pending != 0 {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "fixture did not enter blocked output IO"
            );
            // SAFETY: the fixture owns worker until this witness returns.
            assert_eq!(
                unsafe { WaitForSingleObject(worker.as_raw_handle(), 1) },
                WAIT_TIMEOUT
            );
        }
    }

    #[test]
    fn redirected_file_keeps_exact_bytes_and_joins_writer() {
        let file = tempfile::tempfile().unwrap();
        let mut output = JsonOutput::capture(&file).unwrap();
        let frame = b"{\"instances\":[]}\n".to_vec();
        assert!(
            send(&output, frame.clone())
                .blocking_recv()
                .unwrap()
                .is_ok()
        );
        output.stop_and_join().unwrap();
        assert!(output.worker.is_none());
        use std::io::{Seek, SeekFrom};
        let mut file = file;
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut actual = Vec::new();
        file.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, frame);
    }

    #[test]
    fn closing_reader_preserves_broken_pipe() {
        let (reader, writer) = io::pipe().unwrap();
        drop(reader);
        let mut output = JsonOutput::capture(&writer).unwrap();
        let error = send(&output, b"frame\n".to_vec())
            .blocking_recv()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        output.stop_and_join().unwrap();
    }

    #[test]
    fn idle_output_is_joined_without_a_frame() {
        let (_reader, writer) = io::pipe().unwrap();
        let mut output = JsonOutput::capture(&writer).unwrap();
        output.stop_and_join().unwrap();
        assert!(output.worker.is_none());
        assert!(output.sender.is_none());
    }

    #[test]
    fn cancellation_joins_a_writer_blocked_on_an_unconsumed_pipe() {
        let (mut reader, writer) = io::pipe().unwrap();
        let mut output = JsonOutput::capture(&writer).unwrap();
        let length = 8 * 1024 * 1024;
        let completion = send(&output, vec![b'x'; length]);
        wait_for_pending_write(&output);
        output.stop_and_join().unwrap();
        assert!(output.worker.is_none());
        assert!(completion.blocking_recv().unwrap().is_err());
        drop(writer);
        let mut prefix = Vec::new();
        reader.read_to_end(&mut prefix).unwrap();
        assert!(prefix.len() < length);
        assert!(prefix.iter().all(|byte| *byte == b'x'));
    }

    #[test]
    fn dropping_output_cancels_and_joins_pending_write() {
        let (mut reader, writer) = io::pipe().unwrap();
        let output = JsonOutput::capture(&writer).unwrap();
        let completion = send(&output, vec![b'x'; 8 * 1024 * 1024]);
        wait_for_pending_write(&output);
        drop(output);
        assert!(completion.blocking_recv().unwrap().is_err());
        drop(writer);
        let mut prefix = Vec::new();
        reader.read_to_end(&mut prefix).unwrap();
        assert!(prefix.iter().all(|byte| *byte == b'x'));
    }
}
