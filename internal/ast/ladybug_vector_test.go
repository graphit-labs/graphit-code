package ast

import (
	"fmt"
	"math"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugVectorIndex probes LadybugDB's native vector index, which is the
// half of the consolidation question the FTS measurements never touched.
//
// Why it matters more than FTS parity: the semantic path carries real weight
// (TestSemanticReachOfAbbreviations — CFG_LOAD, reachable by no lexical method,
// ranks first), and today that path runs on sqlite-vec's vec0, which allocates
// fixed 1024-row chunks and NEVER reclaims space on delete — only flipping a
// validity bit. That single property is why RebuildFromCache cannot update in
// place and must write a whole new file and rename it (see the comment in
// fts_sqlite.go). If Ladybug's index updates on insert/delete, that forced
// full-file rebuild disappears.
//
// Probed here, in order of what would kill the idea soonest:
//  1. the extension loads at all;
//  2. a 768-float column (ai.EmbeddingDimensions) can be created and indexed;
//  3. vectors bind from Go as parameters, not just as literals;
//  4. nearest-neighbour queries rank correctly;
//  5. DELETE + INSERT is reflected without rebuilding the index — the claim that
//     decides whether the vec0 workaround can be dropped.
func TestLadybugVectorIndex(t *testing.T) {
	const dim = 768

	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "vec"), lbug.DefaultSystemConfig())
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

	// (1) Extension availability.
	_ = run("INSTALL vector")
	if err := run("LOAD EXTENSION vector"); err != nil {
		t.Skipf("vector extension cannot load — native semantic search is not available in this build: %v", err)
	}
	t.Log("vector extension loaded")

	// (2) A column of the production embedding width.
	if err := run(fmt.Sprintf("CREATE NODE TABLE V(uid STRING, name STRING, emb FLOAT[%d], PRIMARY KEY(uid))", dim)); err != nil {
		t.Fatalf("FLOAT[%d] column rejected — the production embedding width is unusable: %v", dim, err)
	}

	// Deterministic, well-separated unit vectors: each entity's mass sits in its
	// own band of the 768 dimensions, so nearest-neighbour order is predictable
	// without depending on a model.
	vecFor := func(band int) []float32 {
		v := make([]float32, dim)
		start := band * 64
		for i := start; i < start+64 && i < dim; i++ {
			v[i] = 1
		}
		// Normalise so cosine and L2 order agree.
		norm := float32(math.Sqrt(64))
		for i := range v {
			v[i] /= norm
		}
		return v
	}

	entities := []struct {
		uid, name string
		band      int
	}{
		{"e1", "parseConfig", 0},
		{"e2", "loadUserConfig", 1},
		{"e3", "computeChecksum", 2},
		{"e4", "retryPolicy", 3},
	}

	// (3) Parameter binding of a Go []float32 — production cannot build 768-element
	// literals per entity.
	insert, err := conn.Prepare("CREATE (:V {uid: $uid, name: $name, emb: $emb})")
	if err != nil {
		t.Fatalf("prepare vector insert: %v", err)
	}
	defer insert.Close()

	bound := true
	for _, e := range entities {
		res, err := conn.Execute(insert, map[string]any{
			"uid": e.uid, "name": e.name, "emb": vecFor(e.band),
		})
		if err != nil {
			t.Logf("binding []float32 as a parameter failed for %s: %v", e.uid, err)
			bound = false
			break
		}
		res.Close()
	}

	if !bound {
		// Fall back to literals so the remaining probes still produce an answer;
		// record that production would need this workaround.
		if err := run("MATCH (v:V) DELETE v"); err != nil {
			t.Logf("cleanup before literal fallback: %v", err)
		}
		for _, e := range entities {
			lit := make([]string, dim)
			for i, f := range vecFor(e.band) {
				lit[i] = fmt.Sprintf("%g", f)
			}
			q := fmt.Sprintf("CREATE (:V {uid: '%s', name: '%s', emb: [%s]})",
				e.uid, e.name, strings.Join(lit, ","))
			if err := run(q); err != nil {
				t.Fatalf("literal vector insert also failed for %s: %v", e.uid, err)
			}
		}
		t.Log("NOTE: vectors had to be inserted as literals — parameter binding of []float32 is unavailable")
	} else {
		t.Log("[]float32 binds as a query parameter")
	}

	if err := run("CALL CREATE_VECTOR_INDEX('V', 'v_idx', 'emb')"); err != nil {
		t.Fatalf("CREATE_VECTOR_INDEX rejected — no native vector index: %v", err)
	}
	t.Log("vector index created")

	// (4) Nearest-neighbour ranking. Query near band 1, expect loadUserConfig first.
	queryNames := func(band int, k int) []string {
		q, err := conn.Prepare(fmt.Sprintf(
			"CALL QUERY_VECTOR_INDEX('V', 'v_idx', $q, %d) RETURN node.name AS n, distance ORDER BY distance", k))
		if err != nil {
			t.Logf("prepare vector query: %v", err)
			return nil
		}
		defer q.Close()
		res, err := conn.Execute(q, map[string]any{"q": vecFor(band)})
		if err != nil {
			t.Logf("QUERY_VECTOR_INDEX failed: %v", err)
			return nil
		}
		defer res.Close()
		var out []string
		for res.HasNext() {
			tup, e := res.Next()
			if e != nil {
				break
			}
			if v, e2 := tup.GetValue(0); e2 == nil {
				out = append(out, fmt.Sprint(v))
			}
		}
		return out
	}

	got := queryNames(1, 3)
	t.Logf("nearest to band 1 (loadUserConfig): %v", got)
	if len(got) == 0 {
		t.Fatal("QUERY_VECTOR_INDEX returned nothing — native semantic search is not usable")
	}
	if got[0] != "loadUserConfig" {
		t.Errorf("nearest neighbour is %q, want loadUserConfig — ranking is wrong (%v)", got[0], got)
	}

	// (5) The decisive property: does a delete followed by an insert show up
	// WITHOUT rebuilding the index? This is what sqlite-vec cannot do.
	if err := run("MATCH (v:V {uid: 'e2'}) DELETE v"); err != nil {
		t.Fatalf("delete indexed vector: %v", err)
	}
	afterDelete := queryNames(1, 3)
	t.Logf("after deleting loadUserConfig, nearest to band 1: %v", afterDelete)
	for _, n := range afterDelete {
		if n == "loadUserConfig" {
			t.Error("deleted vector still returned by the index — delete is not reflected without a rebuild")
		}
	}

	insert2, err := conn.Prepare("CREATE (:V {uid: $uid, name: $name, emb: $emb})")
	if err != nil {
		t.Fatalf("prepare reinsert: %v", err)
	}
	defer insert2.Close()
	res, err := conn.Execute(insert2, map[string]any{
		"uid": "e5", "name": "reloadUserConfig", "emb": vecFor(1),
	})
	if err != nil {
		t.Fatalf("reinsert into an existing vector index: %v", err)
	}
	res.Close()

	afterInsert := queryNames(1, 3)
	t.Logf("after inserting reloadUserConfig, nearest to band 1: %v", afterInsert)
	if len(afterInsert) == 0 || afterInsert[0] != "reloadUserConfig" {
		t.Errorf("newly inserted vector is not the nearest neighbour (got %v) — inserts are not reflected "+
			"without rebuilding, so the sqlite-vec full-file rebuild workaround could not be dropped",
			afterInsert)
	}

}
