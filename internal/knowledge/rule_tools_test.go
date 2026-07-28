package knowledge

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Every knowledge and wiki tool has to be reachable from the skill the mandate
// points at, or the mandate advertises a capability the agent cannot use.
func TestKnowledgeRuleContentTeachesEveryTool(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	for _, action := range []string{
		"search", "list", "schema", "lint", "export", "install", "remove", "sync",
	} {
		if !strings.Contains(content, brand.MCPToolName("knowledge", action)) {
			t.Errorf("knowledge skill never mentions %s", brand.MCPToolName("knowledge", action))
		}
	}

	for _, action := range []string{"search", "browse", "xrefs"} {
		if !strings.Contains(content, brand.MCPToolName("wiki", action)) {
			t.Errorf("knowledge skill never mentions %s", brand.MCPToolName("wiki", action))
		}
	}
}

// The watcher rebuilds the wiki on its own, so the only reason to reindex by hand
// is a case the watcher cannot have seen. When that happens, knowledge_sync is
// the tool that does just the wiki — the global sync also rebuilds the AST graph,
// both memory wikis and the Hub, none of which the situation asked for.
func TestKnowledgeRuleContentPrefersTheNarrowSyncTool(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	if !strings.Contains(content, brand.MCPToolName("knowledge", "sync")+"(project_dir") {
		t.Error("knowledge skill does not show how to call knowledge_sync")
	}
	if !strings.Contains(content, "Reindexing is automatic") {
		t.Error("knowledge skill lost the section explaining that the daemon reindexes")
	}
}
