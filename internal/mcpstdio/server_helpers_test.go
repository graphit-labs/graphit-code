package mcpstdio

import (
	"context"
	"fmt"
	"io"
	"os"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/output"
)

type nopWriteCloser struct{ io.Writer }

func (nopWriteCloser) Close() error { return nil }

func Serve(ctx context.Context) error {
	output.Mute()

	server := NewServer()

	// IOTransport instead of StdioTransport: decouples from os.Stdout
	// so output.Mute() reassignments don't break the JSON-RPC channel.
	transport := &mcp.IOTransport{
		Reader: io.NopCloser(os.Stdin),
		Writer: nopWriteCloser{os.Stdout},
	}

	if err := server.Run(ctx, transport); err != nil {
		return fmt.Errorf("MCP stdio error: %w", err)
	}
	return nil
}
