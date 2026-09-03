//go:build lancedb

package wiki

import (
	"context"
	"fmt"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

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

// Partial embedding is the normal state of a wiki while the embedder is catching up. Syncing the
// corpus must keep both embedded and pending rows queryable.
func TestSyncIndexesAWikiWhoseEmbeddingIsPartial(t *testing.T) {
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
			db := newTestWikiDB(t)
			if err := db.Sync(context.Background(), chunks, nil, nil); err != nil {
				t.Fatalf("Sync: %v", err)
			}
			for i, chunk := range chunks {
				if tc.cached(i) {
					if err := db.SetChunkVector(context.Background(), chunk.Slug, unitVec(i)); err != nil {
						t.Fatalf("SetChunkVector(%s): %v", chunk.Slug, err)
					}
				}
			}

			total, _, _, _, err := db.Stats(context.Background())
			if err != nil {
				t.Fatalf("Stats: %v", err)
			}
			if total != len(chunks) {
				t.Errorf("indexed %d chunks, want %d — a sync that drops rows is the "+
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

func TestPartialEmbeddingSurvivesMoreThanOneBatchWorth(t *testing.T) {
	chunks := partialChunks(507)

	db := newTestWikiDB(t)
	if err := db.Sync(context.Background(), chunks, nil, nil); err != nil {
		t.Fatalf("Sync across a flush boundary: %v", err)
	}
	for i, chunk := range chunks {
		if i%3 == 0 {
			if err := db.SetChunkVector(context.Background(), chunk.Slug, unitVec(i%ai.EmbeddingDimensions)); err != nil {
				t.Fatalf("SetChunkVector(%s): %v", chunk.Slug, err)
			}
		}
	}
	total, _, _, _, err := db.Stats(context.Background())
	if err != nil {
		t.Fatalf("Stats: %v", err)
	}
	if total != len(chunks) {
		t.Errorf("indexed %d chunks, want %d", total, len(chunks))
	}
}
