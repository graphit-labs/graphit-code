package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// TestCopyWholeTableExport asks whether a table can be exported without naming its columns.
//
// It decides how a Hub artifact would be produced. COPY FROM maps Parquet columns by
// POSITION, so an export that lists columns by hand is a convention both sides must keep;
// an export of the whole table is a contract the engine keeps for you. The difference
// matters because the alternative failure is silent — a reordered file only errors when the
// types happen to clash.
//
// GRAPHIT_COPY_ROWSIZE=1 go test -run TestCopyWholeTableExport ./internal/ast/ -v
func TestCopyWholeTableExport(t *testing.T) {
	if os.Getenv("GRAPHIT_COPY_ROWSIZE") == "" {
		t.Skip("set GRAPHIT_COPY_ROWSIZE=1")
	}
	dir := t.TempDir()
	st, err := openProbeStoreWithJSON(t, filepath.Join(dir, "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer func() { _ = st.Close() }()
	_ = st.Exec("INSTALL vector", nil)
	if err := st.LoadExtensions("vector"); err != nil {
		t.Fatalf("load vector: %v", err)
	}

	// A shape like the real thing: mixed types, plus the embedding column and rows that
	// have no vector.
	ddl := fmt.Sprintf(`CREATE NODE TABLE %%s(id INT64, name STRING, path STRING,
		line INT64, is_exported BOOLEAN, emb FLOAT[%d], PRIMARY KEY(id))`,
		ai.EmbeddingDimensions)
	if err := st.Exec(fmt.Sprintf(ddl, "Src"), nil); err != nil {
		t.Fatalf("ddl: %v", err)
	}
	vec := make([]float32, ai.EmbeddingDimensions)
	for i := range vec {
		vec[i] = float32(i%97) / 97.0
	}
	if err := st.Exec(`CREATE (:Src {id: 1, name: 'Alpha', path: 'a.go', line: 7,
		is_exported: true, emb: $e})`, map[string]any{"e": vec}); err != nil {
		t.Fatalf("seed 1: %v", err)
	}
	if err := st.Exec(`CREATE (:Src {id: 2, name: 'Beta', path: 'b.go', line: 9,
		is_exported: false})`, nil); err != nil {
		t.Fatalf("seed 2: %v", err)
	}

	forms := []struct{ label, query string }{
		{"COPY Tbl TO", "COPY Src TO '%s'"},
		{"RETURN n.*", "COPY (MATCH (n:Src) RETURN n.*) TO '%s'"},
		{"RETURN n", "COPY (MATCH (n:Src) RETURN n) TO '%s'"},
		{"explicit columns", "COPY (MATCH (n:Src) RETURN n.id, n.name, n.path, n.line, " +
			"n.is_exported, n.emb) TO '%s'"},
	}

	for i, f := range forms {
		out := filepath.Join(dir, fmt.Sprintf("t%d.parquet", i))
		err := st.Exec(fmt.Sprintf(f.query, out), nil)
		if err != nil {
			t.Logf("%-18s -> EXPORT FAILED: %v", f.label, err)
			continue
		}
		fi, _ := os.Stat(out)

		// Round trip into a table with the SAME ddl, and check the values landed in the
		// right columns rather than merely landing.
		tbl := fmt.Sprintf("Dst%d", i)
		if derr := st.Exec(fmt.Sprintf(ddl, tbl), nil); derr != nil {
			t.Logf("%-18s -> dst ddl failed: %v", f.label, derr)
			continue
		}
		imp := st.Exec(fmt.Sprintf("COPY %s FROM '%s'", tbl, out), nil)
		if imp != nil {
			t.Logf("%-18s -> export ok (%d B) but IMPORT FAILED: %v", f.label, fi.Size(), imp)
			continue
		}
		rows, qerr := st.Query(fmt.Sprintf(`MATCH (n:%s) WHERE n.id = 1
			RETURN n.name AS name, n.path AS path, n.line AS line,
			n.is_exported AS exp, n.emb IS NOT NULL AS hasvec`, tbl), nil)
		if qerr != nil || len(rows) != 1 {
			t.Logf("%-18s -> readback failed: %v", f.label, qerr)
			continue
		}
		total, _ := st.CountQuery(fmt.Sprintf("MATCH (n:%s) RETURN count(n)", tbl), nil)
		withVec, _ := st.CountQuery(
			fmt.Sprintf("MATCH (n:%s) WHERE n.emb IS NOT NULL RETURN count(n)", tbl), nil)
		ok := ladybugstore.Str(rows[0]["name"]) == "Alpha" &&
			ladybugstore.Str(rows[0]["path"]) == "a.go" &&
			ladybugstore.Int64(rows[0]["line"]) == 7
		t.Logf("%-18s -> ROUND TRIP ok=%v  rows=%d with_vec=%d  file=%d B",
			f.label, ok, total, withVec, fi.Size())
	}
}
