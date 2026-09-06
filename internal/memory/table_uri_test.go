package memory

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// The remote URI must carry the CONFIGURED PREFIX, because the two clients disagree about who
// applies it: s3store.Store.Key prepends it internally, while LanceDB is handed a URI and talks to
// S3 directly. A URI missing the prefix does not fail — it addresses a different prefix and answers
// as an empty store, which is the failure mode this test exists to prevent.
func TestMemoryTableURIRemoteFormCarriesBucketAndPrefix(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
	t.Setenv("GRAPHIT_HUB_PREFIX", "team-a")

	got := MemoryTableURI("memory/project/01ARZ3NDEKTSV4RRFFQ69G5FAV", filepath.Join("unused", "local"))
	const want = "s3://acme-hub/team-a/v2/projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/memory"
	if got != want {
		t.Errorf("MemoryTableURI = %q, want %q", got, want)
	}
}

func TestAnonymousUserMemoryIsLocalEvenWhenS3IsConfigured(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
	t.Setenv("GRAPHIT_HUB_PREFIX", "team-a")

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
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
	t.Setenv("GRAPHIT_HUB_PREFIX", "team-a")

	err := authorizeMemoryURI(context.Background(), "s3://acme-hub/team-a/v2/users/anonymous/memory")
	if err == nil {
		t.Fatal("anonymous S3 memory URI was authorized")
	}
}

func TestUserScopeIDUsesAuthenticationOnlyForS3(t *testing.T) {
	t.Run("local mode stays anonymous", func(t *testing.T) {
		t.Setenv("GRAPHIT_HUB_BUCKET", "")
		t.Setenv("GRAPHIT_HUB_PREFIX", "")
		t.Setenv("GRAPHIT_HUB_SUBJECT_USER", "alice")
		got, err := UserScopeID()
		if err != nil {
			t.Fatal(err)
		}
		if got != hubaccess.AnonymousUserID {
			t.Fatalf("UserScopeID = %q, want %q", got, hubaccess.AnonymousUserID)
		}
	})

	t.Run("unauthenticated S3 mode stays anonymous and local", func(t *testing.T) {
		t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
		t.Setenv("GRAPHIT_HUB_PREFIX", "")
		t.Setenv("GRAPHIT_HUB_SUBJECT_USER", "")
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
		t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
		t.Setenv("GRAPHIT_HUB_PREFIX", "")
		t.Setenv("GRAPHIT_HUB_SUBJECT_USER", "alice")
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
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

	local := filepath.Join(t.TempDir(), "memory-project-01ABC")
	if got := MemoryTableURI("memory/project/01ABC", local); got != local {
		t.Errorf("MemoryTableURI = %q, want the local dir %q", got, local)
	}
}

func TestMemoryTableURIRejectsAnUnqualifiedRemoteScope(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

	if got := MemoryTableURI("project/01ARZ3NDEKTSV4RRFFQ69G5FAV", "l"); got != "" {
		t.Errorf("unqualified scope produced URI %q", got)
	}
}

func TestAContextResolvesToTheProjectPrefixRemotelyAndADoubledNameLocally(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

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
	t.Setenv("GRAPHIT_HUB_BUCKET", "")

	if got, want := filepath.Base(TableDirFor("shared-notes", "shared-notes")),
		"memory-shared-notes-shared-notes"; got != want {
		t.Errorf("table dir segment = %q, want %q", got, want)
	}
	if got, want := MemoryWikiGlobalDir("shared-notes", "shared-notes"),
		store.MemoryWikiDir("shared-notes", "shared-notes"); got != want {
		t.Errorf("wiki dir = %q, want %q", got, want)
	}
}
