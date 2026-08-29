package ai

import (
	"context"
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/config"
)

// NewRerankerFromConfig resolves ai.rerank.provider (default "local") into a ready RerankAdapter.
//
// "local" preserves NewCrossEncoderReranker's existing behavior EXACTLY, including its
// download-if-absent semantics — that path is untouched. Any other provider builds an HTTP-based
// Scorer with no download at all, and fails fast on a missing API key rather than deferring the
// failure to the first query.
func NewRerankerFromConfig(ctx context.Context) (*RerankAdapter, error) {
	provider := normalizeProvider(config.ResolveConfig("ai.rerank.provider", nil, nil))
	switch provider {
	case "", "local":
		ce, err := NewCrossEncoderReranker(ctx)
		if err != nil {
			return nil, err
		}
		return &RerankAdapter{Scorer: ce}, nil
	case "cohere":
		s, err := newCohereReranker(cohereRerankConfig{
			baseURL: firstNonEmpty(config.ResolveConfig("ai.rerank.base_url", nil, nil), cohereRerankDefaultBaseURL),
			apiKey:  resolveAPIKey("ai.rerank.api_key", "COHERE_API_KEY"),
			model:   rerankModelOrDefault(provider, config.ResolveConfig("ai.rerank.model", nil, nil)),
		})
		if err != nil {
			return nil, err
		}
		return &RerankAdapter{Scorer: s}, nil
	case "voyage":
		s, err := newVoyageReranker(voyageRerankConfig{
			baseURL: firstNonEmpty(config.ResolveConfig("ai.rerank.base_url", nil, nil), voyageRerankDefaultBaseURL),
			apiKey:  resolveAPIKey("ai.rerank.api_key", "VOYAGE_API_KEY"),
			model:   rerankModelOrDefault(provider, config.ResolveConfig("ai.rerank.model", nil, nil)),
		})
		if err != nil {
			return nil, err
		}
		return &RerankAdapter{Scorer: s}, nil
	case "jina":
		s, err := newJinaReranker(jinaRerankConfig{
			baseURL: firstNonEmpty(config.ResolveConfig("ai.rerank.base_url", nil, nil), jinaRerankDefaultBaseURL),
			apiKey:  resolveAPIKey("ai.rerank.api_key", "JINA_API_KEY"),
			model:   rerankModelOrDefault(provider, config.ResolveConfig("ai.rerank.model", nil, nil)),
		})
		if err != nil {
			return nil, err
		}
		return &RerankAdapter{Scorer: s}, nil
	default:
		return nil, fmt.Errorf("ai.rerank.provider: unknown provider %q (want local, cohere, voyage, or jina)", provider)
	}
}
