package ai

import (
	"context"
	"fmt"
	"strings"
)

const openAIDefaultBaseURL = "https://api.openai.com/v1"

var openAIDimensionsCapableModels = map[string]bool{
	"text-embedding-3-small": true,
	"text-embedding-3-large": true,
}

type openAIEmbeddingConfig struct {
	provider   string
	baseURL    string
	apiKey     string
	requireKey bool
	model      string
}

type openAIEmbeddingClient struct {
	provider string
	baseURL  string
	apiKey   string
	model    string
	dim      int
}

const openAIEmbedBatchLimit = 2048

type openAIEmbedRequest struct {
	Model      string   `json:"model"`
	Input      []string `json:"input"`
	Dimensions int      `json:"dimensions,omitempty"`
}

type openAIEmbedResponse struct {
	Data []struct {
		Embedding []float32 `json:"embedding"`
		Index     int       `json:"index"`
	} `json:"data"`
}

func newOpenAIEmbeddingClient(cfg openAIEmbeddingConfig) (EmbeddingClient, error) {
	if cfg.requireKey && strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("ai.embedding.provider %q needs an API key (set ai.embedding.api_key or OPENAI_API_KEY)", cfg.provider)
	}

	dim := ResolveEmbeddingDimensions(cfg.provider, cfg.model)
	if dim == 0 {
		return nil, fmt.Errorf("cannot determine the embedding vector width for %s model %q — set ai.embedding.dimensions explicitly", cfg.provider, cfg.model)
	}

	return &openAIEmbeddingClient{
		provider: cfg.provider,
		baseURL:  strings.TrimRight(cfg.baseURL, "/"),
		apiKey:   cfg.apiKey,
		model:    cfg.model,
		dim:      dim,
	}, nil
}

func (c *openAIEmbeddingClient) ModelName() string { return c.model }
func (c *openAIEmbeddingClient) Dimensions() int   { return c.dim }

func (c *openAIEmbeddingClient) requestDimensions() int {
	if c.provider != "openai" || !openAIDimensionsCapableModels[c.model] {
		return 0
	}
	native, ok := knownEmbeddingDims["openai/"+c.model]
	if !ok || c.dim == native {
		return 0
	}
	return c.dim
}

func (c *openAIEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	vecs, err := c.EmbedBatch(ctx, []string{text})
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("openai embeddings: empty response for a single text")
	}
	return vecs[0], nil
}

func (c *openAIEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	out := make([][]float32, len(texts))
	for start := 0; start < len(texts); start += openAIEmbedBatchLimit {
		end := start + openAIEmbedBatchLimit
		if end > len(texts) {
			end = len(texts)
		}
		chunk := texts[start:end]

		reqBody := openAIEmbedRequest{
			Model:      c.model,
			Input:      chunk,
			Dimensions: c.requestDimensions(),
		}
		var respBody openAIEmbedResponse
		if err := postJSON(ctx, httpClient, c.baseURL+"/embeddings", bearerAuth(c.apiKey), reqBody, &respBody); err != nil {
			return nil, fmt.Errorf("%s embeddings: %w", c.provider, err)
		}
		if len(respBody.Data) != len(chunk) {
			return nil, fmt.Errorf("%s embeddings: got %d vectors for %d texts", c.provider, len(respBody.Data), len(chunk))
		}
		for _, d := range respBody.Data {
			if d.Index < 0 || d.Index >= len(chunk) {
				return nil, fmt.Errorf("%s embeddings: response index %d out of range for %d texts", c.provider, d.Index, len(chunk))
			}
			out[start+d.Index] = d.Embedding
		}
	}
	return out, nil
}
