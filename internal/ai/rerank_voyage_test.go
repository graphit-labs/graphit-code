package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestNewVoyageReranker_MissingKeyIsAnError(t *testing.T) {
	_, err := newVoyageReranker(voyageRerankConfig{baseURL: voyageRerankDefaultBaseURL, model: "rerank-2"})
	if err == nil {
		t.Fatal("expected an error for a missing API key")
	}
}

// Voyage's response is NOT necessarily in input order; Score must map data[i].index back onto the
// candidate's original position.
func TestVoyageReranker_Score_MapsResultsBackToOriginalOrder(t *testing.T) {
	var gotPath, gotAuth string
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAuth = r.Header.Get("Authorization")
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"data":[{"index":1,"relevance_score":0.7},{"index":0,"relevance_score":0.2}]}`))
	}))
	defer srv.Close()

	s, err := newVoyageReranker(voyageRerankConfig{baseURL: srv.URL, apiKey: "test-key", model: "rerank-2"})
	if err != nil {
		t.Fatalf("newVoyageReranker: %v", err)
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
	if gotBody["top_k"].(float64) != 2 {
		t.Errorf("top_k = %v, want 2", gotBody["top_k"])
	}
	if scores[0] != 0.2 || scores[1] != 0.7 {
		t.Errorf("scores = %v, want [0.2 0.7] (mapped back to original candidate order)", scores)
	}
}

func TestVoyageReranker_Score_EmptyCandidatesReturnsNil(t *testing.T) {
	s, err := newVoyageReranker(voyageRerankConfig{baseURL: voyageRerankDefaultBaseURL, apiKey: "k", model: "m"})
	if err != nil {
		t.Fatalf("newVoyageReranker: %v", err)
	}
	scores, err := s.Score(context.Background(), "q", nil)
	if err != nil {
		t.Fatalf("Score: %v", err)
	}
	if scores != nil {
		t.Errorf("scores = %v, want nil for no candidates", scores)
	}
}

func TestVoyageReranker_Score_OutOfRangeIndexIsAnError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"data":[{"index":9,"relevance_score":1}]}`))
	}))
	defer srv.Close()

	s, err := newVoyageReranker(voyageRerankConfig{baseURL: srv.URL, apiKey: "k", model: "m"})
	if err != nil {
		t.Fatalf("newVoyageReranker: %v", err)
	}
	if _, err := s.Score(context.Background(), "q", []string{"a"}); err == nil {
		t.Error("expected an error for an out-of-range result index")
	}
}

func TestVoyageReranker_Name(t *testing.T) {
	s, err := newVoyageReranker(voyageRerankConfig{baseURL: voyageRerankDefaultBaseURL, apiKey: "k", model: "rerank-2"})
	if err != nil {
		t.Fatalf("newVoyageReranker: %v", err)
	}
	if s.Name() != "voyage/rerank-2" {
		t.Errorf("Name = %q", s.Name())
	}
}
