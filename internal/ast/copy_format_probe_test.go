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

const searchBatchRows = 500

// TestCopyFormatCapability asks what this engine build actually accepts on the bulk-load
// path, before anything is rewritten to suit it: whether COPY FROM reads Parquet and Arrow
// as the docs claim, whether a glob over a directory works, and whether COPY TO can produce
// the file (which is also how this probe makes one without adding a Parquet writer to the
// module).
//
// It is a capability check, not a benchmark — timings here are one-shot and on a machine
// that may be busy. GRAPHIT_COPY_FORMATS=1 go test -run TestCopyFormatCapability ./internal/ast/ -v
func TestCopyFormatCapability(t *testing.T) {
	if os.Getenv("GRAPHIT_COPY_FORMATS") == "" {
		t.Skip("set GRAPHIT_COPY_FORMATS=1")
	}
	dir := t.TempDir()
	st, err := ladybugstore.Open(filepath.Join(dir, "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer func() { _ = st.Close() }()

	_ = st.Exec("INSTALL json", nil)
	if err := st.LoadExtensions("json"); err != nil {
		t.Fatalf("load json: %v", err)
	}

	if err := st.Exec(`CREATE NODE TABLE Ent(id INT64, name STRING, path STRING,
		doc STRING, PRIMARY KEY(id))`, nil); err != nil {
		t.Fatalf("ddl: %v", err)
	}

	const n = 200_000
	rows := make([]any, 0, 2000)
	for i := 0; i < n; i++ {
		rows = append(rows, map[string]any{
			"id": int64(i), "name": fmt.Sprintf("handleRequest%dValidator", i),
			"path": fmt.Sprintf("src/module%d/handler%d.go", i%400, i),
			"doc":  fmt.Sprintf("Validates the incoming request payload for handler %d.", i),
		})
		if len(rows) == 2000 {
			if err := st.Exec(`UNWIND $b AS r CREATE (:Ent {id: r.id, name: r.name,
				path: r.path, doc: r.doc})`, map[string]any{"b": rows}); err != nil {
				t.Fatalf("seed: %v", err)
			}
			rows = rows[:0]
		}
	}

	for _, ext := range []string{"parquet", "csv", "json", "arrow"} {
		out := filepath.Join(dir, "ent."+ext)
		start := time.Now()
		err := st.Exec(fmt.Sprintf(`COPY (MATCH (e:Ent) RETURN e.id, e.name, e.path, e.doc)
			TO '%s'`, out), nil)
		size := int64(-1)
		if fi, serr := os.Stat(out); serr == nil {
			size = fi.Size()
		}
		t.Logf("COPY TO   %-8s -> %6.2fs  %8.1f MB  err=%v",
			ext, time.Since(start).Seconds(), float64(size)/(1<<20), err)
	}

	jsonPath := filepath.Join(dir, "ent.json")
	{
		data := make([]map[string]any, 0, n)
		for i := 0; i < n; i++ {
			data = append(data, map[string]any{
				"id": int64(i), "name": fmt.Sprintf("handleRequest%dValidator", i),
				"path": fmt.Sprintf("src/module%d/handler%d.go", i%400, i),
				"doc":  fmt.Sprintf("Validates the incoming request payload for handler %d.", i),
			})
		}
		start := time.Now()
		if err := writeJSONFile(jsonPath, data); err != nil {
			t.Fatalf("write json: %v", err)
		}
		fi, _ := os.Stat(jsonPath)
		t.Logf("WRITE     %-8s -> %6.2fs  %8.1f MB (this is serialize_s in production)",
			"json", time.Since(start).Seconds(), float64(fi.Size())/(1<<20))
	}

	for _, ext := range []string{"parquet", "csv", "json", "arrow"} {
		in := filepath.Join(dir, "ent."+ext)
		if _, serr := os.Stat(in); serr != nil {
			t.Logf("COPY FROM %-8s -> skipped, no file produced", ext)
			continue
		}
		tbl := "Load_" + ext
		if err := st.Exec(fmt.Sprintf(`CREATE NODE TABLE %s(id INT64, name STRING,
			path STRING, doc STRING, PRIMARY KEY(id))`, tbl), nil); err != nil {
			t.Logf("COPY FROM %-8s -> ddl failed: %v", ext, err)
			continue
		}
		start := time.Now()
		err := st.Exec(fmt.Sprintf(`COPY %s FROM '%s'`, tbl, in), nil)
		count, _ := st.CountQuery(fmt.Sprintf("MATCH (n:%s) RETURN count(n)", tbl), nil)
		t.Logf("COPY FROM %-8s -> %6.2fs  rows=%d  err=%v",
			ext, time.Since(start).Seconds(), count, err)
	}

	if err := st.Exec(`CREATE NODE TABLE Load_unwind(id INT64, name STRING, path STRING,
		doc STRING, PRIMARY KEY(id))`, nil); err == nil {
		start := time.Now()
		batch := make([]any, 0, searchBatchRows)
		for i := 0; i < n; i++ {
			batch = append(batch, map[string]any{
				"id": int64(i), "name": fmt.Sprintf("handleRequest%dValidator", i),
				"path": fmt.Sprintf("src/module%d/handler%d.go", i%400, i),
				"doc":  fmt.Sprintf("Validates the incoming request payload for handler %d.", i),
			})
			if len(batch) == searchBatchRows {
				if err := st.Exec(`UNWIND $b AS r CREATE (:Load_unwind {id: r.id,
					name: r.name, path: r.path, doc: r.doc})`,
					map[string]any{"b": batch}); err != nil {
					t.Fatalf("unwind: %v", err)
				}
				batch = batch[:0]
			}
		}
		count, _ := st.CountQuery("MATCH (n:Load_unwind) RETURN count(n)", nil)
		t.Logf("UNWIND    %-8s -> %6.2fs  rows=%d (batch=%d)",
			"batches", time.Since(start).Seconds(), count, searchBatchRows)
	}

	probeCopyVectorColumn(t, st, dir)

	if err := st.Exec(`CREATE NODE TABLE Load_glob(id INT64, name STRING, path STRING,
		doc STRING, PRIMARY KEY(id))`, nil); err == nil {
		err := st.Exec(fmt.Sprintf(`COPY Load_glob FROM '%s'`,
			filepath.Join(dir, "*.parquet")), nil)
		count, _ := st.CountQuery("MATCH (n:Load_glob) RETURN count(n)", nil)
		t.Logf("COPY FROM glob '*.parquet' -> rows=%d err=%v", count, err)
	}
}

func probeCopyVectorColumn(t *testing.T, st *ladybugstore.Store, dir string) {
	t.Helper()
	_ = st.Exec("INSTALL vector", nil)
	if err := st.LoadExtensions("vector"); err != nil {
		t.Logf("COPY vector -> load vector failed: %v", err)
		return
	}
	ddl := fmt.Sprintf(`CREATE NODE TABLE Vec%%s(id INT64, name STRING,
		emb FLOAT[%d], PRIMARY KEY(id))`, ai.EmbeddingDimensions)
	if err := st.Exec(fmt.Sprintf(ddl, "Src"), nil); err != nil {
		t.Logf("COPY vector -> ddl failed: %v", err)
		return
	}

	vec := make([]float32, ai.EmbeddingDimensions)
	for i := range vec {
		vec[i] = 0.01
	}
	for i := 0; i < 20_000; i++ {
		row := map[string]any{"id": int64(i), "name": fmt.Sprintf("e%d", i)}
		q := `CREATE (:VecSrc {id: $id, name: $name})`
		if i%2 == 0 {
			row["emb"] = vec
			q = `CREATE (:VecSrc {id: $id, name: $name, emb: $emb})`
		}
		if err := st.Exec(q, row); err != nil {
			t.Logf("COPY vector -> seed failed at %d: %v", i, err)
			return
		}
	}

	out := filepath.Join(dir, "vec.parquet")
	if err := st.Exec(fmt.Sprintf(
		`COPY (MATCH (v:VecSrc) RETURN v.id, v.name, v.emb) TO '%s'`, out), nil); err != nil {
		t.Logf("COPY TO   vector parquet -> %v", err)
		return
	}
	fi, _ := os.Stat(out)
	t.Logf("COPY TO   vector parquet -> ok, %.1f MB", float64(fi.Size())/(1<<20))

	if err := st.Exec(fmt.Sprintf(ddl, "Dst"), nil); err != nil {
		t.Logf("COPY vector -> dst ddl failed: %v", err)
		return
	}
	start := time.Now()
	err := st.Exec(fmt.Sprintf(`COPY VecDst FROM '%s'`, out), nil)
	total, _ := st.CountQuery("MATCH (n:VecDst) RETURN count(n)", nil)
	withEmb, _ := st.CountQuery("MATCH (n:VecDst) WHERE n.emb IS NOT NULL RETURN count(n)", nil)
	t.Logf("COPY FROM vector parquet -> %.2fs rows=%d with_emb=%d err=%v",
		time.Since(start).Seconds(), total, withEmb, err)
}
