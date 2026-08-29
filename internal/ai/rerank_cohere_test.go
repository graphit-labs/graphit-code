package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestNewCohereReranker_MissingKeyIsAnError(t *testing.T) {
	_, err := newCohereReranker(cohereRerankConfig{baseURL: cohereRerankDefaultBaseURL, model: "rerank-english-v3.0"})
	if err == nil {
		t.Fatal("expected an error for a missing API key")
	}
}

// Cohere's response is NOT necessarily in input order; Score must map results[i].index back onto
// the candidate's original position so index i of the output is candidates[i]'s relevance.
func TestCohereReranker_Score_MapsResultsBackToOriginalOrder(t *testing.T) {
	var gotPath, gotAuth string
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAuth = r.Header.Get("Authorization")
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		// Deliberately reversed relative to input order.
		_, _ = w.Write([]byte(`{"results":[{"index":2,"relevance_score":0.9},{"index":0,"relevance_score":0.1},{"index":1,"relevance_score":0.5}]}`))
	}))
	defer srv.Close()

	s, err := newCohereReranker(cohereRerankConfig{baseURL: srv.URL, apiKey: "test-key", model: "rerank-english-v3.0"})
	if err != nil {
		t.Fatalf("newCohereReranker: %v", err)
	}

	scores, err := s.Score(context.Background(), "q", []string{"a", "b", "c"})
	if err != nil {
		t.Fatalf("Score: %v", err)
	}
	if gotPath != "/v2/rerank" {
		t.Errorf("path = %q, want /v2/rerank", gotPath)
	}
	if gotAuth != "Bearer test-key" {
		t.Errorf("Authorization = %q", gotAuth)
	}
	if gotBody["top_n"].(float64) != 3 {
		t.Errorf("top_n = %v, want 3", gotBody["top_n"])
	}
	want := []float64{0.1, 0.5, 0.9}
	for i := range want {
		if scores[i] != want[i] {
			t.Errorf("scores[%d] = %v, want %v (scores must land on the ORIGINAL candidate index, not response order)", i, scores[i], want[i])
		}
	}
}

func TestCohereReranker_Score_ChunksOverTheLimit(t *testing.T) {
	var callCount, maxChunk int
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		callCount++
		var body struct {
			Documents []string `json:"documents"`
		}
		_ = json.NewDecoder(r.Body).Decode(&body)
		if len(body.Documents) > maxChunk {
			maxChunk = len(body.Documents)
		}
		results := make([]map[string]any, len(body.Documents))
		for i := range body.Documents {
			results[i] = map[string]any{"index": i, "relevance_score": float64(i)}
		}
		b, _ := json.Marshal(map[string]any{"results": results})
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(b)
	}))
	defer srv.Close()

	s, err := newCohereReranker(cohereRerankConfig{baseURL: srv.URL, apiKey: "k", model: "rerank-english-v3.0"})
	if err != nil {
		t.Fatalf("newCohereReranker: %v", err)
	}

	docs := make([]string, cohereRerankChunkSize+9)
	for i := range docs {
		docs[i] = "doc"
	}
	scores, err := s.Score(context.Background(), "q", docs)
	if err != nil {
		t.Fatalf("Score: %v", err)
	}
	if len(scores) != len(docs) {
		t.Fatalf("got %d scores, want %d", len(scores), len(docs))
	}
	if callCount != 2 {
		t.Errorf("callCount = %d, want 2 chunks", callCount)
	}
	if maxChunk != cohereRerankChunkSize {
		t.Errorf("max chunk = %d, want %d", maxChunk, cohereRerankChunkSize)
	}
}

func TestCohereReranker_Score_EmptyCandidatesReturnsNil(t *testing.T) {
	s, err := newCohereReranker(cohereRerankConfig{baseURL: cohereRerankDefaultBaseURL, apiKey: "k", model: "m"})
	if err != nil {
		t.Fatalf("newCohereReranker: %v", err)
	}
	scores, err := s.Score(context.Background(), "q", nil)
	if err != nil {
		t.Fatalf("Score: %v", err)
	}
	if scores != nil {
		t.Errorf("scores = %v, want nil for no candidates", scores)
	}
}

func TestCohereReranker_Score_OutOfRangeIndexIsAnError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"results":[{"index":5,"relevance_score":1}]}`))
	}))
	defer srv.Close()

	s, err := newCohereReranker(cohereRerankConfig{baseURL: srv.URL, apiKey: "k", model: "m"})
	if err != nil {
		t.Fatalf("newCohereReranker: %v", err)
	}
	if _, err := s.Score(context.Background(), "q", []string{"a", "b"}); err == nil {
		t.Error("expected an error for an out-of-range result index")
	}
}

func TestCohereReranker_Name(t *testing.T) {
	s, err := newCohereReranker(cohereRerankConfig{baseURL: cohereRerankDefaultBaseURL, apiKey: "k", model: "rerank-english-v3.0"})
	if err != nil {
		t.Fatalf("newCohereReranker: %v", err)
	}
	if s.Name() != "cohere/rerank-english-v3.0" {
		t.Errorf("Name = %q", s.Name())
	}
}
