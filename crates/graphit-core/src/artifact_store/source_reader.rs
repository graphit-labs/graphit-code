//! Positional, bounded reads of an actual captured source; no writing or grants.
use super::{SnapshotContext, SnapshotNode, SnapshotPolicy, SourceSnapshot};
use graphit_fs::NodeKind;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
    sync::Arc,
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

/// Content manifest and ordinary opened files, under the actual I/O request.
pub struct CapturedSourceReader {
    shared: Arc<Shared>,
}
impl CapturedSourceReader {
    fn new(
        snapshot: SourceSnapshot,
        limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        if snapshot.source().to_str().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source root is not Unicode",
            ));
        }
        Ok(Self {
            shared: Arc::new(Shared {
                snapshot,
                context,
                limits,
            }),
        })
    }
    pub fn capture_selected_project(
        scope: &super::PublicationScope,
        criteria: super::ProjectSelectionCriteria<'_>,
        selection_limits: super::ProjectSelectionLimits,
        limits: SourceReadLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let snapshot = super::native_source::capture(scope, criteria, selection_limits, request)?;
        Self::new(snapshot, limits, Arc::clone(request))
    }
    pub fn capture_selected_project_metadata(
        scope: &super::PublicationScope,
        criteria: super::ProjectSelectionCriteria<'_>,
        selection_limits: super::ProjectSelectionLimits,
        limits: SourceReadLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let snapshot =
            super::native_source::capture_metadata(scope, criteria, selection_limits, request)?;
        Self::new(snapshot, limits, Arc::clone(request))
    }
    pub fn capture_selected_project_changes_metadata(
        scope: &super::PublicationScope,
        criteria: super::ProjectSelectionCriteria<'_>,
        changes: &std::collections::BTreeSet<std::path::PathBuf>,
        selection_limits: super::ProjectSelectionLimits,
        limits: SourceReadLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let snapshot = super::native_source::capture_changes_metadata(
            scope,
            criteria,
            changes,
            selection_limits,
            request,
        )?;
        Self::new(snapshot, limits, Arc::clone(request))
    }
    pub fn covers_selected_path(&self, relative: &Path) -> io::Result<bool> {
        self.shared.snapshot.covers_selected_path(relative)
    }
    pub fn has_payload_digests(&self) -> bool {
        self.shared.snapshot.has_payload_digests()
    }
    /// Compare supplied FILE bytes with the recorded content digest.
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
        original.check()?;
        let (_, node) = self
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
        if bytes.len() as u64 != *expected {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut observed = Sha256::new();
        for chunk in bytes.chunks(64 * 1024) {
            original.check()?;
            observed.update(chunk);
        }
        if <[u8; 32]>::from(observed.finalize()) != *digest {
            return Err(io::ErrorKind::InvalidData.into());
        }
        original.check()
    }
    pub fn capture_project(
        scope: &super::PublicationScope,
        relative: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        if !matches!(scope.owner(), super::StoreOwner::Project { .. }) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        super::snapshot::validate_relative(relative)?;
        Self::capture(
            &scope.owner_root().join(relative),
            kind,
            policy,
            limits,
            Arc::clone(request),
        )
    }
    pub fn capture(
        source: &Path,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let snapshot = SourceSnapshot::capture(source, kind, policy, &context)?;
        Self::new(snapshot, limits, context)
    }
    pub fn capture_generated(
        source: &Path,
        kind: NodeKind,
        limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let snapshot = SourceSnapshot::capture_generated(source, kind, &context)?;
        Self::new(snapshot, limits, context)
    }
    /// Declared completed output, with lengths but no invented payload digests.
    pub fn capture_generated_metadata(
        source: &Path,
        files: &[std::path::PathBuf],
        limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let snapshot = SourceSnapshot::capture_generated_metadata(source, files, &context)?;
        Self::new(snapshot, limits, context)
    }
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
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = SnapshotNode<'_>> {
        self.shared.snapshot.nodes()
    }
    pub fn source_path(&self, context: &SnapshotContext) -> io::Result<&Path> {
        context.check()?;
        self.shared.context.check()?;
        Ok(self.shared.snapshot.source())
    }
    pub fn open_file(&self, relative: &Path) -> io::Result<CapturedSourceFile> {
        self.shared.context.check()?;
        super::snapshot::validate_relative(relative)?;
        let plan = self.shared.snapshot.plan();
        let (node_index, node) = plan.node(relative).ok_or(io::ErrorKind::NotFound)?;
        if node.content.directory() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        // Reject an already-present special file before an ordinary blocking open.
        // This is a point-in-time type check, not a pathname replacement guarantee.
        if !std::fs::metadata(super::tree::node_path(plan.source(), relative))?.is_file() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.shared.context.check()?;
        let fd = plan.open_file(node)?;
        let metadata = fd.metadata()?;
        if !metadata.is_file() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.shared.context.check()?;
        Ok(CapturedSourceFile {
            file: Arc::new(OpenFile {
                fd,
                shared: Arc::clone(&self.shared),
                length: metadata.len(),
                node_index,
            }),
        })
    }
}
struct OpenFile {
    fd: File,
    shared: Arc<Shared>,
    length: u64,
    node_index: usize,
}
impl OpenFile {
    fn read(&self, start: u64, buffer: &mut [u8]) -> io::Result<usize> {
        self.shared.context.check()?;
        let read = ranges::read_chunk(&self.fd, buffer, start)?;
        self.shared.context.check()?;
        Ok(read)
    }
}
/// One ordinary open FILE; positional reads share it without changing its cursor.
#[derive(Clone)]
pub struct CapturedSourceFile {
    file: Arc<OpenFile>,
}
impl CapturedSourceFile {
    pub fn len(&self) -> u64 {
        self.file.length
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn last_modified(&self) -> io::Result<SystemTime> {
        self.file.shared.context.check()?;
        let modified = self.file.fd.metadata()?.modified()?;
        self.file.shared.context.check()?;
        Ok(modified)
    }
    pub fn read_range(&self, start: u64, length: usize) -> io::Result<RetainedSourceRange> {
        let request = SourceRangeRequest::new(self, start, length)?;
        let mut result = ranges::read_ranges(
            &self.file.shared,
            &[request],
            AggregateReadLimits::new(1, length.max(1))?,
        )?;
        result
            .pop()
            .ok_or_else(|| io::Error::other("source range result missing"))
    }
    pub fn stream(&self, start: u64) -> io::Result<SourceRangeStream> {
        self.file.shared.context.check()?;
        if start > self.len() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(SourceRangeStream {
            file: Arc::clone(&self.file),
            position: start,
            start,
            integrity: if start == 0 && self.file.shared.snapshot.has_payload_digests() {
                StreamIntegrity::Hashing(Sha256::new())
            } else {
                StreamIntegrity::Unchecked
            },
        })
    }
}
pub struct SourceRangeStream {
    file: Arc<OpenFile>,
    position: u64,
    start: u64,
    integrity: StreamIntegrity,
}
enum StreamIntegrity {
    Unchecked,
    Hashing(Sha256),
    Verified,
    Failed(io::ErrorKind),
}
impl SourceRangeStream {
    fn finish_content(&mut self) -> io::Result<()> {
        let StreamIntegrity::Hashing(_) = &self.integrity else {
            return Ok(());
        };
        let StreamIntegrity::Hashing(observed) = std::mem::replace(
            &mut self.integrity,
            StreamIntegrity::Failed(io::ErrorKind::InvalidData),
        ) else {
            unreachable!()
        };
        let node = self
            .file
            .shared
            .snapshot
            .plan()
            .indexed_node(self.file.node_index)?;
        let super::tree::Content::File { bytes, digest } = &node.content else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        if self.position != *bytes || <[u8; 32]>::from(observed.finalize()) != *digest {
            return Err(io::ErrorKind::InvalidData.into());
        }
        // Observe EOF as part of this read, including growth after File::open.
        // A completed stream never reopens or revalidates its former pathname.
        match self.file.read(self.position, &mut [0; 1]) {
            Ok(0) => self.integrity = StreamIntegrity::Verified,
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(error) => {
                self.integrity = StreamIntegrity::Failed(error.kind());
                return Err(error);
            }
        }
        Ok(())
    }
}
impl Read for SourceRangeStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.shared.context.check()?;
        if let StreamIntegrity::Failed(kind) = self.integrity {
            return Err(kind.into());
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.position == self.file.length {
            self.finish_content()?;
            return Ok(0);
        }
        let mut desired = (buffer.len().min(self.file.shared.limits.block_bytes) as u64)
            .min(self.file.length - self.position);
        if let Some(limit) = self.file.shared.limits.stream_bytes {
            let remaining = limit
                .checked_sub(self.position - self.start)
                .filter(|n| *n != 0)
                .ok_or(io::ErrorKind::FileTooLarge)?;
            desired = desired.min(remaining);
        }
        let read = self
            .file
            .read(self.position, &mut buffer[..desired as usize])?;
        self.position = self
            .position
            .checked_add(read as u64)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if read == 0 {
            self.integrity = StreamIntegrity::Failed(io::ErrorKind::UnexpectedEof);
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if let StreamIntegrity::Hashing(observed) = &mut self.integrity {
            observed.update(&buffer[..read]);
        }
        if self.position == self.file.length {
            self.finish_content()?;
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
