package ast

import (
	"context"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

// TestRealCorpusIncrementalBreakdown reproduces a full rebuild and then a one-file
// incremental against a real shard cache, at a scratch path, with every phase timed.
//
// It exists because the headline numbers — full 988 s, incremental 1178 s — do not say
// WHERE the time goes, and a projection from synthetic corpora accounted for only about a
// third of the incremental. Deciding an architecture on the missing two thirds is guessing.
//
//	GRAPHIT_REAL_CACHE=/path/to/store/shards \
//	GRAPHIT_DB_BUFFER_MB=8192 \
//	go test -run TestRealCorpusIncrementalBreakdown ./internal/ast/ -v -timeout 180m
func TestRealCorpusIncrementalBreakdown(t *testing.T) {
	cacheDir := os.Getenv("GRAPHIT_REAL_CACHE")
	if cacheDir == "" {
		t.Skip("set GRAPHIT_REAL_CACHE to a shard cache directory")
	}

	logger := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelDebug}))

	t0 := time.Now()
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("load shard cache: %v", err)
	}
	embCache, err := NewShardEmbCache(cacheDir, cache)
	if err != nil {
		t.Fatalf("load emb cache: %v", err)
	}
	t.Logf("cache loaded: %d files in %.1fs", cache.Count(), time.Since(t0).Seconds())

	scratch := os.Getenv("GRAPHIT_SCRATCH_DB")
	if scratch == "" {
		scratch = filepath.Join(t.TempDir(), "ladybugdb")
	}
	lb := NewLadybugDB(LadybugConfig{DBPath: scratch})
	defer func() { _ = lb.Close() }()

	// Full rebuild, unless a store was already built at the scratch path by an earlier run.
	if _, err := os.Stat(scratch); os.IsNotExist(err) {
		t1 := time.Now()
		if err := RebuildFromJSONWithSearch(context.Background(), lb, cache, embCache,
			"", "", logger, nil); err != nil {
			t.Fatalf("full rebuild: %v", err)
		}
		t.Logf("FULL REBUILD: %.1fs", time.Since(t1).Seconds())
	} else {
		t.Logf("reusing store at %s", scratch)
	}

	// One file, chosen as the median-sized entry so the delta is typical rather than
	// flattering.
	target := probePickTypicalFile(t, cache)
	t.Logf("incremental target: %s", target)

	t2 := time.Now()
	if err := IncrementalRebuild(context.Background(), lb, cache, embCache,
		[]string{target}, nil, "", "", logger); err != nil {
		t.Fatalf("incremental: %v", err)
	}
	t.Logf("INCREMENTAL (1 file): %.1fs", time.Since(t2).Seconds())

	probeReportStoreSize(t, scratch)
}

// probePickTypicalFile takes the middle path of the sorted list rather than the largest or
// the first. Walking every entry to find a median-sized one would page the whole 8.8 GB
// shard cache into memory, which is itself the thing under measurement.
func probePickTypicalFile(t *testing.T, cache *ShardCache) string {
	t.Helper()
	paths := cache.AllPaths()
	if len(paths) == 0 {
		t.Fatal("shard cache is empty")
	}
	sort.Strings(paths)
	return paths[len(paths)/2]
}

func probeReportStoreSize(t *testing.T, dbPath string) {
	t.Helper()
	dir := filepath.Dir(dbPath)
	entries, err := os.ReadDir(dir)
	if err != nil {
		return
	}
	for _, e := range entries {
		if fi, err := e.Info(); err == nil && !fi.IsDir() {
			t.Logf("  %-40s %8.1f MB", e.Name(), float64(fi.Size())/(1<<20))
		}
	}
}
