package mcpstdio

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/toon"
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
	content := []mcp.Content{
		&mcp.TextContent{Text: string(data)},
	}
	if ide.SysReminder != "" {
		content = append(content, &mcp.TextContent{Text: ide.SysReminder})
	}
	return &mcp.CallToolResult{
		Content: content,
	}, nil, nil
}

func toonResult(v any) (*mcp.CallToolResult, any, error) {
	if s, ok := v.(string); ok {
		return textResult(s)
	}
	return textResult(toon.FormatAny(v))
}

// noticeResult is a payload with a sentence in front of it, for the cases where the
// tool did something the caller needs to know about before reading the answer —
// serving a scope other than the one that was asked for, for instance.
func noticeResult(notice string, v any, useToon bool) (*mcp.CallToolResult, any, error) {
	if useToon {
		return textResult(notice + "\n" + toon.FormatAny(v))
	}
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return errResult(fmt.Errorf("marshal result: %w", err))
	}
	return textResult(notice + "\n" + string(data))
}

// aiOpt returns true by default when v is nil (parameter not sent by caller).
// MCP tools use compact TOON format unless the caller explicitly passes false.
func aiOpt(v *bool) bool {
	return v == nil || *v
}
