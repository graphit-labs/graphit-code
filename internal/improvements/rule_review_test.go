package improvements

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// This skill sent the agent to web search for unfamiliar errors, library quirks
// and "which library should I use" without the Hub having been tried — directly
// against the Hub mandate. The gate now sits above that whole section.
func TestImprovementsRuleContentGatesWebSearchBehindTheHub(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	if !strings.Contains(content, "the Hub Comes First") {
		t.Error("skill does not gate internet search behind the Hub")
	}
	if !strings.Contains(content, brand.MCPToolName("hub", "search")) {
		t.Error("skill never names hub_search where it tells the agent to search the web")
	}
	// The internal equivalent: this project's own graph and wiki know more about
	// this project than any search engine does.
	for _, want := range []string{
		brand.MCPToolName("ast", "search"),
		brand.MCPToolName("knowledge", "search"),
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not point at %s before web search", want)
		}
	}
}

// The decision gate told the agent to check the decisions directory by hand and to
// look for // DECISION: comments — both now have a harness tool that covers the
// whole codebase in one call instead of the file in front of it.
func TestImprovementsRuleContentValidatesDecisionsThroughTheHarness(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	if strings.Contains(content, "Check `docs/decisions/` for an architectural decision record") {
		t.Error("decision gate still tells the agent to browse the decisions directory")
	}
	if !strings.Contains(content, "MATCH (c:Comment)") {
		t.Error("decision gate does not query comments from the graph")
	}
}
