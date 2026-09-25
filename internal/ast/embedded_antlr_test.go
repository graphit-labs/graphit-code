package ast

import (
	"context"
	"path/filepath"
	"testing"
)

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
	if tbl.Line != 5 {
		t.Errorf("the table is at line %d, want 5 (absolute)", tbl.Line)
	}
	if _, ok := entityAt(pf, "Column", "STATUS"); !ok {
		if _, ok := entityAt(pf, "Column", "status"); !ok {
			t.Errorf("no Column from the ANTLR sub-parse; entities: %v", entityLabelsOf(pf))
		}
	}
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
	storeDir := t.TempDir()
	db := NewLadybugDB(LadybugConfig{StoreDir: storeDir, IcebugDir: filepath.Join(storeDir, "graph.icebug")})
	defer func() { _ = db.Close() }()
	ctx := context.Background()
	if _, err := RunPipeline(ctx, db, projectDir, PipelineOptions{CacheDir: storeDir, SkipExternal: true}); err != nil {
		t.Fatal(err)
	}
	relations, err := db.Query(ctx, "MATCH (p:Procedure)-[r:SELECTS]->(t:Table) RETURN r LIMIT 1", nil)
	if err != nil || len(relations.Records) != 1 {
		t.Fatalf("ANTLR relationship materialization: rows=%v err=%v", relations, err)
	}
	relationship, ok := relations.Records[0]["r"].(map[string]any)
	if !ok {
		t.Fatalf("unexpected ANTLR relation: %v", relations.Records[0])
	}
	properties, _ := relationship["Properties"].(map[string]any)
	if properties["source_file"] != "q.xml" || properties["uid"] == "" {
		t.Fatalf("ANTLR relation properties: %v", properties)
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
