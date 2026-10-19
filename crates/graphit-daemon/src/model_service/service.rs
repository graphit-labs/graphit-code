use super::config::{DEFAULT_IDLE_TIMEOUT, ResolvedExecution, ResolvedModel};
use super::{
    ConfigId, EmbedInput, EmbedOutput, ModelErrorKind, ModelResult, ModelServiceError,
    RerankOutput, ResolvedConfig,
};
use graphit_ai::model_artifact::GgufModelSource;
use graphit_ai::{
    context::InferenceContext,
    embedding::EmbeddingLimits,
    gguf_rerank::GgufRerankClient,
    model_manifest::{ModelBackend, ModelTask},
    native_embedding::{
        NativeEmbeddingClient, NativeEmbeddingInput,
        worker::{GgufRerankWorker, NativeEmbeddingWorker},
    },
    native_rerank::NativeRerankLimits,
};
use graphit_core::{file_lock::Cancellation, native_document_file::CapturedDocumentDirectory};
use std::{
    collections::BTreeMap,
    future::Future,
    io::Write,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex as AsyncMutex, OwnedMutexGuard, Semaphore, oneshot, watch},
    task::JoinHandle,
};

#[derive(Clone, Copy)]
pub struct ServiceLimits {
    pub max_active_workers: NonZeroUsize,
    pub embedding: EmbeddingLimits,
    pub rerank: NativeRerankLimits,
}
enum NativeClient {
    Gemma(Arc<NativeEmbeddingWorker>),
    Bge(Arc<GgufRerankWorker>),
}
impl NativeClient {
    async fn shutdown_retained(&self, context: InferenceContext<'_>) -> ModelResult<()> {
        match self {
            Self::Gemma(worker) => worker.shutdown_retained(context).await?,
            Self::Bge(worker) => worker.shutdown_retained(context).await?,
        }
        Ok(())
    }
}
enum GgufOperation {
    Embedding {
        input: EmbedInput,
        texts: Vec<String>,
    },
    Rerank {
        query: String,
        documents: Vec<String>,
    },
}
enum GgufOutput {
    Embedding(EmbedOutput),
    Rerank(RerankOutput),
}
fn gguf_source(
    config: &ResolvedModel,
    context: InferenceContext<'_>,
) -> ModelResult<GgufModelSource> {
    context.check()?;
    let model_role = match &config.manifest.backend {
        ModelBackend::EmbeddingGemma2 { model_role, .. }
        | ModelBackend::BgeRerankerV2M3 { model_role, .. } => model_role,
    };
    let artifact = config
        .manifest
        .artifacts
        .iter()
        .find(|a| &a.role == model_role)
        .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Artifact))?;
    let metadata = std::fs::metadata(config.bundle.join(&artifact.path));
    context.check()?;
    let maximum_bytes = metadata
        .map_err(|cause| ModelServiceError::caused(ModelErrorKind::Artifact, cause))?
        .len();
    if maximum_bytes < artifact.minimum_bytes {
        return Err(ModelServiceError::new(ModelErrorKind::Artifact));
    }
    Ok(GgufModelSource {
        model: config.manifest.id.clone(),
        revision: config.manifest.revision.clone(),
        root: config.bundle.clone(),
        path: artifact.path.clone(),
        minimum_bytes: artifact.minimum_bytes,
        maximum_bytes,
        sha256: artifact.digest,
    })
}
struct Owner {
    config: Arc<ResolvedModel>,
    native: Arc<AsyncMutex<Option<NativeClient>>>,
    activity: watch::Sender<OwnerActivity>,
    idle_started: AtomicBool,
    #[cfg(test)]
    idle_unloaded: watch::Sender<u64>,
}
#[derive(Clone, Copy, Default)]
struct OwnerActivity {
    active: usize,
    last_completed: Option<Instant>,
}
// Counts actual owned operations under the existing admission permits. It is
// neither another quota nor an inference scheduler.
struct ActiveOwner(watch::Sender<OwnerActivity>);
impl ActiveOwner {
    fn new(owner: &Owner) -> Self {
        owner.activity.send_modify(|state| state.active += 1);
        Self(owner.activity.clone())
    }
}
impl Drop for ActiveOwner {
    fn drop(&mut self) {
        self.0.send_modify(|state| {
            state.active -= 1;
            state.last_completed = Some(Instant::now());
        });
    }
}
struct Worker {
    cancellation: Cancellation,
    done: Cancellation,
    join: Mutex<Option<JoinHandle<ModelResult<()>>>>,
}
struct State {
    generation: u64,
    slots: BTreeMap<String, Arc<Owner>>,
    workers: Vec<Arc<Worker>>,
    sealed: bool,
    draining: bool,
    drain_result: Option<Result<(), Arc<ModelServiceError>>>,
}
struct Inner {
    snapshot_parent: Arc<CapturedDocumentDirectory>,
    state: Mutex<State>,
    capacity: Arc<Semaphore>,
    limits: ServiceLimits,
    stopping: Cancellation,
    drained: Cancellation,
    idle_timeout: watch::Sender<Duration>,
    shutdown_timeout: Duration,
}

fn arm_idle(service: &Arc<Inner>, owner: &Arc<Owner>) {
    if owner.idle_started.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut state = service
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.sealed {
        return;
    }
    let worker = Arc::new(Worker {
        cancellation: Cancellation::default(),
        done: Cancellation::default(),
        join: Mutex::new(None),
    });
    let completed = worker.clone();
    let activity = owner.activity.subscribe();
    let owner = Arc::downgrade(owner);
    let timeout = service.idle_timeout.subscribe();
    let cleanup_timeout = service.shutdown_timeout;
    let stopping = worker.cancellation.clone();
    let join = tokio::spawn(async move {
        let result = idle_owner(owner, activity, timeout, cleanup_timeout, stopping).await;
        if result.is_ok() {
            completed.done.cancel();
        }
        result
    });
    *worker
        .join
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(join);
    state.workers.push(worker);
}

async fn idle_changed(
    activity: &mut watch::Receiver<OwnerActivity>,
    timeout: &mut watch::Receiver<Duration>,
    stopping: &Cancellation,
) -> bool {
    tokio::select! {
        biased;
        _ = stopping.cancelled() => false,
        changed = activity.changed() => changed.is_ok(),
        changed = timeout.changed() => changed.is_ok(),
    }
}

async fn idle_owner(
    owner: Weak<Owner>,
    mut activity: watch::Receiver<OwnerActivity>,
    mut timeout: watch::Receiver<Duration>,
    cleanup_timeout: Duration,
    stopping: Cancellation,
) -> ModelResult<()> {
    loop {
        let observed = *activity.borrow_and_update();
        let duration = *timeout.borrow_and_update();
        let Some(completed) = observed.last_completed.filter(|_| observed.active == 0) else {
            if !idle_changed(&mut activity, &mut timeout, &stopping).await {
                return Ok(());
            }
            continue;
        };
        let deadline = completed
            .checked_add(duration)
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?;
        tokio::select! {
            biased;
            _ = stopping.cancelled() => return Ok(()),
            changed = activity.changed() => {
                if changed.is_err() { return Ok(()); }
                continue;
            },
            changed = timeout.changed() => {
                if changed.is_err() { return Ok(()); }
                continue;
            },
            _ = tokio::time::sleep_until(deadline.into()) => {},
        }
        let Some(live) = owner.upgrade() else {
            return Ok(());
        };
        if !unload_if_idle(
            live,
            &activity,
            &timeout,
            completed,
            cleanup_timeout,
            &stopping,
        )
        .await?
        {
            continue;
        }
        // An unloaded owner waits for real activity/policy changes, not polling
        // an already elapsed deadline. Its portable metadata stays installed.
        if !idle_changed(&mut activity, &mut timeout, &stopping).await {
            return Ok(());
        }
    }
}
async fn unload_if_idle(
    owner: Arc<Owner>,
    activity: &watch::Receiver<OwnerActivity>,
    timeout: &watch::Receiver<Duration>,
    completed: Instant,
    cleanup_timeout: Duration,
    stopping: &Cancellation,
) -> ModelResult<bool> {
    let result = async {
        let mut native = tokio::select! {
            biased;
            _ = stopping.cancelled() => return Ok(false),
            native = owner.native.lock() => native,
        };
        let current = *activity.borrow();
        let current_timeout = *timeout.borrow();
        if current.active != 0
            || current.last_completed != Some(completed)
            || completed
                .checked_add(current_timeout)
                .is_none_or(|deadline| Instant::now() < deadline)
        {
            return Ok(false);
        }
        let cleanup_result = async {
            let cleanup = Cancellation::default();
            let deadline = Instant::now()
                .checked_add(cleanup_timeout)
                .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?;
            if let Some(client) = native.take() {
                let result = client
                    .shutdown_retained(InferenceContext::new(&cleanup, deadline))
                    .await;
                // Keep the mutex until real native join and final drop complete.
                let dropped = tokio::task::spawn_blocking(move || drop(client))
                    .await
                    .map_err(|cause| ModelServiceError::caused(ModelErrorKind::Worker, cause));
                #[cfg(test)]
                if dropped.is_ok() {
                    owner.idle_unloaded.send_modify(|epoch| *epoch += 1);
                }
                result.and(dropped)?;
            }
            Ok(true)
        }
        .await;
        if cleanup_result.is_err() {
            // The failed monitor retains its error in the registry. Rearm a
            // later load before releasing the native mutex; an old monitor's
            // finalizer must never reset a newly started monitor's flag.
            owner.idle_started.store(false, Ordering::Release);
        }
        cleanup_result
    }
    .await;
    // A retired Owner can lose its final strong reference here. Its native
    // destructor must not run on the async executor on any result path.
    tokio::task::spawn_blocking(move || drop(owner))
        .await
        .map_err(|cause| ModelServiceError::caused(ModelErrorKind::Worker, cause))?;
    result
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.stopping.cancel();
        for worker in &self
            .state
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .workers
        {
            worker.cancellation.cancel();
        }
        // A detached worker task still owns and awaits its native JoinHandle.
        // Dropping a request or service cannot drop that join responsibility.
    }
}
#[derive(Clone)]
pub struct ModelService(Arc<Inner>);
// Request-local observation only: admission includes capacity and owner access.
// Execution means the owned service task was admitted, not a native kernel phase.
struct ExecutionDeadline<W: Write> {
    deadline: Instant,
    operation: &'static str,
    phase: &'static str,
    completed: bool,
    diagnostics: W,
}
impl<W: Write> Drop for ExecutionDeadline<W> {
    fn drop(&mut self) {
        if !self.completed && Instant::now() >= self.deadline {
            let _ = writeln!(
                self.diagnostics,
                "model service deadline: operation={} phase={}",
                self.operation, self.phase
            );
        }
    }
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
impl ModelService {
    pub fn new(
        snapshot_parent: Arc<CapturedDocumentDirectory>,
        limits: ServiceLimits,
    ) -> ModelResult<Self> {
        if limits.max_active_workers.get() > Semaphore::MAX_PERMITS {
            return Err(ModelServiceError::new(ModelErrorKind::Limit));
        }
        Ok(Self(Arc::new(Inner {
            snapshot_parent,
            state: Mutex::new(State {
                generation: 0,
                slots: BTreeMap::new(),
                workers: Vec::new(),
                sealed: false,
                draining: false,
                drain_result: None,
            }),
            capacity: Arc::new(Semaphore::new(limits.max_active_workers.get())),
            limits,
            stopping: Cancellation::default(),
            drained: Cancellation::default(),
            idle_timeout: watch::channel(DEFAULT_IDLE_TIMEOUT).0,
            shutdown_timeout: Duration::from_secs(30),
        })))
    }
    pub(crate) fn with_shutdown_timeout(mut self, timeout: Duration) -> ModelResult<Self> {
        if timeout.is_zero() || Instant::now().checked_add(timeout).is_none() {
            return Err(ModelServiceError::new(ModelErrorKind::Configuration));
        }
        Arc::get_mut(&mut self.0)
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Configuration))?
            .shutdown_timeout = timeout;
        Ok(self)
    }
    /// An equivalent map is a no-op. Existing owners also survive alias changes.
    pub fn install(
        &self,
        config: ResolvedConfig,
        context: InferenceContext<'_>,
    ) -> ModelResult<u64> {
        context.check()?;
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.sealed {
            return Err(ModelServiceError::new(ModelErrorKind::Shutdown));
        }
        // Refresh-only installations must reap successful native cleanup too.
        // Failed or panicked tasks never publish done and remain owned by drain.
        state.workers.retain(|worker| !worker.done.is_cancelled());
        if state.slots.len() == config.slots.len()
            && config.slots.iter().all(|(name, config)| {
                state
                    .slots
                    .get(name)
                    .is_some_and(|owner| owner.config.id == config.id)
            })
        {
            context.check()?;
            self.0.idle_timeout.send_if_modified(|timeout| {
                if *timeout == config.idle_timeout {
                    false
                } else {
                    *timeout = config.idle_timeout;
                    true
                }
            });
            return Ok(state.generation);
        }
        let generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Limit))?;
        let mut owners: BTreeMap<ConfigId, Arc<Owner>> = state
            .slots
            .values()
            .map(|owner| (owner.config.id, owner.clone()))
            .collect();
        let mut slots = BTreeMap::new();
        for (name, config) in config.slots {
            let owner = owners
                .entry(config.id)
                .or_insert_with(|| {
                    Arc::new(Owner {
                        config,
                        native: Arc::new(AsyncMutex::new(None)),
                        activity: watch::channel(OwnerActivity::default()).0,
                        idle_started: AtomicBool::new(false),
                        #[cfg(test)]
                        idle_unloaded: watch::channel(0).0,
                    })
                })
                .clone();
            slots.insert(name, owner);
        }
        context.check()?;
        self.0.idle_timeout.send_if_modified(|timeout| {
            if *timeout == config.idle_timeout {
                false
            } else {
                *timeout = config.idle_timeout;
                true
            }
        });
        state.generation = generation;
        let retired = std::mem::replace(&mut state.slots, slots);
        // Native proxy Drop performs a real thread join; keep it off executor threads.
        if !retired.is_empty() {
            let worker = Arc::new(Worker {
                cancellation: Cancellation::default(),
                done: Cancellation::default(),
                join: Mutex::new(None),
            });
            let completed = worker.clone();
            let join = tokio::spawn(async move {
                tokio::task::spawn_blocking(move || drop((retired, owners)))
                    .await
                    .map_err(|e| ModelServiceError::caused(ModelErrorKind::Worker, e))?;
                completed.done.cancel();
                Ok(())
            });
            *worker.join.lock().unwrap_or_else(|e| e.into_inner()) = Some(join);
            state.workers.push(worker);
        }
        Ok(generation)
    }
    pub(crate) fn preflight_rerank(
        &self,
        query: &str,
        documents: &[String],
        context: InferenceContext<'_>,
    ) -> ModelResult<()> {
        context.check()?;
        let mut refs = Vec::new();
        refs.try_reserve_exact(documents.len())
            .map_err(|_| ModelServiceError::new(ModelErrorKind::Limit))?;
        refs.extend(documents.iter().map(String::as_str));
        self.0
            .limits
            .rerank
            .validate_input(query, &refs, context)
            .map_err(graphit_ai::gguf_rerank::GgufRerankError::from)?;
        context.check()?;
        Ok(())
    }
    pub(crate) fn slot_names(&self, context: InferenceContext<'_>) -> ModelResult<Vec<String>> {
        context.check()?;
        let state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.sealed {
            return Err(ModelServiceError::new(ModelErrorKind::Shutdown));
        }
        let names = state.slots.keys().cloned().collect();
        context.check()?;
        Ok(names)
    }

    /// Only an absent slot is a lazy-admission candidate. Stale identities stay terminal.
    pub(crate) fn has_model(
        &self,
        slot: &str,
        expected: ConfigId,
        task: ModelTask,
        context: InferenceContext<'_>,
    ) -> ModelResult<bool> {
        context.check()?;
        let state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.sealed {
            return Err(ModelServiceError::new(ModelErrorKind::Shutdown));
        }
        let Some(owner) = state.slots.get(slot) else {
            return Ok(false);
        };
        if owner.config.id != expected {
            return Err(ModelServiceError::new(ModelErrorKind::ConfigChanged));
        }
        if owner.config.manifest.task != task {
            return Err(ModelServiceError::new(ModelErrorKind::Selection));
        }
        context.check()?;
        Ok(true)
    }
    fn admit(
        &self,
        slot: &str,
        expected: ConfigId,
        task: ModelTask,
        context: InferenceContext<'_>,
    ) -> ModelResult<(Arc<Owner>, u64)> {
        context.check()?;
        let state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.sealed {
            return Err(ModelServiceError::new(ModelErrorKind::Shutdown));
        }
        let owner = state
            .slots
            .get(slot)
            .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Selection))?;
        if owner.config.id != expected {
            return Err(ModelServiceError::new(ModelErrorKind::ConfigChanged));
        }
        if owner.config.manifest.task != task {
            return Err(ModelServiceError::new(ModelErrorKind::Selection));
        }
        // Identity comparison and this Arc capture occur under the same map lock.
        Ok((owner.clone(), state.generation))
    }
    async fn execute_owned<T, F>(
        &self,
        owner: Arc<Owner>,
        context: InferenceContext<'_>,
        diagnostics: impl Write + Send,
        run: impl FnOnce(
            Arc<Owner>,
            OwnedMutexGuard<Option<NativeClient>>,
            ServiceLimits,
            Cancellation,
            Instant,
        ) -> F
        + Send
        + 'static,
    ) -> ModelResult<T>
    where
        T: Send + 'static,
        F: Future<Output = ModelResult<T>> + Send + 'static,
    {
        let mut observation = ExecutionDeadline {
            deadline: context.deadline(),
            operation: match owner.config.manifest.task {
                ModelTask::Embedding => "embedding",
                ModelTask::Rerank => "rerank",
            },
            phase: "admission",
            completed: false,
            diagnostics,
        };
        context.check()?;
        let deadline = context.deadline();
        let permit = tokio::select! {
            biased;
            _ = context.cancellation().cancelled() => return Err(ModelServiceError::new(ModelErrorKind::Cancelled)),
            _ = self.0.stopping.cancelled() => return Err(ModelServiceError::new(ModelErrorKind::Shutdown)),
            _ = tokio::time::sleep_until(deadline.into()) => return Err(ModelServiceError::new(ModelErrorKind::Deadline)),
            p = self.0.capacity.clone().acquire_owned() => p.map_err(|e| ModelServiceError::caused(ModelErrorKind::Shutdown,e))?,
        };
        let native = tokio::select! {
            biased;
            _ = context.cancellation().cancelled() => return Err(ModelServiceError::new(ModelErrorKind::Cancelled)),
            _ = self.0.stopping.cancelled() => return Err(ModelServiceError::new(ModelErrorKind::Shutdown)),
            _ = tokio::time::sleep_until(deadline.into()) => return Err(ModelServiceError::new(ModelErrorKind::Deadline)),
            guard = owner.native.clone().lock_owned() => guard,
        };
        context.check()?;
        let worker = Arc::new(Worker {
            cancellation: Cancellation::default(),
            done: Cancellation::default(),
            join: Mutex::new(None),
        });
        let request_guard = CancelOnDrop(worker.cancellation.clone());
        let (send, receive) = oneshot::channel();
        let limits = self.0.limits;
        let worker_task = worker.clone();
        {
            let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.sealed {
                return Err(ModelServiceError::new(ModelErrorKind::Shutdown));
            }
            // Completed entries have already observed the native join. Awaiting
            // the task itself remains the drain's responsibility for live entries.
            state.workers.retain(|w| !w.done.is_cancelled());
            let cancellation = worker.cancellation.clone();
            observation.phase = "execution";
            let join = tokio::spawn(async move {
                let _permit = permit;
                let result = run(owner, native, limits, cancellation, deadline).await;
                let _ = send.send(result);
                worker_task.done.cancel();
                Ok(())
            });
            *worker.join.lock().unwrap_or_else(|e| e.into_inner()) = Some(join);
            state.workers.push(worker);
        }
        let result = tokio::select! {
            biased;
            _ = context.cancellation().cancelled() => Err(ModelServiceError::new(ModelErrorKind::Cancelled)),
            _ = self.0.stopping.cancelled() => Err(ModelServiceError::new(ModelErrorKind::Shutdown)),
            _ = tokio::time::sleep_until(deadline.into()) => Err(ModelServiceError::new(ModelErrorKind::Deadline)),
            result = receive => result.map_err(|e| ModelServiceError::caused(ModelErrorKind::Worker,e))?,
        };
        drop(request_guard);
        context.check()?;
        observation.completed = result.is_ok();
        result
    }
    pub async fn embed(
        &self,
        slot: &str,
        expected: ConfigId,
        input: EmbedInput,
        texts: Vec<String>,
        context: InferenceContext<'_>,
    ) -> ModelResult<EmbedOutput> {
        let (owner, generation) = self.admit(slot, expected, ModelTask::Embedding, context)?;
        let config = owner.config.clone();
        let refs = texts.iter().map(String::as_str).collect::<Vec<_>>();
        let ResolvedExecution::EmbeddingGemma2 { options, .. } = &config.execution else {
            return Err(ModelServiceError::new(ModelErrorKind::Selection));
        };
        NativeEmbeddingClient::validate_request(
            *options,
            self.0.limits.embedding,
            &refs,
            input.native(),
            context,
        )?;
        self.embed_gemma(owner, generation, input, texts, context)
            .await
    }
    async fn embed_gemma(
        &self,
        owner: Arc<Owner>,
        generation: u64,
        input: EmbedInput,
        texts: Vec<String>,
        context: InferenceContext<'_>,
    ) -> ModelResult<EmbedOutput> {
        match self
            .execute_gguf(
                owner,
                generation,
                GgufOperation::Embedding { input, texts },
                context,
            )
            .await?
        {
            GgufOutput::Embedding(output) => Ok(output),
            GgufOutput::Rerank(_) => Err(ModelServiceError::new(ModelErrorKind::Selection)),
        }
    }
    async fn execute_gguf(
        &self,
        owner: Arc<Owner>,
        generation: u64,
        operation: GgufOperation,
        context: InferenceContext<'_>,
    ) -> ModelResult<GgufOutput> {
        let service = Arc::downgrade(&self.0);
        let snapshot_parent = self.0.snapshot_parent.clone();
        self.execute_owned(
            owner,
            context,
            std::io::stderr(),
            move |owner, mut native, limits, cancellation, deadline| async move {
                let activity = ActiveOwner::new(&owner);
                let context = InferenceContext::new(&cancellation, deadline);
                let result = async {
                    context.check()?;
                    if native.is_none() {
                        let source = gguf_source(&owner.config, context)?;
                        *native = Some(match &owner.config.execution {
                            ResolvedExecution::EmbeddingGemma2 {
                                options,
                                queue_capacity,
                            } => NativeClient::Gemma(Arc::new(
                                NativeEmbeddingWorker::start(
                                    snapshot_parent.clone(),
                                    source,
                                    *options,
                                    limits.embedding,
                                    *queue_capacity,
                                    context,
                                )
                                .await?,
                            )),
                            ResolvedExecution::BgeRerankerV2M3 {
                                options,
                                queue_capacity,
                            } => NativeClient::Bge(Arc::new(
                                GgufRerankWorker::start(
                                    snapshot_parent,
                                    source,
                                    *options,
                                    limits.rerank,
                                    *queue_capacity,
                                    context,
                                )
                                .await?,
                            )),
                        });
                        if let Some(service) = service.upgrade() {
                            arm_idle(&service, &owner);
                        }
                    }
                    match operation {
                        GgufOperation::Embedding { input, texts } => {
                            let Some(NativeClient::Gemma(worker)) = native.as_ref() else {
                                return Err(ModelServiceError::new(ModelErrorKind::Selection));
                            };
                            let worker = worker.clone();
                            let device = worker
                                .registered_device()
                                .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Worker))?;
                            let dimensions = worker.dimensions() as u64;
                            drop(native);
                            let output = worker
                                .embed_retained(texts, input.native(), context)
                                .await
                                .map(|batch| {
                                    GgufOutput::Embedding(EmbedOutput {
                                        device,
                                        config_id: owner.config.id,
                                        generation,
                                        model: owner.config.manifest.id.clone(),
                                        revision: owner.config.manifest.revision.clone(),
                                        dimensions,
                                        vectors: batch.into_vectors(),
                                    })
                                })
                                .map_err(ModelServiceError::from);
                            tokio::task::spawn_blocking(move || drop(worker))
                                .await
                                .map_err(|e| {
                                    ModelServiceError::caused(ModelErrorKind::Worker, e)
                                })?;
                            output
                        }
                        GgufOperation::Rerank { query, documents } => {
                            let Some(NativeClient::Bge(worker)) = native.as_ref() else {
                                return Err(ModelServiceError::new(ModelErrorKind::Selection));
                            };
                            let worker = worker.clone();
                            let device = worker
                                .registered_device()
                                .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Worker))?;
                            drop(native);
                            let output = worker
                                .score_retained(query, documents, context)
                                .await
                                .map(|scores| {
                                    GgufOutput::Rerank(RerankOutput {
                                        device,
                                        config_id: owner.config.id,
                                        generation,
                                        model: owner.config.manifest.id.clone(),
                                        revision: owner.config.manifest.revision.clone(),
                                        scores,
                                    })
                                })
                                .map_err(ModelServiceError::from);
                            tokio::task::spawn_blocking(move || drop(worker))
                                .await
                                .map_err(|e| {
                                    ModelServiceError::caused(ModelErrorKind::Worker, e)
                                })?;
                            output
                        }
                    }
                }
                .await;
                drop(activity);
                // Final proxy/owner drops can retain a real native join after response cancellation.
                tokio::task::spawn_blocking(move || drop(owner))
                    .await
                    .map_err(|e| ModelServiceError::caused(ModelErrorKind::Worker, e))?;
                result
            },
        )
        .await
    }
    pub async fn rerank(
        &self,
        slot: &str,
        expected: ConfigId,
        query: String,
        documents: Vec<String>,
        context: InferenceContext<'_>,
    ) -> ModelResult<RerankOutput> {
        let (owner, generation) = self.admit(slot, expected, ModelTask::Rerank, context)?;
        let config = owner.config.clone();
        let ResolvedExecution::BgeRerankerV2M3 { options, .. } = &config.execution else {
            return Err(ModelServiceError::new(ModelErrorKind::Selection));
        };
        GgufRerankClient::validate_request(
            *options,
            self.0.limits.rerank,
            &query,
            &documents.iter().map(String::as_str).collect::<Vec<_>>(),
            context,
        )?;
        match self
            .execute_gguf(
                owner,
                generation,
                GgufOperation::Rerank { query, documents },
                context,
            )
            .await?
        {
            GgufOutput::Rerank(output) => Ok(output),
            GgufOutput::Embedding(_) => Err(ModelServiceError::new(ModelErrorKind::Selection)),
        }
    }
    /// Drain is installation-owned, independent of the shutdown request's future.
    /// A caller deadline may stop waiting; it cannot abandon a native join.
    pub async fn shutdown(&self, context: InferenceContext<'_>) -> ModelResult<()> {
        context.check()?;
        self.start_shutdown();
        tokio::select! {
            biased;
            _ = context.cancellation().cancelled() => return Err(ModelServiceError::new(ModelErrorKind::Cancelled)),
            _ = tokio::time::sleep_until(context.deadline().into()) => return Err(ModelServiceError::new(ModelErrorKind::Deadline)),
            _ = self.0.drained.cancelled() => {},
        }
        context.check()?;
        self.shutdown_result()
    }
    /// Process teardown must retain its SDK root until the actual native drain joins.
    pub(crate) async fn join_shutdown(&self) -> ModelResult<()> {
        self.start_shutdown();
        self.0.drained.cancelled().await;
        self.shutdown_result()
    }
    fn start_shutdown(&self) {
        {
            let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            state.sealed = true;
            self.0.stopping.cancel();
            if !state.draining {
                state.draining = true;
                let owners = std::mem::take(&mut state.slots);
                let workers = std::mem::take(&mut state.workers);
                let inner = self.0.clone();
                for worker in &workers {
                    worker.cancellation.cancel();
                }
                tokio::spawn(async move {
                    let mut failure = None;
                    for worker in workers {
                        let join = worker.join.lock().unwrap_or_else(|e| e.into_inner()).take();
                        if let Some(join) = join {
                            match join.await {
                                Ok(Ok(())) => {}
                                Ok(Err(e)) => failure = Some(Arc::new(e)),
                                Err(e) => {
                                    failure = Some(Arc::new(ModelServiceError::caused(
                                        ModelErrorKind::Worker,
                                        e,
                                    )))
                                }
                            }
                        }
                    }
                    if let Err(e) = tokio::task::spawn_blocking(move || drop(owners)).await {
                        failure = Some(Arc::new(ModelServiceError::caused(
                            ModelErrorKind::Worker,
                            e,
                        )));
                    }
                    inner
                        .state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .drain_result = Some(failure.map_or(Ok(()), Err));
                    inner.drained.cancel();
                });
            }
        }
    }
    fn shutdown_result(&self) -> ModelResult<()> {
        match self
            .0
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain_result
            .as_ref()
        {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(ModelServiceError::caused(ModelErrorKind::Worker, e.clone())),
            None => Err(ModelServiceError::new(ModelErrorKind::Worker)),
        }
    }
}

impl EmbedInput {
    fn native(self) -> NativeEmbeddingInput {
        match self {
            Self::Query => NativeEmbeddingInput::Query,
            Self::Document => NativeEmbeddingInput::Document,
            Self::CodeQuery => NativeEmbeddingInput::CodeQuery,
            Self::CodeDocument => NativeEmbeddingInput::CodeDocument,
        }
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
