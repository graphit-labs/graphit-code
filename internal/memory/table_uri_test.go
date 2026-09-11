package memory

import (
	"context"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// The remote URI must carry the CONFIGURED PREFIX, because the two clients disagree about who
// applies it: s3store.Store.Key prepends it internally, while LanceDB is handed a URI and talks to
// S3 directly. A URI missing the prefix does not fail — it addresses a different prefix and answers
// as an empty store, which is the failure mode this test exists to prevent.
func TestMemoryTableURIRemoteFormCarriesBucketAndPrefix(t *testing.T) {
	seedMemoryAuth(t, "acme-hub", "team-a", "alice")

	got := MemoryTableURI("memory/project/01ARZ3NDEKTSV4RRFFQ69G5FAV", filepath.Join("unused", "local"))
	const want = "s3://acme-hub/team-a/v2/projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/memory"
	if got != want {
		t.Errorf("MemoryTableURI = %q, want %q", got, want)
	}
}

func TestAnonymousUserMemoryIsLocalEvenWhenS3IsConfigured(t *testing.T) {
	seedMemoryAuth(t, "acme-hub", "team-a", "alice")

	local := filepath.Join(t.TempDir(), "memory-user-anonymous")
	if got := MemoryTableURI("memory/user/"+hubaccess.AnonymousUserID, local); got != local {
		t.Fatalf("anonymous MemoryTableURI = %q, want local dir %q", got, local)
	}
	if got := TableURIFor("user", hubaccess.AnonymousUserID); got != store.MemoryTableDir("user", hubaccess.AnonymousUserID) {
		t.Fatalf("anonymous TableURIFor = %q, want local table directory", got)
	}
	if got := hubaccess.UserMemoryPrefix(hubaccess.AnonymousUserID); got != "" {
		t.Fatalf("anonymous S3 memory prefix = %q, want none", got)
	}
}

func TestAnonymousUserMemoryRejectsAHandcraftedS3URI(t *testing.T) {
	seedMemoryAuth(t, "acme-hub", "team-a", "alice")

	err := authorizeMemoryURI(context.Background(), "s3://acme-hub/team-a/v2/users/anonymous/memory")
	if err == nil {
		t.Fatal("anonymous S3 memory URI was authorized")
	}
}

func TestUserScopeIDUsesActiveProfileIndependentlyOfStorage(t *testing.T) {
	t.Run("local provider still isolates the user", func(t *testing.T) {
		seedMemoryAuth(t, "", "", "alice")
		got, err := UserScopeID()
		if err != nil {
			t.Fatal(err)
		}
		if got != "alice" {
			t.Fatalf("UserScopeID = %q, want alice", got)
		}
	})

	t.Run("unauthenticated S3 mode stays anonymous and local", func(t *testing.T) {
		t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
		got, err := UserScopeID()
		if err != nil {
			t.Fatal(err)
		}
		if got != hubaccess.AnonymousUserID {
			t.Fatalf("UserScopeID = %q, want %q", got, hubaccess.AnonymousUserID)
		}
		if uri := TableURIFor("user", got); uri != store.MemoryTableDir("user", hubaccess.AnonymousUserID) {
			t.Fatalf("anonymous table URI = %q, want local table directory", uri)
		}
	})

	t.Run("authenticated S3 mode uses the remote user", func(t *testing.T) {
		seedMemoryAuth(t, "acme-hub", "", "alice")
		got, err := UserScopeID()
		if err != nil {
			t.Fatal(err)
		}
		if got != "alice" {
			t.Fatalf("UserScopeID = %q, want alice", got)
		}
		if uri := TableURIFor("user", got); uri != "s3://acme-hub/v2/users/alice/memory" {
			t.Fatalf("authenticated table URI = %q", uri)
		}
	})
}

// With no bucket the table is local. This is configuration, not a fallback: one store, one schema,
// one code path, and only the URI differs.
func TestMemoryTableURIFallsToTheLocalDirWithNoBucket(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	local := filepath.Join(t.TempDir(), "memory-project-01ABC")
	if got := MemoryTableURI("memory/project/01ABC", local); got != local {
		t.Errorf("MemoryTableURI = %q, want the local dir %q", got, local)
	}
}

func TestMemoryTableURIUsesLocalPathForBrokerWithoutS3(t *testing.T) {
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

	local := filepath.Join(t.TempDir(), "memory-project")
	if got := MemoryTableURI("memory/project/01ARZ3NDEKTSV4RRFFQ69G5FAV", local); got != local {
		t.Fatalf("MemoryTableURI() = %q, want %q", got, local)
	}
}

func TestMemoryTableURIRejectsAnUnqualifiedRemoteScope(t *testing.T) {
	seedMemoryAuth(t, "acme-hub", "", "alice")

	if got := MemoryTableURI("project/01ARZ3NDEKTSV4RRFFQ69G5FAV", "l"); got != "" {
		t.Errorf("unqualified scope produced URI %q", got)
	}
}

func TestAContextResolvesToTheProjectPrefixRemotelyAndADoubledNameLocally(t *testing.T) {
	seedMemoryAuth(t, "acme-hub", "", "alice")

	svc := NewMemoryServiceForContext("01ARZ3NDEKTSV4RRFFQ69G5FAV", nil)
	if got, want := svc.ScopePrefix(), "memory/project/01ARZ3NDEKTSV4RRFFQ69G5FAV"; got != want {
		t.Fatalf("ScopePrefix = %q, want %q", got, want)
	}
	if got, want := MemoryTableURI(svc.ScopePrefix(), "l"), "s3://acme-hub/v2/projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/memory"; got != want {
		t.Errorf("remote URI = %q, want %q", got, want)
	}
	if got := TableDirFor("shared-notes", "shared-notes"); got != store.MemoryTableDir("shared-notes", "shared-notes") {
		t.Errorf("TableDirFor disagreed with the store helper: %q", got)
	}
}

func TestAContextsLocalArtifactsAreNamedFromTheDoubledScope(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	if got, want := filepath.Base(TableDirFor("shared-notes", "shared-notes")),
		"memory-shared-notes-shared-notes"; got != want {
		t.Errorf("table dir segment = %q, want %q", got, want)
	}
}

func seedMemoryAuth(t *testing.T, bucket, prefix, username string) {
	t.Helper()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(auth.Provider{Name: "test", Type: auth.ProviderLocal, Local: &auth.LocalConfig{AllowAWSCredentialChain: true}, S3: auth.S3Config{Bucket: bucket, Prefix: prefix}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "test", Provider: "test", Username: username}); err != nil {
		t.Fatal(err)
	}
}
