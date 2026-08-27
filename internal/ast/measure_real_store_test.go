//go:build lancedb

package ast

import (
	"context"
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// A MEASUREMENT HARNESS, not a gate — it skips unless pointed at a real index.
//
// It exists because both defects in this area were found by running the installed binary against
// the real store and neither was reproducible from the fixtures: one needed a corpus where the
// score ranges of the two passes actually diverge, the other needed a store carried across an
// engine change. When a ranking question comes up again, this is the cheapest way to get numbers
// off a real corpus instead of arguing from a five-row fixture.
//
// MEASURE AGAINST A FROZEN COPY, never against the live store — the daemon rewrites it under you
// and the numbers stop being reproducible:
//
//	cp -r ~/.graphit/ast/project/<id>/search.lance /tmp/frozen-search.lance
//	GRAPHIT_PROBE_INDEX=/tmp/frozen-search.lance go test -tags lancedb \
//	  -run TestMeasureRealStoreScoreRanges -v ./internal/ast/
//
// What it measured on this project's own index (61,446 entities, 39,198 with a vector; 770 files):
//
//	query "evictOldestStaged"
//	  entity pass, keyword : 156.39 evictOldestStaged, 104.31 its doc comment, 53.31, 48.85, 47.95
//	  file pass,   keyword :  29.63, 24.37, 23.87, 23.16, 22.96
//	  entity pass, hybrid  : RRF sums, ~1/(60+rank), so hundredths
//
// which is what disproved the "file IDF beats entity IDF" theory: on one channel the entity leads
// by 5x. The scales only diverge once a vector is in play, and that is a difference of KIND — a
// fused rank score against a raw BM25 score — not of corpus size.
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

	// The direction is meaningless — a real query embedding needs the model, which a unit test does
	// not load. It is enough for the SCALE of a fused score, which is what this measures; it is not
	// enough to judge WHICH entity should win, so do not read the hybrid ordering here as quality.
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

		// topK == 0 is what `ast query --hybrid` passes by default, and it is the value that makes
		// the file pass run beside the entity pass — the shape the reported defect appeared in.
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
