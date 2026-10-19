//! Stdio writers for the existing SDK transport; create inside the I/O runtime.
use tokio::io::AsyncWrite;

/// Avoid Tokio's extra blocking-stdout copy when stdout is a Linux pipe.
/// Files, terminals and systems without an accessible procfs keep Tokio stdout.
pub fn stdout() -> Box<dyn AsyncWrite + Send + Unpin> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsFd;
        if let Some(pipe) = pipe_writer(std::io::stdout().as_fd()) {
            return Box::new(pipe);
        }
    }
    Box::new(tokio::io::stdout())
}

#[cfg(target_os = "linux")]
fn pipe_writer(fd: std::os::fd::BorrowedFd<'_>) -> Option<tokio::net::unix::pipe::Sender> {
    use rustix::fs::{OFlags, fcntl_getfl};
    use std::os::fd::AsRawFd;

    let access = fcntl_getfl(fd).ok()? & OFlags::ACCMODE;
    if access != OFlags::WRONLY && access != OFlags::RDWR {
        return None;
    }
    // Reopening creates an independent file description. Duplicating the fd
    // and setting O_NONBLOCK would also change the inherited stdout's flags.
    // OpenOptions validates FIFO and stays write-only: no hidden reader may
    // mask BrokenPipe. Failure only declines this optimization; writes through
    // the original stdout still report their normal errors.
    tokio::net::unix::pipe::OpenOptions::new()
        .open_sender(format!("/proc/self/fd/{}", fd.as_raw_fd()))
        .ok()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::pipe_writer;
    use rustix::fs::{OFlags, fcntl_getfl};
    use std::{io, os::fd::AsFd, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::unix::pipe,
        time::timeout,
    };

    #[tokio::test]
    async fn pipe_preserves_flags_backpressure_payload_and_eof() -> io::Result<()> {
        let (sender, mut reader) = pipe::pipe()?;
        let original = sender.into_blocking_fd()?;
        let flags = fcntl_getfl(original.as_fd())?;
        assert!(!flags.contains(OFlags::NONBLOCK));
        let mut writer = pipe_writer(original.as_fd()).expect("writable pipe");
        assert_eq!(fcntl_getfl(original.as_fd())?, flags);
        drop(original);
        let payload = "Graphit á界\0\n".repeat(250_000).into_bytes();
        let expected = payload.clone();
        let (_, received) = timeout(Duration::from_secs(5), async move {
            tokio::try_join!(
                async move {
                    writer.write_all(&payload).await?;
                    drop(writer);
                    Ok::<_, io::Error>(())
                },
                async move {
                    let mut bytes = Vec::new();
                    reader.read_to_end(&mut bytes).await?;
                    Ok(bytes)
                }
            )
        })
        .await??;
        assert_eq!(received, expected);
        Ok(())
    }

    #[tokio::test]
    async fn pipe_reports_broken_reader() -> io::Result<()> {
        let (sender, reader) = pipe::pipe()?;
        let original = sender.into_blocking_fd()?;
        let mut writer = pipe_writer(original.as_fd()).expect("writable pipe");
        drop(reader);
        let error = timeout(Duration::from_secs(2), writer.write_all(b"response"))
            .await?
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_blocked_write_drops_without_a_background_writer() -> io::Result<()> {
        let (sender, mut reader) = pipe::pipe()?;
        let original = sender.into_blocking_fd()?;
        let mut writer = pipe_writer(original.as_fd()).expect("writable pipe");
        drop(original);
        let bytes = vec![b'x'; 3 * 1024 * 1024];
        // Establish readiness before timing the blocked write, so scheduling
        // cannot make the nonempty-prefix assertion depend on the timer.
        timeout(Duration::from_secs(2), writer.write_all(&bytes[..1])).await??;
        assert!(
            timeout(Duration::from_millis(30), writer.write_all(&bytes))
                .await
                .is_err()
        );
        drop(writer);
        let mut received = Vec::new();
        timeout(Duration::from_secs(2), reader.read_to_end(&mut received)).await??;
        assert!(!received.is_empty());
        assert!(received.len() < bytes.len());
        assert_eq!(received, bytes[..received.len()]);
        Ok(())
    }

    #[tokio::test]
    async fn non_pipe_and_read_only_descriptors_keep_standard_stdout() -> io::Result<()> {
        let file = tempfile::tempfile()?;
        assert!(pipe_writer(file.as_fd()).is_none());
        let (_sender, reader) = pipe::pipe()?;
        let read_only = reader.into_blocking_fd()?;
        assert!(pipe_writer(read_only.as_fd()).is_none());
        Ok(())
    }
}
