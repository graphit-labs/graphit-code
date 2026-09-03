package daemon

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestEmbeddingModule_Name(t *testing.T) {
	t.Parallel()
	m := NewEmbeddingModule("/tmp", time.Second, "/cache")
	if m.Name() != "embedding" {
		t.Errorf("expected 'embedding', got %q", m.Name())
	}
}

func TestNewEmbeddingModule_DefaultInterval(t *testing.T) {
	t.Parallel()
	m := NewEmbeddingModule("/tmp", 0, "/cache")
	if m.interval != 2*time.Minute {
		t.Errorf("expected default interval 2m, got %s", m.interval)
	}
}

func TestNewEmbeddingModule_NegativeInterval(t *testing.T) {
	t.Parallel()
	m := NewEmbeddingModule("/tmp", -5*time.Second, "/cache")
	if m.interval != 2*time.Minute {
		t.Errorf("expected default interval 2m for negative input, got %s", m.interval)
	}
}

func TestNewEmbeddingModule_CustomInterval(t *testing.T) {
	t.Parallel()
	m := NewEmbeddingModule("/tmp", 30*time.Second, "/cache")
	if m.interval != 30*time.Second {
		t.Errorf("expected 30s, got %s", m.interval)
	}
}

func TestNewEmbeddingModule_Fields(t *testing.T) {
	t.Parallel()
	m := NewEmbeddingModule("/project", 10*time.Second, "/my/cache")
	if m.rootPath != "/project" {
		t.Errorf("rootPath: expected '/project', got %q", m.rootPath)
	}
	if m.cacheDir != "/my/cache" {
		t.Errorf("cacheDir: expected '/my/cache', got %q", m.cacheDir)
	}
}

func TestDreamModule_Name(t *testing.T) {
	t.Parallel()
	m := NewDreamModule("/tmp", "vscode")
	if m.Name() != "dream" {
		t.Errorf("expected 'dream', got %q", m.Name())
	}
}

func TestNewDreamModule_Fields(t *testing.T) {
	t.Parallel()
	m := NewDreamModule("/my/project", "cursor")
	if m.projectDir != "/my/project" {
		t.Errorf("projectDir: expected '/my/project', got %q", m.projectDir)
	}
	if m.ide != "cursor" {
		t.Errorf("ide: expected 'cursor', got %q", m.ide)
	}
}

func TestEmbeddingModule_Start_ReturnsOnCancel(t *testing.T) {
	tmpDir := t.TempDir()
	cacheDir := filepath.Join(tmpDir, "cache")
	_ = os.MkdirAll(cacheDir, 0o755)

	m := NewEmbeddingModule(tmpDir, 50*time.Millisecond, cacheDir)

	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() {
		errCh <- m.Start(ctx)
	}()

	time.Sleep(100 * time.Millisecond)
	cancel()

	err := <-errCh
	if err != nil && !strings.Contains(err.Error(), "context canceled") {
		t.Logf("Start returned: %v (may be expected)", err)
	}
}
