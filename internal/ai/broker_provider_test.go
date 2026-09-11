package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestBrokerProviderDiscoversCapabilitiesAndUsesActiveProfileToken(t *testing.T) {
	var embeddingAuth, rerankAuth string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "services": map[string]any{
				"embeddings": map[string]any{"protocol": "openai-embeddings-v1", "path": "/v1/embeddings", "revision": "embed-r1", "dimensions": 3, "max_batch": 10},
				"rerank":     map[string]any{"protocol": "graphit-rerank-v1", "path": "/v1/rerank", "revision": "rerank-r1", "max_documents": 10},
			}})
		case "/v1/embeddings":
			embeddingAuth = r.Header.Get("Authorization")
			var request struct {
				Input     []string `json:"input"`
				InputType string   `json:"input_type"`
			}
			decoder := json.NewDecoder(r.Body)
			decoder.DisallowUnknownFields()
			if err := decoder.Decode(&request); err != nil {
				t.Errorf("invalid embedding request: %v", err)
				http.Error(w, "invalid request", http.StatusBadRequest)
				return
			}
			if len(request.Input) != 1 || request.Input[0] != "hello" || request.InputType != "document" {
				t.Errorf("embedding request=%#v", request)
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"data": []any{map[string]any{"index": 0, "embedding": []float32{1, 2, 3}}}, "graphit": map[string]any{"revision": "embed-r1", "dimensions": 3}})
		case "/v1/rerank":
			rerankAuth = r.Header.Get("Authorization")
			var request struct {
				Query     string   `json:"query"`
				Documents []string `json:"documents"`
				TopN      int      `json:"top_n"`
			}
			decoder := json.NewDecoder(r.Body)
			decoder.DisallowUnknownFields()
			if err := decoder.Decode(&request); err != nil {
				t.Errorf("invalid rerank request: %v", err)
				http.Error(w, "invalid request", http.StatusBadRequest)
				return
			}
			if request.Query != "q" || len(request.Documents) != 1 || request.Documents[0] != "a" || request.TopN != 1 {
				t.Errorf("rerank request=%#v", request)
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"results": []any{map[string]any{"index": 0, "relevance_score": 0.9}}, "graphit": map[string]any{"revision": "rerank-r1"}})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "broker", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}, Broker: &auth.BrokerConfig{Endpoint: server.URL}, AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "p", Provider: "broker", Username: "alice", BrokerKey: "secret"}); err != nil {
		t.Fatal(err)
	}
	embedder, err := newDirectEmbeddingClientFromConfig()
	if err != nil {
		t.Fatal(err)
	}
	vector, err := embedder.Embed(context.Background(), "hello")
	if err != nil || len(vector) != 3 {
		t.Fatalf("vector=%v err=%v", vector, err)
	}
	if embedder.ModelName() != "embed-r1" || embedder.Dimensions() != 3 {
		t.Fatalf("identity=%s/%d", embedder.ModelName(), embedder.Dimensions())
	}
	reranker, err := NewRerankerFromConfig(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	hits, err := reranker.Rank(context.Background(), "q", []RerankHit{{Text: "a", Index: 0}})
	if err != nil || len(hits) != 1 {
		t.Fatalf("hits=%#v err=%v", hits, err)
	}
	if embeddingAuth != "Bearer secret" || rerankAuth != "Bearer secret" {
		t.Fatalf("auth embedding=%q rerank=%q", embeddingAuth, rerankAuth)
	}
	providerName, model, dimensions := ConfiguredEmbeddingIdentity()
	if providerName != "broker:"+server.URL || model != "embed-r1" || dimensions != 3 {
		t.Fatalf("identity=%q %q %d", providerName, model, dimensions)
	}
}

func TestAnonymousBrokerProviderOmitsAuthorization(t *testing.T) {
	var authorization string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "services": map[string]any{
				"embeddings": map[string]any{"protocol": "openai-embeddings-v1", "path": "/v1/embeddings", "revision": "embed-r1", "dimensions": 3, "max_batch": 10},
			}})
		case "/v1/embeddings":
			authorization = r.Header.Get("Authorization")
			_ = json.NewEncoder(w).Encode(map[string]any{"data": []any{map[string]any{"index": 0, "embedding": []float32{1, 2, 3}}}, "graphit": map[string]any{"revision": "embed-r1", "dimensions": 3}})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "public", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}, Broker: &auth.BrokerConfig{Endpoint: server.URL, AllowAnonymous: true}, AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "public", Provider: provider.Name, Issuer: "anonymous", Subject: "anonymous", Username: "anonymous"}); err != nil {
		t.Fatal(err)
	}
	client, err := newDirectEmbeddingClientFromConfig()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := client.Embed(context.Background(), "public"); err != nil {
		t.Fatal(err)
	}
	if authorization != "" {
		t.Fatalf("anonymous embedding sent Authorization %q", authorization)
	}
}
