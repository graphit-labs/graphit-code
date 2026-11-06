//! Knowledge semantic storage on captured FULL18 and Lance pins.
//! Authorizations remain independent of observations and recovery keys.
use crate::{
    knowledge_index::{
        self as index, KnowledgeDocument, KnowledgeGeneration, KnowledgeIndexReadLimits,
        KnowledgeIndexWriteLimits,
    },
    lance_branch_store::{
        BranchError, BranchSnapshot, DeltaCommitFailure, DeltaRecoveryKey, DeltaResolution,
    },
    lance_module_store::StoreError,
};
use arrow_array::{
    Array, ArrayRef, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, RecordBatch,
    StringArray, UInt8Array, builder::FixedSizeBinaryBuilder,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
mod context;
pub use context::{KnowledgeContextCapture, capture_native_knowledge_context};
mod metadata;
mod readonly;
mod reusable;
use metadata::decode as decode_semantic_metadata;
pub(crate) mod search;
mod stream;
use graphit_auth::{ProjectReadAdmission, ProjectWriteAdmission};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedPublicationScope, ScopeBindingRecord},
    file_io::SnapshotContext,
    native_document_file::{
        CapturedProjectDocumentDirectory, DirectoryPresence, NativeDocumentFileLimits,
        NativeDocumentObservation,
    },
};
use lance::dataset::scanner::DatasetRecordBatchStream;
use lance_linalg::distance::DistanceType;
pub use readonly::{
    KnowledgeEmbeddingSnapshotCapture, capture_knowledge_embedding_snapshots_guarded,
    capture_native_knowledge_embedding_snapshots_guarded,
    query_knowledge_embedding_snapshot_source_search_guarded,
    query_knowledge_vector_snapshots_guarded,
};
use std::{
    collections::BTreeSet,
    error::Error,
    ffi::OsStr,
    fmt, io,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
};
pub use stream::publish_knowledge_embeddings_stream;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeEmbeddingIdentity {
    vector_id: [u8; 32],
    dimensions: NonZeroU32,
    projection_recipe: String,
}
impl KnowledgeEmbeddingIdentity {
    pub fn new(
        vector_id: [u8; 32],
        dimensions: NonZeroU32,
        projection_recipe: String,
    ) -> Result<Self, KnowledgeEmbeddingError> {
        if i32::try_from(dimensions.get()).is_err() || projection_recipe.is_empty() {
            return Err(KnowledgeEmbeddingError::InvalidInput("identity"));
        }
        Ok(Self {
            vector_id,
            dimensions,
            projection_recipe,
        })
    }
    pub fn vector_id(&self) -> [u8; 32] {
        self.vector_id
    }
    pub fn dimensions(&self) -> NonZeroU32 {
        self.dimensions
    }
    pub fn projection_recipe(&self) -> &str {
        &self.projection_recipe
    }
}
#[derive(Clone, Copy)]
pub struct KnowledgeEmbeddingInput<'a> {
    pub slug: &'a str,
    pub input_fingerprint: [u8; 32],
}
pub struct KnowledgeEmbeddingRow<'a> {
    pub input: KnowledgeEmbeddingInput<'a>,
    pub vector: &'a [f32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeEmbeddingGeneration {
    pub version: u64,
    pub physical_branch: Option<String>,
    pub inherited_target: Option<String>,
}
/// Observe the closed documentary/vector schema without reading any row or
/// selecting another corpus. This metadata grants no access.
pub fn native_knowledge_embedding_dimensions(
    schema: &Schema,
) -> Result<Option<NonZeroU32>, KnowledgeEmbeddingError> {
    index::semantic::dimensions(schema).map_err(KnowledgeEmbeddingError::from)
}

/// Observe an exact retained NN generation and validate its complete schema.
/// This reads no vector rows and grants no authority to use the snapshot.
pub fn knowledge_embedding_snapshot_metadata(
    pin: &BranchSnapshot,
) -> Result<(KnowledgeEmbeddingGeneration, NonZeroU32), KnowledgeEmbeddingError> {
    let dimensions = u32::try_from(width(pin.schema())?)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or(KnowledgeEmbeddingError::InvalidStored("dimensions"))?;
    Ok((generation(pin), dimensions))
}

fn generation(pin: &BranchSnapshot) -> KnowledgeEmbeddingGeneration {
    KnowledgeEmbeddingGeneration {
        version: pin.version(),
        physical_branch: pin.physical_branch().map(str::to_owned),
        inherited_target: pin.inherited_target().map(str::to_owned),
    }
}

/// Retire the admitted named semantic branch without loading index or vector rows.
#[derive(Clone, Copy, Debug)]
pub enum KnowledgeEmbeddingWriteMode {
    Attach,
    ReplaceComplete,
}
#[derive(Clone, Copy, Debug)]
pub struct KnowledgeEmbeddingQuery<'a> {
    pub vector: &'a [f32],
    pub top_k: NonZeroUsize,
    pub metric: KnowledgeEmbeddingMetric,
}
#[derive(Clone, Copy, Debug)]
pub enum KnowledgeEmbeddingMetric {
    L2,
    Cosine,
    Dot,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnowledgeEmbeddingUnavailable {
    MissingSemantic,
    Incomplete,
    IncompatibleIdentity,
    IncompatibleDimensions,
}
#[derive(Debug)]
pub struct KnowledgeSemanticHit {
    pub slug: String,
    pub distance: f32,
}
#[derive(Debug)]
pub enum KnowledgeSemanticQuery {
    PublishedEmpty,
    Unavailable(KnowledgeEmbeddingUnavailable),
    Hits {
        source_generation: KnowledgeGeneration,
        semantic_generation: KnowledgeEmbeddingGeneration,
        hits: Vec<KnowledgeSemanticHit>,
    },
}
#[derive(Clone, Debug)]
pub enum KnowledgeEmbeddingRecoveryKey {
    Delta(DeltaRecoveryKey),
}
#[derive(Debug)]
pub enum KnowledgeEmbeddingResolution {
    Delta(DeltaResolution),
}
#[derive(Debug)]
pub enum KnowledgeEmbeddingPublication {
    Unchanged,
    Committed {
        generation: KnowledgeEmbeddingGeneration,
        recovery: KnowledgeEmbeddingRecoveryKey,
    },
}
#[derive(Debug)]
pub enum KnowledgeEmbeddingError {
    Index(index::KnowledgeIndexError),
    Io(io::Error),
    Branch(BranchError),
    Store(StoreError),
    Sdk(Box<lance::Error>),
    Arrow(arrow_schema::ArrowError),
    InputStream(Box<crate::lance_module_store::DataFusionError>),
    InvalidInput(&'static str),
    InvalidStored(&'static str),
    Conflict,
    CorpusChanged {
        generation: KnowledgeGeneration,
        cause: Box<KnowledgeEmbeddingError>,
    },
    Delta(Box<DeltaCommitFailure>),
    IndexFailed {
        publication: Box<KnowledgeEmbeddingPublication>,
        cause: Box<KnowledgeEmbeddingError>,
    },
    CommittedButRecheckFailed {
        publication: Box<KnowledgeEmbeddingPublication>,
        cause: Box<KnowledgeEmbeddingError>,
    },
}
impl fmt::Display for KnowledgeEmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(field) => write!(f, "invalid Knowledge embedding input: {field}"),
            Self::InvalidStored(field) => write!(f, "invalid Knowledge embedding data: {field}"),
            Self::Conflict => f.write_str("Knowledge embedding capture changed"),
            _ => f.write_str("Knowledge embedding operation failed"),
        }
    }
}
impl Error for KnowledgeEmbeddingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Index(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Branch(e) => Some(e),
            Self::Store(e) => Some(e),
            Self::Sdk(e) => Some(e.as_ref()),
            Self::Arrow(e) => Some(e),
            Self::InputStream(e) => Some(e.as_ref()),
            Self::Delta(e) => Some(e.as_ref()),
            Self::IndexFailed { cause, .. }
            | Self::CorpusChanged { cause, .. }
            | Self::CommittedButRecheckFailed { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
impl From<index::KnowledgeIndexError> for KnowledgeEmbeddingError {
    fn from(e: index::KnowledgeIndexError) -> Self {
        Self::Index(e)
    }
}
impl From<io::Error> for KnowledgeEmbeddingError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<BranchError> for KnowledgeEmbeddingError {
    fn from(e: BranchError) -> Self {
        Self::Branch(e)
    }
}
impl From<StoreError> for KnowledgeEmbeddingError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
impl From<lance::Error> for KnowledgeEmbeddingError {
    fn from(e: lance::Error) -> Self {
        Self::Sdk(Box::new(e))
    }
}
impl From<arrow_schema::ArrowError> for KnowledgeEmbeddingError {
    fn from(e: arrow_schema::ArrowError) -> Self {
        Self::Arrow(e)
    }
}
impl From<graphit_auth::AuthError> for KnowledgeEmbeddingError {
    fn from(e: graphit_auth::AuthError) -> Self {
        index::KnowledgeIndexError::Auth(e).into()
    }
}
impl From<graphit_core::artifact_store::ArtifactError> for KnowledgeEmbeddingError {
    fn from(e: graphit_core::artifact_store::ArtifactError) -> Self {
        index::KnowledgeIndexError::Artifact(e).into()
    }
}

struct ManifestCapture {
    pin: BranchSnapshot,
    directory: CapturedProjectDocumentDirectory,
    observation: NativeDocumentObservation,
}
impl ManifestCapture {
    fn recheck(&self, original: &SnapshotContext) -> Result<(), KnowledgeEmbeddingError> {
        self.directory
            .revalidate_observation(&self.observation, original)?;
        Ok(())
    }
}
struct StoredRow {
    slug: String,
    fingerprint: [u8; 32],
    identity: KnowledgeEmbeddingIdentity,
}
struct DocumentCursor {
    counts: index::NamespaceValidation,
    seen: BTreeSet<String>,
    stream: Option<DatasetRecordBatchStream>,
    pending: Option<RecordBatch>,
    eof: bool,
    failed: bool,
}
/// Owns real pins and physical observations; cannot be recreated from generation data.
pub struct KnowledgeEmbeddingCapture {
    original: Arc<SnapshotContext>,
    scope: CapturedPublicationScope,
    binding: ScopeBindingRecord,
    branch: Option<String>,
    corpus: index::LocalKnowledgeCorpus,
    source: ManifestCapture,
    semantic: Option<ManifestCapture>,
    knowledge: CapturedProjectDocumentDirectory,
    source_slugs: BTreeSet<String>,
    source_rows: usize,
    read_limits: KnowledgeIndexReadLimits,
    documents: tokio::sync::Mutex<DocumentCursor>,
    stored: Vec<StoredRow>,
    source_generation: KnowledgeGeneration,
    selected: Option<BTreeSet<String>>,
    semantic_identity: Option<KnowledgeEmbeddingIdentity>,
}
impl KnowledgeEmbeddingCapture {
    /// Revalidate this capture's original authority and physical observations.
    /// No new Source/NN pin or document stream is opened.
    pub fn recheck(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<(), KnowledgeEmbeddingError> {
        self.read(original, admission)
    }

    /// Read certified reusable vectors from this capture's retained semantic pin.
    /// The selected reader reaches EOF; the independent Source cursor is unchanged.
    pub async fn read_reusable_embedding_batch(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        identity: &KnowledgeEmbeddingIdentity,
        inputs: &[KnowledgeEmbeddingInput<'_>],
    ) -> Result<RecordBatch, KnowledgeEmbeddingError> {
        reusable::read(self, original, read, identity, inputs).await
    }
    /// Read one owned document batch from this capture's private source pin.
    /// `Some` may be empty when a batch contains only validated Removed rows;
    /// only `None` certifies EOF. Publication completion and query require it;
    /// vector batches can publish while the producer advances this cursor.
    /// An interrupted/dropped read after stream consumption invalidates the
    /// cursor; later continuation cannot turn an earlier failure into EOF.
    pub async fn next_documents(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<Option<Vec<KnowledgeDocument>>, KnowledgeEmbeddingError> {
        next_documents_guarded(original, self.view(), None, || {
            self.read(original, admission)
        })
        .await
    }
    fn complete_documents(&self) -> Result<(), KnowledgeEmbeddingError> {
        complete_document_cursor(&self.documents)
    }
    /// Own only selected documents from this capture's retained Source pin.
    /// This independent scan does not certify the CurrentInputs cursor EOF.
    pub async fn read_selected_documents(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        slugs: &[&str],
    ) -> Result<
        crate::knowledge_search::KnowledgeSelectedDocuments,
        crate::knowledge_search::KnowledgeSearchError,
    > {
        use crate::knowledge_search::{self as query, KnowledgeSearchError as E};
        self.read(original, admission).map_err(E::Capture)?;
        let graphit_core::artifact_store::StoreOwner::Project { project_id } = self.scope.owner()
        else {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        };
        query::read_selected_validated(
            original,
            self.source.pin.scan(),
            &self.source_generation,
            &self.source_slugs,
            Some(*project_id),
            slugs,
            self.read_limits,
            || self.read(original, admission).map_err(E::Capture),
        )
        .await
    }

    /// Stage the retained documentary pin with the existing SDK clone worker.
    /// The caller keeps this capture and admission and joins the handle before
    /// publishing, returning on failure, or completing shutdown.
    pub fn spawn_source_clone(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        stage: tempfile::TempDir,
    ) -> Result<
        tokio::task::JoinHandle<io::Result<crate::lance_module_store::NativeModuleClone>>,
        KnowledgeEmbeddingError,
    > {
        self.read(original, admission)?;
        self.complete_documents()?;
        if self.selected.is_some() {
            return Err(KnowledgeEmbeddingError::InvalidInput(
                "selected capture is not a snapshot",
            ));
        }
        Ok(self
            .source
            .pin
            .spawn_native_clone_local(stage, Arc::clone(original))?)
    }

    /// Whether the persisted vector space must be replaced before the first new batch.
    /// This reports observed model/schema identity and grants no publication authority.
    pub fn requires_reindex(
        &self,
        identity: &KnowledgeEmbeddingIdentity,
    ) -> Result<bool, KnowledgeEmbeddingError> {
        Ok(self
            .semantic_identity
            .as_ref()
            .is_some_and(|stored| stored != identity)
            || match &self.semantic {
                Some(semantic) => width(semantic.pin.schema())? != identity.dimensions.get() as i32,
                None => false,
            })
    }
    pub fn source_generation(&self) -> &KnowledgeGeneration {
        &self.source_generation
    }
    pub fn semantic_generation(&self) -> Option<KnowledgeEmbeddingGeneration> {
        self.semantic.as_ref().map(|v| generation(&v.pin))
    }
    pub fn stored_inputs(
        &self,
    ) -> impl Iterator<Item = (KnowledgeEmbeddingInput<'_>, &KnowledgeEmbeddingIdentity)> {
        self.stored.iter().map(|v| {
            (
                KnowledgeEmbeddingInput {
                    slug: &v.slug,
                    input_fingerprint: v.fingerprint,
                },
                &v.identity,
            )
        })
    }
    fn recheck_pins(&self, original: &Arc<SnapshotContext>) -> Result<(), KnowledgeEmbeddingError> {
        if !Arc::ptr_eq(original, &self.original) {
            return Err(index::KnowledgeIndexError::DifferentRequest.into());
        }
        original.check()?;
        self.knowledge.revalidate(original)?;
        self.source.recheck(original)?;
        if let Some(semantic) = &self.semantic {
            semantic.recheck(original)?;
        }
        Ok(())
    }
    fn read(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
    ) -> Result<(), KnowledgeEmbeddingError> {
        let project = index::check_read_project(original, admission)?;
        if project.scope.record != &self.binding
            || branch(project.version.lance_branch()) != self.branch
        {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        }
        self.recheck_pins(original)
    }
    fn write(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
    ) -> Result<(), KnowledgeEmbeddingError> {
        let project = index::check_write_project(original, admission)?;
        if project.scope.record != &self.binding
            || branch(project.version.lance_branch()) != self.branch
        {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        }
        self.recheck_pins(original)
    }
}
pub enum KnowledgeEmbeddingRead {
    NoCorpus,
    Published(Box<KnowledgeEmbeddingCapture>),
}
fn branch(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
fn knowledge_directory(
    scope: &CapturedPublicationScope,
    corpus: &index::LocalKnowledgeCorpus,
    original: &Arc<SnapshotContext>,
    limits: NativeDocumentFileLimits,
) -> Result<CapturedProjectDocumentDirectory, KnowledgeEmbeddingError> {
    Ok(CapturedProjectDocumentDirectory::capture_runtime(
        scope,
        corpus
            .relative
            .parent()
            .ok_or(KnowledgeEmbeddingError::InvalidInput("corpus parent"))?,
        limits,
        original,
    )?)
}
async fn capture_manifest(
    scope: &CapturedPublicationScope,
    original: &Arc<SnapshotContext>,
    pin: BranchSnapshot,
    corpus: &Path,
    limits: NativeDocumentFileLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<ManifestCapture, KnowledgeEmbeddingError> {
    before()?;
    let store = pin.dataset().object_store(None).await?;
    before()?;
    if !store.has_direct_local_paths() {
        return Err(KnowledgeEmbeddingError::InvalidInput("nonlocal manifest"));
    }
    let local = native_manifest_path(
        Path::new(&lance_io::local::to_local_path(
            &pin.dataset().manifest_location().path,
        )),
        corpus,
    )?;
    let parent = local
        .parent()
        .ok_or(KnowledgeEmbeddingError::InvalidStored("manifest path"))?;
    let relative = parent
        .strip_prefix(scope.runtime_root())
        .map_err(|_| KnowledgeEmbeddingError::InvalidStored("manifest scope"))?;
    let name = local
        .file_name()
        .ok_or(KnowledgeEmbeddingError::InvalidStored("manifest name"))?;
    let directory =
        CapturedProjectDocumentDirectory::capture_runtime(scope, relative, limits, original)?;
    let observation = directory.read(name, original)?;
    if observation.bytes().is_none() {
        return Err(KnowledgeEmbeddingError::Conflict);
    }
    before()?;
    let reopened = pin.reopen().await?;
    before()?;
    directory.revalidate_observation(&observation, original)?;
    if reopened.dataset().manifest_location().path != pin.dataset().manifest_location().path {
        return Err(KnowledgeEmbeddingError::Conflict);
    }
    Ok(ManifestCapture {
        pin: reopened,
        directory,
        observation,
    })
}

// Translate only the SDK's native spelling back under the retained corpus.
// Captured directory traversal and the leaf read still reject symlinks and swaps.
fn native_manifest_path(local: &Path, corpus: &Path) -> Result<PathBuf, KnowledgeEmbeddingError> {
    let uri = corpus
        .to_str()
        .ok_or(KnowledgeEmbeddingError::InvalidInput("non-Unicode corpus"))?;
    let sdk = crate::lance_module_store::sdk_uri(uri)?;
    let relative = local
        .strip_prefix(Path::new(sdk.as_ref()))
        .map_err(|_| KnowledgeEmbeddingError::InvalidStored("manifest path"))?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(KnowledgeEmbeddingError::InvalidStored("manifest path"));
    }
    Ok(corpus.join(relative))
}
fn schema(width: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("slug", DataType::Utf8, false),
        Field::new("input_fingerprint", DataType::FixedSizeBinary(32), false),
        Field::new("vector_id", DataType::FixedSizeBinary(32), false),
        Field::new("projection_recipe", DataType::Utf8, false),
        Field::new("role", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), width),
            false,
        ),
    ]))
}
fn width(observed: &Schema) -> Result<i32, KnowledgeEmbeddingError> {
    index::semantic::dimensions(observed)?
        .map(|n| n.get() as i32)
        .ok_or(KnowledgeEmbeddingError::InvalidStored(
            "missing vector column",
        ))
}
fn column<'a, T: 'static>(
    batch: &'a RecordBatch,
    index: usize,
    name: &'static str,
) -> Result<&'a T, KnowledgeEmbeddingError> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<T>()
        .ok_or(KnowledgeEmbeddingError::InvalidStored(name))
}
async fn decode_semantic(
    pin: &ManifestCapture,
    original: &Arc<SnapshotContext>,
    limits: KnowledgeIndexReadLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<Vec<StoredRow>, KnowledgeEmbeddingError> {
    let mut rows = Vec::new();
    scan_semantic_rows(pin, original, limits, &mut before, |row, _| {
        rows.push(row);
        Ok(())
    })
    .await?;
    let validated = validate_stored_rows(&mut rows);
    before()?;
    pin.recheck(original)?;
    validated?;
    Ok(rows)
}
async fn scan_semantic_rows(
    pin: &ManifestCapture,
    original: &Arc<SnapshotContext>,
    limits: KnowledgeIndexReadLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    visit: impl FnMut(StoredRow, &[f32]) -> Result<(), KnowledgeEmbeddingError>,
) -> Result<(), KnowledgeEmbeddingError> {
    scan_semantic_pin(
        original,
        &pin.pin,
        limits,
        || {
            before()?;
            pin.recheck(original)
        },
        visit,
    )
    .await
}
async fn scan_semantic_pin(
    original: &SnapshotContext,
    pin: &BranchSnapshot,
    limits: KnowledgeIndexReadLimits,
    before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    visit: impl FnMut(StoredRow, &[f32]) -> Result<(), KnowledgeEmbeddingError>,
) -> Result<(), KnowledgeEmbeddingError> {
    scan_semantic_reader(original, pin, pin.scan(), limits, before, visit).await
}
async fn scan_semantic_reader(
    original: &SnapshotContext,
    pin: &BranchSnapshot,
    mut scanner: lance::dataset::scanner::Scanner,
    limits: KnowledgeIndexReadLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    mut visit: impl FnMut(StoredRow, &[f32]) -> Result<(), KnowledgeEmbeddingError>,
) -> Result<(), KnowledgeEmbeddingError> {
    let dimensions = u32::try_from(width(pin.schema())?)
        .map_err(|_| KnowledgeEmbeddingError::InvalidStored("dimensions"))?;
    let dimensions =
        NonZeroU32::new(dimensions).ok_or(KnowledgeEmbeddingError::InvalidStored("dimensions"))?;
    scanner.project(&index::semantic::COLUMNS)?;
    scanner.batch_size(limits.batch_rows.get() as usize);
    let mut stream = embedding_guarded(scanner.try_into_stream(), &mut before).await??;
    loop {
        let next = embedding_guarded(stream.try_next(), &mut before).await??;
        let Some(batch) = next else {
            break;
        };
        let decoded =
            (|| -> Result<(), KnowledgeEmbeddingError> {
                let slugs = column::<StringArray>(&batch, 0, "slug")?;
                let fingerprints = column::<FixedSizeBinaryArray>(&batch, 1, "fingerprint")?;
                let configs = column::<FixedSizeBinaryArray>(&batch, 2, "config")?;
                let recipes = column::<StringArray>(&batch, 3, "recipe")?;
                let roles = column::<StringArray>(&batch, 4, "role")?;
                let vectors = column::<FixedSizeListArray>(&batch, 5, "vector")?;
                for row in 0..batch.num_rows() {
                    original.check()?;
                    let absent = vectors.is_null(row);
                    if slugs.is_null(row)
                        || !absent && batch.columns()[1..].iter().any(|v| v.is_null(row))
                    {
                        return Err(KnowledgeEmbeddingError::InvalidStored("partial embedding"));
                    }
                    if absent {
                        continue;
                    }
                    let slug = slugs.value(row);
                    let recipe = recipes.value(row);
                    if slug.is_empty()
                        || recipe.is_empty()
                        || roles.value(row) != "Document"
                        || slug.len() > limits.content.max_identifier_bytes.get()
                        || recipe.len() > limits.content.max_identifier_bytes.get()
                    {
                        return Err(KnowledgeEmbeddingError::InvalidStored("content"));
                    }
                    let value = vectors.value(row);
                    let floats = value
                        .as_any()
                        .downcast_ref::<Float32Array>()
                        .ok_or(KnowledgeEmbeddingError::InvalidStored("vector"))?;
                    if floats.len() != dimensions.get() as usize
                        || floats.null_count() != 0
                        || floats.values().iter().any(|v| !v.is_finite())
                    {
                        return Err(KnowledgeEmbeddingError::InvalidStored("vector"));
                    }
                    visit(
                        StoredRow {
                            slug: slug.to_owned(),
                            fingerprint: fingerprints.value(row).try_into().map_err(|_| {
                                KnowledgeEmbeddingError::InvalidStored("fingerprint")
                            })?,
                            identity: KnowledgeEmbeddingIdentity::new(
                                configs.value(row).try_into().map_err(|_| {
                                    KnowledgeEmbeddingError::InvalidStored("config")
                                })?,
                                dimensions,
                                recipe.to_owned(),
                            )?,
                        },
                        floats.values().as_ref(),
                    )?;
                }
                Ok(())
            })();
        before()?;
        decoded?;
    }
    before()?;
    Ok(())
}

async fn matching_stored_rows(
    capture: &KnowledgeEmbeddingCapture,
    original: &Arc<SnapshotContext>,
    identity: &KnowledgeEmbeddingIdentity,
    rows: &[KnowledgeEmbeddingRow<'_>],
    mode: KnowledgeEmbeddingWriteMode,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<BTreeSet<String>, KnowledgeEmbeddingError> {
    let mut equal = BTreeSet::new();
    before()?;
    let Some(semantic) = &capture.semantic else {
        return Ok(equal);
    };
    let mut selected = std::collections::BTreeMap::new();
    for row in rows {
        original.check()?;
        if capture
            .stored
            .binary_search_by(|stored| stored.slug.as_str().cmp(row.input.slug))
            .is_ok()
        {
            selected.insert(row.input.slug, row);
        }
    }
    before()?;
    if selected.is_empty() {
        return Ok(equal);
    }
    let mut scanner = semantic.pin.scan();
    scanner.filter_expr(
        lance::deps::datafusion::logical_expr::col("slug").in_list(
            selected
                .keys()
                .map(|slug| lance::deps::datafusion::logical_expr::lit(*slug))
                .collect(),
            false,
        ),
    );
    let mut seen = BTreeSet::new();
    scan_semantic_reader(
        original,
        &semantic.pin,
        scanner,
        capture.read_limits,
        &mut before,
        |stored, vector| {
            if !seen.insert(stored.slug.clone()) {
                return Err(KnowledgeEmbeddingError::InvalidStored("duplicate slug"));
            }
            let previous = capture
                .stored
                .binary_search_by(|row| row.slug.cmp(&stored.slug))
                .ok()
                .map(|at| &capture.stored[at])
                .ok_or(KnowledgeEmbeddingError::Conflict)?;
            if previous.fingerprint != stored.fingerprint || previous.identity != stored.identity {
                return Err(KnowledgeEmbeddingError::Conflict);
            }
            let row = selected
                .get(stored.slug.as_str())
                .ok_or(KnowledgeEmbeddingError::Conflict)?;
            let same_input =
                stored.fingerprint == row.input.input_fingerprint && &stored.identity == identity;
            if same_input && vector == row.vector {
                equal.insert(stored.slug);
            } else if same_input && matches!(mode, KnowledgeEmbeddingWriteMode::Attach) {
                return Err(KnowledgeEmbeddingError::InvalidInput("already embedded"));
            }
            Ok(())
        },
    )
    .await?;
    if seen.len() != selected.len() {
        return Err(KnowledgeEmbeddingError::Conflict);
    }
    Ok(equal)
}

pub async fn capture_knowledge_embedding_corpus(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeEmbeddingRead, KnowledgeEmbeddingError> {
    capture_selection(original, admission, None, limits).await
}

/// Capture only requested Source documents for one independently confirmed NN batch.
/// This retains genuine Source/semantic pins, authorization and the selected SDK EOF.
pub async fn capture_knowledge_embedding_selection(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    slugs: &BTreeSet<String>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeEmbeddingRead, KnowledgeEmbeddingError> {
    if slugs
        .iter()
        .any(|slug| slug.is_empty() || slug.len() > limits.content.max_identifier_bytes.get())
    {
        return Err(KnowledgeEmbeddingError::InvalidInput("slugs"));
    }
    capture_selection(original, admission, Some(slugs), limits).await
}

async fn capture_selection(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    selection: Option<&BTreeSet<String>>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeEmbeddingRead, KnowledgeEmbeddingError> {
    let project = index::check_read_project(original, admission)?;
    let scope = project.scope.live;
    let corpus = index::local_corpus(scope, project.version.version(), original, limits.native)?;
    let branch = branch(project.version.lance_branch());
    let Some(source_directory) =
        index::capture_index(scope, &corpus, original, limits.native, || {
            index::check_read(original, admission)
        })?
    else {
        index::check_read(original, admission)?;
        return Ok(KnowledgeEmbeddingRead::NoCorpus);
    };
    let before = || {
        index::check_read(original, admission)?;
        source_directory.revalidate(original)?;
        Ok(())
    };
    let uri = corpus.uri.clone();
    before()?;
    let pin = BranchSnapshot::capture_observed(&uri, branch.as_deref(), None).await?;
    before()?;
    index::semantic::dimensions(pin.schema())?;
    let source =
        capture_manifest(scope, original, pin, &corpus.path, limits.native, before).await?;
    let graphit_core::artifact_store::StoreOwner::Project { project_id } = scope.owner() else {
        return Err(index::KnowledgeIndexError::WrongAdmission.into());
    };
    let selected_scan = || {
        let mut scanner = source.pin.scan();
        if let Some(slugs) = selection {
            scanner.filter_expr(
                lance::deps::datafusion::logical_expr::col("slug").in_list(
                    slugs
                        .iter()
                        .map(|slug| lance::deps::datafusion::logical_expr::lit(slug.as_str()))
                        .collect(),
                    false,
                ),
            );
        }
        scanner
    };
    let (source_slugs, source_rows) =
        audit_source_pin(original, selected_scan(), limits, Some(*project_id), || {
            before()?;
            source.recheck(original)
        })
        .await?;
    before()?;
    source.recheck(original)?;
    let mut scanner = selected_scan();
    scanner.project(&index::semantic::DOCUMENT_COLUMNS)?;
    scanner.batch_size(limits.batch_rows.get() as usize);
    let documents_stream = scanner.try_into_stream().await?;
    drop(scanner);
    before()?;
    source.recheck(original)?;
    let knowledge = knowledge_directory(scope, &corpus, original, limits.native)?;
    let semantic = if index::semantic::dimensions(source.pin.schema())?.is_some() {
        Some(
            capture_manifest(
                scope,
                original,
                source.pin.clone(),
                &corpus.path,
                limits.native,
                before,
            )
            .await?,
        )
    } else {
        None
    };
    let (semantic_identity, stored) = match (&semantic, selection) {
        (Some(pin), Some(selection)) => {
            decode_semantic_metadata(pin, original, selection, limits, before).await?
        }
        (Some(pin), None) => {
            let stored = decode_semantic(pin, original, limits, before).await?;
            (stored.first().map(|row| row.identity.clone()), stored)
        }
        (None, _) => (None, Vec::new()),
    };
    let source_generation = index::generation(&uri, &source.pin);
    let capture = KnowledgeEmbeddingCapture {
        original: Arc::clone(original),
        scope: scope.clone(),
        binding: project.scope.record.clone(),
        branch,
        corpus,
        source,
        semantic,
        knowledge,
        source_slugs,
        source_rows,
        read_limits: limits,
        documents: tokio::sync::Mutex::new(DocumentCursor {
            counts: index::NamespaceValidation::bound(Some(*project_id)),
            seen: BTreeSet::new(),
            stream: Some(documents_stream),
            pending: None,
            eof: false,
            failed: false,
        }),
        stored,
        source_generation,
        selected: selection.cloned(),
        semantic_identity,
    };
    capture.read(original, admission)?;
    Ok(KnowledgeEmbeddingRead::Published(Box::new(capture)))
}
fn validate_inputs(
    capture: &KnowledgeEmbeddingCapture,
    inputs: &[KnowledgeEmbeddingInput<'_>],
    complete: bool,
) -> Result<(), KnowledgeEmbeddingError> {
    validate_read_inputs(capture.view(), inputs, complete)
}
fn encode(
    identity: &KnowledgeEmbeddingIdentity,
    rows: &[KnowledgeEmbeddingRow<'_>],
) -> Result<RecordBatch, KnowledgeEmbeddingError> {
    let width = i32::try_from(identity.dimensions.get())
        .map_err(|_| KnowledgeEmbeddingError::InvalidInput("dimensions"))?;
    rows.len()
        .checked_mul(identity.dimensions.get() as usize)
        .and_then(|v| v.checked_mul(size_of::<f32>()))
        .ok_or(KnowledgeEmbeddingError::InvalidInput("vector overflow"))?;
    let mut fingerprints = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    let mut configs = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    for row in rows {
        if row.vector.len() != identity.dimensions.get() as usize
            || row.vector.iter().any(|v| !v.is_finite())
        {
            return Err(KnowledgeEmbeddingError::InvalidInput("vector"));
        }
        fingerprints.append_value(row.input.input_fingerprint)?;
        configs.append_value(identity.vector_id)?;
    }
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|v| v.input.slug),
        )),
        Arc::new(fingerprints.finish()),
        Arc::new(configs.finish()),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|_| identity.projection_recipe.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|_| "Document"),
        )),
        Arc::new(FixedSizeListArray::try_new(
            Arc::new(Field::new("item", DataType::Float32, true)),
            width,
            Arc::new(Float32Array::from_iter_values(
                rows.iter().flat_map(|v| v.vector.iter().copied()),
            )),
            None,
        )?),
    ];
    Ok(RecordBatch::try_new(schema(width), columns)?)
}
/// Ordinary vector input schema, without authorization or capture authority.
pub fn native_knowledge_embedding_schema(identity: &KnowledgeEmbeddingIdentity) -> Arc<Schema> {
    schema(identity.dimensions.get() as i32)
}
/// Encode one borrowed vector batch under the real write admission.
pub fn encode_native_knowledge_embedding_batch(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    identity: &KnowledgeEmbeddingIdentity,
    rows: &[KnowledgeEmbeddingRow<'_>],
    limits: KnowledgeIndexWriteLimits,
) -> Result<RecordBatch, KnowledgeEmbeddingError> {
    index::check_write(original, admission)?;
    if identity.projection_recipe.len() > limits.content.max_identifier_bytes.get() {
        return Err(KnowledgeEmbeddingError::InvalidInput("recipe"));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        index::check_write(original, admission)?;
        if row.input.slug.is_empty()
            || row.input.slug.len() > limits.content.max_identifier_bytes.get()
        {
            return Err(KnowledgeEmbeddingError::InvalidInput("slug"));
        }
        if !seen.insert(row.input.slug) {
            return Err(KnowledgeEmbeddingError::InvalidInput("slugs"));
        }
    }
    let batch = encode(identity, rows)?;
    index::check_write(original, admission)?;
    Ok(batch)
}
pub async fn publish_knowledge_embeddings(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    capture: &KnowledgeEmbeddingCapture,
    identity: &KnowledgeEmbeddingIdentity,
    rows: &[KnowledgeEmbeddingRow<'_>],
    mode: KnowledgeEmbeddingWriteMode,
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeEmbeddingPublication, KnowledgeEmbeddingError> {
    capture.write(original, admission)?;
    let inputs: Vec<_> = rows.iter().map(|row| row.input).collect();
    validate_inputs(
        capture,
        &inputs,
        matches!(mode, KnowledgeEmbeddingWriteMode::ReplaceComplete),
    )?;
    let batch =
        encode_native_knowledge_embedding_batch(original, admission, identity, rows, limits)?;
    let input = Box::pin(crate::lance_module_store::RecordBatchStreamAdapter::new(
        batch.schema(),
        futures::stream::iter([Ok(batch)]),
    ));
    publish_knowledge_embeddings_stream(original, admission, capture, identity, input, mode, limits)
        .await
}

fn finish_publication(
    publication: KnowledgeEmbeddingPublication,
    recheck: impl FnOnce() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<KnowledgeEmbeddingPublication, KnowledgeEmbeddingError> {
    match recheck() {
        Ok(()) => Ok(publication),
        Err(cause) => Err(KnowledgeEmbeddingError::CommittedButRecheckFailed {
            publication: Box::new(publication),
            cause: Box::new(cause),
        }),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnowledgeEmbeddingSearchReadiness {
    Ready,
    PublishedEmpty,
    Unavailable(KnowledgeEmbeddingUnavailable),
}
/// Reuse the actual capture/cursor/identity/coverage guards before a Query RPC.
pub fn knowledge_embedding_search_readiness(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    capture: &KnowledgeEmbeddingCapture,
    identity: &KnowledgeEmbeddingIdentity,
    current_inputs: &[KnowledgeEmbeddingInput<'_>],
) -> Result<KnowledgeEmbeddingSearchReadiness, KnowledgeEmbeddingError> {
    read_search_readiness(capture.view(), identity, current_inputs, || {
        capture.read(original, admission)
    })
}

pub async fn query_knowledge_embeddings(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    capture: &KnowledgeEmbeddingCapture,
    identity: &KnowledgeEmbeddingIdentity,
    current_inputs: &[KnowledgeEmbeddingInput<'_>],
    query: KnowledgeEmbeddingQuery<'_>,
) -> Result<KnowledgeSemanticQuery, KnowledgeEmbeddingError> {
    query_embedding_pin(capture.view(), identity, current_inputs, query, || {
        capture.read(original, admission)
    })
    .await
}
pub async fn resolve_knowledge_embedding_publication(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    recovery: &KnowledgeEmbeddingRecoveryKey,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeEmbeddingResolution, KnowledgeEmbeddingError> {
    let project = index::check_read_project(original, admission)?;
    let scope = project.scope.live;
    let corpus = index::local_corpus(scope, project.version.version(), original, limits.native)?;
    let branch = branch(project.version.lance_branch());
    let uri = corpus.uri.clone();
    let KnowledgeEmbeddingRecoveryKey::Delta(key) = recovery;
    if key.corpus_uri() != uri || key.physical_branch() != branch.as_deref() {
        return Err(KnowledgeEmbeddingError::InvalidInput("recovery binding"));
    }
    let guard = knowledge_directory(scope, &corpus, original, limits.native)?;
    let before = || {
        index::check_read(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    before()?;
    if guard.probe_child_directory(OsStr::new("index.lance"), original)?
        == DirectoryPresence::Absent
    {
        before()?;
        if !corpus.default_layout {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
        }
        return Ok(KnowledgeEmbeddingResolution::Delta(
            DeltaResolution::Unresolved,
        ));
    }
    let pin = BranchSnapshot::capture_observed(&uri, branch.as_deref(), None).await?;
    let captured =
        capture_manifest(scope, original, pin, &corpus.path, limits.native, before).await?;
    let resolution = KnowledgeEmbeddingResolution::Delta(captured.pin.resolve(key).await?);
    before()?;
    captured.recheck(original)?;
    Ok(resolution)
}

#[derive(Clone, Copy)]
struct EmbeddingReadView<'a> {
    semantic: Option<&'a BranchSnapshot>,
    source_slugs: &'a BTreeSet<String>,
    source_rows: usize,
    read_limits: KnowledgeIndexReadLimits,
    documents: &'a tokio::sync::Mutex<DocumentCursor>,
    stored: &'a [StoredRow],
    source_generation: &'a KnowledgeGeneration,
}
impl KnowledgeEmbeddingCapture {
    fn view(&self) -> EmbeddingReadView<'_> {
        EmbeddingReadView {
            semantic: self.semantic.as_ref().map(|v| &v.pin),
            source_slugs: &self.source_slugs,
            source_rows: self.source_rows,
            read_limits: self.read_limits,
            documents: &self.documents,
            stored: &self.stored,
            source_generation: &self.source_generation,
        }
    }
}

async fn next_documents_guarded(
    original: &SnapshotContext,
    view: EmbeddingReadView<'_>,
    initial_scan: Option<lance::dataset::scanner::Scanner>,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<Option<Vec<KnowledgeDocument>>, KnowledgeEmbeddingError> {
    before()?;
    let mut cursor = view
        .documents
        .try_lock()
        .map_err(|_| KnowledgeEmbeddingError::InvalidInput("document cursor busy"))?;
    if cursor.failed {
        return Err(KnowledgeEmbeddingError::InvalidInput(
            "invalid document cursor",
        ));
    }
    if cursor.eof {
        return Ok(None);
    }
    cursor.failed = true;
    let mut stream = match cursor.stream.take() {
        Some(stream) => stream,
        None => {
            let mut scanner = initial_scan.ok_or(KnowledgeEmbeddingError::InvalidInput(
                "invalid document cursor",
            ))?;
            scanner.batch_size(view.read_limits.batch_rows.get() as usize);
            embedding_guarded(scanner.try_into_stream(), &mut before).await??
        }
    };
    // Cached SDK batches may be immediately ready; yield cooperatively so
    // cancellation and other jobs are observable at every cursor boundary.
    tokio::task::yield_now().await;
    before()?;
    let next = match cursor.pending.take() {
        Some(batch) => Some(batch),
        None => embedding_guarded(stream.try_next(), &mut before).await??,
    };
    before()?;
    let Some(batch) = next else {
        let complete = (|| -> Result<(), KnowledgeEmbeddingError> {
            original.check()?;
            if cursor.seen.len() != view.source_rows {
                return Err(KnowledgeEmbeddingError::Conflict);
            }
            for slug in view.source_slugs {
                original.check()?;
                if !cursor.seen.contains(slug) {
                    return Err(KnowledgeEmbeddingError::Conflict);
                }
            }
            Ok(())
        })();
        before()?;
        complete?;
        cursor.eof = true;
        cursor.failed = false;
        return Ok(None);
    };
    let size = view.read_limits.batch_rows.get() as usize;
    let batch = if batch.num_rows() > size {
        cursor.pending = Some(batch.slice(size, batch.num_rows() - size));
        batch.slice(0, size)
    } else {
        batch
    };
    let mut counts = std::mem::take(&mut cursor.counts);
    let mut documents = Vec::new();
    let decoded = (|| -> Result<(), KnowledgeEmbeddingError> {
        index::decode_batch(
            &batch,
            view.read_limits.content,
            &mut counts,
            &mut documents,
            || original.check().map_err(index::KnowledgeIndexError::from),
        )?;
        let slugs: &StringArray = index::array(&batch, 0)?;
        for slug in slugs.iter() {
            original.check()?;
            let slug = slug.ok_or(KnowledgeEmbeddingError::InvalidStored("slug"))?;
            if !cursor.seen.insert(slug.to_owned()) {
                return Err(KnowledgeEmbeddingError::InvalidStored(
                    "duplicate source slug",
                ));
            }
        }
        for document in &documents {
            original.check()?;
            if !view.source_slugs.contains(&document.slug) {
                return Err(KnowledgeEmbeddingError::Conflict);
            }
        }
        Ok(())
    })();
    before()?;
    decoded?;
    cursor.counts = counts;
    cursor.stream = Some(stream);
    cursor.failed = false;
    Ok(Some(documents))
}

fn complete_document_cursor(
    documents: &tokio::sync::Mutex<DocumentCursor>,
) -> Result<(), KnowledgeEmbeddingError> {
    let cursor = documents
        .try_lock()
        .map_err(|_| KnowledgeEmbeddingError::InvalidInput("document cursor not complete"))?;
    if !cursor.eof || cursor.failed {
        return Err(KnowledgeEmbeddingError::InvalidInput(
            "document cursor not complete",
        ));
    }
    Ok(())
}

fn validate_read_inputs(
    view: EmbeddingReadView<'_>,
    inputs: &[KnowledgeEmbeddingInput<'_>],
    complete: bool,
) -> Result<(), KnowledgeEmbeddingError> {
    complete_document_cursor(view.documents)?;
    let mut slugs = BTreeSet::new();
    for input in inputs {
        if !view.source_slugs.contains(input.slug) || !slugs.insert(input.slug) {
            return Err(KnowledgeEmbeddingError::InvalidInput("slugs"));
        }
    }
    if complete && inputs.len() != view.source_slugs.len() {
        return Err(KnowledgeEmbeddingError::InvalidInput("incomplete inputs"));
    }
    Ok(())
}

fn compatible_input(
    stored: &[StoredRow],
    identity: &KnowledgeEmbeddingIdentity,
    input: &KnowledgeEmbeddingInput<'_>,
) -> bool {
    stored
        .binary_search_by(|row| row.slug.as_str().cmp(input.slug))
        .ok()
        .is_some_and(|position| {
            stored[position].fingerprint == input.input_fingerprint
                && &stored[position].identity == identity
        })
}

fn read_search_readiness(
    view: EmbeddingReadView<'_>,
    identity: &KnowledgeEmbeddingIdentity,
    current_inputs: &[KnowledgeEmbeddingInput<'_>],
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<KnowledgeEmbeddingSearchReadiness, KnowledgeEmbeddingError> {
    before()?;
    validate_read_inputs(view, current_inputs, true)?;
    let mut unavailable = |reason| {
        before()?;
        Ok(KnowledgeEmbeddingSearchReadiness::Unavailable(reason))
    };
    if view.source_slugs.is_empty() {
        before()?;
        return Ok(KnowledgeEmbeddingSearchReadiness::PublishedEmpty);
    }
    let Some(semantic) = &view.semantic else {
        return unavailable(KnowledgeEmbeddingUnavailable::MissingSemantic);
    };
    if width(semantic.schema())?
        != i32::try_from(identity.dimensions.get())
            .map_err(|_| KnowledgeEmbeddingError::InvalidInput("dimensions"))?
    {
        return unavailable(KnowledgeEmbeddingUnavailable::IncompatibleDimensions);
    }
    if view.stored.iter().any(|v| &v.identity != identity) {
        return unavailable(KnowledgeEmbeddingUnavailable::IncompatibleIdentity);
    }
    if !current_inputs
        .iter()
        .any(|input| compatible_input(view.stored, identity, input))
    {
        return unavailable(KnowledgeEmbeddingUnavailable::Incomplete);
    }
    before()?;
    Ok(KnowledgeEmbeddingSearchReadiness::Ready)
}

async fn query_embedding_pin(
    view: EmbeddingReadView<'_>,
    identity: &KnowledgeEmbeddingIdentity,
    current_inputs: &[KnowledgeEmbeddingInput<'_>],
    query: KnowledgeEmbeddingQuery<'_>,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<KnowledgeSemanticQuery, KnowledgeEmbeddingError> {
    let KnowledgeEmbeddingQuery {
        vector: query_vector,
        top_k,
        metric,
    } = query;
    let readiness = read_search_readiness(view, identity, current_inputs, &mut before)?;
    if query_vector.len() != identity.dimensions.get() as usize
        || query_vector.iter().any(|v| !v.is_finite())
    {
        return Err(KnowledgeEmbeddingError::InvalidInput("query vector"));
    }
    match readiness {
        KnowledgeEmbeddingSearchReadiness::PublishedEmpty => {
            return Ok(KnowledgeSemanticQuery::PublishedEmpty);
        }
        KnowledgeEmbeddingSearchReadiness::Unavailable(reason) => {
            return Ok(KnowledgeSemanticQuery::Unavailable(reason));
        }
        KnowledgeEmbeddingSearchReadiness::Ready => {}
    }
    let semantic = view
        .semantic
        .ok_or(KnowledgeEmbeddingError::InvalidStored("ready semantic"))?;
    let query = Float32Array::from(query_vector.to_vec());
    let mut scanner = semantic.scan();
    scanner.project(&["slug"])?;
    scanner.prefilter(true);
    let eligible: BTreeSet<_> = current_inputs
        .iter()
        .filter(|input| compatible_input(view.stored, identity, input))
        .map(|input| input.slug)
        .collect();
    scanner.filter_expr(
        lance::deps::datafusion::logical_expr::col("slug").in_list(
            eligible
                .iter()
                .map(|slug| lance::deps::datafusion::logical_expr::lit(*slug))
                .collect(),
            false,
        ),
    );
    scanner
        .nearest("vector", &query, top_k.get())?
        .use_index(true)
        .distance_metric(match metric {
            KnowledgeEmbeddingMetric::L2 => DistanceType::L2,
            KnowledgeEmbeddingMetric::Cosine => DistanceType::Cosine,
            KnowledgeEmbeddingMetric::Dot => DistanceType::Dot,
        });
    before()?;
    let mut stream = embedding_guarded(scanner.try_into_stream(), &mut before).await??;
    before()?;
    let mut hits = Vec::new();
    let mut seen = BTreeSet::new();
    loop {
        before()?;
        let next = embedding_guarded(stream.try_next(), &mut before).await??;
        before()?;
        let Some(batch) = next else {
            break;
        };
        let slugs = batch
            .column_by_name("slug")
            .and_then(|v| v.as_any().downcast_ref::<StringArray>())
            .ok_or(KnowledgeEmbeddingError::InvalidStored("NN slug"))?;
        let distances = batch
            .column_by_name("_distance")
            .and_then(|v| v.as_any().downcast_ref::<Float32Array>())
            .ok_or(KnowledgeEmbeddingError::InvalidStored("distance"))?;
        if slugs.null_count() != 0 || distances.null_count() != 0 || distances.len() != slugs.len()
        {
            return Err(KnowledgeEmbeddingError::InvalidStored("NN null/shape"));
        }
        for row in 0..batch.num_rows() {
            before()?;
            let slug = slugs.value(row);
            let distance = distances.value(row);
            if !distance.is_finite() {
                return Err(KnowledgeEmbeddingError::InvalidStored("distance"));
            }
            if !eligible.contains(slug)
                || !seen.insert(slug.to_owned())
                || hits.len() >= top_k.get()
            {
                return Err(KnowledgeEmbeddingError::InvalidStored("NN slug/count"));
            }
            hits.push(KnowledgeSemanticHit {
                slug: slug.to_owned(),
                distance,
            });
        }
    }
    before()?;
    Ok(KnowledgeSemanticQuery::Hits {
        source_generation: view.source_generation.clone(),
        semantic_generation: generation(semantic),
        hits,
    })
}

async fn audit_source_pin(
    original: &SnapshotContext,
    mut scanner: lance::dataset::scanner::Scanner,
    limits: KnowledgeIndexReadLimits,
    expected: Option<graphit_core::project_identity::ProjectId>,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<(BTreeSet<String>, usize), KnowledgeEmbeddingError> {
    scanner.project(&index::semantic::DOCUMENT_COLUMNS)?;
    scanner.batch_size(limits.batch_rows.get() as usize);
    let mut stream = embedding_guarded(scanner.try_into_stream(), &mut before).await??;
    let mut counts = index::NamespaceValidation::bound(expected);
    let mut seen = BTreeSet::new();
    let mut source_slugs = BTreeSet::new();
    loop {
        let next = embedding_guarded(stream.try_next(), &mut before).await??;
        let Some(batch) = next else { break };
        let decoded = (|| -> Result<(), KnowledgeEmbeddingError> {
            index::validate_full_batch(&batch, limits.content, &mut counts, || {
                original.check().map_err(index::KnowledgeIndexError::from)
            })?;
            let slugs: &StringArray = index::array(&batch, 0)?;
            let states: &UInt8Array = index::array(&batch, 16)?;
            for row in 0..batch.num_rows() {
                original.check()?;
                let slug = slugs.value(row);
                if !seen.insert(slug.to_owned()) {
                    return Err(KnowledgeEmbeddingError::InvalidStored(
                        "duplicate source slug",
                    ));
                }
                if states.value(row)
                    == crate::knowledge_references::KnowledgeDocumentState::Active as u8
                {
                    source_slugs.insert(slug.to_owned());
                }
            }
            Ok(())
        })();
        before()?;
        decoded?;
    }
    drop(stream);
    drop(scanner);
    let source_rows = seen.len();
    drop(seen);
    before()?;
    Ok((source_slugs, source_rows))
}

async fn embedding_guarded<T>(
    future: impl std::future::Future<Output = T>,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<T, KnowledgeEmbeddingError> {
    tokio::pin!(future);
    loop {
        before()?;
        tokio::select! {
            biased;
            () = tokio::time::sleep(std::time::Duration::from_millis(25)) => {},
            value = &mut future => { before()?; return Ok(value); }
        }
    }
}

fn validate_stored_rows(rows: &mut [StoredRow]) -> Result<(), KnowledgeEmbeddingError> {
    rows.sort_unstable_by(|a, b| a.slug.cmp(&b.slug));
    if rows.windows(2).any(|v| v[0].slug == v[1].slug) {
        return Err(KnowledgeEmbeddingError::InvalidStored("duplicate slug"));
    }
    if rows.windows(2).any(|v| v[0].identity != v[1].identity) {
        return Err(KnowledgeEmbeddingError::InvalidStored("mixed identity"));
    }
    Ok(())
}
