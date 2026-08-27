package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// TestCopyValueCeilingByFormat asks whether the ceiling that forces oversized rows onto
// UNWIND is a property of the JSON reader or of COPY itself.
//
// JSON refuses a single string value past somewhere between 16 and 32 MB with "Malformed
// JSON: unexpected end of data", while a 111.9 MB document made of many smaller rows loads
// fine — so it is a per-value limit, not memory and not document size. If Parquet or CSV
// carries the same value, staging oversized rows in that format removes the special case
// entirely.
//
// The big value is put into a table with UNWIND first (which accepts it) and exported with
// COPY TO, which is also how this test obtains a Parquet file without adding a writer to
// the module.
//
// GRAPHIT_COPY_ROWSIZE=1 go test -run TestCopyValueCeilingByFormat ./internal/ast/ -v -timeout 30m
func TestCopyValueCeilingByFormat(t *testing.T) {
	if os.Getenv("GRAPHIT_COPY_ROWSIZE") == "" {
		t.Skip("set GRAPHIT_COPY_ROWSIZE=1")
	}

	for _, mb := range []int{32, 64, 128} {
		t.Run(fmt.Sprintf("%dMB", mb), func(t *testing.T) {
			dir := t.TempDir()
			st, err := openProbeStoreWithJSON(t, filepath.Join(dir, "db"))
			if err != nil {
				t.Fatalf("open: %v", err)
			}
			defer func() { _ = st.Close() }()

			if err := st.Exec(`CREATE NODE TABLE Src(path STRING, source STRING,
				PRIMARY KEY(path))`, nil); err != nil {
				t.Fatalf("ddl: %v", err)
			}
			unit := "<entry>\n    <key>Remote URL</key>\n    <value>https://x/y</value>\n</entry>\n"
			body := strings.Repeat(unit, (mb<<20)/len(unit))
			if err := st.Exec(`UNWIND $b AS r CREATE (:Src {path: r.path, source: r.source})`,
				map[string]any{"b": []any{
					map[string]any{"path": "big.xml", "source": body},
				}}); err != nil {
				t.Fatalf("seed: %v", err)
			}

			for _, ext := range []string{"parquet", "csv"} {
				out := filepath.Join(dir, "big."+ext)
				if err := st.Exec(fmt.Sprintf(
					`COPY (MATCH (s:Src) RETURN s.path, s.source) TO '%s'`, out), nil); err != nil {
					t.Logf("%-8s COPY TO failed: %v", ext, err)
					continue
				}
				fi, _ := os.Stat(out)

				tbl := "Dst_" + ext
				if err := st.Exec(fmt.Sprintf(`CREATE NODE TABLE %s(path STRING,
					source STRING, PRIMARY KEY(path))`, tbl), nil); err != nil {
					t.Logf("%-8s ddl failed: %v", ext, err)
					continue
				}
				err := st.Exec(fmt.Sprintf("COPY %s FROM '%s'", tbl, out), nil)
				stored := int64(-1)
				if rows, qerr := st.Query(fmt.Sprintf(
					"MATCH (n:%s) RETURN size(n.source) AS n", tbl), nil); qerr == nil && len(rows) == 1 {
					stored = ladybugstore.Int64(rows[0]["n"])
				}
				short := ""
				if err != nil {
					short = err.Error()
					if i := strings.Index(short, "Line/record"); i > 0 {
						short = short[:i]
					}
					if len(short) > 120 {
						short = short[:120]
					}
				}
				t.Logf("%-8s value %3d MB (file %6.1f MB) -> stored=%d want=%d err=%s",
					ext, mb, float64(fi.Size())/(1<<20), stored, len(body), short)
				_ = os.Remove(out)
			}
		})
	}
}
