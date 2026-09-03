package mcpproxy

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"time"

	"github.com/modelcontextprotocol/go-sdk/jsonrpc"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestToolListChangedNotification(t *testing.T) {
	got, err := jsonrpc.EncodeMessage(toolListChangedNotification())
	if err != nil {
		t.Fatal(err)
	}
	var value struct {
		JSONRPC string         `json:"jsonrpc"`
		Method  string         `json:"method"`
		Params  map[string]any `json:"params"`
	}
	if err := json.Unmarshal(got, &value); err != nil {
		t.Fatal(err)
	}
	if value.JSONRPC != "2.0" || value.Method != "notifications/tools/list_changed" || value.Params == nil {
		t.Fatalf("notification = %s", got)
	}
}

func TestProxyReconnectRefreshesToolCatalog(t *testing.T) {
	first, firstPort := testToolBackend(t, "old_tool", "stable-agent")
	defer first.Close()
	second, secondPort := testToolBackend(t, "new_tool", "stable-agent")
	defer second.Close()

	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")
	if err := os.WriteFile(portFile, []byte(strconv.Itoa(firstPort)), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(keyFile, []byte("test-key"), 0o600); err != nil {
		t.Fatal(err)
	}

	clientSide, proxySide := net.Pipe()
	proxyDone := make(chan error, 1)
	go func() {
		proxyDone <- RunProxy(Config{PortFile: portFile, KeyFile: keyFile, MCPPath: "/", RetryInterval: 10 * time.Millisecond, AgentSessionID: "stable-agent"}, proxySide, proxySide)
	}()

	changed := make(chan struct{}, 1)
	client := mcp.NewClient(&mcp.Implementation{Name: "catalog-refresh-test", Version: "1"}, &mcp.ClientOptions{
		ToolListChangedHandler: func(context.Context, *mcp.ToolListChangedRequest) {
			select {
			case changed <- struct{}{}:
			default:
			}
		},
	})
	session, err := client.Connect(context.Background(), &mcp.IOTransport{Reader: clientSide, Writer: clientSide}, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer func() {
		_ = session.Close()
		select {
		case <-proxyDone:
		case <-time.After(2 * time.Second):
		}
	}()
	assertOnlyTool(t, session, "old_tool")

	if err := os.WriteFile(portFile, []byte(strconv.Itoa(secondPort)), 0o644); err != nil {
		t.Fatal(err)
	}
	select {
	case <-changed:
	case <-time.After(5 * time.Second):
		t.Fatal("client did not receive tools/list_changed after daemon replacement")
	}
	assertOnlyTool(t, session, "new_tool")
}

func testToolBackend(t *testing.T, toolName, wantAgent string) (*httptest.Server, int) {
	t.Helper()
	server := mcp.NewServer(&mcp.Implementation{Name: toolName, Version: "1"}, nil)
	mcp.AddTool(server, &mcp.Tool{Name: toolName}, func(context.Context, *mcp.CallToolRequest, struct{}) (*mcp.CallToolResult, struct{}, error) {
		return &mcp.CallToolResult{}, struct{}{}, nil
	})
	httpServer := httptest.NewServer(mcp.NewStreamableHTTPHandler(func(request *http.Request) *mcp.Server {
		if got := request.Header.Get(AgentSessionHeader); got != wantAgent {
			t.Errorf("agent session header = %q, want %q", got, wantAgent)
		}
		return server
	}, nil))
	parsed, err := url.Parse(httpServer.URL)
	if err != nil {
		httpServer.Close()
		t.Fatal(err)
	}
	_, portText, err := net.SplitHostPort(parsed.Host)
	if err != nil {
		httpServer.Close()
		t.Fatal(err)
	}
	port, err := strconv.Atoi(portText)
	if err != nil {
		httpServer.Close()
		t.Fatal(err)
	}
	return httpServer, port
}

func TestHostAgentSessionIDUsesExplicitEnvironment(t *testing.T) {
	t.Setenv("GRAPHIT_AGENT_SESSION_ID", "host-session")
	if got := hostAgentSessionID(); got != "host-session" {
		t.Fatalf("host agent session = %q", got)
	}
}

func assertOnlyTool(t *testing.T, session *mcp.ClientSession, want string) {
	t.Helper()
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(listed.Tools) != 1 || listed.Tools[0].Name != want {
		t.Fatalf("tools = %#v, want only %s", listed.Tools, want)
	}
}

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

func TestWaitForDaemon_StaleFilesPreserved(t *testing.T) {
	dir := t.TempDir()
	portFile := filepath.Join(dir, "mcp.port")
	keyFile := filepath.Join(dir, "mcp.key")

	_ = os.WriteFile(portFile, []byte("59999"), 0o644)
	_ = os.WriteFile(keyFile, []byte("stale-key"), 0o600)

	ensureCalled := 0
	cfg := Config{
		PortFile:      portFile,
		KeyFile:       keyFile,
		RetryInterval: 10 * time.Millisecond,
		EnsureDaemon: func() {
			ensureCalled++
		},
	}
	cfg.applyDefaults()

	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()

	_, _, err := waitForDaemon(ctx, cfg)
	if err == nil {
		t.Fatal("expected error when daemon is not available")
	}

	if _, err := os.Stat(portFile); os.IsNotExist(err) {
		t.Error("expected port file to be preserved")
	}
	if _, err := os.Stat(keyFile); os.IsNotExist(err) {
		t.Error("expected key file to be preserved")
	}

	if ensureCalled < 2 {
		t.Errorf("expected EnsureDaemon to be called at least 2 times, got %d", ensureCalled)
	}
}

func TestWaitForDaemon_LiveDaemon(t *testing.T) {
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
		RetryInterval: 10 * time.Millisecond,
	}
	cfg.applyDefaults()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	gotPort, gotKey, err := waitForDaemon(ctx, cfg)
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
