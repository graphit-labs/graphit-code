package ast

import "testing"

// Nothing contains itself.
//
// An element nested in an element of the SAME name is two distinct tree nodes
// with identical text, so the node-identity rule in contextResolver.resolve
// cannot see it: the inner element correctly resolves its parent to the outer
// one, and both are named "frame". The failure was downstream, in the UID
// lookup -- nameToUID[e.Name] = uid ran BEFORE the parent lookup, so the inner
// element overwrote the outer one in the map and then found itself.
//
// Measured on a real corpus before the fix: 354 self CONTAINS edges, all from
// Oracle Reports XML nesting <frame> inside <frame>.
func TestNestedSameNameElementIsNotItsOwnParent(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml",
		`<frame><frame><frame><label>x</label></frame></frame></frame>`)

	entry := ConvertToCache(pf, projectDir, false, "")
	for _, edge := range entry.ContainsEdges {
		if edge.ParentUID == edge.ChildUID {
			t.Errorf("self CONTAINS edge on %s", edge.ChildUID)
		}
	}
	if len(entry.ContainsEdges) == 0 {
		t.Fatal("no CONTAINS edges at all — the fixture stopped producing containment")
	}
}

// The guard must not silence real containment: a same-name chain still has to
// produce one edge per level.
func TestNestedSameNameElementsStillNestInTheGraph(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml",
		`<frame><frame><label>x</label></frame></frame>`)

	entry := ConvertToCache(pf, projectDir, false, "")
	var frameToFrame, frameToLabel int
	for _, edge := range entry.ContainsEdges {
		if edge.ParentLabel == "Element" && edge.ChildLabel == "Element" {
			frameToFrame++
		}
		_ = frameToLabel
	}
	if frameToFrame == 0 {
		t.Error("the outer frame no longer contains the inner one")
	}
}
