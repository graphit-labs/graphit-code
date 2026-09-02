//go:build lancedb

package wiki

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

// The generation pass populates the process cache and only then asks these two
// helpers whether there is anything to do. Both must therefore answer from
// evidence the cache cannot fabricate — the pages on disk, and the set of
// source files that actually exist.

func TestFastPathCheck_FalseUntilTheIndexHoldsEveryEntry(t *testing.T) {
	wikiDir := t.TempDir()

	cache, err := NewWikiProcessCache(wikiDir)
	if err != nil {
		t.Fatalf("NewWikiProcessCache: %v", err)
	}
	cache.Store("mem.md", "h1", []CachedChunk{{Title: "Mem", ContentHash: "h1"}})

	// A read path (search) opens the DB before anything is indexed, so an empty wiki.db on
	// disk is the normal state on a virgin wiki dir — and an empty one must NOT satisfy
	// the gate, or generation is skipped and it stays empty forever. The index is
	// populated here so this test exercises the page condition it is named for, with every
	// other condition genuinely met.
	db, err := OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	if err := db.Rebuild(context.Background(), []WikiChunk{{
		Slug: "Mem", Title: "Mem", Body: "indexed body", ContentHash: "h1",
	}}, nil, &SyncLogEntry{Timestamp: "2026-08-18T00:00:00Z", TotalDocs: 1}, nil); err != nil {
		t.Fatalf("populating the index: %v", err)
	}
	if !db.HasContent(context.Background()) {
		t.Fatalf("the fixture index came out empty")
	}
	if err := db.Close(); err != nil {
		t.Fatalf("closing wiki db: %v", err)
	}
	// The deletion check compares the entries against the INDEX's slug set, not against pages on
	// disk — there are none. An entry the index does not hold, and an entry set larger than the
	// index, both have to defeat the fast path.
	const fixtureSlug = "Mem"

	if FastPathCheck(context.Background(), wikiDir, []DocHashEntry{
		{CacheKey: "mem.md", ContentHash: "h1", Slug: "NotIndexed"},
	}, cache) {
		t.Error("FastPathCheck = true for an entry the index does not hold")
	}

	indexed := []DocHashEntry{{CacheKey: "mem.md", ContentHash: "h1", Slug: fixtureSlug}}
	if !FastPathCheck(context.Background(), wikiDir, indexed, cache) {
		t.Error("FastPathCheck = false with the row indexed and the hash unchanged; want true")
	}

	cache.Store("extra.md", "h2", []CachedChunk{{Title: "Extra", ContentHash: "h2"}})
	orphaned := []DocHashEntry{indexed[0], {CacheKey: "extra.md", ContentHash: "h2", Slug: "Extra"}}
	if FastPathCheck(context.Background(), wikiDir, orphaned, cache) {
		t.Error("FastPathCheck = true with more entries than the index holds; the deletion check regressed")
	}
}

func TestStatPreCheck_DetectsSourceFileTheCacheNeverSaw(t *testing.T) {
	baseDir := t.TempDir()
	wikiDir := t.TempDir()

	knownPath := filepath.Join(baseDir, "known.md")
	if err := os.WriteFile(knownPath, []byte("# known\n"), 0o644); err != nil {
		t.Fatalf("writing source file: %v", err)
	}
	info, err := os.Stat(knownPath)
	if err != nil {
		t.Fatalf("stat source file: %v", err)
	}

	cache, err := NewWikiProcessCache(wikiDir)
	if err != nil {
		t.Fatalf("NewWikiProcessCache: %v", err)
	}
	cache.Store("known.md", "hash-known", []CachedChunk{{Title: "known", ContentHash: "hash-known"}})
	cache.StoreMtime("known.md", info.ModTime().UnixNano(), info.Size())

	// Populated, not merely created: an empty index must not satisfy the gate, or
	// generation is skipped and it stays empty forever. This test is about detecting an
	// unseen SOURCE file, so every other condition is genuinely met.
	db, err := OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	if err := db.Rebuild(context.Background(), []WikiChunk{{
		Slug: "known", Title: "known", Body: "indexed body", ContentHash: "hash-known",
	}}, nil, &SyncLogEntry{Timestamp: "2026-08-18T00:00:00Z", TotalDocs: 1}, nil); err != nil {
		t.Fatalf("populating the index: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("closing wiki db: %v", err)
	}

	if !StatPreCheck(context.Background(), baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"known.md"} },
	}) {
		t.Error("StatPreCheck = false with the only source file cached and stat-unchanged; want true")
	}

	if StatPreCheck(context.Background(), baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"known.md", "brand-new.md"} },
	}) {
		t.Error("StatPreCheck = true with a source file absent from the cache; the addition is invisible")
	}

	// A rename keeps the count identical, so the count alone is not enough.
	if StatPreCheck(context.Background(), baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"renamed.md"} },
	}) {
		t.Error("StatPreCheck = true after a rename; want false")
	}
}

// 🔒 A CACHE THAT SAYS "ALREADY PROCESSED" BESIDE AN INDEX THAT NEVER RECEIVED THE WORK.
//
// This is the state that made `knowledge index` report "0 articles" in 27 ms over an index missing a
// document that had been edited, and keep reporting it on every subsequent run. The mechanism is
// structural rather than a one-off: the generation pass writes the process cache — hashes and
// mtimes — BEFORE it rebuilds the index, so anything that stops the pass in between (a crash, a
// cancelled context, a bug in a later gate) leaves exactly this arrangement. Every file then
// stat-matches, no hash is computed, and the pre-check waves the run through.
//
// The old gate could not see it: it asked only whether the index had ANY rows.
func TestStatPreCheckRefusesAnIndexThatDoesNotMatchTheCache(t *testing.T) {
	baseDir := t.TempDir()
	wikiDir := t.TempDir()

	path := filepath.Join(baseDir, "edited.md")
	if err := os.WriteFile(path, []byte("# edited\n\nthe new body\n"), 0o644); err != nil {
		t.Fatalf("writing the source: %v", err)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat: %v", err)
	}

	cache, err := NewWikiProcessCache(wikiDir)
	if err != nil {
		t.Fatalf("NewWikiProcessCache: %v", err)
	}
	// The cache is current with the file — the state a poisoned run leaves behind.
	cache.Store("edited.md", "new-hash", []CachedChunk{{Title: "edited", ContentHash: "new-hash"}})
	cache.StoreMtime("edited.md", info.ModTime().UnixNano(), info.Size())

	// The index still holds the PREVIOUS content. It is populated, so the old emptiness check is
	// satisfied and cannot be what saves this.
	db, err := OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	if err := db.Rebuild(context.Background(), []WikiChunk{{
		Slug: "edited", Title: "edited", Body: "the old body", ContentHash: "old-hash",
	}}, nil, &SyncLogEntry{Timestamp: "2026-09-01T00:00:00Z", TotalDocs: 1}, nil); err != nil {
		t.Fatalf("populating the index: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("closing: %v", err)
	}

	if StatPreCheck(context.Background(), baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"edited.md"} },
	}) {
		t.Fatal("StatPreCheck = true while the index holds a different hash than the cache claims " +
			"was processed — a poisoned cache makes the staleness permanent")
	}

	// And once the index catches up, the pre-check does its job again — otherwise the incremental
	// would rebuild on every run and the gate would be pointless.
	db, err = OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("reopening: %v", err)
	}
	if err := db.Rebuild(context.Background(), []WikiChunk{{
		Slug: "edited", Title: "edited", Body: "the new body", ContentHash: "new-hash",
	}}, nil, &SyncLogEntry{Timestamp: "2026-09-01T00:00:01Z", TotalDocs: 1}, nil); err != nil {
		t.Fatalf("rebuilding: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("closing: %v", err)
	}

	if !StatPreCheck(context.Background(), baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"edited.md"} },
	}) {
		t.Error("StatPreCheck = false with the index holding exactly what the cache claims; want true")
	}
}
