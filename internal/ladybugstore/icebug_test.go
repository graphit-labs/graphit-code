package ladybugstore

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// buildHeterogeneousStore creates a graph with the properties that break icebug's
// one-CSR-per-table model, so every test here runs against the real difficulty:
//
//   - a relationship type spanning SEVERAL FROM/TO pairs (CONTAINS goes File->Function and
//     File->Comment), which is what this project's graph does 62 times over;
//   - a self-loop (a recursive call), which the reference implementation's own spec claims
//     is dropped;
//   - two labels with DIFFERENT primary keys, and a property only one label has, because
//     folding the labels into one table has to survive both.
func buildHeterogeneousStore(t *testing.T) (*Store, string) {
	t.Helper()

	path := filepath.Join(t.TempDir(), "src")
	st, err := Open(path)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })

	ddl := []string{
		"CREATE NODE TABLE File(path STRING, name STRING, lang STRING, PRIMARY KEY(path))",
		"CREATE NODE TABLE Function(uid STRING, name STRING, line_number INT64, is_exported BOOL, PRIMARY KEY(uid))",
		"CREATE NODE TABLE Comment(uid STRING, name STRING, PRIMARY KEY(uid))",
		"CREATE REL TABLE CONTAINS(FROM File TO Function, FROM File TO Comment)",
		"CREATE REL TABLE CALLS(FROM Function TO Function, line_number INT64)",
	}
	for _, stmt := range ddl {
		if err := st.Exec(stmt, nil); err != nil {
			t.Fatalf("ddl %q: %v", stmt, err)
		}
	}

	data := []string{
		`CREATE (:File {path: 'a.go', name: 'a.go', lang: 'go'})`,
		`CREATE (:File {path: 'b.go', name: 'b.go', lang: 'go'})`,
		`CREATE (:Function {uid: 'fn1', name: 'main', line_number: 10, is_exported: true})`,
		`CREATE (:Function {uid: 'fn2', name: 'recurse', line_number: 20, is_exported: false})`,
		`CREATE (:Function {uid: 'fn3', name: 'helper', line_number: 30, is_exported: false})`,
		`CREATE (:Comment {uid: 'c1', name: '// licence'})`,
		// CONTAINS across two different pairs
		`MATCH (f:File {path: 'a.go'}), (n:Function {uid: 'fn1'}) CREATE (f)-[:CONTAINS]->(n)`,
		`MATCH (f:File {path: 'a.go'}), (n:Function {uid: 'fn2'}) CREATE (f)-[:CONTAINS]->(n)`,
		`MATCH (f:File {path: 'b.go'}), (n:Function {uid: 'fn3'}) CREATE (f)-[:CONTAINS]->(n)`,
		`MATCH (f:File {path: 'a.go'}), (c:Comment {uid: 'c1'}) CREATE (f)-[:CONTAINS]->(c)`,
		// CALLS, including the self-loop of a recursive function
		`MATCH (a:Function {uid: 'fn1'}), (b:Function {uid: 'fn2'}) CREATE (a)-[:CALLS {line_number: 11}]->(b)`,
		`MATCH (a:Function {uid: 'fn2'}), (b:Function {uid: 'fn2'}) CREATE (a)-[:CALLS {line_number: 21}]->(b)`,
		`MATCH (a:Function {uid: 'fn2'}), (b:Function {uid: 'fn3'}) CREATE (a)-[:CALLS {line_number: 22}]->(b)`,
	}
	for _, stmt := range data {
		if err := st.Exec(stmt, nil); err != nil {
			t.Fatalf("data %q: %v", stmt, err)
		}
	}
	return st, path
}

// mountIcebug opens a fresh store and runs the exported schema against it.
func mountIcebug(t *testing.T, dir string) *Store {
	t.Helper()

	raw, err := os.ReadFile(filepath.Join(dir, "schema.cypher"))
	if err != nil {
		t.Fatalf("reading schema: %v", err)
	}

	st, err := Open(filepath.Join(t.TempDir(), "mounted"))
	if err != nil {
		t.Fatalf("open mounted: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })

	for _, stmt := range strings.Split(string(raw), ";") {
		stmt = strings.TrimSpace(stmt)
		if stmt == "" {
			continue
		}
		if err := st.Exec(stmt, nil); err != nil {
			t.Fatalf("mounting failed:\n%s\n%v", stmt, err)
		}
	}
	return st
}

func scalar(t *testing.T, st *Store, query string) int64 {
	t.Helper()
	rows, err := st.Query(query, nil)
	if err != nil {
		t.Fatalf("%s -> %v", query, err)
	}
	if len(rows) == 0 {
		return 0
	}
	for _, v := range rows[0] {
		return Int64(v)
	}
	return 0
}

// labelCount is how a reader counts nodes of a label on the folded table.
func labelCount(t *testing.T, st *Store, label string) int64 {
	t.Helper()
	return scalar(t, st, fmt.Sprintf(
		"MATCH (n:%s) WHERE n.%s = '%s' RETURN count(n) AS c",
		IcebugEntityTable, IcebugLabelColumn, label))
}

// THE TEST THAT MATTERS: every node and every edge survives the round trip, per label and per
// relationship type. A count that drops is data loss, which is the one thing this may not do.
func TestIcebugRoundTripLosesNothing(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	for _, label := range []string{"File", "Function", "Comment"} {
		want := scalar(t, src, fmt.Sprintf("MATCH (n:%s) RETURN count(n) AS c", label))
		if want == 0 {
			t.Errorf("%s: fixture produced no nodes, so the test proves nothing", label)
		}
		if got := labelCount(t, mounted, label); want != got {
			t.Errorf("%s: source has %d nodes, folded table has %d", label, want, got)
		}
	}
	if total := scalar(t, mounted,
		fmt.Sprintf("MATCH (n:%s) RETURN count(n) AS c", IcebugEntityTable)); total != man.NodeCount {
		t.Errorf("folded table has %d nodes, manifest says %d", total, man.NodeCount)
	}

	// A relationship type is ONE table now, so this is the same query on both sides — no
	// alternatives, which is the entire point of folding.
	for _, relType := range []string{"CONTAINS", "CALLS"} {
		q := fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", relType)
		want := scalar(t, src, q)
		if got := scalar(t, mounted, q); want != got {
			t.Errorf("%s: source has %d edges, mounted has %d", relType, want, got)
		}
	}
	if man.EdgeCount != scalar(t, src, "MATCH ()-[r]->() RETURN count(r) AS c") {
		t.Errorf("manifest counts %d edges, source has a different total", man.EdgeCount)
	}
}

// The invariant the format requires: every relationship table declares exactly ONE FROM/TO
// pair. That is what folding buys, and it is what makes the CSR correct.
func TestIcebugSchemaHasOneNodeTableAndSinglePairRelTables(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

	raw, err := os.ReadFile(filepath.Join(out, "schema.cypher"))
	if err != nil {
		t.Fatal(err)
	}
	schema := string(raw)

	if n := strings.Count(schema, "CREATE NODE TABLE"); n != 1 {
		t.Errorf("schema declares %d node tables, want exactly 1:\n%s", n, schema)
	}
	if n := strings.Count(schema, "CREATE REL TABLE"); n != len(man.Rels) {
		t.Errorf("schema declares %d rel tables, manifest has %d", n, len(man.Rels))
	}
	for _, line := range strings.Split(schema, "\n") {
		if !strings.Contains(line, "CREATE REL TABLE") {
			continue
		}
		if n := strings.Count(line, "FROM "); n != 1 {
			t.Errorf("rel table declares %d FROM/TO pairs, want exactly 1:\n%s", n, line)
		}
		if !strings.Contains(line, QuoteIdent(IcebugEntityTable)) {
			t.Errorf("rel table does not point at the entity table:\n%s", line)
		}
	}

	// CONTAINS spanned two pairs in the source; folding must keep it ONE table while
	// recording both pairs for a rebuild.
	var contains *IcebugRelTable
	for i := range man.Rels {
		if man.Rels[i].Type == "CONTAINS" {
			contains = &man.Rels[i]
		}
	}
	if contains == nil {
		t.Fatal("CONTAINS missing from the manifest")
	}
	if len(contains.Pairs) != 2 {
		t.Errorf("CONTAINS records %d pairs, want 2 (File->Function, File->Comment)", len(contains.Pairs))
	}
}

// The recursive call is a real edge. The reference implementation's spec says self-loops are
// excluded; this export keeps them, and the mounted graph must agree.
func TestIcebugKeepsSelfLoops(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	got := scalar(t, mounted, fmt.Sprintf(
		"MATCH (a:%s)-[r:CALLS]->(b:%s) WHERE a.uid >= b.uid AND a.uid <= b.uid RETURN count(r) AS c",
		IcebugEntityTable, IcebugEntityTable))
	if got != 1 {
		t.Fatalf("self-loop count = %d, want 1 — the recursive call was dropped", got)
	}
}

// Edge properties travel, with their values attached to the right edge — which is what the
// cross-pair sort has to preserve.
func TestIcebugPreservesEdgeProperties(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	rows, err := mounted.Query(fmt.Sprintf(
		"MATCH (a:%s)-[r:CALLS]->(b:%s) RETURN a.uid AS from, b.uid AS to, r.line_number AS line ORDER BY line",
		IcebugEntityTable, IcebugEntityTable), nil)
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	want := []struct {
		from, to string
		line     int64
	}{
		{"fn1", "fn2", 11},
		{"fn2", "fn2", 21},
		{"fn2", "fn3", 22},
	}
	if len(rows) != len(want) {
		t.Fatalf("got %d CALLS rows, want %d: %v", len(rows), len(want), rows)
	}
	for i, w := range want {
		if Str(rows[i]["from"]) != w.from || Str(rows[i]["to"]) != w.to || Int64(rows[i]["line"]) != w.line {
			t.Errorf("row %d = %v %v %v; want %s %s %d", i,
				rows[i]["from"], rows[i]["to"], rows[i]["line"], w.from, w.to, w.line)
		}
	}
}

// Folding is only lossless if a property belonging to ONE label survives, if a label's
// original primary key survives as an ordinary column, and if a column a label lacks is null
// rather than invented.
func TestIcebugFoldingKeepsEveryLabelsColumns(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	names := map[string]bool{}
	for _, f := range man.Columns {
		names[f.Name] = true
	}
	for _, required := range []string{
		IcebugIDColumn, IcebugLabelColumn, "path", "uid", "name", "lang", "line_number", "is_exported",
	} {
		if !names[required] {
			t.Errorf("folded table is missing column %q", required)
		}
	}

	for label, key := range map[string]string{"File": "path", "Function": "uid", "Comment": "uid"} {
		if man.LabelKeys[label] != key {
			t.Errorf("LabelKeys[%s] = %q, want %q", label, man.LabelKeys[label], key)
		}
	}

	rows, err := mounted.Query(fmt.Sprintf(
		"MATCH (n:%s) WHERE n.%s = 'File' RETURN n.path AS p, n.lang AS l ORDER BY p",
		IcebugEntityTable, IcebugLabelColumn), nil)
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	if len(rows) != 2 || Str(rows[0]["p"]) != "a.go" || Str(rows[0]["l"]) != "go" {
		t.Fatalf("File rows = %v", rows)
	}

	fn, err := mounted.Query(fmt.Sprintf(
		"MATCH (n:%s) WHERE n.uid IN ['fn2'] RETURN n.name AS nm, n.line_number AS ln, n.is_exported AS ex",
		IcebugEntityTable), nil)
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	if len(fn) != 1 || Str(fn[0]["nm"]) != "recurse" || Int64(fn[0]["ln"]) != 20 {
		t.Fatalf("Function fn2 = %v", fn)
	}
	if b, ok := fn[0]["ex"].(bool); !ok || b {
		t.Errorf("is_exported = %v, want false — a BOOL column did not survive folding", fn[0]["ex"])
	}

	nulls := scalar(t, mounted, fmt.Sprintf(
		"MATCH (n:%s) WHERE n.%s = 'Comment' AND n.lang IS NULL RETURN count(n) AS c",
		IcebugEntityTable, IcebugLabelColumn))
	if nulls != 1 {
		t.Errorf("Comment nodes with a null lang = %d, want 1", nulls)
	}
}

// A variable-length path is native again, because a relationship type is one table with one
// CSR. This is the shape the framework's impact queries use, and it is exactly what the
// partitioned layout could not express correctly.
func TestIcebugVariableLengthTraversalIsNative(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	// fn1 -> fn2 -> fn3, so exactly two functions are reachable from fn1 within three hops.
	got := scalar(t, mounted, fmt.Sprintf(
		"MATCH (a:%s)-[:CALLS*1..3]->(b:%s) WHERE a.uid IN ['fn1'] RETURN count(DISTINCT b.uid) AS c",
		IcebugEntityTable, IcebugEntityTable))
	if got != 2 {
		t.Fatalf("reachable from fn1 within 3 hops = %d, want 2", got)
	}

	// The same question on the source, so the answer is the graph's and not the layout's.
	srcGot := scalar(t, src,
		"MATCH (a:Function)-[:CALLS*1..3]->(b:Function) WHERE a.uid = 'fn1' RETURN count(DISTINCT b.uid) AS c")
	if srcGot != got {
		t.Fatalf("source says %d reachable, mounted says %d", srcGot, got)
	}
}

// A relationship table IS its type — no partitioned name to normalise, which is what makes
// type(r) need no rewriting at the layer above.
//
// Asserted structurally rather than with type(r): that function belongs to the translated
// layer (internal/ast), not to the raw store, where it does not bind.
func TestIcebugRelationshipTableIsItsOwnType(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

	for _, r := range man.Rels {
		wantTable := r.Type
		if r.Reverse {
			wantTable += IcebugReverseSuffix
		}
		if r.Table != wantTable {
			t.Errorf("table %q, want %q for type %q", r.Table, wantTable, r.Type)
		}
		if strings.Contains(r.Table, "__") {
			t.Errorf("table %q looks partitioned; folding should leave one table per type", r.Table)
		}
	}

	raw, err := os.ReadFile(filepath.Join(out, "schema.cypher"))
	if err != nil {
		t.Fatal(err)
	}
	for _, typ := range []string{"CONTAINS", "CALLS"} {
		if !strings.Contains(string(raw), "CREATE REL TABLE "+QuoteIdent(typ)+"(") {
			t.Errorf("schema does not declare a table named %s:\n%s", typ, raw)
		}
	}
}

// Reverse edges go into a SEPARATE table, so the forward answer stays exact.
//
// The reference tool merges the mirror into the type's own table, and measured, that destroys
// direction: 200.000 edges mount as 399.996. In a code graph the direction of CALLS is the
// meaning, so this asserts the forward table is untouched.
func TestIcebugReverseEdgesGoToTheirOwnTable(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	// FORWARD is exactly what the source has — the mirror did not leak into it.
	for _, relType := range []string{"CALLS", "CONTAINS"} {
		q := fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", relType)
		want := scalar(t, src, q)
		if got := scalar(t, mounted, q); got != want {
			t.Errorf("%s forward = %d, want %d — the mirror leaked into the forward table",
				relType, got, want)
		}
	}

	// The mirror exists, in its own table, and does not count as graph edges.
	var forward, reverse *IcebugRelTable
	for i := range man.Rels {
		switch man.Rels[i].Table {
		case "CALLS":
			forward = &man.Rels[i]
		case "CALLS" + IcebugReverseSuffix:
			reverse = &man.Rels[i]
		}
	}
	if forward == nil || reverse == nil {
		t.Fatalf("expected CALLS and CALLS%s; got %+v", IcebugReverseSuffix, man.Rels)
	}
	if !reverse.Reverse || forward.Reverse {
		t.Errorf("the reverse flag is wrong: forward=%v reverse=%v", forward.Reverse, reverse.Reverse)
	}
	// CALLS has 3 edges, one a self-loop, so the mirror has 2: a self-loop's mirror is itself
	// and would duplicate an edge the forward table already holds.
	if reverse.Rows != 2 {
		t.Errorf("CALLS%s has %d rows, want 2 (the self-loop is not mirrored)",
			IcebugReverseSuffix, reverse.Rows)
	}
	if man.EdgeCount != scalar(t, src, "MATCH ()-[r]->() RETURN count(r) AS c") {
		t.Errorf("edge count %d counts the mirror; it must not", man.EdgeCount)
	}

	// The mirror answers an inbound question with a forward traversal, which is its purpose.
	got := scalar(t, mounted, fmt.Sprintf(
		"MATCH (a:%s)-[r:CALLS%s]->(b:%s) WHERE a.uid IN ['fn2'] RETURN count(*) AS c",
		IcebugEntityTable, IcebugReverseSuffix, IcebugEntityTable))
	// fn1 and fn2 both call fn2; the self-loop is not mirrored, so fn2 has one inbound mirror.
	if got != 1 {
		t.Errorf("inbound via the mirror = %d, want 1", got)
	}
	if !man.Reverse {
		t.Error("manifest does not record that reverse edges were added")
	}

	for _, rel := range man.Rels {
		if rel.Table != "CONTAINS"+IcebugReverseSuffix {
			continue
		}
		for _, pair := range rel.Pairs {
			if pair.To != "File" || pair.From == "File" {
				t.Errorf("reverse CONTAINS pair was not swapped: %+v", pair)
			}
			if pair.Rows == 0 {
				t.Errorf("reverse CONTAINS pair records no materialized rows: %+v", pair)
			}
		}
	}
}

func TestIcebugReverseEdgesCanBeDisabled(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	man, err := ExportIcebug(src, t.TempDir(), IcebugOptions{
		StorageURI:          t.TempDir(),
		DisableReverseEdges: true,
	})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	if man.Reverse {
		t.Error("manifest marks reverse edges present after explicit opt-out")
	}
	for _, rel := range man.Rels {
		if rel.Reverse || strings.HasSuffix(rel.Table, IcebugReverseSuffix) {
			t.Errorf("explicit opt-out still exported reverse table %+v", rel)
		}
	}
}

// indptr must cover every source node, including one with no outgoing edge of that type. With
// the folded table every node is a possible source, so most have degree zero for most types.
func TestIcebugIndptrCoversEverySourceNode(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	if got := scalar(t, mounted, fmt.Sprintf(
		"MATCH (a:%s)-[r:CALLS]->() WHERE a.uid IN ['fn3'] RETURN count(r) AS c", IcebugEntityTable)); got != 0 {
		t.Errorf("fn3 has %d outgoing CALLS, want 0", got)
	}
	if got := scalar(t, mounted, "MATCH ()-[r:CALLS]->() RETURN count(r) AS c"); got != 3 {
		t.Errorf("CALLS total = %d, want 3", got)
	}
	for _, r := range man.Rels {
		if r.Type == "CALLS" && !r.Reverse && r.Rows != 3 {
			t.Errorf("manifest records %d rows for CALLS, want 3", r.Rows)
		}
	}
}

func TestIcebugManifestMarksAFinishedExport(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	if HasIcebug(out) {
		t.Fatal("an empty directory reports a finished export")
	}
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	if !HasIcebug(out) {
		t.Fatal("a finished export is not detected")
	}
}

// A rebuild has to restore the schema, not only read the data: the manifest must name every
// label, its original key, every column and every pair.
func TestIcebugManifestDescribesEverythingNeededToRebuild(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}

	if man.EntityTable != IcebugEntityTable || man.LabelColumn != IcebugLabelColumn || man.IDColumn != IcebugIDColumn {
		t.Errorf("manifest does not describe the folded layout: %+v", man)
	}
	if len(man.Labels) != 3 {
		t.Errorf("manifest records %d labels, want 3", len(man.Labels))
	}
	var labelled int64
	for _, l := range man.Labels {
		if l.Rows == 0 {
			t.Errorf("label %s recorded with zero rows", l.Label)
		}
		labelled += l.Rows
	}
	if labelled != man.NodeCount {
		t.Errorf("label rows sum to %d, node count is %d", labelled, man.NodeCount)
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
		for _, f := range []string{r.IndicesRel, r.IndptrRel} {
			if _, err := os.Stat(filepath.Join(out, f)); err != nil {
				t.Errorf("%s names a file that is not there: %v", r.Table, err)
			}
		}
	}
	if man.Version != icebugVersion {
		t.Errorf("manifest version = %q, want %q", man.Version, icebugVersion)
	}
}

// A label declaring a column the folded table reserves must be refused, not silently
// overwritten — that would replace a real property with the synthetic one.
func TestIcebugRefusesAReservedColumnName(t *testing.T) {
	st, err := Open(filepath.Join(t.TempDir(), "reserved"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })

	if err := st.Exec(fmt.Sprintf(
		"CREATE NODE TABLE Bad(%s STRING, other STRING, PRIMARY KEY(%s))",
		IcebugLabelColumn, IcebugLabelColumn), nil); err != nil {
		t.Fatalf("ddl: %v", err)
	}
	if err := st.Exec(fmt.Sprintf("CREATE (:Bad {%s: 'x', other: 'y'})", IcebugLabelColumn), nil); err != nil {
		t.Fatalf("data: %v", err)
	}

	if _, err := ExportIcebug(st, t.TempDir(), IcebugOptions{StorageURI: "s3://b/k"}); err == nil {
		t.Fatal("export accepted a label whose column collides with the reserved one")
	} else if !strings.Contains(err.Error(), "reserves") {
		t.Fatalf("error should say the name is reserved: %v", err)
	}
}

func TestCypherTypeMapping(t *testing.T) {
	cases := map[string]string{
		"BIGINT": "INT64", "INTEGER": "INT32", "VARCHAR": "STRING", "BOOLEAN": "BOOL",
		"REAL": "FLOAT", "DECIMAL(10,2)": "STRING", "STRING": "STRING", "INT64": "INT64",
		"something-unknown": "STRING",
	}
	for in, want := range cases {
		if got := cypherType(in); got != want {
			t.Errorf("cypherType(%q) = %q, want %q", in, got, want)
		}
	}
}

// MEASURED LIMITATION OF THE READER, and the exact workaround.
//
// On an icebug-mounted table, `=` against the PRIMARY KEY column returns nothing: the engine
// routes the predicate through a primary-key index that icebug storage does not provide, and
// answers empty instead of scanning. It does not error — it is silently wrong.
//
// Everything else on the same column works, INCLUDING `IN [value]`, which is semantically
// identical for a single value. That is the rewrite a reader applies, and it loses nothing.
//
// Folding improves this: the key is now _id, which no user query mentions, so it bites a
// reader doing an id lookup rather than a query on path or uid.
func TestIcebugPrimaryKeyEqualityNeedsIN(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	if _, err := ExportIcebug(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("ExportIcebug: %v", err)
	}
	mounted := mountIcebug(t, out)

	if n := scalar(t, mounted, fmt.Sprintf(
		"MATCH (n:%s) WHERE n.%s = 0 RETURN count(n) AS c", IcebugEntityTable, IcebugIDColumn)); n != 0 {
		t.Logf("primary-key equality now returns %d — the engine gained the index, and the "+
			"rewrite could be retired", n)
	}
	if n := scalar(t, mounted, fmt.Sprintf(
		"MATCH (n:%s) WHERE n.%s IN [0] RETURN count(n) AS c", IcebugEntityTable, IcebugIDColumn)); n != 1 {
		t.Errorf("IN on the key -> %d, want 1", n)
	}

	// A non-key column is unaffected, which is what every real query uses.
	if n := scalar(t, mounted, fmt.Sprintf(
		"MATCH (n:%s) WHERE n.name = 'recurse' RETURN count(n) AS c", IcebugEntityTable)); n != 1 {
		t.Errorf("equality on a non-key column -> %d, want 1", n)
	}
}
