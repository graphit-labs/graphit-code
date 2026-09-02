package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type knowledgeIndexInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory to index (required)"`
	Path        string `json:"path,omitempty" jsonschema:"Index this directory wholesale instead of the configured scope. Omit to index knowledge.docs_dir (default docs/) plus the root README."`
	Workers     int    `json:"workers,omitempty" jsonschema:"Number of parallel worker threads"`
	Reset       bool   `json:"reset,omitempty" jsonschema:"Clear graph and re-index from scratch"`
	UseLouvain  bool   `json:"louvain,omitempty" jsonschema:"Use Louvain community detection"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeSearchInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to search a globally installed artifact, naming it in context as id@version."`
	Query       string `json:"query" jsonschema:"Keywords to search for in the knowledge wiki using BM25"`
	TopK        int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context to search"`
	Preview     *bool  `json:"preview,omitempty" jsonschema:"Set to true to include a short text excerpt per hit. Default false: a search answers with titles, and the page is read with wiki_source when the agent decides it needs it"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeSchemaInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type knowledgeLintInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	StaleDays   int    `json:"stale_days,omitempty" jsonschema:"Mark pages older than N days as stale"`
	Context     string `json:"context,omitempty" jsonschema:"Lint an imported context by name"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Name of the imported context to remove. If empty, clears local project knowledge wiki."`
}

type knowledgeSyncInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
}

type knowledgeListInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context     string `json:"context,omitempty" jsonschema:"Named installed knowledge context to list"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

func registerKnowledgeTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "index"),
		Description: "Index the documentation tree (knowledge.docs_dir, default docs/) plus the project's root README into the knowledge graph and regenerate the wiki. Pass path to index a specific directory wholesale instead.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)

		// No path: the configured docs tree plus the root README, scoped under the
		// project so every reported source path resolves from the project root. An
		// explicit path is taken literally and indexed wholesale.
		root := projectDir
		scope := knowledge.ScopeFor(projectDir, nil, projectCfg)
		if input.Path != "" {
			root = input.Path
			if !filepath.IsAbs(root) {
				root = filepath.Join(projectDir, root)
			}
			scope = knowledge.WikiScope{}
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		cfg := knowledge.IndexConfig{
			Workers:    input.Workers,
			Reset:      input.Reset,
			BatchSize:  100,
			UseLouvain: input.UseLouvain,
			ProjectCfg: projectCfg,
			Scope:      scope,
		}

		var result *knowledge.IndexResult
		err = withProjectDir(projectDir, func() error {
			var ierr error
			result, ierr = knowledge.RunIndexPipeline(ctx, root, wikiDir, cfg)
			return ierr
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
		Name: brand.MCPToolName("knowledge", "search"),
		Description: "Search the project knowledge wiki using BM25 keyword ranking. " +
			"Answers with page titles and scores, not page text: pick the page from the titles, then read it with " +
			brand.MCPToolName("wiki", "source") + ", which slices. Pass preview=true only when the titles are not enough to choose. " +
			"Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		var results []wiki.BM25Result
		err = withProjectDir(projectDir, func() error {
			db, oerr := openWikiForReadContext(ctx, projectDir, "knowledge", input.Context)
			if oerr != nil {
				return oerr
			}
			defer func() { _ = db.Close() }()
			results = wiki.BM25SearchFrom(ctx, db, input.Query, input.TopK)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return textResult(wiki.FormatBM25ResultsTOON(results, wantPreview(input.Preview)))
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

		cfg := wiki.LintConfig{StaleDays: input.StaleDays}
		if cfg.StaleDays <= 0 {
			cfg.StaleDays = 30
		}

		var report *wiki.LintReport
		err = withProjectDir(projectDir, func() error {
			db, oerr := openWikiForReadContext(ctx, projectDir, "knowledge", input.Context)
			if oerr != nil {
				return oerr
			}
			defer func() { _ = db.Close() }()
			report, oerr = wiki.LintWikiFrom(ctx, db, input.Context, cfg)
			return oerr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(report)
		}
		return jsonResult(report)
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
			cleanCtx, err := sanitizeContextName(input.Context)
			if err != nil {
				return errResult(err)
			}
			// Only this project's claim is dropped. The wiki is global and another
			// project may have imported the same context.
			if err := store.RemoveContext(projectDir, store.KindKnowledge, cleanCtx); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Knowledge context %q removed.", cleanCtx))
		}

		wikiDir := knowledge.WikiDirFor(projectDir)
		if err := os.RemoveAll(wikiDir); err != nil {
			return errResult(err)
		}
		_ = os.MkdirAll(wikiDir, 0o755)
		return textResult("Project knowledge wiki cleared.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "sync"),
		Description: "Rebuild the local project knowledge wiki from its configured documentation scope.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSyncInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		cfg := knowledge.IndexConfig{
			Workers:    4,
			ProjectCfg: projectCfg,
			Scope:      knowledge.ScopeFor(projectDir, nil, projectCfg),
		}
		var result *knowledge.IndexResult
		err = withProjectDir(projectDir, func() error {
			var ierr error
			result, ierr = knowledge.RunIndexPipeline(ctx, projectDir, wikiDir, cfg)
			return ierr
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "list"),
		Description: "List all articles in the project knowledge wiki or a named installed context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var articles []string
		err = withProjectDir(projectDir, func() error {
			db, oerr := openWikiForReadContext(ctx, projectDir, "knowledge", input.Context)
			if oerr != nil {
				return oerr
			}
			defer func() { _ = db.Close() }()
			articles = wiki.ListPagesFrom(ctx, db)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(articles)
		}
		return jsonResult(articles)
	}))
}

// noKnowledgeToSearch explains an empty source set, which has three different causes
// and only one of them is a mistake.
