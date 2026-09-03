package ast

import "testing"

func TestUniqueIndexCarriesItsTableColumnsAndUniqueness(t *testing.T) {
	pf := plsqlFixture(t, "ix.sql", `
CREATE UNIQUE INDEX IU_PEDIDO_PROD ON PEDIDO_ITEM (ORDER_ID, ID_PROD);
`)

	idx, ok := entityAt(pf, "Index", "IU_PEDIDO_PROD")
	if !ok {
		t.Fatalf("no Index entity; got labels %v", entityLabelsOf(pf))
	}

	if got := getProperty(idx.Properties, "value"); got != "UNIQUE" {
		t.Errorf("index value = %q, want UNIQUE; without it the graph cannot say whether "+
			"the database enforces this", got)
	}

	var cols []string
	for _, c := range entitiesOfLabel(pf, "Column") {
		if c.Context == "IU_PEDIDO_PROD" {
			cols = append(cols, c.Name)
		}
	}
	if len(cols) != 2 || cols[0] != "ORDER_ID" || cols[1] != "ID_PROD" {
		t.Errorf("index columns = %v, want [ORDER_ID ID_PROD] in that order", cols)
	}

	found := false
	for _, r := range pf.References {
		if r.TargetName == "PEDIDO_ITEM" && r.SourceName == "IU_PEDIDO_PROD" {
			found = true
		}
	}
	if !found {
		var got []string
		for _, r := range pf.References {
			got = append(got, r.SourceName+"-"+r.RelType+"->"+r.TargetName)
		}
		t.Errorf("no reference from the index to its table; references: %v", got)
	}
}

// The control: a NON-unique index must not gain the marker. Without this case a bug
// marking everything unique would pass — and "the database enforces this" is precisely
// the claim that must not be invented.
func TestNonUniqueIndexIsNotMarkedUnique(t *testing.T) {
	pf := plsqlFixture(t, "ix2.sql", `
CREATE INDEX IX_PEDIDO_DT ON PEDIDO_ITEM (DT_CRIACAO);
`)

	idx, ok := entityAt(pf, "Index", "IX_PEDIDO_DT")
	if !ok {
		t.Fatalf("no Index entity; got labels %v", entityLabelsOf(pf))
	}
	if got := getProperty(idx.Properties, "value"); got != "" {
		t.Errorf("non-unique index carries value %q, want empty", got)
	}

	var cols []string
	for _, c := range entitiesOfLabel(pf, "Column") {
		if c.Context == "IX_PEDIDO_DT" {
			cols = append(cols, c.Name)
		}
	}
	if len(cols) != 1 || cols[0] != "DT_CRIACAO" {
		t.Errorf("index columns = %v, want [DT_CRIACAO]", cols)
	}
}

// PARITY. The question to ask of every change is whether it is specific to one
// language; the index shape was born in plsql alone, and this test is what stops it
// staying that way. The five dialects' trees are all different — every path came from a
// dump — and what has to be the same is the ANSWER.
func TestEverySQLDialectGivesAnIndexItsTableColumnsAndUniqueness(t *testing.T) {
	cases := []struct {
		lang, grammar string
		antlr         bool
		unique, plain string
	}{
		{lang: "plsql", grammar: "antlr-plsql", antlr: true,
			unique: "CREATE UNIQUE INDEX IU_X ON TAB (A, B);",
			plain:  "CREATE INDEX IX_Y ON TAB (C);"},
		{lang: "tsql", grammar: "antlr-tsql", antlr: true,
			unique: "CREATE UNIQUE INDEX IU_X ON TAB (A, B);",
			plain:  "CREATE INDEX IX_Y ON TAB (C);"},
		{lang: "postgresql", grammar: "antlr-postgresql", antlr: true,
			unique: "CREATE UNIQUE INDEX IU_X ON TAB (A, B);",
			plain:  "CREATE INDEX IX_Y ON TAB (C);"},
		{lang: "db2", grammar: "antlr-db2", antlr: true,
			unique: "CREATE UNIQUE INDEX IU_X ON TAB (A, B);",
			plain:  "CREATE INDEX IX_Y ON TAB (C);"},
		{lang: "sql", grammar: "tree-sitter-sql",
			unique: "CREATE UNIQUE INDEX IU_X ON TAB (A, B);",
			plain:  "CREATE INDEX IX_Y ON TAB (C);"},
	}

	for _, c := range cases {
		t.Run(c.lang, func(t *testing.T) {
			parse := func(src string) *ParsedFile {
				proj := dialectProject(t, c.lang)
				if c.antlr {
					p := &AntlrParser{projectDir: proj}
					cfg := &antlrLangConfig{Language: c.lang, Grammar: c.grammar, Extensions: []string{".sql"}}
					pf, err := p.parseWithConfig("i.sql", ".sql", cfg, []byte(src), false, ParseOptions{})
					if err != nil {
						t.Fatalf("parse: %v", err)
					}
					return pf
				}
				cfg, ok := tsLangConfigByName(proj, c.lang)
				if !ok {
					t.Skip("grammar unavailable")
				}
				p := &TreeSitterParser{projectDir: proj}
				pf, err := p.parseSource("i.sql", ".sql", cfg, []byte(src), 0, 0, false, ParseOptions{})
				if err != nil {
					t.Fatalf("parse: %v", err)
				}
				return pf
			}

			pf := parse(c.unique)

			idx, ok := entityAt(pf, "Index", "IU_X")
			if !ok {
				t.Fatalf("no Index entity; labels %v", entityLabelsOf(pf))
			}
			if got := getProperty(idx.Properties, "value"); got != "UNIQUE" {
				t.Errorf("unique marker = %q, want UNIQUE — without it the graph cannot say "+
					"whether the database enforces this", got)
			}

			var cols []string
			for _, col := range entitiesOfLabel(pf, "Column") {
				if col.Context == "IU_X" {
					cols = append(cols, col.Name)
				}
			}
			if len(cols) != 2 || cols[0] != "A" || cols[1] != "B" {
				t.Errorf("covered columns = %v, want [A B] in order", cols)
			}

			foundTable := false
			for _, r := range pf.References {
				if r.TargetName == "TAB" && r.SourceName == "IU_X" {
					foundTable = true
				}
			}
			if !foundTable {
				var got []string
				for _, r := range pf.References {
					got = append(got, r.SourceName+"-"+r.RelType+"->"+r.TargetName)
				}
				t.Errorf("no index→table reference; references: %v", got)
			}

			plain := parse(c.plain)
			pidx, ok := entityAt(plain, "Index", "IX_Y")
			if !ok {
				t.Fatalf("no Index entity for the non-unique case; labels %v", entityLabelsOf(plain))
			}
			if got := getProperty(pidx.Properties, "value"); got != "" {
				t.Errorf("non-unique index carries marker %q, want empty", got)
			}
		})
	}
}
