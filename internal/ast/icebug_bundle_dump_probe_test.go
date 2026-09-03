package ast

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/apache/arrow-go/v18/parquet"
	"github.com/apache/arrow-go/v18/parquet/file"
	"github.com/apache/arrow-go/v18/parquet/schema"
)

// Renders a whole icebug bundle to a stable text form so two builds can be diffed.
// GRAPHIT_DUMP_BUNDLE=/tmp/out.txt go test -run TestDumpBundle ./internal/ast/
func TestDumpBundle(t *testing.T) {
	dest := os.Getenv("GRAPHIT_DUMP_BUNDLE")
	if dest == "" {
		t.Skip("set GRAPHIT_DUMP_BUNDLE")
	}
	entries := syntheticCorpus(1200)
	ri := newRebuildIndex(entries, targetRulesFor(t.TempDir()))
	bundleDir := filepath.Join(t.TempDir(), "graph.icebug")
	if _, err := ExportDirectFromRebuildIndex(ri, bundleDir, bundleDir); err != nil {
		t.Fatalf("export: %v", err)
	}

	var out strings.Builder
	names, _ := filepath.Glob(filepath.Join(bundleDir, "*"))
	sort.Strings(names)
	for _, path := range names {
		base := filepath.Base(path)
		if !strings.HasSuffix(base, ".parquet") {
			raw, _ := os.ReadFile(path)
			text := strings.ReplaceAll(string(raw), bundleDir, "<BUNDLE>")
			fmt.Fprintf(&out, "== %s ==\n%s\n", base, text)
			continue
		}
		rdr, err := file.OpenParquetFile(path, false)
		if err != nil {
			t.Fatalf("%s: %v", base, err)
		}
		md := rdr.MetaData()
		fmt.Fprintf(&out, "== %s == rows=%d rowgroups=%d cols=%d\n", base, md.NumRows, md.NumRowGroups(), md.Schema.NumColumns())
		for i := 0; i < md.Schema.NumColumns(); i++ {
			c := md.Schema.Column(i)
			fmt.Fprintf(&out, "  col %d %s %s %s\n", i, c.Name(), c.PhysicalType(), repetition(c))
		}
		h := sha256.New()
		for rg := 0; rg < md.NumRowGroups(); rg++ {
			group := rdr.RowGroup(rg)
			for ci := 0; ci < md.Schema.NumColumns(); ci++ {
				cr, err := group.Column(ci)
				if err != nil {
					t.Fatalf("%s col %d: %v", base, ci, err)
				}
				fmt.Fprintf(h, "|col%d|", ci)
				dumpColumn(t, h, cr, md.NumRows)
			}
		}
		fmt.Fprintf(&out, "  values-sha256 %s\n", hex.EncodeToString(h.Sum(nil)))
		rdr.Close()
	}
	if err := os.WriteFile(dest, []byte(out.String()), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Logf("wrote %s (%d bytes)", dest, out.Len())
}

func repetition(c *schema.Column) string {
	if c.MaxDefinitionLevel() > 0 {
		return "optional"
	}
	return "required"
}

func dumpColumn(t *testing.T, h interface{ Write([]byte) (int, error) }, cr file.ColumnChunkReader, total int64) {
	t.Helper()
	const batch = 4096
	defLevels := make([]int16, batch)
	switch r := cr.(type) {
	case *file.ByteArrayColumnChunkReader:
		buf := make([]parquet.ByteArray, batch)
		for {
			_, n, err := r.ReadBatch(batch, buf, defLevels, nil)
			if err != nil || n == 0 {
				return
			}
			for i := 0; i < n; i++ {
				fmt.Fprintf(h, "%s;", string(buf[i]))
			}
		}
	case *file.Int64ColumnChunkReader:
		buf := make([]int64, batch)
		for {
			_, n, err := r.ReadBatch(batch, buf, defLevels, nil)
			if err != nil || n == 0 {
				return
			}
			for i := 0; i < n; i++ {
				fmt.Fprintf(h, "%d;", buf[i])
			}
		}
	case *file.BooleanColumnChunkReader:
		buf := make([]bool, batch)
		for {
			_, n, err := r.ReadBatch(batch, buf, defLevels, nil)
			if err != nil || n == 0 {
				return
			}
			for i := 0; i < n; i++ {
				fmt.Fprintf(h, "%v;", buf[i])
			}
		}
	default:
		fmt.Fprintf(h, "<unhandled %T>", cr)
	}
}
