package ai

import (
	"context"
	"strings"
	"testing"
)

func TestNewRerankerFromConfig_UnknownProviderIsAnError(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "not-a-real-provider")
	_, err := NewRerankerFromConfig(context.Background())
	if err == nil {
		t.Fatal("expected an error for an unknown ai.rerank.provider")
	}
	if !strings.Contains(err.Error(), "not-a-real-provider") {
		t.Errorf("error = %v, want it to name the unknown provider", err)
	}
}

func TestNewRerankerFromConfig_Cohere_MissingKeyIsAnError(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "cohere")
	t.Setenv("GRAPHIT_AI_RERANK_API_KEY", "")
	t.Setenv("COHERE_API_KEY", "")
	_, err := NewRerankerFromConfig(context.Background())
	if err == nil {
		t.Fatal("expected an error when ai.rerank.provider is cohere with no API key available")
	}
}

func TestNewRerankerFromConfig_Cohere_WiresBaseURLKeyAndModel(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "cohere")
	t.Setenv("GRAPHIT_AI_RERANK_API_KEY", "test-key")
	t.Setenv("GRAPHIT_AI_RERANK_BASE_URL", "https://example.test/cohere")
	t.Setenv("GRAPHIT_AI_RERANK_MODEL", "rerank-multilingual-v3.0")

	adapter, err := NewRerankerFromConfig(context.Background())
	if err != nil {
		t.Fatalf("NewRerankerFromConfig: %v", err)
	}
	if adapter.Name() != "cohere/rerank-multilingual-v3.0" {
		t.Errorf("Name = %q, want cohere/rerank-multilingual-v3.0", adapter.Name())
	}
	cr, ok := adapter.Scorer.(*cohereReranker)
	if !ok {
		t.Fatalf("Scorer = %T, want *cohereReranker", adapter.Scorer)
	}
	if cr.baseURL != "https://example.test/cohere" {
		t.Errorf("baseURL = %q", cr.baseURL)
	}
	if cr.apiKey != "test-key" {
		t.Errorf("apiKey = %q", cr.apiKey)
	}
}

func TestNewRerankerFromConfig_Cohere_DefaultsModelWhenUnset(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "cohere")
	t.Setenv("GRAPHIT_AI_RERANK_API_KEY", "test-key")
	t.Setenv("GRAPHIT_AI_RERANK_MODEL", "")

	adapter, err := NewRerankerFromConfig(context.Background())
	if err != nil {
		t.Fatalf("NewRerankerFromConfig: %v", err)
	}
	if adapter.Name() != "cohere/rerank-english-v3.0" {
		t.Errorf("Name = %q, want the default cohere rerank model", adapter.Name())
	}
}

func TestNewRerankerFromConfig_Voyage_MissingKeyIsAnError(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "voyage")
	t.Setenv("GRAPHIT_AI_RERANK_API_KEY", "")
	t.Setenv("VOYAGE_API_KEY", "")
	_, err := NewRerankerFromConfig(context.Background())
	if err == nil {
		t.Fatal("expected an error when ai.rerank.provider is voyage with no API key available")
	}
}

func TestNewRerankerFromConfig_Voyage_WiresBaseURLKeyAndModel(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "voyage")
	t.Setenv("GRAPHIT_AI_RERANK_API_KEY", "test-key")
	t.Setenv("GRAPHIT_AI_RERANK_BASE_URL", "https://example.test/voyage")
	t.Setenv("GRAPHIT_AI_RERANK_MODEL", "")

	adapter, err := NewRerankerFromConfig(context.Background())
	if err != nil {
		t.Fatalf("NewRerankerFromConfig: %v", err)
	}
	if adapter.Name() != "voyage/rerank-2" {
		t.Errorf("Name = %q, want voyage/rerank-2 (default model)", adapter.Name())
	}
	vr, ok := adapter.Scorer.(*voyageReranker)
	if !ok {
		t.Fatalf("Scorer = %T, want *voyageReranker", adapter.Scorer)
	}
	if vr.baseURL != "https://example.test/voyage" {
		t.Errorf("baseURL = %q", vr.baseURL)
	}
}

func TestNewRerankerFromConfig_Jina_MissingKeyIsAnError(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "jina")
	t.Setenv("GRAPHIT_AI_RERANK_API_KEY", "")
	t.Setenv("JINA_API_KEY", "")
	_, err := NewRerankerFromConfig(context.Background())
	if err == nil {
		t.Fatal("expected an error when ai.rerank.provider is jina with no API key available")
	}
}

func TestNewRerankerFromConfig_Jina_WiresBaseURLKeyAndModel(t *testing.T) {
	t.Setenv("GRAPHIT_AI_RERANK_PROVIDER", "jina")
	t.Setenv("GRAPHIT_AI_RERANK_API_KEY", "test-key")
	t.Setenv("GRAPHIT_AI_RERANK_BASE_URL", "https://example.test/jina")
	t.Setenv("GRAPHIT_AI_RERANK_MODEL", "")

	adapter, err := NewRerankerFromConfig(context.Background())
	if err != nil {
		t.Fatalf("NewRerankerFromConfig: %v", err)
	}
	if adapter.Name() != "jina/jina-reranker-v2-base-multilingual" {
		t.Errorf("Name = %q, want the default jina rerank model", adapter.Name())
	}
	jr, ok := adapter.Scorer.(*jinaReranker)
	if !ok {
		t.Fatalf("Scorer = %T, want *jinaReranker", adapter.Scorer)
	}
	if jr.baseURL != "https://example.test/jina" {
		t.Errorf("baseURL = %q", jr.baseURL)
	}
}
