package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestNewVoyageEmbeddingClient_MissingKeyIsAnError(t *testing.T) {
	_, err := newVoyageEmbeddingClient(voyageEmbeddingConfig{
		baseURL: voyageDefaultBaseURL,
		model:   "voyage-3",
	})
	if err == nil {
		t.Fatal("expected an error for a missing API key")
	}
}

func TestNewVoyageEmbeddingClient_UnknownModelDimensionsIsAnError(t *testing.T) {
	_, err := newVoyageEmbeddingClient(voyageEmbeddingConfig{
		baseURL: voyageDefaultBaseURL,
		apiKey:  "k",
		model:   "some-future-voyage-model",
	})
	if err == nil {
		t.Fatal("expected an error when the vector width cannot be resolved")
	}
}

func TestVoyageEmbeddingClient_EmbedBatch_UsesDocumentInputType(t *testing.T) {
	var gotBody map[string]any
	var gotPath, gotAuth string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAuth = r.Header.Get("Authorization")
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"data":[{"embedding":[0.1,0.2],"index":0},{"embedding":[0.3,0.4],"index":1}]}`))
	}))
	defer srv.Close()

	c, err := newVoyageEmbeddingClient(voyageEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "test-key",
		model:   "voyage-3",
	})
	if err != nil {
		t.Fatalf("newVoyageEmbeddingClient: %v", err)
	}

	vecs, err := c.EmbedBatch(context.Background(), []string{"a", "b"})
	if err != nil {
		t.Fatalf("EmbedBatch: %v", err)
	}
	if gotPath != "/embeddings" {
		t.Errorf("path = %q, want /embeddings", gotPath)
	}
	if gotAuth != "Bearer test-key" {
		t.Errorf("Authorization = %q", gotAuth)
	}
	if gotBody["input_type"] != "document" {
		t.Errorf("input_type = %v, want document for EmbedBatch", gotBody["input_type"])
	}
	if len(vecs) != 2 || vecs[0][0] != 0.1 || vecs[1][0] != 0.3 {
		t.Errorf("vecs = %v", vecs)
	}
}

func TestVoyageEmbeddingClient_EmbedQuery_UsesQueryInputType(t *testing.T) {
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		_, _ = w.Write([]byte(`{"data":[{"embedding":[0.9],"index":0}]}`))
	}))
	defer srv.Close()

	c, err := newVoyageEmbeddingClient(voyageEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "test-key",
		model:   "voyage-3",
	})
	if err != nil {
		t.Fatalf("newVoyageEmbeddingClient: %v", err)
	}

	qe, ok := c.(QueryEmbedder)
	if !ok {
		t.Fatalf("%T does not implement QueryEmbedder", c)
	}
	vec, err := qe.EmbedQuery(context.Background(), "find this")
	if err != nil {
		t.Fatalf("EmbedQuery: %v", err)
	}
	if gotBody["input_type"] != "query" {
		t.Errorf("input_type = %v, want query for EmbedQuery", gotBody["input_type"])
	}
	if vec[0] != 0.9 {
		t.Errorf("vec = %v", vec)
	}
}

func TestVoyageEmbeddingClient_ChunksBatchesOverTheLimit(t *testing.T) {
	var callCount, maxChunk int
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

	c, err := newVoyageEmbeddingClient(voyageEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "k",
		model:   "voyage-3",
	})
	if err != nil {
		t.Fatalf("newVoyageEmbeddingClient: %v", err)
	}

	texts := make([]string, voyageEmbedBatchLimit+7)
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
	if maxChunk != voyageEmbedBatchLimit {
		t.Errorf("max chunk = %d, want %d", maxChunk, voyageEmbedBatchLimit)
	}
}

func TestVoyageEmbeddingClient_ModelNameAndDimensions(t *testing.T) {
	c, err := newVoyageEmbeddingClient(voyageEmbeddingConfig{
		baseURL: voyageDefaultBaseURL,
		apiKey:  "k",
		model:   "voyage-3-lite",
	})
	if err != nil {
		t.Fatalf("newVoyageEmbeddingClient: %v", err)
	}
	if c.ModelName() != "voyage-3-lite" {
		t.Errorf("ModelName = %q", c.ModelName())
	}
	if c.Dimensions() != 512 {
		t.Errorf("Dimensions = %d, want 512", c.Dimensions())
	}
}
