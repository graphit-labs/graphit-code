package memory

import (
	"path/filepath"
	"strings"
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

	got := MemoryTableURI("memory/project/01ABC", filepath.Join("unused", "local"))
	const want = "s3://acme-hub/team-a/memory/project/01ABC"
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

// A scope path is normalised by remotePrefix and by nothing else, so a caller that omits the
// leading `memory/` addresses the SAME table. Two normalisation rules would put two tables where
// there is one scope.
func TestMemoryTableURINormalisesTheScopePathLikeTheObjectStore(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

	withPrefix := MemoryTableURI("memory/project/01ABC", "l")
	withoutPrefix := MemoryTableURI("project/01ABC", "l")
	if withPrefix != withoutPrefix {
		t.Errorf("the same scope produced two URIs:\n  %q\n  %q", withPrefix, withoutPrefix)
	}
	if !strings.HasSuffix(withPrefix, "/memory/project/01ABC") {
		t.Errorf("URI = %q, want it to end in /memory/project/01ABC", withPrefix)
	}
}

// An imported context's REMOTE location is another project's prefix — `memory/project/<name>` —
// while its LOCAL directory is named from the doubled scope. Both facts are load-bearing, and a
// single helper that assumed the identity mapping would break one of them.
func TestAContextResolvesToTheProjectPrefixRemotelyAndADoubledNameLocally(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "acme-hub")
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

	svc := NewMemoryServiceForContext("shared-notes", nil)
	if got, want := svc.ScopePrefix(), "memory/project/shared-notes"; got != want {
		t.Fatalf("ScopePrefix = %q, want %q", got, want)
	}
	if got, want := MemoryTableURI(svc.ScopePrefix(), "l"), "s3://acme-hub/memory/project/shared-notes"; got != want {
		t.Errorf("remote URI = %q, want %q", got, want)
	}
	if got := TableDirFor("shared-notes", "shared-notes"); got != store.MemoryTableDir("shared-notes", "shared-notes") {
		t.Errorf("TableDirFor disagreed with the store helper: %q", got)
	}
}

// A context's two local artifacts are named from the doubled pair, so the table directory and the
// wiki directory agree about which scope they belong to. The check used to be against the raw
// markdown store's segment, which is the pair that no longer exists.
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
