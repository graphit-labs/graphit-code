package antlrcommon

import (
	"testing"
)

func TestCompilePattern(t *testing.T) {
	tests := []struct {
		input    string
		wantErr  bool
		segments int
	}{
		{"//functionDeclaration", false, 1},
		{"//a/b", false, 2},
		{"//a//b", false, 2},
		{"/a/b/c", false, 3},
		{"//a/b//c/d", false, 4},
		{"", true, 0},
		{"functionDeclaration", true, 0}, // no leading /
		{"//", true, 0},                  // empty rule name
		{"/", true, 0},                   // empty rule name
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			p, err := CompilePattern(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error for pattern %q", tt.input)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if len(p.segments) != tt.segments {
				t.Fatalf("got %d segments, want %d", len(p.segments), tt.segments)
			}
		})
	}
}

func TestPatternMatch(t *testing.T) {
	// Build a sample ANTLR-style parse tree:
	// compilationUnit
	//   functionDeclaration
	//     functionName
	//       IDENTIFIER "myFunc"
	//     parameterList
	//       parameter
	//         IDENTIFIER "x"
	//   functionDeclaration
	//     functionName
	//       IDENTIFIER "other"
	tree := &TreeNode{
		Rule:  "compilationUnit",
		Start: [2]int{1, 0}, End: [2]int{20, 0},
		Children: []*TreeNode{
			{
				Rule:  "functionDeclaration",
				Start: [2]int{1, 0}, End: [2]int{10, 0},
				Children: []*TreeNode{
					{
						Rule:  "functionName",
						Start: [2]int{1, 5}, End: [2]int{1, 11},
						Children: []*TreeNode{
							{Token: "IDENTIFIER", Text: "myFunc", Start: [2]int{1, 5}, End: [2]int{1, 11}},
						},
					},
					{
						Rule:  "parameterList",
						Start: [2]int{1, 12}, End: [2]int{1, 14},
						Children: []*TreeNode{
							{
								Rule:  "parameter",
								Start: [2]int{1, 12}, End: [2]int{1, 13},
								Children: []*TreeNode{
									{Token: "IDENTIFIER", Text: "x", Start: [2]int{1, 12}, End: [2]int{1, 13}},
								},
							},
						},
					},
				},
			},
			{
				Rule:  "functionDeclaration",
				Start: [2]int{12, 0}, End: [2]int{20, 0},
				Children: []*TreeNode{
					{
						Rule:  "functionName",
						Start: [2]int{12, 5}, End: [2]int{12, 10},
						Children: []*TreeNode{
							{Token: "IDENTIFIER", Text: "other", Start: [2]int{12, 5}, End: [2]int{12, 10}},
						},
					},
				},
			},
		},
	}

	tests := []struct {
		pattern string
		want    int    // expected number of matches
		first   string // first match's FirstTerminalText
	}{
		{"//functionDeclaration", 2, "myFunc"},
		{"//functionDeclaration/functionName", 2, "myFunc"},
		{"//functionName", 2, "myFunc"},
		{"//parameterList/parameter", 1, "x"},
		{"//parameter", 1, "x"},
		{"//compilationUnit/functionDeclaration", 2, "myFunc"},
		{"/compilationUnit", 1, "myFunc"}, // direct child from root — root itself matches
		{"//nonExistent", 0, ""},
		{"//functionDeclaration/parameterList/parameter", 1, "x"},
	}

	for _, tt := range tests {
		t.Run(tt.pattern, func(t *testing.T) {
			p, err := CompilePattern(tt.pattern)
			if err != nil {
				t.Fatalf("compile: %v", err)
			}

			results := p.Match(tree)
			if len(results) != tt.want {
				t.Fatalf("got %d matches, want %d", len(results), tt.want)
			}
			if tt.want > 0 && tt.first != "" {
				got := results[0].Node.FirstTerminalText()
				if got != tt.first {
					t.Fatalf("first match text: got %q, want %q", got, tt.first)
				}
			}
		})
	}
}

func TestPatternMatchDirectChild(t *testing.T) {
	tree := &TreeNode{
		Rule: "root",
		Children: []*TreeNode{
			{Rule: "a", Children: []*TreeNode{
				{Rule: "b", Children: []*TreeNode{
					{Token: "ID", Text: "deep_b"},
				}},
			}},
			{Rule: "b", Children: []*TreeNode{
				{Token: "ID", Text: "direct_b"},
			}},
		},
	}

	// /root/b should only match the direct child "b", not the nested one
	p, err := CompilePattern("/root/b")
	if err != nil {
		t.Fatal(err)
	}
	results := p.Match(tree)
	if len(results) != 1 {
		t.Fatalf("got %d matches, want 1", len(results))
	}
	if results[0].Node.FirstTerminalText() != "direct_b" {
		t.Fatalf("matched wrong node: %q", results[0].Node.FirstTerminalText())
	}

	// //b should match both
	p2, _ := CompilePattern("//b")
	results2 := p2.Match(tree)
	if len(results2) != 2 {
		t.Fatalf("got %d matches, want 2", len(results2))
	}
}

func TestTreeNodeFirstTerminalText(t *testing.T) {
	node := &TreeNode{
		Rule: "wrapper",
		Children: []*TreeNode{
			{Rule: "inner", Children: []*TreeNode{
				{Token: "ID", Text: "found"},
			}},
		},
	}
	if got := node.FirstTerminalText(); got != "found" {
		t.Fatalf("got %q, want %q", got, "found")
	}

	empty := &TreeNode{Rule: "empty"}
	if got := empty.FirstTerminalText(); got != "" {
		t.Fatalf("got %q, want empty", got)
	}
}
