---
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-678a}
    relation: implements
  - target: {type: task, id: tsk-ea469}
    relation: implements
  - target: {type: task, id: tsk-13af}
    relation: implements
  - target: {type: task, id: tsk-e35c}
    relation: implements
  - target: {type: task, id: tsk-b9b4}
    relation: derived_from
---

# Authenticated native Task MCP and HTTP component

The App Task component consumes the Records native protocol and its physical Lance store. It receives the process-owned DaemonAuth, the runtime-retained ProjectPaths/ProjectId/VersionContext tuples, an explicit default tuple, native producer limits and a finite operation deadline. It does not bootstrap a project or issuer from incoming arguments.

Each MCP operation has its own public tool and specific JSON schema. The catalogue contains the native Task reads, Session reads and delivered lifecycle operations. Unsupported operations are absent. Native reads, textual/semantic/hybrid search and delivered writes use the same catalogue on Linux, macOS and Windows. These Source selectors do not extend the historical Linux qualification to another platform or host.

MCP arguments use the current Rust envelope. A machine consumer requiring JSON
sets the output selector alongside the domain fields:

```json
{
  "ai_optimized": false,
  "selection": {"project_id": "01KSH1CRFFG8Z74B5ZS78WW808", "project_version": "main"},
  "input": {"id": "01KSH1CRFFG8Z74B5ZS78WW809"}
}
```

Structured MCP tools accept optional strict boolean `ai_optimized` at the argument
root. Omitted/`true` returns TOON text without a duplicate JSON `structuredContent`;
`false` returns JSON. The selector never belongs inside `input`. JSON-RPC and
catalogue schemas remain JSON. Native HTTP and CLI JSON responses keep their
JSON representation and do not gain this MCP-only argument. See
[MCP output](mcpstdio_module.md#structured-output-and-errors).

The selected tool supplies the native operation; callers cannot supply an actor, admission class, role grant or filesystem authority. Omitted selection uses the configured default tuple. Explicit selectors must identify exactly one retained tuple. project_dir is a literal Unicode path string, preserving spaces, accents and escapes; it conveys no authority. Invalid Unicode and old object envelopes are rejected. No unretained root or version is created.

For reads, `project_dir` identifies an already admitted PROJECT. When an explicit
`project_version` belongs to another retained checkout of that same PROJECT and
storage namespace, the reader selects its existing captured tuple. An exact
root/version match takes precedence; multiple remaining targets are ambiguous.
An explicit `project_id` must still agree. The selected tuple undergoes fresh
authorization and exact live-membership checks before and after reading;
retirement remains terminal. This does not create a version or require a watcher
for the read. Writes keep exact root/version selection and require the caller to
select the intended checkout explicitly.

`TaskServer::native_router()` exposes only the native HTTP routes for composition into the shared Source listener, without mounting a second `/mcp`. The standalone `TaskServer::router()` combines these same routes with its own `/mcp`; `serve_listener(BoundListener)` serves that standalone composition on a dedicated already-reserved loopback listener. Root application composition owns executable startup, shared routing, discovery and daemon teardown. The component uses rmcp Streamable HTTP and Axum.

The canonical native HTTP route is `POST /api/tasks/native/{operation}`. On Linux, macOS and Windows it dispatches the active operations from `task_server::catalog::OPERATIONS`, including reads and writes. The path uses the native protocol operation name in `snake_case`, such as `get_task`, `get_session`, `catalogue`, `query`, `schema`, `session_heads`, `create_task` or `add_task_comment`; it does not use the public MCP tool name. An operation outside the active catalogue returns `unknown_operation` with HTTP404.

The native HTTP body uses `{selection, input}`, without the MCP `ai_optimized` selector. HTTP failures wrap their native code and causal fields in `{error: ...}`; MCP tool failures return those fields in their error content. Successful reads preserve the complete `ReadResponse` envelope `{operation, result}`; successful writes preserve `WriteResponse` `{kind, result}`. The HTTP handler does not strip these tags or return only the inner result. For example, `get_task` returns `{operation: "get_task", result: ...}`. Catalogue cursors remain typed JSON objects. Dates, dependency IDs, revisions, ownership observations and timestamps come from persisted Records.

### Session Get selections

`graphit_task_session_get`, native HTTP `get_session` and CLI Session Get share
`SessionGetInput {id, history?}` and the Records `read_session` dispatcher. The
optional history request contains a positive `page_size` and optional opaque
`SessionDetailCursor`. Omission or null leaves history unloaded. An absent Session
returns JSON null; a present `SelectedSessionRead` contains `session`, `creation`,
nullable `last_event`, `generation`, `history_start_cursor` and nullable `history`.
Default Get does not collect the immutable event list.

A selected history page is `{before, events, next_cursor}`, with events in ascending
revision order. Echo the current response's start cursor for a first pinned page,
then the returned next cursor unchanged. The native reader validates the same
Original, current admission, captured PROJECT/version, SDK generation and branch
provenance, including page predecessor continuity and EOF. Clients do not choose
physical paths or decode cursor authority. Explicit complete audit/export walks
every page to EOF under that pin; one loaded page does not establish completeness.
The native full-audit Session reader remains available to its explicit/internal
consumers. This selected protocol introduces no second transport or snapshot cache.

The cookie Browser route `POST /api/tasks/session` consumes this same dispatch and
returns its inner result through the existing authenticated Browser adapter. Its
[Session journey](ui_dashboard.md#session-current-state-and-audit) opens current
state first and exposes explicit history and complete export. Task Get and the
Memory adapters retain their existing contracts. The selected Session consumer
changes await the coordinated native build and SDK/HTTP qualification; the earlier
Task Get receipts remain separate evidence.

### Task Get selections and output boundary

`graphit_task_get`, native HTTP `get_task` and CLI Get use the same closed input:
Task `id` with optional `history` and `comments`, each requesting a positive
`page_size` and optional opaque cursor. Omission or null leaves that section
unloaded. An absent Task returns null. A present result contains `task`,
`creation`, `last_event` (null at revision one), `generation`,
`history_start_cursor`, `comments_start_cursor`, and null or explicitly loaded
sections. History pages have `{before, events, next_cursor}`; comments pages
have `{comments, next_cursor}`. Present empty pages differ from unloaded
sections. Events ascend by revision, comments by sequence, and the validated
`before` Task supports the first diff without keeping earlier pages.

Use the initial section cursor for a first-page request pinned to the current
result, then pass each returned cursor unchanged. A Get with no cursor sees the
latest authorized snapshot. Cursors bind the Task, section, selected PROJECT
and version, corpus identity, physical generation and optional SDK
`BranchProvenance`. The integrated `at_version_with_provenance(version)` returns
the authorized pin and optional proof for current, named or still-unmaterialized
inherited reads. Named cursors require proof and reject namespace replacement
after retire/recreate. For an inherited cursor after child materialization,
`at_inherited_version(version)` requires a direct default parent, a version no
later than the fork and SDK double-ref identity validation, returning mandatory
proof. Both requested sections are checked against the received binding before
promoting an inherited no-proof cursor; returned start and next cursors carry
the new proof. Each request still rechecks authentication, PROJECT/version,
corpus and physical-generation identities. No new secret, snapshot cache,
lease or caller-selected URI/branch authority is introduced. Selected Records
API/proof paths and the eight App backend cases below passed their respective
focals; principal CLI has the private-artifact functional qualification below.
Browser/React qualification remains pending.

Current-only Get validates current/creation/last-event and its predecessor,
references and associated last comment. It does not audit every old event.
Pages validate their selected records and continuity/ownership seams. Old
out-of-scope corruption can leave current readable while its affected page or
full audit fails. Complete export must traverse every history and comment page
through EOF at one generation; unloaded is not empty. Session Get also uses
selected current state and optional pinned history through `SessionGetInput`
and `SelectedSessionRead`, without a top-level event list. It has no comments
section. See the [typed Task wire](rust_task_protocol.md#selected-task-get-wire),
[Session wire](rust_task_protocol.md#selected-session-get-wire)
and [native read contract](native_task_store.md#current-task-and-explicit-detail-pages).

The response boundary must bounded-encode the typed `ReadResponse` or
`WriteResponse` with the existing limited writer before constructing any JSON
`Value`. If a consumer needs `Value`, it drops the typed response after encoding
and parses only those bounded bytes. The existing output-byte guard, final
authorization/context recheck and `committed=true` write-error semantics remain
required. Changing only the final byte-vector conversion would leave the earlier
Value copy unbounded; encoding hardening alone does not make full-history Get
deliverable. No new cap, quota or endpoint is introduced.

On 2026-10-24 the Records page selection passed three tests and its native
protocol suite passed 10, with zero failures: [13 distinct affected tests](native_task_store.md#selected-get-linux-qualification).
Those results establish the selected Records/wire boundaries and were not rerun
for the App qualification. No product RSS, O(1) memory or speedup claim follows
from these tests or their aggregate build times.

#### Task Get App backend qualification

On 2026-10-25 the App unit executable with SHA256
`bb4f82b397812cf77a2dd58918ced7b4e0b477cd6a83c7d5360547e1687968a1`
was run directly, without Cargo. Each of eight exact filters matched one test;
all eight passed, with zero failures. Each exact filter is
`task_server::tests::<name>`, where `<name>` is the test name below:

| Test name | Harness runtime (s) |
| --- | --- |
| `typed_response_bound_counts_encoded_unicode_and_escaping` | 0.00 |
| `bounded_native_encoding_keeps_the_confirmed_write_observable` | 0.15 |
| `actual_mcp_writes_http_catalogue_dates_history_and_reopen` | 1.39 |
| `schema_selection_and_confirmed_errors_preserve_native_contract` | 0.01 |
| `managed_role_is_verified_per_call_and_revocation_denies_next_request` | 0.17 |
| `sdk_cancellation_drains_native_writer_wait_without_publishing` | 0.05 |
| `actual_mcp_cancelled_child_descendants_and_cancelled_dependency_preserve_native_gates` | 3.22 |
| `retired_selection_after_admission_wait_cannot_deliver_healthy_read` | 0.13 |

The [App backend tests](../../crates/graphit-app/src/task_server/tests.rs)
qualify selected typed-encoding limits, observable confirmed writes, actual
MCP/native HTTP, managed-role local control and denial of its next request,
cancellation and final delivery checks. The role result adds no OIDC, UserInfo
or immediate JWT-revocation guarantee. The private eight-log receipt
`app-unit-qualification-01.json` has SHA256
`6dd911f6d4b5722a22969debecb073054f2afd75b471856707a1b93812c5ea25`.

#### Task Get executable CLI qualification

FAIL01 of `compiled_root_task_cli_mcp_native_http_history_and_current_now`
failed at the `stale_main` gate expecting `!success`. That result remains
historical evidence; passing page assertions before its failure did not qualify
the complete journey. On 2026-10-25, PASS02 ran the entire case after the
permanent caller correction and passed: one test, zero failures, three filtered
tests, 5.08 seconds of harness runtime and 5.09 seconds total. The `stale_main`
gate passed without relaxing its assertion, resolving FAIL01 for this functional
scope.

PASS02 directly reused integration ELF `task_executable-66eb929585168e52`, SHA256
`a707e6e93c14d9a140ea8b58fb22917e1895d90f0c9aa371f8941bc7f899a29d`,
without Cargo. Its corrected private Release caller was 412022192 bytes with
SHA256 `84c8e25ea905c353f601042e62d33e3ae716ec955a9f08f1083e2f1ade3a6f5e`.
That caller used optimization level 3, `test=false` and `strip=none`, with
jemalloc profiling and stats compiled in and `MALLOC_CONF` unset. The tested
artifact contained temporary FILE diagnostics; restored sources retain the
permanent caller fix. The private receipt `app-cli-qualification-02.json` has
SHA256 `f1e41f0ba20c90df0000768bf251806f3b4ae04daef8fe636a28307ac44b657d`.

This qualifies the functional CLI journey and private caller. These artifacts
differ from the App unit executable above; the results were not obtained in a
single common configuration. They do not qualify the normal installed binary,
product RAM/RSS, latency or a memory improvement. Records 13 and App backend
eight results were not rerun. The earlier parallel two-Root CLI case passed once
in 1.96 seconds and was not repeated. Browser cookie/socket and React execution
remain separately pending; historical receipts retain their original scope.

### Catalogue and request admission

Task LIST (`catalogue` in native HTTP) advertises the schema generated from the
Records `CatalogueCursor` and `CatalogueFilter` DTOs. Its JSON-object cursor is a
keyset continuation: it carries the selected project/brand/root/version, pinned
generation and branch, original filter, and last task's `after_created_at`/`after_id`.
Pass the returned object unchanged; it has no offset and grants no authority.
The cursor's root is a bounded literal Unicode path string. Records still
validates its path, selection, filter and publication binding on continuation.
This does not change the separate Query string cursor or Search cursor contract.

`graphit_task_schema` and `graphit task schema` return the producer's query tables
and permitted column types under the same current read admission. Query uses
`graphit_task_query`, `graphit task query --table tasks --column id --column status --page-size 10 --top-k 10` or native HTTP `query`:

```json
{
  "input": {
    "selection": {"table": "tasks", "ids": ["00000000000000000000000003"]},
    "columns": ["id", "status"],
    "page_size": 1,
    "top_k": 1
  }
}
```

The outer `selection` still selects the retained PROJECT/version. Inside `input`,
`selection` chooses `tasks` or `sessions`, with optional typed IDs and status;
Tasks additionally support `session_id`. `columns` determines the exact order of
values in each returned row. Both positive limits are required. A continuation
is the opaque string returned in `next_cursor`; reuse the same project, version,
query, projection and limits. Records validates its generation and binding.
The response keeps `{operation:"query",result:{state:...}}`: `no_corpus` is
distinct from `published` with empty `rows`. It does not establish continuity
with another store or historical dataset.

The Query input schema is generated from the Records DTO with Schemars, using
inline subschemas before embedding it in the closed `selection/input` envelope.
MCP, native HTTP and CLI parse the same native input. Their catalogue no longer
exposes the unbounded `read_view` operation, and the CLI no longer exposes
`read-view`. Existing Browser and internal full-view readers remain available to
their current consumers. Neither query filters nor cursors supply actor,
admission, filesystem authority or claim credentials.

The former native `/api/tasks/get`, `/api/tasks/catalogue`, `/api/tasks/session`, `/api/tasks/view` and `/api/tasks/session-heads` routes have been removed. The separate cookie-authenticated Browser `/api/tasks` surface remains in place.

Authentication is performed on every current incoming request. Authorization Bearer carries a runtime, local or managed-role credential selected by x-graphit-auth-origin. Managed-role requests also carry the existing encoded RoleCredentialReference in x-graphit-role-reference; the issuer verifies that reference, token, binding and actual captures. Initialize does not cache admission. For each accepted job, the ingress constructs one real `RequestContext` with that request's cancellation and finite deadline. The executor borrows this same context and passes its snapshot Arc through capture, authentication, classified Records admission, native execution and delivery; it does not construct another operation context. Raw credentials have operation-owned zeroizing storage and are absent from diagnostic responses.

Only successful participant claim responses include the canonical opaque claim fence. Get, history, catalogue and Session heads never contain it. Native lease, revision, ownership and gate checks remain authoritative. An SDK Confirmed error preserves its confirmed version and nested cause category; it does not claim a rollback. If a successfully committed operation cannot be encoded or fit the configured response limit, the response explicitly records committed=true.

Native Task and PROJECT Memory storage selection renews an expired renewable Broker Code credential on demand through `NativeProfileManager::refresh_broker`. The resolver retains only a weak daemon reference, uses the daemon's real Runtime administrative authority with the original request, and serializes actual renewal before recapturing the service. It compares provider/profile/token revisions plus issuer/subject; it never rotates a different selected account. Native USER Memory also prepares that credential before USER authentication. Device `LoginRequired`, incoming/Web credentials and failed/uncertain rotations remain explicit. This applies both to scoped S3 and to local records selected by a valid S3-disabled Broker descriptor. A potentially applied refresh is not replayed for its exact token revision; new valid credentials remain usable. No request retries a record mutation as part of renewal.

Storage I/O errors retain causal public categories: only interrupted or timed-out requests return `request_cancelled_or_expired`; permission errors and typed Auth/profile/service authorization failures return `authentication_or_permission_denied`; other I/O failures return `store_failure`. Responses never copy private storage error messages. A provider revision change invalidates dependent login state; a new normal Broker login is required when refresh rejects the changed provider.

SDK cancellation, disconnected-handler drop, deadline and component shutdown cancel the original Core token. Native operations run on Tokio blocking workers, using the captured runtime handle for asynchronous SDK calls. Synchronous configuration, scope capture and filesystem work therefore do not occupy the asynchronous scheduler. Actual native work stays in the component-owned JoinSet until joined. Shutdown keeps this set in State while awaiting it, so cancellation of a waiter cannot drop native work or release DaemonAuth early.

BrowserAuth::task_router provides a separate cookie-authenticated read composition. It verifies the local signed Web JWT and obtains a fresh Broker PROJECT decision through ProjectRecordAuthority::admit_broker_read before using this same native Task reader. Exact Origin and an active opaque cookie are required; Origin or X-Graphit-Request alone never establish admission. The browser composition retains the session credential during the operation and reuses its owned cleanup/join lifecycle. Its socket qualification is separate from these native header credentials; neither component alone qualifies React deployment, CLI/stdio startup or a shared executable listener.

## Current Root wiring and Linux qualification

The current component implements the canonical native route, catalogue-based dispatch, full native response envelopes and the borrowed request-context executor. Source composition uses `native_router()` so the Source listener can retain its single MCP service. The standalone Task listener retains its own MCP service.

The current `tsk-b9b4` Source wiring attaches the native TaskServer even without Browser opt-in, shares its live-selection provider and exposes the catalogue through the single Source MCP dispatcher with the real RMCP context. Runtime ordinary permissions explicitly include ReadTask, WriteTask, WriteOwnTask and CoordinateTask; each operation still requires its applicable native admission. Source owns Task shutdown; the Browser composition closes only BrowserAuth.

Task Search accepts `mode: "fts" | "semantic" | "hybrid"`; omission keeps
model-free FTS. Semantic/hybrid use the configured local, Broker or direct embedding
service and progressively published compatible Task vectors. The managed Role read
and authenticated Browser operation use the same producer, with their own existing
corpus authorization. See [incremental Task search](native_task_search.md#incremental-semantic-and-hybrid-modes)
for reuse, content fingerprints, partial coverage and pagination.

The `graphit task` CLI uses `task_entry` and the authenticated live Source MCP connection. Linux exposes 28 native subcommands, with six reads on other platforms. The model-free `task search`/`graphit_task_search` first-corpus integration is qualified separately in [Native Task FTS](native_task_search.md#first-corpus-and-application-consumers). Each operation exposes its fields as typed flags; list options repeat, and no whole-input JSON option is accepted; `--project-dir` and `--project-id` select the project, and `--project-version` is global. SourceProxy completes nested selection only where declared by the tool schema and preserves explicit invalid values for rejection. See [Source foreground](rust_source_foreground.md#task-no-root-source-e-cli) for syntax examples.

## Independent target claims

Task derives its Runtime participant from the server-owned InstallationId and source
origin, independently of the issuer epoch and credential generation. Local adds the
authenticated canonical provider/profile. Fresh authentication with a new valid
credential preserves that Task participant across restart; old credentials remain
invalid. Managed Role participants retain execution-specific identity, and Broker Task
writes remain denied. The same participant is shared across Root stdio clients. That actor may claim distinct Tasks or Sessions concurrently;
there is no aggregate claim quota. Every mutation still requires the target’s actual
private token, live lease and current revision. A different actor cannot claim a target
with a live owner. Retrying that target as its current actor returns the existing claim
without renewal. Read/history responses exclude all fences.

The `tsk-ea469` correction removes the obsolete `actor_busy` error without changing Auth, static MCP configuration or Runtime transport.

## Task comments

On Linux, macOS and Windows, `graphit_task_comment_add` maps directly to the native `add_task_comment` operation. Its required input is `target: {id, claim: {token, expected_revision}}`, positive `lease_ms`, `idempotency_key`, `kind` (`note`, `decision`, `problem`, `lesson` or `knowledge`) and nonblank Markdown `body`. The body is stored literally, including whitespace. The current authenticated participant needs `OwnTask` admission and a live fence; the server supplies authorship. Unknown fields, including a client actor, are rejected.

The response is `{kind: "comment", result: {kind: "created" | "existing", result: {task, comment}}}`.
Repeating the same key and content returns the same comment without another
history entry, using either its accepted pre-comment revision or the current
revision according to the native producer. `graphit_task_get` returns persisted
comments only when its `comments` section is explicitly requested; follow that
section's cursors for the complete list, including after reopening the corpus.
The private same-pin comment-write audit retains complete history and comments
for actual mutation idempotency; it is not an agent/public read API. Complete
read audits use the explicit section cursors through EOF. Neither comments nor Task reads disclose the participant fence.

## Trusted startup roots

DaemonAuth::start receives the trusted retained ProjectPaths list alongside installation paths, startup policy and the original context. Its realm contains installation.root() and the actual configured project roots; AuthStore captures that same root set. Records admissions and managed-role start_run require exact project-root membership. HTTP selectors cannot expand this list. Fixtures use the real sibling project/installation roots and the same startup original, rather than replacing a project root or fabricating a grant.

`tsk-e35c` adds one Linux-qualified integration fixture, [actual_mcp_cancelled_child_descendants_and_cancelled_dependency_preserve_native_gates](../../crates/graphit-app/src/task_server/tests.rs). Current compile03 completed successfully before native01 executed this one exact HTTP/MCP/Auth/SDK case: **1 PASS**, 21.45s test runtime (21.52s runner). The complete five-Task tree exists before the owned child is cancelled; that child remains Cancelled with owner null, epoch1 and Pending checks. Parent completion is rejected before each required grandchild completes: MCP returns `invalid_native_input`, and public native `execute_write` under fresh Runtime Auth/Original/project capture confirms exact `Invalid("completion gates are pending")`; get/audit remain unchanged. This cause differs from structural `GateError`/`gate_blocked`. Parent release precedes each grandchild claim, preserving ActorBusy rules; real check verification and descendant completion precede parent completion. The cancelled child's explicit dependent remains Open/ready=false in the actual HTTP catalogue; HTTP/native claim rejects with exact `Invalid("task is not ready")` and unchanged get/audit. Creation/events, actor, contiguous revision/reference generation, head snapshots, private-token exclusion and owned shutdown joins passed. Prior compile01 Data Box errors, compile02 locked-file guard and the Root Broker missing-module build remain preserved in the evidence; resolved producer Source allowed compile03. App cfg(test) Clippy (`--lib --tests --no-deps -- -D warnings`) passed exit0; formatting, scoped diff and the two documented contracts are included in the qualification recorded in Task `tsk-e35c` (historical local receipt `.build/rust-records-task-cancelled-child-mcp/validation.json`, unavailable in this checkout). This test-only proof covers this Rust TaskServer HTTP/MCP/SDK route; production gates and the external active-MCP scheduler `tsk-bc28` are outside its qualification, and prior producer counts remain separate.

## Verification boundary

Affected qualification uses a real Core-reserved socket, rmcp client, DaemonAuth, filesystem captures and the native Lance producer. It exercises payload operations beyond initialize/list, explicit calendar dates, dependencies, participant fences, stale revisions, HTTP typed cursors, read-only token absence, reopen, credential denial and managed-role revocation. Cancellation waits on the actual Core writer lease and verifies no corpus was published. Scope capture and original-deadline errors remain errors.

The historical component qualification does not cover shared Source dispatch or executable CLI composition. `tsk-b9b4` qualifies the Linux Root consumer integration described above; authentication authorities, Records codec/store and React retain their separate owners.

A newly created Task requires at least one acceptance check and one test check, as enforced by the native Task specification.

## Version selection and fork history

The CLI captures the existing Core checkout/version producers when an explicit project directory has no version override. SourceProxy supplies that effective version only to the advertised nested Task selection. Invalid or unretained tuples remain errors; selectors do not grant access.

Task CLI startup ensures Source service availability without maintaining or reselecting a checkout. If the live owner has replaced the captured tuple, the RPC returns `selection_unavailable`.

A retained version whose Lance branch is not materialized can read the captured default snapshot through the native Data producer. Its first Task write materializes the fork from that captured generation, retaining records, immutable history, claims and epochs. Subsequent publications remain isolated: a new Feature Task is absent from Main, and a later Main Task is absent from the already materialized Feature fork. Do not reinterpret inherited records as NoCorpus or reset their ownership. See [Lance branch delta producer](rust_lance_branch_store.md). G1 validates this through public Task operations; no Data implementation change is part of this consumer integration.

## Session specification revision

`graphit_task_session_revise` and HTTP `POST /api/tasks/native/revise_session` consume the same `selection/input` envelope and actual `CoordinateTask` admission. Required input is `target:{id,claim:{token,expected_revision}}`, positive `lease_ms` and canonical `reason`; optional `title`, `description`, `strategy` replace only supplied fields. Null, empty or padded replacements and a no-op are rejected. The common `references` patch is supported: omit preserves, `[]` clears, and reference-only change is valid. No actor, timestamp or scope can be supplied in the patch.

The result is `{kind:"session",result:Session}`. Specification revisions preserve the initial Creation, checkpoint progress and Task relationships. Default Session Get returns the current specification without the private fence; explicit selected history pages retain immutable Revised events and their reasons. The live owner/revision/lease is rechecked through publication; stale revision, foreign owner, missing coordinate authority, cancellation and terminal state retain native causal errors. Session search/FTS is not added. SDK tests and the actual authenticated MCP/HTTP consumer test are separate evidence boundaries; this component does not qualify CLI, installed host or model delivery.

Session revision SDK and authenticated consumer evidence, separate distinct counts and retained corrections are documented in [Native Session revision qualification](rust_task_protocol.md#native-session-revision-linux-qualification).

## Native Task CLI parity

On Linux, `graphit task` exposes the same 28 native operations as the MCP catalogue.
`graphit task revise-session --id ID --claim-token TOKEN --expected-revision REV --lease-ms MS --reason REASON --title TITLE` selects the existing `revise_session`
operation through the authenticated Source MCP connection. Its flags construct the native
Session revision object: `target:{id,claim:{token,expected_revision}}`, positive
`lease_ms`, canonical `reason`, and supplied `title`, `description` or `strategy`.
The CLI builds the `selection/input` envelope and retains the captured project/version;
selectors and ULIDs grant no authority. Omitted specification fields stay unchanged.
Stale revision and another target’s token are rejected without a new history event.

`tsk-678a` closes the CLI consumer and compiled Task instruction alignment after the SDK/MCP/HTTP delivery in `tsk-13af`. The executor follows one active Task per agent; the authenticated actor may hold independent claims on distinct targets. The current claim contract is described above.

The current Linux proof is three distinct cases: App25s passed CLI/catalogue parity
and the compiled Session revision journey; Runtime25t passed the existing full
skill fixture render, including the six changed Task snapshots. The four native
JSON examples across both namespaces are byte-identical to `tsk-13af` and retain
that earlier parser qualification. Compilation and scoped quality remain separate
from these functional counts in the receipt.


## Named CLI arguments

The flag-based CLI constructs the same native request without a whole-operation
JSON argument. The authoritative option list is the [CLI reference](../reference/cli-options.md);
[argument and cursor conventions](../guides/cli_reference.md#arguments-and-lists)
describe repeatable lists, explicit clears, typed values and opaque continuations.
The JSON examples in this document remain MCP/HTTP contracts.

The affected Linux qualification includes seven `task_entry` contract/parser
cases and four compiled `task_executable` journeys through Source/MCP/HTTP,
including pagination, revisions, history, project isolation and independent
claims. They use private fixtures without vendor executables, accounts or paid
plans. Current implementation and evidence are recorded by Task
`01M6444X27ZX536TPG5JB3NVSD`. This does not qualify native Windows/macOS execution.
