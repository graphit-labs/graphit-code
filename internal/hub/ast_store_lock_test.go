package hub

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

func TestCleanupSharedASTStoreRespectsBuildLock(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	meta := &LockfileArtifactMeta{ProjectID: "01ACME", Version: "1.0.0"}
	dir := ast.HubContextDir(meta.ProjectID, meta.Version)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	marker := filepath.Join(dir, "schema.cypher")
	if err := os.WriteFile(marker, []byte("mounted"), 0o600); err != nil {
		t.Fatal(err)
	}
	lock, err := lockfile.TryAcquire(sharedASTBuildLockPath(dir))
	if err != nil {
		t.Fatal(err)
	}
	service := &HubService{}
	service.cleanupSharedASTStore(meta)
	if _, err := os.Stat(marker); err != nil {
		t.Fatalf("cleanup removed a store while the builder held its lock: %v", err)
	}
	lock.Release()
	service.cleanupSharedASTStore(meta)
	if _, err := os.Stat(dir); !os.IsNotExist(err) {
		t.Fatalf("cleanup did not remove the store after release: %v", err)
	}
}
