---
title: "MCP Tools Reference"
description: "Complete reference of all MCP tools available to AI agents through Graphit Code."
content-type: reference
audience: developers, ai-agents
updated: 2026-11-06
keywords:
  - mcp
  - tools
  - api
  - reference
  - ai agents
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/user_manual.md"
  - "docs/guides/troubleshooting.md"
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
  - target: {type: task, id: tsk-be6e}
    relation: reuses
  - target: {type: task, id: tsk-1b8b}
    relation: reuses
  - target: {type: task, id: tsk-abb4}
    relation: reuses
  - target: {type: task, id: tsk-4b26}
    relation: supports
  - target: {type: task, id: tsk-97d9}
    relation: supports
  - target: {type: task, id: tsk-9bd4}
    relation: supports
  - target: {type: task, id: tsk-a2bb}
    relation: supports
  - target: {type: task, id: tsk-6b5f}
    relation: supports
  - target: {type: task, id: tsk-9a3d}
    relation: supports
  - target: {type: task, id: tsk-c3c2}
    relation: supports
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-de72
    relation: supports
  - target:
      type: task
      id: tsk-a4ca
    relation: supports
  - target: {type: task, id: tsk-ced2}
    relation: relates_to
  - target: {type: task, id: tsk-3ddb}
    relation: relates_to
  - target: {type: task, id: tsk-ad05}
    relation: relates_to
  - target: {type: task, id: tsk-e493}
    relation: relates_to
  - target: {type: task, id: tsk-7a13}
    relation: relates_to
  - target: {type: task, id: tsk-c3bf}
    relation: relates_to
  - target: {type: task, id: tsk-0257}
    relation: relates_to
  - target: {type: task, id: tsk-47fd}
    relation: relates_to
  - target: {type: task, id: tsk-d75f}
    relation: relates_to
  - target: {type: task, id: tsk-b788}
    relation: relates_to
  - target: {type: task, id: tsk-4323}
    relation: relates_to
  - target: {type: task, id: tsk-376b}
    relation: relates_to
---

# MCP Tools Reference

This reference describes the Rust **Root** tools. The
[input schemas](../reference/mcp-input-schemas.json) contain nested required fields,
enums, nullability, bounds and `additionalProperties` rules. The snapshot is captured from a real authenticated SourceProxy `tools/list` fixture and the new public definitions are checked against their registered schemas. Regenerate it with the `root_and_stdio_announce_and_execute_actual_record_operations` library test and `GRAPHIT_UPDATE_MCP_SCHEMAS` set; that opt-in is never set by validation hooks. There is no
legacy flat-argument compatibility layer. Inspect `tools/list` on your actual endpoint when using
a different build, native binding or Role/Live credential.

## Connect and authenticate

A local host launches `graphit mcp --stdio` from its project working directory. Use the executable
on PATH and project-relative selectors; do not publish one machine's absolute installation path.
The SDK proxy obtains the installation's actual authenticated Source transport. stdout is only
MCP frames; stderr carries diagnostics. EOF joins this client without stopping the daemon.

The daemon serves Streamable HTTP at `/mcp`. Remote callers supply their admitted bearer; a
Broker incoming JWT retains its own identity and is never replaced by the active local account.
JWT validity is separate from PROJECT/version access. Runtime/Local/Broker/Role/Live credentials
are not interchangeable. `/roles/mcp` has a separate genuine Role grant and filtered catalog.
A public tool name or a cursor is not authority. See [authentication](authentication.md),
[Source](../specs/rust_source_mcp.md) and [Role MCP](../specs/rust_role_mcp.md).

## Output representation

Every structured tool accepts strict top-level `ai_optimized`:

| Input | Result |
|---|---|
| Omitted or `true` | TOON in text content, without duplicate complete JSON `structuredContent` |
| `false` | JSON; use this for machine consumers that read structured data |
| Null, number or string | Protocol validation failure, not a silently selected default |

JSON-RPC, arguments, negotiation and `tools/list` remain JSON. Variable-format tools do not
advertise an incompatible JSON `outputSchema`. Root AST Source and SessionHook remain original
text; ModuleSkill/KnowledgeSource are structured envelopes whose content strings remain intact.
CLI and HTTP JSON contracts do not change. [Output specification](../specs/output_module.md)
defines null/Unicode/escaping, numeric normalization, ordering and measured token trade-offs.
TOON is not guaranteed to be shorter for every result shape.

Inspect `isError` and the domain result. A protocol `INVALID_PARAMS`/unknown tool, a tool-domain
failure, a valid `not_found` and a confirmed mutation followed by response failure are different.
No blanket retry is safe after a timeout; inspect current state and reported effects first.

## Selection, pagination and cancellation

Task tools use `{selection,input}`; other domains have their own exact schema. Do not put every
field into a generic `input` or force every tool into flat arguments. `project_dir` is a Unicode
string resolved on this host. Prefer stable project ID/version where the operation supports it;
shared records should not persist a private checkout path. Imported contexts use exact authorized
artifact identities and their declared selectors, not arbitrary host paths.

In a project-bound Root stdio session, omit `project_dir` to use its checkout.
The Hub authored reads, FILE and native publication, and local installation also
allow omission in a direct daemon call when exactly one checkout/version is
active. With several active selections, provide the absolute path and specify
`project_version` if more than one version of that checkout is active. A direct
call that specifies a version for these tools also needs the directory.
Installed Hub content uses the same stdio binding in its PROJECT mode. Global
Hub targets, USER Memory and imported Knowledge use their own selectors.

Continue an opaque cursor with unchanged selection/query/page parameters. A page can be empty or
pinned to an older publication; only EOF proves traversal complete. Authentication and current
scope are checked again. Calls retain Original cancellation/deadline through inference, IO and
response delivery; closing a subscriber does not automatically cancel separately server-owned
Live work. Cancel with its explicit history/control contract.

## Examples

Model-free code search with machine-readable output:

```json
{"name":"graphit_ast_search","arguments":{"query":"checkout","mode":"fts","rerank":false,"ai_optimized":false}}
```

Known Source file, preserving original text:

```json
{"name":"graphit_ast_source","arguments":{"path":"Cargo.toml","head":20,"line_numbers":true}}
```

Knowledge search uses a nested search request and explicit mode:

```json
{"name":"graphit_knowledge_search","arguments":{"search":{"query":"configuration","mode":"fts","top_k":5,"page_size":5,"rerank":false},"ai_optimized":false}}
```

A current Task read uses the actual returned ID (the syntactically valid example ID below is fictional):

```json
{"name":"graphit_task_get","arguments":{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"},"ai_optimized":false}}
```

Task current reads do not automatically load history/comments. Use returned first-page cursors
and explicit sections through EOF when an audit is needed. The [Session guide](task-sessions.md)
explains private claim fences, expected revisions, owned writes and commit association.

## Current Root catalog

Required below means required at the top level only; the full JSON schema remains necessary for
nested fields, `oneOf` and omitted/null behavior. `ai_optimized` is omitted from the lists for
readability. Discovery metadata does not imply that the current caller can execute a tool.

### AST

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_ast_source` | `path` | Read Source from a retained local project or installed AST context id@version; imported fallback requires explicit project_dir. |
| `graphit_ast_export` | `output` | Export an admitted immutable AST graph to a native package file. |
| `graphit_ast_imports` | none | List exact installed Native AST contexts. |
| `graphit_ast_query` | `query` | Execute readonly Cypher over the selected PROJECT or exact installed context. |
| `graphit_ast_schema` | none | Read the selected graph schema. |
| `graphit_ast_sample` | none | Sample the selected graph nodes and edges. |
| `graphit_ast_neighborhood` | `anchor_label`, `relationship`, `other_label`, `anchor_key`, `direction`, `limit` | Read a cursor page around an exact graph anchor. |
| `graphit_ast_search` | `query` | Search indexed code with FTS, semantic, vector or hybrid retrieval. Selectors grant no access. A stale Source association is an error; missing, empty and disabled indexes are distinct. |

### Task session tools

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_task_get` | `input` | Read current Task state and explicitly requested snapshot-bound history/comment pages. |
| `graphit_task_list` | `input` | Read a cursor-pinned Task catalogue, including explicit dates and dependencies. |
| `graphit_task_query` | `input` | Read a finite projected Task or Session page from one native pin. |
| `graphit_task_schema` | none | Read the native Task query tables and permitted column types. |
| `graphit_task_search` | `input` | Search current Task specifications and comments using fts (default), semantic or hybrid retrieval; continue with the returned native cursor. |
| `graphit_task_session_get` | `input` | Read current Session state and explicitly requested snapshot-bound history pages. |
| `graphit_task_session_list` | none | Read native Session heads. |
| `graphit_task_create` | `input` | Create a native Task with acceptance and test checks, and explicit optional calendar dates. |
| `graphit_task_session_create` | `input` | Create a native Session. |
| `graphit_task_claim` | `input` | Claim a Task; return the opaque participant fence only to this caller. |
| `graphit_task_session_claim` | `input` | Claim a Session; return its opaque participant fence. |
| `graphit_task_heartbeat` | `input` | Renew a Task owner's actual lease and revision. |
| `graphit_task_session_heartbeat` | `input` | Renew a Session owner's actual lease and revision. |
| `graphit_task_comment_add` | `input` | Add a literal Markdown comment with the current participant fence and an idempotency key. |
| `graphit_task_progress` | `input` | Record the current Task owner's progress. |
| `graphit_task_session_checkpoint` | `input` | Record the current Session owner's checkpoint. |
| `graphit_task_release` | `input` | Release a Task using its current participant fence. |
| `graphit_task_session_release` | `input` | Release a Session using its current participant fence. |
| `graphit_task_commit_set` | `input` | Associate or clear a full Git commit ID using revision CAS; preserve status and checks, including completed tasks. Active claims require their fence. |
| `graphit_task_revise` | `input` | Revise Task specification; omitted fields stay, schedule null clears. |
| `graphit_task_session_revise` | `input` | Revise the coordinated Session specification; omitted fields stay and null is invalid. |
| `graphit_task_check` | `input` | Verify a current Task check with evidence. |
| `graphit_task_check_supersede` | `input` | Supersede a Task check, optionally creating a replacement. |
| `graphit_task_update_links` | `input` | Replace Task parent or dependency links with recorded reason. |
| `graphit_task_flag` | `input` | Set or clear the native Task flag with recorded reason. |
| `graphit_task_complete` | `input` | Complete a Task after its actual gates pass. |
| `graphit_task_cancel` | `input` | Cancel an eligible Task with recorded reason and live fence when owned. |
| `graphit_task_session_complete` | `input` | Complete a Session after its native Task gates pass. |

### Memory

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_memory_search` | `text`, `top_k` | Search PROJECT Memory with FTS (default), semantic distance, or hybrid RRF. Semantic modes use the existing model and never fall back to FTS. Selectors grant no access. |
| `graphit_memory_index` | none | Ensure Memory lexical indexes, or explicitly refresh them, and maintain embeddings when configured. PROJECT selectors grant no access. USER selects the private local installation or the verified Native account and rejects PROJECT selectors. Confirmed lexical and embedding effects survive later failures. |

### Knowledge

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_knowledge_id` | `path` | Calculate the deterministic default document ID with the publisher's KnowledgeSourceIdentity factory. The relative Unicode path is data; it is not opened. No corpus, model, provider or write is needed. Requires authenticated Runtime/Local Read(Knowledge); the result grants no access. |
| `graphit_knowledge_search` | `search` | Find Knowledge pages in an authorized project or installed artifact. Results contain a slug, title and optional Markdown preview. Use the returned cursor for the next page. |
| `graphit_knowledge_export` | `output` | Export authorized PROJECT or installed Knowledge as an installable native package or a Markdown, OKF or Obsidian directory. Requires explicit filesystem publication permission. Output must be an absolute NativePath outside the datasets; selectors grant no access. |
| `graphit_knowledge_live_history` | `operation` | Read or control Live history owned by the authenticated installation. Execution is independently owned; disconnecting a reader does not cancel a run. |
| `graphit_knowledge_ask` | `query`, `model` | Answer a question from explicitly selected slugs or ranked retrieval in one authorized PROJECT Knowledge corpus using the startup Antigravity, Claude, Codex, Cursor, Grok, Qwen, Kimi, Copilot, Kiro or OpenCode completion provider. Progress contains observed native events; provided slugs and requested model do not confirm model citations or execution identity. |

For a new page without an explicit frontmatter `id`, agents call the MCP utility:

```json
{"name":"graphit_knowledge_id","arguments":{"path":"docs/guides/new page.md","ai_optimized":false}}
```

Its JSON result is exactly `{"id":"<26-character canonical ULID>"}`. Omitted or true
`ai_optimized` returns TOON through the normal MCP formatter. This is a deterministic
128-bit identity derived by the publisher from its canonical relative source origin;
it does not encode the page creation time. Repeated calls with the same native
relative path return the same ID. Use the path supplied to the publisher, including
its native separators; Unicode and spaces are preserved. Absolute paths, parent
traversal, NUL and policy limits fail. Nothing is created, opened, indexed or published
by the calculation, and no model/provider is consulted. An explicit frontmatter `id`
continues to take precedence in indexing; this utility calculates the default only.

Both endpoints derive the tool name from the validated Brand. Role keeps its normal envelope:
`{"input":{"path":"docs/guides/new page.md"},"ai_optimized":false}`. Role calls require the
existing PROJECT selection, run credential and Read(Knowledge) admission/rechecks;
Root uses native Runtime/Local Read(Knowledge). These authorization boundaries remain
in force independently of the pure path calculation. Agent templates use the dynamic
`mcp_tool_knowledge_id` variable rather than a machine-specific executable or path.

### Wiki source and history

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_wiki_log` | `history` | Read committed Knowledge document synchronization history for the authorized PROJECT and version. Native cursors grant no access; semantic-only transactions are excluded. |
| `graphit_wiki_source` | `slug` | Read a complete authorized Knowledge page as original Markdown or select a range of its original lines. Slug is the exact page identity. Use the project version or installed artifact to select the corpus. |

### Hub

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_hub_publish_native` | `publication` | Publish captured PROJECT AST or Knowledge SDK snapshots through the active Hub provider. PROJECT read and HubWrite authority are required separately. |
| `graphit_hub_publish_file` | `publication` | Publish a captured PROJECT FILE artifact through the configured HubWrite provider. Selects the maintained resource tree by type, full ID and concrete version. |
| `graphit_hub_install` | `selection`, `global` | Install an authorized remote FILE artifact with verified cache and explicit global or project scope. |
| `graphit_hub_install_local` | `source_dir`, `producer`, `artifact_type`, `id`, `version` | Install or reapply canonical local agent/skill/command/workflow instructions into an authorized PROJECT. The checkout receives a copy. Removal preserves the original directory. |
| `graphit_hub_update` | `global` | Update one installed FILE artifact, or omit selection to update the captured target's remote installations. Local authored entries are left unchanged; failures include per-item effects and unattempted items. |
| `graphit_hub_sync` | `global` | Reapply installed local and remote Hub artifacts from their recorded sources and exact versions; preserve aliases, modes and user changes. |
| `graphit_hub_content` | none | Read authorized installed FILE content without hydration, or explicitly fetch remote content into a temporary request-owned stage. |
| `graphit_hub_list` | `global` | List installed Hub metadata with explicit scope, version filters and pagination. Cache metadata grants no remote project access. |
| `graphit_hub_remove` | `artifact_type`, `scope`, `id` | Remove an exact local PROJECT target or a concrete GlobalCache/GlobalNative metadata version; no remote payload or storage access. |
| `graphit_hub_forget` | `artifact_type`, `scope`, `id` | Forget selected FILE PROJECT metadata and return the forgotten entry while preserving files; global scope is not accepted. |
| `graphit_hub_projects` | `transport`, `provider`, `limit` | Discover currently authorized remote Hub project metadata. Discovery does not grant artifact or payload access. |
| `graphit_hub_authored_list` | `query` | List authored Resources identities in an authorized retained PROJECT/version. Returns exact kind/id/version tuples and a bound pagination cursor; no provider or remote storage access. |
| `graphit_hub_authored_review` | `query` | Review one exact authored Resources tuple in an authorized PROJECT/version. Lists captured relative files and directories; no host paths or remote storage access. |
| `graphit_hub_authored_content` | `query` | Read a relative text file from one exact authored Resources tuple in an authorized PROJECT/version. No installed artifact or arbitrary host path fallback. |
| `graphit_hub_unpublish` | `provider`, `producer`, `artifact_type`, `id`, `target` | Retract an explicit remote Broker artifact version or all observed catalog versions. Payloads, source and local installations are preserved; no local PROJECT selection is required. |
| `graphit_hub_search` | `transport`, `provider`, `producer`, `limit` | List current artifact manifests of an explicit authorized remote PROJECT. Exact kind/id/version filters; no payload reads. |
| `graphit_hub_install_native` | `installation` | Install one exact remote NativeRoot under the authenticated installation; Runtime Native installation and HubRead authority are required separately. |

Local FILE failures retain the actual publication effect: `NotApplied`, `MayHaveApplied`
or `Applied`. Inspect metadata and files before explicitly running the normal operation
again. Files may already exist after a failed metadata save, so an ordinary retry can
conflict. Foreign files, including equal bytes, remain foreign; Forget preserves files.

### Shared administration and instructions

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_module_skill` | `module` | Read one core module skill or its exact framework reference. Omit project_dir for neutral Global guidance; explicit PROJECT and version select current configuration and observed overrides without granting access. |
| `graphit_sync` | none | Reconcile installed Hub artifacts at their recorded versions, native Graphit skills/agents, Coordinator, hooks and MCP. Administrative Runtime access required; user modifications are preserved as explicit conflicts. |
| `graphit_now` | `project_id`, `mode` | Read current local heads or the first authorized Broker full snapshot. Local requires an explicit retained version and rejects provider/instance_id; Broker requires a provider. |
| `graphit_instances` | `project_id`, `provider` | List currently online Broker instances and versions with their full Now heads. Selector DATA confers no access. |
| `graphit_session_hook` | `engine`, `event` | Render readonly PROJECT hook context and the current Native Coordinator mandatory USER selection at native context events. Supported quiet lifecycle events dispatch project rescan only with Configure permission and read no corpus. Event, version and native path are selectors; they grant no access. Unsupported host/event combinations are rejected. |

## Surface boundaries

The Root catalog currently exposes Memory search/index rather than historical Memory CRUD/source
names. Browser CRUD uses its verified HTTP actor and Records services. Session search, generic
Task batch/export and force-takeover are not implied tools. Native `graphit_task_commit_set` does
exist; host integrations with a stale catalog must reconnect before assuming it is unavailable.

`graphit_sync` is the authenticated reconciliation used by Init. With a project
selection, it reconciles global and project artifacts, skills and roles, then
publishes the project Coordinator, hooks and MCP configuration. The first
initialization of a checkout and user installation starts from the CLI.
`graphit_instances` and `graphit_now` describe actual instance/project/version activity rather than
a task-count estimate of delivery progress. The [CLI](cli_reference.md) exposes the supported
component operations; [capability matrix](capability_matrix.md) distinguishes HTTP/Browser and CLI.

## Implementation and validation

`mcp_output.rs` applies format defaults/validation at handlers; `source_server.rs`, `task_server.rs`,
`role_tool_server.rs` and `live_agent_tools.rs` retain typed values until the presentation boundary.
Internal JSON consumers request `ai_optimized:false` explicitly. The Source proxy preserves public
protocol codes while removing private upstream diagnostics, and local cancellation/deadline wins
over late remote failures. Existing stdio connections retain their loaded process/catalog until
reconnected; do not force-kill active clients to update documentation.

This catalog read does not qualify every mutation, platform or external agent plan. Functional
receipts remain in the corresponding contracts. Agent integration tests use controlled documented
protocol peers and do not require vendor executables, accounts or paid plans.


## Explicit Session cancellation

`graphit_task_session_cancel` (`graphit task cancel-session`) requires
`CoordinateTask` authority, `id`, `expected_revision`, and a canonical nonempty
`reason`. An open Session omits `claim`; an in-progress Session supplies its real
live coordinator `claim: {token, expected_revision}` matching the record CAS.
Every linked Task must already be completed or cancelled in the same writer pin.
Resolve pending Tasks individually; cancellation never cascades.

The transition publishes `cancelled` and an immutable `Cancelled { reason }`
event. It clears owner, lease and private fence, preserving claim epoch,
specification, creation and progress. Authored references are preserved unless
explicitly patched. It never marks the demand completed or reopens a terminal
Session. A retry at the cancelled record's current revision with the same reason
and no claim/references is a no-op without a new event. Stale revisions, another
reason and completed Sessions fail. Delegated Role catalogues remain restricted;
this is a Root coordinator and human CLI operation.

```sh
graphit task cancel-session --id SESSION_ID --expected-revision 4 --reason "Demand withdrawn"
# For an in-progress Session, also supply --claim-token with its current fence.
# Add --json for structured output; the default terminal result is text.
```


## Local project discovery

`graphit projects list` and `graphit projects show --project-id ID` (or
`--name NAME`) use the live authenticated service's `graphit_projects_list/get`.
They read the maintained local inventory, independently of the remote Hub.
`list` filters by `--name` (substring), `--project-id`, `--version` and
`--online[=true|false]`. Use `--label-key KEY` to require a label and add
`--label-value VALUE` to require an exact value. `--page-size` defaults to 25. Continue using the exact
`--cursor` returned, with unchanged filters. If the authorized observation
changes, the cursor fails as expired instead of silently changing pages.
`show` matches an exact ID or unique exact name. Unknown and unauthorized IDs
share a single not-found-or-denied result.

Each entry includes `description` and the `cluster` map of label keys to values.
It distinguishes historical `registered`, retained `versions`, and
checkouts with `registered_version`, actual observed `exists` (null when
filesystem observation fails), daemon-local `online` membership and `current`
selection. Online does not describe Broker presence; use `graphit_instances`
for that independent journey. `--no-paths` omits local paths. Returned
`checkouts[].dir` is selection data and never grants module/content access.
Only native callers admitted for that PROJECT/version see it; installation
Configure administrators can inspect historical entries including retired
projects. Discovery never starts a project or adds access.

```sh
graphit projects list --name graphit --page-size 10
graphit projects show --name graphit-code
graphit projects list --online=false --no-paths --json
graphit projects list --label-key service --label-value api
```

Use `graphit projects label-set --project-id ID --key service --value api` to
add a value. `graphit projects label-unset --project-id ID --key service --value api`
removes that value. Omit `--value` from `label-unset` to remove the entire key.
These commands target the active checkout in the current project by default.
Use `--project-dir DIR` to select a different checkout and `--version VERSION`
when that checkout has multiple active versions. The supplied ID must match.

Root MCP tools `graphit_projects_labels_set` and `graphit_projects_labels_unset`
take `project_id`, exact `project_dir`, `key`, optional `project_version` and
`value` (required for set). For MCP discovery use `label_key` and `label_value`.
Keys and values are case-sensitive after trimming surrounding whitespace.
Labels require current administrative access to the selected checkout. A repeat
with the same intent is safe. If the command reports `inventory_refresh_failed`,
the checkout document and inventory may temporarily differ. Inspect the reported
`document_changed` field and repeat the operation after resolving the failure.


## Published Knowledge catalogue and lint

`graphit knowledge list` (`graphit_knowledge_list`) reads document metadata
without a textual search. Filters are exact `--kind`, `--source-prefix` over
the retained origin locator, `--stale[=true|false]` for persisted markers and
`--metadata-errors[=true|false]` for missing required effective kind metadata.
Use `--page-size` (default 25) and the unchanged JSON `--cursor` with the same
filters. Pages expose the actual generation; a changed generation or filter
expires the cursor explicitly. `state: no_corpus` differs from a published
page containing zero documents. Selection remains an admitted PROJECT/version.

`graphit knowledge lint` (`graphit_knowledge_lint`) validates the complete
published snapshot through the existing metadata and reference validators.
It reports missing metadata, broken references, orphan/short pages and stale
markers, plus age only with an explicit positive `--max-age-days`. It does not
inspect current authored files or repair/index documents. Each finding gives
a slug, retained source locator, field, severity and action. The published
metadata producer has no authored line provenance: `line` remains null and
`basis: published_metadata`; field locations are not invented source lines.
No partial report is returned if snapshot reading, limits or lint fail.

Lint exit codes: 0 for a complete published report with no issues (including
published-empty), 1 for findings or operational failure, 2 for CLI usage errors,
3 for an absent corpus. Default output is human-readable; use `--json` for CI.
Root exposes these read operations; Memory/Task Role mutation restrictions are
unchanged. Role Knowledge list continues its existing authorized head listing.

```sh
graphit knowledge list --kind guide --page-size 20
graphit knowledge list --stale --json
graphit knowledge lint --max-age-days 90 --json
```


## Incoming and outgoing references

`graphit references task --task TASK_ID` (or `--session SESSION_ID`) queries
Task/Session edges in the selected native Task corpus. `graphit references
knowledge --outgoing SLUG` queries document edges; add `--target-kind`,
`--target-id` and exactly one `--target-project-id`/`--target-user-id` to query
incoming references to a logical target. `--target-context` preserves an
imported identity. The Root MCP read tools are `graphit_task_references` and
`graphit_knowledge_references`, using the same producer contracts as Role tools.

Read permission applies to the origin corpus. Target IDs, scopes, contexts and
recorded relationships are data; they never open a foreign corpus or grant
content access. Outgoing and incoming pages have independent native opaque
JSON cursors (`--outgoing-cursor`, `--incoming-cursor`), actual generation
bindings and `--page-size` (default 20). Continue each direction until its own
EOF. An expired/invalid cursor is a failure, not an empty page.

```sh
graphit references task --task TASK_ID --json
graphit references knowledge --outgoing GUIDE_SLUG
graphit references knowledge --target-kind task --target-id TASK_ID --target-project-id PROJECT_ID
```


## Explicit index operations

`graphit index status` (`graphit_index_status`) reads the authorized selection's
current producer observations. `graphit index run` (`graphit_index_run`) requires
Configure authority and requests the existing supervisor's coalesced full-project
rescan. It neither creates an indexer/scheduler nor clears corpora/restarts the
daemon. The current administrative handle supports project scope; separate
module selection follows that module’s diagnostics while execution remains project-wide; path requests are unsupported. Automatic indexing and hook
rescans remain unchanged.

Status separates AST graph/source, lexical maintenance and embeddings, and
Knowledge documents/lexical from embeddings. It distinguishes no corpus,
unobserved, pending, running, completed and failed where observed; embeddings
can be disabled, models unavailable or have no new publication. Observations
are runtime facts, not persistent historic telemetry. Completion timestamps
are recorded only at actual observed producer completion; absent fields stay
absent. Independent phase failures and last useful errors remain visible.

Run returns `accepted` and a request sequence; this is not processing completion.
`--wait-ms N` explicitly follows the correlated supervisor pass for a bounded
period. `processed` means that pass returned; its phase errors can still report
a failed outcome. Timeout exits 3 and preserves the accepted receipt. Ctrl-C
cancels the waiter, not the shared producer or an accepted hint; a stopped
supervisor may discard an accepted request. No global corpus transaction or
percentage progress is promised. Default output is text; `--json` is explicit.

```sh
graphit index status
graphit index run
graphit index run --wait-ms 10000 --json
```

Task-origin backlinks also support any logical target (Task, Session, Memory or Knowledge), including foreign project/USER identity and imported context. `graphit references task-backlinks --target-kind knowledge --target-id guide --target-project-id PROJECT --target-context package@1 --page-size 20` calls `graphit_task_references` with `input.target`, `page_size` and an optional native `cursor`. Focus mode retains separate incoming/outgoing cursors. Both modes read the admitted source Task corpus without opening the target.

`graphit index run --module ast` (or `knowledge`; MCP `input.module`) identifies the module whose diagnostics are followed. The existing supervisor coalesces this into a full-project pass, explicitly returned as `scheduled_scope: project` with `requested_module`; other enabled modules may also process. Omission requests the project. Narrow path execution is not supported by this supervisor and is not advertised.
