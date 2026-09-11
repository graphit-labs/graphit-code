package config

import (
	"context"
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
