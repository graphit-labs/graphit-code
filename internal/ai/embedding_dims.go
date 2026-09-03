package ai

import (
	"strconv"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
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
}

var defaultEmbeddingModel = map[string]string{
	"openai": "text-embedding-3-small",
	"cohere": "embed-english-v3.0",
	"voyage": "voyage-3",
	"google": "text-embedding-004",
}

func embeddingModelOrDefault(provider, configured string) string {
	configured = strings.TrimSpace(configured)
	if configured != "" {
		return configured
	}
	return defaultEmbeddingModel[provider]
}

// ResolveEmbeddingDimensions returns the vector width for a given provider and model. An
// explicit ai.embedding.dimensions override always wins — it is how a custom
// openai-compatible model, or an OpenAI model truncated via its own request-time
// `dimensions` parameter, tells the vector store schema what to expect. Absent that, a known
// model resolves from the table above. Zero means "unknown" — callers must refuse to build a
// client rather than guess a width, since a wrong one corrupts the vector column silently.
func ResolveEmbeddingDimensions(provider, model string) int {
	if override := strings.TrimSpace(config.ResolveConfig("ai.embedding.dimensions", nil, nil)); override != "" {
		if n, err := strconv.Atoi(override); err == nil && n > 0 {
			return n
		}
	}
	key := strings.ToLower(provider) + "/" + strings.ToLower(model)
	return knownEmbeddingDims[key]
}

// resolveActiveEmbeddingDimensions answers "how wide are the vectors ai.embedding.provider
// currently configures" without constructing a client — used by the proxy and lazy clients,
// which must answer Dimensions() without paying for a model load or a network call.
func resolveActiveEmbeddingDimensions() int {
	provider := normalizeProvider(config.ResolveConfig("ai.embedding.provider", nil, nil))
	if provider == "" || provider == "local" {
		return EmbeddingDimensions
	}
	model := embeddingModelOrDefault(provider, config.ResolveConfig("ai.embedding.model", nil, nil))
	return ResolveEmbeddingDimensions(provider, model)
}

// ResolveConfiguredEmbeddingDimensions is the exported form of resolveActiveEmbeddingDimensions,
// for a caller outside this package that needs to size a vector store schema — e.g. the AST
// search index and the wiki store — before it has (or wants to eagerly construct) a client.
// Returns EmbeddingDimensions (768) unless ai.embedding.provider names a remote provider AND
// that provider/model resolves to a known or overridden width; falls back to 768 (with a
// caller-visible mismatch on first write, not silent corruption) only when a remote provider's
// width genuinely cannot be determined, which newDirectEmbeddingClientFromConfig itself refuses
// to start with — so in practice any provider actually in use here has already been validated.
func ResolveConfiguredEmbeddingDimensions() int {
	if dim := resolveActiveEmbeddingDimensions(); dim > 0 {
		return dim
	}
	return EmbeddingDimensions
}
