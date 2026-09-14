package auth

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/rsa"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestResolveActiveUsesInboundBrokerBearerWithoutRefreshingDaemonLogin(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := Provider{Name: "broker", Type: ProviderBroker, Broker: &BrokerConfig{Endpoint: "https://broker.invalid"},
		AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "stored", Provider: "broker", Issuer: "https://broker.invalid", Subject: "stored-subject", Username: "stored",
		OIDC: &OIDCSession{AccessToken: "expired-daemon-token", RefreshToken: "revoked-refresh", IDToken: "old-id-token", ExpiresAt: time.Now().Add(-time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	for _, token := range []string{"caller-one", "caller-two"} {
		snapshot, err := ResolveActive(WithBrokerBearer(context.Background(), token))
		if err != nil || snapshot.Profile.OIDC.AccessToken != token || snapshot.Profile.OIDC.IDToken != "" {
			t.Fatalf("request snapshot for %q: %#v err=%v", token, snapshot.Profile.OIDC, err)
		}
	}
	persisted, err := store.Active()
	if err != nil || persisted.Profile.OIDC.AccessToken != "expired-daemon-token" {
		t.Fatalf("daemon profile changed: %#v err=%v", persisted.Profile.OIDC, err)
	}
}

func TestSessionManagerRefreshesAndPublishesBrokerOIDCProfile(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now().UTC()
	var broker *httptest.Server
	broker = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			writeJSON(t, w, brokerDiscoveryForTest(broker.URL))
		case "/.well-known/openid-configuration":
			writeJSON(t, w, oidcDiscoveryForBrokerTest(broker.URL))
		case "/jwks":
			writeJSON(t, w, rsaJWKSForBrokerTest(key))
		case "/token":
			if err := r.ParseForm(); err != nil || r.Form.Get("grant_type") != "refresh_token" || r.Form.Get("refresh_token") != "refresh-1" {
				http.Error(w, "invalid refresh", http.StatusBadRequest)
				return
			}
			writeStandardBrokerToken(t, w, key, map[string]any{"iss": broker.URL, "sub": "gb_sub_1", "aud": "graphit-cli",
				"exp": now.Add(time.Hour).Unix(), "preferred_username": "alice", "organization": "acme", "groups": []string{"platform"}}, "access-2", "refresh-2")
		default:
			http.NotFound(w, r)
		}
	}))
	defer broker.Close()

	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	provider := brokerProviderForTest(broker.URL)
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	profile := Profile{Name: "alice", Provider: "company", ProviderRevision: 1, Issuer: broker.URL, Subject: "gb_sub_1", Username: "alice",
		OIDC: &OIDCSession{AccessToken: "access-1", RefreshToken: "refresh-1", IDToken: "previous-id-token", TokenType: "Bearer", ExpiresAt: time.Now().Add(10 * time.Second)}}
	if err := store.Login(profile); err != nil {
		t.Fatal(err)
	}
	manager := &SessionManager{Store: store, OIDC: &OIDCClient{HTTP: broker.Client(), Now: func() time.Time { return now }}, RefreshBefore: time.Minute}
	snapshot, err := manager.Active(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Profile.OIDC.AccessToken != "access-2" || snapshot.Profile.OIDC.RefreshToken != "refresh-2" {
		t.Fatalf("active profile was not refreshed: %#v", snapshot.Profile.OIDC)
	}
	if !snapshot.Profile.S3.Empty() {
		t.Fatalf("Broker S3 credentials entered the persisted profile: %#v", snapshot.Profile.S3.RedactedForTest())
	}
	persisted, err := store.Active()
	if err != nil || persisted.Profile.OIDC.AccessToken != "access-2" || persisted.Profile.OIDC.RefreshToken != "refresh-2" {
		t.Fatalf("refreshed profile was not atomically published: %#v err=%v", persisted.Profile.OIDC, err)
	}
}

func TestSessionManagerRefreshesBrokerOIDCWithoutEnablingS3(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now().UTC()
	var broker *httptest.Server
	broker = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			writeJSON(t, w, brokerDiscoveryForTest(broker.URL))
		case "/.well-known/openid-configuration":
			writeJSON(t, w, oidcDiscoveryForBrokerTest(broker.URL))
		case "/jwks":
			writeJSON(t, w, rsaJWKSForBrokerTest(key))
		case "/token":
			if err := r.ParseForm(); err != nil || r.Form.Get("grant_type") != "refresh_token" || r.Form.Get("refresh_token") != "refresh-1" {
				http.Error(w, "invalid refresh", http.StatusBadRequest)
				return
			}
			writeStandardBrokerToken(t, w, key, map[string]any{"iss": broker.URL, "sub": "gb_sub_1", "aud": "graphit-cli",
				"exp": now.Add(time.Hour).Unix(), "preferred_username": "alice"}, "access-2", "refresh-2")
		default:
			http.NotFound(w, r)
		}
	}))
	defer broker.Close()

	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	if err := store.AddProvider(brokerProviderForTest(broker.URL)); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "alice", Provider: "company", ProviderRevision: 1, Issuer: broker.URL, Subject: "gb_sub_1", Username: "alice",
		OIDC: &OIDCSession{AccessToken: "access-1", RefreshToken: "refresh-1", IDToken: "previous-id-token", TokenType: "Bearer", ExpiresAt: time.Now().Add(10 * time.Second)}, BrokerS3Disabled: true}); err != nil {
		t.Fatal(err)
	}
	manager := &SessionManager{Store: store, OIDC: &OIDCClient{HTTP: broker.Client(), Now: func() time.Time { return now }}, RefreshBefore: time.Minute}
	snapshot, err := manager.Active(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Profile.OIDC.AccessToken != "access-2" || snapshot.Profile.OIDC.RefreshToken != "refresh-2" {
		t.Fatalf("OIDC session was not refreshed: %#v", snapshot.Profile.OIDC)
	}
	if !snapshot.Profile.S3.Empty() {
		t.Fatalf("S3 was enabled unexpectedly: %#v", snapshot.Profile.S3.RedactedForTest())
	}
	if !snapshot.Profile.BrokerS3Disabled {
		t.Fatal("Broker S3 disabled state was not preserved during OIDC refresh")
	}
}

func TestBrokerLoginDropsS3MaterialBeforePersistence(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	provider := brokerProviderForTest("https://broker.example")
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	original := S3Credentials{AccessKeyID: "old", SecretAccessKey: "old", SessionToken: "old", ExpiresAt: time.Now().Add(time.Second),
		Bucket: "artifacts", Region: "us-east-1", Prefixes: []string{"users/alice"}, AuthorizationRevision: "acl-1"}
	if err := store.Login(Profile{Name: "alice", Provider: "company", ProviderRevision: 1, Issuer: provider.Broker.Endpoint, Subject: "user-1", Username: "alice",
		OIDC: &OIDCSession{AccessToken: "access", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)}, S3: original}); err != nil {
		t.Fatal(err)
	}
	persisted, err := store.Active()
	if err != nil {
		t.Fatal(err)
	}
	if !persisted.Profile.S3.Empty() {
		t.Fatalf("Broker S3 grant was persisted: %#v", persisted.Profile.S3.RedactedForTest())
	}
}

func TestSessionManagerLeavesOIDCSTSUnexchangedUntilStorageScopeIsKnown(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	provider := Provider{
		Name: "oidc", Type: ProviderOIDC,
		OIDC: &OIDCConfig{Issuer: "https://id.example", ClientID: "graphit", UsernameClaim: "preferred_username"},
		S3:   S3Config{Bucket: "artifacts", Region: "us-east-1", Prefix: "graphit", CredentialSource: "sts"},
		STS:  &STSConfig{RoleARN: "arn:aws:iam::123456789012:role/graphit"},
	}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{
		Name: "alice", Provider: "oidc", ProviderRevision: 1, Issuer: "https://id.example", Subject: "subject", Username: "alice",
		OIDC: &OIDCSession{AccessToken: "access-token", IDToken: "id-token", ExpiresAt: time.Now().Add(time.Hour)},
		S3:   S3Credentials{AccessKeyID: "old", SecretAccessKey: "old", SessionToken: "old", ExpiresAt: time.Now().Add(time.Second)},
	}); err != nil {
		t.Fatal(err)
	}
	manager := &SessionManager{Store: store, RefreshBefore: time.Minute}
	snapshot, err := manager.Active(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !snapshot.Profile.S3.Empty() {
		t.Fatalf("OIDC STS credentials appeared without a storage scope: %#v", snapshot.Profile.S3.RedactedForTest())
	}
}

func TestOIDCSTSAuthStateMigratesPersistedCredentialsAway(t *testing.T) {
	path := filepath.Join(t.TempDir(), "auth.json")
	store := OpenAt(path)
	provider := Provider{Name: "oidc", Type: ProviderOIDC,
		OIDC: &OIDCConfig{Issuer: "https://id.example", ClientID: "graphit", UsernameClaim: "preferred_username"},
		S3:   S3Config{Bucket: "artifacts", Region: "us-east-1", CredentialSource: "sts"}, STS: &STSConfig{RoleARN: "role"}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "alice", Provider: "oidc", Issuer: "https://id.example", Subject: "subject", Username: "alice",
		OIDC: &OIDCSession{AccessToken: "access", IDToken: "identity", ExpiresAt: time.Now().Add(time.Hour)},
		S3:   S3Credentials{AccessKeyID: "should-not-save", SecretAccessKey: "should-not-save", SessionToken: "should-not-save"}}); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) == "" || !storeS3Absent(data) {
		t.Fatal("OIDC STS login persisted temporary credentials")
	}
	// Simulate an auth.json produced by the older version. Load must remove its
	// STS grant from disk atomically while retaining the OIDC login session.
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	profile := state.Profiles["alice"]
	profile.S3 = S3Credentials{AccessKeyID: "legacy-access", SecretAccessKey: "legacy-secret", SessionToken: "legacy-session", ExpiresAt: time.Now().Add(time.Hour)}
	state.Profiles["alice"] = profile
	legacy, err := json.Marshal(state)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, legacy, 0o600); err != nil {
		t.Fatal(err)
	}
	loaded, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if !loaded.Profiles["alice"].S3.Empty() || loaded.Profiles["alice"].OIDC.AccessToken != "access" {
		t.Fatalf("migration changed the wrong fields: %#v", loaded.Profiles["alice"].S3.RedactedForTest())
	}
	data, err = os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !storeS3Absent(data) {
		t.Fatal("legacy temporary credentials remain on disk")
	}
}

func TestResolveActiveBindsRemoteOIDCToVerifiedCaller(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := Provider{Name: "oidc", Type: ProviderOIDC,
		OIDC: &OIDCConfig{Issuer: "https://id.example", ClientID: "graphit", UsernameClaim: "sub"},
		S3:   S3Config{Bucket: "artifacts", CredentialSource: "sts"}, STS: &STSConfig{RoleARN: "role", UseAccessToken: true}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "daemon", Provider: "oidc", Issuer: "https://id.example", Subject: "daemon-sub", Username: "daemon",
		OIDC: &OIDCSession{AccessToken: "daemon-access", IDToken: "daemon-identity", RefreshToken: "revoked-refresh", ExpiresAt: time.Now().Add(-time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	identity := VerifiedIdentity{Issuer: "https://id.example", Subject: "caller-sub", Username: "caller", Teams: []string{"dev"}}
	ctx := WithRequestIdentity(WithBrokerBearer(context.Background(), "caller-access"), identity)
	snapshot, err := ResolveActive(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Profile.Username != "caller" || snapshot.Profile.Subject != "caller-sub" || snapshot.Profile.OIDC.AccessToken != "caller-access" ||
		snapshot.Profile.OIDC.IDToken != "" || snapshot.Profile.OIDC.RefreshToken != "" || !snapshot.Profile.S3.Empty() {
		t.Fatalf("remote OIDC request inherited daemon credentials: %#v", snapshot.Profile.S3.RedactedForTest())
	}
	stored, err := store.Active()
	if err != nil {
		t.Fatal(err)
	}
	if stored.Profile.Username != "daemon" || stored.Profile.OIDC.AccessToken != "daemon-access" {
		t.Fatal("request changed the daemon's persisted login")
	}
	if _, err := ResolveActive(WithBrokerBearer(context.Background(), "unverified")); err == nil {
		t.Fatal("OIDC bearer without verified identity was accepted")
	}
}

func storeS3Absent(data []byte) bool {
	var state struct {
		Profiles map[string]map[string]json.RawMessage `json:"profiles"`
	}
	if json.Unmarshal(data, &state) != nil {
		return false
	}
	_, hasS3 := state.Profiles["alice"]["s3"]
	return !hasS3 && !bytes.Contains(data, []byte("legacy-access")) && !bytes.Contains(data, []byte("legacy-secret")) &&
		!bytes.Contains(data, []byte("legacy-session")) && !bytes.Contains(data, []byte("should-not-save"))
}
