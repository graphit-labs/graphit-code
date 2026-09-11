//go:build lancedb

package ast

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func stagedSearchCache(t *testing.T, relPath, source, entityName string) (*ShardCache, *parseCacheEntry) {
	t.Helper()
	root := t.TempDir()
	absPath := filepath.Join(root, filepath.FromSlash(relPath))
	if err := os.MkdirAll(filepath.Dir(absPath), 0o755); err != nil {
		t.Fatalf("mkdir source: %v", err)
	}
	if err := os.WriteFile(absPath, []byte(source), 0o644); err != nil {
		t.Fatalf("write source: %v", err)
	}
	cache, err := NewShardCache(t.TempDir())
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	cache.SetRoot(root)
	entry := entryWith(relPath, source, cachedEntity{Name: entityName})
	if err := cache.Store(relPath, fileContentHash(absPath), entry); err != nil {
		t.Fatalf("store shard: %v", err)
	}
	return cache, entry
}

func publishSearchFixture(t *testing.T, storeDir, relPath, source, entityName string) {
	t.Helper()
	cache, _ := stagedSearchCache(t, relPath, source, entityName)
	idx, err := OpenSearchIndex(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("open active index: %v", err)
	}
	if err := idx.RebuildFromCache(context.Background(), cache, nil); err != nil {
		_ = idx.Close()
		t.Fatalf("build active index: %v", err)
	}
	if err := idx.Close(); err != nil {
		t.Fatalf("close active index: %v", err)
	}
}

func assertPublishedSource(t *testing.T, storeDir, relPath, want string) {
	t.Helper()
	idx, err := OpenSearchIndex(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("open published index: %v", err)
	}
	defer func() { _ = idx.Close() }()
	got, ok := idx.FileSource(context.Background(), relPath)
	if !ok || got != want {
		t.Fatalf("published source: ok=%v got=%q want=%q", ok, got, want)
	}
}

func TestStagedSearchRemainsHiddenUntilPublish(t *testing.T) {
	ctx := context.Background()
	storeDir := t.TempDir()
	const relPath = "internal/example.go"
	publishSearchFixture(t, storeDir, relPath, "package old\n", "OldEntity")

	cache, entry := stagedSearchCache(t, relPath, "package next\n", "NextEntity")
	stage, err := startStagedSearchRebuild(ctx, storeDir, cache, nil)
	if err != nil {
		t.Fatalf("start staging: %v", err)
	}
	if err := stage.Adopt(relPath, entry); err != nil {
		stage.Abort()
		t.Fatalf("adopt: %v", err)
	}
	if err := stage.Complete(map[string]*parseCacheEntry{relPath: entry}); err != nil {
		t.Fatalf("complete: %v", err)
	}

	assertPublishedSource(t, storeDir, relPath, "package old\n")
	if _, err := stage.Publish(ctx); err != nil {
		t.Fatalf("publish: %v", err)
	}
	assertPublishedSource(t, storeDir, relPath, "package next\n")
	status := readEmbedsStatus(storeDir)
	if status.Generation == "" || status.VectorIndex != vectorIndexPending || status.Entities != 1 {
		t.Fatalf("published vector status = %+v", status)
	}
}

func TestStagedSearchAbortPreservesActiveIndex(t *testing.T) {
	ctx := context.Background()
	storeDir := t.TempDir()
	const relPath = "internal/example.go"
	publishSearchFixture(t, storeDir, relPath, "package old\n", "OldEntity")

	cache, entry := stagedSearchCache(t, relPath, "package abandoned\n", "AbandonedEntity")
	stage, err := startStagedSearchRebuild(ctx, storeDir, cache, nil)
	if err != nil {
		t.Fatalf("start staging: %v", err)
	}
	if err := stage.Adopt(relPath, entry); err != nil {
		stage.Abort()
		t.Fatalf("adopt: %v", err)
	}
	stage.Abort()
	assertPublishedSource(t, storeDir, relPath, "package old\n")
	if _, err := os.Stat(stage.stagingPath); !os.IsNotExist(err) {
		t.Fatalf("staging directory survived abort: %v", err)
	}
}

func TestStagedSearchPublishRollbackRestoresIndexAndGeneration(t *testing.T) {
	storeDir := t.TempDir()
	const relPath = "internal/example.go"
	publishSearchFixture(t, storeDir, relPath, "package old\n", "OldEntity")
	previous := readEmbedsStatus(storeDir)

	err := publishStagedSearch(context.Background(), storeDir,
		filepath.Join(storeDir, "missing-staging"), 0, 1)
	if err == nil {
		t.Fatal("publishing a missing staging directory succeeded")
	}
	assertPublishedSource(t, storeDir, relPath, "package old\n")
	if got := readEmbedsStatus(storeDir); got != previous {
		t.Fatalf("vector status after rollback = %+v, want %+v", got, previous)
	}
}
