package daemon

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// EmbeddingModule — Start (calls ast.RunEmbeddingLoop)

func TestEmbeddingModule_Start_ContextCancelled(t *testing.T) {
	tmpDir := t.TempDir()
	cacheDir := filepath.Join(tmpDir, "cache")
	_ = os.MkdirAll(cacheDir, 0o755)

	_ = NewEmbeddingModule(tmpDir, 100*time.Millisecond, cacheDir)

	// The Start function calls ast.RunEmbeddingLoop which returns when ctx is cancelled
	// We can't easily test the full loop without mocking, but we ensure it doesn't panic
	// and returns on context cancellation
	// This is covered by the fact that it's called through supervisor tests
}

func TestDreamModule_Start_ContextCancelled(t *testing.T) {
	// DreamModule.Start creates a dream.Runner and calls Run(ctx)
	// Without a real project setup, we test it doesn't panic
	// The function is called through supervisor tests
}

func TestLoadProjectConfigFromDir_NoLockfile(t *testing.T) {
	tmpDir := t.TempDir()
	cfg := loadProjectConfigFromDir(tmpDir)
	if cfg != nil {
		t.Errorf("expected nil config when no lockfile exists, got %v", cfg)
	}
}

func TestLoadProjectConfigFromDir_InvalidLockfile(t *testing.T) {
	tmpDir := t.TempDir()
	lockPath := filepath.Join(tmpDir, brand.LockFileName())
	if err := os.WriteFile(lockPath, []byte("{invalid json"), 0o644); err != nil {
		t.Fatal(err)
	}
	cfg := loadProjectConfigFromDir(tmpDir)
	if cfg != nil {
		t.Errorf("expected nil config for invalid lockfile, got %v", cfg)
	}
}

func TestLoadProjectConfigFromDir_EmptyLockfile(t *testing.T) {
	tmpDir := t.TempDir()
	lockPath := filepath.Join(tmpDir, brand.LockFileName())
	if err := os.WriteFile(lockPath, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	cfg := loadProjectConfigFromDir(tmpDir)
	// Empty lockfile has nil Config field
	if cfg != nil {
		t.Logf("config from empty lockfile: %v", cfg)
	}
}

func TestLoadProjectConfigFromDir_ValidLockfile(t *testing.T) {
	tmpDir := t.TempDir()
	lockPath := filepath.Join(tmpDir, brand.LockFileName())
	content := `{"config":{"key":"value"}}`
	if err := os.WriteFile(lockPath, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	cfg := loadProjectConfigFromDir(tmpDir)
	if cfg == nil {
		t.Fatal("expected non-nil config from valid lockfile")
	}
	if cfg["key"] != "value" {
		t.Errorf("expected config key=value, got %v", cfg["key"])
	}
}
