package ladybugstore

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"
)

// TestIcebugDefectsReproduceOnTheReferenceToolOutput confirms, on output produced by the
// REFERENCE `icebug-format` tool and not by this package, that the reader defects are the
// engine's rather than ours.
//
// Every graph here is written by the Python tool. If a defect shows up, our writer cannot be
// the cause.
//
//	GRAPHIT_TOOL_ICEBUG=<dir from `uvx icebug-format --source-dir ...`> \
//	GRAPHIT_TOOL_ICEBUG_MULTI=<dir holding two tool-produced graphs, one per subdirectory> \
//	  go test -run TestIcebugDefectsReproduceOnTheReferenceToolOutput ./internal/ladybugstore/ -v
func TestIcebugDefectsReproduceOnTheReferenceToolOutput(t *testing.T) {
	dir := os.Getenv("GRAPHIT_TOOL_ICEBUG")
	if dir == "" {
		t.Skip("set GRAPHIT_TOOL_ICEBUG to a directory produced by uvx icebug-format")
	}

	st := mountIcebug(t, dir)

	ptr, err := readUint64Column(filepath.Join(dir, "indptr_demo_rel.parquet"), "ptr")
	if err != nil {
		t.Fatalf("reading indptr: %v", err)
	}
	targets, err := readUint64Column(filepath.Join(dir, "indices_demo_rel.parquet"), "target")
	if err != nil {
		t.Fatalf("reading indices: %v", err)
	}
	totalEdges := int64(len(targets))
	t.Logf("tool output: %d nodes, %d edges", len(ptr)-1, totalEdges)

	var probe uint64
	var outDegree int64
	for i := 0; i+1 < len(ptr); i++ {
		if d := int64(ptr[i+1] - ptr[i]); d > outDegree {
			probe, outDegree = uint64(i), d
			if outDegree >= 8 {
				break
			}
		}
	}
	t.Logf("probe node: dense id %d with out-degree %d (from the CSR)", probe, outDegree)

	ask := func(q string) (int64, time.Duration) {
		start := time.Now()
		rows, qErr := st.Query(q, nil)
		took := time.Since(start)
		if qErr != nil {
			t.Logf("    query error: %v", qErr)
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

	report := func(defect, q string, got, want int64, took time.Duration) {
		verdict := "NOT reproduced (this form works)"
		if got != want {
			verdict = "REPRODUCED on tool output"
		}
		t.Logf("[%s] %s\n    got=%d want=%d in %dms -> %s", defect, q, got, want, took.Milliseconds(), verdict)
	}

	got, took := ask("MATCH ()-[r:demo_rel]->() RETURN count(r) AS c")
	report("baseline", "count(r), anonymous", got, totalEdges, took)

	q1 := fmt.Sprintf("MATCH (a:demo)-[r:demo_rel]->(b:demo) WHERE a.id = %d RETURN count(*) AS c", probe)
	got, took = ask(q1)
	report("1 source-side filter", q1, got, outDegree, took)

	var inDegree int64
	for _, tgt := range targets {
		if tgt == probe {
			inDegree++
		}
	}
	q1b := fmt.Sprintf("MATCH (a:demo)-[r:demo_rel]->(b:demo) WHERE b.id = %d RETURN count(*) AS c", probe)
	got, took = ask(q1b)
	report("1 target-side filter (contrast)", q1b, got, inDegree, took)

	nodeName := fmt.Sprintf("node_%d", probe)
	q1c := fmt.Sprintf("MATCH (a:demo)-[r:demo_rel]->(b:demo) WHERE a.name = '%s' RETURN count(*) AS c", nodeName)
	got, took = ask(q1c)
	report("1 source-side filter on a NON-key column", q1c, got, outDegree, took)

	q1d := fmt.Sprintf("MATCH (a:demo)-[r:demo_rel]->(b:demo) WHERE b.name = '%s' RETURN count(*) AS c", nodeName)
	got, took = ask(q1d)
	report("1 target-side filter on a NON-key column", q1d, got, inDegree, took)

	second := probe + 1
	if int(second)+1 >= len(ptr) {
		second = probe - 1
	}
	secondDegree := int64(ptr[second+1] - ptr[second])
	both := outDegree + secondDegree

	qMulti := fmt.Sprintf(
		"MATCH (a:demo)-[r:demo_rel]->(b:demo) WHERE a.name IN ['node_%d', 'node_%d'] RETURN count(*) AS c",
		probe, second)
	got, took = ask(qMulti)
	report("1 source-side filter matching TWO nodes", qMulti, got, both, took)

	qSingle := fmt.Sprintf(
		"MATCH (a:demo)-[r:demo_rel]->(b:demo) WHERE a.name IN ['node_%d'] RETURN count(*) AS c", probe)
	got, took = ask(qSingle)
	report("1 source-side filter matching ONE node", qSingle, got, outDegree, took)

	q4 := "MATCH (a:demo)-[:demo_rel]->(b:demo) RETURN count(a) AS c"
	got, took = ask(q4)
	report("4 count(node variable)", q4, got, totalEdges, took)

	q5 := fmt.Sprintf("MATCH (n:demo) WHERE n.id = %d RETURN count(n) AS c", probe)
	got, took = ask(q5)
	report("5 primary-key equality", q5, got, 1, took)

	q5b := fmt.Sprintf("MATCH (n:demo) WHERE n.id IN [%d] RETURN count(n) AS c", probe)
	got, took = ask(q5b)
	report("5 primary key via IN (workaround)", q5b, got, 1, took)
}

func TestIcebugAlternativesDefectOnToolOutput(t *testing.T) {
	root := os.Getenv("GRAPHIT_TOOL_ICEBUG_MULTI")
	if root == "" {
		t.Skip("set GRAPHIT_TOOL_ICEBUG_MULTI to a directory of tool-produced graphs")
	}

	entries, err := os.ReadDir(root)
	if err != nil {
		t.Fatal(err)
	}

	type graph struct {
		dir, node, relTable string
		edges               int64
	}
	var graphs []graph
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		sub := filepath.Join(root, e.Name())
		relTable := e.Name() + "_rel"
		targets, tErr := readUint64Column(filepath.Join(sub, "indices_"+relTable+".parquet"), "target")
		if tErr != nil {
			continue
		}
		graphs = append(graphs, graph{dir: sub, node: e.Name(), relTable: relTable, edges: int64(len(targets))})
	}
	if len(graphs) < 2 {
		t.Skipf("need at least two tool-produced graphs, found %d", len(graphs))
	}
	sort.Slice(graphs, func(i, j int) bool { return graphs[i].edges > graphs[j].edges })

	for _, order := range []string{"largest-first", "smallest-first"} {
		ordered := make([]graph, len(graphs))
		copy(ordered, graphs)
		if order == "smallest-first" {
			for i, j := 0, len(ordered)-1; i < j; i, j = i+1, j-1 {
				ordered[i], ordered[j] = ordered[j], ordered[i]
			}
		}

		t.Run(order, func(t *testing.T) {
			st, openErr := Open(filepath.Join(t.TempDir(), "mounted"))
			if openErr != nil {
				t.Fatal(openErr)
			}
			defer st.Close()

			host := ordered[0]
			if execErr := st.Exec(fmt.Sprintf(
				"CREATE NODE TABLE %s(id INT64, name STRING, PRIMARY KEY(id)) WITH (storage = '%s', format = 'icebug-disk')",
				QuoteIdent(host.node), EscapeLiteral(host.dir)), nil); execErr != nil {
				t.Fatalf("mounting the node table: %v", execErr)
			}

			var sum, boundByFirst int64
			var alts []string
			for _, g := range ordered {
				if execErr := st.Exec(fmt.Sprintf(
					"CREATE REL TABLE %s(FROM %s TO %s) WITH (storage = '%s', format = 'icebug-disk')",
					QuoteIdent(g.relTable), QuoteIdent(host.node), QuoteIdent(host.node),
					EscapeLiteral(g.dir)), nil); execErr != nil {
					t.Fatalf("mounting %s: %v", g.relTable, execErr)
				}
				perTable := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", QuoteIdent(g.relTable)))
				if perTable != g.edges {
					t.Errorf("%s: mounted %d, CSR has %d", g.relTable, perTable, g.edges)
				}
				sum += g.edges
				if g.edges < ordered[0].edges {
					boundByFirst += g.edges
				} else {
					boundByFirst += ordered[0].edges
				}
				alts = append(alts, QuoteIdent(g.relTable))
			}

			viaAlts := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", strings.Join(alts, "|")))
			t.Logf("%s: per-table sum=%d, via [:A|B]=%d, first-alternative bound predicts %d",
				order, sum, viaAlts, boundByFirst)

			if viaAlts != boundByFirst {
				t.Errorf("%s: [:A|B] answered %d; the first-alternative bound predicts %d (exact sum %d) — "+
					"the rule this export relies on no longer holds", order, viaAlts, boundByFirst, sum)
			}
			switch order {
			case "largest-first":
				if viaAlts != sum {
					t.Errorf("largest-first must be exact, got %d want %d", viaAlts, sum)
				}
			case "smallest-first":
				if viaAlts == sum {
					t.Errorf("smallest-first answered the exact sum %d — the upstream defect looks fixed; "+
						"re-measure before relying on it", sum)
				}
			}
		})
	}
}

func TestIcebugFilteredAlternativesDefectOnToolOutput(t *testing.T) {
	root := os.Getenv("GRAPHIT_TOOL_ICEBUG_MULTI")
	if root == "" {
		t.Skip("set GRAPHIT_TOOL_ICEBUG_MULTI to a directory of tool-produced graphs")
	}

	entries, err := os.ReadDir(root)
	if err != nil {
		t.Fatal(err)
	}
	type graph struct {
		dir, node, relTable string
		edges               int64
	}
	var graphs []graph
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		sub := filepath.Join(root, e.Name())
		relTable := e.Name() + "_rel"
		targets, tErr := readUint64Column(filepath.Join(sub, "indices_"+relTable+".parquet"), "target")
		if tErr != nil {
			continue
		}
		graphs = append(graphs, graph{dir: sub, node: e.Name(), relTable: relTable, edges: int64(len(targets))})
	}
	if len(graphs) < 2 {
		t.Skipf("need at least two tool-produced graphs, found %d", len(graphs))
	}
	sort.Slice(graphs, func(i, j int) bool { return graphs[i].edges > graphs[j].edges })

	st, err := Open(filepath.Join(t.TempDir(), "mounted"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	host := graphs[0]
	if execErr := st.Exec(fmt.Sprintf(
		"CREATE NODE TABLE %s(id INT64, name STRING, PRIMARY KEY(id)) WITH (storage = '%s', format = 'icebug-disk')",
		QuoteIdent(host.node), EscapeLiteral(host.dir)), nil); execErr != nil {
		t.Fatalf("mounting the node table: %v", execErr)
	}
	var alts []string
	for _, g := range graphs {
		if execErr := st.Exec(fmt.Sprintf(
			"CREATE REL TABLE %s(FROM %s TO %s) WITH (storage = '%s', format = 'icebug-disk')",
			QuoteIdent(g.relTable), QuoteIdent(host.node), QuoteIdent(host.node),
			EscapeLiteral(g.dir)), nil); execErr != nil {
			t.Fatalf("mounting %s: %v", g.relTable, execErr)
		}
		alts = append(alts, QuoteIdent(g.relTable))
	}

	var sum int64
	for _, g := range graphs {
		sum += g.edges
	}
	if got := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", strings.Join(alts, "|"))); got != sum {
		t.Fatalf("unfiltered alternatives = %d, want %d — cannot isolate the filtered defect", got, sum)
	}

	top, err := st.Query(fmt.Sprintf(
		"MATCH ()-[r:%s]->(b) RETURN b.name AS n, count(*) AS c ORDER BY c DESC LIMIT 1",
		QuoteIdent(host.relTable)), nil)
	if err != nil || len(top) == 0 {
		t.Fatalf("picking a target: %v", err)
	}
	name := Str(top[0]["n"])

	var perTable int64
	for _, g := range graphs {
		n := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->(b:%s) WHERE b.name = '%s' RETURN count(*) AS c",
			QuoteIdent(g.relTable), QuoteIdent(host.node), name))
		t.Logf("  %-16s filtered on %q = %d", g.relTable, name, n)
		perTable += n
	}
	viaAlts := scalar(t, st, fmt.Sprintf("MATCH ()-[r:%s]->(b:%s) WHERE b.name = '%s' RETURN count(*) AS c",
		strings.Join(alts, "|"), QuoteIdent(host.node), name))

	t.Logf("filtered on %q: per-table sum=%d, via [:A|B]=%d", name, perTable, viaAlts)
	if viaAlts == perTable {
		t.Errorf("filtered alternatives answered the exact sum %d on the tool's own files — the "+
			"upstream defect looks fixed; re-measure before relying on it", perTable)
	}
}
