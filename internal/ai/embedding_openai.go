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
	provider         string
	baseURL          string
	apiKey           string
	requireKey       bool
	model            string
	dimensions       int
	endpoint         string
	apiKeyResolver   func(context.Context) (string, error)
	expectedRevision string
}

type openAIEmbeddingClient struct {
	provider         string
	baseURL          string
	apiKey           string
	endpoint         string
	apiKeyResolver   func(context.Context) (string, error)
	expectedRevision string
	model            string
	dim              int
}

const openAIEmbedBatchLimit = 2048

type openAIEmbedRequest struct {
	Model      string   `json:"model,omitempty"`
	Input      []string `json:"input"`
	Dimensions int      `json:"dimensions,omitempty"`
	InputType  string   `json:"input_type,omitempty"`
}

type openAIEmbedResponse struct {
	Data []struct {
		Embedding []float32 `json:"embedding"`
		Index     int       `json:"index"`
	} `json:"data"`
	Graphit struct {
		Revision   string `json:"revision"`
		Dimensions int    `json:"dimensions"`
	} `json:"graphit"`
}

func newOpenAIEmbeddingClient(cfg openAIEmbeddingConfig) (EmbeddingClient, error) {
	if cfg.requireKey && strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("embedding provider %q needs an API key in the active login profile", cfg.provider)
	}

	dim := cfg.dimensions
	if dim == 0 {
		dim = ResolveEmbeddingDimensions(cfg.provider, cfg.model)
	}
	if dim == 0 {
		return nil, fmt.Errorf("cannot determine the embedding vector width for %s model %q — set --embedding-dimensions on the provider", cfg.provider, cfg.model)
	}

	return &openAIEmbeddingClient{
		provider:         cfg.provider,
		baseURL:          strings.TrimRight(cfg.baseURL, "/"),
		apiKey:           cfg.apiKey,
		model:            cfg.model,
		dim:              dim,
		endpoint:         cfg.endpoint,
		apiKeyResolver:   cfg.apiKeyResolver,
		expectedRevision: cfg.expectedRevision,
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
	inputType := ""
	if c.provider == "broker" {
		inputType = "document"
	}
	return c.embedBatch(ctx, texts, inputType)
}

// EmbedQuery preserves asymmetric retrieval semantics when the OpenAI-shaped
// endpoint is Graphit Broker. Ordinary OpenAI-compatible providers continue to
// receive the unextended request body.
func (c *openAIEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	inputType := ""
	if c.provider == "broker" {
		inputType = "query"
	}
	vectors, err := c.embedBatch(ctx, []string{query}, inputType)
	if err != nil {
		return nil, err
	}
	if len(vectors) == 0 {
		return nil, fmt.Errorf("%s embeddings: empty response for a query", c.provider)
	}
	return vectors[0], nil
}

func (c *openAIEmbeddingClient) embedBatch(ctx context.Context, texts []string, inputType string) ([][]float32, error) {
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
			InputType:  inputType,
		}
		if c.provider == "broker" {
			reqBody.Model = ""
		}
		var respBody openAIEmbedResponse
		apiKey := c.apiKey
		if c.apiKeyResolver != nil {
			var err error
			apiKey, err = c.apiKeyResolver(ctx)
			if err != nil {
				return nil, err
			}
		}
		endpoint := c.endpoint
		if endpoint == "" {
			endpoint = c.baseURL + "/embeddings"
		}
		if err := postJSON(ctx, httpClient, endpoint, bearerAuth(apiKey), reqBody, &respBody); err != nil {
			return nil, fmt.Errorf("%s embeddings: %w", c.provider, err)
		}
		if c.expectedRevision != "" && (respBody.Graphit.Revision != c.expectedRevision || respBody.Graphit.Dimensions != c.dim) {
			return nil, fmt.Errorf("broker embedding revision or dimensions changed from %s/%d to %s/%d; rebuild the index before continuing", c.expectedRevision, c.dim, respBody.Graphit.Revision, respBody.Graphit.Dimensions)
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
