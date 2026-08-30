package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/apache/arrow-go/v18/parquet/file"
)

// THE INVARIANT, END TO END: the icebug format requires exactly one row group per Parquet.
// With more than one, Ladybug mounts the bundle, counts correctly through an anonymous
// pattern, and then returns WRONG DATA the moment a pattern binds a node variable — see
// TestIcebugWritesOneRowGroupPerFile in internal/ladybugstore.
//
// The writer now feeds each table to pqarrow in parquetChunkRows slices, so this exports a
// corpus whose tables are LARGER THAN ONE CHUNK — the case the unit tests of writeParquetDirect
// cover in isolation and the older fixtures are far too small to reach — then mounts the
// result and asks the queries that a multi-row-group file gets wrong.
func TestChunkedExportMountsAndAnswersBoundPatterns(t *testing.T) {
	const files = 6000
	entries := syntheticCorpus(files)
	ri := newRebuildIndex(entries, targetRulesFor(t.TempDir()))

	storeDir := filepath.Join(t.TempDir(), "store")
	bundleDir := filepath.Join(storeDir, "graph.icebug")
	man, err := ExportDirectFromRebuildIndex(ri, bundleDir, bundleDir)
	if err != nil {
		t.Fatalf("export: %v", err)
	}

	// The test is worthless unless a table actually crossed a chunk boundary.
	var crossed []string
	for _, nt := range man.NodeTables {
		if nt.Rows > int64(parquetChunkRows) {
			crossed = append(crossed, fmt.Sprintf("%s(%d rows)", nt.File, nt.Rows))
		}
	}
	for _, rg := range man.RelGroups {
		for _, m := range rg.Members {
			if m.Rows > int64(parquetChunkRows) {
				crossed = append(crossed, fmt.Sprintf("%s(%d rows)", m.Indices, m.Rows))
			}
		}
	}
	if len(crossed) == 0 {
		t.Fatalf("no table exceeded parquetChunkRows (%d), so the chunk boundary was never exercised", parquetChunkRows)
	}
	t.Logf("tables larger than one chunk: %s", strings.Join(crossed, ", "))

	dirEntries, err := os.ReadDir(bundleDir)
	if err != nil {
		t.Fatal(err)
	}
	var checked int
	for _, e := range dirEntries {
		if !strings.HasSuffix(e.Name(), ".parquet") {
			continue
		}
		rdr, openErr := file.OpenParquetFile(filepath.Join(bundleDir, e.Name()), false)
		if openErr != nil {
			t.Errorf("%s: %v", e.Name(), openErr)
			continue
		}
		groups := rdr.MetaData().NumRowGroups()
		kv := rdr.MetaData().KeyValueMetadata()
		rdr.Close()
		if groups != 1 {
			t.Errorf("%s has %d row groups, want exactly 1 — Ladybug returns wrong data for more", e.Name(), groups)
		}
		if kv == nil || kv.FindValue("icebug_disk_version") == nil {
			t.Errorf("%s carries no icebug_disk_version", e.Name())
		}
		checked++
	}
	if checked == 0 {
		t.Fatal("no parquet files were checked")
	}
	t.Logf("%d parquet files, one row group each", checked)

	db := NewLadybugDB(LadybugConfig{StoreDir: storeDir, IcebugDir: bundleDir})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	// The signature of a multi-row-group bundle: the anonymous pattern counts right and the
	// bound one does not. These name the PHYSICAL member table rather than the logical CALLS
	// type, because a logical type only runs through the bounded-reachability planner, and
	// the planner is not what the row group count breaks — the reader binding a node
	// variable is.
	const member = "calls__function_function"
	anonymous := chunkedScalar(t, db, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", member))
	bound := chunkedScalar(t, db, fmt.Sprintf("MATCH (a:Function)-[r:%s]->(b:Function) RETURN count(a) AS c", member))
	if anonymous == 0 {
		t.Fatal("no CALLS edges were mounted at all")
	}
	if bound != anonymous {
		t.Errorf("count through a bound node variable = %d, anonymous = %d — the classic multi-row-group symptom", bound, anonymous)
	}
	t.Logf("edges: %d anonymous, %d through a bound node variable", anonymous, bound)

	// syntheticCorpus has file f's Symbol{f}_{i} call file f+1's Symbol{f+1}_{i}, so each of
	// these is exactly one edge, and both are the anchored form that resolved to ZERO on a
	// multi-row-group file while the anonymous count above stayed correct.
	//
	// They anchor on uid rather than on name because uid is the primary key: a single row
	// group removes row-group pruning, so a filter on an ordinary property degenerates into
	// a full scan per candidate on a corpus this size. That is a planner cost, not the
	// invariant under test.
	const callerUID = "internal/pkg5/module5/file5.go:Symbol3"
	const calleeUID = "internal/pkg6/module6/file6.go:Symbol3"
	if got := chunkedScalar(t, db, fmt.Sprintf(
		"MATCH (a:Function)-[r:%s]->(b:Function) WHERE a.uid = '%s' RETURN count(*) AS c", member, callerUID)); got != 1 {
		t.Errorf("source-anchored filter on %s = %d, want 1 — check the row group count", callerUID, got)
	}
	if got := chunkedScalar(t, db, fmt.Sprintf(
		"MATCH (a:Function)-[r:%s]->(b:Function) WHERE b.uid = '%s' RETURN count(*) AS c", member, calleeUID)); got != 1 {
		t.Errorf("target-anchored filter on %s = %d, want 1 — check the row group count", calleeUID, got)
	}

	// NOT asserted here: the same question through the logical :CALLS type. It returns the
	// right answer, but `tryCanonicalBoundedTraversal` walks the whole reachable component
	// even for a one-hop plan, and syntheticCorpus is a call chain spanning every file —
	// 28.9s at 1500 files, over 180s at 6000. That is a planner cost in a subsystem this
	// change does not touch, and the bundle it reads is byte-identical to the one the
	// previous writer produced (see icebug_bundle_dump_probe_test.go).
}

func chunkedScalar(t *testing.T, db *LadybugBackend, query string) int64 {
	t.Helper()
	res, err := db.Query(context.Background(), query, nil)
	if err != nil {
		t.Fatalf("query %q: %v", query, err)
	}
	if len(res.Records) == 0 {
		return 0
	}
	for _, v := range res.Records[0] {
		switch n := v.(type) {
		case int64:
			return n
		case int:
			return int64(n)
		case float64:
			return int64(n)
		}
	}
	return 0
}
