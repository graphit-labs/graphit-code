package commands

import (
	"bufio"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/output"
)

func TestPromptEmbeddingProviderDefaultsToLocalOnBlankInput(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	got, err := promptEmbeddingProvider(p, bufio.NewReader(strings.NewReader("\n")))
	if err != nil {
		t.Fatalf("promptEmbeddingProvider: %v", err)
	}
	if got != "local" {
		t.Fatalf("provider = %q, want local", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.embedding.provider"); v != "local" {
		t.Fatalf("ai.embedding.provider stored = %q, want local", v)
	}
}

func TestPromptEmbeddingProviderStoresModelAndAPIKeyForANamedProvider(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	input := "openai\ntext-embedding-3-small\nsk-test-key\n"
	got, err := promptEmbeddingProvider(p, bufio.NewReader(strings.NewReader(input)))
	if err != nil {
		t.Fatalf("promptEmbeddingProvider: %v", err)
	}
	if got != "openai" {
		t.Fatalf("provider = %q, want openai", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.embedding.model"); v != "text-embedding-3-small" {
		t.Fatalf("ai.embedding.model = %q", v)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.embedding.api_key"); v != "sk-test-key" {
		t.Fatalf("ai.embedding.api_key = %q", v)
	}
}

func TestPromptEmbeddingProviderOpenAICompatibleRequiresABaseURL(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	input := "openai-compatible\n\n\n"
	if _, err := promptEmbeddingProvider(p, bufio.NewReader(strings.NewReader(input))); err == nil {
		t.Fatal("expected an error for openai-compatible with no base URL")
	}
}

func TestPromptEmbeddingProviderOpenAICompatibleStoresBaseURLAndSkipsAPIKey(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	input := "openai-compatible\n\nhttp://localhost:11434/v1\n\n"
	got, err := promptEmbeddingProvider(p, bufio.NewReader(strings.NewReader(input)))
	if err != nil {
		t.Fatalf("promptEmbeddingProvider: %v", err)
	}
	if got != "openai-compatible" {
		t.Fatalf("provider = %q, want openai-compatible", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.embedding.base_url"); v != "http://localhost:11434/v1" {
		t.Fatalf("ai.embedding.base_url = %q", v)
	}
	if v, ok, _ := config.GetGlobalConfigValue("ai.embedding.api_key"); ok && v != "" {
		t.Fatalf("ai.embedding.api_key should be unset, got %q", v)
	}
}

func TestPromptRerankProviderDefaultsToLocalOnBlankInput(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	got, err := promptRerankProvider(p, bufio.NewReader(strings.NewReader("\n")))
	if err != nil {
		t.Fatalf("promptRerankProvider: %v", err)
	}
	if got != "local" {
		t.Fatalf("provider = %q, want local", got)
	}
}

func TestPromptRerankProviderStoresModelAndAPIKeyForANamedProvider(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	input := "cohere\nrerank-english-v3.0\nsk-rerank-key\n"
	got, err := promptRerankProvider(p, bufio.NewReader(strings.NewReader(input)))
	if err != nil {
		t.Fatalf("promptRerankProvider: %v", err)
	}
	if got != "cohere" {
		t.Fatalf("provider = %q, want cohere", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.rerank.model"); v != "rerank-english-v3.0" {
		t.Fatalf("ai.rerank.model = %q", v)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.rerank.api_key"); v != "sk-rerank-key" {
		t.Fatalf("ai.rerank.api_key = %q", v)
	}
}
