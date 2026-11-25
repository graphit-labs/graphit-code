//! Owned Unix readiness registration; the registered descriptor cannot be replaced.
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use std::{
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd},
};
use tokio::io::unix::{AsyncFd, AsyncFdReadyGuard, TryIoError};

struct NonblockingFd {
    fd: OwnedFd,
    previous: Option<OFlags>,
}
impl AsRawFd for NonblockingFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
impl Drop for NonblockingFd {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            let _ = fcntl_setfl(&self.fd, previous);
        }
    }
}

/// Retains the same owned descriptor until Tokio deregisters it.
///
/// Nonblocking flags changed at registration are restored on drop or failure.
/// Duplicated descriptors share these flags; aliases must not change them while
/// this owner is active. The caller must enter a Tokio runtime with IO enabled.
pub struct UnixAsyncFd {
    inner: AsyncFd<NonblockingFd>,
}
impl UnixAsyncFd {
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
        let previous = fcntl_getfl(&fd)?;
        let changed = !previous.contains(OFlags::NONBLOCK);
        if changed {
            fcntl_setfl(&fd, previous | OFlags::NONBLOCK)?;
        }
        let fd = NonblockingFd {
            fd,
            previous: changed.then_some(previous),
        };
        Ok(Self {
            inner: register(fd)?,
        })
    }

    pub async fn readable(&self) -> io::Result<UnixAsyncFdReadyGuard<'_>> {
        Ok(UnixAsyncFdReadyGuard {
            inner: self.inner.readable().await?,
        })
    }

    pub async fn writable(&self) -> io::Result<UnixAsyncFdReadyGuard<'_>> {
        Ok(UnixAsyncFdReadyGuard {
            inner: self.inner.writable().await?,
        })
    }
}
impl AsFd for UnixAsyncFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inner.get_ref().fd.as_fd()
    }
}
#[allow(unsafe_code)]
fn register(fd: NonblockingFd) -> io::Result<AsyncFd<NonblockingFd>> {
    // SAFETY: NonblockingFd retains a valid OwnedFd and returns the same descriptor.
    // The opaque owner and guard expose only borrows, never mutable/replacement
    // access. AsyncFd deregisters before dropping NonblockingFd and its OwnedFd.
    Ok(unsafe { AsyncFd::register(fd) }?)
}

/// Keeps the registration borrowed while readiness is consumed by nonblocking IO.
#[must_use]
pub struct UnixAsyncFdReadyGuard<'a> {
    inner: AsyncFdReadyGuard<'a, NonblockingFd>,
}
impl UnixAsyncFdReadyGuard<'_> {
    pub fn try_io<R>(
        &mut self,
        operation: impl FnOnce(BorrowedFd<'_>) -> io::Result<R>,
    ) -> Result<io::Result<R>, TryIoError> {
        self.inner
            .try_io(|inner| operation(inner.get_ref().fd.as_fd()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixStream, time::Duration};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn read(owner: &UnixAsyncFd, bytes: &mut [u8]) -> io::Result<usize> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut ready = owner.readable().await?;
                match ready.try_io(|fd| rustix::io::read(fd, &mut *bytes).map_err(Into::into)) {
                    Ok(result) => return result,
                    Err(_) => continue,
                }
            }
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "descriptor read readiness"))?
    }

    #[tokio::test]
    async fn writes_preserve_bytes_eof_and_restore_original_flags() -> io::Result<()> {
        for initially_nonblocking in [false, true] {
            let (writer, reader) = UnixStream::pair()?;
            writer.set_nonblocking(initially_nonblocking)?;
            let witness = writer.try_clone()?;
            let previous = fcntl_getfl(&witness)?;
            reader.set_nonblocking(true)?;
            let mut reader = tokio::net::UnixStream::from_std(reader)?;
            let owner = UnixAsyncFd::new(writer.into())?;
            assert!(fcntl_getfl(&witness)?.contains(OFlags::NONBLOCK));
            let body = b"first\nsecond\n";
            let count = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let mut ready = owner.writable().await?;
                    match ready.try_io(|fd| rustix::io::write(fd, body).map_err(io::Error::from)) {
                        Ok(result) => return result,
                        Err(_) => continue,
                    }
                }
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "descriptor write readiness"))??;
            assert!(count > 0 && count <= body.len());
            drop(owner);
            // Darwin records FWASWRITTEN after IO; restoring O_NONBLOCK does
            // not rewind the kernel's history bits returned by F_GETFL.
            assert_eq!(
                fcntl_getfl(&witness)?.contains(OFlags::NONBLOCK),
                previous.contains(OFlags::NONBLOCK)
            );
            drop(witness);
            let mut actual = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), reader.read_to_end(&mut actual))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "descriptor EOF"))??;
            assert_eq!(actual, body[..count]);
        }
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_readiness_and_would_block_preserve_reuse_and_eof() -> io::Result<()> {
        let (reader, writer) = UnixStream::pair()?;
        writer.set_nonblocking(true)?;
        let mut writer = tokio::net::UnixStream::from_std(writer)?;
        let owner = UnixAsyncFd::new(reader.into())?;
        let mut bytes = [0_u8; 1];
        tokio::select! {
            biased;
            result = read(&owner, &mut bytes) => panic!("idle descriptor produced a read: {result:?}"),
            () = std::future::ready(()) => {}
        }
        writer.write_all(b"x").await?;
        let mut ready = tokio::time::timeout(Duration::from_secs(5), owner.readable())
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "read after cancellation"))??;
        assert_eq!(
            ready
                .try_io(|fd| rustix::io::read(fd, &mut bytes).map_err(io::Error::from))
                .expect("fixture byte is ready")?,
            1
        );
        assert_eq!(bytes, *b"x");
        assert!(
            ready
                .try_io(|fd| rustix::io::read(fd, &mut bytes).map_err(io::Error::from))
                .is_err(),
            "WouldBlock must clear readiness through the Tokio guard"
        );
        drop(ready);
        writer.write_all(b"y").await?;
        assert_eq!(read(&owner, &mut bytes).await?, 1);
        assert_eq!(bytes, *b"y");
        drop(writer);
        assert_eq!(read(&owner, &mut bytes).await?, 0);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn failed_registration_restores_flags_before_closing_the_owner() -> io::Result<()> {
        let file = tempfile::tempfile()?;
        let witness = file.try_clone()?;
        let previous = fcntl_getfl(&witness)?;
        assert!(!previous.contains(OFlags::NONBLOCK));
        // Linux epoll rejects regular files; the returned registration error
        // must drop NonblockingFd and restore the shared flags.
        assert!(UnixAsyncFd::new(file.into()).is_err());
        assert_eq!(fcntl_getfl(&witness)?, previous);
        Ok(())
    }
}
