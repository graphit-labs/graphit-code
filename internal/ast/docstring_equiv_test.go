package ast

import (
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// legacyExtractDocstringsTS is the previous string-comparison implementation,
// kept as the oracle for the differential test.
func legacyExtractDocstringsTS(root *sitter.Node, src []byte, result *ParsedFile, langConfig *ExternalQueryFile) {
	if SafeIsNull(root) {
		return
	}
	var declTypes, comTypes map[string]bool
	if langConfig != nil && len(langConfig.DeclarationTypes) > 0 {
		declTypes = make(map[string]bool, len(langConfig.DeclarationTypes))
		for _, dt := range langConfig.DeclarationTypes {
			declTypes[dt] = true
		}
	}
	if langConfig != nil && len(langConfig.CommentTypes) > 0 {
		comTypes = make(map[string]bool, len(langConfig.CommentTypes))
		for _, ct := range langConfig.CommentTypes {
			comTypes[ct] = true
		}
	}
	if declTypes == nil {
		return
	}
	if comTypes == nil {
		comTypes = defaultCommentTypes
	}
	type entityKey struct {
		line int
		name string
	}
	entityIdx := make(map[entityKey]*Entity)
	for dataKey := range result.Entities {
		for i := range result.Entities[dataKey] {
			e := &result.Entities[dataKey][i]
			if e.GraphLabel != "" {
				entityIdx[entityKey{e.Line, e.Name}] = e
			}
		}
	}
	var walk func(node *sitter.Node)
	walk = func(node *sitter.Node) {
		childCount := SafeChildCount(node)
		for i := 0; i < childCount; i++ {
			child := SafeChild(node, i)
			if SafeIsNull(child) {
				continue
			}
			nodeType := SafeType(child)
			if declTypes[nodeType] {
				if i > 0 {
					prev := SafeChild(node, i-1)
					if !SafeIsNull(prev) && comTypes[SafeType(prev)] {
						commentText := cleanDocstring(prev.Utf8Text(src))
						if commentText != "" {
							sp := child.StartPosition()
							declLine := int(sp.Row) + 1
							nameNode := SafeChildByFieldName(child, "name")
							if !SafeIsNull(nameNode) {
								if e, ok := entityIdx[entityKey{declLine, nameNode.Utf8Text(src)}]; ok {
									e.Docstring = commentText
								}
							}
						}
					}
				}
				if nodeType == "function_definition" || nodeType == "class_definition" {
					body := SafeChildByFieldName(child, "body")
					if !SafeIsNull(body) && SafeChildCount(body) > 0 {
						firstStmt := SafeChild(body, 0)
						if !SafeIsNull(firstStmt) && SafeType(firstStmt) == "expression_statement" && SafeChildCount(firstStmt) > 0 {
							expr := SafeChild(firstStmt, 0)
							if !SafeIsNull(expr) && SafeType(expr) == "string" {
								sp := child.StartPosition()
								declLine := int(sp.Row) + 1
								nameNode := SafeChildByFieldName(child, "name")
								if !SafeIsNull(nameNode) {
									if e, ok := entityIdx[entityKey{declLine, nameNode.Utf8Text(src)}]; ok && e.Docstring == "" {
										e.Docstring = cleanDocstring(expr.Utf8Text(src))
									}
								}
							}
						}
					}
				}
			}
			walk(child)
		}
	}
	walk(root)
}

// collectDeclSites enumerates declaration nodes by scanning the whole tree — the
// way the production code used to find them, before the query pass started
// handing them over directly. Feeding these to attachDocstringsTS isolates the
// pairing logic from the change in how sites are gathered, so the differential
// against the legacy implementation still means something.
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

func runDocstringCase(t *testing.T, langName, source string, decls []string, entities []Entity) {
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
	root := tree.RootNode()

	cfg := &ExternalQueryFile{DeclarationTypes: decls}

	mk := func() *ParsedFile {
		cp := make([]Entity, len(entities))
		copy(cp, entities)
		return &ParsedFile{Entities: map[string][]Entity{"functions": cp}}
	}
	newPF, oldPF := mk(), mk()

	m := newDocstringMatchers(cfg, lang)
	attachDocstringsTS(collectDeclSites(root, m), src, newPF, m)
	legacyExtractDocstringsTS(root, src, oldPF, cfg)

	got, want := newPF.Entities["functions"], oldPF.Entities["functions"]
	docs := 0
	for i := range got {
		if got[i].Docstring != want[i].Docstring {
			t.Errorf("%s entity %q (line %d): docstring %q, want %q",
				langName, got[i].Name, got[i].Line, got[i].Docstring, want[i].Docstring)
		}
		if got[i].Docstring != "" {
			docs++
		}
	}
	t.Logf("%s: %d/%d entities got a docstring (identical to legacy)", langName, docs, len(got))
	if docs == 0 {
		t.Errorf("%s: no docstrings extracted at all — the test would not detect a regression", langName)
	}
}

func TestExtractDocstringsMatchesLegacy(t *testing.T) {
	t.Run("go", func(t *testing.T) {
		runDocstringCase(t, "go", docstringGoSrc,
			[]string{"function_declaration", "type_declaration"},
			[]Entity{
				{Name: "Alpha", Line: 4, GraphLabel: "Function"},
				{Name: "Beta", Line: 9, GraphLabel: "Function"},
				{Name: "Undocumented", Line: 11, GraphLabel: "Function"},
			})
	})
	t.Run("python", func(t *testing.T) {
		runDocstringCase(t, "python", docstringPySrc,
			[]string{"function_definition", "class_definition"},
			[]Entity{
				{Name: "alpha", Line: 2, GraphLabel: "Function"},
				{Name: "beta", Line: 7, GraphLabel: "Function"},
				{Name: "Gamma", Line: 10, GraphLabel: "Class"},
			})
	})
	t.Run("unknown kinds fall back to string matching", func(t *testing.T) {
		runDocstringCase(t, "go", docstringGoSrc,
			[]string{"function_declaration", "not_a_real_kind_xyz"},
			[]Entity{{Name: "Alpha", Line: 4, GraphLabel: "Function"}})
	})
}
