//! Lexical query inputs and borrowed wire views over the Knowledge search bridge.
use super::*;
use graphit_data::knowledge_index::KnowledgeGeneration;
use graphit_data::knowledge_search::{
    KnowledgeLexicalHit, KnowledgeLexicalQuery, KnowledgeSearchCursor, KnowledgeSearchError,
    KnowledgeSearchLimits, KnowledgeSearchPage, KnowledgeSearchPreview, KnowledgeSearchRead,
    KnowledgeSearchState, KnowledgeVectorSearchHit, KnowledgeVectorSearchPage,
};
use graphit_knowledge::preview::MarkdownPreviewLimits;

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    Fts,
    Semantic,
    Hybrid,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Metric {
    #[default]
    Cosine,
    L2,
    Dot,
}
impl Metric {
    pub(crate) fn native(self) -> graphit_data::knowledge_embeddings::KnowledgeEmbeddingMetric {
        match self {
            Self::Cosine => graphit_data::knowledge_embeddings::KnowledgeEmbeddingMetric::Cosine,
            Self::L2 => graphit_data::knowledge_embeddings::KnowledgeEmbeddingMetric::L2,
            Self::Dot => graphit_data::knowledge_embeddings::KnowledgeEmbeddingMetric::Dot,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Input {
    pub(crate) query: String,
    pub(crate) mode: Mode,
    #[serde(default)]
    pub(crate) metric: Metric,
    #[serde(default)]
    pub(crate) top_k: usize,
    #[serde(default = "page_size")]
    pub(crate) page_size: usize,
    #[serde(default)]
    pub(crate) preview: bool,
    #[serde(
        default,
        deserialize_with = "crate::reference_explorer::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) rerank: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::reference_explorer::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) cursor: Option<KnowledgeSearchCursor>,
}
fn page_size() -> usize {
    20
}
impl Input {
    pub(crate) fn validate(&self, content: ContentLimits) -> Result<(), &'static str> {
        if self.query.trim().is_empty()
            || self.query.contains('\0')
            || self.query.len() > content.max_scalar_bytes
            || !(1..=100).contains(&self.page_size)
        {
            return Err("invalid_arguments");
        }
        Ok(())
    }
    pub(crate) fn native<'a>(&'a self, binding: &'a str) -> KnowledgeLexicalQuery<'a> {
        KnowledgeLexicalQuery {
            text: &self.query,
            top_k: self.top_k,
            page_size: NonZeroUsize::new(self.page_size).expect("validated page size"),
            cursor: self.cursor.as_ref(),
            caller_binding: binding,
            preview: self.preview,
        }
    }
}
pub(crate) fn query_limits(
    read: KnowledgeIndexReadLimits,
    content: ContentLimits,
) -> Result<KnowledgeSearchLimits, KnowledgeIndexError> {
    Ok(KnowledgeSearchLimits {
        read,
        max_query_bytes: NonZeroUsize::new(content.max_scalar_bytes)
            .ok_or(KnowledgeIndexError::InvalidField("search policy"))?,
        max_output_bytes: NonZeroUsize::new(content.max_output_bytes)
            .ok_or(KnowledgeIndexError::InvalidField("search policy"))?,
    })
}
pub(crate) fn preview_limits(
    content: ContentLimits,
) -> Result<MarkdownPreviewLimits, KnowledgeIndexError> {
    Ok(MarkdownPreviewLimits {
        max_input_bytes: NonZeroUsize::new(content.max_input_bytes)
            .ok_or(KnowledgeIndexError::InvalidField("preview policy"))?,
        max_output_bytes: NonZeroUsize::new(content.max_output_bytes)
            .ok_or(KnowledgeIndexError::InvalidField("preview policy"))?,
        // A presentation excerpt; byte bounds remain those of the existing response policy.
        max_body_scalars: NonZeroUsize::new(1000).expect("positive presentation length"),
    })
}

#[derive(Serialize)]
pub(crate) struct PageView<'a> {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    total: usize,
    hits: Hits<'a>,
    next_cursor: Option<&'a KnowledgeSearchCursor>,
}
impl<'a> From<&'a KnowledgeSearchPage> for PageView<'a> {
    fn from(page: &'a KnowledgeSearchPage) -> Self {
        Self {
            state: match page.state {
                KnowledgeSearchState::PublishedEmpty => "published_empty",
                KnowledgeSearchState::NoHits => "no_hits",
                KnowledgeSearchState::Hits => "hits",
            },
            reason: None,
            total: page.total,
            hits: Hits::Lexical(&page.hits, None),
            next_cursor: page.next_cursor.as_ref(),
        }
    }
}
enum Hits<'a> {
    Lexical(&'a [KnowledgeLexicalHit], Option<&'a [usize]>),
    Vector(&'a [KnowledgeVectorSearchHit], Option<&'a [usize]>),
}
#[derive(Serialize)]
struct Hit<'a> {
    slug: &'a str,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<Preview<'a>>,
}
#[derive(Serialize)]
struct Preview<'a> {
    text: &'a str,
    truncated: bool,
}
impl Serialize for Hits<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (length, order) = match self {
            Self::Lexical(hits, order) => (hits.len(), order),
            Self::Vector(hits, order) => (hits.len(), order),
        };
        let mut sequence = serializer.serialize_seq(Some(length))?;
        for index in 0..length {
            let position = order.map_or(index, |order| order[index]);
            let (slug, title, preview) = match self {
                Self::Lexical(hits, _) => {
                    let hit = &hits[position];
                    (&hit.slug, &hit.title, hit.preview.as_ref())
                }
                Self::Vector(hits, _) => {
                    let hit = &hits[position];
                    (&hit.slug, &hit.title, hit.preview.as_ref())
                }
            };
            sequence.serialize_element(&Hit {
                slug,
                title,
                preview: preview.map(|p: &KnowledgeSearchPreview| Preview {
                    text: &p.text,
                    truncated: p.truncated,
                }),
            })?;
        }
        sequence.end()
    }
}
#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum ReadView<'a> {
    Empty { state: &'static str },
    Page(PageView<'a>),
}
impl<'a> ReadView<'a> {
    pub(crate) fn reranked(mut self, scores: &'a crate::search_rerank::RerankScores) -> Self {
        if let Self::Page(page) = &mut self {
            match &mut page.hits {
                Hits::Lexical(_, order) | Hits::Vector(_, order) => *order = Some(&scores.order),
            }
        }
        self
    }
}

pub(crate) fn rerank_texts<'a>(
    selected: &'a graphit_data::knowledge_search::KnowledgeSelectedDocuments,
    generation: &KnowledgeGeneration,
    slugs: &[&str],
) -> Result<Vec<&'a str>, KnowledgeSearchError> {
    if &selected.generation != generation || selected.documents.len() != slugs.len() {
        return Err(KnowledgeSearchError::GenerationChanged);
    }
    let documents: std::collections::BTreeMap<_, _> = selected
        .documents
        .iter()
        .map(|document| (document.slug.as_str(), document.body.as_str()))
        .collect();
    slugs
        .iter()
        .map(|slug| {
            documents
                .get(slug)
                .copied()
                .ok_or(KnowledgeSearchError::GenerationChanged)
        })
        .collect()
}

pub(crate) enum ProjectPageCapture<'a> {
    Lexical(&'a graphit_data::knowledge_search::KnowledgeSearchCapture),
    Vector(&'a graphit_data::knowledge_embeddings::KnowledgeEmbeddingCapture),
}
impl ProjectPageCapture<'_> {
    pub(crate) async fn selected(
        &self,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
        admission: &graphit_auth::ProjectReadAdmission<'_>,
        slugs: &[&str],
    ) -> Result<graphit_data::knowledge_search::KnowledgeSelectedDocuments, KnowledgeSearchError>
    {
        Ok(match self {
            Self::Lexical(capture) => {
                capture
                    .read_selected_documents(original, admission, slugs)
                    .await?
            }
            Self::Vector(capture) => {
                capture
                    .read_selected_documents(original, admission, slugs)
                    .await?
            }
        })
    }
    pub(crate) fn recheck(
        &self,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
        admission: &graphit_auth::ProjectReadAdmission<'_>,
    ) -> Result<(), KnowledgeSearchError> {
        match self {
            Self::Lexical(capture) => capture.recheck(original, admission)?,
            Self::Vector(capture) => capture
                .recheck(original, admission)
                .map_err(graphit_data::knowledge_search::KnowledgeSearchError::Capture)?,
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub(crate) enum ReadIdentity<'a> {
    Web(&'a graphit_auth::broker::oauth::web::AuthenticatedWebCaller<'a>),
    Native(&'a graphit_auth::AuthenticatedCaller<'a>),
}
impl<'a> ReadIdentity<'a> {
    fn service(
        self,
        issuer: &'a graphit_auth::RoleIssuer,
        admission: &'a ProjectReadAdmission<'a>,
        request: &'a graphit_hub::policy::RequestContext<'a>,
        purpose: graphit_auth::ServicePurpose,
    ) -> Result<graphit_auth::ServiceSelection<'a>, graphit_auth::ServiceUseError> {
        match self {
            Self::Native(caller) => issuer.capture_service(caller, purpose),
            Self::Web(caller) => match purpose {
                graphit_auth::ServicePurpose::Embedding => {
                    issuer.capture_web_embedding_service(caller, admission, request)
                }
                graphit_auth::ServicePurpose::Rerank => {
                    issuer.capture_web_rerank_service(caller, admission, request)
                }
                _ => unreachable!("Knowledge search selects embedding or rerank"),
            },
        }
    }
}
pub(crate) struct BrowserSearchContext<'a> {
    pub(crate) server: &'a crate::task_server::TaskServer,
    pub(crate) auth: &'a graphit_daemon::auth_startup::DaemonAuth,
    pub(crate) project: &'a graphit_core::project_paths::ProjectPaths,
    pub(crate) verifiers: Option<&'a graphit_daemon::broker_verifiers::BrokerVerifierCache>,
}
#[allow(clippy::too_many_arguments)]
async fn rerank_browser<'a>(
    context: &BrowserSearchContext<'a>,
    request: &'a graphit_hub::policy::RequestContext<'a>,
    caller: ReadIdentity<'a>,
    admission: &'a ProjectReadAdmission<'a>,
    input: &Input,
    capture: ProjectPageCapture<'_>,
    generation: &KnowledgeGeneration,
    slugs: &[&str],
) -> Result<Option<crate::search_rerank::RerankScores>, Failure> {
    if input.rerank == Some(false) || slugs.is_empty() {
        return Ok(None);
    }
    let (_, config, _) = match context
        .server
        .live_configuration(Some(context.project), request.snapshot())
    {
        Ok(configuration) => configuration,
        // A standalone Task server without a configuration attachment has only
        // the distribution default. Explicit opt-in still requires the owner.
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported && input.rerank.is_none() => {
            return Ok(None);
        }
        Err(error) => {
            return Err(crate::knowledge_semantic::KnowledgeSemanticError::from(error).into());
        }
    };
    let enabled = match input.rerank {
        Some(value) => value,
        None => crate::search_rerank::configured(&config)
            .map_err(crate::knowledge_semantic::KnowledgeSemanticError::from)?,
    };
    if !enabled {
        return Ok(None);
    }
    let mut service = caller
        .service(
            context.auth.issuer(),
            admission,
            request,
            graphit_auth::ServicePurpose::Rerank,
        )
        .map_err(crate::knowledge_semantic::KnowledgeSemanticError::from)?;
    let selected = capture
        .selected(request.snapshot(), admission, slugs)
        .await?;
    let texts = rerank_texts(&selected, generation, slugs)?;
    let control = if service
        .mode()
        .map_err(crate::knowledge_semantic::KnowledgeSemanticError::from)?
        == graphit_auth::ServiceMode::LocalAuthorized
    {
        let slot = context
            .server
            .local_model_control()
            .map_err(crate::knowledge_semantic::KnowledgeSemanticError::from)?;
        slot.read()
            .map_err(|_| {
                crate::knowledge_semantic::KnowledgeSemanticError::Request(std::io::Error::other(
                    "model control observation failed",
                ))
            })?
            .clone()
    } else {
        None
    };
    let policy = graphit_hub::policy::HubResourcePolicy::resolve(&config).map_err(|error| {
        crate::knowledge_semantic::KnowledgeSemanticError::from(std::io::Error::other(error))
    })?;
    let result = crate::search_rerank::run(
        request.snapshot(),
        &mut service,
        control
            .as_ref()
            .map(|control| (control, context.auth.runtime().as_ref())),
        &config,
        &policy,
        context.verifiers,
        &input.query,
        &texts,
    )
    .await;
    capture.recheck(request.snapshot(), admission)?;
    Ok(Some(result?))
}

#[allow(
    clippy::too_many_arguments,
    reason = "caller authority, admitted selection, policy and model context remain explicit"
)]
pub(crate) async fn read<'a>(
    request: &'a graphit_hub::policy::RequestContext<'a>,
    caller: ReadIdentity<'a>,
    admission: &'a ProjectReadAdmission<'a>,
    policy: &TaskPolicy,
    selection: &AdmittedSelection,
    input: &Input,
    resources: Option<&'a crate::foreground_memory_jobs::MemoryJobResources>,
    context: &BrowserSearchContext<'_>,
) -> Result<Vec<u8>, Failure> {
    let original = request.snapshot();
    input
        .validate(policy.content)
        .map_err(KnowledgeSearchError::InvalidQuery)?;
    let limits = query_limits(super::limits(policy)?, policy.content)?;
    let binding = serde_json::to_string(&(selection, input.mode, input.metric))?;
    if input.mode != Mode::Fts {
        let (page, retained) = if let Some(resources) = resources {
            let mut service = caller
                .service(
                    resources.auth.issuer(),
                    admission,
                    request,
                    graphit_auth::ServicePurpose::Embedding,
                )
                .map_err(crate::knowledge_semantic::KnowledgeSemanticError::from)?;
            let result = query_vector_capture(
                resources,
                request,
                admission,
                &mut service,
                policy.timeout,
                crate::knowledge_semantic::KnowledgeSemanticSearchOptions {
                    page: input.native(&binding),
                    mode: input.vector_mode(),
                    metric: input.metric.native(),
                    limits,
                    preview: preview_limits(policy.content)?,
                },
                false,
            )
            .await?;
            match result {
                Some(execution) => (
                    SearchOutcome::Vector(Some(execution.page)),
                    Some(execution.corpus),
                ),
                None => (SearchOutcome::Vector(None), None),
            }
        } else {
            match query_without_model_capture(
                original,
                admission,
                input.native(&binding),
                input.mode,
                limits,
                preview_limits(policy.content)?,
            )
            .await?
            {
                Some((page, capture)) => (SearchOutcome::WithoutModel(Some(page)), Some(capture)),
                None => (SearchOutcome::WithoutModel(None), None),
            }
        };
        let ranks = if let (Some(capture), Some((generation, slugs))) =
            (retained.as_deref(), page.rerank_selection())
        {
            rerank_browser(
                context,
                request,
                caller,
                admission,
                input,
                ProjectPageCapture::Vector(capture),
                generation,
                &slugs,
            )
            .await?
        } else {
            None
        };
        let native = page.view();
        let native = if let Some(ranks) = &ranks {
            native.reranked(ranks)
        } else {
            native
        };
        let bytes = codec::encode_response(
            &ReferenceBinding {
                selection: Some(selection),
                source: &crate::reference_explorer::SourceKey::ProjectKnowledge,
                focused_source: None,
                native,
            },
            policy.content.max_output_bytes,
        )?;
        admission.recheck().map_err(KnowledgeIndexError::from)?;
        original.check().map_err(KnowledgeIndexError::from)?;
        return Ok(bytes);
    }
    let capture = graphit_data::knowledge_search::capture_native_knowledge_search(
        original,
        admission,
        limits.read,
    )
    .await?;
    let page = match &capture {
        KnowledgeSearchRead::NoCorpus => None,
        KnowledgeSearchRead::Published(capture) => Some(
            graphit_knowledge::search::query_native_knowledge_search(
                original,
                admission,
                capture,
                input.native(&binding),
                limits,
                preview_limits(policy.content)?,
            )
            .await?,
        ),
    };
    let ranks = match (&capture, &page) {
        (KnowledgeSearchRead::Published(capture), Some(page)) => {
            let slugs: Vec<_> = page.hits.iter().map(|hit| hit.slug.as_str()).collect();
            rerank_browser(
                context,
                request,
                caller,
                admission,
                input,
                ProjectPageCapture::Lexical(capture),
                &page.generation,
                &slugs,
            )
            .await?
        }
        _ => None,
    };
    let native = page
        .as_ref()
        .map_or(ReadView::Empty { state: "no_corpus" }, |page| {
            ReadView::Page(page.into())
        });
    let native = if let Some(ranks) = &ranks {
        native.reranked(ranks)
    } else {
        native
    };
    let bytes = codec::encode_response(
        &ReferenceBinding {
            selection: Some(selection),
            source: &crate::reference_explorer::SourceKey::ProjectKnowledge,
            focused_source: None,
            native,
        },
        policy.content.max_output_bytes,
    )?;
    admission.recheck().map_err(KnowledgeIndexError::from)?;
    original.check().map_err(KnowledgeIndexError::from)?;
    Ok(bytes)
}

pub(crate) fn response(error: &KnowledgeSearchError) -> (axum::http::StatusCode, &'static str) {
    use axum::http::StatusCode as S;
    match error {
        KnowledgeSearchError::InvalidCursor => (S::BAD_REQUEST, "invalid_knowledge_cursor"),
        KnowledgeSearchError::InvalidQuery(_) => (S::BAD_REQUEST, "invalid_search_query"),
        KnowledgeSearchError::GenerationChanged => (S::CONFLICT, "knowledge_generation_changed"),
        KnowledgeSearchError::Data(error) => {
            crate::reference_explorer::Failure::knowledge_response(error)
        }
        KnowledgeSearchError::Preview(error) => {
            if let Some(error) =
                error.downcast_ref::<graphit_knowledge::preview::MarkdownPreviewError>()
            {
                match error.kind() {
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut => {
                        (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                    }
                    std::io::ErrorKind::FileTooLarge => (S::PAYLOAD_TOO_LARGE, "preview_limit"),
                    _ => (S::INTERNAL_SERVER_ERROR, "preview_failure"),
                }
            } else {
                (S::INTERNAL_SERVER_ERROR, "preview_failure")
            }
        }
        KnowledgeSearchError::Capture(error) => match error {
            graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Index(error) => {
                crate::reference_explorer::Failure::knowledge_response(error)
            }
            graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Io(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
                ) =>
            {
                (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
            }
            graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Conflict => {
                (S::CONFLICT, "knowledge_generation_changed")
            }
            _ => {
                eprintln!(
                    "Graphit Knowledge search capture failed: {}",
                    capture_failure_diagnostic(error)
                );
                (S::INTERNAL_SERVER_ERROR, "knowledge_capture_failure")
            }
        },
    }
}

/// Never format error messages: they can contain input, paths or credentials.
pub(crate) fn capture_failure_diagnostic(
    error: &graphit_data::knowledge_embeddings::KnowledgeEmbeddingError,
) -> String {
    use graphit_data::knowledge_embeddings::KnowledgeEmbeddingError as E;
    let class = embedding_failure_class(error);
    match error {
        E::InvalidStored(field) | E::InvalidInput(field) => format!("{class}:{field}"),
        E::Io(error) => format!("{class}:{:?}", error.kind()),
        _ => class.into(),
    }
}

#[cfg(test)]
mod capture_diagnostic_tests {
    use super::*;
    use graphit_data::knowledge_embeddings::KnowledgeEmbeddingError;

    #[test]
    fn stored_validation_reason_keeps_capture_response_contract() {
        let cause = KnowledgeEmbeddingError::InvalidStored("dimensions");
        assert_eq!(
            capture_failure_diagnostic(&cause),
            "embedding_dimensions:dimensions"
        );
        assert_eq!(
            response(&KnowledgeSearchError::Capture(cause)),
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "knowledge_capture_failure"
            )
        );
    }

    #[test]
    fn io_diagnostic_exposes_kind_without_dynamic_messages() {
        let private = "PRIVATE_QUERY /private/path authorization=Bearer private-placeholder";
        let cause = KnowledgeEmbeddingError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            private,
        ));
        let diagnostic = capture_failure_diagnostic(&cause);
        assert_eq!(diagnostic, "embedding_io:PermissionDenied");
        for sensitive in ["PRIVATE_QUERY", "/private/path", "authorization", "Bearer"] {
            assert!(!diagnostic.contains(sensitive));
        }
    }
}

impl Input {
    pub(crate) fn vector_mode(&self) -> graphit_data::knowledge_search::KnowledgeVectorSearchMode {
        match self.mode {
            Mode::Semantic => graphit_data::knowledge_search::KnowledgeVectorSearchMode::Semantic,
            Mode::Hybrid => graphit_data::knowledge_search::KnowledgeVectorSearchMode::Hybrid,
            Mode::Fts => unreachable!("lexical query does not request vector ranking"),
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn query_vector_capture(
    resources: &crate::foreground_memory_jobs::MemoryJobResources,
    request: &graphit_hub::policy::RequestContext<'_>,
    read: &ProjectReadAdmission<'_>,
    service: &mut graphit_auth::ServiceSelection<'_>,
    rpc_timeout: std::time::Duration,
    options: crate::knowledge_semantic::KnowledgeSemanticSearchOptions<'_>,
    select_pages: bool,
) -> Result<
    Option<crate::knowledge_semantic::KnowledgeSearchExecution>,
    crate::knowledge_semantic::KnowledgeSemanticError,
> {
    let original = request.snapshot();
    let execution = resources.execution();
    let metadata = crate::embedding_service::metadata(service, &execution).await?;
    let metadata = &metadata;
    crate::knowledge_semantic::query_knowledge_search_capture(
        original,
        read,
        &crate::knowledge_semantic::KnowledgeSemanticModel {
            metadata,
            batch_limits: resources.rpc_limits(),
            rpc_timeout,
        },
        options,
        move |operation| async move {
            resources
                .embed(service, metadata, operation)
                .await
                .map(|output| output.vectors)
        },
        select_pages,
    )
    .await
}
impl<'a> From<&'a KnowledgeVectorSearchPage> for PageView<'a> {
    fn from(page: &'a KnowledgeVectorSearchPage) -> Self {
        use graphit_data::{
            knowledge_embeddings::{
                KnowledgeEmbeddingSearchReadiness as R, KnowledgeEmbeddingUnavailable as U,
            },
            knowledge_search::KnowledgeVectorSearchState as S,
        };
        Self {
            state: match page.state {
                S::PublishedEmpty => "published_empty",
                S::Unavailable => "unavailable",
                S::Partial => "partial",
                S::NoHits => "no_hits",
                S::Hits => "hits",
            },
            reason: match &page.readiness {
                R::Ready | R::PublishedEmpty => None,
                R::Unavailable(reason) => Some(match reason {
                    U::MissingSemantic => "missing_semantic",
                    U::Incomplete => "incomplete",
                    U::IncompatibleIdentity => "incompatible_identity",
                    U::IncompatibleDimensions => "incompatible_dimensions",
                }),
            },
            total: page.total,
            hits: Hits::Vector(&page.hits, None),
            next_cursor: page.next_cursor.as_ref(),
        }
    }
}
pub(crate) fn semantic_response(
    error: &crate::knowledge_semantic::KnowledgeSemanticError,
) -> (axum::http::StatusCode, &'static str) {
    use crate::knowledge_semantic::KnowledgeSemanticError as E;
    use axum::http::StatusCode as S;
    let response = match error {
        E::Search(error) => response(error),
        E::Request(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
            ) =>
        {
            (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
        }
        E::Auth(error) => inference_response(error),
        E::Service(error) => inference_response(error),
        E::InvalidAdmission | E::ForeignRequest => (S::FORBIDDEN, "model_access_denied"),
        E::Models(error) => inference_response(error),
        E::Inference(error) => inference_response(error.as_ref()),
        E::InvalidInferenceOutput => (S::BAD_GATEWAY, "embedding_invalid_output"),
        E::InvalidInferencePolicy => (S::BAD_REQUEST, "embedding_invalid_policy"),
        E::Data(error) => embedding_response(error),
        E::Imported(error) => {
            use graphit_knowledge::imported_knowledge::ImportedKnowledgeSemanticError as I;
            match error.as_ref() {
                I::Reader(error) => crate::reference_explorer::Failure::imported_response(error),
                I::Embedding(error) => embedding_response(error),
                I::Query(error) => response(error),
                I::Projection(_) => (S::INTERNAL_SERVER_ERROR, "knowledge_projection_failure"),
            }
        }
        _ => (S::INTERNAL_SERVER_ERROR, "knowledge_semantic_failure"),
    };
    if response.1 == "knowledge_semantic_failure" {
        eprintln!(
            "Graphit Knowledge semantic query failed: {}",
            semantic_failure_class(error)
        );
    }
    response
}

fn inference_response(
    error: &(dyn std::error::Error + 'static),
) -> (axum::http::StatusCode, &'static str) {
    use axum::http::StatusCode as S;
    let mut cause = Some(error);
    while let Some(error) = cause {
        if let Some(error) = error.downcast_ref::<crate::search_rerank::SearchRerankError>() {
            let code = error.code();
            let status = match code {
                "request_cancelled_or_expired" => S::REQUEST_TIMEOUT,
                "authentication_or_permission_denied" => S::FORBIDDEN,
                "rerank_limit" => S::PAYLOAD_TOO_LARGE,
                "rerank_service_unavailable" => S::SERVICE_UNAVAILABLE,
                _ => S::BAD_GATEWAY,
            };
            return (status, code);
        }
        if let Some(error) = error.downcast_ref::<graphit_ai::embedding::EmbeddingError>() {
            use graphit_ai::embedding::EmbeddingError as E;
            return match error {
                E::Context(_)
                | E::Network(
                    graphit_http::client::NetworkError::Cancelled
                    | graphit_http::client::NetworkError::Deadline,
                ) => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
                E::Limit | E::Network(graphit_http::client::NetworkError::Limit) => {
                    (S::PAYLOAD_TOO_LARGE, "embedding_limit")
                }
                E::Shape | E::NonFinite | E::Codec => (S::BAD_GATEWAY, "embedding_invalid_output"),
                E::Credential => (S::FORBIDDEN, "model_access_denied"),
                _ => (S::BAD_GATEWAY, "embedding_service_failure"),
            };
        }
        if let Some(error) =
            error.downcast_ref::<graphit_daemon::model_service::ModelServiceError>()
        {
            use graphit_daemon::model_service::ModelErrorKind as K;
            return match error.kind() {
                K::Cancelled | K::Deadline => (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
                K::Limit => (S::PAYLOAD_TOO_LARGE, "embedding_limit"),
                _ => (S::BAD_GATEWAY, "model_failure"),
            };
        }
        if let Some(error) = error.downcast_ref::<graphit_auth::AuthError>() {
            return match error {
                graphit_auth::AuthError::Request => {
                    (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                }
                graphit_auth::AuthError::State | graphit_auth::AuthError::Io => {
                    (S::INTERNAL_SERVER_ERROR, "authentication_store_failure")
                }
                _ => (S::FORBIDDEN, "model_access_denied"),
            };
        }
        if let Some(error) = error.downcast_ref::<graphit_auth::ServiceUseError>() {
            // This enum contains only typed causes, never credentials or bodies.
            eprintln!("Graphit Knowledge service admission failed: {error:?}");
            return match crate::embedding_service::service_kind(error) {
                graphit_daemon::model_service::ModelErrorKind::Cancelled
                | graphit_daemon::model_service::ModelErrorKind::Deadline => {
                    (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                }
                graphit_daemon::model_service::ModelErrorKind::Authentication => {
                    (S::FORBIDDEN, "model_access_denied")
                }
                graphit_daemon::model_service::ModelErrorKind::Limit => {
                    (S::PAYLOAD_TOO_LARGE, "embedding_limit")
                }
                graphit_daemon::model_service::ModelErrorKind::Busy => {
                    (S::SERVICE_UNAVAILABLE, "model_service_busy")
                }
                _ => (S::BAD_GATEWAY, "model_service_failure"),
            };
        }
        if let Some(error) = error.downcast_ref::<graphit_auth::broker::BrokerError>() {
            return match error {
                graphit_auth::broker::BrokerError::Cancelled
                | graphit_auth::broker::BrokerError::Deadline => {
                    (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
                }
                _ => (S::BAD_GATEWAY, "embedding_discovery_failure"),
            };
        }
        if let Some(error) = error.downcast_ref::<std::io::Error>()
            && matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
            )
        {
            return (S::REQUEST_TIMEOUT, "request_cancelled_or_expired");
        }
        if let Some(
            crate::embedding_service::EmbeddingServiceError::MissingLocal
            | crate::embedding_service::EmbeddingServiceError::MissingHttp
            | crate::embedding_service::EmbeddingServiceError::Unavailable,
        ) = error.downcast_ref::<crate::embedding_service::EmbeddingServiceError>()
        {
            return (S::SERVICE_UNAVAILABLE, "embedding_service_unavailable");
        }
        cause = error.source();
    }
    (S::BAD_GATEWAY, "embedding_service_failure")
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use graphit_auth::{ServiceUseError, broker::BrokerError};

    #[test]
    fn service_failures_preserve_http_categories_and_explicit_denial() {
        use axum::http::StatusCode as S;
        for (error, expected) in [
            (
                ServiceUseError::Broker(BrokerError::Connection),
                (S::BAD_GATEWAY, "model_service_failure"),
            ),
            (
                ServiceUseError::Broker(BrokerError::Busy),
                (S::SERVICE_UNAVAILABLE, "model_service_busy"),
            ),
            (
                ServiceUseError::Broker(BrokerError::InvalidMetadata),
                (S::BAD_GATEWAY, "model_service_failure"),
            ),
            (
                ServiceUseError::Broker(BrokerError::InvalidKeys),
                (S::BAD_GATEWAY, "model_service_failure"),
            ),
            (
                ServiceUseError::Broker(BrokerError::InvalidToken),
                (S::FORBIDDEN, "model_access_denied"),
            ),
            (
                ServiceUseError::Broker(BrokerError::Deadline),
                (S::REQUEST_TIMEOUT, "request_cancelled_or_expired"),
            ),
            (
                ServiceUseError::Auth(graphit_auth::AuthError::Denied),
                (S::FORBIDDEN, "model_access_denied"),
            ),
        ] {
            assert_eq!(inference_response(&error), expected);
        }
    }
}

fn semantic_failure_class(
    error: &crate::knowledge_semantic::KnowledgeSemanticError,
) -> &'static str {
    use crate::knowledge_semantic::KnowledgeSemanticError as E;
    match error {
        E::Runtime(_) => "runtime_key",
        E::Projection(_) => "document_projection",
        E::MissingEmbeddingDimensions => "model_dimensions_missing",
        E::Data(error) => embedding_failure_class(error),
        _ => "semantic_operation",
    }
}

fn embedding_failure_class(
    error: &graphit_data::knowledge_embeddings::KnowledgeEmbeddingError,
) -> &'static str {
    use graphit_data::knowledge_embeddings::KnowledgeEmbeddingError as D;
    match error {
        D::InvalidStored("FULL16 schema" | "schema" | "schema/null") => "embedding_schema",
        D::InvalidStored("dimensions") => "embedding_dimensions",
        D::InvalidStored(_) => "embedding_stored_data",
        D::InvalidInput(_) => "embedding_input",
        D::Store(_) | D::Sdk(_) | D::Branch(_) => "embedding_storage",
        D::Io(_) => "embedding_io",
        _ => "embedding_operation",
    }
}

fn embedding_response(
    error: &graphit_data::knowledge_embeddings::KnowledgeEmbeddingError,
) -> (axum::http::StatusCode, &'static str) {
    use axum::http::StatusCode as S;
    match error {
        graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Index(error) => {
            crate::reference_explorer::Failure::knowledge_response(error)
        }
        graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Io(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut
            ) =>
        {
            (S::REQUEST_TIMEOUT, "request_cancelled_or_expired")
        }
        graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Conflict => {
            (S::CONFLICT, "knowledge_generation_changed")
        }
        _ => (S::INTERNAL_SERVER_ERROR, "knowledge_semantic_failure"),
    }
}

/// A missing model preserves the Source/NN generations actually captured by the SDK.
pub(crate) struct WithoutModelPage {
    pub(crate) generation: KnowledgeGeneration,
    pub(crate) semantic_generation:
        Option<graphit_data::knowledge_embeddings::KnowledgeEmbeddingGeneration>,
    pub(crate) lexical: Option<KnowledgeSearchPage>,
    pub(crate) empty: bool,
}
pub(crate) enum SearchOutcome {
    Lexical(KnowledgeSearchPage),
    Vector(Option<graphit_data::knowledge_search::KnowledgeVectorSearchPage>),
    WithoutModel(Option<WithoutModelPage>),
    Reranked(Box<SearchOutcome>, crate::search_rerank::RerankScores),
}
impl SearchOutcome {
    pub(crate) fn rerank_selection(&self) -> Option<(&KnowledgeGeneration, Vec<&str>)> {
        match self {
            Self::Reranked(page, _) => page.rerank_selection(),
            Self::Lexical(page) => Some((
                &page.generation,
                page.hits.iter().map(|hit| hit.slug.as_str()).collect(),
            )),
            Self::Vector(Some(page)) => Some((
                &page.generation,
                page.hits.iter().map(|hit| hit.slug.as_str()).collect(),
            )),
            Self::WithoutModel(Some(page)) => page.lexical.as_ref().map(|lexical| {
                (
                    &lexical.generation,
                    lexical.hits.iter().map(|hit| hit.slug.as_str()).collect(),
                )
            }),
            _ => None,
        }
    }
    pub(crate) fn view(&self) -> ReadView<'_> {
        match self {
            Self::Reranked(page, scores) => page.view().reranked(scores),
            Self::Lexical(page) => ReadView::Page(page.into()),
            Self::Vector(Some(page)) => ReadView::Page(page.into()),
            Self::WithoutModel(Some(page)) => ReadView::Page(PageView {
                state: if page.empty {
                    "published_empty"
                } else if page.lexical.is_some() {
                    "partial"
                } else {
                    "unavailable"
                },
                reason: (!page.empty).then_some("model_unavailable"),
                total: page.lexical.as_ref().map_or(0, |p| p.total),
                hits: Hits::Lexical(
                    page.lexical.as_ref().map_or(&[], |p| p.hits.as_slice()),
                    None,
                ),
                next_cursor: page.lexical.as_ref().and_then(|p| p.next_cursor.as_ref()),
            }),
            Self::Vector(None) | Self::WithoutModel(None) => ReadView::Empty { state: "no_corpus" },
        }
    }
}
pub(crate) async fn query_without_model_capture(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    query: KnowledgeLexicalQuery<'_>,
    mode: Mode,
    limits: KnowledgeSearchLimits,
    preview: MarkdownPreviewLimits,
) -> Result<
    Option<(
        WithoutModelPage,
        Box<graphit_data::knowledge_embeddings::KnowledgeEmbeddingCapture>,
    )>,
    KnowledgeSearchError,
> {
    use graphit_data::knowledge_embeddings::{
        KnowledgeEmbeddingRead, capture_knowledge_embedding_corpus,
    };
    if mode == Mode::Semantic && query.cursor.is_some() {
        return Err(KnowledgeSearchError::InvalidCursor);
    }
    let corpus = match capture_knowledge_embedding_corpus(original, read, limits.read)
        .await
        .map_err(KnowledgeSearchError::Capture)?
    {
        KnowledgeEmbeddingRead::NoCorpus => return Ok(None),
        KnowledgeEmbeddingRead::Published(corpus) => corpus,
    };
    let mut empty = true;
    while let Some(documents) = corpus
        .next_documents(original, read)
        .await
        .map_err(KnowledgeSearchError::Capture)?
    {
        empty &= documents.is_empty();
    }
    let lexical = if mode == Mode::Hybrid {
        Some(
            graphit_knowledge::search::query_native_knowledge_embedding_source_search(
                original, read, &corpus, query, limits, preview,
            )
            .await?,
        )
    } else {
        None
    };
    read.recheck().map_err(KnowledgeIndexError::from)?;
    original.check().map_err(KnowledgeIndexError::from)?;
    Ok(Some((
        WithoutModelPage {
            generation: corpus.source_generation().clone(),
            semantic_generation: corpus.semantic_generation(),
            lexical,
            empty,
        },
        corpus,
    )))
}

/// The existing execution attachment and caller-specific service selection.
pub(crate) struct QuestionModel<'a, 'lease> {
    pub(crate) resources: &'a crate::foreground_memory_jobs::MemoryJobResources,
    pub(crate) service: &'a mut graphit_auth::ServiceSelection<'lease>,
    pub(crate) rpc_timeout: std::time::Duration,
}
pub(crate) struct QuestionSearch {
    pub(crate) observation: serde_json::Value,
    pub(crate) selected: Option<graphit_data::knowledge_search::KnowledgeSelectedDocuments>,
}
fn question_search_error(
    error: KnowledgeSearchError,
) -> crate::knowledge_semantic::KnowledgeSemanticError {
    crate::knowledge_semantic::KnowledgeSemanticError::Search(Box::new(error))
}
fn question_observation(
    view: PageView<'_>,
    selection: &[String],
    generation: &KnowledgeGeneration,
    semantic: Option<&graphit_data::knowledge_embeddings::KnowledgeEmbeddingGeneration>,
) -> serde_json::Value {
    serde_json::json!({
        "state": view.state,
        "reason": view.reason,
        "total": view.total,
        "generation": generation,
        "semantic_generation": semantic.map(|g| serde_json::json!({
            "version": g.version,
        })),
        "selected_slugs": selection,
    })
}
/// Retrieval keeps only ranked identifiers until the same pin reads selected bodies.
pub(crate) async fn retrieve_question(
    request: &graphit_hub::policy::RequestContext<'_>,
    read: &ProjectReadAdmission<'_>,
    text: &str,
    retrieval: &super::ask::Retrieval,
    limits: KnowledgeSearchLimits,
    preview: MarkdownPreviewLimits,
    model: Option<QuestionModel<'_, '_>>,
) -> Result<QuestionSearch, crate::knowledge_semantic::KnowledgeSemanticError> {
    let original = request.snapshot();
    let query = KnowledgeLexicalQuery {
        text,
        top_k: retrieval.top_k.get(),
        page_size: NonZeroUsize::new(retrieval.top_k.get().min(100)).expect("nonzero selection"),
        cursor: None,
        caller_binding: "knowledge_question",
        preview: false,
    };
    let absent = || QuestionSearch {
        observation: serde_json::json!({"state":"no_corpus","selected_slugs":[]}),
        selected: None,
    };
    if retrieval.mode == Mode::Fts {
        let capture = match graphit_data::knowledge_search::capture_native_knowledge_search(
            original,
            read,
            limits.read,
        )
        .await
        .map_err(question_search_error)?
        {
            KnowledgeSearchRead::NoCorpus => return Ok(absent()),
            KnowledgeSearchRead::Published(capture) => capture,
        };
        let mut page = graphit_knowledge::search::query_native_knowledge_search(
            original, read, &capture, query, limits, preview,
        )
        .await
        .map_err(question_search_error)?;
        let mut selection: Vec<_> = std::mem::take(&mut page.hits)
            .into_iter()
            .map(|hit| hit.slug)
            .collect();
        while let Some(cursor) = page.next_cursor.take() {
            let mut next_query = query;
            next_query.cursor = Some(&cursor);
            let next = graphit_knowledge::search::query_native_knowledge_search(
                original, read, &capture, next_query, limits, preview,
            )
            .await
            .map_err(question_search_error)?;
            selection.extend(next.hits.into_iter().map(|hit| hit.slug));
            page.next_cursor = next.next_cursor;
        }
        let observation =
            question_observation(PageView::from(&page), &selection, &page.generation, None);
        let selected = if selection.is_empty() {
            None
        } else {
            let slugs: Vec<_> = selection.iter().map(String::as_str).collect();
            Some(
                capture
                    .read_selected_documents(original, read, &slugs)
                    .await
                    .map_err(question_search_error)?,
            )
        };
        return Ok(QuestionSearch {
            observation,
            selected,
        });
    }
    if let Some(QuestionModel {
        resources,
        service,
        rpc_timeout,
    }) = model
    {
        let result = query_vector_capture(
            resources,
            request,
            read,
            service,
            rpc_timeout,
            crate::knowledge_semantic::KnowledgeSemanticSearchOptions {
                page: query,
                mode: match retrieval.mode {
                    Mode::Semantic => {
                        graphit_data::knowledge_search::KnowledgeVectorSearchMode::Semantic
                    }
                    Mode::Hybrid => {
                        graphit_data::knowledge_search::KnowledgeVectorSearchMode::Hybrid
                    }
                    Mode::Fts => unreachable!("lexical handled above"),
                },
                metric: retrieval.metric.unwrap_or_default().native(),
                limits,
                preview,
            },
            true,
        )
        .await?;
        let Some(crate::knowledge_semantic::KnowledgeSearchExecution {
            page,
            corpus: capture,
            selected_slugs: selection,
        }) = result
        else {
            return Ok(absent());
        };
        let observation = question_observation(
            PageView::from(&page),
            &selection,
            &page.generation,
            page.semantic_generation.as_ref(),
        );
        let selected = if selection.is_empty() {
            None
        } else {
            let slugs: Vec<_> = selection.iter().map(String::as_str).collect();
            Some(
                capture
                    .read_selected_documents(original, read, &slugs)
                    .await
                    .map_err(question_search_error)?,
            )
        };
        return Ok(QuestionSearch {
            observation,
            selected,
        });
    }
    let Some((mut page, capture)) =
        query_without_model_capture(original, read, query, retrieval.mode, limits, preview)
            .await
            .map_err(question_search_error)?
    else {
        return Ok(absent());
    };
    let mut selection = Vec::new();
    if let Some(lexical) = &mut page.lexical {
        selection.extend(
            std::mem::take(&mut lexical.hits)
                .into_iter()
                .map(|hit| hit.slug),
        );
        while let Some(cursor) = lexical.next_cursor.take() {
            let mut next_query = query;
            next_query.cursor = Some(&cursor);
            let next = graphit_knowledge::search::query_native_knowledge_embedding_source_search(
                original, read, &capture, next_query, limits, preview,
            )
            .await
            .map_err(question_search_error)?;
            selection.extend(next.hits.into_iter().map(|hit| hit.slug));
            lexical.next_cursor = next.next_cursor;
        }
    }
    let generation = page.generation.clone();
    let semantic = page.semantic_generation.clone();
    let outcome = SearchOutcome::WithoutModel(Some(page));
    let ReadView::Page(view) = outcome.view() else {
        unreachable!("captured a publication")
    };
    let observation = question_observation(view, &selection, &generation, semantic.as_ref());
    let selected = if selection.is_empty() {
        None
    } else {
        let slugs: Vec<_> = selection.iter().map(String::as_str).collect();
        Some(
            capture
                .read_selected_documents(original, read, &slugs)
                .await
                .map_err(question_search_error)?,
        )
    };
    Ok(QuestionSearch {
        observation,
        selected,
    })
}
