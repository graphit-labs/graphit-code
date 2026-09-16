package knowledge

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

func KnowledgeRuleContent(contexts []string, docsDir string) string {
	_ = contexts
	if docsDir == "" {
		docsDir = config.DefaultDocsDir
	}
	return strings.Join([]string{
		"# Graphit Knowledge",
		"",
		"`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host.",
		"",
		"Knowledge maintains user/technical documentation by business domain and reader goal. Make it usable by someone or another project without this conversation. Task owns executable specifications/plans/results; AST proves code behavior; Memory holds durable constraints.",
		"",
		"## Load detail at the decision boundary",
		"",
		"Retrieval alone needs no reference. Before planning/reorganizing docs or reviewing domain coverage, read [documentation design](references/documentation-design.md). Before first substantive domain authoring, read [worked domain](references/worked-domain.md); adapt depth, not its stack/rules/file count. Narrow maintenance needs only the design reference's per-unit consistency section when guidance is missing. Reuse retained references. `DOCS_ROOT` means this project's `" + docsDir + "/` or its existing authoritative location.",
		"If local reference files are unavailable, use `" + brand.MCPToolName("module_skill") + "` with `module: knowledge`, `reference: references/documentation-design.md` or `references/worked-domain.md`, and the known `project_dir` when available; request only the needed reference.",
		"",
		"## Retrieve enough evidence",
		"",
		"1. Reuse a known slug with `" + brand.MCPToolName("wiki_source") + "`; otherwise `" + brand.MCPToolName("knowledge_search") + "` for the project/installed `context`, or `" + brand.MCPToolName("wiki_search") + "` across selected `wikis`/`hub_refs`. Reuse known absolute `project_dir`; for unresolved ecosystem projects, read the Hub skill and use `" + brand.MCPToolName("cluster_projects") + "` before the Hub catalog. Hub-only: announce/install needed Knowledge and query it; the question authorizes preparation. Only without a local checkout, omit `project_dir` and use resolved installed qualified `id@version`. Public technology (e.g. React) needs no Hub lookup; use known knowledge/official docs and verify current details.",
		"2. Use focused terms, `top_k: 5`, `ai_optimized: true`; `preview: true` only to disambiguate titles. Titles are not evidence. Read selected `" + brand.MCPToolName("wiki_source") + "` pages with returned slug as `path`, preserving project/context. Slice long pages; expand for relevant surrounding rules.",
		"3. Reuse results; stop once sources answer the question. Follow `next_cursor` for unresolved coverage/exhaustive inventory only. A miss is not absence: refine once or use `" + brand.MCPToolName("wiki_browse") + "` with relevant `doc_type`; `" + brand.MCPToolName("knowledge_list") + "` is for an intended catalogue.",
		"4. Use `" + brand.MCPToolName("wiki_xrefs") + "` with a shallow depth for unresolved provenance/relationships, or `" + brand.MCPToolName("wiki_log") + "` for change history. Do not load either for every page.",
		"",
		"At any stage, new questions may need Memory facts/lessons or Task investigations/decisions/results. Reuse sufficient context; read its skill and retrieve the missing topic. For Task, known ID → `" + brand.MCPToolName("task_get") + "`; otherwise `" + brand.MCPToolName("task_search") + "` → selected records. Check history against current docs/code; do not query both stores mechanically. State gaps if a needed source is unavailable.",
		"",
		"## Design and write maintained documentation",
		"",
		"Map domains, actors/journeys, rules/contracts, implementation evidence and page targets in Task before writing. Extend existing navigation: root → domain → user/technical/operation pages as needed. File count/headings do not prove coverage; no empty skeletons or generic overview replacing unrelated domains.",
		"Write in `" + docsDir + "/` or the authoritative source, never the generated wiki/store. Users need prerequisites, steps, outcomes and errors/recovery; consumers need contracts, boundaries, data/state, source/version, operations and provenance. Link shared facts; keep examples consistent. Label proposed behavior, assumptions and unverified findings. Resolve scope in Task without inventing rules. Authorized docs maintenance needs no extra approval.",
		"",
		"## Verify both directions for every work unit",
		"",
		"For EVERY code work unit, inspect/update affected user/technical docs, examples and navigation in that unit. For EVERY doc work unit, inspect corresponding implementation with AST and validate its claims. Check rule → source/test → page and changed page → implementation/consumers; inspect affected references for contradictions. No-impact conclusions name inspected targets and rationale.",
		"Resolve drift: correct stale docs, fix authorized defects, or label future intent/current limitations and preserve unresolved work in Task. Never weaken a requirement to bless a bug or expand code scope to match a draft. Verify relevant commands/examples, links and outcomes. Distinguish documentation quality from implementation agreement, executed tests from inspection. Before unit completion record targets, version/scope, actual evidence, findings and next action in Task progress/checks. The references show review criteria and both drift directions.",
		"",
		"Other-project documentation is read only through its resolved wiki tools. If required MCP tools are unavailable or current-project text is unindexed, use focused native reads of current-project authoritative documents, state the fallback once and do not substitute the Graphit CLI.",
		"",
		"## Freshness and maintenance",
		"",
		"The daemon indexes `" + docsDir + "/` after writes. Use `" + brand.MCPToolName("knowledge_index") + "` for missing coverage or a specific directory; `" + brand.MCPToolName("knowledge_sync") + "` only when knowledge freshness is needed for a decision, or `" + brand.MCPToolName("sync") + "` when multiple indexes must align. Do not duplicate the asynchronous completion hook. On stale/locked reads, check `" + brand.MCPToolName("daemon_status") + "`; retry a transient failure once before reporting it.",
		"",
		"Use `" + brand.MCPToolName("knowledge_lint") + "`/`" + brand.MCPToolName("knowledge_schema") + "` for diagnostics, `" + brand.MCPToolName("wiki_embed") + "` for missing semantic coverage, and `" + brand.MCPToolName("knowledge_export") + "` for requested exports. `" + brand.MCPToolName("knowledge_remove") + "` without `context` clears the project wiki; never reset to repair search. For reuse through Hub, make scope/version, prerequisites, supported contracts, navigation and provenance self-contained; use the Hub skill at publication/consumption boundaries. Documentation maintenance alone does not publish an artifact.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return agent.ModuleMandateTrigger(
		"Knowledge & Documentation", knowledgeSkillName,
		"retrieving/writing documentation or starting/completing a code or documentation work unit",
		"Known page → `"+brand.MCPToolName("wiki_source")+"`; unknown → `"+brand.MCPToolName("knowledge_search")+"` then source. Titles are discovery only; reuse evidence. For every code unit inspect/update affected user and technical docs; for every doc unit verify implementation with AST. Resolve drift and record inspected targets/evidence or justified no-impact in Task before completion. Organize docs by domain and reader goals; the skill routes design and worked examples before authoring. Task owns executable plans/results; query history only for a gap. Known local path first, otherwise cluster before Hub; public technologies need no Hub lookup.",
		nil, nil,
	)
}
