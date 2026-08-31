package mcpstdio

import (
	"context"
	"fmt"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/backlog"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

type backlogListInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type backlogAddInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Title       string `json:"title" jsonschema:"One-line description of the task (required)"`
	Body        string `json:"body,omitempty" jsonschema:"The full brief, written for a reader with no conversation history"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type backlogRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Slug       string `json:"slug" jsonschema:"Slug of the backlog item to remove (required)"`
}

func registerBacklogTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("backlog", "list"),
		Description: "List the task backlog recorded in the documentation tree.",
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
		Name:        brand.MCPToolName("backlog", "add"),
		Description: "Record a task in the documentation-backed backlog. Dream is not required.",
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
		Name:        brand.MCPToolName("backlog", "remove"),
		Description: "Remove a task backlog item by slug.",
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
