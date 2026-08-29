//go:build lancedb

package ast

import (
	"context"

	"fmt"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"

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

	// A vector for SOME entities, not all — partial coverage is the normal case, since the
	// grammar's embed_labels list decides who gets one.
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

	// Production order: the store is indexed FIRST, while no vector exists yet, and the
	// embedding cycle writes them afterwards. Asserting on a single rebuild that already
	// had the vectors in hand would exercise a different path.
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

	// And this is the call under test — the one the embedding loop makes.
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

	// A fresh handle, deliberately — see the doc comment.
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

	// THE DEFECT THIS PART GUARDED CANNOT HAPPEN ANY MORE, and that is worth stating rather than
	// deleting silently. Under SQLite there were TWO structures — entity_emb held the durable copy
	// and entity_vec the searchable one, built from it — so a rebuild could fill the first and
	// skip the second, leaving semantic search answering nothing while every count still looked
	// right. Here the embedding is a column of the entity: there is one place for it to be, so
	// stored and searchable are the same fact and cannot disagree.
	//
	// What remains checkable is that the vectors are reachable through a search, which is the
	// property the two-table check was standing in for.
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
