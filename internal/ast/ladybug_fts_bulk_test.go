package ast

import (
	"fmt"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugFTSBulkInsertMaintainsIndex isolates why the first working version of
// SearchIndex answered nothing to every lexical query while CONTAINS found the
// same rows and the vector index ranked them correctly.
//
// The rows were there; the FTS index had never heard of them. The difference from every
// passing probe is the write path: probes insert one CREATE per row with literals, the
// implementation bulk-inserts with "UNWIND $batch AS row CREATE" through a prepared
// statement. TestLadybugFTSUpdateSemantics shows single-row CREATE does maintain the
// index, so the question is whether the bulk path does too.
//
// It matters far beyond tidiness: ~1M entities cannot be inserted one statement at a
// time, so if bulk insert bypasses index maintenance, the index has to be created after
// the bulk load and recreated after every incremental update — a materially different
// design from the one the plan assumed.
//
// CAVEAT, established later and the reason this test is not evidence of working in-place
// updates: every case here writes two rows. With 25 rows, 22 stay invisible in every
// iteration (TestLadybugFTSPerRowInsertIsReliable), so a passing result at this size
// measures a small visibility window rather than index maintenance. Read it as "the write
// paths are equivalent", not as "inserts are indexed".
func TestLadybugFTSBulkInsertMaintainsIndex(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "ftsbulk"), lbug.DefaultSystemConfig())
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
	hits := func(table, index, q string) []string {
		r, e := conn.Query(fmt.Sprintf(
			"CALL QUERY_FTS_INDEX('%s','%s','%s') RETURN node.name AS n ORDER BY score DESC LIMIT 10",
			table, index, escapeQuote(q)))
		if e != nil {
			t.Logf("   query failed: %v", e)
			return nil
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
		return out
	}
	bulkInsert := func(table string, rows []map[string]any) error {
		stmt, err := conn.Prepare(fmt.Sprintf(
			"UNWIND $batch AS row CREATE (:%s {uid: row.uid, name: row.name})", table))
		if err != nil {
			return err
		}
		defer stmt.Close()
		res, err := conn.Execute(stmt, map[string]any{"batch": rows})
		if err != nil {
			return err
		}
		res.Close()
		return nil
	}

	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts unavailable: %v", err)
	}

	rows := []map[string]any{
		{"uid": "b1", "name": "alpha_token"},
		{"uid": "b2", "name": "beta_token"},
	}

	if err := run("CREATE NODE TABLE A(uid STRING, name STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema A: %v", err)
	}
	if err := run("CALL CREATE_FTS_INDEX('A','a_idx',['name'])"); err != nil {
		t.Fatalf("create index A: %v", err)
	}
	if err := bulkInsert("A", rows); err != nil {
		t.Fatalf("bulk insert A: %v", err)
	}
	gotA := hits("A", "a_idx", "alpha")
	t.Logf("A) CREATE_FTS_INDEX then UNWIND bulk insert -> %v", gotA)
	bulkMaintainsIndex := len(gotA) > 0

	if err := run("CREATE NODE TABLE B(uid STRING, name STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema B: %v", err)
	}
	if err := bulkInsert("B", rows); err != nil {
		t.Fatalf("bulk insert B: %v", err)
	}
	if err := run("CALL CREATE_FTS_INDEX('B','b_idx',['name'])"); err != nil {
		t.Fatalf("create index B: %v", err)
	}
	gotB := hits("B", "b_idx", "alpha")
	t.Logf("B) UNWIND bulk insert then CREATE_FTS_INDEX -> %v", gotB)
	indexAfterBulkWorks := len(gotB) > 0

	if err := bulkInsert("B", []map[string]any{{"uid": "b3", "name": "gamma_token"}}); err != nil {
		t.Fatalf("second bulk insert B: %v", err)
	}
	gotC := hits("B", "b_idx", "gamma")
	t.Logf("C) further UNWIND bulk insert into an indexed table -> %v", gotC)
	bulkVisibleAfterIndex := len(gotC) > 0

	dropErr := run("CALL DROP_FTS_INDEX('B','b_idx')")
	createErr := run("CALL CREATE_FTS_INDEX('B','b_idx',['name'])")
	gotD := hits("B", "b_idx", "gamma")
	t.Logf("D) DROP+CREATE after bulk insert -> %v (drop=%v create=%v)", gotD, dropErr, createErr)
	recreateRecovers := len(gotD) > 0

	if err := run("CREATE (:B {uid:'b4', name:'delta_token'})"); err != nil {
		t.Fatalf("single insert B: %v", err)
	}
	gotE := hits("B", "b_idx", "delta")
	t.Logf("E) single-row CREATE into an indexed table (control) -> %v", gotE)
	singleVisible := len(gotE) > 0

	t.Logf("SUMMARY: bulk-then-query=%v, index-after-bulk=%v, bulk-into-indexed=%v, "+
		"drop+create-recovers=%v, single-row-control=%v",
		bulkMaintainsIndex, indexAfterBulkWorks, bulkVisibleAfterIndex, recreateRecovers, singleVisible)

	if !indexAfterBulkWorks {
		t.Fatal("an FTS index built over bulk-loaded rows matches nothing — bulk loading could not " +
			"back this index at all, and the implementation needs a different write path entirely")
	}
	if !bulkVisibleAfterIndex && !recreateRecovers {
		t.Fatal("rows added by bulk insert are invisible to an existing index AND DROP+CREATE does " +
			"not recover them — incremental updates would have no correct implementation")
	}
	if !bulkVisibleAfterIndex && singleVisible {
		t.Log("CONCLUSION: FTS index maintenance is skipped on the UNWIND bulk-insert path but not " +
			"on single-row CREATE. Report upstream; meanwhile create indexes after bulk loading and " +
			"recreate them after incremental writes.")
	}
}
