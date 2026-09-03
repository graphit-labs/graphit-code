package daemon

import (
	"context"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestEmbedServer_Start_ListenError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()

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

func TestEmbedServer_Start_SockFileWriteError(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	portDir := filepath.Join(tempHome, "portdir")
	_ = os.MkdirAll(portDir, 0o755)

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
