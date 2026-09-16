package ast

import "testing"

func TestASTInstructionContextBudgets(t *testing.T) {
	t.Parallel()
	if size := len(ASTRuleContent()); size == 0 || size > 13000 {
		t.Errorf("AST skill outside its context budget: %d bytes", size)
	}
	if size := len(MandateTrigger()); size == 0 || size > 1000 {
		t.Errorf("AST resident mandate outside its context budget: %d bytes", size)
	}
}
