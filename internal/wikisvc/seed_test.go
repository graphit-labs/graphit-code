package wikisvc

import (
	"fmt"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// TestMain points the global brand directory at a scratch directory for the whole
// package.
//
// It has to be here rather than in each test: every wiki now lives once, in the
// global directory, so a test that seeds one would otherwise write into the
// developer's real store — and read from it, which is worse, because it would pass
// for the wrong reason.
func TestMain(m *testing.M) {
	home, err := os.MkdirTemp("", "graphit-wikisvc-home-")
	if err != nil {
		fmt.Fprintln(os.Stderr, "cannot create a scratch home:", err)
		os.Exit(1)
	}
	_ = os.Setenv("HOME", home)
	_ = os.Setenv("USERPROFILE", home)
	code := m.Run()
	_ = os.RemoveAll(home)
	os.Exit(code)
}

var seedCounter atomic.Int64

// projectStoreID gives a project an identity, because that identity is what its
// stores are keyed by. A directory that already has a lockfile keeps its own id.
func projectStoreID(t *testing.T, projectDir string) string {
	t.Helper()
	if id := store.ProjectID(projectDir); id != "" {
		return id
	}
	id := fmt.Sprintf("01SEED%012d", seedCounter.Add(1))
	if err := os.MkdirAll(projectDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"`+id+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	return id
}

// knowledgeWikiDirFor is where a project's documentation wiki lives — in the global
// store, keyed by the project's id. Nothing is created inside the project.
func knowledgeWikiDirFor(t *testing.T, projectDir string) string {
	t.Helper()
	return store.KnowledgeProjectDirByID(projectStoreID(t, projectDir))
}

// memoryWikiDirFor is the same for a project's memory wiki.
func memoryWikiDirFor(t *testing.T, projectDir string) string {
	t.Helper()
	return store.MemoryWikiDir("project", projectStoreID(t, projectDir))
}
