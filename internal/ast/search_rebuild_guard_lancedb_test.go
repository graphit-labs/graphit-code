//go:build lancedb

package ast

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func withSearchDimensions(t *testing.T, dims int) {
	t.Helper()
	previous := searchEmbeddingDimensions
	searchEmbeddingDimensions = func() int { return dims }
	t.Cleanup(func() { searchEmbeddingDimensions = previous })
}

// A rebuild drops the search tables before creating them again; with the embedding width
// momentarily unknown the creation cannot succeed, so the rebuild must stop before the drop.
func TestARebuildWithoutAWidthKeepsTheCurrentIndex(t *testing.T) {
	ctx := context.Background()
	withSearchDimensions(t, 8)
	idx := newLanceIndexForTest(t)
	if err := idx.ensureTables(ctx); err != nil {
		t.Fatal(err)
	}
	if err := idx.files.Append(ctx, []lancestore.Row{buildFileRow("kept.go", "package kept\n")}); err != nil {
		t.Fatal(err)
	}

	withSearchDimensions(t, 0)
	if _, err := idx.beginSearchRebuild(ctx, nil); err == nil {
		t.Fatal("a rebuild started without an embedding width")
	}
	if source, ok := idx.FileSource(ctx, "kept.go"); !ok || source == "" {
		t.Fatal("the existing index was dropped by a rebuild that could not complete")
	}
}

// The embedding cache judges a shard stale by its width; with no width known it must
// neither delete the shards it has nor write shards that record no width.
func TestTheEmbeddingCacheIsKeptWhileTheWidthIsUnknown(t *testing.T) {
	dir := t.TempDir()
	withSearchDimensions(t, 4)
	cache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	cache.Set("a.go", "uid-a", "hash-a", []float32{1, 2, 3, 4})
	if err := cache.Save(); err != nil {
		t.Fatal(err)
	}
	shard := filepath.Join(dir, "shards", "a.go"+shardEmbSuffix)
	before, err := os.ReadFile(shard)
	if err != nil {
		t.Fatal(err)
	}

	withSearchDimensions(t, 0)
	unknown, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	unknown.Set("a.go", "uid-a", "hash-b", []float32{9})
	if err := unknown.Save(); err != nil {
		t.Fatal(err)
	}
	after, err := os.ReadFile(shard)
	if err != nil {
		t.Fatalf("the shard was deleted while the width was unknown: %v", err)
	}
	if string(after) != string(before) {
		t.Fatal("the shard was rewritten while the width was unknown")
	}

	withSearchDimensions(t, 4)
	known, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	if got := known.Get("a.go", "uid-a", "hash-a"); len(got) != 4 {
		t.Fatalf("the cached vector = %v, want it back once the width is known", got)
	}
}
