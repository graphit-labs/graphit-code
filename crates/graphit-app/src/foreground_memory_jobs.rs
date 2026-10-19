//! Foreground Memory jobs share admitted embedding resources and the existing optional model owner.
use crate::embedding_service::{
    self, EmbeddingMetadata, EmbeddingResources, EmbeddingResult, EmbeddingServiceError,
    LocalEmbeddingResources,
};
use crate::models::DEFAULT_SLOT;
use graphit_ai::embedding::EmbeddingLimits;
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput, ServicePurpose,
    profile::{AiService, ProviderKind, ProviderRecord},
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    config::{ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    module_settings::{Module, ModuleSettings},
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::broker_verifiers::BrokerVerifierCache;
use graphit_daemon::{
    auth_startup::DaemonAuth,
    model_child::{ChildLaunch, ChildOptions, ChildProcessResult, ChildShutdown, ModelChild},
    model_service::{
        EmbedOperation, ExpectedModel, LocalExecutionSelection, MemoryEmbeddingIndexRead,
        MemoryEmbeddingModel, ModelServiceClient, ModelSettings, ResolvedConfig, RpcLimits,
        TaskKind,
    },
    supervisor::ProjectContext,
};
use graphit_http::client::BoundedClient;
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, LimitOrigin, RequestContext, ResourceSetting},
};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, Limits, PendingHost,
    ProcessHost, RepairObservation,
};
use graphit_records::memory_store::{
    MEMORY_DOCUMENT_RECIPE, MemoryEmbeddingIdentity, MemoryStoreReadLimits, MemoryStoreWriteLimits,
};
use std::{
    ffi::{OsStr, OsString},
    io,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

fn model_server_executable(
    executable: &Path,
    working_directory: &Path,
    environment: &[(OsString, OsString)],
) -> io::Result<PathBuf> {
    if !executable.is_absolute() || executable.parent().is_none() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let sibling = executable.with_file_name(format!(
        "graphit-model-server{}",
        std::env::consts::EXE_SUFFIX
    ));
    if sibling.try_exists()? {
        return which::which_in(&sibling, None::<&OsStr>, working_directory)
            .map_err(io::Error::other);
    }
    let captured = environment
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
        .map(|(_, value)| value);
    which::which_in("graphit-model-server", captured, working_directory).map_err(io::Error::other)
}

/// Resolve precisely the global configuration the ordinary child will read.
/// Missing/disabled selection preserves the foreground's other services.
#[cfg(test)]
pub(super) fn expected_model(
    installation: &InstallationPaths,
    provider: Option<&ProviderRecord>,
    original: &Arc<graphit_core::artifact_store::SnapshotContext>,
    projects: &[ModuleSettings],
) -> io::Result<Option<ExpectedModel>> {
    original.required_deadline()?;
    original.check()?;
    if !embedding_enabled(projects) {
        return Ok(None);
    }
    expected_selection(installation, provider, original)
}

pub(super) fn expected_selection(
    installation: &InstallationPaths,
    provider: Option<&ProviderRecord>,
    original: &Arc<graphit_core::artifact_store::SnapshotContext>,
) -> io::Result<Option<ExpectedModel>> {
    original.check()?;
    let Some(provider) = provider else {
        return Ok(None);
    };
    provider
        .config()
        .validate_ai_services()
        .map_err(io::Error::other)?;
    if !matches!(
        (&provider.config().kind, &provider.config().embedding),
        (ProviderKind::Local { .. }, AiService::Local(_))
    ) {
        return Ok(None);
    }
    let settings = global_model_settings(installation, original)?;
    let Some(slot) = settings.slots.iter().find(|slot| slot.name == DEFAULT_SLOT) else {
        return Ok(None);
    };
    if slot.selection.task != TaskKind::Embedding {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Memory embedding slot has another model task",
        ));
    }
    let selection = LocalExecutionSelection::from_services(
        &provider.config().embedding,
        &provider.config().rerank,
        false,
    )
    .map_err(io::Error::other)?;
    let resolved = resolve_model_settings(installation, settings, selection, original)?;
    expected_for_provider(&resolved, Some(provider))
}

pub(super) fn expected_local_model(
    installation: &InstallationPaths,
    lease: &graphit_auth::LocalServiceLease<'_>,
) -> io::Result<Option<ExpectedModel>> {
    let (provider, task, execution) = match lease.binding().map_err(io::Error::other)? {
        graphit_auth::ServiceBindingView::Local {
            provider,
            purpose: ServicePurpose::Embedding,
            execution,
        } => (provider, TaskKind::Embedding, execution),
        graphit_auth::ServiceBindingView::Local {
            provider,
            purpose: ServicePurpose::Rerank,
            execution,
        } => (provider, TaskKind::Rerank, execution),
        _ => return Err(io::ErrorKind::PermissionDenied.into()),
    };
    let original = lease.original().map_err(io::Error::other)?.snapshot();
    let settings = global_model_settings(installation, original)?;
    let slot = match task {
        TaskKind::Embedding => DEFAULT_SLOT,
        TaskKind::Rerank => crate::models::DEFAULT_RERANK_SLOT,
    };
    let Some(configured) = settings
        .slots
        .iter()
        .find(|configured| configured.name == slot)
    else {
        lease.check().map_err(io::Error::other)?;
        return Ok(None);
    };
    if configured.selection.task != task {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "selected local model slot has another task",
        ));
    }
    let selection = LocalExecutionSelection {
        embedding: (task == TaskKind::Embedding).then_some(execution),
        rerank: (task == TaskKind::Rerank).then_some(execution),
    };
    let resolved = resolve_model_settings(installation, settings, selection, original)?;
    lease.check().map_err(io::Error::other)?;
    ExpectedModel::new(&resolved, slot, provider)
        .map(Some)
        .map_err(io::Error::other)
}

pub(super) fn embedding_enabled(projects: &[ModuleSettings]) -> bool {
    projects.iter().any(|modules| {
        modules.enabled(Module::Embedding)
            && [Module::Memory, Module::Task, Module::Ast, Module::Knowledge]
                .into_iter()
                .any(|module| modules.enabled(module))
    })
}

pub(super) fn expected_for_provider(
    resolved: &ResolvedConfig,
    provider: Option<&ProviderRecord>,
) -> io::Result<Option<ExpectedModel>> {
    if resolved.config_id(DEFAULT_SLOT).is_none() {
        return Ok(None);
    }
    let provider = provider.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Embeddings require an active local embedding provider",
        )
    })?;
    provider
        .config()
        .validate_ai_services()
        .map_err(io::Error::other)?;
    if !matches!(
        (&provider.config().kind, &provider.config().embedding),
        (ProviderKind::Local { .. }, AiService::Local(_))
    ) {
        return Ok(None);
    }
    let expected =
        ExpectedModel::new(resolved, DEFAULT_SLOT, provider.name()).map_err(io::Error::other)?;
    if expected.dimensions().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Memory embedding slot has another model task",
        ));
    }
    Ok(Some(expected))
}

pub(super) fn resolve_global_models(
    installation: &InstallationPaths,
    provider: Option<&ProviderRecord>,
    original: &Arc<graphit_core::artifact_store::SnapshotContext>,
) -> io::Result<ResolvedConfig> {
    let snapshot = global_model_snapshot(installation, original)?;
    let settings = ModelSettings::read(&snapshot)
        .map_err(io::Error::other)?
        .unwrap_or(ModelSettings { slots: Vec::new() });
    let idle_timeout = ModelSettings::idle_timeout(&snapshot).map_err(io::Error::other)?;
    let selection = match provider {
        Some(provider) => {
            provider
                .config()
                .validate_ai_services()
                .map_err(io::Error::other)?;
            LocalExecutionSelection::from_services(
                &provider.config().embedding,
                &provider.config().rerank,
                false,
            )
            .map_err(io::Error::other)?
        }
        None => LocalExecutionSelection {
            embedding: None,
            rerank: None,
        },
    };
    if selection.embedding.is_some()
        && settings
            .slots
            .iter()
            .any(|slot| slot.name == DEFAULT_SLOT && slot.selection.task != TaskKind::Embedding)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "embedding slot has another model task",
        ));
    }
    resolve_model_settings(installation, settings, selection, original)?
        .with_idle_timeout(idle_timeout)
        .map_err(io::Error::other)
}

fn global_model_settings(
    installation: &InstallationPaths,
    original: &graphit_core::artifact_store::SnapshotContext,
) -> io::Result<ModelSettings> {
    let snapshot = global_model_snapshot(installation, original)?;
    Ok(ModelSettings::read(&snapshot)
        .map_err(io::Error::other)?
        .unwrap_or(ModelSettings { slots: Vec::new() }))
}

fn global_model_snapshot(
    installation: &InstallationPaths,
    original: &graphit_core::artifact_store::SnapshotContext,
) -> io::Result<ConfigSnapshot> {
    original.required_deadline()?;
    original.check()?;
    ConfigSnapshot::new(
        installation.brand(),
        [ConfigStore::global(installation)?.load()?],
    )
}

fn resolve_model_settings(
    installation: &InstallationPaths,
    mut settings: ModelSettings,
    selection: LocalExecutionSelection,
    original: &Arc<graphit_core::artifact_store::SnapshotContext>,
) -> io::Result<ResolvedConfig> {
    let options = ChildOptions::standard()?;
    for slot in &mut settings.slots {
        if slot.bundle.is_relative() {
            slot.bundle = installation.root().join(&slot.bundle);
        }
    }
    let resolved = ResolvedConfig::resolve_declared(
        settings,
        selection,
        original,
        &options.capture,
        options.source,
        &options.manifest,
    )
    .map_err(io::Error::other)?;
    original.check()?;
    Ok(resolved)
}

/// Background index RPCs include service admission and local execution. Explicit
/// request policy remains authoritative; only the built-in BG default is longer.
pub(super) fn background_embedding_rpc_timeout(policy: &HubResourcePolicy) -> Duration {
    let limit = policy.limit(ResourceSetting::RequestDeadlineMs);
    match limit.origin {
        LimitOrigin::DistributionDefault => Duration::from_secs(900),
        LimitOrigin::Configuration(_) => Duration::from_millis(limit.value),
    }
}

pub(super) fn background_embedding_request<'a>(
    policy: &HubResourcePolicy,
    original: &graphit_core::artifact_store::SnapshotContext,
    cancellation: &'a Cancellation,
) -> io::Result<RequestContext<'a>> {
    original.check()?;
    let deadline = Instant::now()
        .checked_add(background_embedding_rpc_timeout(policy))
        .ok_or(io::ErrorKind::InvalidInput)?;
    let deadline = original
        .deadline()
        .map_or(deadline, |outer| deadline.min(outer));
    RequestContext::new(policy.content(), cancellation, deadline).map_err(io::Error::other)
}

/// Observed local execution, borrowing the one process owned by ForegroundMemoryJobs.
pub struct OwnedLocalEmbedding {
    pub client: Arc<ModelServiceClient>,
    pub expected: Arc<ExpectedModel>,
    pub address: SocketAddr,
}
/// Immutable execution data. Service/corpus admissions and document bodies are
/// never cached here. Remote providers need no local artifact or child process.
pub struct MemoryJobResources {
    pub auth: Arc<DaemonAuth>,
    pub local: Option<OwnedLocalEmbedding>,
    pub http: Option<Arc<BoundedClient>>,
    pub brokers: Arc<BrokerVerifierCache>,
    pub rpc: RpcLimits,
    pub limits: EmbeddingLimits,
    pub policy: HubResourcePolicy,
    pub scope: ScopeLimits,
    pub native: NativeDocumentFileLimits,
    pub binding: NativeBinding,
}
impl MemoryJobResources {
    /// Build provider-neutral execution resources without resolving a local
    /// manifest, preparing a model, starting a child or discovering a provider.
    /// The selected service is admitted afresh for each finite operation.
    pub fn remote(
        auth: Arc<DaemonAuth>,
        http: Option<Arc<BoundedClient>>,
        brokers: Arc<BrokerVerifierCache>,
        policy: HubResourcePolicy,
        scope: ScopeLimits,
        native: NativeDocumentFileLimits,
        binding: NativeBinding,
    ) -> io::Result<Self> {
        let rpc = ChildOptions::standard()?.rpc;
        let limits = EmbeddingLimits::new(
            rpc.max_texts,
            rpc.max_input_bytes,
            rpc.max_output_values,
            rpc.max_output_values,
            rpc.max_texts,
            rpc.max_frame_bytes,
            rpc.max_frame_bytes,
        )
        .map_err(io::Error::other)?;
        Ok(Self {
            auth,
            local: None,
            http,
            brokers,
            rpc,
            limits,
            policy,
            scope,
            native,
            binding,
        })
    }
    pub fn memory_identity(
        metadata: &EmbeddingMetadata,
    ) -> Result<MemoryEmbeddingIdentity, graphit_records::memory_store::MemoryStoreError> {
        let dimensions = u32::try_from(metadata.dimensions)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(graphit_records::memory_store::MemoryStoreError::Invalid(
                "embedding dimensions",
            ))?;
        MemoryEmbeddingIdentity::new(
            metadata.vector_id,
            dimensions,
            MEMORY_DOCUMENT_RECIPE.into(),
        )
    }

    pub fn execution(&self) -> EmbeddingResources<'_> {
        EmbeddingResources {
            local: self.local.as_ref().map(|local| LocalEmbeddingResources {
                client: &local.client,
                runtime: self.auth.runtime(),
                expected: &local.expected,
                address: local.address,
            }),
            http: self.http.as_deref(),
            brokers: &self.brokers,
            limits: self.limits,
        }
    }
    pub fn rpc_limits(&self) -> RpcLimits {
        let mut limits = self.rpc;
        if self
            .local
            .as_ref()
            .is_some_and(|local| local.expected.embedding_is_scalar())
        {
            limits.max_texts = 1;
        }
        limits
    }
    pub async fn metadata(
        &self,
        request: &RequestContext<'_>,
    ) -> Result<EmbeddingMetadata, EmbeddingServiceError> {
        let key = self
            .auth
            .runtime()
            .check(request.snapshot())
            .map_err(io::Error::other)?;
        let credential =
            std::str::from_utf8(key.secret(request.snapshot()).map_err(io::Error::other)?)
                .map_err(io::Error::other)?;
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(credential, request)
            .map_err(|error| EmbeddingServiceError::Operation(io::Error::other(error)))?;
        let mut service = self
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::Embedding)?;
        embedding_service::metadata(&mut service, &self.execution()).await
    }
    pub async fn embed(
        &self,
        service: &mut graphit_auth::ServiceSelection<'_>,
        metadata: &EmbeddingMetadata,
        operation: EmbedOperation,
    ) -> Result<EmbeddingResult, EmbeddingServiceError> {
        embedding_service::embed(service, &self.execution(), metadata, operation).await
    }
    /// Execute one finite indexing batch under a fresh runtime service
    /// admission while retaining the caller's original corpus cancellation.
    pub async fn embed_batch(
        &self,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
        metadata: &EmbeddingMetadata,
        mut operation: EmbedOperation,
    ) -> Result<Vec<Vec<f32>>, EmbeddingServiceError> {
        original.check()?;
        if let Some(parent) = original.deadline() {
            operation.deadline = operation.deadline.min(parent);
        }
        let request = RequestContext::new(
            self.policy.content(),
            original.cancellation(),
            operation.deadline,
        )
        .map_err(io::Error::other)?;
        let key = self
            .auth
            .runtime()
            .check(request.snapshot())
            .map_err(io::Error::other)?;
        let credential =
            std::str::from_utf8(key.secret(request.snapshot()).map_err(io::Error::other)?)
                .map_err(io::Error::other)?;
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(credential, &request)
            .map_err(|error| EmbeddingServiceError::Operation(io::Error::other(error)))?;
        let mut service = self
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::Embedding)?;
        let output = self.embed(&mut service, metadata, operation).await?;
        original.check()?;
        Ok(output.vectors)
    }
    pub async fn index(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        cancellation: &Cancellation,
    ) -> io::Result<MemoryEmbeddingIndexRead> {
        if cancellation.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let request = RequestContext::service(self.policy.content(), cancellation)
            .map_err(io::Error::other)?;
        if project.root() != selected.root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let captured = CapturedPublicationScope::project(
            project,
            selected.id,
            self.scope,
            request.snapshot(),
        )?;
        let record = captured.binding()?;
        let scope = ScopeInput {
            installation: self.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &record,
                },
                version: &selected.version,
            }),
            providers: &[],
            binding: self.binding,
            global_version: "",
        };
        let runtime = self
            .auth
            .runtime()
            .check(request.snapshot())
            .map_err(io::Error::other)?;
        let credential = std::str::from_utf8(
            runtime
                .secret(request.snapshot())
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(credential, &request)
            .map_err(io::Error::other)?;
        let authority = ProjectRecordAuthority::new(self.auth.issuer());
        let read = authority
            .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Memory)
            .map_err(io::Error::other)?;
        let write = authority
            .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Memory)
            .map_err(io::Error::other)?;
        let model_request =
            background_embedding_request(&self.policy, request.snapshot(), cancellation)?;
        let metadata = self
            .metadata(&model_request)
            .await
            .map_err(EmbeddingServiceError::into_io)?;
        let dimensions = u32::try_from(metadata.dimensions)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(io::ErrorKind::InvalidData)?;
        let identity = MemoryEmbeddingIdentity::new(
            metadata.vector_id,
            dimensions,
            MEMORY_DOCUMENT_RECIPE.into(),
        )
        .map_err(io::Error::other)?;
        let rpc = self.rpc_limits();
        let rows = rpc
            .max_texts
            .min(rpc.max_output_values / dimensions.get() as usize)
            .min(8192);
        let rows = u32::try_from(rows)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(io::ErrorKind::InvalidInput)?;
        let positive =
            |n| NonZeroUsize::new(n).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput));
        let content = self.policy.content();
        let read_limits = MemoryStoreReadLimits {
            native: self.native,
            batch_rows: rows,
            max_query_bytes: positive(content.max_output_bytes)?,
        };
        let write_limits = MemoryStoreWriteLimits {
            read: read_limits,
            artifacts: self.policy.artifacts(),
            max_input_bytes: positive(content.max_input_bytes)?,
            max_row_bytes: positive(content.max_input_bytes)?,
        };
        let model = MemoryEmbeddingModel::new(
            request.snapshot(),
            identity,
            background_embedding_rpc_timeout(&self.policy),
        )
        .map_err(io::Error::other)?;
        model
            .index(&read, &write, read_limits, write_limits, rpc, |operation| {
                self.embed_batch(request.snapshot(), &metadata, operation)
            })
            .await
            .map_err(memory_embedding_failure_into_io)
    }
}

fn memory_embedding_failure_into_io(
    error: graphit_daemon::model_service::MemoryEmbeddingFailure<EmbeddingServiceError>,
) -> io::Error {
    use graphit_daemon::model_service::{MemoryEmbeddingCause, ModelErrorKind};
    let kind = match &error.cause {
        MemoryEmbeddingCause::Service(cause) => match cause.kind() {
            ModelErrorKind::Cancelled => io::ErrorKind::Interrupted,
            ModelErrorKind::Deadline => io::ErrorKind::TimedOut,
            _ => io::ErrorKind::Other,
        },
        MemoryEmbeddingCause::Store(cause) => match cause.as_ref() {
            graphit_records::memory_store::MemoryStoreError::Request(cause)
                if matches!(
                    cause.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) =>
            {
                cause.kind()
            }
            _ => io::ErrorKind::Other,
        },
    };
    io::Error::new(kind, error)
}

#[cfg(all(test, target_os = "linux"))]
pub(crate) fn test_broker_cache(policy: &HubResourcePolicy) -> Arc<BrokerVerifierCache> {
    Arc::new(BrokerVerifierCache::new(
        crate::foreground_source::inventory_network_limits(policy).unwrap(),
        crate::admin_auth::verifier_limits(policy.content()),
        true,
        None,
        &[],
    ))
}

enum HostOwner {
    Live(ProcessHost),
    Joining(JoinHandle<HostShutdown>),
    Pending(PendingHost),
    Joined(graphit_process::HostReport),
    JoinFailed(tokio::task::JoinError),
}

/// One compute process and the actual supervisor registry, retained until their joins complete.
pub(super) struct ForegroundMemoryJobs {
    auth: Arc<DaemonAuth>,
    client: Arc<ModelServiceClient>,
    expected: Option<Arc<ExpectedModel>>,
    embedding: bool,
    http: Option<Arc<BoundedClient>>,
    brokers: Arc<BrokerVerifierCache>,
    policy: HubResourcePolicy,
    pub(super) child: Option<ModelChild>,
    child_join: Option<JoinHandle<ChildShutdown>>,
    child_result: Option<Result<ChildShutdown, tokio::task::JoinError>>,
    exited_process: Option<ChildProcessResult>,
    host: Option<HostOwner>,
    stop: Cancellation,
}
impl ForegroundMemoryJobs {
    pub fn new(
        auth: Arc<DaemonAuth>,
        policy: HubResourcePolicy,
        expected: Option<ExpectedModel>,
        embedding: bool,
        http: Option<Arc<BoundedClient>>,
        brokers: Arc<BrokerVerifierCache>,
    ) -> io::Result<Self> {
        Ok(Self {
            auth,
            client: Arc::new(
                ModelServiceClient::new(ChildOptions::standard()?.rpc).map_err(io::Error::other)?,
            ),
            expected: expected.map(Arc::new),
            embedding,
            http,
            brokers,
            policy,
            child: None,
            child_join: None,
            child_result: None,
            exited_process: None,
            host: None,
            stop: Cancellation::default(),
        })
    }

    pub async fn start(
        &mut self,
        installation: &InstallationPaths,
        environment: Vec<(OsString, OsString)>,
        scope: ScopeLimits,
        native: NativeDocumentFileLimits,
        binding: NativeBinding,
        original: &RequestContext<'_>,
    ) -> io::Result<Option<Arc<MemoryJobResources>>> {
        if self.expected.is_some() {
            self.ensure_process(installation, environment, original)
                .await?;
        }
        self.resources(scope, native, binding)
    }
    pub async fn ensure_process(
        &mut self,
        installation: &InstallationPaths,
        environment: Vec<(OsString, OsString)>,
        original: &RequestContext<'_>,
    ) -> io::Result<(Arc<ModelServiceClient>, SocketAddr)> {
        if let Some(child) = &self.child {
            original.check()?;
            return Ok((Arc::clone(&self.client), child.address()));
        }
        // An explicitly requested acquisition may create a new owner only
        // after the previous child and native host have completed cleanup.
        if self.stop.is_cancelled() {
            self.shutdown().await?;
            self.stop = Cancellation::default();
        }
        let deadline = original.snapshot().required_deadline()?;
        original.check()?;
        if self.child.is_some() || self.host.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let executable =
            model_server_executable(&std::env::current_exe()?, installation.root(), &environment)?;
        let host = ProcessHost::install(&self.stop, HostLimits::new(1, 1)?)?;
        self.host = Some(HostOwner::Live(host));
        let Some(HostOwner::Live(host)) = self.host.as_ref() else {
            unreachable!()
        };
        let readiness = tokio::select! {
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            result = host.ready(deadline) => result,
        };
        original.check()?;
        match readiness {
            HostReadiness::Ready => {}
            HostReadiness::Pending => return Err(io::ErrorKind::TimedOut.into()),
            HostReadiness::Sealed => return Err(io::ErrorKind::BrokenPipe.into()),
            HostReadiness::Failed(error) => {
                return Err(io::Error::other(format!(
                    "Memory model host readiness: {error:?}"
                )));
            }
        }
        let child = ModelChild::start(
            ChildLaunch {
                executable,
                installation_root: installation.root().to_owned(),
                environment,
                limits: Limits {
                    stdin_bytes: 65,
                    stdout_bytes: 512,
                    stderr_bytes: 4096,
                    termination_grace: Duration::from_secs(1),
                    cleanup: Duration::from_secs(5),
                },
            },
            self.auth.runtime(),
            original.snapshot(),
            host,
        )
        .await
        .map_err(io::Error::other)?;
        self.child = Some(child);
        Ok((
            Arc::clone(&self.client),
            self.child.as_ref().expect("owned child").address(),
        ))
    }

    pub fn resources(
        &self,
        scope: ScopeLimits,
        native: NativeDocumentFileLimits,
        binding: NativeBinding,
    ) -> io::Result<Option<Arc<MemoryJobResources>>> {
        if !self.embedding {
            return Ok(None);
        }
        let local = match &self.expected {
            Some(expected) => Some(OwnedLocalEmbedding {
                client: Arc::clone(&self.client),
                expected: Arc::clone(expected),
                address: self
                    .child
                    .as_ref()
                    .ok_or(io::ErrorKind::NotConnected)?
                    .address(),
            }),
            None => None,
        };
        let mut resources = MemoryJobResources::remote(
            Arc::clone(&self.auth),
            self.http.clone(),
            Arc::clone(&self.brokers),
            self.policy.clone(),
            scope,
            native,
            binding,
        )?;
        resources.local = local;
        Ok(Some(Arc::new(resources)))
    }

    pub async fn refresh(
        &self,
        original: &graphit_core::artifact_store::SnapshotContext,
        expected_config_ids: std::collections::BTreeMap<
            String,
            graphit_daemon::model_service::ConfigId,
        >,
    ) -> io::Result<()> {
        let Some(child) = self.child.as_ref() else {
            original.check()?;
            return Ok(());
        };
        let address = child.address();
        self.client
            .refresh(
                self.auth.runtime(),
                original,
                expected_config_ids,
                TcpStream::connect(address),
            )
            .await
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn replace_selection(
        &mut self,
        expected: Option<ExpectedModel>,
        embedding: bool,
        http: Option<Arc<BoundedClient>>,
    ) {
        self.expected = expected.map(Arc::new);
        self.embedding = embedding;
        self.http = http;
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn child_lifetime_for_test(&self) -> Cancellation {
        self.stop.clone()
    }

    /// Wait for the same owned runner; no health request or restart is issued.
    pub async fn child_exited(&mut self) -> SocketAddr {
        let child = self.child.as_mut().expect("live model child owner");
        let address = child.address();
        let result = child.completed().await;
        match result {
            Ok(Ok(outcome)) => eprintln!(
                "Graphit model child exited: status={} cleanup={:?}",
                outcome.status, outcome.cleanup
            ),
            Ok(Err(failure)) => eprintln!(
                "Graphit model child exited: status={:?} reason={:?} cleanup={:?}",
                failure.status, failure.reason, failure.cleanup
            ),
            Err(error) => eprintln!(
                "Graphit model child runner ended: panic={} cancelled={}",
                error.is_panic(),
                error.is_cancelled()
            ),
        }
        let child = self.child.take().expect("observed model child owner");
        self.exited_process = Some(child.join().await);
        address
    }

    pub fn retirement_complete(&self) -> bool {
        self.child.is_none()
            && self.child_join.is_none()
            && self.child_result.is_none()
            && self.exited_process.is_none()
            && self.host.is_none()
    }

    /// Drain independent phases and report errors after owned cleanup has been observed.
    pub async fn shutdown(&mut self) -> io::Result<()> {
        if self.child.is_some() {
            let cancellation = Cancellation::default();
            let request = self
                .policy
                .request_context(Instant::now(), &cancellation)
                .map_err(io::Error::other)?;
            let original = Arc::clone(request.snapshot());
            let auth = Arc::clone(&self.auth);
            let client = Arc::clone(&self.client);
            let child = self.child.take().expect("checked child owner");
            self.child_join = Some(tokio::spawn(async move {
                child.shutdown(&client, auth.runtime(), &original).await
            }));
        }
        if let Some(join) = self.child_join.as_mut() {
            let observed = join.await;
            self.child_join = None;
            self.child_result = Some(observed);
        }
        let cleanup_deadline = Instant::now()
            .checked_add(Duration::from_secs(5))
            .ok_or(io::ErrorKind::InvalidInput)?;
        let mut child_complete = true;
        let mut process_error = None;
        let process = match self.exited_process.as_mut() {
            Some(process) => Some(process),
            None => self
                .child_result
                .as_mut()
                .and_then(|result| result.as_mut().ok().map(|result| &mut result.process)),
        };
        if let Some(Ok(process)) = process {
            let (cleanup, remaining) = match process {
                Ok(process) => (&mut process.cleanup, &mut process.remaining),
                Err(process) => (&mut process.cleanup, &mut process.remaining),
            };
            if let Some(owner) = remaining.as_mut()
                && let RepairObservation::Observed {
                    cleanup: observed, ..
                } = owner.repair(cleanup_deadline).await
            {
                *cleanup = observed;
                remaining.take();
            }
            if remaining.is_some() {
                child_complete = false;
            } else if !matches!(cleanup, Cleanup::Complete | Cleanup::NotSpawned) {
                child_complete = false;
                process_error = Some(io::Error::other(format!(
                    "Memory model process cleanup ended unconfirmed: {cleanup:?}"
                )));
            }
        }
        self.stop.cancel();
        let host_error = shutdown_host(&mut self.host, cleanup_deadline).await.err();
        // Keep the actual child outcome until the final await has completed:
        // interrupting a host wait must not discard an earlier terminal error.
        let mut first_error = None;
        if child_complete {
            if let Some(process) = self.exited_process.take() {
                match process {
                    Err(error) => first_error = Some(io::Error::other(error)),
                    Ok(Err(error)) => {
                        first_error = Some(io::Error::other(format!(
                            "Memory model process failed: reason={:?}, status={:?}, cleanup={:?}",
                            error.reason, error.status, error.cleanup
                        )));
                    }
                    Ok(Ok(_)) => (),
                }
            }
            if let Some(joined) = self.child_result.take() {
                match joined {
                    Err(error) => {
                        first_error = Some(io::Error::other(error));
                    }
                    Ok(result) => {
                        if let Err(error) = result.drain {
                            first_error = Some(io::Error::other(error));
                        }
                        if let Err(error) = result.process {
                            first_error.get_or_insert_with(|| io::Error::other(error));
                        }
                    }
                }
            }
        }
        if let Some(error) = process_error {
            first_error.get_or_insert(error);
        }
        if let Some(error) = host_error {
            first_error.get_or_insert(error);
        }
        if !child_complete {
            first_error.get_or_insert_with(|| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "Memory model process cleanup remains owned",
                )
            });
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

async fn shutdown_host(owner: &mut Option<HostOwner>, cleanup_deadline: Instant) -> io::Result<()> {
    if matches!(owner, Some(HostOwner::Live(_))) {
        let Some(HostOwner::Live(host)) = owner.take() else {
            unreachable!()
        };
        *owner = Some(HostOwner::Joining(tokio::spawn(
            host.shutdown(cleanup_deadline),
        )));
    }
    if let Some(HostOwner::Joining(join)) = owner.as_mut() {
        let observed = join.await;
        *owner = Some(match observed {
            Err(error) => HostOwner::JoinFailed(error),
            Ok(HostShutdown::Pending(owner)) => HostOwner::Pending(owner),
            Ok(HostShutdown::Joined(report)) => HostOwner::Joined(report),
        });
    }
    if let Some(HostOwner::Pending(pending)) = owner.as_mut()
        && let HostShutdownObservation::Joined(report) = pending.repair(cleanup_deadline).await
    {
        *owner = Some(HostOwner::Joined(report));
    }
    match owner.take() {
        Some(HostOwner::Joined(report)) => match report.failure() {
            Some(error) => Err(io::Error::other(format!(
                "Memory model host join: {error:?}"
            ))),
            None => Ok(()),
        },
        Some(HostOwner::JoinFailed(error)) => Err(io::Error::other(error)),
        Some(pending) => {
            *owner = Some(pending);
            Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Memory model host cleanup remains owned",
            ))
        }
        None => Ok(()),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod shutdown_tests {
    use super::*;
    use crate::foreground_source::foreground_memory_jobs_tests::memory_shutdown_fixture;

    #[test]
    fn packaged_sibling_and_captured_path_resolve_the_real_model_server() {
        let model = PathBuf::from(
            std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").expect("qualified model-server path"),
        );
        assert!(model.is_absolute());
        let directory = tempfile::tempdir_in(model.parent().unwrap()).unwrap();
        let installed = directory.path().join("installed binary siblings");
        let tools = directory.path().join("explicit tool directory");
        std::fs::create_dir(&installed).unwrap();
        std::fs::create_dir(&tools).unwrap();
        let filename = format!("graphit-model-server{}", std::env::consts::EXE_SUFFIX);
        let sibling = installed.join(&filename);
        std::fs::hard_link(&model, &sibling).unwrap();
        let cli = installed.join(format!("graphit{}", std::env::consts::EXE_SUFFIX));
        assert_eq!(
            model_server_executable(&cli, directory.path(), &[]).unwrap(),
            sibling
        );
        let explicit = tools.join(filename);
        std::fs::rename(&sibling, &explicit).unwrap();
        assert_eq!(
            model_server_executable(
                &cli,
                directory.path(),
                &[("Path".into(), tools.into_os_string())]
            )
            .unwrap(),
            explicit
        );
    }

    fn assert_join_panic(error: &io::Error) {
        assert!(
            error
                .get_ref()
                .and_then(|error| error.downcast_ref::<tokio::task::JoinError>())
                .is_some_and(tokio::task::JoinError::is_panic)
        );
    }

    #[tokio::test]
    async fn terminal_host_join_failure_is_reported_once() {
        let (_fixture, source, mut jobs) = memory_shutdown_fixture();
        let join: JoinHandle<HostShutdown> = tokio::spawn(async {
            panic!("Memory host shutdown task failed");
        });
        jobs.host = Some(HostOwner::Joining(join));
        let error = jobs.shutdown().await.unwrap_err();
        assert_join_panic(&error);
        assert!(jobs.host.is_none());
        assert!(jobs.stop.is_cancelled());
        jobs.shutdown().await.unwrap();
        drop(jobs);
        source.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn child_join_failure_still_drains_the_host() {
        let (_fixture, source, mut jobs) = memory_shutdown_fixture();
        let child: JoinHandle<ChildShutdown> = tokio::spawn(async {
            panic!("Memory child shutdown task failed");
        });
        jobs.child_join = Some(child);
        jobs.host = Some(HostOwner::Live(
            ProcessHost::install(&jobs.stop, HostLimits::new(1, 1).unwrap()).unwrap(),
        ));
        let error = jobs.shutdown().await.unwrap_err();
        assert_join_panic(&error);
        assert!(jobs.child_join.is_none());
        assert!(jobs.child_result.is_none());
        assert!(
            jobs.host.is_none(),
            "child join failure must not skip the native host join"
        );
        jobs.shutdown().await.unwrap();
        drop(jobs);
        source.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn interrupted_host_wait_preserves_the_child_error() {
        let (_fixture, source, mut jobs) = memory_shutdown_fixture();
        let child: JoinHandle<ChildShutdown> = tokio::spawn(async {
            panic!("Memory child shutdown task failed");
        });
        let child_error = match child.await {
            Err(error) => error,
            Ok(_) => panic!("expected real join failure"),
        };
        jobs.child_result = Some(Err(child_error));
        let host = ProcessHost::install(&jobs.stop, HostLimits::new(1, 1).unwrap()).unwrap();
        let gate = Arc::new(tokio::sync::Notify::new());
        let worker_gate = Arc::clone(&gate);
        jobs.host = Some(HostOwner::Joining(tokio::spawn(async move {
            worker_gate.notified().await;
            host.shutdown(Instant::now() + Duration::from_secs(1)).await
        })));
        {
            let mut closing = std::pin::pin!(jobs.shutdown());
            assert!(
                std::future::Future::poll(
                    closing.as_mut(),
                    &mut std::task::Context::from_waker(std::task::Waker::noop())
                )
                .is_pending()
            );
        }
        assert!(matches!(jobs.child_result.as_ref(), Some(Err(error)) if error.is_panic()));
        assert!(matches!(jobs.host.as_ref(), Some(HostOwner::Joining(_))));
        gate.notify_one();
        let error = jobs.shutdown().await.unwrap_err();
        assert_join_panic(&error);
        assert!(jobs.child_result.is_none());
        assert!(jobs.host.is_none());
        jobs.shutdown().await.unwrap();
        drop(jobs);
        source.shutdown().await.unwrap();
    }
}

#[cfg(test)]
mod background_deadline_tests {
    use super::*;
    use graphit_core::{
        brand::Brand,
        config::{ConfigDocument, ConfigKey, ConfigSource},
    };

    #[test]
    fn nested_store_interruption_preserves_memory_batch_failure() {
        use graphit_auth::{ServiceUseError, store::AuthStoreError};
        use graphit_core::auth_document::AuthDocumentError;
        use graphit_daemon::model_service::{MemoryEmbeddingCause, MemoryEmbeddingFailure};
        for (cause, kind) in [
            (AuthDocumentError::Cancelled, io::ErrorKind::Interrupted),
            (AuthDocumentError::Deadline, io::ErrorKind::TimedOut),
            (AuthDocumentError::Denied, io::ErrorKind::Other),
            (AuthDocumentError::Conflict, io::ErrorKind::Other),
        ] {
            let error = memory_embedding_failure_into_io(MemoryEmbeddingFailure {
                cause: MemoryEmbeddingCause::Service(EmbeddingServiceError::Service(
                    ServiceUseError::Store(AuthStoreError::Core(cause)),
                )),
                last_attachment: None,
            });
            assert_eq!(error.kind(), kind);
            let failure = error
                .get_ref()
                .unwrap()
                .downcast_ref::<MemoryEmbeddingFailure<EmbeddingServiceError>>()
                .unwrap();
            assert!(matches!(
                &failure.cause,
                MemoryEmbeddingCause::Service(EmbeddingServiceError::Service(ServiceUseError::Store(AuthStoreError::Core(actual)))) if *actual == cause
            ));
        }
        for kind in [
            io::ErrorKind::Interrupted,
            io::ErrorKind::TimedOut,
            io::ErrorKind::PermissionDenied,
        ] {
            let error = memory_embedding_failure_into_io(MemoryEmbeddingFailure {
                cause: MemoryEmbeddingCause::Store(Box::new(
                    graphit_records::memory_store::MemoryStoreError::Request(kind.into()),
                )),
                last_attachment: None,
            });
            assert_eq!(
                error.kind(),
                if kind == io::ErrorKind::PermissionDenied {
                    io::ErrorKind::Other
                } else {
                    kind
                }
            );
        }
    }

    #[test]
    fn background_embedding_request_keeps_explicit_policy_external_deadline_and_cancel() {
        let brand = Brand::default();
        let setting = ResourceSetting::RequestDeadlineMs.spec();
        let cancellation = Cancellation::default();
        let service = graphit_core::artifact_store::SnapshotContext::service(cancellation.clone());
        let defaults =
            HubResourcePolicy::resolve(&ConfigSnapshot::new(&brand, []).unwrap()).unwrap();
        let foreground_timeout =
            Duration::from_millis(defaults.limit(ResourceSetting::RequestDeadlineMs).value);
        let before_default = Instant::now();
        let background = background_embedding_request(&defaults, &service, &cancellation).unwrap();
        let after_default = Instant::now();
        let default_timeout = Duration::from_secs(900);
        assert!(background.deadline().unwrap() >= before_default + default_timeout);
        assert!(background.deadline().unwrap() <= after_default + default_timeout);
        assert!(background.deadline().unwrap() > Instant::now() + foreground_timeout);
        assert_eq!(service.deadline(), None);

        let configured_ms = setting.default_value * 4;
        let config = ConfigSnapshot::new(
            &brand,
            [ConfigDocument::new(
                &brand,
                ConfigSource::Command,
                [(
                    ConfigKey::new(setting.key).unwrap(),
                    serde_json::json!(configured_ms),
                )],
            )
            .unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let before = Instant::now();
        let request = background_embedding_request(&policy, &service, &cancellation).unwrap();
        let after = Instant::now();
        let configured = Duration::from_millis(configured_ms);
        assert!(request.deadline().unwrap() >= before + configured);
        assert!(request.deadline().unwrap() <= after + configured);
        assert_eq!(service.deadline(), None);

        let external = Instant::now() + Duration::from_secs(1);
        let finite =
            graphit_core::artifact_store::SnapshotContext::new(external, cancellation.clone());
        let bounded = background_embedding_request(&policy, &finite, &cancellation).unwrap();
        assert_eq!(bounded.deadline(), Some(external));
        cancellation.cancel();
        assert_eq!(
            request.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(
            bounded.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(background_embedding_request(&policy, &finite, &cancellation).is_err());
    }
}
