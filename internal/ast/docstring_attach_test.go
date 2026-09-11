package ast

import (
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

func collectDeclSites(root *sitter.Node, m docstringMatchers) []*sitter.Node {
	if !m.on {
		return nil
	}
	var out []*sitter.Node
	var walk func(*sitter.Node)
	walk = func(n *sitter.Node) {
		for i := 0; i < SafeChildCount(n); i++ {
			child := SafeChild(n, i)
			if SafeIsNull(child) {
				continue
			}
			if m.decl.match(child) {
				out = append(out, child)
			}
			walk(child)
		}
	}
	walk(root)
	return out
}

const docstringGoSrc = `package p

// Alpha does alpha things.
func Alpha() {}

// Beta is documented too.
//
// With a second paragraph.
func Beta(x int) error { return nil }

func Undocumented() {}

// TypeDoc documents a type.
type Widget struct{}
`

const docstringPySrc = `# leading comment
def alpha():
    """Alpha docstring."""
    pass

# beta comment
def beta(x):
    return x

class Gamma:
    """Gamma docstring."""
    pass
`

func runDocstringCase(t *testing.T, langName, source string, decls []string, entities []Entity, want map[string]string) {
	t.Helper()
	lang, err := resolveTreeSitterLang(langName, "tree-sitter-"+langName)
	if err != nil || lang == nil {
		t.Skipf("grammar %s unavailable: %v", langName, err)
	}
	p := sitter.NewParser()
	defer p.Close()
	if err := p.SetLanguage(lang); err != nil {
		t.Fatalf("set language: %v", err)
	}
	src := []byte(source)
	tree := p.Parse(src, nil)
	if tree == nil {
		t.Fatal("nil tree")
	}
	defer tree.Close()

	result := &ParsedFile{Entities: map[string][]Entity{"functions": entities}}
	m := newDocstringMatchers(&ExternalQueryFile{DeclarationTypes: decls}, lang)
	attachDocstringsTS(collectDeclSites(tree.RootNode(), m), src, result, m)

	for _, entity := range result.Entities["functions"] {
		expected, ok := want[entity.Name]
		if !ok {
			t.Fatalf("%s entity %q has no explicit expectation", langName, entity.Name)
		}
		if entity.Docstring != expected {
			t.Errorf("%s entity %q (line %d): docstring %q, want %q",
				langName, entity.Name, entity.Line, entity.Docstring, expected)
		}
	}
}

func TestAttachDocstringsUsesCurrentLanguageContract(t *testing.T) {
	t.Run("go comments", func(t *testing.T) {
		runDocstringCase(t, "go", docstringGoSrc,
			[]string{"function_declaration", "type_declaration"},
			[]Entity{
				{Name: "Alpha", Line: 4, GraphLabel: "Function"},
				{Name: "Beta", Line: 9, GraphLabel: "Function"},
				{Name: "Undocumented", Line: 11, GraphLabel: "Function"},
			},
			map[string]string{
				"Alpha":        "Alpha does alpha things.",
				"Beta":         "With a second paragraph.",
				"Undocumented": "",
			})
	})
	t.Run("python comments and string literals", func(t *testing.T) {
		runDocstringCase(t, "python", docstringPySrc,
			[]string{"function_definition", "class_definition"},
			[]Entity{
				{Name: "alpha", Line: 2, GraphLabel: "Function"},
				{Name: "beta", Line: 7, GraphLabel: "Function"},
				{Name: "Gamma", Line: 10, GraphLabel: "Class"},
			},
			map[string]string{
				"alpha": "leading comment",
				"beta":  "beta comment",
				"Gamma": "Gamma docstring.",
			})
	})
	t.Run("unknown declaration kinds use string matching", func(t *testing.T) {
		runDocstringCase(t, "go", docstringGoSrc,
			[]string{"function_declaration", "not_a_real_kind_xyz"},
			[]Entity{{Name: "Alpha", Line: 4, GraphLabel: "Function"}},
			map[string]string{"Alpha": "Alpha does alpha things."})
	})
}
