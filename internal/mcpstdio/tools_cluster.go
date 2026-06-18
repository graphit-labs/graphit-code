package mcpstdio

import (
	"context"
	"fmt"
	"path/filepath"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
)

type clusterSetInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Key        string `json:"key" jsonschema:"Cluster label key (required)"`
	Value      string `json:"value" jsonschema:"Cluster label value (required)"`
}

type clusterGetInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Key         string `json:"key,omitempty" jsonschema:"Cluster label key to retrieve. If empty, retrieves all labels."`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type clusterUnsetInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Key        string `json:"key" jsonschema:"Cluster label key to remove (required)"`
}

type clusterProjectsInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Label       string `json:"label,omitempty" jsonschema:"Optional cluster label key to filter by"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

func registerClusterTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("cluster", "set"),
		Description: "Set a cluster label for grouping the project in the ecosystem.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input clusterSetInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		err = withProjectDir(projectDir, func() error {
			lp := filepath.Join(projectDir, brand.LockFileName())
			lf, err := hub.LoadLockfile(lp)
			if err != nil || lf == nil {
				return fmt.Errorf("cannot load lockfile: %w", err)
			}

			projectID := lf.Project.ID
			if projectID == "" {
				return fmt.Errorf("project has no ID")
			}

			mgr, err := hub.NewGlobalLockManager()
			if err != nil {
				return fmt.Errorf("global lock: %w", err)
			}

			if err := mgr.SetCluster(projectID, projectDir, input.Key, input.Value); err != nil {
				return err
			}

			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Cluster label %s=%s set successfully.", input.Key, input.Value))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("cluster", "get"),
		Description: "Get a specific cluster label value, or all cluster labels set on the project.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input clusterGetInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var result any
		err = withProjectDir(projectDir, func() error {
			lp := filepath.Join(projectDir, brand.LockFileName())
			lf, err := hub.LoadLockfile(lp)
			if err != nil || lf == nil {
				return fmt.Errorf("cannot load lockfile: %w", err)
			}

			projectID := lf.Project.ID
			if projectID == "" {
				return fmt.Errorf("project has no ID")
			}

			mgr, err := hub.NewGlobalLockManager()
			if err != nil {
				return fmt.Errorf("global lock: %w", err)
			}

			if input.Key != "" {
				vals, err := mgr.GetCluster(projectID, projectDir, input.Key)
				if err != nil {
					return err
				}
				result = vals
			} else {
				labels, err := mgr.GetAllClusterLabels(projectID, projectDir)
				if err != nil {
					return err
				}
				result = labels
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(result)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("cluster", "unset"),
		Description: "Remove a cluster label from the project.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input clusterUnsetInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		err = withProjectDir(projectDir, func() error {
			lp := filepath.Join(projectDir, brand.LockFileName())
			lf, err := hub.LoadLockfile(lp)
			if err != nil || lf == nil {
				return fmt.Errorf("cannot load lockfile: %w", err)
			}

			projectID := lf.Project.ID
			if projectID == "" {
				return fmt.Errorf("project has no ID")
			}

			mgr, err := hub.NewGlobalLockManager()
			if err != nil {
				return fmt.Errorf("global lock: %w", err)
			}

			if err := mgr.UnsetCluster(projectID, projectDir, input.Key); err != nil {
				return err
			}

			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Cluster label %q removed successfully.", input.Key))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("cluster", "projects"),
		Description: "List all projects in the same cluster as the current project (including itself). Optionally filter by a specific cluster label key.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input clusterProjectsInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var result any
		err = withProjectDir(projectDir, func() error {
			projects, err := hub.GetClusterProjects(projectDir, input.Label)
			if err != nil {
				return err
			}
			result = projects
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(result)
		}
		return jsonResult(result)
	}))
}
