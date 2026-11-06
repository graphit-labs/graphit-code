//! Local link/copy membership with one durable intent and cooperative physical receipts.
//!
//! Only physically proven owned artifacts can be replaced. Recovery without persisted
//! proof preserves an ambiguous entry. OS identifiers are observations, not a
//! security boundary against hostile mutation or identifier reuse. This store
//! does not publish Hub contexts or provide a two-resource atomic transaction.

use crate::file_lock::{Cancellation, FileLock};
use crate::local_files::{check_cancelled, directory_exists, regular_metadata};
use crate::project_identity::ProjectId;
use crate::project_paths::ProjectPaths;
use graphit_fs::{LinkFailure, LinkPlan, NodeKind};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

mod catalog;
mod codec;
mod control;
mod decode;
mod native_document;
pub(crate) mod native_source;
mod payload_plan;
mod publication;
mod publication_slot;
mod receipt;
mod scopes;
mod selection;
mod snapshot;
mod snapshot_binding;
mod source_reader;
mod tree;
use crate::native_path::{NativePath, absolute_input, valid_path};
pub use codec::{ArtifactEntry, GeneratedOrigin};
use codec::{Candidate, Document, Pending, Proof};
pub use native_document::{
    NativeDocumentCompletion, NativeDocumentFailure, NativeDocumentIntent,
    NativeDocumentOperationId, NativeDocumentOutcome, NativeDocumentPhase, NativeDocumentRecovery,
    NativeDocumentRecoveryRequest, NativeDocumentResolution, NativeDocumentStage,
};
pub use native_source::{
    ProjectIgnoreCriteria, ProjectSelectionCriteria, ProjectSelectionLimits,
    selected_project_changed_paths, selected_project_paths,
};
pub use payload_plan::{GeneratedNode, PayloadOverlay, PayloadPolicy, ValidatedPayloadPlan};
pub use publication_slot::PublicationSlot;
pub use scopes::{
    CapturedPublicationScope, OwnerRootBindingLimits, OwnerRootBindingRecord, OwnerRootIdentity,
    OwnerRootMode, ScopeBindingRecord, ScopeContext, ScopeLimits, StoreOwner, UserIdentity,
};
pub use selection::PayloadSelection;
pub use snapshot::{
    PayloadMode, RetainedBytesLimits, RetainedFileSet, SnapshotContext, SnapshotNode,
    SnapshotPolicy, SourceSnapshot,
};
pub use snapshot_binding::SnapshotBindingRecord;
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

/// Captured resource limits for each copied tree; all values are mandatory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyLimits {
    max_total_file_bytes: u64,
    max_tree_nodes: usize,
    max_total_relative_path_bytes: usize,
    max_depth: usize,
}
impl CopyLimits {
    /// Maximum sum of file-content bytes in one copied tree.
    pub fn total_file_bytes_limit(self) -> u64 {
        self.max_total_file_bytes
    }

    /// Maximum sum of native relative-path bytes across all nodes in one copied tree.
    pub fn total_relative_path_bytes_limit(self) -> usize {
        self.max_total_relative_path_bytes
    }

    /// Maximum relative-path component count for a node in one copied tree.
    pub fn depth_limit(self) -> usize {
        self.max_depth
    }

    pub fn node_limit(self) -> usize {
        self.max_tree_nodes
    }

    pub fn new(bytes: u64, nodes: usize, path_bytes: usize, depth: usize) -> io::Result<Self> {
        if bytes == 0 || nodes == 0 || path_bytes == 0 || !(1..=256).contains(&depth) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            max_total_file_bytes: bytes,
            max_tree_nodes: nodes,
            max_total_relative_path_bytes: path_bytes,
            max_depth: depth,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyPolicy {
    RejectFileLinks,
    FollowRegularFiles,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactStage {
    Validate,
    Lock,
    Read,
    Prepare,
    Create,
    Observe,
    RecordProof,
    RecordWrapper,
    Commit,
    Conflict,
    RecoveryRequired,
    RecoveryConflict,
    IncompleteStage,
    SourceChanged,
    CopyChunk,
    Detach,
    Publish,
    Cleanup,
    CleanupRequired,
    GrantAccess,
    GrantMove,
    RecordMove,
    ClearMove,
    RestoreMove,
    RestoreAccess,
    RefreshProof,
    ObservePayload,
    BeforeRename,
    InvalidPolicy,
    CapacityExceeded,
    ScopeChanged,
}
#[derive(Clone, Copy)]
pub struct ArtifactError {
    stage: ArtifactStage,
    kind: io::ErrorKind,
    native: Option<LinkFailure>,
    secondary: Option<io::ErrorKind>,
}
impl ArtifactError {
    pub fn stage(&self) -> ArtifactStage {
        self.stage
    }
    pub fn kind(&self) -> io::ErrorKind {
        self.kind
    }
    pub fn secondary_failure(&self) -> Option<io::ErrorKind> {
        self.secondary
    }
    pub fn native_failure(&self) -> Option<LinkFailure> {
        self.native
    }
}
impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "artifact operation failed at {:?}: {:?}",
            self.stage, self.kind
        )
    }
}
impl fmt::Debug for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for ArtifactError {}
type ArtifactResult<T> = Result<T, ArtifactError>;
fn error(stage: ArtifactStage, kind: io::ErrorKind) -> ArtifactError {
    ArtifactError {
        stage,
        kind,
        native: None,
        secondary: None,
    }
}
fn at(stage: ArtifactStage, e: io::Error) -> ArtifactError {
    let mut error = error(stage, e.kind());
    error.secondary = crate::native_document_file::revalidation_secondary(&e);
    error
}
fn conflict() -> ArtifactError {
    error(ArtifactStage::Conflict, io::ErrorKind::AlreadyExists)
}

/// Observation under an existing writer, without journaling or granting access.
/// Neither outcome reserves the source or authorizes a later publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationCheck {
    VerifiedUnchanged,
    NeedsPublication,
}

/// A package or user supplied source read directly during publication.
#[derive(Clone, Copy)]
pub enum PhysicalSource<'a> {
    ExternalSource(&'a Path),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactPending {
    NativeDocument(NativeDocumentPhase),
    Building,
    Ready,
    Cleanup,
    Inspect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactMode {
    Link,
    Copy(CopyPolicy),
}
pub struct PublicationIntent {
    phase: ArtifactPending,
    candidate: Candidate,
    mode: ArtifactMode,
    stage: Option<PathBuf>,
    old_stage: Option<PathBuf>,
    old_destination: Option<PathBuf>,
    permissions_may_be_modified: bool,
}
impl PublicationIntent {
    /// A durable permission transition may have affected an owned physical path.
    pub fn permissions_may_be_modified(&self) -> bool {
        self.permissions_may_be_modified
    }

    pub fn mode(&self) -> ArtifactMode {
        self.mode
    }
    pub fn staging_directory(&self) -> Option<&Path> {
        self.stage.as_deref()
    }
    pub fn old_destination(&self) -> Option<&Path> {
        self.old_destination.as_deref()
    }
    pub fn old_staging_directory(&self) -> Option<&Path> {
        self.old_stage.as_deref()
    }
    pub fn payload(&self) -> Option<PathBuf> {
        self.stage.as_ref().map(|p| p.join("payload"))
    }
    pub fn backup(&self) -> Option<PathBuf> {
        self.old_stage
            .as_ref()
            .or(self.stage.as_ref())
            .map(|p| p.join("backup"))
    }

    pub fn phase(&self) -> ArtifactPending {
        self.phase
    }
    pub fn key(&self) -> &ArtifactKey {
        &self.candidate.key
    }
    pub fn source(&self) -> &Path {
        &self.candidate.source.0
    }
    pub fn destination(&self) -> &Path {
        &self.candidate.destination.0
    }
    pub fn kind(&self) -> NodeKind {
        self.candidate.kind.into()
    }
}
#[derive(Debug)]
pub enum ArtifactIntent {
    NativeDocument(Box<NativeDocumentIntent>),
    Publication(Box<PublicationIntent>),
}
impl ArtifactIntent {
    pub fn phase(&self) -> ArtifactPending {
        match self {
            Self::NativeDocument(p) => ArtifactPending::NativeDocument(p.phase()),
            Self::Publication(p) => p.phase(),
        }
    }
    pub fn publication(&self) -> Option<&PublicationIntent> {
        match self {
            Self::Publication(p) => Some(p),
            _ => None,
        }
    }
    pub fn key(&self) -> Option<&ArtifactKey> {
        match self {
            Self::NativeDocument(p) => Some(p.key()),
            _ => self.publication().map(PublicationIntent::key),
        }
    }
    pub fn permissions_may_be_modified(&self) -> bool {
        match self {
            Self::NativeDocument(_) => false,
            Self::Publication(p) => p.permissions_may_be_modified(),
        }
    }
}
impl From<Pending> for ArtifactIntent {
    fn from(pending: Pending) -> Self {
        let phase = pending.phase();
        Self::Publication(Box::new(match pending {
            Pending::NativeDocument(op) => return Self::NativeDocument(Box::new((*op).into())),
            Pending::Inspect { entry, .. } => PublicationIntent {
                phase,
                mode: entry.mode(),
                candidate: entry.candidate,
                stage: None,
                old_stage: None,
                old_destination: None,
                permissions_may_be_modified: true,
            },
            Pending::Publication(op) => PublicationIntent {
                phase,
                permissions_may_be_modified: op.permissions_may_be_modified(),
                old_destination: op.old.as_ref().map(|e| e.destination().to_owned()),
                mode: op.mode,
                candidate: op.candidate,
                stage: Some(op.stage.path.0),
                old_stage: op.old_stage.map(|s| s.path.0),
            },
        }))
    }
}
impl fmt::Debug for PublicationIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublicationIntent")
            .field("phase", &self.phase)
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}
/// Metadata was detached; physical paths remain the caller's responsibility.
pub struct ArtifactAbandoned {
    entry: Option<ArtifactEntry>,
    intent: Option<ArtifactIntent>,
}
impl ArtifactAbandoned {
    pub fn entry(&self) -> Option<&ArtifactEntry> {
        self.entry.as_ref()
    }
    pub fn intent(&self) -> Option<&ArtifactIntent> {
        self.intent.as_ref()
    }
}
impl fmt::Debug for ArtifactAbandoned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactAbandoned { .. }")
    }
}

#[derive(Debug)]
pub struct ArtifactSnapshot {
    owner: StoreOwner,
    entries: Vec<ArtifactEntry>,
    pending: Option<ArtifactIntent>,
}
impl ArtifactSnapshot {
    pub fn owner(&self) -> &StoreOwner {
        &self.owner
    }
    pub fn entries(&self) -> &[ArtifactEntry] {
        &self.entries
    }
    pub fn pending(&self) -> Option<&ArtifactIntent> {
        self.pending.as_ref()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactChange {
    Created,
    Unchanged,
    Removed,
    Updated,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactRecovery {
    NoPending,
    AbortedPublication,
    CompletedCreate,
    CompletedRemove,
    CompletedUpdate,
    RestoredAccess,
}

#[derive(Clone, Debug)]
pub struct ArtifactStore {
    scope: CapturedPublicationScope,
    limits: ArtifactLimits,
}
impl ArtifactStore {
    pub fn scope(&self) -> &CapturedPublicationScope {
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
    pub fn new(scope: CapturedPublicationScope, limits: ArtifactLimits) -> Self {
        Self { scope, limits }
    }
    fn runtime(&self) -> PathBuf {
        self.runtime_root().to_owned()
    }
    pub fn manifest_path(&self) -> PathBuf {
        self.runtime_root().join("artifact-store.json")
    }
    fn destination_scope(&self, path: &Path) -> io::Result<()> {
        self.scope.destination(path)
    }
    fn read(&self) -> ArtifactResult<Option<Document>> {
        catalog::validate_limit(self.limits.max_document_bytes)
            .map_err(|e| at(ArtifactStage::Read, e))?;
        self.scope
            .check(None)
            .map_err(|e| at(ArtifactStage::Read, e))?;
        if !directory_exists(&self.runtime()).map_err(|e| at(ArtifactStage::Read, e))? {
            return Ok(None);
        }
        let control =
            control::ControlDirectory::open(&self.scope).map_err(|e| at(ArtifactStage::Read, e))?;
        let observed = catalog::read(self).map_err(|e| at(ArtifactStage::Read, e));
        #[cfg(test)]
        control::test_hooks::after_read();
        let observed = control::checked(
            observed,
            control.revalidate(&self.scope, None),
            ArtifactStage::Read,
        )?;
        Ok(observed)
    }
    pub fn load(&self) -> ArtifactResult<Option<ArtifactSnapshot>> {
        self.read().map(|document| {
            document.map(|d| ArtifactSnapshot {
                owner: d.scope.owner().clone(),
                pending: d.pending.map(ArtifactIntent::from),
                entries: d.entries,
            })
        })
    }
    /// Prepare the project's private runtime before capturing its source tree.
    /// This creates only the scope's control directories, without taking a writer lock.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn prepare_project_runtime_with_context(
        &self,
        request: &std::sync::Arc<SnapshotContext>,
    ) -> ArtifactResult<()> {
        if !matches!(self.owner(), StoreOwner::Project { .. }) {
            return Err(at(
                ArtifactStage::Prepare,
                io::ErrorKind::InvalidInput.into(),
            ));
        }
        request.check().map_err(|e| at(ArtifactStage::Prepare, e))?;
        let chain = native_source::NativeControlChain::capture(&self.scope, request)
            .map_err(|e| at(ArtifactStage::Prepare, e))?;
        chain
            .revalidate()
            .map_err(|e| at(ArtifactStage::Prepare, e))
    }

    /// Acquire the actual native control writer on the original request.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[track_caller]
    pub fn writer_with_context(
        &self,
        request: &std::sync::Arc<SnapshotContext>,
    ) -> ArtifactResult<ArtifactWriter> {
        let caller = std::panic::Location::caller();
        let diagnostic = tracing::debug_span!(
            "artifact_writer",
            caller_file = caller
                .file()
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("<source>"),
            caller_line = caller.line(),
            project_id = tracing::field::Empty,
        );
        if let StoreOwner::Project { project_id } = self.owner() {
            diagnostic.record("project_id", tracing::field::display(project_id));
        }
        tracing::debug!(parent: &diagnostic, phase = "capture_control");
        request.check().map_err(|e| at(ArtifactStage::Lock, e))?;
        let chain = native_source::NativeControlChain::capture(&self.scope, request)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        let control = control::ControlDirectory::from_native_chain(&chain, &self.scope, request)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        let lock_file = {
            let parent = chain.file().map_err(|e| at(ArtifactStage::Lock, e))?;
            graphit_fs::write_relative::open_private_lock_relative(
                &parent,
                std::ffi::OsStr::new("artifact-store.lock"),
                64 * 1024,
                &mut || request.check(),
            )
            .map_err(|e| at(ArtifactStage::Lock, e))?
        };
        let path = self.runtime_root().join("artifact-store.lock");
        tracing::debug!(parent: &diagnostic, phase = "lock_wait");
        let lock = FileLock::acquire_owned_file_checked(
            lock_file,
            path,
            request.deadline(),
            request.cancellation(),
            &mut || request.check(),
        )
        .map_err(|e| at(ArtifactStage::Lock, e))?;
        tracing::debug!(parent: &diagnostic, phase = "acquired");
        chain.revalidate().map_err(|e| at(ArtifactStage::Lock, e))?;
        self.scope
            .revalidate_native(request)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        {
            let parent = chain.file().map_err(|e| at(ArtifactStage::Lock, e))?;
            let observed = graphit_fs::write_relative::open_existing_private_lock_relative(
                &parent,
                std::ffi::OsStr::new("artifact-store.lock"),
                64 * 1024,
                &mut || request.check(),
            )
            .map_err(|e| at(ArtifactStage::Lock, e))?;
            request.check().map_err(|e| at(ArtifactStage::Lock, e))?;
            let expected = crate::native_file_identity::handle_id(lock.file())
                .map_err(|e| at(ArtifactStage::Lock, e))?;
            request.check().map_err(|e| at(ArtifactStage::Lock, e))?;
            if crate::native_file_identity::handle_id(&observed)
                .map_err(|e| at(ArtifactStage::Lock, e))?
                != expected
            {
                return Err(error(ArtifactStage::Lock, io::ErrorKind::InvalidData));
            }
        }
        let scope_context = ScopeContext::from_snapshot(request);
        #[cfg(test)]
        native_writer_acquisition_hooks::after_binding();
        request.check().map_err(|e| at(ArtifactStage::Lock, e))?;
        let mut writer = self.writer_from_resources(
            control,
            lock,
            scope_context,
            request.cancellation().clone(),
            Some(std::sync::Arc::clone(request)),
            Some(WriterCapture {
                chain,
                request: request.clone(),
            }),
        );
        writer.diagnostic = diagnostic;
        Ok(writer)
    }

    /// Bounds contention and cancellable scope checks; native IO is not preemptible.
    /// Durable reconciliation after an effect still completes independently of cancellation.
    pub fn writer(
        &self,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> ArtifactResult<ArtifactWriter> {
        check_cancelled(cancellation).map_err(|e| at(ArtifactStage::Lock, e))?;
        let deadline = std::time::Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| error(ArtifactStage::Lock, io::ErrorKind::InvalidInput))?;
        let scope_context = ScopeContext::new(deadline, cancellation.clone());
        self.scope
            .revalidate(&scope_context)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        native_source::ensure_control_parent(&self.scope, &scope_context)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        self.scope
            .revalidate(&scope_context)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        let control = control::ControlDirectory::create_or_open(&self.scope, &scope_context)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        self.scope
            .revalidate(&scope_context)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        let path = self.runtime().join("artifact-store.lock");
        regular_metadata(&path).map_err(|e| at(ArtifactStage::Lock, e))?;
        #[cfg(test)]
        control::test_hooks::before_lock();
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        let lock = {
            let file = control
                .open_private_lock_file(&self.scope, &scope_context)
                .map_err(|e| at(ArtifactStage::Lock, e))?;
            FileLock::acquire_owned_file_checked(
                file,
                path,
                Some(deadline),
                cancellation,
                &mut || scope_context.check(),
            )
            .map_err(|e| at(ArtifactStage::Lock, e))?
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        let lock = FileLock::acquire_in_existing_parent(
            &path,
            deadline.saturating_duration_since(std::time::Instant::now()),
            cancellation,
        )
        .map_err(|e| at(ArtifactStage::Lock, e))?;
        self.scope
            .revalidate(&scope_context)
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        control
            .revalidate(&self.scope, Some(&scope_context))
            .map_err(|e| at(ArtifactStage::Lock, e))?;
        Ok(self.writer_from_resources(
            control,
            lock,
            scope_context,
            cancellation.clone(),
            None,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            None,
        ))
    }
    fn writer_from_resources(
        &self,
        control: control::ControlDirectory,
        lock: FileLock,
        scope_context: ScopeContext,
        cancellation: Cancellation,
        io_context: Option<std::sync::Arc<SnapshotContext>>,
        #[cfg(any(target_os = "linux", target_os = "macos", windows))] native: Option<
            WriterCapture,
        >,
    ) -> ArtifactWriter {
        ArtifactWriter {
            _lock: lock,
            control,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            native,
            store: self.clone(),
            cancellation,
            scope_context,
            io_context,
            #[cfg(test)]
            before_check_rebind: std::cell::RefCell::new(None),
            #[cfg(test)]
            before_prepare: std::cell::RefCell::new(None),
            #[cfg(test)]
            pause_at: std::cell::RefCell::new(None),
            #[cfg(test)]
            fail_at: std::cell::Cell::new(None),
            #[cfg(test)]
            cancel_at: std::cell::Cell::new(None),
            diagnostic: tracing::Span::none(),
        }
    }
}
#[cfg(all(test, any(target_os = "linux", target_os = "macos", windows)))]
mod native_writer_acquisition_hooks {
    thread_local! {
        pub(super) static AFTER_RECORD_BIRTH: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {
            std::cell::RefCell::new(None)
        };
        pub(super) static AFTER_BINDING: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {
            std::cell::RefCell::new(None)
        };
    }
    pub(super) fn after_record_birth() {
        AFTER_RECORD_BIRTH.with(|hook| {
            if let Some(hook) = hook.borrow_mut().take() {
                hook();
            }
        });
    }
    pub(super) fn after_binding() {
        AFTER_BINDING.with(|hook| {
            if let Some(hook) = hook.borrow_mut().take() {
                hook();
            }
        });
    }
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
struct WriterCapture {
    chain: native_source::NativeControlChain,
    request: std::sync::Arc<SnapshotContext>,
}

/// One whole-operation writer. Reads reload disk after every success or failure.
pub struct ArtifactWriter {
    _lock: FileLock,
    control: control::ControlDirectory,
    store: ArtifactStore,
    cancellation: Cancellation,
    scope_context: ScopeContext,
    io_context: Option<std::sync::Arc<SnapshotContext>>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    native: Option<WriterCapture>,
    #[cfg(test)]
    before_check_rebind: std::cell::RefCell<Option<Box<dyn FnOnce()>>>,
    #[cfg(test)]
    before_prepare: std::cell::RefCell<Option<Box<dyn FnOnce()>>>,
    #[cfg(test)]
    pause_at: std::cell::RefCell<Option<(ArtifactStage, usize, PathBuf)>>,
    #[cfg(test)]
    fail_at: std::cell::Cell<Option<ArtifactStage>>,
    #[cfg(test)]
    cancel_at: std::cell::Cell<Option<(ArtifactStage, usize)>>,
    // Last field: CLOSE follows the FileLock and physical owner drops.
    diagnostic: tracing::Span,
}
impl fmt::Debug for ArtifactWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactWriter { .. }")
    }
}
/// Fixed record corpora that share the held PROJECT runtime acquisition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRecordDirectory {
    Task,
    Knowledge,
    Memory,
}
impl ProjectRecordDirectory {
    fn directory_name(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Knowledge => "knowledge",
            Self::Memory => "memory",
        }
    }
}

impl ArtifactWriter {
    /// The scope whose native writer lock is retained by this value.
    pub fn scope(&self) -> &CapturedPublicationScope {
        &self.store.scope
    }

    /// Revalidate a native writer on the very same Original that acquired it.
    /// This supplies no new authorization and cannot rebind a normal writer.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn revalidate(&self, request: &std::sync::Arc<SnapshotContext>) -> ArtifactResult<()> {
        request
            .check()
            .map_err(|error| at(ArtifactStage::Validate, error))?;
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        if !native.request.same_request(request) {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        self.check_native_acquisition()
    }

    /// Ensure the admitted publication's real parent through held native roots.
    /// Existing components must be directories, never links; created components
    /// are private. Cancellation/failure can leave empty directories, but does
    /// not publish an artifact. The caller must hold its PROJECT write admission.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn ensure_publication_parent(&self, destination: &Path) -> ArtifactResult<()> {
        self.check_native_acquisition()?;
        self.store
            .scope
            .require_project()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.store
            .destination_scope(destination)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.ready(&self.document()?)?;
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        let parent = destination
            .parent()
            .and_then(|parent| parent.strip_prefix(self.store.scope.owner_root()).ok())
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        native
            .chain
            .ensure_directory(parent)
            .map_err(|e| at(ArtifactStage::Create, e))?;
        self.check_native_acquisition()
    }
    /// Read a confirmed native PROJECT Copy publication on the same request.
    /// The held catalog lock selects the current entry. The returned reader
    /// retains physical bindings and the Original, and does not retain the lock.
    /// PROJECT authorization remains the caller's responsibility.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn read_published_copy(
        &self,
        key: &ArtifactKey,
        kind: NodeKind,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
        request: &std::sync::Arc<SnapshotContext>,
    ) -> ArtifactResult<CapturedSourceReader> {
        tracing::debug!(parent: &self.diagnostic, phase = "read_published_copy");
        request
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        if !native.request.same_request(request) {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        self.store
            .scope
            .require_project()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        self.check_native_acquisition()?;
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let document = self.document()?;
        self.ready(&document)?;
        let entry = document
            .entries
            .iter()
            .find(|entry| entry.matches_publication(self.publication_slot(), key))
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::NotFound))?;
        if entry.kind() != kind {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        let Proof::Copy { tree, .. } = &entry.receipt else {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        };
        let relative = entry
            .destination()
            .strip_prefix(self.store.scope.owner_root())
            .map_err(|_| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        let snapshot = native_source::NativeSource::capture_verified_copy(
            &self.store.scope,
            relative,
            entry.kind(),
            policy,
            request,
            tree,
        )
        .map_err(|e| at(ArtifactStage::Conflict, e))?;
        self.check_native_acquisition()?;
        CapturedSourceReader::from_native_snapshot(snapshot, limits, request)
            .map_err(|e| at(ArtifactStage::Conflict, e))
    }
    /// Exclusively create a fixed PROJECT record directory under the held runtime.
    /// Acquire this writer with the original request after authorizing the chosen module WRITE.
    /// Existing entries are rejected. A failure after creation may leave the empty directory.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn create_user_memory_directory(
        &mut self,
        limits: crate::native_document_file::NativeDocumentFileLimits,
    ) -> ArtifactResult<crate::native_document_file::CapturedUserDocumentDirectory> {
        self.store
            .scope
            .user_root()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        let directory = native
            .chain
            .create_record_directory(ProjectRecordDirectory::Memory, limits)
            .map_err(|e| at(ArtifactStage::Create, e))?;
        Ok(crate::native_document_file::CapturedUserDocumentDirectory::from_record(directory))
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn create_record_directory(
        &mut self,
        kind: ProjectRecordDirectory,
        limits: crate::native_document_file::NativeDocumentFileLimits,
    ) -> ArtifactResult<crate::native_document_file::CapturedProjectDocumentDirectory> {
        self.store
            .scope
            .require_project()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
        native
            .chain
            .create_record_directory(kind, limits)
            .map_err(|e| at(ArtifactStage::Create, e))
    }
    /// Bind data I/O for the explicit normal writer domain.
    /// A writer acquired with an original native request rejects rebinding.
    pub fn with_io_context(
        mut self,
        context: std::sync::Arc<SnapshotContext>,
    ) -> ArtifactResult<Self> {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if self.native.is_some() {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        self.io_context = Some(context);
        Ok(self)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    fn check_native_acquisition(&self) -> ArtifactResult<()> {
        if let Some(native) = &self.native {
            native
                .request
                .check()
                .map_err(|e| at(ArtifactStage::Validate, e))?;
            native
                .chain
                .revalidate()
                .map_err(|e| at(ArtifactStage::Validate, e))?;
        }
        Ok(())
    }
    fn grant_tree(
        &self,
        tree: &tree::TreeReceipt,
        root: &Path,
        access: tree::Access,
        missing: bool,
        after_effect: impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        match &self.io_context {
            Some(context) => tree.grant_with_context(root, access, missing, context, after_effect),
            None => tree.grant(root, access, missing, after_effect),
        }
    }
    fn verify_tree(
        &self,
        tree: &tree::TreeReceipt,
        root: &Path,
        access: Option<tree::Access>,
        missing: bool,
        cancellation: &Cancellation,
    ) -> io::Result<()> {
        match &self.io_context {
            Some(context) => {
                tree.verify_with_context(root, self.store.limits.copy, access, missing, context)
            }
            None => tree.verify(root, self.store.limits.copy, access, missing, cancellation),
        }
    }

    fn document(&self) -> ArtifactResult<Document> {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if self.native.is_some() {
            return self.loaded_catalog().map(|loaded| loaded.document);
        }
        catalog::validate_limit(self.store.limits.max_document_bytes)
            .map_err(|e| at(ArtifactStage::Read, e))?;
        self.control
            .revalidate(&self.store.scope, None)
            .map_err(|e| at(ArtifactStage::Read, e))?;
        let observed = control::checked(
            self.store.read(),
            self.control.revalidate(&self.store.scope, None),
            ArtifactStage::Read,
        )?;
        match observed {
            Some(document) => Ok(document),
            None => Document::empty(&self.store).map_err(|e| at(ArtifactStage::Read, e)),
        }
    }
    fn loaded_catalog(&self) -> ArtifactResult<catalog::LoadedCatalog<'_>> {
        catalog::validate_limit(self.store.limits.max_document_bytes)
            .map_err(|e| at(ArtifactStage::Read, e))?;
        catalog::current_boundary(self).map_err(|e| at(ArtifactStage::Read, e))?;
        let observed = catalog::read_loaded(self).map_err(|e| at(ArtifactStage::Read, e));
        #[cfg(test)]
        control::test_hooks::after_read();
        control::checked(
            observed,
            catalog::current_boundary(self),
            ArtifactStage::Read,
        )
    }
    fn save_native_projection(
        &self,
        loaded: catalog::LoadedCatalog<'_>,
        pending: Option<&native_document::Operation>,
        stage: ArtifactStage,
    ) -> ArtifactResult<()> {
        loaded.validate_writer(self).map_err(|e| at(stage, e))?;
        self.control
            .revalidate(&self.store.scope, None)
            .map_err(|e| at(stage, e))?;
        self.store.scope.check(None).map_err(|e| at(stage, e))?;
        let scope = self.store.scope.binding().map_err(|e| at(stage, e))?;
        if let Some(operation) = pending {
            operation
                .validate(&loaded.document, &self.store)
                .map_err(|e| at(stage, e))?;
            operation
                .authorize_destinations(&self.store.scope)
                .map_err(|e| at(stage, e))?;
        }
        #[cfg(test)]
        self.inject(stage)?;
        if stage == ArtifactStage::Prepare {
            self.scope_context.check().map_err(|e| at(stage, e))?;
        }
        let result = catalog::save_loaded(self, loaded, &scope, pending, stage);
        #[cfg(test)]
        control::test_hooks::after_save();
        control::checked(
            result,
            self.control.revalidate(&self.store.scope, None),
            stage,
        )
    }
    pub fn load(&self) -> ArtifactResult<Option<ArtifactSnapshot>> {
        catalog::validate_limit(self.store.limits.max_document_bytes)
            .map_err(|e| at(ArtifactStage::Read, e))?;
        self.control
            .revalidate(&self.store.scope, None)
            .map_err(|e| at(ArtifactStage::Read, e))?;
        control::checked(
            self.store.load(),
            self.control.revalidate(&self.store.scope, None),
            ArtifactStage::Read,
        )
    }
    fn ready(&self, document: &Document) -> ArtifactResult<()> {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        self.check_native_acquisition()?;
        if document.pending.is_some() {
            return Err(error(
                ArtifactStage::RecoveryRequired,
                io::ErrorKind::WouldBlock,
            ));
        }
        self.store
            .scope
            .revalidate(&self.scope_context)
            .map_err(|e| at(ArtifactStage::Validate, e))
    }
    #[cfg(test)]
    fn inject(&self, stage: ArtifactStage) -> ArtifactResult<()> {
        let signal = {
            let mut pause = self.pause_at.borrow_mut();
            if let Some((target, remaining, _)) = pause.as_mut()
                && *target == stage
            {
                if *remaining > 1 {
                    *remaining -= 1;
                    None
                } else {
                    pause.take().map(|(_, _, signal)| signal)
                }
            } else {
                None
            }
        };
        if let Some(signal) = signal {
            fs::write(signal, b"paused").map_err(|e| at(stage, e))?;
            loop {
                std::thread::park();
            }
        }
        if let Some((target, remaining)) = self.cancel_at.get()
            && target == stage
        {
            if remaining <= 1 {
                self.cancel_at.set(None);
                self.cancellation.cancel();
            } else {
                self.cancel_at.set(Some((target, remaining - 1)));
            }
        }
        if self.fail_at.get() == Some(stage) {
            self.fail_at.set(None);
            return Err(error(stage, io::ErrorKind::Other));
        }
        Ok(())
    }
    fn save(&self, document: &Document, stage: ArtifactStage) -> ArtifactResult<()> {
        self.control
            .revalidate(&self.store.scope, None)
            .map_err(|e| at(stage, e))?;
        self.store.scope.check(None).map_err(|e| at(stage, e))?;
        let mut updated = document.clone();
        updated.scope = self.store.scope.binding().map_err(|e| at(stage, e))?;
        let document = &updated;
        document.validate(&self.store).map_err(|e| at(stage, e))?;
        #[cfg(test)]
        self.inject(stage)?;
        // Post-effect recording remains independent of caller cancellation.
        if stage == ArtifactStage::Prepare {
            self.scope_context.check().map_err(|e| at(stage, e))?;
        }
        let result = catalog::save(self, document, stage);
        #[cfg(test)]
        control::test_hooks::after_save();
        control::checked(
            result,
            self.control.revalidate(&self.store.scope, None),
            stage,
        )
    }
    pub fn publish(
        &mut self,
        key: &ArtifactKey,
        source: PhysicalSource<'_>,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<ArtifactChange> {
        tracing::debug!(parent: &self.diagnostic, phase = "publish");
        self.publish_owned(key, source, destination, kind, mode)
    }
    /// Fully verifies an exact, readable owned publication without physical or logical writes.
    /// NeedsPublication does not promise a future publish will succeed. Copy permission
    /// inspection belongs to publish; source capture errors still propagate here.
    /// Acquiring this writer may create the runtime directory and lock file.
    pub fn check_publication(
        &self,
        key: &ArtifactKey,
        source: PhysicalSource<'_>,
        destination: &Path,
        kind: NodeKind,
        mode: ArtifactMode,
    ) -> ArtifactResult<PublicationCheck> {
        self.check_owned_publication(key, source, destination, kind, mode)
    }

    /// Verify a registered owned copy without retaining its original source.
    /// False means only that the key is absent; damaged or mismatched copies fail.
    pub fn verify_owned_copy(
        &self,
        key: &ArtifactKey,
        destination: &Path,
        kind: NodeKind,
    ) -> ArtifactResult<bool> {
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|error| at(ArtifactStage::Validate, error))?;
        self.store
            .destination_scope(destination)
            .map_err(|error| at(ArtifactStage::Validate, error))?;
        let document = self.document()?;
        self.ready(&document)?;
        let Some(entry) = document
            .entries
            .iter()
            .find(|entry| entry.matches_publication(self.publication_slot(), key))
        else {
            return Ok(false);
        };
        if entry.destination() != destination
            || entry.kind() != kind
            || !matches!(entry.mode(), ArtifactMode::Copy(_))
        {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::InvalidInput));
        }
        self.verify(entry, ArtifactStage::Conflict)?;
        self.control
            .revalidate(&self.store.scope, Some(&self.scope_context))
            .map_err(|error| at(ArtifactStage::Conflict, error))?;
        self.ready(&document)?;
        Ok(true)
    }

    fn verify(&self, entry: &ArtifactEntry, stage: ArtifactStage) -> ArtifactResult<()> {
        self.store
            .require_publication_slot(&entry.candidate)
            .map_err(|e| at(stage, e))?;
        self.store
            .destination_scope(entry.destination())
            .map_err(|e| at(stage, e))?;
        match &entry.receipt {
            Proof::Link { receipt: expected } => {
                if receipt::observe(&entry.candidate)
                    .map_err(|e| at(stage, e))?
                    .as_ref()
                    != Some(expected)
                {
                    return Err(error(stage, io::ErrorKind::AlreadyExists));
                }
            }
            Proof::Copy { tree, .. } => {
                if entry.generated_origin().is_some() {
                    tree.recapture_content(
                        entry.destination(),
                        self.store.limits.copy,
                        &self.cancellation,
                        self.io_context.as_deref(),
                    )
                    .map_err(|e| at(stage, e))?;
                } else {
                    self.verify_tree(tree, entry.destination(), None, false, &self.cancellation)
                        .map_err(|e| at(stage, e))?;
                }
            }
        }
        Ok(())
    }
    pub fn remove(&mut self, key: &ArtifactKey) -> ArtifactResult<ArtifactChange> {
        let document = self.document()?;
        self.ready(&document)?;
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        let entry = document
            .entries
            .iter()
            .find(|e| e.matches_publication(self.publication_slot(), key))
            .cloned()
            .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::NotFound))?;
        self.remove_owned(document, entry)
    }
    pub fn recover(&mut self) -> ArtifactResult<ArtifactRecovery> {
        tracing::debug!(parent: &self.diagnostic, phase = "recover");
        let mut document = self.document()?;
        let Some(pending) = document.pending.clone() else {
            return Ok(ArtifactRecovery::NoPending);
        };
        match pending {
            Pending::NativeDocument(_) => Err(error(
                ArtifactStage::RecoveryRequired,
                io::ErrorKind::WouldBlock,
            )),
            Pending::Publication(operation) => self.recover_publication(document, *operation),
            Pending::Inspect { entry } => {
                self.restore_inspect(&entry)?;
                document.pending = None;
                self.save(&document, ArtifactStage::Commit)?;
                Ok(ArtifactRecovery::RestoredAccess)
            }
        }
    }
    /// Explicitly forget metadata. The physical entry is never touched or adopted.
    pub fn forget(&mut self, key: &ArtifactKey) -> ArtifactResult<ArtifactAbandoned> {
        let mut document = self.document()?;
        self.scope_context
            .check()
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        key.validate(self.store.limits.max_key_bytes)
            .map_err(|e| at(ArtifactStage::Validate, e))?;
        if matches!(document.pending, Some(Pending::NativeDocument(_)))
            || document.pending.as_ref().is_some_and(|p| {
                p.key() != Some(key) || p.publication_slot() != Some(self.publication_slot())
            })
        {
            return Err(error(
                ArtifactStage::RecoveryRequired,
                io::ErrorKind::WouldBlock,
            ));
        }
        let abandoned = ArtifactAbandoned {
            entry: document
                .entries
                .iter()
                .find(|e| e.matches_publication(self.publication_slot(), key))
                .cloned(),
            intent: document.pending.clone().map(ArtifactIntent::from),
        };
        let count = document.entries.len();
        document
            .entries
            .retain(|e| !e.matches_publication(self.publication_slot(), key));
        if document.entries.len() == count && document.pending.is_none() {
            return Err(error(ArtifactStage::Validate, io::ErrorKind::NotFound));
        }
        document.pending = None;
        self.save(&document, ArtifactStage::Commit)?;
        Ok(abandoned)
    }
}

fn captured_destination(path: &Path) -> io::Result<PathBuf> {
    let tail = path
        .as_os_str()
        .as_encoded_bytes()
        .rsplit(|b| *b == b'/' || (cfg!(windows) && *b == b'\\'))
        .next()
        .unwrap_or_default();
    if matches!(tail, b"" | b"." | b"..") {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    Ok(fs::canonicalize(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?.join(name))
}

#[cfg(test)]
mod tests;
