package ast

import (
	"fmt"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugFTSPerRowInsertIsReliable decides whether incremental updates can avoid
// rebuilding the FTS indexes.
//
// Bulk-inserting into a table whose FTS index already exists leaves the index populated
// only intermittently, so UpdateIncremental currently drops and recreates all seven
// indexes after writing. Measured cost: 5.3 s to reflect one edited file on a
// 200k-entity index, against ~330 ms for the pipeline it replaces
// (TestSearchIndexScaleCost). That is O(corpus) work for an O(1) change.
//
// Single-row CREATE behaved differently in every observation — it always registered. If
// that holds under repetition, incremental updates can insert row by row (an edited file
// holds tens of entities, not millions) and leave the indexes alone entirely.
//
// Repetition is the point: the bug being worked around is intermittent, so a single
// passing observation is what created the mistaken "FTS updates in place" claim in the
// first place.
func TestLadybugFTSPerRowInsertIsReliable(t *testing.T) {
	const iterations = 12
	const rowsPerIteration = 25

	failures := 0
	for iter := 0; iter < iterations; iter++ {
		dir := t.TempDir()
		db, err := lbug.OpenDatabase(filepath.Join(dir, "perrow"), lbug.DefaultSystemConfig())
		if err != nil {
			t.Skipf("ladybug unavailable: %v", err)
		}
		conn, err := lbug.OpenConnection(db)
		if err != nil {
			db.Close()
			t.Fatalf("conn: %v", err)
		}

		run := func(q string) error {
			r, e := conn.Query(q)
			if e != nil {
				return e
			}
			r.Close()
			return nil
		}
		_ = run("INSTALL fts")
		if err := run("LOAD EXTENSION fts"); err != nil {
			conn.Close()
			db.Close()
			t.Skipf("fts unavailable: %v", err)
		}

		if err := run("CREATE NODE TABLE R(uid STRING, name STRING, PRIMARY KEY(uid))"); err != nil {
			t.Fatalf("schema: %v", err)
		}
		// Seed and index first, so the index exists before the writes under test —
		// the situation an incremental update is always in.
		for i := 0; i < 10; i++ {
			if err := run(fmt.Sprintf("CREATE (:R {uid:'s%d', name:'seed_%d_token'})", i, i)); err != nil {
				t.Fatalf("seed: %v", err)
			}
		}
		if err := run("CALL CREATE_FTS_INDEX('R','r_idx',['name'])"); err != nil {
			t.Fatalf("create index: %v", err)
		}

		// Per-row inserts through a prepared statement, which is what the
		// implementation would use.
		stmt, err := conn.Prepare("CREATE (:R {uid: $uid, name: $name})")
		if err != nil {
			t.Fatalf("prepare: %v", err)
		}
		for i := 0; i < rowsPerIteration; i++ {
			res, err := conn.Execute(stmt, map[string]any{
				"uid":  fmt.Sprintf("n%d", i),
				"name": fmt.Sprintf("novo_%d_marcador", i),
			})
			if err != nil {
				t.Fatalf("insert %d: %v", i, err)
			}
			res.Close()
		}
		stmt.Close()

		// Every inserted row must be reachable through the index.
		missing := 0
		for i := 0; i < rowsPerIteration; i++ {
			res, err := conn.Query(fmt.Sprintf(
				"CALL QUERY_FTS_INDEX('R','r_idx','novo_%d_marcador') "+
					"RETURN node.uid AS u ORDER BY score DESC LIMIT 3", i))
			if err != nil {
				t.Fatalf("query %d: %v", i, err)
			}
			want := fmt.Sprintf("n%d", i)
			found := false
			for res.HasNext() {
				tup, e := res.Next()
				if e != nil {
					break
				}
				if v, e2 := tup.GetValue(0); e2 == nil && fmt.Sprint(v) == want {
					found = true
				}
			}
			res.Close()
			if !found {
				missing++
			}
		}
		if missing > 0 {
			failures++
			t.Logf("iteration %2d: %d/%d per-row inserts invisible to the index", iter, missing, rowsPerIteration)
		}

		conn.Close()
		db.Close()
	}

	t.Logf("per-row insert visibility: %d/%d iterations had missing rows", failures, iterations)

	// The measured answer is that per-row inserts are NOT reliable either: 22 of 25 rows
	// stayed invisible in every one of 12 iterations. The count is stable rather than
	// random, which points at a fixed visibility window and explains why the earliest
	// probes "proved" in-place updates — they inserted one or two rows, which fell inside
	// it.
	//
	// So the assertion is inverted deliberately: this test now WATCHES FOR THE UPSTREAM
	// FIX. While it passes, UpdateIncremental must keep recreating the FTS indexes and pay
	// the O(corpus) cost. When it starts failing, liblbug maintains FTS on insert and the
	// rebuild can be dropped — which is worth being told about, since it is the difference
	// between a 5.3 s and a sub-second incremental update on a 200k-entity index.
	if failures == 0 {
		t.Errorf("per-row inserts are now visible to the index in all %d iterations — liblbug appears "+
			"to maintain FTS on insert. UpdateIncremental can stop calling rebuildFTSIndexes, removing "+
			"the O(corpus) cost per edit; re-measure TestSearchIndexScaleCost and simplify",
			iterations)
	}
}
