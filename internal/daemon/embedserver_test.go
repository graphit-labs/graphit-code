package daemon

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ---------------------------------------------------------------------------
// EmbedPortFile
// ---------------------------------------------------------------------------

func TestEmbedPortFile(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	got := EmbedPortFile()
	expected := filepath.Join(tempHome, "."+brand.Brand, "daemon", portFileName)
	if got != expected {
		t.Errorf("expected %q, got %q", expected, got)
	}
}

func TestNewEmbedServer_PortFilePath(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Use a nil client since we're only testing the path configuration
	srv := NewEmbedServer(nil)
	expected := filepath.Join(GlobalDaemonDir(), portFileName)
	if srv.portFile != expected {
		t.Errorf("expected portFile %q, got %q", expected, srv.portFile)
	}
}

// ---------------------------------------------------------------------------
// EmbedServerModule — Name
// ---------------------------------------------------------------------------

func TestEmbedServerModule_Name(t *testing.T) {
	mod := NewEmbedServerModule(nil)
	if mod.Name() != "embed-server" {
		t.Errorf("expected 'embed-server', got %q", mod.Name())
	}
}
