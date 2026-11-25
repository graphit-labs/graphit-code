---
title: "Rust daemon memory and lifecycle contract"
description: "Planned Rust daemon ownership, shared resources and incremental multi-project journeys."
content-type: reference
audience: developers
keywords: [daemon, rust, lifecycle, memory, watchers]
related:
  - "docs/specs/daemon_module.md"
  - "docs/guides/daemon_operations.md"
references:
  - target: {type: task, id: tsk-4d03}
    relation: relates_to
  - target: {type: task, id: tsk-4840}
    relation: derived_from
  - target: {type: session, id: ses-c98a}
    relation: relates_to
  - target: {type: task, id: tsk-ea45}
    relation: supports
  - target: {type: task, id: tsk-9c58}
    relation: relates_to
  - target: {type: task, id: tsk-1d4e}
    relation: implements
  - target: {type: task, id: tsk-658a}
    relation: supports
  - target: {type: task, id: tsk-26fe}
    relation: supports
  - target: {type: task, id: tsk-8853}
    relation: supports
  - target: {type: task, id: tsk-e90f}
    relation: implements
  - target: {type: task, id: tsk-e87c}
    relation: requires
  - target: {type: task, id: tsk-a5f4}
    relation: relates_to
  - target: {type: task, id: tsk-b577}
    relation: supports
  - target: {type: task, id: tsk-a90a}
    relation: supports
  - target: {type: task, id: tsk-9d92}
    relation: relates_to
  - target: {type: task, id: tsk-6d5a}
    relation: relates_to
  - target: {type: task, id: tsk-5b7b}
    relation: relates_to
  - target: {type: task, id: tsk-598c}
    relation: relates_to
  - target: {type: task, id: tsk-9969}
    relation: relates_to
  - target: {type: task, id: tsk-1da6}
    relation: relates_to
  - target: {type: task, id: tsk-3e9e}
    relation: relates_to
  - target: {type: task, id: tsk-3bae}
    relation: relates_to
  - target: {type: task, id: tsk-ac1e}
    relation: relates_to
  - target: {type: task, id: tsk-a59d}
    relation: relates_to
  - target: {type: task, id: tsk-613a}
    relation: relates_to
  - target: {type: task, id: tsk-dcee}
    relation: relates_to
  - target: {type: task, id: tsk-db94}
    relation: relates_to
  - target: {type: task, id: tsk-f2ba}
    relation: relates_to
  - target: {type: task, id: tsk-6aa2}
    relation: relates_to
  - target: {type: task, id: tsk-0529}
    relation: relates_to
  - target: {type: task, id: tsk-d751}
    relation: implements
  - target: {type: task, id: tsk-3b96}
    relation: relates_to
---

# Rust daemon memory and lifecycle contract

Status: proposed implementation contract under session ses-c98a revision575.
The active daemon remains the implementation documented in
[Daemon Module Specification](daemon_module.md) and
[Daemon Operations](../guides/daemon_operations.md). This page does not establish
Rust daemon delivery, platform qualification or measured memory savings.

## Identity and ownership

A supervisor belongs to a project ID, absolute checkout and captured version.
Two worktrees remain independent even when they refer to the same project/store.
A physical store or immutable compiled resource can be reused only when its
artifact, version, resolved configuration and generation are equivalent and its
SDK permits the requested use. Authorization remains request-scoped.

| Resource | Owner and reuse | Retirement |
|---|---|---|
| Watcher, directory ignore contexts, event batches, mutable project settings | Checkout supervisor; share observation only for the same physical checkout | Stop observation, discard removed directory contexts, cancel/join work and remove project state |
| Compiled query, grammar definitions and parser tables | Immutable generation with SDK lifetime; equivalent profiles may share an Arc | New requests select current generation; old borrowers retain old generation until completion |
| Store/backend and prepared statements | Physical artifact/version owner, subject to Data SDK concurrency rules; request authorization stays separate | Remove project/cache association; close statements/connection/database/canonical resource after last borrow |
| Model runtime and tokenizer | Global lazy service by model/task/manifest/tokenizer/provider configuration | Reuse across projects; replace on configuration change, close after active use during service replacement/shutdown |
| HTTP transport pool | Equivalent transport/origin configuration | Reuse client; headers, tokens and account/project permissions belong to each request |
| Parked project | Lightweight identity and activity state | No resident watcher, running job, loaded per-project store or cache; resume reconstructs resources lazily |

The existing global ANTLR ATN/LEXER_DFA tables remain shared. A daemon must not
instantiate a model per project or duplicate mature SDK pools. Sharing is not
permission to add unsafe Send/Sync around a native SDK.

ORT qualification recorded in Memory 01M4J5AMPCB5S54X91NTJH3PHN establishes that
SessionBuilder in the qualified rc.13 configuration must be constructed and
committed on its worker. Use the supported cancellation handle and join the
worker before close/reuse. Recheck ownership if that SDK configuration changes.

## Execution and cleanup

Filesystem notifications feed incremental batches. Coalesce repeated paths and
record changes arriving during a running job for its next batch. Use ordinary
bounded channels and concurrency for the work that actually conflicts.
Backpressure or a compact rescan request must recover overload without silently
losing changes or storing an unbounded path list. A rescan is recovery/initial
discovery; routine changes do not require complete project snapshots.

Embedding producers stream pending entries. Materialize source text only for
the current batch; retain only the metadata needed for ordering and deduplication.
Do not load every pending snippet before splitting into batches.

Handlers that read audit history, session records or a corpus should use
pagination, streaming or projections when the journey permits. Keep access to
all records without materializing every history on each ordinary request. H must
verify actual producer and consumer APIs; a long session_get response is an
observed payload concern, not an RSS measurement or a new quota.

Each call captures checkout, version, resolved configuration, authorization and
cancellation explicitly. Jobs consume actual domain services in-process where
appropriate; dedicated model/agent/native tools retain their qualified process
boundary. No CLI parser, CWD or process-wide environment emulates a service call.
Blocking native work uses the SDK's appropriate worker; async I/O stays on the
executor.

Removing or parking project A stops new work for A, cancels and joins its tasks,
and releases project-owned watchers, caches and logs. Project B continues.
A handler borrowing a shared resource may finish; it does not keep unrelated
project state resident. Replacement waits for the old owner where its native
resource requires that lifecycle. Shutdown joins active SDK/process work before
closing owners. A native call that cannot stop promptly remains an explicit
recovery condition, never a detached task still using closed resources.

There is no mandatory CPU/RAM/IO reservation, universal heavy-work permit,
quota ledger, RSS floor, p95 target or differential comparison. Input validation, access
checks, actual data integrity, cancellation/timeouts and cleanup remain.

## Independent deliveries and interfaces

All implementation leaves start OPEN and unclaimed. Root composes manifests,
Cargo.lock, global exports and entrypoints serially. A leaf must reconcile its
packet with actual prerequisite APIs before source edits.

| Alias | Independent result | Required interface/readiness | Scope boundary |
|---|---|---|---|
| W (tsk-8b30) | Watcher, checkout supervision, parking/resume and job callback | Completed Core/Project identity/version/config/ignore/log APIs | No AST/AI/DB/UI producer implementation |
| H (tsk-1d4e) | Actual producer API handoff for daemon consumers | Selected AST/Knowledge/Records/Dream/Data/Hub/Auth/model APIs, lifetimes and existing qualification | Refinement; missing producer remains a real dependency |
| M (tsk-b577) | Shared global lazy model service and private RPC | Qualified runtime 658a, clients 26fe, preflight e87c and producer e90f under contract 8853; App E consumes it | Reuse existing runtime/clients; dedicated process integration remains required |
| C (tsk-a5f4) | Project cache eviction and immutable-generation reuse | Handoff from the actual pipeline loader and its AST owner, then lifecycle in the productive job callback of J; Source readiness remains blocked | Reuse SDK owners and Supervisor lifecycle; no project_resources map or producer edits outside ownership |
| J (tsk-9d92) | Actual job dispatcher, coalescing and streaming embeddings | W/H/M plus specific producers selected in H | AST/Knowledge/Records/Dream producer changes require their owner |
| S (tsk-368b) | Per-user native services and daemon control | W plus existing singleton lock/process APIs | Linux/macOS/Windows adapters and control; Root owns CLI composition |
| U (tsk-4d03) | Current UI/MCP/proxy/tray coverage and concrete gaps | Reuse qualified hosting/BrowserAuth, Source/Role SDK callers and service adapter; Root must allocate missing OpenUI/liveness/tray producers | Contract refinement, not Source/UI/tray qualification; retain React/TypeScript |
| I (tsk-cab0) | Integrated multi-project product journeys | Delivered domain producers plus actual UI/MCP/tray consumers allocated by Root from U's handoff | Planning and adapter signatures do not satisfy product integration or FullRefs |

Full-domain completion is not an automatic barrier to W or an independent
adapter. Replace a consumer's broad dependency only when a specific producer's
interoperability is actually ready and recorded. The original 9c58 dependencies
00c1/3ba8/69f2/74bf/7858/8c6a/a4ca/e6ff remain recorded until that reconciliation;
the final integration still covers all required domains. A mock callback or a
readable signature cannot qualify an absent producer.

U's [current contract](rust_ui_mcp_tray_contract.md) reuses the finite Task hosting,
BrowserAuth and captured Source/Role SDK callers; it does not equate Now NDJSON
with Browser SSE or expose every Observatory route. Lazy OpenUI, native tray and
product liveness still need actual producer/owner handoffs under 9c58. The
inspected SourceServer composition declares no direct `/health` route; that is an unresolved
contract location, not an invented endpoint. Service adapter 368b is separate
from 0529's entrypoint/control/discovery integration, which excludes UI/tray.
Root must reconcile its packet before Source and confirm OpenUI/control/status
before tray implementation. Provider login reuses Auth; Start/Restart/OpenUI/Quit
preserve real status/error and stop/join. Quit failure keeps the menu operable;
missing desktop support does not block headless daemon or MCP. FullRefs 51f2/ef75
and UI Hub b788/d75f remain required. No new layout or global API rewrite is
authorized; planning alone does not make cab0 ready.

## Cache integration readiness (2026-10-12)

The current `tsk-a5f4` packet separates the available owners from their pending
integration. `ProjectSupervisor::start` already accepts the producer callback
`FnMut(ProjectContext, ChangeBatch, Cancellation) -> Fut`. Its loop awaits the
producer; stop and park cancel and join work, and parking retains the project
context. A productive callback can own its generation without a new cache map
in the Supervisor.

The shared model service delivered by `tsk-b577` already provides generation
reuse, reload and retirement. Reuse that service. Tree-sitter `GrammarLanguage`
already retains an `Arc` of its language owner; parsed trees and compiled queries
retain the exact language generation. A reload must select the new generation
for new calls while admitted borrowers retain their old owner. Similar artifact
names do not make two independently loaded language owners interchangeable.

The reviewed daemon Source has no grammar/query acquisition callback or
`project_resources` module. The remaining acquisition and reuse work belongs in
the actual pipeline loader that prepares
[`PreparedTreeSitterContexts`](../../crates/graphit-ast/src/tree_sitter_context.rs)
with its grammar/query owner and literal captured configuration. The current
[`ParsedTreeSitterDocument::collect` and `assemble`](../../crates/graphit-ast/src/tree_sitter_model.rs)
produce the borrowed `TreeSitterSemanticDocument` consumed by
[`encode_native_nn_ts`](../../crates/graphit-ast/src/native_nn.rs). The
`borrowed_source` input boundary must retain the actual Source/Original owner;
it is not a replacement cache or a new extractor facade. Root's `tsk-4840`
retires the dedicated extractor/encoder; its remaining fixture transfer and
qualification are still in progress. This contract does not establish a
productive NN loader callback, global cache reuse or generation retargeting.
The actual loader interface, paths and integrator ownership remain pending
handoff with the AST owner. Natural `Arc`/`Drop` alone does not deliver that
integration.

Resource lifecycle must then be linked to the productive callback of `tsk-9d92`
using the Supervisor's existing cancel/join behavior. The loader handoff and that
callback must identify their real inputs, outputs and generation owner. The
Supervisor callback signature and completed prerequisite Tasks alone do not
qualify the integration. A new `project_resources` map or a generic collection
of `Arc`s tested without these consumers would leave the behavior unproved.
The existing natural ownership does not establish that this refinement removed
a cache from Source.

Captured Source and Graph readers retain their operation's Original and snapshot; that does not authorize retaining request authentication in a global cache. Cache product checks cover equivalent/distinct owners, reload with an admitted borrower, retirement of A while B continues, and lazy reacquire after the final SDK owner is released. Those functional fixtures do not by themselves establish full Source integration, SDK close integration or measured memory savings.

## Selected producer interfaces (H)

This is a planning handoff from tsk-1d4e under session ses-c98a575. It records
actual selected sources and returned producer Tasks, without qualifying absent
jobs or the complete daemon. Task states below were read during this handoff;
executors must obtain fresh readiness before changing dependent source.

| Capability and consumer | Actual interface and lifetime | Qualification and remaining producer |
|---|---|---|
| Watch/supervision to J/S/C | ProjectSupervisor::start captures ProjectContext and accepts FnMut(ProjectContext, ChangeBatch, Cancellation) producer; stop/park awaits its future. ProjectSupervisors reconciles separate checkout/version keys. | W tsk-8b30 COMPLETED16: 23 native/functional tests, fmt/Clippy. Callback is the handoff; AST/AI/store integration is still pending. |
| Local inference to M/J | LocalModelRuntime::load(Arc<ModelManifest>, &Path, LocalRuntimeOptions, InferenceContext) creates an owner; run(&mut self, LocalInput, context) returns owned TensorOutput. Runtime retains tokenizer/Session and the verified temporary bundle until native close. | tsk-658a COMPLETED13: 11 ONNX CPU SDK tests; tsk-9bc3 COMPLETED121 is the factory refinement, not a shipped registry or IPC service. GPU/CoreML/native other OS and later integration remain distinct. |
| Local embedding/reranking to M/J | LocalEmbeddingClient::new owns runtime; embed(&mut self, &[&str], Query/Document, context) returns EmbeddingBatch. LocalRerankClient::score(&mut self, query, documents, context) returns scores in document order. Clients reuse their owned runtime and materialize prefixes per chunk. | tsk-26fe COMPLETED11: eight real Linux CPU client tests on Main. Preflight e87c reuses the same request validators before lazy loading. The model service e90f resolves explicit captured bundle/provider configuration and owns generations; active product routing remains an App E consumer. Do not invent an authority from a manifest. |
| Request context to M/J | InferenceContext borrows the original Cancellation and copies the absolute Instant. LocalRuntimeOptions explicitly carries native library/provider/thread/padding configuration. SDK construction must remain on the owning worker where required. | Mature SDK owner and join are required; no unsafe Send/Sync or renewed deadline. Keep the service lifetime independent of the first short request and each request's cancellation independent of other projects. |
| HTTP transport to M/AI/Hub | BoundedClient::new(ClientPolicy, Limits) owns one reqwest pool. execute(&self, NetworkRequest borrowing headers/body, NetworkRequestContext borrowing Cancellation) is per operation. execute_guarded borrows a synchronous gate only for that future and preserves typed cause plus NotSent/MayHaveApplied. | tsk-72bc COMPLETED15; tsk-3350 IN_PROGRESS8, corrected Source qualified with 23 TCP tests. Gate is not AI selection, local/Broker/refresh/completion integration. Reuse transport by policy/origin without retaining Auth or request context. |
| Record authorization to J/C/handlers | ProjectRecordAuthority::new(&RoleIssuer) is a pure selector; admit_read/admit_write borrow actual grant and original RequestContext. Admission recheck/project/request preserves issuer/project/version; write actor and obligation are actual checks. | tsk-1f4a COMPLETED15: qualified PROJECT-only library; global USER Memory jobs need their own actual admission/read/write producer. OwnTask maps WriteOwnTask, Task maps WriteTask, CoordinateTask maps CoordinateTask. USER, store capture, CAS and session/runtime behavior are separate producers. No new grants or controllers. |
| Records/Memory/Task/Dream to J | NativeMemoryStore and NativeTaskStore use real captured PROJECT admissions, SDK reads/writes, CAS and retained confirmed effects. MemoryEmbeddingModel::index consumes the document cursor and publishes independent attachments. Dream Application::apply retains its actual receipts and reobserves uncertain operations. | a59d, 613a and dcee are delivered producer refinements. The Foreground caller remains required: 6d5a composes the shared model and initial Memory execution; 5b7b bounds RPC subrequests. Dream scheduling/application belongs to db94/f2ba/6aa2. There is no separate Dream ledger to implement. |
| AST index/query/source/search to J/C | Selected graphit-ast lib exports grammar/catalog/profile/prepared-input/range/parser modules. a90a distinguishes actual source capture from indexed/imported source; proposed IndexCoordinator/AstQueryService are not shipped APIs. | a90a OPEN7 is the exact interface refinement. Data writer/query/indexed-source and AI/search integrations need real producer handoffs. Do not build a CLI-parser or callback mock in the daemon as their replacement. |
| Knowledge to J/C | ReferenceGraph<'context,'data>::build borrows SnapshotContext and complete page reference corpus, producing indexes of titles/edges. Result queries borrow the graph/corpus and check the same context. Other exports cover compilation/previews/settings/staleness. | 84d5 OPEN56 owns storage/publication/inference bridge gaps. The existing request-bound graph cannot become a global resource merely by caching its context. Whole reference analysis legitimately needs complete reference metadata; ordinary page/history responses should use projections/pages. |
| Icebug graph reads to handlers/C | LocalGraph::capture owns a CapturedSourceReader plus Arc<SnapshotContext>. sample_graph returns borrowed result views; neighborhood_graph has a generation/query-bound cursor, has_more and next_cursor borrowing the retained page. | Actual graphit-data Source is sampling/neighborhood, not Cypher execution or Records writer. Data/Core must own captured/backend integration. Preserve cursor generation and authorization; do not cache a first request's context as a global DB owner. |
| Hub selection/hydration to J/M | select_agent(ConfigSnapshot, explicit selector, AgentMembership) chooses a NativeBinding. Public modules include registry/materialization/publication. Agent selection does not select a model bundle or prove a hydration job. | d1ac L5 remains model-selection refinement; actual hydrate/fetch/launch producer requires its own qualified interface. Data-only IDs/fingerprints do not authorize SDK reopen. |

Errors remain the existing typed producer results: LocalRuntimeError retains
SDK/IO cause under a static category; LocalModelError retains runtime/context/
output/selection causes and discards partial batches. HTTP retains NetworkError
and typed gate cause plus possible-send outcome. Auth returns AuthError; Data
returns DataReadError; Knowledge returns ReferenceError while borrowing the
same context/corpus. Native local clients require mutable exclusive access,
HTTP uses shared async access, and record admission stays borrowed per request.
These lifetimes are not changed to static or erased for a daemon cache.

### Consumer corrections and readiness

M now names the qualified runtime 658a, clients 26fe and request-preflight
producer e87c rather than requiring broad 010b/d1ac closure for the CPU interface.
The [model service](rust_model_service.md), producer e90f under contract 8853,
resolves explicit `daemon.models` configuration using actual captured bundles.
Its constructor captures the same canonical bundle used for manifest binding;
a fingerprint is never authority. Validated data-only ConfigId and generation
bind admission to the same lazy owner before queuing or loading. SDK request
preflight precedes the first native load. Real caller LocalServiceLease and
RuntimeKeyOwner possession are rechecked through queue/send/wait/delivery.
Short private tarpc connections retain native workers until terminal plus join;
authenticated shutdown seals admission and drains them before replying.

App E must consume this producer in the dedicated embedding/rerank child,
including finite private stdin credential bootstrap, parent liveness, lazy
activation, backoff after exit, short valid socket paths and required-mode
failure while down. M's library/RPC qualification does not establish those
process journeys, product active routing or final 0529 integration. GPU,
CoreML, AutoOOM and native other-OS behavior remain their owning AI leaves.

C/J consume the qualified producer interfaces for each capability. Records and
Dream library producers are delivered; their Foreground jobs remain distinct
integration work. The shared model/initial Memory composition is 6d5a under
9d92, using ModelChild and the current client limits getter. AST now supplies
prepared extraction (9969), physical Source (3e9e), source association (3bae)
and model metadata (1da6); its full indexer/locator/caller remains required.
Knowledge supplies first publication, republication and explicit branch
materialization; ac1e replaces retention of all document bodies with batches.
These producer qualifications do not by themselves execute the daemon jobs.
0529 is the final control/entrypoint integration and depends on 9d92; it is not
a model factory prerequisite for those jobs. Full domain outcomes remain in 9c58/I. Existing Hub daemon packet4323 also
contains captured jobs/session/drain requirements overlapping J. Reconcile its
legitimate coverage with J before implementation and supersede its old universal
heavy-slot wording rather than creating a second daemon job controller.

The daemon composition must not reintroduce resource accounting removed from
its producers. Keep input/output validation and the actual ownership,
authorization, integrity and cancellation checks consumed by these jobs. Core ProcessHost may have one service owner/runtime while operations
keep their original contexts; native producer cleanup remains the consumer's
responsibility.

### Pages, batches and retirement fixtures

Ordinary MCP/HTTP reads must select requested fields and bounded pages rather
than return every audit/session record. A user can continue through all pages;
a cursor keeps the selected generation/filter/version and does not grant access.
Authorization/cancellation is checked on every producer page. Changing the
selected generation returns the producer's stale outcome rather than mixing
old/new results. The selected neighborhood API demonstrates these semantics,
but does not supply a Records/session pagination API.

Embedding jobs obtain pending metadata incrementally, fetch source only for the
current batch, preserve ordering/dedup required by the actual producer and drop
that batch before advancing. Existing client chunking starts from a supplied
slice and alone does not prove that the pending corpus is streamed.

Affected future fixtures: two projects share the same lazy qualified model and
transport, a different configuration uses another owner; invalidation during a
borrow leaves the old generation alive until it finishes; retiring A releases
its queue/cache/handles while B completes; last consumer cleanup closes the
actual native owner; multiple source batches produce the expected outputs;
paged session/audit reads cover all matching rows without including unrelated
fields or projects. Tests must exercise real owners/producers, not counters for
invented reservations. No RSS gain, Linux cache saving, GPU or native other-OS
qualification is inferred from this map.

## Incremental fixtures

These are future Rust checks, not results already executed.

| Case | Action and observable expected result | Owners |
|---|---|---|
| F1 | Two worktrees/versions have separate supervisors; branch changes do not alter a running job's captured version | W/J/I |
| F2 | Create directory, edit ignored/non-ignored files, rename/delete; expected incremental events and activity appear, removed contexts disappear | W (tsk-8b30) |
| F3 | Burst while job runs; coalesced next batch includes new changes; bounded overflow recovers by rescan | W/J |
| F4 | Remove/park A while B runs; A closes project resources, B completes; resuming A reacquires fresh resources | W/C/J/I |
| F5 | Same artifact/profile generation has shared immutable owner; different config/version does not mix state or authorization | C/M |
| F6 | Reload A's query profile during a borrow; new requests use new generation, old request finishes, unrelated B profile remains valid | C |
| F7 | Retire last consumer with handler active; SDK closes after borrow ends and reacquisition creates a fresh owner | C |
| F8 | Multiple embedding batches produce required vectors/order/dedup while source snippets are materialized only per batch | J |
| F9 | Equivalent projects reuse lazy model service; config change/cancellation/restart/shutdown preserve correct outputs and owner lifetime | M |
| F10 | Native service double-start/stale PID/stop/restart/replacement; singleton and endpoint publication are correct | S |
| F11 | Linux manager unavailable versus available-but-failed; only the documented unavailable case uses detached fallback | S |
| F12 | UI initially inactive; OpenUI activates one server and publishes actual bound port; access/session/SSE/WS isolation holds | Existing producers plus actual remaining UI consumer integration/I; Source allocation by Root |
| F13 | Proxy reconnect preserves explicit/captured version; denied Auth never reads or mutates another project | H/J and existing captured-selection SDK producers plus actual remaining consumer integration/I; Source allocation by Root |
| F14 | Native tray login/Start/Restart/OpenUI/Quit/error and no-desktop cases; failure retains operable recovery | Actual tray consumer integration with service/OpenUI producers/I; Source allocation by Root |

Preserve functional coverage for version identity, reconciliation, activity, watcher cleanup, query reload, shared-resource retirement, streaming embeddings and model-service lifecycle. Remove harness-only tests with their eliminated mechanism while retaining real lifecycle/error/permission coverage. Run affected Rust suites and fmt/Clippy per delivery. Native platform execution remains necessary where behavior depends on the operating system; cross-compilation alone does not establish runtime behavior.

## Source and documentation evidence

The scout inspected these source contracts; they describe current implementation
and the specific opportunities above, not an RSS measurement.

| Ownership boundary | Required behavior |
|---|---|
| Supervisor | Distinguish checkout/version identity, supervise or park projects, and join retiring work. |
| Store | Keep logical project/store identity distinct from checkout identity and physical versioned artifacts. |
| Watcher | Release per-directory mutable context after deletion or rename. |
| Parser/query resources | Share immutable generations where safe and retire old query generations after borrowers release them. |
| Native SDK | Share backends only under their actual borrow/close contract. |
| Embeddings | Stream pending source snippets into batches rather than retaining the complete pending corpus. |
| HTTP | Reuse the mature client pool without sharing request authorization. |

W is delivered as the library in rust_daemon_watchers.md: notify-debouncer-full
0.7.0/notify8.2.0 with the official NoCache and joining stop outside the executor.
Its 23 tests qualify events and own lifecycle. Linux already used NoCache by
default; no Linux memory gain is asserted. Native Windows/macOS and actual
domain integration remain pending.

SDK references: [Tokio bounded mpsc](https://docs.rs/tokio/latest/tokio/sync/mpsc/)
provides backpressure and receiver shutdown; the
[notify debouncer](https://github.com/notify-rs/notify/blob/main/notify-debouncer-full/src/lib.rs)
owns its watcher/thread and joining stop; [tray-icon](https://docs.rs/tray-icon/latest/tray_icon/)
documents native event-loop constraints. These are options for the implementation
owner to pin and qualify, not dependency changes already applied.

## Reconciliation of blocked daemon task

Apply the full replacement packet saved in tsk-ea45 before implementing9c58
when its actual prerequisites permit claim. Never claim the blocked task merely
to edit it. Preserve history with supersede, rather than passing obsolete checks.

Functional checks cover incremental multi-project behavior, native service journeys, resource ownership and retirement/reuse, failure recovery, concurrency, permission boundaries and documentation consistency. Mechanism-only harnesses are retired with their removed mechanism; active behavior keeps its regression coverage.



A biblioteca de watchers/supervisores W está descrita em [supervisão de checkouts Rust](rust_daemon_watchers.md). Ela entrega lifecycle/eventos próprios; a composição do daemon completo e as folhas de produtores continuam pendentes.

## Foreground Memory composition

The Foreground Memory caller uses one dedicated local model process and the existing checkout supervisor registry. It reads model selection from global `daemon.models`, derives the expected model from the active provider, and shares only immutable model/client/address resources. The project paths, identities and versions come from the same bootstrap collection used by Source, Task and Now. Watcher ignore configuration remains specific to each checkout.

Each initial or explicit rescan obtains a fresh finite request, runtime authentication, PROJECT Memory read/write admissions and a local embedding service lease. Records streams the corpus in batches; SDK partitioning applies actual RPC text, byte and output-value limits. Model input text is moved through the batch consumer. Projects do not share an application mutex around capture, indexing or RPC. Original/cancellation checks, the native SDK owner's mutex and each project's ArtifactWriter/CAS protect their actual resources. The callback does not retain corpus bodies or authorization between jobs and does not index Memory for unrelated incremental filesystem paths.

Retirement cancels and joins the selected supervisor while sibling projects continue using the child. Browser USER Memory and imported Knowledge are independent consumers without a PROJECT; Source retains their model selection and GLOBAL control observation while that real owner exists. Whole-service shutdown joins supervisors before model drain/process/host joins and runtime credential cleanup. Interrupted cleanup preserves the existing owners for explicit continuation. Absent or empty model settings clear configured model owners after ACK; a retained consumer can keep lightweight control for re-enabling. Without a consumer, cleanup joins that child.

The `tsk-6d5a` fixtures qualify this caller independently of the underlying model and supervisor SDKs; eight focused Linux cases passed with the actual tiny CPU ONNX child and persisted Memory data. The shared-child two-project journey also passed after the common registry gained its Dream activity sender. The qualified default EmbeddingGemma2 producer remains separate; this fixture does not claim another default-model or platform qualification. AST, Knowledge and Dream jobs and model replacement remain necessary parts of their owning integration leaves. Role/Source wake ingress after Memory CRUD is qualified in Linux in `tsk-d751`; Main attachment of the reconciled handles is implemented by `tsk-6d5a`; the CRUD→model/persistence and covered-no-RPC proof passed in two Linux functions in `tsk-3b96`, with scoped Clippy passed and this leaf integration qualified in Linux. This initial-rescan composition does not close the complete daemon migration.

### Role/Source wake ingress

The Linux crate-local `SourceServer::attach_memory_rescans(&self, ...)` forwards the owner's current `Arc<[ProjectRescanHandle]>` to its attached Role handler, including after handlers are cloned or served. Empty projections are valid; absent Role tools return an IO error. Role replaces its projection through an RwLock and an admitted write clones the Arc in a short scope before any store await. It selects a handle by project ID, checkout root and captured version after Role grant validation; the lock never crosses the store await. Poisoned projection state fails before the write. This be6e update remains under runtime qualification; earlier ingress receipts preserve their original scope.

An awaited admitted Memory write returning `Ok` or `MemoryStoreError::Confirmed` sends the hint before JSON mapping and outer late-delivery checks. Idempotent creation returning `Ok` may also signal; reads, `NoChange`, denied or malformed requests and pre-effect errors do not. A stale or absent handle changes neither the CRUD outcome nor its cause/version. The hint does not authorize or run model work: each supervisor callback obtains fresh finite request/admissions. The full API and Linux qualification of six distinct functions, App compilation and scoped strict Clippy are documented in [Role/Source Memory wake](rust_daemon_watchers.md#ingresso-rolesource-de-wake-memory). Earlier catalogue-oracle and external IgnorePolicy failures remain in the evidence history; production ingress was unchanged by the test-only oracle correction.

The [Foreground Source](../../crates/graphit-app/src/foreground_source.rs) already projects the actual Memory run-owned handles after registry reconciliation and publishes the current projection to the shared handler. This Main attachment is implemented by `tsk-6d5a`, only when Memory resources are present; otherwise it attaches an empty projection. Dream-only selections are excluded. The CRUD→model→persistence and covered-no-RPC proof passed in two Linux functions in `tsk-3b96`, with scoped Clippy passed and this leaf integration qualified in Linux; Dream receipt wake remains separate integration work. The six ingress cases do not establish those journeys.

<a id="role-httpmemory-integration-under-qualification"></a>

#### Role HTTP→Memory integration qualified in Linux

The implemented [Role rescan fixture](../../crates/graphit-app/src/foreground_memory_jobs/role_rescan_tests.rs) uses actual Role HTTP served by Source/Foreground and the real callback resources. A test-only Linux `Weak` seam in [ForegroundSource](../../crates/graphit-app/src/foreground_source.rs) upgrades the current resources; it changes neither production API nor ownership. The fixture no longer pauses an application-wide unit mutex. [MemoryJobResources](../../crates/graphit-app/src/foreground_memory_jobs.rs) obtains fresh request/admissions for each job. The independent-project contention focal holds A's actual ArtifactWriter, requires native indexing in B to complete, then releases A and joins both jobs; its result is recorded separately from the historical Role qualification.

The fixture uses the actual model-server with tiny `embedding_rank2` CPU ONNX, four dimensions and ORT 1.27.1. Two-project HTTP mutations, persisted attachments and independent parking are implemented test journeys. Fully covered input goes through the real index using a bound TCP endpoint without a responder: the oracle requires `Complete { last_attachment: None, .. }` and no accepted connection. It does not call `Model.query` before its corrective producer completes; it establishes no GGUF or EmbeddingGemma2 qualification.

The current Source03 passed both Linux runtime functions in native-tests02: actual Role HTTP mutations through the shared Main model on two projects with independent parking, and valid CRUD without a model owner. The first case now also verifies an actual Role `get` returning `absent` after delete. Compile03, fmt01, diff01 and strict01 passed. The actual command `cargo clippy --locked --offline -j1 -p graphit-app --lib --no-deps -- -D warnings` exited with code 0, completing Linux qualification of this leaf integration within the stated scope. Compile01 failed on three fixture errors corrected in Source02. Native01 retains its positive-case failure before inference because `GRAPHIT_TEST_ORT_LIBRARY` was omitted, while the no-model case passed. The current two-case run uses actual model-server, qualified CPU ORT 1.27.1 and tiny four-dimensional `embedding_rank2`; it does not qualify GGUF, EmbeddingGemma2 or `Model.query`. The six `tsk-d751` ingress cases and eight `tsk-6d5a` caller cases retain separate scopes/counts. Other platforms and complete Dream/daemon journeys remain separate.
