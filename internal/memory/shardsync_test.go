package memory

import (
	"os"
	"path/filepath"
	"testing"
)

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("reading %s: %v", path, err)
	}
	return string(data)
}

// The import is ADDITIVE, and that is the point: a developer's local shard belongs to
// a memory they wrote and have not pushed yet. Mirroring would destroy it, and there
// is nothing to gain — a stale entry in a content-addressed cache is inert, because
// the cache compares the hash before returning anything.
func TestImportShardsNeverOverwritesLocalWork(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	// The branch carries a colleague's shard, and its own version of one I also have.
	writeFile(t, filepath.Join(rawDir, shardMirrorDirName, "shards", "theirs.md.wiki.json"), `{"v":2,"h":"t"}`)
	writeFile(t, filepath.Join(rawDir, shardMirrorDirName, "shards", "mine.md.wiki.json"), `{"v":2,"h":"BRANCH"}`)

	// Mine is newer locally — a memory I have not pushed.
	writeFile(t, filepath.Join(wikiDir, "shards", "mine.md.wiki.json"), `{"v":2,"h":"LOCAL"}`)

	copied, err := ImportShards(rawDir, wikiDir)
	if err != nil {
		t.Fatalf("ImportShards: %v", err)
	}
	if copied != 1 {
		t.Errorf("copied %d shards, want 1 — only the one I was missing", copied)
	}
	if got := readFile(t, filepath.Join(wikiDir, "shards", "mine.md.wiki.json")); got != `{"v":2,"h":"LOCAL"}` {
		t.Errorf("my unpushed shard was overwritten: %s", got)
	}
	if got := readFile(t, filepath.Join(wikiDir, "shards", "theirs.md.wiki.json")); got != `{"v":2,"h":"t"}` {
		t.Errorf("the colleague's shard did not arrive: %s", got)
	}
}

// A branch with no shard mirror is the ordinary case for a scope nobody has compiled
// yet, and it must not be an error.
func TestImportShardsWithNothingToImport(t *testing.T) {
	copied, err := ImportShards(t.TempDir(), t.TempDir())
	if err != nil {
		t.Fatalf("an absent mirror is not an error: %v", err)
	}
	if copied != 0 {
		t.Errorf("copied %d, want 0", copied)
	}

	// And empty paths are refused quietly rather than resolving to a guess.
	if _, err := ImportShards("", t.TempDir()); err != nil {
		t.Errorf("ImportShards with no raw dir: %v", err)
	}
	if _, err := ImportShards(t.TempDir(), ""); err != nil {
		t.Errorf("ImportShards with no wiki dir: %v", err)
	}
}

// The export MIRRORS, so a deleted memory's shard leaves the branch instead of
// outliving its source forever. It is safe to mirror precisely because it runs after a
// compile over the worktree, so the local cache covers what the branch holds.
func TestExportShardsMirrorsAndDropsWhatIsGone(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	// The branch still carries a shard for a memory that has since been deleted.
	writeFile(t, filepath.Join(rawDir, shardMirrorDirName, "shards", "deleted.md.wiki.json"), `{"v":2,"h":"old"}`)
	// The local cache, after a compile, holds only what still exists.
	writeFile(t, filepath.Join(wikiDir, "shards", "kept.md.wiki.json"), `{"v":2,"h":"k"}`)
	writeFile(t, filepath.Join(wikiDir, "shards", "kept.md.emb.json"), `{"v":2,"h":"k"}`)
	// A half-written shard must not be published.
	writeFile(t, filepath.Join(wikiDir, "shards", "kept.md.wiki.json.tmp"), `{"partial"`)

	if err := ExportShards(rawDir, wikiDir); err != nil {
		t.Fatalf("ExportShards: %v", err)
	}

	mirror := filepath.Join(rawDir, shardMirrorDirName, "shards")
	if _, err := os.Stat(filepath.Join(mirror, "deleted.md.wiki.json")); !os.IsNotExist(err) {
		t.Error("the shard of a deleted memory survived in the branch")
	}
	for _, name := range []string{"kept.md.wiki.json", "kept.md.emb.json"} {
		if _, err := os.Stat(filepath.Join(mirror, name)); err != nil {
			t.Errorf("%s was not published: %v", name, err)
		}
	}
	if _, err := os.Stat(filepath.Join(mirror, "kept.md.wiki.json.tmp")); !os.IsNotExist(err) {
		t.Error("a half-written shard was published into the branch")
	}
}

// Nothing but the shard tree travels. The pages, the index and the database stay local:
// a consumer builds its own from its own raw markdown, and publishing them would put a
// second, divergent copy of every page into a branch many people write to.
func TestExportShardsPublishesOnlyTheShardTree(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	writeFile(t, filepath.Join(wikiDir, "shards", "a.md.wiki.json"), `{"v":2}`)
	writeFile(t, filepath.Join(wikiDir, "index.md"), "# Index\n")
	writeFile(t, filepath.Join(wikiDir, "Some_Memory.md"), "# Some\n")
	writeFile(t, filepath.Join(wikiDir, "wiki.db"), "binary")

	if err := ExportShards(rawDir, wikiDir); err != nil {
		t.Fatal(err)
	}

	mirrorRoot := filepath.Join(rawDir, shardMirrorDirName)
	entries, err := os.ReadDir(mirrorRoot)
	if err != nil {
		t.Fatalf("reading the mirror: %v", err)
	}
	if len(entries) != 1 || entries[0].Name() != "shards" {
		var names []string
		for _, e := range entries {
			names = append(names, e.Name())
		}
		t.Errorf("the mirror holds %v, want only [shards]", names)
	}
}

// A compile that has produced nothing yet has nothing to publish, and that is not a
// failure — it is the state of a scope on its first run.
func TestExportShardsWithNothingCompiled(t *testing.T) {
	if err := ExportShards(t.TempDir(), t.TempDir()); err != nil {
		t.Errorf("ExportShards with no shards: %v", err)
	}
	if err := ExportShards("", t.TempDir()); err != nil {
		t.Errorf("ExportShards with no raw dir: %v", err)
	}
}

// The mirror lives in a subdirectory so the memory scan cannot see it. That scan reads
// the worktree non-recursively and skips directories, which is what makes this free —
// but it is worth pinning, because a shard mistaken for a memory would be indexed as
// one and surface in recall.
func TestTheShardMirrorIsInvisibleToTheMemoryScan(t *testing.T) {
	rawDir := t.TempDir()
	writeFile(t, filepath.Join(rawDir, "A_Real_Memory.md"), "---\ntitle: Real\n---\n\nbody\n")
	writeFile(t, filepath.Join(rawDir, shardMirrorDirName, "shards", "A_Real_Memory.md.wiki.json"), `{"v":2}`)

	names := memorySourceFileNames(rawDir)
	if len(names) != 1 || names[0] != "A_Real_Memory.md" {
		t.Errorf("memorySourceFileNames = %v, want only the memory itself", names)
	}
}
