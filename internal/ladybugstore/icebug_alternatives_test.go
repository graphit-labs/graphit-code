package ladybugstore

// The type-alternatives form `[:A|B|…]` on icebug storage, and the two DISTINCT engine defects
// in it. Both are silent — neither ever raises an error.
//
// DEFECT ONE — the count is bounded by the FIRST-CREATED alternative. The engine truncates every
// alternative's scan to the row count of the alternative with the lowest table id, which is
// creation order. Query order is irrelevant.
//
//	created 54.823 then 92.396 -> 109.646   (= 2x54.823; the second is truncated)
//	created 92.396 then 54.823 -> 147.219   (exact; there is nothing to truncate)
//
// FIXED HERE, by ordering: writeIcebugSchema creates the relationship tables largest first, so the
// lowest-id member of ANY subset is also the largest member of that subset and no alternative is
// ever truncated. See sortRelsLargestFirst. When the largest is first the edges are exact in
// identity too, not merely in count — TestIcebugAlternativesKeepEdgeIdentity.
//
// DEFECT TWO — with a filter on a bound endpoint, the alternatives are matched against the wrong
// node set, and ordering does NOT help. NOT fixable here.
//
// BOTH are reproduced on output produced by the reference `icebug-format` tool, so both are
// UPSTREAM — see icebug_upstream_test.go. An earlier round attributed defect one to this writer
// because the single comparison run against the tool happened to create the bigger table first,
// which is exactly the case that cannot fail.

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/apache/arrow-go/v18/arrow"
)

// --- the controlled harness -------------------------------------------------------------------
//
// The real graph varies row count, property shape and degree distribution across eight tables at
// once, so nothing can be isolated from it. This writes an icebug directory with each of those
// set independently, using the SAME primitives the real export uses.

// synthRel is one relationship table to write.
type synthRel struct {
	name string
	// edges is the CSR, already sorted by (source, target).
	edges []csrEdge
	// props is how many INT64 property columns the indices file carries.
	props int
	// strProps is how many STRING property columns it carries, after the INT64 ones.
	strProps int
}

// edgesFrom puts count edges on consecutive source nodes starting at base, one each — so a
// table's sources occupy a known id range and cannot be confused with another table's.
func edgesFrom(base uint64, count int, n uint64) []csrEdge {
	edges := make([]csrEdge, 0, count)
	for i := 0; i < count; i++ {
		edges = append(edges, csrEdge{source: base + uint64(i), target: uint64(i) % n})
	}
	return edges
}

// writeSynthGraph writes a mountable icebug directory: one folded node table and the given
// relationship tables, created in the order given.
func writeSynthGraph(t *testing.T, dir string, nodes uint64, rels []synthRel) {
	t.Helper()

	g := &foldedGraph{
		columns: []Field{{Name: IcebugIDColumn, Type: "INT64"}, {Name: IcebugLabelColumn, Type: "STRING"}},
		keys:    map[string]string{},
		ids:     map[string]uint64{},
		count:   nodes,
	}
	g.rows = make([]map[string]any, 0, nodes)
	for i := uint64(0); i < nodes; i++ {
		g.rows = append(g.rows, map[string]any{IcebugIDColumn: int64(i), IcebugLabelColumn: "N"})
	}
	if err := writeEntityTable(filepath.Join(dir, "nodes_"+IcebugEntityTable+".parquet"), g); err != nil {
		t.Fatalf("entity table: %v", err)
	}

	var b strings.Builder
	fmt.Fprintf(&b, "CREATE NODE TABLE %s(%s INT64, %s STRING, PRIMARY KEY(%s)) WITH (storage = '%s', format = 'icebug-disk');\n",
		IcebugEntityTable, IcebugIDColumn, IcebugLabelColumn, IcebugIDColumn, EscapeLiteral(dir))

	for _, r := range rels {
		var propFields []arrow.Field
		var propValues [][]any
		var declared []string

		addProp := func(name string, typ arrow.DataType, cypher string, value func(int) any) {
			propFields = append(propFields, arrow.Field{Name: name, Type: typ, Nullable: true})
			vals := make([]any, len(r.edges))
			for i := range r.edges {
				vals[i] = value(i)
			}
			propValues = append(propValues, vals)
			declared = append(declared, name+" "+cypher)
		}
		for p := 0; p < r.props; p++ {
			addProp(fmt.Sprintf("p%d", p), arrow.PrimitiveTypes.Int64, "INT64",
				func(i int) any { return int64(i) })
		}
		for p := 0; p < r.strProps; p++ {
			addProp(fmt.Sprintf("s%d", p), arrow.BinaryTypes.String, "STRING",
				func(i int) any { return fmt.Sprintf("%s/value/%d", r.name, i) })
		}

		if err := writeIndices(filepath.Join(dir, "indices_"+r.name+".parquet"),
			r.edges, propFields, propValues); err != nil {
			t.Fatalf("indices %s: %v", r.name, err)
		}
		if err := writeIndptr(filepath.Join(dir, "indptr_"+r.name+".parquet"), r.edges, nodes); err != nil {
			t.Fatalf("indptr %s: %v", r.name, err)
		}

		parts := append([]string{fmt.Sprintf("FROM %s TO %s", IcebugEntityTable, IcebugEntityTable)}, declared...)
		fmt.Fprintf(&b, "CREATE REL TABLE %s(%s) WITH (storage = '%s', format = 'icebug-disk');\n",
			r.name, strings.Join(parts, ", "), EscapeLiteral(dir))
	}

	if err := os.WriteFile(filepath.Join(dir, "schema.cypher"), []byte(b.String()), 0o644); err != nil {
		t.Fatal(err)
	}
}

func min64(a, b int64) int64 {
	if a < b {
		return a
	}
	return b
}

// --- the rule, on synthetic files ------------------------------------------------------------

// TestIcebugAlternativesBoundIsTheFirstTable pins the rule the export's table ordering relies on.
//
// Two candidate bounds fit two-table evidence equally well — the FIRST alternative's row count, or
// the MINIMUM over the alternatives. With three tables of different sizes they differ, and it is
// the first alternative's count. That is what makes "declare largest first" a complete fix rather
// than a partial one: with the minimum bound, ordering could not have saved it.
//
// Each case is a subtest so its mounted store closes before the next opens one — MEASURED, opening
// dozens of stores in one process fails with "failed to open database with status 1".
func TestIcebugAlternativesBoundIsTheFirstTable(t *testing.T) {
	const nodes = 60000

	type tbl struct {
		name string
		rows int
	}
	// The two-table cases use the real HAS_FIELD and CALLS row counts, a pair that answers
	// correctly on the real export precisely because CALLS is declared first there.
	cases := [][]tbl{
		{{"T1", 2957}, {"T2", 55040}},
		{{"T1", 55040}, {"T2", 2957}},
		{{"T1", 4096}, {"T2", 4096}},
		{{"T1", 100}, {"T2", 1000}, {"T3", 50}},
		{{"T1", 1000}, {"T2", 100}, {"T3", 50}},
		{{"T1", 50}, {"T2", 100}, {"T3", 1000}},
	}

	for _, order := range cases {
		names := make([]string, 0, len(order))
		for _, x := range order {
			names = append(names, fmt.Sprintf("%s=%d", x.name, x.rows))
		}
		t.Run(strings.Join(names, "_"), func(t *testing.T) {
			rels := make([]synthRel, 0, len(order))
			alts := make([]string, 0, len(order))
			base := uint64(0)
			var want, predicted int64
			for _, x := range order {
				rels = append(rels, synthRel{name: x.name, edges: edgesFrom(base, x.rows, nodes)})
				alts = append(alts, x.name)
				base += 10000
				want += int64(x.rows)
				predicted += min64(int64(x.rows), int64(order[0].rows))
			}

			dir := t.TempDir()
			writeSynthGraph(t, dir, nodes, rels)
			st := mountIcebug(t, dir)

			// Each table alone must be exact, or the fixture is what is broken.
			for _, x := range order {
				if got := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", x.name)); got != int64(x.rows) {
					t.Fatalf("[:%s] alone = %d, want %d", x.name, got, x.rows)
				}
			}

			got := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", strings.Join(alts, "|")))
			t.Logf("%v: got=%d exact=%d first-alternative bound predicts %d", names, got, want, predicted)

			if got != predicted {
				t.Errorf("%v: [:A|B|…] = %d; the first-alternative bound predicts %d (exact sum %d). "+
					"The rule sortRelsLargestFirst relies on has changed — re-measure before trusting the order",
					names, got, predicted, want)
			}
			// Largest first is the case the export produces, and it must be exact.
			if predicted == want && got != want {
				t.Errorf("%v: largest-first must be exact, got %d want %d", names, got, want)
			}
		})
	}
}

// TestIcebugAlternativesKeepEdgeIdentity checks the EDGES, not the count.
//
// A count that matches never proved the alternatives were read as themselves — the oldest trap in
// this export. So the two tables' source nodes are put in disjoint id ranges and the pattern is
// asked which sources it reports. With the larger table declared first, both the total and the
// per-range breakdown are exact.
//
// (`count(<property>)` is NOT a way to tell the alternatives apart here: MEASURED, count(r.line_number)
// equals count(r) even for a table with no such column, so it does not skip nulls. An earlier round
// inferred "every row came from CALLS" from that, and the premise was never valid.)
func TestIcebugAlternativesKeepEdgeIdentity(t *testing.T) {
	const nodes = 60000
	const bigRows, smallRows = 2000, 500
	const smallBase = 50000

	// Largest first, which is the order the export writes.
	a := synthRel{name: "A", edges: edgesFrom(0, bigRows, nodes)}
	b := synthRel{name: "B", edges: edgesFrom(smallBase, smallRows, nodes)}

	dir := t.TempDir()
	writeSynthGraph(t, dir, nodes, []synthRel{a, b})
	st := mountIcebug(t, dir)

	if got := scalar(t, st, "MATCH ()-[r:A|B]->() RETURN count(r) AS c"); got != bigRows+smallRows {
		t.Fatalf("count over [:A|B] = %d, want %d", got, bigRows+smallRows)
	}

	sources := func(pattern, predicate string) int64 {
		return scalar(t, st, fmt.Sprintf("MATCH (x:%s)-[r:%s]->() WHERE x.%s %s RETURN count(*) AS c",
			IcebugEntityTable, pattern, IcebugIDColumn, predicate))
	}

	// Baseline: each table alone reports its own source range.
	if got := sources("B", fmt.Sprintf(">= %d", smallBase)); got != smallRows {
		t.Fatalf("[:B] alone reports %d edges from sources >= %d, want %d — the fixture is wrong",
			got, smallBase, smallRows)
	}

	high := sources("A|B", fmt.Sprintf(">= %d", smallBase))
	low := sources("A|B", fmt.Sprintf("< %d", bigRows))
	t.Logf("over [:A|B]: %d edges from sources >= %d (want %d), %d from sources < %d (want %d)",
		high, smallBase, smallRows, low, bigRows, bigRows)

	if high != smallRows || low != bigRows {
		t.Errorf("the count is right and the edges are not: sources >= %d carry %d edges (want %d) "+
			"and sources < %d carry %d (want %d) — the alternatives are resolving against the wrong rows",
			smallBase, high, smallRows, bigRows, low, bigRows)
	}
}

// --- the rule, on the real graph ---------------------------------------------------------------

// TestIcebugEveryPairOfTypesSumsExactly is the regression guard for the ordering fix.
//
// Before it, 9 of these 28 pairs were silently wrong — every pair whose alphabetically-first table
// was also the smaller one. The other 19 were right only because there was nothing to truncate,
// which is why the defect looked like it belonged to one pair of tables.
//
//	GRAPHIT_REAL_STORE=/tmp/icebug-fix/ladybugdb \
//	  go test -tags lancedb -run TestIcebugEveryPairOfTypesSumsExactly ./internal/ladybugstore/ -v
func TestIcebugEveryPairOfTypesSumsExactly(t *testing.T) {
	src := openRealStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

	// The order in schema.cypher IS the fix. Check it before checking what it buys.
	var prev int64 = 1 << 62
	for _, r := range man.Rels {
		if r.Rows > prev {
			t.Errorf("schema.cypher creates %s (%d rows) after a table of %d rows — relationship "+
				"tables must be created largest first, see sortRelsLargestFirst", r.Table, r.Rows, prev)
		}
		prev = r.Rows
	}

	truth := map[string]int64{}
	types := make([]string, 0, len(man.Rels))
	for _, r := range man.Rels {
		truth[r.Type] = r.Rows
		types = append(types, r.Type)
	}
	sort.Strings(types)

	mounted := mountIcebug(t, out)

	var failures int
	for i, a := range types {
		for _, b := range types[i+1:] {
			got := scalar(t, mounted, fmt.Sprintf(
				"MATCH ()-[r:%s|%s]->() RETURN count(r) AS c", QuoteIdent(a), QuoteIdent(b)))
			if want := truth[a] + truth[b]; got != want {
				failures++
				t.Errorf("[:%s|%s] = %d, want %d (delta %d)", a, b, got, want, got-want)
			}
		}
	}

	// Every type at once — the widest alternatives list the graph can produce.
	alts := make([]string, 0, len(types))
	var wantAll int64
	for _, typ := range types {
		alts = append(alts, QuoteIdent(typ))
		wantAll += truth[typ]
	}
	gotAll := scalar(t, mounted, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", strings.Join(alts, "|")))
	if gotAll != wantAll {
		t.Errorf("all %d types at once = %d, want %d", len(types), gotAll, wantAll)
	}
	t.Logf("%d pairs, %d failing; all %d alternatives at once = %d (want %d)",
		len(types)*(len(types)-1)/2, failures, len(types), gotAll, wantAll)
}

// TestIcebugPairsSumWithPerTableStorage keeps a hypothesis that was ELIMINATED from coming back.
//
// Every relationship table in the export declares the same `storage` directory and is told apart
// by file name, so a shared directory was the obvious suspect for two CSRs being confused. It is
// not: re-laying the same bytes one directory per table left all 9 failures exactly as they were.
// The test stays because per-table prefixes are a plausible S3 layout, and it says that layout
// neither causes the defect nor cures it.
func TestIcebugPairsSumWithPerTableStorage(t *testing.T) {
	src := openRealStore(t)

	flat := t.TempDir()
	man, err := ExportIcebug(src, flat, IcebugOptions{StorageURI: flat})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

	// Re-lay the SAME bytes, one directory per table.
	split := t.TempDir()
	place := func(table, file string) {
		dir := filepath.Join(split, table)
		if mkErr := os.MkdirAll(dir, 0o755); mkErr != nil {
			t.Fatal(mkErr)
		}
		raw, readErr := os.ReadFile(filepath.Join(flat, file))
		if readErr != nil {
			t.Fatalf("read %s: %v", file, readErr)
		}
		if wErr := os.WriteFile(filepath.Join(dir, file), raw, 0o644); wErr != nil {
			t.Fatalf("write %s: %v", file, wErr)
		}
	}
	place(IcebugEntityTable, "nodes_"+IcebugEntityTable+".parquet")

	truth := map[string]int64{}
	types := make([]string, 0, len(man.Rels))
	for _, r := range man.Rels {
		place(r.Table, r.IndicesRel)
		place(r.Table, r.IndptrRel)
		truth[r.Type] = r.Rows
		types = append(types, r.Type)
	}
	sort.Strings(types)

	st, err := Open(filepath.Join(t.TempDir(), "mounted"))
	if err != nil {
		t.Fatalf("open mounted: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })

	raw, err := os.ReadFile(filepath.Join(flat, "schema.cypher"))
	if err != nil {
		t.Fatal(err)
	}
	// The creation ORDER of schema.cypher is preserved; only `storage` is rewritten.
	for _, stmt := range strings.Split(string(raw), ";") {
		stmt = strings.TrimSpace(stmt)
		if stmt == "" {
			continue
		}
		table := ddlTableName(stmt)
		if table == "" {
			t.Fatalf("cannot read a table name out of:\n%s", stmt)
		}
		stmt = strings.Replace(stmt,
			"storage = '"+EscapeLiteral(flat)+"'",
			"storage = '"+EscapeLiteral(filepath.Join(split, table))+"'", 1)
		if execErr := st.Exec(stmt, nil); execErr != nil {
			t.Fatalf("mounting failed:\n%s\n%v", stmt, execErr)
		}
	}

	for _, typ := range types {
		if got := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", QuoteIdent(typ))); got != truth[typ] {
			t.Errorf("[:%s] = %d, want %d — per-table storage broke a single table", typ, got, truth[typ])
		}
	}
	if n := scalar(t, st, fmt.Sprintf("MATCH (n:%s) RETURN count(n) AS c", IcebugEntityTable)); n != man.NodeCount {
		t.Errorf("node count = %d, want %d", n, man.NodeCount)
	}
	for i, a := range types {
		for _, b := range types[i+1:] {
			got := scalar(t, st, fmt.Sprintf(
				"MATCH ()-[r:%s|%s]->() RETURN count(r) AS c", QuoteIdent(a), QuoteIdent(b)))
			if want := truth[a] + truth[b]; got != want {
				t.Errorf("[:%s|%s] with per-table storage = %d, want %d", a, b, got, want)
			}
		}
	}
}

// ddlTableName reads the table name out of a CREATE NODE/REL TABLE statement.
func ddlTableName(stmt string) string {
	for _, prefix := range []string{"CREATE NODE TABLE ", "CREATE REL TABLE "} {
		if !strings.HasPrefix(stmt, prefix) {
			continue
		}
		rest := stmt[len(prefix):]
		i := strings.Index(rest, "(")
		if i < 0 {
			return ""
		}
		return strings.Trim(strings.TrimSpace(rest[:i]), "`\"")
	}
	return ""
}

// TestIcebugAlternativesWithAFilteredEndpointIsWRONG documents DEFECT TWO, which the ordering fix
// does NOT address and which is not fixable here.
//
// Unfiltered, `[:A|B]` is now exact for every pair. Add a filter on a bound endpoint and it is
// wrong again — each alternative is matched against the wrong node set. MEASURED on the real graph,
// filtering on the most-called function:
//
//	[:CALLS]                = 3.769  (exact)
//	[:CONTAINS]             = 0      (exact)
//	[:CONTAINS|CALLS]       = 0      (CONTAINS is created first; CALLS's 3.769 edges vanish)
//	[:CALLS|WRITES_FIELD]   = 3.798  (CALLS is created first; WRITES_FIELD invents 29 edges)
//
// Reproduced on the reference tool's own output, so it is UPSTREAM — see
// TestIcebugFilteredAlternativesDefectOnToolOutput.
//
// The defect is asserted, so the day it is fixed upstream this test FAILS and says what to do. It
// is also the reason relationship-per-pair partitioning stays off the table: that layout turns
// every `MATCH (f:Function)-[:CALLS]->(g) WHERE g.name = …` into exactly this shape.
func TestIcebugAlternativesWithAFilteredEndpointIsWRONG(t *testing.T) {
	src := openRealStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	rows := map[string]int64{}
	for _, r := range man.Rels {
		if r.Reverse {
			// Reverse tables carry Type of the base relationship and would overwrite the
			// direct counts here; their Rows also exclude self-loops on purpose.
			continue
		}
		rows[r.Type] = r.Rows
	}
	mounted := mountIcebug(t, out)

	callee, err := src.Query(
		"MATCH (a)-[r:CALLS]->(b) RETURN b.name AS n, count(r) AS c ORDER BY c DESC LIMIT 1", nil)
	if err != nil || len(callee) == 0 {
		t.Fatalf("picking a callee: %v", err)
	}
	name := Str(callee[0]["n"])
	e := IcebugEntityTable

	filtered := func(pattern string) int64 {
		return scalar(t, mounted, fmt.Sprintf(
			"MATCH (a:%s)-[r:%s]->(b:%s) WHERE b.name = '%s' RETURN count(*) AS c", e, pattern, e, name))
	}

	// A SINGLE type with a filtered endpoint is exact — that is the row-group fix holding, and it
	// is what makes the folded layout usable at all.
	for _, typ := range []string{"CALLS", "CONTAINS", "WRITES_FIELD"} {
		want := scalar(t, src, fmt.Sprintf(
			"MATCH (a)-[r:%s]->(b) WHERE b.name = '%s' RETURN count(*) AS c", typ, name))
		if got := filtered(typ); got != want {
			t.Errorf("[:%s] anchored on %q = %d, want %d — a single type must stay exact", typ, name, got, want)
		}
	}
	wantCalls := scalar(t, src, fmt.Sprintf(
		"MATCH (a)-[r:CALLS]->(b) WHERE b.name = '%s' RETURN count(*) AS c", name))

	// Unfiltered, the pair is exact and must stay exact.
	if got := scalar(t, mounted, "MATCH ()-[r:CALLS|CONTAINS]->() RETURN count(r) AS c"); got != rows["CALLS"]+rows["CONTAINS"] {
		t.Errorf("unfiltered [:CALLS|CONTAINS] = %d, want %d — the ordering fix regressed",
			got, rows["CALLS"]+rows["CONTAINS"])
	}

	pair := filtered("CONTAINS|CALLS")
	over := filtered("CALLS|WRITES_FIELD")
	t.Logf("filtered on %q: [:CALLS]=%d (exact) [:CONTAINS|CALLS]=%d [:CALLS|WRITES_FIELD]=%d",
		name, wantCalls, pair, over)

	if pair == wantCalls && over == wantCalls {
		t.Errorf("FIXED UPSTREAM: alternatives with a filtered endpoint now answer correctly "+
			"([:CONTAINS|CALLS]=%d, [:CALLS|WRITES_FIELD]=%d, both want %d). Delete this test and "+
			"reconsider relationship-per-pair partitioning.", pair, over, wantCalls)
	}
}
