package mcpstdio

import (
	"context"
	"fmt"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/backlog"
	"github.com/graphit-labs/graphit-code/internal/brand"

	"github.com/graphit-labs/graphit-code/internal/improvements"
)

type improvementsRulesInput struct {
	Default bool `json:"default,omitempty" jsonschema:"Return compiled-in default rules ignoring any customization"`
}

type backlogListInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type backlogAddInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Title       string `json:"title" jsonschema:"One-line description of the deferred work (required)"`
	Body        string `json:"body,omitempty" jsonschema:"The full brief, written for a reader with no conversation history"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type backlogRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Slug       string `json:"slug" jsonschema:"Slug of the backlog item to remove (required)"`
}

func registerImprovementsTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("improvements", "rules"),
		Description: "Output the resolved code improvement analysis methodology rules.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input improvementsRulesInput) (*mcp.CallToolResult, any, error) {
		if input.Default {
			return textResult(improvements.DefaultRules())
		}
		return textResult(improvements.Rules())
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("improvements", "backlog_list"),
		Description: "List the improvement backlog — work identified but deliberately deferred.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input backlogListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var items []backlog.Item
		err = withProjectDir(projectDir, func() error {
			var lerr error
			items, lerr = backlog.List(projectDir)
			return lerr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(items)
		}
		return jsonResult(items)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("improvements", "backlog_add"),
		Description: "Add an item to the improvement backlog for a later autonomous session to pick up.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input backlogAddInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var item *backlog.Item
		err = withProjectDir(projectDir, func() error {
			var aerr error
			item, aerr = backlog.Add(projectDir, input.Title, input.Body)
			return aerr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(item)
		}
		return jsonResult(item)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("improvements", "backlog_remove"),
		Description: "Remove an item from the improvement backlog by slug.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input backlogRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		err = withProjectDir(projectDir, func() error {
			return backlog.Remove(projectDir, input.Slug)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Backlog item %q removed.", input.Slug))
	}))
}
