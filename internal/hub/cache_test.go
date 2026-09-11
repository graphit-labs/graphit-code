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
	a := newMetadataCache(root, config.S3Config{Bucket: "bucket-a"}, hubaccess.Subject{UserID: "alice"}, "revision-1")
	b := newMetadataCache(root, config.S3Config{Bucket: "bucket-a"}, hubaccess.Subject{UserID: "bob"}, "revision-1")
	other := newMetadataCache(root, config.S3Config{Bucket: "bucket-b"}, hubaccess.Subject{UserID: "alice"}, "revision-1")
	newRevision := newMetadataCache(root, config.S3Config{Bucket: "bucket-a"}, hubaccess.Subject{UserID: "alice"}, "revision-2")
	if a.root == b.root || a.root == other.root || b.root == other.root || a.root == newRevision.root {
		t.Fatal("cache partitions overlap")
	}
}

func TestMetadataCacheExpiresAndEvictsOldestObjects(t *testing.T) {
	cache := newMetadataCache(t.TempDir(), config.S3Config{Bucket: "bucket"}, hubaccess.Subject{UserID: "alice"}, "revision-1")
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
