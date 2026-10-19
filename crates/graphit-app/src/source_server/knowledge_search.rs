//! Authorized PROJECT/imported Knowledge retrieval on the existing Source job owner.
use super::*;
use crate::foreground_knowledge::{reference_binding::ReferenceBinding, search};
use crate::task_server::browser::codec::{AdmittedSelection, encode_response};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority};
use graphit_core::native_path::NativePath;
use graphit_data::{
    knowledge_embeddings::KnowledgeEmbeddingError,
    knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexReadLimits},
    knowledge_search::{KnowledgeSearchRead, capture_native_knowledge_search},
};
use search::ProjectPageCapture;
use std::num::{NonZeroU32, NonZeroUsize};

pub(super) const TOOL: &str = "graphit_knowledge_search";
pub(super) fn definition() -> Tool {
    Tool::new(TOOL,
        "Search an authorized PROJECT or installed Knowledge corpus in fts, semantic or hybrid mode, using configured embeddings only when the captured index is ready. Results retain native namespace, generation, opaque cursor and optional Markdown preview. Selectors and cursors grant no access.",
        json!({"type":"object","properties":{
            "search":{"type":"object","properties":{"query":{"type":"string"},"mode":{"enum":["fts","semantic","hybrid"]},"metric":{"enum":["cosine","l2","dot"]},
                "top_k":{"type":"integer","minimum":0},"page_size":{"type":"integer","minimum":1,"maximum":100},
                "preview":{"type":"boolean"},"rerank":{"type":"boolean"},"cursor":{"type":"object"}},
                "required":["query","mode"],"additionalProperties":false},
            "source":{"type":"object","properties":{"kind":{"enum":["project_knowledge","imported_knowledge"]},
                "artifact":{"type":"object","properties":{"id":{"type":"string"},"version":{"type":"string"},"context_alias":{"type":"string"}},"required":["id","version"],"additionalProperties":false}},
                "required":["kind"],"additionalProperties":false},
            "project_dir":{"type":"string"},"project_version":{"type":"string"}
        },"required":["search"],"additionalProperties":false}).as_object().expect("schema").clone())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    search: search::Input,
    #[serde(default, deserialize_with = "present")]
    source: Option<crate::reference_explorer::SourceKey>,
    #[serde(default, deserialize_with = "present")]
    project_dir: Option<NativePath>,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
}
impl Arguments {
    fn select(
        &self,
        state: &State,
        snapshot: &crate::live_project_selections::LiveProjectSnapshot,
    ) -> Result<Option<usize>, &'static str> {
        self.search.validate(state.policy.content)?;
        match &self.source {
            Some(crate::reference_explorer::SourceKey::ImportedKnowledge { artifact }) => {
                artifact.validate(state.policy.content)?;
                if self.project_dir.is_some() || self.project_version.is_some() {
                    return Err("imported_source_has_project_selection");
                }
                return Ok(None);
            }
            None | Some(crate::reference_explorer::SourceKey::ProjectKnowledge) => {}
            _ => return Err("invalid_knowledge_source"),
        }
        if self.project_version.as_deref().is_some_and(|version| {
            version.is_empty()
                || version.contains('\0')
                || version.len() > state.policy.content.max_scalar_bytes
        }) {
            return Err("invalid_project_version");
        }
        let current = snapshot.current.ok_or("selection_unavailable")?;
        let root = self
            .project_dir
            .as_ref()
            .map(NativePath::as_path)
            .unwrap_or(snapshot.projects[current].0.root());
        if self.project_version.is_none() && root == snapshot.projects[current].0.root() {
            Ok(Some(current))
        } else {
            select_read_project(&snapshot.projects, root, self.project_version.as_deref()).map(Some)
        }
    }
}
fn failure(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"status":"error","code":code}))
}
#[derive(Debug)]
enum Error {
    Remote(Box<crate::remote_native_mount::ErrorKind>),
    Semantic(Box<crate::knowledge_semantic::KnowledgeSemanticError>),
    Imported(graphit_knowledge::imported_knowledge::ImportedKnowledgeReadError),
    ImportedSearch(graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError),
    Auth(graphit_auth::AuthError),
    Operation(io::Error),
    Search(graphit_data::knowledge_search::KnowledgeSearchError),
    Rerank(crate::search_rerank::SearchRerankError),
    Encoding(serde_json::Error),
}
impl Error {
    fn response(self) -> CallToolResult {
        let code = match self {
            Self::Remote(error) => error.code(),
            Self::Semantic(error) => search::semantic_response(&error).1,
            Self::Imported(error) => {
                crate::reference_explorer::Failure::imported_response(&error).1
            }
            Self::ImportedSearch(error) => match error {
                graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError::Reader(
                    error,
                ) => crate::reference_explorer::Failure::imported_response(&error).1,
                graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError::Query(
                    error,
                ) => search::response(&error).1,
            },
            Self::Auth(error) => match error {
                graphit_auth::AuthError::Request => "request_cancelled_or_expired",
                graphit_auth::AuthError::State | graphit_auth::AuthError::Io => {
                    "authentication_store_failure"
                }
                _ => "authentication_or_permission_denied",
            },
            Self::Operation(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) =>
            {
                "request_cancelled_or_expired"
            }
            Self::Operation(error) => {
                let capture = error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<KnowledgeEmbeddingError>())
                    .map(search::capture_failure_diagnostic);
                eprintln!(
                    "Graphit Knowledge Source operation failed: io={:?} capture={}",
                    error.kind(),
                    capture.as_deref().unwrap_or("unclassified")
                );
                "knowledge_capture_failure"
            }
            Self::Search(error) => search::response(&error).1,
            Self::Rerank(error) => error.code(),
            Self::Encoding(error) if error.io_error_kind() == Some(io::ErrorKind::FileTooLarge) => {
                "response_limit"
            }
            Self::Encoding(_) => "response_encoding_failed",
        };
        failure(code)
    }
}
impl From<crate::remote_native_mount::ErrorKind> for Error {
    fn from(error: crate::remote_native_mount::ErrorKind) -> Self {
        Self::Remote(Box::new(error))
    }
}
impl From<crate::knowledge_semantic::KnowledgeSemanticError> for Error {
    fn from(error: crate::knowledge_semantic::KnowledgeSemanticError) -> Self {
        Self::Semantic(Box::new(error))
    }
}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}
impl From<graphit_knowledge::imported_knowledge::ImportedKnowledgeReadError> for Error {
    fn from(error: graphit_knowledge::imported_knowledge::ImportedKnowledgeReadError) -> Self {
        Self::Imported(error)
    }
}
impl From<graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError> for Error {
    fn from(error: graphit_knowledge::imported_knowledge::ImportedKnowledgeSearchError) -> Self {
        Self::ImportedSearch(error)
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Encoding(error)
    }
}
impl From<graphit_auth::AuthError> for Error {
    fn from(error: graphit_auth::AuthError) -> Self {
        Self::Auth(error)
    }
}
impl From<graphit_data::knowledge_search::KnowledgeSearchError> for Error {
    fn from(error: graphit_data::knowledge_search::KnowledgeSearchError) -> Self {
        Self::Search(error)
    }
}
fn read_limits(
    state: &State,
) -> Result<graphit_data::knowledge_search::KnowledgeSearchLimits, Error> {
    let max_identifier_bytes = NonZeroUsize::new(state.policy.content.max_scalar_bytes)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "zero Knowledge scalar limit")
        })?;
    search::query_limits(
        KnowledgeIndexReadLimits {
            native: state.policy.index,
            batch_rows: NonZeroU32::new(8192).expect("positive batch size"),
            content: KnowledgeIndexContentLimits {
                max_identifier_bytes,
            },
        },
        state.policy.content,
    )
    .map_err(graphit_data::knowledge_search::KnowledgeSearchError::from)
    .map_err(Error::from)
}

#[allow(clippy::too_many_arguments)]
async fn rerank_project(
    state: &State,
    caller: &graphit_auth::AuthenticatedCaller<'_>,
    admission: &graphit_auth::ProjectReadAdmission<'_>,
    paths: &ProjectPaths,
    input: &search::Input,
    capture: ProjectPageCapture<'_>,
    generation: &graphit_data::knowledge_index::KnowledgeGeneration,
    slugs: &[&str],
    request: &RequestContext<'_>,
) -> Result<Option<crate::search_rerank::RerankScores>, Error> {
    if input.rerank == Some(false) || slugs.is_empty() {
        return Ok(None);
    }
    let config = crate::ast_profile_loader::project_config(
        &state.policy.installation,
        paths,
        state.graph.project_config()?,
        request.snapshot(),
    )?;
    let enabled = match input.rerank {
        Some(value) => value,
        None => crate::search_rerank::configured(&config)?,
    };
    if !enabled {
        return Ok(None);
    }
    let mut service = state
        .auth
        .issuer()
        .capture_service(caller, graphit_auth::ServicePurpose::Rerank)
        .map_err(crate::knowledge_semantic::KnowledgeSemanticError::from)?;
    let selected = capture
        .selected(request.snapshot(), admission, slugs)
        .await?;
    let texts = search::rerank_texts(&selected, generation, slugs)?;
    let control = state
        .local_models
        .read()
        .map_err(|_| io::Error::other("model control observation failed"))?
        .clone();
    let result = crate::search_rerank::run(
        request.snapshot(),
        &mut service,
        control
            .as_ref()
            .map(|control| (control, state.auth.runtime().as_ref())),
        &config,
        &state.policy.native_install.hub,
        state.native_verifiers.as_deref(),
        &input.query,
        &texts,
    )
    .await;
    capture.recheck(request.snapshot(), admission)?;
    Ok(Some(result.map_err(Error::Rerank)?))
}
async fn execute(
    state: &State,
    selection: Option<&(ProjectPaths, ProjectId, VersionContext)>,
    input: Arguments,
    credential: &str,
    local: bool,
    request: &RequestContext<'_>,
    session: &str,
) -> Result<Value, Error> {
    let original = request.snapshot();
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }?;
    if let Some(crate::reference_explorer::SourceKey::ImportedKnowledge { artifact }) =
        &input.source
    {
        use graphit_knowledge::imported_knowledge::{
            ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadLimits,
            open_imported_knowledge,
        };
        let admission = issuer
            .admit_authenticated_imported_knowledge_read(&caller, state.auth.installation())?;
        let id = ArtifactId::new(&artifact.id, state.policy.registry.limits().projection())
            .map_err(io::Error::other)?;
        let version = ConcreteVersion::new(
            &artifact.version,
            state.policy.registry.limits().projection(),
        )
        .map_err(io::Error::other)?;
        let limits = read_limits(state)?;
        if let Some(page) = super::remote_native::search_admitted(
            state,
            &admission,
            &id,
            &version,
            &input.search,
            limits,
            session,
        )
        .await?
        {
            let bytes = encode_response(
                &ReferenceBinding {
                    selection: None,
                    source: input.source.as_ref().expect("matched imported source"),
                    focused_source: None,
                    native: page.view(),
                },
                state.policy.content.max_output_bytes,
            )?;
            let payload = serde_json::from_slice(&bytes)?;
            admission.recheck()?;
            original.check()?;
            return Ok(payload);
        }

        let opened = open_imported_knowledge(
            &admission,
            ImportedKnowledgeInput {
                registry: &state.policy.registry,
                id: &id,
                version: &version,
                context_alias: artifact.context_alias.as_deref(),
            },
            ImportedKnowledgeReadLimits {
                directory: state.policy.index,
                index: limits.read,
            },
        )
        .await?;
        let rerank = crate::foreground_knowledge::imported_search::ImportedRerankContext {
            auth: &state.auth,
            owner: crate::foreground_knowledge::imported_search::ImportedRerankOwner::Source {
                hook: state.hook.as_deref(),
                installation: &state.policy.installation,
                local_models: &state.local_models,
            },
            verifiers: state.native_verifiers.as_deref(),
        };
        let page = match &opened {
            ImportedKnowledgeOpen::RegistryAbsent => None,
            ImportedKnowledgeOpen::Published(reader) => {
                let outcome = if input.search.mode == search::Mode::Fts {
                    search::SearchOutcome::Lexical(
                        reader
                            .search(
                                input.search.native(session),
                                limits,
                                search::preview_limits(state.policy.content).map_err(
                                    graphit_data::knowledge_search::KnowledgeSearchError::from,
                                )?,
                            )
                            .await?,
                    )
                } else {
                    let resources = state.semantic_model()?;
                    let binding =
                        serde_json::to_string(&(session, input.search.mode, input.search.metric))?;
                    crate::foreground_knowledge::imported_search::query(
                        reader,
                        &admission,
                        resources.as_deref(),
                        crate::knowledge_semantic::KnowledgeSemanticSearchOptions {
                            page: input.search.native(&binding),
                            mode: input.search.vector_mode(),
                            metric: input.search.metric.native(),
                            limits,
                            preview: search::preview_limits(state.policy.content).map_err(
                                graphit_data::knowledge_search::KnowledgeSearchError::from,
                            )?,
                        },
                    )
                    .await?
                };
                Some(
                    rerank
                        .copied(&admission, reader, &input.search, limits.read, outcome)
                        .await?,
                )
            }
        };
        let bytes = encode_response(
            &ReferenceBinding {
                selection: None,
                source: input.source.as_ref().expect("matched imported source"),
                focused_source: None,
                native: page.as_ref().map_or(
                    search::ReadView::Empty {
                        state: "registry_absent",
                    },
                    search::SearchOutcome::view,
                ),
            },
            state.policy.content.max_output_bytes,
        )?;
        let payload = serde_json::from_slice(&bytes)?;
        if let ImportedKnowledgeOpen::Published(reader) = &opened {
            reader.recheck()?;
        }
        admission.recheck()?;
        original.check()?;
        return Ok(payload);
    }
    let selection = selection.ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let (paths, id, version) = selection;
    if !state.live_projects.is_active(paths, *id, version)? {
        return Err(Error::Operation(io::ErrorKind::NotFound.into()));
    }
    let live = CapturedPublicationScope::project(paths, *id, state.policy.scope, original)?;
    let binding = live.binding()?;
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority.admit_authenticated_read(
        &caller,
        request,
        ScopeInput {
            installation: state.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &live,
                    record: &binding,
                },
                version,
            }),
            providers: &[],
            binding: state.policy.binding,
            global_version: "",
        },
        ProjectReadModule::Knowledge,
    )?;
    let limits = read_limits(state)?;
    let selection = AdmittedSelection {
        project_dir: NativePath::new(paths.root().to_owned())?,
        project_id: *id,
        project_version: version.version().to_owned(),
    };
    let caller_binding =
        serde_json::to_string(&(&selection, session, input.search.mode, input.search.metric))?;
    if input.search.mode != search::Mode::Fts {
        let mut resources = state.semantic_model()?;
        if resources.is_some() {
            let modules = state
                .hook
                .as_ref()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
                .selected_modules(&state.policy.installation, paths, original)?;
            if !modules.enabled(graphit_core::module_settings::Module::Embedding) {
                resources = None;
            }
        }
        let (page, retained) = if let Some(resources) = &resources {
            let mut service = issuer
                .capture_service(&caller, graphit_auth::ServicePurpose::Embedding)
                .map_err(crate::knowledge_semantic::KnowledgeSemanticError::from)?;
            let execution = search::query_vector_capture(
                resources,
                request,
                &admission,
                &mut service,
                state.policy.timeout,
                crate::knowledge_semantic::KnowledgeSemanticSearchOptions {
                    page: input.search.native(&caller_binding),
                    mode: input.search.vector_mode(),
                    metric: input.search.metric.native(),
                    limits,
                    preview: search::preview_limits(state.policy.content)
                        .map_err(graphit_data::knowledge_search::KnowledgeSearchError::from)?,
                },
                false,
            )
            .await?;
            match execution {
                Some(execution) => (
                    search::SearchOutcome::Vector(Some(execution.page)),
                    Some(execution.corpus),
                ),
                None => (search::SearchOutcome::Vector(None), None),
            }
        } else {
            let execution = search::query_without_model_capture(
                original,
                &admission,
                input.search.native(&caller_binding),
                input.search.mode,
                limits,
                search::preview_limits(state.policy.content)
                    .map_err(graphit_data::knowledge_search::KnowledgeSearchError::from)?,
            )
            .await?;
            match execution {
                Some((page, capture)) => (
                    search::SearchOutcome::WithoutModel(Some(page)),
                    Some(capture),
                ),
                None => (search::SearchOutcome::WithoutModel(None), None),
            }
        };
        let ranks = if let (Some(capture), Some((generation, slugs))) =
            (retained.as_deref(), page.rerank_selection())
        {
            rerank_project(
                state,
                &caller,
                &admission,
                paths,
                &input.search,
                ProjectPageCapture::Vector(capture),
                generation,
                &slugs,
                request,
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
        let bytes = encode_response(
            &ReferenceBinding {
                selection: Some(&selection),
                source: &crate::reference_explorer::SourceKey::ProjectKnowledge,
                focused_source: None,
                native,
            },
            state.policy.content.max_output_bytes,
        )?;
        let payload = serde_json::from_slice(&bytes)?;
        admission.recheck()?;
        if !state.live_projects.is_active(paths, *id, version)? {
            return Err(Error::Operation(io::ErrorKind::NotFound.into()));
        }
        original.check()?;
        return Ok(payload);
    }
    tracing::debug!("data_reader_enter");
    let capture = capture_native_knowledge_search(original, &admission, limits.read).await?;
    let page = match &capture {
        KnowledgeSearchRead::NoCorpus => None,
        KnowledgeSearchRead::Published(capture) => Some(
            graphit_knowledge::search::query_native_knowledge_search(
                original,
                &admission,
                capture,
                input.search.native(&caller_binding),
                limits,
                search::preview_limits(state.policy.content)
                    .map_err(graphit_data::knowledge_search::KnowledgeSearchError::from)?,
            )
            .await?,
        ),
    };
    let ranks = match (&capture, &page) {
        (KnowledgeSearchRead::Published(capture), Some(page)) => {
            let slugs: Vec<_> = page
                .hits
                .iter()
                .map(|hit| hit.source.slug.as_str())
                .collect();
            rerank_project(
                state,
                &caller,
                &admission,
                paths,
                &input.search,
                ProjectPageCapture::Lexical(capture),
                &page.generation,
                &slugs,
                request,
            )
            .await?
        }
        _ => None,
    };
    let native = page
        .as_ref()
        .map_or(search::ReadView::Empty { state: "no_corpus" }, |page| {
            search::ReadView::Page(page.into())
        });
    let native = if let Some(ranks) = &ranks {
        native.reranked(ranks)
    } else {
        native
    };
    let bytes = encode_response(
        &ReferenceBinding {
            selection: Some(&selection),
            source: &crate::reference_explorer::SourceKey::ProjectKnowledge,
            focused_source: None,
            native,
        },
        state.policy.content.max_output_bytes,
    )?;
    let payload = serde_json::from_slice(&bytes)?;
    admission.recheck()?;
    if !state.live_projects.is_active(paths, *id, version)? {
        return Err(Error::Operation(io::ErrorKind::NotFound.into()));
    }
    original.check()?;
    Ok(payload)
}

#[tracing::instrument(name = "source_knowledge", level = "debug", skip_all)]
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: Arguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(input) => input,
            Err(_) => return failure("invalid_arguments"),
        };
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return failure("selection_unavailable"),
    };
    let selected = match input.select(&server.0, &snapshot) {
        Ok(selected) => selected,
        Err(code) => return failure(code),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("incoming_credentials_required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure("credential_origin_required"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return failure("credential_required");
    };
    let session = parts
        .headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, receive) = oneshot::channel();
    tracing::debug!("queue_enter");
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return failure("request_cancelled_or_expired"),
            () = state.stop.cancelled() => return failure("stopped"),
            () = tokio::time::sleep_until(deadline.into()) => return failure("request_cancelled_or_expired"),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("stopped");
        }
        let owner = Arc::clone(&state);
        let worker_span = tracing::Span::current();
        jobs.tasks.spawn_blocking(move || {
            worker_span.in_scope(|| {
                tracing::debug!("worker_enter");
                let result = handle.block_on(async {
                    let request =
                        RequestContext::new(owner.policy.content, &original_cancel, deadline)
                            .map_err(io::Error::other)?;
                    execute(
                        &owner,
                        selected.map(|selected| &snapshot.projects[selected]),
                        input,
                        &credential,
                        local,
                        &request,
                        &session,
                    )
                    .await
                });
                let _ = send.send(result);
            })
        });
    }
    tokio::select! {
        result = receive => match result {
            Ok(_) if context.ct.is_cancelled() || cancel.is_cancelled() || state.stop.is_cancelled() => failure("request_cancelled_or_expired"),
            Ok(_) if Instant::now() >= deadline => failure("request_cancelled_or_expired"),
            Ok(Ok(payload)) => crate::mcp_output::typed(payload),
            Ok(Err(error)) => error.response(),
            Err(_) => failure("native_job_failed"),
        },
        () = context.ct.cancelled() => failure("request_cancelled_or_expired"),
        () = state.stop.cancelled() => failure("stopped"),
        () = tokio::time::sleep_until(deadline.into()) => failure("request_cancelled_or_expired"),
    }
}
