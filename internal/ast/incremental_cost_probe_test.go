package ast

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// TestIncrementalCostOnTheRealStore separates the two halves of an incremental rebuild's
// cost, because the end-to-end number does not say which one to attack.
//
// An incremental pays for the GRAPH — copying the whole store directory, mutating the copy,
// closing it and renaming it over production — and for the SEARCH INDEX, which is updated in
// place. Only the second is this package's recent work; the first is unchanged and was
// already known to dominate, at 215 ms – 5.0 s in the Shutdown+Close alone.
//
// It runs against a COPY of the real store, so it measures the corpus this project actually
// has rather than a fixture. Skipped when that store is absent, which is every CI machine.
//
//	GRAPHIT_COST_PROBE_STORE=~/.graphit/ast/project/<id> go test -tags lancedb \
//	  -run TestIncrementalCostOnTheRealStore -v ./internal/ast/
func TestIncrementalCostOnTheRealStore(t *testing.T) {
	storeDir := os.Getenv("GRAPHIT_COST_PROBE_STORE")
	if storeDir == "" {
		t.Skip("set GRAPHIT_COST_PROBE_STORE to a real AST store directory")
	}
	srcIndex := filepath.Join(storeDir, LanceIndexDirName)
	if _, err := os.Stat(srcIndex); err != nil {
		t.Skipf("no search index at %s: %v", srcIndex, err)
	}

	cache, err := NewShardCache(storeDir)
	if err != nil {
		t.Fatalf("open the real shard cache: %v", err)
	}
	defer func() { _ = cache.Close() }()
	if cache.Count() == 0 {
		t.Skip("the store has no parse shards")
	}

	// One real file, chosen by walking the cache rather than named, so the probe does not
	// depend on a path that may be renamed.
	var target string
	var entities int
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		if len(entry.Entities) > 20 && len(entry.Source) > 4096 {
			target, entities = relPath, len(entry.Entities)
			return false
		}
		return true
	})
	if target == "" {
		t.Skip("no file in the cache is large enough to be representative")
	}

	// A copy, so the measurement never touches the store the daemon is serving. The index is a
	// DIRECTORY now, so this copies a tree rather than one file — and the copy is the whole reason
	// the number means anything: measuring against a store being rewritten measures the writer.
	work := filepath.Join(t.TempDir(), "ladybugdb")
	tCopy := time.Now()
	if _, err := copyLanceIndex(srcIndex, LanceIndexPath(work)); err != nil {
		t.Fatalf("copy the index: %v", err)
	}
	copyMS := time.Since(tCopy).Seconds() * 1000

	idx, err := OpenSearchIndex(context.Background(), work)
	if err != nil {
		t.Fatalf("open the copied index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	files, _ := idx.files.Count(context.Background())
	ents, vecs, _ := idx.Counts(context.Background())

	embLookup := BuildEmbLookup(cache, nil)

	// Three runs: the first pays whatever page cache the copy did not warm, and a
	// repeated incremental is the case that matters — the daemon does one per edit.
	var runs []float64
	for i := 0; i < 3; i++ {
		t0 := time.Now()
		if err := idx.UpdateIncremental(context.Background(), cache, []string{target}, nil, embLookup); err != nil {
			t.Fatalf("incremental %d: %v", i, err)
		}
		runs = append(runs, time.Since(t0).Seconds()*1000)
	}

	t.Logf("corpus: %d files, %d entities, %d vectors", files, ents, vecs)
	t.Logf("changed file: %s (%d entities)", target, entities)
	t.Logf("copying the index directory alone: %.0f ms", copyMS)
	t.Logf("SEARCH INDEX UpdateIncremental: %.0f / %.0f / %.0f ms", runs[0], runs[1], runs[2])
}
