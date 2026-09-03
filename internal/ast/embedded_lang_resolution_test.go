package ast

import "testing"

// The real case that drove the fix: a config XML carries SQL inside a <value>, the SQL
// is parsed by the PL/SQL grammar, and the table it writes to is declared in a .sql of
// the same repository.
//
// Before this fix the graph answered THE WRONG QUESTION CONFIDENTLY. The reference
// arrived stamped with the FILE's language (`xml`), and two things failed together:
// resolveNamed refuses to cross languages, so the `plsql` table was invisible; and
// refRule looked for `xml`'s TargetRule, which declares no DML at all, so the fallback
// became the file. The result was a File → File self-loop, and "who writes to this
// table?" answered with a list of readers and no sign of the only real writer.
//
// This test is a GRAPH test on purpose. A test over pf.References passes with the whole
// defect in place — which is exactly how the limitation survived two rounds.
func TestEmbeddedSQLReferenceReachesTheDeclaredTable(t *testing.T) {
	ri := newRebuildIndexWithDML(map[string]*parseCacheEntry{
		"schema/pedido.sql": {
			RelPath:  "schema/pedido.sql",
			Language: "plsql",
			FileRow:  []string{"schema/pedido.sql", "pedido.sql", "schema/pedido.sql", "false", "plsql", ""},
			Entities: []cachedEntity{{
				Label: LabelTable, UID: "schema/pedido.sql::T_PEDIDO",
				Name: "T_PEDIDO", Path: "schema/pedido.sql", Lang: "plsql",
			}},
		},
		"etl/pipeline.xml": {
			RelPath:  "etl/pipeline.xml",
			Language: "xml",
			FileRow:  []string{"etl/pipeline.xml", "pipeline.xml", "etl/pipeline.xml", "false", "xml", ""},
			References: []cachedReference{{
				SourceUID: "etl/pipeline.xml", TargetUID: "T_PEDIDO", RelType: RelInserts,
				Path: "etl/pipeline.xml", Line: 42,
				Lang: "plsql",
			}},
		},
	})

	ri.fileNodeJSON()
	ri.entityJSON(LabelTable)
	ri.stubTableJSON()

	rows := ri.dmlEdgeJSON(RelInserts, LabelFile, LabelTable)
	if len(rows) != 1 {
		t.Fatalf("got %d File→Table INSERTS rows, want 1 — the embedded SQL did not reach the table", len(rows))
	}
	if got := rows[0]["target_uid"]; got != "schema/pedido.sql::T_PEDIDO" {
		t.Errorf("target_uid = %v, want the DECLARED table; a stub or the file means resolution still failed", got)
	}
	if got := rows[0]["source_uid"]; got != "etl/pipeline.xml" {
		t.Errorf("source_uid = %v, want the host file", got)
	}
}

// The negative control, which is what proves the stamp is the cause and not a detail:
// the SAME entry with no Lang on the reference resolves as `xml` again and never
// reaches the table. Without this case the test above would pass even if resolution
// ignored language entirely.
func TestReferenceWithoutItsOwnLangStillResolvesAsTheHostFile(t *testing.T) {
	ri := newRebuildIndexWithDML(map[string]*parseCacheEntry{
		"schema/pedido.sql": {
			RelPath:  "schema/pedido.sql",
			Language: "plsql",
			FileRow:  []string{"schema/pedido.sql", "pedido.sql", "schema/pedido.sql", "false", "plsql", ""},
			Entities: []cachedEntity{{
				Label: LabelTable, UID: "schema/pedido.sql::T_PEDIDO",
				Name: "T_PEDIDO", Path: "schema/pedido.sql", Lang: "plsql",
			}},
		},
		"etl/pipeline.xml": {
			RelPath:  "etl/pipeline.xml",
			Language: "xml",
			FileRow:  []string{"etl/pipeline.xml", "pipeline.xml", "etl/pipeline.xml", "false", "xml", ""},
			References: []cachedReference{{
				SourceUID: "etl/pipeline.xml", TargetUID: "T_PEDIDO", RelType: RelInserts,
				Path: "etl/pipeline.xml", Line: 42,
			}},
		},
	})
	ri.fileNodeJSON()
	ri.entityJSON(LabelTable)
	ri.stubTableJSON()

	for _, row := range ri.dmlEdgeJSON(RelInserts, LabelFile, LabelTable) {
		if row["target_uid"] == "schema/pedido.sql::T_PEDIDO" {
			t.Fatal("a reference with no language of its own crossed into plsql; " +
				"the same-language guard is not being applied and resolveNamed's " +
				"whole purpose — not binding a .tsx fill() to a Go one — is gone")
		}
	}
}

// mergeParsedInto is where the stamp is applied, and what it stamps has to be the
// THREE things name resolution uses. A merge stamping only entities would leave
// exactly the DML — which lives in References — with the original defect.
func TestMergeStampsTheEmbeddedLanguageOnEverythingItFolds(t *testing.T) {
	outer := &ParsedFile{Path: "pipeline.xml", Language: "xml"}
	inner := &ParsedFile{
		Path:       "pipeline.xml",
		Language:   "plsql",
		Entities:   map[string][]Entity{"procedures": {{Name: "p_envia", Line: 3}}},
		CallSites:  []CallInfo{{Name: "PCK_X.PR_Y", Line: 4}},
		References: []ReferenceInfo{{TargetName: "T_PEDIDO", RelType: RelInserts, Line: 5}},
	}

	mergeParsedInto(outer, inner)

	if got := outer.Entities["procedures"][0].Lang; got != "plsql" {
		t.Errorf("entity Lang = %q, want plsql", got)
	}
	if got := outer.CallSites[0].Lang; got != "plsql" {
		t.Errorf("call Lang = %q, want plsql", got)
	}
	if got := outer.References[0].Lang; got != "plsql" {
		t.Errorf("reference Lang = %q, want plsql — this is the one the DML edge needs", got)
	}
}

// A block inside a block: the innermost stamp is the true one, and an outer merge must
// not overwrite it. That is why mergeParsedInto fills only what is empty instead of
// assigning outright.
func TestNestedEmbeddedBlockKeepsTheInnermostLanguage(t *testing.T) {
	outer := &ParsedFile{Path: "page.html", Language: "html"}
	inner := &ParsedFile{
		Path:       "page.html",
		Language:   "javascript",
		References: []ReferenceInfo{{TargetName: "T_X", RelType: RelSelects, Lang: "plsql"}},
	}

	mergeParsedInto(outer, inner)

	if got := outer.References[0].Lang; got != "plsql" {
		t.Errorf("reference Lang = %q, want the innermost (plsql) to survive the outer merge", got)
	}
}

// THE HALF THAT MAKES A PROJECT'S OWN GRAMMAR PAY OFF: an embedded block's statement is
// attributed to the entity HOSTING it, not to the file.
//
// Without this the graph can only say "this FILE writes to the table". The question
// driving the analysis is which UNIT of the document writes — and that unit is an entity
// the host grammar declared. The engine does not know what it models (a step, a job, a
// handler are all just entities some grammar declared), and it is precisely by not
// knowing that it serves any of them.
func TestEmbeddedBlockIsAttributedToItsHostEntity(t *testing.T) {
	outer := &ParsedFile{
		Path:     "etl/pipeline.xml",
		Language: "xml",
		Entities: map[string][]Entity{
			"elements": {
				{Name: "pipeline", Line: 1, EndLine: 40, GraphLabel: "Element"},
				{Name: "gravaPedido", Line: 8, EndLine: 20, GraphLabel: "Element"},
			},
			"texts": {{Name: "INSERT INTO T_PEDIDO ...", Line: 11, EndLine: 13, GraphLabel: "Text"}},
		},
	}
	inner := &ParsedFile{
		Path:       "etl/pipeline.xml",
		Language:   "plsql",
		References: []ReferenceInfo{{TargetName: "T_PEDIDO", RelType: RelInserts, Line: 12}},
	}

	attributeToHostEntity(outer, inner, 11, 13, nil)

	if got := inner.References[0].SourceName; got != "gravaPedido" {
		t.Errorf("SourceName = %q, want the innermost non-content host entity (gravaPedido); "+
			"got the root element, the Text node, or nothing at all", got)
	}
}

// A block declaring its OWN named unit keeps that unit as the source — host
// attribution fills the empty case, it does not overwrite what the SQL already said.
func TestHostAttributionDoesNotOverrideASourceTheBlockAlreadyHas(t *testing.T) {
	outer := &ParsedFile{
		Path:     "etl/pipeline.xml",
		Language: "xml",
		Entities: map[string][]Entity{
			"elements": {{Name: "gravaPedido", Line: 8, EndLine: 20, GraphLabel: "Element"}},
		},
	}
	inner := &ParsedFile{
		Language: "plsql",
		References: []ReferenceInfo{
			{TargetName: "T_PEDIDO", RelType: RelInserts, Line: 12, SourceName: "p_grava"},
		},
	}

	attributeToHostEntity(outer, inner, 11, 13, nil)

	if got := inner.References[0].SourceName; got != "p_grava" {
		t.Errorf("SourceName = %q, want the procedure the block itself declares", got)
	}
}

// And the attribution has to REACH THE GRAPH as an edge leaving the host — which is the
// test an assertion over SourceName does not make.
func TestHostSourcedEmbeddedEdgeReachesTheGraph(t *testing.T) {
	ri := newRebuildIndexWithDML(map[string]*parseCacheEntry{
		"schema/pedido.sql": {
			RelPath:  "schema/pedido.sql",
			Language: "plsql",
			FileRow:  []string{"schema/pedido.sql", "pedido.sql", "schema/pedido.sql", "false", "plsql", ""},
			Entities: []cachedEntity{{
				Label: LabelTable, UID: "schema/pedido.sql::T_PEDIDO",
				Name: "T_PEDIDO", Path: "schema/pedido.sql", Lang: "plsql",
			}},
		},
		"etl/pipeline.xml": {
			RelPath:  "etl/pipeline.xml",
			Language: "xml",
			FileRow:  []string{"etl/pipeline.xml", "pipeline.xml", "etl/pipeline.xml", "false", "xml", ""},
			Entities: []cachedEntity{{
				Label: "Element", UID: "etl/pipeline.xml::gravaPedido",
				Name: "gravaPedido", Path: "etl/pipeline.xml", Lang: "xml",
			}},
			References: []cachedReference{{
				SourceUID: "etl/pipeline.xml::gravaPedido", TargetUID: "T_PEDIDO",
				RelType: RelInserts, Path: "etl/pipeline.xml", Line: 12, Lang: "plsql",
			}},
		},
	})

	var declared bool
	for _, l := range ri.dmlSourceLabels {
		if l == "Element" {
			declared = true
		}
	}
	if !declared {
		t.Fatalf("host label absent from dmlSourceLabels %v; the FROM-TO pair is never declared", ri.dmlSourceLabels)
	}

	ri.fileNodeJSON()
	ri.entityJSON(LabelTable)
	ri.entityJSON("Element")
	ri.stubTableJSON()

	rows := ri.dmlEdgeJSON(RelInserts, "Element", LabelTable)
	if len(rows) != 1 {
		t.Fatalf("got %d Element→Table INSERTS rows, want 1", len(rows))
	}
	if got := rows[0]["target_uid"]; got != "schema/pedido.sql::T_PEDIDO" {
		t.Errorf("target_uid = %v, want the declared table", got)
	}

	if rows := ri.dmlEdgeJSON(RelInserts, LabelFile, LabelTable); len(rows) != 0 {
		t.Errorf("the host-sourced edge also landed in the File group: %v", rows)
	}
}
