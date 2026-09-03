package ast

import (
	"strings"
	"testing"
)

func xmlNormalizer() *TextNormalizer {
	return &TextNormalizer{
		Replace: map[string]string{
			"&lt;": "<", "&gt;": ">", "&amp;": "&", "&quot;": "\"", "&apos;": "'",
		},
		NumericCharRefs: true,
	}
}

func TestTextNormalizerPreservesNewlineCount(t *testing.T) {
	cases := []struct {
		name, in, want string
	}{
		{"named", "qt &gt; 0 and w &lt; 9", "qt > 0 and w < 9"},
		{"amp", "a &amp;&amp; b", "a && b"},
		{"quotes", "n = &quot;x&quot; or m = &apos;y&apos;", "n = \"x\" or m = 'y'"},
		{"numeric", "a &#62; b &#x3E; c", "a > b > c"},
		{"newline entity is left alone", "a&#10;b", "a&#10;b"},
		{"hex newline is left alone", "a&#xA;b", "a&#xA;b"},
		{"carriage return is left alone", "a&#13;b", "a&#13;b"},
		{"unknown entity is left alone", "a &nbsp; b", "a &nbsp; b"},
		{"bare ampersand", "a & b", "a & b"},
		{"nothing to do", "select 1 from dual", "select 1 from dual"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := string(applyTextNormalizer([]byte(tc.in), xmlNormalizer()))
			if got != tc.want {
				t.Errorf("decode(%q) = %q, want %q", tc.in, got, tc.want)
			}
			if a, b := strings.Count(tc.in, "\n"), strings.Count(got, "\n"); a != b {
				t.Errorf("newline count changed: %d -> %d", a, b)
			}
		})
	}
}

func TestTextNormalizerKeepsLinesAligned(t *testing.T) {
	in := "select a\n  from t\n where q &gt; 0\n   and w &lt; 9\n"
	got := string(applyTextNormalizer([]byte(in), xmlNormalizer()))
	if strings.Count(got, "\n") != strings.Count(in, "\n") {
		t.Fatalf("newline count changed")
	}
	lines := strings.Split(got, "\n")
	if lines[2] != " where q > 0" {
		t.Errorf("line 3 = %q, want %q", lines[2], " where q > 0")
	}
	if lines[3] != "   and w < 9" {
		t.Errorf("line 4 = %q, want %q", lines[3], "   and w < 9")
	}
}

// The complete case, end to end: a key/value config XML with escaped SQL and a
// METADATA block using the same key — a common shape in an export
// from an orchestration tool. Only the real value is parsed, the SQL arrives whole, and its
// line is absolute.
func TestEmbeddedXMLBlockDecodesAndParsesTheWholeBody(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml", `
# The normalizer is declared HERE, in the project's file, and not in the shipped
# enviada: um esquema de escape e um fato da linguagem, mas QUAL bloco precisa de
# which is a fact about the project. The project > user > runtime chain already allows
# this with no engine change at all, and that is what this test exercises.
text_normalizers:
  xml_entities:
    replace:
      "&lt;": "<"
      "&gt;": ">"
      "&amp;": "&"
      "&quot;": '"'
      "&apos;": "'"
    numeric_char_refs: true

embedded:
  - pattern: '(element (STag (Name) @_p) (content (element (STag (Name) @_e) (content (element (STag (Name) @_k) (content (CharData) @_key)) (element (STag (Name) @_v) (content) @body)))) (#eq? @_p "properties") (#eq? @_e "entry") (#eq? @_k "key") (#eq? @_v "value") (#eq? @_key "db-statement"))'
    text_capture: body
    normalize: xml_entities
    default: plsql
`, "plsql.yaml")

	pf := parseFixture(t, projectDir, "flow.xml", `<processor>
    <descriptors>
        <entry>
<key>db-statement</key>
<value>
    <name>db-statement</name>
</value>
        </entry>
    </descriptors>
    <properties>
        <entry>
<key>db-statement</key>
<value>CREATE TABLE pedido_ativo AS
SELECT id
  FROM pedido
 WHERE qt &gt; 0
   AND dt &lt; SYSDATE</value>
        </entry>
    </properties>
</processor>
`)
	if _, ok := entityAt(pf, "Table", "PEDIDO_ATIVO"); !ok {
		if _, ok := entityAt(pf, "Table", "pedido_ativo"); !ok {
			t.Errorf("no Table from the decoded body; entities: %v", entityLabelsOf(pf))
		}
	}
	if _, ok := entityAt(pf, "Element", "properties"); !ok {
		t.Error("the XML markup disappeared")
	}
}

// Without `decode`, the behaviour is the old one: the body is passed raw. That is what
// keeps SFCs — whose `raw_text` carries no escaping at all — free of the cost.
func TestEmbeddedNormalizeIsOptInAndMustExist(t *testing.T) {
	qf, _ := parseQueryFile([]byte(`language: x
embedded:
  - pattern: '(x) @body'
    text_capture: body
    default: sql
`), "test.yaml")
	if len(qf.Embedded) != 1 {
		t.Fatalf("kept %d blocks", len(qf.Embedded))
	}
	if qf.Embedded[0].Normalize != "" {
		t.Errorf("normalize defaulted to %q, want empty", qf.Embedded[0].Normalize)
	}

	qf2, _ := parseQueryFile([]byte(`language: x
embedded:
  - pattern: '(x) @body'
    text_capture: body
    normalize: banana
    default: sql
`), "test.yaml")
	if len(qf2.Embedded) != 1 {
		t.Fatalf("kept %d blocks", len(qf2.Embedded))
	}
	if qf2.Embedded[0].Normalize != "" {
		t.Errorf("a normalizer this language does not declare survived as %q; it must be dropped",
			qf2.Embedded[0].Normalize)
	}
}

// The engine owns no escaping scheme, so a grammar can declare one it has never
// heard of — and the load-time validator is what keeps the line offset safe.
func TestTextNormalizerRejectsAReplacementThatAddsALine(t *testing.T) {
	qf, _ := parseQueryFile([]byte(`language: x
text_normalizers:
  weird:
    replace:
      "\\n": "\n"
      "~GT~": ">"
embedded:
  - pattern: '(x) @body'
    text_capture: body
    normalize: weird
    default: sql
`), "test.yaml")
	if len(qf.Embedded) != 1 || qf.Embedded[0].Normalize != "weird" {
		t.Fatalf("the block did not survive: %+v", qf.Embedded)
	}
	n := qf.TextNormalizers["weird"]
	if _, bad := n.Replace["\\n"]; bad {
		t.Error("a replacement containing a line break survived validation")
	}
	if n.Replace["~GT~"] != ">" {
		t.Errorf("the safe pair was dropped: %v", n.Replace)
	}
}

// A grammar with no normalizer declared is the common case and must cost nothing.
func TestBlockNamingANormalizerInAnotherLanguageIsDropped(t *testing.T) {
	qf, _ := parseQueryFile([]byte(`language: x
embedded:
  - pattern: '(x) @body'
    text_capture: body
    normalize: xml_entities
    default: sql
`), "test.yaml")
	if len(qf.Embedded) != 1 {
		t.Fatalf("kept %d blocks", len(qf.Embedded))
	}
	if qf.Embedded[0].Normalize != "" {
		t.Error("a normalizer that this language does not declare was kept")
	}
}
