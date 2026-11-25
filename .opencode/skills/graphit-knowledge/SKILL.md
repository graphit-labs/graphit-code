---
name: graphit-knowledge
description: "Knowledge: retrieve and maintain user/technical documentation by business domain; verify code/documentation consistency for every changed work unit using wiki and implementation evidence."
---

# Graphit Knowledge

`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host. Stores follow the checked-out project version (Git branch; `main` without Git); optional `project_version` reaches another.

Persistence boundary for every module: shared project records may contain only non-sensitive content inherent to the project and needed for confirmed contracts, decisions, evidence, operations or technical continuity. Never persist personal or sensitive user/organization data, secrets, credentials, security-risk material, or an engineer's feelings, frustration, transient impressions or unconfirmed speculation. User-only context belongs only in private user memory when it is a durable, legitimately useful preference or correction; otherwise do not persist it. Keep necessary technical hypotheses explicitly labelled and never promote them to facts.

Structured MCP calls accept top-level `ai_optimized`: omitted/`true` selects TOON text without duplicate JSON `structuredContent`; `false` selects JSON for machine consumers. Knowledge Source is a structured envelope: its Markdown string is preserved, rather than treated as a raw-text tool exception.

Knowledge maintains user/technical documentation by business domain and reader goal. Make it usable by someone or another project without this conversation. Task owns executable specifications/plans/results; AST proves code behavior; Memory holds durable constraints.

When a task, session, memory or knowledge page relies on another record, explicitly send `references` with each target's `type` and `id`, plus `relation` (for example supports, implements, derived_from or relates_to). Resolve real IDs through the corresponding read tools; never invent them. Mentioning an ID in prose or making a Markdown link does not create a database relation. Send the complete intended explicit list on reference changes; omit it to preserve existing relations, and send [] to clear explicit relations. Structural task/session links remain managed by their existing fields. Qualify cross-project/user targets with `scope` and `scope_id`, and imported knowledge with `context`; use logical identities, never checkout paths.
Example write input: `references: [{"target": {"type": "memory", "id": "<verified memory ID>"}, "relation": "supports"}]`. The source is the record being written; do not repeat a source ID. Read its existing references before replacing the list so unrelated links survive.
Use `graphit_task_references` for outgoing/incoming Task or Session relations, and `graphit_knowledge_references` for outgoing document relations or incoming logical-target references in the admitted Knowledge corpus. Open the page by slug in the admitted publication. To continue a reference direction, reuse its cursor and the same selectors. A recorded foreign relation grants no target-content access. Use only operations advertised by the selected transport; report missing projection/repair capabilities instead of inventing a generic query or reconcile tool.

For Knowledge, declare the same typed references in the authoritative Markdown YAML frontmatter under `references`, as a list of objects with `target: {type: memory, id: <verified ID>, scope: project, scope_id: <project ID>}` and `relation: derived_from`. Indexing persists that metadata. Keep existing frontmatter and reference entries when editing; an explicit `references: []` clears authored relations. Ordinary wiki links remain a separate persisted wiki-link relationship.

## Load detail at the decision boundary

Retrieval alone needs no reference. Before planning/reorganizing docs or reviewing domain coverage, read [documentation design](references/documentation-design.md). Before first substantive domain authoring, read [worked domain](references/worked-domain.md); adapt depth, not its stack/rules/file count. Narrow maintenance needs only the design reference's per-unit consistency section when guidance is missing. Reuse retained references. `DOCS_ROOT` means this project's `docs/` or its existing authoritative location.
If local reference files are unavailable, use `graphit_module_skill` with `module: knowledge`, `reference: references/documentation-design.md` or `references/worked-domain.md`, and the known `project_dir` when available; request only the needed reference.

## Retrieve enough evidence

Known slug: call `graphit_wiki_source` with its exact `slug`. Without selectors, the Native reader returns the complete virtual Markdown: native metadata and the literal body. For a long page use explicit Core `selectors` and expand only as needed:

```json
{"slug":"alpha","selectors":{"head":20,"line_numbers":true}}
```

Otherwise locate candidates through `graphit_knowledge_search`. The Native search envelope is `search`; preview is a summary or a bounded body excerpt, never evidence that the whole page was read:

```json
{"search":{"query":"deployment policy","mode":"fts","top_k":5,"page_size":2,"preview":true}}
```

Preserve the returned slug, PROJECT/version and source selection. Both tools accept `source: {kind: project_knowledge}` or `source: {kind: imported_knowledge, artifact: {id, version, context_alias?}}`. An imported artifact uses its exact observed ID/version and no PROJECT selectors; `context` and `path` are not Native aliases. For PROJECT reads, resolve `project_dir` on the current host and pass `project_version` only to select another retained version. These selectors grant no authority. A cluster neighbor remains Graphit-managed: use Hub discovery and its admitted tools rather than native discovery. Public technologies need no Hub detour.

Titles and previews locate evidence; read the selected page. Reuse enough sources and stop when the question is answered. Follow opaque cursors unchanged only for unresolved coverage. Preserve no-corpus, published-empty, absent-page, registry-absent and failed reads as distinct outcomes. Output limits fail; the source reader does not silently truncate. For committed document synchronization history, request `graphit_wiki_log` with `history: {limit}` only when that history answers the question. Other browse/reference capabilities require an actually announced tool; do not invent one.
At any stage, new questions may need Memory facts/lessons or Task investigations/decisions/results. Reuse sufficient context; read its skill and retrieve the missing topic. For Task, known ID → `graphit_task_get`; otherwise `graphit_task_search` → selected records. Check history against current docs/code; do not query both stores mechanically. State gaps if a needed source is unavailable.

## Design and write maintained documentation

For each new Knowledge page you are authorized to create without an explicit
ID, call `graphit_knowledge_id` with
`{"path":"docs/guide.md"}`, using the actual relative page path. Receive the real ULID, then save it as YAML frontmatter `id`
together with the document. Do not invent an ID, leave an empty placeholder or
omit this step. If the tool or authoring permission is unavailable, report the
missing capability instead of pretending the page was created with an ID.
Preserve a supplied explicit ID; editing or indexing an existing page does not
regenerate it. A saved ID preserves identity across a rename.

External documents and sources without frontmatter can still be published with
an automatic deterministic ULID derived from their validated relative origin.
The same origin keeps that fallback on reindex; a rename changes it unless an
explicit ID is retained. Its time component is not a creation timestamp.
Neither the tool nor the indexer writes the page or grants authoring access;
the authorized author saves the returned ID. There is no CLI generator.

Treat every new or maintained page as an entry in the documentation guide. Find the owning domain, audience and entry page before choosing a path. Check where the reader comes from and what they should read next. Set root YAML `previous` and `next` to verified local slugs or relative Markdown destinations wherever those neighbors exist. When inserting, moving or removing a step, check adjacent pages and the domain index and update affected links within scope. Omit the missing side at endpoints and both fields for an independent reference. Preserve page IDs and verify authored links in the selected publication.

Map domains, actors/journeys, rules/contracts, implementation evidence and page targets in Task before writing. Extend existing navigation: root → domain → user/technical/operation pages as needed. File count/headings do not prove coverage; no empty skeletons or generic overview replacing unrelated domains.
Write in `docs/` or the authoritative source, never the generated wiki/store. Users need prerequisites, steps, outcomes and errors/recovery; consumers need contracts, boundaries, data/state, source/version, operations and provenance. Link shared facts; keep examples consistent. Label proposed behavior, assumptions and unverified findings. Resolve scope in Task without inventing rules. Authorized docs maintenance needs no extra approval.

## Verify both directions for every work unit

For EVERY code work unit, inspect/update affected user/technical docs, examples and navigation in that unit. For EVERY doc work unit, inspect corresponding implementation with AST and validate its claims. Check rule → source/test → page and changed page → implementation/consumers; inspect affected references for contradictions. No-impact conclusions name inspected targets and rationale.
Resolve drift: correct stale docs, fix authorized defects, or label future intent/current limitations and preserve unresolved work in Task. Never weaken a requirement to bless a bug or expand code scope to match a draft. Verify relevant commands/examples, links and outcomes. Distinguish documentation quality from implementation agreement, executed tests from inspection. Before unit completion record targets, version/scope, actual evidence, findings and next action in Task progress/checks. The references show review criteria and both drift directions.

Other-project documentation is read only through its resolved wiki tools. If required MCP tools are unavailable or current-project text is unindexed, use focused native reads of current-project authoritative documents, state the fallback once and do not substitute the Graphit CLI.

## Freshness and maintenance

The daemon indexes `docs/` after writes. Use `graphit_index_status` when advertised to diagnose observed document/lexical and embedding phases. Explicit administrative `graphit_index_run` requests a coalesced full-project rescan from the current supervisor, not a separate indexer. Acceptance is not completion; optional bounded waiting follows the producer observations. `graphit_sync` reconciles installed artifacts and agent files rather than waiting for indexes. Do not duplicate the asynchronous completion hook. Delegated transports without these controls report the gap to the coordinator. Retry transient read failures once before reporting them.

Use advertised `graphit_knowledge_list` for metadata catalogues and `graphit_knowledge_lint` for published metadata/reference/staleness diagnostics. `graphit_knowledge_export` handles requested exports. Do not clear corpora or invent an independent embed/sync/remove command for recovery. For reuse through Hub, make scope/version, prerequisites, supported contracts, navigation and provenance self-contained; use the Hub skill at publication/consumption boundaries. Documentation maintenance alone does not publish an artifact.
