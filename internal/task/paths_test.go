package task

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func TestTableURIUsesLocalPathForBrokerWithoutS3(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "company", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: "https://broker.example", TokenStrategy: "relay"},
		AI:     auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}},
		S3:     auth.S3Config{CredentialSource: "broker"}}
	if err := authStore.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{Name: "alice", Provider: "company", ProviderRevision: 1, Issuer: provider.Broker.Endpoint, Subject: "user-1", Username: "alice", BrokerS3Disabled: true,
		OIDC: &auth.OIDCSession{AccessToken: "access", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)}}); err != nil {
		t.Fatal(err)
	}

	const projectID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	got := TableURI(projectID, config.ConfigMap{})
	want := filepath.Join(store.TaskTableRoot(), "tasks", projectID)
	if got != want {
		t.Fatalf("TableURI() = %q, want %q", got, want)
	}
}

func TestTableURIPlacesTaskPrefixUnderHubPrefix(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(auth.Provider{Name: "test", Type: auth.ProviderLocal, Local: &auth.LocalConfig{AllowAWSCredentialChain: true}, S3: auth.S3Config{Bucket: "shared", Prefix: "graphit/team"}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "test", Provider: "test", Username: "test"}); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GRAPHIT_TASK_PREFIX", "/work/tasks/")
	cfg := config.ConfigMap{
		"hub":  map[string]any{"bucket": "shared", "prefix": "graphit/team"},
		"task": map[string]any{"prefix": "/work/tasks/"},
	}
	got := TableURI("01ARZ3NDEKTSV4RRFFQ69G5FAV", cfg)
	if got != "s3://shared/graphit/team/v2/projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/work/tasks" {
		t.Fatalf("TableURI() = %q", got)
	}
}

func TestTableURIRemoteRejectsNonULIDProject(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(auth.Provider{Name: "test", Type: auth.ProviderLocal, Local: &auth.LocalConfig{AllowAWSCredentialChain: true}, S3: auth.S3Config{Bucket: "shared"}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "test", Provider: "test", Username: "test"}); err != nil {
		t.Fatal(err)
	}
	cfg := config.ConfigMap{"hub": map[string]any{"bucket": "shared"}}
	if got := TableURI("project", cfg); got != "" {
		t.Fatalf("TableURI() = %q", got)
	}
}
