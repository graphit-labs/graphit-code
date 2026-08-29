package ai

import (
	"context"
	"fmt"
	"strings"
)

const cohereDefaultBaseURL = "https://api.cohere.com/v2"

const cohereEmbedBatchLimit = 96 // Cohere's documented limit for texts per /embed call.

// cohereEmbeddingConfig configures a Cohere v2 /embed client.
type cohereEmbeddingConfig struct {
	baseURL string
	apiKey  string
	model   string
}

type cohereEmbeddingClient struct {
	baseURL string
	apiKey  string
	model   string
	dim     int
}

type cohereEmbedRequest struct {
	Model          string   `json:"model"`
	Texts          []string `json:"texts"`
	InputType      string   `json:"input_type"`
	EmbeddingTypes []string `json:"embedding_types"`
}

type cohereEmbedResponse struct {
	Embeddings struct {
		Float [][]float32 `json:"float"`
	} `json:"embeddings"`
}

func newCohereEmbeddingClient(cfg cohereEmbeddingConfig) (EmbeddingClient, error) {
	if strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("ai.embedding.provider \"cohere\" needs an API key (set ai.embedding.api_key or COHERE_API_KEY)")
	}

	dim := ResolveEmbeddingDimensions("cohere", cfg.model)
	if dim == 0 {
		return nil, fmt.Errorf("cannot determine the embedding vector width for cohere model %q — set ai.embedding.dimensions explicitly", cfg.model)
	}

	return &cohereEmbeddingClient{
		baseURL: strings.TrimRight(cfg.baseURL, "/"),
		apiKey:  cfg.apiKey,
		model:   cfg.model,
		dim:     dim,
	}, nil
}

func (c *cohereEmbeddingClient) ModelName() string { return c.model }
func (c *cohereEmbeddingClient) Dimensions() int   { return c.dim }

func (c *cohereEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	vecs, err := c.embedBatch(ctx, []string{text}, "search_document")
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("cohere embed: empty response for a single text")
	}
	return vecs[0], nil
}

func (c *cohereEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	return c.embedBatch(ctx, texts, "search_document")
}

// EmbedQuery embeds a search query with input_type "search_query" — Cohere's asymmetric
// query/document distinction, which the local client instead handles by prefixing the query text.
func (c *cohereEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	vecs, err := c.embedBatch(ctx, []string{query}, "search_query")
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("cohere embed: empty response for a query")
	}
	return vecs[0], nil
}

func (c *cohereEmbeddingClient) embedBatch(ctx context.Context, texts []string, inputType string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	out := make([][]float32, 0, len(texts))
	for start := 0; start < len(texts); start += cohereEmbedBatchLimit {
		end := start + cohereEmbedBatchLimit
		if end > len(texts) {
			end = len(texts)
		}
		chunk := texts[start:end]

		reqBody := cohereEmbedRequest{
			Model:          c.model,
			Texts:          chunk,
			InputType:      inputType,
			EmbeddingTypes: []string{"float"},
		}
		var respBody cohereEmbedResponse
		if err := postJSON(ctx, httpClient, c.baseURL+"/embed", bearerAuth(c.apiKey), reqBody, &respBody); err != nil {
			return nil, fmt.Errorf("cohere embed: %w", err)
		}
		if len(respBody.Embeddings.Float) != len(chunk) {
			return nil, fmt.Errorf("cohere embed: got %d vectors for %d texts", len(respBody.Embeddings.Float), len(chunk))
		}
		out = append(out, respBody.Embeddings.Float...)
	}
	return out, nil
}
