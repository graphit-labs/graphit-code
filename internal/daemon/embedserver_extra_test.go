package daemon

import (
	"context"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// EmbedServer.Start — listen error (port already in use)
// ---------------------------------------------------------------------------

func TestEmbedServer_Start_ListenError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Occupy a port to force listen error — we bind to a specific port.
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()

	// The EmbedServer binds to 127.0.0.1:0 which picks a random port,
	// so we can't directly force a listen error this way.
	// Instead, test the port file directory creation error path.

	// Block port file directory creation
	blockFile := filepath.Join(tempHome, "blockfile")
	_ = os.WriteFile(blockFile, []byte("x"), 0o600)

	client := &mockEmbeddingClient{modelName: "test"}
	srv := &EmbedServer{
		client:   client,
		portFile: filepath.Join(blockFile, "sub", "embed.port"),
	}

	ctx := context.Background()
	startErr := srv.Start(ctx)
	if startErr == nil {
		t.Error("expected error when port file dir creation fails")
	}
	if !strings.Contains(startErr.Error(), "creating dir") {
		t.Errorf("expected 'creating dir' in error, got %v", startErr)
	}
}

// ---------------------------------------------------------------------------
// EmbedServer.Start — port file write error
// ---------------------------------------------------------------------------

func TestEmbedServer_Start_PortFileWriteError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Create the port file directory but make the file path a directory.
	portDir := filepath.Join(tempHome, "portdir")
	_ = os.MkdirAll(portDir, 0o755)

	// Make the port file path a directory so WriteFile fails.
	portFilePath := filepath.Join(portDir, "embed.port")
	_ = os.MkdirAll(portFilePath, 0o755)

	client := &mockEmbeddingClient{modelName: "test"}
	srv := &EmbedServer{
		client:   client,
		portFile: portFilePath,
	}

	ctx := context.Background()
	startErr := srv.Start(ctx)
	if startErr == nil {
		t.Error("expected error when port file write fails")
	}
	if !strings.Contains(startErr.Error(), "writing port file") {
		t.Errorf("expected 'writing port file' in error, got %v", startErr)
	}
}

// ---------------------------------------------------------------------------
// NewEmbedServer — sets default fields
// ---------------------------------------------------------------------------

func TestNewEmbedServer_Fields(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	client := &mockEmbeddingClient{modelName: "my-model"}
	srv := NewEmbedServer(client)
	if srv.client != client {
		t.Error("client field mismatch")
	}
	if srv.portFile == "" {
		t.Error("portFile should not be empty")
	}
}

// ---------------------------------------------------------------------------
// NewEmbedServerModule — fields
// ---------------------------------------------------------------------------

func TestNewEmbedServerModule_Fields(t *testing.T) {
	client := &mockEmbeddingClient{modelName: "test"}
	mod := NewEmbedServerModule(client)
	if mod.client != client {
		t.Error("client field mismatch")
	}
}
