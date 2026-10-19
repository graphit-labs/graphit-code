# Native embedding worker

Project graphit-code; session ses-c98a. Structural parent tsk-c471/N3;
inference front tsk-e6ff. Worker owner tsk-5b17, refinement r3 and
checkpoint/readback r5. Original qualification: 2026-10-11.

Historical status: qualified private worker producer. Three ordinary tests and the
real BF16 native-worker repeat passed after the retained-join correction;
scoped strict library Clippy and fmt passed. Tracker's corrective Source
readback was NOFAULT, with no runtime execution by the reviewer.
Main composition tsk-73e6 passed the three worker tests, the actual BF16
worker fixture and scoped strict library Clippy on CPU Linux. An active
product consumer had not yet been declared in that qualification.
The underlying synchronous SDK library was qualified separately by
tsk-7e88. Worker qualification uses its own evidence and does not
establish an active GGUF product consumer.

The worker now has a real Models consumer. The long-input NN correction is
recorded in Native Task `01M5CF5RHWVG4GRQ9TS1P63FXG`; its Source qualification is
separate from those historical tests. The measured Source08 candidate had
120284 tokens across 15 windows. The original four-thread runtime exceeded its
300-second request context even without queueing, so the old worker passes
do not establish that this NN workload completes. Four threads were an App
provisioning default, not a native SDK limit or a universal user requirement.
Explicit runtime settings and the original request context still belong to
their callers.

The current App policy gives distribution-default background embedding RPCs
15 minutes. Explicitly configured timeouts and a shorter outer Original still
take precedence; authentication token lifetime is unchanged. This worker does
not select that budget: it receives one absolute deadline and retains it across
the complete document. The recorded corrective full-payload fixture used that
900-second Original and its captured available parallelism. Earlier 300-second
failures remain valid evidence, not passing measurements of the revised policy.

The model process lowers its background priority before constructing Tokio and
native workers. GGML uses the configured native worker count. The former
OpenBLAS constructor environment override is gone because OpenBLAS is no longer
linked. Explicit thread settings and original deadlines remain in force.

## Shared paired worker

The existing lifecycle now also powers the concrete `GgufRerankWorker` proxy.
`start(source, GgufRerankOptions, NativeRerankLimits, capacity, context)` and
`score(query: String, documents: Vec<String>, context)` preserve the same bounded
admission, original context and retained joins. Its native client stays inside
the creating thread. There are no runtime aliases or fallback executors.
The shared source descriptor lives in `model_artifact::GgufModelSource`;
requested/effective device metadata lives in `local_execution`.
The [paired classifier contract](rust_gguf_rerank.md) supplies its own model,
recipe and qualification; historical Gemma proofs do not qualify BGE.

## Interface and configuration

[worker.rs](../../crates/graphit-ai/src/native_embedding/worker.rs)
implements the following API. WorkerResult carries NativeWorkerError.
The startup operation is NativeEmbeddingWorker::start; there is no
additional public NativeWorkerStart type in this implementation.

```text
NativeEmbeddingWorker::start(
    source: GgufModelSource,
    options: NativeEmbeddingOptions,
    limits: EmbeddingLimits,
    capacity: NonZeroUsize,
    context: InferenceContext<'_>,
) -> async WorkerResult<NativeEmbeddingWorker>

NativeEmbeddingWorker::embed(
    &self, texts: Vec<String>, input: NativeEmbeddingInput,
    context: InferenceContext<'_>,
) -> async WorkerResult<EmbeddingBatch<'_>>

NativeEmbeddingWorker::shutdown(
    &self, context: InferenceContext<'_>,
) -> async WorkerResult<()>
```

`model()` and `revision()` borrow the captured worker identity;
`dimensions()` returns its configured output width. `registered_device()` returns
the actual loaded client's registration after successful startup, including
requested/effective provider/index and Auto's finite CPU fallback reason. Returned vectors
are owned, and the existing EmbeddingBatch borrows that worker's model
identity. This is the same result type as the synchronous client.

The caller supplies the source, role, model identity/revision, dimensions,
device/index, threads, weight precision, token limit, EmbeddingLimits and queue
capacity explicitly. CPU0 remains explicit in standalone fixtures; the worker
never invents a device default. No ONNX tensor settings, discovery, profile default
or model selection is fabricated for GGUF. GgufModelSource is a data
descriptor: its caller must supply the actual binding/authorization
before startup. It does not itself grant project or Installation access.
The synchronous client's verified staging and precision/input recipe
are described in [rust_native_embedding.md](rust_native_embedding.md).

NativeEmbeddingWorker is a Send+Sync proxy. NativeEmbeddingClient remains
non-Send and is constructed, used and destroyed inside the named
`graphit-native-embedding` std::thread::Builder thread. Source ownership
moves into that thread; native model/context owners never cross it.
NativeSdkRoot stays with the bootstrap owner until all actual worker
joins. The proxy does not send that root, create a root automatically,
or implement global model admission, shared-weight caching or M/E drain.

The adapter uses the existing tokio 1.53.2 bounded mpsc, oneshot and watch
facilities, plus std JoinHandle/Mutex/Condvar. Production Tokio dependency
and Core async-cancel feature activation are Root-owned fragments using
the existing pins. This is AI ownership, not a global Runtime change.
Main Cargo/Lock/exports remain Root-owned.

The native producer uses the single unpatched Cargo engine and existing CXX
adapter. [Cargo build and distribution](../development/llama-cargo.md) defines
the exact revision, toolchain, optional CUDA/Metal features and notices.
Native platform execution requires its own qualification.

## Admission and response ownership

Startup reuses NativeEmbeddingClient::validate_request with an empty
input batch before spawning. It rejects capacity above
tokio::sync::Semaphore::MAX_PERMITS before mpsc construction. Capacity is
already positive through NonZeroUsize. Captured model/revision copies
are fallible. The startup original signal is published as active before
thread spawn, so sealing can cancel an in-progress model load.

Each embed call takes ownership of the caller's text vector and first
reuses the synchronous whole-request preflight through temporary borrowed
references, with fallible reference-vector allocation:
original context, raw/prepared prefix bytes, count, dimensions and
checked vector values. A short admission mutex yields a temporary
Sender only while open. `try_reserve()` obtains bounded queue capacity
before transferring the vector: full returns Busy, closed returns Closed. It never
waits in an unbounded payload-admission queue.

With the reservation held, the adapter moves the existing text vector
into the command without copying its strings. The model service passes
its already-owned vector directly to this API. The adapter rechecks the
original context and, under the short
admission mutex, rechecks sealing before sending. Thus a reservation
obtained before seal cannot bypass the final sealed-state check. Local
Sender/reservation owners are dropped before the asynchronous response
wait; they do not keep blocking_recv alive across that await.

Commands own texts, role, a clone of the same original Cancellation,
the unchanged absolute Instant and their oneshot reply. Options/model
identity remain fixed in the worker. No deadline is reset for queueing,
load, SDK processing or response publication. A queued invalid context
is rejected before active publication and SDK work. Activation shares
the admission mutex with seal, while native calls run outside the lock.

The worker borrows its owned strings for the synchronous SDK call,
uses the same sequential all-or-error embedding implementation, moves
owned vectors across the reply, and checks the original again before
publication. The caller constructs EmbeddingBatch only after a final
original check. No partial batch, fallback or synthetic success repairs
a failed call.

Response waiting selects original cancellation, absolute deadline or
the oneshot, with original checks before and after receiving. An armed
CancelOnDrop signals the original request when the embed future is
abandoned. Ordinary resolution disarms that guard; a deadline already
remains visible through the unchanged Instant. Returning from a cancelled
or expired response wait is not proof that SDK processing has already
terminated. Native callbacks and the owned drain/join finish separately.

## Seal, shutdown and real joins

Seal removes the shared Sender and cancels the active original signal.
Outstanding temporary/reserved Sender owners either observe seal before
send or drop normally. The receiver's blocking_recv wakes when channel
Sender owners close. Queued commands are checked for original validity
and sealed admission before any SDK call, then return their actual error
without executing native work. Worker thread exit also seals via RAII.

The std JoinHandle has one actual join owner. JoinRecord takes it once;
only after handle.join returns does it store the outcome and notify
watch/Condvar waiters. Multiple shutdown paths converge on that stored
result. Short internal state locks never cover SDK calls or user code;
poison recovery is limited to this adapter's own admission/join state.

shutdown seals first and launches the join before waiting on the
shutdown caller's original context. Within a Tokio runtime,
spawn_blocking owns a RetainedJoin guard containing the Arc<JoinRecord>
and an Option<JoinHandle>. Both normal run and guard Drop consume the
handle through the real finish/join/publication path. If Tokio discards
the blocking job while it is still queued, dropping that closure also
joins and publishes completion. Cancellation, deadline or future drop
of the shutdown waiter ends only that wait: it does not detach or
discard the native thread's JoinHandle.
If no Tokio runtime is available, the adapter performs the join directly;
it does not create an implicit runtime.

Drop of a published worker seals and synchronously waits for its actual join.
If another path owns that join, it waits on the Condvar until that owner finishes.
Startup failure instead launches the existing retained blocking cleanup and
awaits its watch notification asynchronously, preserving the native cause and
actual terminal join before returning an error. A dropped startup future cancels
its original signal and transfers its unpublished worker's join to that same
RetainedJoin path. It does not block a current-thread executor or claim that
native loading has already terminated. These cleanup
paths may outlast the caller's response deadline because an SDK callback
is cooperative. They do not promise hard CPU preemption or a universal
cleanup latency. Destruction of the thread-local native client and its
verified TempDir precedes completed join notification.

## Error and privacy boundary

NativeWorkerError kinds are Context(ContextError), Busy, Closed,
Native(NativeEmbeddingErrorKind), Rerank(GgufRerankErrorKind), Spawn and Join. Allocation failures
reuse Native(Embedding(EmbeddingError::Allocation)); there is no invented
separate public Allocation variant. Spawn preserves real IO cause.
Native failures preserve the synchronous NativeEmbeddingError/source
chain, including its actual status when SDK processing occurred.
An original context failure takes the Context kind on the relevant
return path while retained causes remain available through Error::source.

Debug/Display expose only the controlled worker kind. Model text, path,
identity, credential material and SDK message payloads are not formatted
by the worker. A panicked join records the Join classification without
retaining, displaying or forwarding its panic payload. Callers must
keep source-chain handling consistent with that redaction boundary.

## Qualification and affected commands

Private evidence resides in the repository-relative directory
`.build/rust-ai/embeddinggemma2-sdk-qualification`; these receipts are
not shipped API artifacts. The current `worker-unit-receipt.json`
records exit0, three ordinary tests PASS, runner wall11.387116s. Cargo
reported compilation11.33s and test phase0.00s. The earlier two-test
run recorded wall2.918751s/compilation2.87s. These are command/build
costs, not isolated inference latency or a performance gain.

[worker_tests.rs](../../crates/graphit-ai/src/native_embedding/worker_tests.rs)
contains ordinary preflight/capacity/Send+Sync, actual-thread join
notification and queued-Tokio-job-abort tests. The added test holds the
sole Tokio blocking worker with max_blocking_threads=1, aborts the join
job before it starts, observes the cancelled JoinError and verifies the
real std-thread join outcome Some(false) and watch completion. This is
actual thread/join evidence; it does not execute a BF16 model or use a
fake SDK. The actual module path, used by the unit receipt, is
`native_embedding::worker::tests`.

Affected commands from the private workspace:

```text
cargo test --offline --locked -j1 -p graphit-ai --lib native_embedding::worker::tests -- --skip actual_worker
cargo test --offline --locked -j1 -p graphit-ai --lib native_embedding::worker::tests -- --ignored --nocapture
```

The ignored test requires GRAPHIT_EMBEDDINGGEMMA2_MODEL pointing to the
provided BF16 GGUF pin, 557950176 bytes, SHA-256
`68bae29d62fb8c7d23e98d21fd4662753ddd636e6b62b8a70dfe059a9844f216`.
It uses the real client/model and separate filtered process so SDK root
initialization occurs once. cfg(test) gates, panic command and callback
Trigger provide deterministic lifecycle witnesses; they are not a fake
SDK, public injection interface or production counter framework.

`worker-actual-before-join-drop-fix-receipt.json` preserves the first real
BF16 worker PASS: runner wall29.167423s and test phase28.97s. Subsequent
review found that discarding a queued spawn_blocking closure could drop
the std JoinHandle without calling finish. RetainedJoin fixes that
concrete lifecycle gap. The earlier SDK PASS remains a separate
historical domain and is not the proof for the fixed current Source.

`worker-actual-receipt.json` and its log record the repeat after the
guard fix: exit0, runner wall34.758636s, compilation7.51s and test27.11s.
`worker-clippy-receipt.json` records scoped library strict Clippy exit0,
wall1.257202s. Its initial style FAIL, wall1.26415s, preceded the
collapsible_if correction and remains historical. Source fmt passed.
These are command/build/test costs, not inference latency or a
performance gate. Tracker verified the corrected RetainedJoin run/Drop
and queued-job-abort guard as NOFAULT; that review did not execute tests.

The passing real-worker test covers bounded Busy, queued and active
original failures, startup failure joins, abandoned request cancellation,
seal/reservation closure, a cancelled shutdown waiter with continued
join, native worker panic/join, staging-directory removal and a peer that
remains usable after the other worker retires. Cancellation triggered
after native callback32 preserved actual SDK status2. Inflight deadline
also preserved SDK status2. The deadline fixture does not establish a
universal callback-count threshold or hard cancellation latency beyond
the configured case. No expensive historical 8192-token encode was
replayed or claimed as current worker proof.

That historical private-worker qualification covers CPU Linux only. It does
not establish GPU, Windows/macOS, parallel encodes, all possible failure joins,
hard preemption, performance guarantees or App/M/E integration and an active
GGUF model service. Those consumers require their own evidence. The worker
implementation preserves Broker, Auth, Runtime, Data/Lance, Knowledge, AST,
Core, FS and M/E ownership.

## Documentation and integration handoff

Source-to-document comparison records the actual start/embed/shutdown
ABI, bounded admission, original context, join retention and error kinds.
Document-to-Source comparison uses the linked worker and tests, keeping the
historical ordinary receipt and corrected native repeat separate from the
pre-fix BF16 PASS. The historical activated typed ModelManifest supported ONNX and
EmbeddingGemma2GGUF. The current Linux installation has an active GGUF Models
service with separate Models/Source consumer qualification and a productive
Code/main Knowledge semantic query with its associated Source. These consumer
proofs are distinct from the historical private-worker tests and the bounded
one-file Source proof above; they do not establish every corpus, platform or
global lifecycle path. Service composition is described in
[the model-service contract](rust_model_service.md).

Shared dependency/export activation belongs to the Runtime integration. Native
inference executes in the worker with the caller's captured threads, queue
capacity and Original. App/M/E, provisioning, selection and their actual
consumers retain their own owners and evidence. Worker qualification cannot
inherit full consumer or global lifecycle proof from the synchronous SDK leaf.


## Current shared startup lifecycle check

Five ordinary worker checks passed after the shared paired-worker change,
including current-thread control responsiveness while an interrupted startup
retains a real std-thread join, future-drop cancellation of its Original,
notification only after thread exit and the queued Tokio-job abort guard.
The harness took 0.02 seconds. These are lifecycle fixtures with real threads
and joins; they substitute no native model and claim no inference qualification.
The actual BF16 worker repeat is a separate ignored fixture with the pinned
artifact and an isolated SDK root.


The actual pinned BF16 worker repeat after that correction also passed in
58.42 seconds (58.45 seconds wall time). It exercised bounded queue saturation,
original queued/active cancellation, startup failure cleanup, interrupted
shutdown, a deliberate lifecycle-test panic with its real join, an independent
peer after retirement and removal of verified staging after native owner drop.
The deliberate panic is an exercised test path, not a suppressed production
failure. This short worker regression does not repeat or qualify the long
Source08 workload.


## Retained operation primitives

`NativeEmbeddingWorker::embed_retained(texts, input, context)` and
`GgufRerankWorker::score_retained(query, documents, context)` reuse the same
validated bounded enqueue and oneshot result as the ordinary methods. They wait
for the native command's terminal response even after the Original is cancelled
or expires, then return its typed outcome with Original priority. Their caller
must retain the owned task; an independently cancellable outer response waiter
is not that task's owner. Ordinary `embed` and `score` preserve early response
cancellation for direct consumers.

Both proxies expose `shutdown_retained(context)`. It seals and starts the same
RetainedJoin even when the supplied context is already invalid, awaits the
actual std-thread join, then reports the first context interruption or the real
join failure. It does not renew an inference deadline or create a runtime/thread.
A normal deadline-bounded `shutdown` can stop waiting earlier and remains active
for those callers that separately retain drain ownership.

Models and the Broker are the two actual retained-operation consumers. Their
existing owners implement idle policy and retain bootstrap until actual drain;
AI supplies no idle framework. A dropped startup future preserves the native
join but does not grant its caller a completed-drain receipt. A NativeOwner
already being constructed retains its Kernel; before construction only artifact
IO is active and new ownership is refused after bootstrap closes.

Seven ordinary lifecycle checks passed after these primitives were added,
including terminal-ACK waiting after cancellation and a retained shutdown that
joins before returning while preserving the first deadline interruption. The
channel checks contain no fabricated native model or numerical output; actual
GGUF idle/reload qualification is recorded separately with Models.
