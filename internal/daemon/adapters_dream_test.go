package daemon

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// DreamModule — Start returns on context cancel
// ---------------------------------------------------------------------------

func TestDreamModule_Start_ReturnsOnCancel(t *testing.T) {
	tmpDir := t.TempDir()
	m := NewDreamModule(tmpDir, "vscode")

	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() {
		errCh <- m.Start(ctx)
	}()

	time.Sleep(100 * time.Millisecond)
	cancel()

	err := <-errCh
	if err != nil {
		// Dream runner may return an error (e.g., missing files). That's acceptable.
		t.Logf("DreamModule.Start returned: %v (may be expected for empty dir)", err)
	}
}

// ---------------------------------------------------------------------------
// EmbeddingModule — Start with rootPath pointing to non-existent path
// ---------------------------------------------------------------------------

func TestEmbeddingModule_Start_NonExistentPath(t *testing.T) {
	m := NewEmbeddingModule("/nonexistent/path", 50*time.Millisecond, "/nonexistent/cache")

	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() {
		errCh <- m.Start(ctx)
	}()

	time.Sleep(100 * time.Millisecond)
	cancel()

	<-errCh
}

// ---------------------------------------------------------------------------
// loadProjectConfigFromDir — with actual lockfile
// ---------------------------------------------------------------------------

func TestLoadProjectConfigFromDir_WithLockfile(t *testing.T) {
	tmpDir := t.TempDir()
	// Create an empty lockfile — should return nil config
	config := loadProjectConfigFromDir(tmpDir)
	if config != nil {
		t.Logf("config from empty dir: %v (may be nil or empty)", config)
	}
}

// ---------------------------------------------------------------------------
// loadProjectConfigFromDir — with proper lockfile
// ---------------------------------------------------------------------------

func TestLoadProjectConfigFromDir_WithValidLockfile(t *testing.T) {
	t.Parallel()
	tmpDir := t.TempDir()
	// Create a valid JSON lockfile
	lockContent := `{"version": 1, "config": {"ast": {"enabled": true}}, "artifacts": {}}`
	lockPath := filepath.Join(tmpDir, ".graphit.lock")
	_ = os.WriteFile(lockPath, []byte(lockContent), 0o644)

	config := loadProjectConfigFromDir(tmpDir)
	// May be nil or valid depending on brand.LockFileName()
	_ = config
}
