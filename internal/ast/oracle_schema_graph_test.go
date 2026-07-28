package ast

import (
	"context"
	"path/filepath"
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

	dbPath := filepath.Join(t.TempDir(), "ladybugdb")
	writer := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := writer.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}

	ctx := context.Background()
	if err := RebuildFromJSON(ctx, writer, cache, nil, "", proj, nil); err != nil {
		_ = writer.Close()
		t.Fatalf("rebuild: %v", err)
	}
	// The rebuild builds a temp database and renames it into place, so a handle
	// opened before the swap still points at the file that was moved aside.
	_ = writer.Close()

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := db.connect(); err != nil {
		t.Fatalf("reopen after swap: %v", err)
	}
	defer func() { _ = db.Close() }()

	// The columns of the table, under the table's own name.
	rows, err := db.Query(ctx,
		"MATCH (t:`Table`)-[:CONTAINS]->(c:`Column`) RETURN t.name AS tbl, c.name AS col ORDER BY col", nil)
	if err != nil {
		t.Fatalf("query columns: %v", err)
	}
	if len(rows.Records) != 2 {
		t.Fatalf("got %d Table->Column edges, want 2: %v", len(rows.Records), rows.Records)
	}
	for _, rec := range rows.Records {
		if rec["tbl"] != "PEDIDO" {
			t.Errorf("column %v hangs off %q, want PEDIDO", rec["col"], rec["tbl"])
		}
	}

	// Who writes it — and the target must be the declared node, not a second one.
	rows, err = db.Query(ctx,
		"MATCH (p:`Procedure`)-[:UPDATES]->(t:`Table`) RETURN p.name AS proc, t.name AS tbl, t.is_stub AS stub", nil)
	if err != nil {
		t.Fatalf("query updates: %v", err)
	}
	if len(rows.Records) != 1 {
		t.Fatalf("got %d UPDATES edges, want 1: %v", len(rows.Records), rows.Records)
	}
	rec := rows.Records[0]
	if rec["proc"] != "FATURAR" || rec["tbl"] != "PEDIDO" {
		t.Errorf("UPDATES edge is %v -> %v, want FATURAR -> PEDIDO", rec["proc"], rec["tbl"])
	}
	if stub, _ := rec["stub"].(bool); stub {
		t.Error("UPDATES points at a stub: the DML target did not resolve to the declared table")
	}

	// A table only DML mentions is still recorded, as a stub.
	rows, err = db.Query(ctx,
		"MATCH (p:`Procedure`)-[:INSERTS]->(t:`Table`) RETURN t.name AS tbl, t.is_stub AS stub", nil)
	if err != nil {
		t.Fatalf("query inserts: %v", err)
	}
	if len(rows.Records) != 1 || rows.Records[0]["tbl"] != "FATURA_EXTERNA" {
		t.Fatalf("got %v, want one INSERTS edge to FATURA_EXTERNA", rows.Records)
	}
	if stub, _ := rows.Records[0]["stub"].(bool); !stub {
		t.Error("FATURA_EXTERNA is not marked as a stub, though no DDL declares it")
	}

	// Nothing may be named after the schema, and nothing may contain itself.
	for _, probe := range []struct {
		what  string
		query string
	}{
		{"nodes named after the schema", "MATCH (n) WHERE n.name = 'ACME' RETURN n.name AS name"},
		{"CONTAINS self-loops", "MATCH (a)-[:CONTAINS]->(b) WHERE a.name = b.name RETURN a.name AS name"},
	} {
		rows, err = db.Query(ctx, probe.query, nil)
		if err != nil {
			t.Fatalf("query %s: %v", probe.what, err)
		}
		if len(rows.Records) != 0 {
			t.Errorf("%s: %v", probe.what, rows.Records)
		}
	}
}
