package auth

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"
)

type scopedSTSExchangeFunc func(context.Context, Provider, Profile, BrokerStorageScope) (S3Credentials, error)

func (f scopedSTSExchangeFunc) ExchangeForScope(ctx context.Context, provider Provider, profile Profile, scope BrokerStorageScope) (S3Credentials, error) {
	return f(ctx, provider, profile, scope)
}

func TestSTSS3ManagerCachesOnlyWithinOneScopeAndSession(t *testing.T) {
	now := time.Now().UTC()
	provider := Provider{Name: "oidc", Type: ProviderOIDC, Revision: 1,
		S3: S3Config{Bucket: "artifacts", Prefix: "graphit", CredentialSource: "sts"}, STS: &STSConfig{RoleARN: "role"}}
	base := Snapshot{Provider: provider, Profile: Profile{Name: "alice", Provider: "oidc", Issuer: "https://id.example", Subject: "alice-sub", Username: "alice",
		OIDC: &OIDCSession{AccessToken: "access-a", IDToken: "identity-a"}}}
	var mu sync.Mutex
	calls := map[string]int{}
	manager := &STSS3Manager{Now: func() time.Time { return now }, Exchange: scopedSTSExchangeFunc(func(_ context.Context, _ Provider, profile Profile, scope BrokerStorageScope) (S3Credentials, error) {
		key := profile.Subject + "/" + scope.Kind + "/" + scope.ProjectID
		mu.Lock()
		calls[key]++
		call := calls[key]
		mu.Unlock()
		return S3Credentials{AccessKeyID: fmt.Sprintf("%s-%d", key, call), SecretAccessKey: "secret", SessionToken: "session", ExpiresAt: now.Add(time.Hour), Bucket: "artifacts", Region: "us-east-1", Prefixes: []string{"graphit"}}, nil
	})}
	projectA := ProjectStorageScope("project-a", BrokerStorageModuleTask)
	var wg sync.WaitGroup
	results := make(chan S3Credentials, 8)
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			creds, err := manager.Resolve(context.Background(), base, projectA)
			if err != nil {
				t.Errorf("resolve project A: %v", err)
				return
			}
			results <- creds
		}()
	}
	wg.Wait()
	close(results)
	for creds := range results {
		if creds.AccessKeyID != "alice-sub/project/project-a-1" {
			t.Errorf("concurrent credentials = %q", creds.AccessKeyID)
		}
	}
	for _, scope := range []BrokerStorageScope{ProjectStorageScope("project-b", BrokerStorageModuleTask), UserStorageScope(), HubStorageScope()} {
		if _, err := manager.Resolve(context.Background(), base, scope); err != nil {
			t.Fatal(err)
		}
	}
	other := base
	other.Profile = base.Profile
	other.Profile.OIDC = &OIDCSession{AccessToken: "access-b", IDToken: "identity-b"}
	if _, err := manager.Resolve(context.Background(), other, projectA); err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	if calls["alice-sub/project/project-a"] != 2 || calls["alice-sub/project/project-b"] != 1 || calls["alice-sub/user/"] != 1 || calls["alice-sub/hub/"] != 1 {
		t.Errorf("scope exchanges = %#v", calls)
	}
	mu.Unlock()
	now = now.Add(59 * time.Minute)
	if _, err := manager.Resolve(context.Background(), base, projectA); err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	if calls["alice-sub/project/project-a"] != 3 {
		t.Errorf("near-expiry exchange count = %d", calls["alice-sub/project/project-a"])
	}
	mu.Unlock()
}

func TestSTSS3ManagerRejectsRemoteIDTokenExchange(t *testing.T) {
	snapshot := Snapshot{Provider: Provider{Type: ProviderOIDC, S3: S3Config{Bucket: "artifacts", CredentialSource: "sts"}, STS: &STSConfig{RoleARN: "role"}},
		Profile: Profile{OIDC: &OIDCSession{AccessToken: "caller-access", IDToken: "daemon-identity"}}}
	manager := &STSS3Manager{Exchange: scopedSTSExchangeFunc(func(context.Context, Provider, Profile, BrokerStorageScope) (S3Credentials, error) {
		t.Fatal("remote request must not exchange the daemon's ID token")
		return S3Credentials{}, nil
	})}
	if _, err := manager.Resolve(WithBrokerBearer(context.Background(), "caller-access"), snapshot, ProjectStorageScope("project-a", BrokerStorageModuleTask)); err == nil {
		t.Fatal("remote OIDC STS accepted the daemon's ID token")
	}
}
