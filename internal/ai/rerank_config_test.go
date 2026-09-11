package ai

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestNewRerankerFromConfig_UnknownDirectProtocolIsAnError(t *testing.T) {
	provider := auth.Provider{Name: "invalid", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}, AI: auth.AIConfig{Rerank: auth.AIServiceConfig{Mode: auth.ServiceDirect, Protocol: "not-a-real-provider", Endpoint: "https://example.test", Model: "model"}}}
	err := auth.ValidateProvider(provider)
	if err == nil || !strings.Contains(err.Error(), "not-a-real-provider") {
		t.Fatalf("error = %v", err)
	}
}

func TestDirectRerankProfileRequiresKey(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "direct", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}, AI: auth.AIConfig{Rerank: auth.AIServiceConfig{Mode: auth.ServiceDirect, Protocol: "cohere", Endpoint: "https://example.test", Model: "rerank-v4"}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "p", Provider: "direct", Username: "alice"}); err == nil {
		t.Fatal("direct profile without API key accepted")
	}
}

func TestNewRerankerFromProviderProfileWiresProtocolEndpointKeyAndModel(t *testing.T) {
	for _, tc := range []struct {
		protocol, endpoint, model, wantName string
		dimensions                          int
	}{
		{"cohere", "https://example.test/cohere", "rerank-v4", "cohere/rerank-v4", 0},
		{"voyage", "https://example.test/voyage", "rerank-2", "voyage/rerank-2", 0},
		{"jina", "https://example.test/jina", "jina-v2", "jina/jina-v2", 0},
		{"openai", "https://example.test/openai", "text-embedding-3-small", "openai/text-embedding-3-small@embedding-simulated", 1536},
		{"openai-compatible", "https://example.test/compatible", "embed-v1", "openai-compatible/embed-v1@embedding-simulated", 768},
		{"openai-embeddings-v1", "https://example.test/openai-v1", "embed-v1", "openai-embeddings-v1/embed-v1@embedding-simulated", 768},
		{"google", "https://example.test/google", "gemini-embedding-001", "google/gemini-embedding-001@embedding-simulated", 3072},
	} {
		t.Run(tc.protocol, func(t *testing.T) {
			activateRerankProvider(t, tc.protocol, tc.endpoint, tc.model, tc.dimensions, "test-key")
			adapter, err := NewRerankerFromConfig(context.Background())
			if err != nil {
				t.Fatal(err)
			}
			if adapter.Name() != tc.wantName {
				t.Fatalf("Name=%q want %q", adapter.Name(), tc.wantName)
			}
			switch scorer := adapter.Scorer.(type) {
			case *cohereReranker:
				if scorer.baseURL != tc.endpoint || scorer.apiKey != "test-key" {
					t.Fatalf("scorer=%#v", scorer)
				}
			case *voyageReranker:
				if scorer.baseURL != tc.endpoint || scorer.apiKey != "test-key" {
					t.Fatalf("scorer=%#v", scorer)
				}
			case *jinaReranker:
				if scorer.baseURL != tc.endpoint || scorer.apiKey != "test-key" {
					t.Fatalf("scorer=%#v", scorer)
				}
			case *embeddingSimilarityReranker:
				if scorer.provider != tc.protocol || scorer.client.Dimensions() != tc.dimensions {
					t.Fatalf("scorer=%#v client=%#v", scorer, scorer.client)
				}
				switch tc.protocol {
				case "google":
					client, ok := scorer.client.(*googleEmbeddingClient)
					if !ok {
						t.Fatalf("client=%T, want *googleEmbeddingClient", scorer.client)
					}
					if client.baseURL != tc.endpoint || client.apiKey != "test-key" {
						t.Fatalf("client=%#v", client)
					}
				default:
					client, ok := scorer.client.(*openAIEmbeddingClient)
					if !ok {
						t.Fatalf("client=%T, want *openAIEmbeddingClient", scorer.client)
					}
					if client.baseURL != tc.endpoint || client.apiKey != "test-key" {
						t.Fatalf("client=%#v", client)
					}
				}
			default:
				t.Fatalf("Scorer=%T", adapter.Scorer)
			}
		})
	}
}

func activateRerankProvider(t *testing.T, protocol, endpoint, model string, dimensions int, key string) {
	t.Helper()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "direct", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}, AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceDisabled}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceDirect, Protocol: protocol, Endpoint: endpoint, Model: model, Dimensions: dimensions}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "p", Provider: "direct", Username: "alice", RerankAPIKey: key}); err != nil {
		t.Fatal(err)
	}
}
