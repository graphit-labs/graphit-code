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

// The table directory and the raw directory of one scope must be named from the same pair, or a
// scope owns two differently-named directories and nothing notices.
func TestTableDirAndRawDirShareTheScopeSegment(t *testing.T) {
	rawBase := filepath.Base(RawDirFor("project", "01ABC"))
	tableBase := filepath.Base(TableDirFor("project", "01ABC"))
	if rawBase != tableBase {
		t.Errorf("raw dir segment %q != table dir segment %q", rawBase, tableBase)
	}
	if filepath.Dir(RawDirFor("project", "01ABC")) == filepath.Dir(TableDirFor("project", "01ABC")) {
		t.Error("the raw store and the table share a parent directory; they must be separate roots")
	}
}
