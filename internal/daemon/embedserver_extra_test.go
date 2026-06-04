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
		sockFile: filepath.Join(blockFile, "sub", "embed.sock"),
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
// EmbedServer.Start — sock file write error
// ---------------------------------------------------------------------------

func TestEmbedServer_Start_SockFileWriteError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Create the sock file directory but make the file path a directory.
	portDir := filepath.Join(tempHome, "portdir")
	_ = os.MkdirAll(portDir, 0o755)

	// Make the sock file path a directory and put a file in it so os.Remove fails and Listen fails.
	portFilePath := filepath.Join(portDir, "embed.sock")
	_ = os.MkdirAll(portFilePath, 0o755)
	_ = os.WriteFile(filepath.Join(portFilePath, "dummy"), []byte("data"), 0o644)

	client := &mockEmbeddingClient{modelName: "test"}
	srv := &EmbedServer{
		client:   client,
		sockFile: portFilePath,
	}

	ctx := context.Background()
	startErr := srv.Start(ctx)
	if startErr == nil {
		t.Error("expected error when sock file write fails")
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
	if srv.sockFile == "" {
		t.Error("sockFile should not be empty")
	}
}
