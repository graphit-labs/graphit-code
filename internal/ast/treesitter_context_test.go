package ast

import (
	"testing"
)

// Data-format entities used to be contained by the File: the ancestor walk named
// a context by reading its "name" field, and no data-format grammar has one.
// These pin the containment the grammars actually describe.

func TestXMLElementIsContainedByEnclosingElement(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml", `<server>
  <database>
    <host>db.example.com</host>
  </database>
</server>`)
	got := nodesOf(pf)

	wantNode(t, got, "Element", "database", "server", "Element")
	wantNode(t, got, "Element", "host", "database", "Element")
}

// The element an entity names is not its own parent. `(STag (Name) @name)`
// captures a Name that sits inside the very element it names, so the naive walk
// made every element its own container.
func TestXMLElementIsNotItsOwnParent(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml", `<outer><inner>x</inner></outer>`)

	for _, ents := range pf.Entities {
		for _, e := range ents {
			if e.GraphLabel == "Element" && e.Name == e.Context {
				t.Errorf("Element %q is its own parent", e.Name)
			}
		}
	}
}

// The outermost element has nothing above it, so it is contained by the file —
// that is the one case where an empty context is the right answer.
func TestXMLRootElementHasNoParent(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml", `<root><child>x</child></root>`)
	got := nodesOf(pf)

	root, ok := got[[2]string{"Element", "root"}]
	if !ok {
		t.Fatal("no Element named root")
	}
	if root.parent != "" {
		t.Errorf("root element is contained by %q, want the file", root.parent)
	}
	wantNode(t, got, "Element", "child", "root", "Element")
}

// Deep nesting is where the memoised walk has to agree with the naive one: each
// level must name the level directly above it, not the outermost one.
func TestXMLDeepNestingNamesTheImmediateParent(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml",
		`<a><b><c><d><e>leaf</e></d></c></b></a>`)
	got := nodesOf(pf)

	for _, pair := range [][2]string{{"b", "a"}, {"c", "b"}, {"d", "c"}, {"e", "d"}} {
		wantNode(t, got, "Element", pair[0], pair[1], "Element")
	}
}

// Siblings share every ancestor, which is exactly what the memo caches. If the
// cache were keyed wrong they would inherit each other's answer.
func TestXMLSiblingsShareTheirParentWithoutBorrowingIt(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml", `<cfg>
  <left><x>1</x></left>
  <right><y>2</y></right>
</cfg>`)
	got := nodesOf(pf)

	wantNode(t, got, "Element", "left", "cfg", "Element")
	wantNode(t, got, "Element", "right", "cfg", "Element")
	wantNode(t, got, "Element", "x", "left", "Element")
	wantNode(t, got, "Element", "y", "right", "Element")
}

// A grammar that declares no context types is describing its tree, not asking
// for the programming-language defaults. Before this, `context_types: {}` fell
// through to defaultContextTypes (class_declaration, function_definition, …),
// which cannot match a data-format tree and made every entity walk to the root.
func TestDeclaredEmptyContextTypesDoesNotFallBackToDefaults(t *testing.T) {
	cfg := &ExternalQueryFile{ContextTypes: map[string]string{}}
	r := newContextResolver(nil, cfg, nil)
	if len(r.byID)+len(r.byName) != 0 {
		t.Errorf("declared-empty context_types produced %d context kinds, want 0",
			len(r.byID)+len(r.byName))
	}
}

// Omitting the key is different from declaring it empty: a query file that did
// not opine still gets the defaults.
func TestAbsentContextTypesKeepsTheDefaults(t *testing.T) {
	r := newContextResolver(nil, &ExternalQueryFile{}, nil)
	if len(r.byID)+len(r.byName) != len(defaultContextTypes) {
		t.Errorf("absent context_types produced %d context kinds, want %d",
			len(r.byID)+len(r.byName), len(defaultContextTypes))
	}
}

func TestJSONMemberIsContainedByEnclosingMember(t *testing.T) {
	projectDir := stageGrammar(t, "json", "tree-sitter-json", ".json", "json.yaml")
	pf := parseFixture(t, projectDir, "a.json",
		`{"server": {"database": {"host": "db.example.com"}}}`)
	got := nodesOf(pf)

	wantNode(t, got, "Pair", "database", "server", "Pair")
	wantNode(t, got, "Pair", "host", "database", "Pair")
}

func TestYAMLMappingIsContainedByEnclosingMapping(t *testing.T) {
	projectDir := stageGrammar(t, "yaml", "tree-sitter-yaml", ".yaml", "yaml.yaml")
	pf := parseFixture(t, projectDir, "a.yaml", "server:\n  database:\n    host: db.example.com\n")
	got := nodesOf(pf)

	wantNode(t, got, "Mapping", "database", "server", "Mapping")
	wantNode(t, got, "Mapping", "host", "database", "Mapping")
}

func TestSvelteElementIsContainedByEnclosingElement(t *testing.T) {
	projectDir := stageGrammar(t, "svelte", "tree-sitter-svelte", ".svelte", "svelte.yaml")
	pf := parseFixture(t, projectDir, "a.svelte", `<div><section><Card>x</Card></section></div>`)
	got := nodesOf(pf)

	wantNode(t, got, "Element", "section", "div", "Element")
	wantNode(t, got, "Element", "Card", "section", "Element")
}

// Siblings must not be mistaken for parents. This is the case that rules out a
// configuration-free rule of "name the ancestor after the first entity inside
// it": walking up from <b>, the `content` wrapper's first entity is <x>, which
// is b's SIBLING.
func TestXMLSiblingIsNotMistakenForParent(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml", `<a><x/><b/></a>`)
	got := nodesOf(pf)

	wantNode(t, got, "Element", "x", "a", "Element")
	wantNode(t, got, "Element", "b", "a", "Element")
}
