//! Source-guarded exclusive first publication. A generic Reader and H remain ordinary data.
use crate::{
    knowledge_index::{
        self as index, FirstPublicationPhase, KnowledgeFirstPublication, KnowledgeGeneration,
        KnowledgeIndexError, KnowledgeIndexWriteLimits,
    },
    lance_module_store::{LanceModuleStore, PreparedCreate, RecoveryKey},
};
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::{artifact_store::CapturedSourceReader, file_io::SnapshotContext};
use std::{error::Error, fmt, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeSourcePublicationEffect {
    BeforeCommit,
    CommitOutcomeUnknown,
    PublicationObserved,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeSourcePublicationPhase {
    Admission,
    Image,
    Writer,
    SourceBeforePrepare,
    Prepare,
    SourceBeforeCommit,
    Commit,
    SourceAfterDefault,
    Named,
    SourceAfterNamed,
    Delivery,
}
#[derive(Debug)]
pub struct KnowledgeSourceFirstPublicationError {
    phase: KnowledgeSourcePublicationPhase,
    effect: KnowledgeSourcePublicationEffect,
    recovery_key: Option<Box<RecoveryKey>>,
    publication: Option<Box<KnowledgeFirstPublication>>,
    cause: Box<KnowledgeIndexError>,
}
impl KnowledgeSourceFirstPublicationError {
    pub fn phase(&self) -> KnowledgeSourcePublicationPhase {
        self.phase
    }
    pub fn effect(&self) -> KnowledgeSourcePublicationEffect {
        self.effect
    }
    pub fn recovery_key(&self) -> Option<&RecoveryKey> {
        self.recovery_key.as_deref()
    }
    pub fn known_publication(&self) -> Option<&KnowledgeFirstPublication> {
        self.publication.as_deref()
    }
    pub fn cause(&self) -> &KnowledgeIndexError {
        &self.cause
    }
}
impl fmt::Display for KnowledgeSourceFirstPublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Knowledge Source publication at {:?} ({:?}): {}",
            self.phase, self.effect, self.cause
        )
    }
}
impl Error for KnowledgeSourceFirstPublicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
fn failure(
    phase: KnowledgeSourcePublicationPhase,
    effect: KnowledgeSourcePublicationEffect,
    key: Option<&RecoveryKey>,
    publication: Option<KnowledgeFirstPublication>,
    cause: KnowledgeIndexError,
) -> KnowledgeSourceFirstPublicationError {
    KnowledgeSourceFirstPublicationError {
        phase,
        effect,
        recovery_key: key.map(|key| Box::new(key.clone())),
        publication: publication.map(Box::new),
        cause: Box::new(cause),
    }
}
fn check_authority(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
) -> Result<(), KnowledgeIndexError> {
    let r = index::check_read_project(original, read)?;
    let w = index::check_write_project(original, write)?;
    if r.scope.live.owner() != w.scope.live.owner()
        || r.scope.live.runtime_root() != w.scope.live.runtime_root()
        || r.version.version() != w.version.version()
        || r.version.lance_branch() != w.version.lance_branch()
    {
        return Err(KnowledgeIndexError::InvalidField("admission_scope"));
    }
    source.check_request(original)?;
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceFirstPhase {
    BeforeWriter,
    WriterWaiting,
    WriterAcquired,
    DirectoryCreated,
    Prepared,
    BeforeCommit,
    DefaultConfirmed,
    NamedConfirmed,
    Delivery,
}
async fn acquire_target(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
    limits: KnowledgeIndexWriteLimits,
    observe: &(dyn Fn(SourceFirstPhase) + Sync),
) -> Result<index::NativeFirstPublicationTarget, KnowledgeSourceFirstPublicationError> {
    use KnowledgeSourcePublicationEffect as Effect;
    use KnowledgeSourcePublicationPhase as Phase;
    let authority = || check_authority(original, read, write, source);
    let target = index::acquire_native_first_publication_target(
        original,
        write,
        limits,
        &|step| match step {
            FirstPublicationPhase::BeforeWriter => observe(SourceFirstPhase::BeforeWriter),
            FirstPublicationPhase::WriterWaiting => observe(SourceFirstPhase::WriterWaiting),
            FirstPublicationPhase::WriterAcquired => observe(SourceFirstPhase::WriterAcquired),
            _ => (),
        },
        authority,
    )
    .await
    .map_err(|cause| failure(Phase::Writer, Effect::BeforeCommit, None, None, cause))?;
    observe(SourceFirstPhase::DirectoryCreated);
    Ok(target)
}

fn check_target(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
    target: &index::NativeFirstPublicationTarget,
) -> Result<(), KnowledgeIndexError> {
    check_authority(original, read, write, source)?;
    target.guard.revalidate(original)?;
    Ok(())
}

fn check_physical(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
    target: &index::NativeFirstPublicationTarget,
    file: Option<&arrow_array::RecordBatch>,
) -> Result<(), KnowledgeIndexError> {
    check_target(original, read, write, source, target)?;
    if let Some(file) = file {
        super::reconciliation::validate_source_file(original, source, file)?;
    } else {
        source.revalidate_content()?;
    }
    check_target(original, read, write, source, target)
}

#[allow(clippy::too_many_arguments)]
async fn finish_prepared(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    source: &CapturedSourceReader,
    target: index::NativeFirstPublicationTarget,
    prepared: PreparedCreate,
    file: Option<&arrow_array::RecordBatch>,
    observe: &(dyn Fn(SourceFirstPhase) + Sync),
) -> Result<(KnowledgeFirstPublication, RecoveryKey), KnowledgeSourceFirstPublicationError> {
    use KnowledgeSourcePublicationEffect as Effect;
    use KnowledgeSourcePublicationPhase as Phase;
    let before = || check_target(original, read, write, source, &target);
    let physical = || check_physical(original, read, write, source, &target, file);
    // Capture the real key before commit: from_dataset can return a post-SDK error without one.
    let key = prepared.recovery_key().clone();
    observe(SourceFirstPhase::Prepared);
    observe(SourceFirstPhase::BeforeCommit);
    physical().map_err(|cause| {
        failure(
            Phase::SourceBeforeCommit,
            Effect::BeforeCommit,
            Some(&key),
            None,
            cause,
        )
    })?;
    let (store, committed_key) = prepared.commit().await.map_err(|cause| {
        failure(
            Phase::Commit,
            Effect::CommitOutcomeUnknown,
            Some(&key),
            None,
            cause.into(),
        )
    })?;
    let default = KnowledgeGeneration {
        corpus_uri: target.uri.clone(),
        version: store.snapshot().version(),
        physical_branch: None,
        inherited_target: None,
    };
    let known = KnowledgeFirstPublication {
        default: default.clone(),
        named: None,
    };
    observe(SourceFirstPhase::DefaultConfirmed);
    physical().map_err(|cause| {
        failure(
            Phase::SourceAfterDefault,
            Effect::PublicationObserved,
            Some(&key),
            Some(known),
            cause,
        )
    })?;
    let publication = index::finish_initial_publication_observed(
        &target.uri,
        default,
        target.encoded_branch.as_deref(),
        before,
        || {
            observe(SourceFirstPhase::NamedConfirmed);
            physical()
        },
    )
    .await
    .map_err(|failed| {
        let phase = if failed.publication.named.is_some() {
            Phase::SourceAfterNamed
        } else {
            Phase::Named
        };
        failure(
            phase,
            Effect::PublicationObserved,
            Some(&key),
            Some(*failed.publication),
            *failed.cause,
        )
    })?;
    observe(SourceFirstPhase::Delivery);
    physical().map_err(|cause| {
        failure(
            Phase::Delivery,
            Effect::PublicationObserved,
            Some(&key),
            Some(publication.clone()),
            cause,
        )
    })?;
    drop(store);
    drop(target);
    Ok((publication, committed_key))
}

mod stream;
pub use stream::{publish_native_knowledge_source_file, publish_native_knowledge_source_stream};

#[cfg(test)]
mod tests;
