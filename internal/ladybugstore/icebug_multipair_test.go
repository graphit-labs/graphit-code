package ladybugstore

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/apache/arrow-go/v18/arrow"
	"github.com/apache/arrow-go/v18/arrow/array"
)

// Can a relationship table declare MORE THAN ONE FROM/TO pair on icebug storage?
//
// This is the question the whole folded layout — and therefore the label transpiler — rests on. If
// a multi-pair relationship table worked, node tables could stay per-label, `MATCH (f:Function)`
// would be native, every type would still be ONE table, no alternatives anywhere, and no
// transpiler would be needed.
//
// It was measured as broken BEFORE the row-group fix, and three other defects blamed on the engine
// turned out to be that same fix, so the measurement is re-run here rather than trusted.

// writeLabelNodeTable writes a per-label icebug node table: dense id plus a name.
func writeLabelNodeTable(t *testing.T, dest, label string, rows int) {
	t.Helper()

	schema := arrow.NewSchema([]arrow.Field{
		{Name: "node_id", Type: arrow.PrimitiveTypes.Int64, Nullable: true},
		{Name: "name", Type: arrow.BinaryTypes.String, Nullable: true},
	}, icebugMetadata())

	if err := writeParquet(dest, schema, rows, func(b *array.RecordBuilder, from, to int) {
		ids := b.Field(0).(*array.Int64Builder)
		names := b.Field(1).(*array.StringBuilder)
		for i := from; i < to; i++ {
			ids.Append(int64(i))
			names.Append(fmt.Sprintf("%s_%d", label, i))
		}
	}); err != nil {
		t.Fatalf("node table %s: %v", label, err)
	}
}

// TestIcebugMultiPairRelTableCannotWork re-measures the constraint that forces the folded layout.
//
// Two node tables, one relationship table declaring TWO pairs, and exactly one CSR — because the
// format stores one `indices`/`indptr` per relationship TABLE, with no pair in the file name.
func TestIcebugMultiPairRelTableCannotWork(t *testing.T) {
	const rowsA, rowsB = 1000, 500
	const edges = 300

	dir := t.TempDir()
	writeLabelNodeTable(t, filepath.Join(dir, "nodes_NA.parquet"), "NA", rowsA)
	writeLabelNodeTable(t, filepath.Join(dir, "nodes_NB.parquet"), "NB", rowsB)

	// One CSR: sources are NA's dense ids, targets are dense ids too — and THAT is the problem the
	// count cannot express. A target id is resolved inside the declared TO table's own dense space,
	// so with two TO tables the same number means two different nodes.
	csr := make([]csrEdge, 0, edges)
	for i := 0; i < edges; i++ {
		csr = append(csr, csrEdge{source: uint64(i), target: uint64(i) % rowsB})
	}
	if err := writeIndices(filepath.Join(dir, "indices_R.parquet"), csr, nil, nil); err != nil {
		t.Fatal(err)
	}
	if err := writeIndptr(filepath.Join(dir, "indptr_R.parquet"), csr, rowsA); err != nil {
		t.Fatal(err)
	}

	st, err := Open(filepath.Join(t.TempDir(), "mounted"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })

	for _, stmt := range []string{
		fmt.Sprintf("CREATE NODE TABLE NA(node_id INT64, name STRING, PRIMARY KEY(node_id)) WITH (storage = '%s', format = 'icebug-disk')", EscapeLiteral(dir)),
		fmt.Sprintf("CREATE NODE TABLE NB(node_id INT64, name STRING, PRIMARY KEY(node_id)) WITH (storage = '%s', format = 'icebug-disk')", EscapeLiteral(dir)),
		fmt.Sprintf("CREATE REL TABLE R(FROM NA TO NA, FROM NA TO NB) WITH (storage = '%s', format = 'icebug-disk')", EscapeLiteral(dir)),
	} {
		if execErr := st.Exec(stmt, nil); execErr != nil {
			t.Fatalf("mounting failed:\n%s\n%v", stmt, execErr)
		}
	}

	// Both node tables read back correctly — so whatever goes wrong is the relationship table.
	for _, c := range []struct {
		table string
		want  int64
	}{{"NA", rowsA}, {"NB", rowsB}} {
		if got := scalar(t, st, fmt.Sprintf("MATCH (n:%s) RETURN count(n) AS c", c.table)); got != c.want {
			t.Errorf("%s holds %d nodes, want %d", c.table, got, c.want)
		}
	}

	got := scalar(t, st, "MATCH ()-[r:R]->() RETURN count(r) AS c")
	perPair := scalar(t, st, "MATCH (:NA)-[r:R]->(:NB) RETURN count(r) AS c")
	otherPair := scalar(t, st, "MATCH (:NA)-[r:R]->(:NA) RETURN count(r) AS c")

	t.Logf("one CSR of %d edges, declared over TWO pairs: [:R] = %d, NA->NB = %d, NA->NA = %d",
		edges, got, perPair, otherPair)

	if got == edges && perPair == edges && otherPair == 0 {
		t.Errorf("A MULTI-PAIR REL TABLE NOW READS CORRECTLY (%d edges, all on NA->NB). If this holds "+
			"on the real graph, node tables can stay per-label and the label transpiler can go. "+
			"Re-measure before believing it — and check target IDENTITY, not just the count, "+
			"because one CSR cannot address two TO tables' id spaces.", got)
	}
}

// TestIcebugFormatHasNoPerPairFile records WHY the constraint is structural rather than a bug.
//
// The reference tool emits exactly three files per graph — nodes_<t>, indices_<rel>, indptr_<rel> —
// keyed by TABLE name with no pair anywhere in it. So there is nowhere to put a second pair's CSR,
// and no upstream fix changes that: a target id is a position in ONE table's dense id space.
//
//	GRAPHIT_TOOL_ICEBUG=/tmp/icebug-fix/tool \
//	  go test -tags lancedb -run TestIcebugFormatHasNoPerPairFile ./internal/ladybugstore/ -v
func TestIcebugFormatHasNoPerPairFile(t *testing.T) {
	dir := os.Getenv("GRAPHIT_TOOL_ICEBUG")
	if dir == "" {
		t.Skip("set GRAPHIT_TOOL_ICEBUG to a directory produced by uvx icebug-format")
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var files []string
	for _, e := range entries {
		files = append(files, e.Name())
	}
	t.Logf("the reference tool's whole output: %v", files)

	for _, want := range []string{"nodes_demo.parquet", "indices_demo_rel.parquet", "indptr_demo_rel.parquet"} {
		if _, statErr := os.Stat(filepath.Join(dir, want)); statErr != nil {
			t.Errorf("expected %s in the tool output: %v", want, statErr)
		}
	}
	// One indices file per relationship TABLE. If the format ever grows a per-pair file, this is
	// where it shows up, and the folded layout can be revisited.
	var indices int
	for _, f := range files {
		if len(f) > 8 && f[:8] == "indices_" {
			indices++
		}
	}
	if indices != 1 {
		t.Errorf("found %d indices files for one relationship table — the format may now key CSRs "+
			"by pair, which would remove the reason for folding the labels", indices)
	}
}
