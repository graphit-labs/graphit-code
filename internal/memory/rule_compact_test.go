package memory

import "testing"

func TestMemoryInstructionContextBudgets(t *testing.T) {
	t.Parallel()
	// Allow the explicit-reference contract without removing prior invariants.
	content := RuleContent(nil)
	if size := len(content); size == 0 || size > 6950 {
		t.Errorf("Memory skill outside its context budget: %d bytes", size)
	}
	if size := len(MandateTrigger()); size == 0 || size > 1300 {
		t.Errorf("Memory resident mandate outside its context budget: %d bytes", size)
	}
	if RuleContent([]string{"unrelated-context"}) != content {
		t.Fatal("Memory skill eagerly injects unrelated contexts")
	}
}
