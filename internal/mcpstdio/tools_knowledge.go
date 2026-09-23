package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/lancequery"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
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
	PageSize    int    `json:"page_size,omitempty" jsonschema:"Results per page (default: 20, max: 100); top_k remains the total-result cap"`
	Cursor      string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact search"`
	Preview     *bool  `json:"preview,omitempty" jsonschema:"Set to true to include a short text excerpt per hit. Default false: a search answers with titles, and the page is read with wiki_source when the agent decides it needs it"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeSchemaInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context"`
	Table       string `json:"table,omitempty" jsonschema:"Describe only this table; omit for every table"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeQueryInput struct {
	ProjectDir  string   `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to query a globally installed artifact, naming it in context as id@version."`
	Context     string   `json:"context,omitempty" jsonschema:"Named imported context to query"`
	Table       string   `json:"table" jsonschema:"Table to query: chunks for pages, xrefs for the links between them, sync_log for index history, meta for index metadata; knowledge_schema lists them (required)"`
	Filter      string   `json:"filter,omitempty" jsonschema:"Lance SQL predicate over this table's columns, for example \"stale_since != ''\", \"doc_type = 'guide'\" or, on xrefs, \"target_slug = 'storage-layout'\". This is a WHERE clause only: there is no SELECT, JOIN, GROUP BY, aggregate or ORDER BY. Omit to match every row."`
	Columns     []string `json:"columns,omitempty" jsonschema:"Columns to return, for example [slug, title, stale_since]. Omit for every compact column; the page body, its summary and the search terms are excluded unless named, and the embedding vector is never returned as numbers."`
	TopK        int      `json:"top_k,omitempty" jsonschema:"Total row cap across pages (0 = no cap)"`
	PageSize    int      `json:"page_size,omitempty" jsonschema:"Rows per page (default: 20, max: 100); top_k remains the total-result cap"`
	Cursor      string   `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact query"`
	AiOptimized *bool    `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
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

type knowledgeExportInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported knowledge context"`
	Format     string `json:"format" jsonschema:"Export format: package, okf, or obsidian (required)"`
	Output     string `json:"output" jsonschema:"Output file or directory path (required)"`
}

func registerKnowledgeTools(server *mcp.Server) {
	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "index"),
		Description: "Index the documentation tree (knowledge.docs_dir, default docs/) plus the project's root README into the knowledge index and regenerate the wiki. Pass path to index a specific directory wholesale instead.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)

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

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "export"),
		Description: "Export Knowledge as an importable .knowledge package, Open Knowledge Format, or an Obsidian vault.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeExportInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		if input.Output == "" {
			return errResult(fmt.Errorf("output is required"))
		}
		outputPath := input.Output
		if input.Format == "package" {
			outputPath = artifactpackage.EnsureExtension(outputPath, ".knowledge")
		}
		absOutput, err := filepath.Abs(outputPath)
		if err != nil {
			return errResult(err)
		}
		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		switch input.Format {
		case "package":
			if err := knowledge.ExportPackage(ctx, wikiDir, absOutput); err != nil {
				return errResult(err)
			}
		case "okf":
			if _, err := knowledge.ExportOKF(ctx, wikiDir, absOutput, "knowledge"); err != nil {
				return errResult(err)
			}
		case "obsidian":
			if _, err := knowledge.ExportObsidian(ctx, wikiDir, absOutput, "knowledge"); err != nil {
				return errResult(err)
			}
		default:
			return errResult(fmt.Errorf("unsupported format %q (use package, okf, or obsidian)", input.Format))
		}
		return textResult(fmt.Sprintf("Exported successfully to %s", absOutput))
	}))

	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("knowledge", "search"),
		Description: "Search the project knowledge wiki using BM25 keyword ranking. " +
			"Answers with page titles and scores, not page text: pick the page from the titles, then read it with " +
			brand.MCPToolName("wiki", "source") + ", which slices. Pass preview=true only when the titles are not enough to choose. " +
			"Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSearchInput) (*mcp.CallToolResult, any, error) {
		if input.TopK < 0 {
			return errResult(fmt.Errorf("top_k cannot be negative"))
		}
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		window, err := openPage(input.PageSize, input.Cursor, input.TopK, 20, struct {
			Tool, ProjectDir, Context, Query string
			TopK                             int
		}{"knowledge_search", projectDir, input.Context, input.Query, input.TopK})
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
			results = wiki.BM25SearchFrom(ctx, db, input.Query, window.FetchLimit)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		paged := page.Finish(window, results)
		if aiOpt(input.AiOptimized) {
			return textResult(paginationTOON(wiki.FormatBM25ResultsTOON(paged.Results, wantPreview(input.Preview)), paged.NextCursor))
		}
		return jsonResult(paged)
	}))

	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("knowledge", "schema"),
		Description: "Show the knowledge index tables: every column with its type, and the row count. " +
			"Read this before writing a knowledge_query filter. The index is LanceDB, not a graph database: " +
			"pages live in chunks, the links between them in xrefs.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSchemaInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		var only []string
		if table := strings.TrimSpace(input.Table); table != "" {
			only = []string{table}
		}
		var value lancequery.Schema
		err = withProjectDir(projectDir, func() error {
			db, oerr := openWikiForReadContext(ctx, projectDir, "knowledge", input.Context)
			if oerr != nil {
				return oerr
			}
			defer func() { _ = db.Close() }()
			value, oerr = db.DescribeStore(ctx, only)
			return oerr
		})
		if err != nil {
			return errResult(err)
		}
		return lanceSchemaResult(value, input.AiOptimized)
	}))

	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("knowledge", "query"),
		Description: "Answer a structured question about the knowledge index: filter rows by predicate and return only the columns asked for. " +
			"Use it for questions a ranked search cannot answer — which pages are stale, what a page's doc_type is, " +
			"and, over the xrefs table, which pages link to a given slug. " +
			"knowledge_search ranks pages by relevance; wiki_source reads one page's text. " +
			"The filter is a WHERE clause, not SQL: no SELECT, JOIN, GROUP BY or ORDER BY, and results carry no ordering.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		table := strings.TrimSpace(input.Table)
		window, err := openPage(input.PageSize, input.Cursor, input.TopK, page.DefaultPageSize, struct {
			Tool, ProjectDir, Context, Table, Filter string
			Columns                                  []string
			TopK                                     int
		}{"knowledge_query", projectDir, input.Context, table, input.Filter, input.Columns, input.TopK})
		if err != nil {
			return errResult(err)
		}
		var rows []map[string]any
		err = withProjectDir(projectDir, func() error {
			db, oerr := openWikiForReadContext(ctx, projectDir, "knowledge", input.Context)
			if oerr != nil {
				return oerr
			}
			defer func() { _ = db.Close() }()
			result, oerr := db.QueryStore(ctx, lancequery.Request{
				Table:   table,
				Filter:  input.Filter,
				Columns: input.Columns,
				Limit:   window.FetchLimit - window.Offset,
				Offset:  window.Offset,
			})
			rows = result.Rows
			return oerr
		})
		if err != nil {
			return errResult(err)
		}
		return lanceQueryResult(page.FinishFetched(window, rows), input.AiOptimized)
	}))

	addTool(server, &mcp.Tool{
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

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "remove"),
		Description: "Remove the project knowledge index or an imported context.",
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
			if err := store.RemoveContext(projectDir, store.KindKnowledge, cleanCtx); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Knowledge context %q removed.", cleanCtx))
		}

		wikiDir := knowledge.WikiDirFor(projectDir)
		_, lifecycleLock, lockErr := storelifecycle.Acquire(ctx, wikiDir)
		if lockErr != nil {
			return errResult(lockErr)
		}
		defer lifecycleLock.Release()
		if err := os.RemoveAll(wikiDir); err != nil {
			return errResult(err)
		}
		_ = os.MkdirAll(wikiDir, 0o755)
		return textResult("Project knowledge wiki cleared.")
	}))

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "sync"),
		Description: "Rebuild the local project knowledge wiki from its configured documentation scope.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSyncInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		if err := hub.HydrateProjectKnowledgeLance(ctx, projectDir, projectCfg); err != nil {
			return errResult(fmt.Errorf("hydrate published Lance base: %w", err))
		}
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

	addTool(server, &mcp.Tool{
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
