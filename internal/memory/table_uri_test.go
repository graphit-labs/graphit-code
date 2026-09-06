package memory

import (
	"path/filepath"
	"testing"

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
