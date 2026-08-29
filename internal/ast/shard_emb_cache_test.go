package ast

import (
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

// The binary format is the whole point: the JSON it replaces cost 3.1x for the same numbers.
func TestEmbShardIsBinaryAndCompact(t *testing.T) {
	dir := t.TempDir()
	cache, err := NewShardEmbCache(dir, nil)
	if err != nil {
		t.Fatal(err)
	}
	const rel = "a.go"
	const n = 10
	for i := 0; i < n; i++ {
		cache.Set(rel, "a.go::E", "h1", testVector(i))
	}
	// One uid, so one record — overwriting is what an entity re-embedded looks like.
	cache.Set(rel, "a.go::Other", "h1", testVector(99))
	if err := cache.Save(); err != nil {
		t.Fatal(err)
	}

	info, err := os.Stat(filepath.Join(dir, "shards", rel+shardEmbSuffix))
	if err != nil {
		t.Fatalf("the shard was not written: %v", err)
	}
	// Two records: header + 2 x (2 + len(uid) + dim*4). Anything near the JSON size means
	// the numbers are being written as text again.
	floor := int64(2 * ai.EmbeddingDimensions * 4)
	ceiling := floor + 512
	if info.Size() < floor || info.Size() > ceiling {
		t.Errorf("shard is %d bytes; two %d-dim float32 vectors should be %d..%d",
			info.Size(), ai.EmbeddingDimensions, floor, ceiling)
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
