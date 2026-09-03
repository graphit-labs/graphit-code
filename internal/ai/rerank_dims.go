package ai

import "strings"

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
