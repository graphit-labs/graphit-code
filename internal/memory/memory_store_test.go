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
