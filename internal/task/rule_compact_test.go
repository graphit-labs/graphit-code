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
		"mandate":     {MandateTrigger(), 1500},
		// Detailed authoring and worked examples load through generated references.
		//
		// RAISED FROM 10000 TO 10800. The skill had been engineered to 9996 bytes — four under
		// the old ceiling — so the two additions this session required could not fit at any
		// wording: the structured-query route, and the explicit resume-or-open judgement for
		// sessions. Both were asked for directly, and neither is a restatement of something
		// already here.
		//
		// The alternative was cutting 569 bytes of existing instruction, and there was no
		// passage weak enough to make that an improvement rather than a silent trade of one
		// requirement for another. What was compressible WAS compressed: the mandate's
		// "revise when scope changes" merged into the new resume sentence because they had
		// become the same instruction, and both new passages were rewritten shorter twice.
		//
		// The new ceiling leaves ~230 bytes, deliberately little. A budget that always has
		// room stops being a budget; the next addition should meet this same wall and force
		// the same deliberate choice.
		"skill": {RuleContent(), 10800},
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
