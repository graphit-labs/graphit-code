package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// The corpus and the raw-engine probes that decided the move off SQLite.
//
// TestConsolidationQualityGate used to live here: it built the same corpus twice, once
// through a hand-rolled Ladybug FTS index and once through the SQLite one, and refused the
// migration unless Ladybug ranked at least as well. It is gone because there is no second
// engine to compare against — the SQLite index no longer exists — and a differential test
// with one side removed compares an implementation to a toy.
//
// What replaced it is TestSearchIndexQualityFloor, which asserts an absolute floor on the
// same corpus and the same probes, with the number the differential run measured written
// into it as the bar.
//
// What stays here is the corpus itself, shared by every search test, and
// TestLadybugFTSFeatureParity — the probe that the capabilities the migration needed are
// really in the engine, rather than assumed.

type gateEntity struct {
	uid, name, docstring, entityType, path string
}

func gateCorpus() []gateEntity {
	return []gateEntity{
		{"u1", "parseConfig", "Parses the configuration file into a Config struct.", "Function", "config.go"},
		{"u2", "Config", "Configuration for the parser.", "Struct", "config.go"},
		{"u3", "loadUserConfig", "Loads user level configuration overrides.", "Function", "user.go"},
		{"u4", "validateSchema", "Validates the database schema before migration.", "Function", "schema.go"},
		{"u5", "SchemaValidator", "Validates schemas.", "Class", "schema.go"},
		{"u6", "connectDatabase", "Opens a connection to the database.", "Function", "db.go"},
		{"u7", "closeDatabase", "Closes the database connection.", "Function", "db.go"},
		{"u8", "retryPolicy", "Retry policy with exponential backoff for network calls.", "Struct", "retry.go"},
		{"u9", "computeChecksum", "Computes a checksum of the payload.", "Function", "hash.go"},
		{"u10", "parseSQL", "Parses a SQL statement into an AST.", "Function", "sql.go"},
	}
}

// gateQueries are the probes; each names the entity a developer would expect
// first, so the comparison measures usefulness rather than raw overlap.
func buildLadybugFTS(t *testing.T, dir string) *lbug.Connection {
	t.Helper()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "lbfts"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	t.Cleanup(func() { db.Close() })
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("conn: %v", err)
	}
	t.Cleanup(func() { conn.Close() })

	run := func(q string) error {
		res, err := conn.Query(q)
		if err != nil {
			return err
		}
		res.Close()
		return nil
	}
	if err := run("INSTALL fts"); err != nil {
		t.Skipf("fts extension unavailable: %v", err)
	}
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts extension cannot load: %v", err)
	}
	if err := run("CREATE NODE TABLE Ent(uid STRING, name STRING, docstring STRING, etype STRING, path STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema: %v", err)
	}
	for _, e := range gateCorpus() {
		q := fmt.Sprintf("CREATE (:Ent {uid: '%s', name: '%s', docstring: '%s', etype: '%s', path: '%s'})",
			e.uid, e.name, escapeQuote(e.docstring), e.entityType, e.path)
		if err := run(q); err != nil {
			t.Fatalf("seed %s: %v", e.uid, err)
		}
	}
	if err := run("CALL CREATE_FTS_INDEX('Ent', 'ent_fts', ['name', 'docstring'])"); err != nil {
		t.Fatalf("create fts index: %v", err)
	}
	return conn
}

func escapeQuote(s string) string { return strings.ReplaceAll(s, "'", "''") }

// ladybugSearch returns the entity names Ladybug ranks for a query, best first.
func TestLadybugFTSFeatureParity(t *testing.T) {
	dir := t.TempDir()
	conn := buildLadybugFTS(t, dir)

	// Per-field weighting: a dedicated index per field lets the caller apply its
	// own weights, replacing FTS5's bm25(...) column weights.
	for _, ddl := range []string{
		"CALL CREATE_FTS_INDEX('Ent', 'idx_name_only', ['name'])",
		"CALL CREATE_FTS_INDEX('Ent', 'idx_doc_only', ['docstring'])",
	} {
		res, err := conn.Query(ddl)
		if err != nil {
			t.Fatalf("per-field index unavailable, weighting cannot be reconstructed: %v", err)
		}
		res.Close()
	}
	res, err := conn.Query(`
		CALL QUERY_FTS_INDEX('Ent','idx_name_only','configuration') RETURN node.name AS n, score*10 AS s
		UNION ALL
		CALL QUERY_FTS_INDEX('Ent','idx_doc_only','configuration') RETURN node.name AS n, score*3 AS s`)
	if err != nil {
		t.Fatalf("weighted combination across per-field indexes failed: %v", err)
	}
	res.Close()

	// Substring matching without a trigram tokenizer. Note this is a scan, not an
	// index lookup: availability is proven here, cost at scale is not.
	res, err = conn.Query("MATCH (n:Ent) WHERE n.name CONTAINS 'onfig' RETURN n.name")
	if err != nil {
		t.Fatalf("CONTAINS unavailable, substring search cannot be reconstructed: %v", err)
	}
	found := false
	for res.HasNext() {
		if _, e := res.Next(); e == nil {
			found = true
		}
	}
	res.Close()
	if !found {
		t.Error("CONTAINS returned nothing for a substring that exists — substring parity not demonstrated")
	}

	// BM25 tuning knobs.
	res, err = conn.Query("CALL QUERY_FTS_INDEX('Ent','idx_name_only','configuration', K := 1.2, B := 0.75) RETURN node.name LIMIT 1")
	if err != nil {
		t.Logf("BM25 tuning options rejected (not required for parity): %v", err)
	} else {
		res.Close()
		t.Log("BM25 tuning via K/B accepted")
	}
}
