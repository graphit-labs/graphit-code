package ai

import (
	"context"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/auth"
)

// NewRerankerFromConfig resolves the active account provider's rerank capability (local by
// default) into a ready RerankAdapter.
//
// "local" resolves the selected model manifest and its explicit fetch policy. Any other provider
// builds an HTTP-based Scorer with no local model download, and fails fast on a missing API key
// rather than deferring the failure to the first query.
func NewRerankerFromConfig(ctx context.Context) (*RerankAdapter, error) {
	snapshot, credential, err := auth.ResolveServiceCredential(ctx, "rerank")
	if err != nil {
		return nil, err
	}
	service := snapshot.Provider.AI.Rerank
	switch service.Mode {
	case "", auth.ServiceLocal:
		ce, err := NewCrossEncoderReranker(ctx)
		if err != nil {
			return nil, err
		}
		return &RerankAdapter{Scorer: ce}, nil
	case auth.ServiceDisabled:
		return nil, fmt.Errorf("rerank service is disabled by active provider %q", snapshot.Provider.Name)
	case auth.ServiceBroker:
		discovery, err := auth.DiscoverBroker(ctx, snapshot.Provider, nil)
		if err != nil {
			return nil, err
		}
		if discovery.Services.Rerank == nil {
			return nil, fmt.Errorf("broker does not advertise rerank")
		}
		capability := discovery.Services.Rerank
		if capability.Protocol != "graphit-rerank-v1" || capability.Revision == "" {
			return nil, fmt.Errorf("broker advertised an invalid rerank capability")
		}
		endpoint := strings.TrimRight(snapshot.Provider.Broker.Endpoint, "/") + capability.Path
		providerName, revision := snapshot.Provider.Name, snapshot.Provider.Revision
		scorer := &brokerReranker{endpoint: endpoint, name: "broker/" + capability.Revision, expectedRevision: capability.Revision, resolveToken: func(ctx context.Context) (string, error) {
			current, key, err := auth.ResolveServiceCredential(ctx, "rerank")
			if err != nil {
				return "", err
			}
			if current.Provider.Name != providerName || current.Provider.Revision != revision {
				return "", fmt.Errorf("active rerank provider changed; rebuild the client")
			}
			if key == "" && (current.Provider.Broker == nil || !current.Provider.Broker.AllowAnonymous) {
				return "", fmt.Errorf("active profile has no broker credential")
			}
			return key, nil
		}}
		return &RerankAdapter{Scorer: scorer}, nil
	case auth.ServiceDirect:
		provider := normalizeProvider(service.Protocol)
		switch provider {
		case "openai", "openai-compatible", "openai-embeddings-v1":
			embedder, err := newOpenAIEmbeddingClient(openAIEmbeddingConfig{
				provider: provider, baseURL: service.Endpoint, apiKey: credential, requireKey: true,
				model: service.Model, dimensions: service.Dimensions,
			})
			if err != nil {
				return nil, fmt.Errorf("build %s embedding-simulated reranker: %w", provider, err)
			}
			s, err := newEmbeddingSimilarityReranker(provider, embedder)
			if err != nil {
				return nil, err
			}
			return &RerankAdapter{Scorer: s}, nil
		case "google":
			embedder, err := newGoogleEmbeddingClient(googleEmbeddingConfig{
				baseURL: service.Endpoint, apiKey: credential, model: service.Model, dimensions: service.Dimensions,
			})
			if err != nil {
				return nil, fmt.Errorf("build google embedding-simulated reranker: %w", err)
			}
			s, err := newEmbeddingSimilarityReranker(provider, embedder)
			if err != nil {
				return nil, err
			}
			return &RerankAdapter{Scorer: s}, nil
		case "cohere", "cohere-v2":
			s, err := newCohereReranker(cohereRerankConfig{
				baseURL: service.Endpoint, apiKey: credential, model: service.Model,
			})
			if err != nil {
				return nil, err
			}
			return &RerankAdapter{Scorer: s}, nil
		case "voyage", "voyage-v1":
			s, err := newVoyageReranker(voyageRerankConfig{
				baseURL: service.Endpoint, apiKey: credential, model: service.Model,
			})
			if err != nil {
				return nil, err
			}
			return &RerankAdapter{Scorer: s}, nil
		case "jina", "jina-v1":
			s, err := newJinaReranker(jinaRerankConfig{
				baseURL: service.Endpoint, apiKey: credential, model: service.Model,
			})
			if err != nil {
				return nil, err
			}
			return &RerankAdapter{Scorer: s}, nil
		default:
			return nil, fmt.Errorf("direct rerank protocol %q is unsupported", service.Protocol)
		}
	default:
		return nil, fmt.Errorf("rerank mode %q is unsupported", service.Mode)
	}
}
