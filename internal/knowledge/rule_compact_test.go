package knowledge

import (
	"strings"
	"testing"
)

func TestKnowledgeSkillCompactContract(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")
	for _, want := range []string{
		"before the first task action", "Update it when a step lands", "On resume", "Search returns titles", "daemon indexes",
		"graphit_knowledge_search", "graphit_wiki_search", "graphit_wiki_browse", "graphit_wiki_xrefs", "graphit_wiki_log",
		"graphit_wiki_source", "graphit_wiki_embed", "graphit_knowledge_list", "graphit_knowledge_lint", "graphit_knowledge_schema",
		"graphit_knowledge_export", "graphit_knowledge_install", "graphit_knowledge_remove", "graphit_knowledge_sync",
		"graphit_backlog_list", "graphit_backlog_add", "graphit_backlog_remove", "graphit_cluster_projects",
		"graphit_daemon_status", "graphit_daemon_stop", "graphit_sync",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("compact Knowledge skill missing %q", want)
		}
	}
	if len(content) > 6500 {
		t.Fatalf("Knowledge skill exceeded its token budget: %d bytes", len(content))
	}
}
