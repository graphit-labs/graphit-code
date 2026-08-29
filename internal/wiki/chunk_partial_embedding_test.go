//go:build lancedb

package wiki

import (
	"context"
	"fmt"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// partialChunks builds n chunks, all long enough to qualify for a vector.
//
// Length matters: writeChunks attaches a cached vector only when the chunk has at least
// wikiEmbedMinWords, so a fixture of short bodies silently makes every row vectorless and
// stops exercising the mix this file is about.
func partialChunks(n int) []WikiChunk {
	body := "palavra "
	for len(body) < 40*len("palavra ") {
		body += "palavra "
	}
	chunks := make([]WikiChunk, 0, n)
	for i := range n {
		chunks = append(chunks, WikiChunk{
			Slug:        fmt.Sprintf("pagina-%02d", i),
			Title:       fmt.Sprintf("Página %02d", i),
			Body:        body,
			Summary:     "resumo",
			DocType:     "nota",
			ContentHash: fmt.Sprintf("hash-%02d", i),
			WordCount:   40,
			Updated:     "2026-08-18",
		})
	}
	return chunks
}

// TestRebuildIndexesAWikiWhoseEmbeddingIsPartial pins the shape a real wiki produces and a
// synthetic fixture does not: one insert batch holding both chunks that carry a vector and
// chunks that do not.
//
// The driver builds a single LIST for the whole $batch parameter and refuses mixed element
// types, so before writeChunks partitioned the batch this failed the ENTIRE rebuild with
// "failed to create LIST value ... please make sure all the values are of the same type" —
// which the atomic swap turns into the worst available outcome: the temp store is discarded,
// the live index is left as it was, and nothing about the directory says the index is stale.
//
// Partial embedding is the normal state of a wiki, not an edge case. A chunk gets a vector
// only if its text is unchanged, already in the cache, and long enough, so any wiki the
// embedder has not finished — and any wiki holding one short page — lands here. The existing
// fixtures never did, because they give every chunk a vector or none.
func TestRebuildIndexesAWikiWhoseEmbeddingIsPartial(t *testing.T) {
	chunks := partialChunks(6)

	for _, tc := range []struct {
		name    string
		cached  func(i int) bool
		wantVec int
	}{
		{"every other chunk", func(i int) bool { return i%2 == 0 }, 3},
		{"only the first", func(i int) bool { return i == 0 }, 1},
		{"only the last", func(i int) bool { return i == len(chunks)-1 }, 1},
		{"all of them", func(int) bool { return true }, 6},
		{"none of them", func(int) bool { return false }, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			cache := EmbeddingCache{}
			for i, c := range chunks {
				if tc.cached(i) {
					cache[c.ContentHash] = unitVec(i)
				}
			}

			db := newTestWikiDB(t)
			if err := db.Rebuild(context.Background(), chunks, nil, nil, cache); err != nil {
				t.Fatalf("Rebuild with a partially embedded wiki: %v", err)
			}

			total, _, _, _, err := db.Stats(context.Background())
			if err != nil {
				t.Fatalf("Stats: %v", err)
			}
			if total != len(chunks) {
				t.Errorf("indexed %d chunks, want %d — a rebuild that drops rows is the "+
					"failure this test exists to catch", total, len(chunks))
			}

			vectors, _ := db.EmbeddingStats(context.Background())
			if vectors != tc.wantVec {
				t.Errorf("kept %d vectors, want %d — partitioning the batch must not cost a "+
					"chunk its embedding", vectors, tc.wantVec)
			}
		})
	}
}

// TestPartialEmbeddingSurvivesMoreThanOneBatchWorth runs the same mix over a corpus large
// enough to cross any batch boundary a write might introduce.
//
// The write is one transaction with a prepared statement today, so there is no boundary to
// cross — which is exactly why the size is spelled out here rather than derived from a
// constant that no longer exists. The test is about the corpus, and it keeps its meaning if
// batching ever comes back.
func TestPartialEmbeddingSurvivesMoreThanOneBatchWorth(t *testing.T) {
	chunks := partialChunks(507)
	cache := EmbeddingCache{}
	for i, c := range chunks {
		if i%3 == 0 {
			cache[c.ContentHash] = unitVec(i % ai.EmbeddingDimensions)
		}
	}

	db := newTestWikiDB(t)
	if err := db.Rebuild(context.Background(), chunks, nil, nil, cache); err != nil {
		t.Fatalf("Rebuild across a flush boundary: %v", err)
	}
	total, _, _, _, err := db.Stats(context.Background())
	if err != nil {
		t.Fatalf("Stats: %v", err)
	}
	if total != len(chunks) {
		t.Errorf("indexed %d chunks, want %d", total, len(chunks))
	}
}
