{% if tool_transport | default("root") == "role" -%}
# {{ brand_display }} Knowledge for an authenticated Role

Knowledge explains maintained business and technical contracts. Read the selected PROJECT/version through the tools actually announced to this run. The closed envelope is `{selection?, input, ai_optimized?}`. Top-level `ai_optimized` is an optional strict boolean: omitted/`true` returns TOON text without duplicate JSON `structuredContent`; `false` selects JSON. The Role envelope has no top-level path/context/wikis/hub_refs fields and does not grant an imported, USER or foreign corpus.

{% include "shared/persistence-boundary.md" %}

## Retrieve enough evidence

Known slug: call `{{ mcp_tool_wiki_source }}` directly. Otherwise use `{{ mcp_tool_knowledge_search }}` with focused query text; preview helps distinguish candidates and top_k caps results:

```json
{"input":{"query":"deployment policy","top_k":5,"page_size":2,"preview":true}}
```

Titles and previews locate evidence; read the selected slug. Replace alpha below with an observed slug and keep Core selectors nested:

```json
{"input":{"slug":"alpha","selectors":{"head":20,"line_numbers":true}}}
```

Reuse sufficient sources and stop when the question is answered. Follow an unchanged next_cursor when the unresolved question needs the next page. A miss does not prove absence; refine once or use the announced knowledge_list for an intended catalogue. Preserve distinct no-corpus, published-empty, absent-page, incomplete and failure outcomes. Search uses the native lexical index; it does not require embedding or a provider plan.

If relationships are needed and `{{ mcp_tool_knowledge_references }}` is announced, read source edges without assuming target access:

```json
{"input":{"outgoing":"alpha","page_size":1}}
```

These are native source references and literal backlinks. An unresolved or USER/foreign target is not permission to open it.

For committed document synchronization history, use `{{ mcp_tool_wiki_log }}`. Each entry reports added, updated and removed slugs in the selected PROJECT/version; it is not a conversation log. Follow its opaque next_cursor with the same selection and limit when another page is needed:

```json
{"input":{"limit":10}}
```

Unchanged document publication and semantic-only updates produce no synchronization entry. A no-corpus result differs from a published selection with no entries. A publication can remain committed after a delivery failure; history records its actual SDK transaction. Rich cross-source resolution, imports, index repair, embedding publication, export and reset are not provided by this Role reader. Report the missing capability instead of inventing a tool or substituting Runtime credentials.

## Documentation reasoning

Before a domain coverage review, read [documentation design](references/documentation-design.md). Before unfamiliar domain authoring, read [the worked case](references/worked-domain.md). Request only the needed companion through `{{ mcp_tool_module_skill }}`:

```json
{"input":{"module":"knowledge","reference":"references/documentation-design.md"}}
```

`DOCS_ROOT` means {% if publication_scope | default("project") == "global" %}the authoritative documentation directory of the active PROJECT. Resolve its current guidance through `{{ mcp_tool_module_skill }}` with `module: knowledge` before authoring or indexing{% else %}the actual authoritative `{{ docs_dir }}/` or maintained alternative{% endif %}. Separate supported behavior, intended behavior, assumptions and gaps. Use AST to verify code claims when Read(Ast) is granted and the tool is announced. Task retains the owner's scope/evidence; Memory retains durable constraints. This Role does not acquire coordinator claims or silently expand permissions.

For each new Knowledge page you are authorized to create without an explicit
ID, call `{{ mcp_tool_knowledge_id }}` with
`{"input":{"path":"docs/guide.md"}}`, using the actual relative page path and
current selection. Receive the real ULID, then save it as YAML frontmatter `id`
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

For each authorized work unit, reconcile relevant source/test behavior and the documentation in both directions. Preserve exact user conditions, meaningful errors and examples that match the current DTOs. A read-only Role reports a proposed correction and its evidence; authoring requires actual granted tools and scope. Documentation maintenance does not itself publish a Hub artifact.
{%- else -%}
# {{ brand_display }} Knowledge

`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host. Stores follow the checked-out project version (Git branch; `main` without Git); optional `project_version` reaches another.

{% include "shared/persistence-boundary.md" %}

Structured MCP calls accept top-level `ai_optimized`: omitted/`true` selects TOON text without duplicate JSON `structuredContent`; `false` selects JSON for machine consumers. Knowledge Source is a structured envelope: its Markdown string is preserved, rather than treated as a raw-text tool exception.

Knowledge maintains user/technical documentation by business domain and reader goal. Make it usable by someone or another project without this conversation. Task owns executable specifications/plans/results; AST proves code behavior; Memory holds durable constraints.

{% include "shared/references/agent-contract.md" %}
{% include "shared/references/agent-example.md" %}
{% include "shared/references/query-contract.md" %}
{% include "shared/references/knowledge-contract.md" %}

## Load detail at the decision boundary

Retrieval alone needs no reference. Before planning/reorganizing docs or reviewing domain coverage, read [documentation design](references/documentation-design.md). Before first substantive domain authoring, read [worked domain](references/worked-domain.md); adapt depth, not its stack/rules/file count. Narrow maintenance needs only the design reference's per-unit consistency section when guidance is missing. Reuse retained references. `DOCS_ROOT` means {% if publication_scope | default("project") == "global" %}the authoritative documentation directory of the active PROJECT. Resolve its current guidance through `{{ mcp_tool_module_skill }}` with `module: knowledge` before authoring or indexing{% else %}this project's `{{ docs_dir }}/` or its existing authoritative location{% endif %}.
If local reference files are unavailable, use `{{ mcp_tool_module_skill }}` with `module: knowledge`, `reference: references/documentation-design.md` or `references/worked-domain.md`, and the known `project_dir` when available; request only the needed reference.

## Retrieve enough evidence

Known slug: call `{{ mcp_tool_wiki_source }}` with its exact `slug`. Without selectors, the Native reader returns the complete virtual Markdown: native metadata and the literal body. For a long page use explicit Core `selectors` and expand only as needed:

```json
{"slug":"alpha","selectors":{"head":20,"line_numbers":true}}
```

Otherwise locate candidates through `{{ mcp_tool_knowledge_search }}`. The Native search envelope is `search`; preview is a summary or a bounded body excerpt, never evidence that the whole page was read:

```json
{"search":{"query":"deployment policy","mode":"fts","top_k":5,"page_size":2,"preview":true}}
```

Preserve the returned slug, PROJECT/version and source selection. Both tools accept `source: {kind: project_knowledge}` or `source: {kind: imported_knowledge, artifact: {id, version, context_alias?}}`. An imported artifact uses its exact observed ID/version and no PROJECT selectors; `context` and `path` are not Native aliases. For PROJECT reads, resolve `project_dir` on the current host and pass `project_version` only to select another retained version. These selectors grant no authority. A cluster neighbor remains {{ brand_display }}-managed: use Hub discovery and its admitted tools rather than native discovery. Public technologies need no Hub detour.

Titles and previews locate evidence; read the selected page. Reuse enough sources and stop when the question is answered. Follow opaque cursors unchanged only for unresolved coverage. Preserve no-corpus, published-empty, absent-page, registry-absent and failed reads as distinct outcomes. Output limits fail; the source reader does not silently truncate. For committed document synchronization history, request `{{ mcp_tool_wiki_log }}` with `history: {limit}` only when that history answers the question. Other browse/reference capabilities require an actually announced tool; do not invent one.
At any stage, new questions may need Memory facts/lessons or Task investigations/decisions/results. Reuse sufficient context; read its skill and retrieve the missing topic. For Task, known ID → `{{ mcp_tool_task_get }}`; otherwise `{{ mcp_tool_task_search }}` → selected records. Check history against current docs/code; do not query both stores mechanically. State gaps if a needed source is unavailable.

## Design and write maintained documentation

For each new Knowledge page you are authorized to create without an explicit
ID, call `{{ mcp_tool_knowledge_id }}` with
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

When guides have a genuine reading sequence, fill optional root YAML `previous`/`next` with the existing local slug or relative Markdown destination. Keep current IDs and other links. No field is required for an independent page, and reciprocal links are not a global invariant. Follow destinations only within the page's authorized PROJECT/version or imported artifact; preserve an unavailable-target diagnostic instead of selecting another source.

Map domains, actors/journeys, rules/contracts, implementation evidence and page targets in Task before writing. Extend existing navigation: root → domain → user/technical/operation pages as needed. File count/headings do not prove coverage; no empty skeletons or generic overview replacing unrelated domains.
Write in {% if publication_scope | default("project") == "global" %}the active PROJECT's resolved `DOCS_ROOT`{% else %}`{{ docs_dir }}/` or the authoritative source{% endif %}, never the generated wiki/store. Users need prerequisites, steps, outcomes and errors/recovery; consumers need contracts, boundaries, data/state, source/version, operations and provenance. Link shared facts; keep examples consistent. Label proposed behavior, assumptions and unverified findings. Resolve scope in Task without inventing rules. Authorized docs maintenance needs no extra approval.

## Verify both directions for every work unit

For EVERY code work unit, inspect/update affected user/technical docs, examples and navigation in that unit. For EVERY doc work unit, inspect corresponding implementation with AST and validate its claims. Check rule → source/test → page and changed page → implementation/consumers; inspect affected references for contradictions. No-impact conclusions name inspected targets and rationale.
Resolve drift: correct stale docs, fix authorized defects, or label future intent/current limitations and preserve unresolved work in Task. Never weaken a requirement to bless a bug or expand code scope to match a draft. Verify relevant commands/examples, links and outcomes. Distinguish documentation quality from implementation agreement, executed tests from inspection. Before unit completion record targets, version/scope, actual evidence, findings and next action in Task progress/checks. The references show review criteria and both drift directions.

Other-project documentation is read only through its resolved wiki tools. If required MCP tools are unavailable or current-project text is unindexed, use focused native reads of current-project authoritative documents, state the fallback once and do not substitute the {{ brand_display }} CLI.

## Freshness and maintenance

The daemon indexes {% if publication_scope | default("project") == "global" %}the active PROJECT's configured documentation directory{% else %}`{{ docs_dir }}/`{% endif %} after writes. Use `{{ mcp_tool_knowledge_index }}` for missing coverage or a specific directory; `{{ mcp_tool_knowledge_sync }}` only when knowledge freshness is needed for a decision. Publication `{{ mcp_tool_sync }}` reconciles installed artifacts and agent files rather than waiting for indexes. Do not duplicate the asynchronous completion hook. On stale/locked reads, check `{{ mcp_tool_daemon_status }}`; retry a transient failure once before reporting it.

Use `{{ mcp_tool_knowledge_lint }}`/`{{ mcp_tool_knowledge_schema }}` for diagnostics, `{{ mcp_tool_wiki_embed }}` for missing semantic coverage, and `{{ mcp_tool_knowledge_export }}` for requested exports. `{{ mcp_tool_knowledge_remove }}` without `context` clears the project wiki; never reset to repair search. For reuse through Hub, make scope/version, prerequisites, supported contracts, navigation and provenance self-contained; use the Hub skill at publication/consumption boundaries. Documentation maintenance alone does not publish an artifact.
{%- endif %}
