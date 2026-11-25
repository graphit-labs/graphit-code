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

Read [recall and decided persistence cases](references/durable-context.md) when a new question, correction or durable finding needs guidance; reuse the loaded reference afterward.

{% include "shared/persistence-boundary.md" %}

Reuse mandatory Memory delivered by hooks. Root advertises `{{ mcp_tool_memory_search }}` for PROJECT recall and `{{ mcp_tool_memory_index }}` for explicit administrative index maintenance; check the current catalogue before use. Missing USER identity means USER Memory was not read, never an empty personal store.

Use focused search with `text`, `top_k`, supported `filter` fields (classification, tag, mandatory, important, exclude_mandatory), optional `mode` fts/semantic/hybrid and selected `project_dir`/`project_version`. Top-level `ai_optimized: false` requests JSON. Returned hits are evidence only to the extent of their actual supplied content. Selectors grant no access. Discover a neighbor through `{{ mcp_tool_projects_list }}` only when announced and use authorized `checkouts[].dir`.

List/source/history/create/update/delete and mandatory/important flags belong to authorized Role tools or human Native terminal administration. If not announced on this transport, record the exact missing capability and route decided persistence to an authorized Role; never fabricate Role credentials or substitute the CLI for an unavailable MCP mutation. Read the Role version of this skill for its actual envelopes.

Update an existing subject to preserve identity/history, create only genuinely new durable guidance and delete duplicates only after preserving all distinct constraints. Mark standing necessary context mandatory, consequential conditional context important. Task owns transient investigation and progress. Successful writes acknowledge their native effect; never routinely sync/reindex afterward. Index repair uses the advertised operation; generic storage query/schema, imported Memory sync and destructive context removal are not Root recall operations.

{% include "shared/references/agent-contract.md" %}
{% include "shared/references/agent-example.md" %}
{% include "shared/references/query-contract.md" %}
{%- endif %}
