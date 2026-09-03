package ast

import (
	"fmt"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugFTSQueryAcceptsBoundParameter is the last candidate explanation for a
// SearchIndex that stored its rows correctly — CONTAINS found them, the vector
// index ranked them — yet answered nothing to every FTS query, with no error.
//
// Bulk insert maintains the index (TestLadybugFTSBulkInsertMaintainsIndex) and inserts
// are visible to an index created before them (TestLadybugFTSUpdateSemantics), so the
// write path is sound. What the implementation does differently from every passing
// probe is on the READ side: it binds the query text as a parameter,
//
//	CALL QUERY_FTS_INDEX('T', 'idx', $q)
//
// whereas the probes interpolate the text into the statement. A table function argument
// that must be a literal would match nothing and report no error — exactly the symptom.
//
// This is worth a test rather than a quick edit: passing user text as a parameter is the
// right thing to do (no escaping, no injection), so whether it is possible determines
// whether the implementation has to escape query text by hand forever.
func TestLadybugFTSQueryAcceptsBoundParameter(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "ftsparam"), lbug.DefaultSystemConfig())
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
	collect := func(res *lbug.QueryResult) []string {
		var out []string
		for res.HasNext() {
			tup, err := res.Next()
			if err != nil {
				break
			}
			if v, e := tup.GetValue(0); e == nil {
				out = append(out, fmt.Sprint(v))
			}
		}
		return out
	}

	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts unavailable: %v", err)
	}
	if err := run("CREATE NODE TABLE P(uid STRING, name STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema: %v", err)
	}
	for i, n := range []string{"alpha_token", "beta_token"} {
		if err := run(fmt.Sprintf("CREATE (:P {uid:'p%d', name:'%s'})", i, n)); err != nil {
			t.Fatalf("insert: %v", err)
		}
	}
	if err := run("CALL CREATE_FTS_INDEX('P','p_idx',['name'])"); err != nil {
		t.Fatalf("create index: %v", err)
	}

	res, err := conn.Query(
		"CALL QUERY_FTS_INDEX('P','p_idx','alpha') RETURN node.name AS n ORDER BY score DESC")
	if err != nil {
		t.Fatalf("interpolated query failed: %v", err)
	}
	literalHits := collect(res)
	res.Close()
	t.Logf("interpolated literal -> %v", literalHits)
	if len(literalHits) == 0 {
		t.Fatal("the control query matched nothing — the probe is broken, not the parameter binding")
	}

	stmt, err := conn.Prepare(
		"CALL QUERY_FTS_INDEX('P','p_idx',$q) RETURN node.name AS n ORDER BY score DESC")
	if err != nil {
		t.Logf("bound-parameter query could not even be prepared: %v", err)
		t.Log("CONCLUSION: query text must be interpolated and escaped by hand")
		return
	}
	defer stmt.Close()
	res2, err := conn.Execute(stmt, map[string]any{"q": "alpha"})
	if err != nil {
		t.Logf("bound-parameter query failed at execute: %v", err)
		t.Log("CONCLUSION: query text must be interpolated and escaped by hand")
		return
	}
	paramHits := collect(res2)
	res2.Close()
	t.Logf("bound parameter        -> %v", paramHits)

	if len(paramHits) == 0 {
		t.Errorf("QUERY_FTS_INDEX accepted a bound parameter but matched nothing, while the same text "+
			"interpolated matched %v — binding silently yields no results, so query text must be "+
			"interpolated and escaped by hand", literalHits)
		return
	}
	if len(paramHits) != len(literalHits) {
		t.Errorf("bound parameter and interpolated literal disagree: %v vs %v", paramHits, literalHits)
	}
}
