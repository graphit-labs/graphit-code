package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// TestShardParquetSizeByGranularity measures whether Parquet is smaller than JSON at the
// granularity a shard cache actually uses.
//
// The cache is one document per SOURCE FILE, rewritten whole whenever that file changes.
// Parquet's compression works on row groups and it carries a fixed cost per file — magic
// bytes, schema, row group metadata, footer — so "Parquet is smaller" is a claim about
// large batches and has to be re-checked at tens of rows before a per-file cache is
// rewritten in it.
//
// Parquet is produced by the engine itself (COPY TO), which is also how this avoids adding
// a Parquet writer to the module just to measure.
//
// GRAPHIT_SHARD_SIZE=1 go test -run TestShardParquetSizeByGranularity ./internal/ast/ -v
func TestShardParquetSizeByGranularity(t *testing.T) {
	if os.Getenv("GRAPHIT_SHARD_SIZE") == "" {
		t.Skip("set GRAPHIT_SHARD_SIZE=1")
	}
	dir := t.TempDir()
	st, err := openProbeStoreWithJSON(t, filepath.Join(dir, "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer func() { _ = st.Close() }()

	if err := st.Exec(`CREATE NODE TABLE Ent(id INT64, label STRING, uid STRING,
		name STRING, path STRING, line INT64, end_line INT64, docstring STRING,
		PRIMARY KEY(id))`, nil); err != nil {
		t.Fatalf("ddl: %v", err)
	}

	for _, n := range []int{12, 60, 400, 5000} {
		if err := st.Exec("MATCH (e:Ent) DELETE e", nil); err != nil {
			t.Fatalf("clear: %v", err)
		}
		rows := make([]map[string]any, 0, n)
		for i := 0; i < n; i++ {
			rows = append(rows, map[string]any{
				"id": int64(i), "label": "Function",
				"uid":  fmt.Sprintf("pkg/mod/handler%d.go:handleRequest%d", i%40, i),
				"name": fmt.Sprintf("handleRequest%dValidator", i),
				"path": fmt.Sprintf("src/module%d/handler%d.go", i%40, i),
				"line": int64(i * 7), "end_line": int64(i*7 + 20),
				"docstring": "Validates the incoming request payload for this handler " +
					"and returns a normalised structure the caller can persist.",
			})
		}
		jsonPath := filepath.Join(dir, fmt.Sprintf("shard%d.json", n))
		if err := writeJSONFile(jsonPath, rows); err != nil {
			t.Fatalf("write json: %v", err)
		}
		jf, _ := os.Stat(jsonPath)

		batch := make([]any, len(rows))
		for i, r := range rows {
			batch[i] = r
		}
		if err := st.Exec(`UNWIND $b AS r CREATE (:Ent {id: r.id, label: r.label,
			uid: r.uid, name: r.name, path: r.path, line: r.line,
			end_line: r.end_line, docstring: r.docstring})`,
			map[string]any{"b": batch}); err != nil {
			t.Fatalf("seed: %v", err)
		}
		pqPath := filepath.Join(dir, fmt.Sprintf("shard%d.parquet", n))
		_ = os.Remove(pqPath)
		if err := st.Exec(fmt.Sprintf(`COPY (MATCH (e:Ent) RETURN e.id, e.label, e.uid,
			e.name, e.path, e.line, e.end_line, e.docstring) TO '%s'`, pqPath), nil); err != nil {
			t.Fatalf("copy to parquet: %v", err)
		}
		pf, _ := os.Stat(pqPath)

		ratio := float64(jf.Size()) / float64(pf.Size())
		verdict := "Parquet MENOR"
		if pf.Size() >= jf.Size() {
			verdict = "Parquet MAIOR"
		}
		t.Logf("%5d entidades: json %8d B, parquet %8d B  -> %s (%.2fx)",
			n, jf.Size(), pf.Size(), verdict, ratio)
	}

	t.Log("um shard guarda: entities, params, fields (nodes) + calls, imports, " +
		"inheritance, field_access, references, contains (edges) = 9 formas distintas")
}
