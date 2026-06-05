// Package mcpproxy implements a reconnectable bidirectional proxy between
// an MCP stdio client (IDE) and the daemon's MCP unix socket.
//
// When the daemon restarts or crashes, the proxy automatically reconnects
// instead of dying, keeping the IDE-side MCP process alive.
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
	SockFile       string
	EnsureDaemon   func() // called before each connection attempt; may be nil
	MaxDialRetries int    // default 30
	RetryInterval  time.Duration // default 500ms
	Stderr         io.Writer // diagnostic log output; nil to suppress
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
// and the daemon's MCP unix socket. It returns when stdin closes or when
// all dial retries are exhausted.
func RunProxy(cfg Config, stdin io.Reader, stdout io.Writer) error {
	cfg.applyDefaults()

	lines := make(chan []byte, 32)
	stdinDone := make(chan struct{})
	go func() {
		defer close(stdinDone)
		defer close(lines)
		scanner := bufio.NewScanner(stdin)
		scanner.Buffer(make([]byte, 0, 10<<20), 10<<20)
		for scanner.Scan() {
			raw := scanner.Bytes()
			line := make([]byte, len(raw))
			copy(line, raw)
			lines <- line
		}
	}()

	var pending []byte

	for {
		if cfg.EnsureDaemon != nil {
			cfg.EnsureDaemon()
		}

		conn, err := dialRetry(cfg.SockFile, cfg.MaxDialRetries, cfg.RetryInterval)
		if err != nil {
			return fmt.Errorf("connect to daemon mcp.sock after %d retries: %w",
				cfg.MaxDialRetries, err)
		}
		cfg.logf("connected to daemon")

		connDead := make(chan struct{})
		go func() {
			defer close(connDead)
			_, _ = io.Copy(stdout, conn)
		}()

		if pending != nil {
			if _, werr := conn.Write(pending); werr != nil {
				conn.Close()
				<-connDead
				cfg.logf("dropped pending message after reconnect write failure")
				pending = nil
				time.Sleep(cfg.RetryInterval)
				continue
			}
			pending = nil
		}

		reconnect := false
		for !reconnect {
			select {
			case line, ok := <-lines:
				if !ok {
					conn.Close()
					<-connDead
					return nil
				}
				msg := make([]byte, len(line)+1)
				copy(msg, line)
				msg[len(line)] = '\n'
				if _, werr := conn.Write(msg); werr != nil {
					pending = msg
					reconnect = true
				}

			case <-connDead:
				reconnect = true

			case <-stdinDone:
				conn.Close()
				<-connDead
				return nil
			}
		}

		conn.Close()
		<-connDead

		select {
		case <-stdinDone:
			return nil
		default:
		}

		cfg.logf("daemon connection lost, reconnecting...")
		time.Sleep(cfg.RetryInterval)
	}
}

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
