package mcpproxy

import (
	"context"
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

func TestWatchDaemonFiles_PortChange(t *testing.T) {
	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")

	_ = os.WriteFile(portFile, []byte("12345"), 0o644)
	_ = os.WriteFile(keyFile, []byte("key-aaa"), 0o600)

	cfg := Config{
		PortFile: portFile,
		KeyFile:  keyFile,
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	relayCtx, relayCancel := context.WithCancel(ctx)
	defer relayCancel()

	go watchDaemonFiles(relayCtx, cfg, 12345, "key-aaa", relayCancel)

	time.Sleep(2 * watchPollInterval)
	_ = os.WriteFile(portFile, []byte("99999"), 0o644)

	select {
	case <-relayCtx.Done():
	case <-time.After(3 * time.Second):
		t.Fatal("watchDaemonFiles did not cancel relay context after port change")
	}
}

func TestWatchDaemonFiles_KeyChange(t *testing.T) {
	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")

	_ = os.WriteFile(portFile, []byte("12345"), 0o644)
	_ = os.WriteFile(keyFile, []byte("key-aaa"), 0o600)

	cfg := Config{
		PortFile: portFile,
		KeyFile:  keyFile,
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	relayCtx, relayCancel := context.WithCancel(ctx)
	defer relayCancel()

	go watchDaemonFiles(relayCtx, cfg, 12345, "key-aaa", relayCancel)

	time.Sleep(2 * watchPollInterval)
	_ = os.WriteFile(keyFile, []byte("key-bbb"), 0o600)

	select {
	case <-relayCtx.Done():
	case <-time.After(3 * time.Second):
		t.Fatal("watchDaemonFiles did not cancel relay context after key change")
	}
}

func TestWatchDaemonFiles_MissingFiles(t *testing.T) {
	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")

	_ = os.WriteFile(portFile, []byte("12345"), 0o644)
	_ = os.WriteFile(keyFile, []byte("key-aaa"), 0o600)

	cfg := Config{
		PortFile: portFile,
		KeyFile:  keyFile,
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	relayCtx, relayCancel := context.WithCancel(ctx)
	defer relayCancel()

	go watchDaemonFiles(relayCtx, cfg, 12345, "key-aaa", relayCancel)

	time.Sleep(2 * watchPollInterval)
	_ = os.Remove(portFile)
	_ = os.Remove(keyFile)

	select {
	case <-relayCtx.Done():
	case <-time.After(3 * time.Second):
		t.Fatal("watchDaemonFiles did not cancel relay context after files removed")
	}
}

func TestWatchDaemonFiles_NoChange(t *testing.T) {
	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")

	_ = os.WriteFile(portFile, []byte("12345"), 0o644)
	_ = os.WriteFile(keyFile, []byte("key-aaa"), 0o600)

	cfg := Config{
		PortFile: portFile,
		KeyFile:  keyFile,
	}

	relayCtx, relayCancel := context.WithCancel(context.Background())
	defer relayCancel()

	go watchDaemonFiles(relayCtx, cfg, 12345, "key-aaa", relayCancel)

	time.Sleep(4 * watchPollInterval)

	select {
	case <-relayCtx.Done():
		t.Fatal("watchDaemonFiles unexpectedly cancelled relay context with no changes")
	default:
	}
}
