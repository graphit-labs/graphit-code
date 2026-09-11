package ast

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

func testVector(seed int) []float32 {
	v := make([]float32, ai.EmbeddingDimensions)
	for i := range v {
		v[i] = float32(math.Sin(float64(seed*7919+i)) * 0.5)
	}
	return v
}

// The cache exists to avoid recomputing, so a vector has to come back BIT-IDENTICAL. A lossy
// round trip would make a store's vectors differ before and after a rebuild.
func TestEmbShardRoundTripIsExact(t *testing.T) {
	dir := t.TempDir()
	cache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	const rel = "pkg/svc.go"
	want := map[string][]float32{
		rel + "::Alpha": testVector(1),
		rel + "::Beta":  testVector(2),
	}
	for uid, vec := range want {
		cache.Set(rel, uid, "h1", vec)
	}
	if err := cache.Save(); err != nil {
		t.Fatalf("save: %v", err)
	}

	reopened, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	for uid, expected := range want {
		got := reopened.Get(rel, uid, "h1")
		if got == nil {
			t.Fatalf("%s came back missing", uid)
		}
		if len(got) != len(expected) {
			t.Fatalf("%s came back with %d dims, want %d", uid, len(got), len(expected))
		}
		for i := range expected {
			if got[i] != expected[i] {
				t.Fatalf("%s dim %d: got %v, want %v — the round trip is lossy",
					uid, i, got[i], expected[i])
			}
		}
	}
}

func TestEmbShardIsZstdCompressedAndCompact(t *testing.T) {
	dir := t.TempDir()
	cache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	const rel = "a.go"
	for i := 0; i < 8; i++ {
		cache.Set(rel, fmt.Sprintf("a.go::E%d", i), "h1", make([]float32, ai.EmbeddingDimensions))
	}
	if err := cache.Save(); err != nil {
		t.Fatal(err)
	}

	encoded, err := os.ReadFile(filepath.Join(dir, "shards", rel+shardEmbSuffix))
	if err != nil {
		t.Fatalf("the shard was not written: %v", err)
	}
	if !bytes.HasPrefix(encoded, []byte{0x28, 0xb5, 0x2f, 0xfd}) {
		t.Fatalf("embedding sidecar is not a Zstandard frame: prefix %x", encoded[:min(4, len(encoded))])
	}
	raw, err := shardZstdDecoder.DecodeAll(encoded, nil)
	if err != nil {
		t.Fatalf("decompress: %v", err)
	}
	if len(raw) < 2 {
		t.Fatalf("decompressed payload is too short for the version header: %d bytes", len(raw))
	}
	if got := binary.LittleEndian.Uint16(raw[:2]); got != shardEmbVersion {
		t.Fatalf("decompressed payload version = %d, want %d", got, shardEmbVersion)
	}
	if len(encoded) >= len(raw) {
		t.Fatalf("compressible embedding payload grew: compressed=%d raw=%d", len(encoded), len(raw))
	}
}

func TestEmbShardDropsACorruptZstdFrame(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "shards", "a.go"+shardEmbSuffix)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("not a zstandard frame"), 0o644); err != nil {
		t.Fatal(err)
	}

	cache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	if got := cache.Get("a.go", "a.go::E", "h1"); got != nil {
		t.Fatal("corrupt embedding sidecar was loaded")
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("corrupt embedding sidecar was not removed: %v", err)
	}
}

func TestEmbShardIgnoresRawEmbFiles(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "shards", "a.go.emb")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	want := []byte("development artifact outside the current format")
	if err := os.WriteFile(path, want, 0o644); err != nil {
		t.Fatal(err)
	}

	cache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	if got := cache.Get("a.go", "a.go::E", "h1"); got != nil {
		t.Fatal("raw .emb file was loaded")
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("raw .emb file was modified or removed: %v", err)
	}
	if !bytes.Equal(got, want) {
		t.Fatalf("raw .emb file changed: got %q, want %q", got, want)
	}
}

// A file whose content changed must not answer with the vectors of its previous content.
func TestEmbShardRefusesVectorsForAChangedFile(t *testing.T) {
	dir := t.TempDir()
	cache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	cache.Set("a.go", "a.go::E", "h1", testVector(1))
	if got := cache.Get("a.go", "a.go::E", "h2"); got != nil {
		t.Error("the cache answered for a hash it was not keyed on")
	}
	if got := cache.Get("a.go", "a.go::E", "h1"); got == nil {
		t.Error("the cache lost a vector it was keyed on")
	}
}
