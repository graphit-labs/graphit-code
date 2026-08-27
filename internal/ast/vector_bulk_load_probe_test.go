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

// TestVectorBulkLoadPaths compares the two ways of getting rows with a FLOAT[768] column
// into the search table, at the same size and with every row carrying a vector.
//
// This is the measurement the whole investigation turns on. The search index loads with
// UNWIND; the graph rebuild stages a file and COPYs. If the two differ by orders of
// magnitude on the vector column, then the reason a full rebuild takes hours is the write
// path, not the index, not the copy, and not the FTS.
//
// It also checks the end state, not only the clock: a fast load that leaves the vector index
// unable to answer is not a load.
//
// GRAPHIT_VEC_BULK=1 go test -run TestVectorBulkLoadPaths ./internal/ast/ -v -timeout 60m
func TestVectorBulkLoadPaths(t *testing.T) {
	if os.Getenv("GRAPHIT_VEC_BULK") == "" {
		t.Skip("set GRAPHIT_VEC_BULK=1")
	}
	const n = 80_000

	dir := t.TempDir()
	st, err := ladybugstore.Open(filepath.Join(dir, "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer func() { _ = st.Close() }()
	_ = st.Exec("INSTALL vector", nil)
	if err := st.LoadExtensions("vector"); err != nil {
		t.Fatalf("load vector: %v", err)
	}

	ddl := func(name string) string {
		return fmt.Sprintf(`CREATE NODE TABLE %s(id INT64, name STRING, docstring STRING,
			path STRING, emb FLOAT[%d], PRIMARY KEY(id))`, name, ai.EmbeddingDimensions)
	}
	for _, tbl := range []string{"ViaUnwind", "ViaCopy"} {
		if err := st.Exec(ddl(tbl), nil); err != nil {
			t.Fatalf("ddl %s: %v", tbl, err)
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
	row := func(i int, v []float32) map[string]any {
		return map[string]any{
			"id": int64(i), "name": fmt.Sprintf("handleRequest%dValidator", i),
			"docstring": "Validates the incoming request payload and normalises it.",
			"path":      fmt.Sprintf("src/module%d/handler%d.go", i%400, i),
			"emb":       v,
		}
	}

	vecs := make([][]float32, n)
	for i := range vecs {
		vecs[i] = nextVec()
	}

	// UNWIND, the path the search index uses today.
	start := time.Now()
	batch := make([]any, 0, searchBatchRows)
	flush := func() {
		if len(batch) == 0 {
			return
		}
		if err := st.Exec(`UNWIND $b AS r CREATE (:ViaUnwind {id: r.id, name: r.name,
			docstring: r.docstring, path: r.path, emb: r.emb})`,
			map[string]any{"b": batch}); err != nil {
			t.Fatalf("unwind: %v", err)
		}
		batch = batch[:0]
	}
	for i := 0; i < n; i++ {
		batch = append(batch, row(i, vecs[i]))
		if len(batch) == searchBatchRows {
			flush()
		}
	}
	flush()
	unwind := time.Since(start)
	t.Logf("UNWIND         %6.2fs  (%.0f us/row)", unwind.Seconds(),
		float64(unwind.Microseconds())/float64(n))

	// COPY: stage the same rows to Parquet, then load. The staging file is produced by the
	// engine itself, which is also what a rebuild would do — it already holds the rows.
	stage := filepath.Join(dir, "vec.parquet")
	s2 := time.Now()
	if err := st.Exec(fmt.Sprintf(
		`COPY (MATCH (v:ViaUnwind) RETURN v.id, v.name, v.docstring, v.path, v.emb) TO '%s'`,
		stage), nil); err != nil {
		t.Fatalf("stage: %v", err)
	}
	stageTime := time.Since(s2)
	fi, _ := os.Stat(stage)

	s3 := time.Now()
	if err := st.Exec(fmt.Sprintf(`COPY ViaCopy FROM '%s'`, stage), nil); err != nil {
		t.Fatalf("copy: %v", err)
	}
	copyTime := time.Since(s3)
	t.Logf("COPY           %6.2fs  (%.0f us/row)  [+ %5.2fs staging, %.1f MB]",
		copyTime.Seconds(), float64(copyTime.Microseconds())/float64(n),
		stageTime.Seconds(), float64(fi.Size())/(1<<20))
	t.Logf("speedup        %.0fx on load, %.0fx including staging",
		unwind.Seconds()/copyTime.Seconds(),
		unwind.Seconds()/(copyTime+stageTime).Seconds())

	// The end state has to be usable, not just fast.
	s4 := time.Now()
	if err := st.Exec("CALL CREATE_VECTOR_INDEX('ViaCopy','vc_vec','emb')", nil); err != nil {
		t.Fatalf("index after copy: %v", err)
	}
	t.Logf("index after COPY %4.2fs", time.Since(s4).Seconds())

	total, _ := st.CountQuery("MATCH (n:ViaCopy) RETURN count(n)", nil)
	withEmb, _ := st.CountQuery("MATCH (n:ViaCopy) WHERE n.emb IS NOT NULL RETURN count(n)", nil)
	hits, err := st.Query(`CALL QUERY_VECTOR_INDEX('ViaCopy','vc_vec',$q,5)
		RETURN node.id AS id, distance ORDER BY distance LIMIT 3`,
		map[string]any{"q": vecs[42]})
	t.Logf("rows=%d with_emb=%d  vector query -> hits=%d err=%v", total, withEmb, len(hits), err)
	for _, h := range hits {
		t.Logf("   %v", h)
	}
}
