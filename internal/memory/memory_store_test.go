package memory

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/store"
)

// An unset bucket is local-only mode, which every caller must keep working in — it is what an
// unset memory.repo used to mean, and it is the mode the framework runs in until setup names a
// bucket.
//
// The store no longer reports whether a bucket is configured, because it no longer holds an S3
// client to answer with: a scope's table reaches object storage through its own URI. What this
// asserts instead is the property that mattered — construction succeeds and yields the local root,
// so every caller keeps working with no bucket at all.
func TestNewMemoryStoreWithoutABucketIsLocalOnlyNotAnError(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")

	st, err := NewMemoryStore()
	if err != nil {
		t.Fatalf("NewMemoryStore without a bucket returned an error: %v", err)
	}
	if st.Dir() != store.MemoryTableRoot() {
		t.Errorf("Dir() = %q, want the global table root %q", st.Dir(), store.MemoryTableRoot())
	}
}

// The branch-to-prefix translation is the identity, so the layout the git branches described is
// preserved exactly. A leading `memory/` must not be doubled.
func TestRemotePrefixMatchesTheBranchLayout(t *testing.T) {
	for _, c := range []struct{ branch, want string }{
		{"memory/project/proj-1", "memory/project/proj-1"},
		{"memory/user/abc123", "memory/user/abc123"},
		{"memory/mycontext/mycontext", "memory/mycontext/mycontext"},
		{"project/proj-1", "memory/project/proj-1"},
	} {
		if got := remotePrefix(c.branch); got != c.want {
			t.Errorf("remotePrefix(%q) = %q, want %q", c.branch, got, c.want)
		}
	}
}
