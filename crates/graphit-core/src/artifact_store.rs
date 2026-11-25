//! Artifact installation records. Content hashes protect user edits; files use normal OS paths.
use crate::{
    file_lock::{Cancellation, FileLock},
    local_files::check_cancelled,
};
use graphit_fs::{NodeKind, create_link};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
mod catalog;
mod codec;
pub(crate) mod native_source;
mod payload_plan;
mod publication;
mod publication_slot;
mod scopes;
mod selection;
mod snapshot;
mod source_reader;
mod tree;
use crate::native_path::valid_path;
use codec::Document;
pub use codec::{ArtifactEntry, GeneratedOrigin};
pub use native_source::{
    ProjectIgnoreCriteria, ProjectSelectionCriteria, ProjectSelectionLimits,
    selected_project_changed_paths, selected_project_paths,
};
pub use payload_plan::{GeneratedNode, PayloadOverlay, PayloadPolicy, ValidatedPayloadPlan};
pub use publication_slot::PublicationSlot;
pub use scopes::{PublicationScope, StoreOwner, UserIdentity};
pub use selection::PayloadSelection;
pub use snapshot::{
    PayloadMode, RetainedBytesLimits, RetainedFileSet, SnapshotContext, SnapshotNode,
    SnapshotPolicy, SourceSnapshot,
};
pub use source_reader::{
    AggregateReadLimits, CapturedSourceFile, CapturedSourceReader, RetainedSourceRange,
    SourceRangeRequest, SourceRangeStream, SourceReadLimits,
};
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactKey {
    adapter: String,
    artifact_type: String,
    id: String,
}
impl ArtifactKey {
    pub fn new(adapter: &str, artifact_type: &str, id: &str) -> io::Result<Self> {
        Self::validate_parts([adapter, artifact_type, id], 64 * 1024)?;
        let key = Self {
            adapter: adapter.into(),
            artifact_type: artifact_type.into(),
            id: id.into(),
        };
        Ok(key)
    }
    fn validate(&self, limit: usize) -> io::Result<()> {
        Self::validate_parts([&self.adapter, &self.artifact_type, &self.id], limit)
    }
    fn validate_parts(parts: [&str; 3], limit: usize) -> io::Result<()> {
        if parts
            .iter()
            .try_fold(0usize, |n, p| n.checked_add(p.len()))
            .is_none_or(|n| n > limit)
            || parts
                .iter()
                .any(|p| p.is_empty() || p.chars().any(char::is_control))
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Ok(())
    }
    pub fn adapter(&self) -> &str {
        &self.adapter
    }
    pub fn artifact_type(&self) -> &str {
        &self.artifact_type
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl fmt::Debug for ArtifactKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactKey { .. }")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactLimits {
    max_document_bytes: usize,
    max_key_bytes: usize,
    copy: CopyLimits,
}
impl ArtifactLimits {
    pub fn copy_limits(self) -> CopyLimits {
        self.copy
    }

    /// Validate a planned key against this caller's captured budget.
    pub fn validate_key(&self, key: &ArtifactKey) -> io::Result<()> {
        key.validate(self.max_key_bytes)
    }

    pub fn new(
        max_document_bytes: usize,
        max_key_bytes: usize,
        copy: CopyLimits,
    ) -> io::Result<Self> {
        if !(1..=16 * 1024 * 1024).contains(&max_document_bytes)
            || !(1..=64 * 1024).contains(&max_key_bytes)
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Ok(Self {
            max_document_bytes,
            max_key_bytes,
            copy,
        })
    }
}

/// Bounds file content and entry counts for each copied payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyLimits {
    max_total_file_bytes: u64,
    max_tree_nodes: usize,
}
impl CopyLimits {
    /// Maximum sum of file-content bytes in one copied tree.
    pub fn total_file_bytes_limit(self) -> u64 {
        self.max_total_file_bytes
    }

    pub fn node_limit(self) -> usize {
        self.max_tree_nodes
    }

    pub fn new(bytes: u64, nodes: usize) -> io::Result<Self> {
        if bytes == 0 || nodes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            max_total_file_bytes: bytes,
            max_tree_nodes: nodes,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyPolicy {
    RejectEntryLinks,
    FollowLinks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactStage {
    Validate,
    Lock,
    Read,
    Prepare,
    Create,
    Commit,
    Conflict,
    SourceChanged,
    Publish,
    Detach,
    InvalidPolicy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactEffect {
    NotApplied,
    MayHaveApplied,
    Applied,
}
#[derive(Clone, Copy, Debug)]
pub struct ArtifactError {
    stage: ArtifactStage,
    kind: io::ErrorKind,
    effect: ArtifactEffect,
}
impl ArtifactError {
    pub fn stage(&self) -> ArtifactStage {
        self.stage
    }
    pub fn kind(&self) -> io::ErrorKind {
        self.kind
    }
    pub fn effect(&self) -> ArtifactEffect {
        self.effect
    }
}
impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "artifact operation failed at {:?}: {:?} (effect: {:?})",
            self.stage, self.kind, self.effect
        )
    }
}
impl std::error::Error for ArtifactError {}
type ArtifactResult<T> = Result<T, ArtifactError>;
fn at(stage: ArtifactStage, error: io::Error) -> ArtifactError {
    ArtifactError {
        stage,
        kind: error.kind(),
        effect: ArtifactEffect::NotApplied,
    }
}
fn error(stage: ArtifactStage, kind: io::ErrorKind) -> ArtifactError {
    at(stage, kind.into())
}
fn conflict() -> ArtifactError {
    error(ArtifactStage::Conflict, io::ErrorKind::AlreadyExists)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationCheck {
    VerifiedUnchanged,
    NeedsPublication,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ArtifactMode {
    Link,
    Copy(CopyPolicy),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactChange {
    Created,
    Unchanged,
    Removed,
    Updated,
}
#[derive(Debug)]
pub struct ArtifactSnapshot {
    owner: StoreOwner,
    entries: Vec<ArtifactEntry>,
}
impl ArtifactSnapshot {
    pub fn owner(&self) -> &StoreOwner {
        &self.owner
    }
    pub fn entries(&self) -> &[ArtifactEntry] {
        &self.entries
    }
}
#[derive(Clone, Debug)]
pub struct ArtifactStore {
    scope: PublicationScope,
    limits: ArtifactLimits,
}
impl ArtifactStore {
    pub fn new(scope: PublicationScope, limits: ArtifactLimits) -> Self {
        Self { scope, limits }
    }
    pub fn scope(&self) -> &PublicationScope {
        &self.scope
    }
    pub fn owner(&self) -> &StoreOwner {
        self.scope.owner()
    }
    pub fn runtime_root(&self) -> &Path {
        self.scope.runtime_root()
    }
    pub fn limits(&self) -> ArtifactLimits {
        self.limits
    }
    pub fn manifest_path(&self) -> PathBuf {
        self.runtime_root().join("artifact-store.json")
    }
    fn destination_scope(&self, path: &Path) -> io::Result<()> {
        self.scope.destination(path)
    }
    pub fn load(&self) -> ArtifactResult<Option<ArtifactSnapshot>> {
        catalog::read(self)
            .map(|document| {
                document.map(|d| ArtifactSnapshot {
                    owner: d.owner,
                    entries: d.entries,
                })
            })
            .map_err(|e| at(ArtifactStage::Read, e))
    }
    pub fn writer_with_context(
        &self,
        request: &Arc<SnapshotContext>,
    ) -> ArtifactResult<ArtifactWriter> {
        request
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        Ok(ArtifactWriter {
            store: self.clone(),
            context: Arc::clone(request),
        })
    }
    pub fn writer(
        &self,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> ArtifactResult<ArtifactWriter> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        self.writer_with_context(&Arc::new(SnapshotContext::new(
            deadline,
            cancellation.clone(),
        )))
    }
}
/// The operation context holds no directory handles or installation-wide lock.
pub struct ArtifactWriter {
    store: ArtifactStore,
    context: Arc<SnapshotContext>,
}
impl fmt::Debug for ArtifactWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtifactWriter")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}
impl ArtifactWriter {
    pub fn scope(&self) -> &PublicationScope {
        self.store.scope()
    }
    pub fn with_io_context(mut self, context: Arc<SnapshotContext>) -> ArtifactResult<Self> {
        context
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.context = context;
        Ok(self)
    }
    pub fn load(&self) -> ArtifactResult<Option<ArtifactSnapshot>> {
        self.context
            .check()
            .map_err(|e| at(ArtifactStage::Read, e))?;
        self.store.load()
    }
    fn document(&self) -> ArtifactResult<Document> {
        catalog::read(&self.store)
            .map_err(|e| at(ArtifactStage::Read, e))
            .map(|d| {
                d.unwrap_or_else(|| Document {
                    owner: self.store.owner().clone(),
                    entries: Vec::new(),
                })
            })
    }
    fn lock(&self) -> ArtifactResult<FileLock> {
        self.context
            .check()
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        fs::create_dir_all(self.store.runtime_root()).map_err(|e| at(ArtifactStage::Lock, e))?;
        FileLock::acquire_in_existing_parent_until(
            &self.store.runtime_root().join("artifact-store.lock"),
            self.context.deadline(),
            self.context.cancellation(),
        )
        .map_err(|e| at(ArtifactStage::Lock, e))
    }
    fn save(&self, document: &Document, applied: bool) -> ArtifactResult<()> {
        catalog::save(&self.store, document).map_err(|e| ArtifactError {
            effect: if applied {
                ArtifactEffect::Applied
            } else {
                ArtifactEffect::NotApplied
            },
            ..at(ArtifactStage::Commit, e)
        })
    }
    pub fn ensure_publication_parent(&self, destination: &Path) -> ArtifactResult<()> {
        self.context
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.store
            .destination_scope(destination)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?,
        )
        .map_err(|e| at(ArtifactStage::Create, e))
    }
    fn entry<'a>(&self, document: &'a Document, key: &ArtifactKey) -> Option<&'a ArtifactEntry> {
        document
            .entries
            .iter()
            .find(|e| e.publication_slot() == self.publication_slot() && e.key() == key)
    }
    pub fn verify_owned_copy(
        &self,
        key: &ArtifactKey,
        destination: &Path,
        kind: NodeKind,
    ) -> ArtifactResult<bool> {
        let document = self.document()?;
        let Some(entry) = self.entry(&document, key) else {
            return Ok(false);
        };
        if entry.destination() != destination
            || entry.kind() != kind
            || !matches!(entry.mode(), ArtifactMode::Copy(_))
        {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        self.verify(entry)?;
        Ok(true)
    }
    pub fn forget(&mut self, key: &ArtifactKey) -> ArtifactResult<ArtifactEntry> {
        let _lock = self.lock()?;
        let mut document = self.document()?;
        let index = document
            .entries
            .iter()
            .position(|e| e.publication_slot() == self.publication_slot() && e.key() == key)
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::NotFound))?;
        let entry = document.entries.remove(index);
        self.save(&document, false)?;
        Ok(entry)
    }
}
#[cfg(test)]
mod tests;
