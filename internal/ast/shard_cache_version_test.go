package ast

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// The cache is keyed by the file's content hash, so a change in what the parser or the
// converter PRODUCES does not move the key. Nothing would be reparsed, and a user
// running the new binary would keep the old graph — which is exactly what happened when
// imports became entities: `make install` and a full sync still left the graph without a
// single Import node, because every file came back from the cache unchanged.
//
// shardCacheVersion is the only lever that invalidates all of it. These two tests pin
// its semantics, because the bump is worthless if the mismatch is ever tolerated.
func TestShardCacheIgnoresAManifestFromAnotherVersion(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	stale := shardManifest{
		Version: shardCacheVersion - 1,
		Files: map[string]*shardManifestEntry{
			"main.go": {Hash: "abc123", Lang: "go"},
		},
	}
	raw, err := json.Marshal(stale)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "manifest.json"), raw, 0o644); err != nil {
		t.Fatalf("write manifest: %v", err)
	}

	sc, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("open cache: %v", err)
	}
	defer func() { _ = sc.Close() }()

	if !sc.HasChanged("main.go", "abc123") {
		t.Error("a manifest from an older version was trusted; nothing would be reparsed")
	}
	if len(sc.manifest.Files) != 0 {
		t.Errorf("stale entries survived: %v", sc.manifest.Files)
	}
}

func TestShardCacheTrustsAManifestFromItsOwnVersion(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	current := shardManifest{
		Version: shardCacheVersion,
		Files: map[string]*shardManifestEntry{
			"main.go": {Hash: "abc123", Lang: "go"},
		},
	}
	raw, err := json.Marshal(current)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "manifest.json"), raw, 0o644); err != nil {
		t.Fatalf("write manifest: %v", err)
	}

	sc, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("open cache: %v", err)
	}
	defer func() { _ = sc.Close() }()

	if sc.HasChanged("main.go", "abc123") {
		t.Error("an unchanged file was reported as changed; the cache would never be used")
	}
	if !sc.HasChanged("main.go", "different") {
		t.Error("a changed hash must still invalidate the entry")
	}
}
