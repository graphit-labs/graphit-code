package ai

import (
	"context"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/auth"
)

const EmbeddingDimensions = 768

type EmbeddingClient interface {
	Embed(ctx context.Context, text string) ([]float32, error)

	EmbedBatch(ctx context.Context, texts []string) ([][]float32, error)

	ModelName() string

	// Dimensions is the width of the vectors this client produces. Local is fixed at
	// EmbeddingDimensions; a remote provider's width depends on the provider and model
	// (and, for OpenAI, an optional truncation), which is why the vector store schema is
	// built from this instead of from the constant. See ResolveEmbeddingDimensions.
	Dimensions() int
}

type QueryEmbedder interface {
	EmbedQuery(ctx context.Context, query string) ([]float32, error)
}

// NewEmbeddingClientFromConfig resolves the embedding backend a normal caller (CLI, MCP tool)
// should use: the daemon's socket first, if one is listening, then the embedding capability of
// the active account provider.
func NewEmbeddingClientFromConfig() (EmbeddingClient, error) {

	if proxy := newProxyEmbeddingClient(); proxy != nil {
		return proxy, nil
	}

	return newDirectEmbeddingClientFromConfig()
}

// newDirectEmbeddingClientFromConfig resolves the active provider WITHOUT attempting the daemon
// socket first.
//
// This is what the daemon's own EmbedServer must call (via NewLazyEmbeddingClient) to decide
// what it serves: going through NewEmbeddingClientFromConfig there would have the daemon try
// to dial its own socket, which is either a deadlock (nothing is listening yet on first boot)
// or a pointless detour back to itself.
func newDirectEmbeddingClientFromConfig() (EmbeddingClient, error) {
	snapshot, credential, err := auth.ResolveServiceCredential(context.Background(), "embedding")
	if err != nil {
		return nil, err
	}
	service := snapshot.Provider.AI.Embedding
	switch service.Mode {
	case "", auth.ServiceLocal:
		return NewLocalEmbeddingClient()
	case auth.ServiceDisabled:
		return nil, fmt.Errorf("embedding service is disabled by active provider %q", snapshot.Provider.Name)
	case auth.ServiceBroker:
		discovery, err := auth.DiscoverBroker(context.Background(), snapshot.Provider, nil)
		if err != nil {
			return nil, err
		}
		if discovery.Services.Embeddings == nil {
			return nil, fmt.Errorf("broker does not advertise embeddings")
		}
		capability := discovery.Services.Embeddings
		if capability.Protocol != "openai-embeddings-v1" || capability.Dimensions <= 0 || capability.Revision == "" {
			return nil, fmt.Errorf("broker advertised an invalid embedding capability")
		}
		endpoint := strings.TrimRight(snapshot.Provider.Broker.Endpoint, "/") + capability.Path
		providerName, revision := snapshot.Provider.Name, snapshot.Provider.Revision
		return newOpenAIEmbeddingClient(openAIEmbeddingConfig{provider: "broker", endpoint: endpoint, model: capability.Revision, dimensions: capability.Dimensions, expectedRevision: capability.Revision,
			apiKeyResolver: func(ctx context.Context) (string, error) {
				current, key, err := auth.ResolveServiceCredential(ctx, "embedding")
				if err != nil {
					return "", err
				}
				if current.Provider.Name != providerName || current.Provider.Revision != revision {
					return "", fmt.Errorf("active embedding provider changed; rebuild the client")
				}
				if key == "" && (current.Provider.Broker == nil || !current.Provider.Broker.AllowAnonymous) {
					return "", fmt.Errorf("active profile has no broker credential")
				}
				return key, nil
			},
		})
	case auth.ServiceDirect:
		provider := normalizeProvider(service.Protocol)
		switch provider {
		case "openai", "openai-compatible", "openai-embeddings-v1":
			return newOpenAIEmbeddingClient(openAIEmbeddingConfig{
				provider:   provider,
				baseURL:    service.Endpoint,
				apiKey:     credential,
				requireKey: true,
				model:      service.Model,
				dimensions: service.Dimensions,
			})
		case "cohere":
			return newCohereEmbeddingClient(cohereEmbeddingConfig{
				baseURL: service.Endpoint, apiKey: credential, model: service.Model, dimensions: service.Dimensions,
			})
		case "voyage":
			return newVoyageEmbeddingClient(voyageEmbeddingConfig{
				baseURL: service.Endpoint, apiKey: credential, model: service.Model, dimensions: service.Dimensions,
			})
		case "google":
			return newGoogleEmbeddingClient(googleEmbeddingConfig{
				baseURL: service.Endpoint, apiKey: credential, model: service.Model, dimensions: service.Dimensions,
			})
		default:
			return nil, fmt.Errorf("direct embedding protocol %q is unsupported", service.Protocol)
		}
	default:
		return nil, fmt.Errorf("embedding mode %q is unsupported", service.Mode)
	}
}

func normalizeProvider(v string) string {
	return strings.ToLower(strings.TrimSpace(v))
}
