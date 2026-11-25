---
name: graphit-memory
description: "Memory: retrieve facts, decisions and lessons whenever questions arise; preserve durable guidance, corrections and confirmed discoveries across tasks."
---

# Graphit Memory

Read [recall and decided persistence cases](references/durable-context.md) when a new question, correction or durable finding needs guidance; reuse the loaded reference afterward.

Persistence boundary for every module: shared project records may contain only non-sensitive content inherent to the project and needed for confirmed contracts, decisions, evidence, operations or technical continuity. Never persist personal or sensitive user/organization data, secrets, credentials, security-risk material, or an engineer's feelings, frustration, transient impressions or unconfirmed speculation. User-only context belongs only in private user memory when it is a durable, legitimately useful preference or correction; otherwise do not persist it. Keep necessary technical hypotheses explicitly labelled and never promote them to facts.

Reuse mandatory Memory delivered by hooks. Root advertises `graphit_memory_search` for PROJECT recall and `graphit_memory_index` for explicit administrative index maintenance; check the current catalogue before use. Missing USER identity means USER Memory was not read, never an empty personal store.

Use focused search with `text`, `top_k`, supported `filter` fields (classification, tag, mandatory, important, exclude_mandatory), optional `mode` fts/semantic/hybrid and selected `project_dir`/`project_version`. Top-level `ai_optimized: false` requests JSON. Returned hits are evidence only to the extent of their actual supplied content. Selectors grant no access. Discover a neighbor through `graphit_projects_list` only when announced and use authorized `checkouts[].dir`.

List/source/history/create/update/delete and mandatory/important flags belong to authorized Role tools or human Native terminal administration. If not announced on this transport, record the exact missing capability and route decided persistence to an authorized Role; never fabricate Role credentials or substitute the CLI for an unavailable MCP mutation. Read the Role version of this skill for its actual envelopes.

Update an existing subject to preserve identity/history, create only genuinely new durable guidance and delete duplicates only after preserving all distinct constraints. Mark standing necessary context mandatory, consequential conditional context important. Task owns transient investigation and progress. Successful writes acknowledge their native effect; never routinely sync/reindex afterward. Index repair uses the advertised operation; generic storage query/schema, imported Memory sync and destructive context removal are not Root recall operations.

When a task, session, memory or knowledge page relies on another record, explicitly send `references` with each target's `type` and `id`, plus `relation` (for example supports, implements, derived_from or relates_to). Resolve real IDs through the corresponding read tools; never invent them. Mentioning an ID in prose or making a Markdown link does not create a database relation. Send the complete intended explicit list on reference changes; omit it to preserve existing relations, and send [] to clear explicit relations. Structural task/session links remain managed by their existing fields. Qualify cross-project/user targets with `scope` and `scope_id`, and imported knowledge with `context`; use logical identities, never checkout paths.
Example write input: `references: [{"target": {"type": "memory", "id": "<verified memory ID>"}, "relation": "supports"}]`. The source is the record being written; do not repeat a source ID. Read its existing references before replacing the list so unrelated links survive.
Use `graphit_task_references` for outgoing/incoming Task or Session relations, and `graphit_knowledge_references` for outgoing document relations or incoming logical-target references in the admitted Knowledge corpus. Keep each origin PROJECT/version, target type/scope/context and independent direction cursor. A recorded foreign relation grants no target-content access. Use only operations advertised by the selected transport; report missing projection/repair capabilities instead of inventing a generic query or reconcile tool.
