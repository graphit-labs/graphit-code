package mcpstdio

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type knowledgeIndexInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory to index (required)"`
	Path       string `json:"path,omitempty" jsonschema:"Target path to index (defaults to docs/)"`
	Workers    int    `json:"workers,omitempty" jsonschema:"Number of parallel worker threads"`
	Reset      bool   `json:"reset,omitempty" jsonschema:"Clear graph and re-index from scratch"`
	UseLouvain bool   `json:"louvain,omitempty" jsonschema:"Use Louvain community detection"`
}

type knowledgeQueryInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query      string `json:"query" jsonschema:"Natural language question to search the project knowledge wiki"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search instead of the default project"`
}

type knowledgeSearchInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query      string `json:"query" jsonschema:"Keywords to search for in the knowledge wiki using BM25"`
	TopK       int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search"`
}

type knowledgeSchemaInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type knowledgeLintInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Deep       bool   `json:"deep,omitempty" jsonschema:"Enable AI-assisted contradiction detection"`
	Fix        bool   `json:"fix,omitempty" jsonschema:"Auto-repair fixable issues (backlinks)"`
	StaleDays  int    `json:"stale_days,omitempty" jsonschema:"Mark pages older than N days as stale"`
	Context    string `json:"context,omitempty" jsonschema:"Lint an imported context by name"`
}

type knowledgeInstallInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Name       string `json:"name" jsonschema:"Name of the knowledge context to import from the hub"`
}

type knowledgeRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Name of the imported context to remove. If empty, clears local project knowledge wiki."`
}

type knowledgeSyncInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Sync a specific imported context by name. If empty, syncs local docs/ index."`
}

type knowledgeExportInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
}

type knowledgeListInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
}

func registerKnowledgeTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "index"),
		Description: "Index docs/ into the knowledge graph and regenerate the wiki.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		path := input.Path
		if path == "" {
			path = filepath.Join(projectDir, config.ResolveDocsDir(nil, projectCfg))
		} else if !filepath.IsAbs(path) {
			path = filepath.Join(projectDir, path)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		cfg := knowledge.IndexConfig{
			Workers:    input.Workers,
			Reset:      input.Reset,
			BatchSize:  100,
			UseLouvain: input.UseLouvain,
		}

		var result *knowledge.IndexResult
		err = withProjectDir(projectDir, func() error {
			var ierr error
			result, ierr = knowledge.RunIndexPipeline(ctx, path, wikiDir, cfg)
			return ierr
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "query"),
		Description: "Search the project knowledge wiki using AI-powered retrieval.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		if wikiDir == "" {
			return errResult(fmt.Errorf("knowledge wiki not found"))
		}

		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return errResult(err)
		}

		var result *wiki.SearchResult
		err = withProjectDir(projectDir, func() error {
			var qerr error
			result, qerr = wiki.SearchWiki(ctx, aiClient, input.Query, wiki.SearchConfig{
				WikiDir:   wikiDir,
				ModuleTag: "knowledge",
				UseBM25:   true,
			})
			return qerr
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(result.Answer)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "search"),
		Description: "Search the project knowledge wiki using BM25 keyword ranking.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		if wikiDir == "" {
			return errResult(fmt.Errorf("knowledge wiki not found"))
		}

		var results []wiki.BM25Result
		err = withProjectDir(projectDir, func() error {
			results = wiki.BM25Search(wikiDir, input.Query, input.TopK)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(results)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "schema"),
		Description: "Show the knowledge graph schema and node properties.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSchemaInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		schemaText := fmt.Sprintf("KNOWLEDGE Wiki\nWiki directory: %s\nArchitecture: file-based wiki (no graph database)", wikiDir)
		return textResult(schemaText)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "lint"),
		Description: "Audit the knowledge wiki for structural issues.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeLintInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		if wikiDir == "" {
			return errResult(fmt.Errorf("knowledge wiki not found"))
		}

		cfg := wiki.LintConfig{
			Deep:      input.Deep,
			Fix:       input.Fix,
			StaleDays: input.StaleDays,
		}
		if cfg.StaleDays <= 0 {
			cfg.StaleDays = 30
		}

		var report *wiki.LintReport
		err = withProjectDir(projectDir, func() error {
			var lerr error
			report, lerr = wiki.LintWiki(wikiDir, cfg)
			return lerr
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(report)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "install"),
		Description: "Import an external knowledge context from the hub.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeInstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		gs, err := hub.NewGitStore(nil, projectCfg)
		if err != nil {
			return errResult(fmt.Errorf("hub not configured: %w", err))
		}

		branch := fmt.Sprintf("knowledge/project/%s", input.Name)
		knowledge.EnsureContextCopy(input.Name)
		wikiDir := knowledge.WikiDirForContext(input.Name)
		globalCtxBase := filepath.Dir(wikiDir)
		localDocsDir := filepath.Join(globalCtxBase, "docs")

		err = withProjectDir(projectDir, func() error {
			if err := gs.ExtractBranchDir(branch, "docs", localDocsDir); err != nil {
				return fmt.Errorf("fetching from hub: %w", err)
			}
			cfg := knowledge.IndexConfig{
				Workers:   4,
				BatchSize: 100,
			}
			_, err = knowledge.RunIndexPipeline(ctx, localDocsDir, wikiDir, cfg)
			if err != nil {
				return err
			}
			memStore, _ := memory.NewMemoryGitStore()
			memory.OnHubImport(ctx, input.Name, projectDir, memStore, nil)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Knowledge context %q imported successfully.", input.Name))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "remove"),
		Description: "Remove the project knowledge graph or an imported context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if input.Context != "" {
			linkDir := filepath.Join(projectDir, brand.DotDir(), "knowledge", input.Context)
			if err := os.RemoveAll(linkDir); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Knowledge context %q removed.", input.Context))
		}

		wikiDir := knowledge.WikiDir()
		err = withProjectDir(projectDir, func() error {
			if err := os.RemoveAll(wikiDir); err != nil {
				return err
			}
			_ = os.MkdirAll(wikiDir, 0o755)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return textResult("Project knowledge wiki cleared.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "sync"),
		Description: "Re-sync an imported context from the global cache or rebuild local wiki.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSyncInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if input.Context != "" {
			projectCfg := loadProjectConfig(projectDir)
			gs, err := hub.NewGitStore(nil, projectCfg)
			if err != nil {
				return errResult(err)
			}
			branch := fmt.Sprintf("knowledge/project/%s", input.Context)
			knowledge.EnsureContextCopy(input.Context)
			wikiDir := knowledge.WikiDirForContext(input.Context)
			globalCtxBase := filepath.Dir(wikiDir)
			localDocsDir := filepath.Join(globalCtxBase, "docs")
			err = withProjectDir(projectDir, func() error {
				if err := gs.ExtractBranchDir(branch, "docs", localDocsDir); err != nil {
					return err
				}
				cfg := knowledge.IndexConfig{Workers: 4, BatchSize: 100}
				_, err = knowledge.RunIndexPipeline(ctx, localDocsDir, wikiDir, cfg)
				return err
			})
			if err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Sync context %q complete.", input.Context))
		}

		projectCfg := loadProjectConfig(projectDir)
		docsDir := filepath.Join(projectDir, config.ResolveDocsDir(nil, projectCfg))
		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		cfg := knowledge.IndexConfig{Workers: 4}
		var result *knowledge.IndexResult
		err = withProjectDir(projectDir, func() error {
			var ierr error
			result, ierr = knowledge.RunIndexPipeline(ctx, docsDir, wikiDir, cfg)
			return ierr
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "export"),
		Description: "Export the project knowledge wiki and graph to the hub.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeExportInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		gs, err := hub.NewGitStore(nil, projectCfg)
		if err != nil {
			return errResult(err)
		}

		lf, err := hub.LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
		if err != nil || lf == nil {
			return errResult(fmt.Errorf("project not initialised"))
		}

		branch := fmt.Sprintf("knowledge/project/%s", lf.Project.ID)
		var wt *hub.MemoryWorktree
		err = withProjectDir(projectDir, func() error {
			var werr error
			wt, werr = gs.MemoryWorktree(branch)
			return werr
		})
		if err != nil {
			return errResult(err)
		}

		docsDir := config.ResolveDocsDir(nil, projectCfg)
		docsSrc := filepath.Join(projectDir, docsDir)
		destDocs := filepath.Join(wt.Dir(), "docs")
		_ = os.RemoveAll(destDocs)
		if err := copyDirRecursive(docsSrc, destDocs); err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		if _, err := os.Stat(wikiDir); err == nil {
			destWiki := filepath.Join(wt.Dir(), "wiki")
			_ = os.RemoveAll(destWiki)
			_ = copyDirRecursive(wikiDir, destWiki)
		}

		if err := wt.CommitAndPush("export knowledge"); err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Knowledge exported to branch %s", branch))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "list"),
		Description: "List all articles in the local knowledge wiki.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var articles []string
		err = withProjectDir(projectDir, func() error {
			wikiDir := resolveWikiDir("knowledge", projectDir, "")
			entries, err := os.ReadDir(wikiDir)
			if err != nil {
				return err
			}
			for _, e := range entries {
				if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
					continue
				}
				name := strings.TrimSuffix(e.Name(), ".md")
				if name == "index" || name == "log" {
					continue
				}
				articles = append(articles, name)
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(articles)
	}))
}

func copyDirRecursive(src, dst string) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
}
