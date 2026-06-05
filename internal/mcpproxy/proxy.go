// Package mcpproxy implements a reconnectable bidirectional proxy between
// an MCP stdio client (IDE) and the daemon's MCP unix socket.
//
// When the daemon restarts or crashes, the proxy automatically reconnects
// instead of dying, keeping the IDE-side MCP process alive. The MCP stdio
// transport uses newline-delimited JSON-RPC, so messages are buffered at
// line boundaries to prevent data loss during reconnection.
package mcpproxy

import (
	"bufio"
	"fmt"
	"io"
	"net"
	"time"
)

// Config configures the reconnectable MCP proxy.
type Config struct {
	// SockFile is the path to the daemon's MCP unix socket.
	SockFile string

	// EnsureDaemon is called before each connection attempt to guarantee
	// the daemon process is running. May be nil.
	EnsureDaemon func()

	// MaxDialRetries is the maximum number of dial attempts per connection
	// cycle. Each retry waits RetryInterval. Default: 30.
	MaxDialRetries int

	// RetryInterval is the pause between dial retries. Default: 500ms.
	RetryInterval time.Duration

	// Stderr receives diagnostic log lines. May be nil to suppress logging.
	Stderr io.Writer
}

func (c *Config) applyDefaults() {
	if c.MaxDialRetries <= 0 {
		c.MaxDialRetries = 30
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

// RunProxy starts a reconnectable bidirectional proxy between stdin/stdout
// and the daemon's MCP unix socket.
//
// It reads stdin line-by-line into a buffered channel that survives daemon
// reconnections. When the daemon connection breaks (EOF, write error), the
// proxy closes the old connection, re-ensures the daemon is running, and
// dials a new connection — all transparently to the IDE.
//
// The function returns when stdin is closed (IDE disconnected) or when all
// dial retries are exhausted.
func RunProxy(cfg Config, stdin io.Reader, stdout io.Writer) error {
	cfg.applyDefaults()

	// Read stdin lines into a buffered channel.
	// This goroutine lives for the entire proxy lifetime, surviving reconnections.
	// MCP stdio uses newline-delimited JSON-RPC, so each line is one message.
	lines := make(chan []byte, 32)
	stdinDone := make(chan struct{})
	go func() {
		defer close(stdinDone)
		defer close(lines)

		scanner := bufio.NewScanner(stdin)
		scanner.Buffer(make([]byte, 0, 10<<20), 10<<20) // 10 MiB per message
		for scanner.Scan() {
			raw := scanner.Bytes()
			line := make([]byte, len(raw))
			copy(line, raw)
			lines <- line
		}
	}()

	var pending []byte // message read from stdin but not yet sent to daemon

	for {
		// Ensure daemon is running before dialing.
		if cfg.EnsureDaemon != nil {
			cfg.EnsureDaemon()
		}

		conn, err := dialRetry(cfg.SockFile, cfg.MaxDialRetries, cfg.RetryInterval)
		if err != nil {
			return fmt.Errorf("connect to daemon mcp.sock after %d retries: %w",
				cfg.MaxDialRetries, err)
		}
		cfg.logf("connected to daemon")

		// Start reader goroutine: daemon → stdout.
		connDead := make(chan struct{})
		go func() {
			defer close(connDead)
			_, _ = io.Copy(stdout, conn)
		}()

		// If we have a pending message from a previously broken connection,
		// try to deliver it on the fresh connection.
		if pending != nil {
			if _, werr := conn.Write(pending); werr != nil {
				conn.Close()
				<-connDead
				// Drop the pending message to avoid infinite retry loops.
				// The IDE client will timeout and resend.
				cfg.logf("dropped pending message after reconnect write failure")
				pending = nil
				time.Sleep(cfg.RetryInterval)
				continue
			}
			pending = nil
		}

		// Main loop: forward stdin messages to daemon.
		reconnect := false
		for !reconnect {
			select {
			case line, ok := <-lines:
				if !ok {
					// stdin closed — IDE disconnected. Clean exit.
					conn.Close()
					<-connDead
					return nil
				}
				msg := append(line, '\n')
				if _, werr := conn.Write(msg); werr != nil {
					// Write failed — daemon died mid-request.
					// Preserve the message for the next connection.
					pending = msg
					reconnect = true
				}

			case <-connDead:
				// Daemon closed the connection (restart, crash, etc).
				reconnect = true

			case <-stdinDone:
				// stdin goroutine exited (e.g. broken pipe).
				conn.Close()
				<-connDead
				return nil
			}
		}

		conn.Close()
		<-connDead // wait for reader goroutine to finish

		// If stdin is already closed, exit instead of reconnecting.
		select {
		case <-stdinDone:
			return nil
		default:
		}

		cfg.logf("daemon connection lost, reconnecting...")
		time.Sleep(cfg.RetryInterval)
	}
}

// dialRetry dials the unix socket with exponential-ish retry.
func dialRetry(sockFile string, maxRetries int, interval time.Duration) (net.Conn, error) {
	var lastErr error
	for i := 0; i < maxRetries; i++ {
		conn, err := net.Dial("unix", sockFile)
		if err == nil {
			return conn, nil
		}
		lastErr = err
		time.Sleep(interval)
	}
	return nil, lastErr
}
