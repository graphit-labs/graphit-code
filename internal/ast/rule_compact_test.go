package ast

import "testing"

func TestASTInstructionContextBudgets(t *testing.T) {
	t.Parallel()
	// 13100 allows the project-version contract (stores follow the checked-out branch).
	if size := len(ASTRuleContent()); size == 0 || size > 13100 {
		t.Errorf("AST skill outside its context budget: %d bytes", size)
	}
	if size := len(MandateTrigger()); size == 0 || size > 1000 {
		t.Errorf("AST resident mandate outside its context budget: %d bytes", size)
	}
}
