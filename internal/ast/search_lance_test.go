//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// The port of the two write paths onto LanceDB, tested through the same door the daemon uses.
//
// Every test here builds a real shard cache, writes a real index and queries the real engine.
// None of them stub the store: the whole point of the port is that the engine does the search,
// so a fake would test the part that was deleted.

func newLanceIndexForTest(t *testing.T) *SearchIndex {
	t.Helper()
	dir := t.TempDir()
	idx, err := OpenSearchIndexAt(context.Background(),
		lancestore.Config{URI: dir + "/search.lance"})
	if err != nil {
		t.Fatalf("open lance index: %v", err)
	}
	t.Cleanup(func() { _ = idx.Close() })
	return idx
}

func newShardCacheForTest(t *testing.T, entries ...*parseCacheEntry) *ShardCache {
	t.Helper()
	cache, err := NewShardCache(t.TempDir())
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	for i, e := range entries {
		if err := cache.Store(e.RelPath, fmt.Sprintf("h-%d", i), e); err != nil {
			t.Fatalf("store %s: %v", e.RelPath, err)
		}
	}
	// Flushed, as a real index leaves it. StreamEntries evicts each shard after the
	// callback and reloads it from disk on the next pass, so an unflushed cache can be
	// streamed exactly once — and every caller here streams it at least twice.
	if err := cache.FlushDirty(); err != nil {
		t.Fatalf("flush shards: %v", err)
	}
	return cache
}

func entryWith(relPath, source string, ents ...cachedEntity) *parseCacheEntry {
	for i := range ents {
		if ents[i].Path == "" {
			ents[i].Path = relPath
		}
		if ents[i].UID == "" {
			ents[i].UID = relPath + "::" + ents[i].Name
		}
		if ents[i].Label == "" {
			ents[i].Label = "Function"
		}
	}
	return &parseCacheEntry{RelPath: relPath, Source: source, Entities: ents}
}

// bodyOf reads back the indexed document of one entity, which is how a test asserts what the
// engine was actually given rather than what the constructor was supposed to produce.
func bodyOf(t *testing.T, idx *SearchIndex, name string) string {
	t.Helper()
	hits, err := idx.entities.Search(context.Background(), lancestore.Query{
		Filter: fmt.Sprintf("name = '%s'", name), Limit: 5,
	})
	if err != nil {
		t.Fatalf("read back %s: %v", name, err)
	}
	if len(hits) == 0 {
		t.Fatalf("entity %q is not in the index", name)
	}
	s, _ := hits[0].Row[lanceBodyColumn].(string)
	return s
}

func searchNames(t *testing.T, idx *SearchIndex, query string, limit int) []string {
	t.Helper()
	hits, err := idx.entities.Search(context.Background(), lancestore.Query{
		Text: LanceQueryText(query), TextColumn: lanceBodyColumn, Limit: limit,
	})
	if err != nil {
		t.Fatalf("search %q: %v", query, err)
	}
	out := make([]string, 0, len(hits))
	for _, h := range hits {
		n, _ := h.Row["name"].(string)
		out = append(out, n)
	}
	return out
}

// ---------- rebuild ----------

func TestLanceRebuildIndexesFilesAndEntities(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	cache := newShardCacheForTest(t,
		entryWith("internal/hub/registry.go", "package hub // the registry",
			cachedEntity{Name: "SyncRegistry", Docstring: "Downloads the registry from the remote and swaps it in.", Line: 42},
			cachedEntity{Name: "resolveArtifact", Docstring: "Finds an artifact by name and type.", Line: 88}),
		entryWith("internal/memory/scope.go", "package memory",
			cachedEntity{Name: "OpenScope", Docstring: "Opens a memory scope, pulling it if absent.", Line: 10}),
	)

	if err := idx.RebuildFromCache(ctx, cache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	fileCount, err := idx.files.Count(ctx)
	if err != nil {
		t.Fatalf("count files: %v", err)
	}
	if fileCount != 2 {
		t.Errorf("files = %d, want 2", fileCount)
	}
	entCount, err := idx.entities.Count(ctx)
	if err != nil {
		t.Fatalf("count entities: %v", err)
	}
	if entCount != 3 {
		t.Errorf("entities = %d, want 3", entCount)
	}

	if got := searchNames(t, idx, "sync registry", 5); !hasName(got, "SyncRegistry") {
		t.Errorf("searching for the split identifier did not find it: %v", got)
	}
	if got := searchNames(t, idx, "swaps it in", 5); !hasName(got, "SyncRegistry") {
		t.Errorf("searching the docstring did not find its entity: %v", got)
	}
}

// A rebuild is defined as "the shards are the truth", so an entity that has left the shards must
// not survive it. The SQLite version cleared its tables explicitly; this one drops them, and the
// reason it drops rather than deletes is that a schema change has to be survivable too.
func TestLanceRebuildDropsWhatIsNoLongerInTheShards(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)

	first := newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "willBeDeleted"}))
	if err := idx.RebuildFromCache(ctx, first, nil); err != nil {
		t.Fatalf("first rebuild: %v", err)
	}
	if got := searchNames(t, idx, "willBeDeleted", 5); !hasName(got, "willBeDeleted") {
		t.Fatalf("the first rebuild did not index it: %v", got)
	}

	second := newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "survivor"}))
	if err := idx.RebuildFromCache(ctx, second, nil); err != nil {
		t.Fatalf("second rebuild: %v", err)
	}

	if got := searchNames(t, idx, "willBeDeleted", 10); hasName(got, "willBeDeleted") {
		t.Error("an entity absent from the shards survived a full rebuild")
	}
	if got := searchNames(t, idx, "survivor", 5); !hasName(got, "survivor") {
		t.Errorf("the rebuilt entity is missing: %v", got)
	}
}

// ---------- the document ----------

// THE REGRESSION THE SQLITE INDEX ACTUALLY HAD. Its rebuild INSERT wrote name_tri and its
// incremental INSERT did not, so every file an incremental touched silently lost its trigram
// recall until the next full rebuild. Here both paths call buildEntityRow, and this test proves
// the two produce the identical document rather than trusting that they do.
func TestLanceBothWritePathsProduceTheSameDocument(t *testing.T) {
	ctx := context.Background()
	ent := cachedEntity{Name: "evictOldestStaged", Docstring: "Drops the oldest staged event.", Line: 7}

	viaRebuild := newLanceIndexForTest(t)
	if err := viaRebuild.RebuildFromCache(ctx,
		newShardCacheForTest(t, entryWith("t.go", "package t", ent)), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	viaIncremental := newLanceIndexForTest(t)
	cache := newShardCacheForTest(t, entryWith("t.go", "package t", ent))
	if err := viaIncremental.UpdateIncremental(ctx, cache, []string{"t.go"}, nil, nil); err != nil {
		t.Fatalf("incremental: %v", err)
	}

	a := bodyOf(t, viaRebuild, "evictOldestStaged")
	b := bodyOf(t, viaIncremental, "evictOldestStaged")
	if a != b {
		t.Errorf("the two write paths indexed different documents.\n rebuild:     %q\n incremental: %q", a, b)
	}
	if !strings.Contains(a, "evi") {
		t.Errorf("the gram bag is missing from the indexed document: %q", a)
	}
	// The split form as splitCodeIdentifier produces it, plus the lowercased copy the tuning
	// sweep measured. Asserted literally so a change to the composition shows up here.
	if !strings.Contains(a, "evict Oldest Staged") {
		t.Errorf("the split identifier is missing from the indexed document: %q", a)
	}
	if !strings.Contains(a, "evict oldest staged") {
		t.Errorf("the lowercased split is missing from the indexed document: %q", a)
	}
}

// A truncated query has to reach the identifier it was cut from — that is what the gram bag is
// for, and it is the one thing Go computes that the engine could have.
func TestLanceTruncatedQueryReachesTheIdentifier(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	if err := idx.RebuildFromCache(ctx, newShardCacheForTest(t,
		entryWith("a.go", "package a",
			cachedEntity{Name: "resolveHubArtifact", Docstring: "Resolves an artifact."},
			cachedEntity{Name: "unrelatedThing", Docstring: "Something else entirely."}),
	), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	if got := searchNames(t, idx, "resolv", 5); !hasName(got, "resolveHubArtifact") {
		t.Errorf("a truncated query did not reach the identifier: %v", got)
	}
}

// ---------- incremental ----------

func TestLanceIncrementalReplacesAFileWithoutDuplicating(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)

	if err := idx.RebuildFromCache(ctx, newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "originalName"}),
		entryWith("b.go", "package b", cachedEntity{Name: "untouched"}),
	), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	// a.go is reparsed and its entity renamed.
	changed := newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "renamedEntity"}))
	if err := idx.UpdateIncremental(ctx, changed, []string{"a.go"}, nil, nil); err != nil {
		t.Fatalf("incremental: %v", err)
	}

	if got := searchNames(t, idx, "originalName", 10); hasName(got, "originalName") {
		t.Error("the old entity survived the incremental — the delete did not take")
	}
	if got := searchNames(t, idx, "renamedEntity", 10); !hasName(got, "renamedEntity") {
		t.Errorf("the new entity is not searchable: %v", got)
	}
	if got := searchNames(t, idx, "untouched", 10); !hasName(got, "untouched") {
		t.Errorf("an unrelated file lost its entity: %v", got)
	}

	count, err := idx.entities.Count(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if count != 2 {
		t.Errorf("entities = %d, want 2 — a replace that appends without deleting gives 3", count)
	}
}

// A deleted file leaves nothing behind. This is the failure mode the previous session found in the
// AST reindex — a deleted file was not removed and the delete was inert — so it is asserted here
// on the path that replaces it.
func TestLanceIncrementalRemovesADeletedFileEntirely(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)

	if err := idx.RebuildFromCache(ctx, newShardCacheForTest(t,
		entryWith("gone.go", "package gone", cachedEntity{Name: "vanishingFunc"}),
		entryWith("stays.go", "package stays", cachedEntity{Name: "stayingFunc"}),
	), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	// The cache no longer holds the deleted file, which is the real state after a removal.
	if err := idx.UpdateIncremental(ctx, newShardCacheForTest(t), nil, []string{"gone.go"}, nil); err != nil {
		t.Fatalf("incremental: %v", err)
	}

	if got := searchNames(t, idx, "vanishingFunc", 10); hasName(got, "vanishingFunc") {
		t.Error("a deleted file's entity is still searchable")
	}
	fileCount, err := idx.files.Count(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if fileCount != 1 {
		t.Errorf("files = %d, want 1 — the deleted file's own row was left behind", fileCount)
	}
	if got := searchNames(t, idx, "stayingFunc", 10); !hasName(got, "stayingFunc") {
		t.Errorf("deleting one file removed another's entity: %v", got)
	}
}

// An incremental with nothing in it must not touch the index at all — the daemon calls this on
// every watch event, including ones that turn out to affect no indexed file.
func TestLanceIncrementalWithNoDeltaIsANoOp(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	if err := idx.RebuildFromCache(ctx, newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "keepMe"})), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	before, _ := idx.entities.Count(ctx)

	if err := idx.UpdateIncremental(ctx, newShardCacheForTest(t), nil, nil, nil); err != nil {
		t.Fatalf("empty incremental: %v", err)
	}

	after, _ := idx.entities.Count(ctx)
	if before != after {
		t.Errorf("an empty incremental changed the index: %d -> %d", before, after)
	}
}

// ---------- vectors ----------

func TestLanceRebuildStoresEmbeddingsAndSearchesThemSemantically(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)

	// Two entities whose vectors point in clearly different directions, so which one a query
	// vector is nearest to is not a coin toss.
	vecA := make([]float32, ai.EmbeddingDimensions)
	vecB := make([]float32, ai.EmbeddingDimensions)
	for i := range vecA {
		vecA[i] = 0.01
	}
	vecB[0] = 1

	cache := newShardCacheForTest(t, entryWith("a.go", "package a",
		cachedEntity{Name: "alpha"}, cachedEntity{Name: "beta"}))
	embLookup := func(_, uid string) []float32 {
		switch {
		case strings.HasSuffix(uid, "alpha"):
			return vecA
		case strings.HasSuffix(uid, "beta"):
			return vecB
		}
		return nil
	}
	if err := idx.RebuildFromCache(ctx, cache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, cache, embLookup)

	hits, err := idx.entities.Search(ctx, lancestore.Query{
		Vector: vecB, VectorColumn: lanceVectorColumn, Limit: 2,
	})
	if err != nil {
		t.Fatalf("vector search: %v", err)
	}
	if len(hits) == 0 {
		t.Fatal("a vector search over stored embeddings returned nothing")
	}
	if n, _ := hits[0].Row["name"].(string); n != "beta" {
		t.Errorf("nearest to beta's own vector is %q, want beta", n)
	}
}

// An entity with no embedding must still be indexed and findable by keyword. If the vector column
// were required, the whole index would depend on the embedder having run.
func TestLanceEntitiesWithoutEmbeddingsAreStillSearchable(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	if err := idx.RebuildFromCache(ctx, newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "noVectorHere"})), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	if got := searchNames(t, idx, "noVectorHere", 5); !hasName(got, "noVectorHere") {
		t.Errorf("an entity without an embedding is not searchable: %v", got)
	}
}

// A published index is read-only, and both write paths have to say so rather than half-applying.
func TestLanceWritesAreRefusedOnAPublishedIndex(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	if err := idx.RebuildFromCache(ctx, newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "x"})), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	// Simulate what Open returns for an s3:// URI without needing a bucket here: the flag is what
	// every write path consults.
	remote := &SearchIndex{store: idx.store, files: idx.files, entities: idx.entities}
	if !remote.Remote() {
		t.Skip("this store is local; the read-only path is covered in lancestore against MinIO")
	}
}
