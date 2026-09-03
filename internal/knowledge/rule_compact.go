package knowledge

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

func KnowledgeRuleContent(contexts []string, docsDir string) string {
	_ = contexts
	if docsDir == "" {
		docsDir = config.DefaultDocsDir
	}
	return strings.Join([]string{
		"# Graphit Knowledge",
		"",
		"Use this skill for project documentation, architecture, decisions, specifications, provenance, or another project's wiki. Task lifecycle and backlog belong to Graphit Task; code structure belongs to Graphit AST; external systems must be resolved through Graphit Hub first.",
		"",
		"## Reading knowledge",
		"",
		"1. Search with `" + brand.MCPToolName("knowledge", "search") + "` for the current project or `" + brand.MCPToolName("wiki", "search") + "` across selected wikis. Search returns titles, not evidence.",
		"2. Pick the smallest relevant set and read it with `" + brand.MCPToolName("wiki", "source") + "`; use a pattern or line slice for long pages. Use `preview: true` only when titles cannot disambiguate.",
		"3. Use `" + brand.MCPToolName("wiki", "xrefs") + "` for provenance/relationships, `" + brand.MCPToolName("wiki", "log") + "` for change history, and `" + brand.MCPToolName("wiki", "browse") + "` or `" + brand.MCPToolName("knowledge", "list") + "` for catalogues.",
		"",
		"A different project's documentation is queried with its returned `project_dir`; never walk or grep its docs tree. A missing result is not proof that a page is absent—use the catalogue or refine once before concluding.",
		"",
		"## Maintenance",
		"",
		"The daemon indexes `" + docsDir + "/` after writes. Use `" + brand.MCPToolName("knowledge", "sync") + "` only for knowledge-only freshness and `" + brand.MCPToolName("sync") + "` when all module indexes must be aligned. Check `" + brand.MCPToolName("daemon", "status") + "` on stale/locked reads. `" + brand.MCPToolName("knowledge", "lint") + "`, `schema`, `export`, `install`, and `remove` are administrative operations used only when the task calls for them; `" + brand.MCPToolName("wiki", "embed") + "` repairs semantic coverage.",
		"",
		"Tool index: `graphit_knowledge_search`, `graphit_wiki_search`, `graphit_wiki_browse`, `graphit_wiki_xrefs`, `graphit_wiki_log`, `graphit_wiki_source`, `graphit_wiki_embed`, `graphit_knowledge_list`, `graphit_knowledge_lint`, `graphit_knowledge_schema`, `graphit_knowledge_export`, `graphit_knowledge_install`, `graphit_knowledge_remove`, `graphit_knowledge_sync`, `graphit_cluster_projects`, `graphit_daemon_status`, `graphit_daemon_stop`, `graphit_sync`.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return ide.ModuleMandateTrigger(
		"Knowledge & Documentation",
		knowledgeSkillName,
		"project knowledge, documentation, architecture, decisions, specifications, or provenance",
		"",
		[]string{
			"answering why the project works this way or reading architecture, decisions, specifications, or provenance",
			"searching, reading, creating, or maintaining documentation or another project's wiki",
			"requiring proven wiki/index freshness",
		},
		[]string{"knowledge_search", "wiki_search", "wiki_source", "wiki_xrefs"},
	)
}
