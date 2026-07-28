package knowledge

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The Wiki Paths table listed this project and imported contexts, and left out the
// case the agent actually meets: a sibling project with its own compiled wiki,
// reachable by passing its dir as project_dir. Without that row the agent walks
// the sibling's docs tree instead of searching its wiki.
func TestKnowledgeRuleContentCoversSiblingWikis(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	for _, want := range []string{
		"a sibling project in the ecosystem",
		brand.MCPToolName("cluster", "projects") + "(project_dir",
		brand.MCPToolName("knowledge", "search") + "(project_dir: \"<sibling dir>\"",
		brand.MCPToolName("wiki", "search") + "(project_dir: \"<sibling dir>\"",
		// wiki_log answers "what changed over there recently" in one call.
		brand.MCPToolName("wiki", "log") + "(project_dir: \"<sibling dir>\"",
		// And a file the wiki points at is still read through the graph.
		brand.MCPToolName("ast", "source") + "(project_dir: \"<sibling dir>\"",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("knowledge skill does not cover a sibling's wiki: missing %q", want)
		}
	}

	if !strings.Contains(content, "installed, linked or imported") {
		t.Error("knowledge skill does not say a sibling needs no install, link or import")
	}
}

// Order matters and both halves have to be stated: the ecosystem lookup precedes
// the reading, and the wiki precedes the files.
func TestKnowledgeRuleContentPutsTheLookupBeforeTheReading(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	if !strings.Contains(content, "the lookup comes before the reading") {
		t.Error("knowledge skill does not state the required order")
	}
}
