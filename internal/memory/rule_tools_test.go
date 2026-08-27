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
		"promote", "demote", "index", "schema", "export", "remove", "sync",
	} {
		if !strings.Contains(content, brand.MCPToolName("memory", action)) {
			t.Errorf("memory skill never mentions %s", brand.MCPToolName("memory", action))
		}
	}
}

// The inverse of the test above, for the tool that was removed: consolidation is
// not something the agent delegates. It has the task context that makes each
// judgement correct, so it resolves duplicates and contradictions with update and
// delete as it reads them — the Sanitise On Sight protocol. A tool would hand it a
// way to trade that judgement for a batch job's caution, and a garbage collector
// keyed on age would delete memories for having gone unread.
func TestMemoryRuleContentOffersNoConsolidateOrGCTool(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	for _, gone := range []string{"gc", "consolidate"} {
		if strings.Contains(content, brand.MCPToolName("memory", gone)) {
			t.Errorf("memory skill mentions %s, which is not a tool", brand.MCPToolName("memory", gone))
		}
	}
	if !strings.Contains(content, "There is no consolidation tool, and no garbage collection tool") {
		t.Error("skill does not tell the agent those tools do not exist")
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
