package ast

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The daemon published a one-file graph over a complete one, and every layer reported
// success. From this repository's daemon log:
//
//	strategy selected type=full-rebuild files=1
//	cache loaded files=1
//	COPY complete nodes=1 edges=0
//	swapping DB mode=atomic
//
// The chain: a shardCacheVersion bump discards the manifest, the watcher reports one
// changed file, the scoped run parses only that file, so the cache holds 1 — and
// `(changed+deleted) < cache.Count()` is 1 < 1, false, which selects a FULL rebuild.
// The full rebuild builds the whole graph from the cache it has, and the swap publishes
// it. The swap was never the problem; the precondition for publishing was missing.
func TestFullRebuildRefusesToPublishAGraphThatLostMostOfItsFiles(t *testing.T) {
	// Staged, so the fixture is parsed by this repository's grammar rather than the
	// copy the last sync installed into the runtime directory.
	work := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	tmp := t.TempDir()

	// A project comfortably above the floor.
	const total = 40
	for i := 0; i < total; i++ {
		src := "package p\n\nfunc F" + string(rune('A'+i%26)) + string(rune('a'+i/26)) + "() {}\n"
		name := filepath.Join(work, "f"+string(rune('a'+i%26))+string(rune('a'+i/26))+".go")
		if err := os.WriteFile(name, []byte(src), 0o644); err != nil {
			t.Fatalf("write fixture: %v", err)
		}
	}

	dbPath := filepath.Join(tmp, "ladybugdb")
	cacheDir := filepath.Join(tmp, "cache")

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	pres, err := RunPipeline(context.Background(), db, work, PipelineOptions{CacheDir: cacheDir})
	if err != nil {
		_ = db.Close()
		t.Fatalf("first index: %v", err)
	}
	t.Logf("pipeline: total=%d parsed=%d empty=%d errors=%d", pres.TotalFiles, pres.ParsedFiles, pres.EmptyCount, pres.ErrorCount)
	_ = db.Close()

	live := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	res, err2 := live.Query(context.Background(), "MATCH (f:File) RETURN count(f) AS n", nil)
	if err2 != nil {
		_ = live.Close()
		t.Fatalf("count live files: %v", err2)
	}
	liveFiles := toInt(res.Records[0]["n"])
	if liveFiles < shrinkFloor {
		_ = live.Close()
		t.Skipf("fixture produced %d files, below the floor of %d", liveFiles, shrinkFloor)
	}
	_ = live.Close()

	// The cache is gone — a version bump, a wiped directory, a fresh clone. Rebuilding
	// from it would publish a graph holding almost nothing.
	if err := os.RemoveAll(cacheDir); err != nil {
		t.Fatalf("drop cache: %v", err)
	}

	starved := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := starved.connect(); err != nil {
		t.Fatalf("connect: %v", err)
	}
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("new cache: %v", err)
	}
	pf := &ParsedFile{
		Path:     filepath.Join(work, "faa.go"),
		Language: "go",
		Entities: map[string][]Entity{"functions": {{Name: "FAa", GraphLabel: "Function", Line: 3}}},
	}
	if err := cache.Store("faa.go", "hash-of-one-file", ConvertToCache(pf, work, false, "")); err != nil {
		t.Fatalf("store: %v", err)
	}

	err = RebuildFromJSON(context.Background(), starved, cache, nil, "", work, nil)
	if err == nil {
		t.Fatal("a one-file rebuild against a full graph must not be published")
	}
	if !strings.Contains(err.Error(), "parse cache is incomplete") {
		t.Errorf("the error does not name the cause:\n  %s", err)
	}
	_ = starved.Close()

	// And the live graph survived, which is the whole point.
	after := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	defer func() { _ = after.Close() }()
	res, err = after.Query(context.Background(), "MATCH (f:File) RETURN count(f) AS n", nil)
	if err != nil {
		t.Fatalf("count after: %v", err)
	}
	if got := toInt(res.Records[0]["n"]); got != liveFiles {
		t.Errorf("the live graph was replaced anyway: %d files, was %d", got, liveFiles)
	}
}

// The guard must not fire on the ordinary cases, or it turns every real deletion into a
// stuck pipeline.
func TestShrinkGuardStaysOutOfTheWay(t *testing.T) {
	t.Parallel()

	for _, c := range []struct {
		name            string
		live, published int
	}{
		{"no live graph to protect", 0, 1},
		{"a graph below the floor is cheap to rebuild", shrinkFloor - 1, 0},
		{"an ordinary edit keeps every file", 400, 400},
		{"a real deletion of a few files", 400, 380},
		{"exactly at the ratio", 400, 200},
	} {
		if err := guardAgainstShrink(c.live, c.published, nil); err != nil {
			t.Errorf("%s: refused %d→%d: %v", c.name, c.live, c.published, err)
		}
	}

	// And it does fire where it must.
	if err := guardAgainstShrink(400, 1, nil); err == nil {
		t.Error("400 files down to 1 must not be published")
	}
}

// The first layer, upstream of the guard: a scoped run — the shape the daemon uses,
// naming the files a watcher saw change — must not treat an empty cache as "the project
// is these files". Discovery costs one slow pass; the alternative cost the graph.
func TestScopedRunWithAnEmptyCacheRediscoversTheProject(t *testing.T) {
	work := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	tmp := t.TempDir()

	const total = 30
	for i := 0; i < total; i++ {
		name := filepath.Join(work, "f"+string(rune('a'+i%26))+string(rune('a'+i/26))+".go")
		src := "package p\n\nfunc F" + string(rune('A'+i%26)) + string(rune('a'+i/26)) + "() {}\n"
		if err := os.WriteFile(name, []byte(src), 0o644); err != nil {
			t.Fatalf("write fixture: %v", err)
		}
	}

	dbPath := filepath.Join(tmp, "ladybugdb")
	cacheDir := filepath.Join(tmp, "cache")

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if _, err := RunPipeline(context.Background(), db, work, PipelineOptions{CacheDir: cacheDir}); err != nil {
		_ = db.Close()
		t.Fatalf("first index: %v", err)
	}
	_ = db.Close()

	// The cache is discarded — which is exactly what a shardCacheVersion bump does.
	if err := os.RemoveAll(cacheDir); err != nil {
		t.Fatalf("drop cache: %v", err)
	}

	// The watcher reports one file. Before the fallback, this published a one-file graph.
	touched := filepath.Join(work, "faa.go")
	if err := os.WriteFile(touched, []byte("package p\n\nfunc FAa() { _ = 1 }\n"), 0o644); err != nil {
		t.Fatalf("touch: %v", err)
	}

	db2 := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	res, err := RunPipelineForPaths(context.Background(), db2, work, []string{touched}, nil,
		PipelineOptions{CacheDir: cacheDir})
	if err != nil {
		_ = db2.Close()
		t.Fatalf("scoped run: %v", err)
	}
	_ = db2.Close()

	if res.TotalFiles < total {
		t.Errorf("the scoped run saw %d files; it should have rediscovered all %d", res.TotalFiles, total)
	}

	graph := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	defer func() { _ = graph.Close() }()
	count, err := graph.Query(context.Background(), "MATCH (f:File) RETURN count(f) AS n", nil)
	if err != nil {
		t.Fatalf("count: %v", err)
	}
	if got := toInt(count.Records[0]["n"]); got < total {
		t.Errorf("the graph holds %d files after a scoped run with an empty cache, want %d", got, total)
	}
}
