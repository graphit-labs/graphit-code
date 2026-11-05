//! One installation-owned USER lane. Startup is distinct from human activity.
use super::*;
use graphit_ai::{cli_codex::CodexSink, cli_codex_wire::CodexEventView};
use graphit_auth::{
    AdminParent, ExternalEffect, ServiceUseError, UserReadAdmission, UserWriteAdmission,
    profile::ProfileError,
};
use graphit_core::{
    artifact_store::{StoreOwner, UserIdentity},
    file_io::SnapshotContext,
    module_settings::ModuleSettings,
};
use graphit_daemon::{broker_verifiers::BrokerVerifierCache, dream::UserApplication};
use graphit_process::SinkFailure;
use graphit_records::user_memory_store::{NativeUserMemoryStore, UserMemoryListRead};
use serde::ser::SerializeSeq;

struct UserPlan {
    auth: Arc<DaemonAuth>,
    verifiers: Arc<BrokerVerifierCache>,
    policy: RoleToolPolicy,
    settings: Settings,
    #[cfg(test)]
    iteration_completed: Arc<tokio::sync::Notify>,
}
#[derive(Default)]
struct UserOwner {
    application: Option<UserApplication>,
}
struct UserExit {
    owner: UserOwner,
    error: Option<io::Error>,
}
pub(crate) struct ForegroundUserDream {
    plan: Arc<UserPlan>,
    activity: watch::Sender<Option<Instant>>,
    receiver: Option<watch::Receiver<Option<Instant>>>,
    stop: Cancellation,
    host_stop: Cancellation,
    host: Option<HostOwner>,
    join: Option<JoinHandle<UserExit>>,
    exit: Option<UserExit>,
}
impl ForegroundUserDream {
    pub fn new(
        auth: Arc<DaemonAuth>,
        verifiers: Arc<BrokerVerifierCache>,
        source: SourcePolicy,
        config: ConfigSnapshot,
    ) -> io::Result<Option<Self>> {
        if !enabled(&config)? || !ModuleSettings::resolve(&config)?.enabled(Module::Memory) {
            return Ok(None);
        }
        if source.binding != NativeBinding::Codex {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "USER Dream requires Codex binding",
            ));
        }
        let settings = Settings::capture(&config)?;
        let policy = RoleToolPolicy::capture(source, config)?;
        // None denotes startup, not an invented human observation.
        let (activity, receiver) = watch::channel(None);
        Ok(Some(Self {
            plan: Arc::new(UserPlan {
                auth,
                verifiers,
                policy,
                settings,
                #[cfg(test)]
                iteration_completed: Arc::new(tokio::sync::Notify::new()),
            }),
            activity,
            receiver: Some(receiver),
            stop: Cancellation::default(),
            host_stop: Cancellation::default(),
            host: None,
            join: None,
            exit: None,
        }))
    }
    pub fn activity_sender(&self) -> watch::Sender<Option<Instant>> {
        self.activity.clone()
    }
    pub async fn start(
        &mut self,
        environment: Vec<(OsString, OsString)>,
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        if self.host.is_some() || self.stop.is_cancelled() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        host_cleanup_deadline(self.plan.policy.source.timeout)?;
        let host = Arc::new(ProcessHost::install(
            &self.host_stop,
            HostLimits::new(1, 1)?,
        )?);
        self.host = Some(HostOwner::Live(Arc::clone(&host)));
        match host.ready(original.snapshot().required_deadline()?).await {
            HostReadiness::Ready => (),
            HostReadiness::Pending => return Err(io::ErrorKind::TimedOut.into()),
            HostReadiness::Failed(_) => {
                return Err(io::Error::other("USER Dream host readiness failed"));
            }
            HostReadiness::Sealed => return Err(io::ErrorKind::BrokenPipe.into()),
        }
        let receiver = self.receiver.take().ok_or(io::ErrorKind::AlreadyExists)?;
        let plan = Arc::clone(&self.plan);
        let stop = self.stop.clone();
        self.join = Some(tokio::spawn(async move {
            run_user(plan, host, receiver, stop, environment).await
        }));
        Ok(())
    }
    pub fn cancel(&self) {
        self.stop.cancel();
    }
    async fn observe(&mut self) {
        if let Some(join) = self.join.as_mut() {
            let exit = match join.await {
                Ok(exit) => exit,
                Err(error) => UserExit {
                    owner: UserOwner::default(),
                    error: Some(io::Error::other(DreamCause::Join(Arc::new(error)))),
                },
            };
            self.join = None;
            self.exit = Some(exit);
        }
    }
    pub async fn next(&mut self) -> io::Result<()> {
        self.observe().await;
        match self.exit.as_mut().and_then(|exit| exit.error.take()) {
            Some(error) => Err(error),
            None if self.stop.is_cancelled() => Ok(()),
            None => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "USER Dream job ended",
            )),
        }
    }
    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.cancel();
        // observe borrows the JoinHandle: cancellation cannot detach its native frame.
        self.observe().await;
        shutdown_host(&mut self.host, self.plan.policy.source.timeout).await?;
        if let Some(exit) = &mut self.exit {
            // No native borrower remains; inert recipe/receipts are released only now.
            exit.owner.application = None;
            if let Some(error) = exit.error.take() {
                return Err(error);
            }
        }
        Ok(())
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn retained_application(&self) -> Option<&UserApplication> {
        self.exit.as_ref()?.owner.application.as_ref()
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn iteration_completion(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.plan.iteration_completed)
    }
}
impl Drop for ForegroundUserDream {
    fn drop(&mut self) {
        self.cancel();
        self.host_stop.cancel();
    }
}
enum Trigger {
    Startup,
    Human(Instant),
}
async fn run_user(
    plan: Arc<UserPlan>,
    host: Arc<ProcessHost>,
    mut activity: watch::Receiver<Option<Instant>>,
    stop: Cancellation,
    environment: Vec<(OsString, OsString)>,
) -> UserExit {
    let mut owner = UserOwner::default();
    let mut trigger = Trigger::Startup;
    'activity: loop {
        if let Trigger::Human(observed) = trigger {
            let Some(due) = observed.checked_add(plan.settings.idle) else {
                return UserExit {
                    owner,
                    error: Some(io::ErrorKind::InvalidInput.into()),
                };
            };
            tokio::select! {
                biased;
                () = stop.cancelled() => return UserExit { owner, error: None },
                changed = activity.changed() => {
                    if changed.is_err() { return UserExit { owner, error: Some(io::ErrorKind::BrokenPipe.into()) }; }
                    if let Some(observed) = *activity.borrow_and_update() {
                        trigger = Trigger::Human(observed);
                    }
                    continue;
                },
                () = tokio::time::sleep_until(due.into()) => (),
            }
        }
        for attempt in 0..=plan.settings.retries {
            match perform_user(&plan, &host, &stop, &environment, &mut owner).await {
                Ok(()) => break,
                Err(error) if stop.is_cancelled() => {
                    return UserExit {
                        owner,
                        error: Some(error),
                    };
                }
                Err(error) if attempt == plan.settings.retries => {
                    if owner.application.is_none()
                        && activity.has_changed().unwrap_or(false)
                        && let Some(observed) = *activity.borrow_and_update()
                    {
                        trigger = Trigger::Human(observed);
                        continue 'activity;
                    }
                    return UserExit {
                        owner,
                        error: Some(error),
                    };
                }
                Err(_) => {
                    tokio::select! {
                        biased;
                        () = stop.cancelled() => return UserExit { owner, error: Some(io::ErrorKind::Interrupted.into()) },
                        () = tokio::time::sleep(plan.settings.backoff) => (),
                    }
                }
            }
        }
        owner = UserOwner::default();
        #[cfg(test)]
        plan.iteration_completed.notify_one();
        // Success/noop consumes this trigger. No periodic consolidation is introduced.
        if !activity.has_changed().unwrap_or(false) {
            tokio::select! {
                biased;
                () = stop.cancelled() => return UserExit { owner, error: None },
                changed = activity.changed() => {
                    if changed.is_err() { return UserExit { owner, error: Some(io::ErrorKind::BrokenPipe.into()) }; }
                },
            }
        }
        if let Some(observed) = *activity.borrow_and_update() {
            trigger = Trigger::Human(observed);
        }
    }
}

struct NativeGuard<'a, 'r, 'w, 'p> {
    original: &'a SnapshotContext,
    parent: &'a AdminParent<'p>,
    read: &'a UserReadAdmission<'r>,
    write: &'a UserWriteAdmission<'w>,
    failure: Option<AuthError>,
}
impl NativeGuard<'_, '_, '_, '_> {
    fn check(&self) -> Result<(), AuthError> {
        self.read.recheck()?;
        self.write.recheck()?;
        self.parent
            .authorize(Permission::External(ExternalEffect::NativeLaunch))?;
        Ok(())
    }
}
impl CodexSink for NativeGuard<'_, '_, '_, '_> {
    async fn push(
        &mut self,
        _event: CodexEventView<'_>,
        original: &SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if !std::ptr::eq(self.original, original) {
            return Err(SinkFailure::Closed);
        }
        if let Err(error) = self.check() {
            if self.failure.is_none() {
                self.failure = Some(error);
            }
            return Err(SinkFailure::Closed);
        }
        Ok(())
    }
}
struct UserCorpus<'a>(&'a [Memory]);
impl Serialize for UserCorpus<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for memory in self.0 {
            sequence.serialize_element(&CorpusMemory::from(memory))?;
        }
        sequence.end()
    }
}
struct CorpusOutput<'a> {
    bytes: Zeroizing<Vec<u8>>,
    original: &'a SnapshotContext,
    maximum: usize,
}
impl io::Write for CorpusOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.original.check()?;
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if length > self.maximum {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.original.check()
    }
}

async fn perform_user(
    plan: &UserPlan,
    host: &ProcessHost,
    stop: &Cancellation,
    environment: &[(OsString, OsString)],
    owner: &mut UserOwner,
) -> io::Result<()> {
    let original = RequestContext::new(
        plan.policy.source.content,
        stop,
        Instant::now()
            .checked_add(plan.settings.duration)
            .ok_or(io::ErrorKind::InvalidInput)?,
    )
    .map_err(io::Error::other)?;
    let caller = runtime_caller(&plan.auth, &original)?;
    let (binding, revision) = match caller.native_provider() {
        Ok(binding) => binding,
        Err(ServiceUseError::Profile(ProfileError::NoActive) | ServiceUseError::WrongMode)
            if owner.application.is_none() =>
        {
            return Ok(());
        }
        // A retained application can never disappear into an absence/noop result.
        Err(_) => return Err(io::Error::other("USER Dream Native profile unavailable")),
    };
    let verifier = plan
        .verifiers
        .get_native(&binding, revision)
        .map_err(|_| io::Error::other("USER Dream Native verifier unavailable"))?;
    plan.auth
        .issuer()
        .prepare_native_record_access(&caller)
        .await?;
    let caller = caller
        .authenticate_native(&verifier)
        .await
        .map_err(|_| io::Error::other("USER Dream Native authentication failed"))?;
    let subject = plan
        .auth
        .issuer()
        .user_from_authenticated(&caller)
        .map_err(io::Error::other)?;
    let StoreOwner::Installation { installation_id } = plan.auth.installation().live.owner() else {
        return Err(io::ErrorKind::InvalidData.into());
    };
    let identity = UserIdentity::new(
        subject.issuer().map_err(io::Error::other)?.to_owned(),
        subject.subject().map_err(io::Error::other)?.to_owned(),
    )?;
    let captured = CapturedPublicationScope::user(
        &plan.policy.source.installation,
        installation_id.clone(),
        identity,
        plan.policy.source.scope,
        original.snapshot(),
    )?;
    let record = captured.binding()?;
    let read = plan
        .auth
        .issuer()
        .admit_authenticated_user_memory_read(
            &caller,
            &original,
            ScopeCapture {
                live: &captured,
                record: &record,
            },
        )
        .map_err(io::Error::other)?;
    let memories = if owner.application.is_none() {
        match NativeUserMemoryStore::list(
            original.snapshot(),
            &read,
            MemoryListFilter::default(),
            plan.policy.memory_read,
        )
        .await
        .map_err(|error| io::Error::other(DreamCause::Memory(error)))?
        {
            UserMemoryListRead::NoCorpus => return Ok(()),
            UserMemoryListRead::Published { memories, .. } if memories.is_empty() => return Ok(()),
            UserMemoryListRead::Published { memories, .. } => Some(memories),
        }
    } else {
        None
    };
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
    let write = parent
        .admit_native_user_memory_fenced_write(
            &original,
            ScopeCapture {
                live: &captured,
                record: &record,
            },
        )
        .map_err(io::Error::other)?;
    if let Some(memories) = memories {
        let program = resolve_program(
            &plan.settings.program,
            environment,
            plan.policy.source.installation.root(),
        )?;
        let mut corpus = CorpusOutput {
            bytes: Zeroizing::new(Vec::new()),
            original: original.snapshot(),
            maximum: plan.policy.instructions.max_variable_bytes.get(),
        };
        serde_json::to_writer(&mut corpus, &UserCorpus(&memories))
            .map_err(|_| io::Error::other("USER Dream corpus encoding or bound failed"))?;
        let corpus_text =
            std::str::from_utf8(&corpus.bytes).map_err(|_| io::ErrorKind::InvalidData)?;
        let attempt_id = operation_id()?;
        let set = policy_prompt(PolicyPrompt::UserDreamConsolidation);
        let sources = crate::instruction_overrides::InstructionSources::capture(
            plan.auth.issuer(),
            &plan.policy.source,
            None,
            &original,
        )?;
        let prompt = sources
            .render(
                set.entry(),
                set.candidates(),
                &[
                    InstructionVariable {
                        name: "dream_session_id",
                        value: InstructionValue::Text(&attempt_id),
                    },
                    InstructionVariable {
                        name: "memories_json",
                        value: InstructionValue::Text(corpus_text),
                    },
                ],
                plan.policy.instructions,
                &original,
            )
            .map_err(io::Error::other)?;
        let mut guard = NativeGuard {
            original: original.snapshot(),
            parent: &parent,
            read: &read,
            write: &write,
            failure: None,
        };
        guard.check().map_err(io::Error::other)?;
        let cwd = tempfile::Builder::new()
            .prefix("graphit-user-dream-")
            .tempdir()?;
        let content = plan.policy.source.content;
        let request = Request {
            program: program.to_owned(),
            args: native_args(&plan.settings.model, None),
            cwd: cwd.path().to_owned(),
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
        let mut proposal = dream::propose_guarded(
            original.snapshot(),
            host,
            request,
            &plan.settings.model,
            &CodexWireLimits {
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
            &mut guard,
        )
        .await
        .map_err(io::Error::other)?;
        let (succeeded, mut remaining) = match proposal.process.as_mut() {
            Some(Ok(process)) => (
                process.status.success()
                    && matches!(process.cleanup, graphit_process::Cleanup::Complete)
                    && process.pipes.stdout == graphit_process::PipeEnd::Eof
                    && process.pipes.stderr == graphit_process::PipeEnd::Eof,
                process.remaining.take(),
            ),
            Some(Err(process)) => (false, process.remaining.take()),
            None => (false, None),
        };
        let succeeded = succeeded && remaining.is_none();
        // Stay inside THIS frame, retaining caller/verifier/Parent/admissions/cwd.
        // A cleanup deadline bounds observation only; it never reauthorizes effects.
        if let Some(remaining) = remaining.as_mut() {
            while let RepairObservation::Pending { .. } = remaining
                .repair(Instant::now() + plan.policy.source.timeout)
                .await
            {}
        }
        if let Some(error) = guard.failure.take() {
            proposal.answer = None;
            return Err(io::Error::other(error));
        }
        if !succeeded || proposal.answer.is_none() {
            proposal.answer = None;
            return Err(io::Error::other(DreamCause::Native(Box::new(proposal))));
        }
        guard.check().map_err(io::Error::other)?;
        let answer = Zeroizing::new(proposal.answer.take().ok_or(io::ErrorKind::InvalidData)?);
        original.check()?;
        let recipe = dream::recipe::prepare(
            &answer,
            &memories,
            &attempt_id,
            plan.policy.memory_write.max_input_bytes,
            original.snapshot(),
        )
        .map_err(|error| io::Error::other(DreamCause::Recipe(error)))?;
        owner.application = Some(
            UserApplication::new(recipe, &read)
                .map_err(|error| io::Error::other(DreamCause::Memory(error)))?,
        );
    }
    original.check()?;
    read.recheck().map_err(io::Error::other)?;
    write.recheck().map_err(io::Error::other)?;
    owner
        .application
        .as_mut()
        .ok_or(io::ErrorKind::InvalidInput)?
        .apply(original.snapshot(), &read, &write, plan.policy.memory_write)
        .await
        .map_err(|error| io::Error::other(DreamCause::Memory(error)))
}
