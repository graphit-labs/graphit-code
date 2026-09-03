package wikisvc

import (
	"fmt"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

// TestMain points the global brand directory at a scratch directory for the whole
// package.
//
// It has to be here rather than in each test: every wiki now lives once, in the
// global directory, so a test that seeds one would otherwise write into the
// developer's real store — and read from it, which is worse, because it would pass
// for the wrong reason.
func TestMain(m *testing.M) {
	os.Exit(testenv.Run(m))
}

var seedCounter atomic.Int64

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

func knowledgeWikiDirFor(t *testing.T, projectDir string) string {
	t.Helper()
	return store.KnowledgeProjectDirByID(projectStoreID(t, projectDir))
}

func memoryWikiDirFor(t *testing.T, projectDir string) string {
	t.Helper()
	return store.MemoryWikiDir("project", projectStoreID(t, projectDir))
}
