---
name: graphit-memory
description: 'Memory: recall and preserve preferences, corrections, user-provided project facts, standing guidance, and agent-discovered structural or non-obvious system knowledge across tasks.'
---

# Graphit Memory

`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host.

Preserve preferences, corrections, standing guidance, project facts and confirmed non-obvious knowledge that affect future work. Use Graphit, not native/model memory. Task holds investigation/progress; Knowledge holds maintained docs.

## Recall once, then reuse

Adapter hooks load mandatory project and user memories. Reuse that context; call `graphit_memory_mandatory` only if the hook reports fallback, mandatory context is absent, or project/scope changes. In fallback, call once per missing scope (`scope: project` and `scope: user` for a project session); the default fetch covers only project scope. Use only user scope on an artifact-only server, without inventing `project_dir`. Recover required context after compaction only when it is missing, not simply because a boundary occurred.

Before a material plan or when a new scope, earlier decision or blocker requires context, use `graphit_memory_search` with a focused query, `exclude_mandatory: true`, `top_k: 5` and `ai_optimized: true`. Results are titles and IDs; read selected entries with `graphit_memory_source`, normally one or two initially. Use `preview: true` only to disambiguate. Follow more hits/pages when an unresolved decision needs them; stop once the needed constraints are known. Reuse recalled entries across related searches. A superseded revision is historical: read its `current` entry before relying on it.

Use project scope and its known absolute `project_dir` by default; user scope is for cross-project preferences. Otherwise read the Hub skill and resolve the project with `graphit_cluster_projects` before Hub discovery; never guess paths or read stores as files. Installed AST/Knowledge contexts do not provide project memories. If expected context is missing, refine once; use `graphit_memory_list` for an intentional inventory or suspected empty store, not routine recall or post-write verification.

## Capture at the first durable finding

Before a correction changes an existing subject, merging duplicates, or capturing a non-obvious discovery for the first time, read [capture and reconciliation cases](references/durable-context.md). It shows full content, preserved conditions and no-write cases; unchanged recall needs no reread. If the local resource is unavailable, call `graphit_module_skill` with `module: memory`, `reference: references/durable-context.md` and the known `project_dir` when available.

Capture user facts about lifecycle/environment, policies, conventions or standing instructions even without “remember this”. Preserve its rationale and conditions when they affect future planning, compatibility, migration, review or risk. Also capture confirmed structural, reusable or costly discoveries: implicit contracts, sources of truth/generation flows, coupling, recurring failure modes, trade-offs and learned procedures. Capture now, not at repetition or task end.

Before writing, reuse an existing matching entry already recalled; otherwise make one focused subject search. If the same fact is present and current, do not write it again. Use `graphit_memory_update` for an existing subject so its ID/history survives; use `graphit_memory_insert` when genuinely new. Include the concrete conclusion, scope/conditions, rationale and a compact source reference. Classify lifecycle/environment state as `fact`, standing choices as `decision`/`convention`, and corrections as `correction`. Task-local instructions, transient progress, speculation, obvious code facts and one-off results without future value stay out of Memory.

Mark standing context needed in every session `mandatory`; use `important` for consequential but conditional knowledge. Use `graphit_memory_mark_mandatory`/`graphit_memory_unmark_mandatory` or `graphit_memory_promote`/`graphit_memory_demote` only when recall needs change. Never demote, shorten away conditions, or delete critical constraints merely to reduce tokens. `graphit_memory_important` is for intentional review of promoted entries, not a second mandatory-memory fetch.

## Reconcile without losing knowledge

On contradiction, preserve provenance and conditions, then update the current entry using confirmed newer evidence; do not silently replace a user constraint with an inference. For duplicates, merge every distinct fact and constraint into the survivor, read that entry once to verify preservation, then `graphit_memory_delete` the redundant entry. Preserve unique knowledge; leave unrelated memories alone.

A successful write is durable and refreshes its own search indexes: trust the acknowledgement; do not list/search/sync after each write. Read back only if the response is ambiguous or verification is needed before removing a duplicate. Use `graphit_memory_index` to repair indexes, `graphit_memory_schema` for graph diagnostics, and `graphit_memory_sync` to refresh an imported authoritative context. `graphit_memory_remove` is destructive context maintenance, never a recall repair.

Keep investigation/next action in Task and the reusable conclusion in Memory. If Memory MCP is unavailable, state that persistence was unavailable and retain the conclusion in the available task handoff; do not claim it was saved or substitute the Graphit CLI.
