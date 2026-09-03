package ladybugstore

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/apache/arrow-go/v18/arrow"
)

type synthRel struct {
	name     string
	edges    []csrEdge
	props    int
	strProps int
}

func edgesFrom(base uint64, count int, n uint64) []csrEdge {
	edges := make([]csrEdge, 0, count)
	for i := 0; i < count; i++ {
		edges = append(edges, csrEdge{source: base + uint64(i), target: uint64(i) % n})
	}
	return edges
}

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

func TestIcebugAlternativesBoundIsTheFirstTable(t *testing.T) {
	const nodes = 60000

	type tbl struct {
		name string
		rows int
	}
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
			if predicted == want && got != want {
				t.Errorf("%v: largest-first must be exact, got %d want %d", names, got, want)
			}
		})
	}
}

func TestIcebugAlternativesKeepEdgeIdentity(t *testing.T) {
	const nodes = 60000
	const bigRows, smallRows = 2000, 500
	const smallBase = 50000

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

func TestIcebugEveryPairOfTypesSumsExactly(t *testing.T) {
	src := openRealStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

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

func TestIcebugPairsSumWithPerTableStorage(t *testing.T) {
	src := openRealStore(t)

	flat := t.TempDir()
	man, err := ExportIcebug(src, flat, IcebugOptions{StorageURI: flat})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

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

	for _, typ := range []string{"CALLS", "CONTAINS", "WRITES_FIELD"} {
		want := scalar(t, src, fmt.Sprintf(
			"MATCH (a)-[r:%s]->(b) WHERE b.name = '%s' RETURN count(*) AS c", typ, name))
		if got := filtered(typ); got != want {
			t.Errorf("[:%s] anchored on %q = %d, want %d — a single type must stay exact", typ, name, got, want)
		}
	}
	wantCalls := scalar(t, src, fmt.Sprintf(
		"MATCH (a)-[r:CALLS]->(b) WHERE b.name = '%s' RETURN count(*) AS c", name))

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
