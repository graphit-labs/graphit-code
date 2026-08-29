package ai

import (
	"context"
	"fmt"
	"strings"
)

const voyageDefaultBaseURL = "https://api.voyageai.com/v1"

const voyageEmbedBatchLimit = 128 // Voyage's documented limit for texts per /embeddings call.

type voyageEmbeddingConfig struct {
	baseURL string
	apiKey  string
	model   string
}

type voyageEmbeddingClient struct {
	baseURL string
	apiKey  string
	model   string
	dim     int
}

type voyageEmbedRequest struct {
	Model     string   `json:"model"`
	Input     []string `json:"input"`
	InputType string   `json:"input_type"`
}

type voyageEmbedResponse struct {
	Data []struct {
		Embedding []float32 `json:"embedding"`
		Index     int       `json:"index"`
	} `json:"data"`
}

func newVoyageEmbeddingClient(cfg voyageEmbeddingConfig) (EmbeddingClient, error) {
	if strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("ai.embedding.provider \"voyage\" needs an API key (set ai.embedding.api_key or VOYAGE_API_KEY)")
	}

	dim := ResolveEmbeddingDimensions("voyage", cfg.model)
	if dim == 0 {
		return nil, fmt.Errorf("cannot determine the embedding vector width for voyage model %q — set ai.embedding.dimensions explicitly", cfg.model)
	}

	return &voyageEmbeddingClient{
		baseURL: strings.TrimRight(cfg.baseURL, "/"),
		apiKey:  cfg.apiKey,
		model:   cfg.model,
		dim:     dim,
	}, nil
}

func (c *voyageEmbeddingClient) ModelName() string { return c.model }
func (c *voyageEmbeddingClient) Dimensions() int   { return c.dim }

func (c *voyageEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	vecs, err := c.embedBatch(ctx, []string{text}, "document")
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("voyage embeddings: empty response for a single text")
	}
	return vecs[0], nil
}

func (c *voyageEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	return c.embedBatch(ctx, texts, "document")
}

// EmbedQuery embeds a search query with input_type "query" — Voyage's asymmetric query/document
// distinction, which the local client instead handles by prefixing the query text.
func (c *voyageEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	vecs, err := c.embedBatch(ctx, []string{query}, "query")
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("voyage embeddings: empty response for a query")
	}
	return vecs[0], nil
}

func (c *voyageEmbeddingClient) embedBatch(ctx context.Context, texts []string, inputType string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	out := make([][]float32, len(texts))
	for start := 0; start < len(texts); start += voyageEmbedBatchLimit {
		end := start + voyageEmbedBatchLimit
		if end > len(texts) {
			end = len(texts)
		}
		chunk := texts[start:end]

		reqBody := voyageEmbedRequest{
			Model:     c.model,
			Input:     chunk,
			InputType: inputType,
		}
		var respBody voyageEmbedResponse
		if err := postJSON(ctx, httpClient, c.baseURL+"/embeddings", bearerAuth(c.apiKey), reqBody, &respBody); err != nil {
			return nil, fmt.Errorf("voyage embeddings: %w", err)
		}
		if len(respBody.Data) != len(chunk) {
			return nil, fmt.Errorf("voyage embeddings: got %d vectors for %d texts", len(respBody.Data), len(chunk))
		}
		for _, d := range respBody.Data {
			if d.Index < 0 || d.Index >= len(chunk) {
				return nil, fmt.Errorf("voyage embeddings: response index %d out of range for %d texts", d.Index, len(chunk))
			}
			out[start+d.Index] = d.Embedding
		}
	}
	return out, nil
}
