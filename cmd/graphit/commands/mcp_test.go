package commands

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
)

func TestResolveMCPStdioBearerUsesDaemonKeyWithoutActiveProfile(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	keyPath := daemonctl.KeyFilePath()
	if err := os.MkdirAll(filepath.Dir(keyPath), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(keyPath, []byte("runtime-key\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	got, err := resolveMCPStdioBearer(context.Background())
	if err != nil || got != "runtime-key" {
		t.Fatalf("bearer=%q err=%v", got, err)
	}

	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "local", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "local-user", Provider: provider.Name, Username: "alice"}); err != nil {
		t.Fatal(err)
	}
	got, err = resolveMCPStdioBearer(context.Background())
	if err != nil || got != "runtime-key" {
		t.Fatalf("local fallback bearer=%q err=%v", got, err)
	}
}

func TestResolveMCPStdioBearerTracksActiveProfileCredential(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	local := auth.Provider{Name: "local", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}}
	if err := store.AddProvider(local); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "local-user", Provider: local.Name, Username: "alice", MCPKey: "local-key"}); err != nil {
		t.Fatal(err)
	}
	got, err := resolveMCPStdioBearer(context.Background())
	if err != nil || got != "local-key" {
		t.Fatalf("local bearer=%q err=%v", got, err)
	}

	broker := auth.Provider{
		Name: "broker", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: "https://broker.example"},
		AI: auth.AIConfig{
			Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker},
			Rerank:    auth.AIServiceConfig{Mode: auth.ServiceBroker},
		},
	}
	if err := store.AddProvider(broker); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{
		Name: "broker-user", Provider: broker.Name, Issuer: "https://broker.example", Subject: "subject", Username: "bob",
		OIDC: &auth.OIDCSession{AccessToken: "broker-access-token", IDToken: "id-token", ExpiresAt: time.Now().Add(time.Hour)},
	}); err != nil {
		t.Fatal(err)
	}
	got, err = resolveMCPStdioBearer(context.Background())
	if err != nil || got != "broker-access-token" {
		t.Fatalf("broker bearer=%q err=%v", got, err)
	}
}
