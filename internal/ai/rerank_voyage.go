package ai

import (
	"context"
	"fmt"
	"strings"
)

const voyageRerankDefaultBaseURL = "https://api.voyageai.com"

type voyageRerankConfig struct {
	baseURL string
	apiKey  string
	model   string
}

type voyageReranker struct {
	baseURL string
	apiKey  string
	model   string
}

type voyageRerankRequest struct {
	Model     string   `json:"model"`
	Query     string   `json:"query"`
	Documents []string `json:"documents"`
	TopK      int      `json:"top_k"`
}

type voyageRerankResponse struct {
	Data []struct {
		Index          int     `json:"index"`
		RelevanceScore float64 `json:"relevance_score"`
	} `json:"data"`
}

func newVoyageReranker(cfg voyageRerankConfig) (Scorer, error) {
	if strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("ai.rerank.provider \"voyage\" needs an API key (set ai.rerank.api_key or VOYAGE_API_KEY)")
	}
	return &voyageReranker{
		baseURL: strings.TrimRight(cfg.baseURL, "/"),
		apiKey:  cfg.apiKey,
		model:   cfg.model,
	}, nil
}

func (v *voyageReranker) Name() string { return "voyage/" + v.model }

// Score maps Voyage's response — which is NOT necessarily in input order — back onto the
// candidates' original positions, which is what RerankAdapter (and every other Scorer) expects.
func (v *voyageReranker) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	if len(candidates) == 0 {
		return nil, nil
	}

	reqBody := voyageRerankRequest{
		Model:     v.model,
		Query:     query,
		Documents: candidates,
		TopK:      len(candidates),
	}
	var respBody voyageRerankResponse
	if err := postJSON(ctx, httpClient, v.baseURL+"/v1/rerank", bearerAuth(v.apiKey), reqBody, &respBody); err != nil {
		return nil, fmt.Errorf("voyage rerank: %w", err)
	}

	out := make([]float64, len(candidates))
	for _, r := range respBody.Data {
		if r.Index < 0 || r.Index >= len(candidates) {
			return nil, fmt.Errorf("voyage rerank: result index %d out of range for %d documents", r.Index, len(candidates))
		}
		out[r.Index] = r.RelevanceScore
	}
	return out, nil
}
