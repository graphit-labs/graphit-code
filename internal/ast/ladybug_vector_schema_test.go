package ast

import (
	"fmt"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugVectorSchemaConstraints resolves the three unknowns that decide the
// schema of the Ladybug-backed search index, before writing it.
//
// They matter because embeddings do NOT arrive with the parse. A full index writes
// every entity immediately, while the embedder loop fills vectors in later cycles,
// so at rebuild time most rows have no vector at all. Three ways that can go wrong:
//
//  1. Can a vector index be created on an EMPTY table? If not, index creation has to
//     be deferred until the first vector exists, which is extra state to carry.
//  2. Are rows with a NULL vector accepted by a table that has a vector index, and
//     does the query skip them? If not, vectors need their own table — which is what
//     the SQLite design did with entity_vec_map, and would have been mirrored.
//  3. Does bulk insert via UNWIND carry a FLOAT[768] property? Per-row inserts are
//     the fallback, but ~1M entities makes that the difference between minutes and
//     hours.
func TestLadybugVectorSchemaConstraints(t *testing.T) {
	const dim = 768

	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "vschema"), lbug.DefaultSystemConfig())
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
	_ = run("INSTALL vector")
	if err := run("LOAD EXTENSION vector"); err != nil {
		t.Skipf("vector extension unavailable: %v", err)
	}

	vec := func(band int) []float32 {
		v := make([]float32, dim)
		for i := band * 64; i < band*64+64 && i < dim; i++ {
			v[i] = 0.125
		}
		return v
	}

	if err := run(fmt.Sprintf(
		"CREATE NODE TABLE E(uid STRING, name STRING, emb FLOAT[%d], PRIMARY KEY(uid))", dim)); err != nil {
		t.Fatalf("schema: %v", err)
	}

	emptyIndexOK := run("CALL CREATE_VECTOR_INDEX('E', 'e_vec', 'emb')") == nil
	if emptyIndexOK {
		t.Log("(1) CREATE_VECTOR_INDEX on an EMPTY table: accepted")
	} else {
		err := run("CALL CREATE_VECTOR_INDEX('E', 'e_vec', 'emb')")
		t.Logf("(1) CREATE_VECTOR_INDEX on an EMPTY table: REJECTED (%v) — creation must be deferred", err)
	}

	nullRowOK := true
	if err := run("CREATE (:E {uid:'n1', name:'noVector'})"); err != nil {
		nullRowOK = false
		t.Logf("(2) row with NULL emb: REJECTED (%v) — vectors need a separate table", err)
	} else {
		t.Log("(2) row with NULL emb: accepted")
	}

	stmt, err := conn.Prepare(
		"UNWIND $batch AS row CREATE (:E {uid: row.uid, name: row.name, emb: row.emb})")
	unwindOK := false
	if err != nil {
		t.Logf("(3) UNWIND prepare failed: %v", err)
	} else {
		defer stmt.Close()
		batch := []map[string]any{
			{"uid": "b1", "name": "batchOne", "emb": vec(0)},
			{"uid": "b2", "name": "batchTwo", "emb": vec(1)},
		}
		res, err := conn.Execute(stmt, map[string]any{"batch": batch})
		if err != nil {
			t.Logf("(3) UNWIND with a FLOAT[%d] property: REJECTED (%v) — per-row inserts required", dim, err)
		} else {
			res.Close()
			unwindOK = true
			t.Logf("(3) UNWIND with a FLOAT[%d] property: accepted", dim)
		}
	}

	if !emptyIndexOK {
		if err := run("CALL CREATE_VECTOR_INDEX('E', 'e_vec', 'emb')"); err != nil {
			t.Fatalf("CREATE_VECTOR_INDEX after inserting rows also failed: %v", err)
		}
		t.Log("(1b) CREATE_VECTOR_INDEX after rows exist: accepted")
	}

	if nullRowOK && unwindOK {
		q, err := conn.Prepare(
			"CALL QUERY_VECTOR_INDEX('E', 'e_vec', $q, 5) RETURN node.name AS n, distance ORDER BY distance")
		if err != nil {
			t.Fatalf("prepare vector query: %v", err)
		}
		defer q.Close()
		res, err := conn.Execute(q, map[string]any{"q": vec(0)})
		if err != nil {
			t.Errorf("QUERY_VECTOR_INDEX over a table containing NULL-vector rows failed: %v — "+
				"vectors would need a separate table", err)
		} else {
			var names []string
			for res.HasNext() {
				tup, e := res.Next()
				if e != nil {
					break
				}
				if v, e2 := tup.GetValue(0); e2 == nil {
					names = append(names, fmt.Sprint(v))
				}
			}
			res.Close()
			t.Logf("(2b) query with NULL-vector rows present returned: %v", names)
			if len(names) == 0 {
				t.Error("query returned nothing although vectors were inserted")
			}
			for _, n := range names {
				if n == "noVector" {
					t.Error("a row with NULL emb was returned as a neighbour — it would pollute results")
				}
			}
		}
	}

	t.Logf("SUMMARY: empty-table index=%v, null-vector rows=%v, UNWIND vectors=%v",
		emptyIndexOK, nullRowOK, unwindOK)
}
