package ai

import (
	"context"
	"fmt"
	"strings"
)

const openAIDefaultBaseURL = "https://api.openai.com/v1"

// openAIDimensionsCapableModels are the OpenAI embedding models trained with Matryoshka
// representation learning, which is what makes a request-time `dimensions` truncation valid.
// text-embedding-ada-002 predates this and has no such support — asking it for a narrower vector
// is simply not a thing its API accepts, so it is deliberately absent here.
var openAIDimensionsCapableModels = map[string]bool{
	"text-embedding-3-small": true,
	"text-embedding-3-large": true,
}

// openAIEmbeddingConfig configures the client for BOTH provider "openai" and provider
// "openai-compatible" — they are the same wire format. This literally IS the OpenAI
// /v1/embeddings shape, which is what "compatible" means: Ollama, vLLM, LM Studio, TEI, Together
// AI and friends all speak it. One client type serves both; requireKey and model are what differ.
type openAIEmbeddingConfig struct {
	provider   string // "openai" or "openai-compatible"
	baseURL    string
	apiKey     string
	requireKey bool
	model      string
}

// openAIEmbeddingClient implements EmbeddingClient over the OpenAI /v1/embeddings shape.
type openAIEmbeddingClient struct {
	provider string
	baseURL  string
	apiKey   string
	model    string
	dim      int
}

const openAIEmbedBatchLimit = 2048 // OpenAI's documented request limit for /v1/embeddings.

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

// requestDimensions returns the width to ask the API to truncate to via the request-time
// `dimensions` field, or 0 to omit it.
//
// It only applies to provider "openai" (an openai-compatible server's actual model is unknown to
// this code, so sending a field it may not support risks a 400 for no benefit — the operator can
// truncate on their own end if their server supports it) and only to the two v3 models that
// support Matryoshka truncation. It fires when the resolved width differs from that model's
// native table width, which is exactly the case where ai.embedding.dimensions was set to
// something other than the default — sending it lets OpenAI truncate server-side, which is
// strictly better than truncating the vector after the fact.
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
