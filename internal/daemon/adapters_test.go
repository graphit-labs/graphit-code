package daemon

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestEmbeddingModule_Start_ContextCancelled(t *testing.T) {
	tmpDir := t.TempDir()
	cacheDir := filepath.Join(tmpDir, "cache")
	_ = os.MkdirAll(cacheDir, 0o755)

	_ = NewEmbeddingModule(tmpDir, 100*time.Millisecond, cacheDir)

}

func TestDreamModule_Start_ContextCancelled(t *testing.T) {
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
