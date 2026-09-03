package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestEmbeddedBlockIsAttributedToTheContainingUnitNotTheSiblingAbove(t *testing.T) {
	projectDir := stageHostSpanXML(t)

	pf := parseFixture(t, projectDir, "flow.xml", `<flow>
  <step>
    <config>
      <entry>
        <key>sql</key>
        <value>
  insert into pedido (id) values (1);
        </value>
      </entry>
    </config>
    <name>gravaPedido</name>
  </step>
</flow>
`)

	ref, ok := referenceTo(pf, "pedido")
	if !ok {
		t.Fatalf("no reference to the inserted table; references: %v", referencesOf(pf))
	}
	if ref.SourceName != "gravaPedido" {
		t.Errorf("SourceName = %q, want the unit that CONTAINS the block (gravaPedido); "+
			"%q is the sibling above the block, %q is the element wrapping it",
			ref.SourceName, "key", "value")
	}
	unit, ok := entityAt(pf, "Step", "gravaPedido")
	if !ok {
		t.Fatalf("no Step entity; entities: %v", entityLabelsOf(pf))
	}
	if unit.Line != 2 || unit.EndLine != 12 {
		t.Errorf("Step spans %d..%d, want 2..12 — the whole <step>, from span_capture",
			unit.Line, unit.EndLine)
	}
}

// Containment is the whole rule: an entity that ends inside the block is not hosting
// it. This is what disqualifies the element that literally wraps the statement, whose
// span in a data grammar is just its start tag — and it is the reason a grammar has to
// declare a real span to be a host at all.
func TestHostEntityMustContainTheWholeBlock(t *testing.T) {
	outer := &ParsedFile{
		Path:     "etl/flow.xml",
		Language: "xml",
		Entities: map[string][]Entity{
			"elements": {
				{Name: "value", Line: 10, EndLine: 10, GraphLabel: "Element"},
				{Name: "key", Line: 9, EndLine: 9, GraphLabel: "Element"},
			},
			"steps": {{Name: "gravaPedido", Line: 4, EndLine: 20, GraphLabel: "Step"}},
		},
	}

	if got := hostEntityAt(outer, 10, 14, nil); got != "gravaPedido" {
		t.Errorf("host = %q, want gravaPedido: the only entity containing lines 10..14", got)
	}
	bare := &ParsedFile{Entities: map[string][]Entity{
		"elements": {{Name: "value", Line: 10, EndLine: 10, GraphLabel: "Element"}},
	}}
	if got := hostEntityAt(bare, 10, 14, nil); got != "" {
		t.Errorf("host = %q, want empty: no entity contains the block", got)
	}
}

// The one-line statement, which containment alone gets wrong: `<value>select …</value>`
// puts the element's tag and the whole block on the same line, so the wrapper's span
// COINCIDES with the block's and it would win on being smallest. It is the wrapper by
// the same argument that excludes the block's own text node — the unit around it is the
// answer.
func TestHostMustExtendBeyondTheBlockNotCoincideWithIt(t *testing.T) {
	outer := &ParsedFile{
		Path:     "etl/flow.xml",
		Language: "xml",
		Entities: map[string][]Entity{
			"elements": {{Name: "value", Line: 10, EndLine: 10, GraphLabel: "Element"}},
			"steps":    {{Name: "gravaPedido", Line: 4, EndLine: 20, GraphLabel: "Step"}},
		},
	}

	if got := hostEntityAt(outer, 10, 10, nil); got != "gravaPedido" {
		t.Errorf("host = %q, want gravaPedido; %q spans exactly the block, so it is the "+
			"element carrying the statement and not a unit around it", got, "value")
	}
}

// span_capture as a unit: the entity's line range comes from the captured node, and
// the same query without it keeps the old range (name node → end of its parent).
func TestSpanCaptureDelimitsTheEntity(t *testing.T) {
	projectDir := stageHostSpanXML(t)

	pf := parseFixture(t, projectDir, "unit.xml", `<flow>
  <step>
    <config>
      <entry>
        <key>sql</key>
      </entry>
    </config>
    <name>gravaPedido</name>
  </step>
</flow>
`)
	unit, ok := entityAt(pf, "Step", "gravaPedido")
	if !ok {
		t.Fatalf("no Step entity; entities: %v", entityLabelsOf(pf))
	}
	if unit.Line != 2 || unit.EndLine != 9 {
		t.Errorf("Step spans %d..%d, want 2..9 (the <step> element)", unit.Line, unit.EndLine)
	}
	el, ok := entityAt(pf, "Element", "config")
	if !ok {
		t.Fatalf("no Element for <config>; entities: %v", entityLabelsOf(pf))
	}
	if el.Line != el.EndLine {
		t.Errorf("Element <config> spans %d..%d; without span_capture an XML entity is "+
			"its start tag, and this test's control depends on that", el.Line, el.EndLine)
	}
}

// A span_capture naming a capture the pattern does not have resolves to -1 and the
// entity keeps the range it would have had. A grammar with a typo loses the wider
// span, not its entities.
func TestSpanCaptureNamingAnAbsentCaptureIsHarmless(t *testing.T) {
	projectDir := stageGrammarWithQueries(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
		`language: xml
grammar: tree-sitter-xml
extensions: [".xml"]
queries:
  - data_key: steps
    graph_label: Step
    pattern: '(element (STag (Name) @_s) (content (element (STag (Name) @_n) (content (CharData) @name))) (#eq? @_s "step") (#eq? @_n "name"))'
    span_capture: nao_existe
exports:
  strategy: none
comment_types:
  - Comment
`)

	pf := parseFixture(t, projectDir, "unit.xml", `<flow>
  <step>
    <name>gravaPedido</name>
  </step>
</flow>
`)
	unit, ok := entityAt(pf, "Step", "gravaPedido")
	if !ok {
		t.Fatalf("the entity disappeared with an unknown span_capture; entities: %v",
			entityLabelsOf(pf))
	}
	if unit.Line != 3 {
		t.Errorf("Step is at line %d, want 3 — the name node's own line", unit.Line)
	}
}

func stageHostSpanXML(t *testing.T) string {
	t.Helper()
	projectDir := stageGrammarWithQueries(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
		`language: xml
grammar: tree-sitter-xml
extensions: [".xml"]
queries:
  - data_key: elements
    graph_label: Element
    pattern: '(STag (Name) @name)'
  - data_key: steps
    graph_label: Step
    pattern: '(element (STag (Name) @_s) (content (element (STag (Name) @_n) (content (CharData) @name))) (#eq? @_s "step") (#eq? @_n "name")) @scope'
    span_capture: scope
exports:
  strategy: none
comment_types:
  - Comment
embedded:
  - pattern: '(element (STag (Name) @_v) (content (CharData) @body) (#eq? @_v "value"))'
    text_capture: body
    default: plsql
`)

	body, err := os.ReadFile(filepath.Join("queries", "plsql.yaml"))
	if err != nil {
		t.Skipf("no plsql.yaml: %v", err)
	}
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.WriteFile(filepath.Join(qdir, "plsql.yaml"), body, 0o644); err != nil {
		t.Fatal(err)
	}
	return projectDir
}

func referenceTo(pf *ParsedFile, target string) (ReferenceInfo, bool) {
	for _, r := range pf.References {
		if r.TargetName == target || r.TargetName == upperASCII(target) {
			return r, true
		}
	}
	return ReferenceInfo{}, false
}

func referencesOf(pf *ParsedFile) []string {
	var out []string
	for _, r := range pf.References {
		out = append(out, r.SourceName+" -"+r.RelType+"-> "+r.TargetName)
	}
	return out
}

func upperASCII(s string) string {
	b := []byte(s)
	for i := range b {
		if b[i] >= 'a' && b[i] <= 'z' {
			b[i] -= 32
		}
	}
	return string(b)
}

// THE OTHER SHAPE OF THE SAME QUESTION: the block lives in an ATTRIBUTE of the element
// that names the unit, so unit and block occupy the same line. "Strictly contains"
// excludes exactly the entity that should answer, and everything that does contain it is
// coarser — the screen, the module. `host_labels` says which entities are units, and the
// choice stops depending on whether a span happens to be wider.
//
// This is what an XML-exported screen's `<Trigger Name="POST-QUERY" TriggerText="…"/>`
// looks like: the
// statement's newlines are encoded, so the whole trigger is one physical line.
func TestBlockInAnAttributeIsAttributedToTheUnitItDeclares(t *testing.T) {
	projectDir := stageGrammarWithQueries(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
		`language: xml
grammar: tree-sitter-xml
extensions: [".xml"]
queries:
  - data_key: elements
    graph_label: Element
    pattern: '(STag (Name) @name)'
  - data_key: unit_triggers
    graph_label: UnitTrigger
    pattern: '(EmptyElemTag (Name) @_t (Attribute (Name) @_a (AttValue) @name) (#eq? @_t "Trigger") (#eq? @_a "Name")) @scope'
    span_capture: scope
    # The name comes from an attribute value, which is quoted at the source.
    name_is_data: true
exports:
  strategy: none
comment_types:
  - Comment
text_normalizers:
  attr_text:
    replace:
      '"': ' '
embedded:
  - pattern: '(EmptyElemTag (Name) @_t (Attribute (Name) @_a (AttValue) @body) (#eq? @_t "Trigger") (#eq? @_a "Code"))'
    text_capture: body
    normalize: attr_text
    host_labels: [UnitTrigger]
    default: plsql
`)
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	plsql, err := os.ReadFile(filepath.Join("queries", "plsql.yaml"))
	if err != nil {
		t.Skipf("no plsql.yaml: %v", err)
	}
	if err := os.WriteFile(filepath.Join(qdir, "plsql.yaml"), plsql, 0o644); err != nil {
		t.Fatal(err)
	}

	pf := parseFixture(t, projectDir, "screen.xml", `<Module>
  <Screen Name="PEDIDO">
    <Trigger Name="POST-INSERT" Code="begin insert into pedido (id) values (1); end;"/>
  </Screen>
</Module>
`)

	ref, ok := referenceTo(pf, "pedido")
	if !ok {
		t.Fatalf("no reference to the inserted table; references: %v", referencesOf(pf))
	}
	if ref.SourceName != "POST-INSERT" {
		t.Errorf("SourceName = %q, want POST-INSERT — the unit whose attribute carries the "+
			"statement. Empty means no host was found, which is what strict containment "+
			"alone answers here", ref.SourceName)
	}

	if got := hostEntityAt(&ParsedFile{Entities: map[string][]Entity{
		"unit_triggers": {{Name: "POST-INSERT", Line: 3, EndLine: 3, GraphLabel: "UnitTrigger"}},
	}}, 3, 3, nil); got != "" {
		t.Errorf("without host_labels the coincident unit is %q, want no host", got)
	}
}

func TestCallFromAHostUnitReachesTheGraph(t *testing.T) {
	ri := newRebuildIndexWithDML(map[string]*parseCacheEntry{
		"screens/pedido.xml": {
			RelPath:  "screens/pedido.xml",
			Language: "xml",
			FileRow: []string{"screens/pedido.xml", "pedido.xml", "screens/pedido.xml",
				"false", "xml", ""},
			Entities: []cachedEntity{{
				Label: "UnitTrigger", UID: "screens/pedido.xml::POST-INSERT",
				Name: "POST-INSERT", Path: "screens/pedido.xml", Lang: "xml",
			}},
			Calls: []cachedCall{{
				CallerUID: "screens/pedido.xml::POST-INSERT", CalleeUID: "pr_grava",
				SourceType: "UnitTrigger", Path: "screens/pedido.xml", Line: 3, Lang: "plsql",
			}},
		},
	})

	var declared bool
	for _, l := range ri.callerLabels {
		if l == "UnitTrigger" {
			declared = true
		}
	}
	if !declared {
		t.Fatalf("the host label is not a caller label; got %v — the CALLS pair is never "+
			"declared and the edge has nowhere to go", ri.callerLabels)
	}

	ri.fileNodeJSON()
	ri.entityJSON("UnitTrigger")
	ri.stubFunctionJSON()

	if !ri.canWriteCallerLabel("UnitTrigger") {
		t.Error("the writer would refuse the unit-sourced CALLS group")
	}
	rows := ri.callEdgeJSON("UnitTrigger", LabelFunction)
	if len(rows) != 1 {
		t.Fatalf("got %d UnitTrigger→Function CALLS rows, want 1", len(rows))
	}
	if rows[0]["caller_uid"] != "screens/pedido.xml::POST-INSERT" {
		t.Errorf("caller_uid = %v, want the unit", rows[0]["caller_uid"])
	}
}

// And the attribution is what puts that label on the call in the first place.
func TestHostAttributionStampsTheHostLabelOnACall(t *testing.T) {
	outer := &ParsedFile{
		Path:     "screens/pedido.xml",
		Language: "xml",
		Entities: map[string][]Entity{
			"unit_triggers": {{Name: "POST-INSERT", Line: 3, EndLine: 3, GraphLabel: "UnitTrigger"}},
		},
	}
	inner := &ParsedFile{
		Language:  "plsql",
		CallSites: []CallInfo{{Name: "pr_grava", Line: 3}},
	}

	attributeToHostEntity(outer, inner, 3, 3, []string{"UnitTrigger"})

	if got := inner.CallSites[0].SourceName; got != "POST-INSERT" {
		t.Errorf("SourceName = %q, want POST-INSERT", got)
	}
	if got := inner.CallSites[0].SourceType; got != "UnitTrigger" {
		t.Errorf("SourceType = %q, want UnitTrigger — without the label the edge is "+
			"written from a Function that does not exist, and dropped", got)
	}
}

func TestEmbeddedFragmentIsWrappedIntoSomethingItsLanguageCanParse(t *testing.T) {
	grammar := func(wrap bool) string {
		g := `language: xml
grammar: tree-sitter-xml
extensions: [".xml"]
queries:
  - data_key: elements
    graph_label: Element
    pattern: '(STag (Name) @name)'
exports:
  strategy: none
comment_types:
  - Comment
text_normalizers:
  attr_text:
    replace:
      '"': ' '
embedded:
  - pattern: '(EmptyElemTag (Name) @_t (Attribute (Name) @_a (AttValue) @body) (#eq? @_t "Unit") (#eq? @_a "Body"))'
    text_capture: body
    normalize: attr_text
`
		if wrap {
			g += "    wrap_prefix: 'DECLARE '\n    wrap_suffix: ' BEGIN NULL; END;'\n"
		}
		return g + "    default: plsql\n"
	}

	const fixture = `<Module>
  <Unit Name="CGFK$CHK" Body="PROCEDURE CGFK$CHK(p IN BOOLEAN) IS BEGIN pck_pedido.pr_grava(1); END;"/>
</Module>
`
	callNames := func(wrap bool) []string {
		projectDir := stageGrammarWithQueries(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
			grammar(wrap))
		qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
		plsql, err := os.ReadFile(filepath.Join("queries", "plsql.yaml"))
		if err != nil {
			t.Skipf("no plsql.yaml: %v", err)
		}
		if err := os.WriteFile(filepath.Join(qdir, "plsql.yaml"), plsql, 0o644); err != nil {
			t.Fatal(err)
		}
		pf := parseFixture(t, projectDir, "unit.xml", fixture)
		var out []string
		for _, c := range pf.CallSites {
			out = append(out, c.Name)
		}
		return out
	}

	if got := callNames(false); len(got) != 0 {
		t.Errorf("unwrapped fragment produced calls %v; the case this covers is that it "+
			"produces none", got)
	}

	got := callNames(true)
	found := false
	for _, n := range got {
		if strings.Contains(strings.ToLower(n), "pr_grava") {
			found = true
		}
	}
	if !found {
		t.Errorf("wrapped fragment produced calls %v, want the call inside the "+
			"procedure body", got)
	}
}

// The line invariant again, from the other side: a wrapping that adds a line would move
// every entity the sub-parse reports, so it is dropped at load rather than honoured.
func TestEmbeddedWrapWithALineBreakIsDropped(t *testing.T) {
	blocks := validEmbeddedBlocks([]EmbeddedBlock{{
		Pattern:     "(x) @body",
		TextCapture: "body",
		Default:     "plsql",
		WrapPrefix:  "DECLARE\n",
		WrapSuffix:  " BEGIN NULL; END;",
	}}, nil, "test.yaml")

	if len(blocks) != 1 {
		t.Fatalf("got %d blocks, want the block kept with its wrapping dropped", len(blocks))
	}
	if blocks[0].WrapPrefix != "" || blocks[0].WrapSuffix != "" {
		t.Errorf("wrap survived with a line break in it: %q / %q",
			blocks[0].WrapPrefix, blocks[0].WrapSuffix)
	}
}
