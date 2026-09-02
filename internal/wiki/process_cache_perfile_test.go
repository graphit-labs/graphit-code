package wiki

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func chunk(title, body string) CachedChunk {
	return CachedChunk{Title: title, Body: body, ContentHash: ContentHash([]byte(body))}
}

// The cache must leave NO file that two writers would both have to write.
//
// This is the whole reason the shared manifest.json was split up: a memory scope's
// cache travels in a git branch that every developer on a team pushes to, and a single
// shared index file conflicts on every concurrent push — which breaks the rebase the
// memory store depends on, not just the cache.
func TestCacheHasNoSharedIndexFile(t *testing.T) {
	dir := t.TempDir()
	c, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	c.Store("docs/a.md", "hash-a", []CachedChunk{chunk("A", "body a")})
	c.Store("docs/b.md", "hash-b", []CachedChunk{chunk("B", "body b")})
	c.StoreSlug("docs/a.md", "A")
	c.StoreSlug("docs/b.md", "B")
	if err := c.Save(); err != nil {
		t.Fatalf("Save: %v", err)
	}

	// Every file the cache wrote must sit under a per-source-file path.
	var shared []string
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil //nolint:nilerr
		}
		rel, _ := filepath.Rel(dir, path)
		rel = filepath.ToSlash(rel)
		if strings.HasPrefix(rel, "shards/") || strings.HasPrefix(rel, "watch/") {
			return nil
		}
		shared = append(shared, rel)
		return nil
	})
	if len(shared) != 0 {
		t.Errorf("cache wrote shared file(s) %v — two writers would collide on them", shared)
	}
	if _, err := os.Stat(filepath.Join(dir, "manifest.json")); err == nil {
		t.Error("manifest.json is back; the per-file layout exists to remove it")
	}

	// And each source file has its own sidecar beside its own shard.
	for _, rel := range []string{"docs/a.md", "docs/b.md"} {
		for _, suffix := range []string{chunkShardSuffix, metaShardSuffix} {
			p := filepath.Join(dir, shardsDirName, filepath.FromSlash(rel)+suffix)
			if _, err := os.Stat(p); err != nil {
				t.Errorf("missing %s: %v", p, err)
			}
		}
	}
}

// The property that matters, stated directly: two independent writers each adding a
// different source file both survive, with no file written by both.
func TestTwoWritersDoNotClobberEachOther(t *testing.T) {
	dir := t.TempDir()

	first, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	second, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}

	first.Store("mine.md", "h1", []CachedChunk{chunk("Mine", "my body")})
	first.StoreSlug("mine.md", "Mine")
	second.Store("theirs.md", "h2", []CachedChunk{chunk("Theirs", "their body")})
	second.StoreSlug("theirs.md", "Theirs")

	if err := first.Save(); err != nil {
		t.Fatalf("first Save: %v", err)
	}
	if err := second.Save(); err != nil {
		t.Fatalf("second Save: %v", err)
	}

	reopened, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	if reopened.Count() != 2 {
		t.Fatalf("Count = %d after two independent writers, want 2", reopened.Count())
	}
	if got := reopened.Get("mine.md", "h1"); len(got) != 1 || got[0].Title != "Mine" {
		t.Errorf("the first writer's entry did not survive: %+v", got)
	}
	if got := reopened.Get("theirs.md", "h2"); len(got) != 1 || got[0].Title != "Theirs" {
		t.Errorf("the second writer's entry did not survive: %+v", got)
	}
}

// A reopened cache must know everything the previous one recorded, since the index is
// now rebuilt by walking the shard tree rather than read from one file.
func TestCacheRoundTripsThroughTheShardTree(t *testing.T) {
	dir := t.TempDir()
	c, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	c.Store("a/b/deep.md", "hash-deep", []CachedChunk{chunk("Deep", "deep body")})
	c.StoreMtime("a/b/deep.md", 12345, 678)
	c.StoreSlug("a/b/deep.md", "Deep_Page")
	c.StoreOutRefs("a/b/deep.md", []string{"Other"})
	if err := c.Save(); err != nil {
		t.Fatal(err)
	}

	reopened, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	if reopened.HasChanged("a/b/deep.md", "hash-deep") {
		t.Error("a file recorded at this hash was reported as changed")
	}
	if got, ok := reopened.StatMatch("a/b/deep.md", 12345, 678); !ok || got != "hash-deep" {
		t.Errorf("StatMatch = (%q, %v), want (hash-deep, true)", got, ok)
	}
	if got := reopened.Slug("a/b/deep.md"); got != "Deep_Page" {
		t.Errorf("Slug = %q, want Deep_Page", got)
	}
	if got := reopened.GetOutRefs("a/b/deep.md"); len(got) != 1 || got[0] != "Other" {
		t.Errorf("GetOutRefs = %v, want [Other]", got)
	}
	if entries := reopened.AllStatEntries(); len(entries) != 1 || entries[0].RelPath != "a/b/deep.md" {
		t.Errorf("AllStatEntries = %+v", entries)
	}
}

// Removing a source file must take its sidecar with it, or a reopened cache would
// report a file whose chunks are gone.
func TestRemoveAndPruneDropTheSidecar(t *testing.T) {
	dir := t.TempDir()
	c, _ := NewWikiProcessCache(dir)
	c.Store("gone.md", "h", []CachedChunk{chunk("Gone", "b")})
	c.Store("stays.md", "h", []CachedChunk{chunk("Stays", "b")})
	_ = c.Save()

	c.Remove("gone.md")
	if pruned := c.Prune(map[string]bool{"stays.md": true}); pruned != 0 {
		t.Errorf("Prune removed %d after Remove already took the only stale entry", pruned)
	}

	reopened, _ := NewWikiProcessCache(dir)
	if reopened.Count() != 1 {
		t.Fatalf("Count = %d, want 1", reopened.Count())
	}
	if !reopened.HasChanged("gone.md", "h") {
		t.Error("a removed file is still cached")
	}

	// And the reverse: Prune takes what Remove was not told about.
	c2, _ := NewWikiProcessCache(dir)
	if pruned := c2.Prune(map[string]bool{}); pruned != 1 {
		t.Errorf("Prune = %d, want 1", pruned)
	}
	again, _ := NewWikiProcessCache(dir)
	if again.Count() != 0 {
		t.Errorf("Count = %d after pruning everything, want 0", again.Count())
	}
}

// A cache written by an older version must be ignored rather than misread, and its
// shared index file must not be left behind to travel in a branch.
func TestOldVersionCacheIsIgnoredAndItsManifestRemoved(t *testing.T) {
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, shardsDirName), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "manifest.json"),
		[]byte(`{"v":1,"files":{"old.md":{"h":"x","n":1}}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, shardsDirName, "old.md"+metaShardSuffix),
		[]byte(`{"v":1,"h":"x","n":1}`), 0o644); err != nil {
		t.Fatal(err)
	}

	c, err := NewWikiProcessCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	if c.Count() != 0 {
		t.Errorf("Count = %d; a version-1 sidecar must be ignored", c.Count())
	}
	if _, err := os.Stat(filepath.Join(dir, "manifest.json")); err == nil {
		t.Error("the version-1 manifest survived; it would be committed into a branch and read by nobody")
	}
}

// StoreDerived is called for every document on every run, so it must write nothing
// when nothing changed — otherwise the cache stops being incremental and every sync
// rewrites every shard.
func TestStoreDerivedIsAWriteOnlyWhenSomethingChanged(t *testing.T) {
	dir := t.TempDir()
	c, _ := NewWikiProcessCache(dir)
	c.Store("a.md", "h", []CachedChunk{chunk("A", "body")})
	derived := DerivedChunkFields{Slug: "A", ClusterID: 3, ClusterName: "core", Confidence: 0.9, Updated: "2026-08-14", Important: true}
	c.StoreDerived("a.md", derived)
	_ = c.Save()

	shard := filepath.Join(dir, shardsDirName, "a.md"+chunkShardSuffix)
	before, err := os.Stat(shard)
	if err != nil {
		t.Fatal(err)
	}

	// Same values: nothing may be marked dirty, so Save has nothing to write.
	c2, _ := NewWikiProcessCache(dir)
	c2.StoreDerived("a.md", derived)
	if err := c2.Save(); err != nil {
		t.Fatal(err)
	}
	after, err := os.Stat(shard)
	if err != nil {
		t.Fatal(err)
	}
	if !after.ModTime().Equal(before.ModTime()) {
		t.Error("an unchanged StoreDerived rewrote the shard; the cache is no longer incremental")
	}

	// A different community IS a change and must land.
	c3, _ := NewWikiProcessCache(dir)
	moved := derived
	moved.ClusterID, moved.ClusterName = 7, "moved"
	c3.StoreDerived("a.md", moved)
	if err := c3.Save(); err != nil {
		t.Fatal(err)
	}
	c4, _ := NewWikiProcessCache(dir)
	got := c4.Get("a.md", "h")
	if len(got) != 1 || got[0].ClusterID != 7 || got[0].ClusterName != "moved" {
		t.Errorf("the changed community did not persist: %+v", got)
	}
}

// Store must not wipe the metadata that other calls recorded. It used to replace the
// whole entry, which dropped the slug of every file whose content had changed — and a
// chunk with no slug is a page nothing can open.
func TestStorePreservesTheSlugAcrossAContentChange(t *testing.T) {
	dir := t.TempDir()
	c, _ := NewWikiProcessCache(dir)
	c.Store("a.md", "h1", []CachedChunk{chunk("A", "first")})
	c.StoreSlug("a.md", "A_Page")
	_ = c.Save()

	c2, _ := NewWikiProcessCache(dir)
	c2.Store("a.md", "h2", []CachedChunk{chunk("A", "second")})
	_ = c2.Save()

	c3, _ := NewWikiProcessCache(dir)
	if got := c3.Slug("a.md"); got != "A_Page" {
		t.Errorf("Slug = %q after a content change, want A_Page", got)
	}
}

// Only the index is derived. The shards are what a consumer needs, because the index is rebuilt
// FROM them — so anything shard-shaped has to survive the filter.
func TestIsDerivedFileNamesOnlyTheIndex(t *testing.T) {
	t.Parallel()
	for _, derived := range []string{
		WikiIndexDirName,
		WikiIndexDirName + "-scratch",
		strings.ToUpper(WikiIndexDirName),
		filepath.Join("nested", WikiIndexDirName),
		// Anything INSIDE the index directory. A base-only check answers false for every one
		// of these, which published the whole index — see IsDerivedFile. Callers walk
		// recursively and ask per entry, so these are the paths that actually reach it.
		filepath.Join(WikiIndexDirName, "chunks.lance", "data", "part_0.lance"),
		filepath.Join(WikiIndexDirName, "chunks.lance", "_transactions", "0-abc.txn"),
		filepath.Join(WikiIndexDirName, "chunks.lance", "_versions", "1.manifest"),
		WikiIndexDirName + "/chunks.lance/_indices/uuid/part_0_invert.lance",
	} {
		if !IsDerivedFile(derived) {
			t.Errorf("IsDerivedFile(%q) = false, want true", derived)
		}
	}
	for _, kept := range []string{
		filepath.Join("shards", "docs", "a.md.wiki.json"),
		filepath.Join("shards", "docs", "a.md.emb.json"),
		filepath.Join("shards", "docs", "a.md.meta.json"),
		".cluster_cache.json", ".gitignore",
	} {
		if IsDerivedFile(kept) {
			t.Errorf("IsDerivedFile(%q) = true; it is not rebuildable from the shards", kept)
		}
	}
}
