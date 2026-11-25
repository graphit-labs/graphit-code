{% if tool_transport | default("root") == "role" -%}
# {{ brand_display }} Memory for an authenticated Role

Memory holds what future work in this project needs to know and cannot cheaply rediscover: confirmed decisions with their reasons, conventions, corrections and lessons. Task holds current progress; Knowledge holds maintained documentation. Tools use the closed `{selection?, input, ai_optimized?}` envelope and the run's authorized PROJECT/version. This interface does not accept USER scope, arbitrary project_dir or query fields. Top-level `ai_optimized` is an optional strict boolean: omitted/`true` returns TOON text without duplicate JSON `structuredContent`; `false` selects JSON. A selector does not grant another PROJECT.

{% include "shared/persistence-boundary.md" %}

## Recall only what is missing

Reuse mandatory Memory already delivered by the hook. If that context is absent, request `{{ mcp_tool_memory_mandatory }}` once for the missing PROJECT context:

```json
{"input":{}}
```

A new question can need another focused recall at any point in the same run. Use `{{ mcp_tool_memory_search }}` (lexical); `exclude_mandatory` belongs inside `filter` and the search text in `text`:

```json
{"input":{"text":"webhook replay","filter":{"exclude_mandatory":true},"top_k":5}}
```

Titles are candidates, not evidence. Read selected IDs with `{{ mcp_tool_memory_source }}` (selectors nested under `selectors`), replacing the illustrative ID with one the search returned:

```json
{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","selectors":{"head":20,"line_numbers":true}}}
```

`{{ mcp_tool_memory_get }}` reads the current record with its revision, and `{{ mcp_tool_memory_history }}` its immutable history. `{{ mcp_tool_memory_list }}`, `{{ mcp_tool_memory_important }}` and `{{ mcp_tool_memory_mandatory }}` accept the same `classification`, `tag`, `mandatory` and `important` filters; use them for an intended inventory, not after every recall or write. An unknown ID is absent, not permission for another namespace. No-corpus, absent, deleted, index failure and denied access are distinct outcomes.

## Capture only authorized, decided findings

Read(Memory) permits retrieval. Only a run actually granted WriteMemory and announcing the write tools may insert, update, delete, mark/unmark mandatory or promote/demote importance. A Scout or Tracker reports a reusable finding to its coordinator; a Scribe transcribes only decided, authorized content. Read [the Role recall and capture companion](references/durable-context.md) before an unfamiliar correction or merge, fetching it through `{{ mcp_tool_module_skill }}` if it is not installed:

```json
{"input":{"module":"memory","reference":"references/durable-context.md"}}
```

One subject per record: search first, update the matching subject with its `expected_revision` rather than inserting a near-duplicate, and insert only a genuinely new subject. Pick the classification by meaning, covering episodic, semantic and procedural memory: `decision` (a choice taken, its reason and rejected alternatives), `fact` (verified behavior, domain concept or business rule), `convention` (a preference of the project: how things are done here), `skill` (a reusable procedure with exact steps), `correction` (a wrong approach and the right one) or `tension` (an unresolved trade-off to keep in view). When a brief carries several items, write each as its own record. Keep the user's conditions, the rationale and the provenance (Task, commit, page or source) in the body, and add typed `references` where a record supports it. Mark mandatory only context every session needs; mark important consequential conditional context. Speculation and one-off progress stay in the owner's Task handoff. Before deleting a duplicate, verify every distinct constraint survived the merge; never remove unique knowledge to reduce a count. A successful write is durable, so do not routinely search, list or sync afterward.

Index repair, sync, reset, USER Memory and coordinator Task lifecycle are not Role recall tools. Report unavailable persistence and the exact continuation to the coordinator; do not substitute a Runtime credential, another PROJECT or an unannounced CLI.
{%- else -%}
# {{ brand_display }} Memory

Memory holds what future work in this project needs to know and cannot cheaply rediscover: confirmed decisions with their reasons, conventions, corrections and lessons. Task holds current progress and evidence; Knowledge holds maintained documentation. Read [recall and decided persistence cases](references/durable-context.md) when a new question, correction or durable finding needs guidance, and reuse it afterward.

{% include "shared/persistence-boundary.md" %}

## Recall

Mandatory Memory arrives through the session hook: treat it as standing context and do not fetch it again. Recall again whenever a new doubt appears about the system, a rationale or learned behavior, at any point in the work and not only at session start. Root advertises `{{ mcp_tool_memory_search }}` for PROJECT recall:

```json
{"text":"webhook replay horizon","top_k":5,"filter":{"exclude_mandatory":true},"ai_optimized":false}
```

`text` and `top_k` are required. Optional `mode` is `fts` (default, model-free), `semantic` or `hybrid`; semantic modes need the configured embedding service and fail explicitly instead of falling back. `filter` accepts `classification` (`fact`, `skill`, `tension`, `decision`, `convention`, `correction`), `tag`, `mandatory`, `important` and `exclude_mandatory`; `include_deleted_history` widens to historical content. Pass `project_dir`/`project_version` only to select another checkout or version; selectors grant no access. Discover a neighbor through `{{ mcp_tool_projects_list }}` when announced and use its authorized `checkouts[].dir`. A missing USER identity means USER Memory was not read, never that the personal store is empty.

Returned hits are evidence only to the extent of their supplied content. Compare a recalled decision with current AST and Knowledge evidence before relying on it: memories can go stale.

## What to remember

Memory exists so the project stops relearning the same things. Think of it in three kinds:

| Kind | What it holds | Where it goes |
| --- | --- | --- |
| Episodic | Past requests, what was tried, outcomes and decisions taken in specific work | Task records it as you work (session description, checkpoints, comments of kind `decision`, `problem`, `lesson`). Promote a `decision` to Memory when it must be recalled outside that Task. |
| Semantic | Facts about the system, domain concepts, business rules, and the general preferences of the project and of the user | `fact` for verified behavior and business rules, `convention` for how the project wants things done (project preferences). Personal preferences of the user belong only in private USER memory. |
| Procedural | How to act: build, test, release and debugging recipes, workarounds, rules the agent must follow | `skill` for a reusable procedure, `correction` for a wrong approach and the right one; mark `mandatory` when every session must follow it. |
| Unresolved | Conflicting constraints and open trade-offs | `tension` |

## When to capture

Capture at the moment it happens; at the end of a long session the details and the reason are gone:
- The user corrects you, or says "always", "never", "prefer" or "don't" about the project → `correction` or `convention`.
- A decision is made or confirmed → `decision`, with the reason and the alternatives rejected.
- You discover something non-obvious: a root cause, hidden constraint, undocumented behavior, business rule or gotcha → `fact`.
- A procedure works after some trial (a command sequence, a debugging route, a workaround) → `skill`, with the exact steps.
- An approach fails for a reason future work would repeat → `correction`.
- Two valid constraints conflict and stay unresolved → `tension`.

Memory sweep: at every checkpoint, before a handoff, before closing a session and whenever the conversation grows long, ask what this work taught that the next session would otherwise rediscover, and capture each answer. A missed lesson costs more than an extra record: the Dream consolidation later merges duplicates, so when something is confirmed and plausibly reusable, save it. Skip only what the code, Git history or maintained documentation already state, transient progress, unconfirmed hypotheses and sensitive data.

## How to write from Root

Root has no Memory write, list or source tools; check the current catalogue before assuming otherwise. Note candidates as they arise (a Task comment of kind `lesson`, `decision` or `knowledge` keeps them safe), then at the sweep send one brief to the Scribe role listing every decided item: whether it updates an existing subject (with its ID) or is new, the title, a body with scope, rationale and provenance (Task, commit, page or source), the classification, tags, typed references, and whether it is mandatory or important. One subject per record; prefer updating an existing subject so its history survives. Never fabricate Role credentials or substitute the CLI for an unavailable MCP mutation.

Personal preferences of the user (language, response format, working style) have no agent write path: Role and Root Memory tools are PROJECT-only. Tell the engineer what you noticed and that `memory create --scope user` in the {{ brand_display }} CLI stores it privately; do not put it in PROJECT Memory.

## Maintenance

`{{ mcp_tool_memory_index }}` ensures or, with `refresh: true`, rebuilds Memory lexical indexes and maintains embeddings when configured; `scope` selects `project` or `user`. Use it for an explicit administrative repair, not after writes. Periodic consolidation of duplicates and conflicts is done by the Dream process, not by ad hoc deletes.

{% include "shared/references/agent-contract.md" %}
{% include "shared/references/agent-example.md" %}
{% include "shared/references/query-contract.md" %}
{%- endif %}
