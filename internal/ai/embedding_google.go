package ai

import (
	"context"
	"fmt"
	"strings"
)

const googleDefaultBaseURL = "https://generativelanguage.googleapis.com/v1beta"

const googleEmbedBatchLimit = 100

type googleEmbeddingConfig struct {
	baseURL string
	apiKey  string
	model   string
}

type googleEmbeddingClient struct {
	baseURL string
	apiKey  string
	model   string
	dim     int
}

type googleContentPart struct {
	Text string `json:"text"`
}

type googleContent struct {
	Parts []googleContentPart `json:"parts"`
}

type googleEmbedContentRequest struct {
	Content googleContent `json:"content"`
}

type googleEmbedContentResponse struct {
	Embedding struct {
		Values []float32 `json:"values"`
	} `json:"embedding"`
}

type googleBatchRequestItem struct {
	Model   string        `json:"model"`
	Content googleContent `json:"content"`
}

type googleBatchEmbedRequest struct {
	Requests []googleBatchRequestItem `json:"requests"`
}

type googleBatchEmbedResponse struct {
	Embeddings []struct {
		Values []float32 `json:"values"`
	} `json:"embeddings"`
}

func newGoogleEmbeddingClient(cfg googleEmbeddingConfig) (EmbeddingClient, error) {
	if strings.TrimSpace(cfg.apiKey) == "" {
		return nil, fmt.Errorf("ai.embedding.provider \"google\" needs an API key (set ai.embedding.api_key, GOOGLE_API_KEY or GEMINI_API_KEY)")
	}

	dim := ResolveEmbeddingDimensions("google", cfg.model)
	if dim == 0 {
		return nil, fmt.Errorf("cannot determine the embedding vector width for google model %q — set ai.embedding.dimensions explicitly", cfg.model)
	}

	return &googleEmbeddingClient{
		baseURL: strings.TrimRight(cfg.baseURL, "/"),
		apiKey:  cfg.apiKey,
		model:   cfg.model,
		dim:     dim,
	}, nil
}

func (c *googleEmbeddingClient) ModelName() string { return c.model }
func (c *googleEmbeddingClient) Dimensions() int   { return c.dim }

// Embed uses the single-text embedContent endpoint rather than routing through EmbedBatch: it is
// the cheaper call for one text and, unlike batchEmbedContents, does not need the model repeated
// per item.
func (c *googleEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	reqBody := googleEmbedContentRequest{Content: googleContent{Parts: []googleContentPart{{Text: text}}}}
	var respBody googleEmbedContentResponse
	url := fmt.Sprintf("%s/models/%s:embedContent", c.baseURL, c.model)
	if err := postJSON(ctx, httpClient, url, headerAuth("x-goog-api-key", c.apiKey), reqBody, &respBody); err != nil {
		return nil, fmt.Errorf("google embedContent: %w", err)
	}
	if len(respBody.Embedding.Values) == 0 {
		return nil, fmt.Errorf("google embedContent: empty embedding in response")
	}
	return respBody.Embedding.Values, nil
}

func (c *googleEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	out := make([][]float32, len(texts))
	modelPath := "models/" + c.model
	url := fmt.Sprintf("%s/models/%s:batchEmbedContents", c.baseURL, c.model)

	for start := 0; start < len(texts); start += googleEmbedBatchLimit {
		end := start + googleEmbedBatchLimit
		if end > len(texts) {
			end = len(texts)
		}
		chunk := texts[start:end]

		reqBody := googleBatchEmbedRequest{Requests: make([]googleBatchRequestItem, len(chunk))}
		for i, text := range chunk {
			reqBody.Requests[i] = googleBatchRequestItem{
				Model:   modelPath,
				Content: googleContent{Parts: []googleContentPart{{Text: text}}},
			}
		}

		var respBody googleBatchEmbedResponse
		if err := postJSON(ctx, httpClient, url, headerAuth("x-goog-api-key", c.apiKey), reqBody, &respBody); err != nil {
			return nil, fmt.Errorf("google batchEmbedContents: %w", err)
		}
		if len(respBody.Embeddings) != len(chunk) {
			return nil, fmt.Errorf("google batchEmbedContents: got %d vectors for %d texts", len(respBody.Embeddings), len(chunk))
		}
		for i, e := range respBody.Embeddings {
			out[start+i] = e.Values
		}
	}
	return out, nil
}
