package mcpserver

import (
	"context"
	"encoding/json"
	"fmt"
	"os"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/hub"
)

type hubListInput struct {
	Type string `json:"type,omitempty" jsonschema:"Filter by artifact type: knowledge, ast, rule, skill, command, agent, mcp, power"`
}

type hubSearchInput struct {
	Query string `json:"query" jsonschema:"Search term to find artifacts"`
	Type  string `json:"type,omitempty" jsonschema:"Filter by artifact type"`
}

type hubShowInput struct {
	ID   string `json:"id" jsonschema:"Artifact ID to show details for"`
	Type string `json:"type,omitempty" jsonschema:"Artifact type (helps disambiguate)"`
}

type hubInstallInput struct {
	ID         string `json:"id" jsonschema:"Artifact ID to install. Supports @version suffix for version pinning (e.g. my-rule@1.2.0 for exact version or my-rule@1 for latest 1.x.x)"`
	Type       string `json:"type,omitempty" jsonschema:"Artifact type"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE (claude, cursor, gemini, etc.)"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory"`
}

type hubUninstallInput struct {
	ID         string `json:"id" jsonschema:"Artifact ID to uninstall"`
	Type       string `json:"type,omitempty" jsonschema:"Artifact type"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory"`
}

type hubUpdateInput struct {
	ID         string `json:"id,omitempty" jsonschema:"Artifact ID to update. If omitted updates all installed artifacts."`
	Type       string `json:"type,omitempty" jsonschema:"Artifact type (helps disambiguate when updating a specific artifact)"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory"`
}



func registerHubTools(server *mcp.Server) {

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_hub_list",
		Description: "List available artifacts in the Graphit Hub registry, optionally filtered by type.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input hubListInput) (*mcp.CallToolResult, any, error) {
		hubSvc := hub.NewHubAppService("")
		summaries, err := hubSvc.ListSummary(ctx, input.Type)
		if err != nil {
			return errResult(err)
		}
		if len(summaries) == 0 {
			return textResult("No artifacts found.")
		}

		data, err := json.MarshalIndent(summaries, "", "  ")
		if err != nil {
			return errResult(fmt.Errorf("failed to format entries: %w", err))
		}
		return textResult(string(data))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_hub_search",
		Description: "Search the Graphit Hub registry for artifacts by name, ID, or description.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input hubSearchInput) (*mcp.CallToolResult, any, error) {
		hubSvc := hub.NewHubAppService("")
		summaries, err := hubSvc.SearchSummary(ctx, input.Query, input.Type)
		if err != nil {
			return errResult(err)
		}
		if len(summaries) == 0 {
			return textResult(fmt.Sprintf("No results for %q.", input.Query))
		}

		data, err := json.MarshalIndent(summaries, "", "  ")
		if err != nil {
			return errResult(fmt.Errorf("failed to format results: %w", err))
		}
		return textResult(string(data))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_hub_show",
		Description: "Show detailed information about a specific artifact in the Graphit Hub.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input hubShowInput) (*mcp.CallToolResult, any, error) {
		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(fmt.Errorf("failed to load hub registry: %w", err))
		}

		entry := reg.GetEntry(input.ID, hub.ArtifactType(input.Type))
		if entry == nil {
			return errResult(fmt.Errorf("artifact %q not found", input.ID))
		}

		data, err := json.MarshalIndent(entry, "", "  ")
		if err != nil {
			return errResult(fmt.Errorf("failed to format entry: %w", err))
		}
		return textResult(string(data))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_hub_install",
		Description: "Install an artifact from the Graphit Hub into the current project.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input hubInstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		hubSvc := hub.NewHubAppService(projectDir)
		ide := hubSvc.ResolveIDE(input.IDE)

		origWd, _ := os.Getwd()
		_ = os.Chdir(projectDir)
		defer func() { _ = os.Chdir(origWd) }()

		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(fmt.Errorf("failed to load hub registry: %w", err))
		}

		presenter := hub.NewHubPresenter(reg)
		presenter.Install(ctx, input.ID, "", ide, hub.ArtifactType(input.Type))

		return textResult(fmt.Sprintf("Artifact %q installed successfully.", input.ID))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_hub_uninstall",
		Description: "Remove an installed artifact from the current project.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input hubUninstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		hubSvc := hub.NewHubAppService(projectDir)
		ide := hubSvc.ResolveIDE(input.IDE)

		origWd, _ := os.Getwd()
		_ = os.Chdir(projectDir)
		defer func() { _ = os.Chdir(origWd) }()

		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(fmt.Errorf("failed to load hub registry: %w", err))
		}

		presenter := hub.NewHubPresenter(reg)
		presenter.Uninstall(ctx, input.ID, hub.ArtifactType(input.Type), ide)

		return textResult(fmt.Sprintf("Artifact %q uninstalled.", input.ID))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_hub_update",
		Description: "Update installed hub artifacts. Without an ID updates all artifacts. With an ID updates only that specific artifact.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input hubUpdateInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		hubSvc := hub.NewHubAppService(projectDir)
		ide := hubSvc.ResolveIDE(input.IDE)

		origWd, _ := os.Getwd()
		_ = os.Chdir(projectDir)
		defer func() { _ = os.Chdir(origWd) }()

		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(fmt.Errorf("failed to load hub registry: %w", err))
		}

		presenter := hub.NewHubPresenter(reg)

		if input.ID != "" {
			presenter.UpdateOneArtifact(ctx, input.ID, hub.ArtifactType(input.Type), ide)
			return textResult(fmt.Sprintf("Artifact %q updated.", input.ID))
		}

		presenter.Update(ctx, ide)
		return textResult("All artifacts updated.")
	})
}
