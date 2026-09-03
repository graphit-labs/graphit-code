package ai

import (
	"context"
	"fmt"
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
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
// should use: the daemon's socket first, if one is listening, then whatever
// ai.embedding.provider configures directly.
func NewEmbeddingClientFromConfig() (EmbeddingClient, error) {

	if proxy := newProxyEmbeddingClient(); proxy != nil {
		return proxy, nil
	}

	return newDirectEmbeddingClientFromConfig()
}

// newDirectEmbeddingClientFromConfig resolves ai.embedding.provider WITHOUT attempting the
// daemon socket first.
//
// This is what the daemon's own EmbedServer must call (via NewLazyEmbeddingClient) to decide
// what it serves: going through NewEmbeddingClientFromConfig there would have the daemon try
// to dial its own socket, which is either a deadlock (nothing is listening yet on first boot)
// or a pointless detour back to itself.
func newDirectEmbeddingClientFromConfig() (EmbeddingClient, error) {
	provider := normalizeProvider(config.ResolveConfig("ai.embedding.provider", nil, nil))

	switch provider {
	case "", "local":
		return NewLocalEmbeddingClient()
	case "openai":
		return newOpenAIEmbeddingClient(openAIEmbeddingConfig{
			provider:   "openai",
			baseURL:    firstNonEmpty(config.ResolveConfig("ai.embedding.base_url", nil, nil), openAIDefaultBaseURL),
			apiKey:     resolveAPIKey("ai.embedding.api_key", "OPENAI_API_KEY"),
			requireKey: true,
			model:      embeddingModelOrDefault(provider, config.ResolveConfig("ai.embedding.model", nil, nil)),
		})
	case "openai-compatible":
		baseURL := strings.TrimSpace(config.ResolveConfig("ai.embedding.base_url", nil, nil))
		if baseURL == "" {
			return nil, fmt.Errorf("ai.embedding.provider is %q, which needs ai.embedding.base_url pointing at the server's OpenAI-compatible endpoint", provider)
		}
		return newOpenAIEmbeddingClient(openAIEmbeddingConfig{
			provider:   "openai-compatible",
			baseURL:    baseURL,
			apiKey:     resolveAPIKey("ai.embedding.api_key", ""),
			requireKey: false,
			model:      config.ResolveConfig("ai.embedding.model", nil, nil),
		})
	case "cohere":
		return newCohereEmbeddingClient(cohereEmbeddingConfig{
			baseURL: firstNonEmpty(config.ResolveConfig("ai.embedding.base_url", nil, nil), cohereDefaultBaseURL),
			apiKey:  resolveAPIKey("ai.embedding.api_key", "COHERE_API_KEY"),
			model:   embeddingModelOrDefault(provider, config.ResolveConfig("ai.embedding.model", nil, nil)),
		})
	case "voyage":
		return newVoyageEmbeddingClient(voyageEmbeddingConfig{
			baseURL: firstNonEmpty(config.ResolveConfig("ai.embedding.base_url", nil, nil), voyageDefaultBaseURL),
			apiKey:  resolveAPIKey("ai.embedding.api_key", "VOYAGE_API_KEY"),
			model:   embeddingModelOrDefault(provider, config.ResolveConfig("ai.embedding.model", nil, nil)),
		})
	case "google":
		return newGoogleEmbeddingClient(googleEmbeddingConfig{
			baseURL: firstNonEmpty(config.ResolveConfig("ai.embedding.base_url", nil, nil), googleDefaultBaseURL),
			apiKey:  resolveAPIKey("ai.embedding.api_key", "GOOGLE_API_KEY", "GEMINI_API_KEY"),
			model:   embeddingModelOrDefault(provider, config.ResolveConfig("ai.embedding.model", nil, nil)),
		})
	default:
		return nil, fmt.Errorf("ai.embedding.provider: unknown provider %q (want local, openai, openai-compatible, cohere, voyage, or google)", provider)
	}
}

func normalizeProvider(v string) string {
	return strings.ToLower(strings.TrimSpace(v))
}

func firstNonEmpty(vals ...string) string {
	for _, v := range vals {
		if v != "" {
			return v
		}
	}
	return ""
}

func resolveAPIKey(configKey string, nativeEnvVars ...string) string {
	if v := strings.TrimSpace(config.ResolveConfig(configKey, nil, nil)); v != "" {
		return v
	}
	for _, envVar := range nativeEnvVars {
		if envVar == "" {
			continue
		}
		if v := strings.TrimSpace(os.Getenv(envVar)); v != "" {
			return v
		}
	}
	return ""
}
