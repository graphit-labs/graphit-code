package commands

import (
	"context"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	"github.com/graphit-labs/graphit-code/internal/mcpserver"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newMCPCmd() *cobra.Command {
	var (
		host  string
		port  int
		stdio bool
	)

	cmd := &cobra.Command{
		Use:   "mcp",
		Short: "Start the MCP (Model Context Protocol) server",
		Long: `Start an MCP server exposing AST, Knowledge, Memory, and Hub tools.

The server supports two transports:
  • Streamable HTTP (default): listens on --host:--port
  • Stdio: reads JSON-RPC from stdin, writes to stdout (--stdio)

Examples:
  graphit mcp                     # HTTP on 127.0.0.1:8282
  graphit mcp --port 9090         # HTTP on custom port
  graphit mcp --stdio             # stdio transport for IDE integration`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			ctx := context.Background()

			if stdio {
				output.Mute()
				return runMCPStdioProxy()
			}

			p.Info("Starting MCP server (HTTP transport)...")
			return mcpserver.ServeHTTP(ctx, mcpserver.Options{
				Host: host,
				Port: port,
			})
		},
	}

	cmd.Flags().StringVar(&host, "host", "127.0.0.1", "Host to bind the HTTP server to")
	cmd.Flags().IntVar(&port, "port", 8282, "Port for the HTTP server")
	cmd.Flags().BoolVar(&stdio, "stdio", false, "Use stdio transport instead of HTTP")

	return cmd
}

func runMCPStdioProxy() error {
	mcpSockFile := filepath.Join(brand.GlobalDir(), "daemon", "mcp.sock")

	return mcpproxy.RunProxy(mcpproxy.Config{
		SockFile:     mcpSockFile,
		EnsureDaemon: func() { _, _ = daemon.EnsureRunning() },
		Stderr:       os.Stderr,
	}, os.Stdin, os.Stdout)
}

