package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestNewOpenAIEmbeddingClient_MissingKeyWhenRequired(t *testing.T) {
	_, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai",
		baseURL:    openAIDefaultBaseURL,
		requireKey: true,
		model:      "text-embedding-3-small",
	})
	if err == nil {
		t.Fatal("expected an error for a missing required API key")
	}
}

func TestNewOpenAIEmbeddingClient_OpenAICompatibleAllowsNoKey(t *testing.T) {
	// A self-hosted server has no entry in the known-model table, so the width must come from an
	// explicit override — this test is about the missing-key path, not the width resolution.
	t.Setenv("GRAPHIT_AI_EMBEDDING_DIMENSIONS", "1536")
	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai-compatible",
		baseURL:    "http://localhost:1234/v1",
		requireKey: false,
		model:      "text-embedding-3-small",
	})
	if err != nil {
		t.Fatalf("openai-compatible without a key should be accepted: %v", err)
	}
	if c.ModelName() != "text-embedding-3-small" {
		t.Errorf("ModelName = %q", c.ModelName())
	}
}

// A width that cannot be resolved must refuse to build a client rather than guess — a wrong width
// silently corrupts the vector store.
func TestNewOpenAIEmbeddingClient_UnknownModelDimensionsIsAnError(t *testing.T) {
	_, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai-compatible",
		baseURL:    "http://localhost:1234/v1",
		requireKey: false,
		model:      "some-custom-self-hosted-model",
	})
	if err == nil {
		t.Fatal("expected an error when the vector width cannot be resolved")
	}
	if !strings.Contains(err.Error(), "some-custom-self-hosted-model") {
		t.Errorf("error = %v, want it to name the model", err)
	}
}

func TestNewOpenAIEmbeddingClient_DimensionsOverrideResolves(t *testing.T) {
	t.Setenv("GRAPHIT_AI_EMBEDDING_DIMENSIONS", "512")
	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai-compatible",
		baseURL:    "http://localhost:1234/v1",
		requireKey: false,
		model:      "some-custom-model",
	})
	if err != nil {
		t.Fatalf("newOpenAIEmbeddingClient: %v", err)
	}
	if c.Dimensions() != 512 {
		t.Errorf("Dimensions() = %d, want 512", c.Dimensions())
	}
}

func TestOpenAIEmbeddingClient_EmbedBatch_MapsResponseIndexBackToInputOrder(t *testing.T) {
	var gotBody map[string]any
	var gotPath string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		if got := r.Header.Get("Authorization"); got != "Bearer test-key" {
			t.Errorf("Authorization = %q, want Bearer test-key", got)
		}
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		// Deliberately out of input order, to prove the client maps by index rather than by
		// response position.
		_, _ = w.Write([]byte(`{"data":[{"embedding":[0.1,0.2],"index":1},{"embedding":[0.3,0.4],"index":0}]}`))
	}))
	defer srv.Close()

	t.Setenv("GRAPHIT_AI_EMBEDDING_DIMENSIONS", "2")
	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai",
		baseURL:    srv.URL,
		apiKey:     "test-key",
		requireKey: true,
		model:      "text-embedding-3-small",
	})
	if err != nil {
		t.Fatalf("newOpenAIEmbeddingClient: %v", err)
	}

	vecs, err := c.EmbedBatch(context.Background(), []string{"a", "b"})
	if err != nil {
		t.Fatalf("EmbedBatch: %v", err)
	}
	if gotPath != "/embeddings" {
		t.Errorf("path = %q, want /embeddings", gotPath)
	}
	if len(vecs) != 2 {
		t.Fatalf("got %d vectors, want 2", len(vecs))
	}
	if vecs[0][0] != 0.3 || vecs[1][0] != 0.1 {
		t.Errorf("vectors were not mapped back to input order: %v", vecs)
	}
	if gotBody["model"] != "text-embedding-3-small" {
		t.Errorf("model = %v", gotBody["model"])
	}
	dimsVal, ok := gotBody["dimensions"]
	if !ok || dimsVal.(float64) != 2 {
		t.Errorf("dimensions field = %v (present=%v), want 2 (override differs from native 1536)", gotBody["dimensions"], ok)
	}
}

// Sending a Matryoshka `dimensions` field to a server whose actual model is unknown to this code
// risks a 400 for no benefit, so openai-compatible must never send it.
func TestOpenAIEmbeddingClient_OpenAICompatibleOmitsDimensionsField(t *testing.T) {
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		_, _ = w.Write([]byte(`{"data":[{"embedding":[0.1],"index":0}]}`))
	}))
	defer srv.Close()

	t.Setenv("GRAPHIT_AI_EMBEDDING_DIMENSIONS", "1")
	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai-compatible",
		baseURL:    srv.URL,
		requireKey: false,
		model:      "text-embedding-3-small",
	})
	if err != nil {
		t.Fatalf("newOpenAIEmbeddingClient: %v", err)
	}
	if _, err := c.Embed(context.Background(), "hi"); err != nil {
		t.Fatalf("Embed: %v", err)
	}
	if _, ok := gotBody["dimensions"]; ok {
		t.Errorf("openai-compatible request sent a dimensions field: %v", gotBody)
	}
}

// Sending `dimensions` at all for ada-002 would be rejected by OpenAI's API — it predates
// Matryoshka truncation — so it must never be requested even with an override in play.
func TestOpenAIEmbeddingClient_AdaOmitsDimensionsField(t *testing.T) {
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		_, _ = w.Write([]byte(`{"data":[{"embedding":[0.1],"index":0}]}`))
	}))
	defer srv.Close()

	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai",
		baseURL:    srv.URL,
		apiKey:     "k",
		requireKey: true,
		model:      "text-embedding-ada-002",
	})
	if err != nil {
		t.Fatalf("newOpenAIEmbeddingClient: %v", err)
	}
	if _, err := c.Embed(context.Background(), "hi"); err != nil {
		t.Fatalf("Embed: %v", err)
	}
	if _, ok := gotBody["dimensions"]; ok {
		t.Errorf("ada-002 request sent a dimensions field: %v", gotBody)
	}
}

func TestOpenAIEmbeddingClient_ChunksBatchesOverTheLimit(t *testing.T) {
	var callCount int
	var maxChunk int
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		callCount++
		var body struct {
			Input []string `json:"input"`
		}
		_ = json.NewDecoder(r.Body).Decode(&body)
		if len(body.Input) > maxChunk {
			maxChunk = len(body.Input)
		}
		data := make([]map[string]any, len(body.Input))
		for i := range body.Input {
			data[i] = map[string]any{"embedding": []float32{float32(i)}, "index": i}
		}
		b, _ := json.Marshal(map[string]any{"data": data})
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(b)
	}))
	defer srv.Close()

	t.Setenv("GRAPHIT_AI_EMBEDDING_DIMENSIONS", "1")
	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai",
		baseURL:    srv.URL,
		apiKey:     "k",
		requireKey: true,
		model:      "text-embedding-3-small",
	})
	if err != nil {
		t.Fatalf("newOpenAIEmbeddingClient: %v", err)
	}

	texts := make([]string, openAIEmbedBatchLimit+10)
	for i := range texts {
		texts[i] = "text"
	}
	vecs, err := c.EmbedBatch(context.Background(), texts)
	if err != nil {
		t.Fatalf("EmbedBatch: %v", err)
	}
	if len(vecs) != len(texts) {
		t.Fatalf("got %d vectors, want %d", len(vecs), len(texts))
	}
	if callCount != 2 {
		t.Errorf("callCount = %d, want 2 chunks for %d texts over a %d limit", callCount, len(texts), openAIEmbedBatchLimit)
	}
	if maxChunk != openAIEmbedBatchLimit {
		t.Errorf("max chunk size = %d, want %d", maxChunk, openAIEmbedBatchLimit)
	}
}

func TestOpenAIEmbeddingClient_ModelNameAndKnownDimensions(t *testing.T) {
	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai",
		baseURL:    openAIDefaultBaseURL,
		apiKey:     "k",
		requireKey: true,
		model:      "text-embedding-3-small",
	})
	if err != nil {
		t.Fatalf("newOpenAIEmbeddingClient: %v", err)
	}
	if c.ModelName() != "text-embedding-3-small" {
		t.Errorf("ModelName = %q", c.ModelName())
	}
	if c.Dimensions() != 1536 {
		t.Errorf("Dimensions = %d, want 1536 (native table width)", c.Dimensions())
	}
}

func TestOpenAIEmbeddingClient_MismatchedResponseCountIsAnError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"data":[{"embedding":[0.1],"index":0}]}`))
	}))
	defer srv.Close()

	c, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
		provider:   "openai",
		baseURL:    srv.URL,
		apiKey:     "k",
		requireKey: true,
		model:      "text-embedding-3-small",
	})
	if err != nil {
		t.Fatalf("newOpenAIEmbeddingClient: %v", err)
	}
	if _, err := c.EmbedBatch(context.Background(), []string{"a", "b"}); err == nil {
		t.Error("expected an error when the response has fewer vectors than requested texts")
	}
}
