package mcpstdio

import (
	"context"
	"fmt"
	"path/filepath"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/oklog/ulid/v2"
)

type initInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"The directory of the project to initialize (required)"`
	IDE         string `json:"ide,omitempty" jsonschema:"Target IDE (claude, cursor, gemini, etc.)"`
	ID          string `json:"id,omitempty" jsonschema:"Project ID (ULID) override"`
	Name        string `json:"name,omitempty" jsonschema:"Project name override"`
	Description string `json:"description,omitempty" jsonschema:"Project description"`
}

type syncInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory to sync (required)"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
}

type updateInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory to update (required)"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
}

type removeInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory to remove from (required)"`
	IDE        string `json:"ide,omitempty" jsonschema:"Target IDE"`
}

type configSetInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. If global is true, this is ignored."`
	Key        string `json:"key" jsonschema:"Configuration key (e.g. ide, cli, hub.repo)"`
	Value      string `json:"value" jsonschema:"Configuration value"`
	Global     bool   `json:"global,omitempty" jsonschema:"Save to global configuration instead of project"`
}

type configGetInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. If global is true, this is ignored."`
	Key        string `json:"key" jsonschema:"Configuration key to retrieve"`
	Global     bool   `json:"global,omitempty" jsonschema:"Load from global configuration instead of project"`
}

type configUnsetInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. If global is true, this is ignored."`
	Key        string `json:"key" jsonschema:"Configuration key to unset"`
	Global     bool   `json:"global,omitempty" jsonschema:"Remove from global configuration instead of project"`
}

type configListInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory."`
	Global     bool   `json:"global,omitempty" jsonschema:"List global configuration"`
}

type versionInput struct{}

func registerLifecycleTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("init"),
		Description: "Initialize a new project in the given project directory, creating project identity and lockfiles.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input initInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		lockPath := filepath.Join(projectDir, brand.LockFileName())
		var lf *hub.Lockfile
		if existing, err := hub.LoadLockfile(lockPath); err == nil && existing != nil {
			lf = existing
		} else {
			lf = &hub.Lockfile{Artifacts: make(map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta)}
		}

		if input.ID != "" {
			lf.Project.ID = input.ID
		} else if lf.Project.ID == "" {
			lf.Project.ID = ulid.Make().String()
		}

		if input.Name != "" {
			lf.Project.Name = input.Name
		} else if lf.Project.Name == "" {
			lf.Project.Name = filepath.Base(projectDir)
		}

		if input.Description != "" {
			lf.Project.Description = input.Description
		}

		if err := hub.SaveLockfile(lockPath, lf); err != nil {
			return errResult(fmt.Errorf("saving lockfile: %w", err))
		}

		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			reg, _ = hub.NewRegistryManager(ctx)
		}

		resolvedIDE := config.ResolveProjectIDE(input.IDE, nil, lf.Config, lf.IDEs)

		if err := hub.OnInit(ctx, reg, resolvedIDE); err != nil {
			return errResult(fmt.Errorf("hub OnInit: %w", err))
		}

		gitignorePath := filepath.Join(projectDir, ".gitignore")
		ignoreContent := brand.DotDir() + "/"
		_ = git.InjectGitignore(gitignorePath, ignoreContent)

		_ = memory.EnsureScopeDirs("project", projectDir)
		_ = memory.EnsureScopeDirs("user", projectDir)

		if mgr, err := hub.NewGlobalLockManager(); err == nil {
			var regOpts []func(*hub.InstanceEntry)
			if lf.Project.Name != "" {
				regOpts = append(regOpts, hub.WithProjectName(lf.Project.Name))
			}
			if lf.Project.Description != "" {
				regOpts = append(regOpts, hub.WithProjectDescription(lf.Project.Description))
			}
			_ = mgr.RegisterProject(lf.Project.ID, projectDir, regOpts...)
		}

		return textResult(fmt.Sprintf("Project %q initialized successfully (ID: %s, IDE: %s)", lf.Project.Name, lf.Project.ID, resolvedIDE))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("sync"),
		Description: "Sync and reindex all local modules, AST DB, memory wikis, and update IDE rules.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input syncInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg, ides := loadProjectLockInfo(projectDir)
		resolvedIDE := config.ResolveProjectIDE(input.IDE, nil, projectCfg, ides)

		// 1. AST Indexing
		if !config.IsModuleDisabled("ast", nil, projectCfg) {
			db, err := openASTDBReadWrite(projectDir, "")
			if err == nil {
				pipeOpts := ast.PipelineOptions{
					Workers:     4,
					IndexSource: config.ResolveIndexSource(nil, projectCfg),
					CacheDir:    filepath.Dir(ast.DefaultLadybugConfig().DBPath),
				}
				_, _ = ast.RunPipeline(ctx, db, projectDir, pipeOpts)
				_ = db.Close()
			}
		}

		// 2. Knowledge Indexing
		if !config.IsModuleDisabled("knowledge", nil, projectCfg) {
			docsDir := filepath.Join(projectDir, config.ResolveDocsDir(nil, projectCfg))
			wikiDir := resolveWikiDir("knowledge", projectDir, "")
			_, _ = knowledge.RunIndexPipeline(ctx, docsDir, wikiDir, knowledge.IndexConfig{
				Workers: 4,
			})
		}

		// 3. Memory Cycle
		if !config.IsModuleDisabled("memory", nil, projectCfg) {
			_ = withProjectDir(projectDir, func() error {
				memory.RunProjectCycle(ctx)
				memory.RunUserCycle(ctx)
				return nil
			})
		}

		// 4. Hub Sync
		gs, err := hub.NewGitStore(nil, projectCfg)
		if err == nil {
			_ = gs.Sync()
		}

		// 5. Install IDE rules
		for _, r := range []func(string, string) error{
			knowledge.InstallRule,
			ast.InstallRule,
			hub.InstallRule,
			memory.InstallRule,
		} {
			_ = r(projectDir, resolvedIDE)
		}

		// 6. Sync IDE adapter
		if lf, err := hub.LoadLockfile(filepath.Join(projectDir, brand.LockFileName())); err == nil && lf != nil {
			_ = hub.SyncIDEAdapter(resolvedIDE, lf)
		}

		return textResult("Sync completed successfully.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("update"),
		Description: "Update all installed hub artifacts to their latest version and refresh rules.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input updateInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg, ides := loadProjectLockInfo(projectDir)
		resolvedIDE := config.ResolveProjectIDE(input.IDE, nil, projectCfg, ides)

		reg, err := hub.NewRegistryManager(ctx)
		if err != nil {
			return errResult(fmt.Errorf("registry unavailable: %w", err))
		}

		if err := hub.OnUpdate(ctx, reg, resolvedIDE); err != nil {
			return errResult(err)
		}

		for _, r := range []func(string, string) error{
			knowledge.InstallRule,
			ast.InstallRule,
			hub.InstallRule,
			memory.InstallRule,
		} {
			_ = r(projectDir, resolvedIDE)
		}

		return textResult("Update completed successfully.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("remove"),
		Description: "Uninstall and remove Graphit from the current project.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input removeInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg, ides := loadProjectLockInfo(projectDir)
		resolvedIDE := config.ResolveProjectIDE(input.IDE, nil, projectCfg, ides)

		hm := git.NewHookManager("")
		_ = hm.Remove()

		_, _ = git.RemoveGitignore(filepath.Join(projectDir, ".gitignore"))

		reg, _ := hub.NewRegistryManager(ctx)
		_ = hub.OnRemove(ctx, reg, resolvedIDE)

		for _, r := range []func(string, string) error{
			knowledge.RemoveRule,
			ast.RemoveRule,
			hub.RemoveRule,
			memory.RemoveRule,
		} {
			_ = r(projectDir, resolvedIDE)
		}

		return textResult("Graphit removed from this project successfully.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("config", "set"),
		Description: "Set a configuration key to the specified value globally or locally.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input configSetInput) (*mcp.CallToolResult, any, error) {
		if input.Global {
			if err := config.SetGlobalConfigValue(input.Key, input.Value); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Global config set: %s=%s", input.Key, input.Value))
		}

		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		lp := filepath.Join(projectDir, brand.LockFileName())
		lf, err := hub.LoadLockfile(lp)
		if err != nil || lf == nil {
			return errResult(fmt.Errorf("project not initialized. Run init first"))
		}

		if lf.Config == nil {
			lf.Config = make(config.ConfigMap)
		}
		config.SetConfigValue(lf.Config, input.Key, input.Value)
		if err := hub.SaveLockfile(lp, lf); err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Project config set: %s=%s", input.Key, input.Value))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("config", "get"),
		Description: "Get the value of a configuration key.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input configGetInput) (*mcp.CallToolResult, any, error) {
		if input.Global {
			val, ok, err := config.GetGlobalConfigValue(input.Key)
			if err != nil {
				return errResult(err)
			}
			if !ok {
				return textResult(fmt.Sprintf("Key %q is not set globally.", input.Key))
			}
			return textResult(val)
		}

		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		if projectCfg == nil {
			return errResult(fmt.Errorf("project not initialized"))
		}

		val, ok := config.GetConfigValue(projectCfg, input.Key)
		if !ok {
			return textResult(fmt.Sprintf("Key %q is not set locally.", input.Key))
		}
		return textResult(val)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("config", "unset"),
		Description: "Unset a configuration key.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input configUnsetInput) (*mcp.CallToolResult, any, error) {
		if input.Global {
			if err := config.UnsetGlobalConfigValue(input.Key); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Global key %q unset.", input.Key))
		}

		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		lp := filepath.Join(projectDir, brand.LockFileName())
		lf, err := hub.LoadLockfile(lp)
		if err != nil || lf == nil {
			return errResult(fmt.Errorf("project not initialized"))
		}

		config.UnsetConfigValue(lf.Config, input.Key)
		if err := hub.SaveLockfile(lp, lf); err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Project key %q unset.", input.Key))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("config", "list"),
		Description: "List all configuration keys and their values.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input configListInput) (*mcp.CallToolResult, any, error) {
		var cfg config.ConfigMap
		var err error

		if input.Global {
			cfg, err = config.LoadGlobalConfig()
			if err != nil {
				return errResult(err)
			}
		} else {
			projectDir, err := resolveProjectDir(input.ProjectDir)
			if err != nil {
				return errResult(err)
			}
			cfg = loadProjectConfig(projectDir)
			if cfg == nil {
				return errResult(fmt.Errorf("project not initialized"))
			}
		}

		entries := config.ListConfigEntries(cfg)
		return jsonResult(entries)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("version"),
		Description: "Get the current version of the Graphit CLI and MCP server.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input versionInput) (*mcp.CallToolResult, any, error) {
		return textResult(version.Version)
	}))
}
