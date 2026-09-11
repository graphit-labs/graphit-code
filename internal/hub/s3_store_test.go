package hub

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

type brokerS3TestResponse struct {
	auth.S3Credentials
	Scope     string `json:"scope"`
	ProjectID string `json:"project_id,omitempty"`
}

func activateBrokerProvider(t *testing.T, endpoint string) {
	t.Helper()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "team", Type: auth.ProviderBroker, Broker: &auth.BrokerConfig{Endpoint: endpoint}, S3: auth.S3Config{CredentialSource: "broker"}, AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "alice", Provider: provider.Name, Username: "alice", Issuer: endpoint, Subject: "alice", OIDC: &auth.OIDCSession{AccessToken: "broker-key", IDToken: "id-token"}}); err != nil {
		t.Fatal(err)
	}
}

func TestS3StoreDiscoversTemporaryCredentialsFromBroker(t *testing.T) {
	var credentialCalls int
	var scopes []auth.BrokerStorageScope
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "issuer": "http://" + r.Host, "services": map[string]any{"s3_credentials": map[string]any{
				"protocol": "graphit-s3-credentials-v2", "path": "/v1/s3/credentials", "authorization_revision": "acl-1",
			}}})
		case "/v1/s3/credentials":
			credentialCalls++
			var scope auth.BrokerStorageScope
			if err := json.NewDecoder(r.Body).Decode(&scope); err != nil {
				t.Errorf("decode scope: %v", err)
			}
			scopes = append(scopes, scope)
			_ = json.NewEncoder(w).Encode(brokerS3TestResponse{S3Credentials: auth.S3Credentials{AccessKeyID: "A-" + scope.Kind + "-" + scope.ProjectID, SecretAccessKey: "S", SessionToken: "T", ExpiresAt: time.Now().Add(time.Hour), Bucket: "artifacts", Region: "us-east-1", Endpoint: "http://" + r.Host, Prefixes: []string{"v2"}, AuthorizationRevision: "acl-1"}, Scope: scope.Kind, ProjectID: scope.ProjectID})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	activateBrokerProvider(t, server.URL)
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !store.Configured() || store.Bucket() != "artifacts" || credentialCalls != 1 {
		t.Fatalf("configured=%v bucket=%q credentialCalls=%d", store.Configured(), store.Bucket(), credentialCalls)
	}
	if _, err := store.projectStore(context.Background(), testProjectOne); err != nil {
		t.Fatal(err)
	}
	if _, err := store.projectStore(context.Background(), testProjectTwo); err != nil {
		t.Fatal(err)
	}
	if credentialCalls != 3 || len(scopes) != 3 || scopes[0].Kind != "hub" || scopes[1] != auth.ProjectStorageScope(testProjectOne) || scopes[2] != auth.ProjectStorageScope(testProjectTwo) {
		t.Fatalf("scoped credential calls=%d scopes=%#v", credentialCalls, scopes)
	}
}

func TestArtifactPrefixIsProjectScopedForEveryType(t *testing.T) {
	for _, artifactType := range ValidTypes {
		prefix := ArtifactPrefix(artifactType, "shared-id", "branch/feature/api", testProjectOne)
		wantStart := "v2/projects/" + testProjectOne + "/artifacts/" + TypeFolderMap[artifactType] + "/shared-id/"
		if !strings.HasPrefix(prefix, wantStart) {
			t.Fatalf("ArtifactPrefix(%s) = %q", artifactType, prefix)
		}
		if strings.Contains(strings.TrimPrefix(prefix, wantStart), "/") {
			t.Fatalf("version created nested prefix: %q", prefix)
		}
	}
	if got := ArtifactPrefix(TypeSkill, "skill", "1", ""); got != "" {
		t.Fatalf("artifact without project ULID = %q", got)
	}
}

func TestRemoteOperationsInLocalOnlyModeReturnErrNotConfigured(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := store.ReadFile(context.Background(), hubaccess.BaselinesKey()); !errors.Is(err, s3store.ErrNotConfigured) {
		t.Fatalf("ReadFile error = %v", err)
	}
}

func TestS3StoreUsesBrokerHubAccessExclusivelyWhenAdvertised(t *testing.T) {
	var credentialCalls int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "issuer": "http://" + r.Host, "services": map[string]any{
				"s3_credentials": map[string]any{"protocol": "graphit-s3-credentials-v2", "path": "/v1/s3/credentials", "authorization_revision": "9"},
				"hub_access":     map[string]any{"protocol": "graphit-hub-access-v1", "path": "/v1/hub/access/resolve", "authorization_revision": "9"},
			}})
		case "/v1/s3/credentials":
			credentialCalls++
			_ = json.NewEncoder(w).Encode(brokerS3TestResponse{S3Credentials: auth.S3Credentials{AccessKeyID: "A", SecretAccessKey: "S", SessionToken: "T", ExpiresAt: time.Now().Add(time.Hour), Bucket: "artifacts", Region: "us-east-1", Endpoint: "http://" + r.Host, Prefixes: []string{"users/alice"}, AuthorizationRevision: "9"}, Scope: "hub"})
		case "/v1/hub/access/resolve":
			if r.Header.Get("Authorization") != "Bearer broker-key" {
				t.Errorf("authorization=%q", r.Header.Get("Authorization"))
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"v": 1, "authorization_revision": "9", "subject": "local:team|alice", "selectors": []map[string]any{{"id": testProjectOne}}})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	activateBrokerProvider(t, server.URL)
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	grants, subject, revision, err := store.ResolveAccess(context.Background())
	if err != nil || !grants.Allows(testProjectOne, "") {
		t.Fatalf("grants=%#v err=%v", grants, err)
	}
	if subject.UserID == "alice" || !strings.HasPrefix(subject.UserID, "broker-") || revision != "9" {
		t.Fatalf("broker cache scope subject=%#v revision=%q", subject, revision)
	}
	if credentialCalls != 1 {
		t.Fatalf("temporary S3 credentials fetched %d times, want one unexpired grant", credentialCalls)
	}
}

func TestS3StoreBrokerHubAccessFailureDoesNotFallBackToProjectsJSON(t *testing.T) {
	var credentialCalls int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "issuer": "http://" + r.Host, "services": map[string]any{
				"s3_credentials": map[string]any{"protocol": "graphit-s3-credentials-v2", "path": "/v1/s3/credentials", "authorization_revision": "4"},
				"hub_access":     map[string]any{"protocol": "graphit-hub-access-v1", "path": "/v1/hub/access/resolve", "authorization_revision": "4"},
			}})
		case "/v1/s3/credentials":
			credentialCalls++
			_ = json.NewEncoder(w).Encode(brokerS3TestResponse{S3Credentials: auth.S3Credentials{AccessKeyID: "A", SecretAccessKey: "S", SessionToken: "T", ExpiresAt: time.Now().Add(time.Hour), Bucket: "artifacts", Region: "us-east-1", Endpoint: "http://" + r.Host, Prefixes: []string{"users/alice"}, AuthorizationRevision: "4"}, Scope: "hub"})
		case "/v1/hub/access/resolve":
			http.Error(w, "synthetic outage", http.StatusServiceUnavailable)
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	activateBrokerProvider(t, server.URL)
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, _, _, err := store.ResolveAccess(context.Background()); err == nil {
		t.Fatal("broker authorization outage was ignored")
	}
	if credentialCalls != 1 {
		t.Fatalf("broker authorization outage unexpectedly refreshed S3 grant %d times", credentialCalls)
	}
}
