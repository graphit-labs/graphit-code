package ai

import (
	"context"
	"fmt"
)

type brokerReranker struct {
	endpoint         string
	name             string
	expectedRevision string
	resolveToken     func(context.Context) (string, error)
}

func (b *brokerReranker) Name() string { return b.name }

func (b *brokerReranker) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	if len(candidates) == 0 {
		return nil, nil
	}
	token, err := b.resolveToken(ctx)
	if err != nil {
		return nil, err
	}
	request := struct {
		Query     string   `json:"query"`
		Documents []string `json:"documents"`
		TopN      int      `json:"top_n"`
	}{query, candidates, len(candidates)}
	var response struct {
		Results []struct {
			Index          int     `json:"index"`
			RelevanceScore float64 `json:"relevance_score"`
		} `json:"results"`
		Graphit struct {
			Revision string `json:"revision"`
		} `json:"graphit"`
	}
	if err := postJSON(ctx, httpClient, b.endpoint, bearerAuth(token), request, &response); err != nil {
		return nil, fmt.Errorf("broker rerank: %w", err)
	}
	if len(response.Results) != len(candidates) {
		return nil, fmt.Errorf("broker rerank returned %d scores for %d candidates", len(response.Results), len(candidates))
	}
	if response.Graphit.Revision != b.expectedRevision {
		return nil, fmt.Errorf("broker rerank revision changed from %q to %q; rebuild the client before continuing", b.expectedRevision, response.Graphit.Revision)
	}
	scores := make([]float64, len(candidates))
	seen := make([]bool, len(candidates))
	for _, result := range response.Results {
		if result.Index < 0 || result.Index >= len(candidates) || seen[result.Index] {
			return nil, fmt.Errorf("broker rerank returned invalid index %d", result.Index)
		}
		seen[result.Index] = true
		scores[result.Index] = result.RelevanceScore
	}
	return scores, nil
}
