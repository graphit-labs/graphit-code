package improvements

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The methodology in this skill is a default that a project or a Hub artifact can
// replace. improvements_rules returns the version actually in force, and without
// it the agent reviews against rules this codebase may have deliberately
// overridden — reporting its own conventions as defects.
func TestImprovementsRuleContentTeachesTheRulesTool(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	rules := brand.MCPToolName("improvements", "rules")
	if !strings.Contains(content, rules+"()") {
		t.Errorf("improvements skill never shows how to call %s", rules)
	}
	if !strings.Contains(content, rules+"(default: true)") {
		t.Errorf("improvements skill never shows the default:true form of %s", rules)
	}
}
