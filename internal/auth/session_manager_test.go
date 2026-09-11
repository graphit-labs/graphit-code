package auth

import (
	"context"
	"crypto/rand"
	"crypto/rsa"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

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

func TestSessionManagerSerializesOIDCSTSRefresh(t *testing.T) {
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
	var exchanges atomic.Int32
	manager := &SessionManager{
		Store: store, RefreshBefore: time.Minute,
		STS: exchangerFunc(func(_ context.Context, _ Provider, profile Profile) (S3Credentials, error) {
			exchanges.Add(1)
			if profile.OIDC == nil || profile.OIDC.IDToken != "id-token" {
				t.Fatal("STS exchange did not receive the current OIDC session")
			}
			return S3Credentials{
				AccessKeyID: "new", SecretAccessKey: "new", SessionToken: "new-token",
				ExpiresAt: time.Now().Add(time.Hour), Bucket: "artifacts", Region: "us-east-1", Prefixes: []string{"graphit"},
			}, nil
		}),
	}
	var wg sync.WaitGroup
	errs := make(chan error, 8)
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			snapshot, err := manager.Active(context.Background())
			if err == nil && snapshot.Profile.S3.AccessKeyID != "new" {
				err = fmt.Errorf("active S3 access key = %q", snapshot.Profile.S3.AccessKeyID)
			}
			errs <- err
		}()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatal(err)
		}
	}
	if got := exchanges.Load(); got != 1 {
		t.Fatalf("STS exchanges = %d, want 1", got)
	}
}
