use super::*;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::mpsc as sync_channel, time::Duration};
fn context(signal: &Cancellation) -> InferenceContext<'_> {
    InferenceContext::new(signal, Instant::now() + Duration::from_secs(60))
}
fn options() -> NativeEmbeddingOptions {
    NativeEmbeddingOptions {
        provider: LocalProvider::CPU,
        device_id: 0,
        threads: NonZeroU32::new(1).unwrap(),
        max_tokens: NonZeroU32::new(8192).unwrap(),
        dimensions: 128,
        precision: NativeWeightPrecision::Bf16,
    }
}
fn limits() -> EmbeddingLimits {
    EmbeddingLimits::new(4, 65536, 768, 4 * 768, 4, 65536, 65536).unwrap()
}
fn invalid_source(root: &Path) -> GgufModelSource {
    GgufModelSource {
        model: "worker-fixture".into(),
        revision: "bad-native".into(),
        root: root.into(),
        path: "bad.gguf".into(),
        minimum_bytes: 3,
        maximum_bytes: 3,
        sha256: Sha256::digest(b"bad").into(),
    }
}
fn actual_source(path: &Path) -> GgufModelSource {
    GgufModelSource {
        model: "embeddinggemma2".into(),
        revision: "bfcd298762cc34d0357ece5ebdd31791a3a374d8".into(),
        root: path.parent().unwrap().into(),
        path: path.file_name().unwrap().to_str().unwrap().into(),
        minimum_bytes: 557950176,
        maximum_bytes: 557950176,
        sha256: [
            0x68, 0xba, 0xe2, 0x9d, 0x62, 0xfb, 0x8c, 0x7d, 0x23, 0xe9, 0x8d, 0x21, 0xfd, 0x46,
            0x62, 0x75, 0x3d, 0xdd, 0x63, 0x6e, 0x6b, 0x62, 0xb8, 0xa7, 0x0d, 0xfe, 0x05, 0x9a,
            0x98, 0x44, 0xf2, 0x16,
        ],
    }
}
fn error<T>(result: WorkerResult<T>) -> NativeWorkerError {
    match result {
        Ok(_) => panic!("expected worker error"),
        Err(error) => error,
    }
}
fn native_status(error: &NativeWorkerError) -> Option<i32> {
    error
        .source()?
        .downcast_ref::<NativeEmbeddingError>()?
        .native_status()
}
#[tokio::test]
async fn preflight_rejects_invalid_context_and_channel_capacity_before_spawn() {
    let (_snapshot_root, snapshot_parent) = crate::model_artifact::test_snapshot_parent();
    let directory = tempfile::tempdir().unwrap();
    let signal = Cancellation::default();
    signal.cancel();
    let result = NativeEmbeddingWorker::start(
        snapshot_parent.clone(),
        invalid_source(directory.path()),
        options(),
        limits(),
        NonZeroUsize::new(1).unwrap(),
        InferenceContext::new(&signal, Instant::now()),
    )
    .await;
    assert_eq!(
        error(result).kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
    let signal = Cancellation::default();
    let result = NativeEmbeddingWorker::start(
        snapshot_parent.clone(),
        invalid_source(directory.path()),
        options(),
        limits(),
        NonZeroUsize::new(tokio::sync::Semaphore::MAX_PERMITS + 1).unwrap(),
        context(&signal),
    )
    .await;
    assert_eq!(
        error(result).kind(),
        NativeWorkerErrorKind::Native(NativeEmbeddingErrorKind::Configuration)
    );
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<NativeEmbeddingWorker>();
}
#[tokio::test]
async fn join_record_notifies_only_after_actual_thread_exit() {
    let (release, wait) = sync_channel::channel();
    let (entered, entry) = sync_channel::channel();
    let handle = thread::spawn(move || {
        entered.send(()).unwrap();
        wait.recv().unwrap();
    });
    entry.recv().unwrap();
    let join = Arc::new(JoinRecord::new(handle));
    join.launch();
    assert_eq!(lock(&join.state).outcome, None);
    assert_eq!(*join.completion.borrow(), None);
    release.send(()).unwrap();
    let observed = join.clone();
    tokio::task::spawn_blocking(move || observed.blocking_join())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lock(&join.state).outcome, Some(false));
    assert_eq!(*join.completion.borrow(), Some(false));
}
#[test]
fn queued_tokio_job_abort_still_finishes_actual_std_join() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let (release, wait) = sync_channel::channel();
    let (entered, entry) = sync_channel::channel();
    let blocker = runtime.spawn_blocking(move || {
        entered.send(()).unwrap();
        wait.recv().unwrap();
    });
    entry.recv().unwrap();
    let handle = thread::spawn(|| {});
    let join = Arc::new(JoinRecord::new(handle));
    let owner = join.pending().unwrap();
    let job = runtime.spawn_blocking(move || owner.run());
    assert_eq!(lock(&join.state).outcome, None);
    job.abort(); // Guaranteed queued behind the held sole blocking worker.
    release.send(()).unwrap();
    runtime.block_on(blocker).unwrap();
    assert!(runtime.block_on(job).unwrap_err().is_cancelled());
    join.blocking_join().unwrap();
    assert_eq!(lock(&join.state).outcome, Some(false));
    assert_eq!(*join.completion.borrow(), Some(false));
}

// Lifecycle-only fixture: a real std thread and join, no native model or SDK
// is substituted. Startup's production finish/drop paths are exercised directly.
fn held_startup() -> (
    NativeWorker<NativeEmbeddingClient>,
    oneshot::Receiver<WorkerResult<LocalDeviceRegistration>>,
    sync_channel::Sender<()>,
) {
    let (sender, _receiver) = mpsc::channel(1);
    let (release, wait) = sync_channel::channel();
    let (reply, started) = oneshot::channel();
    let handle = thread::spawn(move || {
        // Safety timeout makes an executor-blocking regression fail, not hang.
        let _ = wait.recv_timeout(Duration::from_secs(2));
        let _ = reply.send(Err(NativeWorkerError::invalid(
            NativeWorkerErrorKind::Closed,
        )));
    });
    let worker = NativeWorker {
        shared: Arc::new(Shared {
            admission: Mutex::new(Admission {
                sender: Some(sender),
                active: None,
            }),
            bundle: Mutex::new(None),
        }),
        registration: None,
        join: Arc::new(JoinRecord::new(handle)),
        model: "lifecycle-only".into(),
        revision: "no-sdk".into(),
        options: options(),
        limits: limits(),
    };
    (worker, started, release)
}
#[tokio::test(flavor = "current_thread")]
async fn interrupted_startup_retains_join_without_blocking_control_executor() {
    let (worker, started, release) = held_startup();
    let join = worker.join.clone();
    let signal = Cancellation::default();
    signal.cancel();
    let mut startup = Box::pin(worker.finish_start(started, context(&signal)));
    let before = Instant::now();
    tokio::select! {
        result = &mut startup => panic!("startup returned before actual join: {:?}", error(result)),
        _ = tokio::time::sleep(Duration::from_millis(10)) => {},
    }
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "control executor was blocked by native join"
    );
    assert_eq!(*join.completion.borrow(), None);
    release.send(()).unwrap();
    assert_eq!(
        error(startup.await).kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
    assert_eq!(*join.completion.borrow(), Some(false));
}
#[tokio::test(flavor = "current_thread")]
async fn dropped_startup_cancels_original_and_retains_actual_join() {
    let (worker, started, release) = held_startup();
    let join = worker.join.clone();
    let signal = Cancellation::default();
    let mut startup = Box::pin(worker.finish_start(started, context(&signal)));
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(startup.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let before = Instant::now();
    drop(startup);
    assert!(signal.is_cancelled());
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "startup Drop blocked the executor"
    );
    assert_eq!(*join.completion.borrow(), None);
    release.send(()).unwrap();
    join.joined().await.unwrap();
    assert_eq!(*join.completion.borrow(), Some(false));
}

#[tokio::test(flavor = "current_thread")]
async fn retained_response_waits_for_terminal_ack_after_cancellation() {
    let signal = Cancellation::default();
    let (terminal, response) = oneshot::channel::<WorkerResult<()>>();
    let mut waiting = Box::pin(wait_retained_reply(response, context(&signal)));
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(waiting.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    signal.cancel();
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(waiting.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    // Channel ACK only: no native model or numerical output is fabricated.
    terminal.send(Ok(())).unwrap();
    assert_eq!(
        error(waiting.await).kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn retained_shutdown_joins_before_return_and_keeps_first_interruption() {
    let (worker, _started, release) = held_startup();
    let join = worker.join.clone();
    let signal = Cancellation::default();
    let mut shutdown =
        Box::pin(worker.shutdown_retained(InferenceContext::new(&signal, Instant::now())));
    tokio::select! {
        result = &mut shutdown => panic!("cleanup returned without native join: {:?}", error(result)),
        _ = tokio::time::sleep(Duration::from_millis(10)) => {},
    }
    assert_eq!(*join.completion.borrow(), None);
    signal.cancel();
    release.send(()).unwrap();
    assert_eq!(
        error(shutdown.await).kind(),
        NativeWorkerErrorKind::Context(ContextError::Deadline)
    );
    assert_eq!(*join.completion.borrow(), Some(false));
}

// Only these test commands gate queue/lifecycle transitions. Every actual worker
// still loads and owns the real native client; no fake native implementation exists.
fn controlled(
    worker: &NativeEmbeddingWorker,
    signal: &Cancellation,
    control: Option<TestControl>,
    trigger: Option<super::super::tests::Trigger>,
    deadline: Instant,
) -> oneshot::Receiver<WorkerResult<Vec<Vec<f32>>>> {
    let (reply, response) = oneshot::channel();
    lock(&worker.worker.shared.admission)
        .sender
        .as_ref()
        .unwrap()
        .try_send(Command {
            payload: EmbeddingRequest {
                texts: vec!["What is the capital of France?".into()],
                input: NativeEmbeddingInput::Query,
            },
            cancellation: signal.clone(),
            deadline,
            reply,
            control,
            trigger,
        })
        .unwrap_or_else(|_| panic!("fixture command admission"));
    response
}
struct GateRelease(Option<sync_channel::Sender<()>>);
impl GateRelease {
    fn release(mut self) {
        let _ = self.0.take().unwrap().send(());
    }
}
impl Drop for GateRelease {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}
fn gate(
    worker: &NativeEmbeddingWorker,
    signal: &Cancellation,
) -> (GateRelease, oneshot::Receiver<WorkerResult<Vec<Vec<f32>>>>) {
    let (release, wait) = sync_channel::channel();
    let (entered, entry) = sync_channel::channel();
    let result = controlled(
        worker,
        signal,
        Some(TestControl::Gate {
            entered,
            release: wait,
        }),
        None,
        context(signal).deadline(),
    );
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    (GateRelease(Some(release)), result)
}
fn assert_joined(worker: &NativeEmbeddingWorker, panicked: bool) {
    assert_eq!(lock(&worker.worker.join.state).outcome, Some(panicked));
    let bundle = lock(&worker.worker.shared.bundle).clone().unwrap();
    assert!(
        !bundle.exists(),
        "verified staging removed after real native thread join"
    );
}
#[tokio::test]
#[ignore = "requires the pinned real BF16 GGUF artifact"]
async fn actual_worker() {
    let (_snapshot_root, snapshot_parent) = crate::model_artifact::test_snapshot_parent();
    let path = PathBuf::from(std::env::var("GRAPHIT_EMBEDDINGGEMMA2_MODEL").unwrap());
    let root = NativeSdkRoot::new().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    std::fs::write(fixture.path().join("bad.gguf"), b"bad").unwrap();
    let signal = Cancellation::default();
    let (failed, mut ready) = NativeEmbeddingWorker::spawn(
        snapshot_parent.clone(),
        invalid_source(fixture.path()),
        options(),
        limits(),
        NonZeroUsize::new(1).unwrap(),
        context(&signal),
    )
    .unwrap();
    let failed_load = error(wait_reply(&mut ready, context(&signal)).await);
    assert_eq!(
        failed_load.kind(),
        NativeWorkerErrorKind::Native(NativeEmbeddingErrorKind::Native)
    );
    failed.worker.join.blocking_join().unwrap();
    assert_eq!(lock(&failed.worker.join.state).outcome, Some(false));
    assert!(lock(&failed.worker.shared.admission).sender.is_none());
    drop(failed);
    let result = NativeEmbeddingWorker::start(
        snapshot_parent.clone(),
        invalid_source(fixture.path()),
        options(),
        limits(),
        NonZeroUsize::new(1).unwrap(),
        context(&signal),
    )
    .await;
    assert_eq!(
        error(result).kind(),
        NativeWorkerErrorKind::Native(NativeEmbeddingErrorKind::Native)
    );

    // Original startup cancellation, including its actual native thread join.
    let start_signal = Cancellation::default();
    let (cancelled, mut ready) = NativeEmbeddingWorker::spawn(
        snapshot_parent.clone(),
        actual_source(&path),
        options(),
        limits(),
        NonZeroUsize::new(1).unwrap(),
        context(&start_signal),
    )
    .unwrap();
    start_signal.cancel();
    let cancelled_start = error(wait_reply(&mut ready, context(&start_signal)).await);
    assert_eq!(
        cancelled_start.kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
    cancelled.worker.shared.seal();
    cancelled.worker.join.blocking_join().unwrap();
    assert_eq!(lock(&cancelled.worker.join.state).outcome, Some(false));
    drop(cancelled);

    let worker = NativeEmbeddingWorker::start(
        snapshot_parent.clone(),
        actual_source(&path),
        options(),
        limits(),
        NonZeroUsize::new(1).unwrap(),
        context(&signal),
    )
    .await
    .unwrap();
    let mut second_options = options();
    second_options.threads = NonZeroU32::new(2).unwrap();
    let peer = NativeEmbeddingWorker::start(
        snapshot_parent.clone(),
        actual_source(&path),
        second_options,
        limits(),
        NonZeroUsize::new(1).unwrap(),
        context(&signal),
    )
    .await
    .unwrap();
    let baseline = peer
        .embed(
            vec!["Paris is the capital of France.".to_owned()],
            NativeEmbeddingInput::Document,
            context(&signal),
        )
        .await
        .unwrap()
        .into_vectors();
    let values = worker
        .embed(
            vec!["What is the capital of France?".to_owned()],
            NativeEmbeddingInput::Query,
            context(&signal),
        )
        .await
        .unwrap();
    assert_eq!(values.model(), worker.model());
    assert_eq!(values.dimensions(), 128);
    assert_eq!(values.vectors()[0].len(), 128);
    assert!(
        (values.vectors()[0]
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt()
            - 1.0)
            .abs()
            < 1e-5
    );

    let gate_signal = Cancellation::default();
    let (release, held) = gate(&worker, &gate_signal);
    let queued_signal = Cancellation::default();
    let queued = worker
        .enqueue(
            vec!["queued request".to_owned()],
            NativeEmbeddingInput::Query,
            context(&queued_signal),
        )
        .unwrap();
    assert_eq!(
        error(worker.enqueue(
            vec!["queue full".to_owned()],
            NativeEmbeddingInput::Query,
            context(&signal)
        ))
        .kind(),
        NativeWorkerErrorKind::Busy
    );
    // Invalid requests retain preflight precedence even while capacity is full.
    assert_eq!(
        error(worker.enqueue(
            vec!["".to_owned()],
            NativeEmbeddingInput::Query,
            context(&signal)
        ))
        .kind(),
        NativeWorkerErrorKind::Native(NativeEmbeddingErrorKind::Embedding(
            EmbeddingError::InvalidRequest
        ))
    );
    queued_signal.cancel();
    release.release();
    held.await.unwrap().unwrap();
    assert_eq!(
        error(queued.await.unwrap()).kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );

    let (release, held) = gate(&worker, &gate_signal);
    let queued_signal = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(20);
    let queued = worker
        .enqueue(
            vec!["expired queue".to_owned()],
            NativeEmbeddingInput::Query,
            InferenceContext::new(&queued_signal, deadline),
        )
        .unwrap();
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    release.release();
    held.await.unwrap().unwrap();
    assert_eq!(
        error(queued.await.unwrap()).kind(),
        NativeWorkerErrorKind::Context(ContextError::Deadline)
    );

    for expire in [false, true] {
        let active = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(if expire { 1000 } else { 30000 });
        let result = controlled(
            &worker,
            &active,
            None,
            Some(super::super::tests::Trigger::new(2, 32, expire)),
            deadline,
        )
        .await
        .unwrap();
        let error = error(result);
        assert_eq!(
            error.kind(),
            NativeWorkerErrorKind::Context(if expire {
                ContextError::Deadline
            } else {
                ContextError::Cancelled
            })
        );
        assert_eq!(native_status(&error), Some(2));
    }
    worker
        .embed(
            vec!["recovery".to_owned()],
            NativeEmbeddingInput::Query,
            context(&signal),
        )
        .await
        .unwrap();

    // A dropped request future cancels that original request without ending the worker.
    let abandoned = Cancellation::default();
    let (release, held) = gate(&worker, &gate_signal);
    let mut abandoned_future = Box::pin(worker.embed(
        vec!["abandoned".to_owned()],
        NativeEmbeddingInput::Query,
        context(&abandoned),
    ));
    tokio::select! { biased; _ = &mut abandoned_future => panic!("queue fixture should await"), _ = tokio::task::yield_now() => {} }
    drop(abandoned_future);
    assert!(abandoned.is_cancelled());
    release.release();
    held.await.unwrap().unwrap();

    // Retain an admission reservation across seal: it cannot publish after seal and
    // must drop before blocking_recv closes. The join cannot complete prematurely.
    let sender = lock(&worker.worker.shared.admission)
        .sender
        .clone()
        .unwrap();
    let permit = sender.try_reserve().unwrap();
    let shutdown = Cancellation::default();
    shutdown.cancel();
    let result = worker.shutdown(context(&shutdown)).await;
    assert_eq!(
        error(result).kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
    assert_eq!(lock(&worker.worker.join.state).outcome, None);
    drop(permit);
    drop(sender);
    worker.shutdown(context(&signal)).await.unwrap();
    assert_joined(&worker, false);
    assert_eq!(
        error(worker.enqueue(
            vec!["closed".to_owned()],
            NativeEmbeddingInput::Query,
            context(&signal)
        ))
        .kind(),
        NativeWorkerErrorKind::Closed
    );
    let after = peer
        .embed(
            vec!["Paris is the capital of France.".to_owned()],
            NativeEmbeddingInput::Document,
            context(&signal),
        )
        .await
        .unwrap()
        .into_vectors();
    assert_eq!(after, baseline);

    // Sealed queued requests are rejected; cancelled shutdown waiter leaves drain alive.
    let gate_signal = Cancellation::default();
    let (release, held) = gate(&peer, &gate_signal);
    let queued_signal = Cancellation::default();
    let queued = peer
        .enqueue(
            vec!["seal queue".to_owned()],
            NativeEmbeddingInput::Query,
            context(&queued_signal),
        )
        .unwrap();
    let shutdown = Cancellation::default();
    shutdown.cancel();
    assert_eq!(
        error(peer.shutdown(context(&shutdown)).await).kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
    assert!(gate_signal.is_cancelled());
    assert_eq!(lock(&peer.worker.join.state).outcome, None);
    release.release();
    assert_eq!(
        error(held.await.unwrap()).kind(),
        NativeWorkerErrorKind::Context(ContextError::Cancelled)
    );
    assert_eq!(
        error(queued.await.unwrap()).kind(),
        NativeWorkerErrorKind::Closed
    );
    peer.shutdown(context(&signal)).await.unwrap();
    assert_joined(&peer, false);
    drop(worker);
    drop(peer);

    let panicked = NativeEmbeddingWorker::start(
        snapshot_parent.clone(),
        actual_source(&path),
        options(),
        limits(),
        NonZeroUsize::new(1).unwrap(),
        context(&signal),
    )
    .await
    .unwrap();
    let panic_signal = Cancellation::default();
    let failed = controlled(
        &panicked,
        &panic_signal,
        Some(TestControl::Panic),
        None,
        context(&panic_signal).deadline(),
    );
    assert!(failed.await.is_err());
    assert_eq!(
        error(panicked.shutdown(context(&signal)).await).kind(),
        NativeWorkerErrorKind::Join
    );
    assert_joined(&panicked, true);
    drop(panicked);
    drop(root);
    println!(
        "actual_worker_pass=true bounded_full=true original_queued_and_active=true startup_failure_joined=true cancelled_shutdown_still_joined=true panic_joined=true peer_after_retire=true verified_dirs_removed=true"
    );
}
