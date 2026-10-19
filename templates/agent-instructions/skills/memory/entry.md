{% if tool_transport | default("root") == "role" -%}
# {{ brand_display }} Memory for an authenticated Role

Recall durable constraints and confirmed context that affect this run. Task holds current progress; Knowledge holds maintained documentation. Tools use the closed `{selection?, input, ai_optimized?}` envelope and the run's authorized PROJECT/version. This interface does not accept USER scope, arbitrary project_dir or query fields. Top-level `ai_optimized` is an optional strict boolean: omitted/`true` returns TOON text without duplicate JSON `structuredContent`; `false` selects JSON. A selector does not grant another PROJECT.

{% include "shared/persistence-boundary.md" %}

## Recover only missing context

Reuse mandatory Memory already delivered by the hook. If that context is absent, request `{{ mcp_tool_memory_mandatory }}` once for the missing PROJECT context:

```json
{"input":{}}
```

A new question can need another focused recall during the same session. Use `{{ mcp_tool_memory_search }}`; exclude_mandatory belongs inside filter, and the search text belongs in text:

```json
{"input":{"text":"webhook replay","filter":{"exclude_mandatory":true},"top_k":5}}
```

Titles are candidates, not evidence. Read selected IDs with `{{ mcp_tool_memory_source }}`. Replace the illustrative ID below with one returned by the actual search. Core selectors are nested under selectors:

```json
{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","selectors":{"head":20,"line_numbers":true}}}
```

An unknown ID is absent; it does not authorize another namespace. Native no-corpus, absent, deleted, index failure and denied access are distinct outcomes. Retain the current revision before relying on or changing an entry. Use list only for an intended inventory, not after every recall or write.

## Capture only authorized durable findings

Read [the Role recall and capture companion](references/durable-context.md) before an unfamiliar correction or merge. Request it through `{{ mcp_tool_module_skill }}`:

```json
{"input":{"module":"memory","reference":"references/durable-context.md"}}
```

Read(Memory) permits retrieval. Only a run actually granted WriteMemory and announcing the corresponding tools may insert, update, mark or delete. A Scout/Tracker reports a reusable finding to its coordinator; it does not invent write authority. A Scribe transcribes only decided, authorized content. Use the advertised idempotency and expected_revision fields for writes; a tool name or example is not a grant.

Preserve user conditions, rationale and provenance. Update a matching subject rather than leaving contradictory current entries. Keep speculation and one-off progress in the owner's Task handoff. A successful write is durable; do not routinely search/list/sync afterward. Before deleting a duplicate, verify that every distinct constraint survived the merge. Never remove unique knowledge to reduce a count.

Index repair, sync, reset, USER Memory and coordinator Task lifecycle are not Role recall tools. Report unavailable persistence and the exact continuation to the coordinator; do not substitute a Runtime credential, another PROJECT or an unannounced CLI.
{%- else -%}
# {{ brand_display }} Memory

`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host. Stores follow the checked-out project version (Git branch; `main` without Git); optional `project_version` reaches another.

{% include "shared/persistence-boundary.md" %}

Structured MCP calls accept top-level `ai_optimized`: omitted/`true` selects TOON text without duplicate JSON `structuredContent`; `false` selects JSON for machine consumers. Keep the domain payload in its advertised envelope.

Preserve preferences, corrections, standing guidance, project facts and confirmed non-obvious knowledge that affect future work. Use {{ brand_display }}, not native/model memory. Task holds investigation/progress; Knowledge holds maintained docs.

{% include "shared/references/agent-contract.md" %}
{% include "shared/references/agent-example.md" %}
{% include "shared/references/query-contract.md" %}

## Recall whenever a question needs context

Hooks load mandatory project/user memories. Reuse them; call `{{ mcp_tool_memory_mandatory }}` only if the hook reports fallback, mandatory context is absent, or project/scope changes. In fallback, call once per missing scope (`scope: project` and `scope: user` for a project session); the default fetch covers only project scope. Use only user scope on an artifact-only server, without inventing `project_dir`. After compaction recover only missing context.

At any stage—questions, exploration, implementation, debugging or review—when retained context cannot explain the system, a decision or a surprising result, use `{{ mcp_tool_memory_search }}` with a focused query, `exclude_mandatory: true`, `top_k: 5` and `ai_optimized: true`. Do not wait for a new session, plan or blocker. Known ID: read directly. Search returns titles/IDs; read selected entries with `{{ mcp_tool_memory_source }}`. Use `preview: true` to disambiguate; expand only for remaining gaps. Reuse sufficient context; a new question can need a new search in the same scope. Read a superseded entry's `current` version before relying on it.

Use the questioned project's known/cluster-returned `dir` as `project_dir` with project scope, including neighbors; user scope is for cross-project preferences. If unresolved, read Hub and resolve with `{{ mcp_tool_cluster_projects }}` before its catalog; never guess paths or read stores as files. Installed AST/Knowledge contexts provide no project memories. For missing context, refine once; use `{{ mcp_tool_memory_list }}` for an intended inventory or suspected empty store, not routine recall or post-write verification.

## Capture at the first durable finding

For the first unfamiliar mid-work recall, correction, duplicate merge or durable discovery, read [recall and capture cases](references/durable-context.md); reuse thereafter. If the local resource is unavailable, call `{{ mcp_tool_module_skill }}` with `module: memory`, `reference: references/durable-context.md` and the known `project_dir` when available.

Capture user facts about lifecycle/environment, policies, conventions or standing instructions even without “remember this”. Preserve its rationale and conditions when they affect future planning, compatibility, migration, review or risk. Also capture confirmed structural, reusable or costly discoveries: implicit contracts, sources of truth/generation flows, coupling, recurring failure modes, trade-offs and learned procedures. Capture now, not at repetition or task end.

Before writing, reuse an existing matching entry already recalled; otherwise make one focused subject search. If the same fact is present and current, do not write it again. Use `{{ mcp_tool_memory_update }}` for an existing subject so its ID/history survives; use `{{ mcp_tool_memory_insert }}` when genuinely new. Include the concrete conclusion, scope/conditions, rationale and a compact source reference. Classify lifecycle/environment state as `fact`, standing choices as `decision`/`convention`, and corrections as `correction`. Task-local instructions, transient progress, speculation, obvious code facts and one-off results without future value stay out of Memory.

Mark standing context needed in every session `mandatory`; use `important` for consequential but conditional knowledge. Use `{{ mcp_tool_memory_mark_mandatory }}`/`{{ mcp_tool_memory_unmark_mandatory }}` or `{{ mcp_tool_memory_promote }}`/`{{ mcp_tool_memory_demote }}` only when recall needs change. Never demote, shorten away conditions, or delete critical constraints merely to reduce tokens. `{{ mcp_tool_memory_important }}` is for intentional review of promoted entries, not a second mandatory-memory fetch.

## Reconcile without losing knowledge

On contradiction, preserve provenance and conditions, then update the current entry using confirmed newer evidence; do not silently replace a user constraint with an inference. For duplicates, merge every distinct fact and constraint into the survivor, read that entry once to verify preservation, then `{{ mcp_tool_memory_delete }}` the redundant entry. Preserve unique knowledge; leave unrelated memories alone.

A successful write is durable and refreshes its own search indexes: trust the acknowledgement; do not list/search/sync after each write. Read back only if the response is ambiguous or verification is needed before removing a duplicate. Use `{{ mcp_tool_memory_index }}` to repair indexes, `{{ mcp_tool_memory_schema }}` for graph diagnostics, and `{{ mcp_tool_memory_sync }}` to refresh an imported authoritative context. `{{ mcp_tool_memory_remove }}` is destructive context maintenance, never a recall repair.

Keep investigation/next action in Task and the reusable conclusion in Memory. If Memory MCP is unavailable, state that persistence was unavailable and retain the conclusion in the available task handoff; do not claim it was saved or substitute the {{ brand_display }} CLI.
{%- endif %}
