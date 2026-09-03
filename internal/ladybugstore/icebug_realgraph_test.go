package ladybugstore

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"testing"
	"time"

	"strings"

	"github.com/apache/arrow-go/v18/arrow/array"
	"github.com/apache/arrow-go/v18/arrow/memory"
	"github.com/apache/arrow-go/v18/parquet/file"
	"github.com/apache/arrow-go/v18/parquet/pqarrow"
)

var (
	realStoreOnce sync.Once
	realStore     *Store
	realStoreErr  error
)

func openRealStore(t *testing.T) *Store {
	t.Helper()
	path := os.Getenv("GRAPHIT_REAL_STORE")
	if path == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}
	realStoreOnce.Do(func() {
		realStore, realStoreErr = OpenReadOnly(path)
	})
	if realStoreErr != nil {
		t.Fatalf("open source read-only: %v", realStoreErr)
	}
	return realStore
}

// TestIcebugAgainstARealGraph exports a real, fully populated store and proves the round trip
// label by label and relationship type by relationship type.
//
// It exists because a hand-made fixture HIDES defects: a two-row table survives an encoding
// bug that a five-thousand-row table does not, and a two-alternative query can coincide with
// the right answer. Both happened. Any change to the export is checked here before it is
// believed.
//
//	GRAPHIT_REAL_STORE=~/.graphit/ast/project/<id>/ladybugdb \
//	  go test -run TestIcebugAgainstARealGraph ./internal/ladybugstore/ -v
func TestIcebugAgainstARealGraph(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}

	src, err := OpenReadOnly(storePath)
	if err != nil {
		t.Fatalf("open source read-only: %v", err)
	}
	defer src.Close()

	out := t.TempDir()
	start := time.Now()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	exportTook := time.Since(start)
	t.Logf("exported %d nodes across %d labels and %d edges across %d relationship tables in %.1fs",
		man.NodeCount, len(man.Labels), man.EdgeCount, len(man.Rels), exportTook.Seconds())

	mounted := mountIcebug(t, out)

	for _, l := range man.Labels {
		want := scalar(t, src, fmt.Sprintf("MATCH (x:%s) RETURN count(x) AS c", QuoteIdent(l.Label)))
		if want != l.Rows {
			t.Errorf("%s: manifest says %d rows, source has %d", l.Label, l.Rows, want)
		}
		if got := labelCount(t, mounted, l.Label); want != got {
			t.Errorf("%s: source %d nodes, folded table %d", l.Label, want, got)
		}
	}
	if total := scalar(t, mounted,
		fmt.Sprintf("MATCH (n:%s) RETURN count(n) AS c", IcebugEntityTable)); total != man.NodeCount {
		t.Errorf("folded table holds %d nodes, manifest says %d", total, man.NodeCount)
	}

	types := make([]string, 0, len(man.Rels))
	byType := map[string]int64{}
	for _, r := range man.Rels {
		types = append(types, r.Type)
		byType[r.Type] = r.Rows
	}
	sort.Strings(types)

	var totalWant int64
	for _, typ := range types {
		q := fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", QuoteIdent(typ))
		want := scalar(t, src, q)
		got := scalar(t, mounted, q)
		totalWant += want
		status := "ok"
		if want != got || want != byType[typ] {
			status = "MISMATCH"
			t.Errorf("%s: source %d edges, mounted %d, manifest %d", typ, want, got, byType[typ])
		}
		t.Logf("  %-16s %8d edges  %s", typ, want, status)
	}
	if man.EdgeCount != totalWant {
		t.Errorf("manifest counts %d edges, source has %d", man.EdgeCount, totalWant)
	}

	selfLoops := scalar(t, src,
		"MATCH (a)-[r:CALLS]->(b) WHERE offset(id(a)) = offset(id(b)) AND label(a) = label(b) RETURN count(r) AS c")
	inCSR, err := countSelfLoopsInCSR(out, "CALLS")
	if err != nil {
		t.Fatalf("reading the CSR back: %v", err)
	}
	t.Logf("self-loops on CALLS: source %d, present in the CSR %d", selfLoops, inCSR)
	if inCSR != selfLoops {
		t.Errorf("self-loops: source %d, CSR %d — recursive calls were dropped", selfLoops, inCSR)
	}
}

// TestIcebugRealGraphQueryCost measures what folding costs a read, which is the question
// on-the-fly querying turns on.
//
// It compares the SAME question against the source store and the mounted export, on a local
// filesystem. That isolates the layout's cost from the network: a remote read adds latency on
// top of these numbers, it does not change their ratio.
//
// The two shapes that matter pull in opposite directions:
//
//   - A traversal should be no worse and often better, because a relationship type is one CSR
//     instead of the 62 the partitioned layout needed.
//
//   - A label-selective scan reads more rows, because every label lives in one table. Labels
//     remain contiguous, but every Parquet must have exactly one row group: multiple groups make
//     the Icebug reader return silently wrong bound-endpoint results on large graphs. We therefore
//     accept that row-group pruning is unavailable and optimize traversal through the CSR instead.
//
//     GRAPHIT_REAL_STORE=<ladybugdb> go test -run TestIcebugRealGraphQueryCost ./internal/ladybugstore/ -v
func TestIcebugRealGraphQueryCost(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}

	src, err := OpenReadOnly(storePath)
	if err != nil {
		t.Fatalf("open source read-only: %v", err)
	}
	defer src.Close()

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	var bytes int64
	entries, _ := os.ReadDir(out)
	for _, e := range entries {
		if info, statErr := e.Info(); statErr == nil {
			bytes += info.Size()
		}
	}
	t.Logf("artifact: %d files, %.1f MiB for %d nodes and %d edges",
		len(entries), float64(bytes)/(1<<20), man.NodeCount, man.EdgeCount)

	timed := func(st *Store, query string) (int64, time.Duration) {
		start := time.Now()
		rows, qErr := st.Query(query, nil)
		took := time.Since(start)
		if qErr != nil {
			t.Errorf("%s -> %v", query, qErr)
			return -1, took
		}
		var n int64
		if len(rows) > 0 {
			for _, v := range rows[0] {
				n = Int64(v)
				break
			}
		}
		return n, took
	}

	cases := []struct {
		what           string
		native, folded string
	}{
		{
			what:   "count one label",
			native: "MATCH (n:Function) RETURN count(n) AS c",
			folded: fmt.Sprintf("MATCH (n:%s) WHERE n.%s = 'Function' RETURN count(n) AS c",
				IcebugEntityTable, IcebugLabelColumn),
		},
		{
			what:   "filter a label on a property",
			native: "MATCH (n:Function) WHERE n.cyclomatic_complexity > 10 RETURN count(n) AS c",
			folded: fmt.Sprintf("MATCH (n:%s) WHERE n.%s = 'Function' AND n.cyclomatic_complexity > 10 RETURN count(n) AS c",
				IcebugEntityTable, IcebugLabelColumn),
		},
		{
			what:   "count one relationship type",
			native: "MATCH ()-[r:CALLS]->() RETURN count(r) AS c",
			folded: "MATCH ()-[r:CALLS]->() RETURN count(r) AS c",
		},
		{
			what:   "one-hop with bound endpoints",
			native: "MATCH (a)-[r:CALLS]->(b) RETURN count(*) AS c",
			folded: "MATCH (a)-[r:CALLS]->(b) RETURN count(*) AS c",
		},
	}

	t.Logf("%-32s %12s %12s %10s", "query", "native", "icebug", "ratio")
	for _, c := range cases {
		nWant, dNative := timed(src, c.native)
		nGot, dFolded := timed(mounted, c.folded)

		ratio := "n/a"
		if dNative > 0 {
			ratio = fmt.Sprintf("%.2fx", float64(dFolded)/float64(dNative))
		}
		t.Logf("%-32s %10dms %10dms %10s   rows native=%d icebug=%d",
			c.what, dNative.Milliseconds(), dFolded.Milliseconds(), ratio, nWant, nGot)

		if nWant >= 0 && nGot >= 0 && nWant != nGot {
			t.Errorf("%s: native answered %d, icebug answered %d — the layouts disagree",
				c.what, nWant, nGot)
		}
	}
}

// TestIcebugRealGraphThreeHopPlans compares the physical plans available for the
// target-anchored impact query without executing the recursive variants. The original
// Icebug plan is known to time out on a populated graph, so EXPLAIN is intentionally the
// diagnostic boundary here: it exposes global node enumeration without paying for it.
//
//	GRAPHIT_REAL_STORE=~/.graphit/ast/project/<id>/ladybugdb \
//	  go test -run TestIcebugRealGraphThreeHopPlans ./internal/ladybugstore/ -v
func TestIcebugRealGraphThreeHopPlans(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}

	src, err := OpenReadOnly(storePath)
	if err != nil {
		t.Fatalf("open source read-only: %v", err)
	}
	defer src.Close()

	targets, err := src.Query(
		"MATCH ()-[:CALLS]->(t) WHERE t.uid IN ['internal/ast/ladybug.go::runQuery'] "+
			"RETURN t.uid AS uid, count(*) AS inbound LIMIT 1", nil)
	if err != nil || len(targets) == 0 {
		t.Fatalf("select the representative runQuery target: rows=%v err=%v", targets, err)
	}
	targetUID := Str(targets[0]["uid"])
	t.Logf("target uid=%q inbound=%d", targetUID, Int64(targets[0]["inbound"]))

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	target := "'" + EscapeLiteral(targetUID) + "'"
	cases := []struct {
		name  string
		query string
	}{
		{
			name: "original recursive, filter on target",
			query: fmt.Sprintf("MATCH (caller:%s)-[:CALLS*1..3]->(t:%s) "+
				"WHERE t.uid IN [%s] RETURN count(DISTINCT caller.uid) AS c",
				IcebugEntityTable, IcebugEntityTable, target),
		},
		{
			name: "reverse recursive, filter on source",
			query: fmt.Sprintf("MATCH (t:%s)-[:CALLS%s*1..3]->(caller:%s) "+
				"WHERE t.uid IN [%s] RETURN count(DISTINCT caller.uid) AS c",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable, target),
		},
		{
			name: "reverse fixed one hop",
			query: fmt.Sprintf("MATCH (t:%s)-[:CALLS%s]->(caller:%s) "+
				"WHERE t.uid IN [%s] RETURN count(DISTINCT caller.uid) AS c",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable, target),
		},
		{
			name: "reverse fixed two hops",
			query: fmt.Sprintf("MATCH (t:%s)-[:CALLS%s]->(:%s)-[:CALLS%s]->(caller:%s) "+
				"WHERE t.uid IN [%s] RETURN count(DISTINCT caller.uid) AS c",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable,
				IcebugReverseSuffix, IcebugEntityTable, target),
		},
		{
			name: "reverse fixed three hops",
			query: fmt.Sprintf("MATCH (t:%s)-[:CALLS%s]->(:%s)-[:CALLS%s]->(:%s)-[:CALLS%s]->(caller:%s) "+
				"WHERE t.uid IN [%s] RETURN count(DISTINCT caller.uid) AS c",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable,
				IcebugReverseSuffix, IcebugEntityTable, IcebugReverseSuffix,
				IcebugEntityTable, target),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			rows, err := mounted.Query("EXPLAIN "+tc.query, nil)
			if err != nil {
				t.Fatalf("EXPLAIN: %v\nquery: %s", err, tc.query)
			}
			t.Logf("query: %s", tc.query)
			for _, row := range rows {
				for _, value := range row {
					t.Logf("%s", Str(value))
				}
			}
		})
	}

	profileMode := os.Getenv("GRAPHIT_PROFILE_ICEBUG")
	if profileMode == "" {
		return
	}
	nativeQuery := fmt.Sprintf("MATCH (caller)-[:CALLS*1..3]->(t) "+
		"WHERE t.uid IN [%s] RETURN count(DISTINCT caller.uid) AS c", target)
	want, wantTook := timedScalarQuery(t, src, nativeQuery)
	t.Logf("native recursive result=%d took=%s", want, wantTook)
	if profileMode == "iterative" {
		frontier := []string{targetUID}
		all := map[string]struct{}{}
		var total time.Duration
		for hop := 1; hop <= 3 && len(frontier) > 0; hop++ {
			query := fmt.Sprintf("MATCH (t:%s)-[:CALLS%s]->(caller:%s) "+
				"WHERE t.uid IN [%s] RETURN DISTINCT caller.uid AS uid",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable,
				cypherStringList(frontier))
			uids, took := timedUIDQuery(t, mounted, query)
			total += took
			frontier = frontier[:0]
			for uid := range uids {
				if _, seen := all[uid]; seen {
					continue
				}
				all[uid] = struct{}{}
				frontier = append(frontier, uid)
			}
			sort.Strings(frontier)
			t.Logf("icebug iterative hop=%d new=%d total=%d took=%s", hop, len(frontier), len(all), took)
		}
		t.Logf("icebug iterative union result=%d total=%s", len(all), total)
		if int64(len(all)) != want {
			t.Errorf("iterative result=%d, native result=%d", len(all), want)
		}
		return
	}
	if profileMode == "fixed" {
		fixedQueries := []string{
			fmt.Sprintf("MATCH (t:%s)-[:CALLS%s]->(caller:%s) WHERE t.uid IN [%s] RETURN DISTINCT caller.uid AS uid",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable, target),
			fmt.Sprintf("MATCH (t:%s)-[:CALLS%s]->(:%s)-[:CALLS%s]->(caller:%s) WHERE t.uid IN [%s] RETURN DISTINCT caller.uid AS uid",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable,
				IcebugReverseSuffix, IcebugEntityTable, target),
			fmt.Sprintf("MATCH (t:%s)-[:CALLS%s]->(:%s)-[:CALLS%s]->(:%s)-[:CALLS%s]->(caller:%s) WHERE t.uid IN [%s] RETURN DISTINCT caller.uid AS uid",
				IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable,
				IcebugReverseSuffix, IcebugEntityTable, IcebugReverseSuffix,
				IcebugEntityTable, target),
		}
		all := map[string]struct{}{}
		var total time.Duration
		for hop, query := range fixedQueries {
			uids, took := timedUIDQuery(t, mounted, query)
			total += took
			for uid := range uids {
				all[uid] = struct{}{}
			}
			t.Logf("icebug reverse fixed hop=%d rows=%d took=%s", hop+1, len(uids), took)
		}
		t.Logf("icebug reverse fixed union result=%d total=%s", len(all), total)
		if int64(len(all)) != want {
			t.Errorf("reverse fixed union result=%d, native result=%d", len(all), want)
		}
		return
	}
	got, gotTook := timedScalarQuery(t, mounted, cases[1].query)
	t.Logf("icebug reverse recursive result=%d took=%s", got, gotTook)
	if got != want {
		t.Errorf("reverse recursive result=%d, native result=%d", got, want)
	}
}

func cypherStringList(values []string) string {
	quoted := make([]string, len(values))
	for i, value := range values {
		quoted[i] = "'" + EscapeLiteral(value) + "'"
	}
	return strings.Join(quoted, ",")
}

func timedUIDQuery(t *testing.T, store *Store, query string) (map[string]struct{}, time.Duration) {
	t.Helper()
	start := time.Now()
	rows, err := store.Query(query, nil)
	took := time.Since(start)
	if err != nil {
		t.Fatalf("query: %v\n%s", err, query)
	}
	uids := make(map[string]struct{}, len(rows))
	for _, row := range rows {
		uids[Str(row["uid"])] = struct{}{}
	}
	return uids, took
}

func timedScalarQuery(t *testing.T, store *Store, query string) (int64, time.Duration) {
	t.Helper()
	start := time.Now()
	rows, err := store.Query(query, nil)
	took := time.Since(start)
	if err != nil {
		t.Fatalf("query: %v\n%s", err, query)
	}
	if len(rows) != 1 {
		t.Fatalf("query returned %d rows, want one\n%s", len(rows), query)
	}
	for _, value := range rows[0] {
		return Int64(value), took
	}
	return 0, took
}

// A rebuild has to restore the schema, not only read the data.
func TestIcebugRealGraphManifestCoversEveryLabel(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}

	src, err := OpenReadOnly(storePath)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer src.Close()

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

	for _, l := range man.Labels {
		if man.LabelKeys[l.Label] == "" {
			t.Errorf("label %s has no recorded primary key, so a rebuild cannot restore it", l.Label)
		}
	}
	for _, r := range man.Rels {
		if len(r.Pairs) == 0 {
			t.Errorf("%s records no pairs, so a rebuild cannot restore its FROM/TO", r.Type)
		}
		var pairRows int64
		for _, p := range r.Pairs {
			pairRows += p.Rows
		}
		if pairRows != r.Rows {
			t.Errorf("%s: pair rows sum to %d, table has %d", r.Type, pairRows, r.Rows)
		}
	}
	if _, err := os.Stat(filepath.Join(out, "nodes_"+IcebugEntityTable+".parquet")); err != nil {
		t.Errorf("entity table missing: %v", err)
	}
}

func countSelfLoopsInCSR(dir, relType string) (int64, error) {
	targets, err := readUint64Column(filepath.Join(dir, "indices_"+relType+".parquet"), "target")
	if err != nil {
		return 0, err
	}
	ptr, err := readUint64Column(filepath.Join(dir, "indptr_"+relType+".parquet"), "ptr")
	if err != nil {
		return 0, err
	}

	var loops int64
	for source := 0; source+1 < len(ptr); source++ {
		for i := ptr[source]; i < ptr[source+1]; i++ {
			if i < uint64(len(targets)) && targets[i] == uint64(source) {
				loops++
			}
		}
	}
	return loops, nil
}

func readUint64Column(path, column string) ([]uint64, error) {
	rdr, err := file.OpenParquetFile(path, false)
	if err != nil {
		return nil, err
	}
	defer rdr.Close()

	fr, err := pqarrow.NewFileReader(rdr,
		pqarrow.ArrowReadProperties{BatchSize: 4096}, memory.DefaultAllocator)
	if err != nil {
		return nil, err
	}
	tbl, err := fr.ReadTable(context.Background())
	if err != nil {
		return nil, err
	}
	defer tbl.Release()

	idx := -1
	for i, f := range tbl.Schema().Fields() {
		if f.Name == column {
			idx = i
		}
	}
	if idx < 0 {
		return nil, fmt.Errorf("%s has no column %s", path, column)
	}

	var out []uint64
	for _, chunk := range tbl.Column(idx).Data().Chunks() {
		for row := 0; row < chunk.Len(); row++ {
			switch c := chunk.(type) {
			case *array.Uint64:
				out = append(out, c.Value(row))
			case *array.Int64:
				out = append(out, uint64(c.Value(row)))
			default:
				return nil, fmt.Errorf("%s.%s is %s, want an integer", path, column, chunk.DataType())
			}
		}
	}
	return out, nil
}

func TestIcebugCountOfANodeVariableAgrees(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}

	src, err := OpenReadOnly(storePath)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer src.Close()

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	want := scalar(t, src, "MATCH ()-[r:CALLS]->() RETURN count(r) AS c")

	for _, q := range []string{
		"MATCH ()-[r:CALLS]->() RETURN count(r) AS c",
		fmt.Sprintf("MATCH (a:%s)-[r:CALLS]->(b:%s) RETURN count(r) AS c", IcebugEntityTable, IcebugEntityTable),
		fmt.Sprintf("MATCH (a:%s)-[r:CALLS]->(b:%s) RETURN count(*) AS c", IcebugEntityTable, IcebugEntityTable),
	} {
		if got := scalar(t, mounted, q); got != want {
			t.Errorf("%s -> %d, want %d", q, got, want)
		}
	}

	nodeCount := scalar(t, mounted, fmt.Sprintf(
		"MATCH (a:%s)-[:CALLS]->(b:%s) RETURN count(a) AS c", IcebugEntityTable, IcebugEntityTable))
	if nodeCount != want {
		t.Errorf("count(a) = %d, want %d — check the row group count of the exported files",
			nodeCount, want)
	}
}

func TestIcebugFiltersOnBothSidesOfAPattern(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}

	src, err := OpenReadOnly(storePath)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer src.Close()

	callers, err := src.Query(
		"MATCH (a)-[r:CALLS]->(b) RETURN a.name AS n, count(r) AS c ORDER BY c DESC LIMIT 1", nil)
	if err != nil || len(callers) == 0 {
		t.Fatalf("picking a caller: %v", err)
	}
	caller := Str(callers[0]["n"])

	callees, err := src.Query(
		"MATCH (a)-[r:CALLS]->(b) RETURN b.name AS n, count(r) AS c ORDER BY c DESC LIMIT 1", nil)
	if err != nil || len(callees) == 0 {
		t.Fatalf("picking a callee: %v", err)
	}
	callee := Str(callees[0]["n"])

	callerEdges := scalar(t, src, fmt.Sprintf(
		"MATCH (a)-[r:CALLS]->(b) WHERE a.name = '%s' RETURN count(*) AS c", caller))
	calleeEdges := scalar(t, src, fmt.Sprintf(
		"MATCH (a)-[r:CALLS]->(b) WHERE b.name = '%s' RETURN count(*) AS c", callee))

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	gotTarget := scalar(t, mounted, fmt.Sprintf(
		"MATCH (a:%s)-[r:CALLS]->(b:%s) WHERE b.name = '%s' RETURN count(*) AS c",
		IcebugEntityTable, IcebugEntityTable, callee))
	if gotTarget != calleeEdges {
		t.Errorf("target-anchored filter = %d, want %d", gotTarget, calleeEdges)
	}

	gotSource := scalar(t, mounted, fmt.Sprintf(
		"MATCH (a:%s)-[r:CALLS]->(b:%s) WHERE a.name = '%s' RETURN count(*) AS c",
		IcebugEntityTable, IcebugEntityTable, caller))
	if gotSource != callerEdges {
		t.Errorf("source-anchored filter = %d, want %d — check the row group count", gotSource, callerEdges)
	}
}

// THE INVARIANT THAT COST THE MOST TO LEARN: one row group per file.
//
// pqarrow makes this easy to get wrong — every FileWriter.Write starts a NEW row group, so
// writing in batches produced 49 row groups where the reference tool produces 1, and
// parquet.WithMaxRowGroupLength does not merge them. A multi-row-group file mounts, counts
// correctly through an anonymous pattern, and then silently fails to resolve a node the moment
// a pattern binds one. Nothing errors.
func TestIcebugWritesOneRowGroupPerFile(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

	entries, err := os.ReadDir(out)
	if err != nil {
		t.Fatal(err)
	}
	var checked int
	for _, e := range entries {
		if !strings.HasSuffix(e.Name(), ".parquet") {
			continue
		}
		rdr, openErr := file.OpenParquetFile(filepath.Join(out, e.Name()), false)
		if openErr != nil {
			t.Errorf("%s: %v", e.Name(), openErr)
			continue
		}
		groups := rdr.MetaData().NumRowGroups()
		kv := rdr.MetaData().KeyValueMetadata()
		rdr.Close()

		if groups != 1 {
			t.Errorf("%s has %d row groups, want exactly 1", e.Name(), groups)
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
}
