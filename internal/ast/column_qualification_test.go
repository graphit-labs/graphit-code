package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The object names here are synthetic.

// A column write's target has to come out QUALIFIED. Without that the edge is the
// harmful one, not a lesser version of the good one: `ST_PROC` lives in dozens of
// tables, so all of them collapse onto a single node and "who writes this column"
// starts answering with every homonym's writers, presented as though they were one.
func TestColumnWriteTargetsAreQualifiedByTheirTable(t *testing.T) {
	pf := plsqlFixture(t, "p.sql", `
CREATE OR REPLACE PROCEDURE p_grava IS
BEGIN
  UPDATE PEDIDO SET ST_PROC = 'S', DT_UPD = SYSDATE WHERE ID = 1;
  INSERT INTO ITEM (ID_ITEM, QT) VALUES (1, 2);
END;
`)

	got := map[string]string{}
	for _, r := range pf.References {
		if r.RelType == "WRITES_COLUMN" {
			got[r.TargetName] = r.SourceName
		}
	}

	for _, want := range []string{"PEDIDO.ST_PROC", "PEDIDO.DT_UPD", "ITEM.ID_ITEM", "ITEM.QT"} {
		if _, ok := got[want]; !ok {
			t.Errorf("missing qualified write target %q; got %v", want, got)
		}
	}

	// And the source is the unit doing the writing, not the file.
	if src := got["PEDIDO.ST_PROC"]; src != "p_grava" {
		t.Errorf("source of the write = %q, want the enclosing procedure", src)
	}

	// No target may have escaped unqualified — a single one already reintroduces the
	// aggregation for that column.
	for target := range got {
		if !strings.Contains(target, ".") {
			t.Errorf("unqualified write target %q leaked; it would merge every table's column of that name", target)
		}
	}
}

// The QUALITY of the mechanism is this: a qualified target has to RESOLVE to the
// declared column, not fall back to a stub. A target that always falls back improves
// nothing — it makes things worse, because it looks like an answer.
func TestQualifiedColumnTargetResolvesToTheDeclaredColumn(t *testing.T) {
	rules := BuildTargetRules([]ExternalQueryFile{{
		Language: "plsql",
		TargetRules: map[string]TargetRuleDecl{
			"WRITES_COLUMN": {Labels: []string{LabelColumn}, Fallback: LabelColumn},
		},
	}})
	ri := newRebuildIndex(map[string]*parseCacheEntry{
		// Two tables with a column of the SAME NAME — exactly the case bare-name
		// resolution cannot decide, and qualification can.
		"schema/pedido.sql": {
			RelPath: "schema/pedido.sql", Language: "plsql",
			FileRow: []string{"schema/pedido.sql", "pedido.sql", "schema/pedido.sql", "false", "plsql", ""},
			Entities: []cachedEntity{
				{Label: LabelTable, UID: "t:PEDIDO", Name: "PEDIDO", Path: "schema/pedido.sql", Lang: "plsql"},
				{Label: LabelColumn, UID: "c:PEDIDO.ST_PROC", Name: "ST_PROC",
					Context: "PEDIDO", Path: "schema/pedido.sql", Lang: "plsql"},
			},
		},
		"schema/item.sql": {
			RelPath: "schema/item.sql", Language: "plsql",
			FileRow: []string{"schema/item.sql", "item.sql", "schema/item.sql", "false", "plsql", ""},
			Entities: []cachedEntity{
				{Label: LabelTable, UID: "t:ITEM", Name: "ITEM", Path: "schema/item.sql", Lang: "plsql"},
				{Label: LabelColumn, UID: "c:ITEM.ST_PROC", Name: "ST_PROC",
					Context: "ITEM", Path: "schema/item.sql", Lang: "plsql"},
			},
		},
		"proc/grava.sql": {
			RelPath: "proc/grava.sql", Language: "plsql",
			FileRow: []string{"proc/grava.sql", "grava.sql", "proc/grava.sql", "false", "plsql", ""},
			Entities: []cachedEntity{{
				Label: LabelProcedure, UID: "p:grava", Name: "p_grava",
				Path: "proc/grava.sql", Lang: "plsql",
			}},
			References: []cachedReference{{
				SourceUID: "p:grava", TargetUID: "PEDIDO.ST_PROC", RelType: "WRITES_COLUMN",
				Path: "proc/grava.sql", Line: 4, Lang: "plsql",
			}},
		},
	}, rules)

	uid, label := ri.resolveRefTarget(cachedReference{
		TargetUID: "PEDIDO.ST_PROC", RelType: "WRITES_COLUMN", Lang: "plsql",
	}, "plsql")

	if label != LabelColumn {
		t.Fatalf("resolved label = %q, want Column", label)
	}
	if uid != "c:PEDIDO.ST_PROC" {
		t.Fatalf("resolved uid = %q, want the column DECLARED IN PEDIDO — a stub or the "+
			"other table's column means qualification is not doing its job", uid)
	}

	// The control proving qualification is what decides: the BARE name is ambiguous
	// between the two tables, and must keep failing to resolve.
	if _, ok := ri.resolveNamed("ST_PROC", "plsql",
		ri.rules.ForRelation("plsql", "WRITES_COLUMN")); ok {
		t.Error("the bare column name resolved despite living in two tables; " +
			"the ambiguity guard is gone and every column edge is now a coin flip")
	}
}

// The question to ask of every change: is this specific to one language, or do the
// others need it too? The mechanism belongs to the engine and knows no SQL, but the
// functionality exists only where some grammar declares it — so this test covers EVERY
// dialect that declares it, against each one's tree, which differs.
func TestColumnWritesAreQualifiedInEverySQLDialectThatDeclaresThem(t *testing.T) {
	cases := []struct {
		lang, grammar, ext, src string
		want                    []string
	}{
		{
			lang: "tsql", grammar: "antlr-tsql", ext: ".sql",
			src:  "UPDATE PEDIDO SET ST_PROC = 'S';\nINSERT INTO ITEM (ID_ITEM, QT) VALUES (1,2);",
			want: []string{"PEDIDO.ST_PROC", "ITEM.ID_ITEM", "ITEM.QT"},
		},
		{
			lang: "postgresql", grammar: "antlr-postgresql", ext: ".sql",
			src:  "UPDATE PEDIDO SET ST_PROC = 'S';\nINSERT INTO ITEM (ID_ITEM, QT) VALUES (1,2);",
			want: []string{"PEDIDO.ST_PROC", "ITEM.ID_ITEM", "ITEM.QT"},
		},
		{
			// db2 on UPDATE only: this grammar's INSERT does not descend into column
			// nodes, so declaring the query would be a pattern matching zero — worse
			// than absence, because it looks like coverage.
			lang: "db2", grammar: "antlr-db2", ext: ".sql",
			src:  "UPDATE PEDIDO SET ST_PROC = 'S';",
			want: []string{"PEDIDO.ST_PROC"},
		},
	}

	for _, c := range cases {
		t.Run(c.lang, func(t *testing.T) {
			proj := dialectProject(t, c.lang)
			parser := &AntlrParser{projectDir: proj}
			cfg := &antlrLangConfig{Language: c.lang, Grammar: c.grammar, Extensions: []string{c.ext}}
			pf, err := parser.parseWithConfig("q"+c.ext, c.ext, cfg, []byte(c.src), false, ParseOptions{})
			if err != nil {
				t.Fatalf("parse: %v", err)
			}

			got := map[string]bool{}
			for _, r := range pf.References {
				if r.RelType == "WRITES_COLUMN" {
					got[r.TargetName] = true
					if !strings.Contains(r.TargetName, ".") {
						t.Errorf("unqualified target %q", r.TargetName)
					}
				}
			}
			for _, w := range c.want {
				if !got[w] {
					t.Errorf("missing %q; got %v", w, got)
				}
			}
		})
	}
}

// dialectProject stages this repository's query file for one language.
func dialectProject(t *testing.T, lang string) string {
	t.Helper()
	proj := t.TempDir()
	dir := filepath.Join(proj, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("queries dir: %v", err)
	}
	yaml, err := os.ReadFile(filepath.Join("queries", lang+".yaml"))
	if err != nil {
		t.Fatalf("read %s.yaml: %v", lang, err)
	}
	if err := os.WriteFile(filepath.Join(dir, lang+".yaml"), yaml, 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	return proj
}

// The tree-sitter backend has the SAME feature by a different route: the qualifier is
// another capture of the same pattern, because a tree-sitter pattern matches the whole
// shape at once. Without this case that side's code would have no user at all — a
// feature that exists in only one backend is half a feature.
func TestTreeSitterBackendQualifiesColumnWritesToo(t *testing.T) {
	// parseSource directly, PURE tree-sitter. Over CompositeParser this test would
	// start lying: it falls back to ANTLR on seeing zero entities, and a relation
	// query leaves no entity behind — processRelations consumes them.
	proj := dialectProject(t, "sql")
	cfg, ok := tsLangConfigByName(proj, "sql")
	if !ok {
		t.Skip("sql grammar unavailable")
	}
	p := &TreeSitterParser{projectDir: proj}
	pf, err := p.parseSource("q.sql", ".sql", cfg,
		[]byte("UPDATE PEDIDO SET ST_PROC = 'S';\nINSERT INTO ITEM (ID_ITEM, QT) VALUES (1,2);"),
		0, 0, false, ParseOptions{})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}

	got := map[string]bool{}
	for _, r := range pf.References {
		if r.RelType == "WRITES_COLUMN" {
			got[r.TargetName] = true
			if !strings.Contains(r.TargetName, ".") {
				t.Errorf("unqualified target %q from the tree-sitter backend", r.TargetName)
			}
		}
	}
	for _, w := range []string{"PEDIDO.ST_PROC", "ITEM.ID_ITEM", "ITEM.QT"} {
		if !got[w] {
			t.Errorf("missing %q; got %v", w, got)
		}
	}
}
