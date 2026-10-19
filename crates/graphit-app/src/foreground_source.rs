//! A real Source service owner; executable routing remains the central caller's job.
use crate::foreground_dream::{ForegroundDream, ForegroundUserDream};
#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_ast_progressive_tests.rs"]
mod ast_progressive_tests;
#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_ast_sources_tests.rs"]
mod ast_source_tests;
#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_dream_tests.rs"]
mod dream_tests;
#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_knowledge_sources/tests.rs"]
mod knowledge_source_tests;
#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_ast_sources/progressive_cohort_tests.rs"]
mod progressive_cohort_tests;
#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_ast_sources/progressive_lifetime_tests.rs"]
mod progressive_lifetime_tests;
#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_ast_sources/scip_pipeline_tests.rs"]
mod scip_pipeline_tests;
use crate::foreground_ast_sources::{ForegroundAstSourceConfig, ForegroundAstSources};
use crate::foreground_memory_jobs::ForegroundMemoryJobs;
use crate::foreground_now::{ForegroundNow, Settings as NowSettings};
use crate::foreground_tasks::{ForegroundTaskConfig, ForegroundTasks};
use crate::source_server::{SourcePolicy, SourceServer};
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits,
    Permission, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceAuthorityInput,
    ServiceOrigin, ServicePurpose, ServiceRuleInput, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, ProfileCredentials, ProfileError, ProfileLimits, ProviderKind, ProviderRecord,
        S3Source,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, CopyPolicy, ScopeLimits, SourceReadLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    config::{ConfigKey, ConfigSnapshot},
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    native_path::MAX_PATH_BYTES,
    project_identity::{ProjectId, ProjectIdentityStore},
    project_inventory::GlobalProjectInventory,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyEffect, RuntimeKeyLimits, RuntimeKeyMutationOutcome},
    tcp_listener::{self, BoundListener, Options, PortSelection},
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    broker_inventory::{InventoryCredentialMode, publish_maintained_inventory},
    broker_verifiers::BrokerVerifierCache,
    project_bootstrap::{ProjectBootstrapInput, ProjectBootstrapLimits, bootstrap_project},
    source_discovery::{self, DiscoveryRecord},
};
use graphit_http::client::{BoundedClient, ClientPolicy, Limits};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
    registry::HubRegistry,
};
use std::{fmt, io, net::IpAddr, num::NonZeroU16, sync::Arc, time::Duration, time::Instant};
use tokio_util::sync::CancellationToken;

fn log_project_job_error(
    module: &str,
    project: &graphit_daemon::supervisor::ProjectContext,
    error: io::Error,
) {
    use std::io::Write;
    // Report the job's actual failure and effects without retrying publication.
    // Cancellation still drains this job and the other service owners.
    let _ = writeln!(
        io::stderr().lock(),
        "Graphit {module} job failed for {} version {}: {error:?}",
        project.id,
        project.version.version(),
    );
}

/// A causal error and any service owner/outcome that still needs explicit cleanup.
/// An authentication startup error retains its own SDK resources inside `error`.
pub struct ForegroundFailure {
    pub error: io::Error,
    pub owner: Option<Box<ForegroundSource>>,
    pub runtime: Option<RuntimeKeyMutationOutcome>,
    pub service: Option<io::Error>,
}
impl fmt::Debug for ForegroundFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ForegroundFailure")
            .field("error", &self.error)
            .field("service", &self.service)
            .field("retained_owner", &self.owner.is_some())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for ForegroundFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}
impl std::error::Error for ForegroundFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl From<io::Error> for ForegroundFailure {
    fn from(error: io::Error) -> Self {
        Self {
            error,
            owner: None,
            runtime: None,
            service: None,
        }
    }
}

struct ProjectControlOwner {
    control: crate::foreground_project_control::ProjectControl,
    requests: tokio::sync::mpsc::Receiver<crate::foreground_project_control::OwnerCommand>,
    installation: InstallationPaths,
    config: ConfigSnapshot,
    environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
}

type DreamActivity = Option<(
    graphit_daemon::supervisor::ProjectContext,
    tokio::sync::watch::Sender<Instant>,
)>;

/// Holds one issuer, singleton and listener for Source and optional Task/browser.
pub struct ForegroundSource {
    project_control: Option<ProjectControlOwner>,
    auth: Arc<DaemonAuth>,
    broker_verifiers: Option<Arc<BrokerVerifierCache>>,
    server: Option<SourceServer>,
    tasks: Option<ForegroundTasks>,
    projects: Option<Arc<Vec<(ProjectPaths, ProjectId, VersionContext)>>>,
    now: Option<ForegroundNow>,
    dream: Option<ForegroundDream>,
    user_dream: Option<ForegroundUserDream>,
    dream_activity: Arc<std::sync::RwLock<DreamActivity>>,
    memory_jobs: Option<ForegroundMemoryJobs>,
    requested_local_rerank: Option<(String, graphit_daemon::model_service::ConfigId)>,
    model_changes: Arc<tokio::sync::Notify>,
    #[cfg(all(test, target_os = "linux"))]
    model_refreshes: tokio::sync::watch::Sender<Option<Result<(), io::ErrorKind>>>,
    #[cfg(all(test, target_os = "linux"))]
    model_child_exits: tokio::sync::watch::Sender<Option<bool>>,
    ast_jobs: Option<Arc<ForegroundAstSources>>,
    #[cfg(all(test, target_os = "linux"))]
    ast_full_completed: Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    external_watcher: Option<graphit_daemon::watcher::ProjectWatcher>,
    #[cfg(all(test, target_os = "linux"))]
    memory_job_resources: std::sync::Weak<crate::foreground_memory_jobs::MemoryJobResources>,
    supervisors: graphit_daemon::supervisor::ProjectSupervisors,
    listener: Option<BoundListener>,
    record: Option<DiscoveryRecord>,
    policy: HubResourcePolicy,
    scope: ScopeLimits,
    native: NativeDocumentFileLimits,
}

fn number(policy: &HubResourcePolicy, setting: ResourceSetting) -> io::Result<usize> {
    usize::try_from(policy.limit(setting).value).map_err(io::Error::other)
}
fn io_error(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(error)
}

const SOURCE_WATCH_DEBOUNCE: Duration = Duration::from_millis(200);

pub(super) fn project_watch_config(
    project: &ProjectPaths,
    config: &ConfigSnapshot,
    original: &RequestContext<'_>,
) -> io::Result<graphit_daemon::watcher::WatchConfig> {
    use graphit_ast::ignore_policy::AstIgnorePolicy;
    use graphit_daemon::watcher::{IgnoreConfig, WatchConfig};
    use graphit_knowledge::ignore_policy::KnowledgeIgnorePolicy;

    original.check()?;
    let ast = AstIgnorePolicy::from_config(config, original.snapshot()).map_err(io_error)?;
    let wiki = KnowledgeIgnorePolicy::resolve(original.snapshot(), config).map_err(io_error)?;
    original.check()?;
    Ok(WatchConfig {
        root: project.root().to_owned(),
        ast: IgnoreConfig {
            filename: Some(ast.filename().into()),
            defaults: ast.defaults().to_vec(),
        },
        wiki: IgnoreConfig {
            filename: Some(wiki.custom_filename().into()),
            defaults: wiki.patterns().to_vec(),
        },
        control_files: vec![
            project
                .identity_file()
                .strip_prefix(project.root())
                .map_err(io_error)?
                .to_owned(),
        ],
        control_directories: vec![crate::ast_profile_loader::queries_directory(
            project, config,
        )?],
        debounce: SOURCE_WATCH_DEBOUNCE,
    })
}

fn auth_snapshot(
    installation: &InstallationPaths,
    config: &ConfigSnapshot,
    original: &RequestContext<'_>,
) -> io::Result<graphit_auth::store::AuthSnapshot> {
    let startup_deadline = original.snapshot().required_deadline()?;
    if config.brand() != installation.brand() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let policy = HubResourcePolicy::resolve(config).map_err(io_error)?;
    let depth = number(&policy, ResourceSetting::SnapshotDepth)?;
    let bytes = number(&policy, ResourceSetting::ContentInputBytes)?;
    let scalar = number(&policy, ResourceSetting::ContentScalarBytes)?;
    let entries = number(&policy, ResourceSetting::ContentCollectionEntries)?;
    let bootstrap = BootstrapLimits::new(MAX_PATH_BYTES, depth).map_err(io_error)?;
    let private = installation_bootstrap::prepare(installation, bootstrap, original.snapshot())
        .into_installation()
        .map_err(io_error)?;
    let identity = InstallationIdentityStore::new(installation)?.ensure(
        {
            original.check()?;
            startup_deadline
                .checked_duration_since(Instant::now())
                .ok_or(io::ErrorKind::TimedOut)?
        },
        original.cancellation(),
    )?;
    private.revalidate(original.snapshot()).map_err(io_error)?;
    let scope = CapturedPublicationScope::installation(
        installation,
        identity.unit_id().clone(),
        [installation.root().to_owned()],
        ScopeLimits::new(1, MAX_PATH_BYTES, depth)?,
        original.snapshot(),
    )?;
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(bytes, MAX_PATH_BYTES, depth, scalar).map_err(io_error)?,
            original.snapshot(),
        )
        .map_err(io_error)?,
        ProfileLimits {
            bytes,
            nodes: number(&policy, ResourceSetting::ContentNodes)?,
            scalar_bytes: scalar,
            collection_entries: entries,
            providers: entries,
            profiles: entries,
        },
        original,
    )
    .map_err(io_error)?;
    store.load(original).map_err(io_error)
}

fn active_provider(
    installation: &InstallationPaths,
    config: &ConfigSnapshot,
    original: &RequestContext<'_>,
) -> io::Result<Option<ProviderRecord>> {
    let snapshot = auth_snapshot(installation, config, original)?;
    if !snapshot.was_present() {
        return Ok(None);
    }
    let provider = match snapshot.document().active(original) {
        Ok((provider, _)) => provider,
        Err(ProfileError::NoActive) => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    Ok(Some(provider.clone()))
}

pub(super) fn inventory_network_limits(policy: &HubResourcePolicy) -> io::Result<Limits> {
    let transport = number(policy, ResourceSetting::TransportBytes)?;
    Ok(Limits {
        request_body_bytes: transport,
        response_body_bytes: transport,
        url_bytes: 8192,
        header_bytes: 65536,
        header_count: 32,
        max_in_flight: number(policy, ResourceSetting::TransportInflight)?,
        max_idle_per_host: 1,
        idle_timeout: Duration::from_secs(30),
        max_certificates: 8,
        max_certificate_bytes: 65536,
    })
}

fn inventory_client(
    provider: &ProviderRecord,
    config: &ConfigSnapshot,
    policy: &HubResourcePolicy,
) -> io::Result<BoundedClient> {
    let endpoint = match &provider.config().kind {
        ProviderKind::Local {
            broker: Some(broker),
            ..
        }
        | ProviderKind::Broker(broker) => &broker.endpoint,
        ProviderKind::Local { broker: None, .. } => return Err(io::ErrorKind::InvalidInput.into()),
    };
    BoundedClient::new(
        ClientPolicy {
            origin: endpoint,
            allow_loopback_http: config
                .get::<bool>(&ConfigKey::new("broker.allow_loopback_http")?)?
                .unwrap_or(false),
            proxy: None,
            additional_roots: &[],
        },
        inventory_network_limits(policy)?,
    )
    .map_err(io_error)
}

fn embedding_client(
    provider: Option<&ProviderRecord>,
    config: &ConfigSnapshot,
    policy: &HubResourcePolicy,
) -> io::Result<Option<Arc<BoundedClient>>> {
    let Some(provider) = provider else {
        return Ok(None);
    };
    provider.config().validate_ai_services().map_err(io_error)?;
    match &provider.config().embedding {
        AiService::Disabled | AiService::Local(_) => Ok(None),
        AiService::Broker => inventory_client(provider, config, policy)
            .map(Arc::new)
            .map(Some),
        AiService::Direct { endpoint, .. } => BoundedClient::new(
            ClientPolicy {
                origin: endpoint,
                allow_loopback_http: config
                    .get::<bool>(&ConfigKey::new("broker.allow_loopback_http")?)?
                    .unwrap_or(false),
                proxy: None,
                additional_roots: &[],
            },
            inventory_network_limits(policy)?,
        )
        .map(Arc::new)
        .map(Some)
        .map_err(io_error),
    }
}

impl ForegroundSource {
    /// Capture native destination inputs before publication, without granting HOME access.
    pub fn with_global_native_paths(
        mut self,
        paths: crate::managed_roles::global_paths::NativeGlobalPaths,
    ) -> Result<Self, ForegroundFailure> {
        let result = self
            .server
            .as_mut()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))
            .and_then(|server| server.attach_global_paths(paths));
        match result {
            Ok(()) => Ok(self),
            Err(error) => Err(ForegroundFailure {
                error,
                owner: Some(Box::new(self)),
                runtime: None,
                service: None,
            }),
        }
    }
    /// Publish configured Broker inventory after startup, before serving requests.
    pub async fn start_and_publish(
        installation: &InstallationPaths,
        inputs: &[ProjectBootstrapInput<'_>],
        current: usize,
        config: &ConfigSnapshot,
        binding: NativeBinding,
        environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
        original: &RequestContext<'_>,
    ) -> Result<Self, ForegroundFailure> {
        if inputs.get(current).is_none() && !(inputs.is_empty() && current == 0) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let provider = active_provider(installation, config, original)?;
        let mut owner = Self::start_with_inventory(
            installation,
            inputs,
            current,
            config,
            binding,
            original,
            provider.as_ref(),
        )?;
        if let Some(home) = installation.home_directory() {
            owner = owner.with_global_native_paths(
                crate::managed_roles::global_paths::NativeGlobalPaths::capture(
                    home,
                    installation.start_directory(),
                    &environment,
                ),
            )?;
        }
        let resources = owner
            .server
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))
            .and_then(|server| server.project_modules(original))
            .and_then(|modules| {
                owner.create_memory_jobs(
                    installation,
                    config,
                    provider.as_ref(),
                    original,
                    &modules,
                )
            });
        match resources {
            Ok(jobs) => owner.memory_jobs = jobs,
            Err(error) => {
                return Err(ForegroundFailure {
                    error,
                    owner: Some(Box::new(owner)),
                    runtime: None,
                    service: None,
                });
            }
        }
        if let Some(provider) = provider.as_ref().filter(|provider| {
            !matches!(
                provider.config().kind,
                ProviderKind::Local { broker: None, .. }
            )
        }) {
            let client = match inventory_client(provider, config, &owner.policy) {
                Ok(client) => client,
                Err(error) => {
                    return Err(ForegroundFailure {
                        error,
                        owner: Some(Box::new(owner)),
                        runtime: None,
                        service: None,
                    });
                }
            };
            let mut refreshed = false;
            let published = loop {
                let published = async {
                    let caller = {
                        let key = owner
                            .auth
                            .runtime()
                            .check(original.snapshot())
                            .map_err(io_error)?;
                        let raw =
                            std::str::from_utf8(key.secret(original.snapshot()).map_err(io_error)?)
                                .map_err(io_error)?;
                        owner
                            .auth
                            .issuer()
                            .authenticate_runtime(raw, original)
                            .map_err(io_error)?
                    };
                    publish_maintained_inventory(
                        &client,
                        &owner.auth,
                        &caller,
                        if matches!(provider.config().kind, ProviderKind::Broker(_)) {
                            InventoryCredentialMode::BrokerNative(
                                owner
                                    .broker_verifiers
                                    .as_deref()
                                    .ok_or(io::ErrorKind::InvalidInput)?,
                            )
                        } else {
                            InventoryCredentialMode::LocalBrokerKey
                        },
                        original,
                        number(&owner.policy, ResourceSetting::ContentInputBytes)?,
                    )
                    .await
                    .map_err(io_error)
                }
                .await;
                match published {
                    Err(error) if !refreshed => {
                        let Some(now) = owner.now.as_mut() else {
                            break Err(error);
                        };
                        if !now.require_inventory_native_refresh(&error) {
                            break Err(error);
                        }
                        let Some(desired) = owner.projects.as_ref() else {
                            break Err(io::ErrorKind::InvalidInput.into());
                        };
                        if let Err(error) = now
                            .refresh_native(Arc::clone(desired), original.cancellation())
                            .await
                        {
                            break Err(error);
                        }
                        refreshed = true;
                    }
                    published => break published,
                }
            };
            if let Err(error) = published {
                return Err(ForegroundFailure {
                    error,
                    owner: Some(Box::new(owner)),
                    runtime: None,
                    service: None,
                });
            }
        }
        if let Some(control) = owner.project_control.as_mut() {
            control.environment = environment.clone();
        }
        if let Some(user) = owner.user_dream.as_mut()
            && let Err(error) = user.start(environment.clone(), original).await
        {
            return Err(ForegroundFailure {
                error,
                owner: Some(Box::new(owner)),
                runtime: None,
                service: None,
            });
        }
        if let Some(dream) = owner.dream.as_mut()
            && let Err(error) = dream.start(environment.clone(), original).await
        {
            return Err(ForegroundFailure {
                error,
                owner: Some(Box::new(owner)),
                runtime: None,
                service: None,
            });
        }
        if let Err(error) = owner
            .attach_jobs(installation, config, binding, &environment, original)
            .await
        {
            return Err(ForegroundFailure {
                error,
                owner: Some(Box::new(owner)),
                runtime: None,
                service: None,
            });
        }
        if let Err(error) = owner.attach_ask(config, environment, original).await {
            return Err(ForegroundFailure {
                error,
                owner: Some(Box::new(owner)),
                runtime: None,
                service: None,
            });
        }
        Ok(owner)
    }

    async fn attach_jobs(
        &mut self,
        installation: &InstallationPaths,
        config: &ConfigSnapshot,
        binding: NativeBinding,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        use graphit_daemon::supervisor::{DesiredProject, ProjectContext, ProjectSupervisor};

        if self.ast_jobs.is_none() {
            self.ast_jobs = Some(Arc::new(ForegroundAstSources::new(
                Arc::clone(&self.auth),
                ForegroundAstSourceConfig {
                    live: self
                        .server
                        .as_ref()
                        .ok_or(io::ErrorKind::NotConnected)?
                        .graph_server()
                        .live_ast(),
                    installation: installation.clone(),
                    base: config.clone(),
                    policy: self.policy.clone(),
                    scope: self.scope,
                    native: self.native,
                    binding,
                    environment: environment.to_vec(),
                },
                original.snapshot(),
            )?));
        }
        let projects = Arc::clone(self.projects.as_ref().ok_or(io::ErrorKind::InvalidInput)?);
        let modules = self
            .server
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .project_modules(original)?;
        if self
            .memory_jobs
            .as_ref()
            .is_some_and(|jobs| jobs.child.is_some())
            || self
                .server
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .semantic_model()?
                .is_some()
        {
            self.refresh_model_selection(installation, config, environment, original)
                .await?;
        } else if self.memory_jobs.is_none() {
            let provider = active_provider(installation, config, original)?;
            self.memory_jobs = self.create_memory_jobs(
                installation,
                config,
                provider.as_ref(),
                original,
                &modules,
            )?;
        }
        let dream = self.dream.as_ref();
        let active_dream = projects.iter().find_map(|(_, id, version)| {
            let context = graphit_daemon::supervisor::ProjectContext {
                id: *id,
                version: version.clone(),
            };
            dream
                .and_then(|dream| dream.activity_sender(&context))
                .map(|activity| (context, activity))
        });
        *self
            .dream_activity
            .write()
            .map_err(|_| io::Error::other("Dream activity state poisoned"))? = active_dream;
        let ast_resources = self.ast_jobs.clone();
        if self.tasks.is_some() && self.external_watcher.is_none() {
            self.external_watcher = Some(graphit_daemon::watcher::ProjectWatcher::start_external(
                graphit_daemon::watcher::ExternalControls {
                    files: vec![graphit_core::config::ConfigStore::global(installation)?.path()],
                    ..Default::default()
                },
                SOURCE_WATCH_DEBOUNCE,
            )?);
        }
        let mut desired = Vec::with_capacity(projects.len());
        let mut watches = Vec::with_capacity(projects.len());
        for (index, (paths, id, version)) in projects.iter().enumerate() {
            let context = ProjectContext {
                id: *id,
                version: version.clone(),
            };
            let memory = self.memory_jobs.is_some()
                && modules[index].enabled(Module::Embedding)
                && (modules[index].enabled(Module::Memory) || modules[index].enabled(Module::Task));
            // Keep the same supervisor observing explicit config controls, even if Ast starts disabled.
            let active = ast_resources.is_some()
                || memory
                || dream
                    .and_then(|owner| owner.activity_sender(&context))
                    .is_some();
            watches.push(if active {
                let fresh = crate::ast_profile_loader::project_config(
                    installation,
                    paths,
                    config,
                    original.snapshot(),
                )?;
                Some(project_watch_config(paths, &fresh, original)?)
            } else {
                None
            });
            if active {
                desired.push(DesiredProject {
                    context,
                    active: true,
                });
            }
        }
        let previous_model = self
            .server
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .semantic_model()?;
        let model_started = previous_model.is_none() && self.memory_jobs.is_some();
        let resources = match previous_model {
            Some(resources) => Some(resources),
            None => match self.memory_jobs.as_mut() {
                Some(memory) => {
                    memory
                        .start(
                            installation,
                            environment.to_vec(),
                            self.scope,
                            self.native,
                            binding,
                            original,
                        )
                        .await?
                }
                None => None,
            },
        };
        self.publish_semantic_model(resources.clone())?;
        if let Some(tasks) = &self.tasks {
            tasks.attach_project_configuration(
                self.server
                    .as_ref()
                    .ok_or(io::ErrorKind::NotConnected)?
                    .project_configuration()?,
            )?;
        }
        let semantic = self
            .server
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .semantic_slot();
        let base = Arc::new(config.clone());
        let installation = Arc::new(installation.clone());
        let knowledge = Arc::new(
            crate::foreground_knowledge_sources::ForegroundKnowledgeSources::new(
                Arc::clone(&self.auth),
                crate::foreground_knowledge_sources::KnowledgeSourceConfig {
                    installation: Arc::clone(&installation),
                    base: Arc::clone(&base),
                    policy: self.policy.clone(),
                    scope: self.scope,
                    native: self.native,
                    binding,
                },
            ),
        );
        let model_changes = Arc::clone(&self.model_changes);
        let content = self.policy.content();
        let wake_on_model_start = if model_started {
            self.supervisors
                .supervised()
                .filter(|context| {
                    projects.iter().enumerate().any(|(i, (_, id, version))| {
                        *id == context.id
                            && version == &context.version
                            && modules[i].enabled(Module::Embedding)
                            && (modules[i].enabled(Module::Ast)
                                || modules[i].enabled(Module::Memory)
                                || modules[i].enabled(Module::Task)
                                || modules[i].enabled(Module::Knowledge))
                    })
                })
                .filter_map(|context| self.supervisors.rescan_handle(&context.key()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        #[cfg(all(test, target_os = "linux"))]
        let ast_full_completed = Arc::clone(&self.ast_full_completed);
        self.supervisors
            .reconcile(desired, |context| {
                let index = projects.iter().position(|(paths, id, version)| {
                    *id == context.id
                        && paths.root() == context.root()
                        && version == &context.version
                });
                let watch = index
                    .and_then(|i| watches.get(i))
                    .and_then(Option::as_ref)
                    .cloned();
                let project = index.map(|i| Arc::new(projects[i].0.clone()));
                let semantic = Arc::clone(&semantic);
                let model_changes = Arc::clone(&model_changes);
                let base = Arc::clone(&base);
                let installation = Arc::clone(&installation);
                let ast_resources = ast_resources.clone();
                #[cfg(all(test, target_os = "linux"))]
                let ast_full_completed = Arc::clone(&ast_full_completed);
                let knowledge = Arc::clone(&knowledge);
                async move {
                    let project = project.ok_or(io::ErrorKind::InvalidInput)?;
                    let external = ast_resources
                        .as_ref()
                        .map(|ast| ast.external_controls())
                        .transpose()?
                        .unwrap_or_default();
                    let mut initial = true;
                    let mut observed_model = None::<std::sync::Weak<crate::foreground_memory_jobs::MemoryJobResources>>;
                    let raw_full_pending = Arc::new(std::sync::atomic::AtomicBool::new(true));
                    let ast_model_refresh_pending = Arc::new(std::sync::atomic::AtomicBool::new(true));
                    let semantic_pending = Arc::new(std::sync::Mutex::new(None::<std::collections::BTreeSet<String>>));
                    let knowledge_pending = Arc::new(std::sync::atomic::AtomicBool::new(true));
                    let knowledge_source_pending = Arc::new(std::sync::Mutex::new(None::<std::collections::BTreeSet<std::path::PathBuf>>));
                    let memory_pending = Arc::new(std::sync::atomic::AtomicBool::new(true));
                    let task_pending = Arc::new(std::sync::atomic::AtomicBool::new(true));
                    ProjectSupervisor::start_controlled(
                        context,
                        watch.ok_or(io::ErrorKind::InvalidInput)?,
                        external,
                        move |selected, mut batch, cancellation, controls, background_cancellation, source_preemption| {
                            let first = std::mem::replace(&mut initial, false);
                            // Model readiness/reload resumes durable projections without
                            // reparsing intact source. Do not retain the previous model.
                            let resources = semantic
                                .read()
                                .map_err(|_| io::Error::other("semantic model state poisoned"))
                                .map(|resources| resources.clone());
                            let semantic_refresh = first || batch.controls_changed;
                            // Knowledge consumes the model/config event once;
                            // its own pending flag controls semantic retries.
                            let mut model_refresh = batch.controls_changed;
                            if let Ok(resources) = &resources {
                                let current_model = resources.as_ref().map(Arc::downgrade);
                                let model_changed = match (&observed_model, &current_model) {
                                    (Some(previous), Some(current)) => !std::sync::Weak::ptr_eq(previous, current),
                                    (None, None) => false,
                                    _ => true,
                                };
                                observed_model = current_model;
                                model_refresh |= model_changed;
                            }
                            if model_refresh {
                                ast_model_refresh_pending.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                            let ast_model_refresh = ast_model_refresh_pending.load(std::sync::atomic::Ordering::Relaxed);
                            let ast_model_refresh_pending = Arc::clone(&ast_model_refresh_pending);
                            let project = Arc::clone(&project);
                            let model_changes = Arc::clone(&model_changes);
                            let base = Arc::clone(&base);
                            let installation = Arc::clone(&installation);
                            let ast_resources = ast_resources.clone();
                            #[cfg(all(test, target_os = "linux"))]
                            let ast_full_completed = Arc::clone(&ast_full_completed);
                            let knowledge = Arc::clone(&knowledge);
                            let raw_full_pending = Arc::clone(&raw_full_pending);
                            let semantic_pending = Arc::clone(&semantic_pending);
                            let knowledge_pending = Arc::clone(&knowledge_pending);
                            let knowledge_source_pending = Arc::clone(&knowledge_source_pending);
                            let memory_pending = Arc::clone(&memory_pending);
                            let task_pending = Arc::clone(&task_pending);
                            async move {
                                // The supervisor retains and joins this batch, including cancellation.
                                // Async producers own their native boundaries; do not hold an
                                // outer blocking worker while awaiting their work or model RPC.
                                if cancellation.is_cancelled() {
                                    return Ok(());
                                }
                                if semantic_refresh {
                                    *semantic_pending.lock().map_err(|_| io::Error::other("AST pending FILEs poisoned"))? = None;
                                }
                                let rescan = batch.rescan;
                                if rescan {
                                    raw_full_pending.store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                let resources = resources?;
                                // A watcher failure cannot become a healthy selection in any module.
                                if let Some(error) = batch.error.take() {
                                    log_project_job_error("Source watch", &selected, error);
                                    return Ok(());
                                }
                                if batch.controls_changed {
                                    model_changes.notify_one();
                                }
                                let knowledge_resume_semantics = model_refresh
                                    || knowledge_pending.load(std::sync::atomic::Ordering::Relaxed);
                                if rescan || batch.controls_changed || batch.records_changed {
                                    memory_pending.store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                if rescan || model_refresh || batch.records_changed {
                                    task_pending.store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                let (knowledge_refresh, knowledge_paths) = {
                                    let mut pending = knowledge_source_pending.lock()
                                        .map_err(|_| io::Error::other("Knowledge pending FILEs poisoned"))?;
                                    if first || rescan || batch.controls_changed {
                                        *pending = None;
                                    }
                                    match pending.as_mut() {
                                        None => (true, batch.paths.clone()),
                                        Some(paths) => {
                                            paths.extend(batch.paths.iter().cloned());
                                            (false, paths.clone())
                                        }
                                    }
                                };
                                // Keep the SDK-heavy branch futures out of the joined batch's
                                // stack frame while retaining both until their owned work joins.
                                let records_only = batch.records_changed && !first && !rescan && !batch.controls_changed
                                    && batch.paths.is_empty();
                                let ast_semantic = Box::pin(async {
                                    if let Some(ast) = ast_resources {
                                        #[cfg(all(test, target_os = "linux"))]
                                        if !records_only { ast.observe_raw_batch(&batch); }
                                        let explicit_delta = !batch.paths.is_empty()
                                            && !batch.paths.contains(project.root())
                                            && !batch.controls_changed;
                                        if explicit_delta {
                                            // A queued full pass must not turn a concrete FILE
                                            // event into another broad enumeration before it.
                                            batch.rescan = false;
                                        } else if !records_only {
                                            batch.rescan |= raw_full_pending.load(std::sync::atomic::Ordering::Relaxed);
                                        }
                                        let mut refresh_model = ast_model_refresh;
                                        loop {
                                            let full_raw = batch.rescan || batch.controls_changed
                                                || batch.paths.contains(project.root());
                                            let delta_paths = if full_raw {
                                                std::collections::BTreeSet::new()
                                            } else {
                                                batch.paths.clone()
                                            };
                                            if batch.rescan {
                                                let owner = Arc::clone(&ast);
                                                let project = Arc::clone(&project);
                                                let cancellation = cancellation.clone();
                                                let controls = controls.clone();
                                                let refreshed = tokio::task::spawn_blocking(move || {
                                                    owner.refresh_controls(project.as_ref(), &cancellation, &controls)
                                                }).await.map_err(io::Error::other)?;
                                                if let Err(error) = refreshed {
                                                    // A missing/revoked checkout is a failed source
                                                    // admission, not a watcher join failure. Keep the
                                                    // full pass pending for a later real source event.
                                                    log_project_job_error("AST controls", &selected, error);
                                                    break;
                                                }
                                            }
                                            let selection = if !delta_paths.is_empty() {
                                                // Raw already publishes syntax for these FILEs.
                                                // Admit its remaining native-only work without
                                                // replaying syntax or an unrelated earlier tail.
                                                Some(std::collections::BTreeSet::new())
                                            } else {
                                                semantic_pending.lock()
                                                    .map_err(|_| io::Error::other("AST pending FILEs poisoned"))?.clone()
                                            };
                                            match ast.index_productive(project.as_ref(), &selected, batch,
                                                resources.clone(), selection, refresh_model,
                                                &source_preemption, &background_cancellation, &cancellation).await {
                                                Ok(progress) => {
                                                    if progress.model_reconciled {
                                                        refresh_model = false;
                                                        ast_model_refresh_pending.store(false, std::sync::atomic::Ordering::Relaxed);
                                                    }
                                                    if full_raw {
                                                        raw_full_pending.store(false, std::sync::atomic::Ordering::Relaxed);
                                                        #[cfg(all(test, target_os = "linux"))]
                                                        if let Some(sender) = ast_full_completed.lock().unwrap().take() {
                                                            let _ = sender.send(());
                                                        }
                                                    }
                                                    if !cancellation.is_cancelled() {
                                                        let mut pending = semantic_pending.lock()
                                                            .map_err(|_| io::Error::other("AST pending FILEs poisoned"))?;
                                                        if delta_paths.is_empty() {
                                                            *pending = Some(progress.pending_files);
                                                        } else if let Some(pending) = pending.as_mut() {
                                                            pending.retain(|path| !delta_paths.iter().any(|changed|
                                                                project.root().join(path).starts_with(changed)));
                                                            pending.extend(progress.pending_files);
                                                        }
                                                        // None retains an unbounded earlier tail. Only a
                                                        // successful full admission can replace that state.
                                                    }
                                                    if let Some(version) = progress.version {
                                                        eprintln!("Graphit AST job for {} version {:?}: generation {version} published",
                                                            selected.id, selected.version.version());
                                                    }
                                                },
                                                Err(error) => {
                                                    if crate::foreground_ast_sources::semantic::inference_model_reconciled(&error) {
                                                        ast_model_refresh_pending.store(false, std::sync::atomic::Ordering::Relaxed);
                                                    }
                                                    let raw_failed = error.get_ref().is_some_and(|cause|
                                                        cause.is::<crate::foreground_ast_sources::RawPublicationFailure>());
                                                    if full_raw || raw_failed {
                                                        raw_full_pending.store(raw_failed, std::sync::atomic::Ordering::Relaxed);
                                                    }
                                                    if !cancellation.is_cancelled() {
                                                        let incoming = crate::foreground_ast_sources::semantic::inference_pending_files(&error).cloned();
                                                        let mut pending = semantic_pending.lock()
                                                            .map_err(|_| io::Error::other("AST pending FILEs poisoned"))?;
                                                        if delta_paths.is_empty() || incoming.is_none() {
                                                            // An error before typed pending metadata cannot
                                                            // prove this delta's new FILE work is covered.
                                                            *pending = incoming;
                                                        } else if let (Some(pending), Some(incoming)) = (pending.as_mut(), incoming) {
                                                            pending.retain(|path| !delta_paths.iter().any(|changed|
                                                                project.root().join(path).starts_with(changed)));
                                                            pending.extend(incoming);
                                                        }
                                                    }
                                                    log_project_job_error(if raw_failed { "AST Raw" } else { "AST semantic" }, &selected, error);
                                                    break;
                                                },
                                            }
                                            if delta_paths.is_empty()
                                                || !raw_full_pending.load(std::sync::atomic::Ordering::Relaxed)
                                                || source_preemption.is_cancelled()
                                                || cancellation.is_cancelled() {
                                                break;
                                            }
                                            // Resume the retained full pass only after the explicit
                                            // FILEs are queryable. A newer event can yield this tail
                                            // between FILEs; the supervisor retains its queued delta.
                                            batch = graphit_daemon::watcher::ChangeBatch {
                                                rescan: true, ..Default::default()
                                            };
                                        }
                                    }
                                    Ok::<(), io::Error>(())
                                });
                                let knowledge_publication = Box::pin(async {
                                    let no_source_changes = std::collections::BTreeSet::new();
                                    let result = knowledge.index(
                                        project.as_ref(), &selected,
                                        knowledge_refresh && !records_only, knowledge_resume_semantics,
                                        if records_only { &no_source_changes } else { &knowledge_paths }, &source_preemption, &background_cancellation, &cancellation,
                                        resources.as_ref(),
                                    ).await;
                                    let source_pending = match &result {
                                        Ok(_) => Some(std::collections::BTreeSet::new()),
                                        Err(error) if crate::foreground_knowledge_sources::only_knowledge_semantics_failed(error) => {
                                            Some(std::collections::BTreeSet::new())
                                        },
                                        Err(error) => crate::foreground_knowledge_sources::knowledge_source_pending_paths(error)
                                            .map(|paths| paths.iter().map(|path| project.root().join(path)).collect()),
                                    };
                                    if !records_only {
                                        *knowledge_source_pending.lock()
                                            .map_err(|_| io::Error::other("Knowledge pending FILEs poisoned"))? = source_pending;
                                    }
                                    knowledge_pending.store(result.is_err(), std::sync::atomic::Ordering::Relaxed);
                                    match result {
                                        Ok(Some(publication)) => {
                                            use std::io::Write;
                                            let _ = writeln!(io::stderr().lock(),
                                                "Graphit Knowledge job for {} version {}: {publication}",
                                                selected.id, selected.version.version());
                                        }
                                        Ok(None) => (),
                                        Err(error) if error.kind() == io::ErrorKind::Interrupted
                                            && background_cancellation.is_cancelled()
                                            && !cancellation.is_cancelled() => (),
                                        Err(error) => log_project_job_error(
                                            "Knowledge", &selected, error,
                                        ),
                                    }
                                    Ok::<(), io::Error>(())
                                });
                                let memory_embeddings = Box::pin(async {
                                    if cancellation.is_cancelled() { return Ok::<(), io::Error>(()); }
                                let memory = if memory_pending.load(std::sync::atomic::Ordering::Relaxed)
                                    && resources.is_some() {
                                    let installation = Arc::clone(&installation);
                                    let project = Arc::clone(&project);
                                    let base = Arc::clone(&base);
                                    let cancellation = cancellation.clone();
                                    let configured = tokio::task::spawn_blocking(move || {
                                        let original = RequestContext::service(content, &cancellation)
                                            .map_err(io_error)?;
                                        let fresh = crate::ast_profile_loader::project_config(
                                            installation.as_ref(),
                                            project.as_ref(),
                                            base.as_ref(),
                                            original.snapshot(),
                                        )?;
                                        let modules =
                                            graphit_core::module_settings::ModuleSettings::resolve(
                                                &fresh,
                                            )?;
                                        Ok::<_, io::Error>(modules.enabled(Module::Embedding)
                                            && modules.enabled(Module::Memory))
                                    }).await.map_err(io::Error::other)?;
                                    match configured {
                                        Ok(enabled) => enabled,
                                        Err(error) => {
                                            log_project_job_error("Memory configuration", &selected, error);
                                            return Ok(());
                                        }
                                    }
                                } else {
                                    false
                                };
                                if memory && !background_cancellation.is_cancelled()
                                    && let Some(resources) = resources.as_ref()
                                {
                                    match resources.index(project.as_ref(), &selected, &background_cancellation).await {
                                        Ok(_) => memory_pending.store(false, std::sync::atomic::Ordering::Relaxed),
                                        Err(error) if error.kind() == io::ErrorKind::Interrupted
                                            && background_cancellation.is_cancelled()
                                            && !cancellation.is_cancelled() => (),
                                        Err(error) => log_project_job_error("Memory", &selected, error),
                                    }
                                }
                                    Ok::<(), io::Error>(())
                                });
                                let task_embeddings = Box::pin(async {
                                    if cancellation.is_cancelled() { return Ok::<(), io::Error>(()); }
                                if task_pending.load(std::sync::atomic::Ordering::Relaxed)
                                    && !background_cancellation.is_cancelled()
                                    && let Some(resources) = resources.as_ref()
                                {
                                    let original = RequestContext::service(content, &background_cancellation).map_err(io_error)?;
                                    let configured = crate::ast_profile_loader::project_config(installation.as_ref(), project.as_ref(), base.as_ref(), original.snapshot())
                                        .and_then(|fresh| graphit_core::module_settings::ModuleSettings::resolve(&fresh));
                                    let modules = match configured {
                                        Ok(modules) => modules,
                                        Err(error) => {
                                            log_project_job_error("Task embeddings configuration", &selected, error);
                                            return Ok(());
                                        }
                                    };
                                    if modules.enabled(Module::Embedding) && modules.enabled(Module::Task) {
                                        match resources.index_tasks(project.as_ref(), &selected, &background_cancellation).await {
                                            Ok(()) => task_pending.store(false, std::sync::atomic::Ordering::Relaxed),
                                            Err(error) if error.kind() == io::ErrorKind::Interrupted && background_cancellation.is_cancelled() && !cancellation.is_cancelled() => (),
                                            Err(error) => log_project_job_error("Task embeddings", &selected, error),
                                        }
                                    }
                                }
                                    Ok::<(), io::Error>(())
                                });
                                // Each module publishes independently; a long source
                                // embedding tail cannot postpone record embeddings.
                                let (ast_result, knowledge_result, memory_result, task_result) = tokio::join!(
                                    ast_semantic, knowledge_publication, memory_embeddings, task_embeddings);
                                ast_result?;
                                knowledge_result?;
                                memory_result?;
                                task_result?;
                                Ok::<(), io::Error>(())
                            }
                        },
                    )
                    .await
                }
            })
            .await?;
        for handle in wake_on_model_start {
            if !handle.request_rescan() {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
        }
        // Lifecycle rescan belongs to the project supervisor, even without a model.
        // Each retained worker selects its enabled modules when handling the request.
        let handles = self
            .supervisors
            .supervised()
            .filter(|context| {
                projects.iter().any(|(paths, id, version)| {
                    *id == context.id
                        && paths.root() == context.root()
                        && version == &context.version
                })
            })
            .filter_map(|context| self.supervisors.rescan_handle(&context.key()))
            .collect::<Vec<_>>();
        let handles: Arc<[graphit_daemon::supervisor::ProjectRescanHandle]> = Arc::from(handles);
        let server = self.server.as_mut().ok_or(io::ErrorKind::NotConnected)?;
        server.task_server()?.attach_rescans(Arc::clone(&handles))?;
        server.attach_memory_rescans(handles)
    }

    async fn refresh_models(&mut self, cancellation: &Cancellation) -> io::Result<()> {
        let control = self
            .project_control
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?;
        let installation = control.installation.clone();
        let base = control.config.clone();
        let environment = control.environment.clone();
        let policy = self.policy.clone();
        let original = policy
            .request_context(Instant::now(), cancellation)
            .map_err(io_error)?;
        self.refresh_model_selection(&installation, &base, &environment, &original)
            .await
    }

    async fn refresh_model_selection(
        &mut self,
        installation: &InstallationPaths,
        base: &ConfigSnapshot,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        let server = self.server.as_ref().ok_or(io::ErrorKind::NotConnected)?;
        let binding = server.policy().binding;
        let modules = server.project_modules(original)?;
        let provider = active_provider(installation, base, original)?;
        let embedding =
            self.embedding_requested(installation, base, provider.as_ref(), original, &modules)?;
        let rerank = self.requested_rerank_config(installation, provider.as_ref(), original)?;
        if !embedding && rerank.is_none() {
            return self.retire_unused_model(original).await;
        }
        let local_embedding = embedding
            && provider.as_ref().is_some_and(|provider| {
                matches!(
                    (&provider.config().kind, &provider.config().embedding),
                    (ProviderKind::Local { .. }, AiService::Local(_))
                )
            });
        let resolved = if local_embedding {
            Some(crate::foreground_memory_jobs::resolve_global_models(
                installation,
                provider.as_ref(),
                original.snapshot(),
            )?)
        } else {
            None
        };
        let mut config_ids = resolved.as_ref().map(|resolved| resolved.config_ids());
        if let Some(config) = rerank {
            config_ids
                .get_or_insert_with(Default::default)
                .insert(crate::models::DEFAULT_RERANK_SLOT.to_owned(), config);
        }
        if !embedding {
            self.publish_semantic_model(None)?;
            if let Some(jobs) = &mut self.memory_jobs {
                if let Some(config_ids) = config_ids {
                    jobs.refresh(original.snapshot(), config_ids).await?;
                }
                jobs.replace_selection(None, false, None);
            }
            return Ok(());
        }
        let expected = resolved
            .as_ref()
            .filter(|_| local_embedding)
            .map(|resolved| {
                crate::foreground_memory_jobs::expected_for_provider(resolved, provider.as_ref())
            })
            .transpose()?
            .flatten();
        if local_embedding && expected.is_none() {
            if let Some(jobs) = &mut self.memory_jobs {
                // Removing the embedding slot must reach the owned service too.
                // Preserve any independently selected rerank slot in the real map.
                if let Some(config_ids) = config_ids {
                    jobs.refresh(original.snapshot(), config_ids).await?;
                }
                jobs.replace_selection(None, false, None);
            }
            return self.publish_semantic_model(None);
        }
        let http = embedding_client(provider.as_ref(), base, &self.policy)?;
        if !local_embedding
            && rerank.is_none()
            && self
                .memory_jobs
                .as_ref()
                .is_some_and(|jobs| !jobs.retirement_complete())
        {
            if self
                .server
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .semantic_model()?
                .is_some_and(|resources| resources.local.is_some())
            {
                self.publish_semantic_model(None)?;
            }
            if let Some(jobs) = self.memory_jobs.as_mut() {
                jobs.refresh(original.snapshot(), std::collections::BTreeMap::new())
                    .await?;
                jobs.shutdown().await?;
            }
            self.memory_jobs = None;
            self.requested_local_rerank = None;
        }
        if self.memory_jobs.is_none() {
            self.memory_jobs = Some(ForegroundMemoryJobs::new(
                Arc::clone(&self.auth),
                self.policy.clone(),
                None,
                embedding,
                http.clone(),
                Arc::clone(
                    self.broker_verifiers
                        .as_ref()
                        .ok_or(io::ErrorKind::NotConnected)?,
                ),
            )?);
        }
        let jobs = self
            .memory_jobs
            .as_mut()
            .ok_or(io::ErrorKind::NotConnected)?;
        if let Some(config_ids) = config_ids
            && let Err(error) = jobs.refresh(original.snapshot(), config_ids).await
        {
            use graphit_daemon::model_service::{ModelErrorKind, ModelServiceError};
            if local_embedding
                && error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<ModelServiceError>())
                    .is_some_and(|cause| {
                        matches!(
                            cause.kind(),
                            ModelErrorKind::Transport
                                | ModelErrorKind::Cancelled
                                | ModelErrorKind::Deadline
                                | ModelErrorKind::Authentication
                        )
                    })
            {
                self.publish_semantic_model(None)?;
            }
            return Err(error);
        }
        jobs.replace_selection(expected, embedding, http);
        let resources = jobs
            .start(
                installation,
                environment.to_vec(),
                self.scope,
                self.native,
                binding,
                original,
            )
            .await?;
        self.publish_semantic_model(resources)?;
        for context in self.supervisors.supervised() {
            if let Some(handle) = self.supervisors.rescan_handle(&context.key())
                && !handle.request_rescan()
            {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
        }
        Ok(())
    }

    fn embedding_requested(
        &self,
        installation: &InstallationPaths,
        base: &ConfigSnapshot,
        provider: Option<&ProviderRecord>,
        original: &RequestContext<'_>,
        modules: &[graphit_core::module_settings::ModuleSettings],
    ) -> io::Result<bool> {
        if !provider
            .is_some_and(|provider| !matches!(provider.config().embedding, AiService::Disabled))
        {
            return Ok(false);
        }
        // Source serves authenticated USER Memory independently of PROJECT
        // selection or the optional Browser. Configuration enables resources;
        // the consumer still obtains its real USER admission and service lease.
        original.check()?;
        let layers = [
            graphit_core::config::ConfigSource::Command,
            graphit_core::config::ConfigSource::Environment,
            graphit_core::config::ConfigSource::Distribution,
        ]
        .into_iter()
        .filter_map(|kind| base.layer(kind).cloned())
        .chain([graphit_core::config::ConfigStore::global(installation)?.load()?]);
        let global = ConfigSnapshot::new(base.brand(), layers)?;
        let global = graphit_core::module_settings::ModuleSettings::resolve(&global)?;
        original.check()?;
        let user = global.enabled(Module::Memory) && global.enabled(Module::Embedding);
        Ok(user || crate::foreground_memory_jobs::embedding_enabled(modules))
    }

    fn requested_rerank_config(
        &mut self,
        installation: &InstallationPaths,
        provider: Option<&ProviderRecord>,
        original: &RequestContext<'_>,
    ) -> io::Result<Option<graphit_daemon::model_service::ConfigId>> {
        if !self
            .memory_jobs
            .as_ref()
            .is_some_and(|jobs| jobs.child.is_some())
        {
            self.requested_local_rerank = None;
            return Ok(None);
        }
        let Some((requested_provider, _)) = &self.requested_local_rerank else {
            return Ok(None);
        };
        if !provider.is_some_and(|provider| {
            provider.name() == requested_provider
                && matches!(
                    (&provider.config().kind, &provider.config().rerank),
                    (ProviderKind::Local { .. }, AiService::Local(_))
                )
        }) {
            self.requested_local_rerank = None;
            return Ok(None);
        }
        // Only an earlier admitted acquisition makes this a live demand. Capture
        // fresh authority/configuration; the retained identity is never a grant.
        let runtime = self
            .auth
            .runtime()
            .check(original.snapshot())
            .map_err(io_error)?;
        let credential =
            std::str::from_utf8(runtime.secret(original.snapshot()).map_err(io_error)?)
                .map_err(io_error)?;
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(credential, original)
            .map_err(io_error)?;
        let mut service = self
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::Rerank)
            .map_err(io_error)?;
        let lease = service.local().map_err(io_error)?;
        match lease.binding().map_err(io_error)? {
            graphit_auth::ServiceBindingView::Local {
                provider,
                purpose: ServicePurpose::Rerank,
                ..
            } if provider == requested_provider => {}
            _ => return Err(io::ErrorKind::PermissionDenied.into()),
        }
        let Some(expected) =
            crate::foreground_memory_jobs::expected_local_model(installation, &lease)?
        else {
            self.requested_local_rerank = None;
            return Ok(None);
        };
        lease.check().map_err(io_error)?;
        original.check()?;
        let selected = (requested_provider.clone(), expected.config_id());
        self.requested_local_rerank = Some(selected.clone());
        Ok(Some(selected.1))
    }

    fn create_memory_jobs(
        &self,
        installation: &InstallationPaths,
        config: &ConfigSnapshot,
        provider: Option<&ProviderRecord>,
        original: &RequestContext<'_>,
        modules: &[graphit_core::module_settings::ModuleSettings],
    ) -> io::Result<Option<ForegroundMemoryJobs>> {
        if !self.embedding_requested(installation, config, provider, original, modules)? {
            return Ok(None);
        }
        let expected =
            self.capture_model_selection(installation, config, provider, original, modules)?;
        if expected.is_none()
            && provider.is_some_and(|provider| {
                matches!(
                    (&provider.config().kind, &provider.config().embedding),
                    (ProviderKind::Local { .. }, AiService::Local(_))
                )
            })
        {
            return Ok(None);
        }
        let http = embedding_client(provider, config, &self.policy)?;
        ForegroundMemoryJobs::new(
            Arc::clone(&self.auth),
            self.policy.clone(),
            expected,
            true,
            http,
            Arc::clone(
                self.broker_verifiers
                    .as_ref()
                    .ok_or(io::ErrorKind::NotConnected)?,
            ),
        )
        .map(Some)
    }

    fn capture_model_selection(
        &self,
        installation: &InstallationPaths,
        config: &ConfigSnapshot,
        provider: Option<&graphit_auth::profile::ProviderRecord>,
        original: &RequestContext<'_>,
        modules: &[graphit_core::module_settings::ModuleSettings],
    ) -> io::Result<Option<graphit_daemon::model_service::ExpectedModel>> {
        if !self.embedding_requested(installation, config, provider, original, modules)? {
            return Ok(None);
        }
        crate::foreground_memory_jobs::expected_selection(
            installation,
            provider,
            original.snapshot(),
        )
    }

    async fn retire_unused_model(&mut self, original: &RequestContext<'_>) -> io::Result<()> {
        let modules = self
            .server
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .project_modules(original)?;
        let control = self
            .project_control
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?;
        let installation = control.installation.clone();
        let config = control.config.clone();
        let provider = active_provider(&installation, &config, original)?;
        let embedding = self.embedding_requested(
            &installation,
            &config,
            provider.as_ref(),
            original,
            &modules,
        )?;
        if self.memory_jobs.is_none() {
            self.requested_local_rerank = None;
            return if embedding {
                Ok(())
            } else {
                self.publish_semantic_model(None)
            };
        }
        let rerank = self.requested_rerank_config(&installation, provider.as_ref(), original)?;
        if embedding {
            if !provider.as_ref().is_some_and(|provider| {
                matches!(
                    (&provider.config().kind, &provider.config().embedding),
                    (ProviderKind::Local { .. }, AiService::Local(_))
                )
            }) && rerank.is_none()
                && self
                    .memory_jobs
                    .as_ref()
                    .is_some_and(|jobs| !jobs.retirement_complete())
            {
                let http = embedding_client(provider.as_ref(), &config, &self.policy)?;
                let binding = self.server.as_ref().unwrap().policy().binding;
                if self
                    .server
                    .as_ref()
                    .unwrap()
                    .semantic_model()?
                    .is_some_and(|resources| resources.local.is_some())
                {
                    self.publish_semantic_model(None)?;
                }
                let jobs = self.memory_jobs.as_mut().unwrap();
                jobs.refresh(original.snapshot(), std::collections::BTreeMap::new())
                    .await?;
                jobs.shutdown().await?;
                jobs.replace_selection(None, true, http);
                let resources = jobs.resources(self.scope, self.native, binding)?;
                self.publish_semantic_model(resources)?;
            }
            return Ok(());
        }
        self.publish_semantic_model(None)?;
        if let Some(config) = rerank {
            if let Some(jobs) = &mut self.memory_jobs {
                jobs.refresh(
                    original.snapshot(),
                    std::collections::BTreeMap::from([(
                        crate::models::DEFAULT_RERANK_SLOT.to_owned(),
                        config,
                    )]),
                )
                .await?;
                jobs.replace_selection(None, false, None);
            }
            return Ok(());
        }
        if let Some(memory) = self.memory_jobs.as_mut() {
            memory.shutdown().await?;
        }
        self.memory_jobs = None;
        self.requested_local_rerank = None;
        Ok(())
    }

    fn publish_semantic_model(
        &mut self,
        resources: Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
    ) -> io::Result<()> {
        self.server
            .as_mut()
            .ok_or(io::ErrorKind::NotConnected)?
            .attach_semantic_model(resources.clone())?;
        if let Some(tasks) = &self.tasks {
            tasks.attach_memory_model(resources.clone())?;
        }
        #[cfg(all(test, target_os = "linux"))]
        {
            self.memory_job_resources = resources
                .as_ref()
                .map_or_else(std::sync::Weak::new, Arc::downgrade);
        }
        Ok(())
    }

    async fn attach_ask(
        &mut self,
        config: &ConfigSnapshot,
        environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        if let Some(server) = self.server.as_mut() {
            server.attach_ask(config, environment, original).await?;
        }
        Ok(())
    }

    /// Native startup is finite; callers may run it on their existing blocking executor.
    pub fn start(
        installation: &InstallationPaths,
        inputs: &[ProjectBootstrapInput<'_>],
        current: usize,
        config: &ConfigSnapshot,
        binding: NativeBinding,
        original: &RequestContext<'_>,
    ) -> Result<Self, ForegroundFailure> {
        if crate::foreground_dream::enabled(config)? {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Dream requires async Foreground startup with captured environment",
            )
            .into());
        }
        Self::start_with_inventory(
            installation,
            inputs,
            current,
            config,
            binding,
            original,
            None,
        )
    }

    fn start_with_inventory(
        installation: &InstallationPaths,
        inputs: &[ProjectBootstrapInput<'_>],
        current: usize,
        config: &ConfigSnapshot,
        binding: NativeBinding,
        original: &RequestContext<'_>,
        publication: Option<&ProviderRecord>,
    ) -> Result<Self, ForegroundFailure> {
        let startup_deadline = original.snapshot().required_deadline()?;
        if (inputs.get(current).is_none() && !(inputs.is_empty() && current == 0))
            || config.brand() != installation.brand()
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let mut trusted = Vec::with_capacity(inputs.len());
        let mut identities = Vec::with_capacity(inputs.len());
        for input in inputs {
            if input.project.brand() != installation.brand() {
                return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
            }
            let identity = ProjectIdentityStore::at(input.project)
                .load()?
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Source project identity is absent")
                })?;
            if !trusted
                .iter()
                .any(|p: &ProjectPaths| p.root() == input.project.root())
            {
                trusted.push(input.project.clone());
            }
            identities.push(identity.id());
        }
        let policy = HubResourcePolicy::resolve(config).map_err(io_error)?;
        let graph_query = query_options(config, policy.content())?;
        let native_install =
            crate::native_install_http::NativeInstallPolicy::capture(config, original.snapshot())?;
        let depth = number(&policy, ResourceSetting::SnapshotDepth)?;
        let paths = number(&policy, ResourceSetting::SnapshotPathBytes)?;
        let bytes = number(&policy, ResourceSetting::ContentInputBytes)?;
        let scalar = number(&policy, ResourceSetting::ContentScalarBytes)?;
        let entries = number(&policy, ResourceSetting::ContentCollectionEntries)?;
        let retained = number(&policy, ResourceSetting::RetainedBytes)?;
        let roots = trusted
            .len()
            .checked_add(1)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let observations = roots
            .checked_mul(depth)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let scope = ScopeLimits::new(roots, MAX_PATH_BYTES, observations)?;
        let bootstrap = BootstrapLimits::new(MAX_PATH_BYTES, depth).map_err(io_error)?;
        let native = NativeDocumentFileLimits::new(bytes, MAX_PATH_BYTES, depth, scalar)?;
        let task_config = ForegroundTaskConfig::capture(
            config,
            &policy,
            scope,
            native,
            binding,
            original.snapshot(),
        )?;
        // Capture the configured transport pairs once. Selection stays explicit per request.
        let web_provider_snapshot = Some(auth_snapshot(installation, config, original)?);
        let permissions = [
            Permission::Read(Module::Ast),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Memory),
            Permission::MutateModule(Module::Knowledge),
        ];
        let remote_permissions = [
            Permission::Read(Module::Ast),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Memory),
            Permission::MutateModule(Module::Knowledge),
            Permission::External(ExternalEffect::RemoteRequest),
        ];
        let mut rules =
            publication.map_or_else(Vec::new, |provider| match &provider.config().kind {
                ProviderKind::Local {
                    broker: Some(broker),
                    ..
                } => vec![ServiceRuleInput::Remote {
                    origin: ServiceOrigin::Runtime,
                    binding: RemoteServiceBindingInput::BrokerInventory {
                        provider: provider.name(),
                        purpose: ServicePurpose::InventoryPublish,
                        endpoint: &broker.endpoint,
                    },
                    credential: CredentialSource::LocalBrokerKey,
                    incoming: None,
                }],
                ProviderKind::Broker(broker) => [
                    ServicePurpose::InventoryRead,
                    ServicePurpose::InventoryPublish,
                ]
                .into_iter()
                .map(|purpose| ServiceRuleInput::Remote {
                    origin: ServiceOrigin::Runtime,
                    binding: RemoteServiceBindingInput::BrokerInventory {
                        provider: provider.name(),
                        purpose,
                        endpoint: &broker.endpoint,
                    },
                    credential: CredentialSource::NativeAccess,
                    incoming: None,
                })
                .collect(),
                ProviderKind::Local { broker: None, .. } => Vec::new(),
            });
        if let Some(snapshot) = &web_provider_snapshot {
            for provider in snapshot.document().providers() {
                if matches!(
                    provider.config().kind,
                    ProviderKind::Local { broker: None, .. }
                ) && provider.config().s3.source != S3Source::BrokerScope
                {
                    rules.push(ServiceRuleInput::StandaloneS3 {
                        purpose: graphit_auth::ServicePurpose::HubRead,
                        origin: ServiceOrigin::Runtime,
                        provider: provider.name(),
                        profile: None,
                    });
                    for profile in snapshot
                        .document()
                        .profiles()
                        .iter()
                        .filter(|profile| profile.provider() == provider.name())
                    {
                        rules.push(ServiceRuleInput::StandaloneS3 {
                            purpose: graphit_auth::ServicePurpose::HubRead,
                            origin: ServiceOrigin::Runtime,
                            provider: provider.name(),
                            profile: Some(profile.name()),
                        });
                    }
                }
            }
        }
        // Records resolve their own provider under each live admission. These
        // transport rules grant no PROJECT or USER data permission.
        if let Some(snapshot) = &web_provider_snapshot {
            for provider in snapshot.document().providers() {
                match &provider.config().kind {
                    ProviderKind::Local { .. } if provider.config().s3.bucket.is_some() => {
                        for origin in [ServiceOrigin::Runtime, ServiceOrigin::Local] {
                            rules.push(ServiceRuleInput::StandaloneS3 {
                                purpose: ServicePurpose::RecordStorage,
                                origin,
                                provider: provider.name(),
                                profile: None,
                            });
                            for profile in snapshot
                                .document()
                                .profiles()
                                .iter()
                                .filter(|profile| profile.provider() == provider.name())
                            {
                                rules.push(ServiceRuleInput::StandaloneS3 {
                                    purpose: ServicePurpose::RecordStorage,
                                    origin,
                                    provider: provider.name(),
                                    profile: Some(profile.name()),
                                });
                            }
                        }
                    }
                    ProviderKind::Broker(broker) => {
                        for origin in [ServiceOrigin::Runtime, ServiceOrigin::Local] {
                            rules.push(ServiceRuleInput::Remote {
                                origin,
                                binding: RemoteServiceBindingInput::Broker {
                                    provider: provider.name(),
                                    purpose: ServicePurpose::RecordStorage,
                                    endpoint: &broker.endpoint,
                                    resource: broker.mcp_resource.as_deref(),
                                },
                                credential: CredentialSource::NativeAccess,
                                incoming: None,
                            });
                        }
                    }
                    _ => (),
                }
            }
        }
        let remote_services_enabled = !rules.is_empty();
        let mut remote_inference = false;
        if let Some(provider) = publication {
            provider.config().validate_ai_services().map_err(io_error)?;
            for (purpose, service) in [
                (ServicePurpose::Embedding, &provider.config().embedding),
                (ServicePurpose::Rerank, &provider.config().rerank),
            ] {
                let (binding, credential) = match service {
                    AiService::Disabled => continue,
                    AiService::Local(execution) => {
                        for origin in [
                            ServiceOrigin::Runtime,
                            ServiceOrigin::Local,
                            ServiceOrigin::Web,
                        ] {
                            rules.push(ServiceRuleInput::Local {
                                origin,
                                provider: provider.name(),
                                purpose,
                                execution: *execution,
                            });
                        }
                        continue;
                    }
                    AiService::Direct {
                        protocol,
                        endpoint,
                        model,
                        model_revision,
                        recipe,
                        dimensions,
                        request_dimensions,
                        batch_endpoint,
                    } => (
                        RemoteServiceBindingInput::Direct {
                            provider: provider.name(),
                            purpose,
                            protocol: *protocol,
                            endpoint,
                            model,
                            model_revision,
                            recipe,
                            dimensions: *dimensions,
                            request_dimensions: *request_dimensions,
                            batch_endpoint: batch_endpoint.as_deref(),
                        },
                        CredentialSource::DirectKey,
                    ),
                    AiService::Broker => {
                        let (broker, credential) = match &provider.config().kind {
                            ProviderKind::Broker(broker) => {
                                (broker, CredentialSource::NativeAccess)
                            }
                            ProviderKind::Local {
                                broker: Some(broker),
                                ..
                            } => {
                                let snapshot = web_provider_snapshot
                                    .as_ref()
                                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
                                let (active_provider, profile) =
                                    snapshot.document().active(original).map_err(io_error)?;
                                if active_provider.name() != provider.name() {
                                    return Err(io_error(ProfileError::Stale).into());
                                }
                                let anonymous = broker.allow_anonymous && profile.data().is_some_and(|data| {
                                    matches!(&data.credentials, ProfileCredentials::Local(keys) if keys.broker_key.is_none())
                                });
                                (
                                    broker,
                                    if anonymous {
                                        CredentialSource::Anonymous
                                    } else {
                                        CredentialSource::LocalBrokerKey
                                    },
                                )
                            }
                            ProviderKind::Local { broker: None, .. } => {
                                return Err(io::Error::from(io::ErrorKind::InvalidData).into());
                            }
                        };
                        (
                            RemoteServiceBindingInput::Broker {
                                provider: provider.name(),
                                purpose,
                                endpoint: &broker.endpoint,
                                resource: broker.mcp_resource.as_deref(),
                            },
                            credential,
                        )
                    }
                };
                remote_inference = true;
                for origin in [ServiceOrigin::Runtime, ServiceOrigin::Local] {
                    rules.push(ServiceRuleInput::Remote {
                        origin,
                        binding,
                        credential,
                        incoming: None,
                    });
                }
            }
        }
        let local_permissions = if remote_inference {
            &remote_permissions[..]
        } else {
            &permissions[..]
        };
        let mut runtime_permissions = if remote_services_enabled || remote_inference {
            remote_permissions.to_vec()
        } else {
            permissions.to_vec()
        };
        // Ordinary explicit Memory maintenance works without an embedding service.
        // The handler checks the effective selected module before any write.
        runtime_permissions.push(Permission::Read(Module::Hub));
        runtime_permissions.push(Permission::WriteMemory);
        // USER consumers still require a verified Native subject and their
        // ordinary read/write admissions; this ceiling grants no PROJECT ACL.
        runtime_permissions.extend([Permission::ReadUser, Permission::MutateUser]);
        runtime_permissions.extend([
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::WriteOwnTask,
            Permission::CoordinateTask,
        ]);
        runtime_permissions.push(Permission::MutateModule(Module::Ast));
        // Explicit authority for live PROJECT maintenance; selectors confer none.
        runtime_permissions.push(Permission::Configure);
        // Explicit write authority for the authenticated native Knowledge export.
        runtime_permissions.push(Permission::External(ExternalEffect::PublishedFilesystem));
        if let Some(provider) = publication {
            let (broker, credential) = match &provider.config().kind {
                ProviderKind::Local { broker, .. } => {
                    (broker.as_ref(), CredentialSource::LocalBrokerKey)
                }
                ProviderKind::Broker(broker) => (Some(broker), CredentialSource::NativeAccess),
            };
            if let Some(broker) = broker {
                for purpose in [
                    ServicePurpose::PresenceRead,
                    ServicePurpose::PresencePublish,
                    ServicePurpose::HubWrite,
                ] {
                    rules.push(ServiceRuleInput::Remote {
                        origin: ServiceOrigin::Runtime,
                        binding: RemoteServiceBindingInput::Broker {
                            provider: provider.name(),
                            purpose,
                            endpoint: &broker.endpoint,
                            resource: broker.mcp_resource.as_deref(),
                        },
                        credential,
                        incoming: None,
                    });
                }
                if credential == CredentialSource::NativeAccess {
                    rules.push(ServiceRuleInput::Remote {
                        origin: ServiceOrigin::Runtime,
                        binding: RemoteServiceBindingInput::Broker {
                            provider: provider.name(),
                            purpose: ServicePurpose::HubRead,
                            endpoint: &broker.endpoint,
                            resource: broker.mcp_resource.as_deref(),
                        },
                        credential,
                        incoming: None,
                    });
                }
            }
        }
        if let (Some(browser), Some(snapshot)) = (&task_config, &web_provider_snapshot) {
            for name in browser.browser_providers() {
                if let Some(provider) = snapshot.document().provider(name)
                    && let ProviderKind::Broker(broker) = &provider.config().kind
                {
                    for purpose in [
                        ServicePurpose::HubRead,
                        ServicePurpose::HubWrite,
                        ServicePurpose::RecordStorage,
                    ] {
                        rules.push(ServiceRuleInput::Remote {
                            origin: ServiceOrigin::Web,
                            binding: RemoteServiceBindingInput::Broker {
                                provider: provider.name(),
                                purpose,
                                endpoint: &broker.endpoint,
                                resource: broker.mcp_resource.as_deref(),
                            },
                            credential: CredentialSource::IncomingBearer,
                            incoming: None,
                        });
                    }
                    for (purpose, service) in [
                        (ServicePurpose::Embedding, &provider.config().embedding),
                        (ServicePurpose::Rerank, &provider.config().rerank),
                    ] {
                        if matches!(service, AiService::Broker) {
                            rules.push(ServiceRuleInput::Remote {
                                origin: ServiceOrigin::Web,
                                binding: RemoteServiceBindingInput::Broker {
                                    provider: provider.name(),
                                    purpose,
                                    endpoint: &broker.endpoint,
                                    resource: broker.mcp_resource.as_deref(),
                                },
                                credential: CredentialSource::IncomingBearer,
                                incoming: None,
                            });
                        }
                    }
                }
            }
        }
        let mut administrative = vec![
            Permission::ManageCache,
            Permission::ManageAuthProfiles,
            Permission::ManageAccessGrants,
            Permission::External(ExternalEffect::NativeLaunch),
            Permission::Configure,
            Permission::External(ExternalEffect::PublishedFilesystem),
            Permission::Read(Module::Memory),
            Permission::Read(Module::Task),
            Permission::Read(Module::Ast),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Hub),
            Permission::InspectGit,
            Permission::WriteMemory,
            Permission::WriteOwnTask,
        ];
        if crate::foreground_dream::enabled(config)? {
            administrative.push(Permission::WriteFencedMemory);
        }
        let origin = |ordinary, administrative| SourceAuthorityPolicy {
            ordinary,
            administrative,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(15 * 60),
                maximum: Duration::from_secs(60 * 60),
            },
        };
        let mut authority = IssuerAuthorityPolicy::new(
            origin(&runtime_permissions, &administrative),
            origin(local_permissions, &[]),
            IssuerLimits {
                max_permissions: entries,
                max_scope_bytes: paths,
                max_parent_bearer_bytes: scalar,
            },
            ServiceAuthorityInput {
                rules: &rules,
                limits: ServiceAdmissionLimits {
                    max_rules: entries,
                    max_binding_bytes: paths,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .map_err(io_error)?
        .with_native_installation_permissions(&[
            Permission::Configure,
            Permission::ManageCache,
            Permission::External(ExternalEffect::PublishedFilesystem),
        ])
        .map_err(io_error)?;
        if let Some(browser) = &task_config {
            for provider in browser.browser_providers() {
                authority = authority
                    .with_incoming_user_permissions(
                        provider,
                        &if browser.knowledge_conversations() {
                            vec![
                                Permission::ReadUser,
                                Permission::Read(Module::Memory),
                                Permission::Read(Module::Knowledge),
                                Permission::MutateUser,
                                Permission::MutateModule(Module::Knowledge),
                                Permission::WriteMemory,
                                Permission::WriteFencedMemory,
                            ]
                        } else {
                            vec![
                                Permission::ReadUser,
                                Permission::Read(Module::Memory),
                                Permission::MutateUser,
                                Permission::WriteMemory,
                                Permission::WriteFencedMemory,
                            ]
                        },
                    )
                    .map_err(io_error)?
                    .with_web_imported_knowledge_read(provider)
                    .map_err(io_error)?
                    .with_web_imported_ast_read(provider)
                    .map_err(io_error)?;
                authority = authority
                    .with_web_installed_hub_read(provider)
                    .map_err(io_error)?;
            }
        }
        let startup = StartupPolicy {
            bootstrap,
            runtime: RuntimeKeyLimits::new(MAX_PATH_BYTES, depth, scalar).map_err(io_error)?,
            scope,
            document: AuthDocumentLimits::new(bytes, MAX_PATH_BYTES, depth, scalar)
                .map_err(io_error)?,
            profiles: ProfileLimits {
                bytes,
                nodes: number(&policy, ResourceSetting::ContentNodes)?,
                scalar_bytes: scalar,
                collection_entries: entries,
                providers: entries,
                profiles: entries,
            },
            authority,
            ttl: TtlPolicy {
                ttl: Duration::from_secs(30 * 60),
                maximum: Duration::from_secs(24 * 60 * 60),
                refresh_margin: Duration::from_secs(60),
            },
        };
        // Validate listener selection before creating a runtime credential.
        let address = config
            .get::<String>(&ConfigKey::new("mcp.host")?)?
            .unwrap_or_else(|| "127.0.0.1".into())
            .parse::<IpAddr>()
            .map_err(io_error)?;
        if !address.is_loopback() && !address.is_unspecified() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let port = config
            .get::<u16>(&ConfigKey::new("mcp.port")?)?
            .unwrap_or(0);
        let ports =
            NonZeroU16::new(port).map_or(PortSelection::Ephemeral, |port| PortSelection::Range {
                first: port,
                last: port,
            });
        // Configuration may gain an active Native profile after startup. The
        // shared owner is lazy: construction performs no discovery or JWT I/O.
        let broker_verifiers = Some(Arc::new(BrokerVerifierCache::new(
            inventory_network_limits(&policy)?,
            crate::admin_auth::verifier_limits(policy.content()),
            config
                .get::<bool>(&ConfigKey::new("broker.allow_loopback_http")?)?
                .unwrap_or(false),
            None,
            &[],
        )));
        let auth = Arc::new(
            DaemonAuth::start(installation, &trusted, startup, original).map_err(io_error)?,
        );
        let mut owner = Self {
            project_control: None,
            auth,
            broker_verifiers,
            server: None,
            tasks: None,
            projects: None,
            now: None,
            dream: None,
            user_dream: None,
            dream_activity: Arc::new(std::sync::RwLock::new(None)),
            memory_jobs: None,
            requested_local_rerank: None,
            model_changes: Arc::new(tokio::sync::Notify::new()),
            #[cfg(all(test, target_os = "linux"))]
            model_refreshes: tokio::sync::watch::channel(None).0,
            #[cfg(all(test, target_os = "linux"))]
            model_child_exits: tokio::sync::watch::channel(None).0,
            ast_jobs: None,
            #[cfg(all(test, target_os = "linux"))]
            ast_full_completed: Arc::new(std::sync::Mutex::new(None)),
            external_watcher: None,
            #[cfg(all(test, target_os = "linux"))]
            memory_job_resources: std::sync::Weak::new(),
            supervisors: graphit_daemon::supervisor::ProjectSupervisors::default(),
            listener: None,
            record: None,
            policy,
            scope,
            native,
        };
        let configured = (|| -> io::Result<()> {
            let inventory = GlobalProjectInventory::at(installation, bytes)?;
            let mut projects: Vec<(ProjectPaths, ProjectId, VersionContext)> =
                Vec::with_capacity(inputs.len());
            for (input, id) in inputs.iter().zip(identities) {
                let private =
                    installation_bootstrap::prepare(installation, bootstrap, original.snapshot())
                        .into_installation()
                        .map_err(io_error)?;
                let selected = bootstrap_project(
                    ProjectBootstrapInput {
                        project: input.project,
                        explicit_version: input.explicit_version,
                        pin: input.pin,
                        detached_base: input.detached_base,
                    },
                    ProjectBootstrapLimits { scope, native },
                    &inventory,
                    private,
                    original.snapshot(),
                )
                .map_err(io_error)?;
                if projects.iter().any(|(project, _, version)| {
                    project.root() == input.project.root()
                        && version.version() == selected.version().version()
                }) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "duplicate Source checkout/version selection",
                    ));
                }
                projects.push((input.project.clone(), id, selected.version().clone()));
            }
            let timeout =
                Duration::from_millis(owner.policy.limit(ResourceSetting::RequestDeadlineMs).value);
            let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
                owner.auth.installation().live.owner()
            else {
                return Err(io::Error::other("Source installation owner invalid"));
            };
            let registry = HubRegistry::global(installation, installation_id, owner.policy.hub())?;
            let projects = Arc::new(projects);
            {
                owner.projects = Some(Arc::clone(&projects));
            }
            if let Some(provider) = publication.filter(|provider| {
                !matches!(
                    provider.config().kind,
                    ProviderKind::Local { broker: None, .. }
                )
            }) {
                owner.now = Some(ForegroundNow::new(
                    Arc::clone(&owner.auth),
                    Arc::clone(&projects),
                    provider.clone(),
                    owner.broker_verifiers.clone(),
                    NowSettings {
                        policy: owner.policy.clone(),
                        scope,
                        native,
                        binding,
                        network: inventory_network_limits(&owner.policy)?,
                        allow_loopback_http: config
                            .get::<bool>(&ConfigKey::new("broker.allow_loopback_http")?)?
                            .unwrap_or(false),
                    },
                )?);
            }

            let stop = CancellationToken::new();
            let imported_registry = registry.clone();
            let source_policy = SourcePolicy {
                graph_query,
                installation: installation.clone(),
                native_install,
                content: owner.policy.content(),
                timeout,
                scope,
                file: owner
                    .policy
                    .snapshot_policy(CopyPolicy::RejectFileLinks, [])?,
                read: SourceReadLimits::new(
                    retained,
                    64 * 1024,
                    owner.policy.limit(ResourceSetting::SnapshotBytes).value,
                )?,
                path: owner.policy.retained(),
                index: native,
                binding,
                registry,
            };
            if !projects.is_empty() && crate::foreground_dream::enabled(config)? {
                owner.dream = ForegroundDream::new(
                    Arc::clone(&owner.auth),
                    Arc::clone(&projects),
                    current,
                    source_policy.clone(),
                    config.clone(),
                    original,
                )?;
            }
            if crate::foreground_dream::enabled(config)?
                && graphit_core::module_settings::ModuleSettings::resolve(config)?
                    .enabled(Module::Memory)
            {
                owner.user_dream = ForegroundUserDream::new(
                    Arc::clone(&owner.auth),
                    owner
                        .broker_verifiers
                        .clone()
                        .ok_or(io::ErrorKind::NotConnected)?,
                    source_policy.clone(),
                    config.clone(),
                )?;
            }
            let mut server = SourceServer::new(
                Arc::clone(&owner.auth),
                Arc::clone(&projects),
                current,
                source_policy,
                stop.clone(),
            )?;
            {
                let (control, requests) = crate::foreground_project_control::ProjectControl::new(
                    owner.policy.content(),
                    timeout,
                    stop.clone(),
                );
                server.attach_local_model_control(control.clone())?;
                owner.project_control = Some(ProjectControlOwner {
                    control,
                    requests,
                    installation: installation.clone(),
                    config: config.clone(),
                    environment: std::env::vars_os().collect(),
                });
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            server.attach_native_verifiers(Arc::clone(
                owner
                    .broker_verifiers
                    .as_ref()
                    .expect("captured verifier cache"),
            ))?;
            server.attach_role_tools(config.clone())?;
            server.attach_now(owner.broker_verifiers.clone(), config)?;
            server.attach_human_activity(Arc::clone(&owner.dream_activity))?;
            if let Some(user) = &owner.user_dream {
                server.attach_user_activity(user.activity_sender())?;
            }
            let mut native_tasks =
                crate::foreground_tasks::task_policy(&owner.policy, scope, native, binding)?;
            native_tasks.imported_knowledge = Some(crate::task_server::ImportedKnowledgePolicy {
                registry: imported_registry.clone(),
                directory: native,
            });
            server.attach_tasks(native_tasks)?;
            owner.server = Some(server);
            let listener = tcp_listener::reserve(
                Options {
                    address,
                    ports,
                    timeout: {
                        original.check()?;
                        startup_deadline
                            .checked_duration_since(Instant::now())
                            .ok_or(io::ErrorKind::TimedOut)?
                    },
                },
                original.cancellation(),
            )
            .map_err(io_error)?;
            let address = source_discovery::local_address(&listener)?;
            owner.listener = Some(listener);
            if let Some(config) = task_config {
                owner.tasks = Some(ForegroundTasks::new(
                    Arc::clone(&owner.auth),
                    owner
                        .server
                        .as_ref()
                        .expect("captured Source server")
                        .task_server()?,
                    config,
                    address,
                    stop,
                    original,
                )?);
                if let Some(access) = owner
                    .tasks
                    .as_ref()
                    .and_then(ForegroundTasks::conversation_access)
                {
                    owner
                        .server
                        .as_mut()
                        .expect("captured Source server")
                        .attach_knowledge_conversations(access)?;
                }
            }
            let record = source_discovery::publish_discovery(
                &owner.auth,
                owner
                    .listener
                    .as_ref()
                    .expect("reserved foreground listener"),
                native,
                original.snapshot(),
            )
            .map_err(io_error)?;
            owner.record = Some(record);
            Ok(())
        })();
        if let Err(error) = configured {
            return Err(ForegroundFailure {
                error,
                owner: Some(Box::new(owner)),
                runtime: None,
                service: None,
            });
        }
        Ok(owner)
    }

    pub fn address(&self) -> Option<std::net::SocketAddr> {
        self.record.as_ref().map(DiscoveryRecord::address)
    }

    async fn maintain_live_checkout(
        &mut self,
        pending: &crate::foreground_project_control::PendingCommand,
        original: &RequestContext<'_>,
        caller: &graphit_auth::AuthenticatedCaller<'_>,
        installation: &InstallationPaths,
        config: &ConfigSnapshot,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> (axum::http::StatusCode, serde_json::Value, Option<io::Error>) {
        use axum::http::StatusCode;
        let crate::foreground_project_control::ProjectCommand::MaintainCheckout {
            project_id,
            project_dir,
            version,
            detached_base,
            select,
        } = &pending.command
        else {
            unreachable!("maintain command")
        };
        let mut registered = None;
        let mut trusted = None;
        let mut selected_version = None;
        let mut broker_effect = None;
        let mut phase = "selection";
        let result: io::Result<()> = async {
            original.check()?;
            let paths = ProjectPaths::at(project_dir.as_path(), installation)?;
            if paths.root() != project_dir.as_path() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "checkout selector must be exact",
                ));
            }
            let identity = ProjectIdentityStore::at(&paths)
                .load()?
                .ok_or(io::ErrorKind::NotFound)?;
            original.check()?;
            if identity.id() != *project_id {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            let server = self.server.as_ref().ok_or(io::ErrorKind::NotConnected)?;
            let membership = server.live_projects();
            let previous = membership.snapshot()?;
            let source_policy = server.policy().clone();
            // Observe the checkout before deciding whether any productive state changes.
            // An unchanged MCP startup does not register, start a model, or publish again.
            let remaining = original
                .snapshot()
                .required_deadline()?
                .checked_duration_since(Instant::now())
                .ok_or(io::ErrorKind::TimedOut)?;
            let checkout = graphit_core::git_checkout::GitCheckout::inspect(
                paths.root(),
                remaining,
                original.cancellation(),
            )?;
            original.check()?;
            let observed_version = VersionContext::select(
                &paths,
                checkout.as_checkout(),
                version,
                None,
                detached_base,
            )?;
            let active = previous
                .projects
                .iter()
                .position(|(candidate, id, selected)| {
                    candidate.root() == paths.root()
                        && *id == *project_id
                        && selected.version() == observed_version.version()
                        && selected.default_version() == observed_version.default_version()
                        && selected.is_git() == observed_version.is_git()
                        && selected.is_detached() == observed_version.is_detached()
                });
            // Provenance alone is not a different held version.
            let observed_version = active
                .map(|i| previous.projects[i].2.clone())
                .unwrap_or(observed_version);
            if active.is_some() && (!*select || previous.current == active) {
                selected_version = Some(observed_version.version().to_owned());
                phase = "unchanged";
                return Ok(());
            }
            let private = installation_bootstrap::prepare(
                installation,
                BootstrapLimits::new(
                    MAX_PATH_BYTES,
                    number(&self.policy, ResourceSetting::SnapshotDepth)?,
                )
                .map_err(io_error)?,
                original.snapshot(),
            )
            .into_installation()
            .map_err(io_error)?;
            let inventory = GlobalProjectInventory::at(
                installation,
                number(&self.policy, ResourceSetting::ContentInputBytes)?,
            )?;
            caller.authorize(Permission::Configure).map_err(io_error)?;
            phase = "inventory_registration";
            let captured = CapturedPublicationScope::project(
                &paths,
                *project_id,
                self.scope,
                original.snapshot(),
            )?;
            let captured_inventory =
                graphit_core::project_inventory::CapturedProjectInventory::capture_in_scope(
                    &paths,
                    &checkout,
                    &observed_version,
                    &captured,
                    self.native,
                    original.snapshot(),
                )?;
            registered = Some(
                inventory
                    .writer(private, original.snapshot())?
                    .register_checkout(&captured_inventory)?,
            );
            // Register checks before commit; keep the known effect before a later recheck.
            selected_version = Some(observed_version.version().to_owned());
            let selection = (paths, *project_id, observed_version);
            phase = "project_trust";
            let record = captured.binding()?;
            self.auth
                .trust_project_root(
                    caller,
                    original,
                    graphit_auth::ScopeCapture {
                        live: &captured,
                        record: &record,
                    },
                )
                .map_err(io_error)?;
            trusted = Some(true);
            original.check()?;
            let replaced = previous.projects.iter().any(|(paths, id, version)| {
                *id == *project_id
                    && paths.root() == project_dir.as_path()
                    && version != &selection.2
            });
            phase = "project_join";
            if replaced {
                membership.retire_checkout(*project_id, project_dir.as_path())?;
                if let Some(now) = self.now.as_mut() {
                    now.stop_checkout(*project_id, project_dir.as_path(), None)
                        .await?;
                }
                self.supervisors
                    .stop_checkout(*project_id, project_dir.as_path())
                    .await?;
            }
            original.check()?;
            caller.authorize(Permission::Configure).map_err(io_error)?;
            let mut desired: Vec<_> = previous
                .projects
                .iter()
                .filter(|(paths, id, _)| {
                    *id != *project_id || paths.root() != project_dir.as_path()
                })
                .cloned()
                .collect();
            let new_index = desired.len();
            desired.push(selection);
            let current = if *select {
                new_index
            } else {
                previous
                    .current
                    .and_then(|i| previous.projects.get(i))
                    .and_then(|(paths, id, version)| {
                        desired.iter().position(|(candidate, identity, selected)| {
                            candidate.root() == paths.root()
                                && identity == id
                                && selected == version
                        })
                    })
                    .unwrap_or(new_index)
            };
            let desired = Arc::new(desired);
            phase = "selection";
            membership.replace(Arc::clone(&desired), current)?;
            self.projects = Some(Arc::clone(&desired));
            let context = graphit_daemon::supervisor::ProjectContext {
                id: desired[current].1,
                version: desired[current].2.clone(),
            };
            let dream_changed = self
                .dream
                .as_ref()
                .is_some_and(|dream| dream.activity_sender(&context).is_none());
            if dream_changed {
                phase = "dream_join";
                if let Some(dream) = self.dream.as_mut() {
                    dream.shutdown().await?;
                }
                self.dream = None;
            }
            if self.dream.is_none() && crate::foreground_dream::enabled(config)? {
                phase = "dream_factory";
                self.dream = ForegroundDream::new(
                    Arc::clone(&self.auth),
                    Arc::clone(&desired),
                    current,
                    source_policy.clone(),
                    config.clone(),
                    original,
                )?;
                if let Some(dream) = self.dream.as_mut() {
                    dream.start(environment.to_vec(), original).await?;
                }
            }
            phase = "project_factories";
            self.attach_jobs(
                installation,
                config,
                source_policy.binding,
                environment,
                original,
            )
            .await?;
            // A new publisher requires its project/version in the Broker inventory.
            // Retain this remote effect even if its subsequent admission fails.
            phase = "broker_admission";
            let (effect, cause) = self
                .publish_live_inventory(caller, original, installation, config)
                .await?;
            broker_effect = effect;
            if let Some(cause) = cause {
                phase = "broker_publication";
                return Err(cause);
            }
            phase = "now_publication";
            if let Some(now) = self.now.as_mut() {
                now.reconcile(desired, original).await?;
            }
            Ok(())
        }
        .await;
        let response = |status, phase, effect| {
            (
                status,
                serde_json::json!({
                "phase": phase, "inventory_registered": registered, "trusted_root": trusted, "version": selected_version,
                        "broker_effect": effect, "checkout_removed": null,
                    }),
            )
        };
        if let Err(cause) = result {
            let status = if matches!(phase, "broker_admission" | "broker_publication") {
                StatusCode::BAD_GATEWAY
            } else {
                match cause.kind() {
                    io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
                    io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
                    io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
                        StatusCode::GATEWAY_TIMEOUT
                    }
                    _ => StatusCode::SERVICE_UNAVAILABLE,
                }
            };
            let (status, body) = response(status, phase, broker_effect);
            return (status, body, Some(cause));
        }
        if phase == "unchanged" {
            let (status, body) = response(StatusCode::OK, phase, Option::<String>::None);
            return (status, body, None);
        }
        let (status, body) = response(StatusCode::OK, "complete", broker_effect);
        (status, body, None)
    }

    async fn publish_live_inventory(
        &self,
        caller: &graphit_auth::AuthenticatedCaller<'_>,
        original: &RequestContext<'_>,
        installation: &InstallationPaths,
        config: &ConfigSnapshot,
    ) -> io::Result<(Option<String>, Option<io::Error>)> {
        let provider = active_provider(installation, config, original)?;
        let Some(provider) = provider.filter(|provider| {
            !matches!(
                provider.config().kind,
                ProviderKind::Local { broker: None, .. }
            )
        }) else {
            return Ok((None, None));
        };
        let client = inventory_client(&provider, config, &self.policy)?;
        match publish_maintained_inventory(
            &client,
            &self.auth,
            caller,
            if matches!(provider.config().kind, ProviderKind::Broker(_)) {
                InventoryCredentialMode::BrokerNative(
                    self.broker_verifiers
                        .as_deref()
                        .ok_or(io::ErrorKind::InvalidInput)?,
                )
            } else {
                InventoryCredentialMode::LocalBrokerKey
            },
            original,
            number(&self.policy, ResourceSetting::ContentInputBytes)?,
        )
        .await
        {
            Ok(_) => Ok((Some("confirmed".to_owned()), None)),
            Err(cause) => Ok((Some(format!("{:?}", cause.effect())), Some(io_error(cause)))),
        }
    }

    async fn ensure_local_model(
        &mut self,
        pending: &crate::foreground_project_control::PendingLocalModel,
    ) -> io::Result<crate::foreground_project_control::LocalModelResources> {
        let control = self
            .project_control
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?;
        let installation = control.installation.clone();
        let environment = control.environment.clone();
        pending.original.check()?;
        let request = RequestContext::new(
            self.policy.content(),
            pending.original.cancellation(),
            pending.original.required_deadline()?,
        )
        .map_err(io_error)?;
        let auth = Arc::clone(&self.auth);
        let runtime = auth.runtime().check(request.snapshot()).map_err(io_error)?;
        let credential = std::str::from_utf8(runtime.secret(request.snapshot()).map_err(io_error)?)
            .map_err(io_error)?;
        let caller = auth
            .issuer()
            .authenticate_runtime(credential, &request)
            .map_err(io_error)?;
        let mut service = auth
            .issuer()
            .capture_service(&caller, pending.purpose)
            .map_err(io_error)?;
        let lease = service.local().map_err(io_error)?;
        match lease.binding().map_err(io_error)? {
            graphit_auth::ServiceBindingView::Local {
                provider,
                purpose,
                execution,
            } if provider == pending.provider
                && purpose == pending.purpose
                && execution == pending.execution => {}
            _ => return Err(io::ErrorKind::PermissionDenied.into()),
        }
        let expected = crate::foreground_memory_jobs::expected_local_model(&installation, &lease)?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "selected local model slot is absent",
                )
            })?;
        if self.memory_jobs.is_none() {
            self.memory_jobs = Some(ForegroundMemoryJobs::new(
                Arc::clone(&auth),
                self.policy.clone(),
                None,
                false,
                None,
                Arc::clone(
                    self.broker_verifiers
                        .as_ref()
                        .ok_or(io::ErrorKind::NotConnected)?,
                ),
            )?);
        }
        if self
            .memory_jobs
            .as_ref()
            .is_some_and(|jobs| jobs.child.is_none())
        {
            self.requested_local_rerank = None;
        }
        let (client, address) = self
            .memory_jobs
            .as_mut()
            .ok_or(io::ErrorKind::NotConnected)?
            .ensure_process(&installation, environment, &request)
            .await?;
        lease.check().map_err(io_error)?;
        pending.original.check()?;
        if pending.purpose == ServicePurpose::Rerank {
            self.requested_local_rerank = Some((pending.provider.clone(), expected.config_id()));
        }
        Ok((client, expected, address))
    }

    async fn execute_project_command(
        &mut self,
        pending: &crate::foreground_project_control::PendingCommand,
    ) -> (axum::http::StatusCode, serde_json::Value, Option<io::Error>) {
        use crate::foreground_project_control::ProjectCommand;
        use crate::project_retirement::{
            CheckoutCredential, CheckoutRetirementInput, retire_checkout,
        };
        use axum::http::StatusCode;
        let failure = |status, phase, changed: Option<bool>, remote: Option<String>| {
            (
                status,
                serde_json::json!({"phase": phase, "checkout_removed": changed, "broker_effect": remote}),
                None,
            )
        };
        let caused = |mut response: (StatusCode, serde_json::Value, Option<io::Error>),
                      cause: io::Error| {
            response.2 = Some(cause);
            response
        };
        let Some(control) = &self.project_control else {
            return failure(StatusCode::SERVICE_UNAVAILABLE, "unavailable", None, None);
        };
        let installation = control.installation.clone();
        let config = control.config.clone();
        let environment = control.environment.clone();
        let original = match RequestContext::new(
            self.policy.content(),
            &pending.cancellation,
            pending.deadline,
        ) {
            Ok(original) => original,
            Err(_) => return failure(StatusCode::GATEWAY_TIMEOUT, "original", None, None),
        };
        let auth = Arc::clone(&self.auth);
        let caller = match auth
            .issuer()
            .authenticate_runtime(&pending.credential, &original)
        {
            Ok(caller) => caller,
            Err(_) => return failure(StatusCode::UNAUTHORIZED, "authentication", None, None),
        };
        if !matches!(
            caller.authorize(Permission::Configure),
            Ok(graphit_auth::Obligation::Unconditional)
        ) {
            return failure(StatusCode::FORBIDDEN, "configure", None, None);
        }
        if let ProjectCommand::MaintainCheckout { .. } = &pending.command {
            return self
                .maintain_live_checkout(
                    pending,
                    &original,
                    &caller,
                    &installation,
                    &config,
                    &environment,
                )
                .await;
        }
        let ProjectCommand::RetireCheckout {
            project_id,
            project_dir,
        } = &pending.command
        else {
            unreachable!("maintain command dispatched");
        };
        let project_dir = project_dir.as_path();
        if !project_dir.is_absolute() {
            return failure(StatusCode::BAD_REQUEST, "selection", None, None);
        }
        let Some(server) = self.server.as_ref() else {
            return failure(StatusCode::SERVICE_UNAVAILABLE, "source", None, None);
        };
        let membership = server.live_projects();
        if let Err(cause) = self.retire_unused_model(&original).await {
            return caused(
                failure(StatusCode::SERVICE_UNAVAILABLE, "model_join", None, None),
                cause,
            );
        }
        let prepared = (|| -> io::Result<_> {
            original.check()?;
            let depth = number(&self.policy, ResourceSetting::SnapshotDepth)?;
            let private = installation_bootstrap::prepare(
                &installation,
                BootstrapLimits::new(MAX_PATH_BYTES, depth).map_err(io_error)?,
                original.snapshot(),
            )
            .into_installation()
            .map_err(io_error)?;
            let inventory = GlobalProjectInventory::at(
                &installation,
                number(&self.policy, ResourceSetting::ContentInputBytes)?,
            )?;
            let snapshot = inventory.load(original.snapshot())?;
            let exists = snapshot.get(*project_id).is_some_and(|project| {
                project
                    .checkouts()
                    .any(|checkout| checkout.path() == project_dir)
            });
            if !exists {
                return Err(io::ErrorKind::NotFound.into());
            }
            Ok((
                private,
                number(&self.policy, ResourceSetting::ContentInputBytes)?,
            ))
        })();
        let (private, inventory_bytes) = match prepared {
            Ok(private) => private,
            Err(cause) => {
                let status = match cause.kind() {
                    io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                    io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
                    io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
                        StatusCode::GATEWAY_TIMEOUT
                    }
                    _ => StatusCode::SERVICE_UNAVAILABLE,
                };
                return caused(failure(status, "inventory", None, None), cause);
            }
        };
        if let Err(cause) = membership.retire_checkout(*project_id, project_dir) {
            return caused(
                failure(StatusCode::SERVICE_UNAVAILABLE, "selection", None, None),
                cause,
            );
        }
        if let Some(now) = self.now.as_mut()
            && let Err(cause) = now.stop_checkout(*project_id, project_dir, None).await
        {
            return caused(
                failure(StatusCode::SERVICE_UNAVAILABLE, "now_join", None, None),
                cause,
            );
        }
        let retire_dream = self.dream.as_ref().is_some_and(|dream| {
            self.projects.as_ref().is_some_and(|projects| {
                projects.iter().any(|(paths, id, version)| {
                    *id == *project_id
                        && paths.root() == project_dir
                        && dream
                            .activity_sender(&graphit_daemon::supervisor::ProjectContext {
                                id: *id,
                                version: version.clone(),
                            })
                            .is_some()
                })
            })
        });
        if retire_dream {
            if let Some(dream) = self.dream.as_mut()
                && let Err(cause) = dream.shutdown().await
            {
                return caused(
                    failure(StatusCode::SERVICE_UNAVAILABLE, "dream_join", None, None),
                    cause,
                );
            }
            self.dream = None;
        }
        let changed = match retire_checkout(
            &self.auth,
            CheckoutRetirementInput {
                credential: CheckoutCredential::Runtime(&pending.credential),
                installation: private,
                expected: *project_id,
                checkout: project_dir,
            },
            &mut self.supervisors,
            inventory_bytes,
            &original,
        )
        .await
        {
            Ok(changed) => changed,
            Err(cause) => {
                return caused(
                    failure(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "checkout_retirement",
                        None,
                        None,
                    ),
                    io_error(cause),
                );
            }
        };
        let snapshot = match membership.snapshot() {
            Ok(snapshot) => snapshot,
            Err(cause) => {
                return caused(
                    failure(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "selection",
                        Some(changed),
                        None,
                    ),
                    cause,
                );
            }
        };
        self.projects = Some(Arc::clone(&snapshot.projects));
        if let Err(cause) = self.retire_unused_model(&original).await {
            return caused(
                failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "model_join",
                    Some(changed),
                    None,
                ),
                cause,
            );
        }
        let restore_dream: io::Result<()> = async {
            if retire_dream
                && let Some(current) = snapshot.current
                && crate::foreground_dream::enabled(&config)?
            {
                let policy = self
                    .server
                    .as_ref()
                    .ok_or(io::ErrorKind::NotConnected)?
                    .policy()
                    .clone();
                self.dream = ForegroundDream::new(
                    Arc::clone(&self.auth),
                    Arc::clone(&snapshot.projects),
                    current,
                    policy,
                    config.clone(),
                    &original,
                )?;
                if let Some(dream) = self.dream.as_mut() {
                    dream.start(environment, &original).await?;
                }
            }
            let active = snapshot.projects.iter().find_map(|(_, id, version)| {
                let context = graphit_daemon::supervisor::ProjectContext {
                    id: *id,
                    version: version.clone(),
                };
                self.dream
                    .as_ref()
                    .and_then(|dream| dream.activity_sender(&context))
                    .map(|sender| (context, sender))
            });
            *self
                .dream_activity
                .write()
                .map_err(|_| io::Error::other("Dream activity state poisoned"))? = active;
            Ok(())
        }
        .await;
        if let Err(cause) = restore_dream {
            return caused(
                failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "dream_factory",
                    Some(changed),
                    None,
                ),
                cause,
            );
        }
        let remote = self
            .publish_live_inventory(&caller, &original, &installation, &config)
            .await;
        match remote {
            Ok((effect, None)) => failure(StatusCode::OK, "complete", Some(changed), effect),
            Ok((effect, Some(cause))) => caused(
                failure(
                    StatusCode::BAD_GATEWAY,
                    "broker_publication",
                    Some(changed),
                    effect,
                ),
                cause,
            ),
            Err(cause) => caused(
                failure(
                    StatusCode::BAD_GATEWAY,
                    "broker_admission",
                    Some(changed),
                    None,
                ),
                cause,
            ),
        }
    }

    /// Always await the live server before native cleanup and singleton release.
    pub async fn run(mut self, service: &Cancellation) -> Result<(), ForegroundFailure> {
        let Some(server) = self.server.as_ref().cloned() else {
            return Err(ForegroundFailure {
                error: io::ErrorKind::InvalidInput.into(),
                owner: Some(Box::new(self)),
                runtime: None,
                service: None,
            });
        };
        let Some(listener) = self.listener.take() else {
            return Err(ForegroundFailure {
                error: io::ErrorKind::InvalidInput.into(),
                owner: Some(Box::new(self)),
                runtime: None,
                service: None,
            });
        };
        let additional = self.tasks.as_ref().map_or_else(axum::Router::new, |tasks| {
            tasks.router(
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                server.clone(),
            )
        });
        let additional = match &self.project_control {
            Some(control) => additional.merge(control.control.router()),
            None => additional,
        };
        let ui_url = self.tasks.as_ref().map(|tasks| tasks.ui_url().to_owned());
        // HTTP admission is independent of the first Now snapshot/welcome. Own the
        // server task here so other startup/refresh awaits cannot stop polling it.
        let mut serving = tokio::task::JoinSet::new();
        let http = server.clone();
        serving.spawn(async move { http.serve_composed(listener, additional, ui_url).await });
        let served = async {
            serving
                .join_next()
                .await
                .ok_or_else(|| io::Error::other("foreground HTTP task missing"))?
                .map_err(io::Error::other)?
        };
        tokio::pin!(served);
        if !service.is_cancelled()
            && let Some(now) = self.now.as_mut()
            && let Err(error) = now.start_jobs()
        {
            now.cancel();
            let stopped = server.shutdown().await;
            let joined = served.await;
            return match self.shutdown().await {
                Ok(()) => Err(ForegroundFailure {
                    error,
                    owner: None,
                    runtime: None,
                    service: joined.and(stopped).err(),
                }),
                Err(mut failure) => {
                    failure.service = Some(error);
                    Err(failure)
                }
            };
        }

        let model_changes = Arc::clone(&self.model_changes);
        let (result, selected_branch) = loop {
            tokio::select! {
                biased;
                () = service.cancelled() => {
                    if let Some(now) = &self.now { now.cancel(); }
                    if let Some(dream) = &self.dream { dream.cancel(); }
                    if let Some(user) = &self.user_dream { user.cancel(); }
                    let stopped = server.shutdown().await;
                    let result = served.await;
                    break (result.and(stopped), "service_stop")
                }
                result = &mut served => break (result, "served"),
                _address = async {
                    self.memory_jobs.as_mut().expect("configured model owner").child_exited().await
                }, if self.memory_jobs.as_ref().is_some_and(|jobs| jobs.child.is_some()) => {
                    self.requested_local_rerank = None;
                    // Only the local execution resource borrowed this process.
                    // Remote embeddings remain usable when a rerank-only child exits.
                    let invalidate = server.semantic_model().map(|resources| {
                        resources.is_some_and(|resources| resources.local.is_some())
                    });
                    match invalidate {
                        Ok(true) => {
                            if let Err(error) = self.publish_semantic_model(None) {
                                eprintln!("Graphit model child resource retirement: {:?}", error.kind());
                            }
                        },
                        Ok(false) => (),
                        Err(error) => eprintln!("Graphit model child resource inspection: {:?}", error.kind()),
                    }
                    let jobs = self.memory_jobs.as_mut().expect("observed model owner");
                    if let Err(error) = jobs.shutdown().await {
                        eprintln!("Graphit model child cleanup: kind={:?}", error.kind());
                    }
                    let retired = jobs.retirement_complete();
                    if retired {
                        self.memory_jobs = None;
                    }
                    #[cfg(all(test, target_os = "linux"))]
                    self.model_child_exits.send_replace(Some(retired));
                }
                batch = async { self.external_watcher.as_mut().expect("configured external control watcher").next().await }, if self.external_watcher.is_some() => {
                    let observed: io::Result<()> = match batch {
                        Some(mut batch) => match batch.error.take() {
                            Some(error) => Err(error),
                            None => {
                                if batch.controls_changed { model_changes.notify_one(); }
                                continue;
                            },
                        },
                        None => Err(io::Error::new(io::ErrorKind::BrokenPipe, "external control observation ended")),
                    };
                    if let Some(now) = &self.now { now.cancel(); }
                    if let Some(dream) = &self.dream { dream.cancel(); }
                    if let Some(user) = &self.user_dream { user.cancel(); }
                    let stopped = server.shutdown().await;
                    let joined = served.await;
                    break (observed.and(stopped).and(joined), "external_watcher")
                }
                () = model_changes.notified() => {
                    let result = {
                        let refresh = self.refresh_models(service);
                        tokio::pin!(refresh);
                        tokio::select! {
                            biased;
                            () = service.cancelled() => None,
                            result = &mut refresh => Some(result),
                        }
                    };
                    #[cfg(all(test, target_os = "linux"))]
                    if let Some(result) = &result {
                        self.model_refreshes.send_replace(Some(result.as_ref().map(|_| ()).map_err(|error| error.kind())));
                    }
                    if let Some(Err(error)) = result {
                        use std::io::Write;
                        let _ = writeln!(io::stderr().lock(), "Graphit global model refresh: {error}");
                    }
                }
                command = async { self.project_control.as_mut().expect("configured PROJECT control").requests.recv().await }, if self.project_control.is_some() => {
                    match command {
                        Some(crate::foreground_project_control::OwnerCommand::Project(command)) => {
                            let response = {
                                let operation = self.execute_project_command(&command);
                                tokio::pin!(operation);
                                tokio::select! {
                                    biased;
                                    () = service.cancelled() => {
                                        command.cancellation.cancel();
                                        operation.await
                                    },
                                    result = &mut operation => result,
                                }
                            };
                            let _ = command.response.send(response);
                        },
                        Some(crate::foreground_project_control::OwnerCommand::LocalModel(command)) => {
                            let response = {
                                let operation = self.ensure_local_model(&command);
                                tokio::pin!(operation);
                                tokio::select! {
                                    biased;
                                    () = service.cancelled() => {
                                        command.original.cancellation().cancel();
                                        operation.await
                                    },
                                    result = &mut operation => result,
                                }
                            };
                            let _ = command.response.send(response);
                        },
                        None => {},
                    }
                }

                result = async { self.now.as_mut().expect("configured Foreground Now").next().await }, if self.now.is_some() => {
                    let (result, selected_branch) = match result {
                        Err(error) if ForegroundNow::needs_native_refresh(&error) => {
                            let refreshed = match self.projects.as_ref() {
                                Some(desired) => self.now.as_mut().expect("configured Foreground Now")
                                    .refresh_native(Arc::clone(desired), service).await,
                                None => Err(io::ErrorKind::InvalidInput.into()),
                            };
                            match refreshed {
                                Ok(()) => continue,
                                Err(error) => (Err(error), "now_refresh"),
                            }
                        }
                        Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::TimedOut)
                            || error.get_ref().is_some_and(|cause| {
                                cause.is::<graphit_daemon::now_publisher::NowPublisherError>()
                                    || cause.is::<graphit_auth::ServiceUseError>()
                            }) => {
                            // Next has joined and removed this publisher. Its terminal
                            // PROJECT failure must not cancel sibling or local owners.
                            // Join/owner failures remain on the Source shutdown path.
                            eprintln!("Graphit Now publisher terminal: {error}");
                            continue;
                        }
                        result => (result, "now"),
                    };
                    if let Some(now) = &self.now { now.cancel(); }
                    if let Some(dream) = &self.dream { dream.cancel(); }
                    if let Some(user) = &self.user_dream { user.cancel(); }
                    let stopped = server.shutdown().await;
                    let joined = served.await;
                    break (result.and(stopped).and(joined), selected_branch)
                }
                // Administrative cleanup can retain a stopped Dream owner for retry.
                result = async { self.dream.as_mut().expect("configured Foreground Dream").next().await }, if self.dream.as_ref().is_some_and(|dream| !dream.is_stopping()) => {
                    if let Some(dream) = &self.dream { dream.cancel(); }
                    if let Some(user) = &self.user_dream { user.cancel(); }
                    if let Some(now) = &self.now { now.cancel(); }
                    let stopped = server.shutdown().await;
                    let joined = served.await;
                    break (result.and(stopped).and(joined), "dream")
                }
                result = async { self.user_dream.as_mut().expect("configured Foreground USER Dream").next().await }, if self.user_dream.is_some() => {
                    if let Some(user) = &self.user_dream { user.cancel(); }
                    if let Some(dream) = &self.dream { dream.cancel(); }
                    if let Some(now) = &self.now { now.cancel(); }
                    let stopped = server.shutdown().await;
                    let joined = served.await;
                    break (result.and(stopped).and(joined), "user_dream")
                }
            }
        };
        {
            use std::io::Write;
            let _ = writeln!(
                io::stderr().lock(),
                "Graphit Source terminal: selected_branch={selected_branch} error_kind={:?}",
                result.as_ref().err().map(io::Error::kind)
            );
        }
        if let Some(now) = &self.now {
            now.cancel();
        }
        if let Some(dream) = &self.dream {
            dream.cancel();
        }
        if let Some(user) = &self.user_dream {
            user.cancel();
        }
        match self.shutdown().await {
            Ok(()) => result.map_err(ForegroundFailure::from),
            Err(mut failure) => {
                if let Err(error) = result {
                    failure.service = Some(error);
                }
                Err(failure)
            }
        }
    }

    /// Retry cleanup using the retained owner from a failed startup or shutdown.
    pub async fn shutdown(mut self) -> Result<(), ForegroundFailure> {
        if let Some(mut watcher) = self.external_watcher.take() {
            watcher.stop();
        }
        self.listener.take();
        let mut first_error = None;
        if let Some(user) = self.user_dream.as_mut()
            && let Err(error) = user.shutdown().await
        {
            first_error.get_or_insert(error);
        }
        if let Some(dream) = self.dream.as_mut()
            && let Err(error) = dream.shutdown().await
        {
            first_error.get_or_insert(error);
        }
        if let Some(now) = self.now.as_mut()
            && let Err(error) = now.shutdown().await
        {
            first_error.get_or_insert(error);
        }
        if let Err(error) = self.supervisors.shutdown().await {
            first_error.get_or_insert(error);
        }
        if let Some(ast) = &self.ast_jobs
            && let Err(error) = ast.shutdown().await
        {
            first_error.get_or_insert(error);
        }
        if let Some(memory) = self.memory_jobs.as_mut()
            && let Err(error) = memory.shutdown().await
        {
            first_error.get_or_insert(error);
        }
        if let Some(server) = &self.server
            && let Err(error) = server.shutdown().await
        {
            first_error.get_or_insert(error);
        }
        if let Some(tasks) = &self.tasks
            && let Err(error) = tasks.shutdown().await
        {
            first_error.get_or_insert(error);
        }
        if let Some(error) = first_error {
            return Err(ForegroundFailure {
                error,
                owner: Some(Box::new(self)),
                runtime: None,
                service: None,
            });
        }
        let cancellation = Cancellation::default();
        let cleanup = match self.policy.request_context(Instant::now(), &cancellation) {
            Ok(cleanup) => cleanup,
            Err(error) => {
                return Err(ForegroundFailure {
                    error: io_error(error),
                    owner: Some(Box::new(self)),
                    runtime: None,
                    service: None,
                });
            }
        };
        if let Some(record) = &self.record {
            match source_discovery::remove_discovery(
                &self.auth,
                record,
                self.native,
                cleanup.snapshot(),
            ) {
                Ok(true) => self.record = None,
                Ok(false) => {
                    first_error.get_or_insert_with(|| {
                        io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "Source discovery changed before cleanup",
                        )
                    });
                }
                Err(error) => {
                    first_error.get_or_insert_with(|| io_error(error));
                }
            }
        }
        if let Some(error) = first_error {
            return Err(ForegroundFailure {
                error,
                owner: Some(Box::new(self)),
                runtime: None,
                service: None,
            });
        }
        let outcome = self.auth.remove_runtime(cleanup.snapshot());
        if let Some(error) = outcome.failure().or(outcome.cleanup_failure()) {
            let owner = match outcome.effects().key {
                RuntimeKeyEffect::Applied => None,
                RuntimeKeyEffect::NotApplied | RuntimeKeyEffect::Unknown => Some(Box::new(self)),
            };
            return Err(ForegroundFailure {
                error: io_error(error),
                owner,
                runtime: Some(outcome),
                service: None,
            });
        }
        // No request, corpus or project jobs remain; owned SDK resources now drop.
        Ok(())
    }
}

pub(super) fn query_options(
    config: &ConfigSnapshot,
    content: graphit_hub::native::ContentLimits,
) -> io::Result<graphit_ast::icebug_query::QueryOptions> {
    // Zero delegates sizing to the SDK's available-memory default.
    let buffer_pool_bytes = config
        .get::<u64>(&ConfigKey::new("ast.query.buffer_pool_bytes")?)?
        .unwrap_or(0);
    let query_threads = config
        .get::<u64>(&ConfigKey::new("ast.query.threads")?)?
        .unwrap_or(2);
    if (buffer_pool_bytes != 0 && buffer_pool_bytes < 4096) || query_threads == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ast.query requires buffer_pool_bytes = 0 (SDK default) or >= 4096, and threads > 0",
        ));
    }
    Ok(graphit_ast::icebug_query::QueryOptions {
        buffer_pool_bytes,
        query_threads,
        max_rows: content.max_collection_entries,
        max_bytes: content.max_output_bytes,
        max_depth: content.max_depth,
    })
}

#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_memory_jobs/tests.rs"]
pub(crate) mod foreground_memory_jobs_tests;

#[cfg(test)]
mod query_config_tests {
    use super::*;
    use graphit_core::config::{ConfigDocument, ConfigSource};
    #[test]
    fn unicast_listener_config_is_rejected_before_installation_effects() {
        for host in ["192.0.2.1", "2001:db8::1"] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("absent-installation");
            let brand = graphit_core::brand::Brand::default();
            let installation =
                InstallationPaths::resolve(brand.clone(), directory.path(), None, Some(&root))
                    .unwrap();
            let config = ConfigSnapshot::new(
                &brand,
                [ConfigDocument::new(
                    &brand,
                    ConfigSource::Command,
                    [(ConfigKey::new("mcp.host").unwrap(), serde_json::json!(host))],
                )
                .unwrap()],
            )
            .unwrap();
            let policy = HubResourcePolicy::resolve(&config).unwrap();
            let cancel = Cancellation::default();
            let original = policy.request_context(Instant::now(), &cancel).unwrap();
            let error = ForegroundSource::start(
                &installation,
                &[],
                0,
                &config,
                NativeBinding::Codex,
                &original,
            )
            .err()
            .unwrap();
            assert_eq!(error.error.kind(), io::ErrorKind::InvalidInput);
            assert!(!root.exists());
        }
    }

    #[test]
    fn inventory_startup_requires_finite_original_before_installation_preparation() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("absent-installation");
        let brand = graphit_core::brand::Brand::default();
        let installation =
            InstallationPaths::resolve(brand.clone(), directory.path(), None, Some(&root)).unwrap();
        let config = ConfigSnapshot::new(&brand, []).unwrap();
        let signal = Cancellation::default();
        let original = RequestContext::service(
            HubResourcePolicy::resolve(&config).unwrap().content(),
            &signal,
        )
        .unwrap();
        let error = active_provider(&installation, &config, &original)
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!root.exists());
    }

    #[test]
    fn query_config_defaults_overrides_and_actual_response_bounds() {
        let brand = graphit_core::brand::Brand::default();
        let empty = ConfigSnapshot::new(&brand, []).unwrap();
        let content = HubResourcePolicy::resolve(&empty).unwrap().content();
        let options = query_options(&empty, content).unwrap();
        assert_eq!(options.buffer_pool_bytes, 0);
        assert_eq!(options.query_threads, 2);
        assert_eq!(options.max_rows, content.max_collection_entries);
        assert_eq!(options.max_bytes, content.max_output_bytes);
        assert_eq!(options.max_depth, content.max_depth);
        let document = ConfigDocument::new(
            &brand,
            ConfigSource::Command,
            [
                (
                    ConfigKey::new("ast.query.buffer_pool_bytes").unwrap(),
                    serde_json::json!(128 * 1024 * 1024),
                ),
                (
                    ConfigKey::new("ast.query.threads").unwrap(),
                    serde_json::json!(3),
                ),
            ],
        )
        .unwrap();
        let configured = ConfigSnapshot::new(&brand, [document]).unwrap();
        let options = query_options(&configured, content).unwrap();
        assert_eq!(options.buffer_pool_bytes, 128 * 1024 * 1024);
        assert_eq!(options.query_threads, 3);
    }
    #[test]
    fn query_config_rejects_native_invalid_values_without_startup_effects() {
        let brand = graphit_core::brand::Brand::default();
        for (key, value) in [
            ("ast.query.buffer_pool_bytes", serde_json::json!(4095)),
            ("ast.query.threads", serde_json::json!(0)),
            ("ast.query.threads", serde_json::json!(-1)),
            ("ast.query.threads", serde_json::json!("many")),
        ] {
            let document = ConfigDocument::new(
                &brand,
                ConfigSource::Command,
                [(ConfigKey::new(key).unwrap(), value)],
            )
            .unwrap();
            let config = ConfigSnapshot::new(&brand, [document]).unwrap();
            let content = HubResourcePolicy::resolve(&config).unwrap().content();
            assert!(query_options(&config, content).is_err(), "{key}");
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_native_publication_tests.rs"]
pub(crate) mod foreground_native_publication_tests;

#[cfg(test)]
#[path = "foreground_source/startup_tests.rs"]
mod startup_tests;
