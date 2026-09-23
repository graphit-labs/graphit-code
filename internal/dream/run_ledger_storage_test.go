package dream

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestRunLedgerStorageUsesGlobalDreamDirectoryWithoutS3(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	projectDir := t.TempDir()
	projectID := writeDreamTestProject(t, projectDir)
	uri, cfg, err := runLedgerStorage(context.Background(), projectDir)
	if err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(globalDir, "dream", "dreams", projectID)
	if uri != want || cfg.Configured() {
		t.Fatalf("Dream storage = %q, %#v; want local %q", uri, cfg, want)
	}
}

func TestRunLedgerStorageUsesExplicitS3ForLocalProvider(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	projectDir := t.TempDir()
	projectID := writeDreamTestProject(t, projectDir)
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "s3-local", Type: auth.ProviderLocal,
		Local: &auth.LocalConfig{AllowAWSCredentialChain: true},
		S3:    auth.S3Config{Bucket: "bucket", Prefix: "tenant"}}
	if err := authStore.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{Name: "local", Provider: provider.Name, Username: "local"}); err != nil {
		t.Fatal(err)
	}
	uri, cfg, err := runLedgerStorage(context.Background(), projectDir)
	if err != nil {
		t.Fatal(err)
	}
	want := "s3://bucket/tenant/v2/projects/" + projectID + "/dream"
	if uri != want || !cfg.Configured() || cfg.Bucket != "bucket" {
		t.Fatalf("Dream storage = %q, %#v; want explicit S3 %q", uri, cfg, want)
	}
}

func TestRunLedgerStorageFallsBackToLocalWhenBrokerS3IsDisabled(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	projectDir := t.TempDir()
	projectID := writeDreamTestProject(t, projectDir)
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "broker-no-s3", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: "https://broker.example"},
		AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker},
			Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}}}
	if err := authStore.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{Name: "broker-user", Provider: provider.Name,
		Issuer: "https://broker.example", Subject: "broker-user", Username: "broker-user",
		OIDC:             &auth.OIDCSession{AccessToken: "access", IDToken: "identity"},
		BrokerS3Disabled: true}); err != nil {
		t.Fatal(err)
	}
	uri, cfg, err := runLedgerStorage(context.Background(), projectDir)
	if err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(globalDir, "dream", "dreams", projectID)
	if uri != want || cfg.Configured() {
		t.Fatalf("Dream storage = %q, %#v; want local %q", uri, cfg, want)
	}
}

func TestRunLedgerStorageUsesScopedOIDCSTSGrant(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	projectDir := t.TempDir()
	projectID := writeDreamTestProject(t, projectDir)
	sts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// OIDC STS currently constrains grants to the project namespace; the
		// scoped request/cache key still distinguishes the Dream module.
		if err := r.ParseForm(); err != nil || !strings.Contains(r.Form.Get("Policy"), "v2/projects/"+projectID+"/*") {
			t.Errorf("OIDC STS scope policy = %q, parse error = %v", r.Form.Get("Policy"), err)
			http.Error(w, "missing Dream scope", http.StatusBadRequest)
			return
		}
		w.Header().Set("Content-Type", "text/xml")
		fmt.Fprintf(w, `<AssumeRoleWithWebIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/"><AssumeRoleWithWebIdentityResult><Credentials><AccessKeyId>temporary-dream</AccessKeyId><SecretAccessKey>temporary-secret</SecretAccessKey><SessionToken>temporary-session</SessionToken><Expiration>%s</Expiration></Credentials></AssumeRoleWithWebIdentityResult><ResponseMetadata><RequestId>request</RequestId></ResponseMetadata></AssumeRoleWithWebIdentityResponse>`, time.Now().Add(time.Hour).UTC().Format(time.RFC3339))
	}))
	defer sts.Close()
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "oidc-dream", Type: auth.ProviderOIDC,
		OIDC: &auth.OIDCConfig{Issuer: "https://identity.example", ClientID: "graphit", UsernameClaim: "preferred_username"},
		S3:   auth.S3Config{Bucket: "bucket", Region: "us-east-1", Prefix: "tenant", CredentialSource: "sts"},
		STS:  &auth.STSConfig{Endpoint: sts.URL, RoleARN: "arn:aws:iam::123456789012:role/graphit"}}
	if err := authStore.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{Name: "alice", Provider: provider.Name,
		Issuer: "https://identity.example", Subject: "alice", Username: "alice",
		OIDC: &auth.OIDCSession{AccessToken: "access", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	uri, cfg, err := runLedgerStorage(context.Background(), projectDir)
	if err != nil {
		t.Fatal(err)
	}
	want := "s3://bucket/tenant/v2/projects/" + projectID + "/dream"
	if uri != want || cfg.AccessKeyID != "temporary-dream" || cfg.SessionToken != "temporary-session" || cfg.Refresh == nil {
		t.Fatalf("Dream storage = %q, %#v; want scoped OIDC S3 %q", uri, cfg, want)
	}
	state, err := os.ReadFile(authStore.Path())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(state), "temporary-dream") || strings.Contains(string(state), "temporary-session") {
		t.Fatal("temporary Dream credentials were persisted in auth.json")
	}
}

func TestRunLedgerStorageUsesScopedBrokerGrant(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	projectDir := t.TempDir()
	projectID := writeDreamTestProject(t, projectDir)
	var requested auth.BrokerStorageScope
	broker := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "issuer": "http://" + r.Host,
				"services": map[string]any{"s3_credentials": map[string]any{
					"protocol": "graphit-s3-credentials-v3", "path": "/v1/s3/credentials", "authorization_revision": "acl-1"}}})
		case "/v1/s3/credentials":
			if err := json.NewDecoder(r.Body).Decode(&requested); err != nil {
				t.Errorf("decode Broker scope: %v", err)
			}
			_ = json.NewEncoder(w).Encode(struct {
				auth.S3Credentials
				Scope     string                   `json:"scope"`
				ProjectID string                   `json:"project_id"`
				Module    auth.BrokerStorageModule `json:"module"`
			}{S3Credentials: auth.S3Credentials{AccessKeyID: "temporary-dream", SecretAccessKey: "secret", SessionToken: "session", ExpiresAt: time.Now().Add(time.Hour), Bucket: "bucket", Region: "region", Prefixes: []string{"tenant"}, AuthorizationRevision: "acl-1"},
				Scope: requested.Kind, ProjectID: requested.ProjectID, Module: requested.Module})
		default:
			http.NotFound(w, r)
		}
	}))
	defer broker.Close()
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "broker-dream", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: broker.URL}, S3: auth.S3Config{CredentialSource: "broker"},
		AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker},
			Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}}}
	if err := authStore.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{Name: "alice", Provider: provider.Name,
		Issuer: broker.URL, Subject: "alice", Username: "alice",
		OIDC: &auth.OIDCSession{AccessToken: "access", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	uri, cfg, err := runLedgerStorage(context.Background(), projectDir)
	if err != nil {
		t.Fatal(err)
	}
	want := "s3://bucket/tenant/v2/projects/" + projectID + "/dream"
	if uri != want || cfg.AccessKeyID != "temporary-dream" || cfg.Refresh == nil ||
		requested != auth.ProjectStorageScope(projectID, auth.BrokerStorageModuleDream) {
		t.Fatalf("Dream storage = %q, requested=%#v; want scoped Broker S3 %q", uri, requested, want)
	}
}

func TestRunLedgerStorageDoesNotFallbackAfterBrokerResolutionFailure(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	projectDir := t.TempDir()
	writeDreamTestProject(t, projectDir)
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "broken-broker", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: "http://127.0.0.1:1"},
		S3:     auth.S3Config{CredentialSource: "broker"},
		AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker},
			Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}}}
	if err := authStore.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{Name: "alice", Provider: provider.Name,
		Issuer: "http://127.0.0.1:1", Subject: "alice", Username: "alice",
		OIDC: &auth.OIDCSession{AccessToken: "access", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	uri, _, err := runLedgerStorage(context.Background(), projectDir)
	if err == nil || uri != "" || !strings.Contains(err.Error(), "resolving Dream storage") {
		t.Fatalf("Broker resolution = %q, %v; want an error without local fallback", uri, err)
	}
}

func writeDreamTestProject(t *testing.T, projectDir string) string {
	t.Helper()
	const projectID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(`{"project":{"id":"`+projectID+`"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	return projectID
}
