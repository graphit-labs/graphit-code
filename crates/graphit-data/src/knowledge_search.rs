//! Lexical Knowledge search on an admitted, validated SDK pin.
//! Continuations and source observations are ordinary data, never authority.
use crate::{
    knowledge_document_projection::CompilationDigest32,
    knowledge_embeddings::{KnowledgeEmbeddingError, search::LexicalSourceCapture},
    knowledge_index::{
        self as index, KnowledgeDocument, KnowledgeGeneration, KnowledgeIndexError,
        KnowledgeIndexReadLimits,
    },
    knowledge_reference_query::{
        KnowledgeReferenceOwnedNamespace, KnowledgeReferenceOwnedScope, KnowledgeReferenceSource,
    },
    knowledge_references::{self as refs, KnowledgeDocumentState, KnowledgeReferenceScope},
    lance_module_store::{ModuleSnapshot, StoreError},
};
use arrow_array::{
    Array, FixedSizeBinaryArray, Float32Array, ListArray, RecordBatch, StringArray, StructArray,
    UInt8Array,
};
use futures::{Future, TryStreamExt};
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use lance::dataset::scanner::Scanner;
use lance_index::scalar::{FullTextSearchQuery, inverted::SCORE_COL};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    num::NonZeroUsize,
    sync::Arc,
};

#[derive(Debug)]
pub enum KnowledgeSearchError {
    Data(KnowledgeIndexError),
    Preview(Box<dyn Error + Send + Sync>),
    Capture(KnowledgeEmbeddingError),
    InvalidQuery(&'static str),
    InvalidCursor,
    GenerationChanged,
}
impl From<KnowledgeIndexError> for KnowledgeSearchError {
    fn from(e: KnowledgeIndexError) -> Self {
        Self::Data(e)
    }
}
impl fmt::Display for KnowledgeSearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge search: {self:?}")
    }
}
impl Error for KnowledgeSearchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Data(e) => Some(e),
            Self::Preview(e) => Some(e.as_ref()),
            Self::Capture(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, KnowledgeSearchError>;

/// Closed continuation data. Deserialized input is checked against the live pin and query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSearchCursor {
    pub(crate) binding: [u8; 32],
    pub(crate) semantic_generation: Option<(u64, Option<String>, Option<String>)>,
    pub(crate) uri: String,
    pub(crate) version: u64,
    pub(crate) physical_branch: Option<String>,
    pub(crate) inherited_target: Option<String>,
    pub(crate) offset: usize,
}
#[derive(Clone, Copy)]
pub struct KnowledgeLexicalQuery<'a> {
    pub text: &'a str,
    /// Zero means all matches, delivered in bounded pages.
    pub top_k: usize,
    pub page_size: NonZeroUsize,
    pub cursor: Option<&'a KnowledgeSearchCursor>,
    /// Bind the caller's actual source selection and MCP session, when applicable.
    pub caller_binding: &'a str,
    /// Include selected documents for the Knowledge preview formatter.
    pub preview: bool,
}
#[derive(Clone, Copy)]
pub struct KnowledgeSearchLimits {
    pub read: KnowledgeIndexReadLimits,
    pub max_query_bytes: NonZeroUsize,
    /// Copied UTF-8 payload per page; transport serialization has its own limit.
    pub max_output_bytes: NonZeroUsize,
}
#[derive(Debug)]
pub struct KnowledgeLexicalHit {
    pub source: KnowledgeReferenceSource,
    pub score: f32,
    pub preview: Option<KnowledgeSearchPreview>,
}
pub(crate) struct MaterializedHit {
    pub source: KnowledgeReferenceSource,
    pub preview: Option<KnowledgeSearchPreview>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeSearchPreviewSource {
    Summary,
    Body,
}
#[derive(Debug)]
pub struct KnowledgeSearchPreview {
    pub text: String,
    pub source: KnowledgeSearchPreviewSource,
    pub range: std::ops::Range<usize>,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeSearchState {
    PublishedEmpty,
    NoHits,
    Hits,
}
#[derive(Debug)]
pub struct KnowledgeSearchPage {
    pub generation: KnowledgeGeneration,
    pub state: KnowledgeSearchState,
    pub hits: Vec<KnowledgeLexicalHit>,
    /// Total matches after top_k, independent of the current page.
    pub total: usize,
    pub next_cursor: Option<KnowledgeSearchCursor>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeVectorSearchMode {
    Semantic,
    Hybrid,
}
#[derive(Clone, Copy)]
pub struct KnowledgeVectorSearchQuery<'a> {
    pub page: KnowledgeLexicalQuery<'a>,
    pub mode: KnowledgeVectorSearchMode,
    /// None is accepted only for a genuinely observed non-Ready corpus.
    pub vector: Option<&'a [f32]>,
    pub metric: crate::knowledge_embeddings::KnowledgeEmbeddingMetric,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeVectorSearchState {
    PublishedEmpty,
    Unavailable,
    Partial,
    NoHits,
    Hits,
}
#[derive(Debug)]
pub struct KnowledgeVectorSearchHit {
    pub source: KnowledgeReferenceSource,
    pub lexical_score: Option<f32>,
    pub semantic_distance: Option<f32>,
    pub rrf_score: Option<f64>,
    pub preview: Option<KnowledgeSearchPreview>,
}
#[derive(Debug)]
pub struct KnowledgeVectorSearchPage {
    pub generation: KnowledgeGeneration,
    pub semantic_generation: Option<crate::knowledge_embeddings::KnowledgeEmbeddingGeneration>,
    pub readiness: crate::knowledge_embeddings::KnowledgeEmbeddingSearchReadiness,
    pub state: KnowledgeVectorSearchState,
    pub hits: Vec<KnowledgeVectorSearchHit>,
    pub total: usize,
    pub next_cursor: Option<KnowledgeSearchCursor>,
}
pub use crate::knowledge_embeddings::search::{
    query_native_knowledge_embedding_source_search, query_native_knowledge_vector_search,
};

/// Source publication and selection are distinct observations, never authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeSelectedDocumentsState {
    PublishedEmpty,
    EmptySelection,
    Selected,
}
#[derive(Debug)]
pub struct KnowledgeSelectedDocuments {
    pub generation: KnowledgeGeneration,
    pub state: KnowledgeSelectedDocumentsState,
    pub documents: Vec<KnowledgeDocument>,
}

pub struct KnowledgeSearchCapture {
    source: LexicalSourceCapture,
}
pub enum KnowledgeSearchRead {
    NoCorpus,
    Published(Box<KnowledgeSearchCapture>),
}
impl KnowledgeSearchCapture {
    /// Revalidate authority and this retained Source pin after external awaits.
    /// This neither consumes documents nor captures a newer generation.
    pub fn recheck(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<()> {
        self.source.check(original, read)
    }

    pub fn generation(&self) -> &KnowledgeGeneration {
        &self.source.generation
    }

    /// Validate an export destination against this retained dataset before staging.
    pub fn validate_export_destination(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        destination: &std::path::Path,
    ) -> Result<()> {
        self.source
            .validate_export_destination(original, read, destination)
    }

    /// Stream owned document batches from this same Source pin without opening NN.
    /// Some(empty) contains validated Removed rows; only None certifies EOF.
    /// Abandoning or failing a read invalidates further continuation.
    pub async fn next_documents(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<Option<Vec<crate::knowledge_index::KnowledgeDocument>>> {
        self.source.next_documents(original, read).await
    }

    /// Validate the selected stream to EOF on this retained Source pin.
    pub async fn read_selected_documents(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        slugs: &[&str],
    ) -> Result<KnowledgeSelectedDocuments> {
        let source = &self.source;
        source.check(original, read)?;
        read_selected_validated(
            original,
            source.scan(),
            &source.generation,
            &source.slugs,
            Some(source.project(read)?),
            slugs,
            source.limits,
            || source.check(original, read),
        )
        .await
    }
}
/// Ordinary path validation shared by the admitted Native and Imported readers.
/// This performs no writes and does not grant access to either path.
pub fn validate_knowledge_export_destination_path(
    captured_root: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    use std::{fs, io};
    if !destination.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "export destination must be absolute",
        ));
    }
    let name = destination.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let parent = fs::canonicalize(destination.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    let resolved = parent.join(name);
    let root = fs::canonicalize(captured_root)?;
    let inside = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "export destination is inside captured Knowledge",
        )
    };
    if resolved.starts_with(&root) {
        return Err(inside());
    }
    match fs::canonicalize(&resolved) {
        Ok(existing) if existing.starts_with(&root) => return Err(inside()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(())
}
/// Shared selected-body read; the caller supplies its retained pin and live guards.
pub(crate) async fn read_selected_validated(
    original: &SnapshotContext,
    mut scanner: Scanner,
    generation: &KnowledgeGeneration,
    active: &BTreeSet<String>,
    expected_project: Option<ProjectId>,
    slugs: &[&str],
    limits: KnowledgeIndexReadLimits,
    mut check: impl FnMut() -> Result<()>,
) -> Result<KnowledgeSelectedDocuments> {
    check()?;
    let mut requested = BTreeSet::new();
    for slug in slugs {
        original.check().map_err(KnowledgeIndexError::from)?;
        index::validate_slug(slug)
            .map_err(|_| KnowledgeSearchError::InvalidQuery("selected_slugs"))?;
        if slug.len() > limits.content.max_identifier_bytes.get() {
            return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes").into());
        }
        if !requested.insert(*slug) {
            return Err(KnowledgeSearchError::InvalidQuery("selected_slugs"));
        }
    }
    if !active.is_empty() && requested.iter().any(|slug| !active.contains(*slug)) {
        return Err(KnowledgeSearchError::InvalidQuery("selected_slugs"));
    }
    scanner
        .filter_expr(
            lance::deps::datafusion::logical_expr::col("slug").in_list(
                requested
                    .iter()
                    .filter(|_| !active.is_empty())
                    .map(|slug| lance::deps::datafusion::logical_expr::lit(*slug))
                    .collect(),
                false,
            ),
        )
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = guarded(original, scanner.try_into_stream(), &mut check)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    let (_, mut documents) = crate::knowledge_snapshot::collect_matching_bound(
        original,
        &mut stream,
        |slug| requested.contains(slug),
        limits,
        expected_project,
        || check().map_err(|cause| KnowledgeIndexError::Request(std::io::Error::other(cause))),
    )
    .await?;
    drop(stream);
    drop(scanner);
    if !active.is_empty() && documents.len() != requested.len() {
        return Err(KnowledgeIndexError::InvalidField("selected_documents").into());
    }
    documents.sort_unstable_by(|a, b| a.slug.cmp(&b.slug));
    check()?;
    Ok(KnowledgeSelectedDocuments {
        generation: generation.clone(),
        state: if active.is_empty() {
            KnowledgeSelectedDocumentsState::PublishedEmpty
        } else if requested.is_empty() {
            KnowledgeSelectedDocumentsState::EmptySelection
        } else {
            KnowledgeSelectedDocumentsState::Selected
        },
        documents,
    })
}

pub async fn capture_native_knowledge_search(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeSearchRead> {
    Ok(
        match LexicalSourceCapture::capture(original, read, limits).await? {
            Some(source) => {
                KnowledgeSearchRead::Published(Box::new(KnowledgeSearchCapture { source }))
            }
            None => KnowledgeSearchRead::NoCorpus,
        },
    )
}
pub async fn query_native_knowledge_search(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    capture: &KnowledgeSearchCapture,
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
    preview: impl FnMut(&str, &str, usize) -> Result<KnowledgeSearchPreview>,
) -> Result<KnowledgeSearchPage> {
    let source = &capture.source;
    source.check(original, read)?;
    // The capture validated with these limits; tighter per-query limits cannot
    // silently claim a validation that happened under a different policy.
    if limits.read.content.max_identifier_bytes != source.limits.content.max_identifier_bytes
        || limits.read.batch_rows != source.limits.batch_rows
    {
        return Err(KnowledgeSearchError::InvalidQuery("capture_limits"));
    }
    let expected_project = Some(source.project(read)?);
    search_validated(
        original,
        || source.scan(),
        &source.generation,
        &source.slugs,
        expected_project,
        query,
        limits,
        None,
        None,
        || source.check(original, read),
        preview,
    )
    .await
}

/// Query an actual caller-owned snapshot. Validates this standalone reader once.
/// No path is opened and no latest version or authorization is manufactured.
pub async fn search_knowledge_snapshot_guarded(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    generation: &KnowledgeGeneration,
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
    mut before: impl FnMut() -> std::result::Result<(), KnowledgeIndexError>,
    preview: impl FnMut(&str, &str, usize) -> Result<KnowledgeSearchPreview>,
) -> Result<KnowledgeSearchPage> {
    let mut check = || {
        original.check().map_err(KnowledgeIndexError::from)?;
        before()?;
        original.check().map_err(KnowledgeIndexError::from)?;
        Ok(())
    };
    check()?;
    input(query, limits)?;
    if snapshot.corpus_uri() != generation.corpus_uri || snapshot.version() != generation.version {
        return Err(KnowledgeSearchError::GenerationChanged);
    }
    crate::knowledge_index::semantic::dimensions(snapshot.schema())?;
    check_cursor(
        query,
        limits,
        generation,
        query_binding(query, limits),
        &None,
    )?;
    let slugs =
        validate_membership(original, snapshot.scan(), limits.read, None, &mut check).await?;
    search_validated(
        original,
        || snapshot.scan(),
        generation,
        &slugs,
        None,
        query,
        limits,
        None,
        None,
        check,
        preview,
    )
    .await
}

pub(crate) fn input(query: KnowledgeLexicalQuery<'_>, limits: KnowledgeSearchLimits) -> Result<()> {
    if query.text.trim().is_empty() {
        return Err(KnowledgeSearchError::InvalidQuery("text"));
    }
    if query.text.len() > limits.max_query_bytes.get()
        || query.caller_binding.len() > limits.max_query_bytes.get()
    {
        return Err(KnowledgeSearchError::InvalidQuery("query_bytes"));
    }
    if query.page_size.get() > 100 {
        return Err(KnowledgeSearchError::InvalidQuery("page_size"));
    }
    Ok(())
}
pub(crate) fn query_binding(
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    // Explicit lengths keep arbitrary literal text unambiguous.
    for bytes in [
        b"knowledge-fts/title-body/v1".as_slice(),
        query.text.as_bytes(),
        query.caller_binding.as_bytes(),
    ] {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    for value in [
        query.top_k,
        query.page_size.get(),
        limits.max_query_bytes.get(),
        limits.max_output_bytes.get(),
    ] {
        hash.update((value as u64).to_le_bytes());
    }
    hash.update([u8::from(query.preview)]);
    hash.update((limits.read.content.max_identifier_bytes.get() as u64).to_le_bytes());
    hash.update(limits.read.batch_rows.get().to_le_bytes());
    hash.finalize().into()
}
fn check_cursor(
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
    generation: &KnowledgeGeneration,
    binding: [u8; 32],
    semantic_generation: &Option<(u64, Option<String>, Option<String>)>,
) -> Result<usize> {
    input(query, limits)?;
    let Some(cursor) = query.cursor else {
        return Ok(0);
    };
    if cursor.binding != binding {
        return Err(KnowledgeSearchError::InvalidCursor);
    }
    if &cursor.semantic_generation != semantic_generation
        || cursor.uri != generation.corpus_uri
        || cursor.version != generation.version
        || cursor.physical_branch != generation.physical_branch
        || cursor.inherited_target != generation.inherited_target
    {
        return Err(KnowledgeSearchError::GenerationChanged);
    }
    if cursor.offset == 0 {
        return Err(KnowledgeSearchError::InvalidCursor);
    }
    Ok(cursor.offset)
}
pub(crate) async fn guarded<T>(
    original: &SnapshotContext,
    future: impl Future<Output = T>,
    mut check: impl FnMut() -> Result<()>,
) -> Result<T> {
    original.check().map_err(KnowledgeIndexError::from)?;
    check()?;
    let deadline = async {
        match original.deadline() {
            Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(future);
    let value = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => return Err(KnowledgeIndexError::from(std::io::Error::from(std::io::ErrorKind::Interrupted)).into()),
        () = deadline => return Err(KnowledgeIndexError::from(std::io::Error::from(std::io::ErrorKind::TimedOut)).into()),
        value = &mut future => value,
    };
    original.check().map_err(KnowledgeIndexError::from)?;
    check()?;
    Ok(value)
}
pub(crate) async fn validate_membership(
    original: &SnapshotContext,
    scanner: Scanner,
    limits: KnowledgeIndexReadLimits,
    expected: Option<ProjectId>,
    check: impl FnMut() -> Result<()>,
) -> Result<BTreeSet<String>> {
    validate_membership_counted(original, scanner, limits, expected, check)
        .await
        .map(|(active, _)| active)
}
pub(crate) async fn validate_membership_counted(
    original: &SnapshotContext,
    mut scanner: Scanner,
    limits: KnowledgeIndexReadLimits,
    expected: Option<ProjectId>,
    mut check: impl FnMut() -> Result<()>,
) -> Result<(BTreeSet<String>, usize)> {
    // Lexical membership needs the document columns, never the potentially
    // large embedding column of the unified dataset.
    scanner
        .project(&index::semantic::DOCUMENT_COLUMNS)
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    scanner.batch_size(limits.batch_rows.get() as usize);
    let mut stream = guarded(original, scanner.try_into_stream(), &mut check)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    let mut all = BTreeSet::new();
    let mut active = BTreeSet::new();
    let mut counts = index::NamespaceValidation::bound(expected);
    loop {
        let next = guarded(original, stream.try_next(), &mut check)
            .await?
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
        let Some(batch) = next else { break };
        // The batch is owned RAM. Keep request cancellation per row/reference;
        // revalidate live authority and physical observations before consuming it.
        let decoded = (|| -> Result<()> {
            index::validate_full_batch(&batch, limits.content, &mut counts, || {
                original.check().map_err(KnowledgeIndexError::from)
            })?;
            let slugs: &StringArray = index::array(&batch, 0)?;
            let states: &UInt8Array = index::array(&batch, 16)?;
            for row in 0..batch.num_rows() {
                original.check().map_err(KnowledgeIndexError::from)?;
                if !all.insert(slugs.value(row).to_owned()) {
                    return Err(KnowledgeIndexError::DuplicateSlug.into());
                }
                if states.value(row) == KnowledgeDocumentState::Active as u8 {
                    active.insert(slugs.value(row).to_owned());
                }
            }
            Ok(())
        })();
        check()?;
        decoded?;
        tokio::task::yield_now().await;
    }
    check()?;
    Ok((active, all.len()))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn search_validated(
    original: &SnapshotContext,
    scan: impl Fn() -> Scanner,
    generation: &KnowledgeGeneration,
    slugs: &BTreeSet<String>,
    expected: Option<ProjectId>,
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
    binding: Option<[u8; 32]>,
    semantic_generation: Option<(u64, Option<String>, Option<String>)>,
    mut check: impl FnMut() -> Result<()>,
    preview: impl FnMut(&str, &str, usize) -> Result<KnowledgeSearchPreview>,
) -> Result<KnowledgeSearchPage> {
    check()?;
    let binding = binding.unwrap_or_else(|| query_binding(query, limits));
    let offset = check_cursor(query, limits, generation, binding, &semantic_generation)?;
    let mut ranked = rank_lexical(original, &scan, slugs, query.text, limits, &mut check).await?;
    if query.top_k > 0 {
        ranked.truncate(query.top_k);
    }
    materialize_page(
        original,
        scan,
        generation,
        slugs,
        expected,
        query,
        limits,
        offset,
        ranked,
        binding,
        semantic_generation,
        &mut check,
        preview,
    )
    .await
}

pub(crate) async fn rank_lexical(
    original: &SnapshotContext,
    scan: &impl Fn() -> Scanner,
    slugs: &BTreeSet<String>,
    text: &str,
    limits: KnowledgeSearchLimits,
    mut check: impl FnMut() -> Result<()>,
) -> Result<Vec<(String, f32)>> {
    let columns = ["title".to_owned(), "body".to_owned()];
    let fts = FullTextSearchQuery::new(text.to_owned())
        .with_columns(&columns)
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?
        .limit(None);
    let mut scanner = scan();
    scanner
        .project(&["slug", SCORE_COL])
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?
        .full_text_search(fts)
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?
        .batch_size(limits.read.batch_rows.get() as usize);
    scanner.prefilter(true);
    scanner.filter_expr(
        lance::deps::datafusion::logical_expr::col("document_state").eq(
            lance::deps::datafusion::logical_expr::lit(KnowledgeDocumentState::Active as u8),
        ),
    );
    let mut stream = guarded(original, scanner.try_into_stream(), &mut check)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    let mut scores = BTreeMap::new();
    loop {
        let next = guarded(original, stream.try_next(), &mut check)
            .await?
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
        let Some(batch) = next else { break };
        let ids = batch
            .column_by_name("slug")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(KnowledgeIndexError::InvalidField("search_slug"))?;
        let values = batch
            .column_by_name(SCORE_COL)
            .and_then(|a| a.as_any().downcast_ref::<Float32Array>())
            .ok_or(KnowledgeIndexError::InvalidField("search_score"))?;
        for row in 0..batch.num_rows() {
            original.check().map_err(KnowledgeIndexError::from)?;
            if ids.is_null(row) || values.is_null(row) || !values.value(row).is_finite() {
                return Err(KnowledgeIndexError::InvalidField("search_hit").into());
            }
            let slug = ids.value(row);
            if !slugs.contains(slug) {
                return Err(KnowledgeIndexError::InvalidField("search_membership").into());
            }
            if scores.insert(slug.to_owned(), values.value(row)).is_some() {
                return Err(KnowledgeIndexError::DuplicateSlug.into());
            }
        }
        tokio::task::yield_now().await;
    }
    drop(stream);
    drop(scanner);
    check()?;
    let mut ranked: Vec<_> = scores.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(ranked)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn materialize_page(
    original: &SnapshotContext,
    scan: impl Fn() -> Scanner,
    generation: &KnowledgeGeneration,
    slugs: &BTreeSet<String>,
    expected: Option<ProjectId>,
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
    offset: usize,
    ranked: Vec<(String, f32)>,
    binding: [u8; 32],
    semantic_generation: Option<(u64, Option<String>, Option<String>)>,
    mut check: impl FnMut() -> Result<()>,
    mut render_preview: impl FnMut(&str, &str, usize) -> Result<KnowledgeSearchPreview>,
) -> Result<KnowledgeSearchPage> {
    let total = ranked.len();
    if offset > total {
        return Err(KnowledgeSearchError::InvalidCursor);
    }
    let end = offset.saturating_add(query.page_size.get()).min(total);
    let selected: Vec<_> = ranked[offset..end]
        .iter()
        .map(|(slug, _)| slug.clone())
        .collect();
    let materialized = materialize_hits(
        original,
        scan,
        expected,
        &selected,
        query.preview,
        limits,
        &mut check,
        &mut render_preview,
    )
    .await?;
    let hits = materialized
        .into_iter()
        .zip(&ranked[offset..end])
        .map(|(hit, (_, score))| KnowledgeLexicalHit {
            source: hit.source,
            score: *score,
            preview: hit.preview,
        })
        .collect();
    let next_cursor = (end < total).then(|| KnowledgeSearchCursor {
        binding,
        semantic_generation,
        uri: generation.corpus_uri.clone(),
        version: generation.version,
        physical_branch: generation.physical_branch.clone(),
        inherited_target: generation.inherited_target.clone(),
        offset: end,
    });
    Ok(KnowledgeSearchPage {
        generation: generation.clone(),
        state: if slugs.is_empty() {
            KnowledgeSearchState::PublishedEmpty
        } else if total == 0 {
            KnowledgeSearchState::NoHits
        } else {
            KnowledgeSearchState::Hits
        },
        hits,
        total,
        next_cursor,
    })
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn materialize_hits(
    original: &SnapshotContext,
    scan: impl Fn() -> Scanner,
    expected: Option<ProjectId>,
    selected: &[String],
    preview: bool,
    limits: KnowledgeSearchLimits,
    mut check: impl FnMut() -> Result<()>,
    mut render_preview: impl FnMut(&str, &str, usize) -> Result<KnowledgeSearchPreview>,
) -> Result<Vec<MaterializedHit>> {
    check()?;
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let requested: BTreeSet<_> = selected.iter().map(String::as_str).collect();
    if requested.len() != selected.len() {
        return Err(KnowledgeIndexError::DuplicateSlug.into());
    }
    let mut scanner = scan();
    scanner
        .filter_expr(
            lance::deps::datafusion::logical_expr::col("slug").in_list(
                requested
                    .iter()
                    .map(|slug| lance::deps::datafusion::logical_expr::lit(*slug))
                    .collect(),
                false,
            ),
        )
        .batch_size(limits.read.batch_rows.get() as usize);
    let mut stream = guarded(original, scanner.try_into_stream(), &mut check)
        .await?
        .map_err(StoreError::from)
        .map_err(KnowledgeIndexError::from)?;
    // Preview keeps Arrow slices only for this result page; without preview,
    // retain metadata only. Ranking can differ from physical row order: render
    // below in caller order so each formatter sees the same remaining budget.
    let mut rows = BTreeMap::new();
    let mut namespaces = index::NamespaceValidation::bound(expected);
    loop {
        let next = guarded(original, stream.try_next(), &mut check)
            .await?
            .map_err(StoreError::from)
            .map_err(KnowledgeIndexError::from)?;
        let Some(batch) = next else { break };
        index::validate_full_batch(&batch, limits.read.content, &mut namespaces, || {
            original.check().map_err(KnowledgeIndexError::from)
        })?;
        let slugs: &StringArray = index::array(&batch, 0)?;
        for row in 0..batch.num_rows() {
            original.check().map_err(KnowledgeIndexError::from)?;
            let slug = slugs.value(row);
            if !requested.contains(slug) {
                return Err(KnowledgeIndexError::InvalidField("search_membership").into());
            }
            let one = batch.slice(row, 1);
            let source = source(&one, expected, limits.read, || {
                original.check().map_err(KnowledgeIndexError::from)
            })?;
            if rows
                .insert(slug.to_owned(), (source, preview.then_some(one)))
                .is_some()
            {
                return Err(KnowledgeIndexError::DuplicateSlug.into());
            }
        }
    }
    drop(stream);
    drop(scanner);
    check()?;
    if rows.len() != selected.len() {
        return Err(KnowledgeIndexError::InvalidField("search_membership").into());
    }
    let mut hits = Vec::with_capacity(selected.len());
    let mut bytes = 0usize;
    for slug in selected {
        check()?;
        let (source, one) = rows
            .remove(slug)
            .ok_or(KnowledgeIndexError::InvalidField("search_membership"))?;
        let mut owned_bytes = source.slug.len() + source.title.len() + source.source_path.len();
        if let Some(ns) = &source.namespace {
            owned_bytes += ns.context.as_ref().map_or(0, String::len);
            if let KnowledgeReferenceOwnedScope::User(id) = &ns.scope {
                owned_bytes += id.len();
            }
        }
        let metadata_bytes = bytes
            .checked_add(owned_bytes)
            .ok_or(KnowledgeIndexError::Overflow("search_output_bytes"))?;
        if metadata_bytes > limits.max_output_bytes.get() {
            return Err(KnowledgeIndexError::LimitExceeded("search_output_bytes").into());
        }
        let preview = if let Some(one) = one {
            let body: &StringArray = index::array(&one, 2)?;
            let summary: &StringArray = index::array(&one, 4)?;
            let remaining = limits.max_output_bytes.get() - metadata_bytes;
            let preview = render_preview(body.value(0), summary.value(0), remaining)?;
            original.check().map_err(KnowledgeIndexError::from)?;
            let input = match preview.source {
                KnowledgeSearchPreviewSource::Summary => summary.value(0),
                KnowledgeSearchPreviewSource::Body => body.value(0),
            };
            let selected = input
                .get(preview.range.clone())
                .ok_or(KnowledgeIndexError::InvalidField("search_preview_range"))?;
            let literal = if preview.truncated {
                preview.text.strip_suffix('…')
            } else {
                Some(preview.text.as_str())
            };
            if literal != Some(selected) || preview.text.len() > remaining {
                return Err(KnowledgeIndexError::InvalidField("search_preview_text").into());
            }
            owned_bytes = owned_bytes
                .checked_add(preview.text.len())
                .ok_or(KnowledgeIndexError::Overflow("search_output_bytes"))?;
            Some(preview)
        } else {
            None
        };
        bytes = bytes
            .checked_add(owned_bytes)
            .ok_or(KnowledgeIndexError::Overflow("search_output_bytes"))?;
        if bytes > limits.max_output_bytes.get() {
            return Err(KnowledgeIndexError::LimitExceeded("search_output_bytes").into());
        }
        hits.push(MaterializedHit { source, preview });
    }
    check()?;
    Ok(hits)
}

pub(crate) fn source(
    batch: &RecordBatch,
    expected: Option<ProjectId>,
    limits: KnowledgeIndexReadLimits,
    mut before: impl FnMut() -> std::result::Result<(), KnowledgeIndexError>,
) -> std::result::Result<KnowledgeReferenceSource, KnowledgeIndexError> {
    let slug: &StringArray = index::array(batch, 0)?;
    let title: &StringArray = index::array(batch, 1)?;
    let path: &StringArray = index::array(batch, 7)?;
    let digest: &FixedSizeBinaryArray = index::array(batch, 9)?;
    let wiki: &ListArray = index::array(batch, 15)?;
    let state: &UInt8Array = index::array(batch, 16)?;
    let projection: &StructArray = index::array(batch, 17)?;
    let targets = wiki.value(0);
    let targets = targets
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
    let digest: [u8; 32] = digest
        .value(0)
        .try_into()
        .map_err(|_| KnowledgeIndexError::InvalidField("digest"))?;
    let mut source = KnowledgeReferenceSource {
        slug: slug.value(0).to_owned(),
        title: title.value(0).to_owned(),
        source_path: path.value(0).to_owned(),
        compilation_digest: CompilationDigest32::from_bytes(digest),
        namespace: None,
        witness: None,
    };
    let state = refs::visit_row(
        refs::ReferenceRowInput {
            projection,
            state: state.value(0),
            row: 0,
            header: refs::WitnessHeader {
                slug: slug.value(0),
                title: title.value(0),
                source_path: path.value(0),
                digest: &digest,
            },
            wiki: targets,
            expected_project: expected,
        },
        Some(limits.content.max_identifier_bytes),
        &mut before,
        |fields, witness| {
            source.namespace = Some(KnowledgeReferenceOwnedNamespace {
                scope: match fields.namespace.scope {
                    KnowledgeReferenceScope::Project(id) => {
                        KnowledgeReferenceOwnedScope::Project(id)
                    }
                    KnowledgeReferenceScope::User(id) => {
                        KnowledgeReferenceOwnedScope::User(id.to_owned())
                    }
                },
                context: fields.namespace.context.map(str::to_owned),
            });
            source.witness = Some(witness);
            Ok(())
        },
    )?;
    if state != KnowledgeDocumentState::Active {
        return Err(KnowledgeIndexError::InvalidField("search_state"));
    }
    Ok(source)
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    #[tokio::test]
    async fn selected_page_reads_once_and_renders_in_ranking_order()
    -> std::result::Result<(), Box<dyn Error>> {
        use crate::{knowledge_document_projection as h, lance_module_store::LanceModuleStore};
        use graphit_core::native_document_file::NativeDocumentFileLimits;
        let n = |value| NonZeroUsize::new(value).unwrap();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            Cancellation::default(),
        );
        let fields: Vec<_> = ["alpha", "beta", "gamma"]
            .iter()
            .map(|slug| h::AuthoredPageFields {
                slug: Some(slug),
                title: Some(slug),
                body: Some(slug),
                kind: Some("page"),
                summary: Some(""),
                description: None,
                summary_truncated: false,
                source_path: "docs/page.md",
                physical_digest: None,
                compilation_digest: CompilationDigest32::from_bytes([1; 32]),
                recipe: h::RecipeFields {
                    id: h::AUTHORED_RECIPE_ID,
                    parameters: Some(h::AuthoredParameters {
                        fallback_title: "page",
                        kind_on_absent: "page",
                        max_summary_bytes: n(16),
                    }),
                },
                updated_at: None,
                stale_since: None,
                stale_after: None,
                resolved_wiki_targets: &[],
            })
            .collect();
        let content = index::KnowledgeIndexContentLimits {
            max_identifier_bytes: n(256),
        };
        let rows = h::prepare(
            &original,
            &fields,
            &[],
            h::KnowledgeDocumentProjectionLimits {
                max_documents: n(8),
                max_observations: n(8),
                max_rows: n(64),
                max_field_bytes: n(65536),
                max_text_bytes: n(131072),
            },
        )?;
        let batch = index::encode_with_references(&rows, None, &original, content, || Ok(()))?;
        let directory = tempfile::tempdir()?;
        let uri = directory.path().join("documents.lance");
        let store =
            LanceModuleStore::create(uri.to_str().unwrap(), batch.schema(), vec![batch]).await?;
        let snapshot = store.snapshot();
        let calls = AtomicUsize::new(0);
        let selected = vec!["gamma".to_owned(), "alpha".to_owned()];
        let limits = KnowledgeSearchLimits {
            read: KnowledgeIndexReadLimits {
                content,
                native: NativeDocumentFileLimits::new(65536, 4096, 128, 65536)?,
                batch_rows: std::num::NonZeroU32::new(1).unwrap(),
            },
            max_query_bytes: n(4096),
            max_output_bytes: n(65536),
        };
        let mut rendered = Vec::new();
        let hits = materialize_hits(
            &original,
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                snapshot.scan()
            },
            None,
            &selected,
            true,
            limits,
            || Ok(()),
            |body, _, remaining| {
                rendered.push((body.to_owned(), remaining));
                Ok(KnowledgeSearchPreview {
                    text: body.to_owned(),
                    source: KnowledgeSearchPreviewSource::Body,
                    range: 0..body.len(),
                    truncated: false,
                })
            },
        )
        .await?;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            hits.iter()
                .map(|hit| hit.source.slug.as_str())
                .collect::<Vec<_>>(),
            ["gamma", "alpha"]
        );
        assert_eq!(
            rendered
                .iter()
                .map(|(body, _)| body.as_str())
                .collect::<Vec<_>>(),
            ["gamma", "alpha"]
        );
        assert_eq!(
            rendered[0].1 - rendered[1].1,
            "gamma".len() + 2 * "alpha".len() + "docs/page.md".len()
        );
        let without_preview = materialize_hits(
            &original,
            || snapshot.scan(),
            None,
            &selected,
            false,
            limits,
            || Ok(()),
            |_, _, _| panic!("preview disabled"),
        )
        .await?;
        assert!(without_preview.iter().all(|hit| hit.preview.is_none()));
        let missing = materialize_hits(
            &original,
            || snapshot.scan(),
            None,
            &["missing".to_owned()],
            false,
            limits,
            || Ok(()),
            |_, _, _| panic!("preview disabled"),
        )
        .await;
        assert!(matches!(
            missing,
            Err(KnowledgeSearchError::Data(
                KnowledgeIndexError::InvalidField("search_membership")
            ))
        ));
        Ok(())
    }

    #[tokio::test]
    async fn pending_read_checks_authority_only_at_io_boundaries() {
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        );
        let checks = AtomicUsize::new(0);
        let started = tokio::sync::Notify::new();
        let finish = tokio::sync::Notify::new();
        let read = guarded(
            &original,
            async {
                started.notify_one();
                finish.notified().await;
                7
            },
            || {
                checks.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        let release = async {
            started.notified().await;
            tokio::time::sleep(Duration::from_millis(60)).await;
            assert_eq!(checks.load(Ordering::SeqCst), 1);
            finish.notify_one();
        };
        let (result, ()) = tokio::join!(read, release);
        assert_eq!(result.unwrap(), 7);
        assert_eq!(checks.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cancellation_and_deadline_stop_pending_reads() {
        let cancel = Cancellation::default();
        let original =
            SnapshotContext::new(Instant::now() + Duration::from_secs(5), cancel.clone());
        let started = tokio::sync::Notify::new();
        let read = guarded(
            &original,
            async {
                started.notify_one();
                std::future::pending::<()>().await
            },
            || Ok(()),
        );
        let stop = async {
            started.notified().await;
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(read, stop);
        assert!(
            matches!(result, Err(KnowledgeSearchError::Data(KnowledgeIndexError::Request(error))) if error.kind() == std::io::ErrorKind::Interrupted)
        );
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_millis(120),
            Cancellation::default(),
        );
        let result = guarded(&original, std::future::pending::<()>(), || Ok(())).await;
        assert!(
            matches!(result, Err(KnowledgeSearchError::Data(KnowledgeIndexError::Request(error))) if error.kind() == std::io::ErrorKind::TimedOut)
        );
    }

    #[tokio::test]
    async fn completed_read_still_requires_the_final_authority_check() {
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        );
        let mut checks = 0;
        let result = guarded(&original, std::future::ready(7), || {
            checks += 1;
            if checks == 2 {
                Err(KnowledgeSearchError::GenerationChanged)
            } else {
                Ok(())
            }
        })
        .await;
        assert!(matches!(
            result,
            Err(KnowledgeSearchError::GenerationChanged)
        ));
        assert_eq!(checks, 2);
    }
}
