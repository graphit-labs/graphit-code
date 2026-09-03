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

// TestVectorIndexBuildOrder measures what it costs to bulk-load into a table that already
// carries a vector index, against loading first and indexing after.
//
// EnsureSchema creates se_vec before the load, on the reasoning that the index is accepted
// on an empty table so there is no ordering constraint. That is true of correctness. This
// asks whether it is true of cost — every insert then has to maintain the index, and the
// FTS indexes are already deliberately created after the load for a different reason.
//
// GRAPHIT_VEC_ORDER=1 go test -run TestVectorIndexBuildOrder ./internal/ast/ -v -timeout 60m
func TestVectorIndexBuildOrder(t *testing.T) {
	if os.Getenv("GRAPHIT_VEC_ORDER") == "" {
		t.Skip("set GRAPHIT_VEC_ORDER=1")
	}

	cases := []struct {
		n          int
		indexFirst bool
	}{
		{20_000, false},
		{80_000, false},
		{2_000, true},
		{20_000, true},
	}
	for _, c := range cases {
		n, indexFirst := c.n, c.indexFirst
		{
			label := "index after load"
			if indexFirst {
				label = "index before load"
			}
			t.Run(fmt.Sprintf("n=%d/%s", n, label), func(t *testing.T) {
				st, err := ladybugstore.Open(filepath.Join(t.TempDir(), "db"))
				if err != nil {
					t.Fatalf("open: %v", err)
				}
				defer func() { _ = st.Close() }()

				_ = st.Exec("INSTALL vector", nil)
				if err := st.LoadExtensions("vector"); err != nil {
					t.Fatalf("load vector: %v", err)
				}
				if err := st.Exec(fmt.Sprintf(`CREATE NODE TABLE SearchEntity(id INT64,
					name STRING, docstring STRING, path STRING,
					emb FLOAT[%d], PRIMARY KEY(id))`, ai.EmbeddingDimensions), nil); err != nil {
					t.Fatalf("ddl: %v", err)
				}

				if indexFirst {
					if err := st.Exec(
						"CALL CREATE_VECTOR_INDEX('SearchEntity','se_vec','emb')", nil); err != nil {
						t.Fatalf("create vector index: %v", err)
					}
				}

				rnd := uint64(0x9E3779B97F4A7C15)
				nextVec := func() []float32 {
					v := make([]float32, ai.EmbeddingDimensions)
					for j := range v {
						rnd ^= rnd << 13
						rnd ^= rnd >> 7
						rnd ^= rnd << 17
						v[j] = float32(rnd%2000)/1000.0 - 1.0
					}
					return v
				}

				start := time.Now()
				batch := make([]any, 0, searchBatchRows)
				flush := func() {
					if len(batch) == 0 {
						return
					}
					if err := st.Exec(`UNWIND $b AS r CREATE (:SearchEntity {id: r.id,
						name: r.name, docstring: r.docstring, path: r.path, emb: r.emb})`,
						map[string]any{"b": batch}); err != nil {
						t.Fatalf("insert: %v", err)
					}
					batch = batch[:0]
				}
				for i := 0; i < n; i++ {
					batch = append(batch, map[string]any{
						"id": int64(i), "name": fmt.Sprintf("handleRequest%dValidator", i),
						"docstring": "Validates the incoming request payload and normalises it.",
						"path":      fmt.Sprintf("src/module%d/handler%d.go", i%400, i),
						"emb":       nextVec(),
					})
					if len(batch) == searchBatchRows {
						flush()
					}
				}
				flush()
				insert := time.Since(start)

				var index time.Duration
				if !indexFirst {
					s2 := time.Now()
					if err := st.Exec(
						"CALL CREATE_VECTOR_INDEX('SearchEntity','se_vec','emb')", nil); err != nil {
						t.Fatalf("create vector index after load: %v", err)
					}
					index = time.Since(s2)
				}

				t.Logf("n=%-7d %-18s insert %6.2fs + index %5.2fs = %6.2fs  (%.0f us/row)",
					n, label, insert.Seconds(), index.Seconds(),
					(insert + index).Seconds(),
					float64((insert+index).Microseconds())/float64(n))
			})
		}
	}
}
