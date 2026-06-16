package mcpproxy

import (
	"net"
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"time"
)

func TestIsPortAlive_ListeningPort(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()

	port := ln.Addr().(*net.TCPAddr).Port
	if !isPortAlive(port) {
		t.Errorf("expected port %d to be alive", port)
	}
}

func TestIsPortAlive_ClosedPort(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := ln.Addr().(*net.TCPAddr).Port
	ln.Close()

	if isPortAlive(port) {
		t.Errorf("expected port %d to be dead after close", port)
	}
}

func TestWaitForDaemon_StaleFilesGetCleaned(t *testing.T) {
	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")

	// Write stale files pointing to a dead port.
	_ = os.WriteFile(portFile, []byte("59999"), 0o644)
	_ = os.WriteFile(keyFile, []byte("stale-key"), 0o600)

	ensureCalled := 0
	cfg := Config{
		PortFile:      portFile,
		KeyFile:       keyFile,
		MaxRetries:    3,
		RetryInterval: 10 * time.Millisecond,
		EnsureDaemon: func() {
			ensureCalled++
		},
	}
	cfg.applyDefaults()

	_, _, err := waitForDaemon(cfg)
	if err == nil {
		t.Fatal("expected error when daemon is not available")
	}

	// Stale files should have been removed.
	if _, err := os.Stat(portFile); !os.IsNotExist(err) {
		t.Error("expected port file to be removed")
	}
	if _, err := os.Stat(keyFile); !os.IsNotExist(err) {
		t.Error("expected key file to be removed")
	}

	// EnsureDaemon should have been called on each retry.
	if ensureCalled < 2 {
		t.Errorf("expected EnsureDaemon to be called at least 2 times, got %d", ensureCalled)
	}
}

func TestWaitForDaemon_LiveDaemon(t *testing.T) {
	// Start a real TCP listener.
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()

	port := ln.Addr().(*net.TCPAddr).Port

	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")

	_ = os.WriteFile(portFile, []byte(strconv.Itoa(port)), 0o644)
	_ = os.WriteFile(keyFile, []byte("real-key"), 0o600)

	cfg := Config{
		PortFile:      portFile,
		KeyFile:       keyFile,
		MaxRetries:    3,
		RetryInterval: 10 * time.Millisecond,
	}
	cfg.applyDefaults()

	gotPort, gotKey, err := waitForDaemon(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if gotPort != port {
		t.Errorf("expected port %d, got %d", port, gotPort)
	}
	if gotKey != "real-key" {
		t.Errorf("expected key 'real-key', got %q", gotKey)
	}
}
