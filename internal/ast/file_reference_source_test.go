package ast

import (
	"testing"
)

// A reference with no entity around it belongs to the FILE, not to nobody.
//
// The case that forced it: a SQL statement at the top of a script — `insert into
// auditoria …` — has no procedure around it, so `SourceName` is empty, `SourceUID` was
// left empty alongside it, and the edge was built and discarded. Exactly as the Import
// entity was built and thrown away with a `continue`.
//
// The same holds for a `var(--x)` at the top of a CSS rule and for SQL inside an
// embedded block of a config XML, which is a bare statement by nature.
//
// The file as source is the shape `IMPORTS` already uses — `File -[:IMPORTS]-> Module`
// — and the question "what touches this table" is about the file when there is nothing
// smaller that can be named.

func TestReferenceWithNoEnclosingEntityIsSourcedAtTheFile(t *testing.T) {
	pf := &ParsedFile{
		Path:     "a.sql",
		Language: "sql",
		References: []ReferenceInfo{
			{TargetName: "auditoria", RelType: "INSERTS", Line: 1},
			{TargetName: "cliente", RelType: "SELECTS", Line: 3, SourceName: "p_lista"},
		},
		Entities: map[string][]Entity{
			"procedures": {{Name: "p_lista", Line: 3, EndLine: 5, GraphLabel: "Procedure"}},
		},
	}
	entry := ConvertToCache(pf, ".", false, "")
	if entry == nil {
		t.Fatal("nil entry")
	}
	if len(entry.References) != 2 {
		t.Fatalf("got %d references, want 2", len(entry.References))
	}
	byTarget := map[string]cachedReference{}
	for _, r := range entry.References {
		byTarget[r.TargetUID] = r
	}
	// No entity around it: the file is the source.
	if got := byTarget["auditoria"].SourceUID; got != "a.sql" {
		t.Errorf("orphan reference sourced at %q, want the file path", got)
	}
	// Com entidade em volta: nada muda.
	if got := byTarget["cliente"].SourceUID; got == "a.sql" || got == "" {
		t.Errorf("a contained reference was re-sourced at the file: %q", got)
	}
}

// End to end: the writer has to EMIT the edge, not merely carry it. A test over
// ConvertToCache alone would pass with the edge discarded further down — which is
// exactly how the limitation survived the first round.
func TestFileSourcedDMLEdgeReachesTheGraph(t *testing.T) {
	ri := newRebuildIndexWithDML(map[string]*parseCacheEntry{
		"schema.sql": {
			RelPath:  "schema.sql",
			Language: "sql",
			FileRow:  []string{"schema.sql", "schema.sql", "schema.sql", "false", "sql", ""},
			References: []cachedReference{
				{SourceUID: "schema.sql", TargetUID: "auditoria", RelType: "INSERTS", Path: "schema.sql", Line: 1},
			},
		},
	})

	// File does NOT enter dmlSourceLabels: the schema already declares
	// `FROM File TO <target>` unconditionally, and announcing it here would emit the
	// pair twice — LadybugDB rejects the whole group with "duplicate FROM-TO pairs".
	// Measured on the live graph, not deduced.
	for _, l := range ri.dmlSourceLabels {
		if l == "File" {
			t.Errorf("File leaked into dmlSourceLabels; the DDL would declare the pair twice")
		}
	}

	// And the rows have to come out. Order matters and is the real rebuild's: nodes are
	// emitted BEFORE edges, because dmlEdgeJSON only writes an edge whose two ends
	// already exist.
	ri.fileNodeJSON("")
	ri.stubTableJSON()
	rows := ri.dmlEdgeJSON("INSERTS", "File", LabelTable)
	if len(rows) != 1 {
		t.Fatalf("got %d File-sourced INSERTS rows, want 1", len(rows))
	}
	if rows[0]["source_uid"] != "schema.sql" {
		t.Errorf("source_uid = %v, want the file path", rows[0]["source_uid"])
	}
}

// An entity's reference still leaves through the entity, not through the file.
func TestEntitySourcedEdgeIsUnchanged(t *testing.T) {
	ri := newRebuildIndexWithDML(map[string]*parseCacheEntry{
		"pkg.sql": {
			RelPath:  "pkg.sql",
			Language: "plsql",
			FileRow:  []string{"pkg.sql", "pkg.sql", "pkg.sql", "false", "plsql", ""},
			Entities: []cachedEntity{
				{Label: "Procedure", UID: "pkg.sql::p_lista", Name: "p_lista", Path: "pkg.sql", Lang: "plsql"},
			},
			References: []cachedReference{
				{SourceUID: "pkg.sql::p_lista", TargetUID: "xpto", RelType: "SELECTS", Path: "pkg.sql", Line: 4},
			},
		},
	})
	ri.fileNodeJSON("")
	ri.entityJSON("Procedure")
	ri.stubTableJSON()
	if rows := ri.dmlEdgeJSON("SELECTS", "File", LabelTable); len(rows) != 0 {
		t.Errorf("an entity-sourced edge leaked into the File group: %v", rows)
	}
	if rows := ri.dmlEdgeJSON("SELECTS", "Procedure", LabelTable); len(rows) != 1 {
		t.Errorf("got %d Procedure-sourced rows, want 1", len(rows))
	}
}

// The symmetry: a CALL at the top of a script also has no entity around it, and the
// CALLS edge was discarded just as the DML one was.
//
// It holds for any language with top-level calls — an `init()` at the end of a
// JavaScript module, a Python `main()`, a bare SQL statement — not only for the
// embedded SQL that exposed the case.
func TestCallWithNoEnclosingEntityIsCalledByTheFile(t *testing.T) {
	pf := &ParsedFile{
		Path:     "script.sql",
		Language: "plsql",
		CallSites: []CallInfo{
			{Name: "p_carga", Line: 1},
			{Name: "p_log", Line: 7, SourceName: "p_principal", SourceType: "Procedure"},
		},
		Entities: map[string][]Entity{
			"procedures": {{Name: "p_principal", Line: 5, EndLine: 9, GraphLabel: "Procedure"}},
		},
	}
	entry := ConvertToCache(pf, ".", false, "")
	if entry == nil {
		t.Fatal("nil entry")
	}
	byCallee := map[string]cachedCall{}
	for _, c := range entry.Calls {
		byCallee[c.CalleeUID] = c
	}
	top, ok := byCallee["p_carga"]
	if !ok {
		t.Fatalf("the top-level call vanished; calls: %+v", entry.Calls)
	}
	if top.CallerUID != "script.sql" || top.SourceType != LabelFile {
		t.Errorf("top-level call has caller %q/%q, want the file", top.CallerUID, top.SourceType)
	}
	// The contained call does not change.
	inner := byCallee["p_log"]
	if inner.SourceType != "Procedure" || inner.CallerUID == "script.sql" {
		t.Errorf("a contained call was re-sourced at the file: %q/%q",
			inner.CallerUID, inner.SourceType)
	}
}

func TestFileCalledEdgeReachesTheGraph(t *testing.T) {
	ri := newRebuildIndexWithDML(map[string]*parseCacheEntry{
		"script.sql": {
			RelPath:  "script.sql",
			Language: "plsql",
			FileRow:  []string{"script.sql", "script.sql", "script.sql", "false", "plsql", ""},
			Calls: []cachedCall{
				{CallerUID: "script.sql", CalleeUID: "p_carga", SourceType: LabelFile,
					Path: "script.sql", Line: 1},
			},
		},
	})
	// Unlike DML, the CALLS schema has no fixed File step: it derives its pairs from
	// CallerLabels, so File MUST be there — and the generator's `seen` prevents the
	// duplicate pair that broke the first attempt on the DML side.
	found := false
	for _, l := range ri.callerLabels {
		if l == LabelFile {
			found = true
		}
	}
	if !found {
		t.Errorf("File is not a caller label; got %v", ri.callerLabels)
	}

	// The GATE, not just the row generator. The first version of this test called
	// callEdgeJSON directly and passed while the real path discarded everything: the
	// writer's loop consulted labelSet alone, and File is deliberately not in it.
	if !ri.canWriteCallerLabel(LabelFile) {
		t.Error("the writer would refuse the file-sourced CALLS group")
	}

	ri.fileNodeJSON("")
	ri.stubFunctionJSON()
	rows := ri.callEdgeJSON(LabelFile, LabelFunction)
	if len(rows) != 1 {
		t.Fatalf("got %d File-called rows, want 1", len(rows))
	}
	if rows[0]["caller_uid"] != "script.sql" {
		t.Errorf("caller_uid = %v, want the file path", rows[0]["caller_uid"])
	}
}

// A file is never a call TARGET, so the pair `X → File` must not be declared.
func TestFileIsNeverACallTarget(t *testing.T) {
	info := SchemaInfo{CallerLabels: []string{LabelFile, "Procedure"}, CalleeLabels: []string{LabelFunction}, Labels: []string{"Procedure"}}
	for _, pair := range callRelPairsForTest(info) {
		if pair[1] == LabelFile {
			t.Errorf("declared a call pair targeting File: %v", pair)
		}
	}
}

func callRelPairsForTest(info SchemaInfo) [][2]string {
	nodeTables := map[string]bool{LabelFile: true, LabelFunction: true}
	for _, l := range info.Labels {
		nodeTables[l] = true
	}
	for _, l := range info.CallerLabels {
		nodeTables[l] = true
	}
	return callRelPairs(info.CallerLabels, info.CalleeLabels, nodeTables)
}
