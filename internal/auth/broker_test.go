package auth

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestBrokerDiscoveryAndCredentialExchange(t *testing.T) {
	var authorization string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "issuer": serverURL(r), "services": map[string]any{
				"embeddings":     map[string]any{"protocol": "openai-embeddings-v1", "path": "/v1/embeddings", "revision": "r1", "dimensions": 3},
				"s3_credentials": map[string]any{"protocol": "graphit-s3-credentials-v2", "path": "/v1/s3/credentials", "authorization_revision": "acl-1"},
			}})
		case "/v1/s3/credentials":
			authorization = r.Header.Get("Authorization")
			var request map[string]any
			_ = json.NewDecoder(r.Body).Decode(&request)
			if request["scope"] != "project" || request["project_id"] != "project-a" || len(request) != 2 {
				t.Errorf("broker request scope: %#v", request)
			}
			_ = json.NewEncoder(w).Encode(struct {
				S3Credentials
				Scope     string `json:"scope"`
				ProjectID string `json:"project_id,omitempty"`
			}{S3Credentials: S3Credentials{AccessKeyID: "A", SecretAccessKey: "S", SessionToken: "T", ExpiresAt: time.Now().Add(time.Hour), Bucket: "b", Region: "r", Endpoint: serverURL(r), Prefixes: []string{"users/alice"}, AuthorizationRevision: "acl-1"}, Scope: "project", ProjectID: "project-a"})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	provider := Provider{Name: "p", Type: ProviderBroker, Broker: &BrokerConfig{Endpoint: server.URL}, S3: S3Config{CredentialSource: "broker"}, AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
	discovery, err := DiscoverBroker(context.Background(), provider, server.Client())
	if err != nil || discovery.Services.Embeddings == nil || discovery.Services.S3Credentials == nil {
		t.Fatalf("discovery=%#v err=%v", discovery, err)
	}
	credentials, err := (BrokerCredentialExchanger{HTTP: server.Client()}).ExchangeForScope(context.Background(), provider, Profile{BrokerKey: "broker-secret"}, ProjectStorageScope("project-a"))
	if err != nil {
		t.Fatal(err)
	}
	if authorization != "Bearer broker-secret" || credentials.AuthorizationRevision != "acl-1" || credentials.Bucket != "b" || credentials.Endpoint != server.URL {
		t.Fatalf("authorization=%q credentials=%#v", authorization, credentials)
	}
}

func TestBrokerCredentialExchangeDistinguishesDisabledFromInvalidS3(t *testing.T) {
	tests := []struct {
		name       string
		capability any
		wantAbsent bool
	}{
		{name: "disabled", capability: nil, wantAbsent: true},
		{name: "invalid protocol", capability: map[string]any{"protocol": "unsupported", "path": "/v1/s3/credentials"}},
		{name: "invalid path", capability: map[string]any{"protocol": "graphit-s3-credentials-v2", "path": "https://other.example/credentials"}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			credentialRequests := 0
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch r.URL.Path {
				case "/.well-known/graphit-broker":
					services := map[string]any{}
					if tc.capability != nil {
						services["s3_credentials"] = tc.capability
					}
					_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "services": services})
				case "/v1/s3/credentials":
					credentialRequests++
					http.Error(w, "must not be called", http.StatusInternalServerError)
				default:
					http.NotFound(w, r)
				}
			}))
			defer server.Close()

			provider := brokerProviderForTest(server.URL)
			_, err := (BrokerCredentialExchanger{HTTP: server.Client()}).ExchangeForScope(context.Background(), provider, Profile{BrokerKey: "token"}, ProjectStorageScope("project-a"))
			if tc.wantAbsent {
				if !errors.Is(err, ErrBrokerS3Unavailable) {
					t.Fatalf("error = %v, want ErrBrokerS3Unavailable", err)
				}
			} else if err == nil || errors.Is(err, ErrBrokerS3Unavailable) {
				t.Fatalf("invalid advertised capability error = %v", err)
			}
			if credentialRequests != 0 {
				t.Fatalf("credential endpoint requests = %d, want 0", credentialRequests)
			}
		})
	}
}

func TestBrokerCredentialExchangeRejectsDifferentResponseScope(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "services": map[string]any{
				"s3_credentials": map[string]any{"protocol": "graphit-s3-credentials-v2", "path": "/v1/s3/credentials", "authorization_revision": "1"},
			}})
		case "/v1/s3/credentials":
			_ = json.NewEncoder(w).Encode(struct {
				S3Credentials
				Scope     string `json:"scope"`
				ProjectID string `json:"project_id,omitempty"`
			}{S3Credentials: S3Credentials{AccessKeyID: "A", SecretAccessKey: "S", SessionToken: "T", ExpiresAt: time.Now().Add(time.Hour), Bucket: "b", Region: "r", Prefixes: []string{"v2"}, AuthorizationRevision: "1"}, Scope: "project", ProjectID: "project-b"})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	provider := brokerProviderForTest(server.URL)
	_, err := (BrokerCredentialExchanger{HTTP: server.Client()}).ExchangeForScope(context.Background(), provider, Profile{BrokerKey: "token"}, ProjectStorageScope("project-a"))
	if err == nil || !strings.Contains(err.Error(), "different storage scope") {
		t.Fatalf("scope mismatch error=%v", err)
	}
}

func TestBrokerCredentialsRejectIncompleteTopologyAndExpiredGrant(t *testing.T) {
	base := S3Credentials{AccessKeyID: "A", SecretAccessKey: "S", SessionToken: "T", ExpiresAt: time.Now().Add(time.Hour), Bucket: "b", Region: "r", Prefixes: []string{"users/alice"}, AuthorizationRevision: "acl-1"}
	if err := validateBrokerS3Credentials(base, "acl-1", time.Now()); err != nil {
		t.Fatal(err)
	}
	for name, mutate := range map[string]func(*S3Credentials){
		"expired":           func(c *S3Credentials) { c.ExpiresAt = time.Now().Add(-time.Minute) },
		"no token":          func(c *S3Credentials) { c.SessionToken = "" },
		"no bucket":         func(c *S3Credentials) { c.Bucket = "" },
		"revision mismatch": func(c *S3Credentials) { c.AuthorizationRevision = "acl-2" },
		"multiple roots":    func(c *S3Credentials) { c.Prefixes = []string{"a", "b"} },
	} {
		t.Run(name, func(t *testing.T) {
			got := base
			mutate(&got)
			if err := validateBrokerS3Credentials(got, "acl-1", time.Now()); err == nil {
				t.Fatalf("invalid grant accepted: %#v", got.RedactedForTest())
			}
		})
	}
}

func (c S3Credentials) RedactedForTest() S3Credentials {
	c.AccessKeyID, c.SecretAccessKey, c.SessionToken = "[redacted]", "[redacted]", "[redacted]"
	return c
}

type exchangerFunc func(context.Context, Provider, Profile) (S3Credentials, error)

func (f exchangerFunc) Exchange(ctx context.Context, provider Provider, profile Profile) (S3Credentials, error) {
	return f(ctx, provider, profile)
}

func serverURL(r *http.Request) string { return "http://" + r.Host }

func TestProviderRejectsModelSelectionForLocalAndBrokerAI(t *testing.T) {
	providers := []Provider{
		{Name: "local", Type: ProviderLocal, Local: &LocalConfig{}, AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceLocal, Model: "forbidden"}}},
		{Name: "broker", Type: ProviderLocal, Local: &LocalConfig{}, Broker: &BrokerConfig{Endpoint: "https://broker.example"}, AI: AIConfig{
			Embedding: AIServiceConfig{Mode: ServiceBroker, Dimensions: 768},
			Rerank:    AIServiceConfig{Mode: ServiceBroker},
		}},
	}
	for _, provider := range providers {
		if err := ValidateProvider(provider); err == nil {
			t.Fatalf("provider accepted %#v", provider)
		}
	}
}

func TestProviderRequiresBrokerForBothAIServices(t *testing.T) {
	brokerService := AIServiceConfig{Mode: ServiceBroker}
	directEmbedding := AIServiceConfig{Mode: ServiceDirect, Protocol: "openai-compatible", Endpoint: "https://ai.example/v1", Model: "embed", Dimensions: 768}
	directRerank := AIServiceConfig{Mode: ServiceDirect, Protocol: "cohere", Endpoint: "https://ai.example", Model: "rerank"}
	tests := []struct {
		name      string
		embedding AIServiceConfig
		rerank    AIServiceConfig
	}{
		{name: "embedding omitted", rerank: brokerService},
		{name: "embedding local", embedding: AIServiceConfig{Mode: ServiceLocal}, rerank: brokerService},
		{name: "embedding direct", embedding: directEmbedding, rerank: brokerService},
		{name: "embedding disabled", embedding: AIServiceConfig{Mode: ServiceDisabled}, rerank: brokerService},
		{name: "rerank omitted", embedding: brokerService},
		{name: "rerank local", embedding: brokerService, rerank: AIServiceConfig{Mode: ServiceLocal}},
		{name: "rerank direct", embedding: brokerService, rerank: directRerank},
		{name: "rerank disabled", embedding: brokerService, rerank: AIServiceConfig{Mode: ServiceDisabled}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, Broker: &BrokerConfig{Endpoint: "https://broker.example"}, AI: AIConfig{Embedding: tt.embedding, Rerank: tt.rerank}}
			err := ValidateProvider(provider)
			if err == nil || !strings.Contains(err.Error(), "requires embedding and rerank modes to be broker") {
				t.Fatalf("expected broker-only AI routing rejection, got %v", err)
			}
		})
	}
}

func TestProviderAcceptsBrokerForBothAIServices(t *testing.T) {
	provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, Broker: &BrokerConfig{Endpoint: "https://broker.example"}, AI: AIConfig{
		Embedding: AIServiceConfig{Mode: ServiceBroker},
		Rerank:    AIServiceConfig{Mode: ServiceBroker},
	}}
	if err := ValidateProvider(provider); err != nil {
		t.Fatalf("broker-only AI routing rejected: %v", err)
	}
}

func TestBrokerAuthenticationProviderRejectsClientSelectedTokenContract(t *testing.T) {
	for name, mutate := range map[string]func(*BrokerConfig){
		"audience": func(c *BrokerConfig) { c.Audience = "selected-by-client" },
		"resource": func(c *BrokerConfig) { c.Resource = "https://other.example" },
		"exchange": func(c *BrokerConfig) { c.TokenExchangeEndpoint = "https://id.example/token" },
	} {
		t.Run(name, func(t *testing.T) {
			config := &BrokerConfig{Endpoint: "https://broker.example", TokenStrategy: "relay"}
			mutate(config)
			provider := Provider{Name: "broker", Type: ProviderBroker, Broker: config, AI: AIConfig{
				Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
			if err := ValidateProvider(provider); err == nil || !strings.Contains(err.Error(), "discovers its token contract") {
				t.Fatalf("provider validation error = %v", err)
			}
		})
	}
}

func TestProviderKeepsNonBrokerAIModesWithoutBroker(t *testing.T) {
	tests := []struct {
		name string
		ai   AIConfig
	}{
		{name: "local", ai: AIConfig{Embedding: AIServiceConfig{Mode: ServiceLocal}, Rerank: AIServiceConfig{Mode: ServiceLocal}}},
		{name: "disabled", ai: AIConfig{Embedding: AIServiceConfig{Mode: ServiceDisabled}, Rerank: AIServiceConfig{Mode: ServiceDisabled}}},
		{name: "direct", ai: AIConfig{
			Embedding: AIServiceConfig{Mode: ServiceDirect, Protocol: "openai-compatible", Endpoint: "https://ai.example/v1", Model: "embed", Dimensions: 768},
			Rerank:    AIServiceConfig{Mode: ServiceDirect, Protocol: "cohere", Endpoint: "https://ai.example", Model: "rerank"},
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, AI: tt.ai}
			if err := ValidateProvider(provider); err != nil {
				t.Fatalf("non-broker provider rejected: %v", err)
			}
		})
	}
}

func TestProviderAcceptsEmbeddingSimulatedRerankProtocolsWithDimensions(t *testing.T) {
	for _, protocol := range []string{"openai", "openai-compatible", "openai-embeddings-v1", "google"} {
		t.Run(protocol, func(t *testing.T) {
			provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, AI: AIConfig{
				Embedding: AIServiceConfig{Mode: ServiceDisabled},
				Rerank:    AIServiceConfig{Mode: ServiceDirect, Protocol: protocol, Endpoint: "https://ai.example/v1", Model: "embed", Dimensions: 768},
			}}
			if err := ValidateProvider(provider); err != nil {
				t.Fatalf("embedding-simulated rerank protocol rejected: %v", err)
			}
		})
	}
}

func TestProviderValidatesRerankDimensionsByProtocolKind(t *testing.T) {
	tests := []struct {
		name       string
		protocol   string
		dimensions int
		want       string
	}{
		{name: "simulated requires dimensions", protocol: "google", want: "requires positive dimensions"},
		{name: "native rejects dimensions", protocol: "cohere", dimensions: 768, want: "does not accept dimensions"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, AI: AIConfig{
				Embedding: AIServiceConfig{Mode: ServiceDisabled},
				Rerank:    AIServiceConfig{Mode: ServiceDirect, Protocol: tc.protocol, Endpoint: "https://ai.example/v1", Model: "model", Dimensions: tc.dimensions},
			}}
			err := ValidateProvider(provider)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error = %v, want substring %q", err, tc.want)
			}
		})
	}
}

func TestProviderRejectsBrokerAIWithoutBroker(t *testing.T) {
	for _, ai := range []AIConfig{
		{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceLocal}},
		{Embedding: AIServiceConfig{Mode: ServiceLocal}, Rerank: AIServiceConfig{Mode: ServiceBroker}},
	} {
		provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, AI: ai}
		err := ValidateProvider(provider)
		if err == nil || !strings.Contains(err.Error(), "broker AI mode requires broker configuration") {
			t.Fatalf("expected missing broker rejection, got %v", err)
		}
	}
}

func TestProviderAllowsHTTPOnlyForActualLoopbackHost(t *testing.T) {
	for _, endpoint := range []string{"http://localhost.evil.example", "http://127.0.0.1.evil.example", "http://example.com"} {
		provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, Broker: &BrokerConfig{Endpoint: endpoint}, AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
		if err := ValidateProvider(provider); err == nil {
			t.Fatalf("unsafe broker endpoint %q was accepted", endpoint)
		}
	}
	for _, endpoint := range []string{"http://localhost:8080", "http://127.0.0.1:8080", "http://[::1]:8080", "https://broker.example"} {
		provider := Provider{Name: "p", Type: ProviderLocal, Local: &LocalConfig{}, Broker: &BrokerConfig{Endpoint: endpoint}, AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
		if err := ValidateProvider(provider); err != nil {
			t.Fatalf("safe broker endpoint %q rejected: %v", endpoint, err)
		}
	}
}

func TestExplicitBrokerKeyOverridesOIDCAccessToken(t *testing.T) {
	profile := Profile{BrokerKey: "broker-key", OIDC: &OIDCSession{AccessToken: "oidc-token"}}
	if got := brokerToken(profile); got != "broker-key" {
		t.Fatalf("broker token = %q", got)
	}
}

func TestBrokerHubAccessClientKeepsConcurrentRequestBearersIsolated(t *testing.T) {
	var mu sync.Mutex
	seen := map[string]int{}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "services": map[string]any{"hub_access": map[string]any{
				"protocol": "graphit-hub-access-v1", "path": "/v1/hub/access/resolve", "authorization_revision": "12",
			}}})
		case "/v1/hub/access/resolve":
			bearer := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
			mu.Lock()
			seen[bearer]++
			mu.Unlock()
			_ = json.NewEncoder(w).Encode(map[string]any{"v": 1, "authorization_revision": "12", "subject": "issuer|" + bearer, "selectors": []map[string]any{{"all": true}}})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := Provider{Name: "corp", Type: ProviderLocal, Local: &LocalConfig{}, Broker: &BrokerConfig{Endpoint: server.URL}, AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "profile", Provider: provider.Name, Username: "service", BrokerKey: "profile-key"}); err != nil {
		t.Fatal(err)
	}
	client, err := NewBrokerHubAccessClient(context.Background(), server.Client())
	if err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	for _, token := range []string{"alice-token", "bob-token"} {
		token := token
		wg.Add(1)
		go func() {
			defer wg.Done()
			response, resolveErr := client.Resolve(WithBrokerBearer(context.Background(), token))
			if resolveErr != nil || response.Subject != "issuer|"+token {
				t.Errorf("token=%q response=%#v err=%v", token, response, resolveErr)
			}
		}()
	}
	wg.Wait()
	mu.Lock()
	defer mu.Unlock()
	if seen["alice-token"] != 1 || seen["bob-token"] != 1 || seen["profile-key"] != 0 {
		t.Fatalf("broker bearers crossed requests: %v", seen)
	}
}
