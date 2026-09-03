package ai

import (
	"context"
	"fmt"
	"strings"
)

const cohereRerankDefaultBaseURL = "https://api.cohere.com"

const cohereRerankChunkSize = 200

type cohereRerankConfig struct {
	baseURL string
	apiKey  string
	model   string
}

type cohereReranker struct {
	baseURL string
	apiKey  string
	model   string
}

type cohereRerankRequest struct {
	Model     string   `json:"model"`
	Query     string   `json:"query"`
	Documents []string `json:"documents"`
	TopN      int      `json:"top_n"`
}

type cohereRerankResponse struct {
	Results []struct {
		Index          int     `json:"index"`
		RelevanceScore float64 `json:"relevance_score"`
	} `json:"results"`
}

func newCohereReranker(cfg cohereRerankConfig) (Scorer, error) {
	if strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("ai.rerank.provider \"cohere\" needs an API key (set ai.rerank.api_key or COHERE_API_KEY)")
	}
	return &cohereReranker{
		baseURL: strings.TrimRight(cfg.baseURL, "/"),
		apiKey:  cfg.apiKey,
		model:   cfg.model,
	}, nil
}

func (c *cohereReranker) Name() string { return "cohere/" + c.model }

// Score maps Cohere's response — which is NOT necessarily in input order — back onto the
// candidates' original positions, which is what RerankAdapter (and every other Scorer) expects.
func (c *cohereReranker) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	if len(candidates) == 0 {
		return nil, nil
	}

	out := make([]float64, len(candidates))
	for start := 0; start < len(candidates); start += cohereRerankChunkSize {
		end := start + cohereRerankChunkSize
		if end > len(candidates) {
			end = len(candidates)
		}
		chunk := candidates[start:end]

		reqBody := cohereRerankRequest{
			Model:     c.model,
			Query:     query,
			Documents: chunk,
			TopN:      len(chunk),
		}
		var respBody cohereRerankResponse
		if err := postJSON(ctx, httpClient, c.baseURL+"/v2/rerank", bearerAuth(c.apiKey), reqBody, &respBody); err != nil {
			return nil, fmt.Errorf("cohere rerank: %w", err)
		}
		for _, r := range respBody.Results {
			if r.Index < 0 || r.Index >= len(chunk) {
				return nil, fmt.Errorf("cohere rerank: result index %d out of range for %d documents", r.Index, len(chunk))
			}
			out[start+r.Index] = r.RelevanceScore
		}
	}
	return out, nil
}
