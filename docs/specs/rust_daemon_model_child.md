---
title: Rust dedicated model child
description: Private finite credential bootstrap, real model RPC and owned process cleanup.
type: specification
updated: 2026-10-24
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-933e}
    relation: implements
  - target: {type: task, id: tsk-6ffa}
    relation: uses
  - target: {type: task, id: tsk-96ad}
    relation: uses
  - target: {type: task, id: tsk-b4e2}
    relation: uses
  - target: {type: task, id: tsk-0529}
    relation: consumer_boundary
  - target: {type: task, id: tsk-79b9}
    relation: implements
  - target: {type: task, id: tsk-f1ba}
    relation: uses
---

# Rust dedicated model child

`graphit_daemon::model_child` launches the private `graphit-model-server` binary
through the existing [Process service producer](rust_process_service.md). It
consumes the [shared model service](rust_model_service.md) and its qualified
transport. The ordinary Graphit daemon remains the sole RuntimeKeyOwner.

## Local purpose admission

The child resolves the canonical active Auth provider with AuthStore, using a
finite parsing/capture subcontext with the caller's same absolute deadline and
cancellation. It checks the outer Original before and after resolution. This is
a configuration observation, never caller authority. Startup requires an active
provider with local embedding or rerank capability; remote, disabled and NoActive
configurations cannot bootstrap a local child. It validates idle policy but
does not parse `ModelSettings`, capture manifests or read model bundles. The
initial metadata catalog is empty for both local purposes.

Actual finite authenticated embedding and rerank RPCs share `ensure_model(task)`
for an absent slot. A validated rerank RPC is explicit demand even when the
caller's global `search.rerank` default is false; that switch is caller policy,
not child preloading. The existing Global configuration guard serializes both
purposes and refresh. It is acquired before reading `ModelService::slot_names`
under the registry mutex, then preserves the union of installed admitted slot
names and the new name through fresh resolution and capture/install. The guard
and actual blocking join remain owned after request Drop. Exact existing slots
need no new capture; no parallel demand cache or new wire flag is introduced.
Runtime Global resolution selects only requested names and their task-specific
local executions, captures only those manifest FILEs and shares
the declarative model compiler with parent callers. Configuration and refresh
do not read or copy GGUF weights. First native load still verifies the real
artifact bytes, SHA256, size and supported model through the existing SDK;
configuration presence is not a weight-integrity or residency observation.
Wrong ConfigId/task, Auth, cancellation, deadline and model errors remain terminal.
The current canonical selection must still admit the local rerank purpose.
Installation reuses unchanged owners; a new rerank owner has no native context
until the existing rerank execution path loads the actual
[BGE v2-m3 F16 GGUF worker](rust_gguf_rerank.md). No new wire admission flag or
remote model descriptor is introduced.


Missing or malformed model settings, slot/manifest errors and wrong purpose or
ConfigId remain real errors on the first admitted RPC, rather than startup model
availability checks or synthetic success. Requested resolution validates the
complete structured `ModelSettings` schema before selecting names; only unused
bundle and manifest independence is guaranteed, not malformed-setting tolerance.

## Startup and credentials

The binary accepts one absolute installation-root positional argument. It uses
the fixed Graphit Brand and resolves that installation's actual `daemon.models`
configuration on admitted model demand; it reads no environment settings, discovery paths or legacy
configuration. Each slot declares `gguf-embeddinggemma2` or
`gguf-bge-reranker-v2-m3`, with threads and queue capacity. Hardware comes
from the actual installation AuthStore, separately for embedding and rerank.
The GGUF manifest supplies dimensions, token limits and explicit precision.
Relative bundle paths use the installation root as their base; absolute paths remain explicit, independent of the caller's current
directory.

The parent supplies exactly 64 lowercase-hex runtime-key bytes through finite,
zeroizing Process stdin followed by EOF. Truncated, excessive or malformed input
fails before configuration, binding or readiness. The child retains only its
zeroizing compute credential and creates no RuntimeKeyOwner, RoleIssuer or
project authorization store. Credentials appear in no argv, environment,
readiness data, files or diagnostic text.

`serve_child(root, ChildOptions, bootstrap, ready, lifetime)` receives actual
async IO and a service cancellation token. Its finite startup context covers
bootstrap, private installation qualification, active Auth/local capability,
idle policy, listener reservation and readiness. It is dropped after startup.
Model settings and bundle reads are deferred to admitted demand; subsequent
configuration and capture use the actual M/Core producers, and resolved owners
retain no startup reader or request context.

Core reserves the actual loopback listener. The same handle becomes nonblocking
and transfers to Tokio, without closing and rebinding the port. Only then does
the child emit a single JSON readiness line containing the selected address.
Startup Auth/idle/bootstrap/bind failure emits no ready endpoint. Missing or
malformed model settings do not block readiness and remain demand-time errors.
The binary reports
only IO/model failure categories on stderr, never the raw error chain. The launcher
retains stderr privately only before readiness, bounded by the supplied Process
stderr limit. On success it zeroizes and releases that buffer. After a valid readiness address has been accepted by the startup channel, the
same borrowed Process ServiceSink forwards stderr chunks to the parent's stderr,
without decoding, retaining or publishing them on stdout. The existing process
stderr-byte bound, callback cancellation and cleanup/drain remain in force;
write failure is a consumer failure. Invalid readiness or a closed startup receiver
never enables forwarding, even after its sender has been consumed. This makes the model service's finite
operation/phase deadline observation visible after readiness. Failed startup
preserves the actual Process status/cleanup and a private zeroizing diagnostic;
only a complete canonical line with recognized typed categories can enrich the
error display. Arbitrary, truncated or multiline stderr is never rendered. No
stdin, bootstrap credential, environment or raw stdout is added to diagnostics.
The library retains the actual error source.

## Ordinary options and request lifetime

Library callers explicitly supply `ChildOptions`. The binary uses
`ChildOptions::standard`: finite 30-second startup and cleanup windows, an
ephemeral loopback port, four accepted connections and one active native worker.
Wire and SDK bounds cover actual frame/input/vector/batch sizes; they add no
resource accounting or reservation framework. Capture policy inspects model
bundles without retaining the entire corpus in memory. These defaults describe
product behavior, not measured RSS or throughput.

Each accepted connection receives its own Cancellation and mature M transport
wrapped by `DisconnectTransport`; that exact token goes to `serve_connection`.
EOF, IO error and drop cancel only that connection. A JoinSet owns connection
tasks. Native worker/session ownership and terminal joins remain M's responsibility.
The client continues to use a short private connection and dispatcher per operation
and its original LocalServiceLease, runtime possession and expected model identity.

The binary uses a current-thread async runtime and retains a lightweight
`NativeSdkRoot` on its bootstrap thread until cleanup finishes, including for an
initially empty configuration. The root owns no models; M retains its existing
lazy native workers and model owners.

Startup installs an empty catalog with the validated idle policy. Authenticated
GLOBAL refresh resolves exactly the required map's names from the child's fresh
global source, validates each exact ConfigId and acknowledges that installed map
and actual generation, without unrelated global slots. `GlobalModelConfig`
retains the explicit installation root and capture/manifest options. Invalid
configuration or a missing/changed required slot preserves the valid generation.
An empty required map retires every slot without model settings or bundle reads;
the control endpoint remains available for re-enablement. An independently
admitted rerank slot can remain in a nonempty required map with the caller's
global default false. See the [shared service refresh contract](rust_model_service.md#global-refresh).

Foreground propagates that refresh even when the selected local embedding slot
has disappeared. It sends the actual resolved map, preserving any independently
admitted rerank slot, before clearing its published embedding resources. Clearing
only the parent selection would leave the child's old model map resident and
accepting fresh inference. Existing in-flight borrowers retain their captured
owners; retirement releases unused models without closing the control endpoint.
Restoring the same valid selection after an acknowledged empty map is a real map
transition and advances the service generation. Refreshing an unchanged installed
map remains a no-op; no counter reset or artificial generation bump is used.

Refresh capture uses `spawn_blocking` with the request cancellation token and
original deadline, allowing the async executor to process disconnect/cancellation.
The serialization guard stays in the blocking worker until capture/install ends,
including after RPC Drop. The child retains joins and observes worker failures;
completed captures do not accumulate.

## Parent ownership and shutdown

`ModelChild::start(ChildLaunch, runtime, original, host)` receives the literal
executable, explicit root, complete non-secret environment and Process limits.
The launcher passes only that environment and its existing BLAS startup value;
parent `TMPDIR`/`TEMP`/`TMP` settings are not implicitly inherited. Callers that
need a particular temporary filesystem must supply the applicable variables
explicitly. GGUF staging uses the child process's normal temporary-directory
selection and preserves actual filesystem failures without changing quotas or
retrying elsewhere.
It obtains a fresh runtime-key borrow under the same startup SnapshotContext and
calls `spawn_service`. An owned service runner consumes bounded stdout/stderr.
Readiness is parsed incrementally within a 512-byte line; no transcript, secret,
startup context or per-project model instance is retained.

The returned owner exposes `address()`. Cancelling the startup request after
handover does not terminate the service; the installed ProcessHost service signal
and the child-local ServiceStop control its lifetime. Drop requests local stop;
it does not claim a joined cleanup receipt.

The Source also awaits completion of the same existing runner while serving
HTTP. `ModelChild::completed()` is cancellation-safe and retains its actual
Process result; repeated observations do not join or reconstruct another child.
`join()` transfers that result without issuing an RPC to an exited process.
Unexpected completion removes published local semantic execution resources,
reports the actual status/reason/cleanup and reaps the native host. HTTP,
authentication and Task keep running. Remote embedding resources do not depend
on a rerank-only local child and remain published.
Likewise, a failed refresh RPC to that local control endpoint is reported without
retiring a Direct or Broker embedding binding; only a local embedding consumer
loses its execution resources through this local failure path.

The completion event never restarts the child, changes a device selection or
claims NN readiness. Once cleanup is confirmed, an ordinary local-model demand
or configuration refresh may acquire a fresh process; retained cleanup owners
continue to prevent acquisition while cleanup is incomplete. An embedding
configuration refresh republishes its real resources and schedules the existing
normal supervisor rescan. Previous borrowers still receive the actual failed
RPC; no success, vector or publication is synthesized. A process exit status
alone does not identify a model slot or prove a hardware OOM.

`ModelChild::shutdown(client, runtime, original)` first calls the actual
authenticated M shutdown RPC. M seals admission, cancels and joins native workers
before a successful response. The parent then requests Process stop and awaits
its owned runner, including when the RPC fails or expires. `ChildShutdown`
preserves the separate drain result and actual Process result, status, cleanup
and any remaining owner. Process stop may return a typed stopped/cancelled result;
it is not converted into an invented exit-zero observation.

The Linux systemd producer uses `KillMode=mixed` so the initial TERM reaches
only the parent daemon. Sending TERM to the entire cgroup would race this
ordered RPC with the child's signal-driven listener shutdown. The Source must
drain and reap the child before the MainPID returns: the manager sends its
final signal to remaining cgroup processes when that PID exits or when the
existing stop timeout is reached. Actual RPC/Process failures remain errors.
An installed service stop remains a separate consumer check after publishing
the generated unit.

On SIGTERM (Unix) or Ctrl-C, the binary cancels its service lifetime and awaits
the service. After joining connection tasks and outstanding refresh captures,
child cleanup starts M's owned drain
with its finite cleanup context. Even if that wait expires, the child awaits the
existing drain and every native join before releasing `NativeSdkRoot`. The timeout
remains the returned error; it cannot abandon native ownership. This final join
can outlast the cleanup window. Process termination grace still determines when
the parent may escalate to an OS kill, which cannot promise graceful native drain.

On readiness failure the launcher requests stop and waits for the runner before
returning `ChildStartError`. Its process field preserves actual cleanup and
remaining ownership rather than discarding an unfinished child.

## Qualification boundary

`crates/graphit-daemon/tests/model_child.rs` launches the actual Rust child
with the public Auth, RuntimeKeyOwner, Core and Process producers. Ordinary
cases cover finite invalid bootstrap, Auth/idle/bind failure, startup expiry,
cancellation-safe completion and repeated observation of the same real result.
The Unix nonzero-exit case checks the original exit status and cleanup without
model weights. Native inference cases additionally require the provisioned
BF16 embedding and F16 reranker assets selected through
`GRAPHIT_EMBEDDINGGEMMA2_MODEL` and `GRAPHIT_BGE_RERANKER_V2_M3_MODEL`.
The child has no test-environment model loader; fixtures install normal relative
bundles and use the production configuration.

The 2026-10-24 demand-only startup contract supersedes the old
missing-model-configuration-before-ready expectation. The SDK4 Release no-run
build of the Daemon library and `model_child` test passed in 991.947 seconds.
The ordinary-child selection then passed five tests, with zero failures and two
ignored tests, in 2.133 seconds; the reload focal passed 10 tests, with zero
failures and one ignored test, in 0.234 seconds. The wider service run first
failed one test outside the MODEL-DEMAND delta because its real GGUF fixture was
absent. That case subsequently passed individually with the provisioned pinned
weights in the same library ELF, without rebuilding: one passed, zero failed or
ignored, in 30.664 seconds. The original failure, explicit single-case skip and
complete counts remain in the [shared service qualification](rust_model_service.md#global-refresh).
Only that named case was replayed with real weights; the complete service suite
was not rerun as a single invocation.
These results do not establish broader AI execution or installed Source
qualification. Prior process and inference receipts remain historical evidence
with their original scope.

The Source consumer case in
`foreground_memory_jobs/tests/native_gguf_source.rs` observes a real compute
termination while HTTP/Task keep serving, checks local resource removal and
remote resource preservation, and exercises fresh acquisition through an
ordinary configuration change. It uses `GRAPHIT_TEST_MODEL_SERVER` to select
the actual compiled child. A shutdown RPC drains native workers; stopping the
ProcessHost lifetime supplies the separate process termination event. It does
not treat the RPC acknowledgment itself as proof of process exit.

Cancelled and expired shutdown callers must retain the native drain until its
actual join completes. Existing service tests cover queued-handler EOF and
native cancellation after dispatch. These tests do not establish cancellation
during a long native Run in the dedicated child, GPU behavior, other native
platforms or unmeasured performance characteristics. Production activation is
validated through the installed Source and its real consumers.


## Foreground Memory caller

`ForegroundSource::start_and_publish` resolves the global `daemon.models` embedding selection and the active local provider before starting a dedicated child. The executable is the actual `graphit-model-server`, resolved from the caller's captured PATH. A missing or disabled selection keeps Source available without compute; a malformed selected configuration remains an error.

`foreground_memory_jobs::start` calls `ensure_process` before constructing
resources. Its local metadata comes from the parent's validated `ExpectedModel`;
no metadata or wait-ready wire depends on a prepopulated child catalog. Actual
inference responses still validate expected identity, device and dimensions.

One child and RPC client serve the selected Memory projects. Expected metadata is immutable for each captured request and can be replaced globally after a validated refresh ACK. The project supervisors belong to ForegroundSource. Per-job runtime authentication, PROJECT Memory admissions and the embedding lease are fresh; the same request snapshot reaches the Records and model operations. Configuration/model identity checks remain in the existing SDK. Batches follow its text, UTF-8 and output-dimension limits. The owner retains no corpus, project grants or startup snapshot. A shared Tokio mutex serializes the Memory pipeline for this child's one native worker. Cancellation is observed while waiting; the job creates its request and admissions only after acquiring the mutex, and holds the guard through persistence. This avoids retaining batches from multiple projects while they wait for the same worker.

Foreground shutdown first joins the project supervisors, then retains and awaits the child/host cleanup owners before removing runtime authentication. Stopping a single project preserves the shared child for the others. Global model replacement uses the authenticated refresh and ACK boundary above. Source publishes validated replacement resources and schedules its existing supervisor rescan. Unexpected child completion retires local resources before a subsequent normal demand or configuration refresh can acquire a fresh owner; it does not restart the process automatically.


## Idle model residency

The child reads the global positive integer `models.idle_timeout_seconds`,
default 300 seconds, from its captured ConfigSnapshot. Invalid zero or an
unrepresentable duration fails Configuration before model artifact capture.
The selected policy is carried by `ResolvedConfig::with_idle_timeout`; slots,
ConfigIds and portable vector identities are unchanged. The existing global
refresh applies a policy-only change even when its required model map and
acknowledged generation remain equal. The App's existing global-config watcher
must propagate this key through that same refresh path.

Embedding and reranking owners count actual retained operations independently.
A completion-driven Tokio timer unloads a native worker only after the last
operation has completed and no operation remains active. It retains cleanup
through the real native join, using the child's existing shutdown timeout;
no new cleanup TTL or request deadline is introduced. Idle teardown releases
native model/context/staging ownership, while the process, private listener,
configuration and portable metadata remain available. The next admitted use
loads the same verified artifact on demand and returns its actual selected
device. Idle residency alone does not produce ConfigChanged or reindex vectors.

There is no new status RPC. CLI status describes configured selection and
verified artifacts on disk; it does not claim that a model is resident. Existing
inference responses report actual registration after loading. Remote requests
create no local owner or idle lifecycle. Disabled reranking still causes no
discovery, download, artifact capture, native load or rerank RPC.
