//! Explicit index maintenance under the incoming caller's ordinary Memory authority.
use super::*;
use crate::embedding_service::EmbeddingServiceError;
use crate::memory_index_entry::Scope;
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ServicePurpose,
    UserMemoryWriteOperation,
    profile::{AiService, ProfileError, ProfileLimits},
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{StoreOwner, UserIdentity},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    config::{ConfigSnapshot, ConfigStore},
    module_settings::{Module, ModuleSettings},
    native_path::{MAX_PATH_BYTES, NativePath},
};
use graphit_daemon::model_service::{
    MemoryEmbeddingCause, MemoryEmbeddingIndexRead, MemoryEmbeddingModel, ModelSettings,
    UserMemoryEmbeddingIndexRead,
};
use graphit_hub::policy::ResourceSetting;
use graphit_records::{memory_store::*, user_memory_store::*};
use std::num::{NonZeroU32, NonZeroUsize};

pub(super) const TOOL: &str = "graphit_memory_index";

pub(super) fn definition() -> Tool {
    Tool::new(
        TOOL,
        "Ensure Memory lexical indexes, or explicitly refresh them, and maintain embeddings when configured. PROJECT selectors grant no access. USER requires the verified Native subject and rejects PROJECT selectors. Confirmed lexical and embedding effects survive later failures.",
        json!({"type":"object","properties":{
            "scope":{"enum":["project","user"],"default":"project"},
            "refresh":{"type":"boolean","default":false},
            "project_dir":{"type":"string"},"project_version":{"type":"string"}
        },"additionalProperties":false})
        .as_object()
        .expect("schema")
        .clone(),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    #[serde(default)]
    scope: Scope,
    #[serde(default)]
    refresh: bool,
    #[serde(default, deserialize_with = "present")]
    project_dir: Option<NativePath>,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
}

struct Failure(Value);
impl Failure {
    fn new(code: &str, message: impl std::fmt::Display) -> Self {
        Self(json!({"kind":"error","code":code,"message":message.to_string()}))
    }
    fn operation(error: impl std::fmt::Display) -> Self {
        Self::new("operation_failed", error)
    }
    fn response(self) -> CallToolResult {
        crate::mcp_output::typed_error(self.0)
    }
    fn with_result(mut self, result: Value) -> Self {
        self.0["result"] = result;
        self
    }
}
impl From<io::Error> for Failure {
    fn from(cause: io::Error) -> Self {
        Self::new(
            if matches!(
                cause.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
            ) {
                "request_cancelled_or_expired"
            } else {
                "operation_failed"
            },
            cause,
        )
    }
}
impl From<graphit_auth::AuthError> for Failure {
    fn from(cause: graphit_auth::AuthError) -> Self {
        Self::new("access_denied", cause)
    }
}
impl From<graphit_auth::ServiceUseError> for Failure {
    fn from(cause: graphit_auth::ServiceUseError) -> Self {
        Self::new("service_denied", cause)
    }
}
impl From<EmbeddingServiceError> for Failure {
    fn from(cause: EmbeddingServiceError) -> Self {
        Self::new("model_failed", cause)
    }
}
impl From<graphit_daemon::model_service::MemoryEmbeddingFailure<EmbeddingServiceError>>
    for Failure
{
    fn from(
        error: graphit_daemon::model_service::MemoryEmbeddingFailure<EmbeddingServiceError>,
    ) -> Self {
        let mut failure = model_failure(error.cause);
        failure.0["last_attachment"] = json!(
            error
                .last_attachment
                .map(|value| project_attachment(*value))
        );
        failure
    }
}
impl From<MemoryStoreError> for Failure {
    fn from(cause: MemoryStoreError) -> Self {
        let code = match &cause {
            MemoryStoreError::Auth(_) => "access_denied",
            MemoryStoreError::Request(_) => "request_cancelled_or_expired",
            MemoryStoreError::DifferentRequest => "different_original_request",
            MemoryStoreError::WrongAdmission => "wrong_admission",
            MemoryStoreError::LexicalIndexUnavailable(_) => "lexical_index_unavailable",
            MemoryStoreError::IndexConfirmed { .. }
            | MemoryStoreError::UserIndexConfirmed { .. } => "index_confirmed",
            MemoryStoreError::EmbeddingConfirmed { .. }
            | MemoryStoreError::UserEmbeddingConfirmed { .. } => "embedding_confirmed",
            MemoryStoreError::Commit(_) => "index_publication_unconfirmed",
            _ => "native_store_failed",
        };
        let mut failure = Self::new(code, &cause);
        match cause {
            MemoryStoreError::IndexConfirmed { publication, cause } => {
                failure.0["lexical"] = project_publication(&publication);
                failure.0["cause"] = Self::from(*cause).0;
            }
            MemoryStoreError::UserIndexConfirmed { publication, cause } => {
                failure.0["lexical"] = user_publication(&publication);
                failure.0["cause"] = Self::from(*cause).0;
            }
            MemoryStoreError::EmbeddingConfirmed { publication, cause } => {
                failure.0["attachment"] =
                    project_attachment(MemoryEmbeddingAttachOutcome::Published(*publication));
                failure.0["cause"] = Self::from(*cause).0;
            }
            MemoryStoreError::UserEmbeddingConfirmed { publication, cause } => {
                failure.0["attachment"] =
                    user_attachment(UserMemoryEmbeddingAttachOutcome::Published(*publication));
                failure.0["cause"] = Self::from(*cause).0;
            }
            MemoryStoreError::Commit(cause) => {
                failure.0["recovery_key"] = json!(cause.recovery_key());
            }
            _ => {}
        }
        failure
    }
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: Arguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(value) => value,
            Err(error) => return Failure::new("invalid_arguments", error).response(),
        };
    if input.scope == Scope::User
        && (input.project_dir.is_some() || input.project_version.is_some())
    {
        return Failure::new("invalid_arguments", "USER has no PROJECT selection").response();
    }
    if input.project_version.as_deref().is_some_and(|value| {
        value.trim().is_empty()
            || value.contains('\0')
            || value.len() > server.0.policy.content.max_scalar_bytes
    }) {
        return Failure::new("invalid_arguments", "invalid PROJECT version").response();
    }
    let selection = if input.scope == Scope::Project {
        let snapshot = match server.0.live_projects.snapshot() {
            Ok(value) => value,
            Err(error) => return Failure::operation(error).response(),
        };
        let root = input
            .project_dir
            .as_ref()
            .map(NativePath::as_path)
            .or_else(|| {
                snapshot
                    .current
                    .map(|current| snapshot.projects[current].0.root())
            });
        let Some(root) = root else {
            return Failure::new("selection_unavailable", "PROJECT selection unavailable")
                .response();
        };
        let selected = if input.project_version.is_none()
            && snapshot
                .current
                .is_some_and(|current| snapshot.projects[current].0.root() == root)
        {
            snapshot.current.expect("checked current")
        } else {
            match select_project(&snapshot.projects, root, input.project_version.as_deref()) {
                Ok(value) => value,
                Err(error) => return Failure::new("selection_unavailable", error).response(),
            }
        };
        let (paths, id, version) = &snapshot.projects[selected];
        if !matches!(
            server.0.live_projects.is_active(paths, *id, version),
            Ok(true)
        ) {
            return Failure::new("selection_unavailable", "PROJECT selection inactive").response();
        }
        Some((paths.clone(), *id, version.clone()))
    } else {
        None
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return Failure::new("access_denied", "incoming credentials required").response();
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return Failure::new("access_denied", "Native credential origin required").response(),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return Failure::new("access_denied", "bearer credential required").response();
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let job_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, mut receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return Failure::new("request_cancelled_or_expired", "request cancelled").response(),
            () = state.stop.cancelled() => return Failure::new("request_cancelled_or_expired", "service stopped").response(),
            () = tokio::time::sleep_until(deadline.into()) => return Failure::new("request_cancelled_or_expired", "deadline expired").response(),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return Failure::new("request_cancelled_or_expired", "service stopped").response();
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(execute(
                &owner,
                selection.as_ref(),
                input,
                &credential,
                local,
                &job_cancel,
                deadline,
            ));
            let _ = send.send(result);
        });
    }
    // Let the existing SDK finish its cancellation barrier so a known commit is
    // returned with its receipt rather than being misreported as no effect.
    tokio::select! {
        result = &mut receive => return finish_response(result,
            context.ct.is_cancelled() || cancel.is_cancelled() || state.stop.is_cancelled() || Instant::now() >= deadline),
        () = context.ct.cancelled() => {},
        () = state.stop.cancelled() => {},
        () = tokio::time::sleep_until(deadline.into()) => {},
    }
    cancel.cancel();
    finish_response(receive.await, true)
}

fn finish_response(
    result: Result<Result<Value, Failure>, oneshot::error::RecvError>,
    cancelled: bool,
) -> CallToolResult {
    match result {
        Ok(Ok(value)) if cancelled => Failure::new(
            "request_cancelled_or_expired",
            "request cancelled after maintenance",
        )
        .with_result(value)
        .response(),
        Ok(Ok(value)) => crate::mcp_output::typed(value),
        Ok(Err(error)) => error.response(),
        Err(error) => Failure::operation(error).response(),
    }
}

async fn execute(
    state: &State,
    selection: Option<&(ProjectPaths, ProjectId, VersionContext)>,
    input: Arguments,
    credential: &str,
    local: bool,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<Value, Failure> {
    let request = RequestContext::new(state.policy.content, cancel, deadline)
        .map_err(|error| Failure::new("request_cancelled_or_expired", error))?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, &request)
    } else {
        issuer.authenticate_runtime(credential, &request)
    }?;
    let config = selected_configuration(state, selection.map(|(paths, _, _)| paths), &request)?;
    if !ModuleSettings::resolve(&config)?.enabled(Module::Memory) {
        return Err(Failure::new(
            "module_disabled",
            "Memory is disabled for this selection",
        ));
    }
    let operation = if input.refresh {
        MemoryLexicalIndexOperation::Refresh
    } else {
        MemoryLexicalIndexOperation::Ensure
    };
    let limits = write_limits(state)?;
    if let Some((paths, id, version)) = selection {
        if !state.live_projects.is_active(paths, *id, version)? {
            return Err(Failure::new(
                "selection_unavailable",
                "PROJECT selection inactive",
            ));
        }
        let live =
            CapturedPublicationScope::project(paths, *id, state.policy.scope, request.snapshot())?;
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
        let write = authority.admit_authenticated_write(
            &caller,
            &request,
            scope,
            ProjectWriteOperation::Memory,
        )?;
        let lexical = NativeMemoryStore::maintain_lexical_index(
            request.snapshot(),
            &write,
            operation,
            limits,
        )
        .await?;
        let no_corpus = matches!(lexical, MemoryLexicalIndexOutcome::NoCorpus);
        let lexical = match lexical {
            MemoryLexicalIndexOutcome::NoCorpus => json!({"state":"no_corpus"}),
            MemoryLexicalIndexOutcome::Ready(generation) => {
                json!({"state":"ready","generation":project_generation(&generation)})
            }
            MemoryLexicalIndexOutcome::Published(publication) => project_publication(&publication),
        };
        let mut output = json!({"scope":"project","project":{"id":id,"version":version.version()},"lexical":lexical});
        if no_corpus {
            output["semantic"] = json!({"state":"no_corpus"});
        }
        let result: Result<(), Failure> = async {
            if !no_corpus {
                if let Some(resources) = model_resources(state, Some(paths), &request)? {
                    let read = authority.admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Memory)?;
                    let mut service = issuer.capture_service(&caller, ServicePurpose::Embedding)?;
                    let metadata = crate::embedding_service::metadata(&mut service, &resources.execution()).await?;
                    let identity = crate::foreground_memory_jobs::MemoryJobResources::memory_identity(&metadata)?;
                    let model = MemoryEmbeddingModel::new(request.snapshot(), identity, state.policy.timeout)?;
                    let limits = model_limits(limits, &resources, &metadata)?;
                    output["semantic"] = match model.index(&read, &write, limits.read, limits, resources.rpc_limits(), |operation| {
                        let resources = &resources;
                        let metadata = &metadata;
                        let caller = &caller;
                        async move {
                            let mut service = issuer.capture_service(caller, ServicePurpose::Embedding)?;
                            resources.embed(&mut service, metadata, operation).await.map(|result| result.vectors)
                        }
                    }).await {
                        Ok(MemoryEmbeddingIndexRead::NoCorpus) => json!({"state":"no_corpus"}),
                        Ok(MemoryEmbeddingIndexRead::Complete { generation, last_attachment }) => json!({"state":"complete","generation":project_generation(&generation),"last_attachment":last_attachment.map(|value| project_attachment(*value))}),
                        Err(error) => {
                            let mut failure = model_failure(error.cause);
                            failure.0["last_attachment"] = json!(error.last_attachment.map(|value| project_attachment(*value)));
                            return Err(failure);
                        }
                    };
                    service.mode()?;
                } else { output["semantic"] = json!({"state":"not_configured"}); }
            }
            write.recheck()?;
            request.snapshot().check()?;
            if !state.live_projects.is_active(paths, *id, version)? { return Err(Failure::new("selection_unavailable", "PROJECT selection inactive")); }
            Ok(())
        }.await;
        match result {
            Ok(()) => Ok(output),
            Err(error) => {
                if output.get("semantic").is_none() {
                    output["semantic"] = json!({"state":"failed"});
                }
                Err(error.with_result(output))
            }
        }
    } else {
        issuer.prepare_native_record_access(&caller).await?;
        let (binding, revision) = caller.native_provider()?;
        let cache = state.native_verifiers.as_ref().ok_or_else(|| {
            Failure::new(
                "native_verifier_unavailable",
                "Native verifier owner unavailable",
            )
        })?;
        let verifier = cache
            .get_native(&binding, revision)
            .map_err(Failure::operation)?;
        let caller = caller.authenticate_native(&verifier).await?;
        let verified = issuer.user_from_authenticated(&caller)?;
        let StoreOwner::Installation { installation_id } = state.auth.installation().live.owner()
        else {
            return Err(Failure::new("access_denied", "installation owner invalid"));
        };
        let identity = UserIdentity::new(
            verified.issuer()?.to_owned(),
            verified.subject()?.to_owned(),
        )?;
        let live = CapturedPublicationScope::user(
            &state.policy.installation,
            installation_id.clone(),
            identity,
            state.policy.scope,
            request.snapshot(),
        )?;
        let record = live.binding()?;
        let scope = ScopeCapture {
            live: &live,
            record: &record,
        };
        let write = issuer.admit_authenticated_user_memory_write(
            &caller,
            &request,
            scope,
            UserMemoryWriteOperation::Memory,
        )?;
        let lexical = NativeUserMemoryStore::maintain_lexical_index(
            request.snapshot(),
            &write,
            operation,
            limits,
        )
        .await?;
        let no_corpus = matches!(lexical, UserMemoryLexicalIndexOutcome::NoCorpus);
        let lexical = match lexical {
            UserMemoryLexicalIndexOutcome::NoCorpus => json!({"state":"no_corpus"}),
            UserMemoryLexicalIndexOutcome::Ready(generation) => {
                json!({"state":"ready","generation":generation})
            }
            UserMemoryLexicalIndexOutcome::Published(publication) => user_publication(&publication),
        };
        let mut output = json!({"scope":"user","lexical":lexical});
        if no_corpus {
            output["semantic"] = json!({"state":"no_corpus"});
        }
        let result: Result<(), Failure> = async {
            if !no_corpus {
                if let Some(resources) = model_resources(state, None, &request)? {
                    let read = issuer.admit_authenticated_user_memory_read(&caller, &request, scope)?;
                    let mut service = issuer.capture_service(&caller, ServicePurpose::Embedding)?;
                    let metadata = crate::embedding_service::metadata(&mut service, &resources.execution()).await?;
                    let identity = crate::foreground_memory_jobs::MemoryJobResources::memory_identity(&metadata)?;
                    let model = MemoryEmbeddingModel::new(request.snapshot(), identity, state.policy.timeout)?;
                    let limits = model_limits(limits, &resources, &metadata)?;
                    output["semantic"] = match model.index_user(&read, &write, limits.read, limits, resources.rpc_limits(), |operation| {
                        let resources = &resources;
                        let metadata = &metadata;
                        let caller = &caller;
                        async move {
                            let mut service = issuer.capture_service(caller, ServicePurpose::Embedding)?;
                            resources.embed(&mut service, metadata, operation).await.map(|result| result.vectors)
                        }
                    }).await {
                        Ok(UserMemoryEmbeddingIndexRead::NoCorpus) => json!({"state":"no_corpus"}),
                        Ok(UserMemoryEmbeddingIndexRead::Complete { generation, last_attachment }) => json!({"state":"complete","generation":generation,"last_attachment":last_attachment.map(|value| user_attachment(*value))}),
                        Err(error) => {
                            let mut failure = model_failure(error.cause);
                            failure.0["last_attachment"] = json!(error.last_attachment.map(|value| user_attachment(*value)));
                            return Err(failure);
                        }
                    };
                    service.mode()?;
                } else { output["semantic"] = json!({"state":"not_configured"}); }
            }
            write.recheck()?;
            request.snapshot().check()?;
            Ok(())
        }.await;
        match result {
            Ok(()) => Ok(output),
            Err(error) => {
                if output.get("semantic").is_none() {
                    output["semantic"] = json!({"state":"failed"});
                }
                Err(error.with_result(output))
            }
        }
    }
}

fn write_limits(state: &State) -> Result<MemoryStoreWriteLimits, Failure> {
    let n = |value| {
        NonZeroUsize::new(value)
            .ok_or_else(|| Failure::new("invalid_policy", "positive Memory limit required"))
    };
    let content = state.policy.native_install.hub.content();
    Ok(MemoryStoreWriteLimits {
        read: MemoryStoreReadLimits {
            max_query_bytes: n(content.max_output_bytes)?,
            batch_rows: NonZeroU32::new(8192).expect("positive batch"),
            native: state.policy.index,
        },
        artifacts: state.policy.native_install.hub.artifacts(),
        max_input_bytes: n(content.max_input_bytes)?,
        max_row_bytes: n(content.max_input_bytes)?,
    })
}

fn selected_configuration(
    state: &State,
    project: Option<&ProjectPaths>,
    request: &RequestContext<'_>,
) -> io::Result<ConfigSnapshot> {
    match state.hook.as_ref() {
        Some(hook) => {
            hook.selected_configuration(&state.policy.installation, project, request.snapshot())
        }
        None => ConfigSnapshot::new(
            state.policy.installation.brand(),
            [ConfigStore::global(&state.policy.installation)?.load()?],
        ),
    }
}

fn model_resources(
    state: &State,
    project: Option<&ProjectPaths>,
    request: &RequestContext<'_>,
) -> Result<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>, Failure> {
    let config = selected_configuration(state, project, request)?;
    let modules = ModuleSettings::resolve(&config)?;
    if !modules.enabled(Module::Memory) || !modules.enabled(Module::Embedding) {
        return Ok(None);
    }
    let StoreOwner::Installation { installation_id } = state.auth.installation().live.owner()
    else {
        return Err(Failure::new("access_denied", "installation owner invalid"));
    };
    // This is only the SDK's readonly configuration capture. Caller authority is
    // admitted separately, and the eventual service lease rechecks its binding.
    let live = CapturedPublicationScope::installation(
        &state.policy.installation,
        installation_id.clone(),
        [state.policy.installation.root().to_owned()],
        state.policy.scope,
        request.snapshot(),
    )?;
    let hub = &state.policy.native_install.hub;
    let content = hub.content();
    let depth = usize::try_from(hub.limit(ResourceSetting::SnapshotDepth).value)
        .map_err(Failure::operation)?;
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            live,
            AuthDocumentLimits::new(
                content.max_input_bytes,
                MAX_PATH_BYTES,
                depth,
                content.max_scalar_bytes,
            )
            .map_err(Failure::operation)?,
            request.snapshot(),
        )
        .map_err(Failure::operation)?,
        ProfileLimits {
            bytes: content.max_input_bytes,
            nodes: content.max_nodes,
            scalar_bytes: content.max_scalar_bytes,
            collection_entries: content.max_collection_entries,
            providers: content.max_collection_entries,
            profiles: content.max_collection_entries,
        },
        request,
    )
    .map_err(Failure::operation)?;
    let snapshot = store.load(request).map_err(Failure::operation)?;
    let service = match snapshot.document().active(request) {
        Ok((provider, _)) => {
            provider
                .config()
                .validate_ai_services()
                .map_err(Failure::operation)?;
            &provider.config().embedding
        }
        Err(ProfileError::NoActive) => return Ok(None),
        Err(error) => return Err(Failure::operation(error)),
    };
    match service {
        AiService::Disabled => return Ok(None),
        AiService::Local(_) => {
            let global = ConfigSnapshot::new(
                state.policy.installation.brand(),
                [ConfigStore::global(&state.policy.installation)?.load()?],
            )?;
            let configured = ModelSettings::read(&global)
                .map_err(Failure::operation)?
                .is_some_and(|settings| {
                    settings
                        .slots
                        .iter()
                        .any(|slot| slot.name == crate::models::DEFAULT_SLOT)
                });
            if !configured {
                return Ok(None);
            }
        }
        AiService::Broker | AiService::Direct { .. } => {}
    }
    state.semantic_model()?.map(Some).ok_or_else(|| {
        Failure::new(
            "model_unavailable",
            "configured Memory model is unavailable",
        )
    })
}

fn model_limits(
    mut limits: MemoryStoreWriteLimits,
    resources: &crate::foreground_memory_jobs::MemoryJobResources,
    metadata: &crate::embedding_service::EmbeddingMetadata,
) -> Result<MemoryStoreWriteLimits, Failure> {
    let dimensions = usize::try_from(metadata.dimensions)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| Failure::new("model_failed", "invalid embedding dimensions"))?;
    let rpc = resources.rpc_limits();
    limits.read.batch_rows = u32::try_from(
        rpc.max_texts
            .min(rpc.max_output_values / dimensions)
            .min(limits.read.batch_rows.get() as usize),
    )
    .ok()
    .and_then(NonZeroU32::new)
    .ok_or_else(|| Failure::new("model_failed", "invalid embedding batch limits"))?;
    Ok(limits)
}
fn model_failure(cause: MemoryEmbeddingCause<EmbeddingServiceError>) -> Failure {
    match cause {
        MemoryEmbeddingCause::Service(error) => error.into(),
        MemoryEmbeddingCause::Store(error) => (*error).into(),
    }
}
fn project_generation(value: &MemoryGeneration) -> Value {
    json!({"sdk_version":value.sdk_version,"project_id":value.project_id,"project_version":value.project_version,"physical_branch":value.physical_branch,"inherited_target":value.inherited_target})
}
fn project_publication(value: &MemoryLexicalIndexPublication) -> Value {
    json!({"state":"published","generation":project_generation(&value.generation),"recovery_key":value.recovery_key})
}
fn user_publication(value: &UserMemoryLexicalIndexPublication) -> Value {
    json!({"state":"published","generation":value.generation,"recovery_key":value.recovery_key})
}
fn project_attachment(value: MemoryEmbeddingAttachOutcome) -> Value {
    match value {
        MemoryEmbeddingAttachOutcome::Existing { generation } => {
            json!({"state":"existing","generation":project_generation(&generation)})
        }
        MemoryEmbeddingAttachOutcome::Published(value) => {
            json!({"state":"published","generation":project_generation(&value.generation),"recovery_key":value.recovery_key})
        }
    }
}
fn user_attachment(value: UserMemoryEmbeddingAttachOutcome) -> Value {
    match value {
        UserMemoryEmbeddingAttachOutcome::Existing { generation } => {
            json!({"state":"existing","generation":generation})
        }
        UserMemoryEmbeddingAttachOutcome::Published(value) => {
            json!({"state":"published","generation":value.generation,"recovery_key":value.recovery_key})
        }
    }
}

#[cfg(test)]
mod tests;
