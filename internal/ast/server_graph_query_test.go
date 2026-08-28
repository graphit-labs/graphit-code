package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The explorer's default query has to bound the NODES it reads, not the rows it
// returns.
//
// It used to read `MATCH (n) OPTIONAL MATCH (n)-[r]->(m) ... LIMIT 300`, which asks
// the engine to expand every node against every outgoing edge and only then keep
// 300 rows. Invisible on a small graph. On a 2.5M-node one the intermediate result
// exhausted the buffer pool and /api/graph answered 500 with "Buffer manager
// exception: Unable to allocate memory".
//
// A unit test cannot hold 2.5M nodes, so what is pinned here is the property that
// made the difference: the limit binds the scan, and it is the first thing the query
// does.
func TestDefaultGraphQueryBoundsTheScan(t *testing.T) {
	q := defaultGraphQueryText()

	limitPos := strings.Index(q, "WITH n LIMIT")
	returnPos := strings.Index(q, "RETURN")
	if limitPos < 0 {
		t.Fatalf("the node limit is gone — the query would read the whole graph:\n%s", q)
	}
	if returnPos >= 0 && limitPos > returnPos {
		t.Errorf("the limit must bind before the projection, not after it:\n%s", q)
	}
}

// The node sample answers "what is in this graph" and must not expand, because the
// expansion is where the cost is and it is not a data cost.
//
// Measured on a 2M-node graph: with `OPTIONAL MATCH (n)-[r]->(m) WHERE id(m) IN
// sample_ids` the sample took 0.45s — and took the same 0.35s on a graph 34x
// smaller, which is what proves it is fan-out over the tables and an IN filter per
// row rather than anything an index could reach. Without the expansion: 0.01s.
//
// Connectivity is defaultGraphEdgeQuery's job. Reintroducing an expansion here
// brings the 0.45s back for edges that query already covers.
func TestDefaultGraphQueryDoesNotExpand(t *testing.T) {
	q := defaultGraphQueryText()

	for _, forbidden := range []string{"OPTIONAL MATCH", "]->(", "sample_ids", "m."} {
		if strings.Contains(q, forbidden) {
			t.Errorf("the node sample expands (%q): that cost 0.45s per open on a "+
				"large graph and returned only the directory tree:\n%s", forbidden, q)
		}
	}
}

// The two budgets are what bound the work, so they are worth pinning: nothing about
// either query stays safe if a later change makes them unbounded or enormous.
func TestGraphSampleBudgetsStayBounded(t *testing.T) {
	if graphSampleNodes <= 0 || graphSampleNodes > 2000 {
		t.Errorf("node budget %d is outside what a force-directed canvas draws and "+
			"what the buffer pool absorbs", graphSampleNodes)
	}
	if graphSampleEdges <= 0 || graphSampleEdges > 5000 {
		t.Errorf("edge budget %d is outside what a force-directed canvas draws and "+
			"what the buffer pool absorbs", graphSampleEdges)
	}
	if want := fmt.Sprintf("LIMIT %d", graphSampleNodes); !strings.Contains(defaultGraphQueryText(), want) {
		t.Errorf("the node budget did not reach the query — the constant is decorative:\n%s",
			defaultGraphQueryText())
	}
	if want := fmt.Sprintf("LIMIT %d", graphSampleEdges); !strings.Contains(defaultGraphEdgeQueryText(), want) {
		t.Errorf("the edge budget did not reach the query — the constant is decorative:\n%s",
			defaultGraphEdgeQueryText())
	}
}

// The edge sample has the same obligation: bound the scan before returning. It is
// the half that makes the picture connected — sampling nodes alone on a
// repository-shaped graph returns Files, which have no edges between them.
func TestDefaultGraphEdgeQueryBoundsItsScan(t *testing.T) {
	q := defaultGraphEdgeQueryText()

	limitPos := strings.Index(q, "LIMIT")
	returnPos := strings.Index(q, "RETURN")
	if limitPos < 0 {
		t.Fatalf("the edge sample is unbounded:\n%s", q)
	}
	if returnPos >= 0 && limitPos > returnPos {
		t.Errorf("the limit must bind before the projection, not after it:\n%s", q)
	}
}

// The two samples are merged by id, and both have to survive the merge: the node
// sample carries nodes the edge sample never touches, and the edge sample is the
// only source of links. Dropping either half is the failure this pins.
func TestBothSamplesReachTheDrawing(t *testing.T) {
	nodes := map[string]map[string]any{}
	var edges []map[string]any

	extractBuiltinQueryGraph(map[string]any{
		"src_id": "0:1", "src_label": "File", "src_name": "lonely.go",
		"src_path": "internal/lonely.go",
	}, nodes, &edges)

	extractBuiltinQueryGraph(map[string]any{
		"src_id": "0:2", "src_label": "File", "src_name": "caller.go",
		"src_path": "internal/caller.go",
		"dst_id":   "0:3", "dst_label": "Function", "dst_name": "Validate",
		"dst_path": "internal/auth/validate.go", "rel_type": "CONTAINS",
	}, nodes, &edges)

	if len(nodes) != 3 {
		t.Fatalf("expected the node sample's lone node plus the edge sample's two "+
			"endpoints, got %d: %v", len(nodes), nodes)
	}
	if nodes["0:1"]["name"] != "lonely.go" {
		t.Errorf("a node with no edges did not survive the merge — a graph whose "+
			"sample has no links would be drawn empty: %v", nodes["0:1"])
	}
	if nodes["0:3"]["name"] != "Validate" || nodes["0:3"]["file"] != "internal/auth/validate.go" {
		t.Errorf("the far endpoint lost its identity, so it draws as an unnamed "+
			"dot: %v", nodes["0:3"])
	}
	if len(edges) != 1 {
		t.Fatalf("expected the single CONTAINS link, got %v", edges)
	}
}

// And the samples have to actually run against a real graph — this one holding a
// single file, which is the shape that breaks them.
//
// Two failures live here, both of which draw an empty explorer. The first is losing
// the nodes that have no edges: the edge sample returns nothing on a graph this
// small, so the node sample is the only thing between the user and a blank canvas.
//
// The second is the line column. `n` is unlabelled and this graph's tables are
// File, Directory, Field, Parameter and CONTAINS — not one of them carries
// line_number, so asking for it is a Binder exception that fails the whole query
// rather than an empty column. That is why the handler goes through querySample,
// and why this test does too: calling the query directly would pass a contract the
// explorer does not actually use.
func TestGraphSamplesRunOnAGraphWithoutEntities(t *testing.T) {
	tmp := t.TempDir()
	dbPath := filepath.Join(tmp, "ladybugdb")
	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	defer func() { _ = db.Close() }()

	work := t.TempDir()
	if err := os.WriteFile(filepath.Join(work, "solo.go"), []byte("package solo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := RunPipeline(context.Background(), db, work, PipelineOptions{
		CacheDir: filepath.Join(tmp, "cache"),
	}); err != nil {
		t.Fatalf("pipeline: %v", err)
	}
	_ = db.Close()

	graph := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	defer func() { _ = graph.Close() }()

	if _, err := graph.Query(context.Background(), defaultGraphQueryText(), nil); err == nil {
		t.Log("this graph now carries line_number; the fallback below is no longer " +
			"exercised here and needs a graph that still lacks the property")
	}

	res, err := querySample(context.Background(), graph, defaultGraphQuery, graphNodeSampleQuery(false))
	if err != nil {
		t.Fatalf("the explorer's node sample must run on any graph: %v", err)
	}
	if len(res.Records) == 0 {
		t.Fatal("no rows: a graph whose nodes have no edges would be drawn empty")
	}

	if _, err := querySample(context.Background(), graph, defaultGraphEdgeQuery, graphEdgeSampleQuery(false)); err != nil {
		t.Fatalf("the edge sample must not error on a graph with no entities: %v", err)
	}
}
