//! Controlled shared-document operation data and native Stage lifecycle.
//! Restored data alone cannot construct a live Stage or authorize replay.
use super::*;
use crate::{
    file_io::{NativeDocumentIo, NativeIoPass},
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentDirectoryIdentity, NativeDocumentObservation,
        NativeDocumentReceipt, inspect_file, open_access_directory, open_file,
    },
    native_file_identity::{PhysicalId, handle_id, physical_id},
};
use std::{fs::File, sync::Arc};

/// Canonical operation data, persisted by the caller before Core preparation.
/// An identifier grants no Stage, publication, recovery or cleanup capability.
///
/// ```compile_fail
/// use graphit_core::artifact_store::{NativeDocumentOperationId, NativeDocumentStage};
/// let id = NativeDocumentOperationId::generate();
/// let _: NativeDocumentStage<'static> = id.into();
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NativeDocumentOperationId(ulid::Ulid);

impl NativeDocumentOperationId {
    /// Generate operation data. Persist it before invoking Core preparation.
    pub fn generate() -> Self {
        Self(ulid::Ulid::generate())
    }
}
impl std::fmt::Display for NativeDocumentOperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::str::FromStr for NativeDocumentOperationId {
    type Err = io::Error;
    fn from_str(value: &str) -> io::Result<Self> {
        if value.len() != 26 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let id = value
            .parse::<ulid::Ulid>()
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        if id.to_string() != value {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self(id))
    }
}
impl Serialize for NativeDocumentOperationId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for NativeDocumentOperationId {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct IdVisitor;
        impl serde::de::Visitor<'_> for IdVisitor {
            type Value = NativeDocumentOperationId;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a canonical 26-byte operation ULID")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value
                    .parse()
                    .map_err(|_| E::custom("invalid operation ULID"))
            }
        }
        decoder.deserialize_str(IdVisitor)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Operation {
    pub id: String,
    pub key: ArtifactKey,
    pub destination: NativePath,
    pub parent: PhysicalId,
    #[serde(deserialize_with = "deserialize_previous")]
    pub previous: Option<NativeDocumentReceipt>,
    pub state: State,
}
fn deserialize_previous<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Option<NativeDocumentReceipt>, D::Error> {
    Deserialize::deserialize(decoder)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", deny_unknown_fields)]
pub(super) enum State {
    Intent {},
    AbortedBeforeBirth {},
    Directory {
        directory: PhysicalId,
    },
    File {
        directory: PhysicalId,
        file: PhysicalId,
    },
    Prepared {
        directory: PhysicalId,
        stage: NativeDocumentReceipt,
    },
    Publishing {
        directory: PhysicalId,
        stage: NativeDocumentReceipt,
    },
    Resolved {
        directory: PhysicalId,
        stage: NativeDocumentReceipt,
        outcome: Outcome,
    },
    Acknowledged {
        directory: PhysicalId,
        stage: NativeDocumentReceipt,
        outcome: Outcome,
    },
    Cleaned {
        directory: PhysicalId,
        stage: NativeDocumentReceipt,
        outcome: Outcome,
    },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", deny_unknown_fields)]
pub(super) enum Outcome {
    Applied { destination: NativeDocumentReceipt },
    Unchanged {},
    NotPublished {},
}

/// Diagnostic phase data; no mutation or cleanup capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeDocumentPhase {
    Intent,
    AbortedBeforeBirth,
    Directory,
    File,
    Prepared,
    Publishing,
    Resolved,
    Acknowledged,
    Cleaned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeDocumentOutcome {
    Applied,
    Unchanged,
    NotPublished,
}
impl Outcome {
    fn diagnostic(&self) -> NativeDocumentOutcome {
        match self {
            Self::Applied { .. } => NativeDocumentOutcome::Applied,
            Self::Unchanged {} => NativeDocumentOutcome::Unchanged,
            Self::NotPublished {} => NativeDocumentOutcome::NotPublished,
        }
    }
}
impl State {
    pub(super) fn phase(&self) -> NativeDocumentPhase {
        match self {
            Self::Intent {} => NativeDocumentPhase::Intent,
            Self::AbortedBeforeBirth {} => NativeDocumentPhase::AbortedBeforeBirth,
            Self::Directory { .. } => NativeDocumentPhase::Directory,
            Self::File { .. } => NativeDocumentPhase::File,
            Self::Prepared { .. } => NativeDocumentPhase::Prepared,
            Self::Publishing { .. } => NativeDocumentPhase::Publishing,
            Self::Resolved { .. } => NativeDocumentPhase::Resolved,
            Self::Acknowledged { .. } => NativeDocumentPhase::Acknowledged,
            Self::Cleaned { .. } => NativeDocumentPhase::Cleaned,
        }
    }
    fn directory(&self) -> Option<&PhysicalId> {
        match self {
            Self::Intent {} | Self::AbortedBeforeBirth {} => None,
            Self::Directory { directory }
            | Self::File { directory, .. }
            | Self::Prepared { directory, .. }
            | Self::Publishing { directory, .. }
            | Self::Resolved { directory, .. }
            | Self::Acknowledged { directory, .. }
            | Self::Cleaned { directory, .. } => Some(directory),
        }
    }
    fn stage(&self) -> Option<&NativeDocumentReceipt> {
        match self {
            Self::Prepared { stage, .. }
            | Self::Publishing { stage, .. }
            | Self::Resolved { stage, .. }
            | Self::Acknowledged { stage, .. }
            | Self::Cleaned { stage, .. } => Some(stage),
            _ => None,
        }
    }
}
impl Operation {
    fn parent_path(&self) -> io::Result<&Path> {
        self.destination
            .0
            .parent()
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
    pub(super) fn directory_path(&self) -> io::Result<PathBuf> {
        Ok(self
            .parent_path()?
            .join(format!(".graphit-doc-{}", self.id)))
    }
    pub(super) fn file_path(&self) -> io::Result<PathBuf> {
        Ok(self.directory_path()?.join("document"))
    }
    pub(super) fn validate_destinations(&self, scope: &ScopeBindingRecord) -> io::Result<()> {
        for path in [
            &self.destination.0,
            &self.directory_path()?,
            &self.file_path()?,
        ] {
            scope.destination(path)?;
        }
        Ok(())
    }
    pub(super) fn authorize_destinations(
        &self,
        scope: &CapturedPublicationScope,
    ) -> io::Result<()> {
        for path in [
            &self.destination.0,
            &self.directory_path()?,
            &self.file_path()?,
        ] {
            scope.destination(path)?;
        }
        Ok(())
    }
    pub(super) fn validate(&self, document: &Document, store: &ArtifactStore) -> io::Result<()> {
        let id = self
            .id
            .parse::<ulid::Ulid>()
            .map_err(|_| io::ErrorKind::InvalidData)?;
        if self.id.len() != 26 || id.to_string() != self.id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.key.validate(store.limits.max_key_bytes)?;
        self.parent.validate()?;
        let directory = self.directory_path()?;
        let file = self.file_path()?;
        for path in [&self.destination.0, &directory, &file] {
            valid_path(path)?;
        }
        if self.destination.0.starts_with(&directory) || directory.starts_with(&self.destination.0)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for entry in &document.entries {
            if (entry.publication_slot() == PublicationSlot::Scope && entry.key() == &self.key)
                || [&self.destination.0, &directory, &file].iter().any(|path| {
                    path.starts_with(entry.destination()) || entry.destination().starts_with(path)
                })
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        if let Some(directory) = self.state.directory() {
            directory.validate()?;
            if directory == &self.parent {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        if let State::File { directory, file } = &self.state {
            file.validate()?;
            if file == directory || file == &self.parent {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        if let Some(stage) = self.state.stage() {
            if Some(stage.file_id()) == self.state.directory() || stage.file_id() == &self.parent {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if let State::Resolved {
                outcome: Outcome::Applied { destination },
                ..
            }
            | State::Acknowledged {
                outcome: Outcome::Applied { destination },
                ..
            }
            | State::Cleaned {
                outcome: Outcome::Applied { destination },
                ..
            } = &self.state
                && destination != &stage.destination_after(self.previous.as_ref())?
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        Ok(())
    }
}

/// Expected correlation data from the caller's durable logical intent.
/// This request grants no Stage or scope capability. Recovery still requires a
/// freshly captured parent and the writer's revalidated publication scope.
///
/// ```compile_fail
/// use graphit_core::artifact_store::{NativeDocumentRecoveryRequest, NativeDocumentStage};
/// fn restore<'a>(request: NativeDocumentRecoveryRequest<'a>) -> NativeDocumentStage<'a> {
///     request.into()
/// }
/// ```
pub struct NativeDocumentRecoveryRequest<'a> {
    pub operation_id: NativeDocumentOperationId,
    pub key: &'a ArtifactKey,
    pub destination: &'a Path,
    pub parent_identity: &'a NativeDocumentDirectoryIdentity,
    pub previous: Option<&'a NativeDocumentReceipt>,
    pub scope: &'a ScopeBindingRecord,
}

/// Read-only Pending diagnostics, without a Stage constructor or raw content.
pub struct NativeDocumentIntent {
    operation: Operation,
}
impl NativeDocumentIntent {
    pub fn id(&self) -> &str {
        &self.operation.id
    }
    pub fn key(&self) -> &ArtifactKey {
        &self.operation.key
    }
    pub fn destination(&self) -> &Path {
        &self.operation.destination.0
    }
    pub fn phase(&self) -> NativeDocumentPhase {
        self.operation.state.phase()
    }
    pub fn outcome(&self) -> Option<NativeDocumentOutcome> {
        match &self.operation.state {
            State::Resolved { outcome, .. }
            | State::Acknowledged { outcome, .. }
            | State::Cleaned { outcome, .. } => Some(outcome.diagnostic()),
            State::AbortedBeforeBirth {} => Some(NativeDocumentOutcome::NotPublished),
            _ => None,
        }
    }
}
impl fmt::Debug for NativeDocumentIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeDocumentIntent")
            .field("phase", &self.phase())
            .finish_non_exhaustive()
    }
}
impl From<Operation> for NativeDocumentIntent {
    fn from(operation: Operation) -> Self {
        Self { operation }
    }
}

/// A live SDK-created Stage. Dropping it leaves the durable Pending untouched.
///
/// This capability cannot be cloned, decoded or reconstructed from receipts.
/// The writer borrow keeps the original Core lock held through publication.
///
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentStage;
/// fn copy(stage: NativeDocumentStage<'_>) { let _ = stage.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentStage;
/// fn inspect(stage: NativeDocumentStage<'_>) { let _ = format!("{stage:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentStage;
/// let _: NativeDocumentStage<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentStage;
/// let _ = NativeDocumentStage {};
/// ```
pub struct NativeDocumentStage<'w> {
    runtime: Runtime<'w>,
}
/// A proven terminal outcome, recorded under the original writer lock.
///
/// Callers must first durably record their logical intent/refsets or abort
/// under their provider and Hub locks, then consume this acknowledgement.
///
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentResolution;
/// fn copy(resolution: NativeDocumentResolution<'_>) { let _ = resolution.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentResolution;
/// fn inspect(resolution: NativeDocumentResolution<'_>) { let _ = format!("{resolution:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentResolution;
/// let _: NativeDocumentResolution<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentResolution;
/// let _ = NativeDocumentResolution {};
/// ```
pub struct NativeDocumentResolution<'w> {
    writer: &'w mut ArtifactWriter,
    parent: &'w CapturedDocumentDirectory,
    operation: Box<Operation>,
    directory: File,
}
/// A correlated recovery result; no absence is interpreted as a terminal effect.
pub enum NativeDocumentRecovery<'w> {
    Resolution(NativeDocumentResolution<'w>),
    Completion(NativeDocumentCompletion<'w>),
}
/// A durable abort or completed cleanup, retained in Core until caller release.
/// Persist the caller's logical result and projection, then its Released phase,
/// before consuming `release`. Dropping this capability retains Pending.
///
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentCompletion;
/// fn copy(value: NativeDocumentCompletion<'_>) { let _ = value.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentCompletion;
/// fn inspect(value: NativeDocumentCompletion<'_>) { let _ = format!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentCompletion;
/// let _: NativeDocumentCompletion<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::artifact_store::NativeDocumentCompletion;
/// let _ = NativeDocumentCompletion {};
/// ```
pub struct NativeDocumentCompletion<'w> {
    writer: &'w mut ArtifactWriter,
    parent: &'w CapturedDocumentDirectory,
    operation: Box<Operation>,
}
impl NativeDocumentCompletion<'_> {
    pub fn id(&self) -> &str {
        &self.operation.id
    }
    pub fn key(&self) -> &ArtifactKey {
        &self.operation.key
    }
    pub fn destination(&self) -> &Path {
        &self.operation.destination.0
    }
    pub fn outcome(&self) -> NativeDocumentOutcome {
        match &self.operation.state {
            State::AbortedBeforeBirth {} => NativeDocumentOutcome::NotPublished,
            State::Cleaned { outcome, .. } => outcome.diagnostic(),
            _ => unreachable!("completion requires a durable terminal state"),
        }
    }
    /// Historical Applied receipt data; no current destination or SDK authority.
    pub fn destination_receipt(&self) -> Option<&NativeDocumentReceipt> {
        match &self.operation.state {
            State::Cleaned {
                outcome: Outcome::Applied { destination },
                ..
            } => Some(destination),
            _ => None,
        }
    }
    /// Clear only this exact terminal after the caller's Released record is durable.
    pub fn release(self) -> ArtifactResult<()> {
        let pass = context(self.writer)?
            .native_inspection(0)
            .map_err(|e| at(ArtifactStage::Cleanup, e))?;
        parent_binding(self.writer, self.parent, &self.operation, &pass)
            .map_err(|e| at(ArtifactStage::Cleanup, e))?;
        if matches!(self.operation.state, State::AbortedBeforeBirth {}) {
            absent_before_birth(&self.operation)?;
        }
        parent_binding(self.writer, self.parent, &self.operation, &pass)
            .map_err(|e| at(ArtifactStage::Cleanup, e))?;
        control::checked(
            clear_operation(self.writer, &self.operation),
            parent_binding(self.writer, self.parent, &self.operation, &pass),
            ArtifactStage::Cleanup,
        )
    }
}
/// The first failure and, when durable proof exists, its terminal resolution.
/// A native error may accompany Applied; it does not imply zero effect.
pub struct NativeDocumentFailure<'w> {
    error: ArtifactError,
    resolution: Option<NativeDocumentResolution<'w>>,
}
impl<'w> NativeDocumentFailure<'w> {
    pub fn error(&self) -> &ArtifactError {
        &self.error
    }
    pub fn resolution(&self) -> Option<&NativeDocumentResolution<'w>> {
        self.resolution.as_ref()
    }
    pub fn into_parts(self) -> (ArtifactError, Option<NativeDocumentResolution<'w>>) {
        (self.error, self.resolution)
    }
    fn without_resolution(error: ArtifactError) -> Self {
        Self {
            error,
            resolution: None,
        }
    }
}
impl<'w> NativeDocumentResolution<'w> {
    pub fn id(&self) -> &str {
        &self.operation.id
    }
    pub fn key(&self) -> &ArtifactKey {
        &self.operation.key
    }
    pub fn destination(&self) -> &Path {
        &self.operation.destination.0
    }
    /// The actual published destination data, without filesystem authority.
    pub fn destination_receipt(&self) -> Option<&NativeDocumentReceipt> {
        match &self.operation.state {
            State::Resolved {
                outcome: Outcome::Applied { destination },
                ..
            }
            | State::Acknowledged {
                outcome: Outcome::Applied { destination },
                ..
            } => Some(destination),
            _ => None,
        }
    }

    pub fn outcome(&self) -> NativeDocumentOutcome {
        match &self.operation.state {
            State::Resolved { outcome, .. } | State::Acknowledged { outcome, .. } => {
                outcome.diagnostic()
            }
            _ => unreachable!("resolution only constructed after confirmed terminal recording"),
        }
    }
    /// Persist acknowledgement, clean verified SDK state and retain a terminal.
    /// Failure retains Acknowledged or the terminal actually saved; no clear.
    pub fn acknowledge(mut self) -> ArtifactResult<NativeDocumentCompletion<'w>> {
        let State::Resolved {
            directory,
            stage,
            outcome,
        } = &self.operation.state
        else {
            return Err(error(
                ArtifactStage::RecoveryConflict,
                io::ErrorKind::InvalidData,
            ));
        };
        self.operation.state = State::Acknowledged {
            directory: directory.clone(),
            stage: stage.clone(),
            outcome: outcome.clone(),
        };
        save_bound_operation(
            self.writer,
            self.parent,
            &self.operation,
            ArtifactStage::Commit,
        )?;
        cleanup(
            self.writer,
            self.parent,
            &self.operation,
            Some(self.directory),
        )?;
        record_cleaned(self.writer, self.parent, &mut self.operation)?;
        Ok(NativeDocumentCompletion {
            writer: self.writer,
            parent: self.parent,
            operation: self.operation,
        })
    }
}
struct Runtime<'w> {
    writer: &'w mut ArtifactWriter,
    parent: &'w CapturedDocumentDirectory,
    expected: &'w NativeDocumentObservation,
    desired: &'w [u8],
    operation: Box<Operation>,
    directory: Option<File>,
    file: Option<File>,
    file_identity: Option<PhysicalId>,
    io: NativeDocumentIo,
    attempted: bool,
    record_failed: bool,
}
impl ArtifactWriter {
    /// Prepare a shared native document with exclusive SDK birth and actual input bounds.
    /// The caller must durably persist `operation_id` before calling this method.
    /// Core records that same ID in Intent; matching ID never adopts Pending.
    /// This never adds an ArtifactEntry owning the whole provider document.
    pub fn prepare_native_document<'w>(
        &'w mut self,
        operation_id: NativeDocumentOperationId,
        key: &ArtifactKey,
        parent: &'w CapturedDocumentDirectory,
        expected: &'w NativeDocumentObservation,
        desired: &'w [u8],
    ) -> Result<NativeDocumentStage<'w>, NativeDocumentFailure<'w>> {
        let admission = (|| {
            let context = self
                .io_context
                .as_ref()
                .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))?;
            let destination = parent
                .check_expected(expected, context)
                .map_err(|e| at(ArtifactStage::Validate, e))?;
            let limits = parent.limits();
            let n = u64::try_from(desired.len())
                .map_err(|_| error(ArtifactStage::CapacityExceeded, io::ErrorKind::FileTooLarge))?;
            limits
                .check_stage(n)
                .map_err(|e| at(ArtifactStage::CapacityExceeded, e))?;
            let operation = Operation {
                id: operation_id.to_string(),
                key: key.clone(),
                destination: NativePath(destination),
                parent: parent.identity().clone(),
                previous: expected.receipt().cloned(),
                state: State::Intent {},
            };
            let document = self.document()?;
            self.ready(&document)?;
            operation
                .validate(&document, &self.store)
                .map_err(|e| at(ArtifactStage::Validate, e))?;
            operation
                .authorize_destinations(&self.store.scope)
                .map_err(|e| at(ArtifactStage::Validate, e))?;
            for path in [operation.directory_path(), operation.file_path()] {
                limits
                    .check_path(&path.map_err(|e| at(ArtifactStage::Validate, e))?)
                    .map_err(|e| at(ArtifactStage::CapacityExceeded, e))?;
            }
            let io = context
                .native_document_io(
                    n,
                    operation
                        .previous
                        .as_ref()
                        .map(NativeDocumentReceipt::length),
                )
                .map_err(|e| at(ArtifactStage::CapacityExceeded, e))?;
            save_operation(self, &operation, ArtifactStage::Prepare)?;
            Ok((operation, io))
        })();
        let (operation, io) = admission.map_err(NativeDocumentFailure::without_resolution)?;
        let mut runtime = Runtime {
            writer: self,
            parent,
            expected,
            desired,
            operation: Box::new(operation),
            directory: None,
            file: None,
            file_identity: None,
            io,
            attempted: false,
            record_failed: false,
        };
        match runtime.prepare() {
            Ok(()) => Ok(NativeDocumentStage { runtime }),
            Err(primary) => Err(runtime.failure(primary)),
        }
    }

    /// Correlate the caller's durable intent before investigating its persisted
    /// native operation. Recovery never replays rename or reconstructs a Stage.
    /// Prepared can only resolve as NotPublished.
    pub fn recover_native_document<'w>(
        &'w mut self,
        expected: NativeDocumentRecoveryRequest<'_>,
        parent: &'w CapturedDocumentDirectory,
    ) -> Result<NativeDocumentRecovery<'w>, ArtifactError> {
        let context = context(self)?;
        if !expected.scope.matches_scope(&self.store.scope) {
            return Err(error(
                ArtifactStage::RecoveryConflict,
                io::ErrorKind::InvalidData,
            ));
        }
        let document = self.document()?;
        let Some(Pending::NativeDocument(operation)) = document.pending else {
            return Err(error(
                ArtifactStage::RecoveryRequired,
                io::ErrorKind::WouldBlock,
            ));
        };
        if operation.id.parse::<NativeDocumentOperationId>().ok() != Some(expected.operation_id)
            || &operation.key != expected.key
            || operation.destination.0 != expected.destination
            || &parent.directory_identity() != expected.parent_identity
            || &operation.parent != parent.identity()
            || operation.previous.as_ref() != expected.previous
        {
            return Err(error(
                ArtifactStage::RecoveryConflict,
                io::ErrorKind::InvalidData,
            ));
        }
        let mut operation = operation;
        operation
            .authorize_destinations(&self.store.scope)
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let pass = context
            .native_inspection(0)
            .map_err(|e| at(ArtifactStage::Observe, e))?;
        parent_binding(self, parent, &operation, &pass)
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        if matches!(operation.state, State::Intent {}) {
            absent_before_birth(&operation)?;
            operation.state = State::AbortedBeforeBirth {};
            save_bound_operation(self, parent, &operation, ArtifactStage::RecordProof)?;
        }
        if matches!(
            operation.state,
            State::Directory { .. } | State::File { .. }
        ) {
            return Err(error(
                ArtifactStage::IncompleteStage,
                io::ErrorKind::InvalidData,
            ));
        }
        if matches!(operation.state, State::Acknowledged { .. }) {
            cleanup(self, parent, &operation, None)?;
            record_cleaned(self, parent, &mut operation)?;
        }
        if matches!(
            operation.state,
            State::AbortedBeforeBirth {} | State::Cleaned { .. }
        ) {
            if matches!(operation.state, State::AbortedBeforeBirth {}) {
                absent_before_birth(&operation)?;
            }
            parent_binding(self, parent, &operation, &pass)
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            return Ok(NativeDocumentRecovery::Completion(
                NativeDocumentCompletion {
                    writer: self,
                    parent,
                    operation,
                },
            ));
        }
        let directory = open_access_directory(
            &operation
                .directory_path()
                .map_err(|e| at(ArtifactStage::Validate, e))?,
        )
        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let stage = operation
            .state
            .stage()
            .ok_or_else(|| error(ArtifactStage::IncompleteStage, io::ErrorKind::InvalidData))?
            .clone();
        let actual_stage = inspect_path(
            self,
            parent,
            &operation,
            &directory,
            Target::Stage,
            context
                .native_inspection(stage.length())
                .map_err(|e| at(ArtifactStage::Observe, e))?,
            None,
        )
        .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
        let outcome = match &operation.state {
            State::Prepared { .. } if actual_stage.as_ref() == Some(&stage) => {
                Outcome::NotPublished {}
            }
            State::Publishing { .. } => {
                let bound = stage.length().max(
                    operation
                        .previous
                        .as_ref()
                        .map_or(0, NativeDocumentReceipt::length),
                );
                let destination = inspect_path(
                    self,
                    parent,
                    &operation,
                    &directory,
                    Target::Destination,
                    context
                        .native_inspection(bound)
                        .map_err(|e| at(ArtifactStage::Observe, e))?,
                    None,
                )
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                classify(&operation, actual_stage.as_ref(), destination.as_ref())
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?
            }
            State::Resolved {
                outcome: Outcome::NotPublished {},
                ..
            } if actual_stage.as_ref() == Some(&stage) => Outcome::NotPublished {},
            State::Resolved { outcome, .. } => {
                let bound = stage.length().max(
                    operation
                        .previous
                        .as_ref()
                        .map_or(0, NativeDocumentReceipt::length),
                );
                let destination = inspect_path(
                    self,
                    parent,
                    &operation,
                    &directory,
                    Target::Destination,
                    context
                        .native_inspection(bound)
                        .map_err(|e| at(ArtifactStage::Observe, e))?,
                    None,
                )
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                let actual = classify(&operation, actual_stage.as_ref(), destination.as_ref())
                    .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
                if &actual != outcome {
                    return Err(conflict());
                }
                actual
            }
            _ => {
                return Err(error(
                    ArtifactStage::RecoveryConflict,
                    io::ErrorKind::InvalidData,
                ));
            }
        };
        operation.state = State::Resolved {
            directory: operation.state.directory().ok_or_else(conflict)?.clone(),
            stage,
            outcome,
        };
        save_operation(self, &operation, ArtifactStage::RecordProof)?;
        Ok(NativeDocumentRecovery::Resolution(
            NativeDocumentResolution {
                writer: self,
                parent,
                operation,
                directory,
            },
        ))
    }
}
impl<'w> NativeDocumentStage<'w> {
    pub fn id(&self) -> &str {
        &self.runtime.operation.id
    }
    pub fn destination(&self) -> &Path {
        &self.runtime.operation.destination.0
    }
    pub fn receipt(&self) -> &NativeDocumentReceipt {
        self.runtime
            .operation
            .state
            .stage()
            .expect("Stage requires confirmed Prepared")
    }

    pub fn publish(mut self) -> Result<NativeDocumentResolution<'w>, NativeDocumentFailure<'w>> {
        let result = self.runtime.publish();
        match result {
            Ok(()) => self.runtime.complete(None),
            Err(primary) if self.runtime.attempted => self.runtime.complete(Some(primary)),
            Err(primary) => Err(self.runtime.failure(primary)),
        }
    }
    /// Resolve a live prepared Stage as NotPublished; never publish it afterwards.
    pub fn discard(self) -> Result<NativeDocumentResolution<'w>, NativeDocumentFailure<'w>> {
        self.runtime.abort(None)
    }
}
impl<'w> Runtime<'w> {
    fn record(&mut self, stage: ArtifactStage) -> ArtifactResult<()> {
        let result = save_operation(self.writer, &self.operation, stage);
        if result.is_err() {
            self.record_failed = true;
        }
        result
    }

    fn prepare(&mut self) -> ArtifactResult<()> {
        let pass = self
            .io
            .normal(0)
            .map_err(|e| at(ArtifactStage::Create, e))?;
        parent_binding(self.writer, self.parent, &self.operation, &pass)
            .map_err(|e| at(ArtifactStage::Create, e))?;
        self.directory = Some(
            create_directory(
                &self
                    .operation
                    .directory_path()
                    .map_err(|e| at(ArtifactStage::Create, e))?,
            )
            .map_err(|e| at(ArtifactStage::Create, e))?,
        );
        let directory = handle_id(self.directory.as_ref().ok_or_else(conflict)?)
            .map_err(|e| at(ArtifactStage::Observe, e))?;
        self.operation.state = State::Directory {
            directory: directory.clone(),
        };
        directory_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::Observe, e))?;
        self.record(ArtifactStage::RecordWrapper)?;
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_DIRECTORY_RECORD);
        directory_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::Create, e))?;
        self.file = Some(
            create_file(
                &self
                    .operation
                    .file_path()
                    .map_err(|e| at(ArtifactStage::Create, e))?,
            )
            .map_err(|e| at(ArtifactStage::Create, e))?,
        );
        let file = self.file.as_ref().ok_or_else(conflict)?;
        graphit_fs::verify_private_file(file).map_err(|e| at(ArtifactStage::Observe, e))?;
        let identity = handle_id(file).map_err(|e| at(ArtifactStage::Observe, e))?;
        self.file_identity = Some(identity.clone());
        self.operation.state = State::File {
            directory: directory.clone(),
            file: identity,
        };
        file_binding(
            &self
                .operation
                .file_path()
                .map_err(|e| at(ArtifactStage::Observe, e))?,
            file,
            self.file_identity.as_ref().ok_or_else(conflict)?,
        )
        .map_err(|e| at(ArtifactStage::Observe, e))?;
        directory_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::Observe, e))?;
        self.record(ArtifactStage::RecordProof)?;
        let mut pass = self
            .io
            .normal(self.desired.len() as u64)
            .map_err(|e| at(ArtifactStage::CopyChunk, e))?;
        let file = self.file.as_mut().ok_or_else(conflict)?;
        writing_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            file,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::CopyChunk, e))?;
        for chunk in self.desired.chunks(64 * 1024) {
            let mut remaining = chunk;
            while !remaining.is_empty() {
                writing_binding(
                    self.writer,
                    self.parent,
                    &self.operation,
                    self.directory.as_ref().ok_or_else(conflict)?,
                    file,
                    &pass,
                )
                .map_err(|e| at(ArtifactStage::CopyChunk, e))?;
                let written = pass
                    .write(file, remaining)
                    .map_err(|e| at(ArtifactStage::CopyChunk, e))?;
                if written == 0 {
                    return Err(error(ArtifactStage::CopyChunk, io::ErrorKind::WriteZero));
                }
                remaining = &remaining[written..];
                #[cfg(test)]
                test_hooks::run(&test_hooks::AFTER_WRITE);
            }
        }
        writing_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            file,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::CopyChunk, e))?;
        file.sync_all()
            .map_err(|e| at(ArtifactStage::CopyChunk, e))?;
        #[cfg(test)]
        if let Some(sync) = test_hooks::AFTER_SYNC.with(|h| h.borrow_mut().take()) {
            sync(file).map_err(|e| at(ArtifactStage::CopyChunk, e))?;
        }
        writing_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            file,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::GrantAccess, e))?;
        self.expected
            .apply_access(file, self.parent.limits())
            .map_err(|e| at(ArtifactStage::GrantAccess, e))?;
        let pass = self
            .io
            .normal(self.desired.len() as u64 + 1)
            .map_err(|e| at(ArtifactStage::Observe, e))?;
        directory_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::Observe, e))?;
        file_binding(
            &self
                .operation
                .file_path()
                .map_err(|e| at(ArtifactStage::Observe, e))?,
            file,
            self.file_identity.as_ref().ok_or_else(conflict)?,
        )
        .map_err(|e| at(ArtifactStage::Observe, e))?;
        let stage = inspect_file(
            &self
                .operation
                .file_path()
                .map_err(|e| at(ArtifactStage::Observe, e))?,
            file,
            self.parent.limits(),
            pass,
            Some(self.desired),
            |pass| {
                directory_binding(
                    self.writer,
                    self.parent,
                    &self.operation,
                    self.directory.as_ref().ok_or(io::ErrorKind::InvalidData)?,
                    pass,
                )
            },
        )
        .map_err(|e| at(ArtifactStage::Observe, e))?;
        self.expected
            .verify_stage_access(&stage)
            .map_err(|e| at(ArtifactStage::Observe, e))?;
        if self.operation.previous.is_none() {
            graphit_fs::verify_private_file(file).map_err(|e| at(ArtifactStage::Observe, e))?;
        }
        self.operation.state = State::Prepared { directory, stage };
        self.record(ArtifactStage::RecordProof)?;
        // The Windows publication route requires closing incompatible file handles.
        self.file = None;
        Ok(())
    }
    fn publish(&mut self) -> ArtifactResult<()> {
        let pass = self
            .io
            .normal(0)
            .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        directory_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        let directory = self
            .operation
            .state
            .directory()
            .ok_or_else(conflict)?
            .clone();
        let stage = self.operation.state.stage().ok_or_else(conflict)?.clone();
        self.operation.state = State::Publishing { directory, stage };
        self.record(ArtifactStage::Prepare)?;
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_PUBLISHING_RECORD);

        let pass = self
            .io
            .normal(self.desired.len() as u64 + 1)
            .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        let actual = inspect_path(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            Target::Stage,
            pass,
            Some(self.desired),
        )
        .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        if actual.as_ref() != self.operation.state.stage() {
            return Err(conflict());
        }
        let old = self
            .operation
            .previous
            .as_ref()
            .map_or(0, |r| r.length() + 1);
        let pass = self
            .io
            .normal(old)
            .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        self.parent
            .verify_expected(self.expected, pass)
            .map_err(|e| at(ArtifactStage::Conflict, e))?;
        let pass = self
            .io
            .normal(0)
            .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        directory_binding(
            self.writer,
            self.parent,
            &self.operation,
            self.directory.as_ref().ok_or_else(conflict)?,
            &pass,
        )
        .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        #[cfg(test)]
        test_hooks::run(&test_hooks::BEFORE_RENAME);
        pass.check()
            .map_err(|e| at(ArtifactStage::BeforeRename, e))?;
        self.attempted = true;
        native_publish(
            &self
                .operation
                .file_path()
                .map_err(|e| at(ArtifactStage::Publish, e))?,
            &self.operation.destination.0,
            self.operation.previous.is_some(),
        )
        .map_err(|e| at(ArtifactStage::Publish, e))?;
        #[cfg(test)]
        test_hooks::run(&test_hooks::AFTER_RENAME);
        Ok(())
    }
    fn failure(self, primary: ArtifactError) -> NativeDocumentFailure<'w> {
        if self.record_failed {
            return NativeDocumentFailure::without_resolution(primary);
        }

        match self.abort(Some(primary)) {
            Ok(resolution) => NativeDocumentFailure {
                error: primary,
                resolution: Some(resolution),
            },
            Err(failure) => failure,
        }
    }
    fn abort(
        mut self,
        primary: Option<ArtifactError>,
    ) -> Result<NativeDocumentResolution<'w>, NativeDocumentFailure<'w>> {
        let result = (|| {
            let identity = self
                .file_identity
                .as_ref()
                .ok_or_else(|| error(ArtifactStage::IncompleteStage, io::ErrorKind::InvalidData))?;
            let pass = self.io.complete_stage();
            let stage = if let Some(file) = &mut self.file {
                directory_binding(
                    self.writer,
                    self.parent,
                    &self.operation,
                    self.directory.as_ref().ok_or_else(conflict)?,
                    &pass,
                )
                .map_err(|e| at(ArtifactStage::IncompleteStage, e))?;
                file_binding(
                    &self
                        .operation
                        .file_path()
                        .map_err(|e| at(ArtifactStage::IncompleteStage, e))?,
                    file,
                    identity,
                )
                .map_err(|e| at(ArtifactStage::IncompleteStage, e))?;
                inspect_file(
                    &self
                        .operation
                        .file_path()
                        .map_err(|e| at(ArtifactStage::IncompleteStage, e))?,
                    file,
                    self.parent.limits(),
                    pass,
                    None,
                    |pass| {
                        directory_binding(
                            self.writer,
                            self.parent,
                            &self.operation,
                            self.directory.as_ref().ok_or(io::ErrorKind::InvalidData)?,
                            pass,
                        )
                    },
                )
                .map_err(|e| at(ArtifactStage::IncompleteStage, e))?
            } else {
                inspect_path(
                    self.writer,
                    self.parent,
                    &self.operation,
                    self.directory.as_ref().ok_or_else(conflict)?,
                    Target::Stage,
                    pass,
                    None,
                )
                .map_err(|e| at(ArtifactStage::IncompleteStage, e))?
                .ok_or_else(conflict)?
            };
            if stage.file_id() != identity {
                return Err(conflict());
            }
            if self
                .operation
                .state
                .stage()
                .is_some_and(|prepared| prepared != &stage)
            {
                return Err(conflict());
            }
            self.operation.state = State::Resolved {
                directory: self
                    .operation
                    .state
                    .directory()
                    .ok_or_else(conflict)?
                    .clone(),
                stage,
                outcome: Outcome::NotPublished {},
            };
            save_operation(self.writer, &self.operation, ArtifactStage::RecordProof)
        })();
        self.file = None;
        match result {
            Err(secondary) => Err(NativeDocumentFailure::without_resolution(first_failure(
                primary, secondary,
            ))),
            Ok(()) => {
                let directory = self.directory.take().ok_or_else(|| {
                    NativeDocumentFailure::without_resolution(first_failure(
                        primary,
                        error(ArtifactStage::IncompleteStage, io::ErrorKind::InvalidData),
                    ))
                })?;
                let resolution = NativeDocumentResolution {
                    writer: self.writer,
                    parent: self.parent,
                    operation: self.operation,
                    directory,
                };
                match primary {
                    Some(error) => Err(NativeDocumentFailure {
                        error,
                        resolution: Some(resolution),
                    }),
                    None => Ok(resolution),
                }
            }
        }
    }
    fn complete(
        mut self,
        primary: Option<ArtifactError>,
    ) -> Result<NativeDocumentResolution<'w>, NativeDocumentFailure<'w>> {
        let result = (|| {
            let directory = self.directory.as_ref().ok_or_else(conflict)?;
            let stage = inspect_path(
                self.writer,
                self.parent,
                &self.operation,
                directory,
                Target::Stage,
                self.io.complete_stage(),
                None,
            )
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e));
            let destination = inspect_path(
                self.writer,
                self.parent,
                &self.operation,
                directory,
                Target::Destination,
                self.io.complete_destination(),
                None,
            )
            .map_err(|e| at(ArtifactStage::RecoveryConflict, e));
            let (stage, destination) = match (stage, destination) {
                (Ok(stage), Ok(destination)) => (stage, destination),
                (Err(first), Err(second)) => return Err(first_failure(Some(first), second)),
                (Err(first), _) | (_, Err(first)) => return Err(first),
            };
            let outcome = classify(&self.operation, stage.as_ref(), destination.as_ref())
                .map_err(|e| at(ArtifactStage::RecoveryConflict, e))?;
            self.operation.state = State::Resolved {
                directory: self
                    .operation
                    .state
                    .directory()
                    .ok_or_else(conflict)?
                    .clone(),
                stage: self.operation.state.stage().ok_or_else(conflict)?.clone(),
                outcome,
            };
            save_operation(self.writer, &self.operation, ArtifactStage::RecordProof)
        })();
        match result {
            Err(secondary) => Err(NativeDocumentFailure::without_resolution(first_failure(
                primary, secondary,
            ))),
            Ok(()) => {
                let directory = self.directory.take().ok_or_else(|| {
                    NativeDocumentFailure::without_resolution(first_failure(
                        primary,
                        error(ArtifactStage::IncompleteStage, io::ErrorKind::InvalidData),
                    ))
                })?;
                let resolution = NativeDocumentResolution {
                    writer: self.writer,
                    parent: self.parent,
                    operation: self.operation,
                    directory,
                };
                match primary {
                    Some(error) => Err(NativeDocumentFailure {
                        error,
                        resolution: Some(resolution),
                    }),
                    None => Ok(resolution),
                }
            }
        }
    }
}
fn context(writer: &ArtifactWriter) -> ArtifactResult<Arc<SnapshotContext>> {
    writer
        .io_context
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| error(ArtifactStage::Validate, io::ErrorKind::InvalidInput))
}
fn first_failure(primary: Option<ArtifactError>, secondary: ArtifactError) -> ArtifactError {
    match primary {
        Some(mut primary) => {
            if primary.secondary.is_none() {
                primary.secondary = Some(secondary.kind());
            }
            primary
        }
        None => secondary,
    }
}
fn save_operation(
    writer: &ArtifactWriter,
    operation: &Operation,
    stage: ArtifactStage,
) -> ArtifactResult<()> {
    let loaded = writer.loaded_catalog()?;
    let document = &loaded.document;
    if let Some(pending) = &document.pending {
        match pending {
            Pending::NativeDocument(saved)
                if saved.id == operation.id
                    && saved.key == operation.key
                    && saved.destination == operation.destination
                    && saved.parent == operation.parent
                    && saved.previous == operation.previous => {}
            _ => {
                return Err(error(
                    ArtifactStage::RecoveryConflict,
                    io::ErrorKind::InvalidData,
                ));
            }
        }
    } else if !matches!(operation.state, State::Intent {}) {
        return Err(error(
            ArtifactStage::RecoveryConflict,
            io::ErrorKind::InvalidData,
        ));
    }
    writer.save_native_projection(loaded, Some(operation), stage)
}
fn clear_operation(writer: &ArtifactWriter, operation: &Operation) -> ArtifactResult<()> {
    let loaded = writer.loaded_catalog()?;
    if !matches!(&loaded.document.pending, Some(Pending::NativeDocument(saved)) if **saved == *operation)
    {
        return Err(error(
            ArtifactStage::RecoveryConflict,
            io::ErrorKind::InvalidData,
        ));
    }
    writer.save_native_projection(loaded, None, ArtifactStage::Commit)?;
    #[cfg(test)]
    test_hooks::run(&test_hooks::AFTER_CLEAR_OPERATION);
    Ok(())
}
fn absent_before_birth(operation: &Operation) -> ArtifactResult<()> {
    let path = operation
        .directory_path()
        .map_err(|e| at(ArtifactStage::Validate, e))?;
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        _ => Err(error(
            ArtifactStage::IncompleteStage,
            io::ErrorKind::InvalidData,
        )),
    }
}
fn save_bound_operation(
    writer: &ArtifactWriter,
    parent: &CapturedDocumentDirectory,
    operation: &Operation,
    stage: ArtifactStage,
) -> ArtifactResult<()> {
    let pass = context(writer)?
        .native_inspection(0)
        .map_err(|e| at(stage, e))?;
    parent_binding(writer, parent, operation, &pass).map_err(|e| at(stage, e))?;
    #[cfg(test)]
    if matches!(
        operation.state,
        State::AbortedBeforeBirth {} | State::Cleaned { .. }
    ) {
        test_hooks::run(&test_hooks::BEFORE_TERMINAL_RECORD);
    }
    parent_binding(writer, parent, operation, &pass).map_err(|e| at(stage, e))?;
    let saved = save_operation(writer, operation, stage);
    #[cfg(test)]
    if saved.is_ok()
        && matches!(
            operation.state,
            State::AbortedBeforeBirth {} | State::Cleaned { .. }
        )
    {
        test_hooks::run(&test_hooks::AFTER_TERMINAL_RECORD);
    }
    control::checked(
        saved,
        parent_binding(writer, parent, operation, &pass),
        stage,
    )
}
fn record_cleaned(
    writer: &ArtifactWriter,
    parent: &CapturedDocumentDirectory,
    operation: &mut Operation,
) -> ArtifactResult<()> {
    let State::Acknowledged {
        directory,
        stage,
        outcome,
    } = &operation.state
    else {
        return Err(error(
            ArtifactStage::RecoveryConflict,
            io::ErrorKind::InvalidData,
        ));
    };
    operation.state = State::Cleaned {
        directory: directory.clone(),
        stage: stage.clone(),
        outcome: outcome.clone(),
    };
    save_bound_operation(writer, parent, operation, ArtifactStage::Commit)
}
#[derive(Clone, Copy)]
enum Target {
    Stage,
    Destination,
}
fn parent_binding(
    writer: &ArtifactWriter,
    parent: &CapturedDocumentDirectory,
    operation: &Operation,
    pass: &NativeIoPass,
) -> io::Result<()> {
    pass.check()?;
    parent.revalidate_io(pass)?;
    if parent.identity() != &operation.parent
        || parent.canonical_path() != operation.parent_path()?
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    writer.control.revalidate(&writer.store.scope, None)?;
    writer.store.scope.check(None)
}
fn directory_binding(
    writer: &ArtifactWriter,
    parent: &CapturedDocumentDirectory,
    operation: &Operation,
    directory: &File,
    pass: &NativeIoPass,
) -> io::Result<()> {
    parent_binding(writer, parent, operation, pass)?;
    let path = operation.directory_path()?;
    let metadata = fs::symlink_metadata(&path)?;
    let identity = operation
        .state
        .directory()
        .ok_or(io::ErrorKind::InvalidData)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || handle_id(directory)? != *identity
        || physical_id(&path, &metadata)? != *identity
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    graphit_fs::verify_private_directory(directory)?;
    pass.check()
}
fn writing_binding(
    writer: &ArtifactWriter,
    parent: &CapturedDocumentDirectory,
    operation: &Operation,
    directory: &File,
    file: &File,
    pass: &NativeIoPass,
) -> io::Result<()> {
    directory_binding(writer, parent, operation, directory, pass)?;
    graphit_fs::verify_private_file(file)?;
    let State::File { file: identity, .. } = &operation.state else {
        return Err(io::ErrorKind::InvalidData.into());
    };
    file_binding(&operation.file_path()?, file, identity)?;
    pass.check()
}
fn file_binding(path: &Path, file: &File, identity: &PhysicalId) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || handle_id(file)? != *identity
        || physical_id(path, &metadata)? != *identity
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(())
}
/// Own the pass even on absence, open/root/access/metadata failure.
fn inspect_path(
    writer: &ArtifactWriter,
    parent: &CapturedDocumentDirectory,
    operation: &Operation,
    directory: &File,
    target: Target,
    pass: NativeIoPass,
    bytes: Option<&[u8]>,
) -> io::Result<Option<NativeDocumentReceipt>> {
    directory_binding(writer, parent, operation, directory, &pass)?;
    let path = match target {
        Target::Stage => operation.file_path()?,
        Target::Destination => operation.destination.0.clone(),
    };
    let result = match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            parent.limits().check_bytes(metadata.len())?;
            let id = physical_id(&path, &metadata)?;
            if matches!(target, Target::Stage) {
                let expected = match &operation.state {
                    State::File { file, .. } => file,
                    _ => operation
                        .state
                        .stage()
                        .map(NativeDocumentReceipt::file_id)
                        .ok_or(io::ErrorKind::InvalidData)?,
                };
                if &id != expected {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
            let mut file = open_file(&path)?;
            file_binding(&path, &file, &id)?;
            return inspect_file(&path, &mut file, parent.limits(), pass, bytes, |pass| {
                directory_binding(writer, parent, operation, directory, pass)
            })
            .map(Some);
        }
    };
    directory_binding(writer, parent, operation, directory, &pass)?;
    Ok(result)
}
fn classify(
    operation: &Operation,
    actual_stage: Option<&NativeDocumentReceipt>,
    destination: Option<&NativeDocumentReceipt>,
) -> io::Result<Outcome> {
    let stage = operation.state.stage().ok_or(io::ErrorKind::InvalidData)?;
    let applied = stage.destination_after(operation.previous.as_ref())?;
    if actual_stage.is_none() && destination == Some(&applied) {
        Ok(Outcome::Applied {
            destination: applied,
        })
    } else if actual_stage == Some(stage) && destination == operation.previous.as_ref() {
        Ok(Outcome::Unchanged {})
    } else {
        Err(io::ErrorKind::InvalidData.into())
    }
}
fn cleanup(
    writer: &ArtifactWriter,
    parent: &CapturedDocumentDirectory,
    operation: &Operation,
    directory: Option<File>,
) -> ArtifactResult<()> {
    let State::Acknowledged { stage, outcome, .. } = &operation.state else {
        return Err(error(
            ArtifactStage::RecoveryRequired,
            io::ErrorKind::WouldBlock,
        ));
    };
    let context = context(writer)?;
    let pass = context
        .native_inspection(0)
        .map_err(|e| at(ArtifactStage::Cleanup, e))?;
    parent_binding(writer, parent, operation, &pass).map_err(|e| at(ArtifactStage::Cleanup, e))?;
    let path = operation
        .directory_path()
        .map_err(|e| at(ArtifactStage::Cleanup, e))?;
    let directory = match directory {
        Some(directory) => Some(directory),
        None => match open_access_directory(&path) {
            Ok(directory) => Some(directory),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(at(ArtifactStage::Cleanup, e)),
        },
    };
    if let Some(directory) = directory {
        let actual = inspect_path(
            writer,
            parent,
            operation,
            &directory,
            Target::Stage,
            context
                .native_inspection(stage.length())
                .map_err(|e| at(ArtifactStage::Cleanup, e))?,
            None,
        )
        .map_err(|e| at(ArtifactStage::Cleanup, e))?;
        match outcome {
            Outcome::Applied { .. } if actual.is_some() => return Err(conflict()),
            Outcome::Unchanged {} | Outcome::NotPublished {}
                if actual.as_ref().is_some_and(|actual| actual != stage) =>
            {
                return Err(conflict());
            }
            _ => {}
        }
        // Admit one bounded directory listing; any foreign child prevents deletion.
        for entry in fs::read_dir(&path).map_err(|e| at(ArtifactStage::Cleanup, e))? {
            let entry = entry.map_err(|e| at(ArtifactStage::Cleanup, e))?;
            if entry.file_name() != "document" || actual.is_none() {
                return Err(conflict());
            }
        }
        directory_binding(writer, parent, operation, &directory, &pass)
            .map_err(|e| at(ArtifactStage::Cleanup, e))?;
        if actual.is_some() {
            fs::remove_file(
                operation
                    .file_path()
                    .map_err(|e| at(ArtifactStage::Cleanup, e))?,
            )
            .map_err(|e| at(ArtifactStage::Cleanup, e))?;
            #[cfg(test)]
            if let Some(cleanup) = test_hooks::AFTER_UNLINK.with(|h| h.borrow_mut().take()) {
                cleanup().map_err(|e| at(ArtifactStage::Cleanup, e))?;
            }
        }
        directory_binding(writer, parent, operation, &directory, &pass)
            .map_err(|e| at(ArtifactStage::Cleanup, e))?;
        // Close before remove_dir on Windows; it only succeeds for an empty directory.
        drop(directory);
        fs::remove_dir(&path).map_err(|e| at(ArtifactStage::Cleanup, e))?;
        #[cfg(test)]
        if let Some(cleanup) = test_hooks::AFTER_REMOVE_DIRECTORY.with(|h| h.borrow_mut().take()) {
            cleanup().map_err(|e| at(ArtifactStage::Cleanup, e))?;
        }
    }
    Ok(())
}
fn create_directory(path: &Path) -> io::Result<File> {
    #[cfg(test)]
    if let Some(create) = test_hooks::CREATE_DIRECTORY.with(|h| h.borrow_mut().take()) {
        return create(path);
    }
    graphit_fs::create_private_directory(path)
}
fn create_file(path: &Path) -> io::Result<File> {
    #[cfg(test)]
    if let Some(create) = test_hooks::CREATE_FILE.with(|h| h.borrow_mut().take()) {
        return create(path);
    }
    graphit_fs::create_private_file(path)
}
fn native_publish(stage: &Path, destination: &Path, exists: bool) -> io::Result<()> {
    #[cfg(test)]
    if let Some(native) = test_hooks::NATIVE.with(|h| h.borrow_mut().take()) {
        return native(stage, destination, exists);
    }
    if !exists {
        graphit_fs::rename_exclusive(stage, destination)
    } else {
        #[cfg(windows)]
        {
            graphit_fs::replace_existing_file(stage, destination)
        }
        #[cfg(unix)]
        {
            fs::rename(stage, destination)
        }
    }
}
#[cfg(test)]
pub(super) mod test_hooks {
    use super::*;
    use std::cell::RefCell;
    type Create = Box<dyn FnOnce(&Path) -> io::Result<File>>;
    type Action = RefCell<Option<Box<dyn FnOnce()>>>;
    type Sync = RefCell<Option<Box<dyn FnOnce(&File) -> io::Result<()>>>>;
    type Cleanup = RefCell<Option<Box<dyn FnOnce() -> io::Result<()>>>>;
    type Native = Box<dyn FnOnce(&Path, &Path, bool) -> io::Result<()>>;
    thread_local! {
        pub static BEFORE_TERMINAL_RECORD: Action = const { RefCell::new(None) };
        pub static AFTER_TERMINAL_RECORD: Action = const { RefCell::new(None) };
        pub static AFTER_CLEAR_OPERATION: Action = const { RefCell::new(None) };
        pub static CREATE_DIRECTORY: RefCell<Option<Create>> = RefCell::new(None);
        pub static CREATE_FILE: RefCell<Option<Create>> = RefCell::new(None);
        pub static NATIVE: RefCell<Option<Native>> = RefCell::new(None);
        pub static AFTER_UNLINK: Cleanup = const { RefCell::new(None) };
        pub static AFTER_REMOVE_DIRECTORY: Cleanup = const { RefCell::new(None) };
        pub static AFTER_SYNC: Sync = const { RefCell::new(None) };
        pub static AFTER_PUBLISHING_RECORD: Action = const { RefCell::new(None) };
        pub static AFTER_WRITE: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
        pub static AFTER_DIRECTORY_RECORD: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
        pub static BEFORE_RENAME: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
        pub static AFTER_RENAME: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
    }
    pub fn run(hook: &'static std::thread::LocalKey<Action>) {
        if let Some(hook) = hook.with(|h| h.borrow_mut().take()) {
            hook();
        }
    }
}
