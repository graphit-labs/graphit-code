package lancererank

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// stubScorer scores a candidate by how many query words it contains.
type stubScorer struct{ calls int }

func (s *stubScorer) Name() string { return "stub" }

func (s *stubScorer) Score(_ context.Context, query string, candidates []string) ([]float64, error) {
	s.calls++
	words := strings.Fields(query)
	out := make([]float64, len(candidates))
	for i, c := range candidates {
		for _, w := range words {
			if strings.Contains(c, w) {
				out[i]++
			}
		}
	}
	return out, nil
}

func TestLanceRerankerUsesReadableFieldsAndPreservesRows(t *testing.T) {
	scorer := &stubScorer{}
	ranker := lanceReranker{adapter: &ai.RerankAdapter{Scorer: scorer}}
	hits := []lancestore.Hit{
		{Row: lancestore.Row{"name": "parseToken", "docstring": "unrelated"}, Score: 3},
		{Row: lancestore.Row{"name": "loadAccount", "docstring": "token account"}, Score: 1},
	}
	got, err := ranker.Rerank(context.Background(), "token account", hits)
	if err != nil {
		t.Fatal(err)
	}
	if got[0].Row["name"] != "loadAccount" || got[0].Score != 2 {
		t.Fatalf("reranked hits = %#v", got)
	}
	if len(got) != len(hits) || scorer.calls != 1 {
		t.Fatalf("len=%d calls=%d", len(got), scorer.calls)
	}
}

func TestLanceRerankTextSupportsWikiRows(t *testing.T) {
	text := lanceRerankText(lancestore.Row{"title": "Authentication", "summary": "OIDC and ACL", "body": "Broker details", "slug": "auth"})
	for _, want := range []string{"Authentication", "OIDC and ACL", "Broker details", "auth"} {
		if !strings.Contains(text, want) {
			t.Fatalf("rerank text %q does not contain %q", text, want)
		}
	}
}
