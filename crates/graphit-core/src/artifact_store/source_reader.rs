//! Positional, bounded reads of an actual captured source; no writing or grants.
use super::{SnapshotBindingRecord, SnapshotContext, SnapshotNode, SnapshotPolicy, SourceSnapshot};
use graphit_fs::NodeKind;
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
    sync::{Arc, RwLock, RwLockReadGuard},
    time::SystemTime,
};

mod ranges;
pub use ranges::{AggregateReadLimits, RetainedSourceRange, SourceRangeRequest};

/// Positive bounds for returned ranges and read chunks. Input streams may
/// additionally bound content; generated output streams to its captured EOF.
#[derive(Clone, Copy)]
pub struct SourceReadLimits {
    range_bytes: usize,
    block_bytes: usize,
    stream_bytes: Option<u64>,
}
impl SourceReadLimits {
    pub fn new(range_bytes: usize, block_bytes: usize, stream_bytes: u64) -> io::Result<Self> {
        if range_bytes == 0 || block_bytes == 0 || stream_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            range_bytes,
            block_bytes,
            stream_bytes: Some(stream_bytes),
        })
    }
    /// Completed local output streams to its captured EOF. Range and chunk
    /// sizes still bound each returned allocation and filesystem read.
    pub fn generated(range_bytes: usize, block_bytes: usize) -> io::Result<Self> {
        if range_bytes == 0 || block_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            range_bytes,
            block_bytes,
            stream_bytes: None,
        })
    }
}
struct Shared {
    snapshot: SourceSnapshot,
    context: Arc<SnapshotContext>,
    limits: SourceReadLimits,
}

/// Owns the real snapshot and original request; data cannot restore it.
/// ```compile_fail
/// # use graphit_core::artifact_store::CapturedSourceReader;
/// fn restore(binding: graphit_core::artifact_store::SnapshotBindingRecord) {
///     let _ = CapturedSourceReader::from_binding(binding);
/// }
/// ```
/// ```compile_fail
/// # use graphit_core::artifact_store::CapturedSourceReader;
/// fn inspect(reader: &CapturedSourceReader) { println!("{:?}", reader); }
/// ```
pub struct CapturedSourceReader {
    shared: Arc<Shared>,
}
impl CapturedSourceReader {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) fn from_native_snapshot(
        snapshot: SourceSnapshot,
        limits: SourceReadLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        request.check()?;
        let reader = Self {
            shared: Arc::new(Shared {
                snapshot,
                context: Arc::clone(request),
                limits,
            }),
        };
        request.check()?;
        Ok(reader)
    }
    /// Capture only documents selected under the original PROJECT request.
    /// Criteria cannot restore a scope, reader or native authority.
    pub fn capture_selected_project(
        scope: &super::CapturedPublicationScope,
        criteria: super::ProjectSelectionCriteria<'_>,
        selection_limits: super::ProjectSelectionLimits,
        limits: SourceReadLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        scope.require_project()?;
        request.check()?;
        let snapshot = super::native_source::NativeSource::capture_selected(
            scope,
            criteria,
            selection_limits,
            request,
        )?;
        let reader = Self {
            shared: Arc::new(Shared {
                snapshot,
                context: Arc::clone(request),
                limits,
            }),
        };
        request.check()?;
        Ok(reader)
    }
    /// Capture the same native PROJECT selection and hashes without retaining file bodies.
    pub fn capture_selected_project_metadata(
        scope: &super::CapturedPublicationScope,
        criteria: super::ProjectSelectionCriteria<'_>,
        selection_limits: super::ProjectSelectionLimits,
        limits: SourceReadLimits,
        request: &Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        scope.require_project()?;
        request.check()?;
        let snapshot = super::native_source::NativeSource::capture_selected_metadata(
            scope,
            criteria,
            selection_limits,
            request,
        )?;
        let reader = Self {
            shared: Arc::new(Shared {
                snapshot,
                context: Arc::clone(request),
                limits,
            }),
        };
        request.check()?;
        Ok(reader)
    }
    /// Capture only paths affected by a genuine event within the same PROJECT
    /// selection policy. Missing paths and directory events retain coverage;
    /// unobserved sibling payloads are neither opened nor hashed.
    pub fn capture_selected_project_changes_metadata(
        scope: &super::CapturedPublicationScope,
        criteria: super::ProjectSelectionCriteria<'_>,
        changes: &std::collections::BTreeSet<std::path::PathBuf>,
        selection_limits: super::ProjectSelectionLimits,
        limits: SourceReadLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        scope.require_project()?;
        request.check()?;
        let snapshot = super::native_source::NativeSource::capture_selected_changes_metadata(
            scope,
            criteria,
            changes,
            selection_limits,
            request,
        )?;
        let reader = Self {
            shared: Arc::new(Shared {
                snapshot,
                context: Arc::clone(request),
                limits,
            }),
        };
        request.check()?;
        Ok(reader)
    }
    /// Whether absence from this capture answers the selection question for
    /// `relative`. This is not a claim that a physical file does not exist.
    pub fn covers_selected_path(&self, relative: &Path) -> io::Result<bool> {
        self.shared.context.check()?;
        self.shared
            .snapshot
            .native()
            .ok_or(io::ErrorKind::InvalidInput)?
            .covers_selected_path(relative)
    }
    /// Validate one complete FILE read against this capture's actual length and digest.
    /// Bytes or a relative path cannot reconstruct the selected reader or its request.
    pub fn validate_file_bytes(
        &self,
        relative: &Path,
        bytes: &[u8],
        original: &SnapshotContext,
    ) -> io::Result<()> {
        use sha2::{Digest, Sha256};
        if !self.shared.snapshot.has_payload_digests() {
            return Err(io::ErrorKind::Unsupported.into());
        }
        self.check_request(original)?;
        self.shared.snapshot.validate_reader_relative(relative)?;
        let (index, node) = self
            .shared
            .snapshot
            .plan()
            .node(relative)
            .ok_or(io::ErrorKind::NotFound)?;
        let super::tree::Content::File {
            bytes: expected,
            digest,
        } = &node.content
        else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        if u64::try_from(bytes.len()).map_err(|_| io::ErrorKind::FileTooLarge)? != *expected {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.shared
            .snapshot
            .check_reader_binding(index, None, original)?;
        let mut observed = Sha256::new();
        for chunk in bytes.chunks(64 * 1024) {
            original.check()?;
            observed.update(chunk);
        }
        if <[u8; 32]>::from(observed.finalize()) != *digest {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.shared
            .snapshot
            .check_reader_binding(index, None, original)?;
        self.check_request(original)
    }
    pub fn capture_project(
        expected_scope: &super::CapturedPublicationScope,
        relative_selection: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        expected_scope.require_project()?;
        Self::capture_native_scope(
            expected_scope,
            relative_selection,
            kind,
            policy,
            limits,
            request,
        )
    }
    /// Internal physical capture; membership and Auth remain the caller's job.
    /// ```compile_fail
    /// use graphit_core::artifact_store::{CapturedPublicationScope, CapturedSourceReader, SnapshotPolicy, SourceReadLimits};
    ///
    /// use graphit_fs::NodeKind;
    /// use std::path::Path;
    /// fn capture(scope: &CapturedPublicationScope, policy: &SnapshotPolicy, limits: SourceReadLimits, request: &std::sync::Arc<crate::file_io::SnapshotContext>) {
    ///     let _ = CapturedSourceReader::capture_native_scope(scope, Path::new("runtime/payload"), NodeKind::Directory, policy, limits, request);
    /// }
    /// ```
    pub(super) fn capture_native_scope(
        expected_scope: &super::CapturedPublicationScope,
        relative_selection: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        request.check()?;
        let snapshot = SourceSnapshot::capture_native_scope(
            expected_scope,
            relative_selection,
            kind,
            policy,
            request,
        )?;
        let context = Arc::clone(request);
        let reader = Self {
            shared: Arc::new(Shared {
                snapshot,
                context,
                limits,
            }),
        };
        request.check()?;
        Ok(reader)
    }
    pub fn capture(
        source: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        context.check()?;
        let snapshot = SourceSnapshot::capture(source, kind, policy, &context)?;
        context.check()?;
        let reader = Self {
            shared: Arc::new(Shared {
                snapshot,
                context,
                limits,
            }),
        };
        reader.shared.context.check()?;
        Ok(reader)
    }
    /// Capture a completed locally produced artifact. The caller owns and
    /// authorizes its source; this does not grant access to arbitrary paths.
    /// No tree quota or retained copy of the corpus is introduced.
    pub fn capture_generated(
        source: &Path,
        kind: NodeKind,
        limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        context.check()?;
        let snapshot = SourceSnapshot::capture_generated(source, kind, &context)?;
        context.check()?;
        let reader = Self {
            shared: Arc::new(Shared {
                snapshot,
                context,
                limits,
            }),
        };
        reader.shared.context.check()?;
        Ok(reader)
    }
    /// Capture exactly the declared files of completed owned output, plus their
    /// ancestor bindings. This reads metadata only: no tree walk or payload hash.
    /// The caller owns/authorizes the root and declaration; neither grants access.
    /// SnapshotNode::digest is None and content revalidation is unsupported.
    pub fn capture_generated_metadata(
        source: &Path,
        files: &[std::path::PathBuf],
        limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let snapshot = SourceSnapshot::capture_generated_metadata(source, files, &context)?;
        Ok(Self {
            shared: Arc::new(Shared {
                snapshot,
                context,
                limits,
            }),
        })
    }
    /// Read an ordered plan from this exact captured reader. Failed batches
    /// return no partial ranges.
    pub fn read_ranges(
        &self,
        requests: &[SourceRangeRequest<'_>],
        limits: AggregateReadLimits,
    ) -> io::Result<Vec<RetainedSourceRange>> {
        ranges::read_ranges(&self.shared, requests, limits)
    }
    pub fn retained(&self, relative: &Path) -> Option<&[u8]> {
        self.shared.snapshot.retained(relative)
    }
    pub fn retained_bytes(&self) -> usize {
        self.shared.snapshot.retained_bytes()
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        self.shared.snapshot.fingerprint()
    }
    /// Compare stored binding data with this retained snapshot without I/O or admission.
    /// Callers still recheck their authority and the actual captured reader before access.
    pub fn matches_binding(&self, binding: &SnapshotBindingRecord) -> bool {
        self.shared.snapshot.has_payload_digests()
            && binding.matches_snapshot(&self.shared.snapshot)
    }
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = SnapshotNode<'_>> {
        self.shared.snapshot.nodes()
    }
    pub fn check_request(&self, context: &SnapshotContext) -> io::Result<()> {
        self.shared.context.check()?;
        context.check()?;
        if !self.shared.context.same_request(context) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.shared.context.check()
    }
    /// Borrow the real captured location for pathname-only SDK readers.
    /// The caller retains this owner and revalidates bindings around SDK IO.
    /// A location is an observation, never authority to restore a capture.
    pub fn source_path(&self, context: &SnapshotContext) -> io::Result<&Path> {
        self.check_request(context)?;
        self.revalidate_bindings()?;
        Ok(self.shared.snapshot.canonical_source())
    }
    /// Recheck inventory and physical bindings without hashing the captured corpus again.
    /// Captured content digests remain those of the original capture, not a new capture.
    pub fn revalidate_bindings(&self) -> io::Result<()> {
        self.shared.context.check()?;
        self.shared
            .snapshot
            .revalidate_bindings(&self.shared.context)?;
        self.shared.context.check()
    }
    pub fn revalidate_content(&self) -> io::Result<()> {
        self.shared.context.check()?;
        self.shared
            .snapshot
            .revalidate_content(&self.shared.context)?;
        self.shared.context.check()
    }
    /// Open this captured FILE with its local native binding and applicable controls.
    /// Global inventory/content validity requires explicit reader revalidation.
    pub fn open_file(&self, relative: &Path) -> io::Result<CapturedSourceFile> {
        self.shared.context.check()?;
        self.shared.snapshot.validate_reader_relative(relative)?;
        let plan = self.shared.snapshot.plan();
        let (index, node) = plan.node(relative).ok_or(io::ErrorKind::NotFound)?;
        if node.content.directory() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let length = node.content.bytes();
        self.shared
            .snapshot
            .check_reader_binding(index, None, &self.shared.context)?;
        let (native, fd) = match self.shared.snapshot.native() {
            Some(backing) => {
                let owner = backing.open_file(relative, plan)?;
                let fd = Arc::clone(&owner.handle.file);
                #[cfg(test)]
                let owner = {
                    let mut owner = owner;
                    NATIVE_OPENED.with(|hook| {
                        if let Some(hook) = hook.borrow_mut().take() {
                            hook(&mut owner, &fd);
                        }
                    });
                    owner
                };
                (Some(owner), fd)
            }
            None => (None, Arc::new(RwLock::new(plan.open_verified(node)?))),
        };
        self.shared.context.check()?;
        let guard = fd
            .read()
            .map_err(|_| io::Error::other("source reader lock poisoned"))?;
        self.shared
            .snapshot
            .check_reader_binding(index, Some(&guard), &self.shared.context)?;
        drop(guard);
        let captured = CapturedSourceFile {
            file: Arc::new(OpenFile {
                shared: Arc::clone(&self.shared),
                fd,
                _native: native,
                index,
                length,
            }),
        };
        self.shared.context.check()?;
        Ok(captured)
    }
}
struct OpenFile {
    fd: Arc<RwLock<File>>,
    _native: Option<super::native_source::NativeOpenedFile>,
    shared: Arc<Shared>,
    index: usize,
    length: u64,
}
impl OpenFile {
    fn read_handle(&self) -> io::Result<RwLockReadGuard<'_, File>> {
        self.shared.context.check()?;
        let guard = self.fd.try_read().map_err(|error| match error {
            std::sync::TryLockError::WouldBlock => io::Error::from(io::ErrorKind::WouldBlock),
            std::sync::TryLockError::Poisoned(_) => io::Error::other("source reader lock poisoned"),
        })?;
        self.shared.context.check()?;
        Ok(guard)
    }
    fn check(&self, fd: &File) -> io::Result<()> {
        self.shared
            .snapshot
            .check_reader_binding(self.index, Some(fd), &self.shared.context)
    }
    fn read(&self, fd: &File, start: u64, buffer: &mut [u8]) -> io::Result<usize> {
        self.check(fd)?;
        self.shared.snapshot.before_native_read()?;
        let read = ranges::read_chunk(fd, buffer, start)?;
        #[cfg(test)]
        tests::after_read(self);
        self.check(fd)?;
        Ok(read)
    }
}

/// A captured regular source file; the raw descriptor stays private.
/// ```compile_fail
/// # use graphit_core::artifact_store::CapturedSourceFile;
/// fn raw(file: CapturedSourceFile) { let _: std::fs::File = file.fd; }
/// ```
/// ```compile_fail
/// # use graphit_core::artifact_store::CapturedSourceFile;
/// fn duplicate(file: &CapturedSourceFile) { let _: CapturedSourceFile = file.clone(); }
/// ```
pub struct CapturedSourceFile {
    file: Arc<OpenFile>,
}
impl CapturedSourceFile {
    /// Historical length, without a new filesystem observation.
    pub fn len(&self) -> u64 {
        self.file.length
    }
    pub fn is_empty(&self) -> bool {
        self.file.length == 0
    }
    /// Observe the retained descriptor's modification time after validating the
    /// original request and captured binding, without reading payload bytes.
    /// Changed metadata is an error; this does not return the historical time.
    pub fn last_modified(&self) -> io::Result<SystemTime> {
        self.file.shared.context.check()?;
        let fd = self.file.read_handle()?;
        self.file.check(&fd)?;
        let modified = self
            .file
            .shared
            .snapshot
            .reader_last_modified(self.file.index, &fd)?;
        #[cfg(test)]
        tests::metadata_observed(&self.file);
        self.file.check(&fd)?;
        Ok(modified)
    }
    /// Read one bounded range with the same engine as an ordered batch.
    pub fn read_range(&self, start: u64, length: usize) -> io::Result<RetainedSourceRange> {
        let request = SourceRangeRequest::new(self, start, length)?;
        let limits = AggregateReadLimits::new(1, length.max(1))?;
        let mut result = ranges::read_ranges(&self.file.shared, &[request], limits)?;
        result
            .pop()
            .ok_or_else(|| io::Error::other("source range result missing"))
    }
    pub fn stream(&self, start: u64) -> io::Result<SourceRangeStream> {
        self.file.shared.context.check()?;
        if start > self.len() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let fd = self.file.read_handle()?;
        self.file.check(&fd)?;
        let stream = SourceRangeStream {
            file: Arc::clone(&self.file),
            position: start,
            start,
        };
        self.file.shared.context.check()?;
        Ok(stream)
    }
}
/// An owned positional stream retaining its file and original request.
/// Errors may alter the caller's buffer according to Read's ordinary contract.
/// ```compile_fail
/// # use graphit_core::artifact_store::SourceRangeStream;
/// fn duplicate(stream: &SourceRangeStream) { let _: SourceRangeStream = stream.clone(); }
/// ```
pub struct SourceRangeStream {
    file: Arc<OpenFile>,
    position: u64,
    start: u64,
}
impl Read for SourceRangeStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.shared.context.check()?;
        let fd = self.file.read_handle()?;
        self.file.check(&fd)?;
        if buffer.is_empty() || self.position == self.file.length {
            self.file.check(&fd)?;
            return Ok(0);
        }
        let mut desired = (buffer.len().min(self.file.shared.limits.block_bytes) as u64)
            .min(self.file.length - self.position);
        if let Some(limit) = self.file.shared.limits.stream_bytes {
            let remaining = limit
                .checked_sub(self.position - self.start)
                .filter(|remaining| *remaining != 0)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            desired = desired.min(remaining);
        }
        let desired = desired as usize;
        // Preserve the actual offset even if final physical validation fails.
        self.file.shared.snapshot.before_native_read()?;
        let read = positional_read(&fd, &mut buffer[..desired], self.position)?;
        self.position = self
            .position
            .checked_add(read as u64)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        #[cfg(test)]
        tests::after_read(&self.file);
        self.file.check(&fd)?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(read)
    }
}
fn positional_read(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_at(buffer, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        file.seek_read(buffer, offset)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (file, buffer, offset);
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
type NativeOpenHook =
    Box<dyn FnOnce(&mut super::native_source::NativeOpenedFile, &Arc<RwLock<File>>)>;
#[cfg(test)]
thread_local! {
    pub(super) static NATIVE_OPENED: std::cell::RefCell<Option<NativeOpenHook>> = const { std::cell::RefCell::new(None) };
}
