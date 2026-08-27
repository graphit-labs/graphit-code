package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// TestVectorIndexCacheEmbeddings measures the knob that decides whether building the vector
// index after the load is viable at all.
//
// Building it after the rows are in is 3.1x cheaper than maintaining it during the load, but
// on a 2.5 M entity corpus it exhausted an 8 GiB buffer pool — and production clamps the
// pool at 1 GiB. cache_embeddings := false is documented to trade construction time for
// memory; this asks what that trade actually costs here, and whether the resulting index
// still answers.
//
// GRAPHIT_VEC_MEM=1 go test -run TestVectorIndexCacheEmbeddings ./internal/ast/ -v -timeout 60m
func TestVectorIndexCacheEmbeddings(t *testing.T) {
	if os.Getenv("GRAPHIT_VEC_MEM") == "" {
		t.Skip("set GRAPHIT_VEC_MEM=1")
	}
	const n = 80_000

	// indexFirst is the cell the earlier probe never covered: it measured "index before
	// load" against UNWIND, and COPY is a different write path. If the engine maintains
	// the index cheaply during a COPY, neither the memory spike of a batch build nor the
	// 5x of cache_embeddings := false has to be paid.
	t.Run("index before COPY", func(t *testing.T) {
		st, rows, dir := probeVecStore(t, n)
		defer func() { _ = st.Close() }()
		if err := st.Exec("CALL CREATE_VECTOR_INDEX('E','e_vec','emb')", nil); err != nil {
			t.Fatalf("create index on empty table: %v", err)
		}
		stage := filepath.Join(dir, "e.json")
		if err := writeJSONFile(stage, rows); err != nil {
			t.Fatalf("stage: %v", err)
		}
		start := time.Now()
		err := st.Exec(fmt.Sprintf("COPY E FROM '%s'", stage), nil)
		elapsed := time.Since(start)
		_ = os.Remove(stage)
		if err != nil {
			t.Fatalf("copy into indexed table: %v", err)
		}
		probeVecReport(t, st, rows, "index before COPY", elapsed)
	})

	for _, cached := range []bool{true, false} {
		t.Run(fmt.Sprintf("cache_embeddings=%v", cached), func(t *testing.T) {
			st, err := ladybugstore.Open(filepath.Join(t.TempDir(), "db"))
			if err != nil {
				t.Fatalf("open: %v", err)
			}
			defer func() { _ = st.Close() }()
			_ = st.Exec("INSTALL vector", nil)
			if err := st.LoadExtensions("vector"); err != nil {
				t.Fatalf("load vector: %v", err)
			}
			_ = st.Exec("INSTALL json", nil)
			if err := st.LoadExtensions("json"); err != nil {
				t.Fatalf("load json: %v", err)
			}
			if err := st.Exec(fmt.Sprintf(`CREATE NODE TABLE E(id INT64, emb FLOAT[%d],
				PRIMARY KEY(id))`, ai.EmbeddingDimensions), nil); err != nil {
				t.Fatalf("ddl: %v", err)
			}

			// Load with COPY, the path the rebuild now uses.
			rnd := uint64(0x9E3779B97F4A7C15)
			rows := make([]map[string]any, 0, n)
			for i := 0; i < n; i++ {
				v := make([]float32, ai.EmbeddingDimensions)
				for j := range v {
					rnd ^= rnd << 13
					rnd ^= rnd >> 7
					rnd ^= rnd << 17
					v[j] = float32(rnd%2000)/1000.0 - 1.0
				}
				rows = append(rows, map[string]any{"id": int64(i), "emb": v})
			}
			dir := t.TempDir()
			stage := filepath.Join(dir, "e.json")
			if err := writeJSONFile(stage, rows); err != nil {
				t.Fatalf("stage: %v", err)
			}
			if err := st.Exec(fmt.Sprintf("COPY E FROM '%s'", stage), nil); err != nil {
				t.Fatalf("copy: %v", err)
			}
			_ = os.Remove(stage)

			start := time.Now()
			q := "CALL CREATE_VECTOR_INDEX('E','e_vec','emb')"
			if !cached {
				q = "CALL CREATE_VECTOR_INDEX('E','e_vec','emb', cache_embeddings := false)"
			}
			err = st.Exec(q, nil)
			elapsed := time.Since(start)
			if err != nil {
				t.Fatalf("create index (cached=%v): %v", cached, err)
			}

			// The index has to answer, not merely build.
			probe := rows[42]["emb"].([]float32)
			hits, qerr := st.Query(`CALL QUERY_VECTOR_INDEX('E','e_vec',$q,3)
				RETURN node.id AS id ORDER BY distance`, map[string]any{"q": probe})
			top := int64(-1)
			if len(hits) > 0 {
				top = ladybugstore.Int64(hits[0]["id"])
			}
			t.Logf("cache_embeddings=%-5v build %7.2fs  query hits=%d top=%d (want 42) err=%v",
				cached, elapsed.Seconds(), len(hits), top, qerr)
		})
	}
}

func probeVecStore(t *testing.T, n int) (*ladybugstore.Store, []map[string]any, string) {
	t.Helper()
	dir := t.TempDir()
	st, err := ladybugstore.Open(filepath.Join(dir, "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	_ = st.Exec("INSTALL vector", nil)
	if err := st.LoadExtensions("vector"); err != nil {
		t.Fatalf("load vector: %v", err)
	}
	_ = st.Exec("INSTALL json", nil)
	if err := st.LoadExtensions("json"); err != nil {
		t.Fatalf("load json: %v", err)
	}
	if err := st.Exec(fmt.Sprintf(`CREATE NODE TABLE E(id INT64, emb FLOAT[%d],
		PRIMARY KEY(id))`, ai.EmbeddingDimensions), nil); err != nil {
		t.Fatalf("ddl: %v", err)
	}
	rnd := uint64(0x9E3779B97F4A7C15)
	rows := make([]map[string]any, 0, n)
	for i := 0; i < n; i++ {
		v := make([]float32, ai.EmbeddingDimensions)
		for j := range v {
			rnd ^= rnd << 13
			rnd ^= rnd >> 7
			rnd ^= rnd << 17
			v[j] = float32(rnd%2000)/1000.0 - 1.0
		}
		rows = append(rows, map[string]any{"id": int64(i), "emb": v})
	}
	return st, rows, dir
}

func probeVecReport(t *testing.T, st *ladybugstore.Store, rows []map[string]any,
	label string, elapsed time.Duration) {
	t.Helper()
	probe := rows[42]["emb"].([]float32)
	hits, qerr := st.Query(`CALL QUERY_VECTOR_INDEX('E','e_vec',$q,3)
		RETURN node.id AS id ORDER BY distance`, map[string]any{"q": probe})
	top := int64(-1)
	if len(hits) > 0 {
		top = ladybugstore.Int64(hits[0]["id"])
	}
	t.Logf("%-22s %7.2fs  query hits=%d top=%d (want 42) err=%v",
		label, elapsed.Seconds(), len(hits), top, qerr)
}
