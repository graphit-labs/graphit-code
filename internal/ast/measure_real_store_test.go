//go:build lancedb

package ast

import (
	"context"
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func TestMeasureRealStoreScoreRanges(t *testing.T) {
	uri := os.Getenv("GRAPHIT_PROBE_INDEX")
	if uri == "" {
		t.Skip("set GRAPHIT_PROBE_INDEX to a frozen copy of a real search.lance — see the comment above")
	}
	ctx := context.Background()
	idx, err := OpenSearchIndexAt(ctx, lancestore.Config{URI: uri})
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer func() { _ = idx.Close() }()

	if err := idx.ensureTables(ctx); err != nil {
		t.Fatalf("ensureTables: %v", err)
	}
	ents, vecs, err := idx.Counts(ctx)
	if err != nil {
		t.Fatalf("counts: %v", err)
	}
	files, err := idx.files.Count(ctx)
	if err != nil {
		t.Fatalf("file count: %v", err)
	}
	t.Logf("corpus: %d entities (%d with a vector), %d files", ents, vecs, files)

	probeVec := make([]float32, ai.EmbeddingDimensions)
	for i := range probeVec {
		probeVec[i] = float32(i%17) / 17.0
	}

	for _, query := range []string{"evictOldestStaged", "sortResultsDeterministic", "SyncRegistry"} {
		text := LanceQueryText(query)
		t.Logf("=== %q ===", query)

		entHits, err := idx.entities.Search(ctx, lancestore.Query{
			Text: text, TextColumn: lanceBodyColumn, Limit: 5,
		})
		if err != nil {
			t.Fatalf("entity pass %q: %v", query, err)
		}
		for i, h := range entHits {
			t.Logf("  ENTITY-KEYWORD[%d] score=%-12.4f %v (%v)", i, h.Score, h.Row["name"], h.Row["etype"])
		}

		fileHits, err := idx.files.Search(ctx, lancestore.Query{
			Text: text, TextColumn: lanceBodyColumn, Limit: 5,
		})
		if err != nil {
			t.Fatalf("file pass %q: %v", query, err)
		}
		for i, h := range fileHits {
			t.Logf("  FILE-KEYWORD  [%d] score=%-12.4f %v", i, h.Score, h.Row["path"])
		}

		hybHits, err := idx.entities.Search(ctx, lancestore.Query{
			Text: text, TextColumn: lanceBodyColumn,
			Vector: probeVec, VectorColumn: lanceVectorColumn, Limit: 5,
		})
		if err != nil {
			t.Fatalf("hybrid entity pass %q: %v", query, err)
		}
		for i, h := range hybHits {
			t.Logf("  ENTITY-HYBRID [%d] fused=%-12.6f raw=%-12.4f %v", i, h.RelevanceScore, h.RawScore, h.Row["name"])
		}

		merged, err := idx.HybridSearch(ctx, query, probeVec, 0)
		if err != nil {
			t.Fatalf("merged %q: %v", query, err)
		}
		for i, r := range merged {
			if i >= 8 {
				break
			}
			t.Logf("  MERGED[%d] %-10s score=%-12.6f %s (%s)", i, r.Type, r.RelevanceScore, r.Name, r.Path)
		}
	}
}
