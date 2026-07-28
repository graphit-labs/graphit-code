package memory

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// project_dir is not fixed to the project the agent is sitting in. A sibling's
// memories are the only record of why it behaves as it does — reading its source
// reconstructs what it does, never the decision behind it.
func TestMemoryRuleContentCoversSiblingMemories(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	for _, want := range []string{
		brand.MCPToolName("cluster", "projects") + "(project_dir",
		brand.MCPToolName("memory", "search") + "(project_dir: \"<sibling dir>\"",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("memory skill does not show how to read a sibling's memories: missing %q", want)
		}
	}
}
