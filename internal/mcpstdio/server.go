package mcpstdio

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/version"
)

func NewServer() *mcp.Server {
	server := mcp.NewServer(
		&mcp.Implementation{
			Name:    brand.MCPServerName("code-stdio"),
			Version: version.Version,
		},
		&mcp.ServerOptions{},
	)

	registerLifecycleTools(server)
	registerASTTools(server)
	registerKnowledgeTools(server)
	registerMemoryTools(server)
	registerHubTools(server)
	registerWikiTools(server)
	registerDreamTools(server)
	registerDaemonTools(server)
	registerClusterTools(server)
	registerImprovementsTools(server)

	return server
}

func ServeConn(ctx context.Context, conn net.Conn) error {
	server := NewServer()
	transport := &mcp.IOTransport{
		Reader: conn,
		Writer: conn,
	}
	if err := server.Run(ctx, transport); err != nil {
		return fmt.Errorf("MCP conn error: %w", err)
	}
	return nil
}

// safeTool wraps a tool handler with panic recovery and background daemon autostart validation.
func safeTool[T any](
	handler func(ctx context.Context, req *mcp.CallToolRequest, input T) (*mcp.CallToolResult, any, error),
) func(ctx context.Context, req *mcp.CallToolRequest, input T) (*mcp.CallToolResult, any, error) {
	return func(ctx context.Context, req *mcp.CallToolRequest, input T) (result *mcp.CallToolResult, session any, err error) {
		defer func() {
			if r := recover(); r != nil {
				err = fmt.Errorf("internal error (panic): %v", r)
				result = nil
				session = nil
			}
		}()
		if _, dErr := daemon.EnsureRunning(); dErr != nil {
			slog.Warn("failed to ensure daemon is running", "error", dErr)
		}
		return handler(ctx, req, input)
	}
}

func textResult(text string) (*mcp.CallToolResult, any, error) {
	return &mcp.CallToolResult{
		Content: []mcp.Content{&mcp.TextContent{Text: text + ide.SysReminder}},
	}, nil, nil
}

func errResult(err error) (*mcp.CallToolResult, any, error) {
	return nil, nil, err
}

func jsonResult(v any) (*mcp.CallToolResult, any, error) {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return errResult(fmt.Errorf("marshal result: %w", err))
	}
	return &mcp.CallToolResult{
		Content: []mcp.Content{
			&mcp.TextContent{Text: string(data)},
			&mcp.TextContent{Text: ide.SysReminder},
		},
	}, nil, nil
}
