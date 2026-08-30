package ast

import (
	"context"
	"fmt"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

// deepChainBundle builds a corpus whose CALLS graph is one chain `files` long: file f's
// Symbol{f}_{i} calls file f+1's Symbol{f+1}_{i}. The depth behind any anchor is the whole
// corpus, which is what separates a planner that stops at its hop bound from one that walks
// to visited saturation.
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

// A bounded plan must stop at its bound. The filter in the frontier loop already refuses
// every uid found deeper than maxHops, so continuing to expand is work whose result is
// discarded — and the discarded work is proportional to the DEPTH behind the anchor, not
// to the answer. MEASURED before the bound was honoured: 1.8 s at 300 files, 28.9 s at
// 1500, over 180 s at 6000.
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
			// Generous by two orders of magnitude against the 28.9 s this cost when the
			// loop ran to saturation: this asserts "bounded", not a benchmark figure.
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
	// The chain is a cycle over every file, so an unbounded walk reaches every OTHER node:
	// the anchor is seeded into the visited set at depth 0, so coming back round to it does
	// not re-admit it.
	if len(res.Records) != files-1 {
		t.Errorf("unbounded traversal reached %d nodes, want %d — saturation must be unaffected", len(res.Records), files-1)
	}
}

// A traversal that projects PROPERTIES returns its rows in a specified order. It used to
// fall out of the iteration — reached uid, then candidate label — which is keyed on a uid
// the caller never sees and moves whenever the planner batches differently.
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

	// And it is the same order on a second execution.
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
