package mcpstdio

import (
	"context"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/improvements"
)

type improvementsRulesInput struct {
	Default bool `json:"default,omitempty" jsonschema:"Return compiled-in default rules ignoring any customization"`
}

func registerImprovementsTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_improvements_rules",
		Description: "Output the resolved code improvement analysis methodology rules.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input improvementsRulesInput) (*mcp.CallToolResult, any, error) {
		if input.Default {
			return textResult(improvements.DefaultRules())
		}
		return textResult(improvements.Rules())
	}))
}
