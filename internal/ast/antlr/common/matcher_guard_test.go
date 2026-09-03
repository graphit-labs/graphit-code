package antlrcommon

import "testing"

// A grammar can spell two different things with one rule, and then the rule name
// alone cannot tell them apart. Oracle is the case that forced this: there is no
// constant_declaration rule, a constant is a variable_declaration carrying the
// CONSTANT keyword, so a query written against the rule name indexed every
// constant as a variable and the query against the rule that does not exist
// matched nothing at all.
func TestKeywordGuard(t *testing.T) {
	tree := &TreeNode{Rule: "block", Children: []*TreeNode{
		{Rule: "decl", Children: []*TreeNode{
			{Rule: "id", Children: []*TreeNode{term("C_MAX")}},
			term("CONSTANT"),
		}},
		{Rule: "decl", Children: []*TreeNode{
			{Rule: "id", Children: []*TreeNode{term("V_URL")}},
		}},
		{Rule: "decl", Children: []*TreeNode{
			{Rule: "id", Children: []*TreeNode{term("C_MIN")}},
			term("constant"),
		}},
	}}

	for _, tc := range []struct {
		pattern string
		want    []string
	}{
		{"//decl", []string{"C_MAX", "V_URL", "C_MIN"}},
		{"//decl[CONSTANT]", []string{"C_MAX", "C_MIN"}},
		{"//decl[!CONSTANT]", []string{"V_URL"}},
		{"//decl[constant]", []string{"C_MAX", "C_MIN"}},
		{"//decl[CONSTANT]/id", []string{"C_MAX", "C_MIN"}},
	} {
		p, err := CompilePattern(tc.pattern)
		if err != nil {
			t.Errorf("%s: compile: %v", tc.pattern, err)
			continue
		}
		var got []string
		for _, m := range p.Match(tree) {
			got = append(got, m.Node.FirstTerminalText())
		}
		if !equalStrings(got, tc.want) {
			t.Errorf("%s: got %v, want %v", tc.pattern, got, tc.want)
		}
	}
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestKeywordGuardRejectsMalformedPatterns(t *testing.T) {
	for _, pattern := range []string{
		"//decl[CONSTANT",
		"//decl[]",
		"//decl[!]",
		"//[CONSTANT]",
		"//decl[ ! ]",
	} {
		if _, err := CompilePattern(pattern); err == nil {
			t.Errorf("CompilePattern(%q) accepted a malformed pattern", pattern)
		}
	}
}

// A guard only looks at direct terminal children. A keyword buried in a nested
// rule belongs to that rule, not to this one.
func TestKeywordGuardIgnoresNestedTerminals(t *testing.T) {
	tree := &TreeNode{Rule: "block", Children: []*TreeNode{
		{Rule: "decl", Children: []*TreeNode{
			{Rule: "id", Children: []*TreeNode{term("X")}},
			{Rule: "inner", Children: []*TreeNode{term("CONSTANT")}},
		}},
	}}
	p, err := CompilePattern("//decl[CONSTANT]")
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if got := p.Match(tree); len(got) != 0 {
		t.Errorf("a CONSTANT nested under `inner` satisfied the guard on `decl`")
	}
	p, err = CompilePattern("//decl[!CONSTANT]")
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if got := p.Match(tree); len(got) != 1 {
		t.Errorf("got %d matches for [!CONSTANT], want 1", len(got))
	}
}
