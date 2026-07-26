package ast

import (
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestEnrichmentQueriesInsideTransaction finds which enrichment statement aborts
// an open transaction. RunEnrichment inside a BEGIN/COMMIT made the COMMIT fail
// with "No active transaction", so the in-place incremental path now runs it
// after committing; this pins down the actual offender.
func TestEnrichmentQueriesInsideTransaction(t *testing.T) {
	cases := []struct {
		name string
		q    string
	}{
		{"read Function", "MATCH (f:Function) RETURN f.uid LIMIT 10"},
		// The decorator scan as DetectFrameworks issues it today. It previously
		// used len(n.decorators) > 0, but LadybugDB has no LEN function, so the
		// query failed on every run ("Catalog exception: function LEN does not
		// exist") — silently, because the caller skips failing queries, which meant
		// decorator-based framework detection never worked. Inside a transaction
		// the same error aborted the transaction and broke the following COMMIT.
		{"read decorators (no len())", "MATCH (n:Function) WHERE n.decorators IS NOT NULL RETURN n.decorators LIMIT 10"},
		{"len() is unsupported (documents the old failure)", "MATCH (n:Function) WHERE len(n.decorators) > 0 RETURN n.decorators LIMIT 10"},
		{"read rel pattern", "MATCH (c)-[:CONTAINS]->(p) RETURN p.name LIMIT 10"},
		{"merge config node", "MERGE (c:File {path: '__config__'}) SET c.name = 'project_config'"},
		{"set entry point", "MATCH (f:Function {uid: 'x'}) SET f.entry_point_score = 1"},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			dir := t.TempDir()
			db, err := lbug.OpenDatabase(filepath.Join(dir, "e"), lbug.DefaultSystemConfig())
			if err != nil {
				t.Skipf("ladybug unavailable: %v", err)
			}
			defer db.Close()
			conn, err := lbug.OpenConnection(db)
			if err != nil {
				t.Fatalf("conn: %v", err)
			}
			defer conn.Close()

			run := func(q string) error {
				res, err := conn.Query(q)
				if err != nil {
					return err
				}
				res.Close()
				return nil
			}
			// Minimal schema mirroring the real one.
			_ = run("CREATE NODE TABLE File(path STRING, name STRING, source STRING, lang STRING, PRIMARY KEY(path))")
			_ = run("CREATE NODE TABLE Function(uid STRING, name STRING, decorators STRING, entry_point_score INT64, PRIMARY KEY(uid))")
			_ = run("CREATE REL TABLE CONTAINS(FROM File TO Function)")
			_ = run("CREATE (:File {path: 'a.sql', name: 'a'})")
			_ = run("CREATE (:Function {uid: 'u1', name: 'f'})")

			if err := run("BEGIN TRANSACTION"); err != nil {
				t.Fatalf("begin: %v", err)
			}
			qErr := run(c.q)
			cErr := run("COMMIT")

			// The len() case is expected to fail; it exists to document why.
			if strings.Contains(c.q, "len(") {
				if qErr == nil {
					t.Errorf("len() unexpectedly works now — the workaround in DetectFrameworks can be revisited")
				} else {
					t.Logf("expected: %v (and it aborts the transaction: commit err = %v)", qErr, cErr)
				}
				return
			}

			switch {
			case cErr != nil:
				t.Errorf(">>> ABORTS TX: %q\n    query error: %v\n    commit error: %v", c.q, qErr, cErr)
			case qErr != nil:
				t.Logf("query errored but tx survived: %v", qErr)
			default:
				t.Logf("ok — tx survived")
			}
		})
	}
}
