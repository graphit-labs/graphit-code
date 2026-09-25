package config

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
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
		{name: "Broker", provider: auth.Provider{Name: "company", Type: auth.ProviderBroker,
			Broker: &auth.BrokerConfig{Endpoint: "https://broker.example"},
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

func TestS3ConfigForURIResolvesPhysicalBrokerModuleAndFailsClosed(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	var requested []auth.BrokerStorageScope
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "issuer": "http://" + r.Host, "services": map[string]any{
				"s3_credentials": map[string]any{"protocol": "graphit-s3-credentials-v3", "path": "/v1/s3/credentials", "authorization_revision": "acl-1"},
			}})
		case "/v1/s3/credentials":
			var scope auth.BrokerStorageScope
			if err := json.NewDecoder(r.Body).Decode(&scope); err != nil {
				t.Errorf("decode scope: %v", err)
			}
			requested = append(requested, scope)
			_ = json.NewEncoder(w).Encode(struct {
				auth.S3Credentials
				Scope     string                   `json:"scope"`
				ProjectID string                   `json:"project_id,omitempty"`
				Module    auth.BrokerStorageModule `json:"module"`
			}{S3Credentials: auth.S3Credentials{AccessKeyID: "A-" + string(scope.Module), SecretAccessKey: "S", SessionToken: "T", ExpiresAt: time.Now().Add(time.Hour), Bucket: "bucket", Region: "region", Prefixes: []string{"root"}, AuthorizationRevision: "acl-1"}, Scope: scope.Kind, ProjectID: scope.ProjectID, Module: scope.Module})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "broker", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: server.URL}, S3: auth.S3Config{CredentialSource: "broker"},
		AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "alice", Provider: provider.Name, Issuer: server.URL, Subject: "alice", Username: "alice", OIDC: &auth.OIDCSession{AccessToken: "token", IDToken: "id"}}); err != nil {
		t.Fatal(err)
	}

	projectID := "project-a"
	for _, uri := range []string{
		"s3://bucket/root/v2/projects/" + projectID + "/tasks",
		"s3://bucket/root/v2/projects/" + projectID + "/memory",
		"s3://bucket/root/v2/projects/" + projectID + "/dream",
		"s3://bucket/root/v2/projects/" + projectID + "/knowledge/search",
		"s3://bucket/root/v2/projects/" + projectID + "/ast/graph",
		"s3://bucket/root/v2/projects/" + projectID + "/project.json",
		"s3://bucket/root/v2/projects/" + projectID + "/registry/knowledge/id.json",
		"s3://bucket/root/v2/projects/" + projectID + "/artifacts/knowledge/id/1/file",
		"s3://bucket/root/v2/projects/" + projectID + "/artifacts/ast/id/1/file",
		"s3://bucket/root/v2/projects/" + projectID + "/events/e.json",
		"s3://bucket/root/v2/users/alice/memory",
		"s3://bucket/root/v2/registry/names/name.json",
		"s3://bucket/root/v2/global/rules/rule.json",
	} {
		cfg := S3ConfigForURI(context.Background(), uri)
		if cfg.ResolutionError != nil || cfg.AccessKeyID == "" {
			t.Fatalf("S3ConfigForURI(%q) = %#v", uri, cfg)
		}
	}
	if cfg := S3ConfigForURI(context.Background(), "s3://bucket/root/v2/projects/"+projectID+"/unknown"); cfg.ResolutionError == nil || cfg.AccessKeyID != "" {
		t.Fatalf("unknown project URI did not fail closed: %#v", cfg)
	}
	want := map[auth.BrokerStorageScope]bool{
		auth.ProjectStorageScope(projectID, auth.BrokerStorageModuleTask):      true,
		auth.ProjectStorageScope(projectID, auth.BrokerStorageModuleMemory):    true,
		auth.ProjectStorageScope(projectID, auth.BrokerStorageModuleDream):     true,
		auth.ProjectStorageScope(projectID, auth.BrokerStorageModuleKnowledge): true,
		auth.ProjectStorageScope(projectID, auth.BrokerStorageModuleAST):       true,
		auth.ProjectStorageScope(projectID, auth.BrokerStorageModuleHub):       true,
		auth.UserStorageScope(): true,
		auth.HubStorageScope():  true,
	}
	for _, scope := range requested {
		delete(want, scope)
	}
	if len(want) != 0 {
		t.Fatalf("missing broker scopes: %#v; requested=%#v", want, requested)
	}
}
