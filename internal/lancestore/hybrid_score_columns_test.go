//go:build lancedb

package lancestore

import (
	"context"
	"testing"
)

// _score AND _relevance_score ARE TWO DIFFERENT COLUMNS, and a hybrid row carries both.
//
// They used to share one `case` arm in the hit assembly, both assigning to Hit.Score, inside a
// `for k, v := range row`. Go randomises map iteration, so which of the two survived was decided
// per row, per call. The caller sorts by that field, so the effect was not a wobbly number but a
// wrong ORDER: on the AST index the entity a query named by name dropped out of rank one.
//
// The two tests below are what that defect could not survive.

// TestHybridRowScoreIsStableAcrossIdenticalQueries runs the same hybrid query many times against
// ONE unchanged index. The rows cannot change, so any variation in the score comes from this
// package rather than from the engine.
//
// Measured with the columns collapsed: EVERY row returned exactly two distinct scores across
// twenty runs — RowGroups at 0.015625 three times and 1.0 seventeen times, and so on for the rest.
func TestHybridRowScoreIsStableAcrossIdenticalQueries(t *testing.T) {
	ctx := context.Background()
	tbl := hybridScoreTable(t)

	q := Query{
		Text: "fusion rankings", TextColumn: "body",
		Vector: []float32{0, 0.95, 0.05, 0}, VectorColumn: "embedding", Limit: 5,
	}

	const runs = 20
	seen := map[string]map[float64]int{}
	for i := 0; i < runs; i++ {
		hits, err := tbl.Search(ctx, q)
		if err != nil {
			t.Fatalf("run %d: %v", i, err)
		}
		if len(hits) == 0 {
			t.Fatalf("run %d returned nothing — the test would pass vacuously", i)
		}
		for _, h := range hits {
			name, _ := h.Row["name"].(string)
			if seen[name] == nil {
				seen[name] = map[float64]int{}
			}
			seen[name][h.Score]++
		}
	}
	for name, scores := range seen {
		if len(scores) > 1 {
			t.Errorf("row %q returned %d different scores across %d identical queries: %v",
				name, len(scores), runs, scores)
		}
	}
}

// TestHybridScoreAgreesWithTheEnginesOrder is the property that makes sorting by Score correct at
// all: the engine hands rows back in its fused ranking, so the score it exposes for them has to be
// monotone with that order. If it is not, every caller that sorts by score reorders the engine's
// answer into something the engine did not rank.
func TestHybridScoreAgreesWithTheEnginesOrder(t *testing.T) {
	ctx := context.Background()
	tbl := hybridScoreTable(t)

	hits, err := tbl.Search(ctx, Query{
		Text: "fusion rankings", TextColumn: "body",
		Vector: []float32{0, 0.95, 0.05, 0}, VectorColumn: "embedding", Limit: 5,
	})
	if err != nil {
		t.Fatalf("hybrid search: %v", err)
	}
	if len(hits) < 2 {
		t.Fatalf("need at least two hits to compare an order, got %d", len(hits))
	}

	for i := 1; i < len(hits); i++ {
		if hits[i].Score > hits[i-1].Score {
			t.Errorf("hit[%d] (%v) scores %g above hit[%d] (%v) at %g, so the score contradicts the engine's own order",
				i, hits[i].Row["name"], hits[i].Score, i-1, hits[i-1].Row["name"], hits[i-1].Score)
		}
	}

	// And the fused column is the one being exposed — not the raw channel score, which on this
	// fixture runs the other way (0.834, 0, 0.883, 0.939, 1.0 against the engine's order).
	for i, h := range hits {
		if h.Score != h.RelevanceScore {
			t.Errorf("hit[%d] exposes Score %g but RelevanceScore is %g; a fused query must rank by the fused column",
				i, h.Score, h.RelevanceScore)
		}
	}
}

// A keyword query has no fusion, so there is no _relevance_score and the raw channel score is the
// ranking signal. This pins that the fallback did not get lost in separating the two.
func TestKeywordRowScoreIsTheRawChannelScore(t *testing.T) {
	ctx := context.Background()
	tbl := hybridScoreTable(t)

	hits, err := tbl.Search(ctx, Query{Text: "fusion rankings", TextColumn: "body", Limit: 5})
	if err != nil {
		t.Fatalf("fts search: %v", err)
	}
	if len(hits) == 0 {
		t.Fatal("fts search returned nothing")
	}
	if hits[0].Score == 0 {
		t.Error("a keyword hit carries no score at all")
	}
	if hits[0].Score != hits[0].RawScore {
		t.Errorf("keyword hit exposes Score %g but RawScore is %g", hits[0].Score, hits[0].RawScore)
	}
	if hits[0].RelevanceScore != 0 {
		t.Errorf("a keyword query ran no fusion, so RelevanceScore should be zero, got %g",
			hits[0].RelevanceScore)
	}
}

func hybridScoreTable(t *testing.T) *Table {
	t.Helper()
	st := openLocal(t)
	tbl, err := st.CreateTable(context.Background(), "entities", testSchema())
	if err != nil {
		t.Fatalf("create table: %v", err)
	}
	populate(t, tbl)
	return tbl
}
