package ai

import (
	"context"
	"fmt"
	"math"
)

// embeddingSimilarityReranker simulates second-stage reranking for providers that expose
// embeddings but no native rerank endpoint. QueryEmbedder is used when the provider distinguishes
// query and document vectors; otherwise the ordinary single-text embedding path is used.
type embeddingSimilarityReranker struct {
	provider string
	client   EmbeddingClient
}

func newEmbeddingSimilarityReranker(provider string, client EmbeddingClient) (Scorer, error) {
	if client == nil {
		return nil, fmt.Errorf("%s embedding-simulated rerank: no embedding client", provider)
	}
	if client.Dimensions() <= 0 {
		return nil, fmt.Errorf("%s embedding-simulated rerank: invalid vector width %d", provider, client.Dimensions())
	}
	return &embeddingSimilarityReranker{provider: provider, client: client}, nil
}

func (r *embeddingSimilarityReranker) Name() string {
	return r.provider + "/" + r.client.ModelName() + "@embedding-simulated"
}

func (r *embeddingSimilarityReranker) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	if len(candidates) == 0 {
		return nil, nil
	}

	var (
		queryVector []float32
		err         error
	)
	if queryEmbedder, ok := r.client.(QueryEmbedder); ok {
		queryVector, err = queryEmbedder.EmbedQuery(ctx, query)
	} else {
		queryVector, err = r.client.Embed(ctx, query)
	}
	if err != nil {
		return nil, fmt.Errorf("%s embedding-simulated rerank query: %w", r.provider, err)
	}

	candidateVectors, err := r.client.EmbedBatch(ctx, candidates)
	if err != nil {
		return nil, fmt.Errorf("%s embedding-simulated rerank documents: %w", r.provider, err)
	}
	if len(candidateVectors) != len(candidates) {
		return nil, fmt.Errorf("%s embedding-simulated rerank: got %d vectors for %d documents", r.provider, len(candidateVectors), len(candidates))
	}

	dimensions := r.client.Dimensions()
	if len(queryVector) != dimensions {
		return nil, fmt.Errorf("%s embedding-simulated rerank: query vector has width %d, want %d", r.provider, len(queryVector), dimensions)
	}
	scores := make([]float64, len(candidateVectors))
	for i, vector := range candidateVectors {
		if len(vector) != dimensions {
			return nil, fmt.Errorf("%s embedding-simulated rerank: document %d vector has width %d, want %d", r.provider, i, len(vector), dimensions)
		}
		score, err := cosineSimilarity(queryVector, vector)
		if err != nil {
			return nil, fmt.Errorf("%s embedding-simulated rerank document %d: %w", r.provider, i, err)
		}
		scores[i] = score
	}
	return scores, nil
}

func cosineSimilarity(a, b []float32) (float64, error) {
	if len(a) == 0 || len(a) != len(b) {
		return 0, fmt.Errorf("cannot compare vector widths %d and %d", len(a), len(b))
	}
	var dot, normA, normB float64
	for i := range a {
		av, bv := float64(a[i]), float64(b[i])
		if math.IsNaN(av) || math.IsInf(av, 0) || math.IsNaN(bv) || math.IsInf(bv, 0) {
			return 0, fmt.Errorf("vector contains a non-finite value")
		}
		dot += av * bv
		normA += av * av
		normB += bv * bv
	}
	if normA == 0 || normB == 0 {
		return 0, fmt.Errorf("cannot compare a zero-norm vector")
	}
	return dot / (math.Sqrt(normA) * math.Sqrt(normB)), nil
}
