package config

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestHubS3ConfigTreatsAuthenticatedProvidersWithoutS3AsLocal(t *testing.T) {
	tests := []struct {
		name     string
		provider auth.Provider
	}{
		{name: "OIDC", provider: auth.Provider{Name: "identity", Type: auth.ProviderOIDC,
			OIDC: &auth.OIDCConfig{Issuer: "https://identity.example", ClientID: "graphit", UsernameClaim: "preferred_username"}}},
		{name: "Broker", provider: auth.Provider{Name: "company", Type: auth.ProviderBroker,
			Broker: &auth.BrokerConfig{Endpoint: "https://broker.example", TokenStrategy: "relay"},
			AI:     auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}},
			S3:     auth.S3Config{CredentialSource: "broker"}}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
			store, err := auth.Open()
			if err != nil {
				t.Fatal(err)
			}
			if err := store.AddProvider(tc.provider); err != nil {
				t.Fatal(err)
			}
			issuer := "https://identity.example"
			if tc.provider.Broker != nil {
				issuer = tc.provider.Broker.Endpoint
			}
			profile := auth.Profile{Name: "alice", Provider: tc.provider.Name, ProviderRevision: 1, Issuer: issuer, Subject: "user-1", Username: "alice",
				OIDC: &auth.OIDCSession{AccessToken: "access", RefreshToken: "refresh", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)}}
			profile.BrokerS3Disabled = tc.provider.Type == auth.ProviderBroker
			if err := store.Login(profile); err != nil {
				t.Fatal(err)
			}

			cfg := hubS3Config(context.Background(), nil)
			if cfg.ResolutionError != nil || cfg.Configured() {
				t.Fatalf("S3 config = %#v, resolution error = %v", cfg, cfg.ResolutionError)
			}
			if cfg.Bucket != "" || cfg.AccessKeyID != "" || cfg.SecretAccessKey != "" || cfg.SessionToken != "" {
				t.Fatalf("local mode exposed S3 topology or credentials: %#v", cfg)
			}
		})
	}
}

func TestOIDCSTSConfigExchangesPerScopeWithoutPersistingGrants(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	var calls atomic.Int32
	sts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil || !strings.Contains(r.Form.Get("Policy"), "v2/") {
			http.Error(w, "missing scoped policy", http.StatusBadRequest)
			return
		}
		key := fmt.Sprintf("temporary-%d", calls.Add(1))
		w.Header().Set("Content-Type", "text/xml")
		fmt.Fprintf(w, `<AssumeRoleWithWebIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/"><AssumeRoleWithWebIdentityResult><Credentials><AccessKeyId>%s</AccessKeyId><SecretAccessKey>temporary-secret</SecretAccessKey><SessionToken>temporary-session</SessionToken><Expiration>%s</Expiration></Credentials></AssumeRoleWithWebIdentityResult><ResponseMetadata><RequestId>request</RequestId></ResponseMetadata></AssumeRoleWithWebIdentityResponse>`, key, time.Now().Add(time.Hour).UTC().Format(time.RFC3339))
	}))
	defer sts.Close()
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "oidc-scope", Type: auth.ProviderOIDC,
		OIDC: &auth.OIDCConfig{Issuer: "https://id.example", ClientID: "graphit", UsernameClaim: "preferred_username"},
		S3:   auth.S3Config{Bucket: "artifacts", Region: "us-east-1", Prefix: "graphit", CredentialSource: "sts"},
		STS:  &auth.STSConfig{Endpoint: sts.URL, RoleARN: "arn:aws:iam::123456789012:role/graphit"}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "alice", Provider: provider.Name, Issuer: "https://id.example", Subject: "alice-sub", Username: "alice",
		OIDC: &auth.OIDCSession{AccessToken: "access", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	projectA := ProjectS3Config(ctx, "project-a")
	projectAAgain := ProjectS3Config(ctx, "project-a")
	projectB := ProjectS3Config(ctx, "project-b")
	user := UserS3Config(ctx)
	hub := HubMetadataS3Config(ctx)
	for _, cfg := range []S3Config{projectA, projectAAgain, projectB, user, hub} {
		if cfg.ResolutionError != nil || !cfg.HasStaticCredentials() || cfg.SessionToken == "" || cfg.Prefix != "graphit" {
			t.Fatalf("scoped config = %#v", cfg)
		}
	}
	if projectA.AccessKeyID != projectAAgain.AccessKeyID || projectA.AccessKeyID == projectB.AccessKeyID ||
		projectA.AccessKeyID == user.AccessKeyID || projectA.AccessKeyID == hub.AccessKeyID || calls.Load() != 4 {
		t.Fatalf("STS credentials were reused across scopes; calls = %d", calls.Load())
	}
	unscoped := HubS3Config()
	if unscoped.ResolutionError == nil || unscoped.HasStaticCredentials() {
		t.Fatalf("unscoped OIDC STS credentials were exposed: %#v", unscoped)
	}
	state, err := os.ReadFile(store.Path())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(state), "temporary-") || strings.Contains(string(state), "temporary-session") {
		t.Fatal("STS credentials were written to auth.json")
	}
}
