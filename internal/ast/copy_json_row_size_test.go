package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// TestCopyJSONLargeRowLimit brackets how big a single string value may be in a JSON document
// the engine will COPY.
//
// It exists because staging SearchFile through JSON failed on a real corpus with "Malformed
// JSON: unexpected end of data" while the graph's own COPY of the same corpus succeeded —
// and the difference between the two is that SearchFile carries a whole file's source in a
// column, with individual files reaching tens of megabytes.
//
// GRAPHIT_COPY_ROWSIZE=1 go test -run TestCopyJSONLargeRowLimit ./internal/ast/ -v -timeout 30m
func TestCopyJSONLargeRowLimit(t *testing.T) {
	if os.Getenv("GRAPHIT_COPY_ROWSIZE") == "" {
		t.Skip("set GRAPHIT_COPY_ROWSIZE=1")
	}
	dir := t.TempDir()

	for _, mb := range []int{1, 8, 16, 32, 64} {
		t.Run(fmt.Sprintf("%dMB", mb), func(t *testing.T) {
			st, err := openProbeStoreWithJSON(t, filepath.Join(dir, fmt.Sprintf("db%d", mb)))
			if err != nil {
				t.Fatalf("open: %v", err)
			}
			defer func() { _ = st.Close() }()

			if err := st.Exec(`CREATE NODE TABLE Doc(path STRING, source STRING,
				PRIMARY KEY(path))`, nil); err != nil {
				t.Fatalf("ddl: %v", err)
			}

			unit := "<entry>\n    <key>Remote URL</key>\n    <value>https://x/y</value>\n</entry>\n"
			body := strings.Repeat(unit, (mb<<20)/len(unit))

			stage := filepath.Join(dir, fmt.Sprintf("doc%d.json", mb))
			if err := writeJSONFile(stage, []map[string]any{
				{"path": "big.xml", "source": body},
			}); err != nil {
				t.Fatalf("stage: %v", err)
			}
			fi, _ := os.Stat(stage)

			err = st.Exec(fmt.Sprintf("COPY Doc FROM '%s'", stage), nil)
			count, _ := st.CountQuery("MATCH (n:Doc) RETURN count(n)", nil)
			short := ""
			if err != nil {
				short = err.Error()
				if i := strings.Index(short, "Line/record"); i > 0 {
					short = short[:i]
				}
				if len(short) > 160 {
					short = short[:160]
				}
			}
			t.Logf("ONE row, source %2d MB (file %.1f MB) -> rows=%d err=%s",
				mb, float64(fi.Size())/(1<<20), count, short)
			_ = os.Remove(stage)
		})
	}

	t.Run("many rows same volume", func(t *testing.T) {
		st, err := openProbeStoreWithJSON(t, filepath.Join(dir, "dbmany"))
		if err != nil {
			t.Fatalf("open: %v", err)
		}
		defer func() { _ = st.Close() }()
		if err := st.Exec(`CREATE NODE TABLE Doc(path STRING, source STRING,
			PRIMARY KEY(path))`, nil); err != nil {
			t.Fatalf("ddl: %v", err)
		}

		unit := "<entry>\n    <key>Remote URL</key>\n    <value>https://x/y</value>\n</entry>\n"
		one := strings.Repeat(unit, (1<<20)/len(unit))
		rows := make([]map[string]any, 0, 60)
		for i := 0; i < 60; i++ {
			rows = append(rows, map[string]any{
				"path": fmt.Sprintf("f%d.xml", i), "source": one,
			})
		}
		stage := filepath.Join(dir, "many.json")
		if err := writeJSONFile(stage, rows); err != nil {
			t.Fatalf("stage: %v", err)
		}
		fi, _ := os.Stat(stage)
		err = st.Exec(fmt.Sprintf("COPY Doc FROM '%s'", stage), nil)
		count, _ := st.CountQuery("MATCH (n:Doc) RETURN count(n)", nil)
		short := ""
		if err != nil {
			short = err.Error()
			if i := strings.Index(short, "Line/record"); i > 0 {
				short = short[:i]
			}
		}
		t.Logf("60 rows of 1 MB (file %.1f MB) -> rows=%d err=%s",
			float64(fi.Size())/(1<<20), count, short)
		_ = os.Remove(stage)
	})

	t.Run("unwind takes what copy refuses", func(t *testing.T) {
		st, err := openProbeStoreWithJSON(t, filepath.Join(dir, "dbunwind"))
		if err != nil {
			t.Fatalf("open: %v", err)
		}
		defer func() { _ = st.Close() }()
		if err := st.Exec(`CREATE NODE TABLE Doc(path STRING, source STRING,
			PRIMARY KEY(path))`, nil); err != nil {
			t.Fatalf("ddl: %v", err)
		}
		unit := "<entry>\n    <key>Remote URL</key>\n    <value>https://x/y</value>\n</entry>\n"
		for _, mb := range []int{32, 64, 128} {
			body := strings.Repeat(unit, (mb<<20)/len(unit))
			err := st.Exec(`UNWIND $b AS r CREATE (:Doc {path: r.path, source: r.source})`,
				map[string]any{"b": []any{
					map[string]any{"path": fmt.Sprintf("u%d.xml", mb), "source": body},
				}})
			got, _ := st.Query(fmt.Sprintf(
				"MATCH (n:Doc) WHERE n.path = 'u%d.xml' RETURN size(n.source) AS n", mb), nil)
			stored := int64(-1)
			if len(got) == 1 {
				stored = ladybugstore.Int64(got[0]["n"])
			}
			t.Logf("UNWIND %3d MB -> stored=%d bytes err=%v", mb, stored, err)
		}
	})
}

func openProbeStoreWithJSON(t *testing.T, path string) (*ladybugstore.Store, error) {
	t.Helper()
	st, err := ladybugstore.Open(path)
	if err != nil {
		return nil, err
	}
	_ = st.Exec("INSTALL json", nil)
	if err := st.LoadExtensions("json"); err != nil {
		_ = st.Close()
		return nil, err
	}
	return st, nil
}
