package ast

import (
	"context"
	"fmt"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

func deepChainBundle(t *testing.T, files int) *LadybugBackend {
	t.Helper()
	entries := syntheticCorpus(files)
	ri := newRebuildIndex(entries, targetRulesFor(t.TempDir()))
	storeDir := filepath.Join(t.TempDir(), "store")
	bundleDir := filepath.Join(storeDir, "graph.icebug")
	if _, err := ExportDirectFromRebuildIndex(ri, bundleDir, bundleDir); err != nil {
		t.Fatalf("export: %v", err)
	}
	db := NewLadybugDB(LadybugConfig{StoreDir: storeDir, IcebugDir: bundleDir})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	t.Cleanup(func() { _ = db.Close() })
	return db
}

func TestBoundedTraversalStopsAtItsHopBound(t *testing.T) {
	const files = 1500
	db := deepChainBundle(t, files)
	anchor := "internal/pkg5/module5/file5.go:Symbol3"

	for _, tc := range []struct {
		name   string
		cypher string
		want   int
	}{
		{"one hop", fmt.Sprintf(
			"MATCH (a:Function)-[:CALLS]->(b:Function) WHERE a.uid IN ['%s'] RETURN DISTINCT b.uid", anchor), 1},
		{"two hops", fmt.Sprintf(
			"MATCH (a:Function)-[:CALLS*1..2]->(b:Function) WHERE a.uid IN ['%s'] RETURN DISTINCT b.uid", anchor), 2},
		{"three hops", fmt.Sprintf(
			"MATCH (a:Function)-[:CALLS*1..3]->(b:Function) WHERE a.uid IN ['%s'] RETURN DISTINCT b.uid", anchor), 3},
	} {
		t.Run(tc.name, func(t *testing.T) {
			start := time.Now()
			res, err := db.Query(context.Background(), tc.cypher, nil)
			took := time.Since(start)
			if err != nil {
				t.Fatalf("query: %v", err)
			}
			if len(res.Records) != tc.want {
				t.Errorf("rows = %d, want %d", len(res.Records), tc.want)
			}
			if took > 5*time.Second {
				t.Errorf("took %s — the plan is bounded to %d hop(s) and must not walk the whole component",
					took.Round(time.Millisecond), tc.want)
			}
			t.Logf("%s: %d rows in %s", tc.name, len(res.Records), took.Round(time.Millisecond))
		})
	}
}

// An unbounded plan must still saturate: the hop bound is the only thing that changed.
func TestUnboundedTraversalStillReachesTheWholeComponent(t *testing.T) {
	const files = 60
	db := deepChainBundle(t, files)
	anchor := "internal/pkg5/module5/file5.go:Symbol3"

	res, err := db.Query(context.Background(), fmt.Sprintf(
		"MATCH (a:Function)-[:CALLS*]->(b:Function) WHERE a.uid IN ['%s'] RETURN DISTINCT b.uid", anchor), nil)
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	if len(res.Records) != files-1 {
		t.Errorf("unbounded traversal reached %d nodes, want %d — saturation must be unaffected", len(res.Records), files-1)
	}
}

func TestTraversalProjectionIsOrderedAndDeduplicated(t *testing.T) {
	const files = 400
	db := deepChainBundle(t, files)
	anchor := "internal/pkg5/module5/file5.go:Symbol3"

	cypher := fmt.Sprintf(
		"MATCH (a:Function)-[:CALLS*1..3]->(b:Function) WHERE a.uid IN ['%s'] RETURN DISTINCT b.name", anchor)
	res, err := db.Query(context.Background(), cypher, nil)
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	if len(res.Records) == 0 {
		t.Fatal("no rows")
	}

	keys := make([]string, 0, len(res.Records))
	seen := map[string]bool{}
	for _, r := range res.Records {
		k := icebugRecordKey(r)
		if seen[k] {
			t.Errorf("duplicate row %v", r)
		}
		seen[k] = true
		keys = append(keys, k)
	}
	if !sort.StringsAreSorted(keys) {
		t.Errorf("rows are not in the specified order: %v", keys)
	}

	again, err := db.Query(context.Background(), cypher, nil)
	if err != nil {
		t.Fatalf("query again: %v", err)
	}
	if len(again.Records) != len(res.Records) {
		t.Fatalf("second run returned %d rows, first returned %d", len(again.Records), len(res.Records))
	}
	for i := range again.Records {
		if icebugRecordKey(again.Records[i]) != keys[i] {
			t.Fatalf("row %d differs between two executions of the same query", i)
		}
	}
}
