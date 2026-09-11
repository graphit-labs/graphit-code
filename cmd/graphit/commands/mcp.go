package commands

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newMCPCmd() *cobra.Command {
	var stdio bool

	cmd := &cobra.Command{
		Use:   "mcp",
		Short: "Start the MCP (Model Context Protocol) server",
		Long: `Start an MCP server exposing all Graphit tools.

The server runs inside the daemon process and is accessible via HTTP.
Agent integration uses the --stdio flag, which starts a local proxy that
relays MCP messages between stdin/stdout and the daemon's HTTP endpoint.

Transports:
  • Stdio: reads JSON-RPC from stdin, proxies to daemon HTTP (--stdio)
  • HTTP:  the daemon exposes the MCP endpoint directly

Examples:
  ` + brand.BinName() + ` mcp                     # Show MCP HTTP endpoint info
  ` + brand.BinName() + ` mcp --stdio             # stdio transport for Agent integration`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			if stdio {
				output.Mute()
				return runMCPStdioProxy()
			}
			return showMCPEndpoint()
		},
	}

	cmd.Flags().BoolVar(&stdio, "stdio", false, "Use stdio transport (Agent integration)")

	return cmd
}

func runMCPStdioProxy() error {
	cfg := mcpproxy.Config{
		PortFile:     daemonctl.PortFilePath(),
		KeyFile:      daemonctl.KeyFilePath(),
		EnsureDaemon: func() { _, _ = daemon.EnsureRunning() },
		Stderr:       os.Stderr,
	}
	snapshot, err := auth.ResolveActive(context.Background())
	if err == nil && snapshot.Provider.MCP.Endpoint != "" {
		cfg.Endpoint = snapshot.Provider.MCP.Endpoint
		cfg.EnsureDaemon = nil
		cfg.ResolveBearer = func(ctx context.Context) (string, error) {
			active, resolveErr := auth.ResolveActive(ctx)
			if resolveErr != nil {
				return "", resolveErr
			}
			if active.Provider.MCP.Endpoint != cfg.Endpoint {
				return "", fmt.Errorf("active profile MCP provider changed")
			}
			if active.Profile.OIDC != nil {
				return active.Profile.OIDC.AccessToken, nil
			}
			return active.Profile.MCPKey, nil
		}
	} else if err != nil && !strings.Contains(err.Error(), "no active account profile") {
		return err
	}
	return mcpproxy.RunProxy(cfg, os.Stdin, os.Stdout)
}

func showMCPEndpoint() error {
	p := output.NewPrinter("")
	if snapshot, err := auth.ResolveActive(context.Background()); err == nil && snapshot.Provider.MCP.Endpoint != "" {
		p.Header("MCP Server")
		p.KeyValue("Endpoint", snapshot.Provider.MCP.Endpoint)
		p.KeyValue("Provider", snapshot.Provider.Name)
		p.KeyValue("Auth", "Bearer credential from active profile "+snapshot.Profile.Name)
		return nil
	}

	_, _ = daemon.EnsureRunning()

	var port int
	for i := 0; i < 20; i++ {
		var err error
		port, err = mcpproxy.ReadPort(daemonctl.PortFilePath())
		if err == nil && port > 0 {
			break
		}
		time.Sleep(500 * time.Millisecond)
	}

	if port == 0 {
		p.Error("Daemon MCP endpoint not available. Is the daemon running?")
		p.Step("Try: %s daemon", brand.BinName())
		return fmt.Errorf("MCP endpoint not available")
	}

	p.Header("MCP Server")
	p.KeyValue("Endpoint", fmt.Sprintf("http://127.0.0.1:%d/mcp", port))
	p.KeyValue("Transport", "Streamable HTTP")
	p.KeyValue("Auth", "Bearer token (see ~/"+brand.DotDir()+"/daemon/mcp.key)")
	p.Step("For Agent integration: %s mcp --stdio", brand.BinName())

	return nil
}
