package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestNewCohereEmbeddingClient_MissingKeyIsAnError(t *testing.T) {
	_, err := newCohereEmbeddingClient(cohereEmbeddingConfig{
		baseURL: cohereDefaultBaseURL,
		model:   "embed-english-v3.0",
	})
	if err == nil {
		t.Fatal("expected an error for a missing API key")
	}
}

// Cohere has no self-hosted variant, so an unresolvable width must still refuse rather than
// guess — same fail-fast contract as every other remote client.
func TestNewCohereEmbeddingClient_UnknownModelDimensionsIsAnError(t *testing.T) {
	_, err := newCohereEmbeddingClient(cohereEmbeddingConfig{
		baseURL: cohereDefaultBaseURL,
		apiKey:  "k",
		model:   "some-future-cohere-model",
	})
	if err == nil {
		t.Fatal("expected an error when the vector width cannot be resolved")
	}
}

func TestCohereEmbeddingClient_EmbedBatch_UsesSearchDocumentInputType(t *testing.T) {
	var gotBody map[string]any
	var gotPath, gotAuth string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAuth = r.Header.Get("Authorization")
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"embeddings":{"float":[[0.1,0.2],[0.3,0.4]]}}`))
	}))
	defer srv.Close()

	c, err := newCohereEmbeddingClient(cohereEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "test-key",
		model:   "embed-english-v3.0",
	})
	if err != nil {
		t.Fatalf("newCohereEmbeddingClient: %v", err)
	}

	vecs, err := c.EmbedBatch(context.Background(), []string{"a", "b"})
	if err != nil {
		t.Fatalf("EmbedBatch: %v", err)
	}
	if gotPath != "/embed" {
		t.Errorf("path = %q, want /embed", gotPath)
	}
	if gotAuth != "Bearer test-key" {
		t.Errorf("Authorization = %q", gotAuth)
	}
	if gotBody["input_type"] != "search_document" {
		t.Errorf("input_type = %v, want search_document for EmbedBatch", gotBody["input_type"])
	}
	if len(vecs) != 2 || vecs[0][0] != 0.1 || vecs[1][0] != 0.3 {
		t.Errorf("vecs = %v", vecs)
	}
}

// This is the asymmetry the local client handles with a query prefix string; Cohere handles it
// with this field instead.
func TestCohereEmbeddingClient_EmbedQuery_UsesSearchQueryInputType(t *testing.T) {
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		_, _ = w.Write([]byte(`{"embeddings":{"float":[[0.5]]}}`))
	}))
	defer srv.Close()

	c, err := newCohereEmbeddingClient(cohereEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "test-key",
		model:   "embed-english-v3.0",
	})
	if err != nil {
		t.Fatalf("newCohereEmbeddingClient: %v", err)
	}

	qe, ok := c.(QueryEmbedder)
	if !ok {
		t.Fatalf("%T does not implement QueryEmbedder", c)
	}
	vec, err := qe.EmbedQuery(context.Background(), "find this")
	if err != nil {
		t.Fatalf("EmbedQuery: %v", err)
	}
	if gotBody["input_type"] != "search_query" {
		t.Errorf("input_type = %v, want search_query for EmbedQuery", gotBody["input_type"])
	}
	if vec[0] != 0.5 {
		t.Errorf("vec = %v", vec)
	}
}

func TestCohereEmbeddingClient_ChunksBatchesOverTheLimit(t *testing.T) {
	var callCount int
	var maxChunk int
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		callCount++
		var body struct {
			Texts []string `json:"texts"`
		}
		_ = json.NewDecoder(r.Body).Decode(&body)
		if len(body.Texts) > maxChunk {
			maxChunk = len(body.Texts)
		}
		floats := make([][]float32, len(body.Texts))
		for i := range floats {
			floats[i] = []float32{float32(i)}
		}
		b, _ := json.Marshal(map[string]any{"embeddings": map[string]any{"float": floats}})
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(b)
	}))
	defer srv.Close()

	c, err := newCohereEmbeddingClient(cohereEmbeddingConfig{
		baseURL: srv.URL,
		apiKey:  "k",
		model:   "embed-english-v3.0",
	})
	if err != nil {
		t.Fatalf("newCohereEmbeddingClient: %v", err)
	}

	texts := make([]string, cohereEmbedBatchLimit+5)
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
	if maxChunk != cohereEmbedBatchLimit {
		t.Errorf("max chunk = %d, want %d", maxChunk, cohereEmbedBatchLimit)
	}
}

func TestCohereEmbeddingClient_ModelNameAndDimensions(t *testing.T) {
	c, err := newCohereEmbeddingClient(cohereEmbeddingConfig{
		baseURL: cohereDefaultBaseURL,
		apiKey:  "k",
		model:   "embed-english-v3.0",
	})
	if err != nil {
		t.Fatalf("newCohereEmbeddingClient: %v", err)
	}
	if c.ModelName() != "embed-english-v3.0" {
		t.Errorf("ModelName = %q", c.ModelName())
	}
	if c.Dimensions() != 1024 {
		t.Errorf("Dimensions = %d, want 1024", c.Dimensions())
	}
}
