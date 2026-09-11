package ai

import "testing"

func TestRerankModelOrDefault_UsesConfiguredWhenSet(t *testing.T) {
	if got := rerankModelOrDefault("cohere", "rerank-multilingual-v3.0"); got != "rerank-multilingual-v3.0" {
		t.Errorf("rerankModelOrDefault = %q, want the configured value", got)
	}
}

func TestRerankModelOrDefault_FallsBackToTable(t *testing.T) {
	cases := map[string]string{
		"cohere": "rerank-english-v3.0",
		"voyage": "rerank-2",
		"jina":   "jina-reranker-v2-base-multilingual",
	}
	for provider, want := range cases {
		if got := rerankModelOrDefault(provider, ""); got != want {
			t.Errorf("rerankModelOrDefault(%q, \"\") = %q, want %q", provider, got, want)
		}
	}
}

func TestRerankModelOrDefault_UnknownProviderIsEmpty(t *testing.T) {
	if got := rerankModelOrDefault("unknown-provider", ""); got != "" {
		t.Errorf("rerankModelOrDefault(unknown, \"\") = %q, want empty", got)
	}
}
