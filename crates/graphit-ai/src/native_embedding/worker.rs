//! Bounded owned requests to a thread-local native client, with retained real joins.
use super::*;
use crate::gguf_rerank::{
    GgufRerankClient, GgufRerankError, GgufRerankErrorKind, GgufRerankOptions,
};
use crate::native_rerank::NativeRerankLimits;
use graphit_core::native_document_file::CapturedDocumentDirectory;
#[cfg(test)]
use std::path::PathBuf;
use std::{
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeWorkerErrorKind {
    Context(ContextError),
    Busy,
    Closed,
    Native(NativeEmbeddingErrorKind),
    Rerank(GgufRerankErrorKind),
    Spawn,
    Join,
}
pub struct NativeWorkerError {
    kind: NativeWorkerErrorKind,
    cause: Option<Box<dyn Error + Send + Sync>>,
}
impl NativeWorkerError {
    fn invalid(kind: NativeWorkerErrorKind) -> Self {
        Self { kind, cause: None }
    }
    fn caused(kind: NativeWorkerErrorKind, cause: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            cause: Some(Box::new(cause)),
        }
    }
    fn original(mut self, context: InferenceContext<'_>) -> Self {
        if let Err(error) = context.check() {
            self.kind = NativeWorkerErrorKind::Context(error);
        }
        self
    }
    pub fn kind(&self) -> NativeWorkerErrorKind {
        self.kind
    }
}
impl fmt::Debug for NativeWorkerError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("NativeWorkerError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for NativeWorkerError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "native worker: {:?}", self.kind)
    }
}
impl Error for NativeWorkerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|e| e as &(dyn Error + 'static))
    }
}
impl From<ContextError> for NativeWorkerError {
    fn from(error: ContextError) -> Self {
        Self::caused(NativeWorkerErrorKind::Context(error), error)
    }
}
impl From<NativeEmbeddingError> for NativeWorkerError {
    fn from(error: NativeEmbeddingError) -> Self {
        let kind = match error.kind() {
            NativeEmbeddingErrorKind::Context(cause) => NativeWorkerErrorKind::Context(cause),
            kind => NativeWorkerErrorKind::Native(kind),
        };
        Self::caused(kind, error)
    }
}
impl From<GgufRerankError> for NativeWorkerError {
    fn from(error: GgufRerankError) -> Self {
        let kind = match error.kind() {
            GgufRerankErrorKind::Context(cause) => NativeWorkerErrorKind::Context(cause),
            kind => NativeWorkerErrorKind::Rerank(kind),
        };
        Self::caused(kind, error)
    }
}
type WorkerResult<T> = std::result::Result<T, NativeWorkerError>;
trait WorkerClient: Sized + 'static {
    const THREAD_NAME: &'static str;
    type Options: Copy + Send + 'static;
    type Limits: Copy + Send + 'static;
    type Input: Send + 'static;
    type Output: Send + Default + 'static;
    fn preflight(
        options: Self::Options,
        limits: Self::Limits,
        input: Option<&Self::Input>,
        context: InferenceContext<'_>,
    ) -> WorkerResult<()>;
    fn load(
        snapshot_parent: &CapturedDocumentDirectory,
        source: GgufModelSource,
        options: Self::Options,
        limits: Self::Limits,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Self>;
    fn execute(
        &mut self,
        input: &Self::Input,
        request: &RequestState,
    ) -> WorkerResult<Self::Output>;
    fn registration(&self) -> LocalDeviceRegistration;
    #[cfg(test)]
    fn verified_bundle(&self) -> &std::path::Path;
}
struct EmbeddingRequest {
    texts: Vec<String>,
    input: NativeEmbeddingInput,
}
struct RerankRequest {
    query: String,
    documents: Vec<String>,
}
fn borrow_texts(texts: &[String]) -> WorkerResult<Vec<&str>> {
    let mut refs = Vec::new();
    refs.try_reserve_exact(texts.len())
        .map_err(|_| allocation())?;
    refs.extend(texts.iter().map(String::as_str));
    Ok(refs)
}
impl WorkerClient for NativeEmbeddingClient {
    const THREAD_NAME: &'static str = "graphit-native-embedding";
    type Options = NativeEmbeddingOptions;
    type Limits = EmbeddingLimits;
    type Input = EmbeddingRequest;
    type Output = Vec<Vec<f32>>;
    fn preflight(
        options: Self::Options,
        limits: Self::Limits,
        input: Option<&Self::Input>,
        context: InferenceContext<'_>,
    ) -> WorkerResult<()> {
        let refs = input
            .map(|input| borrow_texts(&input.texts))
            .transpose()?
            .unwrap_or_default();
        Self::validate_request(
            options,
            limits,
            &refs,
            input.map_or(NativeEmbeddingInput::Query, |input| input.input),
            context,
        )?;
        Ok(())
    }
    fn load(
        snapshot_parent: &CapturedDocumentDirectory,
        source: GgufModelSource,
        options: Self::Options,
        limits: Self::Limits,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Self> {
        Ok(Self::load(
            snapshot_parent,
            source,
            options,
            limits,
            context,
        )?)
    }
    fn execute(
        &mut self,
        input: &Self::Input,
        request: &RequestState,
    ) -> WorkerResult<Self::Output> {
        Ok(self
            .embed_with_state(&borrow_texts(&input.texts)?, input.input, request)?
            .into_vectors())
    }
    fn registration(&self) -> LocalDeviceRegistration {
        self.registered_device()
    }
    #[cfg(test)]
    fn verified_bundle(&self) -> &std::path::Path {
        self._verified_bundle.path()
    }
}
impl WorkerClient for GgufRerankClient {
    const THREAD_NAME: &'static str = "graphit-native-rerank";
    type Options = GgufRerankOptions;
    type Limits = NativeRerankLimits;
    type Input = RerankRequest;
    type Output = Vec<f64>;
    fn preflight(
        options: Self::Options,
        limits: Self::Limits,
        input: Option<&Self::Input>,
        context: InferenceContext<'_>,
    ) -> WorkerResult<()> {
        let refs = input
            .map(|input| borrow_texts(&input.documents))
            .transpose()?
            .unwrap_or_default();
        Self::validate_request(
            options,
            limits,
            input.map_or("", |input| &input.query),
            &refs,
            context,
        )?;
        Ok(())
    }
    fn load(
        snapshot_parent: &CapturedDocumentDirectory,
        source: GgufModelSource,
        options: Self::Options,
        limits: Self::Limits,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Self> {
        Ok(Self::load(
            snapshot_parent,
            source,
            options,
            limits,
            context,
        )?)
    }
    fn execute(
        &mut self,
        input: &Self::Input,
        request: &RequestState,
    ) -> WorkerResult<Self::Output> {
        Ok(self.score_with_state(&input.query, &borrow_texts(&input.documents)?, request)?)
    }
    fn registration(&self) -> LocalDeviceRegistration {
        self.registered_device()
    }
    #[cfg(test)]
    fn verified_bundle(&self) -> &std::path::Path {
        self.verified_bundle()
    }
}

struct Command<C: WorkerClient> {
    payload: C::Input,
    cancellation: Cancellation,
    deadline: Instant,
    reply: oneshot::Sender<WorkerResult<C::Output>>,
    #[cfg(test)]
    control: Option<TestControl>,
    #[cfg(test)]
    trigger: Option<super::tests::Trigger>,
}
struct Admission<C: WorkerClient> {
    sender: Option<mpsc::Sender<Command<C>>>,
    active: Option<Cancellation>,
}
struct Shared<C: WorkerClient> {
    admission: Mutex<Admission<C>>,
    #[cfg(test)]
    bundle: Mutex<Option<PathBuf>>,
}
// These locks contain only this adapter's short admission/join state, never native calls.
// Poison recovery preserves this adapter's own state; locks never cover user code or SDK calls.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
impl<C: WorkerClient> Shared<C> {
    fn seal(&self) {
        let mut state = lock(&self.admission);
        state.sender.take();
        if let Some(active) = state.active.take() {
            active.cancel();
        }
    }
    fn activate(&self, cancellation: &Cancellation) -> WorkerResult<()> {
        let mut state = lock(&self.admission);
        if state.sender.is_none() {
            return Err(NativeWorkerError::invalid(NativeWorkerErrorKind::Closed));
        }
        state.active = Some(cancellation.clone());
        Ok(())
    }
    fn clear_active(&self) {
        lock(&self.admission).active.take();
    }
}
struct ThreadEnd<C: WorkerClient>(Arc<Shared<C>>);
impl<C: WorkerClient> Drop for ThreadEnd<C> {
    fn drop(&mut self) {
        self.0.seal();
    }
}
struct JoinState {
    handle: Option<JoinHandle<()>>,
    outcome: Option<bool>,
}
struct JoinRecord {
    state: Mutex<JoinState>,
    changed: Condvar,
    completion: watch::Sender<Option<bool>>,
}
impl JoinRecord {
    fn new(handle: JoinHandle<()>) -> Self {
        let (completion, _) = watch::channel(None);
        Self {
            state: Mutex::new(JoinState {
                handle: Some(handle),
                outcome: None,
            }),
            changed: Condvar::new(),
            completion,
        }
    }
    fn take(&self) -> Option<JoinHandle<()>> {
        lock(&self.state).handle.take()
    }
    fn finish(&self, handle: JoinHandle<()>) {
        // No panic payload is retained, displayed or sent to another caller.
        let panicked = handle.join().is_err();
        lock(&self.state).outcome = Some(panicked);
        self.completion.send_replace(Some(panicked));
        self.changed.notify_all();
    }
    fn blocking_join(&self) -> WorkerResult<()> {
        if let Some(handle) = self.take() {
            self.finish(handle);
        }
        let mut state = lock(&self.state);
        while state.outcome.is_none() {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        join_result(state.outcome == Some(true))
    }
    async fn joined(&self) -> WorkerResult<()> {
        let mut completion = self.completion.subscribe();
        loop {
            if let Some(panicked) = *completion.borrow_and_update() {
                return join_result(panicked);
            }
            completion
                .changed()
                .await
                .map_err(|_| NativeWorkerError::invalid(NativeWorkerErrorKind::Join))?;
        }
    }
    fn pending(self: &Arc<Self>) -> Option<RetainedJoin> {
        self.take().map(|handle| RetainedJoin {
            record: self.clone(),
            handle: Some(handle),
        })
    }
    fn launch(self: &Arc<Self>) {
        if let Some(owner) = self.pending() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                // The closure owns a drop guard even if Tokio discards the queued job.
                runtime.spawn_blocking(move || owner.run());
            } else {
                owner.run();
            }
        }
    }
}
// A Tokio blocking job may be cancelled before it starts. Ownership includes
// that path: dropping the queued closure must also complete/publish the real join.
struct RetainedJoin {
    record: Arc<JoinRecord>,
    handle: Option<JoinHandle<()>>,
}
impl RetainedJoin {
    fn run(mut self) {
        if let Some(handle) = self.handle.take() {
            self.record.finish(handle);
        }
    }
}
impl Drop for RetainedJoin {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.record.finish(handle);
        }
    }
}
fn join_result(panicked: bool) -> WorkerResult<()> {
    if panicked {
        Err(NativeWorkerError::invalid(NativeWorkerErrorKind::Join))
    } else {
        Ok(())
    }
}
struct CancelOnDrop {
    cancellation: Cancellation,
    armed: bool,
}
impl CancelOnDrop {
    fn new(cancellation: &Cancellation) -> Self {
        Self {
            cancellation: cancellation.clone(),
            armed: true,
        }
    }
    fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.cancellation.cancel();
        }
    }
}
async fn wait_reply<T>(
    reply: &mut oneshot::Receiver<WorkerResult<T>>,
    context: InferenceContext<'_>,
) -> WorkerResult<T> {
    context.check()?;
    tokio::select! {
        biased;
        _ = context.cancellation().cancelled() => Err(ContextError::Cancelled.into()),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(context.deadline())) => Err(ContextError::Deadline.into()),
        result = reply => {
            context.check()?;
            result.unwrap_or_else(|_| Err(NativeWorkerError::invalid(NativeWorkerErrorKind::Closed))).map_err(|error| error.original(context))
        }
    }
}
// An owned service task retains admission until the native operation actually
// returns, independently of its outer caller's response wait.
async fn wait_retained_reply<T>(
    response: oneshot::Receiver<WorkerResult<T>>,
    context: InferenceContext<'_>,
) -> WorkerResult<T> {
    let mut cancellation = CancelOnDrop::new(context.cancellation());
    let result = response.await.unwrap_or_else(|cause| {
        Err(NativeWorkerError::caused(
            NativeWorkerErrorKind::Closed,
            cause,
        ))
    });
    cancellation.disarm();
    match result {
        Ok(values) => {
            context.check()?;
            Ok(values)
        }
        Err(error) => Err(error.original(context)),
    }
}
/// Send/Sync proxy. Native owners never leave the thread where they are constructed.
struct NativeWorker<C: WorkerClient> {
    registration: Option<LocalDeviceRegistration>,
    shared: Arc<Shared<C>>,
    join: Arc<JoinRecord>,
    model: String,
    revision: String,
    options: C::Options,
    limits: C::Limits,
}
impl<C: WorkerClient> NativeWorker<C> {
    fn spawn(
        snapshot_parent: Arc<CapturedDocumentDirectory>,
        source: GgufModelSource,
        options: C::Options,
        limits: C::Limits,
        capacity: NonZeroUsize,
        context: InferenceContext<'_>,
    ) -> WorkerResult<(
        Self,
        oneshot::Receiver<WorkerResult<LocalDeviceRegistration>>,
    )> {
        C::preflight(options, limits, None, context)?;
        if capacity.get() > tokio::sync::Semaphore::MAX_PERMITS {
            return Err(
                NativeEmbeddingError::invalid(NativeEmbeddingErrorKind::Configuration).into(),
            );
        }
        let model = copy_text(&source.model)?;
        let revision = copy_text(&source.revision)?;
        let (sender, receiver) = mpsc::channel(capacity.get());
        let shared = Arc::new(Shared {
            admission: Mutex::new(Admission {
                sender: Some(sender),
                active: Some(context.cancellation().clone()),
            }),
            #[cfg(test)]
            bundle: Mutex::new(None),
        });
        let (ready, started) = oneshot::channel();
        let worker_state = shared.clone();
        let cancellation = context.cancellation().clone();
        let deadline = context.deadline();
        let handle = thread::Builder::new()
            .name(C::THREAD_NAME.into())
            .spawn(move || {
                let _end = ThreadEnd(worker_state.clone());
                let result = C::load(
                    &snapshot_parent,
                    source,
                    options,
                    limits,
                    InferenceContext::new(&cancellation, deadline),
                );
                worker_state.clear_active();
                let client = match result {
                    Ok(client) => client,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                #[cfg(test)]
                {
                    *lock(&worker_state.bundle) = Some(C::verified_bundle(&client).to_owned());
                }
                if ready.send(Ok(C::registration(&client))).is_err() {
                    return;
                }
                run(client, receiver, &worker_state);
            })
            .map_err(|error| {
                NativeWorkerError::caused(NativeWorkerErrorKind::Spawn, error).original(context)
            })?;
        let worker = Self {
            registration: None,
            shared,
            join: Arc::new(JoinRecord::new(handle)),
            model,
            revision,
            options,
            limits,
        };
        Ok((worker, started))
    }
    pub async fn start(
        snapshot_parent: Arc<CapturedDocumentDirectory>,
        source: GgufModelSource,
        options: C::Options,
        limits: C::Limits,
        capacity: NonZeroUsize,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Self> {
        let (worker, started) =
            Self::spawn(snapshot_parent, source, options, limits, capacity, context)?;
        worker.finish_start(started, context).await
    }
    async fn finish_start(
        mut self,
        mut started: oneshot::Receiver<WorkerResult<LocalDeviceRegistration>>,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Self> {
        let mut cancellation = CancelOnDrop::new(context.cancellation());
        let result = wait_reply(&mut started, context)
            .await
            .and_then(|registration| {
                context.check()?;
                Ok(registration)
            });
        match result {
            Ok(registration) => {
                self.registration = Some(registration);
                cancellation.disarm();
                Ok(self)
            }
            Err(mut error) => {
                self.shared.seal();
                // Keep the actual native join, without blocking a current-thread
                // executor while cooperative model loading reaches its terminal state.
                self.join.launch();
                if let Err(join) = self.join.joined().await
                    && error.cause.is_none()
                {
                    error.cause = Some(Box::new(join));
                }
                if let Ok(Err(native)) = started.try_recv() {
                    error.cause = Some(Box::new(native));
                }
                cancellation.disarm();
                Err(error.original(context))
            }
        }
    }
    pub fn registered_device(&self) -> Option<LocalDeviceRegistration> {
        self.registration
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
    fn enqueue(
        &self,
        payload: C::Input,
        context: InferenceContext<'_>,
    ) -> WorkerResult<oneshot::Receiver<WorkerResult<C::Output>>> {
        context.check()?;
        C::preflight(self.options, self.limits, Some(&payload), context)?;
        let sender = lock(&self.shared.admission)
            .sender
            .clone()
            .ok_or_else(|| NativeWorkerError::invalid(NativeWorkerErrorKind::Closed))?;
        let permit = sender.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => {
                NativeWorkerError::invalid(NativeWorkerErrorKind::Busy)
            }
            mpsc::error::TrySendError::Closed(_) => {
                NativeWorkerError::invalid(NativeWorkerErrorKind::Closed)
            }
        })?;
        context.check()?;
        let (reply, response) = oneshot::channel();
        {
            let state = lock(&self.shared.admission);
            context.check()?;
            if state.sender.is_none() {
                return Err(NativeWorkerError::invalid(NativeWorkerErrorKind::Closed));
            }
            permit.send(Command {
                payload,
                cancellation: context.cancellation().clone(),
                deadline: context.deadline(),
                reply,
                #[cfg(test)]
                control: None,
                #[cfg(test)]
                trigger: None,
            });
        }
        // All temporary Sender/Permit owners drop before any asynchronous response wait.
        Ok(response)
    }
    async fn request(
        &self,
        payload: C::Input,
        context: InferenceContext<'_>,
    ) -> WorkerResult<C::Output> {
        let mut response = self.enqueue(payload, context)?;
        let mut cancel = CancelOnDrop::new(context.cancellation());
        let result = wait_reply(&mut response, context).await;
        cancel.disarm();
        let values = result?;
        context.check()?;
        Ok(values)
    }
    async fn request_retained(
        &self,
        payload: C::Input,
        context: InferenceContext<'_>,
    ) -> WorkerResult<C::Output> {
        let response = self.enqueue(payload, context)?;
        wait_retained_reply(response, context).await
    }
    async fn shutdown_retained(&self, context: InferenceContext<'_>) -> WorkerResult<()> {
        let first_interruption = context.check().err();
        self.shared.seal();
        self.join.launch();
        let result = self.join.joined().await;
        match (first_interruption.or_else(|| context.check().err()), result) {
            (Some(cause), Err(mut error)) => {
                error.kind = NativeWorkerErrorKind::Context(cause);
                Err(error)
            }
            (Some(cause), Ok(())) => Err(cause.into()),
            (None, result) => result,
        }
    }
    pub async fn shutdown(&self, context: InferenceContext<'_>) -> WorkerResult<()> {
        self.shared.seal();
        self.join.launch();
        let mut done = self.join.completion.subscribe();
        loop {
            context.check()?;
            let outcome = *done.borrow_and_update();
            if let Some(panicked) = outcome {
                return join_result(panicked).map_err(|error| error.original(context));
            }
            tokio::select! {
                biased;
                _ = context.cancellation().cancelled() => return Err(ContextError::Cancelled.into()),
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(context.deadline())) => return Err(ContextError::Deadline.into()),
                changed = done.changed() => if changed.is_err() { return Err(NativeWorkerError::invalid(NativeWorkerErrorKind::Join)); }
            }
        }
    }
}
impl<C: WorkerClient> Drop for NativeWorker<C> {
    fn drop(&mut self) {
        self.shared.seal();
        if self.registration.is_some() {
            let _ = self.join.blocking_join();
        } else {
            // A dropped startup future has no published worker owner. Transfer
            // its real join to the same retained cleanup path as shutdown.
            self.join.launch();
        }
    }
}
fn allocation() -> NativeWorkerError {
    NativeEmbeddingError::from(EmbeddingError::Allocation).into()
}
fn copy_text(text: &str) -> WorkerResult<String> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(text.len())
        .map_err(|_| allocation())?;
    owned.push_str(text);
    Ok(owned)
}
fn run<C: WorkerClient>(
    mut client: C,
    mut receiver: mpsc::Receiver<Command<C>>,
    shared: &Shared<C>,
) {
    while let Some(command) = receiver.blocking_recv() {
        #[cfg(test)]
        let mut command = command;
        let context = InferenceContext::new(&command.cancellation, command.deadline);
        let result = context
            .check()
            .map_err(NativeWorkerError::from)
            .and_then(|()| shared.activate(&command.cancellation));
        let result = result.and_then(|()| {
            #[cfg(test)]
            if let Some(control) = command.control.take() {
                match control {
                    TestControl::Gate { entered, release } => {
                        let _ = entered.send(());
                        let _ = release.recv();
                        return Ok(C::Output::default());
                    }
                    TestControl::Panic => panic!("native worker lifecycle fixture"),
                }
            }
            let request = RequestState::new(context);
            #[cfg(test)]
            let request = RequestState {
                trigger: command.trigger.take(),
                ..request
            };
            C::execute(&mut client, &command.payload, &request)
        });
        shared.clear_active();
        let result = result
            .and_then(|values| {
                context.check()?;
                Ok(values)
            })
            .map_err(|error| error.original(context));
        let _ = command.reply.send(result);
    }
}

/// Thread-safe embedding proxy over the shared bounded native owner lifecycle.
pub struct NativeEmbeddingWorker {
    worker: NativeWorker<NativeEmbeddingClient>,
}
impl NativeEmbeddingWorker {
    pub async fn start(
        snapshot_parent: Arc<CapturedDocumentDirectory>,
        source: GgufModelSource,
        options: NativeEmbeddingOptions,
        limits: EmbeddingLimits,
        capacity: NonZeroUsize,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Self> {
        Ok(Self {
            worker: NativeWorker::start(
                snapshot_parent,
                source,
                options,
                limits,
                capacity,
                context,
            )
            .await?,
        })
    }
    #[cfg(test)]
    fn spawn(
        snapshot_parent: Arc<CapturedDocumentDirectory>,
        source: GgufModelSource,
        options: NativeEmbeddingOptions,
        limits: EmbeddingLimits,
        capacity: NonZeroUsize,
        context: InferenceContext<'_>,
    ) -> WorkerResult<(
        Self,
        oneshot::Receiver<WorkerResult<LocalDeviceRegistration>>,
    )> {
        let (worker, ready) =
            NativeWorker::spawn(snapshot_parent, source, options, limits, capacity, context)?;
        Ok((Self { worker }, ready))
    }
    #[cfg(test)]
    fn enqueue(
        &self,
        texts: Vec<String>,
        input: NativeEmbeddingInput,
        context: InferenceContext<'_>,
    ) -> WorkerResult<oneshot::Receiver<WorkerResult<Vec<Vec<f32>>>>> {
        self.worker
            .enqueue(EmbeddingRequest { texts, input }, context)
    }
    pub fn model(&self) -> &str {
        self.worker.model()
    }
    pub fn revision(&self) -> &str {
        self.worker.revision()
    }
    pub fn dimensions(&self) -> usize {
        self.worker.options.dimensions
    }
    pub fn registered_device(&self) -> Option<LocalDeviceRegistration> {
        self.worker.registered_device()
    }
    pub async fn embed(
        &self,
        texts: Vec<String>,
        input: NativeEmbeddingInput,
        context: InferenceContext<'_>,
    ) -> WorkerResult<EmbeddingBatch<'_>> {
        let values = self
            .worker
            .request(EmbeddingRequest { texts, input }, context)
            .await?;
        Ok(EmbeddingBatch::new(self.model(), self.dimensions(), values))
    }
    /// For retained owner tasks: return only after the native operation ends.
    /// The outer request may stop waiting independently; this future must stay owned.
    pub async fn embed_retained(
        &self,
        texts: Vec<String>,
        input: NativeEmbeddingInput,
        context: InferenceContext<'_>,
    ) -> WorkerResult<EmbeddingBatch<'_>> {
        let values = self
            .worker
            .request_retained(EmbeddingRequest { texts, input }, context)
            .await?;
        Ok(EmbeddingBatch::new(self.model(), self.dimensions(), values))
    }
    pub async fn shutdown(&self, context: InferenceContext<'_>) -> WorkerResult<()> {
        self.worker.shutdown(context).await
    }
    /// Seal and observe the actual join before returning, including interruption.
    pub async fn shutdown_retained(&self, context: InferenceContext<'_>) -> WorkerResult<()> {
        self.worker.shutdown_retained(context).await
    }
}
/// Thread-safe paired reranker. The native client remains on one creating thread.
pub struct GgufRerankWorker {
    worker: NativeWorker<GgufRerankClient>,
}
impl GgufRerankWorker {
    pub async fn start(
        snapshot_parent: Arc<CapturedDocumentDirectory>,
        source: GgufModelSource,
        options: GgufRerankOptions,
        limits: NativeRerankLimits,
        capacity: NonZeroUsize,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Self> {
        Ok(Self {
            worker: NativeWorker::start(
                snapshot_parent,
                source,
                options,
                limits,
                capacity,
                context,
            )
            .await?,
        })
    }
    pub fn model(&self) -> &str {
        self.worker.model()
    }
    pub fn revision(&self) -> &str {
        self.worker.revision()
    }
    pub fn registered_device(&self) -> Option<LocalDeviceRegistration> {
        self.worker.registered_device()
    }
    pub async fn score(
        &self,
        query: String,
        documents: Vec<String>,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Vec<f64>> {
        self.worker
            .request(RerankRequest { query, documents }, context)
            .await
    }
    pub async fn shutdown(&self, context: InferenceContext<'_>) -> WorkerResult<()> {
        self.worker.shutdown(context).await
    }
    /// For retained owner tasks: return only after the paired operation ends.
    pub async fn score_retained(
        &self,
        query: String,
        documents: Vec<String>,
        context: InferenceContext<'_>,
    ) -> WorkerResult<Vec<f64>> {
        self.worker
            .request_retained(RerankRequest { query, documents }, context)
            .await
    }
    /// Seal and observe the actual join before returning, including interruption.
    pub async fn shutdown_retained(&self, context: InferenceContext<'_>) -> WorkerResult<()> {
        self.worker.shutdown_retained(context).await
    }
}

#[cfg(test)]
enum TestControl {
    Gate {
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    },
    Panic,
}
#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
