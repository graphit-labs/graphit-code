package ai

import (
	"context"
	"errors"
	"math"
	"strings"
	"testing"
)

type fakeSimilarityEmbeddingClient struct {
	queryVector     []float32
	documentVectors [][]float32
	dimensions      int
	queryCalls      int
	embedCalls      int
	batchCalls      int
	err             error
}

func (f *fakeSimilarityEmbeddingClient) Embed(context.Context, string) ([]float32, error) {
	f.embedCalls++
	return f.queryVector, f.err
}

func (f *fakeSimilarityEmbeddingClient) EmbedQuery(context.Context, string) ([]float32, error) {
	f.queryCalls++
	return f.queryVector, f.err
}

func (f *fakeSimilarityEmbeddingClient) EmbedBatch(context.Context, []string) ([][]float32, error) {
	f.batchCalls++
	return f.documentVectors, f.err
}

func (f *fakeSimilarityEmbeddingClient) ModelName() string { return "fake-embed" }
func (f *fakeSimilarityEmbeddingClient) Dimensions() int   { return f.dimensions }

func TestEmbeddingSimilarityRerankerRanksByCosineAndUsesQuerySemantics(t *testing.T) {
	client := &fakeSimilarityEmbeddingClient{
		queryVector:     []float32{1, 0},
		documentVectors: [][]float32{{0, 1}, {1, 0}, {2, 0}},
		dimensions:      2,
	}
	scorer, err := newEmbeddingSimilarityReranker("google", client)
	if err != nil {
		t.Fatal(err)
	}
	adapter := RerankAdapter{Scorer: scorer}
	hits := []RerankHit{{Text: "unrelated", Index: 0}, {Text: "match-a", Index: 1}, {Text: "match-b", Index: 2}}
	ranked, err := adapter.Rank(context.Background(), "query", hits)
	if err != nil {
		t.Fatal(err)
	}
	if got := []int{ranked[0].Index, ranked[1].Index, ranked[2].Index}; got[0] != 1 || got[1] != 2 || got[2] != 0 {
		t.Fatalf("ranked indexes = %v, want [1 2 0]", got)
	}
	if client.queryCalls != 1 || client.embedCalls != 0 || client.batchCalls != 1 {
		t.Fatalf("calls: query=%d embed=%d batch=%d", client.queryCalls, client.embedCalls, client.batchCalls)
	}
	if scorer.Name() != "google/fake-embed@embedding-simulated" {
		t.Fatalf("Name = %q", scorer.Name())
	}
}

func TestEmbeddingSimilarityRerankerEmptyCandidatesDoesNoWork(t *testing.T) {
	client := &fakeSimilarityEmbeddingClient{dimensions: 2, err: errors.New("must not be called")}
	scorer, err := newEmbeddingSimilarityReranker("openai", client)
	if err != nil {
		t.Fatal(err)
	}
	scores, err := scorer.Score(context.Background(), "query", nil)
	if err != nil || scores != nil || client.queryCalls != 0 || client.batchCalls != 0 {
		t.Fatalf("scores=%v err=%v queryCalls=%d batchCalls=%d", scores, err, client.queryCalls, client.batchCalls)
	}
}

func TestEmbeddingSimilarityRerankerRejectsInvalidVectors(t *testing.T) {
	tests := []struct {
		name      string
		query     []float32
		documents [][]float32
		want      string
	}{
		{name: "count", query: []float32{1, 0}, documents: nil, want: "0 vectors for 1 documents"},
		{name: "query width", query: []float32{1}, documents: [][]float32{{1, 0}}, want: "query vector has width 1, want 2"},
		{name: "document width", query: []float32{1, 0}, documents: [][]float32{{1}}, want: "document 0 vector has width 1, want 2"},
		{name: "zero norm", query: []float32{1, 0}, documents: [][]float32{{0, 0}}, want: "zero-norm"},
		{name: "not finite", query: []float32{1, 0}, documents: [][]float32{{float32(math.Inf(1)), 0}}, want: "non-finite"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			client := &fakeSimilarityEmbeddingClient{queryVector: tc.query, documentVectors: tc.documents, dimensions: 2}
			scorer, err := newEmbeddingSimilarityReranker("openai", client)
			if err != nil {
				t.Fatal(err)
			}
			_, err = scorer.Score(context.Background(), "query", []string{"document"})
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error = %v, want substring %q", err, tc.want)
			}
		})
	}
}
