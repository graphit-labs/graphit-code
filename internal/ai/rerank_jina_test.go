package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestNewJinaReranker_MissingKeyIsAnError(t *testing.T) {
	_, err := newJinaReranker(jinaRerankConfig{baseURL: jinaRerankDefaultBaseURL, model: "jina-reranker-v2-base-multilingual"})
	if err == nil {
		t.Fatal("expected an error for a missing API key")
	}
}

// Jina's response is NOT necessarily in input order; Score must map results[i].index back onto
// the candidate's original position, same shape and contract as Cohere's.
func TestJinaReranker_Score_MapsResultsBackToOriginalOrder(t *testing.T) {
	var gotPath, gotAuth string
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAuth = r.Header.Get("Authorization")
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"results":[{"index":1,"relevance_score":0.8},{"index":0,"relevance_score":0.3}]}`))
	}))
	defer srv.Close()

	s, err := newJinaReranker(jinaRerankConfig{baseURL: srv.URL, apiKey: "test-key", model: "jina-reranker-v2-base-multilingual"})
	if err != nil {
		t.Fatalf("newJinaReranker: %v", err)
	}

	scores, err := s.Score(context.Background(), "q", []string{"a", "b"})
	if err != nil {
		t.Fatalf("Score: %v", err)
	}
	if gotPath != "/v1/rerank" {
		t.Errorf("path = %q, want /v1/rerank", gotPath)
	}
	if gotAuth != "Bearer test-key" {
		t.Errorf("Authorization = %q", gotAuth)
	}
	if gotBody["top_n"].(float64) != 2 {
		t.Errorf("top_n = %v, want 2", gotBody["top_n"])
	}
	if scores[0] != 0.3 || scores[1] != 0.8 {
		t.Errorf("scores = %v, want [0.3 0.8] (mapped back to original candidate order)", scores)
	}
}

func TestJinaReranker_Score_EmptyCandidatesReturnsNil(t *testing.T) {
	s, err := newJinaReranker(jinaRerankConfig{baseURL: jinaRerankDefaultBaseURL, apiKey: "k", model: "m"})
	if err != nil {
		t.Fatalf("newJinaReranker: %v", err)
	}
	scores, err := s.Score(context.Background(), "q", nil)
	if err != nil {
		t.Fatalf("Score: %v", err)
	}
	if scores != nil {
		t.Errorf("scores = %v, want nil for no candidates", scores)
	}
}

func TestJinaReranker_Score_OutOfRangeIndexIsAnError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"results":[{"index":9,"relevance_score":1}]}`))
	}))
	defer srv.Close()

	s, err := newJinaReranker(jinaRerankConfig{baseURL: srv.URL, apiKey: "k", model: "m"})
	if err != nil {
		t.Fatalf("newJinaReranker: %v", err)
	}
	if _, err := s.Score(context.Background(), "q", []string{"a"}); err == nil {
		t.Error("expected an error for an out-of-range result index")
	}
}

func TestJinaReranker_Name(t *testing.T) {
	s, err := newJinaReranker(jinaRerankConfig{baseURL: jinaRerankDefaultBaseURL, apiKey: "k", model: "jina-reranker-v2-base-multilingual"})
	if err != nil {
		t.Fatalf("newJinaReranker: %v", err)
	}
	if s.Name() != "jina/jina-reranker-v2-base-multilingual" {
		t.Errorf("Name = %q", s.Name())
	}
}
