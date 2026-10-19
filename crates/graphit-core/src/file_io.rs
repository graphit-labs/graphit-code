//! Shared cancellation and deadline for captured file and artifact operations.
//! Synchronous native I/O checks the request between bounded chunks.
use crate::file_lock::Cancellation;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    sync::Arc,
    time::Instant,
};

/// Retains the original request identity, deadline and cancellation signal.
pub struct SnapshotContext {
    identity: RequestIdentity,
    deadline: Option<Instant>,
    cancellation: Cancellation,
}
impl SnapshotContext {
    pub fn new(deadline: Instant, cancellation: Cancellation) -> Self {
        Self {
            identity: RequestIdentity(Arc::new(())),
            deadline: Some(deadline),
            cancellation,
        }
    }
    /// A continuous service has cancellation but no artificial expiry.
    pub fn service(cancellation: Cancellation) -> Self {
        Self {
            identity: RequestIdentity(Arc::new(())),
            deadline: None,
            cancellation,
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Finite transports must reject a service context rather than invent a deadline.
    pub fn required_deadline(&self) -> io::Result<Instant> {
        self.check()?;
        self.deadline.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "operation requires a finite deadline",
            )
        })
    }
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }
    /// Equal deadlines and signals do not establish request identity or authority.
    pub fn same_request(&self, other: &Self) -> bool {
        self.identity.matches(other)
    }
    pub fn check(&self) -> io::Result<()> {
        crate::local_files::check_cancelled(&self.cancellation)?;
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(())
    }
    pub(crate) fn request_identity(&self) -> RequestIdentity {
        self.identity.clone()
    }
    pub(crate) fn read_exact_length(
        &self,
        file: &mut File,
        expected: u64,
        buffer: &mut [u8],
        mut retained: Option<&mut Vec<u8>>,
    ) -> io::Result<[u8; 32]> {
        self.check()?;
        if let Some(bytes) = &mut retained {
            bytes
                .try_reserve_exact(
                    usize::try_from(expected).map_err(|_| io::ErrorKind::FileTooLarge)?,
                )
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
        }
        self.consume_file(file, expected, buffer, |chunk| {
            if let Some(bytes) = &mut retained {
                bytes.extend_from_slice(chunk);
            }
            Ok(())
        })
    }

    pub(crate) fn consume_file(
        &self,
        file: &mut File,
        expected: u64,
        buffer: &mut [u8],
        mut consume: impl FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<[u8; 32]> {
        let mut digest = Sha256::new();
        let mut left = expected;
        while left != 0 {
            self.check()?;
            let length = left.min(buffer.len() as u64) as usize;
            let read = file.read(&mut buffer[..length])?;
            if read == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            digest.update(&buffer[..read]);
            consume(&buffer[..read])?;
            left -= read as u64;
        }
        self.check()?;
        Ok(digest.finalize().into())
    }
}
/// An allocation identity avoids address reuse when a request context is moved.
/// It is internal data, never a serialized request or filesystem capability.
#[derive(Clone)]
pub(crate) struct RequestIdentity(Arc<()>);
impl RequestIdentity {
    pub(crate) fn matches(&self, context: &SnapshotContext) -> bool {
        Arc::ptr_eq(&self.0, &context.identity.0)
    }
}

/// Actual document bounds used during publication and recovery.
pub(crate) struct NativeDocumentIo {
    context: Arc<SnapshotContext>,
    stage_bytes: u64,
    destination_bytes: u64,
}
impl SnapshotContext {
    pub(crate) fn native_inspection(self: &Arc<Self>, length: u64) -> io::Result<NativeIoPass> {
        self.check()?;
        let bytes = length.checked_add(1).ok_or(io::ErrorKind::FileTooLarge)?;
        Ok(NativeIoPass::new(Arc::clone(self), bytes, false))
    }
    pub(crate) fn native_document_io(
        self: &Arc<Self>,
        desired: u64,
        previous: Option<u64>,
    ) -> io::Result<NativeDocumentIo> {
        self.check()?;
        Ok(NativeDocumentIo {
            context: Arc::clone(self),
            stage_bytes: desired.checked_add(1).ok_or(io::ErrorKind::FileTooLarge)?,
            destination_bytes: desired
                .max(previous.unwrap_or(0))
                .checked_add(1)
                .ok_or(io::ErrorKind::FileTooLarge)?,
        })
    }
}
impl NativeDocumentIo {
    pub(crate) fn normal(&self, bytes: u64) -> io::Result<NativeIoPass> {
        self.context.check()?;
        Ok(NativeIoPass::new(Arc::clone(&self.context), bytes, false))
    }
    // A durable effect must be classified and cleaned up after cancellation.
    pub(crate) fn complete_stage(&self) -> NativeIoPass {
        NativeIoPass::new(Arc::clone(&self.context), self.stage_bytes, true)
    }
    pub(crate) fn complete_destination(&self) -> NativeIoPass {
        NativeIoPass::new(Arc::clone(&self.context), self.destination_bytes, true)
    }
}
/// One bounded native content operation; completion belongs to atomic recovery.
pub(crate) struct NativeIoPass {
    context: Arc<SnapshotContext>,
    remaining: u64,
    completion: bool,
    inspection: bool,
    ended: bool,
}
impl NativeIoPass {
    fn new(context: Arc<SnapshotContext>, remaining: u64, completion: bool) -> Self {
        Self {
            context,
            remaining,
            completion,
            inspection: false,
            ended: false,
        }
    }
    pub(crate) fn check(&self) -> io::Result<()> {
        if self.completion {
            Ok(())
        } else {
            self.context.check()
        }
    }

    pub(crate) fn matches_request(&self, identity: &RequestIdentity) -> bool {
        identity.matches(&self.context)
    }

    pub(crate) fn begin_inspection(&mut self, bytes: u64) -> io::Result<()> {
        self.check()?;
        if self.inspection || self.ended {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if bytes > self.remaining {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.inspection = true;
        Ok(())
    }

    pub(crate) fn read(&mut self, file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        if !self.inspection || self.ended || buffer.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if buffer.len() as u64 > self.remaining {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        match file.read(buffer) {
            Ok(read) => {
                self.remaining -= read as u64;
                if read == 0 {
                    self.remaining = 0;
                    self.ended = true;
                }
                Ok(read)
            }
            Err(error) => {
                self.remaining = 0;
                self.ended = true;
                Err(error)
            }
        }
    }

    pub(crate) fn write(&mut self, file: &mut File, bytes: &[u8]) -> io::Result<usize> {
        use std::io::Write;
        self.check()?;
        if self.inspection || self.ended || bytes.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if self.completion || bytes.len() as u64 > self.remaining {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        #[cfg(test)]
        let bytes = if let Some(limit) = native_io_tests::MAX_WRITE.with(|limit| limit.take()) {
            &bytes[..bytes.len().min(limit)]
        } else {
            bytes
        };
        match file.write(bytes) {
            Ok(written) => {
                self.remaining -= written as u64;
                Ok(written)
            }
            Err(error) => {
                self.remaining = 0;
                self.ended = true;
                Err(error)
            }
        }
    }
}

/// Validate a relative locator before resolving it through a held directory.
pub(crate) fn validate_relative(path: &std::path::Path, allow_empty: bool) -> io::Result<()> {
    use std::path::Component;
    let bytes = path.as_os_str().as_encoded_bytes();
    if bytes.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        };
    }
    if bytes.contains(&0) || path.is_absolute() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let separator = if cfg!(windows) { b'\\' } else { b'/' };
    if bytes
        .split(|byte| *byte == separator)
        .any(|name| matches!(name, b"" | b"." | b".."))
        || (cfg!(windows) && bytes.iter().any(|byte| matches!(byte, b'/' | b':')))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
pub(crate) fn native_name_bytes(name: &std::ffi::OsStr) -> io::Result<usize> {
    #[cfg(unix)]
    {
        Ok(name.as_encoded_bytes().len())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        name.encode_wide()
            .count()
            .checked_mul(2)
            .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = name;
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
pub(crate) mod native_io_tests {
    use super::*;
    use std::{io::Write, time::Duration};
    thread_local! { pub(crate) static MAX_WRITE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) }; }
    fn context() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ))
    }
    #[test]
    fn clones_keep_request_identity_without_accepting_an_equal_fresh_context() {
        let original = context();
        let clone = Arc::clone(&original);
        let fresh = SnapshotContext::new(
            original.required_deadline().unwrap(),
            original.cancellation().clone(),
        );
        assert!(original.same_request(&clone));
        assert!(!original.same_request(&fresh));
    }
    #[test]
    fn cancellation_and_deadline_reject_normal_native_work() {
        let context = context();
        context.cancellation().cancel();
        assert_eq!(
            context.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            context.native_inspection(0).err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
        assert_eq!(expired.check().unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(
            expired.required_deadline().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }
    #[test]
    fn service_context_retains_identity_and_cancels_without_a_deadline() {
        let original = Arc::new(SnapshotContext::service(Cancellation::default()));
        assert_eq!(original.deadline(), None);
        original.check().unwrap();
        assert_eq!(
            original.required_deadline().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(original.same_request(&Arc::clone(&original)));
        assert!(!original.same_request(&SnapshotContext::service(original.cancellation().clone())));
        original.cancellation().cancel();
        assert_eq!(
            original.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            original.required_deadline().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn bounded_completion_reads_actual_content_after_cancellation() {
        let context = context();
        let document = context.native_document_io(3, Some(2)).unwrap();
        context.cancellation().cancel();
        assert_eq!(
            document.normal(3).err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"abc").unwrap();
        std::io::Seek::rewind(&mut file).unwrap();
        let mut pass = document.complete_stage();
        pass.begin_inspection(4).unwrap();
        let mut bytes = [0u8; 4];
        assert_eq!(pass.read(&mut file, &mut bytes).unwrap(), 3);
        assert_eq!(&bytes[..3], b"abc");
        assert_eq!(pass.read(&mut file, &mut bytes[..1]).unwrap(), 0);
    }
}
