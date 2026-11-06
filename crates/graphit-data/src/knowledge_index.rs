//! Native Knowledge publication and complete owned reads of one captured generation.
//!
//! Arrow and SDK handles stay private. Capture and authorization are checked at public
//! SDK boundaries; pathname I/O does not provide continuous confinement.

pub(crate) mod semantic;

use crate::knowledge_document_projection::{
    AUTHORED_RECIPE_ID, CompilationDigest32, DocumentHeader, KnowledgeDocumentProjectionError,
    MemberRow, PhysicalSourceDigest32, PreparedKnowledgeDocumentRows,
};
use crate::knowledge_references::{
    self as refs, KnowledgeDocumentState, PreparedKnowledgeReferences,
};
use crate::{lance_branch_store as lance_branch, lance_module_store as lance_foundation};
use arrow_array::{
    Array, ArrayRef, BooleanArray, DurationNanosecondArray, FixedSizeBinaryArray, ListArray,
    RecordBatch, StringArray, StructArray, TimestampNanosecondArray, UInt8Array, UInt64Array,
    builder::{FixedSizeBinaryBuilder, ListBuilder, StringBuilder},
};
use arrow_schema::{ArrowError, DataType, Field, Fields, Schema, TimeUnit};
use graphit_auth::{
    AuthError, ProjectCapture, ProjectReadAdmission, ProjectReadModule, ProjectWriteAdmission,
    ProjectWriteOperation,
};
use graphit_core::{artifact_store::StoreOwner, project_identity::ProjectId};
use graphit_core::{
    artifact_store::{
        ArtifactError, ArtifactLimits, ArtifactStore, ArtifactWriter, CapturedPublicationScope,
        ProjectRecordDirectory,
    },
    file_io::SnapshotContext,
    native_document_file::{
        CapturedProjectDocumentDirectory, DirectoryPresence, NativeDocumentFileLimits,
    },
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    error::Error,
    ffi::OsStr,
    fmt, io,
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug)]
pub struct KnowledgeIndexContentLimits {
    pub max_identifier_bytes: NonZeroUsize,
}

#[derive(Clone, Copy)]
pub struct KnowledgeIndexReadLimits {
    pub content: KnowledgeIndexContentLimits,
    pub native: NativeDocumentFileLimits,
    pub batch_rows: NonZeroU32,
}
/// Bounds on admitted head/witness metadata; the body is not projected.
#[derive(Clone, Copy)]
pub struct KnowledgeHeadReadLimits {
    pub max_slug_bytes: NonZeroUsize,
    pub native: NativeDocumentFileLimits,
    pub batch_rows: NonZeroU32,
}
#[derive(Clone, Copy)]
pub struct KnowledgeIndexWriteLimits {
    pub content: KnowledgeIndexContentLimits,
    pub native: NativeDocumentFileLimits,
    pub artifacts: ArtifactLimits,
}

/// Storage observations; they do not authorize a read or a subsequent write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeGeneration {
    pub corpus_uri: String,
    pub version: u64,
    pub physical_branch: Option<String>,
    pub inherited_target: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeFirstPublication {
    pub default: KnowledgeGeneration,
    pub named: Option<KnowledgeGeneration>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeIndexRead {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        documents: Vec<KnowledgeDocument>,
    },
}

/// Compact temporal observations from validated Active rows, never authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeDocumentDates {
    pub slug: String,
    pub updated_at: Option<SystemTime>,
    pub stale_since: Option<SystemTime>,
    pub stale_after: Option<Duration>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeDateRead {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        dates: Vec<KnowledgeDocumentDates>,
    },
}

/// One selected local page after selected-stream EOF on the admitted generation.
#[derive(Debug, Eq, PartialEq)]
pub enum KnowledgeIndexPageRead {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        page: crate::knowledge_snapshot::KnowledgeSnapshotPage,
    },
}

/// Persisted metadata; an absent date and an empty title are retained literally.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeHead {
    pub slug: String,
    pub title: String,
    pub updated_at: Option<SystemTime>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeHeadRead {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        heads: Vec<KnowledgeHead>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeDocument {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub kind: String,
    pub summary: String,
    pub description: Option<String>,
    pub summary_truncated: bool,
    pub source_path: String,
    pub physical_digest: Option<PhysicalSourceDigest32>,
    pub compilation_digest: CompilationDigest32,
    pub recipe: String,
    pub recipe_parameters: KnowledgeRecipeParameters,
    pub updated_at: Option<SystemTime>,
    pub stale_since: Option<SystemTime>,
    pub stale_after: Option<Duration>,
    pub resolved_wiki_targets: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeRecipeParameters {
    pub fallback_title: String,
    pub kind_on_absent: String,
    pub max_summary_bytes: NonZeroUsize,
}

#[derive(Debug)]
pub enum KnowledgeIndexError {
    Auth(AuthError),
    Artifact(ArtifactError),
    Request(io::Error),
    Projection(KnowledgeDocumentProjectionError),
    ReferenceSerialization(serde_json::Error),
    Arrow(ArrowError),
    Store(lance_foundation::StoreError),
    InputStream(Box<lance_foundation::DataFusionError>),
    Branch(lance_branch::BranchError),
    InvalidField(&'static str),
    LimitExceeded(&'static str),
    Overflow(&'static str),
    DuplicateSlug,
    WrongAdmission,
    DifferentRequest,
    WriterAcquisition(tokio::task::JoinError),
    InitialTargetChanged,
    PartialDefault {
        generation: KnowledgeGeneration,
        cause: Box<KnowledgeIndexError>,
    },
}
impl KnowledgeIndexError {
    // Auth may observe expiry inside its recheck, after our initial check.
    // Recover only this same Original's real error; retain every other cause.
    fn with_original_request(self, original: &SnapshotContext) -> Self {
        match self {
            Self::Auth(AuthError::Request) => original
                .check()
                .err()
                .map(Self::Request)
                .unwrap_or(Self::Auth(AuthError::Request)),
            cause => cause,
        }
    }
}

#[cfg(test)]
mod request_cause_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::Instant;

    #[test]
    fn auth_request_recovers_same_original_cancellation_and_deadline() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            cancellation.clone(),
        );
        original.check().unwrap();
        cancellation.cancel();
        let cause = KnowledgeIndexError::Auth(AuthError::Request).with_original_request(&original);
        assert!(
            matches!(cause, KnowledgeIndexError::Request(e) if e.kind() == io::ErrorKind::Interrupted)
        );

        let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
        let cause = KnowledgeIndexError::Auth(AuthError::Request).with_original_request(&expired);
        assert!(
            matches!(cause, KnowledgeIndexError::Request(e) if e.kind() == io::ErrorKind::TimedOut)
        );
    }

    #[test]
    fn live_auth_request_denial_and_existing_io_keep_their_cause() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            cancellation.clone(),
        );
        assert!(matches!(
            KnowledgeIndexError::Auth(AuthError::Request).with_original_request(&original),
            KnowledgeIndexError::Auth(AuthError::Request)
        ));
        cancellation.cancel();
        for auth in [AuthError::Denied, AuthError::Expired, AuthError::State] {
            assert!(matches!(
                KnowledgeIndexError::Auth(auth).with_original_request(&original),
                KnowledgeIndexError::Auth(actual) if actual == auth
            ));
        }
        let inner = Box::new(io::Error::other("existing IO cause"));
        let pointer = &*inner as *const io::Error;
        let inner: Box<dyn Error + Send + Sync> = inner;
        let cause =
            KnowledgeIndexError::Request(io::Error::new(io::ErrorKind::PermissionDenied, inner))
                .with_original_request(&original);
        let KnowledgeIndexError::Request(actual) = cause else {
            panic!("existing IO category changed");
        };
        assert_eq!(actual.kind(), io::ErrorKind::PermissionDenied);
        assert!(std::ptr::eq(
            actual
                .get_ref()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap(),
            pointer,
        ));
    }
}
impl fmt::Display for KnowledgeIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge index: {self:?}")
    }
}
impl Error for KnowledgeIndexError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(e) => Some(e),
            Self::Artifact(e) => Some(e),
            Self::Request(e) => Some(e),
            Self::Projection(e) => Some(e),
            Self::ReferenceSerialization(e) => Some(e),
            Self::Arrow(e) => Some(e),
            Self::Store(e) => Some(e),
            Self::InputStream(e) => Some(e.as_ref()),
            Self::Branch(e) => Some(e),
            Self::PartialDefault { cause, .. } => Some(cause.as_ref()),
            Self::WriterAcquisition(e) => Some(e),
            _ => None,
        }
    }
}
impl From<AuthError> for KnowledgeIndexError {
    fn from(e: AuthError) -> Self {
        Self::Auth(e)
    }
}
impl From<ArtifactError> for KnowledgeIndexError {
    fn from(e: ArtifactError) -> Self {
        Self::Artifact(e)
    }
}
impl From<io::Error> for KnowledgeIndexError {
    fn from(e: io::Error) -> Self {
        Self::Request(e)
    }
}
impl From<ArrowError> for KnowledgeIndexError {
    fn from(e: ArrowError) -> Self {
        Self::Arrow(e)
    }
}
impl From<KnowledgeDocumentProjectionError> for KnowledgeIndexError {
    fn from(e: KnowledgeDocumentProjectionError) -> Self {
        Self::Projection(e)
    }
}
impl From<lance_foundation::StoreError> for KnowledgeIndexError {
    fn from(e: lance_foundation::StoreError) -> Self {
        Self::Store(e)
    }
}
impl From<lance_branch::BranchError> for KnowledgeIndexError {
    fn from(e: lance_branch::BranchError) -> Self {
        Self::Branch(e)
    }
}

pub(crate) fn check_read(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
) -> Result<(), KnowledgeIndexError> {
    check_read_project(original, admission).map(|_| ())
}

pub(crate) fn check_read_project<'admission, 'request>(
    original: &Arc<SnapshotContext>,
    admission: &'admission ProjectReadAdmission<'request>,
) -> Result<&'admission ProjectCapture<'request>, KnowledgeIndexError> {
    if admission.module() != ProjectReadModule::Knowledge {
        return Err(KnowledgeIndexError::WrongAdmission);
    }
    original.check()?;
    let (project, request) = admission
        .project_and_request()
        .map_err(|cause| KnowledgeIndexError::from(cause).with_original_request(original))?;
    if !Arc::ptr_eq(original, request.snapshot()) {
        return Err(KnowledgeIndexError::DifferentRequest);
    }
    Ok(project)
}
pub(crate) fn check_write(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
) -> Result<(), KnowledgeIndexError> {
    check_write_project(original, admission).map(|_| ())
}

pub(crate) fn check_write_project<'admission, 'request>(
    original: &Arc<SnapshotContext>,
    admission: &'admission ProjectWriteAdmission<'request>,
) -> Result<&'admission ProjectCapture<'request>, KnowledgeIndexError> {
    if admission.operation() != ProjectWriteOperation::Knowledge {
        return Err(KnowledgeIndexError::WrongAdmission);
    }
    original.check()?;
    let (project, request) = admission
        .project_and_request()
        .map_err(|cause| KnowledgeIndexError::from(cause).with_original_request(original))?;
    if !Arc::ptr_eq(original, request.snapshot()) {
        return Err(KnowledgeIndexError::DifferentRequest);
    }
    Ok(project)
}
pub(crate) struct LocalKnowledgeCorpus {
    pub(crate) relative: std::path::PathBuf,
    pub(crate) path: std::path::PathBuf,
    pub(crate) uri: String,
    pub(crate) default_layout: bool,
}
pub(crate) fn local_corpus(
    scope: &CapturedPublicationScope,
    project_version: &str,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
) -> Result<LocalKnowledgeCorpus, KnowledgeIndexError> {
    let relative = graphit_core::local_corpus::resolve(
        scope,
        project_version,
        "knowledge",
        "index.lance",
        limits,
        original,
    )?;
    // This is the resolver's documented default layout, not a grant. A mapped
    // corpus may only be read/updated; first publication cannot create it.
    let default_layout = relative == Path::new("knowledge/index.lance");
    let path = scope.runtime_root().join(&relative);
    let uri = path
        .to_str()
        .ok_or(KnowledgeIndexError::InvalidField("non-Unicode corpus_path"))?
        .to_owned();
    Ok(LocalKnowledgeCorpus {
        relative,
        path,
        uri,
        default_layout,
    })
}
pub(crate) fn generation(uri: &str, pin: &lance_branch::BranchSnapshot) -> KnowledgeGeneration {
    KnowledgeGeneration {
        corpus_uri: uri.to_owned(),
        version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}
pub(crate) fn capture_index(
    scope: &CapturedPublicationScope,
    corpus: &LocalKnowledgeCorpus,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<Option<CapturedProjectDocumentDirectory>, KnowledgeIndexError> {
    before()?;
    let relative = &corpus.relative;
    let presence =
        CapturedProjectDocumentDirectory::probe_runtime(scope, relative, limits, original)?;
    before()?;
    if presence == DirectoryPresence::Absent {
        if !corpus.default_layout {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
        }
        return Ok(None);
    }
    let guard =
        CapturedProjectDocumentDirectory::capture_runtime(scope, relative, limits, original)?;
    guard.revalidate(original)?;
    before()?;
    Ok(Some(guard))
}

/// Existing native capture and artifact-writer bounds; retirement does not scan rows.
#[derive(Clone, Copy)]
pub struct KnowledgeRetirementLimits {
    pub native: NativeDocumentFileLimits,
    pub artifacts: ArtifactLimits,
}

/// One corpus observation, independent of inventory metadata and other corpora.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeBranchRetirement {
    NoCorpus,
    Retired { generation: KnowledgeGeneration },
}

/// Keeps a confirmed SDK effect even if the subsequent request recheck fails.
/// SDK errors can have uncertain effects and carry no fabricated confirmation.
pub struct KnowledgeRetirementFailure {
    cause: Box<KnowledgeIndexError>,
    confirmed: Option<KnowledgeBranchRetirement>,
}
impl KnowledgeRetirementFailure {
    pub fn error(&self) -> &KnowledgeIndexError {
        &self.cause
    }
    pub fn confirmed(&self) -> Option<&KnowledgeBranchRetirement> {
        self.confirmed.as_ref()
    }
}
impl From<KnowledgeIndexError> for KnowledgeRetirementFailure {
    fn from(cause: KnowledgeIndexError) -> Self {
        Self {
            cause: Box::new(cause),
            confirmed: None,
        }
    }
}
impl fmt::Debug for KnowledgeRetirementFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KnowledgeRetirementFailure")
            .field("sdk_effect_confirmed", &self.confirmed.is_some())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for KnowledgeRetirementFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Knowledge branch retirement failed")
    }
}
impl Error for KnowledgeRetirementFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

fn finish_retirement(
    result: Result<(), lance_branch::BranchError>,
    generation: KnowledgeGeneration,
    after: impl FnOnce() -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeBranchRetirement, KnowledgeRetirementFailure> {
    // A late cancellation must not replace an actual SDK failure.
    result.map_err(KnowledgeIndexError::Branch)?;
    let confirmed = KnowledgeBranchRetirement::Retired { generation };
    match after() {
        Ok(()) => Ok(confirmed),
        Err(cause) => Err(KnowledgeRetirementFailure {
            cause: Box::new(cause),
            confirmed: Some(confirmed),
        }),
    }
}

pub(crate) async fn retire_corpus(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: KnowledgeRetirementLimits,
) -> Result<KnowledgeBranchRetirement, KnowledgeRetirementFailure> {
    let project = check_write_project(original, admission)?;
    let scope = project.scope.live;
    let branch = project.version.lance_branch();
    if branch.is_empty() {
        return Err(KnowledgeIndexError::Branch(lance_branch::BranchError::DefaultSnapshot).into());
    }
    let corpus = local_corpus(scope, project.version.version(), original, limits.native)?;
    let capture = || {
        capture_index(scope, &corpus, original, limits.native, || {
            check_write(original, admission)
        })
    };
    if capture()?.is_none() {
        check_write(original, admission)?;
        return Ok(KnowledgeBranchRetirement::NoCorpus);
    }
    let artifacts = ArtifactStore::new(scope.clone(), limits.artifacts);
    let request = Arc::clone(original);
    let writer = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request))
        .await
        .map_err(KnowledgeIndexError::WriterAcquisition)?
        .map_err(KnowledgeIndexError::Artifact)?;
    let Some(guard) = capture()? else {
        check_write(original, admission)?;
        return Ok(KnowledgeBranchRetirement::NoCorpus);
    };
    let before = || {
        check_write(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    let uri = corpus.uri.clone();
    before()?;
    let pin = lance_branch::BranchSnapshot::capture_observed(&uri, Some(&branch), None)
        .await
        .map_err(KnowledgeIndexError::Branch)?;
    before()?;
    let observed = generation(&uri, &pin);
    let result = finish_retirement(pin.retire_named().await, observed, before);
    drop(writer);
    result
}

/// Retire only the originally admitted physical named index branch, once.
/// Does not create a corpus, materialize inherited targets or alter inventory.
pub async fn retire_native_knowledge_index(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: KnowledgeRetirementLimits,
) -> Result<KnowledgeBranchRetirement, KnowledgeRetirementFailure> {
    retire_corpus(original, admission, limits).await
}

#[cfg(test)]
mod retirement_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::Instant;

    async fn named(uri: &str) -> lance_branch::BranchSnapshot {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::UInt64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(UInt64Array::from(vec![7]))],
        )
        .unwrap();
        lance_foundation::LanceModuleStore::create(uri, Arc::clone(&schema), vec![batch])
            .await
            .unwrap();
        lance_branch::LanceBranchStore::open(uri, schema, None)
            .await
            .unwrap()
            .capture_target(Some("named"))
            .await
            .unwrap()
            .materialize()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn index_retirement_actual_sdk_success_retains_confirmation_after_original_cancellation()
    {
        let dir = tempfile::tempdir().unwrap();
        let uri = url::Url::from_file_path(dir.path().join("index.lance"))
            .unwrap()
            .to_string();
        let pin = named(&uri).await;
        let observed = generation(&uri, &pin);
        let cancel = Cancellation::default();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancel.clone(),
        ));
        original.check().unwrap();
        let result = pin.retire_named().await;
        cancel.cancel();
        let failure = finish_retirement(result, observed.clone(), || {
            original.check().map_err(KnowledgeIndexError::Request)
        })
        .unwrap_err();
        assert_eq!(
            failure.confirmed(),
            Some(&KnowledgeBranchRetirement::Retired {
                generation: observed
            })
        );
        assert!(
            matches!(failure.error(), KnowledgeIndexError::Request(e) if e.kind()==io::ErrorKind::Interrupted)
        );
        let sdk = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
            .load()
            .await
            .unwrap();
        assert!(matches!(
            sdk.branches().get("named").await,
            Err(lance::Error::RefNotFound { .. })
        ));
    }

    #[tokio::test]
    async fn index_retirement_actual_sdk_error_keeps_first_cause_over_late_original_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let uri = url::Url::from_file_path(dir.path().join("index.lance"))
            .unwrap()
            .to_string();
        let pin = named(&uri).await;
        let observed = generation(&uri, &pin);
        let sdk = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
            .load()
            .await
            .unwrap();
        sdk.tags()
            .create(
                "retained",
                lance::dataset::refs::Ref::Version(Some("named".into()), Some(pin.version())),
            )
            .await
            .unwrap();
        let cancel = Cancellation::default();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancel.clone(),
        ));
        original.check().unwrap();
        let result = pin.retire_named().await;
        cancel.cancel();
        let mut checked = false;
        let failure = finish_retirement(result, observed, || {
            checked = true;
            original.check().map_err(KnowledgeIndexError::Request)
        })
        .unwrap_err();
        assert!(!checked);
        assert!(failure.confirmed().is_none());
        assert!(
            matches!(failure.error(), KnowledgeIndexError::Branch(lance_branch::BranchError::Sdk(e)) if matches!(e.as_ref(), lance::Error::RefConflict { .. }))
        );
        assert!(sdk.branches().get("named").await.is_ok());
    }
}

/// Acquire active documents from the complete native corpus. No output precedes EOF.
pub async fn read_native_knowledge_index(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeIndexRead, KnowledgeIndexError> {
    let read = scan_native_knowledge_index(
        original,
        admission,
        limits,
        |batch, counts, rows, before| decode_batch(batch, limits.content, counts, rows, before),
        |row: &KnowledgeDocument| row.slug.as_str(),
    )
    .await?;
    Ok(match read {
        None => KnowledgeIndexRead::NoCorpus,
        Some((generation, documents)) => KnowledgeIndexRead::Published {
            generation,
            documents,
        },
    })
}

/// Validate every FULL18 row through EOF while retaining only temporal metadata.
/// Source bodies remain in local SDK batches and are never copied into DTOs.
pub async fn read_native_knowledge_dates(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeDateRead, KnowledgeIndexError> {
    let read = scan_native_knowledge_index(
        original,
        admission,
        limits,
        |batch, counts, rows, before| {
            decode_batch_inner(
                batch,
                limits.content,
                counts,
                None,
                Some(rows),
                None,
                before,
            )
        },
        |row: &KnowledgeDocumentDates| row.slug.as_str(),
    )
    .await?;
    Ok(match read {
        None => KnowledgeDateRead::NoCorpus,
        Some((generation, dates)) => KnowledgeDateRead::Published { generation, dates },
    })
}

// Only an unpinned Main open can observe no published corpus. Preparation may
// leave SDK fragments without publishing Main; their presence grants no read or
// recovery authority. Recheck the capture and admission even on an SDK error.
pub(crate) async fn open_local_index(
    corpus: &LocalKnowledgeCorpus,
    branch: Option<&str>,
    before: &impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<Option<lance_branch::BranchSnapshot>, KnowledgeIndexError> {
    let pin = open_native_index(&corpus.uri, branch, before).await?;
    if pin.is_none() && !corpus.default_layout {
        return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
    }
    Ok(pin)
}
pub(crate) async fn open_native_index(
    uri: &str,
    branch: Option<&str>,
    before: &impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<Option<lance_branch::BranchSnapshot>, KnowledgeIndexError> {
    before()?;
    let opened = lance_branch::BranchSnapshot::capture_observed(uri, branch, None).await;
    let rechecked = before();
    match opened {
        Ok(pin) => {
            rechecked?;
            semantic::dimensions(pin.schema())?;
            Ok(Some(pin))
        }
        Err(lance_branch::BranchError::Sdk(cause))
            if matches!(
                cause.as_ref(),
                lance_branch::LanceError::DatasetNotFound { .. }
            ) =>
        {
            rechecked?;
            Ok(None)
        }
        // Revalidation ran, but cannot replace an already causal SDK failure.
        Err(cause) => Err(cause.into()),
    }
}

async fn scan_native_knowledge_index<T>(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: KnowledgeIndexReadLimits,
    mut visit: impl FnMut(
        &RecordBatch,
        &mut NamespaceValidation,
        &mut Vec<T>,
        &dyn Fn() -> Result<(), KnowledgeIndexError>,
    ) -> Result<(), KnowledgeIndexError>,
    slug: impl for<'row> Fn(&'row T) -> &'row str,
) -> Result<Option<(KnowledgeGeneration, Vec<T>)>, KnowledgeIndexError> {
    use futures::TryStreamExt;
    let project = check_read_project(original, admission)?;
    let scope = project.scope.live;
    let branch = project.version.lance_branch();
    let encoded_branch = (!branch.is_empty()).then_some(branch.as_str());
    let corpus = local_corpus(scope, project.version.version(), original, limits.native)?;
    let Some(guard) = capture_index(scope, &corpus, original, limits.native, || {
        check_read(original, admission)
    })?
    else {
        check_read(original, admission)?;
        return Ok(None);
    };
    let before = || {
        check_read(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    before()?;
    let uri = corpus.uri.clone();
    let Some(pin) = open_local_index(&corpus, encoded_branch, &before).await? else {
        return Ok(None);
    };
    before()?;
    let generation = generation(&uri, &pin);
    let mut scanner = pin.scan();
    scanner
        .project(&semantic::DOCUMENT_COLUMNS)
        .map_err(lance_foundation::StoreError::from)?;
    scanner.batch_size(limits.batch_rows.get() as usize);
    let mut stream = scanner
        .try_into_stream()
        .await
        .map_err(lance_foundation::StoreError::from)?;
    before()?;
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission);
    };
    let mut counts = NamespaceValidation {
        expected_project: Some(*project_id),
    };
    let mut seen_slugs = BTreeSet::new();
    let mut rows = Vec::new();
    loop {
        before()?;
        let next = stream
            .try_next()
            .await
            .map_err(lance_foundation::StoreError::from)?;
        before()?;
        let Some(batch) = next else {
            break;
        };
        visit(&batch, &mut counts, &mut rows, &before)?;
        let slugs: &StringArray = array(&batch, 0)?;
        for slug in slugs.iter() {
            before()?;
            if !seen_slugs.insert(
                slug.ok_or(KnowledgeIndexError::InvalidField("slug"))?
                    .to_owned(),
            ) {
                return Err(KnowledgeIndexError::DuplicateSlug);
            }
        }
    }
    drop(stream);
    drop(scanner);
    drop(pin);
    before()?;
    rows.sort_unstable_by(|a, b| slug(a).cmp(slug(b)));
    for pair in rows.windows(2) {
        before()?;
        if slug(&pair[0]) == slug(&pair[1]) {
            return Err(KnowledgeIndexError::DuplicateSlug);
        }
    }
    before()?;
    Ok(Some((generation, rows)))
}

/// Read one exact slug from the admitted local pin through selected-stream EOF.
/// Duplicate, namespace and integrity checks apply to matching records only.
pub async fn read_native_knowledge_page(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    slug: &str,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeIndexPageRead, KnowledgeIndexError> {
    let project = check_read_project(original, admission)?;
    validate_slug(slug)?;
    if slug.len() > limits.content.max_identifier_bytes.get() {
        return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"));
    }
    let scope = project.scope.live;
    let branch = project.version.lance_branch();
    let encoded_branch = (!branch.is_empty()).then_some(branch.as_str());
    let corpus = local_corpus(scope, project.version.version(), original, limits.native)?;
    let Some(guard) = capture_index(scope, &corpus, original, limits.native, || {
        check_read(original, admission)
    })?
    else {
        check_read(original, admission)?;
        return Ok(KnowledgeIndexPageRead::NoCorpus);
    };
    let before = || {
        check_read(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    before()?;
    let uri = corpus.uri.clone();
    let Some(pin) = open_local_index(&corpus, encoded_branch, &before).await? else {
        return Ok(KnowledgeIndexPageRead::NoCorpus);
    };
    before()?;
    let generation = generation(&uri, &pin);
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission);
    };
    let page = crate::knowledge_snapshot::read_page_guarded(
        original,
        pin.schema(),
        || pin.scan(),
        slug,
        limits,
        Some(*project_id),
        before,
    )
    .await?;
    drop(pin);
    before()?;
    Ok(KnowledgeIndexPageRead::Published { generation, page })
}

/// Read heads and validate their state/witness metadata on one pin through EOF.
/// The body is not projected or validated. Removed rows count toward input bounds
/// and are validated before exclusion. Results are sorted without a page limit.
pub async fn read_native_knowledge_heads(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: KnowledgeHeadReadLimits,
) -> Result<KnowledgeHeadRead, KnowledgeIndexError> {
    use futures::TryStreamExt;
    let project = check_read_project(original, admission)?;
    let scope = project.scope.live;
    let branch = project.version.lance_branch();
    let encoded_branch = (!branch.is_empty()).then_some(branch.as_str());
    let corpus = local_corpus(scope, project.version.version(), original, limits.native)?;
    let Some(guard) = capture_index(scope, &corpus, original, limits.native, || {
        check_read(original, admission)
    })?
    else {
        check_read(original, admission)?;
        return Ok(KnowledgeHeadRead::NoCorpus);
    };
    let before = || {
        check_read(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    before()?;
    let uri = corpus.uri.clone();
    let Some(pin) = open_local_index(&corpus, encoded_branch, &before).await? else {
        return Ok(KnowledgeHeadRead::NoCorpus);
    };
    let generation = generation(&uri, &pin);
    let mut scanner = pin.scan();
    scanner
        .project(&[
            "slug",
            "title",
            "updated_at",
            "source_path",
            "compilation_digest",
            "resolved_wiki_targets",
            "document_state",
            "reference_projection",
        ])
        .map_err(lance_foundation::StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let stream = scanner.try_into_stream().await;
    let rechecked = before();
    let mut stream = stream.map_err(lance_foundation::StoreError::from)?;
    rechecked?;
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission);
    };
    let mut counts = NamespaceValidation {
        expected_project: Some(*project_id),
    };
    let mut seen_slugs = BTreeSet::new();
    let mut heads = Vec::new();
    loop {
        before()?;
        let next = stream.try_next().await;
        let rechecked = before();
        let next = next.map_err(lance_foundation::StoreError::from)?;
        rechecked?;
        let Some(batch) = next else {
            break;
        };
        let decoded = (|| -> Result<(), KnowledgeIndexError> {
            decode_head_batch(&batch, limits, &mut counts, &mut heads, || {
                original.check().map_err(KnowledgeIndexError::from)
            })?;
            let slugs: &StringArray = array(&batch, 0)?;
            for slug in slugs.iter() {
                original.check()?;
                if !seen_slugs.insert(
                    slug.ok_or(KnowledgeIndexError::InvalidField("slug"))?
                        .to_owned(),
                ) {
                    return Err(KnowledgeIndexError::DuplicateSlug);
                }
            }
            Ok(())
        })();
        before()?;
        decoded?;
    }
    drop(stream);
    drop(scanner);
    drop(pin);
    before()?;
    heads.sort_unstable_by(|a, b| a.slug.cmp(&b.slug));
    let ordered = (|| -> Result<(), KnowledgeIndexError> {
        for pair in heads.windows(2) {
            original.check()?;
            if pair[0].slug == pair[1].slug {
                return Err(KnowledgeIndexError::DuplicateSlug);
            }
        }
        Ok(())
    })();
    before()?;
    ordered?;
    Ok(KnowledgeHeadRead::Published { generation, heads })
}

/// Exclusively publish a new corpus; an existing knowledge directory is never adopted.
/// A failure after default confirmation reports that default and preserves its cause.
pub async fn publish_native_authored(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError> {
    publish_native_authored_inner(original, admission, rows, None, limits).await
}

/// Publish complete typed references and document state with the same atomic batch.
pub async fn publish_native_authored_with_references(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &PreparedKnowledgeReferences<'_, '_>,
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError> {
    publish_native_authored_inner(original, admission, rows, Some(references), limits).await
}

async fn publish_native_authored_inner(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: Option<&PreparedKnowledgeReferences<'_, '_>>,
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError> {
    let project = check_write_project(original, admission)?;
    let scope = project.scope.live;
    if let Some(references) = references {
        references.check(original)?;
        let StoreOwner::Project { project_id } = scope.owner() else {
            return Err(KnowledgeIndexError::WrongAdmission);
        };
        refs::check_native_source(references, *project_id)?;
    }
    let batch = encode_with_references(rows, references, original, limits.content, || {
        check_write(original, admission)
    })?;
    publish_native_authored_batch(original, admission, batch, limits, &|_| {}).await
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FirstPublicationPhase {
    RowsValidated,
    ReferencesEncoded,
    ImageValidated,
    BeforeWriter,
    WriterWaiting,
    WriterAcquired,
    DefaultConfirmed,
    Delivery,
}

// Only internal encoders may reach this sink. The admissions still select and
// authorize the exclusive native directory and the authenticated branch.
pub(crate) async fn publish_native_authored_batch(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    batch: RecordBatch,
    limits: KnowledgeIndexWriteLimits,
    phase: &(dyn Fn(FirstPublicationPhase) + Sync),
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError> {
    check_write(original, admission)?;
    let NativeFirstPublicationTarget {
        writer,
        guard,
        uri,
        encoded_branch,
    } = acquire_native_first_publication_target(original, admission, limits, phase, || {
        check_write(original, admission)
    })
    .await?;
    let before = || {
        check_write(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    before()?;
    let changes =
        crate::knowledge_sync_history::KnowledgeSyncChanges::first_batch(&batch, &before)?;
    let mut prepared =
        lance_foundation::LanceModuleStore::prepare_create(&uri, physical_schema(), vec![batch])
            .await?;
    prepared.set_transaction_properties(changes.into_properties()?);
    before()?;
    let (store, _) = prepared.commit().await?;
    let default = KnowledgeGeneration {
        corpus_uri: uri.clone(),
        version: store.snapshot().version(),
        physical_branch: None,
        inherited_target: None,
    };
    phase(FirstPublicationPhase::DefaultConfirmed);
    let result = finish_initial_publication(&uri, default, encoded_branch.as_deref(), before).await;
    let result = result?;
    phase(FirstPublicationPhase::Delivery);
    before().map_err(|cause| KnowledgeIndexError::PartialDefault {
        generation: result.default.clone(),
        cause: Box::new(cause),
    })?;
    drop(writer);
    Ok(result)
}

// This private result owns the actual Core writer/directory; it cannot be restored
// from a locator, serialized, or cloned into another authority domain.
pub(crate) struct NativeFirstPublicationTarget {
    pub(crate) writer: ArtifactWriter,
    pub(crate) guard: CapturedProjectDocumentDirectory,
    pub(crate) uri: String,
    pub(crate) encoded_branch: Option<String>,
}
pub(crate) async fn acquire_native_first_publication_target(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: KnowledgeIndexWriteLimits,
    phase: &(dyn Fn(FirstPublicationPhase) + Sync),
    caller_guard: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<NativeFirstPublicationTarget, KnowledgeIndexError> {
    let project = check_write_project(original, admission)?;
    let scope = project.scope.live;
    let branch = project.version.lance_branch();
    let encoded_branch = (!branch.is_empty()).then_some(branch);
    let corpus = local_corpus(scope, project.version.version(), original, limits.native)?;
    let uri = corpus.uri.clone();
    if !corpus.default_layout {
        let guard = capture_index(scope, &corpus, original, limits.native, &caller_guard)?
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let before = || {
            check_write(original, admission)?;
            caller_guard()?;
            guard.revalidate(original)?;
            Ok(())
        };
        open_local_index(&corpus, encoded_branch.as_deref(), &before)
            .await?
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        return Err(KnowledgeIndexError::InitialTargetChanged);
    }
    let artifacts = ArtifactStore::new(scope.clone(), limits.artifacts);
    phase(FirstPublicationPhase::BeforeWriter);
    check_write(original, admission)?;
    // Waiting for the cooperative native lock must not block the executor
    // which advances the current lock holder's SDK future.
    let request = Arc::clone(original);
    let pending = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request));
    phase(FirstPublicationPhase::WriterWaiting);
    let mut writer = pending
        .await
        .map_err(KnowledgeIndexError::WriterAcquisition)??;
    phase(FirstPublicationPhase::WriterAcquired);
    check_write(original, admission)?;
    caller_guard()?;
    // The lock serializes SDK publication, not the existence of its parent directory.
    // A rejected earlier attempt can leave this authorized directory without a corpus.
    let parent = CapturedProjectDocumentDirectory::capture_runtime(
        scope,
        Path::new(""),
        limits.native,
        original,
    )?;
    parent.revalidate(original)?;
    let present = parent.probe_child_directory(OsStr::new("knowledge"), original)?;
    parent.revalidate(original)?;
    check_write(original, admission)?;
    caller_guard()?;
    let guard = if present == DirectoryPresence::Absent {
        writer.create_record_directory(ProjectRecordDirectory::Knowledge, limits.native)?
    } else {
        let relative = Path::new("knowledge");
        let guard = CapturedProjectDocumentDirectory::capture_runtime(
            scope,
            relative,
            limits.native,
            original,
        )?;
        guard.revalidate(original)?;
        let index_present = guard.probe_child_directory(OsStr::new("index.lance"), original)?;
        guard.revalidate(original)?;
        check_write(original, admission)?;
        caller_guard()?;
        if index_present == DirectoryPresence::PresentDirectory {
            let index_guard = CapturedProjectDocumentDirectory::capture_runtime(
                scope,
                &relative.join("index.lance"),
                limits.native,
                original,
            )?;
            let before = || {
                index_guard.revalidate(original)?;
                check_write(original, admission)?;
                caller_guard()
            };
            if open_native_index(&uri, encoded_branch.as_deref(), &before)
                .await?
                .is_some()
            {
                return Err(KnowledgeIndexError::InitialTargetChanged);
            }
            // No Main is published. Keep old fragments untouched and prepare
            // fresh authorized input under this writer and the SDK Create CAS.
            index_guard
        } else {
            guard
        }
    };
    Ok(NativeFirstPublicationTarget {
        writer,
        guard,
        uri,
        encoded_branch,
    })
}

// The ordinary supplied-image publisher retains its established PartialDefault
// error contract. Its real guards continue to run through shared finalization.
async fn finish_initial_publication(
    uri: &str,
    default: KnowledgeGeneration,
    encoded_branch: Option<&str>,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError> {
    finish_initial_publication_observed(uri, default, encoded_branch, &before, &before)
        .await
        .map_err(|failure| KnowledgeIndexError::PartialDefault {
            generation: failure.publication.default,
            cause: failure.cause,
        })
}

pub(crate) struct InitialPublicationFailure {
    pub(crate) publication: Box<KnowledgeFirstPublication>,
    pub(crate) cause: Box<KnowledgeIndexError>,
}
// A materialized named generation is recorded immediately after SDK success.
// Any later failed guard retains that observation rather than only default.
pub(crate) async fn finish_initial_publication_observed(
    uri: &str,
    default: KnowledgeGeneration,
    encoded_branch: Option<&str>,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
    after_named: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeFirstPublication, InitialPublicationFailure> {
    let mut publication = KnowledgeFirstPublication {
        default,
        named: None,
    };
    let outcome = async {
        before()?;
        if let Some(name) = encoded_branch {
            let branch = lance_branch::LanceBranchStore::open(uri, physical_schema(), None).await?;
            before()?;
            let target = branch.capture_target(Some(name)).await?;
            before()?;
            if target.version() != publication.default.version
                || target.inherited_target() != Some(name)
            {
                return Err(KnowledgeIndexError::InitialTargetChanged);
            }
            let target = target.materialize().await?;
            publication.named = Some(generation(uri, &target));
            after_named()?;
            before()?;
        }
        before()?;
        Ok(())
    }
    .await;
    match outcome {
        Ok(()) => Ok(publication),
        Err(cause) => Err(InitialPublicationFailure {
            publication: Box::new(publication),
            cause: Box::new(cause),
        }),
    }
}
fn recipe_fields() -> Fields {
    vec![
        Field::new("fallback_title", DataType::Utf8, false),
        Field::new("kind_on_absent", DataType::Utf8, false),
        Field::new("max_summary_bytes", DataType::UInt64, false),
    ]
    .into()
}
/// Closed native schema: sixteen document fields plus state and reference projection.
///
/// Pass this schema to [`crate::lance_module_store::LanceModuleStore::open`] to check an
/// existing corpus format without scanning its rows. Schema admission does not
/// validate document contents or grant access; callers retain their own authority
/// and original request checks.
pub fn physical_schema() -> Arc<Schema> {
    let text = |name| Field::new(name, DataType::Utf8, false);
    let timestamp = || DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into()));
    Arc::new(Schema::new(vec![
        text("slug"),
        text("title"),
        text("body"),
        text("kind"),
        text("summary"),
        Field::new("description", DataType::Utf8, true),
        Field::new("summary_truncated", DataType::Boolean, false),
        text("source_path"),
        Field::new("physical_digest", DataType::FixedSizeBinary(32), true),
        Field::new("compilation_digest", DataType::FixedSizeBinary(32), false),
        text("recipe"),
        Field::new(
            "recipe_parameters",
            DataType::Struct(recipe_fields()),
            false,
        ),
        Field::new("updated_at", timestamp(), true),
        Field::new("stale_since", timestamp(), true),
        Field::new(
            "stale_after",
            DataType::Duration(TimeUnit::Nanosecond),
            true,
        ),
        Field::new(
            "resolved_wiki_targets",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, false))),
            false,
        ),
        Field::new("document_state", DataType::UInt8, false),
        Field::new(
            "reference_projection",
            DataType::Struct(refs::projection_fields()),
            true,
        ),
    ]))
}

#[derive(Default)]
pub(crate) struct NamespaceValidation {
    pub(crate) expected_project: Option<ProjectId>,
}
impl NamespaceValidation {
    pub(crate) fn bound(expected_project: Option<ProjectId>) -> Self {
        Self { expected_project }
    }
}
fn identifier(value: &str, limits: KnowledgeIndexContentLimits) -> Result<(), KnowledgeIndexError> {
    if value.len() > limits.max_identifier_bytes.get() {
        return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"));
    }
    Ok(())
}
pub(crate) fn logical_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains([':', '\\'])
        && !value.chars().any(char::is_control)
        && value.split('/').all(|p| !matches!(p, "" | "." | ".."))
}
/// Validate the literal native Knowledge slug before a caller performs I/O.
pub fn validate_slug(slug: &str) -> Result<(), KnowledgeIndexError> {
    if slug.is_empty()
        || slug.contains(['/', '\\'])
        || slug.starts_with("..")
        || slug.chars().any(char::is_control)
    {
        return Err(KnowledgeIndexError::InvalidField("slug"));
    }
    Ok(())
}
fn validate_identity(slug: &str, source: &str, recipe: &str) -> Result<(), KnowledgeIndexError> {
    validate_slug(slug)?;
    if !logical_path(source) {
        return Err(KnowledgeIndexError::InvalidField("source_path"));
    }
    if recipe != AUTHORED_RECIPE_ID {
        return Err(KnowledgeIndexError::InvalidField("recipe"));
    }
    Ok(())
}
fn timestamp(value: SystemTime) -> Result<i64, KnowledgeIndexError> {
    let signed = match value.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::try_from(d.as_nanos()).ok(),
        Err(e) => i128::try_from(e.duration().as_nanos())
            .ok()
            .and_then(i128::checked_neg),
    }
    .ok_or(KnowledgeIndexError::InvalidField("timestamp_range"))?;
    i64::try_from(signed).map_err(|_| KnowledgeIndexError::InvalidField("timestamp_range"))
}
fn duration(value: Duration) -> Result<i64, KnowledgeIndexError> {
    i64::try_from(value.as_nanos())
        .ok()
        .filter(|n| *n > 0)
        .ok_or(KnowledgeIndexError::InvalidField("stale_after"))
}
pub(crate) fn time(value: i64) -> Result<SystemTime, KnowledgeIndexError> {
    let d = Duration::from_nanos(value.unsigned_abs());
    (if value >= 0 {
        UNIX_EPOCH.checked_add(d)
    } else {
        UNIX_EPOCH.checked_sub(d)
    })
    .ok_or(KnowledgeIndexError::InvalidField("timestamp_range"))
}

#[cfg(test)]
fn encode(
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    original: &SnapshotContext,
    limits: KnowledgeIndexContentLimits,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<RecordBatch, KnowledgeIndexError> {
    encode_with_references(rows, None, original, limits, before)
}
pub(crate) fn preflight_document_rows(
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    original: &SnapshotContext,
    limits: KnowledgeIndexContentLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    let rows = rows.rows(original)?;
    let mut slugs = BTreeSet::new();
    for row in rows {
        original.check()?;
        before()?;
        match row {
            MemberRow::CompilationObservation(_) => {
                return Err(KnowledgeIndexError::InvalidField("compilation_observation"));
            }
            MemberRow::WikiTarget { .. } => (),
            MemberRow::DocumentHeader(header) => {
                validate_identity(header.slug, header.source_path, header.recipe.id())?;
                identifier(header.slug, limits)?;
                u64::try_from(header.recipe.parameters().max_summary_bytes.get())
                    .map_err(|_| KnowledgeIndexError::InvalidField("max_summary_bytes"))?;
                for target in header.resolved_wiki_targets.as_slice() {
                    original.check()?;
                    before()?;
                    identifier(target, limits)?;
                }
                header.updated_at.map(timestamp).transpose()?;
                header.stale_since.map(timestamp).transpose()?;
                header.stale_after.map(duration).transpose()?;
                if header.stale_after.is_some() && header.updated_at.is_none() {
                    return Err(KnowledgeIndexError::InvalidField("stale_after"));
                }
                if !slugs.insert(header.slug) {
                    return Err(KnowledgeIndexError::DuplicateSlug);
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn encode_with_references(
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: Option<&PreparedKnowledgeReferences<'_, '_>>,
    original: &SnapshotContext,
    limits: KnowledgeIndexContentLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<RecordBatch, KnowledgeIndexError> {
    preflight_document_rows(rows, original, limits, &mut before)?;
    let rows = rows.rows(original)?;
    if let Some(references) = references {
        references.check(original)?;
        refs::validate_prepared(references, limits, &mut before)?;
    }
    let headers = || {
        rows.iter().filter_map(|r| match r {
            MemberRow::DocumentHeader(h) => Some(h.as_ref()),
            _ => None,
        })
    };
    let strings = |select: for<'d> fn(&DocumentHeader<'d>) -> &'d str| -> ArrayRef {
        Arc::new(StringArray::from_iter_values(headers().map(select)))
    };
    let mut physical = FixedSizeBinaryBuilder::new(32);
    let mut compilation = FixedSizeBinaryBuilder::new(32);
    let mut targets = ListBuilder::new(StringBuilder::new()).with_field(Field::new(
        "item",
        DataType::Utf8,
        false,
    ));
    for h in headers() {
        original.check()?;
        before()?;
        match h.physical_digest {
            Some(d) => physical.append_value(d.as_bytes())?,
            None => physical.append_null(),
        }
        compilation.append_value(h.compilation_digest.as_bytes())?;
        for t in h.resolved_wiki_targets.as_slice() {
            targets.values().append_value(t);
        }
        targets.append(true);
    }
    let recipe: ArrayRef = Arc::new(StructArray::new(
        recipe_fields(),
        vec![
            Arc::new(StringArray::from_iter_values(
                headers().map(|h| h.recipe.parameters().fallback_title),
            )),
            Arc::new(StringArray::from_iter_values(
                headers().map(|h| h.recipe.parameters().kind_on_absent),
            )),
            Arc::new(UInt64Array::from_iter_values(headers().map(|h| {
                u64::try_from(h.recipe.parameters().max_summary_bytes.get()).expect("validated u64")
            }))),
        ],
        None,
    ));
    let stamps = |select: fn(&DocumentHeader<'_>) -> Option<SystemTime>| -> Result<ArrayRef, KnowledgeIndexError> {
        let values: Result<Vec<_>,_> = headers().map(|h| select(h).map(timestamp).transpose()).collect();
        Ok(Arc::new(TimestampNanosecondArray::from(values?).with_timezone("UTC")))
    };
    let ages: Result<Vec<_>, _> = headers()
        .map(|h| h.stale_after.map(duration).transpose())
        .collect();
    let [states, projections] = refs::encode_columns(original, headers(), references, &mut before)?;
    let result = RecordBatch::try_new(
        physical_schema(),
        vec![
            strings(|h| h.slug),
            strings(|h| h.title),
            strings(|h| h.body),
            strings(|h| h.kind),
            strings(|h| h.summary),
            Arc::new(StringArray::from_iter(headers().map(|h| h.description))),
            Arc::new(BooleanArray::from_iter(
                headers().map(|h| Some(h.summary_truncated)),
            )),
            strings(|h| h.source_path),
            Arc::new(physical.finish()),
            Arc::new(compilation.finish()),
            strings(|h| h.recipe.id()),
            recipe,
            stamps(|h| h.updated_at)?,
            stamps(|h| h.stale_since)?,
            Arc::new(DurationNanosecondArray::from(ages?)),
            Arc::new(targets.finish()),
            states,
            projections,
        ],
    )?;
    original.check()?;
    before()?;
    Ok(result)
}

pub(crate) fn array<T: Array + 'static>(
    batch: &RecordBatch,
    index: usize,
) -> Result<&T, KnowledgeIndexError> {
    batch
        .column(index)
        .as_any()
        .downcast_ref()
        .ok_or(KnowledgeIndexError::InvalidField("array_type"))
}
pub(crate) fn head_schema() -> Arc<Schema> {
    let full = physical_schema();
    Arc::new(Schema::new(vec![
        full.field(0).clone(),
        full.field(1).clone(),
        full.field(12).clone(),
        full.field(7).clone(),
        full.field(9).clone(),
        full.field(15).clone(),
        full.field(16).clone(),
        full.field(17).clone(),
    ]))
}
pub(crate) fn decode_head_batch(
    batch: &RecordBatch,
    limits: KnowledgeHeadReadLimits,
    counts: &mut NamespaceValidation,
    output: &mut Vec<KnowledgeHead>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    before()?;
    if batch.schema().as_ref() != head_schema().as_ref() {
        return Err(KnowledgeIndexError::InvalidField("head_schema"));
    }
    for (field, col) in batch.schema().fields().iter().zip(batch.columns()) {
        if !field.is_nullable() && col.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("null_required"));
        }
    }
    let slugs: &StringArray = array(batch, 0)?;
    let titles: &StringArray = array(batch, 1)?;
    let updated: &TimestampNanosecondArray = array(batch, 2)?;
    let source_path: &StringArray = array(batch, 3)?;
    let compilation: &FixedSizeBinaryArray = array(batch, 4)?;
    let targets: &ListArray = array(batch, 5)?;
    let states: &UInt8Array = array(batch, 6)?;
    let projections: &StructArray = array(batch, 7)?;
    for i in 0..batch.num_rows() {
        before()?;
        let slug = slugs.value(i);
        let title = titles.value(i);
        validate_slug(slug)?;
        if slug.len() > limits.max_slug_bytes.get() {
            return Err(KnowledgeIndexError::LimitExceeded("slug_bytes"));
        }
        let updated_at = (!updated.is_null(i))
            .then(|| time(updated.value(i)))
            .transpose()?;
        let raw_targets = targets.value(i);
        let raw_targets = raw_targets
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
        if raw_targets.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("target_null"));
        }
        if !projections.is_null(i) && !logical_path(source_path.value(i)) {
            return Err(KnowledgeIndexError::InvalidField("source_path"));
        }
        let digest: &[u8; 32] = compilation
            .value(i)
            .try_into()
            .map_err(|_| KnowledgeIndexError::InvalidField("compilation_digest"))?;
        let state = refs::visit_row(
            refs::ReferenceRowInput {
                projection: projections,
                state: states.value(i),
                row: i,
                header: refs::WitnessHeader {
                    slug,
                    title,
                    source_path: source_path.value(i),
                    digest,
                },
                wiki: raw_targets,
                expected_project: counts.expected_project,
            },
            None,
            &mut before,
            |_, _| Ok(()),
        )?;
        before()?;
        if state == KnowledgeDocumentState::Removed {
            continue;
        }
        output.push(KnowledgeHead {
            slug: slug.to_owned(),
            title: title.to_owned(),
            updated_at,
        });
    }
    before()?;
    Ok(())
}
pub(crate) fn decode_batch(
    batch: &RecordBatch,
    limits: KnowledgeIndexContentLimits,
    counts: &mut NamespaceValidation,
    output: &mut Vec<KnowledgeDocument>,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    decode_batch_matching(batch, limits, counts, output, None, before)
}

pub(crate) fn decode_batch_matching(
    batch: &RecordBatch,
    limits: KnowledgeIndexContentLimits,
    counts: &mut NamespaceValidation,
    output: &mut Vec<KnowledgeDocument>,
    requested: Option<&str>,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    decode_batch_inner(batch, limits, counts, Some(output), None, requested, before)
}

pub(crate) fn validate_full_batch(
    batch: &RecordBatch,
    limits: KnowledgeIndexContentLimits,
    counts: &mut NamespaceValidation,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    decode_batch_inner(batch, limits, counts, None, None, None, before)
}

fn decode_batch_inner(
    batch: &RecordBatch,
    limits: KnowledgeIndexContentLimits,
    counts: &mut NamespaceValidation,
    mut output: Option<&mut Vec<KnowledgeDocument>>,
    mut dates: Option<&mut Vec<KnowledgeDocumentDates>>,
    requested: Option<&str>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    before()?;
    semantic::dimensions(batch.schema().as_ref())?;
    for (field, col) in batch.schema().fields().iter().zip(batch.columns()) {
        if !field.is_nullable() && col.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("null_required"));
        }
    }
    let strings: Vec<&StringArray> = [0, 1, 2, 3, 4, 5, 7, 10]
        .into_iter()
        .map(|i| array(batch, i))
        .collect::<Result<_, _>>()?;
    let boolean: &BooleanArray = array(batch, 6)?;
    let physical: &FixedSizeBinaryArray = array(batch, 8)?;
    let compilation: &FixedSizeBinaryArray = array(batch, 9)?;
    let recipe: &StructArray = array(batch, 11)?;
    let fallback = recipe
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or(KnowledgeIndexError::InvalidField("recipe_parameters"))?;
    let kind = recipe
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or(KnowledgeIndexError::InvalidField("recipe_parameters"))?;
    let summary = recipe
        .column(2)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .ok_or(KnowledgeIndexError::InvalidField("recipe_parameters"))?;
    if recipe.columns().iter().any(|c| c.null_count() != 0) {
        return Err(KnowledgeIndexError::InvalidField("recipe_parameters_null"));
    }
    let updated: &TimestampNanosecondArray = array(batch, 12)?;
    let stale: &TimestampNanosecondArray = array(batch, 13)?;
    let age: &DurationNanosecondArray = array(batch, 14)?;
    let targets: &ListArray = array(batch, 15)?;
    let states: &UInt8Array = array(batch, 16)?;
    let projections: &StructArray = array(batch, 17)?;
    for i in 0..batch.num_rows() {
        before()?;
        let text = |n: usize| strings[n].value(i);
        validate_identity(text(0), text(6), text(7))?;
        let description = (!strings[5].is_null(i)).then(|| text(5));
        let limit = usize::try_from(summary.value(i))
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or(KnowledgeIndexError::InvalidField("max_summary_bytes"))?;
        let updated_at = (!updated.is_null(i))
            .then(|| time(updated.value(i)))
            .transpose()?;
        let stale_since = (!stale.is_null(i))
            .then(|| time(stale.value(i)))
            .transpose()?;
        let stale_after = if age.is_null(i) {
            None
        } else {
            let value = u64::try_from(age.value(i))
                .ok()
                .filter(|n| *n > 0)
                .ok_or(KnowledgeIndexError::InvalidField("stale_after"))?;
            if updated_at.is_none() {
                return Err(KnowledgeIndexError::InvalidField("stale_after"));
            }
            Some(Duration::from_nanos(value))
        };
        let raw_targets = targets.value(i);
        let raw_targets = raw_targets
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
        if raw_targets.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("target_null"));
        }
        identifier(text(0), limits)?;
        for target in raw_targets.iter().flatten() {
            before()?;
            identifier(target, limits)?;
        }
        let physical_digest = (!physical.is_null(i))
            .then(|| {
                physical
                    .value(i)
                    .try_into()
                    .map(PhysicalSourceDigest32::from_bytes)
                    .map_err(|_| KnowledgeIndexError::InvalidField("physical_digest"))
            })
            .transpose()?;
        let compilation_digest = CompilationDigest32::from_bytes(
            compilation
                .value(i)
                .try_into()
                .map_err(|_| KnowledgeIndexError::InvalidField("compilation_digest"))?,
        );
        let state = refs::visit_row(
            refs::ReferenceRowInput {
                projection: projections,
                state: states.value(i),
                row: i,
                header: refs::WitnessHeader {
                    slug: text(0),
                    title: text(1),
                    source_path: text(6),
                    digest: compilation_digest.as_bytes(),
                },
                wiki: raw_targets,
                expected_project: counts.expected_project,
            },
            Some(limits.max_identifier_bytes),
            &mut before,
            |_, _| Ok(()),
        )?;
        before()?;
        if state == KnowledgeDocumentState::Removed {
            continue;
        }
        if requested.is_some_and(|slug| slug != text(0)) {
            continue;
        }
        if let Some(dates) = dates.as_deref_mut() {
            dates.push(KnowledgeDocumentDates {
                slug: text(0).to_owned(),
                updated_at,
                stale_since,
                stale_after,
            });
        }
        let Some(output) = output.as_deref_mut() else {
            continue;
        };
        if requested.is_some() && !output.is_empty() {
            return Err(KnowledgeIndexError::DuplicateSlug);
        }
        output.push(KnowledgeDocument {
            slug: text(0).to_owned(),
            title: text(1).to_owned(),
            body: text(2).to_owned(),
            kind: text(3).to_owned(),
            summary: text(4).to_owned(),
            description: description.map(str::to_owned),
            summary_truncated: boolean.value(i),
            source_path: text(6).to_owned(),
            physical_digest,
            compilation_digest,
            recipe: text(7).to_owned(),
            recipe_parameters: KnowledgeRecipeParameters {
                fallback_title: fallback.value(i).to_owned(),
                kind_on_absent: kind.value(i).to_owned(),
                max_summary_bytes: limit,
            },
            updated_at,
            stale_since,
            stale_after,
            resolved_wiki_targets: raw_targets
                .iter()
                .map(|s| s.expect("validated target").to_owned())
                .collect(),
        });
    }
    before()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn main_absence_requires_recheck_and_other_sdk_failures_keep_their_cause()
    -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let index = dir.path().join("index.lance");
        std::fs::create_dir(&index)?;
        let uri = url::Url::from_file_path(&index).unwrap().to_string();
        let checks = std::cell::Cell::new(0);
        let before = || {
            checks.set(checks.get() + 1);
            if checks.get() == 2 {
                Err(KnowledgeIndexError::Request(
                    io::ErrorKind::Interrupted.into(),
                ))
            } else {
                Ok(())
            }
        };
        let cause = match open_native_index(&uri, None, &before).await {
            Err(cause) => cause,
            Ok(_) => panic!("absence must not bypass the failed recheck"),
        };
        assert!(
            matches!(cause, KnowledgeIndexError::Request(ref e) if e.kind() == io::ErrorKind::Interrupted)
        );
        assert_eq!(checks.get(), 2);
        // Corrupt a protobuf inside a valid SDK envelope. Upstream does not
        // guarantee checked offsets for arbitrary truncated manifest bytes.
        use object_store::ObjectStoreExt;
        lance_foundation::LanceModuleStore::create(
            &uri,
            physical_schema(),
            vec![RecordBatch::new_empty(physical_schema())],
        )
        .await?;
        let dataset = lance::Dataset::open(&uri).await?;
        let store = dataset.object_store(None).await?;
        let path = &dataset.manifest_location().path;
        let mut bytes = store.inner.get(path).await?.bytes().await?.to_vec();
        let offset =
            u64::from_le_bytes(bytes[bytes.len() - 16..bytes.len() - 8].try_into()?) as usize;
        let len = u32::from_le_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        bytes[offset + 4..offset + 4 + len].fill(0xff);
        store.inner.put(path, bytes.into()).await?;
        checks.set(0);
        let cause = match open_native_index(&uri, None, &before).await {
            Err(cause) => cause,
            Ok(_) => panic!("corruption is never absence"),
        };
        assert!(matches!(cause, KnowledgeIndexError::Branch(_)));
        assert!(cause.source().is_some());
        assert_eq!(checks.get(), 2);
        Ok(())
    }
    use crate::knowledge_document_projection::{
        self as h, AuthoredPageFields, AuthoredParameters, KnowledgeDocumentProjectionLimits,
        RecipeFields,
    };
    use futures::TryStreamExt;
    use graphit_core::file_lock::Cancellation;
    use std::time::Instant;

    #[tokio::test]
    async fn named_sdk_failure_retains_real_confirmed_default_and_cause()
    -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let uri = dir.path().join("index.lance").to_str().unwrap().to_owned();
        let batch = RecordBatch::new_empty(physical_schema());
        let store =
            lance_foundation::LanceModuleStore::create(&uri, physical_schema(), vec![batch])
                .await?;
        let default = KnowledgeGeneration {
            corpus_uri: uri.clone(),
            version: store.snapshot().version(),
            physical_branch: None,
            inherited_target: None,
        };
        let error =
            finish_initial_publication(&uri, default.clone(), Some("/invalid-branch"), || Ok(()))
                .await
                .unwrap_err();
        let KnowledgeIndexError::PartialDefault { generation, cause } = error else {
            panic!("confirmed default must remain observable");
        };
        assert_eq!(generation, default);
        assert!(matches!(*cause, KnowledgeIndexError::Branch(_)));
        assert!(cause.source().is_some());
        let reopened = lance_foundation::LanceModuleStore::open(&uri, physical_schema()).await?;
        assert_eq!(reopened.snapshot().version(), generation.version);
        let pin = reopened.snapshot();
        let scan = pin.scan();
        let mut stream = scan.try_into_stream().await?;
        assert!(stream.try_next().await?.is_none());
        Ok(())
    }
    fn n(v: usize) -> NonZeroUsize {
        NonZeroUsize::new(v).unwrap()
    }
    fn limits() -> KnowledgeIndexContentLimits {
        KnowledgeIndexContentLimits {
            max_identifier_bytes: n(256),
        }
    }
    fn hl() -> KnowledgeDocumentProjectionLimits {
        KnowledgeDocumentProjectionLimits {
            max_documents: n(8),
            max_observations: n(8),
            max_rows: n(64),
            max_field_bytes: n(65536),
            max_text_bytes: n(131072),
        }
    }
    // SystemTime uses 100 ns FILETIME intervals on Windows and ns on Unix.
    const NATIVE_TIMESTAMP_TICK_NS: i64 = if cfg!(windows) { 100 } else { 1 };
    const MIN_NATIVE_TIMESTAMP_NS: i64 =
        i64::MIN / NATIVE_TIMESTAMP_TICK_NS * NATIVE_TIMESTAMP_TICK_NS;
    const MAX_NATIVE_TIMESTAMP_NS: i64 =
        i64::MAX / NATIVE_TIMESTAMP_TICK_NS * NATIVE_TIMESTAMP_TICK_NS;

    fn page<'a>(slug: &'a str, targets: &'a [&'a str]) -> AuthoredPageFields<'a> {
        AuthoredPageFields {
            slug: Some(slug),
            title: Some(""),
            body: Some("---\ntitle: literal\n---\nBody [link](ghost)\n"),
            kind: Some("page"),
            summary: Some("síntese"),
            description: Some(""),
            summary_truncated: true,
            source_path: "wiki/document.md",
            physical_digest: Some(PhysicalSourceDigest32::from_bytes([0x80; 32])),
            compilation_digest: CompilationDigest32::from_bytes([0xff; 32]),
            recipe: RecipeFields {
                id: AUTHORED_RECIPE_ID,
                parameters: Some(AuthoredParameters {
                    fallback_title: "fallback",
                    kind_on_absent: "page",
                    max_summary_bytes: n(13),
                }),
            },
            updated_at: Some(
                UNIX_EPOCH - Duration::from_nanos(7 * NATIVE_TIMESTAMP_TICK_NS as u64),
            ),
            stale_since: Some(
                UNIX_EPOCH + Duration::from_nanos(11 * NATIVE_TIMESTAMP_TICK_NS as u64),
            ),
            stale_after: Some(Duration::from_nanos(123)),
            resolved_wiki_targets: targets,
        }
    }

    fn head_limits() -> KnowledgeHeadReadLimits {
        KnowledgeHeadReadLimits {
            max_slug_bytes: n(256),
            native: NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        }
    }
    #[tokio::test]
    async fn heads_project_metadata_without_body_and_keep_one_pin_after_replace()
    -> Result<(), Box<dyn Error>> {
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(120),
            Cancellation::default(),
        );
        let mut a = page("a", &[]);
        a.updated_at = Some(time(i64::MIN)?);
        let mut b = page("b", &[]);
        b.title = Some("literal λ");
        b.updated_at = Some(time(i64::MAX)?);
        let inputs = [a, b];
        let prepared = h::prepare(&ctx, &inputs, &[], hl())?;
        let batch = encode(&prepared, &ctx, limits(), || Ok(()))?;
        let dir = tempfile::tempdir()?;
        let uri = dir.path().join("index.lance").to_str().unwrap().to_owned();
        let store =
            lance_foundation::LanceModuleStore::create(&uri, physical_schema(), vec![batch])
                .await?;
        let branches = lance_branch::LanceBranchStore::open(&uri, physical_schema(), None).await?;
        let old = branches.capture_target(None).await?;
        let mut scanner = old.scan();
        scanner
            .project(&[
                "slug",
                "title",
                "updated_at",
                "source_path",
                "compilation_digest",
                "resolved_wiki_targets",
                "document_state",
                "reference_projection",
            ])?
            .batch_size(1);
        let mut stream = scanner.try_into_stream().await?;
        let first = stream.try_next().await?.unwrap();
        assert_eq!(first.schema(), head_schema());
        assert_eq!(first.num_columns(), 8);
        let later_inputs = [page("later", &[])];
        let later = h::prepare(&ctx, &later_inputs, &[], hl())?;
        let new_batch = encode(&later, &ctx, limits(), || Ok(()))?;
        let new = store
            .snapshot()
            .prepare_replace(vec![new_batch])
            .await?
            .commit()
            .await?;
        assert!(new.snapshot().version() > old.version());
        let mut counts = NamespaceValidation::default();
        let mut output = Vec::new();
        decode_head_batch(&first, head_limits(), &mut counts, &mut output, || Ok(()))?;
        while let Some(batch) = stream.try_next().await? {
            assert_eq!(batch.schema(), head_schema());
            decode_head_batch(&batch, head_limits(), &mut counts, &mut output, || Ok(()))?;
        }
        assert_eq!(
            output,
            vec![
                KnowledgeHead {
                    slug: "a".into(),
                    title: "".into(),
                    updated_at: Some(time(i64::MIN)?)
                },
                KnowledgeHead {
                    slug: "b".into(),
                    title: "literal λ".into(),
                    updated_at: Some(time(i64::MAX)?)
                },
            ]
        );
        assert_eq!(output.len(), 2);
        Ok(())
    }
    #[test]
    fn head_slug_limit_and_schema_fail_before_copying_the_rejected_row()
    -> Result<(), Box<dyn Error>> {
        let schema = head_schema();
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["á"])),
                Arc::new(StringArray::from(vec!["x"])),
                Arc::new(TimestampNanosecondArray::from(vec![None]).with_timezone("UTC")),
                Arc::new(StringArray::from(vec!["docs/a.md"])),
                Arc::new(
                    FixedSizeBinaryArray::try_from_iter([&[0u8; 32][..]].into_iter()).unwrap(),
                ),
                {
                    let mut targets = ListBuilder::new(StringBuilder::new())
                        .with_field(Field::new("item", DataType::Utf8, false));
                    targets.append(true);
                    Arc::new(targets.finish())
                },
                Arc::new(UInt8Array::from(vec![1])),
                arrow_array::new_null_array(physical_schema().field(17).data_type(), 1),
            ],
        )?;
        let exact = KnowledgeHeadReadLimits {
            max_slug_bytes: n(2),
            ..head_limits()
        };
        let mut counts = NamespaceValidation::default();
        let mut owned = Vec::new();
        decode_head_batch(&batch, exact, &mut counts, &mut owned, || Ok(()))?;
        decode_head_batch(&batch, exact, &mut counts, &mut owned, || Ok(()))?;
        assert_eq!(owned.len(), 2);
        assert_eq!(owned[0].updated_at, None);
        let mut out = Vec::new();
        assert!(matches!(
            decode_head_batch(
                &batch,
                KnowledgeHeadReadLimits {
                    max_slug_bytes: n(1),
                    ..exact
                },
                &mut NamespaceValidation::default(),
                &mut out,
                || Ok(())
            ),
            Err(KnowledgeIndexError::LimitExceeded("slug_bytes"))
        ));
        assert!(out.is_empty());
        assert!(matches!(
            decode_head_batch(
                &RecordBatch::new_empty(physical_schema()),
                exact,
                &mut NamespaceValidation::default(),
                &mut out,
                || Ok(())
            ),
            Err(KnowledgeIndexError::InvalidField("head_schema"))
        ));
        Ok(())
    }

    #[test]
    fn repeated_batches_preserve_documents_and_individual_identifier_limits()
    -> Result<(), Box<dyn Error>> {
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let inputs = [page("á", &["á", "ghost", "ghost", ""])];
        let prepared = h::prepare(&ctx, &inputs, &[], hl())?;
        let batch = encode(&prepared, &ctx, limits(), || Ok(()))?;
        let mut counts = NamespaceValidation::default();
        let mut output = vec![];
        decode_batch(&batch, limits(), &mut counts, &mut output, || Ok(()))?;
        assert_eq!(output[0].resolved_wiki_targets, ["á", "ghost", "ghost", ""]);
        let exact = KnowledgeIndexContentLimits {
            max_identifier_bytes: n(5),
        };
        let mut cumulative = NamespaceValidation::default();
        let mut complete = vec![];
        decode_batch(&batch, exact, &mut cumulative, &mut complete, || Ok(()))?;
        decode_batch(&batch, exact, &mut cumulative, &mut complete, || Ok(()))?;
        assert_eq!(complete.len(), 2);
        assert_eq!(complete[0].body, complete[1].body);
        let mut owner = vec![];
        assert!(matches!(
            decode_batch(
                &batch,
                KnowledgeIndexContentLimits {
                    max_identifier_bytes: n(4),
                },
                &mut NamespaceValidation::default(),
                &mut owner,
                || Ok(())
            ),
            Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"))
        ));
        assert!(owner.is_empty());
        Ok(())
    }

    #[test]
    fn representable_timestamps_roundtrip_and_invalid_payloads_fail_without_sdk()
    -> Result<(), Box<dyn Error>> {
        for value in [
            MIN_NATIVE_TIMESTAMP_NS,
            -NATIVE_TIMESTAMP_TICK_NS,
            0,
            NATIVE_TIMESTAMP_TICK_NS,
            MAX_NATIVE_TIMESTAMP_NS,
        ] {
            assert_eq!(timestamp(time(value)?)?, value);
        }
        assert!(matches!(
            timestamp(
                time(MAX_NATIVE_TIMESTAMP_NS)?
                    + Duration::from_nanos(NATIVE_TIMESTAMP_TICK_NS as u64)
            ),
            Err(KnowledgeIndexError::InvalidField("timestamp_range"))
        ));
        assert!(matches!(
            timestamp(
                time(MIN_NATIVE_TIMESTAMP_NS)?
                    - Duration::from_nanos(NATIVE_TIMESTAMP_TICK_NS as u64)
            ),
            Err(KnowledgeIndexError::InvalidField("timestamp_range"))
        ));
        assert!(duration(Duration::ZERO).is_err());
        assert!(duration(Duration::from_nanos(i64::MAX as u64 + 1)).is_err());
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let observation = [h::CompilationObservationFields {
            path: "wiki/page.md",
            state: h::CompilationObservationState::Removed,
        }];
        let prepared = h::prepare(&ctx, &[], &observation, hl())?;
        assert!(matches!(
            encode(&prepared, &ctx, limits(), || Ok(())),
            Err(KnowledgeIndexError::InvalidField("compilation_observation"))
        ));
        let inputs = [page("same", &[]), page("same", &[])];
        let prepared = h::prepare(&ctx, &inputs, &[], hl())?;
        assert!(matches!(
            encode(&prepared, &ctx, limits(), || Ok(())),
            Err(KnowledgeIndexError::DuplicateSlug)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn empty_full16_is_published_with_version_and_real_eof() -> Result<(), Box<dyn Error>> {
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(120),
            Cancellation::default(),
        );
        let prepared = h::prepare(&ctx, &[], &[], hl())?;
        let batch = encode(&prepared, &ctx, limits(), || Ok(()))?;
        let dir = tempfile::tempdir()?;
        let uri = dir.path().join("index.lance").to_str().unwrap().to_owned();
        let created =
            lance_foundation::LanceModuleStore::create(&uri, physical_schema(), vec![batch])
                .await?;
        let version = created.snapshot().version();
        drop(created);
        let opened = lance_foundation::LanceModuleStore::open(&uri, physical_schema()).await?;
        let pin = opened.snapshot();
        assert_eq!(pin.version(), version);
        assert_eq!(pin.schema(), physical_schema().as_ref());
        let mut stream = pin.scan().try_into_stream().await?;
        let mut counts = NamespaceValidation::default();
        let mut documents = vec![];
        while let Some(b) = stream.try_next().await? {
            decode_batch(&b, limits(), &mut counts, &mut documents, || Ok(()))?;
        }
        assert!(documents.is_empty());
        assert!(documents.is_empty());
        assert!(version > 0);
        Ok(())
    }

    #[tokio::test]
    async fn one_pin_survives_replace_and_named_initialization_refuses_adoption()
    -> Result<(), Box<dyn Error>> {
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(120),
            Cancellation::default(),
        );
        let first = [page("first", &[])];
        let prepared = h::prepare(&ctx, &first, &[], hl())?;
        let batch = encode(&prepared, &ctx, limits(), || Ok(()))?;
        let dir = tempfile::tempdir()?;
        let uri = dir.path().join("index.lance").to_str().unwrap().to_owned();
        let created =
            lance_foundation::LanceModuleStore::create(&uri, physical_schema(), vec![batch])
                .await?;
        let branches = lance_branch::LanceBranchStore::open(&uri, physical_schema(), None).await?;
        let old = branches.capture_target(None).await?;
        let second = [page("second", &[])];
        let prepared = h::prepare(&ctx, &second, &[], hl())?;
        let batch = encode(&prepared, &ctx, limits(), || Ok(()))?;
        let newer = created
            .snapshot()
            .prepare_replace(vec![batch])
            .await?
            .commit()
            .await?;
        assert!(newer.snapshot().version() > old.version());
        let mut stream = old.scan().try_into_stream().await?;
        let mut documents = vec![];
        let mut counts = NamespaceValidation::default();
        while let Some(b) = stream.try_next().await? {
            decode_batch(&b, limits(), &mut counts, &mut documents, || Ok(()))?;
        }
        assert_eq!(documents[0].slug, "first");
        let inherited = branches.capture_target(Some("fresh-named")).await?;
        assert_eq!(inherited.version(), newer.snapshot().version());
        assert_eq!(inherited.inherited_target(), Some("fresh-named"));
        let named = inherited.materialize().await?;
        assert_eq!(named.version(), newer.snapshot().version());
        let existing = branches.capture_target(Some("fresh-named")).await?;
        assert_eq!(existing.inherited_target(), None);
        assert!(
            existing.inherited_target() != Some("fresh-named"),
            "initial publisher must refuse adoption"
        );
        Ok(())
    }
    #[tokio::test]
    async fn nonempty_full16_create_reopen_scan_roundtrip() -> Result<(), Box<dyn Error>> {
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(120),
            Cancellation::default(),
        );
        let mut second = page("á", &[]);
        second.description = None;
        second.physical_digest = None;
        second.updated_at = Some(time(MIN_NATIVE_TIMESTAMP_NS)?);
        second.stale_since = Some(time(MAX_NATIVE_TIMESTAMP_NS)?);
        second.stale_after = Some(Duration::from_nanos(i64::MAX as u64));
        let inputs = [page("z", &["ghost", "z", "ghost", ""]), second];
        let prepared = h::prepare(&ctx, &inputs, &[], hl())?;
        let batch = encode(&prepared, &ctx, limits(), || {
            ctx.check()?;
            Ok(())
        })?;
        assert_eq!(batch.num_columns(), 18);
        assert_eq!(
            array::<TimestampNanosecondArray>(&batch, 12)?.value(1),
            MIN_NATIVE_TIMESTAMP_NS
        );
        assert_eq!(
            array::<TimestampNanosecondArray>(&batch, 13)?.value(1),
            MAX_NATIVE_TIMESTAMP_NS
        );
        let dir = tempfile::tempdir()?;
        let uri = dir.path().join("index.lance").to_str().unwrap().to_owned();
        let created = lance_foundation::LanceModuleStore::create(
            &uri,
            physical_schema(),
            vec![batch.clone()],
        )
        .await?;
        let version = created.snapshot().version();
        drop(created);
        let opened = lance_foundation::LanceModuleStore::open(&uri, physical_schema()).await?;
        let pin = opened.snapshot();
        assert_eq!(pin.version(), version);
        assert_eq!(pin.schema(), physical_schema().as_ref());
        let mut scanner = pin.scan();
        scanner.batch_size(1);
        let mut stream = scanner.try_into_stream().await?;
        let mut output = vec![];
        let mut counts = NamespaceValidation::default();
        let mut batches = 0;
        while let Some(b) = stream.try_next().await? {
            decode_batch(&b, limits(), &mut counts, &mut output, || {
                ctx.check()?;
                Ok(())
            })?;
            batches += 1;
        }
        assert_eq!(batches, 2);
        assert_eq!(output.len(), 2);
        // Independent H expectations prevent a symmetric encoder/decoder error
        // from satisfying the SDK roundtrip by accident.
        let expected: Vec<_> = inputs
            .iter()
            .map(|h| {
                let p = h.recipe.parameters.unwrap();
                KnowledgeDocument {
                    slug: h.slug.unwrap().to_owned(),
                    title: h.title.unwrap().to_owned(),
                    body: h.body.unwrap().to_owned(),
                    kind: h.kind.unwrap().to_owned(),
                    summary: h.summary.unwrap().to_owned(),
                    description: h.description.map(str::to_owned),
                    summary_truncated: h.summary_truncated,
                    source_path: h.source_path.to_owned(),
                    physical_digest: h.physical_digest,
                    compilation_digest: h.compilation_digest,
                    recipe: h.recipe.id.to_owned(),
                    recipe_parameters: KnowledgeRecipeParameters {
                        fallback_title: p.fallback_title.to_owned(),
                        kind_on_absent: p.kind_on_absent.to_owned(),
                        max_summary_bytes: p.max_summary_bytes,
                    },
                    updated_at: h.updated_at,
                    stale_since: h.stale_since,
                    stale_after: h.stale_after,
                    resolved_wiki_targets: h
                        .resolved_wiki_targets
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect(),
                }
            })
            .collect();
        assert_eq!(output, expected);
        assert_eq!(output[0].title, "");
        assert_eq!(output[0].body, inputs[0].body.unwrap());
        assert_eq!(output[0].resolved_wiki_targets, ["ghost", "z", "ghost", ""]);
        assert!(output[1].resolved_wiki_targets.is_empty());
        assert_eq!(
            timestamp(output[1].updated_at.unwrap())?,
            MIN_NATIVE_TIMESTAMP_NS
        );
        assert_eq!(
            timestamp(output[1].stale_since.unwrap())?,
            MAX_NATIVE_TIMESTAMP_NS
        );
        assert_eq!(output[1].physical_digest, None);
        assert_eq!(output[1].description, None);
        assert!(pin.schema().metadata().is_empty());
        for f in pin.schema().fields() {
            assert!(f.metadata().is_empty());
            match f.data_type() {
                DataType::Struct(fields) => {
                    for child in fields {
                        assert!(child.metadata().is_empty());
                        assert_eq!(
                            child.is_nullable(),
                            f.name() == "reference_projection" && child.name() == "source_context"
                        );
                    }
                }
                DataType::List(child) => {
                    assert!(child.metadata().is_empty());
                    assert_eq!(child.name(), "item");
                    assert!(!child.is_nullable());
                }
                _ => (),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod reference_storage_tests {
    use super::*;
    use crate::knowledge_document_projection::{
        self as h, AuthoredPageFields, AuthoredParameters, KnowledgeDocumentProjectionLimits,
        RecipeFields,
    };
    use crate::knowledge_references::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::Instant;
    fn n(v: usize) -> NonZeroUsize {
        NonZeroUsize::new(v).unwrap()
    }
    fn original() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            Cancellation::default(),
        )
    }
    fn limits() -> KnowledgeIndexContentLimits {
        KnowledgeIndexContentLimits {
            max_identifier_bytes: n(256),
        }
    }
    fn hl() -> KnowledgeDocumentProjectionLimits {
        KnowledgeDocumentProjectionLimits {
            max_documents: n(8),
            max_observations: n(8),
            max_rows: n(64),
            max_field_bytes: n(65536),
            max_text_bytes: n(131072),
        }
    }
    fn project() -> ProjectId {
        "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()
    }
    fn ns() -> KnowledgeReferenceNamespace<'static> {
        KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project()),
            context: None,
        }
    }
    fn page<'a>(slug: &'a str, targets: &'a [&'a str]) -> AuthoredPageFields<'a> {
        AuthoredPageFields {
            slug: Some(slug),
            title: Some("title"),
            body: Some("# source\n"),
            kind: Some("page"),
            summary: Some("source"),
            description: None,
            summary_truncated: false,
            source_path: "docs/page.md",
            physical_digest: None,
            compilation_digest: CompilationDigest32::from_bytes([3; 32]),
            recipe: RecipeFields {
                id: AUTHORED_RECIPE_ID,
                parameters: Some(AuthoredParameters {
                    fallback_title: "title",
                    kind_on_absent: "page",
                    max_summary_bytes: n(16),
                }),
            },
            updated_at: None,
            stale_since: None,
            stale_after: None,
            resolved_wiki_targets: targets,
        }
    }
    fn authored(scope: KnowledgeReferenceScope<'_>) -> KnowledgeAuthoredReference<'_> {
        KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Knowledge,
            scope,
            id: "é",
            context: None,
            relation: " supports ",
            field: Some(""),
        }
    }
    fn fields<'a>(
        slug: &'a str,
        state: KnowledgeDocumentState,
        authored: &'a [KnowledgeAuthoredReference<'a>],
    ) -> KnowledgeReferenceFields<'a> {
        KnowledgeReferenceFields {
            slug,
            state,
            namespace: ns(),
            authored,
        }
    }
    fn replace(batch: &RecordBatch, column: usize, array: ArrayRef) -> RecordBatch {
        let mut columns = batch.columns().to_vec();
        columns[column] = array;
        RecordBatch::try_new(batch.schema(), columns).unwrap()
    }
    fn change_projection(batch: &RecordBatch, child: usize, array: ArrayRef) -> RecordBatch {
        let old = batch
            .column(17)
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        let mut columns = old.columns().to_vec();
        columns[child] = array;
        replace(
            batch,
            17,
            Arc::new(StructArray::new(
                old.fields().clone(),
                columns,
                old.nulls().cloned(),
            )),
        )
    }
    fn decode(batch: &RecordBatch) -> Result<Vec<KnowledgeDocument>, KnowledgeIndexError> {
        let mut docs = vec![];
        decode_batch(
            batch,
            limits(),
            &mut NamespaceValidation::bound(Some(project())),
            &mut docs,
            || Ok(()),
        )?;
        Ok(docs)
    }
    #[test]
    fn witness_distinguishes_target_scope_kind_with_identical_scope_id() {
        let ctx = original();
        let pages = [page("page", &[])];
        let h = h::prepare(&ctx, &pages, &[], hl()).unwrap();
        let text = "01KSH1CRFFG8Z74B5ZS78WW808";
        let user = [authored(KnowledgeReferenceScope::User(text))];
        let project_refs = [authored(KnowledgeReferenceScope::Project(project()))];
        let user_fields = [fields("page", KnowledgeDocumentState::Active, &user)];
        let project_fields = [fields(
            "page",
            KnowledgeDocumentState::Active,
            &project_refs,
        )];
        let u = prepare_knowledge_references(&ctx, &h, &user_fields, limits()).unwrap();
        let p = prepare_knowledge_references(&ctx, &h, &project_fields, limits()).unwrap();
        assert_ne!(u.rows[0].witness, p.rows[0].witness);
        let batch = encode_with_references(&h, Some(&p), &ctx, limits(), || Ok(())).unwrap();
        let projection = batch
            .column(17)
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        let list = projection
            .column(3)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        let value = list.value(0);
        let value = value.as_any().downcast_ref::<StructArray>().unwrap();
        let mut columns = value.columns().to_vec();
        columns[1] = Arc::new(UInt8Array::from(vec![2]));
        let value = StructArray::new(value.fields().clone(), columns, None);
        let DataType::List(item) = list.data_type() else {
            unreachable!()
        };
        let list = ListArray::new(
            Arc::clone(item),
            list.offsets().clone(),
            Arc::new(value),
            list.nulls().cloned(),
        );
        let tampered = change_projection(&batch, 3, Arc::new(list));
        assert!(matches!(
            decode(&tampered),
            Err(KnowledgeIndexError::InvalidField("reference_witness"))
        ));
    }
    #[test]
    fn projection_none_is_incomplete_and_removed_is_not_returned() {
        let ctx = original();
        let pages = [page("active", &["ghost"]), page("removed", &[])];
        let h = h::prepare(&ctx, &pages, &[], hl()).unwrap();
        let ordinary = encode(&h, &ctx, limits(), || Ok(())).unwrap();
        let proj = ordinary
            .column(17)
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        assert_eq!(proj.null_count(), 2);
        let refs = [authored(KnowledgeReferenceScope::User(" target "))];
        let fs = [
            fields("active", KnowledgeDocumentState::Active, &refs),
            fields("removed", KnowledgeDocumentState::Removed, &[]),
        ];
        let prepared = prepare_knowledge_references(&ctx, &h, &fs, limits()).unwrap();
        let batch = encode_with_references(&h, Some(&prepared), &ctx, limits(), || Ok(())).unwrap();
        let docs = decode(&batch).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].slug, "active");
        assert_eq!(docs[0].body, "# source\n");
        let state_only = replace(&ordinary, 16, Arc::new(UInt8Array::from(vec![1, 2])));
        assert!(matches!(
            decode(&state_only),
            Err(KnowledgeIndexError::InvalidField("removed_projection"))
        ));
    }
    #[test]
    fn witness_rejects_header_state_namespace_and_digest_tampering() {
        let ctx = original();
        let pages = [page("page", &[])];
        let h = h::prepare(&ctx, &pages, &[], hl()).unwrap();
        let refs = [authored(KnowledgeReferenceScope::User("subject"))];
        let fs = [fields("page", KnowledgeDocumentState::Active, &refs)];
        let prepared = prepare_knowledge_references(&ctx, &h, &fs, limits()).unwrap();
        let batch = encode_with_references(&h, Some(&prepared), &ctx, limits(), || Ok(())).unwrap();
        for (column, value) in [(0, "other"), (1, "changed"), (7, "docs/other.md")] {
            let bad = replace(&batch, column, Arc::new(StringArray::from(vec![value])));
            assert!(matches!(
                decode(&bad),
                Err(KnowledgeIndexError::InvalidField("reference_witness"))
            ));
        }
        let mut digest = FixedSizeBinaryBuilder::new(32);
        digest.append_value([7; 32]).unwrap();
        assert!(matches!(
            decode(&replace(&batch, 9, Arc::new(digest.finish()))),
            Err(KnowledgeIndexError::InvalidField("reference_witness"))
        ));
        let mut witness = FixedSizeBinaryBuilder::new(32);
        witness.append_value([0; 32]).unwrap();
        assert!(matches!(
            decode(&change_projection(&batch, 4, Arc::new(witness.finish()))),
            Err(KnowledgeIndexError::InvalidField("reference_witness"))
        ));
        assert!(matches!(
            decode(&replace(&batch, 16, Arc::new(UInt8Array::from(vec![9])))),
            Err(KnowledgeIndexError::InvalidField("document_state"))
        ));
        assert!(matches!(
            decode(&change_projection(
                &batch,
                1,
                Arc::new(StringArray::from(vec!["01ARZ3NDEKTSV4RRFFQ69G5FAV"]))
            )),
            Err(KnowledgeIndexError::InvalidField("source_namespace"))
        ));
    }
    #[test]
    fn complete_empty_is_present_and_authored_order_is_integrity_input() {
        let ctx = original();
        let pages = [page("page", &[])];
        let h = h::prepare(&ctx, &pages, &[], hl()).unwrap();
        let empty = [fields("page", KnowledgeDocumentState::Active, &[])];
        let p = prepare_knowledge_references(&ctx, &h, &empty, limits()).unwrap();
        let batch = encode_with_references(&h, Some(&p), &ctx, limits(), || Ok(())).unwrap();
        let projection = batch
            .column(17)
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        assert_eq!(projection.null_count(), 0);
        assert_eq!(decode(&batch).unwrap().len(), 1);
        let refs = [
            authored(KnowledgeReferenceScope::User("first")),
            authored(KnowledgeReferenceScope::User("second")),
        ];
        let reversed = [refs[1], refs[0]];
        let original_fields = [fields("page", KnowledgeDocumentState::Active, &refs)];
        let reverse_fields = [fields("page", KnowledgeDocumentState::Active, &reversed)];
        let a = prepare_knowledge_references(&ctx, &h, &original_fields, limits()).unwrap();
        let b = prepare_knowledge_references(&ctx, &h, &reverse_fields, limits()).unwrap();
        let first = encode_with_references(&h, Some(&a), &ctx, limits(), || Ok(())).unwrap();
        let reversed = encode_with_references(&h, Some(&b), &ctx, limits(), || Ok(())).unwrap();
        let reversed = reversed
            .column(17)
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        assert!(matches!(
            decode(&change_projection(
                &first,
                3,
                Arc::clone(reversed.column(3))
            )),
            Err(KnowledgeIndexError::InvalidField("reference_witness"))
        ));
    }
    #[test]
    fn null_witness_is_rejected_by_arrow_nonnullable_child() {
        let ctx = original();
        let pages = [page("page", &[])];
        let h = h::prepare(&ctx, &pages, &[], hl()).unwrap();
        let fs = [fields("page", KnowledgeDocumentState::Active, &[])];
        let p = prepare_knowledge_references(&ctx, &h, &fs, limits()).unwrap();
        let batch = encode_with_references(&h, Some(&p), &ctx, limits(), || Ok(())).unwrap();
        let projection = batch
            .column(17)
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        let mut columns = projection.columns().to_vec();
        let mut witness = FixedSizeBinaryBuilder::new(32);
        witness.append_null();
        columns[4] = Arc::new(witness.finish());
        let error = StructArray::try_new(projection.fields().clone(), columns, None).unwrap_err();
        assert!(matches!(
            error,
            arrow_schema::ArrowError::InvalidArgumentError(_)
        ));
    }
    #[test]
    fn prepared_plan_cannot_be_paired_with_another_header_body() {
        let ctx = original();
        let first = [page("page", &[])];
        let mut other = page("page", &[]);
        other.body = Some("different");
        let other = [other];
        let h = h::prepare(&ctx, &first, &[], hl()).unwrap();
        let changed = h::prepare(&ctx, &other, &[], hl()).unwrap();
        let fs = [fields("page", KnowledgeDocumentState::Active, &[])];
        let prepared = prepare_knowledge_references(&ctx, &h, &fs, limits()).unwrap();
        assert!(matches!(
            encode_with_references(&changed, Some(&prepared), &ctx, limits(), || Ok(())),
            Err(KnowledgeIndexError::InvalidField("reference_document_map"))
        ));
    }
    #[test]
    fn repeated_long_reference_text_preserves_integrity_and_causal_failure() {
        let ctx = original();
        let pages = [page("page", &["wiki"])];
        let h = h::prepare(&ctx, &pages, &[], hl()).unwrap();
        let relation = "supports ".repeat(512);
        let refs = [KnowledgeAuthoredReference {
            relation: &relation,
            ..authored(KnowledgeReferenceScope::User("user"))
        }; 2];
        let fs = [fields("page", KnowledgeDocumentState::Active, &refs)];
        let prepared = prepare_knowledge_references(&ctx, &h, &fs, limits()).unwrap();
        let cap = KnowledgeIndexContentLimits {
            max_identifier_bytes: n(26),
        };
        assert!(prepare_knowledge_references(&ctx, &h, &fs, cap).is_ok());
        let smaller = KnowledgeIndexContentLimits {
            max_identifier_bytes: n(25),
        };
        assert!(matches!(
            prepare_knowledge_references(&ctx, &h, &fs, smaller),
            Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"))
        ));
        let batch = encode_with_references(&h, Some(&prepared), &ctx, limits(), || Ok(())).unwrap();
        let mut docs = vec![];
        decode_batch(
            &batch,
            limits(),
            &mut NamespaceValidation::default(),
            &mut docs,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(docs.len(), 1);
        docs.clear();
        let e = decode_batch(
            &batch,
            limits(),
            &mut NamespaceValidation::default(),
            &mut docs,
            || Err(KnowledgeIndexError::Auth(AuthError::Denied)),
        )
        .unwrap_err();
        assert!(matches!(e, KnowledgeIndexError::Auth(AuthError::Denied)));
        assert!(docs.is_empty());
    }
}
