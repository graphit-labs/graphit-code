//go:build lancedb

package ast

import "testing"

// The reason the cache exists: a rebuild DROPS the entity table, and recomputing an embedding
// costs a model inference where re-reading one costs a file read. Without this replay every
// rebuild would re-run the model over the whole corpus.
func TestRebuildRestoresVectorsFromTheCacheInsteadOfRecomputing(t *testing.T) {
	ctx := t.Context()
	dir := t.TempDir()
	idx := newLanceIndexForTest(t)

	cache := newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "Alpha"}, cachedEntity{Name: "Beta"}))

	embCache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	for i, uid := range []string{"a.go::Alpha", "a.go::Beta"} {
		embCache.Set("a.go", uid, cache.GetHash("a.go"), testVector(i+1))
	}
	if err := embCache.Save(); err != nil {
		t.Fatal(err)
	}

	// A rebuild, exactly as the pipeline performs one.
	if err := idx.RebuildFromCache(ctx, cache, BuildEmbLookup(cache, embCache)); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	_, withVector, err := idx.Counts(ctx)
	if err != nil {
		t.Fatalf("counts: %v", err)
	}
	if withVector != 2 {
		t.Fatalf("the rebuild restored %d vectors, want 2 — it would have to recompute them",
			withVector)
	}
}
