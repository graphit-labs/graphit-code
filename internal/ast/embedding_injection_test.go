//go:build lancedb

package ast

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// TestEmbeddingCycleInjectsVectorsIntoTheStore pins the end of the embedding pipeline: a
// store is indexed before any vector exists, the embedding cycle computes them, and they
// have to be readable from the PUBLISHED store afterwards.
//
// The assertion reads through a FRESH handle on purpose: anything that answered through the
// handle that did the writing proves nothing about what the next reader will open.
//
// This is the shape of the failure it exists to catch: entities present, vectors absent, and
// nothing logged — semantic search with nothing to match against.
func TestEmbeddingCycleInjectsVectorsIntoTheStore(t *testing.T) {
	tmp := t.TempDir()
	cacheDir := filepath.Join(tmp, "cache")

	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	const files = 3
	const perFile = 4
	for f := 0; f < files; f++ {
		rel := fmt.Sprintf("pkg/file%d.go", f)
		ents := make([]cachedEntity, 0, perFile)
		for i := 0; i < perFile; i++ {
			ents = append(ents, cachedEntity{
				Label: "Function", UID: fmt.Sprintf("%s::Fn%d", rel, i),
				Name: fmt.Sprintf("handleRequest%d_%d", f, i), Path: rel,
				Line: i + 1, EndLine: i + 2,
				Docstring: "Validates the incoming request payload.",
			})
		}
		if err := cache.Store(rel, fmt.Sprintf("h-%d", f), &parseCacheEntry{
			RelPath: rel, Language: "go", Source: "package pkg\n", Entities: ents,
		}); err != nil {
			t.Fatalf("store %s: %v", rel, err)
		}
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	var embedded []cachedEntity
	var vecs [][]float32
	for f := 0; f < files; f++ {
		rel := fmt.Sprintf("pkg/file%d.go", f)
		for i := 0; i < perFile; i++ {
			if i%2 == 1 {
				continue
			}
			vec := make([]float32, ai.EmbeddingDimensions)
			for j := range vec {
				vec[j] = float32((f*perFile+i+j)%211) / 211.0
			}
			embedded = append(embedded, cachedEntity{
				Label: "Function", UID: fmt.Sprintf("%s::Fn%d", rel, i),
				Name: fmt.Sprintf("handleRequest%d_%d", f, i), Path: rel,
				Line: i + 1, Docstring: "Validates the incoming request payload.",
			})
			vecs = append(vecs, vec)
		}
	}
	wantVectors := len(embedded)

	storeDir := filepath.Join(tmp, "store")
	bundleDir := filepath.Join(storeDir, "graph.icebug")
	entries := make(map[string]*parseCacheEntry, cache.Count())
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		entries[relPath] = entry
		return true
	})
	ri := newRebuildIndex(entries, nil)
	if _, err := ExportDirectFromRebuildIndex(ri, bundleDir, bundleDir); err != nil {
		t.Fatalf("initial index: %v", err)
	}
	if err := BuildSearchIndexFor(context.Background(), storeDir, cache, nil); err != nil {
		t.Fatalf("initial search index: %v", err)
	}

	writer, err := OpenSearchIndex(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	if err := writer.StoreEntityVectors(context.Background(), embedded, vecs); err != nil {
		t.Fatalf("store vectors: %v", err)
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("close writer: %v", err)
	}

	idx, err := OpenSearchIndex(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("reopen search index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	total, vectors, err := idx.Counts(context.Background())
	if err != nil {
		t.Fatalf("count the index: %v", err)
	}

	if total != int64(files*perFile) {
		t.Errorf("the index has %d entity rows, want %d", total, files*perFile)
	}
	if vectors != int64(wantVectors) {
		t.Fatalf("the index has %d vectors, want %d — the embeddings did not reach the "+
			"published store", vectors, wantVectors)
	}

	hits, err := idx.entities.Search(context.Background(), lancestore.Query{
		Filter: lanceVectorColumn + " IS NOT NULL", Limit: 1,
	})
	if err != nil {
		t.Fatalf("querying for stored vectors: %v", err)
	}
	if wantVectors > 0 && len(hits) == 0 {
		t.Error("the index reports stored embeddings but a query for them returns nothing")
	}
}

func TestEmbeddingCycleReopensStoreAfterIncrementalAndReset(t *testing.T) {
	projectDir := stageEmbedLabelsGrammar(t, "Function")
	storeDir := filepath.Join(t.TempDir(), "store")
	client := &recordingEmbClient{}

	seedEmbeddingLoopStore(t, storeDir, "before.go", "BeforeReset")
	n, deferred, err := runEmbeddingCycleIfReady(context.Background(), storeDir, projectDir, client, nil)
	if err != nil {
		t.Fatalf("initial embedding cycle: %v", err)
	}
	if deferred || n != 1 {
		t.Fatalf("initial embedding cycle = (%d, deferred %v), want (1, false)", n, deferred)
	}
	assertEmbeddingLoopStoreCounts(t, storeDir, 1, 1)

	seedEmbeddingLoopStore(t, storeDir, "before.go", "AfterIncremental")
	n, deferred, err = runEmbeddingCycleIfReady(context.Background(), storeDir, projectDir, client, nil)
	if err != nil {
		t.Fatalf("post-incremental embedding cycle: %v", err)
	}
	if deferred || n != 1 {
		t.Fatalf("post-incremental embedding cycle = (%d, deferred %v), want (1, false)", n, deferred)
	}
	assertEmbeddingLoopStoreCounts(t, storeDir, 1, 1)

	_, resetLock, err := storelifecycle.Acquire(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("acquire reset lock: %v", err)
	}
	if err := os.RemoveAll(storeDir); err != nil {
		resetLock.Release()
		t.Fatal(err)
	}
	if _, _, err := storelifecycle.TryAcquire(context.Background(), storeDir); !errors.Is(err, storelifecycle.ErrLocked) {
		resetLock.Release()
		t.Fatalf("embedding lock during reset = %v, want ErrLocked", err)
	}
	n, deferred, err = runEmbeddingCycleIfReady(context.Background(), storeDir, projectDir, client, nil)
	if err != nil || !deferred || n != 0 {
		resetLock.Release()
		t.Fatalf("embedding cycle during reset = (%d, deferred %v, %v), want (0, true, nil)", n, deferred, err)
	}
	seedEmbeddingLoopStore(t, storeDir, "after.go", "AfterReset")
	resetLock.Release()

	n, deferred, err = runEmbeddingCycleIfReady(context.Background(), storeDir, projectDir, client, nil)
	if err != nil {
		t.Fatalf("post-reset embedding cycle: %v", err)
	}
	if deferred || n != 1 {
		t.Fatalf("post-reset embedding cycle = (%d, deferred %v), want (1, false)", n, deferred)
	}

	if _, err := os.Stat(filepath.Join(storeDir, "shards", "after.go"+shardEmbSuffix)); err != nil {
		t.Fatalf("new embedding shard: %v", err)
	}
	if _, err := os.Stat(filepath.Join(storeDir, "shards", "before.go"+shardEmbSuffix)); !os.IsNotExist(err) {
		t.Fatalf("old embedding shard survived reset: %v", err)
	}

	assertEmbeddingLoopStoreCounts(t, storeDir, 1, 1)
}

func assertEmbeddingLoopStoreCounts(t *testing.T, storeDir string, wantEntities, wantVectors int64) {
	t.Helper()
	idx, err := OpenSearchIndex(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	defer func() { _ = idx.Close() }()
	entities, vectors, err := idx.Counts(context.Background())
	if err != nil {
		t.Fatalf("count search index: %v", err)
	}
	if entities != wantEntities || vectors != wantVectors {
		t.Fatalf("search counts = (%d entities, %d vectors), want (%d, %d)", entities, vectors, wantEntities, wantVectors)
	}
}

func seedEmbeddingLoopStore(t *testing.T, storeDir, relPath, name string) {
	t.Helper()
	cache, err := NewShardCache(storeDir)
	if err != nil {
		t.Fatalf("new shard cache: %v", err)
	}
	defer func() { _ = cache.Close() }()

	entity := cachedEntity{
		Label: "Function", Lang: embedLabelsTestLang, UID: relPath + "::" + name,
		Name: name, Path: relPath, Line: 1, EndLine: 1,
	}
	if err := cache.Store(relPath, "hash-"+name, &parseCacheEntry{
		RelPath: relPath, Language: embedLabelsTestLang, Entities: []cachedEntity{entity},
	}); err != nil {
		t.Fatalf("store parse cache: %v", err)
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatalf("flush parse cache: %v", err)
	}
	if err := BuildSearchIndexFor(context.Background(), storeDir, cache, nil); err != nil {
		t.Fatalf("build search index: %v", err)
	}
}
