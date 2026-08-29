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

func TestFastPathCheck_FalseUntilEveryEntryHasItsPage(t *testing.T) {
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

	entries := []DocHashEntry{{CacheKey: "mem.md", ContentHash: "h1", Slug: "Mem"}}

	if FastPathCheck(context.Background(), wikiDir, entries, cache) {
		t.Error("FastPathCheck = true on a virgin wiki dir; the page has not been generated yet")
	}

	pagePath := filepath.Join(wikiDir, "Mem.md")
	if err := os.WriteFile(pagePath, []byte("page"), 0o644); err != nil {
		t.Fatalf("writing page: %v", err)
	}
	if !FastPathCheck(context.Background(), wikiDir, entries, cache) {
		t.Error("FastPathCheck = false with the page present and the hash unchanged; want true")
	}

	if err := os.WriteFile(filepath.Join(wikiDir, "Orphan.md"), []byte("page"), 0o644); err != nil {
		t.Fatalf("writing orphan page: %v", err)
	}
	if FastPathCheck(context.Background(), wikiDir, entries, cache) {
		t.Error("FastPathCheck = true with a page that has no entry; the deletion check regressed")
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

	if !StatPreCheck(baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"known.md"} },
	}) {
		t.Error("StatPreCheck = false with the only source file cached and stat-unchanged; want true")
	}

	if StatPreCheck(baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"known.md", "brand-new.md"} },
	}) {
		t.Error("StatPreCheck = true with a source file absent from the cache; the addition is invisible")
	}

	// A rename keeps the count identical, so the count alone is not enough.
	if StatPreCheck(baseDir, wikiDir, cache, StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return []string{"renamed.md"} },
	}) {
		t.Error("StatPreCheck = true after a rename; want false")
	}
}
