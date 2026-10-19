//! Opt-in PROJECT Dream over the existing native proposal and automatic Records SDKs.
use crate::{managed_roles, role_tool_server::RoleToolPolicy, source_server::SourcePolicy};
use graphit_ai::{cli_codex_wire::CodexWireLimits, codex_completion::CompletionLimits};
use graphit_auth::{
    AdminSessionHandle, AuthError, IssuerLimits, Permission, ProjectCapture, ProjectReadModule,
    ProjectRecordAuthority, ProjectWriteOperation, RoleDefinition, RunHandle, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::CapturedPublicationScope,
    config::{ConfigKey, ConfigSnapshot},
    file_lock::Cancellation,
    module_settings::Module,
    project_identity::ProjectId,
    project_paths::ProjectPaths,
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::DaemonAuth,
    dream::{self, Application},
    supervisor::ProjectContext,
};
use graphit_hub::{
    ArtifactType, ConcreteVersion, NativeBinding, SourceRef,
    native::{DocumentFormat, FieldValue, NativeDocument, Role, RoleCredentialReference},
    policy::RequestContext,
};
use graphit_process::{
    HostLimits, HostReadiness, HostReport, HostShutdown, HostShutdownObservation, PendingHost,
    ProcessHost, RemainingProcess, RepairObservation, Request,
};
use graphit_records::memory_store::{Memory, MemoryListFilter, MemoryListRead, NativeMemoryStore};
use graphit_runtime::{
    instructions::{InstructionOrigin, InstructionValue, InstructionVariable, render_instruction},
    policy_instructions::{PolicyPrompt, policy_prompt},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fmt, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::watch, task::JoinHandle};
use zeroize::Zeroizing;

mod user;
pub(super) use user::ForegroundUserDream;

type Projects = Vec<(ProjectPaths, ProjectId, VersionContext)>;
const PERMISSIONS: [Permission; 2] = [
    Permission::Read(Module::Memory),
    Permission::WriteFencedMemory,
];

pub(super) fn enabled(config: &ConfigSnapshot) -> io::Result<bool> {
    config
        .get::<bool>(&ConfigKey::new("dream.enabled")?)
        .map(|v| v.unwrap_or(false))
}
struct Settings {
    model: String,
    program: String,
    idle: Duration,
    duration: Duration,
    retries: u32,
    backoff: Duration,
}
impl Settings {
    fn text(config: &ConfigSnapshot, key: &str) -> io::Result<String> {
        config
            .get::<String>(&ConfigKey::new(key)?)?
            .filter(|value| !value.trim().is_empty() && !value.contains('\0'))
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }
    fn capture(config: &ConfigSnapshot) -> io::Result<Self> {
        let text = |key: &str| Self::text(config, key);
        let seconds = |key: &str, default| -> io::Result<Duration> {
            let n = config.get::<u64>(&ConfigKey::new(key)?)?.unwrap_or(default);
            let d = Duration::from_secs(n);
            if n == 0 || Instant::now().checked_add(d).is_none() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            Ok(d)
        };
        Ok(Self {
            model: text("dream.model")?,
            program: text("dream.program")?,
            idle: seconds("dream.idle_timeout", 7200)?,
            duration: seconds("dream.max_duration", 600)?,
            retries: config.get(&ConfigKey::new("dream.retries")?)?.unwrap_or(2),
            backoff: seconds("dream.backoff", 30)?,
        })
    }
}
struct Plan {
    #[cfg(test)]
    iteration_completed: Arc<tokio::sync::Notify>,
    auth: Arc<DaemonAuth>,
    projects: Arc<Projects>,
    selected: usize,
    policy: RoleToolPolicy,
    settings: Settings,
    agent: String,
    source: SourceRef,
    policy_hash: [u8; 32],
    reference: RoleCredentialReference,
}
struct RunResidual {
    session: AdminSessionHandle,
    run: RunHandle,
}
#[derive(Default)]
struct AttemptOwner {
    run: Option<RunResidual>,
    application: Option<Application>,
    remaining: Option<RemainingProcess>,
    cwd: Option<tempfile::TempDir>,
    cleanup_error: Option<io::Error>,
}
struct JobExit {
    owner: AttemptOwner,
    error: Option<io::Error>,
}
struct Job {
    context: ProjectContext,
    activity: watch::Sender<Instant>,
    receiver: Option<watch::Receiver<Instant>>,
    join: Option<JoinHandle<JobExit>>,
    exit: Option<JobExit>,
}
enum HostOwner {
    Live(Arc<ProcessHost>),
    Joining(JoinHandle<HostShutdown>),
    Pending(PendingHost),
    Joined(HostReport),
    Failed(Arc<tokio::task::JoinError>),
}
pub(super) struct ForegroundDream {
    plan: Arc<Plan>,
    stop: Cancellation,
    host_stop: Cancellation,
    host: Option<HostOwner>,
    jobs: Vec<Job>,
}
impl ForegroundDream {
    pub fn new(
        auth: Arc<DaemonAuth>,
        projects: Arc<Projects>,
        selected: usize,
        source_policy: SourcePolicy,
        config: ConfigSnapshot,
        original: &RequestContext<'_>,
    ) -> io::Result<Option<Self>> {
        if !enabled(&config)? {
            return Ok(None);
        }
        if source_policy.binding != NativeBinding::Codex {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Dream requires Codex binding",
            ));
        }
        let settings = Settings::capture(&config)?;
        let agent = Settings::text(&config, "dream.agent")?;
        let agent_version: Option<String> = config.get(&ConfigKey::new("dream.agent_version")?)?;
        let policy = RoleToolPolicy::capture(source_policy, config)?;
        let (paths, id, version) = projects.get(selected).ok_or(io::ErrorKind::InvalidInput)?;
        let caller = runtime_caller(&auth, original)?;
        let session = auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(io::Error::other)?;
        let parent = auth
            .issuer()
            .admin_parent(&caller, &session)
            .map_err(io::Error::other)?;
        parent
            .authorize(Permission::Read(Module::Hub))
            .map_err(io::Error::other)?;
        let document = policy.source.registry.load()?;
        original.check()?;
        let artifact = if let Some(version) = &agent_version {
            let version =
                ConcreteVersion::new(version, policy.source.native_install.hub.projection())
                    .map_err(io::Error::other)?;
            document.get_version(ArtifactType::Agent, &agent, &version)
        } else {
            document.get(ArtifactType::Agent, &agent)
        }
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "selected Dream Hub Agent unavailable or ambiguous",
            )
        })?;
        let mut installations = artifact
            .installations
            .iter()
            .filter(|i| i.target.includes(NativeBinding::Codex));
        let installed = installations.next().ok_or(io::ErrorKind::NotFound)?;
        if installations.next().is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let source = installed.original_source.clone();
        let observed = capture_agent(&source, paths, &policy, original)?;
        let body = observed.body.as_deref().ok_or(io::ErrorKind::NotFound)?;
        let mut renderer = graphit_hub::native::RenderContext::new(
            policy.source.content,
            original.cancellation(),
            original.snapshot().required_deadline()?,
        )
        .map_err(io::Error::other)?;
        graphit_hub::native::AgentDescriptor::parse(body.as_bytes(), &mut renderer)
            .map_err(io::Error::other)?;
        let policy_hash: [u8; 32] = Sha256::digest(body.as_bytes()).into();
        let definition = RoleDefinition::new(
            Role::Dream,
            &source,
            NativeBinding::Codex,
            &PERMISSIONS,
            policy_hash,
            IssuerLimits {
                max_permissions: policy.source.content.max_collection_entries,
                max_scope_bytes: policy.source.content.max_input_bytes,
                max_parent_bearer_bytes: policy.source.content.max_scalar_bytes,
            },
        )
        .map_err(io::Error::other)?;
        let reference = definition.reference().clone();
        observed.revalidate(original)?;
        auth.issuer()
            .register_or_reuse_hub_definition(&parent, definition)
            .map_err(io::Error::other)?;
        let context = ProjectContext {
            id: *id,
            version: version.clone(),
        };
        let (activity, receiver) = watch::channel(Instant::now());
        Ok(Some(Self {
            plan: Arc::new(Plan {
                #[cfg(test)]
                iteration_completed: Arc::new(tokio::sync::Notify::new()),
                auth,
                projects,
                selected,
                policy,
                settings,
                agent,
                source,
                policy_hash,
                reference,
            }),
            stop: Cancellation::default(),
            host_stop: Cancellation::default(),
            host: None,
            jobs: vec![Job {
                context,
                activity,
                receiver: Some(receiver),
                join: None,
                exit: None,
            }],
        }))
    }
    pub fn activity_sender(&self, selected: &ProjectContext) -> Option<watch::Sender<Instant>> {
        self.jobs
            .iter()
            .find(|job| {
                job.context.id == selected.id
                    && job.context.version == selected.version
                    && job.context.root() == selected.root()
            })
            .map(|job| job.activity.clone())
    }
    pub async fn start(
        &mut self,
        environment: Vec<(OsString, OsString)>,
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        if self.host.is_some() || self.stop.is_cancelled() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let program = resolve_program(
            &self.plan.settings.program,
            &environment,
            self.plan.policy.source.installation.root(),
        )?;
        let host = Arc::new(ProcessHost::install(
            &self.host_stop,
            HostLimits::new(1, 1)?,
        )?);
        self.host = Some(HostOwner::Live(Arc::clone(&host)));
        match host.ready(original.snapshot().required_deadline()?).await {
            HostReadiness::Ready => (),
            HostReadiness::Pending => return Err(io::ErrorKind::TimedOut.into()),
            HostReadiness::Failed(error) => {
                return Err(io::Error::other(format!("Dream host readiness: {error:?}")));
            }
            HostReadiness::Sealed => return Err(io::ErrorKind::BrokenPipe.into()),
        }
        for job in &mut self.jobs {
            let activity = job.receiver.take().ok_or(io::ErrorKind::AlreadyExists)?;
            let plan = Arc::clone(&self.plan);
            let host = Arc::clone(&host);
            let stop = self.stop.clone();
            let program = program.clone();
            let environment = environment.clone();
            job.join = Some(tokio::spawn(async move {
                run(plan, host, activity, stop, program, environment).await
            }));
        }
        Ok(())
    }
    pub(crate) fn is_stopping(&self) -> bool {
        self.stop.is_cancelled()
    }
    pub fn cancel(&self) {
        self.stop.cancel();
    }
    #[cfg(test)]
    pub(super) fn iteration_completion(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.plan.iteration_completed)
    }
    #[cfg(test)]
    pub(super) fn retained_application(
        &self,
    ) -> Option<(
        &Application,
        &RunHandle,
        &AdminSessionHandle,
        &RoleCredentialReference,
    )> {
        let owner = &self.jobs.first()?.exit.as_ref()?.owner;
        let run = owner.run.as_ref()?;
        Some((
            owner.application.as_ref()?,
            &run.run,
            &run.session,
            &self.plan.reference,
        ))
    }
    pub async fn next(&mut self) -> io::Result<()> {
        let job = self.jobs.first_mut().ok_or(io::ErrorKind::InvalidInput)?;
        if let Some(join) = job.join.as_mut() {
            let exit = match join.await {
                Ok(exit) => exit,
                Err(error) => JobExit {
                    owner: AttemptOwner::default(),
                    error: Some(io::Error::other(DreamCause::Join(Arc::new(error)))),
                },
            };
            job.join = None;
            job.exit = Some(exit);
        }
        match job.exit.as_mut().and_then(|exit| exit.error.take()) {
            Some(error) => Err(error),
            None if self.stop.is_cancelled() => Ok(()),
            None => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Dream job ended",
            )),
        }
    }
    /// Joins remain borrowed until observed; interrupted cleanup retains each owner.
    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.cancel();
        for job in &mut self.jobs {
            if let Some(join) = job.join.as_mut() {
                let exit = match join.await {
                    Ok(exit) => exit,
                    Err(error) => JobExit {
                        owner: AttemptOwner::default(),
                        error: Some(io::Error::other(DreamCause::Join(Arc::new(error)))),
                    },
                };
                job.join = None;
                job.exit = Some(exit);
            }
            if let Some(exit) = &mut job.exit
                && let Some(remaining) = exit.owner.remaining.as_mut()
            {
                let deadline = cleanup_deadline(&self.plan)?;
                match remaining.repair(deadline).await {
                    RepairObservation::Observed {
                        cleanup:
                            graphit_process::Cleanup::Complete | graphit_process::Cleanup::NotSpawned,
                        ..
                    } => {
                        exit.owner.remaining = None;
                        exit.owner.cwd = None;
                    }
                    RepairObservation::Observed { cleanup, .. } => {
                        // Observed ends this observer, including terminal Unknown reports.
                        // The host still drains its guards; retain cwd until that join.
                        exit.owner.cleanup_error.get_or_insert_with(|| {
                            io::Error::other(format!(
                                "Dream process cleanup remains unconfirmed: {cleanup:?}"
                            ))
                        });
                        exit.owner.remaining = None;
                    }
                    RepairObservation::Pending { .. } => (),
                }
            }
        }
        let host_result = shutdown_host(&mut self.host, self.plan.policy.source.timeout).await;
        for job in &mut self.jobs {
            if let Some(exit) = &mut job.exit {
                if exit.owner.remaining.is_some() || self.host.is_some() {
                    continue;
                }
                // The observer and host have finished. This does not turn an
                // unconfirmed cleanup report into successful native reaping.
                exit.owner.cwd = None;
                if let Some(run) = exit.owner.run.as_ref() {
                    let closed = close_run(&self.plan, run);
                    // Physical ownership has drained; release inert proposal data
                    // independently of whether Auth can formally close this old handle.
                    exit.owner.application = None;
                    match closed {
                        Ok(()) => {
                            exit.owner.run = None;
                        }
                        Err(error)
                            if error
                                .get_ref()
                                .and_then(|e| e.downcast_ref::<AuthError>())
                                .is_some_and(|e| {
                                    matches!(e, AuthError::Denied | AuthError::Expired)
                                }) =>
                        {
                            exit.owner.run = None;
                            exit.owner.cleanup_error.get_or_insert(error);
                        }
                        Err(error) => {
                            exit.owner.cleanup_error.get_or_insert(error);
                        }
                    }
                }
            }
        }
        // Consume diagnostics only after the last await, so interrupted shutdown
        // cannot lose the original job failure while the host still owns cleanup.
        for job in &mut self.jobs {
            if let Some(error) = job.exit.as_mut().and_then(|exit| exit.error.take()) {
                return Err(error);
            }
        }
        for job in &mut self.jobs {
            if let Some(error) = job
                .exit
                .as_mut()
                .and_then(|exit| exit.owner.cleanup_error.take())
            {
                return Err(error);
            }
        }
        host_result?;
        if self.jobs.iter().any(|job| {
            job.exit.as_ref().is_some_and(|exit| {
                exit.owner.remaining.is_some()
                    || exit.owner.cwd.is_some()
                    || exit.owner.run.is_some()
            })
        }) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(())
    }
}

impl Drop for ForegroundDream {
    fn drop(&mut self) {
        self.cancel();
        self.host_stop.cancel();
    }
}

fn runtime_caller<'a>(
    auth: &'a DaemonAuth,
    original: &'a RequestContext<'a>,
) -> io::Result<graphit_auth::AuthenticatedCaller<'a>> {
    let key = auth
        .runtime()
        .check(original.snapshot())
        .map_err(io::Error::other)?;
    let raw = std::str::from_utf8(key.secret(original.snapshot()).map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    auth.issuer()
        .authenticate_runtime(raw, original)
        .map_err(io::Error::other)
}
fn cleanup_deadline(plan: &Plan) -> io::Result<Instant> {
    Instant::now()
        .checked_add(plan.policy.source.timeout)
        .ok_or_else(|| io::ErrorKind::InvalidInput.into())
}
fn close_run(plan: &Plan, run: &RunResidual) -> io::Result<()> {
    let stop = Cancellation::default();
    let original = RequestContext::new(plan.policy.source.content, &stop, cleanup_deadline(plan)?)
        .map_err(io::Error::other)?;
    let caller = runtime_caller(&plan.auth, &original)?;
    let parent = plan
        .auth
        .issuer()
        .admin_parent(&caller, &run.session)
        .map_err(io::Error::other)?;
    plan.auth
        .issuer()
        .close_run(&parent, &run.run)
        .map_err(io::Error::other)
}
fn capture_agent(
    source: &SourceRef,
    paths: &ProjectPaths,
    policy: &RoleToolPolicy,
    original: &RequestContext<'_>,
) -> io::Result<managed_roles::TemplateObservation> {
    let root = source.directory().as_path();
    if !root.starts_with(paths.root()) && !root.starts_with(policy.source.installation.root()) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    managed_roles::capture_template(
        root,
        Path::new("AGENT.md"),
        InstructionOrigin::Hub,
        policy.source.index,
        original,
    )
}
fn resolve_program(
    program: &str,
    environment: &[(OsString, OsString)],
    cwd: &Path,
) -> io::Result<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    if path.components().count() != 1 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let paths = environment
        .iter()
        .find(|(key, _)| key == OsStr::new("PATH"))
        .map(|(_, value)| value)
        .ok_or(io::ErrorKind::NotFound)?;
    which::which_in(program, Some(paths), cwd).map_err(io::Error::other)
}
async fn run(
    plan: Arc<Plan>,
    host: Arc<ProcessHost>,
    mut activity: watch::Receiver<Instant>,
    stop: Cancellation,
    program: PathBuf,
    environment: Vec<(OsString, OsString)>,
) -> JobExit {
    let mut owner = AttemptOwner::default();
    'activity: loop {
        let generation = *activity.borrow_and_update();
        let Some(due) = generation.checked_add(plan.settings.idle) else {
            return JobExit {
                owner,
                error: Some(io::ErrorKind::InvalidInput.into()),
            };
        };
        tokio::select! {
            biased;
            () = stop.cancelled() => return JobExit { owner, error: None },
            changed = activity.changed() => {
                if changed.is_err() { return JobExit { owner, error: Some(io::ErrorKind::BrokenPipe.into()) }; }
                continue;
            },
            () = tokio::time::sleep_until(due.into()) => (),
        }
        for attempt in 0..=plan.settings.retries {
            let result = perform(&plan, &host, &stop, &program, &environment, &mut owner).await;
            match result {
                Ok(()) => break,
                Err(error)
                    if stop.is_cancelled() || owner.remaining.is_some() || owner.cwd.is_some() =>
                {
                    return JobExit {
                        owner,
                        error: Some(error),
                    };
                }
                Err(error) if attempt == plan.settings.retries => {
                    if owner.application.is_none() && activity.has_changed().unwrap_or(false) {
                        if let Some(run) = owner.run.as_ref()
                            && let Err(error) = close_run(&plan, run)
                        {
                            return JobExit {
                                owner,
                                error: Some(error),
                            };
                        }
                        owner = AttemptOwner::default();
                        continue 'activity;
                    }
                    return JobExit {
                        owner,
                        error: Some(error),
                    };
                }
                Err(_) => {
                    // A prepared application keeps this same Run/Actor. Native-only
                    // failures close their run before a fresh proposal can be admitted.
                    if owner.application.is_none() {
                        if let Some(run) = owner.run.as_ref()
                            && let Err(error) = close_run(&plan, run)
                        {
                            return JobExit {
                                owner,
                                error: Some(error),
                            };
                        }
                        owner.run = None;
                    }
                    tokio::select! {
                        biased;
                        () = stop.cancelled() => return JobExit { owner, error: Some(io::ErrorKind::Interrupted.into()) },
                        () = tokio::time::sleep(plan.settings.backoff) => (),
                    }
                }
            }
        }
        if let Some(run) = owner.run.as_ref()
            && let Err(error) = close_run(&plan, run)
        {
            return JobExit {
                owner,
                error: Some(error),
            };
        }
        owner = AttemptOwner::default();
        #[cfg(test)]
        plan.iteration_completed.notify_one();
        // Only consume the generation that produced this result. A receipt-time
        // activity arriving during the operation remains due for its own idle wait.
        if activity.has_changed().unwrap_or(false) {
            continue;
        }
        tokio::select! {
            biased;
            () = stop.cancelled() => return JobExit { owner, error: None },
            changed = activity.changed() => {
                if changed.is_err() { return JobExit { owner, error: Some(io::ErrorKind::BrokenPipe.into()) }; }
            },
        }
    }
}

async fn perform(
    plan: &Plan,
    host: &ProcessHost,
    stop: &Cancellation,
    program: &Path,
    environment: &[(OsString, OsString)],
    owner: &mut AttemptOwner,
) -> io::Result<()> {
    let original = RequestContext::new(
        plan.policy.source.content,
        stop,
        Instant::now()
            .checked_add(plan.settings.duration)
            .ok_or(io::ErrorKind::InvalidInput)?,
    )
    .map_err(io::Error::other)?;
    let (paths, id, version) = &plan.projects[plan.selected];
    let captured = CapturedPublicationScope::project(
        paths,
        *id,
        plan.policy.source.scope,
        original.snapshot(),
    )?;
    let record = captured.binding()?;
    let scope = || ScopeInput {
        installation: plan.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::Codex,
        global_version: "",
    };
    let caller = runtime_caller(&plan.auth, &original)?;
    if owner.run.is_none() {
        let session = plan
            .auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(io::Error::other)?;
        let parent = plan
            .auth
            .issuer()
            .admin_parent(&caller, &session)
            .map_err(io::Error::other)?;
        let run = plan
            .auth
            .issuer()
            .start_run(&parent, &scope(), plan.settings.duration)
            .map_err(io::Error::other)?;
        owner.run = Some(RunResidual { session, run });
    }
    let run = owner.run.as_ref().ok_or(io::ErrorKind::InvalidInput)?;
    let parent = plan
        .auth
        .issuer()
        .admin_parent(&caller, &run.session)
        .map_err(io::Error::other)?;
    let bearer = plan
        .auth
        .issuer()
        .issue(&parent, &run.run, &plan.reference, &PERMISSIONS, &scope())
        .map_err(io::Error::other)?;
    let grant = plan
        .auth
        .issuer()
        .verify(
            bearer.expose_for_transport(),
            &plan.reference,
            scope(),
            &original,
        )
        .map_err(io::Error::other)?;
    let authority = ProjectRecordAuthority::new(plan.auth.issuer());
    let read = authority
        .admit_read(&grant, &original, ProjectReadModule::Memory)
        .map_err(io::Error::other)?;
    let write = authority
        .admit_write(&grant, &original, ProjectWriteOperation::FencedMemory)
        .map_err(io::Error::other)?;
    let agent = capture_agent(&plan.source, paths, &plan.policy, &original)?;
    let authored = agent.body.as_deref().ok_or(io::ErrorKind::NotFound)?;
    if <[u8; 32]>::from(Sha256::digest(authored.as_bytes())) != plan.policy_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Dream Agent changed after definition capture",
        ));
    }
    if owner.application.is_none() {
        let memories = match NativeMemoryStore::list(
            original.snapshot(),
            &read,
            MemoryListFilter::default(),
            plan.policy.memory_read,
        )
        .await
        .map_err(|e| io::Error::other(DreamCause::Memory(e)))?
        {
            MemoryListRead::NoCorpus => return Ok(()),
            MemoryListRead::Published { memories, .. } => memories,
        };
        if memories.is_empty() {
            return Ok(());
        }
        let corpus = Zeroizing::new(
            serde_json::to_string(&memories.iter().map(CorpusMemory::from).collect::<Vec<_>>())
                .map_err(io::Error::other)?,
        );
        let attempt_id = operation_id()?;
        let project_id = id.to_string();
        let set = policy_prompt(PolicyPrompt::DreamConsolidation);
        let prompt = render_instruction(
            set.entry(),
            set.candidates(),
            &[
                InstructionVariable {
                    name: "dream_session_id",
                    value: InstructionValue::Text(&attempt_id),
                },
                InstructionVariable {
                    name: "dream_agent",
                    value: InstructionValue::Text(&plan.agent),
                },
                InstructionVariable {
                    name: "project_id",
                    value: InstructionValue::Text(&project_id),
                },
                InstructionVariable {
                    name: "project_version",
                    value: InstructionValue::Text(version.version()),
                },
                InstructionVariable {
                    name: "memories_json",
                    value: InstructionValue::Text(&corpus),
                },
            ],
            plan.policy.instructions,
            original.snapshot(),
        )
        .map_err(io::Error::other)?;
        let config = managed_roles::capture_template(
            paths.root(),
            Path::new(".codex/config.toml"),
            InstructionOrigin::Project,
            plan.policy.source.index,
            &original,
        )?;
        let overrides = mcp_override(config.body.as_deref(), &plan.policy, &original)?;
        agent.revalidate(&original)?;
        config.revalidate(&original)?;
        parent
            .authorize(Permission::External(
                graphit_auth::ExternalEffect::NativeLaunch,
            ))
            .map_err(io::Error::other)?;
        let cwd = tempfile::Builder::new()
            .prefix("graphit-dream-")
            .tempdir()?;
        if cwd.path().starts_with(paths.root()) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let content = plan.policy.source.content;
        owner.cwd = Some(cwd);
        let request = Request {
            program: program.to_owned(),
            args: native_args(&plan.settings.model, overrides),
            cwd: owner
                .cwd
                .as_ref()
                .ok_or(io::ErrorKind::InvalidInput)?
                .path()
                .to_owned(),
            environment: environment.to_vec(),
            stdin: Some(Zeroizing::new(prompt.into_body().into_bytes())),
            limits: graphit_process::Limits {
                stdin_bytes: content.max_output_bytes,
                stdout_bytes: content.max_output_bytes,
                stderr_bytes: content.max_output_bytes,
                termination_grace: Duration::from_millis(100),
                cleanup: plan.policy.source.timeout,
            },
        };
        let proposal = dream::propose(
            original.snapshot(),
            host,
            request,
            &plan.settings.model,
            &CodexWireLimits {
                // The pinned SDK rejects frames above 8 MiB and depth above 128.
                frame_bytes: content.max_output_bytes.min(8 * 1024 * 1024),
                depth: content.max_depth.min(128),
                nodes: content.max_nodes,
                fields: content.max_collection_entries,
                array_items: content.max_collection_entries,
                string_bytes: content.max_output_bytes,
                decoded_string_bytes: content.max_output_bytes,
                id_bytes: content.max_scalar_bytes,
            },
            CompletionLimits {
                max_graphit_attempts: 1,
                max_response_bytes: content.max_output_bytes,
                max_model_id_bytes: content.max_scalar_bytes,
            },
        )
        .await;
        let mut proposal = match proposal {
            Ok(proposal) => proposal,
            Err(error) => {
                // CompletionRun construction failed before native submission.
                owner.cwd = None;
                return Err(io::Error::other(error));
            }
        };
        let succeeded = match proposal.process.as_mut() {
            Some(Ok(process)) => {
                owner.remaining = process.remaining.take();
                if matches!(process.cleanup, graphit_process::Cleanup::Complete)
                    && owner.remaining.is_none()
                {
                    owner.cwd = None;
                }
                process.status.success()
                    && matches!(process.cleanup, graphit_process::Cleanup::Complete)
                    && process.pipes.stdout == graphit_process::PipeEnd::Eof
                    && process.pipes.stderr == graphit_process::PipeEnd::Eof
            }
            Some(Err(process)) => {
                owner.remaining = process.remaining.take();
                if matches!(
                    process.cleanup,
                    graphit_process::Cleanup::Complete | graphit_process::Cleanup::NotSpawned
                ) && owner.remaining.is_none()
                {
                    owner.cwd = None;
                }
                false
            }
            None => {
                // run_codex returns no Process only before Process::run submission.
                owner.cwd = None;
                false
            }
        };
        if !succeeded || owner.remaining.is_some() || proposal.answer.is_none() {
            proposal.answer = None;
            return Err(io::Error::other(DreamCause::Native(Box::new(proposal))));
        }
        let answer = Zeroizing::new(proposal.answer.take().ok_or(io::ErrorKind::InvalidData)?);
        original.check()?;
        agent.revalidate(&original)?;
        config.revalidate(&original)?;
        let recipe = dream::recipe::prepare(
            &answer,
            &memories,
            &attempt_id,
            plan.policy.memory_write.max_input_bytes,
            original.snapshot(),
        )
        .map_err(|e| io::Error::other(DreamCause::Recipe(e)))?;
        owner.application = Some(Application::new(recipe));
    }
    agent.revalidate(&original)?;
    owner
        .application
        .as_mut()
        .ok_or(io::ErrorKind::InvalidInput)?
        .apply(original.snapshot(), &read, &write, plan.policy.memory_write)
        .await
        .map_err(|e| io::Error::other(DreamCause::Memory(e)))
}
fn operation_id() -> io::Result<String> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let mut output = String::from("dream-");
    for byte in random {
        use std::fmt::Write;
        write!(output, "{byte:02x}").map_err(io::Error::other)?;
    }
    Ok(output)
}
#[derive(Serialize)]
struct CorpusMemory<'a> {
    id: graphit_records::memory_store::MemoryId,
    revision: u64,
    title: &'a str,
    body: &'a str,
    classification: &'static str,
    mandatory: bool,
    important: bool,
}
impl<'a> From<&'a Memory> for CorpusMemory<'a> {
    fn from(memory: &'a Memory) -> Self {
        use graphit_records::memory_attributes::MemoryClass;
        Self {
            id: memory.id,
            revision: memory.revision,
            title: &memory.title,
            body: &memory.body,
            classification: match memory.attributes.classification {
                MemoryClass::Fact => "fact",
                MemoryClass::Skill => "skill",
                MemoryClass::Tension => "tension",
                MemoryClass::Decision => "decision",
                MemoryClass::Convention => "convention",
                MemoryClass::Correction => "correction",
            },
            mandatory: memory.attributes.mandatory,
            important: memory.attributes.important,
        }
    }
}
fn mcp_override(
    body: Option<&str>,
    policy: &RoleToolPolicy,
    original: &RequestContext<'_>,
) -> io::Result<Option<String>> {
    let Some(body) = body else {
        return Ok(None);
    };
    let document = NativeDocument::parse(
        DocumentFormat::Toml,
        body.as_bytes(),
        policy.source.native_install.hub.shared(),
        original,
    )
    .map_err(io::Error::other)?;
    let Some(value) = document.get(&["mcp_servers"]).map_err(io::Error::other)? else {
        return Ok(None);
    };
    let FieldValue::Object(servers) = value else {
        return Err(io::ErrorKind::InvalidData.into());
    };
    if servers.is_empty() {
        return Ok(None);
    }
    let mut output = String::from("mcp_servers={");
    for (index, name) in servers.keys().enumerate() {
        if name.is_empty()
            || name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if index != 0 {
            output.push(',');
        }
        output.push_str(&serde_json::to_string(name).map_err(io::Error::other)?);
        output.push_str("={enabled=false}");
    }
    output.push('}');
    if output.len() > policy.source.content.max_input_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(Some(output))
}
fn native_args(model: &str, mcp: Option<String>) -> Vec<OsString> {
    let mut args = [
        "exec",
        "--json",
        "--sandbox",
        "read-only",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--model",
        model,
        "--skip-git-repo-check",
        "-c",
        "features.apps=false",
        "-c",
        "features.shell_tool=false",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    if let Some(mcp) = mcp {
        args.push("-c".into());
        args.push(mcp.into());
    }
    args.push("-".into());
    args
}
enum DreamCause {
    Join(Arc<tokio::task::JoinError>),
    Native(Box<dream::NativeProposal>),
    Recipe(dream::recipe::RecipeError),
    Memory(graphit_records::memory_store::MemoryStoreError),
}
impl fmt::Debug for DreamCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native(proposal) => f
                .debug_struct("DreamNativeFailure")
                .field("reason", &proposal.reason)
                .field(
                    "completion",
                    &proposal.completion_failure.as_ref().map(|e| e.kind()),
                )
                .field("model_unavailable", &proposal.model_unavailable)
                .field("process", &proposal.process)
                .finish(),
            Self::Memory(error) => {
                use graphit_records::memory_store::MemoryStoreError;
                match error {
                    MemoryStoreError::Auth(cause) => {
                        f.debug_tuple("DreamMemoryAuth").field(cause).finish()
                    }
                    MemoryStoreError::Invalid(label) | MemoryStoreError::Limit(label) => {
                        f.debug_tuple("DreamMemoryPolicy").field(label).finish()
                    }
                    MemoryStoreError::RevisionConflict { expected, actual } => f
                        .debug_struct("DreamMemoryRevisionConflict")
                        .field("expected", expected)
                        .field("actual", actual)
                        .finish(),
                    _ => f
                        .debug_tuple("DreamMemoryFailure")
                        .field(&std::mem::discriminant(error))
                        .finish(),
                }
            }
            _ => fmt::Display::fmt(self, f),
        }
    }
}
impl fmt::Display for DreamCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Join(_) => "Dream job join failed",
            Self::Native(_) => "Dream native proposal failed",
            Self::Recipe(_) => "Dream recipe rejected",
            Self::Memory(_) => "Dream Memory operation failed",
        })
    }
}
impl std::error::Error for DreamCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Join(error) => Some(error.as_ref()),
            Self::Native(proposal) => proposal
                .completion_failure
                .as_ref()
                .map(|e| e as &dyn std::error::Error),
            Self::Recipe(error) => Some(error),
            Self::Memory(error) => Some(error),
        }
    }
}

async fn shutdown_host(owner: &mut Option<HostOwner>, timeout: Duration) -> io::Result<()> {
    if let Some(HostOwner::Live(host)) = owner.as_ref() {
        if Arc::strong_count(host) != 1 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let deadline = host_cleanup_deadline(timeout)?;
        let Some(HostOwner::Live(host)) = owner.take() else {
            unreachable!()
        };
        let host = Arc::try_unwrap(host).map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        *owner = Some(HostOwner::Joining(tokio::spawn(host.shutdown(deadline))));
    }
    if let Some(HostOwner::Joining(join)) = owner.as_mut() {
        let result = join.await;
        *owner = Some(match result {
            Ok(HostShutdown::Joined(report)) => HostOwner::Joined(report),
            Ok(HostShutdown::Pending(owner)) => HostOwner::Pending(owner),
            Err(error) => HostOwner::Failed(Arc::new(error)),
        });
    }
    if let Some(HostOwner::Pending(pending)) = owner.as_mut()
        && let HostShutdownObservation::Joined(report) =
            pending.repair(host_cleanup_deadline(timeout)?).await
    {
        *owner = Some(HostOwner::Joined(report));
    }
    match owner.as_ref() {
        Some(HostOwner::Joined(report)) => {
            let failure = report.failure();
            *owner = None;
            if let Some(error) = failure {
                return Err(io::Error::other(format!("Dream host: {error:?}")));
            }
        }
        Some(HostOwner::Failed(error)) => {
            let error = DreamCause::Join(Arc::clone(error));
            *owner = None;
            return Err(io::Error::other(error));
        }
        Some(_) => return Err(io::ErrorKind::WouldBlock.into()),
        None => (),
    }
    Ok(())
}
fn host_cleanup_deadline(timeout: Duration) -> io::Result<Instant> {
    Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| io::ErrorKind::InvalidInput.into())
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;

    #[tokio::test]
    async fn terminal_host_join_failure_is_reported_once() {
        let join: JoinHandle<HostShutdown> = tokio::spawn(async {
            panic!("Dream host shutdown task failed");
        });
        let mut owner = Some(HostOwner::Joining(join));
        let error = shutdown_host(&mut owner, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(
            matches!(error.get_ref().and_then(|error| error.downcast_ref::<DreamCause>()), Some(DreamCause::Join(join)) if join.is_panic())
        );
        assert!(
            owner.is_none(),
            "terminal join must not remain pending cleanup"
        );
        shutdown_host(&mut owner, Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn interrupted_host_shutdown_keeps_the_join_owner() {
        let stop = Cancellation::default();
        let host = ProcessHost::install(&stop, HostLimits::new(1, 1).unwrap()).unwrap();
        let gate = Arc::new(tokio::sync::Notify::new());
        let worker_gate = Arc::clone(&gate);
        let join = tokio::spawn(async move {
            worker_gate.notified().await;
            host.shutdown(Instant::now() + Duration::from_secs(1)).await
        });
        let mut owner = Some(HostOwner::Joining(join));
        {
            let mut closing = std::pin::pin!(shutdown_host(&mut owner, Duration::from_secs(1)));
            assert!(
                std::future::Future::poll(
                    closing.as_mut(),
                    &mut std::task::Context::from_waker(std::task::Waker::noop())
                )
                .is_pending()
            );
        }
        assert!(matches!(owner, Some(HostOwner::Joining(_))));
        gate.notify_one();
        shutdown_host(&mut owner, Duration::from_secs(1))
            .await
            .unwrap();
        assert!(owner.is_none());
        shutdown_host(&mut owner, Duration::from_secs(1))
            .await
            .unwrap();
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn original_job_failure_precedes_cleanup_and_host_failure() {
        let (_temp, source, mut dream) =
            crate::foreground_source::foreground_memory_jobs_tests::dream_shutdown_fixture()
                .await
                .unwrap();
        let joined: JoinHandle<JobExit> = tokio::spawn(async {
            panic!("original Dream job failed");
        });
        let joined = match joined.await {
            Err(error) => error,
            Ok(_) => panic!("job must fail with a real join error"),
        };
        let original_id = joined.id();
        dream.jobs[0].exit = Some(JobExit {
            owner: AttemptOwner {
                cleanup_error: Some(io::Error::other("later Dream cleanup failure")),
                ..AttemptOwner::default()
            },
            error: Some(io::Error::other(DreamCause::Join(Arc::new(joined)))),
        });
        dream.host = Some(HostOwner::Joining(tokio::spawn(async {
            panic!("later Dream host join failed");
        })));
        let error = dream.shutdown().await.unwrap_err();
        assert!(
            matches!(error.get_ref().and_then(|error| error.downcast_ref::<DreamCause>()), Some(DreamCause::Join(join)) if join.is_panic() && join.id() == original_id)
        );
        assert!(
            dream.host.is_none(),
            "host must drain before reporting the job"
        );
        assert!(dream.jobs[0].exit.as_ref().unwrap().error.is_none());
        let cleanup = dream.shutdown().await.unwrap_err();
        assert_eq!(cleanup.to_string(), "later Dream cleanup failure");
        dream.shutdown().await.unwrap();
        drop(dream);
        source.shutdown().await.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn interrupted_host_wait_preserves_the_job_error_and_cwd() {
        let (_temp, source, mut dream) =
            crate::foreground_source::foreground_memory_jobs_tests::dream_shutdown_fixture()
                .await
                .unwrap();
        let joined: JoinHandle<JobExit> = tokio::spawn(async {
            panic!("Dream job failed before interrupted host wait");
        });
        let joined = match joined.await {
            Err(error) => error,
            Ok(_) => panic!("job must fail with a real join error"),
        };
        let original_id = joined.id();
        let cwd = tempfile::tempdir().unwrap();
        let cwd_path = cwd.path().to_path_buf();
        dream.jobs[0].exit = Some(JobExit {
            owner: AttemptOwner {
                cwd: Some(cwd),
                cleanup_error: Some(io::Error::other("terminal Dream cleanup is unconfirmed")),
                ..AttemptOwner::default()
            },
            error: Some(io::Error::other(DreamCause::Join(Arc::new(joined)))),
        });
        let stop = Cancellation::default();
        let host = ProcessHost::install(&stop, HostLimits::new(1, 1).unwrap()).unwrap();
        let gate = Arc::new(tokio::sync::Notify::new());
        let worker_gate = Arc::clone(&gate);
        dream.host = Some(HostOwner::Joining(tokio::spawn(async move {
            worker_gate.notified().await;
            host.shutdown(Instant::now() + Duration::from_secs(1)).await
        })));
        {
            let mut closing = std::pin::pin!(dream.shutdown());
            assert!(
                std::future::Future::poll(
                    closing.as_mut(),
                    &mut std::task::Context::from_waker(std::task::Waker::noop())
                )
                .is_pending()
            );
        }
        assert!(dream.jobs[0].exit.as_ref().unwrap().error.is_some());
        assert!(matches!(dream.host, Some(HostOwner::Joining(_))));
        assert!(cwd_path.is_dir(), "host still owns physical cleanup");
        assert!(dream.jobs[0].exit.as_ref().unwrap().owner.cwd.is_some());
        gate.notify_one();
        let error = dream.shutdown().await.unwrap_err();
        assert!(
            matches!(error.get_ref().and_then(|error| error.downcast_ref::<DreamCause>()), Some(DreamCause::Join(join)) if join.is_panic() && join.id() == original_id)
        );
        assert!(dream.host.is_none());
        assert!(!cwd_path.exists(), "cwd releases only after the host join");
        let cleanup = dream.shutdown().await.unwrap_err();
        assert_eq!(cleanup.to_string(), "terminal Dream cleanup is unconfirmed");
        dream.shutdown().await.unwrap();
        drop(dream);
        source.shutdown().await.unwrap();
    }
}
