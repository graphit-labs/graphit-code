// Package mcpproxy relays MCP JSON-RPC messages between stdio and the
// daemon's HTTP endpoint using the official MCP go-sdk transports.
package mcpproxy

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/jsonrpc"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

const watchPollInterval = 500 * time.Millisecond

type Config struct {
	PortFile      string
	KeyFile       string
	MCPPath       string // default "/mcp"
	EnsureDaemon  func()
	RetryInterval time.Duration // default 500ms
	Stderr        io.Writer
}

func (c *Config) applyDefaults() {
	if c.MCPPath == "" {
		c.MCPPath = "/mcp"
	}
	if c.RetryInterval <= 0 {
		c.RetryInterval = 500 * time.Millisecond
	}
}

func (c *Config) logf(format string, args ...any) {
	if c.Stderr != nil {
		fmt.Fprintf(c.Stderr, "[mcp-proxy] "+format+"\n", args...)
	}
}

func GenerateAPIKey() (string, error) {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return "", fmt.Errorf("generate API key: %w", err)
	}
	return hex.EncodeToString(b), nil
}

func ReadPort(portFile string) (int, error) {
	data, err := os.ReadFile(portFile)
	if err != nil {
		return 0, err
	}
	return strconv.Atoi(strings.TrimSpace(string(data)))
}

func ReadKey(keyFile string) (string, error) {
	data, err := os.ReadFile(keyFile)
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(data)), nil
}

func RunProxy(cfg Config, stdin io.ReadCloser, stdout io.WriteCloser) error {
	cfg.applyDefaults()
	ctx := context.Background()

	if cfg.EnsureDaemon != nil {
		cfg.EnsureDaemon()
	}
	stdioTransport := &mcp.IOTransport{Reader: stdin, Writer: stdout}
	stdioConn, err := stdioTransport.Connect(ctx)
	if err != nil {
		return fmt.Errorf("stdio transport connect: %w", err)
	}
	defer stdioConn.Close()

	var initReq, initNotif jsonrpc.Message
	firstConnect := true

	for {
		port, key, err := waitForDaemon(ctx, cfg)
		if err != nil {
			return err
		}

		endpoint := fmt.Sprintf("http://127.0.0.1:%d%s", port, cfg.MCPPath)
		cfg.logf("connecting to daemon at %s", endpoint)

		httpConn, err := connectHTTP(ctx, endpoint, key)
		if err != nil {
			cfg.logf("HTTP connect failed: %v, retrying…", err)
			if cfg.EnsureDaemon != nil {
				cfg.EnsureDaemon()
			}
			time.Sleep(cfg.RetryInterval)
			continue
		}
		cfg.logf("connected")

		if firstConnect {
			initReq, err = stdioConn.Read(ctx)
			if err != nil {
				httpConn.Close()
				return fmt.Errorf("reading initialize request: %w", err)
			}
			if err := httpConn.Write(ctx, initReq); err != nil {
				httpConn.Close()
				return fmt.Errorf("forwarding initialize request: %w", err)
			}
			resp, err := httpConn.Read(ctx)
			if err != nil {
				httpConn.Close()
				return fmt.Errorf("reading initialize response: %w", err)
			}
			if err := stdioConn.Write(ctx, resp); err != nil {
				httpConn.Close()
				return fmt.Errorf("forwarding initialize response: %w", err)
			}
			initNotif, err = stdioConn.Read(ctx)
			if err != nil {
				httpConn.Close()
				return fmt.Errorf("reading initialized notification: %w", err)
			}
			if err := httpConn.Write(ctx, initNotif); err != nil {
				httpConn.Close()
				return fmt.Errorf("forwarding initialized notification: %w", err)
			}
			firstConnect = false
		} else {
			cfg.logf("replaying MCP initialize handshake for reconnected session")
			if err := httpConn.Write(ctx, initReq); err != nil {
				cfg.logf("replay initialize failed: %v", err)
				httpConn.Close()
				time.Sleep(cfg.RetryInterval)
				continue
			}
			if _, err := httpConn.Read(ctx); err != nil {
				cfg.logf("replay initialize response failed: %v", err)
				httpConn.Close()
				time.Sleep(cfg.RetryInterval)
				continue
			}
			if err := httpConn.Write(ctx, initNotif); err != nil {
				cfg.logf("replay initialized notification failed: %v", err)
				httpConn.Close()
				time.Sleep(cfg.RetryInterval)
				continue
			}
		}

		relayCtx, cancelRelay := context.WithCancel(ctx)
		go watchDaemonFiles(relayCtx, cfg, port, key, cancelRelay)

		err = relay(relayCtx, stdioConn, httpConn)
		cancelRelay()
		httpConn.Close()

		if isStdioClosed(err) {
			return nil
		}

		if isDaemonRestarted(err) {
			cfg.logf("daemon restarted (port/key changed), reconnecting...")
		} else {
			cfg.logf("connection lost: %v, reconnecting...", err)
		}
		time.Sleep(cfg.RetryInterval)

		if cfg.EnsureDaemon != nil {
			cfg.EnsureDaemon()
		}
	}
}

func watchDaemonFiles(ctx context.Context, cfg Config, port int, key string, relayCancel context.CancelFunc) {
	ticker := time.NewTicker(watchPollInterval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			newPort, perr := ReadPort(cfg.PortFile)
			newKey, kerr := ReadKey(cfg.KeyFile)
			if perr != nil || kerr != nil {
				relayCancel()
				return
			}
			if newPort != port || newKey != key {
				relayCancel()
				return
			}
		}
	}
}

const errDaemonRestartedMsg = "context canceled"

func isDaemonRestarted(err error) bool {
	if err == nil {
		return false
	}
	return strings.Contains(err.Error(), errDaemonRestartedMsg)
}

func connectHTTP(ctx context.Context, endpoint, apiKey string) (mcp.Connection, error) {
	httpTransport := &mcp.StreamableClientTransport{
		Endpoint: endpoint,
		HTTPClient: &http.Client{
			Timeout:   5 * time.Minute,
			Transport: &authRoundTripper{key: apiKey, base: http.DefaultTransport},
		},
	}
	return httpTransport.Connect(ctx)
}

func waitForDaemon(ctx context.Context, cfg Config) (int, string, error) {
	for {
		port, perr := ReadPort(cfg.PortFile)
		key, kerr := ReadKey(cfg.KeyFile)
		if perr == nil && kerr == nil && port > 0 && key != "" {
			if isPortAlive(port) {
				return port, key, nil
			}
			cfg.logf("port %d not reachable, retrying…", port)
		}
		if cfg.EnsureDaemon != nil {
			cfg.EnsureDaemon()
		}
		select {
		case <-ctx.Done():
			return 0, "", ctx.Err()
		case <-time.After(cfg.RetryInterval):
		}
	}
}

func isPortAlive(port int) bool {
	conn, err := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", port), 500*time.Millisecond)
	if err != nil {
		return false
	}
	_ = conn.Close()
	return true
}

func relay(ctx context.Context, stdioConn, httpConn mcp.Connection) error {
	errc := make(chan error, 2)

	go func() {
		for {
			msg, err := stdioConn.Read(ctx)
			if err != nil {
				errc <- fmt.Errorf("stdio read: %w", err)
				return
			}
			if err := httpConn.Write(ctx, msg); err != nil {
				errc <- fmt.Errorf("http write: %w", err)
				return
			}
		}
	}()
	go func() {
		for {
			msg, err := httpConn.Read(ctx)
			if err != nil {
				errc <- fmt.Errorf("http read: %w", err)
				return
			}
			if err := stdioConn.Write(ctx, msg); err != nil {
				errc <- fmt.Errorf("stdio write: %w", err)
				return
			}
		}
	}()

	return <-errc
}

func isStdioClosed(err error) bool {
	if err == nil {
		return false
	}
	s := err.Error()
	return strings.Contains(s, "stdio read") &&
		(strings.Contains(s, "EOF") || strings.Contains(s, "closed"))
}

type authRoundTripper struct {
	key  string
	base http.RoundTripper
}

func (t *authRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
	req = req.Clone(req.Context())
	req.Header.Set("Authorization", "Bearer "+t.key)
	return t.base.RoundTrip(req)
}
