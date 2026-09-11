package ast

import "testing"

func TestRebuildPreparationCombinesUnchangedAndFreshEntries(t *testing.T) {
	cache, err := NewShardCache(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	unchanged := &parseCacheEntry{Entities: []cachedEntity{{UID: "unchanged"}}}
	oldChanged := &parseCacheEntry{Entities: []cachedEntity{{UID: "old"}}}
	if err := cache.Store("unchanged.go", "h1", unchanged); err != nil {
		t.Fatal(err)
	}
	if err := cache.Store("changed.go", "h2", oldChanged); err != nil {
		t.Fatal(err)
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	prep := startRebuildEntryPreparation(cache, map[string]bool{"changed.go": true})
	fresh := &parseCacheEntry{Entities: []cachedEntity{{UID: "fresh"}}}
	if err := cache.Store("changed.go", "h3", fresh); err != nil {
		t.Fatal(err)
	}
	prep.adopt("changed.go", fresh)

	entries, _ := prep.finish(cache)
	if len(entries) != 2 {
		t.Fatalf("prepared entries = %d, want 2", len(entries))
	}
	if got := entries["unchanged.go"].Entities[0].UID; got != "unchanged" {
		t.Fatalf("unchanged uid = %q", got)
	}
	if got := entries["changed.go"].Entities[0].UID; got != "fresh" {
		t.Fatalf("changed uid = %q, want fresh entry", got)
	}
}

func TestRebuildPreparationKeepsCachedEntryWhenReparseFails(t *testing.T) {
	cache, err := NewShardCache(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	old := &parseCacheEntry{Entities: []cachedEntity{{UID: "old"}}}
	if err := cache.Store("failed.go", "h1", old); err != nil {
		t.Fatal(err)
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	prep := startRebuildEntryPreparation(cache, map[string]bool{"failed.go": true})
	entries, _ := prep.finish(cache)
	if got := entries["failed.go"].Entities[0].UID; got != "old" {
		t.Fatalf("failed reparse uid = %q, want cached entry", got)
	}
}
