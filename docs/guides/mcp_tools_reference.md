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
enums, nullability, bounds and `additionalProperties` rules. The original 67-tool snapshot
was captured from an isolated `graphit mcp --stdio` client on 2026-10-29. The added
`graphit_knowledge_id` entry is checked against the real SDK catalog in its focused tests. There is no
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
| `graphit_memory_index` | none | Ensure Memory lexical indexes, or explicitly refresh them, and maintain embeddings when configured. PROJECT selectors grant no access. USER requires the verified Native subject and rejects PROJECT selectors. Confirmed lexical and embedding effects survive later failures. |

### Knowledge

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_knowledge_id` | `path` | Calculate the deterministic default document ID with the publisher's KnowledgeSourceIdentity factory. The relative Unicode path is data; it is not opened. No corpus, model, provider or write is needed. Requires authenticated Runtime/Local Read(Knowledge); the result grants no access. |
| `graphit_knowledge_search` | `search` | Search an authorized PROJECT or installed Knowledge corpus in fts, semantic or hybrid mode, using configured embeddings only when the captured index is ready. Results retain native namespace, generation, opaque cursor and optional Markdown preview. Selectors and cursors grant no access. |
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
| `graphit_wiki_source` | `slug` | Read a complete authorized Knowledge page as native metadata and literal Markdown, or an explicit Core slice. Slug is the exact page identity, never a filesystem path. PROJECT/version and installed-artifact selections grant no access; output limits fail instead of truncating. |

### Hub

| Tool | Required root fields | Purpose |
|---|---|---|
| `graphit_hub_publish_native` | `publication`, `project_dir` | Publish captured PROJECT AST or Knowledge SDK snapshots through the active Hub provider. PROJECT read and HubWrite authority are required separately. |
| `graphit_hub_publish_file` | `publication`, `project_dir` | Publish a captured PROJECT FILE artifact through the configured HubWrite provider. Selects the maintained resource tree by type, full ID and concrete version. |
| `graphit_hub_install` | `selection`, `global` | Install an authorized remote FILE artifact with verified cache and explicit global or project scope. |
| `graphit_hub_install_local` | `source_dir`, `producer`, `artifact_type`, `id`, `version`, `project_dir` | Install or reapply canonical local agent/skill/command/workflow instructions into an authorized PROJECT. Source provenance is separate from the physical copy/link mode; removal preserves the original directory. |
| `graphit_hub_update` | `global` | Update one installed FILE artifact, or omit selection to update the captured target's remote installations. Local authored entries are left unchanged; failures include per-item effects and unattempted items. |
| `graphit_hub_sync` | `global` | Reapply installed local and remote Hub artifacts from their recorded sources and exact versions; preserve aliases, modes and user changes. |
| `graphit_hub_content` | none | Read authorized installed FILE content without hydration, or explicitly fetch remote content into a temporary request-owned stage. |
| `graphit_hub_list` | `global` | List installed Hub metadata with explicit scope, version filters and pagination. Cache metadata grants no remote project access. |
| `graphit_hub_remove` | `artifact_type`, `scope`, `id` | Remove an exact local PROJECT target or a concrete GlobalCache/GlobalNative metadata version; no remote payload or storage access. |
| `graphit_hub_recover` | `artifact_type`, `scope`, `id` | Recover the selected FILE PROJECT pending operation; global cache/native metadata cannot enter a physical journal. |
| `graphit_hub_forget` | `artifact_type`, `scope`, `id` | Forget same-key FILE PROJECT metadata/pending state and preserve physical leftovers; global scope is not accepted. |
| `graphit_hub_projects` | `transport`, `provider`, `limit` | Discover currently authorized remote Hub project metadata. Discovery does not grant artifact or payload access. |
| `graphit_hub_authored_list` | `project_dir`, `query` | List authored Resources identities in an authorized retained PROJECT/version. Returns exact kind/id/version tuples and a bound pagination cursor; no provider or remote storage access. |
| `graphit_hub_authored_review` | `project_dir`, `query` | Review one exact authored Resources tuple in an authorized PROJECT/version. Lists captured relative files and directories; no host paths or remote storage access. |
| `graphit_hub_authored_content` | `project_dir`, `query` | Read a relative text file from one exact authored Resources tuple in an authorized PROJECT/version. No installed artifact or arbitrary host path fallback. |
| `graphit_hub_unpublish` | `provider`, `producer`, `artifact_type`, `id`, `target` | Retract an explicit remote Broker artifact version or all observed catalog versions. Payloads, source and local installations are preserved; no local PROJECT selection is required. |
| `graphit_hub_search` | `transport`, `provider`, `producer`, `limit` | List current artifact manifests of an explicit authorized remote PROJECT. Exact kind/id/version filters; no payload reads. |
| `graphit_hub_install_native` | `installation` | Install one exact remote NativeRoot under the authenticated installation; Runtime Native installation and HubRead authority are required separately. |

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

`graphit_sync` reconciles existing artifact versions and agent files with administrative Runtime
admission; it does not upgrade remote releases, force index completion or grant Role writes.
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
