package memory

import "testing"

func TestMemoryInstructionContextBudgets(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)
	if size := len(content); size == 0 || size > 5500 {
		t.Errorf("Memory skill outside its context budget: %d bytes", size)
	}
	if size := len(MandateTrigger()); size == 0 || size > 1100 {
		t.Errorf("Memory resident mandate outside its context budget: %d bytes", size)
	}
	if RuleContent([]string{"unrelated-context"}) != content {
		t.Fatal("Memory skill eagerly injects unrelated contexts")
	}
}
