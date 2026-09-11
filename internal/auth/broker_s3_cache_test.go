package auth

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type scopedExchangerFunc func(context.Context, Provider, Profile, BrokerStorageScope) (S3Credentials, error)

func (f scopedExchangerFunc) ExchangeForScope(ctx context.Context, provider Provider, profile Profile, scope BrokerStorageScope) (S3Credentials, error) {
	return f(ctx, provider, profile, scope)
}

func brokerSnapshotForCache(profile string, revision uint64) Snapshot {
	return Snapshot{
		Provider: Provider{Name: "broker", Type: ProviderBroker, Revision: revision, Broker: &BrokerConfig{Endpoint: "https://broker.example"}},
		Profile: Profile{Name: profile, Provider: "broker", ProviderRevision: revision, Issuer: "https://broker.example", Subject: profile + "-subject", Username: profile,
			OIDC: &OIDCSession{AccessToken: "access", IDToken: "id", ExpiresAt: time.Now().Add(time.Hour)}},
	}
}

func TestBrokerS3ManagerSingleFlightsAndIsolatesEveryCacheDimension(t *testing.T) {
	now := time.Now().UTC()
	var calls atomic.Int32
	manager := &BrokerS3Manager{Now: func() time.Time { return now }}
	manager.Exchange = scopedExchangerFunc(func(_ context.Context, provider Provider, profile Profile, scope BrokerStorageScope) (S3Credentials, error) {
		calls.Add(1)
		time.Sleep(10 * time.Millisecond)
		identity := fmt.Sprintf("%s-%d-%s-%s-%s", provider.Name, provider.Revision, profile.Name, scope.Kind, scope.ProjectID)
		return S3Credentials{AccessKeyID: identity, SecretAccessKey: "secret", SessionToken: "token", ExpiresAt: now.Add(time.Hour), Bucket: "bucket", Region: "region", Prefixes: []string{"v2"}, AuthorizationRevision: "1"}, nil
	})

	base := brokerSnapshotForCache("alice", 1)
	var wg sync.WaitGroup
	results := make(chan S3Credentials, 16)
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			credentials, err := manager.Resolve(context.Background(), base, ProjectStorageScope("project-a"))
			if err != nil {
				t.Errorf("Resolve: %v", err)
				return
			}
			results <- credentials
		}()
	}
	wg.Wait()
	close(results)
	for credentials := range results {
		if credentials.AccessKeyID != "broker-1-alice-project-project-a" {
			t.Fatalf("shared result = %#v", credentials.RedactedForTest())
		}
	}
	if got := calls.Load(); got != 1 {
		t.Fatalf("same-key exchanges = %d, want 1", got)
	}

	for _, test := range []struct {
		snapshot Snapshot
		scope    BrokerStorageScope
	}{
		{base, ProjectStorageScope("project-b")},
		{brokerSnapshotForCache("bob", 1), ProjectStorageScope("project-a")},
		{brokerSnapshotForCache("alice", 2), ProjectStorageScope("project-a")},
		{base, UserStorageScope()},
		{base, HubStorageScope()},
	} {
		if _, err := manager.Resolve(context.Background(), test.snapshot, test.scope); err != nil {
			t.Fatal(err)
		}
	}
	if got := calls.Load(); got != 6 {
		t.Fatalf("isolated exchanges = %d, want 6", got)
	}
}

func TestBrokerS3ManagerRenewsEarlyWithoutServingExpiredGrant(t *testing.T) {
	now := time.Now().UTC()
	var calls atomic.Int32
	manager := &BrokerS3Manager{Now: func() time.Time { return now }, RefreshBefore: 2 * time.Minute}
	manager.Exchange = scopedExchangerFunc(func(context.Context, Provider, Profile, BrokerStorageScope) (S3Credentials, error) {
		call := calls.Add(1)
		return S3Credentials{AccessKeyID: fmt.Sprintf("key-%d", call), SecretAccessKey: "secret", SessionToken: "token", ExpiresAt: now.Add(10 * time.Minute), Bucket: "bucket", Region: "region", Prefixes: []string{"v2"}, AuthorizationRevision: "1"}, nil
	})
	snapshot := brokerSnapshotForCache("alice", 1)
	first, err := manager.Resolve(context.Background(), snapshot, ProjectStorageScope("project-a"))
	if err != nil {
		t.Fatal(err)
	}
	now = now.Add(9 * time.Minute)
	second, err := manager.Resolve(context.Background(), snapshot, ProjectStorageScope("project-a"))
	if err != nil {
		t.Fatal(err)
	}
	if first.AccessKeyID == second.AccessKeyID || calls.Load() != 2 {
		t.Fatalf("grant was not renewed: first=%q second=%q calls=%d", first.AccessKeyID, second.AccessKeyID, calls.Load())
	}
}

func TestBrokerS3ManagerNeverPersistsResolvedOrRenewedGrant(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := brokerProviderForTest("https://broker.example")
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	profile := Profile{Name: "alice", Provider: provider.Name, ProviderRevision: 1, Username: "alice", Issuer: "https://broker.example", Subject: "subject", OIDC: &OIDCSession{AccessToken: "access", IDToken: "id"}}
	if err := store.Login(profile); err != nil {
		t.Fatal(err)
	}
	snapshot, err := store.Active()
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now().UTC()
	calls := 0
	manager := &BrokerS3Manager{Now: func() time.Time { return now }, Exchange: scopedExchangerFunc(func(_ context.Context, _ Provider, _ Profile, _ BrokerStorageScope) (S3Credentials, error) {
		calls++
		return S3Credentials{AccessKeyID: fmt.Sprintf("MEMORY-ACCESS-%d", calls), SecretAccessKey: "MEMORY-SECRET", SessionToken: "MEMORY-TOKEN", ExpiresAt: now.Add(3 * time.Minute), Bucket: "MEMORY-BUCKET", Region: "region", Endpoint: "https://memory-s3.example", Prefixes: []string{"MEMORY-PREFIX"}, AuthorizationRevision: fmt.Sprint(calls)}, nil
	})}
	if _, err := manager.Resolve(context.Background(), snapshot, ProjectStorageScope("project-a")); err != nil {
		t.Fatal(err)
	}
	now = now.Add(2 * time.Minute)
	if _, err := manager.Resolve(context.Background(), snapshot, ProjectStorageScope("project-a")); err != nil {
		t.Fatal(err)
	}
	if calls != 2 {
		t.Fatalf("credential exchanges=%d, want initial plus renewal", calls)
	}
	raw, err := os.ReadFile(filepath.Join(brand.GlobalDir(), stateFile))
	if err != nil {
		t.Fatal(err)
	}
	for _, forbidden := range []string{"MEMORY-ACCESS", "MEMORY-SECRET", "MEMORY-TOKEN", "MEMORY-BUCKET", "memory-s3.example", "MEMORY-PREFIX"} {
		if strings.Contains(string(raw), forbidden) {
			t.Fatalf("auth.json persisted in-memory Broker value %q: %s", forbidden, raw)
		}
	}
}
