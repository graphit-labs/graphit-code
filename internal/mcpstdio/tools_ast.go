package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/memory"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	"github.com/graphit-labs/graphit-code/internal/store"
)

type astIndexInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory to index (required)"`
	Path        string `json:"path,omitempty" jsonschema:"Target path to index (defaults to project_dir)"`
	Workers     int    `json:"workers,omitempty" jsonschema:"Number of parallel worker threads"`
	Reset       bool   `json:"reset,omitempty" jsonschema:"Delete the whole store before indexing — graph, search index and caches — discarding every embedding. Prefer reindex, which re-parses everything but keeps them"`
	Reindex     bool   `json:"reindex,omitempty" jsonschema:"Force reindexing of unchanged files"`
	Cluster     string `json:"cluster,omitempty" jsonschema:"Optional cluster label for grouping"`
	NoSource    bool   `json:"no_source,omitempty" jsonschema:"Do not index file source contents"`
	Grammar     string `json:"grammar,omitempty" jsonschema:"Override grammar per extension (comma-separated: .ext=grammar-name, e.g. .sql=antlr-plsql,.pks=antlr-plsql)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type astQueryInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to query a globally installed artifact, naming it in context as id@version."`
	Query       string `json:"query" jsonschema:"Cypher query to execute against the AST graph database"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context to query instead of the default project"`
	PageSize    int    `json:"page_size,omitempty" jsonschema:"Results per page (default: 20, max: 100); independent of any LIMIT in the Cypher query"`
	Cursor      string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact query"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type astSchemaInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to query a globally installed artifact, naming it in context as id@version."`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type astInstallInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Path        string `json:"path" jsonschema:"Absolute path to the source project to import (required)"`
	Context     string `json:"context" jsonschema:"Name of the context to assign to the imported project (required)"`
	Reset       bool   `json:"reset,omitempty" jsonschema:"Reset the context database before importing"`
	Workers     int    `json:"workers,omitempty" jsonschema:"Number of parallel worker threads"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type astRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Name of the imported context to remove. If empty, clears the main project graph."`
}

type astListInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to list the artifacts installed globally."`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type astSourceInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to query a globally installed artifact, naming it in context as id@version."`
	Path        string `json:"path" jsonschema:"Relative path to the file (required)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context where the file resides"`
	Entity      string `json:"entity,omitempty" jsonschema:"Entity name (function, class, etc.) to extract source using its line range from the graph"`
	EntityType  string `json:"entity_type,omitempty" jsonschema:"Entity type for disambiguation: Function, Class, Method, Struct, etc."`
	Head        int    `json:"head,omitempty" jsonschema:"Show only the first N lines"`
	Tail        int    `json:"tail,omitempty" jsonschema:"Show only the last N lines"`
	StartLine   int    `json:"start_line,omitempty" jsonschema:"Start line number (1-indexed)"`
	EndLine     int    `json:"end_line,omitempty" jsonschema:"End line number (1-indexed, inclusive)"`
	Pattern     string `json:"pattern,omitempty" jsonschema:"Search for a pattern (literal text or regex if regex=true)"`
	IsRegex     bool   `json:"regex,omitempty" jsonschema:"Treat pattern as a regular expression"`
	Before      int    `json:"before,omitempty" jsonschema:"Number of context lines before each pattern match"`
	After       int    `json:"after,omitempty" jsonschema:"Number of context lines after each pattern match"`
	LineNumbers bool   `json:"line_numbers,omitempty" jsonschema:"Include line numbers in the output (default: false)"`
}

type astExportInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Format     string `json:"format" jsonschema:"Export format: obsidian or bundle (required)"`
	Output     string `json:"output" jsonschema:"Output directory path where files will be exported (required)"`
	NoSources  bool   `json:"no_sources,omitempty" jsonschema:"Do not include file source contents in bundle"`
}

type astEmbedInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type astSearchInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to query a globally installed artifact, naming it in context as id@version."`
	Query       string `json:"query" jsonschema:"Search query (keywords, natural language, or code identifiers)"`
	TopK        int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (default: 15)"`
	Mode        string `json:"mode,omitempty" jsonschema:"Search mode: hybrid (default, combines BM25 + semantic via RRF), fts (BM25 only), semantic (vector only)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context to search"`
	PageSize    int    `json:"page_size,omitempty" jsonschema:"Results per page (max: 100); top_k remains the total-result cap"`
	Cursor      string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact search"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

func registerASTTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "index"),
		Description: "Index files in the project to build the AST code graph database. Call this once at the end of a session in which you changed code, so the graph the next query reads is current.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if store.IsEphemeralProject(projectDir) {
			return errResult(errEphemeralHasNoGraph())
		}

		target := input.Path
		if target == "" {
			target = projectDir
		} else if !filepath.IsAbs(target) {
			target = filepath.Join(projectDir, target)
		}

		absPath, err := filepath.Abs(target)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)

		ladybugCfg := astConfigForProject(projectDir, "")

		if input.Reset {
			_ = os.RemoveAll(ladybugCfg.StoreDir)
		}

		db, err := openASTDBReadWrite(projectDir, "")
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		workers := input.Workers
		if workers <= 0 {
			workers = 4
		}

		indexSource := config.ResolveIndexSource(nil, projectCfg)
		if input.NoSource {
			indexSource = false
		}

		grammarOverrides := config.ResolveGrammarOverrides(nil, projectCfg)
		if input.Grammar != "" {
			flagOverrides := config.ParseGrammarOverrides(input.Grammar)
			grammarOverrides = config.MergeGrammarOverrides(grammarOverrides, flagOverrides)
		}

		revEdges := config.ResolveHubIcebugReverseEdges(nil, projectCfg)
		pipeOpts := ast.PipelineOptions{
			Workers:          workers,
			IndexSource:      indexSource,
			CacheDir:         ladybugCfg.StoreDir,
			Cluster:          input.Cluster,
			ForceRebuild:     input.Reindex,
			ReverseEdges:     &revEdges,
			GrammarOverrides: grammarOverrides,
		}

		result, err := ast.RunPipeline(ctx, db, absPath, pipeOpts)
		if err != nil {
			return errResult(err)
		}

		if aiOpt(input.AiOptimized) {
			return toonResult(result)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "query"),
		Description: "Execute a Cypher query against the AST code graph database. Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		window, err := openPage(input.PageSize, input.Cursor, 0, 20, struct {
			Tool, ProjectDir, Context, Query string
		}{"ast_query", projectDir, input.Context, input.Query})
		if err != nil {
			return errResult(err)
		}

		var paged page.Page[ast.QueryRecord]
		if pager, ok := db.(ast.QueryPager); ok {
			result, qerr := pager.QueryPage(ctx, input.Query, nil, window.Offset, window.PageSize+1)
			if qerr != nil {
				return errResult(qerr)
			}
			paged = page.FinishFetched(window, result.Records)
		} else {
			result, qerr := db.Query(ctx, input.Query, nil)
			if qerr != nil {
				return errResult(qerr)
			}
			paged = page.Finish(window, result.Records)
		}

		if aiOpt(input.AiOptimized) {
			return textResult(paginationTOON(ast.FormatRecordsTOON(paged.Results), paged.NextCursor))
		}
		return jsonResult(paged)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "schema"),
		Description: "Return the AST graph database schema: node labels, properties, and relationship types. Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSchemaInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		schemaText, err := ast.SchemaText(ctx, db)
		if err != nil {
			return errResult(err)
		}

		return textResult(schemaText)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "install"),
		Description: "Import another local repository's code graph as a named context. The graph is built once in the global store and shared; the project records that it may query it.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astInstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		absSource, err := filepath.Abs(input.Path)
		if err != nil {
			return errResult(err)
		}

		ictx, err := ast.AddImportedContext(projectDir, input.Context, absSource)
		if err != nil {
			return errResult(err)
		}

		if input.Reset {
			_ = os.RemoveAll(ictx.StoreDir)
		}

		db, err := openASTDBReadWrite(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		workers := input.Workers
		if workers <= 0 {
			workers = 4
		}

		projectCfg := loadProjectConfig(projectDir)
		revEdges := config.ResolveHubIcebugReverseEdges(nil, projectCfg)
		pipeOpts := ast.PipelineOptions{
			Workers:          workers,
			IndexSource:      true,
			CacheDir:         ictx.StoreDir,
			ReverseEdges:     &revEdges,
			GrammarOverrides: config.ResolveGrammarOverrides(nil, projectCfg),
		}

		result, err := ast.RunPipeline(ctx, db, absSource, pipeOpts)
		if err != nil {
			return errResult(err)
		}

		ms, msErr := memory.NewMemoryStore()
		if msErr == nil {
			memsvc := memory.NewMemoryServiceForContext(input.Context, ms)
			_ = memsvc.SyncWiki()
			_ = memsvc.Close()
		}

		if aiOpt(input.AiOptimized) {
			return toonResult(result)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "remove"),
		Description: "Remove an imported context or clear the main project code graph. Removing a context drops this project's claim on it; the shared store stays for whoever else imported it.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if input.Context != "" {
			if err := ast.RemoveImportedContext(projectDir, input.Context); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Imported context %q removed.", input.Context))
		}

		db, err := openASTDBReadWrite(projectDir, "")
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		if _, err := db.Execute(ctx, `MATCH (n) DETACH DELETE n`, nil); err != nil {
			return errResult(err)
		}

		return textResult("Project code graph cleared.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "list"),
		Description: "List all imported AST contexts and their repository paths. Without project_dir, lists the artifacts installed globally, which are the ones a project-less caller can query.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		contexts := ast.ListImportedContextsIn(projectDir)
		if aiOpt(input.AiOptimized) {
			return toonResult(contexts)
		}
		return jsonResult(contexts)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "source"),
		Description: "Retrieve source code from the indexed code graph with support for head/tail, line ranges, entity extraction, and pattern search with context. This is the only way to read the source of an imported context or another project: the graph and its file text live in the global store, not in any project directory. Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSourceInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		svc := ast.NewSourceService(db).
			WithStore(astConfigForProject(projectDir, input.Context).StoreDir)
		result, err := svc.GetSource(ctx, ast.SourceRequest{
			Path:        input.Path,
			Entity:      input.Entity,
			EntityType:  input.EntityType,
			Head:        input.Head,
			Tail:        input.Tail,
			StartLine:   input.StartLine,
			EndLine:     input.EndLine,
			Pattern:     input.Pattern,
			IsRegex:     input.IsRegex,
			Before:      input.Before,
			After:       input.After,
			LineNumbers: input.LineNumbers,
		})
		if err != nil {
			return errResult(err)
		}

		if result.Source == "" && len(result.Matches) == 0 {
			return textResult(fmt.Sprintf("No matches found for pattern %q in %s", input.Pattern, input.Path))
		}

		return textResult(result.Source)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "export"),
		Description: "Export the AST database to Obsidian markdown format or an archive bundle.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astExportInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, "")
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		absDir, err := filepath.Abs(input.Output)
		if err != nil {
			return errResult(err)
		}

		switch input.Format {
		case "obsidian":
			exporter := ast.NewObsidianExporter(db, projectDir)
			if err := exporter.Export(ctx, absDir); err != nil {
				return errResult(err)
			}
		case "bundle":
			opts := ast.BundleOptions{
				StorePath: astConfigForProject(projectDir, "").StoreDir,
				NoSources: input.NoSources,
			}
			if err := ast.ExportBundle(ctx, db, projectDir, absDir, opts, nil); err != nil {
				return errResult(err)
			}
		default:
			return errResult(fmt.Errorf("unsupported format %q (use obsidian or bundle)", input.Format))
		}

		return textResult(fmt.Sprintf("Exported successfully to %s", absDir))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "embed"),
		Description: "Run embedding cycle to precompute or update semantic embeddings.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astEmbedInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var count int
		err = withProjectDir(projectDir, func() error {
			embCfg := ast.DefaultEmbeddingConfig()
			var ladybugCfg ast.LadybugConfig
			if input.Context != "" {
				ladybugCfg = ast.LadybugConfigForContextIn(projectDir, input.Context)
				embCfg.RepoRoot = ast.ListImportedContextsIn(projectDir)[input.Context].SourcePath
			} else {
				ladybugCfg = ast.LadybugConfigFor(projectDir)
				embCfg.RepoRoot = projectDir
			}
			cacheDir := ladybugCfg.StoreDir

			parseCache, cacheErr := ast.NewShardCache(cacheDir)
			if cacheErr != nil {
				return cacheErr
			}
			parseCache.SetRoot(embCfg.RepoRoot)
			embCfg.ParseCache = parseCache

			idx, idxErr := ast.OpenSearchIndex(ctx, cacheDir)
			if idxErr != nil {
				return idxErr
			}
			defer func() { _ = idx.Close() }()
			embCfg.Index = idx

			if embCache, embErr := ast.NewShardEmbCache(cacheDir, parseCache); embErr == nil {
				embCfg.EmbCache = embCache
				defer func() { _ = embCache.Close() }()
			}

			embClient, err := ai.NewEmbeddingClientFromConfig()
			if err != nil {
				return err
			}

			embedder := ast.NewEmbedder(embClient, embCfg)
			var rerr error
			count, rerr = embedder.RunCycle(ctx)
			return rerr
		})
		if err != nil {
			return errResult(err)
		}

		return textResult(fmt.Sprintf("%d entities embedded successfully.", count))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "search"),
		Description: "Hybrid search combining BM25 full-text and semantic vector search with Reciprocal Rank Fusion (RRF). Supports three modes: hybrid (default, best results), fts (keyword only), semantic (vector only). Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		if input.TopK < 0 {
			return errResult(fmt.Errorf("top_k cannot be negative"))
		}
		topK := input.TopK
		if topK == 0 {
			topK = 15
		}

		mode := input.Mode
		if mode == "" {
			mode = "hybrid"
		}
		window, err := openPage(input.PageSize, input.Cursor, topK, topK, struct {
			Tool, ProjectDir, Context, Query, Mode string
			TopK                                   int
		}{"ast_search", projectDir, input.Context, input.Query, mode, topK})
		if err != nil {
			return errResult(err)
		}

		qs := ast.NewQueryService(db)
		defer qs.Close()

		switch mode {
		case "fts":
			results, err := qs.FullTextSearch(ctx, input.Query, window.FetchLimit)
			if err != nil {
				return errResult(err)
			}
			paged := page.Finish(window, results)
			if aiOpt(input.AiOptimized) {
				return textResult(paginationTOON(ast.FormatSearchResultsTOON(paged.Results), paged.NextCursor))
			}
			return jsonResult(paged)

		case "semantic":
			embClient, err := ai.NewEmbeddingClientFromConfig()
			if err != nil {
				return errResult(err)
			}
			qs.SetEmbeddingClient(embClient)
			results, err := qs.SemanticSearch(ctx, input.Query, window.FetchLimit, "")
			if err != nil {
				return errResult(err)
			}
			paged := page.Finish(window, results)
			if aiOpt(input.AiOptimized) {
				return textResult(paginationTOON(ast.FormatSearchResultsTOON(paged.Results), paged.NextCursor))
			}
			return jsonResult(paged)

		default:
			embClient, embErr := ai.NewEmbeddingClientFromConfig()
			if embErr == nil {
				qs.SetEmbeddingClient(embClient)
			}
			results, err := qs.HybridSearch(ctx, input.Query, window.FetchLimit)
			if err != nil {
				return errResult(err)
			}
			paged := page.Finish(window, results)
			if aiOpt(input.AiOptimized) {
				return textResult(paginationTOON(ast.FormatSearchResultsTOON(paged.Results), paged.NextCursor))
			}
			return jsonResult(paged)
		}
	}))
}
