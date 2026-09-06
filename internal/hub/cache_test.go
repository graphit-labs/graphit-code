package hub

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
)

func TestMetadataCacheIsPartitionedByHubAndSubject(t *testing.T) {
	root := t.TempDir()
	cfg := config.S3Config{Endpoint: "https://s3.example", Bucket: "hub", Prefix: "tenant"}
	a := newMetadataCache(root, cfg, hubaccess.Subject{UserID: "alice"})
	b := newMetadataCache(root, cfg, hubaccess.Subject{UserID: "bob"})
	other := newMetadataCache(root, config.S3Config{Bucket: "other"}, hubaccess.Subject{UserID: "alice"})
	if a.root == b.root || a.root == other.root || b.root == other.root {
		t.Fatal("cache partitions overlap")
	}
}

func TestMetadataCacheExpiresAndEvictsOldestObjects(t *testing.T) {
	cache := newMetadataCache(t.TempDir(), config.S3Config{Bucket: "hub"}, hubaccess.Subject{UserID: "alice"})
	now := time.Date(2026, 9, 5, 12, 0, 0, 0, time.UTC)
	cache.now = func() time.Time { return now }
	cache.maxBytes = 400
	if err := cache.Put("one", []byte("first"), "etag-1"); err != nil {
		t.Fatal(err)
	}
	oldPath := cache.path("one")
	old := now.Add(-time.Minute)
	if err := os.Chtimes(oldPath, old, old); err != nil {
		t.Fatal(err)
	}
	if err := cache.Put("two", make([]byte, 300), "etag-2"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(oldPath); !os.IsNotExist(err) {
		t.Fatalf("oldest entry was not evicted: %v", err)
	}

	cache.maxBytes = 1 << 20
	if err := cache.Put("fresh", []byte("value"), "etag"); err != nil {
		t.Fatal(err)
	}
	if data, _, ok := cache.GetFresh("fresh"); !ok || string(data) != "value" {
		t.Fatalf("fresh cache read = %q, %v", data, ok)
	}
	now = now.Add(metadataCacheTTL + time.Second)
	if _, _, ok := cache.GetFresh("fresh"); ok {
		t.Fatal("expired cache entry was accepted")
	}
	if !filepath.IsAbs(cache.root) {
		t.Fatalf("cache root is not absolute: %s", cache.root)
	}
}
