package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

type hubListInput struct {
	Type        string `json:"type,omitempty" jsonschema:"Filter by artifact type: knowledge, ast, rule, skill, command, agent, mcp, power"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type hubSearchInput struct {
	Query       string `json:"query" jsonschema:"Search term to find artifacts (required)"`
	Type        string `json:"type,omitempty" jsonschema:"Filter by artifact type"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type hubShowInput struct {
	ID          string `json:"id" jsonschema:"Artifact ID to show details for (required)"`
	Type        string `json:"type,omitempty" jsonschema:"Artifact type (helps disambiguate)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type hubInstallInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to install globally, with no project: the artifact lands in the shared version-keyed store and is addressed afterwards by its qualified id@version."`
	ID          string `json:"id" jsonschema:"Artifact ID to install. Supports @version suffix for version pinning (required)"`
	Type        string `json:"type,omitempty" jsonschema:"Artifact type"`
	IDE         string `json:"ide,omitempty" jsonschema:"Target IDE (claude, cursor, gemini, etc.). Ignored for a global install, which has no IDE directory to materialise into."`
	Alias       string `json:"alias,omitempty" jsonschema:"Alias to assign to installed artifact"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type hubUninstallInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to drop a global install."`
	ID         string `json:"id" jsonschema:"Artifact ID to uninstall (required)"`
	Type       string `json:"type,omitempty" jsonschema:"Artifact type"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
}

type hubContentInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to read a globally installed artifact."`
	ID         string `json:"id" jsonschema:"Artifact ID, optionally qualified with @version (required)"`
	Type       string `json:"type,omitempty" jsonschema:"Artifact type: rule, skill, command or agent. Only needed when the same id exists under more than one type."`
	Path       string `json:"path,omitempty" jsonschema:"Return only this file, as an artifact-relative path. Omit to return every file."`
}

type hubUpdateInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID         string `json:"id,omitempty" jsonschema:"Artifact ID to update. If omitted, updates all artifacts"`
	Type       string `json:"type,omitempty" jsonschema:"Artifact type"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
}

type hubSubmitInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Artifact ID to publish (required)"`
	LocalPath   string `json:"local_path" jsonschema:"Local directory path to artifact source (required)"`
	Version     string `json:"version,omitempty" jsonschema:"Artifact version (defaults to 1.0.0)"`
	Name        string `json:"name,omitempty" jsonschema:"Display name override"`
	Description string `json:"description,omitempty" jsonschema:"Detailed description"`
	Type        string `json:"type,omitempty" jsonschema:"Artifact type (defaults to rule)"`
	Tags        string `json:"tags,omitempty" jsonschema:"Comma-separated tags"`
}

type hubLinkInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Name        string `json:"name" jsonschema:"Name of the linked artifact (required)"`
	SourcePath  string `json:"source_path" jsonschema:"Path to local source project to link (required)"`
	Type        string `json:"type" jsonschema:"Artifact type: ast, knowledge, rule, skill, command, agent, mcp (required)"`
	IDE         string `json:"ide,omitempty" jsonschema:"Target IDE"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type hubUnlinkInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Name       string `json:"name" jsonschema:"Name of linked artifact to remove (required)"`
	Type       string `json:"type" jsonschema:"Artifact type (required)"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
}

type hubProjectsInput struct {
	AiOptimized *bool `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type hubTypePathInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Type       string `json:"type" jsonschema:"Artifact type: skill, rule, command, agent, mcp (required)"`
	Name       string `json:"name" jsonschema:"Artifact name (required)"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE (claude, cursor, gemini, etc.)"`
}

func registerHubTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "list"),
		Description: "List available artifacts in the Graphit Hub registry.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubListInput) (*mcp.CallToolResult, any, error) {
		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(err)
		}

		entries := reg.ListEntries(hub.ArtifactType(input.Type))
		if aiOpt(input.AiOptimized) {
			return toonResult(entries)
		}
		return jsonResult(entries)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "search"),
		Description: "Search the Graphit Hub registry for artifacts by name, ID, or description.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubSearchInput) (*mcp.CallToolResult, any, error) {
		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(err)
		}

		entries := reg.SearchEntries(input.Query, hub.ArtifactType(input.Type))
		if aiOpt(input.AiOptimized) {
			return toonResult(entries)
		}
		return jsonResult(entries)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "show"),
		Description: "Show detailed information about a specific artifact in the Graphit Hub.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubShowInput) (*mcp.CallToolResult, any, error) {
		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(err)
		}

		entry := reg.GetEntry(input.ID, hub.ArtifactType(input.Type))
		if entry == nil {
			return errResult(fmt.Errorf("artifact %q not found", input.ID))
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(entry)
		}
		return jsonResult(entry)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("hub", "install"),
		Description: "Install an artifact from the Graphit Hub into the current project, or globally when project_dir is omitted. " +
			"A global install needs no project: it populates the same shared, version-keyed store, and the artifact is " +
			"addressed afterwards by its qualified id@version — as 'context' for ast and knowledge, and as 'id' for " +
			brand.MCPToolName("hub", "content") + ".",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubInstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		resolvedIDE := ""
		if projectDir != "" {
			resolvedIDE = resolveIDEFromProject(input.IDE, projectDir)
		}

		var result *hub.InstallResult
		err = withProjectDir(projectDir, func() error {
			reg, rerr := hub.NewRegistryManager(ctx)
			if rerr != nil {
				return rerr
			}
			svc := hub.NewHubService(reg)
			result, rerr = svc.Install(ctx, input.ID, input.Alias, resolvedIDE, hub.ArtifactType(input.Type), "", projectDir)
			return rerr
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
		Name:        brand.MCPToolName("hub", "uninstall"),
		Description: "Remove an installed artifact from the current project, or drop a global install when project_dir is omitted.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubUninstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		resolvedIDE := ""
		if projectDir != "" {
			resolvedIDE = resolveIDEFromProject(input.IDE, projectDir)
		}

		err = withProjectDir(projectDir, func() error {
			reg, rerr := hub.NewRegistryManager(ctx)
			if rerr != nil {
				return rerr
			}
			svc := hub.NewHubService(reg)
			return svc.Uninstall(ctx, input.ID, hub.ArtifactType(input.Type), true, resolvedIDE, projectDir)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Artifact %q uninstalled.", input.ID))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "update"),
		Description: "Update installed hub artifacts in the current project.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubUpdateInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		resolvedIDE := resolveIDEFromProject(input.IDE, projectDir)

		err = withProjectDir(projectDir, func() error {
			reg, rerr := hub.NewRegistryManager(ctx)
			if rerr != nil {
				return rerr
			}
			svc := hub.NewHubService(reg)

			if input.ID != "" {
				return svc.UpdateOne(ctx, input.ID, hub.ArtifactType(input.Type), resolvedIDE, projectDir)
			}

			results := svc.UpdateAll(ctx, resolvedIDE, projectDir)
			var errs []string
			for artID, uerr := range results {
				if uerr != nil {
					errs = append(errs, fmt.Sprintf("%s: %v", artID, uerr))
				}
			}
			if len(errs) > 0 {
				return fmt.Errorf("update completed with errors:\n%s", strings.Join(errs, "\n"))
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return textResult("Hub update completed successfully.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "submit"),
		Description: "Publish a local artifact to the hub.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubSubmitInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		localPath := input.LocalPath
		if !filepath.IsAbs(localPath) {
			localPath = filepath.Join(projectDir, localPath)
		}
		if _, err := os.Stat(localPath); err != nil {
			return errResult(err)
		}

		tagList := []string{}
		if input.Tags != "" {
			for _, t := range strings.Split(input.Tags, ",") {
				if t = strings.TrimSpace(t); t != "" {
					tagList = append(tagList, t)
				}
			}
		}

		version := input.Version
		if version == "" {
			version = "1.0.0"
		}

		artType := input.Type
		if artType == "" {
			artType = "rule"
		}

		meta := &hub.Entry{
			ID:          input.ID,
			Name:        input.Name,
			Type:        hub.ArtifactType(artType),
			Description: input.Description,
			Tags:        tagList,
		}

		err = withProjectDir(projectDir, func() error {
			reg, rerr := hub.NewRegistryManager(ctx)
			if rerr != nil {
				return rerr
			}

			if err := reg.PublishEntry(ctx, input.ID, localPath, meta, version); err != nil {
				return err
			}

			projectCfg, ides := loadProjectLockInfo(projectDir)
			resolvedIDE := config.ResolveProjectIDE("", nil, projectCfg, ides)
			hubSvc := hub.NewHubService(reg)
			_ = hubSvc.RecordPublish(ctx, input.ID, hub.ArtifactType(artType), version, resolvedIDE, projectDir)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Artifact %q@%s published successfully.", input.ID, version))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "link"),
		Description: "Link a local project's artifacts into the current project via symlinks.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubLinkInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		resolvedIDE := resolveIDEFromProject(input.IDE, projectDir)

		var result *hub.LinkResult
		err = withProjectDir(projectDir, func() error {
			reg, rerr := hub.NewRegistryManager(ctx)
			if rerr != nil {
				return rerr
			}
			svc := hub.NewHubService(reg)
			result, rerr = svc.Link(ctx, input.Name, input.SourcePath, resolvedIDE, hub.ArtifactType(input.Type), projectDir)
			if rerr != nil {
				return rerr
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
		Name:        brand.MCPToolName("hub", "unlink"),
		Description: "Remove a linked artifact from the current project.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubUnlinkInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		resolvedIDE := resolveIDEFromProject(input.IDE, projectDir)

		err = withProjectDir(projectDir, func() error {
			reg, rerr := hub.NewRegistryManager(ctx)
			if rerr != nil {
				return rerr
			}
			svc := hub.NewHubService(reg)
			if err := svc.Unlink(ctx, input.Name, resolvedIDE, hub.ArtifactType(input.Type), projectDir); err != nil {
				return err
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Artifact %q unlinked.", input.Name))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("hub", "content"),
		Description: "Read the CONTENT of an installed rule, skill, command or agent artifact. " +
			"An artifact is often several files — a skill is — so the answer is a map KEYED BY the artifact-relative " +
			"PATH, with each file's text as the value, and 'canonical' naming the entry-point file to read first. " +
			"project_dir is optional: with one, that project's claim decides the version; without one, the globally " +
			"installed artifact is read and the id may carry an @version. ast and knowledge artifacts are not served " +
			"here — they are mounted rather than downloaded, so read them with " +
			brand.MCPToolName("ast", "source") + " and " + brand.MCPToolName("wiki", "source") + ".",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubContentInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		if input.ID == "" {
			return errResult(fmt.Errorf("id is required"))
		}

		var content *hub.ArtifactContent
		err = withProjectDir(projectDir, func() error {
			reg, rerr := hub.NewRegistryManager(ctx)
			if rerr != nil {
				return rerr
			}
			svc := hub.NewHubService(reg)
			content, rerr = svc.ArtifactContentFor(ctx, projectDir, input.ID,
				hub.ArtifactType(strings.ToLower(input.Type)), input.Path)
			return rerr
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(content)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "projects"),
		Description: "List registered projects in the global lock.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubProjectsInput) (*mcp.CallToolResult, any, error) {
		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(err)
		}

		projects := reg.ListProjects()
		if aiOpt(input.AiOptimized) {
			return toonResult(projects)
		}
		return jsonResult(projects)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("hub", "type-path"),
		Description: "Resolve the absolute IDE path where a physical skill, command, or agent artifact should be created. Hub rules are hook-delivered and intentionally have no IDE path.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input hubTypePathInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		resolvedIDE := resolveIDEFromProject(input.IDE, projectDir)

		typePath, err := ide.ArtifactTypePath(projectDir, resolvedIDE, strings.ToLower(input.Type), input.Name)
		if err != nil {
			return errResult(err)
		}
		return textResult(typePath)
	}))
}
