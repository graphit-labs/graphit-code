package ast

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

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

	if _, err := queryGraphEdgeSample(context.Background(), graph); err != nil {
		t.Fatalf("the edge sample must not error on a graph with no entities: %v", err)
	}
}

// Distinct physical members have overlapping offsets. The public sample must
// keep their endpoints separate and omit reverse acceleration tables.
func TestGraphSamplePreservesCanonicalRelationships(t *testing.T) {
	ctx := context.Background()
	work := t.TempDir()
	for name, source := range map[string]string{
		"orders.go": "package demo\nfunc Checkout(id string) { Validate(id); Save(id) }\nfunc Validate(id string) {}\nfunc Save(id string) {}\n",
		"events.go": "package demo\nfunc Emit(id string) { Append(id) }\nfunc Append(id string) {}\n",
	} {
		if err := os.WriteFile(filepath.Join(work, name), []byte(source), 0600); err != nil {
			t.Fatal(err)
		}
	}
	root := t.TempDir()
	cfg := LadybugConfig{StoreDir: root, IcebugDir: filepath.Join(root, "graph.icebug")}
	db := NewLadybugDB(cfg)
	if _, err := RunPipeline(ctx, db, work, PipelineOptions{CacheDir: root, Workers: 2, SkipExternal: true}); err != nil {
		t.Fatal(err)
	}
	_ = db.Close()
	db = NewLadybugDB(cfg)
	defer db.Close()
	for _, query := range []string{"MATCH (n)-[r]->(m) RETURN n,r,m", "MATCH (n)-->(m) RETURN m"} {
		if _, err := db.Query(ctx, query, nil); err == nil || !strings.Contains(err.Error(), "incorrect endpoints") {
			t.Fatalf("Query must refuse unsafe scan: %v", err)
		}
		if _, err := db.QueryPage(ctx, query, nil, 0, 10); err == nil || !strings.Contains(err.Error(), "incorrect endpoints") {
			t.Fatalf("QueryPage must refuse unsafe scan: %v", err)
		}
	}
	safe := "MATCH (a:Function)-[:CALLS]->(b:Function) WHERE a.name = 'Checkout' RETURN DISTINCT b.name"
	for _, paged := range []bool{false, true} {
		var result *QueryResult
		var err error
		if paged {
			result, err = db.QueryPage(ctx, safe, nil, 0, 10)
		} else {
			result, err = db.Query(ctx, safe, nil)
		}
		if err != nil || len(result.Records) != 2 {
			t.Fatalf("supported traversal: result=%v err=%v", result, err)
		}
	}
	server := &Server{db: db}
	for attempt := 0; attempt < 2; attempt++ {
		w := httptest.NewRecorder()
		server.handleGraph(w, httptest.NewRequest("GET", "/api/graph", nil))
		if w.Code != 200 {
			t.Fatalf("graph: %d %s", w.Code, w.Body.String())
		}
		var graph struct {
			Nodes []map[string]any `json:"nodes"`
			Links []map[string]any `json:"links"`
		}
		if err := json.Unmarshal(w.Body.Bytes(), &graph); err != nil {
			t.Fatal(err)
		}
		nodes := map[string]map[string]any{}
		for _, n := range graph.Nodes {
			nodes[n["id"].(string)] = n
		}
		calls := map[string]bool{}
		for _, e := range graph.Links {
			a, b := nodes[e["source"].(string)], nodes[e["target"].(string)]
			if a == nil || b == nil {
				t.Fatalf("missing endpoint: %v", e)
			}
			switch e["type"] {
			case "CALLS":
				calls[fmt.Sprint(a["name"], "->", b["name"])] = true
			case "HAS_PARAMETER":
				if a["type"] != "Function" || b["type"] != "Parameter" {
					t.Fatalf("reversed parameter relation: %v -> %v", a, b)
				}
			}
		}
		want := []string{"Checkout->Validate", "Checkout->Save", "Emit->Append"}
		if len(calls) != len(want) {
			t.Fatalf("calls: %v", calls)
		}
		for _, pair := range want {
			if !calls[pair] {
				t.Fatalf("missing %s: %v", pair, calls)
			}
		}
	}
}
