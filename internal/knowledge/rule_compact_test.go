package knowledge

import (
	"strings"
	"testing"
)

func TestKnowledgeSkillCompactContract(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")
	for _, want := range []string{
		"Task lifecycle and backlog belong to Graphit Task", "Search returns titles", "daemon indexes", "without a context clears the project wiki",
		"pair every Knowledge search", "one focused `graphit_task_search`", "pass `ai_optimized: true`", "follow `next_cursor`",
		"Read every relevant hit with `graphit_task_get`", "specification, analytical result, progress, comments, check evidence, next step, and audit history",
		"Task history supplements Knowledge", "If Task is disabled or its tools are unavailable, continue the Knowledge workflow",
		"graphit_knowledge_search", "graphit_wiki_search", "graphit_wiki_browse", "graphit_wiki_xrefs", "graphit_wiki_log",
		"graphit_wiki_source", "graphit_wiki_embed", "graphit_knowledge_list", "graphit_knowledge_lint", "graphit_knowledge_schema",
		"graphit_knowledge_index", "graphit_knowledge_remove", "graphit_knowledge_sync",
		"graphit_cluster_projects",
		"graphit_daemon_status", "graphit_sync",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("compact Knowledge skill missing %q", want)
		}
	}
	if strings.Contains(content, "docs/tasks") || strings.Contains(content, "graphit_backlog") {
		t.Fatalf("Knowledge skill still owns Markdown task state: %s", content)
	}
	if len(content) > 6500 {
		t.Fatalf("Knowledge skill exceeded its token budget: %d bytes", len(content))
	}
	for _, obsolete := range []string{"graphit_knowledge_export", "graphit_knowledge_install", "graphit_daemon_stop"} {
		if strings.Contains(content, obsolete) {
			t.Fatalf("Knowledge skill advertises unsupported or unrelated tool %q", obsolete)
		}
	}
	mandate := MandateTrigger()
	if strings.Contains(mandate, "graphit_task_") || strings.Contains(mandate, "pair every Knowledge search") {
		t.Fatalf("Knowledge mandate duplicates Task-owned paired retrieval guidance: %s", mandate)
	}
	if len(mandate) > 1400 {
		t.Fatalf("Knowledge mandate exceeded its resident token budget: %d bytes", len(mandate))
	}
}
