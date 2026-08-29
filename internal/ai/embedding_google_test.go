package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestNewGoogleEmbeddingClient_MissingKeyIsAnError(t *testing.T) {
	_, err := newGoogleEmbeddingClient(googleEmbeddingConfig{
		baseURL: googleDefaultBaseURL,
		model:   "text-embedding-004",
	})
	if err == nil {
		t.Fatal("expected an error for a missing API key")
	}
}

func TestNewGoogleEmbeddingClient_UnknownModelDimensionsIsAnError(t *testing.T) {
	_, err := newGoogleEmbeddingClient(googleEmbeddingConfig{
		baseURL: googleDefaultBaseURL,
		apiKey:  "k",
		model:   "some-future-gemini-embedding-model",
	})
	if err == nil {
		t.Fatal("expected an error when the vector width cannot be resolved")
	}
}

func TestGoogleEmbeddingClient_Embed_UsesEmbedContentEndpoint(t *testing.T) {
	var gotPath, gotAuthHeader string
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAuthHeader = r.Header.Get("x-goog-api-key")
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"embedding":{"values":[0.1,0.2,0.3]}}`))
	}))
	defer srv.Close()

	c, err := newGoogleEmbeddingClient(googleEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "test-key",
		model:   "text-embedding-004",
	})
	if err != nil {
		t.Fatalf("newGoogleEmbeddingClient: %v", err)
	}

	vec, err := c.Embed(context.Background(), "hello")
	if err != nil {
		t.Fatalf("Embed: %v", err)
	}
	if gotPath != "/models/text-embedding-004:embedContent" {
		t.Errorf("path = %q, want /models/text-embedding-004:embedContent", gotPath)
	}
	if gotAuthHeader != "test-key" {
		t.Errorf("x-goog-api-key = %q, want test-key", gotAuthHeader)
	}
	content, ok := gotBody["content"].(map[string]any)
	if !ok {
		t.Fatalf("request body missing content: %v", gotBody)
	}
	parts, ok := content["parts"].([]any)
	if !ok || len(parts) != 1 {
		t.Fatalf("request body content.parts = %v", content["parts"])
	}
	if vec[0] != 0.1 {
		t.Errorf("vec = %v", vec)
	}
}

func TestGoogleEmbeddingClient_EmbedBatch_UsesBatchEmbedContentsEndpoint(t *testing.T) {
	var gotPath string
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"embeddings":[{"values":[0.1]},{"values":[0.2]}]}`))
	}))
	defer srv.Close()

	c, err := newGoogleEmbeddingClient(googleEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "test-key",
		model:   "text-embedding-004",
	})
	if err != nil {
		t.Fatalf("newGoogleEmbeddingClient: %v", err)
	}

	vecs, err := c.EmbedBatch(context.Background(), []string{"a", "b"})
	if err != nil {
		t.Fatalf("EmbedBatch: %v", err)
	}
	if gotPath != "/models/text-embedding-004:batchEmbedContents" {
		t.Errorf("path = %q, want /models/text-embedding-004:batchEmbedContents", gotPath)
	}
	requests, ok := gotBody["requests"].([]any)
	if !ok || len(requests) != 2 {
		t.Fatalf("request body requests = %v", gotBody["requests"])
	}
	first, _ := requests[0].(map[string]any)
	if first["model"] != "models/text-embedding-004" {
		t.Errorf("requests[0].model = %v, want models/text-embedding-004", first["model"])
	}
	if len(vecs) != 2 || vecs[0][0] != 0.1 || vecs[1][0] != 0.2 {
		t.Errorf("vecs = %v", vecs)
	}
}

func TestGoogleEmbeddingClient_ChunksBatchesOverTheLimit(t *testing.T) {
	var callCount, maxChunk int
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		callCount++
		var body struct {
			Requests []struct{} `json:"requests"`
		}
		_ = json.NewDecoder(r.Body).Decode(&body)
		if len(body.Requests) > maxChunk {
			maxChunk = len(body.Requests)
		}
		embs := make([]map[string]any, len(body.Requests))
		for i := range embs {
			embs[i] = map[string]any{"values": []float32{float32(i)}}
		}
		b, _ := json.Marshal(map[string]any{"embeddings": embs})
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(b)
	}))
	defer srv.Close()

	c, err := newGoogleEmbeddingClient(googleEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "k",
		model:   "text-embedding-004",
	})
	if err != nil {
		t.Fatalf("newGoogleEmbeddingClient: %v", err)
	}

	texts := make([]string, googleEmbedBatchLimit+3)
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
		t.Errorf("callCount = %d, want 2 chunks", callCount)
	}
	if maxChunk != googleEmbedBatchLimit {
		t.Errorf("max chunk = %d, want %d", maxChunk, googleEmbedBatchLimit)
	}
}

func TestGoogleEmbeddingClient_ModelNameAndDimensions(t *testing.T) {
	c, err := newGoogleEmbeddingClient(googleEmbeddingConfig{
		baseURL: googleDefaultBaseURL,
		apiKey:  "k",
		model:   "gemini-embedding-001",
	})
	if err != nil {
		t.Fatalf("newGoogleEmbeddingClient: %v", err)
	}
	if c.ModelName() != "gemini-embedding-001" {
		t.Errorf("ModelName = %q", c.ModelName())
	}
	if c.Dimensions() != 3072 {
		t.Errorf("Dimensions = %d, want 3072", c.Dimensions())
	}
}
