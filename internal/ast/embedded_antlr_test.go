package ast

import (
	"testing"
)

// The config names the LANGUAGE; which backend parses it is the engine's problem.
//
// Until now it was not: the sub-parse ran on tree-sitter only, so `plsql`,
// `postgresql`, `tsql`, `db2` and `cobol85` — five languages that exist in the
// product — could not be a block's inner language. Whoever wrote `default: plsql` got
// silence, and silence there is the worst outcome because
// it looks like it worked.
//
// The case that drove this: the body of `<execute>select * from xpto</execute>` in a
// XML. The tree-sitter SQL grammar does not know PL/SQL, and it is `plsql.yaml` that
// tem as arestas SELECTS/INSERTS/UPDATES.

const xmlPLSQLBlock = `
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#eq? @_tag "execute"))'
    text_capture: body
    default: plsql
`

func TestEmbeddedBlockCanBeParsedByTheANTLRBackend(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
		xmlPLSQLBlock, "plsql.yaml")

	pf := parseFixture(t, projectDir, "changelog.xml", `<mapper>
  <changeset id="1">
    <note>nada aqui</note>
    <execute>
      CREATE TABLE pedido (
        id NUMBER NOT NULL,
        status VARCHAR2(10)
      );
    </execute>
  </changeset>
</mapper>
`)
	tbl, ok := entityAt(pf, "Table", "pedido")
	if !ok {
		t.Fatalf("no Table from the ANTLR sub-parse; entities: %v", entityLabelsOf(pf))
	}
	// The line is ABSOLUTE, and the block does not start at line 1 — that is what the
	// offset has to prove, and it is the same shiftParsedLines on both backends.
	if tbl.Line != 5 {
		t.Errorf("the table is at line %d, want 5 (absolute)", tbl.Line)
	}
	if _, ok := entityAt(pf, "Column", "STATUS"); !ok {
		if _, ok := entityAt(pf, "Column", "status"); !ok {
			t.Errorf("no Column from the ANTLR sub-parse; entities: %v", entityLabelsOf(pf))
		}
	}
	// O markup do XML segue intacto.
	if _, ok := entityAt(pf, "Element", "execute"); !ok {
		t.Error("the execute Element disappeared")
	}
}

// What the original request wanted: a SELECT inside <execute>, with the edges only the
// ANTLR side produces.
func TestEmbeddedANTLRBlockProducesDMLEdges(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
		xmlPLSQLBlock, "plsql.yaml")

	pf := parseFixture(t, projectDir, "q.xml", `<mapper>
  <execute>
    CREATE OR REPLACE PROCEDURE p_lista IS
    BEGIN
      SELECT nome INTO v FROM xpto;
    END;
  </execute>
</mapper>
`)
	if _, ok := entityAt(pf, "Procedure", "p_lista"); !ok {
		t.Errorf("no Procedure from the ANTLR sub-parse; entities: %v", entityLabelsOf(pf))
	}
	// The table the procedure reads, which is what a dialect grammar knows and the
	// tree-sitter one does not.
	found := false
	for _, r := range pf.References {
		if r.TargetName == "xpto" || r.TargetName == "XPTO" {
			found = true
		}
	}
	if !found {
		var got []string
		for _, r := range pf.References {
			got = append(got, r.RelType+"->"+r.TargetName)
		}
		t.Errorf("no reference to the selected table; references: %v", got)
	}
}

// A language that exists in no backend is still skipped silently: it is the author
// writing something we do not ship, and a WARN per block would be one log line per
// file.
func TestEmbeddedUnknownLanguageIsStillSilent(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml", `
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#eq? @_tag "execute"))'
    text_capture: body
    default: cobolscript
`, "plsql.yaml")

	pf := parseFixture(t, projectDir, "u.xml", `<mapper>
  <execute>CREATE TABLE pedido (id NUMBER);</execute>
</mapper>
`)
	if _, ok := entityAt(pf, "Table", "pedido"); ok {
		t.Error("an unknown language produced entities")
	}
	if _, ok := entityAt(pf, "Element", "execute"); !ok {
		t.Error("the rest of the file stopped being indexed")
	}
}

// Resolution by name crosses both backends, and it is what makes the choice of backend
// an engine detail rather than a decision for whoever writes the YAML.
func TestEmbeddedLangResolvesAcrossBothBackends(t *testing.T) {
	if _, ok := embeddedLangConfig("", "sql"); !ok {
		t.Error("sql (tree-sitter) does not resolve")
	}
	for _, name := range []string{"plsql", "postgresql", "tsql", "db2", "cobol85"} {
		if _, ok := embeddedLangConfig("", name); !ok {
			t.Errorf("%s (ANTLR) does not resolve", name)
		}
	}
	if _, ok := embeddedLangConfig("", "scss"); ok {
		t.Error("scss is not a language here and must not resolve")
	}
}
