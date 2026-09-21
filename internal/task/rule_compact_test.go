package task

import (
	"strings"
	"testing"
)

// This checks the context cost of the actual generated output. Semantic quality
// is evaluated with cold-start planning scenarios, not exact prose snapshots.
func TestTaskInstructionBudgets(t *testing.T) {
	t.Parallel()
	for name, instruction := range map[string]struct {
		text string
		max  int
	}{
		"description": {skillDescription, 220},
		"mandate":     {MandateTrigger(), 1750},
		// Explicit typed references are a new required write contract. Retain the
		// existing lifecycle guidance instead of trading it for this requirement.
		"skill": {RuleContent(), 12200},
	} {
		t.Run(name, func(t *testing.T) {
			if strings.TrimSpace(instruction.text) == "" {
				t.Fatal("generated instruction is empty")
			}
			if len(instruction.text) > instruction.max {
				t.Fatalf("%s context budget exceeded: %d bytes (maximum %d)", name, len(instruction.text), instruction.max)
			}
			t.Logf("%s: %d bytes", name, len(instruction.text))
		})
	}
}
