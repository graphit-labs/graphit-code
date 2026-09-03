package memory

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/store"
)

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
