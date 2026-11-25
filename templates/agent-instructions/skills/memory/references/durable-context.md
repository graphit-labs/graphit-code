{% if tool_transport | default("root") == "role" -%}
# Role recall, capture and reconciliation

Use the authenticated run's PROJECT/version and the closed input envelope. A source reference naming another namespace confers no access to it. USER scope and maintenance operations are not part of this Role interface.

## A question in the middle of work

A handler contains a replay horizon whose reason is missing from retained context. Reuse mandatory Memory already present, then one focused `{{ mcp_tool_memory_search }}`:

```json
{"input":{"text":"webhook replay","filter":{"exclude_mandatory":true},"top_k":5}}
```

Read a relevant returned ID through `{{ mcp_tool_memory_source }}` before relying on its title. Compare its conditions and provenance with current AST and Knowledge evidence. A new question may require a new recall; sufficient context requires no repeat. Report the finding and any unresolved difference to the owner of the current Task, without claiming or reopening it.

## Is it worth saving?

Save it when all of these hold: it is confirmed (by the user, a decision record, a test or the source), it will matter to future work beyond the current Task, and it is not already obvious from the code, Git history or maintained documentation. Otherwise leave it in the Task record. Corrections deserve particular care: when the user corrects an approach, the corrected rule and the reason for it prevent the same mistake in every later session.

## Update an existing subject first

Search for the subject before writing. When a matching record exists, read it with `{{ mcp_tool_memory_get }}` for its current revision and change it with `{{ mcp_tool_memory_update }}`: `input` carries `id`, `expected_revision`, `idempotency_key` and `reason`, plus only the fields that change (`title`, `body`, `classification`, `tags`, `mandatory`, `important`, `references`); omitted fields are preserved. This keeps one subject per record and preserves its history. A conflict means another write landed first: read the current entry and reconcile, never replay a stale revision. Do not claim a timeout rolled back a write; a write_confirmed response preserves the confirmed effect and its later delivery cause.

## Insert a genuinely new subject

Only a run with WriteMemory may call `{{ mcp_tool_memory_insert }}`. This fictional write demonstrates the schema, not a fact to save blindly; replace all example content with an actually confirmed, authorized conclusion:

```json
{"input":{"idempotency_key":"role-memory-example","title":"Webhook replay contract","body":"Preserve the confirmed webhook replay horizon and its provider retry conditions; cite the actual source and decision before saving this example as real knowledge.","classification":"decision","important":true,"tags":["webhook"]}}
```

The acknowledgement supplies the real ID and revision. Read the result back through `{{ mcp_tool_memory_source }}` only when you need to confirm it; the ID below is illustrative:

```json
{"input":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","selectors":{"head":20,"line_numbers":true}}}
```

Use `{{ mcp_tool_memory_mark_mandatory }}` or `{{ mcp_tool_memory_promote }}` (important) only when the subject warrants it, with the current revision and a reason.

## Preserve before reconciling

Retain every still-valid condition, including exceptions and why the rule exists. A newer observation is not automatically a replacement for a user constraint. Do not capture speculative conclusions, temporary blockers or routine test output as durable facts. When the same current fact is repeated, reuse it without writing again.

Before removing a duplicate, merge all distinct content into its survivor and verify that preservation once. If unsure, retain both and report the conflict, or record it as a `tension`. Do not delete or demote unique constraints to meet a token target. A normal successful write needs no routine readback/search/index operation.

If access, storage or persistence is unavailable, preserve the exact remaining action in the coordinator handoff and state what was not saved. Never widen scope, use USER as a fallback, borrow the Runtime key or repair an index through an unannounced operation.
{%- else -%}
# Root contextual recall and decided persistence

Reuse standing context supplied by hooks. A question during implementation can need focused recall even without a new session, plan or blocker. Root offers PROJECT search; it does not announce the Role source/list/write tools or a USER search fallback.

## A question in the middle of work

When a replay horizon lacks a retained explanation, call the advertised `{{ mcp_tool_memory_search }}` using the current project's resolved checkout only in the live envelope:

```json
{"text":"webhook replay horizon","filter":{"exclude_mandatory":true},"top_k":5,"ai_optimized":false}
```

Read the actual supplied hit content. Titles alone do not establish conditions or provenance. If full retrieval is needed, route the exact returned ID to an authorized Role announcing `{{ mcp_tool_memory_source }}`; do not invent a Root source call. A referenced Task ID can be read through `{{ mcp_tool_task_get }}` under that tool's actual envelope and authority, without claiming or reopening historical work. Compare the recorded conditions with current AST/Knowledge evidence; a memory that contradicts current code is a finding to report, not a rule to follow blindly.

## Is it worth saving?

Save it when all of these hold: it is confirmed (by the user, a decision record, a test or the source), it will matter to future work beyond the current Task, and it is not already obvious from the code, Git history or maintained documentation. Corrections deserve particular care: when the user corrects an approach, the corrected rule and its reason prevent the same mistake in every later session.

## Route decided persistence

Keep the confirmed conclusion, scope, rationale and provenance in the current Task checkpoint. Collect the unit's items and send them to the Scribe role in one brief at the sweep (checkpoint, handoff or session close), for example:

```text
Persist these decided PROJECT Memory items for session <id>, Task <id>:
1. New, decision: "Webhook replay horizon is 72 hours". Body: provider retries for up to 72h; shorter
   horizons dropped retried deliveries in the incident traced on Task <id>. Rejected: 24h (data loss).
   Tags: webhook. Important.
2. Update <memory id>, convention: "Integration tests run with the fixture provider", adding that
   the real provider is never called from CI (user instruction in this session).
3. New, skill: "Reproduce the replay bug locally": the exact three commands that worked, in order.
Report each written ID and revision.
```

Each item states whether it updates an existing subject (with its ID) or creates a new one, the exact title and body, the classification, tags and typed references, and whether it should be mandatory or important. Report unavailable persistence precisely; never claim it was saved or use the CLI, another identity or USER scope to bypass the boundary.

A duplicate is not disposable merely because its title overlaps. The authorized writer merges all distinct conditions, verifies preservation once and only then deletes the redundant record using its native expected revision/idempotency contract. Unchanged repetition requires no write. Never weaken a user constraint, discard unique knowledge or promote speculation to reduce context size.

USER preferences are not PROJECT facts. A missing verified Native USER subject means no USER corpus was read; it does not prove emptiness or grant a Role access. Human Native terminal administration and hook delivery have their own authority and are not agent MCP fallbacks.
{%- endif %}
