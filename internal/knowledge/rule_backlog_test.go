package knowledge

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestKnowledgeRuleOwnsDreamIndependentTaskBacklogGuidance(t *testing.T) {
	content := KnowledgeRuleContent(nil, "docs")

	for _, tool := range []string{
		brand.MCPToolRef("backlog", "list"),
		brand.MCPToolRef("backlog", "add"),
		brand.MCPToolRef("backlog", "remove"),
	} {
		if !strings.Contains(content, tool) {
			t.Errorf("knowledge skill does not teach %s", tool)
		}
	}
	if !strings.Contains(content, "Dream never consumes backlog items") {
		t.Error("knowledge skill does not state the strict Dream/backlog boundary")
	}
	if strings.Contains(content, "optional consumer") || strings.Contains(content, "pending item later") {
		t.Error("knowledge skill still describes Dream as a backlog consumer")
	}
}

func TestKnowledgeMandateOwnsBacklogWithoutDreamGate(t *testing.T) {
	mandate := MandateTrigger()

	for _, tool := range []string{
		brand.MCPToolName("backlog", "list"),
		brand.MCPToolName("backlog", "add"),
		brand.MCPToolName("backlog", "remove"),
	} {
		if !strings.Contains(mandate, tool) {
			t.Errorf("knowledge mandate does not list %s", tool)
		}
	}
	if strings.Contains(mandate, "Dream must be enabled") || strings.Contains(mandate, "Dream needs to be enabled") {
		t.Error("knowledge mandate gates task recording on Dream")
	}
	if !strings.Contains(mandate, "Dream never consumes backlog items") {
		t.Error("knowledge mandate does not state the strict Dream/backlog boundary")
	}
}
