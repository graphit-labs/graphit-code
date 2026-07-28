package ast

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// A tool the skill never names does not exist as far as the agent is concerned:
// the mandate promises the module owns it, and then the skill it points at is
// silent about how to call it. This test is the invariant that closes that gap —
// every tool the module owns has to be reachable from its own skill.
func TestASTRuleContentTeachesEveryASTTool(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, action := range []string{
		"search", "query", "schema", "source",
		"list", "index", "embed", "export", "install", "remove",
	} {
		if !strings.Contains(content, brand.MCPToolName("ast", action)) {
			t.Errorf("ast skill never mentions %s", brand.MCPToolName("ast", action))
		}
	}
}

// The skill tells the agent not to pass `context` for its own project, which only
// makes sense if it also says where a context comes from.
func TestASTRuleContentExplainsImportedContexts(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, want := range []string{
		"Imported Contexts",
		brand.MCPToolName("ast", "install") + "(project_dir",
		brand.MCPToolName("ast", "list") + "(project_dir",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("ast skill is missing %q", want)
		}
	}
}
