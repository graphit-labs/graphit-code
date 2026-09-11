package ast

import (
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

func loadGoLangForComplexityTest(t *testing.T) *sitter.Language {
	t.Helper()
	lang := NativeLanguage("go")
	if lang == nil {
		t.Fatal("native Go grammar is unavailable")
	}
	return lang
}

func parseGoForComplexityTest(t *testing.T, lang *sitter.Language, src string) (*sitter.Node, []byte) {
	t.Helper()
	p := sitter.NewParser()
	t.Cleanup(p.Close)
	if err := p.SetLanguage(lang); err != nil {
		t.Fatalf("SetLanguage: %v", err)
	}
	b := []byte(src)
	tree := p.Parse(b, nil)
	t.Cleanup(tree.Close)
	root := tree.RootNode()
	for i := 0; i < int(root.ChildCount()); i++ {
		if c := root.Child(uint(i)); c.Kind() == "function_declaration" {
			return c, b
		}
	}
	t.Fatal("no function_declaration found in source")
	return nil, nil
}

func goComplexityConfig() *ExternalQueryFile {
	return &ExternalQueryFile{
		Language: "go",
		Complexity: &ComplexityConfig{
			NodeTypes: []string{"if_statement", "for_statement", "expression_case", "communication_case"},
			Operators: []string{"&&", "||"},
		},
		ContextTypes: map[string]string{
			"function_declaration": "Function",
			"func_literal":         "Function",
		},
	}
}

func TestComplexityWalksRealSyntaxTree(t *testing.T) {
	lang := loadGoLangForComplexityTest(t)
	langConfig := goComplexityConfig()

	tests := []struct {
		name string
		src  string
		want int
	}{
		{
			name: "straight line — no branches",
			src:  "package p\nfunc f() int {\n\treturn 1\n}",
			want: 1,
		},
		{
			name: "if plus chained else-if — two if_statement nodes",
			src: "package p\nfunc f(x int) int {\n" +
				"\tif x > 0 {\n\t\treturn x\n\t} else if x < 0 {\n\t\treturn -x\n\t}\n" +
				"\treturn 0\n}",
			want: 3,
		},
		{
			name: "for loop with a boolean-combined condition inside",
			src: "package p\nfunc f(a, b, c bool) int {\n" +
				"\tfor i := 0; i < 10; i++ {\n\t\tif a && b || c {\n\t\t\tcontinue\n\t\t}\n\t}\n" +
				"\treturn 0\n}",
			want: 5,
		},
		{
			name: "switch with two case labels, no default",
			src: "package p\nfunc f(x int) int {\n" +
				"\tswitch x {\n\tcase 1:\n\t\treturn 1\n\tcase 2:\n\t\treturn 2\n\t}\n" +
				"\treturn 0\n}",
			want: 3,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fn, src := parseGoForComplexityTest(t, lang, tt.src)
			m := newComplexityMatcher(langConfig, lang)
			if !m.on {
				t.Fatal("matcher did not activate — Complexity config was not picked up")
			}
			if got := m.score(fn, src); got != tt.want {
				t.Errorf("score() = %d, want %d\nsource:\n%s", got, tt.want, src)
			}
		})
	}
}

func TestComplexityStopsAtNestedDeclaration(t *testing.T) {
	lang := loadGoLangForComplexityTest(t)
	langConfig := goComplexityConfig()

	src := "package p\nfunc outer(x int) int {\n" +
		"\tif x > 0 {\n\t\treturn x\n\t}\n" +
		"\tinner := func(y int) int {\n" +
		"\t\tif y > 0 {\n\t\t\treturn y\n\t\t} else if y < 0 {\n\t\t\treturn -y\n\t\t}\n" +
		"\t\treturn 0\n\t}\n" +
		"\treturn inner(x)\n}"

	p := sitter.NewParser()
	t.Cleanup(p.Close)
	if err := p.SetLanguage(lang); err != nil {
		t.Fatalf("SetLanguage: %v", err)
	}
	b := []byte(src)
	tree := p.Parse(b, nil)
	t.Cleanup(tree.Close)
	root := tree.RootNode()

	var outer *sitter.Node
	for i := 0; i < int(root.ChildCount()); i++ {
		if c := root.Child(uint(i)); c.Kind() == "function_declaration" {
			outer = c
		}
	}
	if outer == nil {
		t.Fatal("no function_declaration found")
	}

	m := newComplexityMatcher(langConfig, lang)
	if !m.on {
		t.Fatal("matcher did not activate")
	}
	if got, want := m.score(outer, b), 2; got != want {
		t.Errorf("score() = %d, want %d — nested declaration was not skipped\nsource:\n%s", got, want, b)
	}
}

func TestComplexityMatcherOffWithoutConfig(t *testing.T) {
	lang := loadGoLangForComplexityTest(t)
	if m := newComplexityMatcher(nil, lang); m.on {
		t.Error("nil langConfig should leave the matcher off")
	}
	if m := newComplexityMatcher(&ExternalQueryFile{Language: "go"}, lang); m.on {
		t.Error("langConfig without a Complexity block should leave the matcher off")
	}
}
