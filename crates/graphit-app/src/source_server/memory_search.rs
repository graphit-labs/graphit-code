//! PROJECT Memory retrieval over the existing Source owner and borrowed model.
use super::*;
use crate::embedding_service::EmbeddingServiceError;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ServicePurpose};
use graphit_core::{module_settings::Module, native_path::NativePath};
use graphit_daemon::model_service::{
    MemoryEmbeddingCause, MemoryEmbeddingFailure, MemoryEmbeddingModel,
};
use graphit_records::{memory_attributes::MemoryClass, memory_store::*};
use std::num::{NonZeroU32, NonZeroUsize};

pub(super) const TOOL: &str = "graphit_memory_search";

pub(super) fn definition() -> Tool {
    Tool::new(
        TOOL,
        "Search PROJECT Memory with FTS (default), semantic distance, or hybrid RRF. Semantic modes use the existing model and never fall back to FTS. Selectors grant no access.",
        json!({"type":"object","properties":{
            "text":{"type":"string","minLength":1},
            "top_k":{"type":"integer","minimum":1},
            "mode":{"enum":["fts","semantic","hybrid"]},
            "filter":{"type":"object","properties":{
                "classification":{"enum":["fact","skill","tension","decision","convention","correction"]},
                "tag":{"type":"string"},"mandatory":{"type":"boolean"},
                "important":{"type":"boolean"},"exclude_mandatory":{"type":"boolean"}
            },"additionalProperties":false},
            "include_deleted_history":{"type":"boolean"},
            "project_dir":{"type":"string"},"project_version":{"type":"string"}
        },"required":["text","top_k"],"additionalProperties":false})
        .as_object().expect("object schema").clone(),
    )
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    #[default]
    Fts,
    Semantic,
    Hybrid,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Fts => "fts",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Class {
    Fact,
    Skill,
    Tension,
    Decision,
    Convention,
    Correction,
}
impl From<Class> for MemoryClass {
    fn from(value: Class) -> Self {
        match value {
            Class::Fact => Self::Fact,
            Class::Skill => Self::Skill,
            Class::Tension => Self::Tension,
            Class::Decision => Self::Decision,
            Class::Convention => Self::Convention,
            Class::Correction => Self::Correction,
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    #[serde(default, deserialize_with = "present")]
    classification: Option<Class>,
    #[serde(default, deserialize_with = "present")]
    tag: Option<String>,
    #[serde(default, deserialize_with = "present")]
    mandatory: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    important: Option<bool>,
    #[serde(default)]
    exclude_mandatory: bool,
}
impl From<Filter> for MemoryListFilter {
    fn from(value: Filter) -> Self {
        Self {
            classification: value.classification.map(Into::into),
            tag: value.tag,
            mandatory: value.mandatory,
            important: value.important,
            exclude_mandatory: value.exclude_mandatory,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    text: String,
    top_k: NonZeroUsize,
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    filter: Filter,
    #[serde(default)]
    include_deleted_history: bool,
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
    ) -> Result<usize, &'static str> {
        let current = snapshot.current.ok_or("PROJECT selection unavailable")?;
        let scalar = state.policy.content.max_scalar_bytes;
        let invalid_scalar =
            |value: &str| value.trim().is_empty() || value.contains('\0') || value.len() > scalar;
        if invalid_scalar(&self.text)
            || self.top_k.get() > state.policy.content.max_collection_entries
            || self.filter.tag.as_deref().is_some_and(invalid_scalar)
            || self.project_version.as_deref().is_some_and(invalid_scalar)
        {
            return Err("Memory query or selection invalid");
        }
        let root = self
            .project_dir
            .as_ref()
            .map(NativePath::as_path)
            .unwrap_or(snapshot.projects[current].0.root());
        if self.project_version.is_none() && root == snapshot.projects[current].0.root() {
            Ok(current)
        } else {
            select_read_project(&snapshot.projects, root, self.project_version.as_deref())
        }
    }
}

#[derive(Debug)]
enum SearchError {
    Request(graphit_hub::policy::RequestError),
    Auth(graphit_auth::AuthError),
    Operation(io::Error),
    Service(graphit_auth::ServiceUseError),
    Store(MemoryStoreError),
    Model(EmbeddingServiceError),
    Json(serde_json::Error),
}
impl From<io::Error> for SearchError {
    fn from(value: io::Error) -> Self {
        Self::Operation(value)
    }
}
impl From<io::ErrorKind> for SearchError {
    fn from(value: io::ErrorKind) -> Self {
        Self::Operation(value.into())
    }
}
impl From<MemoryEmbeddingFailure<EmbeddingServiceError>> for SearchError {
    fn from(value: MemoryEmbeddingFailure<EmbeddingServiceError>) -> Self {
        match value.cause {
            MemoryEmbeddingCause::Store(cause) => Self::Store(*cause),
            MemoryEmbeddingCause::Service(cause) => Self::Model(cause),
        }
    }
}
impl SearchError {
    fn response(self) -> CallToolResult {
        let (code, message) = match self {
            Self::Request(cause) => ("request_cancelled_or_expired", cause.to_string()),
            Self::Auth(cause) => ("access_denied", cause.to_string()),
            Self::Operation(cause) => {
                let code = if matches!(
                    cause.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) {
                    "request_cancelled_or_expired"
                } else {
                    "operation_failed"
                };
                (code, cause.to_string())
            }
            Self::Service(cause) => {
                let code = match crate::embedding_service::service_kind(&cause) {
                    graphit_daemon::model_service::ModelErrorKind::Cancelled
                    | graphit_daemon::model_service::ModelErrorKind::Deadline => {
                        "request_cancelled_or_expired"
                    }
                    _ => "service_denied",
                };
                (code, cause.to_string())
            }
            Self::Store(cause) => {
                let code = match &cause {
                    MemoryStoreError::Auth(_) => "access_denied",
                    MemoryStoreError::Request(_) => "request_cancelled_or_expired",
                    MemoryStoreError::DifferentRequest => "different_original_request",
                    MemoryStoreError::WrongAdmission => "wrong_admission",
                    MemoryStoreError::LexicalIndexUnavailable(_) => "lexical_index_unavailable",
                    MemoryStoreError::Invalid(_) | MemoryStoreError::Limit(_) => {
                        "invalid_native_input"
                    }
                    _ => "native_store_failed",
                };
                (code, cause.to_string())
            }
            Self::Model(cause) => {
                let code = match cause.kind() {
                    graphit_daemon::model_service::ModelErrorKind::Cancelled
                    | graphit_daemon::model_service::ModelErrorKind::Deadline => {
                        "request_cancelled_or_expired"
                    }
                    _ => "model_failed",
                };
                (code, cause.to_string())
            }
            Self::Json(cause) => ("response_invalid", cause.to_string()),
        };
        crate::mcp_output::typed_error(json!({"kind":"error","code":code,"message":message}))
    }
}

fn failed(code: &str, message: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"kind":"error","code":code,"message":message}))
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: Arguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(value) => value,
            Err(_) => return failed("invalid_arguments", "Memory arguments invalid"),
        };
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            return failed(
                "selection_unavailable",
                "Memory PROJECT selection unavailable",
            );
        }
    };
    let selected = match input.select(&server.0, &snapshot) {
        Ok(value) => value,
        Err(cause) => return failed("invalid_query", cause),
    };
    let (paths, id, version) = &snapshot.projects[selected];
    if !matches!(
        server.0.live_projects.is_active(paths, *id, version),
        Ok(true)
    ) {
        return failed(
            "selection_unavailable",
            "Memory PROJECT selection unavailable",
        );
    }
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failed("access_denied", "Memory incoming credentials required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failed("access_denied", "Memory credential origin required"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return failed("access_denied", "Memory bearer credential required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return failed("request_cancelled_or_expired", "Memory request cancelled"),
            () = state.stop.cancelled() => return failed("request_cancelled_or_expired", "Memory service stopped"),
            () = tokio::time::sleep_until(deadline.into()) => return failed("request_cancelled_or_expired", "Memory deadline expired"),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failed("request_cancelled_or_expired", "Memory service stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(execute(
                &owner,
                &snapshot.projects[selected],
                input,
                &credential,
                local,
                &original_cancel,
                deadline,
            ));
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result = receive => match result {
            Ok(_) if context.ct.is_cancelled() || cancel.is_cancelled() => failed("request_cancelled_or_expired", "Memory request cancelled"),
            Ok(_) if state.stop.is_cancelled() => failed("request_cancelled_or_expired", "Memory service stopped"),
            Ok(_) if Instant::now() >= deadline => failed("request_cancelled_or_expired", "Memory deadline expired"),
            Ok(Ok(payload)) => crate::mcp_output::typed(payload),
            Ok(Err(cause)) => cause.response(),
            Err(_) => failed("operation_failed", "Memory operation failed"),
        },
        () = context.ct.cancelled() => failed("request_cancelled_or_expired", "Memory request cancelled"),
        () = state.stop.cancelled() => failed("request_cancelled_or_expired", "Memory service stopped"),
        () = tokio::time::sleep_until(deadline.into()) => failed("request_cancelled_or_expired", "Memory deadline expired"),
    }
}

fn read_limits(state: &State) -> Result<MemoryStoreReadLimits, SearchError> {
    Ok(MemoryStoreReadLimits {
        max_query_bytes: NonZeroUsize::new(state.policy.content.max_output_bytes)
            .ok_or(io::ErrorKind::InvalidInput)?,
        batch_rows: NonZeroU32::new(8192).ok_or(io::ErrorKind::InvalidInput)?,
        native: state.policy.index,
    })
}

async fn execute(
    state: &State,
    selection: &(
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    input: Arguments,
    credential: &str,
    local: bool,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<Value, SearchError> {
    let request = RequestContext::new(state.policy.content, cancel, deadline)
        .map_err(SearchError::Request)?;
    let original = request.snapshot();
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, &request)
    } else {
        issuer.authenticate_runtime(credential, &request)
    }
    .map_err(SearchError::Auth)?;
    let (paths, id, version) = selection;
    let live = CapturedPublicationScope::project(paths, *id, state.policy.scope, original)?;
    let record = live.binding()?;
    let scope = ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: state.policy.binding,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Memory)
        .map_err(SearchError::Auth)?;
    let limits = read_limits(state)?;
    let check_read = || -> Result<(), SearchError> {
        read.recheck().map_err(SearchError::Auth)?;
        original.check()?;
        Ok(())
    };
    let mode = input.mode;
    let payload = match mode {
        Mode::Fts => lexical(
            NativeMemoryStore::search_lexical(
                original,
                &read,
                MemoryLexicalQuery {
                    text: input.text,
                    filter: input.filter.into(),
                    include_deleted_history: input.include_deleted_history,
                    top_k: input.top_k,
                },
                limits,
            )
            .await
            .map_err(SearchError::Store)?,
        ),
        Mode::Semantic | Mode::Hybrid => {
            let enabled = state
                .hook
                .as_ref()
                .map(|hook| {
                    hook.selected_modules(&state.policy.installation, paths, original)
                        .map(|modules| {
                            modules.enabled(Module::Memory) && modules.enabled(Module::Embedding)
                        })
                })
                .transpose()?;
            if enabled == Some(false) {
                json!({"kind":"unavailable","reason":{"kind":"model_disabled"}})
            } else if let Some(resources) = state.semantic_model()? {
                enabled.ok_or(io::ErrorKind::NotConnected)?;
                read.recheck().map_err(SearchError::Auth)?;
                let mut service = issuer
                    .capture_service(&caller, ServicePurpose::Embedding)
                    .map_err(SearchError::Service)?;
                let metadata =
                    crate::embedding_service::metadata(&mut service, &resources.execution())
                        .await
                        .map_err(SearchError::Model)?;
                let identity =
                    crate::foreground_memory_jobs::MemoryJobResources::memory_identity(&metadata)
                        .map_err(SearchError::Store)?;
                let model = MemoryEmbeddingModel::new(original, identity, state.policy.timeout)
                    .map_err(SearchError::Store)?;
                let value = if matches!(mode, Mode::Semantic) {
                    semantic(
                        model
                            .query(
                                &read,
                                input.text,
                                MemorySemanticSearchOptions {
                                    filter: input.filter.into(),
                                    include_deleted_history: input.include_deleted_history,
                                    top_k: input.top_k,
                                },
                                limits,
                                |operation| {
                                    let resources = &resources;
                                    let metadata = &metadata;
                                    let caller = &caller;
                                    async move {
                                        let mut service = issuer
                                            .capture_service(caller, ServicePurpose::Embedding)?;
                                        resources
                                            .embed(&mut service, metadata, operation)
                                            .await
                                            .map(|result| result.vectors)
                                    }
                                },
                            )
                            .await?,
                    )
                } else {
                    hybrid(
                        model
                            .query_hybrid(
                                &read,
                                input.text,
                                MemoryHybridSearchOptions {
                                    filter: input.filter.into(),
                                    include_deleted_history: input.include_deleted_history,
                                    top_k: input.top_k,
                                },
                                limits,
                                |operation| {
                                    let resources = &resources;
                                    let metadata = &metadata;
                                    let caller = &caller;
                                    async move {
                                        let mut service = issuer
                                            .capture_service(caller, ServicePurpose::Embedding)?;
                                        resources
                                            .embed(&mut service, metadata, operation)
                                            .await
                                            .map(|result| result.vectors)
                                    }
                                },
                            )
                            .await?,
                    )
                };
                return delivery(state, mode, value, || {
                    check_read()?;
                    service.mode().map(|_| ()).map_err(SearchError::Service)
                });
            } else {
                json!({"kind":"unavailable","reason":{"kind":"model_unavailable"}})
            }
        }
    };
    delivery(state, mode, payload, check_read)
}

fn delivery(
    state: &State,
    mode: Mode,
    mut payload: Value,
    mut before: impl FnMut() -> Result<(), SearchError>,
) -> Result<Value, SearchError> {
    before()?;
    payload
        .as_object_mut()
        .ok_or(io::ErrorKind::InvalidData)?
        .insert("mode".into(), json!(mode.name()));
    if serde_json::to_vec(&payload)
        .map_err(SearchError::Json)?
        .len()
        > state.policy.content.max_output_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Memory response exceeds output limit",
        )
        .into());
    }
    before()?;
    Ok(payload)
}

fn generation(value: &MemoryGeneration) -> Value {
    json!({"sdk_version":value.sdk_version,"project_id":value.project_id,
        "project_version":value.project_version,"physical_branch":value.physical_branch,
        "inherited_target":value.inherited_target})
}
fn location(value: MemoryMatchLocation) -> Value {
    match value {
        MemoryMatchLocation::Current => json!({"kind":"current"}),
        MemoryMatchLocation::Archived { revision } => {
            json!({"kind":"archived","revision":revision})
        }
    }
}
fn unavailable(g: MemoryGeneration, reason: MemoryEmbeddingUnavailable) -> Value {
    let kind = match reason {
        MemoryEmbeddingUnavailable::MissingCoverage => "missing_coverage",
        MemoryEmbeddingUnavailable::IncompatibleIdentity => "incompatible_identity",
    };
    json!({"kind":"unavailable","generation":generation(&g),"reason":{"kind":kind}})
}
fn lexical(value: MemoryLexicalSearchRead) -> Value {
    match value {
        MemoryLexicalSearchRead::NoCorpus => json!({"kind":"no_corpus"}),
        MemoryLexicalSearchRead::PublishedEmpty { generation: g } => {
            json!({"kind":"published_empty","generation":generation(&g)})
        }
        MemoryLexicalSearchRead::Published {
            generation: g,
            total_hits,
            hits,
        } => json!({
            "kind":"published","generation":generation(&g),"total_hits":total_hits,
            "hits":hits.into_iter().map(|hit|json!({"current":hit.current,"matched":hit.matched,
                "location":location(hit.location),"deleted":hit.deleted,"score":hit.score})).collect::<Vec<_>>()
        }),
    }
}
fn semantic(value: MemorySemanticSearchRead) -> Value {
    match value {
        MemorySemanticSearchRead::NoCorpus => json!({"kind":"no_corpus"}),
        MemorySemanticSearchRead::PublishedEmpty { generation: g } => {
            json!({"kind":"published_empty","generation":generation(&g)})
        }
        MemorySemanticSearchRead::Unavailable {
            generation: g,
            reason,
        } => unavailable(g, reason),
        MemorySemanticSearchRead::Published {
            generation: g,
            total_hits,
            hits,
        } => json!({
            "kind":"published","generation":generation(&g),"total_hits":total_hits,
            "hits":hits.into_iter().map(|hit|json!({"current":hit.current,"matched_revision":hit.matched_revision,
                "location":location(hit.location),"distance":hit.distance})).collect::<Vec<_>>()
        }),
    }
}
fn hybrid(value: MemoryHybridSearchRead) -> Value {
    match value {
        MemoryHybridSearchRead::NoCorpus => json!({"kind":"no_corpus"}),
        MemoryHybridSearchRead::PublishedEmpty { generation: g } => {
            json!({"kind":"published_empty","generation":generation(&g)})
        }
        MemoryHybridSearchRead::Unavailable {
            generation: g,
            reason,
        } => unavailable(g, reason),
        MemoryHybridSearchRead::Published {
            generation: g,
            total_hits,
            hits,
        } => json!({
            "kind":"published","generation":generation(&g),"total_hits":total_hits,
            "hits":hits.into_iter().map(|hit|json!({"current":hit.current,"score":hit.score,
                "lexical":hit.lexical.map(|matched|json!({"matched_revision":matched.matched_revision,
                    "location":location(matched.location),"score":matched.score})),
                "semantic":hit.semantic.map(|matched|json!({"matched_revision":matched.matched_revision,
                    "location":location(matched.location),"distance":matched.distance}))})).collect::<Vec<_>>()
        }),
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(super) mod tests;
