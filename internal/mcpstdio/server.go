package mcpstdio

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"os"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/output"
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

func Serve(ctx context.Context) error {
	output.Mute()
	log.SetOutput(os.Stderr)

	server := NewServer()

	// Use IOTransport with explicit reader/writer instead of StdioTransport
	// which hardcodes os.Stdin/os.Stdout. This decouples the JSON-RPC
	// transport from the Go-level os.Stdout variable, so even if something
	// reassigns os.Stdout, the transport is unaffected.
	transport := &mcp.IOTransport{
		Reader: io.NopCloser(os.Stdin),
		Writer: nopWriteCloser{os.Stdout},
	}

	if err := server.Run(ctx, transport); err != nil {
		return fmt.Errorf("MCP stdio error: %w", err)
	}
	return nil
}

// nopWriteCloser wraps a Writer with a no-op Close.
type nopWriteCloser struct{ io.Writer }

func (nopWriteCloser) Close() error { return nil }

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
		// Check if the daemon is running, and lift it if necessary, just like the CLI does
		if _, dErr := daemon.EnsureRunning(); dErr != nil {
			log.Printf("[MCP] Failed to ensure daemon is running: %v\n", dErr)
		}
		return handler(ctx, req, input)
	}
}

func textResult(text string) (*mcp.CallToolResult, any, error) {
	return &mcp.CallToolResult{
		Content: []mcp.Content{&mcp.TextContent{Text: text}},
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
	return textResult(string(data))
}
