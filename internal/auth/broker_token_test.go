package auth

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"sync"
	"testing"
	"time"
)

func TestBrokerCredentialResolverRelaysOrExchangesPerRequestIdentity(t *testing.T) {
	var mu sync.Mutex
	calls := map[string]int{}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Error(err)
		}
		if r.Form.Get("grant_type") != "urn:ietf:params:oauth:grant-type:token-exchange" || r.Form.Get("audience") != "graphit-broker" {
			t.Errorf("form=%v", r.Form)
		}
		subject := r.Form.Get("subject_token")
		mu.Lock()
		calls[subject]++
		mu.Unlock()
		if subject == "reject-me" {
			http.Error(w, `{"error":"invalid_grant"}`, http.StatusBadRequest)
			return
		}
		if subject == "invalid-response" {
			_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "unexpected", "token_type": "Bearer", "expires_in": 0})
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "broker-" + subject, "token_type": "Bearer", "expires_in": 60})
	}))
	defer server.Close()

	now := time.Unix(1_700_000_000, 0)
	provider := Provider{Name: "corp", Type: ProviderOIDC, Revision: 3,
		OIDC:   &OIDCConfig{Issuer: server.URL, ClientID: "graphit", TokenAuthMethod: "none", UsernameClaim: "preferred_username"},
		Broker: &BrokerConfig{Endpoint: server.URL, Audience: "graphit-broker", TokenStrategy: "token-exchange", TokenExchangeEndpoint: server.URL}}
	resolver := &BrokerCredentialResolver{OIDC: &OIDCClient{HTTP: server.Client(), Now: func() time.Time { return now }}, Now: func() time.Time { return now }}
	snapshot := Snapshot{Provider: provider, Profile: Profile{OIDC: &OIDCSession{AccessToken: "profile-token"}}}

	for _, source := range []string{"alice-token", "bob-token", "alice-token"} {
		got, err := resolver.Resolve(WithBrokerBearer(context.Background(), source), snapshot)
		if err != nil || got != "broker-"+source {
			t.Fatalf("source=%q got=%q err=%v", source, got, err)
		}
	}
	mu.Lock()
	if calls["alice-token"] != 1 || calls["bob-token"] != 1 {
		t.Fatalf("calls=%v", calls)
	}
	mu.Unlock()

	now = now.Add(50 * time.Second)
	if _, err := resolver.Resolve(WithBrokerBearer(context.Background(), "alice-token"), snapshot); err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	if calls["alice-token"] != 2 {
		t.Fatalf("expired cache was used: %v", calls)
	}
	mu.Unlock()
	if got, err := resolver.Resolve(WithBrokerBearer(context.Background(), "reject-me"), snapshot); err == nil || got == "reject-me" {
		t.Fatalf("exchange failure fell back to relay: token=%q err=%v", got, err)
	}
	if got, err := resolver.Resolve(WithBrokerBearer(context.Background(), "invalid-response"), snapshot); err == nil || got != "" {
		t.Fatalf("invalid exchange response was accepted: token=%q err=%v", got, err)
	}

	relayProvider := provider
	relayProvider.Broker = &BrokerConfig{Endpoint: server.URL, TokenStrategy: "relay"}
	relay, err := resolver.Resolve(WithBrokerBearer(context.Background(), "raw-user-token"), Snapshot{Provider: relayProvider})
	if err != nil || relay != "raw-user-token" {
		t.Fatalf("relay=%q err=%v", relay, err)
	}
}

func TestExchangeAccessTokenUsesConfiguredClientAuthentication(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		username, password, ok := r.BasicAuth()
		if !ok || username != "client" || password != "synthetic-secret" {
			http.Error(w, "missing client authentication", http.StatusUnauthorized)
			return
		}
		_ = r.ParseForm()
		if _, err := url.ParseRequestURI(r.URL.RequestURI()); err != nil {
			t.Error(err)
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "exchanged", "token_type": "bearer", "expires_in": 30})
	}))
	defer server.Close()
	provider := Provider{OIDC: &OIDCConfig{ClientID: "client", ClientSecret: "synthetic-secret", TokenAuthMethod: "client_secret_basic"}, Broker: &BrokerConfig{Audience: "broker", TokenExchangeEndpoint: server.URL}}
	token, err := (&OIDCClient{HTTP: server.Client()}).ExchangeAccessToken(context.Background(), provider, "subject-token")
	if err != nil || token.AccessToken != "exchanged" || !token.ExpiresAt.After(time.Now()) {
		t.Fatalf("token=%#v err=%v", token, err)
	}
}
