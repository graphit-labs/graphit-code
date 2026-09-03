package ast

import (
	"fmt"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

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
		for i := 0; i < 10; i++ {
			if err := run(fmt.Sprintf("CREATE (:R {uid:'s%d', name:'seed_%d_token'})", i, i)); err != nil {
				t.Fatalf("seed: %v", err)
			}
		}
		if err := run("CALL CREATE_FTS_INDEX('R','r_idx',['name'])"); err != nil {
			t.Fatalf("create index: %v", err)
		}

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

	if failures == 0 {
		t.Errorf("per-row inserts are now visible to the index in all %d iterations — liblbug appears "+
			"to maintain FTS on insert. UpdateIncremental can stop calling rebuildFTSIndexes, removing "+
			"the O(corpus) cost per edit; re-measure TestSearchIndexScaleCost and simplify",
			iterations)
	}
}
