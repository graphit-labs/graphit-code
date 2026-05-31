package improvements

import (
	"strings"
	"testing"
)

func TestImprovementsRuleBasic(t *testing.T) {
	ruleContent := ImprovementsRuleContent()
	if !strings.Contains(ruleContent, "# Code Improvement Methodology Rule") {
		t.Errorf("unexpected rule content: %q", ruleContent)
	}

	routerContent := ImprovementsRouterContent("AGENTS.md")
	if !strings.Contains(routerContent, "# 🔧 Code Improvement Methodology") {
		t.Errorf("unexpected router content: %q", routerContent)
	}
}
