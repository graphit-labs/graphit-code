package hub

import "testing"

func TestHubInstructionContextBudgets(t *testing.T) {
	t.Parallel()
	if size := len(HubRuleContent()); size == 0 || size > 5600 {
		t.Errorf("Hub skill outside its context budget: %d bytes", size)
	}
	if size := len(MandateTrigger()); size == 0 || size > 1000 {
		t.Errorf("Hub resident mandate outside its context budget: %d bytes", size)
	}
}
