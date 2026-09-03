//go:build lancedb

package ast

import (
	"context"
	"math"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func unitVector(components map[int]float32) []float32 {
	v := make([]float32, ai.EmbeddingDimensions)
	var norm float64
	for i, x := range components {
		v[i] = x
		norm += float64(x) * float64(x)
	}
	norm = math.Sqrt(norm)
	for i := range v {
		v[i] = float32(float64(v[i]) / norm)
	}
	return v
}

func TestVectorMetricIsSquaredL2OnUnitVectors(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)

	query := unitVector(map[int]float32{0: 1})
	cases := map[string]float64{
		"identical": 1.0,
		"half":      0.5,
		"orthogon":  0.0,
	}
	vectors := map[string][]float32{
		"identical": unitVector(map[int]float32{0: 1}),
		"half":      unitVector(map[int]float32{0: 1, 1: 1, 2: 1, 3: 1}),
		"orthogon":  unitVector(map[int]float32{1: 1}),
	}

	entries := make([]*parseCacheEntry, 0, len(vectors))
	for name := range vectors {
		entries = append(entries, entryWith(name+".go", "package p", cachedEntity{Name: name}))
	}
	metricCache := newShardCacheForTest(t, entries...)
	if err := idx.RebuildFromCache(ctx, metricCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, metricCache, func(_, uid string) []float32 {
		for name, v := range vectors {
			if len(uid) >= len(name) && uid[len(uid)-len(name):] == name {
				return v
			}
		}
		return nil
	})

	hits, err := idx.entities.Search(ctx, lancestore.Query{
		Vector: query, VectorColumn: lanceVectorColumn, Limit: 10,
	})
	if err != nil {
		t.Fatalf("vector search: %v", err)
	}
	if len(hits) != len(vectors) {
		t.Fatalf("got %d hits for %d vectors", len(hits), len(vectors))
	}

	for _, h := range hits {
		name, _ := h.Row["name"].(string)
		wantCos, ok := cases[name]
		if !ok {
			t.Fatalf("unexpected entity %q", name)
		}
		if h.Score != 0 {
			t.Errorf("%s: a vector-only query returned Score %g; if the engine now scores these, "+
				"SemanticSearch should stop deriving it", name, h.Score)
		}
		if got := cosineFromSquaredL2(h.Distance); math.Abs(got-wantCos) > 0.001 {
			t.Errorf("%s: distance %g converts to cosine %g, want %g — the engine's default vector "+
				"metric is no longer squared L2, and cosineFromSquaredL2 is now wrong",
				name, h.Distance, got, wantCos)
		}
	}
}

// SemanticSearch returned NOTHING for every query on every corpus, because the confidence floor
// compared a cosine against a relevance field that a vector-only query never fills.
func TestSemanticSearchReturnsItsNeighbours(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)

	query := unitVector(map[int]float32{0: 1})
	near := unitVector(map[int]float32{0: 1, 1: 0.1})
	far := unitVector(map[int]float32{1: 1})
	mid := unitVector(map[int]float32{0: 1, 1: 1.6})

	vectors := map[string][]float32{"near": near, "mid": mid, "far": far}
	entries := make([]*parseCacheEntry, 0, len(vectors))
	for name := range vectors {
		entries = append(entries, entryWith(name+".go", "package p", cachedEntity{Name: name}))
	}
	metricCache := newShardCacheForTest(t, entries...)
	if err := idx.RebuildFromCache(ctx, metricCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, metricCache, func(_, uid string) []float32 {
		for name, v := range vectors {
			if len(uid) >= len(name) && uid[len(uid)-len(name):] == name {
				return v
			}
		}
		return nil
	})

	res, err := idx.SemanticSearch(ctx, query, 10)
	if err != nil {
		t.Fatalf("semantic search: %v", err)
	}
	if len(res) == 0 {
		t.Fatal("semantic search returned nothing although vectors were written")
	}
	if res[0].Name != "near" {
		t.Errorf("nearest is %q, want near", res[0].Name)
	}
	if res[0].RelevanceScore < semanticFloorCosine {
		t.Errorf("the nearest neighbour scores %g, below the floor %g — the relevance field is "+
			"not being derived from the distance", res[0].RelevanceScore, semanticFloorCosine)
	}

	for _, r := range res {
		if r.Name == "far" {
			t.Errorf("an orthogonal neighbour survived the confidence floor (score %g)", r.RelevanceScore)
		}
	}
}
