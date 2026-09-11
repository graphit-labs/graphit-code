package ast

import (
	"path/filepath"
	"testing"
)

func oracleLikeEntries() map[string]*parseCacheEntry {
	return map[string]*parseCacheEntry{
		"tables/PEDIDO.sql": {
			RelPath:  "tables/PEDIDO.sql",
			Language: "plsql",
			Entities: []cachedEntity{
				{Label: LabelTable, UID: "tables/PEDIDO.sql::PEDIDO", Name: "PEDIDO", Path: "tables/PEDIDO.sql", Line: 1},
				{Label: LabelColumn, UID: "tables/PEDIDO.sql::PEDIDO.ID", Name: "ID", Path: "tables/PEDIDO.sql", Line: 2,
					Context: "PEDIDO", ContextType: LabelTable},
				{Label: LabelColumn, UID: "tables/PEDIDO.sql::ORFA.X", Name: "X", Path: "tables/PEDIDO.sql", Line: 3,
					Context: "ORFA", ContextType: "Tablespace"},
			},
			ContainsEdges: []cachedContainsEdge{
				{ParentUID: "tables/PEDIDO.sql::PEDIDO", ChildUID: "tables/PEDIDO.sql::PEDIDO.ID",
					ParentLabel: LabelTable, ChildLabel: LabelColumn},
				{ParentUID: "tables/PEDIDO.sql::ORFA", ChildUID: "tables/PEDIDO.sql::ORFA.X",
					ParentLabel: "Tablespace", ChildLabel: LabelColumn},
			},
		},
		"packages/PCK_VENDA.sql": {
			RelPath:  "packages/PCK_VENDA.sql",
			Language: "plsql",
			Entities: []cachedEntity{
				{Label: LabelProcedure, UID: "packages/PCK_VENDA.sql::FATURAR", Name: "FATURAR",
					Path: "packages/PCK_VENDA.sql", Line: 2},
			},
			References: []cachedReference{
				{SourceUID: "packages/PCK_VENDA.sql::FATURAR", TargetUID: "PEDIDO", RelType: RelSelects,
					Path: "packages/PCK_VENDA.sql", Line: 5},
				{SourceUID: "packages/PCK_VENDA.sql::FATURAR", TargetUID: "FATURA_EXTERNA", RelType: RelInserts,
					Path: "packages/PCK_VENDA.sql", Line: 6},
			},
		},
	}
}

func TestContainsPairWithoutNodeTableIsDropped(t *testing.T) {
	ri := newRebuildIndex(oracleLikeEntries(), dmlTargetRules("plsql"))

	for _, p := range ri.containsPairs {
		if !ri.labelSet[p[0]] || !ri.labelSet[p[1]] {
			t.Errorf("pair %s->%s survived with no node table for one end", p[0], p[1])
		}
	}
	if !hasPair(ri.containsPairs, LabelTable, LabelColumn) {
		t.Errorf("the valid Table->Column pair was dropped: %v", ri.containsPairs)
	}
	if hasPair(ri.containsPairs, "Tablespace", LabelColumn) {
		t.Errorf("the unbacked Tablespace->Column pair survived: %v", ri.containsPairs)
	}
}

func hasPair(pairs [][2]string, from, to string) bool {
	for _, p := range pairs {
		if p[0] == from && p[1] == to {
			return true
		}
	}
	return false
}

// TestInitSchemaForLabelsSurvivesAnUnbackedPair drives the DDL against a real database:
// the filter above is the fix, and this is the assertion that the statement it produces
// is one LadybugDB accepts. It passes an unbacked pair explicitly, so it keeps holding
// even if a future caller stops filtering.
func TestInitSchemaForLabelsSurvivesAnUnbackedPair(t *testing.T) {
	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), IcebugDir: filepath.Join(filepath.Dir(filepath.Join(t.TempDir(), "ladybugdb")), "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	info := SchemaInfo{
		Labels: []string{LabelTable, LabelColumn, LabelProcedure},
		ContainsPairs: [][2]string{
			{LabelTable, LabelColumn},
			{"Tablespace", LabelColumn},
		},
		CallerLabels:    []string{LabelProcedure},
		DMLTypes:        []string{RelSelects},
		DMLTargetLabels: []string{LabelTable, "Tablespace"},
		DMLSourceLabels: []string{LabelProcedure},
	}
	if err := db.initSchemaForLabels(info); err != nil {
		t.Fatalf("schema: %v", err)
	}

	if err := db.execQuery("CREATE (t:`Table` {uid: 't', name: 'PEDIDO'})"); err != nil {
		t.Fatalf("insert table node: %v", err)
	}
	if err := db.execQuery("CREATE (c:`Column` {uid: 'c', name: 'ID'})"); err != nil {
		t.Fatalf("insert column node: %v", err)
	}
	if err := db.execQuery("MATCH (t:`Table` {uid: 't'}), (c:`Column` {uid: 'c'}) CREATE (t)-[:CONTAINS]->(c)"); err != nil {
		t.Fatalf("CONTAINS Table->Column rejected: %v", err)
	}
}

// TestDMLTargetResolvesToTheDeclaringNode covers the other half of a usable schema graph.
//
// A reference is cached as a bare object name while an entity's uid is scoped by the file
// that declares it, so no DML target ever matched a declaration: every table became a stub,
// and the graph held two nodes per table — one with the columns, one with the inbound
// SELECTS — which no query could join.
func TestDMLTargetResolvesToTheDeclaringNode(t *testing.T) {
	ri := newRebuildIndex(oracleLikeEntries(), dmlTargetRules("plsql"))

	uid, label := ri.resolveRefTarget(cachedReference{
		TargetUID: "PEDIDO", RelType: RelSelects, Path: "packages/PCK_VENDA.sql",
	}, "plsql")
	if uid != "tables/PEDIDO.sql::PEDIDO" || label != LabelTable {
		t.Errorf("PEDIDO resolved to %q/%q, want the declaring node", uid, label)
	}

	uid, label = ri.resolveRefTarget(cachedReference{
		TargetUID: "FATURA_EXTERNA", RelType: RelInserts, Path: "packages/PCK_VENDA.sql",
	}, "plsql")
	if uid != "FATURA_EXTERNA" || label != LabelTable {
		t.Errorf("undeclared target resolved to %q/%q, want a stub Table", uid, label)
	}

	if len(ri.dmlTargetLabels) != 1 || ri.dmlTargetLabels[0] != LabelTable {
		t.Errorf("dml target labels = %v, want [Table]", ri.dmlTargetLabels)
	}

	declared := ri.entityJSON(LabelTable)
	if len(declared) != 1 {
		t.Fatalf("got %d Table rows, want the declared one", len(declared))
	}
	for _, label := range ri.labels {
		if label != LabelTable {
			ri.entityJSON(label)
		}
	}
	stubs := ri.stubTableJSON()
	if len(stubs) != 1 || stubs[0]["uid"] != "FATURA_EXTERNA" {
		t.Fatalf("stubs = %v, want only FATURA_EXTERNA — the declared table must not be duplicated", stubs)
	}

	edges := ri.dmlEdgeJSON(RelSelects, LabelProcedure, LabelTable)
	if len(edges) != 1 {
		t.Fatalf("got %d SELECTS edges, want 1: %v", len(edges), edges)
	}
	if edges[0]["target_uid"] != "tables/PEDIDO.sql::PEDIDO" {
		t.Errorf("SELECTS points at %q, want the declared table node", edges[0]["target_uid"])
	}
}
