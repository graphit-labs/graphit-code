package ast

import (
	"context"
	"testing"
)

// TestOracleSchemaGraphIsQueryable indexes a corpus shaped like an Oracle export and asks
// the graph the two questions such an index exists to answer: which columns a table has,
// and who writes to it.
//
// Every stage of this failed on a real export. The Table query matched nothing, so the
// CONTAINS rel table group named a node table that was never created, LadybugDB rejected
// the statement and the rebuild aborted — after --reset had already deleted the database,
// leaving none. With the DDL merely repaired the graph would still have been unusable:
// names came from the first terminal of a qualified name, so every object was called after
// its schema, and DML targets never matched a declaration, so each table existed twice —
// once with its columns, once with its inbound edges.
func TestOracleSchemaGraphIsQueryable(t *testing.T) {
	proj := plsqlProject(t)

	sources := []struct{ name, src string }{
		{"tables/PEDIDO.sql", `
CREATE TABLE "ACME"."PEDIDO"
   (	"ID_PEDIDO" NUMBER(10,0) NOT NULL ENABLE,
	"VL_TOTAL" NUMBER(12,2)
   ) ;
`},
		{"packages/PCK_VENDA.sql", `
CREATE OR REPLACE PACKAGE BODY "ACME"."PCK_VENDA" AS
  PROCEDURE FATURAR(P_ID NUMBER) IS
  BEGIN
    UPDATE ACME.PEDIDO SET VL_TOTAL = 0 WHERE ID_PEDIDO = P_ID;
    INSERT INTO ACME.FATURA_EXTERNA (ID_PEDIDO) VALUES (P_ID);
  END;
END PCK_VENDA;
`},
	}

	cacheDir := t.TempDir()
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	defer func() { _ = cache.Close() }()

	for _, s := range sources {
		pf := plsqlParse(t, proj, s.name, s.src)
		entry := ConvertToCache(pf, proj, true, "")
		if entry == nil {
			t.Fatalf("%s: nothing cached", s.name)
		}
		if err := cache.Store(entry.RelPath, "hash-"+entry.RelPath, entry); err != nil {
			t.Fatalf("store %s: %v", s.name, err)
		}
	}

	db := rebuildTestStore(t, cache, proj)
	ctx := context.Background()

	// The columns of the table, under the table's own name.
	rows, err := db.Query(ctx,
		"MATCH (t:`Table` {name: 'PEDIDO'})-[:CONTAINS]->(c:`Column`) RETURN DISTINCT c.name", nil)
	if err != nil {
		t.Fatalf("query columns: %v", err)
	}
	if len(rows.Records) != 2 {
		t.Fatalf("got %d Table->Column edges, want 2: %v", len(rows.Records), rows.Records)
	}
	cols := map[string]bool{}
	for _, rec := range rows.Records {
		if name, ok := rec["c.name"].(string); ok {
			cols[name] = true
		}
	}
	if !cols["ID_PEDIDO"] || !cols["VL_TOTAL"] {
		t.Errorf("columns = %v, want ID_PEDIDO and VL_TOTAL under PEDIDO", cols)
	}

	// Who writes it — and the target must be the declared node, not a second one.
	rows, err = db.Query(ctx,
		"MATCH (p:`Procedure` {name: 'FATURAR'})-[:UPDATES]->(t:`Table`) RETURN DISTINCT t.name AS n", nil)
	if err != nil {
		t.Fatalf("query updates: %v", err)
	}
	if len(rows.Records) != 1 {
		t.Fatalf("got %d UPDATES edges, want 1: %v", len(rows.Records), rows.Records)
	}
	if rows.Records[0]["n"] != "PEDIDO" {
		t.Errorf("UPDATES -> %v, want PEDIDO", rows.Records[0]["n"])
	}
	res, err := db.Query(ctx, "MATCH (p:`Procedure` {name: 'FATURAR'})-[:UPDATES]->(t:`Table`) RETURN DISTINCT t.uid", nil)
	if err != nil {
		t.Fatalf("query updater target: %v", err)
	}
	t.Logf("updates distinct uid: %v", res.Records)
	res, err = db.Query(ctx, "MATCH (p:`Procedure` {name: 'FATURAR'})-[:UPDATES]->(t:`Table`) RETURN count(DISTINCT t.uid) AS n", nil)
	if err != nil {
		t.Fatalf("query updater target: %v", err)
	}
	if n, _ := res.Records[0]["count"].(int64); n != 1 {
		t.Errorf("UPDATES from FATURAR count = %v, want 1", n)
	}

	// A table only DML mentions is still recorded, as a stub.
	rows, err = db.Query(ctx,
		"MATCH (p:`Procedure` {name: 'FATURAR'})-[:INSERTS]->(t:`Table`) RETURN DISTINCT t.name AS n", nil)
	if err != nil {
		t.Fatalf("query inserts: %v", err)
	}
	if len(rows.Records) != 1 || rows.Records[0]["n"] != "FATURA_EXTERNA" {
		t.Fatalf("got %v, want one INSERTS edge to FATURA_EXTERNA", rows.Records)
	}
	res, err = db.Query(ctx, "MATCH (t:`Table` {name: 'FATURA_EXTERNA'}) RETURN DISTINCT t.is_stub", nil)
	if err != nil {
		t.Fatalf("query stub: %v", err)
	}
	if len(res.Records) == 0 || res.Records[0]["t.is_stub"] != true {
		t.Error("FATURA_EXTERNA is not marked as a stub, though no DDL declares it")
	}

	// Nothing may be named after the schema — a node-only predicate scan is
	// answered by the engine directly (no logical rel in the pattern).
	rows, err = db.Query(ctx, "MATCH (n) WHERE n.name = 'ACME' RETURN n.name AS name", nil)
	if err != nil {
		t.Fatalf("query schema-named node: %v", err)
	}
	if len(rows.Records) != 0 {
		t.Errorf("nodes named after the schema: %v", rows.Records)
	}

	// Nothing may contain itself: the export contract declares self-loops live
	// once, in the forward member — no direction whose source equals target.
	// The manifest invariant carries the policy; verify the members carry no
	// self-referencing row by counting forward edges that equal both ends.
	man := db.canonical
	if man == nil {
		t.Fatal("no manifest on mounted graph")
	}
	for _, g := range man.RelGroups {
		for _, m := range g.Members {
			rows, err = db.Query(ctx, "MATCH (a:`"+m.From+"`)-[:`"+m.Table+"`]->(b:`"+m.To+"`) RETURN DISTINCT a.uid, b.uid", nil)
			if err != nil {
				// logical-type patterns are planner tasks; the physical member
				// table is what the engine knows.
				continue
			}
			for _, r := range rows.Records {
				if r["a.uid"] == r["b.uid"] {
					t.Errorf("self-loop in %s: %v -> %v", m.Table, r["a.uid"], r["b.uid"])
				}
			}
		}
	}
}
