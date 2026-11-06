---
title: Rust shared model service
description: Captured local model selection, lazy owners and authenticated private inference RPC.
type: specification
updated: 2026-10-24
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-8140}
    relation: consumer_boundary
  - target: {type: task, id: tsk-222c}
    relation: consumer_boundary
  - target: {type: task, id: tsk-598c}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: consumer_boundary
  - target: {type: task, id: tsk-1da6}
    relation: implements
  - target: {type: task, id: tsk-9d34}
    relation: corrects
  - target: {type: task, id: tsk-bcf2}
    relation: uses_api
  - target: {type: task, id: tsk-b577}
    relation: implements
  - target: {type: task, id: tsk-933e}
    relation: implements
  - target: {type: task, id: tsk-79b9}
    relation: implements
  - target: {type: task, id: tsk-8e7b}
    relation: consumer_boundary
  - target: {type: task, id: tsk-5c9b}
    relation: consumer_boundary
  - target: {type: task, id: tsk-f1ba}
    relation: implements
  - target: {type: task, id: tsk-ec10}
    relation: derived_from
  - target: {type: task, id: tsk-73e6}
    relation: uses
  - target: {type: task, id: tsk-8853}
    relation: implements
  - target: {type: task, id: tsk-e90f}
    relation: implements
  - target: {type: task, id: tsk-6ffa}
    relation: implements
  - target: {type: task, id: tsk-658a}
    relation: uses
  - target: {type: task, id: tsk-26fe}
    relation: uses
  - target: {type: task, id: tsk-e87c}
    relation: requires
  - target: {type: task, id: tsk-0529}
    relation: consumer_boundary
---

# Rust shared model service

`graphit_daemon::model_service` supplies the Rust model-service producer for
[daemon lifecycle](rust_daemon_memory_lifecycle.md). It reuses the
[local runtime](rust_local_runtime.md), [model binding](rust_model_bindings.md)
and [local embedding/rerank clients](rust_local_models.md). GGUF uses the
[native EmbeddingGemma2 worker](rust_native_embedding_worker.md). App E owns product
routing, the dedicated child, listener and Process lifecycle. Qualification of
this library and RPC does not qualify those process journeys.

## Captured configuration and identity

`ModelSettings::read(&ConfigSnapshot)` reads `daemon.models`. Missing returns
`None`; explicit empty `slots` is an empty configuration. Layer resolution and
Unset remain ConfigStore behavior. Structured settings reject unknown fields.
Each unique slot supplies name, selection id/revision/task, bundle and
manifest_relative. Runtime settings use a required `backend` discriminator:
`gguf-embeddinggemma2` and `gguf-bge-reranker-v2-m3` each carry required
nonzero `threads` and `queue_capacity`; no runtime library or external tokenizer
field is accepted.
Device and index come only from the independent canonical Auth services
`provider.embedding` and `provider.rerank`, through `LocalExecutionSelection`.
The duplicate slot `provider` field is rejected. GGUF takes dimensions, token
limit and precision from its bound manifest recipe. Mixed fields and mismatched backends fail Configuration;
there is no old untagged settings decoder. Requests cannot override paths,
options or providers.

Create each `CapturedModelInput::capture(slot, bundle, &SnapshotPolicy,
SourceReadLimits, Arc<SnapshotContext>)` with the actual original context.
Private fields tie its Core capture to the same canonical bundle. Before capture,
exclude remote, disabled and inactive purposes using the actual selection.
`LocalExecutionSelection::from_services(embedding, rerank, effective_rerank)`
validates only admitted local purposes. `search.rerank` is caller policy, default
false; an explicit authenticated rerank RPC remains possible with that default.
Provider availability and configuration alone do not populate the child catalog.
Then
`ResolvedConfig::resolve(settings, selection, inputs, &SnapshotContext, &ManifestLimits)`
compares that bundle and binds the manifest using existing Core/AI producers.
Synchronous filesystem work checks the original before and after calls; this
is not syscall preemption. The resolved owner retains validated configuration,
not captured readers, contexts or Auth leases.

Runtime configuration uses
`ResolvedConfig::resolve_declared(settings, selection, &Arc<SnapshotContext>,
&SnapshotPolicy, SourceReadLimits, &ManifestLimits)`. It captures only each
selected manifest as a Core FILE, validates the portable configured path and
bundle containment, parses the same bounded recipe/selection and revalidates
that manifest under the same Original. It does not read or copy GGUF weights.
Both resolutions use one compiler for ConfigId, vector_id, device and worker
options. The child Global resolver uses this declarative path; callers that
need a complete captured-source proof continue to use `resolve` and its full
binding API.

The dedicated child starts with an empty catalog. Startup validates active Auth
and local capability, idle policy, finite bootstrap and listener readiness; it
does not parse `ModelSettings` or read model bundles. For an absent slot, both
authenticated embedding and rerank RPCs use `GlobalModelConfig::ensure_model`
with their actual task. Under the existing serialization guard, it snapshots
`ModelService::slot_names` from the registry mutex, adds the new slot name and
resolves that union against fresh Global settings. Only those names and their
task-specific local executions reach manifest capture. There is no parallel
demand cache or new wire flag. The guard remains owned through capture/install
and the actual blocking terminal, including after caller Drop.

Requested resolution still parses the complete structured `ModelSettings`
schema before selecting slots. Independence from unused bundles and manifests
does not imply tolerance of malformed unused settings. Missing settings, absent
or invalid slots, manifest errors, wrong purposes and ConfigId mismatches remain
real errors at the first admitted RPC; readiness is not model availability.
Remote, disabled and NoActive configurations cannot bootstrap a local child.

A configured declaration can precede installed or resident weights. Actual
file/length/digest verification remains mandatory in the existing native staging
producer before the first load and output, including after idle reload. Missing,
corrupt, changed or unsupported weights return their real terminal error;
declaration and refresh ACK do not prove physical artifact integrity.
The two runtime backends require one self-contained GGUF with its embedded
tokenizer. Additional declared artifacts fail Configuration with an Unsupported
cause, so no required auxiliary integrity declaration is silently ignored.
License/model-card files may remain in the bundle without being inference
artifacts. Generic full binding retains its existing multi-artifact contract.

The declarative security focal passes for both recipes: equality with the full
compiler, declarations with absent weights, and rejection of an extra required
artifact before any weight read. A private copy of the real pinned BF16 model
with a changed byte returns Integrity before output and completes its native
worker join (16.45 seconds in the recorded Linux run). The full binding's ten
integrity/limit/Original tests remain passing. A preceding phase run measured
36.405 seconds resolving the full F16 configuration before native load; that is
historical preparation cost, not a claimed universal speedup or encoder latency.

`config_id(slot)` returns data-only SHA256 equality of validated selection,
artifacts/digests/roles/formats/bounds, tokenizer/recipe, conservative canonical
bundle, canonical device/index and execution options. Slot names, ordering,
Auth and credentials are excluded. ConfigId establishes neither authority nor
artifact integrity. Runtime load still verifies the captured manifest's files.
`ExpectedModel::new(&ResolvedConfig, slot, auth_provider)` binds caller metadata
to the explicit actual Auth provider; it cannot construct a grant.

`ResolvedConfig::config_ids()` returns the complete `BTreeMap<String, ConfigId>`
by slot. Refresh resolves exactly the caller's required slot names, validates
their ConfigIds and returns that installed map without unrelated global slots;
it introduces no global ConfigId. An empty required map retires all slots.

`ExpectedModel::vector_id()` exposes a separate portable SHA256 digest resolved once
after manifest and artifact verification. It hashes required artifact content digests,
model/revision/task, pinned backend recipe, tokenizer roles and digests, input/output
names, prefixes, token limit, pooling/normalization, precision and dimensions/MRL;
The paired classifier identity includes its strict token template, truncation,
precision and score transform. Vector identity excludes bundle locations, threads,
queues and execution-provider selection. No weight file is
rehashed by a query or by a project consumer. AST NN, Memory and Knowledge persist
this vector identity, including document fingerprints and the Hub Knowledge Pair.
RPC expected IDs, lazy owners and configuration reload still use operational
`ConfigId`; relocating a verified bundle or changing scheduling therefore changes
that operational ID without invalidating compatible persisted vectors.

`ExpectedModel::model()` and `revision()` borrow identity copied from that
validated manifest. Together with `config_id()`, `vector_id()` and `dimensions()`, they
let a real consumer observe metadata without an independent model/revision input,
a full manifest accessor or a dummy inference request. Those values remain data;
lease, current configuration, RuntimeKey, purpose and operation Original checks
still happen at their actual boundaries. The App
`ast_semantic::expected_native_nn_model(&ExpectedModel)` uses that same owner for
NativeNN schema/codec metadata, including an empty corpus; a rerank expectation
with no embedding width returns `NativeNnError::InvalidModel`. This bridge does
not execute ModelService inference or qualify the product NN caller. The actual App
bridge qualification passes three captured-config/NativeNN codec tests on Linux
(0.01 seconds; 22.373 seconds including Cargo), plus strict Rust Clippy in
169.228 seconds. It reuses the qualified CPU ORT1.27.1 library without starting
a worker or executing inference. Evidence is in
`.build/ast-native-nn-model-identity/qualification.json`; Windows/macOS are UNRUN.
Actual product caller/assembler and ModelService CodeDocument/CodeQuery integration
remain required.

## Owners, execution and shutdown

Core's `lower_background_priority` runs in `main` before Tokio and native
workers. Upstream GGML uses the configured runtime thread count. The obsolete
OpenBLAS constructor override is removed; no BLAS helper pool is initialized.
On Linux, later workers inherit the startup priority;
existing threads are not scanned. Unavailable CPU/I/O changes report only the
Core stage/error category and startup continues. Priority is best effort,
without a CPU quota or latency guarantee. Emitted-runtime and native platform
qualification remain separate from this launcher/entrypoint contract.

`ModelService::new(ServiceLimits)` requires explicit nonzero maximum active
workers and existing SDK EmbeddingLimits/NativeRerankLimits. `install(config,
InferenceContext)` atomically changes a checked process-local generation only
for effective configuration changes. Equivalent slots share an existing lazy
owner; distinct configurations do not. Admission checks expected ConfigId and
captures the same Arc/generation under the map lock before waiting. A changed
identity yields ConfigChanged without loading or running. An admitted old
request retains old identity/generation after replacement.

`embed(slot, expected_config_id, Query|Document|CodeQuery|CodeDocument, Vec<String>, context)` and
`rerank(slot, expected_config_id, query, documents, context)` call the existing
SDK request-preflight producer before capacity wait or first native load. This
preserves empty input, prefixes, byte/dimension budgets and context errors.
Both GGUF families lazily start their concrete native worker and release the
owner mutex before queueing inference. They reuse existing ConfigId owners across
projects; native model state is not reconstructed for each request. Embedding
uses its query/document/code retrieval recipe; reranking uses the actual paired
classifier recipe. Busy and Closed remain distinct, and actual native causes
stay in the producing process.

The model artifact's observed file length bounds the SDK staging read, while
its bound digest provides integrity. Native owners and proxy drops run through
owned blocking cleanup so their real thread joins do not block executor threads.
Retirement moves both the old slot map and its temporary reuse references into
blocking cleanup, retaining admitted owners until their requests finish. A cleanup
join failure remains a causal worker error for the drain. Shutdown retains remaining
owners through the drain and awaits their final native drops.
Successful completed cleanup entries are reaped by both inference admission and
`install`, including equivalent-map refresh. Repeated refresh without inference
therefore does not retain completed retirement records indefinitely. Failed or
panicked tasks do not publish successful completion and remain in the owned
registry for the actual drain to observe their causes.

A request-local stderr observation distinguishes `admission` (waiting for global
capacity or owner access) from `execution` (the owned service task has been
admitted). It emits only when the existing absolute deadline has expired at
return or future Drop, with the operation and phase, and never includes inputs,
model settings, paths, credentials or native error payloads. Execution includes
load, worker response and retained cleanup; it does not identify a native kernel
phase. An ordinary success or cancellation before expiry emits nothing. This
observation changes neither the deadline nor the returned error and introduces
no queue, concurrency change or worker state. A peer disconnect before the
child's reconstructed deadline can leave no observation; absence is not evidence
that native execution began. Concurrent projects cannot be attributed from a
nearby timestamp alone.

Each request has its own cancellation guard. Dropping its future cancels that
worker but does not treat future abortion as native completion. The service
registry retains native ownership and a join handle through the worker's actual
terminal result and join. Replacing/removing A does not cancel B. `shutdown`
seals admission, cancels workers and drains joins. Caller interruption may end
waiting for shutdown while the owned drain continues. The authenticated shutdown
RPC responds only after drain; E invokes it before Process cancel/await and
retains its Process cleanup on RPC failure.

## Private RPC and caller gates

Transport pins are tarpc `=0.38.0` with default features disabled and
`serde-transport-bincode`, bincode `=1.3.3`, and tarpc's tokio-serde `0.9`
Bincode reexport. Mature LengthDelimitedCodec enforces max frame length before
decode; both ends use the same bincode byte limit and reject trailing bytes.
`RpcLimits::new(frame_bytes, texts, input_bytes, output_values)` requires explicit
positive representable bounds. These wire bounds supplement SDK limits; they
do not prove exact decoded allocations or RSS.

`ModelServiceClient::limits(&self) -> RpcLimits` returns a read-only Copy of
these four bounds after the client constructor validates them. Consumers can
size text batches against the effective count, input-byte and output-value
bounds without duplicating RPC configuration. Changing the returned copy does
not change the client. The observation grants no service access and does not
replace the client's actual input/output, frame, model metadata or Auth checks.
`tsk-598c` adds this producer; its focused qualification is recorded separately
in `.build/rust-model-client-limits/validation.json`, without inference or J8
integration qualification.

`server_transport(stream, limits)` returns the bounded mature transport.
E wraps it with qualified `model_transport::DisconnectTransport::new(transport,
connection.clone())` and passes the **same** Cancellation to
`serve_connection(transport, service, Arc<RuntimeSecret>, limits, connection)`.
EOF, transport error and Drop signal only that connection. The server keeps
polling the request stream while its handler waits, cancels handlers on the
connection signal, and preserves worker join ownership. One operation uses one
short connection: pending/inflight/server-response capacity is one. E owns
listener/connection admission policy.

E is the sole parent RuntimeKeyOwner. The child receives finite zeroizing
64-byte lowercase-hex credential through private Process stdin plus EOF. No
credential belongs in argv, environment, discovery, caches, logs or evidence.
RuntimeSecret and RuntimeCredential redact Debug; mature constant-time comparison
authenticates before model admission. The child reads the canonical installation
AuthStore for configuration selection; that observation creates no grant,
RoleIssuer, project realm or project parser.

`ModelServiceClient::embed/rerank` borrow the actual LocalServiceLease and
RuntimeKeyOwner, ExpectedModel, owned inputs and a connection future returning
AsyncRead + AsyncWrite + Unpin. They recheck actual purpose/provider/execution,
the original request and fresh runtime possession before polling, after every
wait, and before delivering success **or error**. A caller with a mismatched
purpose/execution is denied. The device and index must exactly match the
captured canonical selection; CUDA receives the actual nonnegative index.
Metal requires macOS/index zero under the canonical Auth platform contract. Explicit unavailable devices return UnsupportedExecution. NoActive, Disabled and stale leases come from Auth.
NeedsRefresh belongs to the BrokerNative credential lease, not this local lease.
Auth `Profile(Cancelled/Deadline)`, `Store(Profile(Cancelled/Deadline))` and
`Store(Core(Cancelled/Deadline))` map to `Cancelled`/`Deadline` even before the
first client poll; the actual `ServiceUseError` remains the source. Physical
credential inspection can observe request cancellation or expiry; neither means
that a credential or profile was rejected. Denied, Conflict, Stale, WrongMode and
other actual denial errors retain the `Authentication` category.

The embedding/rerank callers preserve this classification at their existing
boundaries: IO uses `Interrupted`/`TimedOut`; Knowledge and Memory Browser search
use HTTP 408 and `request_cancelled_or_expired`; Memory MCP uses that same
request error code.
Other provider/store errors retain their existing status and error codes. Memory
background batch failures retain the complete typed failure, including any last
confirmed attachment, while exposing interruption or timeout to the existing
preemption handling. Classification neither changes an Auth profile nor retries,
renews an Original, or treats unconfirmed work as successfully published.

Embedding calls require `EmbedOperation { input, texts, deadline }`, with an
explicit finite absolute deadline for that RPC. The client captures the effective
deadline once as the earlier of the operation deadline and the original deadline,
when the latter exists. A service Original without a global deadline is valid
for embedding only with this explicit RPC deadline. The same Original,
cancellation, service lease and PROJECT admissions remain authoritative; the
operation does not create or renew any of them. The background indexing callers
for AST, Knowledge and Memory use a 900-second (15-minute) embedding RPC default
only for `DistributionDefault`, covering service admission and execution. This
is a finite background processing budget, not a speed improvement, SLA or
universal completion promise. Every explicit `hub.limits.request.deadline_ms`
value from `Configuration` remains authoritative, smaller or larger. This BG
budget does not change `AuthAccessTokenTTL` or renew service credentials.
The daemon builder no longer injects a redundant Distribution document: the
existing Hub catalog already resolves the same defaults as `DistributionDefault`.
Every explicitly configured value remains `Configuration`, even when equal to the
catalog default; BG code uses that existing origin without comparing values or
adding provenance. All concrete Hub limits and the global Hub/foreground default
of 120000 ms stay unchanged. Each actual RPC gets its deadline once, including
connection, admission
and execution; a shorter finite Original still wins. Cancellation, input limits,
model identity/weights and the Broker JWT validity (600 seconds) are unchanged.
There is no retry, reservation, polling or new scheduler, and no corpus-wide TTL;
the client still receives an explicit deadline.

The private operation gate and its timer use the same Tokio monotonic clock.
The connection and post-send cancellation/deadline regression pauses that clock
after real service admission, observes entry into connect or the first RPC byte,
then
cancels the Original or advances to the captured operation deadline. The
fixture requires no clock advance before that barrier and observes RPC EOF
afterward; pre-send expiry does not prove post-send cancellation. Tokio
`test-util` is a Daemon dev dependency, with no public clock configuration.

Rerank, refresh and authenticated shutdown retain their finite-original contract.
They call Core `SnapshotContext::required_deadline` before runtime-secret access
or polling any connection future. A service context with no deadline fails these
operations locally as Configuration with its actual `io::ErrorKind::InvalidInput`
cause; cancellation and expiration retain Cancelled and Deadline. The cab0
embedding contract supersedes only the earlier blanket embedding rejection of
service Originals recorded in bcf2/9d34; those historical qualifications and the
other finite-original boundaries remain intact.
The child launcher rejects an unbounded startup context before reading its
bootstrap secret or spawning a native process. The child server itself creates
its finite startup context from the explicit configured startup timeout and
reuses that deadline through readiness and installation.

The client polls tarpc dispatch alongside the RPC within the operation, without
a detached task. Embedding reuses its one effective deadline through connection,
dispatch, response validation and delivery; other calls use the original absolute
deadline. Tarpc serializes
remaining duration and reconstructs a remote deadline. Cancel/timeout drops its
stream and dispatch; no default ten-second deadline or retry replaces the
original. Responses return owned complete f32 vectors with dimensions or f64
scores in document order, plus actual admitted ConfigId/generation/model/revision
and `LocalDeviceRegistration`. Registration includes requested/effective provider
and index and a finite `AcceleratorUnavailable` reason when Auto selects CPU.
The removed ONNX execution metadata is not part of this contract. Registration
is not proof of all-node GPU execution. Explicit GPU requests cannot report CPU success;
a model/session/integrity failure never retries on CPU. The client checks limits, shape, finite values and expected metadata, then
rechecks Auth/possession and the operation deadline before returning. No partial
batch is exposed.

`ModelServiceClient::shutdown(runtime, &SnapshotContext, connect)` uses actual
parent possession and original context for the authenticated drain operation.
Producer errors retain their real source locally under static categories,
including binding interruption/deadline/capacity/selection and SDK failures.
Embedding wire errors use `ModelEmbedError { kind, limit }`: the existing category
and an optional closed `ModelLimitReason`. Other RPC errors still expose only
the typed category. Private paths, settings and input bodies are not serialized
as error diagnostics. Concrete channel/IO/codec errors
remain actual transport causes in the receiving process.

Runtime-key cancellation and expiration retain `Cancelled` and `Deadline`,
including checks made while reading the physical secret; other runtime-key
failures remain `Authentication` with their original typed cause. Local error
`Debug` includes only known runtime-key, service Auth/store/profile, deadline and
finite limit enum diagnostics. Arbitrary causes and Broker SDK details remain redacted; neither
`Display` nor the wire includes these local Auth diagnostics. A category-only
`Authentication` from the child does not identify a particular JWT, provider,
key, or source operation. Installed parent/child provenance and the observed
local cause must be checked before choosing a repair.

For the RPC client's operation-timer errors and received `Deadline` replies,
the existing local cause and `Debug` distinguish `DeadlineLocalOperation` from
`RemoteDeadlineCategory`. The
first means the client's operation deadline expired; the second means the child
returned the existing `Deadline` wire category. These finite labels do not
identify connection, queueing or native execution phases, and do not replace
existing typed Original, RuntimeKey or service Auth causes. The deadline category,
`Display`, deadline policy and Original identity remain unchanged. Arbitrary
causes remain redacted. An earlier category-only installed Source error cannot
be classified retroactively by this diagnostic change.

Embedding input and output bound failures are marked at the embedding callsites
as `LocalInputLimit` or `LocalOutputLimit`. The shared `RpcLimits` helpers and
other RPCs retain their behavior. When the child returns a classified limit,
the client retains `RemoteLimit(reason)` in the existing cause slot. The finite
reasons are `InputLimit`, `OutputLimit`, `NativeTokenLimit`, `ArtifactLimit`,
`NativeInputLimit` and `AllocationLimit`. Classification inspects only existing
`NativeEmbeddingErrorKind` and `NativeWorkerErrorKind::Native` variants; it does
not format their private causes or expose token counts, input, paths or secrets.
Actual native causes remain intact in the producer process.

An unclassified limit keeps `limit: None`; it does not imply a native phase.
Non-limit categories always use `None`, including the existing deadline and Auth
errors. The client rejects an incoherent non-limit error with a limit reason as
`Transport`. The public embedding wrapper still returns `ModelResult<EmbedOutput>`;
only its private embedding RPC error changes. The parent and sidecar must be
emitted together. These diagnostics do not increase limits, change batching,
retry, tokenize input, load a model or identify the historical installed limit.

## Qualification and limits of evidence

Two additional local error tests cover all runtime-key categories, preservation
of each original typed cause, safe known Auth/store/profile diagnostics and
redaction of arbitrary causes. Those tests serialize the existing category
itself; the embedding RPC now uses the finite record described above. These
tests do not identify the historical installed Source failure
or establish that replacing an older sidecar fixes it; that consumer remains a
separate integration check.

The additional deadline diagnostic unit test passed together with those two
existing error tests: three passed, zero failures or ignored tests. It checks
the two private cause constructors, finite `Debug`, preserved source downcasts,
unchanged wire/`Display`, and redaction of an arbitrary timeout canary. It does
not replay model inference or establish the cause of the installed Source
deadline; the next installed observation must distinguish the boundary.
Strict Clippy for the affected Daemon library and scoped formatting/diff checks
also passed. The focal build used the existing qualified Ladybug Release SDK;
it does not qualify another operating system or an installed Source journey.

The additional embedding-limit checks cover finite reason serialization,
local/remote origin, absent and incoherent reasons, preservation of the remote
deadline diagnostic, and secret-canary redaction. Direct and worker native
preflight errors are exercised without opening a model. The existing authenticated
IPC fixture also checks local input denial before connect and remote input
denial before model admission. These checks do not execute a model tokenizer,
qualify an installed inference or attribute an earlier `Limit` to a particular bound.

The separately selected `actual_gemma_service_rpc_single_query_768` test reuses
the existing Gemma bundle fixture, Auth producers, model service and TCP RPC.
It requires the pinned local BF16 file through `GRAPHIT_EMBEDDINGGEMMA2_MODEL`
and is ignored by default. One short query checks the actual model/revision,
768 finite values and normalized output. This isolated fixture does not touch
the installed Source, prove NN publication or exercise corpus-sized inputs.

On local Linux the five error unit tests and the extended IPC test passed
(0.00 s and 0.58 s respectively). The single-query Gemma2 test also passed with
revision `bfcd298762cc34d0357ece5ebdd31791a3a374d8` and 768 finite normalized
values. Its 28.33 s harness time includes fixture preparation, model load, RPC
and shutdown; it is not isolated encoder latency. The existing local model was
referenced through the fixture with a same-filesystem hard link, without a
download or another model asset copy. These results do not classify the earlier
installed `Limit`, Auth denial or operation deadline.

The cab0 per-RPC embedding change passed nine focused Linux integration cases:
the two `embedding_operation_` cases, real embedding/rerank/shutdown,
original interruption during connect/RPC, interruption before the first poll,
finite-only operation rejection, stale leases, revocation before delivery, and
active/disabled purpose and execution checks. The real embedding case uses a
service Original without a global deadline and confirms unchanged Original
identity after receiving actual vectors. Expiring one RPC leaves that service
Original valid for another RPC; cancellation and deadline close pending connect
and transport work with their typed causes. A later Original deadline cannot
extend the operation deadline, and a later operation deadline cannot extend the
Original deadline.

The first attempt to run the new cases failed during fixture setup because the
existing CPU ORT library was not supplied. Supplying it to the already built
test executable produced two passes; the other seven selected cases also passed
with zero failures or ignored tests. These are focused producer results, not a
new Native15, GGUF or complete Source qualification.

Current Cargo validation uses the shared target, `CARGO_INCREMENTAL=0`,
`--config profile.dev.debug=0 --config profile.test.debug=0
--config build.incremental=false`, `--locked --offline -j4`, and the existing
CPU ORT library for native execution. The service Original change introduces no
dependency or SDK pin change.

The affected Daemon library test target compiled successfully, including the two
converted `project_generations` fixtures; that compile-only check did not execute
the unit suite. Strict Clippy passed for the library and the `model_service`,
`model_child`, `memory_embeddings` and `user_memory_embeddings` integration
targets with warnings denied. Its first attempt found missing `child_api`
imports in the two Memory fixture consumers; both imports were corrected before
the passing run. Scoped formatting and diff whitespace checks passed. App
consumer compilation and the complete Source journey remain coordinator-owned
integration evidence.

Historical producer commands, run from the qualified workspace with an existing CPU ORT
library supplied by `GRAPHIT_TEST_ORT_LIBRARY`:

```
cargo test --locked --offline -p graphit-daemon --lib model_service:: -- --nocapture
cargo test --locked --offline -p graphit-daemon --test model_service -- --nocapture
cargo clippy --locked --offline -p graphit-daemon --lib --test model_service -- -D warnings
rustfmt --check --edition 2024 crates/graphit-daemon/src/model_service.rs crates/graphit-daemon/tests/model_service.rs
```

The producer was promoted into Main after its private qualification. Main passed
the same five unit and thirteen integration tests with zero failures or ignored
tests, using the existing Main target and the ORT library supplied only to each
native test invocation. Scoped formatting and strict Clippy for the library and
integration target passed with warnings denied.

The service retirement regression also admits B with its own cancellation
before cancelling and removing A's slot. B completes with its admitted
generation; a subsequent B request sees the new generation. These are service
borrowers. Two registered projects and their independent PROJECT authorization
remain part of the daemon/supervisor integration, not a claim of this unit test.

Linux CPU ORT 1.27.1/API27 tests use actual tiny ONNX/tokenizer fixtures,
Core captured bundles/listeners/private installation and public Auth producers.
Five unit cases cover equivalent lazy owners, config replacement, queued
cancel/deadline, actual native worker retirement/drain, remote EOF while queued,
and invalid first input rejected before lazy load. Thirteen integration cases
cover real embedding/rerank outputs, configuration identity, binding errors,
physical Auth/RuntimeKeyOwner and TCP RPC, malformed frames/credentials,
revocation before connect/while waiting/on actual response bytes before delivery,
shape/no-partial errors, and original cancel/deadline before the first poll and during connect/RPC wait.

The deterministic active-worker cancellation barrier occurs after an actual
ORT run but before terminal join. EOF is exercised while the handler waits for
owner access. These prove retained ownership and drain at those phases, not
cancellation during a long ORT Run. Arc/registry observations are lifecycle
checks, not RSS measurements. The dedicated Rust child bootstrap and joined
shutdown are qualified by tsk-933e. GGUF bootstrap and an explicit process restart
with changed configuration are qualified by tsk-79b9. Dynamic activation,
unexpected process death, backoff and parent liveness remain integration work
under tsk-0529. GPU/CoreML/AutoOOM, other operating systems and final product
acceptance require their own execution evidence.

## EmbeddingGemma2 integration status

The typed runtime/configuration, four-role RPC and native-worker dispatch were
qualified on Linux CPU by tsk-f1ba using the pinned BF16 GGUF. The real Auth/TCP
fixture covers four 768-dimensional roles, normalized output, a distinct MRL-128
owner, manifest integrity rejection before SDK startup, incompatible execution
before connection, stale lease rejection, renewed binding, retirement and native
shutdown. It passed in 86.20 seconds. Seven configuration/service unit tests,
thirteen RPC integration cases and four existing child cases also passed.
The synchronous SDK and worker were separately qualified by tsk-73e6. The
dedicated GGUF child passed its own real BF16 test under tsk-79b9: four roles at
768 and 128 dimensions, root-relative artifacts without ORT, explicit process
restart with changed configuration, normal shutdown and cancelled-caller
shutdown followed by the actual native join. That test passed in 81.41 seconds.
App provisioning and each corpus consumer have separate qualification; these
service tests do not establish the final daemon/CLI composition.

The App default producer halves Rust's `available_parallelism` thread count,
rounding up (minimum 1), propagates discovery and conversion errors and persists
queue capacity 8. Explicit runtime settings and an existing compatible runtime
selection retain precedence; changing this default does not rewrite persisted
thread counts. Its model default
is BF16/768 dimensions/max_tokens 8192; MRL selections are explicit. Runtime
settings are consumed as persisted, without rediscovering defaults. The Auth
lease must bind the selected provider/purpose and exact captured device/index.
CPU0 remains the profile default. GGUF supports CPU, CUDA, Metal and Auto;
CoreML and ONNX are not active local execution paths. Hardware qualification is separate from these
selection checks and from the historical CPU receipts.

## Memory consumer

`MemoryEmbeddingModel` recebe também a duração de RPC capturada pelo caller.
PROJECT e USER calculam o prazo de cada Document/Query no momento da chamada;
o client existente o combina com a Original finita quando houver. A indexação
Memory em background usa uma única Original de serviço cancelável, sem prazo
para o corpus inteiro. O default BG por RPC é 900 segundos (15 minutos), apenas
para `DistributionDefault`. Todo valor explícito de
`hub.limits.request.deadline_ms` com origem `Configuration`, menor ou maior,
é preservado. O mesmo prazo cobre espera e execução, limitado por uma Original
externa finita menor. Nenhum
novo TTL de corpus ou Original por lote é criado. Os consumers
HTTP/CLI mantêm a Original finita e a duração da política de requisição.

`MemoryEmbeddingModel` compõe este client/lease/runtime/ExpectedModel com os
produtores Records PROJECT Memory, sob o mesmo Original. Document recebe a
projeção JSON literal; Query recebe uma única string. A query decide vazio pelos
IDs já capturados (`MemoryEmbeddingCorpus::is_empty`); ausência de revisões missing
não impede RPC sobre um corpus fully covered. O getter é observacional, sem nova
autoridade ou varredura. Não cria outro worker ou issuer. Identidade, publicações independentes por lote, falha parcial e recuperação
são descritas no [contrato Memory](native_memory_embeddings.md#consumer-daemon-documentquery).
Sua integração BF16 usa RPC real e a tabela SDK e passou Main/Linux em um teste
de 102,79s, com scoped strict lib/test exit0. Qualificação do model service isolado
não substitui essa jornada nem entrega endpoints App.

Os métodos `index_user` e `query_user` do mesmo consumer integram Records USER
com a inferência existente. As admissões USER de leitura/escrita e o lease do
modelo são independentes; corpus ausente ou sem documentos não abre RPC. O
[contrato USER](native_user_memory_embeddings.md) descreve resultados reais,
publicações por lote, causa tipada, recuperação e a qualificação BF16 própria.


`query_hybrid` is a borrowed PROJECT Memory caller over the same model owners.
It prepares the bounded literal FTS value, drops preflight before one Query RPC,
and uses the [single-pin hybrid store](native_memory_search.md#busca-híbrida-project).
The shared private query-vector routine keeps the existing response identity,
dimension and one-vector checks; semantic query behavior remains unchanged.
NoCorpus/physical-empty do not poll connect, while fully-covered nonempty still
queries. The maintained tiny ONNX/CPU ORT1.27.1 +authenticated RPC +SDK case
passed in 6.399s; this is separate from the earlier BF16 receipt and is no new
quality, USER, shared-M/Main or MCP qualification.

## Source Memory consumer

`tsk-8140` expõe [Memory search no Source MCP](native_memory_search.md#source-mcp-project-memory-search)
sem adicionar M. Semantic/hybrid tomam emprestado `State.semantic` (client,
address, ExpectedModel e jobs) já usado pelo Source; FTS não consulta esse recurso.
O caller Runtime/local autenticado obtém ReadMemory PROJECT e, separadamente,
`capture_service(Embedding).local()` no mesmo Original finito. O mutex jobs é
cancelável por cancel/deadline e exclusivo ao período do produtor, sem index
aninhado. Depois do produtor, o mutex é liberado e o mesmo lease continua vivo
na serialização/bytebound; `delivery` revalida lease, ReadMemory e Original antes
e após essa fronteira. Não há fresh context, grant implícito ou promessa de
atomicidade Auth/transporte.

O Source usa seu JoinSet e spawn_blocking/Handle existentes; CancelOnDrop,
shutdown e deadline cancelam Original. O job fica retido até join, antes de
teardown de M/Auth. Módulo Memory/Embedding desabilitado retorna model_disabled;
M ausente retorna model_unavailable, sem RPC ou geração fabricada. Com M válido,
o preflight borrowed distingue NoCorpus/vazio e evita connect; fully-covered
nonempty executa Query real e o SDK captura seu pin final próprio. Falhas de
Auth/lease/SDK/modelo conservam sua classe e causa; nenhum fallback é ativado.

A qualificação causal usa HTTP `/mcp`, RMCP, Auth real Runtime/local, tiny ONNX
embedding_rank2, CPU ORT1.27.1, ModelChild autenticado e Native SDK. O teste de
cancel/join bloqueia o mutex real do M, expira/cancela a requisição e observa o
JoinSet Source vazio no shutdown antes de liberar o mutex. Os casos de resultado
observam os canais/proveniência e a recuperação de coverage com publicação real.
Essa prova funcional não acrescenta medição de qualidade, Gemma, USER ou Dream
à qualificação própria do serviço de modelo.

A [qualificação Source MCP](native_memory_search.md#qualificação-source-mcp) registra
os três casos distintos e suas tentativas; resultados do modelo foram
requalificados depois da correção do lifetime de lease no delivery.

## Portable vector identity update (154e)

The producer and consumers use the split described above. Deterministic affected
tests are `graphit-app --test integration -- ast_semantic::` (verified captures in distinct roots,
relocated library/scheduling, recipe/tokenizer changes and integrity rejection)
and Daemon `model_service::config::backend_identity_tests` plus the existing
`equivalent_owners_lazy_distinct_and_retired_arc_lifetime` test (same vectors,
distinct operational owners). Persisted schemas remain strict; there is no reader
for the previous field names. Memory PROJECT/USER and productive Gemma AST tests
now change a real token-limit recipe at unchanged width instead of invalidating
vectors for a threads-only change. Compilation and affected test results for
154e are tracked separately from earlier qualification receipts.

## Global refresh

`ModelServiceClient::refresh(runtime, original, expected_config_ids, connect)`
returns `ModelResult<RefreshOutput>`, whose public fields are `config_ids:
BTreeMap<String, ConfigId>` and `generation: u64`. This GLOBAL control operation
uses `RuntimeCredential`, like shutdown, with fresh runtime possession and the
original finite request deadline. It creates no PROJECT lease or snapshot.

The child captures its own global configuration using its installation root and
capture options, resolves only the required slot names and validates their exact
ConfigIds before calling existing `ModelService::install`, then acknowledges
that exact installed map and generation. Unrequested global slots are excluded.
Invalid configuration and a mismatched map leave the installed generation intact.
An equivalent map keeps the existing generation. Installation is lazy: successful
resolution/install does not claim that native loading or inference has executed.

An empty required map installs an empty catalog without parsing model settings
or reading bundles. Missing settings or selected slots with a nonempty required
map fail `ConfigChanged`; they do not acknowledge an unrelated or empty map.
The child control remains available for re-enablement. A parent publishes replacement model
metadata only after validating the ACK. Borrowers already admitted by ModelService
retain their real owner and generation; metadata captured before replacement may
receive `ConfigChanged` if its RPC arrives after the swap. Cancellation, deadlines
and actual authorization still govern those operations.

Blocking capture retains the original cancellation/deadline and an owned join;
RPC Drop cancels it without abandoning the join or releasing serialization before
the physical work terminates. Child shutdown joins these captures before draining
model owners and releasing native bootstrap. Filesystem calls remain cooperative,
not preemptible syscalls. App publication and the active two-project reload journey
are separate consumer qualification; historical service/restart tests do not prove
that journey.

The required map represents the owner's current admitted demands. It may retain
an independently admitted rerank slot even with `search.rerank=false`; the child
does not infer demand from that default or from provider configuration. Omitted
slots retire normally. The ACK is the exact required map and grants neither a
lease nor ExpectedModel authority. Invalid configuration, missing or changed
requirements preserve the valid installed generation.

The same serialization and retained blocking joins cover refresh and lazy
embedding/rerank admission, including dropped callers. The guard is acquired
before taking the installed-slot roster and remains held through the actual
capture/install terminal, so overlapping admissions preserve the union of
existing admitted names and the newly requested slot.

The 2026-10-24 demand-only change supersedes the earlier startup requirement to
resolve model configuration before readiness. With SDK4,
`cargo test --release --locked --offline -p graphit-daemon --lib --test model_child --no-run`
passed in 991.947 seconds. The `model_service::rpc::reload_tests::` focal passed
10 tests, with zero failures and one ignored test, in 0.234 seconds. The
ordinary `model_child` selection passed five tests, with zero failures and two
ignored tests, in 2.133 seconds.

The wider `model_service::` run without model environment variables recorded
34 passed, one failed and seven ignored tests in 1.533 seconds. Its sole failure
was the nonignored test outside the MODEL-DEMAND delta,
`project_generations::project_profile_change_keeps_borrowed_generation_and_sibling_independent`,
which requires a real GGUF at
`crates/graphit-daemon/tests/support/model_fixture.rs:111`. That failure
remains recorded. Repeating with `--skip` for only that case passed 34 tests,
with zero failures and seven ignored tests, in 1.059 seconds. This is an explicit
test selection, not a passing result for the complete suite.

The real GGUF case is now opt-in with `--ignored --exact` and
`GRAPHIT_EMBEDDINGGEMMA2_MODEL`. The default
`configuration_change_preserves_admitted_owner_and_unchanged_sibling` case
uses the existing metadata fixture and real ModelService admission to check
that a blocked operation keeps its owner across a configuration change, that
new admissions reject the stale config, and that the unchanged sibling keeps
its owner. It does not qualify native inference or the paired PROJECT RPC
journey.

The remaining case was then run individually with `--exact --nocapture` in the
same library ELF, without rebuilding. `GRAPHIT_EMBEDDINGGEMMA2_MODEL` and
`GRAPHIT_BGE_RERANKER_V2_M3_MODEL` selected the already provisioned pinned real
GGUF fixtures. It passed one test, with zero failures or ignored tests, in
30.664 seconds. This completes that case's actual qualification while preserving
the earlier missing-fixture failure and skip history; it is not a new monolithic
suite run. No other real-model replay was executed in this qualification step.

These results qualify the selected demand-only service and ordinary-child
boundaries and the named real-fixture project-generation case. They do not
establish broader AI or installed Source qualification; historical service,
process and inference receipts retain their original scope.

## Historical refresh qualification

The focused Linux producer run, `cargo test --locked --offline -j4 --config
profile.dev.debug=0 --config profile.test.debug=0 --config build.incremental=false
-p graphit-daemon --lib model_service::rpc::reload_tests`, passed both tests:
0.04 seconds in the harness, 13.54 seconds including Cargo. The existing qualified
CPU ORT library supplied the configuration path; this run performs no native
inference. It covers the public client and bounded codec with actual runtime
possession, invalid credentials, identity mismatch, invalid configuration preserving
the installed map, equivalent-map no-op, disable/re-enable and cancelled/expired
originals at preflight. It does not prove cancellation during an active capture or
the App two-project journey. Strict Clippy for `--lib --test model_service` passed
with warnings denied in 140.08 seconds; owned-source formatting, diff checks and
affected local documentation links also passed. The initial focal build failed on
two shared-helper imports before tests ran (265.27 seconds); those imports were
corrected before the passing run.


## Completion and idle residency

`ModelSettings::idle_timeout(snapshot)` reads the global
`models.idle_timeout_seconds` key as a positive integer, default 300 seconds.
`ResolvedConfig::with_idle_timeout(Duration)` validates and carries that policy;
`idle_timeout()` observes it. The policy is separate from slot ConfigId,
vector_id and generation. `install` updates the policy for an equivalent map
without replacing its owners or inventing a model-identity change.

The existing owned execution task now uses `embed_retained` or
`score_retained`. The outer RPC may stop waiting on its original cancellation
or deadline; its owned task retains the admission permit and operation guard
until the native command returns. Completion time therefore records actual
native completion, not a cancelled caller's early return. The existing worker
queue, limits, errors and inference deadline remain unchanged.

Each loaded Owner arms one Tokio timer in the existing retained worker registry.
A watch channel carries its active-operation guard count and last completion;
it is lifecycle state, not a new quota or scheduler. The timer also observes
policy changes. It holds only a Weak Owner while waiting, so retirement is not
extended by an idle sleeper. After the interval it acquires the existing native
mutex and rechecks activity, completion time and policy before taking the
client. In-flight work prevents unloading, independently for embedding and
reranking. A new request either wins that mutex and prevents teardown or waits
for cleanup and reloads on demand.

Cleanup uses `shutdown_retained` with the existing model-child shutdown policy,
then completes the final native proxy drop outside the executor. The mutex
stays held through actual join and resource release. Errors retain their typed
causes and the registry preserves failed cleanup for drain. On cleanup failure,
the owner resets its monitor admission while still holding that mutex. A later
successful load can arm a fresh timer without an old finalizer resetting it;
the earlier failure remains observable at drain. No thread, process
kill, polling loop, resource reservation or model cache framework is added.
Configuration metadata remains installed after unloading; the next load
revalidates the same bytes and digest and reports its actual registration.
Integrity, unavailable-device, Auth, cancellation and deadline failures remain
terminal and trigger no fallback.

The configured-model view is not a residency observation. No new status endpoint
is introduced; internal lifecycle tests inspect actual native ownership and
post-join events. Process/SDK bootstrap ownership remains with the existing
child until all operation, idle-cleanup and shutdown joins finish.

The real CPU two-model lifecycle focal passed in 167.87 seconds: independent
embedding/rerank unload, retained active work, reload with unchanged identities
and generation, and actual native join/drop. This duration includes fixture
capture and four loads; it is not an inference latency measurement. The same
two-model lifecycle focal passed on Linux CUDA0 in 209.09 seconds. The matching
CUDA plugin and driver libraries were observed; one-second GPU sampling reached
966 MiB. These observations do not establish all-node placement or zero driver
context memory after model release.

The cleanup-error regression passed with the real BF16 model in 59.21 seconds.
An already expired cleanup context still joins and drops the actual worker;
the same owner reloads and unloads twice, with unchanged identities/generation.
Its earlier Deadline remains observable in the retained shutdown result. The
expired context is injected only by that test; public shutdown configuration
continues to reject a zero timeout.
