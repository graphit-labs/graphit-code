package ai

import (
	"context"
	"fmt"
	"strings"
)

const jinaRerankDefaultBaseURL = "https://api.jina.ai"

type jinaRerankConfig struct {
	baseURL string
	apiKey  string
	model   string
}

type jinaReranker struct {
	baseURL string
	apiKey  string
	model   string
}

type jinaRerankRequest struct {
	Model     string   `json:"model"`
	Query     string   `json:"query"`
	Documents []string `json:"documents"`
	TopN      int      `json:"top_n"`
}

type jinaRerankResponse struct {
	Results []struct {
		Index          int     `json:"index"`
		RelevanceScore float64 `json:"relevance_score"`
	} `json:"results"`
}

func newJinaReranker(cfg jinaRerankConfig) (Scorer, error) {
	if strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("ai.rerank.provider \"jina\" needs an API key (set ai.rerank.api_key or JINA_API_KEY)")
	}
	return &jinaReranker{
		baseURL: strings.TrimRight(cfg.baseURL, "/"),
		apiKey:  cfg.apiKey,
		model:   cfg.model,
	}, nil
}

func (j *jinaReranker) Name() string { return "jina/" + j.model }

// Score maps Jina's response — which is NOT necessarily in input order — back onto the
// candidates' original positions, which is what RerankAdapter (and every other Scorer) expects.
func (j *jinaReranker) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	if len(candidates) == 0 {
		return nil, nil
	}

	reqBody := jinaRerankRequest{
		Model:     j.model,
		Query:     query,
		Documents: candidates,
		TopN:      len(candidates),
	}
	var respBody jinaRerankResponse
	if err := postJSON(ctx, httpClient, j.baseURL+"/v1/rerank", bearerAuth(j.apiKey), reqBody, &respBody); err != nil {
		return nil, fmt.Errorf("jina rerank: %w", err)
	}

	out := make([]float64, len(candidates))
	for _, r := range respBody.Results {
		if r.Index < 0 || r.Index >= len(candidates) {
			return nil, fmt.Errorf("jina rerank: result index %d out of range for %d documents", r.Index, len(candidates))
		}
		out[r.Index] = r.RelevanceScore
	}
	return out, nil
}
