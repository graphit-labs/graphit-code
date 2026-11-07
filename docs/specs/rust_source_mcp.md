---
title: Rust authenticated Source HTTP MCP component
doc_type: technical
references:
  - target: {type: task, id: tsk-1b8b}
    relation: implements
  - target: {type: task, id: tsk-a5f9}
    relation: derived_from
  - target: {type: task, id: tsk-60a8}
    relation: derived_from
  - target: {type: task, id: tsk-2436}
    relation: derived_from
  - target: {type: task, id: tsk-7740}
    relation: implements
  - target: {type: task, id: tsk-db46}
    relation: implements
---

# Authenticated Source HTTP MCP component

`graphit-app::source_server::SourceServer` serves `graphit_ast_source` through
rmcp 3.5.1 and Axum 0.8.9 at `/mcp`. The component accepts the actual Core
`BoundListener`; it requires a loopback address and transfers the same reserved
listener into Tokio without a bind probe. The SDK owns JSON-RPC, HTTP framing,
protocol negotiation, request cancellation and legacy session cleanup.

The caller supplies one shared `Arc<DaemonAuth>`, an `Arc<Vec<(ProjectPaths,
ProjectId, VersionContext)>>` of already resolved runtime entries, an explicit
current entry, service cancellation, and `SourcePolicy`. Policy contains existing
Core/Hub limits, a finite operation timeout, the actual native binding and one
`HubRegistry` for the actual installation owner. ForegroundSource constructs it
from the installation capture and effective Hub policy without loading metadata
at startup. The
shared DaemonAuth startup receives the actual trusted checkout `ProjectPaths`;
native Source fixtures pass those same paths before retaining their versions.
The component never discovers credentials, bootstraps a project, queries Git or
changes project identities during a Source request. This is a library component;
compiled CLI discovery and managed role credentials remain separate consumers;
registered Native entity resolution uses the captured graph/raw composition
described in [entity Source](rust_ast_entity_source.md). Local project entity
resolution still requires its real graph producer.

## Incoming requests and output

Every call carries `Authorization: Bearer <credential>` and
`x-graphit-auth-origin: runtime` or `local`. The handler reads that call's HTTP
Parts. Initialization does not cache authentication. Managed/unknown origins or
missing bearer credentials fail explicitly. Client consumers should mark bearer
HeaderValue sensitive rather than place secrets in a Debug-visible SDK String.

The tool requires a nonempty relative `path`. Integer selectors are `head`,
`tail`, `start_line`, `end_line`, `before` and `after`; `pattern`, `regex` and
`line_numbers` use their Core semantics. The listed schema and `get_tool` schema
are identical; typed serde decoding rejects null, wrong types, unknown fields
and coercions. The SDK schema lookup validates MCP parameter/header metadata;
it does not replace Graphit's typed argument validation.

`project_dir` and `project_version` select existing runtime entries. For Source
and Knowledge log/source/search/export reads, an admitted directory identifies
its PROJECT: an explicit version may select an existing captured tuple in another
checkout of the same PROJECT and storage namespace. Exact root/version wins;
otherwise multiple matching tuples are an error. This neither constructs a
version nor grants access: fresh authorization and exact live-membership checks
still apply to the selected tuple, including retirement. Physical installation
and other mutations keep exact root/version selection. Omitting
both uses the supplied current entry, never the server's current directory.
Unknown or ambiguous entries, empty selectors and invalid paths fail. A version
selects only among retained facts; incoming strings provide no project authority.
`entity` or `entity_key` with `context` resolves the selected registered Native
graph and its associated indexed Source. Name selection optionally filters by
`entity_type`; key selection requires that exact label. `entity_key` is a closed
object such as `{"type":"int64","value":"9007199254740993"}` or
`{"type":"string","value":"selected-uid"}`. It preserves STRING/INT64 types
without numeric coercion. Both selectors together, missing labels for keys,
invalid INT64 values and empty names/types are invalid. Unknown, unavailable or
ambiguous entities return domain errors. The resolved node's path must equal
`path`. Local PROJECT selection uses its current live graph and existing Raw/SHA
association. See [entity Source](rust_ast_entity_source.md) for point lookup and
the literal CLI flags `--entity-key` and `--entity-key-type`.

`context` selects an installed native AST artifact as its full namespace-qualified
`id@artifact-version`, for example `vendor/context@release-1`. The selected
installation must match the service binding's AST target, original payload and
exact concrete artifact version. SourceRef identities and paths remain metadata,
never authorization. Without project selectors, imported Source does not capture
or fall back to the current checkout. Physical fallback requires explicit
`project_dir` selecting a retained Runtime tuple; `project_version` may be omitted
only when that root has a single retained tuple. Version-only fallback and
ambiguous roots fail. Artifact version never becomes a project/Git version.

A selected native operation creates exactly one Hub RequestContext and original
Snapshot Arc. Local operations form ScopeInput from inline project capture and
the retained version under the same DaemonAuth installation. Capture is metadata
observation, not a grant. Local operations call the actual AST `inspect_source_runtime` or
`inspect_source_local` composer once; imported operations call
`inspect_imported_source_runtime` or `inspect_imported_source_local` once with
that same request and installation. Project capture exists only for a local
operation or explicit imported fallback. The composer authenticates and authorizes
ReadAst before registry/payload acquisition and again before delivery; App does
not add another authorization pipeline. Imported entity operations use the same
State/request in `open_imported_graph_runtime` or `open_imported_graph_local` and
`ImportedGraph::entity_source`; receiver authorization and capture observations
remain live through selection and delivery. Imported raw corpus capture and a fixed
ModuleSnapshot follow the [imported Source contract](rust_imported_source.md).

Available indexed text, including an empty string, wins over physical content.
Only file-only requests with actual typed index/file absence or NULL may use the
requested physical FILE. An entity window requires indexed raw text associated
with its graph; unavailable raw text does not permit an unbound physical fallback.
Corruption, permission errors, schema mismatch, changed observations,
authentication failure, cancellation and deadline never become absence.
BOM/CRLF/NUL/Unicode and formatted selections are returned exactly as the Core
result's `source`. Empty and no-match Source are successful text results, without
an added explanatory message. Imported explicit misses without a supplied project
fallback return Source-unavailable errors; malformed registry, schema, SDK and
origin failures retain their separate categories. The lookup does not install,
download, repair or write metadata/corpora. Actual input-body and response-size limits are
selected through existing operator policy, without a quota framework.

Recognizable argument/domain errors use SDK `CallToolResult::error` with
`isError=true`. Unknown tools use the SDK protocol method-not-found boundary.
Public diagnostics retain the actual authentication/index/indexed/inspection/
operation category without embedding credentials or source content. Native error
values are preserved until category mapping; the MCP response does not expose
sensitive nested diagnostics.

## Cancellation and lifetime

Synchronous FILE acquisition/inspection runs on Tokio's existing blocking
executor. A service-owned JoinSet holds actual native jobs. The existing current
Tokio Handle drives the AST async future and Lance awaits; no runtime per request,
extra pool or detached native task is created. Only the selected project is
borrowed within an operation, and credentials are owned in Zeroizing storage
until it terminates.

The rmcp request token, service stop and finite operation deadline cancel only
that operation's Core Cancellation. Dropping its handler also cancels through a
small local guard. Sibling requests have independent cancellation. Completed
joins are drained and response delivery uses a oneshot; a disconnected caller
cannot retain a retry buffer of source text or project captures.

`shutdown` stops admissions, serializes concurrent shutdown callers and joins all
actual blocking operations. Aborting a blocking task is not a join. The caller
must await terminal service shutdown before removing DaemonAuth's runtime key
or dropping its singleton owner. `serve` also waits for SDK/Axum graceful shutdown
and native joins before returning.

For current stateless SDK protocols, disconnect terminates that request's stream.
Legacy resumable SSE disconnect alone is not a terminal cancellation proof:
clients use the SDK cancellation notification/session termination. The official
SDK session manager remains responsible for supported stateful MCP negotiation; this does not restore a Kiro V2 binding. No
Graphit legacy parser, directory conversion or native config mutation is added.

## Affected qualification

Native tests use real Core private installation and runtime-key ownership,
DaemonAuth/AuthStore local profile publication/replacement, runtime-selected
project facts, Arrow/Lance datasets and LanceBranchStore branch publication,
with actual rmcp clients over loopback. No always-Missing provider or fixture
grant replaces the producer. Lifecycle tests use a cfg(test)-only barrier around
the actual native closure to ensure cancellation and shutdown race an unfinished
job; the closure still executes real Auth/AST and no gate is in the product API.

Run the affected `source_server` integration target and the Source lifecycle
unit filter, scoped rustfmt and strict Clippy. This evidence covers Linux SDK
components. The imported consumer qualification below covers its compiled CLI
journey separately. Managed/entity routes, Windows/macOS hosts and installed Kiro
event execution remain outside this qualification.

The Main qualification on 2026-10-11 passed seven HTTP integration groups and
two lifecycle unit tests. The latter exercise both SDK request cancellation and
current stateless transport closure while real native operations are unfinished,
including a cancelled shutdown waiter and terminal owner release. Commands and
results are retained in
`.build/rust-source-http-fixtures-20261011/qualification.md`.

The imported consumer `7740` qualification on 2026-10-11 passed all eleven
`source_server` HTTP groups, three `ast_source` client groups, three `mcp_source`
proxy groups and seven `source_executable` compiled CLI groups. The four imported
HTTP groups exercise full namespace and exact version/target selection, raw text
with the current checkout removed, empty text, NULL and missing corpora, explicit
unique/ambiguous Runtime fallback, foreign producer refusal, real local/runtime
Auth before malformed registry access, and actual SDK/schema failure categories.
The imported proxy group proves no current project/version injection; the
compiled CLI group also preserves explicit `--project-version` for fallback and
keeps raw reads working after the current project capture becomes unavailable.
The two existing Source lifecycle unit tests passed separately. Fixtures publish
native metadata through the public HubWriter and create actual Lance corpora;
they do not qualify an archive import-to-registry producer, semantic AST dataset
restoration, installed Kiro hosts or other operating systems. Task `tsk-7740`
retains the commands, results and historical failed compilation receipts.
