package memory

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Every memory tool has to be reachable from the memory skill: this framework is
// the agent's only memory, so a tool it cannot find has no native equivalent to
// fall back on.
func TestMemoryRuleContentTeachesEveryMemoryTool(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	for _, action := range []string{
		"search", "insert", "update", "delete", "list", "important",
		"promote", "demote", "index", "gc", "schema", "export", "remove", "sync",
	} {
		if !strings.Contains(content, brand.MCPToolName("memory", action)) {
			t.Errorf("memory skill never mentions %s", brand.MCPToolName("memory", action))
		}
	}
}

// memory_remove and memory_sync take a required `context` and act on imported
// contexts — neither is a way to delete a memory. Saying so is the point: the
// names read like the opposite.
func TestMemoryRuleContentDistinguishesContextToolsFromDelete(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	for _, want := range []string{
		brand.MCPToolName("memory", "remove") + "(project_dir: \"/path/to/project\", context:",
		brand.MCPToolName("memory", "sync") + "(project_dir: \"/path/to/project\", context:",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("memory skill does not show the context form: %q", want)
		}
	}
}
