---
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-678a}
    relation: implements
  - target: {type: task, id: tsk-ea469}
    relation: implements
  - target: {type: task, id: tsk-59c7}
    relation: implements
  - target: {type: task, id: tsk-cd41}
    relation: uses
  - target: {type: task, id: tsk-6cf9}
    relation: requires_contract
  - target: {type: task, id: tsk-1962}
    relation: implements
  - target: {type: task, id: tsk-c182}
    relation: extends
  - target: {type: task, id: tsk-3e62}
    relation: specified_by
  - target: {type: task, id: tsk-add4}
    relation: requires_contract
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-13af}
    relation: implements
---

# Native Task protocol

The Records Task protocol defines owned Rust wire values for authenticated hosts. It dispatches directly into NativeTaskStore with the same original SnapshotContext Arc, admission and operator-selected read/write policy. It creates no issuer, grant, scope, corpus adapter or request context. The host authenticates each current request and selects retained physical project/version captures before using the dispatcher.

Requests use an operation discriminator and, when needed, an input object. The current native operations are explicit: Task/Session creation and reads; finite projected query and public schema; catalogue, complete read view and Session heads; Task/Session claim, heartbeat, release; Task progress and Session checkpoint; Task and Session specification revision, check verification/supersession, links, flags, completion/cancellation and Session completion. AddTaskComment typed dispatch (`tsk-1962`) is Linux-qualified with affected functional tests, Records docs and scoped strict passed; lifecycle completion is not inferred. HTTP, MCP, CLI and browser authentication are separate consumers. Operations not delivered by NativeTaskStore are not advertised here.

[Native Task FTS](native_task_search.md) follows the closed `tsk-3e62` contract.
Projection/index publisher `tsk-dd92` is implemented and Linux-qualified:
35 distinct functional tests, six Records doctests separately and scoped strict/fmt0;
reader/cursor `tsk-add4` is Linux-qualified with 45 distinct functional tests,
six Records doctests separately and scoped strict/fmt0. The Linux protocol now
exposes `ReadRequest::Search(TaskLexicalQuery)` and `ReadResponse::Search`.
`execute_read` receives the host's individual `max_query_bytes` alongside native
read limits; Source/HTTP use the captured content policy, never a client bound.
Native producer qualification does not qualify App, HTTP, CLI or MCP consumers;
their current integration evidence belongs to `tsk-59c7`.

## Data and presence

Inputs reject unknown fields and use the existing canonical ULID identifier serializers, strict civil-calendar schedule and immutable native result models. Actors, timestamps, newly generated check IDs, scope, physical URI, branch and admission classes are never input fields. Verification and supersession inputs select existing canonical check IDs. Catalogue results carry actual schedule and dependency IDs from the same selected SDK pin.

Revision fields use the existing FieldUpdate contract: omission keeps a value; present values replace it; a null schedule or parent clears it. An empty or invalid schedule remains an error. Calendar-only changes preserve verification evidence under the native specification planner. Lease values are positive integer milliseconds; publication checks enforce actual live ownership, expected revision and expiry.

The request classifier selects broad Task authority for creation and unclaimed open cancellation, CoordinateTask for Session lifecycle, and OwnTask for participant mutations. This classification is data for the authenticating host, not authority. NativeTaskStore still validates the actual admission and its obligation. A client cannot choose a weaker classification.

## Finite query and public schema

`ReadRequest::Query(TaskQuery)` dispatches to `NativeTaskStore::query` using
the original request, existing Task read admission and host read limits.
`ReadRequest::Schema` dispatches to `NativeTaskStore::query_schema` under
the same authority. Their responses are `ReadResponse::Query(TaskQueryRead)`
and `ReadResponse::Schema(TaskQuerySchema)`. The complete `ReadView` remains
a separate internal consumer operation; it is not a finite query adapter.
See [finite public queries](native_task_store.md#finite-public-task-queries)
for the column whitelist, presence, EOF, bounds and cursor contract.

A native query request selecting the state of known IDs is:

```json
{"operation":"query","input":{"selection":{"table":"tasks","ids":["01KSH1CRFFG8Z74B5ZS78WW808"]},"columns":["id","status"],"page_size":20,"top_k":100}}
```

This example illustrates canonical native ULIDs, not an assertion that the
example ID exists in the selected Task corpus. No prefixed legacy identifiers
are accepted. Schema requests are `{"operation":"schema"}`. Transport hosts
can generate the query-input JSON Schema directly using
`schemars::schema_for!(graphit_records::task_store::TaskQuery)`, including
closed selections/columns, positive page bounds, native ID shapes and an
opaque string cursor. The host wraps that producer DTO in its actual transport
selection/input envelope; the protocol does not mint admission or infer actors.

## Participant fences and results

A participant claim explicitly returns its record and a canonical base64url token without padding. The token contains the native 32-byte cryptographic nonce. Its transport type has no Debug or Display; only participant claim responses and private mutation inputs serialize it. It is absent from Task/Session records, catalogue, exact history, views and head projections. A token by itself is not an admission or permission.

Read responses preserve the native distinctions between NoCorpus, absent exact
records and an available empty result. `ReadRequest::GetTask(TaskGetInput)` routes
to `NativeTaskStore::read_task` and returns
`ReadResponse::GetTask(Option<Box<TaskGetRead>>)`. `GetSession(SessionGetInput)`
routes to `NativeTaskStore::read_session` and returns
`ReadResponse::GetSession(Option<Box<SelectedSessionRead>>)`. Box is transparent in Serde;
an absent exact result remains JSON null. Complete read audits explicitly traverse the selected history/comment pages.
The private comment-write audit retains `TaskDetail` for real idempotency checks;
unused public full getters are removed. Historical post-boxing tests apply to that earlier response contract.
Write responses contain native Task/Session results or participant claims. SDK,
authorization, cancellation, generation, fence and revision failures retain their
typed causes. Confirmed publication failures keep their committed SDK version;
the dispatcher performs no automatic retry or rollback claim.

The native write dispatcher is composed on Linux, macOS and Windows, using the same Original, admission and Records writer APIs. The Task full-text reader and publisher use that same supported-platform composition. Historical Linux receipts retain their recorded scope; compilation for another target and native execution are separate evidence. Read transport values are portable Rust data; this module does not qualify an HTTP endpoint, external launcher or executable merely by serialization.

## Selected Task Get wire

The closed `GetTask` input is `id` plus optional `history` and `comments`.
Each selected section is `{page_size, cursor?}`, with positive `page_size` and
an opaque string cursor. Omission or null means unloaded, not empty. A present
result contains `task`, `creation`, `last_event` (null at revision one),
`generation`, both `history_start_cursor` and `comments_start_cursor`, and the
two selected sections. `history` is null or `{before, events, next_cursor}`;
`comments` is null or `{comments, next_cursor}`. History events ascend by
revision and comments by sequence. `before` is the validated predecessor Task
for a replaceable page's first diff.

The initial cursors fix the generation for a later explicit first-page request.
Without a cursor, Get captures the latest authorized snapshot. Continuation
now carries optional SDK `BranchProvenance` inside its opaque binding. Records
uses `at_version_with_provenance(version)` to return the pin and optional proof
for current, named or still-unmaterialized inherited reads. Named continuations
require proof and reject a different namespace after branch retire/recreate.
`at_inherited_version(version)` returns a pin and required proof for an inherited
cursor after child materialization; the SDK checks a direct default parent,
version no later than the fork and double-ref identity. Both section cursors are
validated against their received binding before an inherited no-proof binding
is promoted to proof; returned start/next cursors carry that proof. Current
authority, project/version, corpus and physical-generation bindings are still
rechecked. No new secret, snapshot lease/cache or arbitrary URI/branch authority
is added. The selected Records API/proof and wire paths have the focal
qualification below; App backend and remaining CLI/browser boundaries are
tracked separately.

Current state validates head/creation/last-event and predecessor consistency,
including references and an associated last comment. Selected pages validate
their records, seam continuity, reference chain and comment/event ownership;
they do not certify corruption outside their selected scope. A complete audit
or export explicitly traverses both section chains through EOF.
`SessionGet` retains its existing complete detail contract.

On 2026-10-24,
`cargo test --release --locked --offline -p graphit-records --test task_protocol`
passed 10 tests with zero failures or filtered tests. The cases include positive
page sizes and closed inputs, real comments, actual Auth and cancellation.
Harness runtime was 1.98 seconds; build time was 4 minutes 20 seconds and total
command time 4:22.73. Together with the three selected native page tests, these
are [13 distinct affected tests](native_task_store.md#selected-get-linux-qualification),
without adding historical SDK counts. These command times are not product
latency or memory measurements.

On 2026-10-25, [eight exact App backend selections](rust_task_mcp.md#task-get-app-backend-qualification)
passed with zero failures, including typed encoding/confirmed writes, actual
MCP/native HTTP, local managed-role control and cancellation/delivery. They do
not replace or rerun the 13 Records results. The principal executable CLI case
then passed completely in [PASS02 with a private corrected caller](rust_task_mcp.md#task-get-executable-cli-qualification),
including the unchanged `stale_main` assertion. FAIL01 is preserved as history;
the earlier two-Root parallel CLI pass was not repeated. Browser cookie/socket
and React qualification remain pending, and no
installed App delivery is established. Historical wire, Comment and Session
results retain their original scope; no new endpoint, parallel protocol or
default page size is added.

## Host integration

A host decodes the typed request, uses its actual issuer and selected scope to obtain the corresponding admission, and calls execute_read or execute_write with its original Arc. It rechecks its original request before delivery. A read handler keeps its original request and admission:

```no_run
use std::sync::Arc;
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_records::{task_protocol::{execute_read, ReadRequest, ReadResponse}, task_store::{TaskStoreError, TaskStoreReadLimits}};
async fn view(original: &Arc<SnapshotContext>, admission: &ProjectReadAdmission<'_>, limits: TaskStoreReadLimits, max_query_bytes: std::num::NonZeroUsize) -> Result<ReadResponse, TaskStoreError> {
    execute_read(ReadRequest::ReadView, original, admission, limits, max_query_bytes).await
}
```

Task ServerHandler, native HTTP and the compiled CLI consume these operations. Search uses graphit_task_search, POST /api/tasks/native/search and task search --text TEXT --mode fts --page-size 10 --top-k 10 with flags mapped to the same native selection/input contract. Source returns operation:search/result; the existing Role endpoint preserves its direct native search result. Missing/incompatible indices and invalid/stale cursors remain causal errors, not healthy empty results. Browser ingress and the React catalogue retain their own authentication and consumer qualification.

The browser's existing same-origin request marker is not authentication. Runtime credentials remain outside JavaScript and localStorage. Native browser integration requires its own authenticated ingress and consumer qualification; a CLI or Runtime-header test does not supply that proof.

## Authored references in the Task MCP catalogue

The App [Task catalogue](../../crates/graphit-app/src/task_server/catalog.rs) advertises the existing native `input.references` patch on creation, comment, progress/checkpoint, release, Task/Session revision, check verification/supersession, links, flag, completion and cancellation. Claims and heartbeats reject the field. Each array entry requires `target: {type, id}` and `relation`; optional `field`, `target.scope`, `target.scope_id` and `target.context` are strings when supplied. Entry and target objects reject unknown properties. Omission preserves authored links; `[]` replaces them with an empty authored set; null, nonsequence and malformed entries are rejected by the real reference producer. The catalogue does not create an admission or weaken target validation. Runtime [Task instructions](../../templates/agent-instructions/skills/task/entry.md) consume this advertised contract using `selection` and `input`, not flat selectors or caller-selected actors.

The catalogue/rendered-example test and actual authenticated TaskServer MCP consumer test live under [catalog](../../crates/graphit-app/src/task_server/catalog/). They are contract/transport/storage checks; they do not prove installed Kiro delivery or a model receiving context. `SessionRevisionInput` (`tsk-13af`) is dispatched as `WriteRequest::ReviseSession`, operation `revise_session`, classified CoordinateTask. Its required fields are `target:{id,claim:{token,expected_revision}}`, positive `lease_ms` and canonical `reason`; optional `title`, `description`, `strategy` use Keep/Set presence and reject null. References retain the common flat patch. The dispatcher borrows the same Original and admission and returns the existing Session result. Checkpoint updates progress; revision changes specification. A request with no effective change is rejected, and Session search/FTS remains outside this operation.

## Authored references adapter: native Linux qualification

`tsk-6cf9` now includes the Root-ceded [input/DTO seam](../../crates/graphit-records/src/task_protocol/input.rs) and [write dispatcher](../../crates/graphit-records/src/task_protocol/write.rs). The affected native qualification passed 70 distinct functional tests (Task SDK47, TaskStore library15 and TaskProtocol8), six Records doctests separately, locked no-run compilation and scoped strict Clippy. Existing Comment/protocol receipts retain their historical scopes and are not added to these counts. The same flat JSON references field is represented by FieldUpdate<serde_json::Value>: omitted maps to Keep/borrowed Omitted, [] maps to a borrowed empty Sequence, and explicit null/nonsequence maps to rejection rather than omission. Unknown fields and invalid optional shapes remain invalid. The parsed temporary entries borrow the actual field retained for the call; in that qualification, fifteen creation/mutation dispatch paths passed AuthoredReferencesField before limits; Session revision adds the sixteenth current path. Claims/heartbeats have no patch field and reject references.

WriteRequest::CreateTask uses TaskCreationInput, CreateSession uses SessionCreationInput, and completion uses CompletionInput<T> with id, claim and references. CommentInput also carries that flat references field; its native TaskComment kernel remains unchanged. Classification, same Original Arc, real admission and input bounds precede dispatch. The caller maps reference max_entries/max_text_bytes from existing ContentLimits.max_collection_entries/max_input_bytes; no new knobs, authority, old overload or compatibility path are introduced. Storage normalizes Preserve versus Replace in creation/comment/lifecycle envelopes for exact retry and never reapplies historical patches.

Root ceded the minimal R7 consumer exception in [App TaskServer](../../crates/graphit-app/src/task_server.rs): Failure::store maps TaskStoreError::Reference(ReferenceProjectionError::Request(_)) to request_cancelled_or_expired, whose Failure::status is HTTP408. Other Reference errors map to reference_projection_failure and conservative HTTP500, not generic client-input classification. Confirmed keeps its existing write_confirmed response with observed version, committed=true and recursively mapped cause; no failure is relabeled as rollback or uncommitted. The real reference_failure_mapping_preserves_request_status_and_confirmed_cause unit passed **one case separately**, under the Root/core_writer `tsk-cd41` receipt. Scoped App/Daemon strict checks passed externally; Role4 and rustdoc1 from that receipt are not added to the native70/Records doctest6 counts. This mechanical mapping does not qualify the references HTTP route or App/browser journey; the native references qualification comes from the separate Records suites.

The [native persistence/query contract](record_relations.md#rust-tasksession-persistence-and-query-native-linux-qualification) defines required snapshots/current markers and one-pin coverage. This adapter change supplies writes only: App POST /api/tasks/references and cross-source FullNoLoss remain separate planned deliveries, with no HTTP/MCP/browser qualification here. TaskReferencesRead::Published represents outgoing and incoming as Box<TaskReferencePage>; Serde keeps the page JSON unchanged, with no additional wire wrapper. This Rust layout correction alone supplies no consumer qualification; the native query is qualified by the affected Records suites.

## AddTaskComment: Linux-qualified

The [input](../../crates/graphit-records/src/task_protocol/input.rs) contains `WriteRequest::AddTaskComment(CommentInput { references: FieldUpdate<serde_json::Value>, target: OwnedTarget<TaskId>, lease_ms: LeaseMillis, idempotency_key, kind, body })`. `required_operation()` classifies it as **OwnTask**. The actual [write dispatcher](../../crates/graphit-records/src/task_protocol/write.rs) forwards the same original Arc and actual admission into `NativeTaskStore::add_task_comment`; classification alone is not authority. Actor/time/new ID/sequence, scope and branch are server-derived rather than caller fields. This typed protocol Source is Linux-qualified in the affected scope, not a qualified MCP/App method.

The [output](../../crates/graphit-records/src/task_protocol/output.rs) is
`WriteResponse::Comment(Box<TaskCommentOutcome>)`. Box serialization is
transparent: no extra wire wrapper is added. The outcome distinguishes
Created/Existing with the observed Task and actual comment. Native full detail
retains its complete validated comments; public Get loads comments only through
its explicit ordered section pages. Public records/comments never contain the
private claim nonce.

Kinds are note/decision/problem/lesson/knowledge; literal nonblank Markdown is preserved. Retry identity is scoped to Task + key and exact kind/body/actor under the same live fence and historical claim epoch. Either the current revision or original pre-comment revision is accepted for an exact retry. Existing does not materialize a branch, advance SDK publication or extend the lease. Release/reclaim with a new epoch rejects the old key, even for the same actor holding a valid new token. Full read sequence, event bijection, neutral codec headers and equal-millisecond timestamp rules are in the [transactional Comment contract](rust_record_transactions.md#taskcomment-contract-tsk-1962) and [native store](native_task_store.md#task-comments-linux-qualified).

Known committed publication with failed final recheck retains Confirmed version/cause; uncertain SDK errors are causal and not rollback or NoEffect. No new context, retry loop or continuous Auth/SDK fence is supplied by the dispatcher. Comment affected functional/docs suites and scoped strict passed, with the prior protocol receipt kept separate and no MCP/App consumer implementation claimed. Recovery uses a fresh request, newly authenticated caller and new admission for the same Task participant. Runtime/Local participant identity survives issuer restart with a new valid credential under the installation and canonical identity described by the [native store](native_task_store.md#claims-renewal-and-handoff). Managed Role participants remain execution-specific. Cancelled-request callers/admissions are not reused; token, revision, live lease and PROJECT authority remain required.

The affected Task protocol suite passed **seven cases**, within TaskStore21 + TaskProtocol7 + library `task_`32 = **60 distinct Linux tests**. Focal7/1 are included. Records doctests separately passed **six cases (four ordinary/two compile_fail)**. The actual SDK-published comment and production Confirmed/cancellation composition preserved version/cause and allowed Existing under a fresh same-issuer caller; no SDK interception or atomic Auth/SDK proof is claimed. [The transaction evidence](rust_record_transactions.md#comment-fixtures-documentation-and-qualification-gate) retains prior fixture/strict failures and qualification limits. After GetTask/GetSession boxing, protocol7 and Records doctests6 passed again; scoped strict `--lib --test task_store --test task_protocol --no-deps -- -D warnings` exited zero. These repeat runs do not add distinct tests or imply Task completion.

## Verification

Focused wire tests cover strict identifiers, fields, calendar presence and validity, leases, canonical participant tokens and operation-derived admission classes. Native temporary-project tests use real Core captures, Auth admissions and Lance storage, decode actual owned requests, persist and reopen calendar/check/terminal history, and test wrong admission, different original and cancellation. Existing receipts qualify the prior native protocol boundary; the new Comment Source is Linux-qualified with its affected functional/docs/scoped strict evidence. They do not qualify pending transport or browser deployment.

## Native Session revision: Linux qualification

The ActorBusy outcome below belongs to the historical `tsk-13af` Source. `tsk-ea469`
removes that aggregate gate for distinct Session and Task targets while preserving
per-target actor/token/revision/lease checks and same-target retry semantics.

The authenticated App TaskServer suite passed 16 distinct cases (24.94s), including actual MCP/HTTP Session revision, foreign-owner and missing-coordinate denial, references-only mutation/preserve/clear and exact get/history without private tokens. Runtime passed six instruction/snapshot cases (0.06s); the App rendered-example check validates four examples in each of two namespaces against the actual advertised DTO. This separates SDK/storage proof, rendered contracts and actual consumers. Corrected example-nonce, HTTP error-envelope and TaskSearch snapshot failures remain in the receipt; reruns are not added to distinct counts.

## Session revision CLI consumer

The Linux CLI maps `graphit task revise-session --id ID --claim-token TOKEN --expected-revision REV --lease-ms MS --reason REASON --title TITLE` to the existing
`revise_session` write request, with the same native input, captured selection,
CoordinateTask admission and per-target fence/revision checks. `tsk-678a` adds only
this consumer mapping and its executable CLI proof. The SDK, MCP/HTTP and four
rendered JSON examples from `tsk-13af` retain their separate qualification.

## Selected Session Get wire

`GetSession` accepts the closed `SessionGetInput`: `id` and optional
`history: {page_size, cursor?}`. Positive page size is required for a requested
page; omitted or null history remains unloaded. It returns current `session`,
`creation`, `last_event`, `generation`, `history_start_cursor` and `history`.
There is no top-level `events` field or comments section. History pages contain
`before`, ascending `events` and `next_cursor`, bound to the selected Session and
SDK publication through the producer's opaque cursor. A missing result stays
JSON null. A present empty page differs from an unloaded history.

The dispatcher uses the existing actual Original/admission and invokes the
selected producer directly. Complete Session audit/export traverses its explicit history pages through EOF.
The public full getter and its unused DTO are removed; the hook recovery
contract now deliberately selects current state and producer cursors. Generic JSON CLI transport requires no separate
Session parser or new command. MCP schema and HTTP/React integration belong to
the transport host and need their own affected qualification. The selected
Session wire and native dispatcher tests are pending the joint build; no earlier
Task or Comment qualification is reused as proof of this new boundary.

`CreateTask` accepts optional `commit`. `set_task_commit` is a native metadata write with required `id`, `expected_revision`, `commit` (full lowercase SHA-1/SHA-256 or null) and canonical `reason`, plus optional `claim`. It returns the updated Task and permits terminal Task metadata updates without changing status or check evidence. Live ownership requires the correct unexpired fence and matching revision; unowned updates require Task admission. Reference updates are not accepted by this operation. All transports use the same store implementation and original cancellation/deadline context. Associations persist in existing payloads and lifecycle snapshots; no new physical column or authoritative corpus is introduced.
