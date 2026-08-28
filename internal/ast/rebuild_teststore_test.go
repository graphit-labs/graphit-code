package ast

import (
	"context"
	"path/filepath"
	"testing"
)

// rebuildTestStore builds an icebug bundle from a shard cache (the new direct path,
// no intermediate Ladybug DB) and returns an open backend over it, mounted in-memory.
func rebuildTestStore(t *testing.T, cache *ShardCache, proj string) *LadybugBackend {
	t.Helper()
	storeDir := filepath.Join(t.TempDir(), "store")
	bundleDir := filepath.Join(storeDir, "graph.icebug")

	entries := make(map[string]*parseCacheEntry, cache.Count())
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		entries[relPath] = entry
		return true
	})
	ri := newRebuildIndex(entries, targetRulesFor(proj))
	if _, err := ExportDirectFromRebuildIndex(ri, bundleDir, bundleDir); err != nil {
		t.Fatalf("rebuild bundle: %v", err)
	}

	db := NewLadybugDB(LadybugConfig{StoreDir: storeDir, IcebugDir: bundleDir})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	t.Cleanup(func() { _ = db.Close() })
	_ = context.Background
	return db
}
