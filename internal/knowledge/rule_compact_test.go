package knowledge

import (
	"strings"
	"testing"
)

func TestKnowledgeInstructionContextBudgets(t *testing.T) {
	t.Parallel()
	// Retrieval and per-unit invariants stay resident; conditional authoring detail
	// is generated separately and loaded only at its documented decision boundary.
	if size := len(KnowledgeRuleContent(nil, "docs")); size == 0 || size > 6200 {
		t.Errorf("Knowledge skill outside its context budget: %d bytes", size)
	} else {
		t.Logf("Knowledge entrypoint: %d bytes", size)
	}
	if size := len(MandateTrigger()); size == 0 || size > 1000 {
		t.Errorf("Knowledge resident mandate outside its context budget: %d bytes", size)
	} else {
		t.Logf("Knowledge mandate: %d bytes", size)
	}
	for name, content := range SkillReferences() {
		if len(content) > 30000 {
			t.Errorf("conditional reference %s exceeds 30000 bytes: %d", name, len(content))
		}
		t.Logf("Knowledge conditional resource %s: %d bytes", name, len(content))
	}
}

func TestKnowledgeSkillUsesConfiguredDocsWithoutPreloadingContexts(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent([]string{"private-unused-context"}, "handbook")
	if !strings.Contains(content, "`handbook/`") || strings.Contains(content, "`docs/`") {
		t.Fatal("Knowledge instructions ignore the configured authoritative documentation scope")
	}
	if strings.Contains(content, "private-unused-context") {
		t.Fatal("Knowledge skill eagerly injects unrelated installed contexts")
	}
	if !strings.Contains(KnowledgeRuleContent(nil, ""), "`docs/`") {
		t.Fatal("Knowledge instructions lack the default documentation scope")
	}
}
