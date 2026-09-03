package ast

import (
	"fmt"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugFTSUpdateSemantics establishes exactly when a Ladybug FTS index sees
// data, because the consolidation plan recorded "FTS and VECTOR update on
// insert/delete without a rebuild" as an established fact and the first working
// implementation contradicted it:
//
//	FTS index 'se_split' is inconsistent: term 'checksum' is missing during delete.
//	Drop and recreate the FTS index.
//
// Every earlier probe created the index AFTER inserting rows, so create-then-insert
// was never exercised. The vector index genuinely does update in place
// (TestLadybugVectorIndex), which is what made the claim look proven for both.
//
// The four questions, in the order that determines the design:
//  1. does an index created BEFORE inserts see those rows?
//  2. does an index created AFTER inserts see rows added later?
//  3. does DELETE against a row the index never saw fail, and does it corrupt?
//  4. does DROP + CREATE recover?
//
// CAVEAT: this probe writes two or three rows, and at that size inserts appear to be
// indexed. They are not, in general — with 25 rows, 22 stay invisible in every iteration
// (TestLadybugFTSPerRowInsertIsReliable). The "true" answers below therefore describe a
// small visibility window, and the plan's recorded fact that "FTS updates on
// insert/delete without a rebuild" is false at any real size.
func TestLadybugFTSUpdateSemantics(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "ftsupd"), lbug.DefaultSystemConfig())
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
		r, e := conn.Query(q)
		if e != nil {
			return e
		}
		r.Close()
		return nil
	}
	hits := func(index, q string) ([]string, error) {
		r, e := conn.Query(fmt.Sprintf(
			"CALL QUERY_FTS_INDEX('T','%s','%s') RETURN node.name AS n ORDER BY score DESC LIMIT 10",
			index, escapeQuote(q)))
		if e != nil {
			return nil, e
		}
		defer r.Close()
		var out []string
		for r.HasNext() {
			tup, err := r.Next()
			if err != nil {
				break
			}
			if v, e2 := tup.GetValue(0); e2 == nil {
				out = append(out, fmt.Sprint(v))
			}
		}
		return out, nil
	}

	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts unavailable: %v", err)
	}
	if err := run("CREATE NODE TABLE T(uid STRING, name STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema: %v", err)
	}

	if err := run("CALL CREATE_FTS_INDEX('T','idx_before',['name'])"); err != nil {
		t.Fatalf("CREATE_FTS_INDEX on empty table: %v", err)
	}
	for i, n := range []string{"alpha_token", "beta_token"} {
		if err := run(fmt.Sprintf("CREATE (:T {uid:'e%d', name:'%s'})", i, n)); err != nil {
			t.Fatalf("insert %s: %v", n, err)
		}
	}
	before, err := hits("idx_before", "alpha")
	t.Logf("(1) index created BEFORE inserts, query 'alpha' -> %v (err=%v)", before, err)
	indexSeesLaterInserts := len(before) > 0

	if err := run("CALL CREATE_FTS_INDEX('T','idx_after',['name'])"); err != nil {
		t.Fatalf("CREATE_FTS_INDEX after inserts: %v", err)
	}
	existing, err := hits("idx_after", "alpha")
	t.Logf("(2a) index created AFTER inserts, query 'alpha' -> %v (err=%v)", existing, err)
	if len(existing) == 0 {
		t.Error("an FTS index created over existing rows does not match them — FTS is unusable")
	}

	if err := run("CREATE (:T {uid:'e9', name:'gamma_token'})"); err != nil {
		t.Fatalf("insert gamma_token: %v", err)
	}
	later, err := hits("idx_after", "gamma")
	t.Logf("(2b) row inserted AFTER index creation, query 'gamma' -> %v (err=%v)", later, err)
	incrementalInsertVisible := len(later) > 0

	deleteErr := run("MATCH (t:T {uid:'e0'}) DELETE t")
	t.Logf("(3) DELETE of a row missing from an index -> err=%v", deleteErr)

	dropErr := run("CALL DROP_FTS_INDEX('T','idx_after')")
	recreateErr := run("CALL CREATE_FTS_INDEX('T','idx_after',['name'])")
	after, queryErr := hits("idx_after", "gamma")
	t.Logf("(4) DROP+CREATE -> drop=%v create=%v; query 'gamma' -> %v (err=%v)",
		dropErr, recreateErr, after, queryErr)
	recovers := dropErr == nil && recreateErr == nil && len(after) > 0

	t.Logf("SUMMARY: index sees inserts made after creation = %v / %v; DROP+CREATE recovers = %v",
		indexSeesLaterInserts, incrementalInsertVisible, recovers)

	if !incrementalInsertVisible && !recovers {
		t.Fatal("rows inserted after index creation are invisible AND DROP+CREATE does not recover — " +
			"Ladybug FTS could not back this index at all")
	}
	if !incrementalInsertVisible {
		t.Logf("CONCLUSION: FTS indexes must be created after writing rows, and recreated on " +
			"incremental update. Only the VECTOR index updates in place.")
	}
}
