package ai

import "strings"

// defaultRerankModel is used when ai.rerank.model is unset for a named remote provider. There is
// no "local" entry here — the local cross-encoder's model is fixed and does not go through this
// table; see rerank_local.go.
var defaultRerankModel = map[string]string{
	"cohere": "rerank-english-v3.0",
	"voyage": "rerank-2",
	"jina":   "jina-reranker-v2-base-multilingual",
}

func rerankModelOrDefault(provider, configured string) string {
	configured = strings.TrimSpace(configured)
	if configured != "" {
		return configured
	}
	return defaultRerankModel[provider]
}
