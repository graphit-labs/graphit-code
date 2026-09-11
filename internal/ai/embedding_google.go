package ai

import (
	"context"
	"fmt"
	"strings"
)

const googleDefaultBaseURL = "https://generativelanguage.googleapis.com/v1beta"

const googleEmbedBatchLimit = 100

type googleEmbeddingConfig struct {
	baseURL    string
	apiKey     string
	model      string
	dimensions int
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
	Content              googleContent `json:"content"`
	TaskType             string        `json:"taskType,omitempty"`
	OutputDimensionality int           `json:"outputDimensionality,omitempty"`
}

type googleEmbedContentResponse struct {
	Embedding struct {
		Values []float32 `json:"values"`
	} `json:"embedding"`
}

type googleBatchRequestItem struct {
	Model                string        `json:"model"`
	Content              googleContent `json:"content"`
	TaskType             string        `json:"taskType,omitempty"`
	OutputDimensionality int           `json:"outputDimensionality,omitempty"`
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
		return nil, fmt.Errorf("embedding provider \"google\" needs an API key in the active login profile")
	}

	dim := cfg.dimensions
	if dim == 0 {
		dim = ResolveEmbeddingDimensions("google", cfg.model)
	}
	if dim == 0 {
		return nil, fmt.Errorf("cannot determine the embedding vector width for google model %q — set --embedding-dimensions on the provider", cfg.model)
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
	return c.embed(ctx, text, "RETRIEVAL_DOCUMENT")
}

// EmbedQuery preserves Google's asymmetric retrieval semantics by selecting the query task type.
func (c *googleEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	return c.embed(ctx, query, "RETRIEVAL_QUERY")
}

func (c *googleEmbeddingClient) embed(ctx context.Context, text, taskType string) ([]float32, error) {
	text, taskType = c.prepareRetrievalInput(text, taskType)
	reqBody := googleEmbedContentRequest{
		Content:              googleContent{Parts: []googleContentPart{{Text: text}}},
		TaskType:             taskType,
		OutputDimensionality: c.dim,
	}
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
			text, taskType := c.prepareRetrievalInput(text, "RETRIEVAL_DOCUMENT")
			reqBody.Requests[i] = googleBatchRequestItem{
				Model:                modelPath,
				Content:              googleContent{Parts: []googleContentPart{{Text: text}}},
				TaskType:             taskType,
				OutputDimensionality: c.dim,
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

// Gemini Embedding 2 replaced taskType with textual retrieval instructions. Earlier text-only
// embedding models still use the taskType field.
func (c *googleEmbeddingClient) prepareRetrievalInput(text, taskType string) (string, string) {
	if !strings.HasPrefix(strings.ToLower(c.model), "gemini-embedding-2") {
		return text, taskType
	}
	if taskType == "RETRIEVAL_QUERY" {
		return "task: search result | query: " + text, ""
	}
	return "title: none | text: " + text, ""
}
