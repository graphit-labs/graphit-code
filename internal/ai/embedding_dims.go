package ai

import (
	"context"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/auth"
)

var knownEmbeddingDims = map[string]int{
	"openai/text-embedding-3-small": 1536,
	"openai/text-embedding-3-large": 3072,
	"openai/text-embedding-ada-002": 1536,

	"cohere/embed-english-v3.0":            1024,
	"cohere/embed-multilingual-v3.0":       1024,
	"cohere/embed-english-light-v3.0":      384,
	"cohere/embed-multilingual-light-v3.0": 384,

	"voyage/voyage-3":       1024,
	"voyage/voyage-3-lite":  512,
	"voyage/voyage-code-3":  1024,
	"voyage/voyage-large-2": 1536,

	"google/text-embedding-004":   768,
	"google/gemini-embedding-001": 3072,
	"google/gemini-embedding-2":   3072,
}

// ResolveEmbeddingDimensions returns the known native vector width for a provider/model pair.
// A custom width belongs to the named provider configuration and is passed directly to the
// client constructor. Zero means "unknown" — callers must refuse to guess because a wrong width
// makes an existing vector index incompatible.
func ResolveEmbeddingDimensions(provider, model string) int {
	key := strings.ToLower(provider) + "/" + strings.ToLower(model)
	return knownEmbeddingDims[key]
}

// resolveActiveEmbeddingDimensions answers how wide the active provider's vectors are without
// constructing a client — used by the proxy and lazy clients,
// which must answer Dimensions() without paying for a model load or a network call.
func resolveActiveEmbeddingDimensions() int {
	if snapshot, ok := activeAuthSnapshot(); ok {
		service := snapshot.Provider.AI.Embedding
		switch service.Mode {
		case auth.ServiceDirect:
			return service.Dimensions
		case auth.ServiceBroker:
			discovery, err := auth.DiscoverBroker(context.Background(), snapshot.Provider, nil)
			if err == nil && discovery.Services.Embeddings != nil {
				return discovery.Services.Embeddings.Dimensions
			}
			return 0
		case auth.ServiceDisabled:
			return 0
		default:
			model, err := LoadConfiguredModel(ModelTaskEmbedding)
			if err != nil {
				return 0
			}
			return model.Manifest.Inference.Dimensions
		}
	}
	return 0
}

// ResolveConfiguredEmbeddingDimensions is the exported form of resolveActiveEmbeddingDimensions,
// for a caller outside this package that needs to size a vector store schema — e.g. the AST
// search index and the wiki store — before it has (or wants to eagerly construct) a client.
// Local manifests and direct providers carry an explicit width; broker providers publish it
// through discovery. Zero means configuration or discovery is unavailable and must not be guessed.
func ResolveConfiguredEmbeddingDimensions() int {
	return resolveActiveEmbeddingDimensions()
}

// ConfiguredEmbeddingIdentity returns the non-secret inputs that determine vector compatibility.
func ConfiguredEmbeddingIdentity() (provider, model string, dimensions int) {
	if snapshot, ok := activeAuthSnapshot(); ok {
		service := snapshot.Provider.AI.Embedding
		switch service.Mode {
		case auth.ServiceDirect:
			return "direct:" + service.Protocol + "@" + strings.TrimRight(service.Endpoint, "/"), service.Model, service.Dimensions
		case auth.ServiceBroker:
			discovery, err := auth.DiscoverBroker(context.Background(), snapshot.Provider, nil)
			if err == nil && discovery.Services.Embeddings != nil {
				capability := discovery.Services.Embeddings
				return "broker:" + strings.TrimRight(snapshot.Provider.Broker.Endpoint, "/"), capability.Revision, capability.Dimensions
			}
			return "broker:" + strings.TrimRight(snapshot.Provider.Broker.Endpoint, "/"), "unavailable", 0
		case auth.ServiceDisabled:
			return "disabled:" + snapshot.Provider.Name, "", 0
		default:
			return configuredLocalEmbeddingIdentity()
		}
	}
	return "unconfigured", "", 0
}

func configuredLocalEmbeddingIdentity() (provider, model string, dimensions int) {
	resolved, err := LoadConfiguredModel(ModelTaskEmbedding)
	if err != nil {
		return "local", configuredModelID(ModelTaskEmbedding) + "@invalid", 0
	}
	identity := resolved.Manifest.compatibilityIdentity()
	if len(identity) > 16 {
		identity = identity[:16]
	}
	return "local", resolved.Manifest.ID + "@" + resolved.Manifest.Revision + "#" + identity, resolved.Manifest.Inference.Dimensions
}

func activeAuthSnapshot() (auth.Snapshot, bool) {
	store, err := auth.Open()
	if err != nil {
		return auth.Snapshot{}, false
	}
	snapshot, err := store.ActiveOrDefaultLocal()
	return snapshot, err == nil
}
