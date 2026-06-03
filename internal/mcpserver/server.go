package mcpserver

import (
	"context"
	"fmt"
	"log/slog"
	"net/http"
	"os/signal"
	"syscall"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

type Options struct {
	Host string

	Port int

	Stdio bool

	Verbose bool
}

func NewServer() *mcp.Server {
	server := mcp.NewServer(
		&mcp.Implementation{
			Name:    brand.MCPServerName("code"),
			Version: version.Version,
		},
		&mcp.ServerOptions{},
	)

	registerASTTools(server)
	registerKnowledgeTools(server)
	registerMemoryTools(server)
	registerHubTools(server)

	return server
}

func ServeHTTP(ctx context.Context, opts Options) error {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	host := opts.Host
	if host == "" {
		host = "127.0.0.1"
	}
	port := opts.Port
	if port == 0 {
		port = 8282
	}

	addr := fmt.Sprintf("%s:%d", host, port)

	httpServer := &http.Server{
		Addr:    addr,
		Handler: handler,
	}

	sigCtx, stop := signal.NotifyContext(ctx, syscall.SIGTERM, syscall.SIGINT)
	defer stop()

	go func() {
		<-sigCtx.Done()
		_ = httpServer.Close()
	}()

	slog.Info("MCP server listening", "addr", "http://"+addr+"/mcp")
	if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		return fmt.Errorf("MCP HTTP server error: %w", err)
	}
	return nil
}

func ServeStdio(ctx context.Context) error {
	server := NewServer()

	if err := server.Run(ctx, &mcp.StdioTransport{}); err != nil {
		return fmt.Errorf("MCP stdio error: %w", err)
	}
	return nil
}

