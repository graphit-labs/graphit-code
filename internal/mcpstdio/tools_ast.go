package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancequery"
	"github.com/graphit-labs/graphit-code/internal/memory"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
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

type astFTSSchemaInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to describe a globally installed artifact, naming it in context as id@version."`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context"`
	Table       string `json:"table,omitempty" jsonschema:"Describe only this table; omit for both"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type astFTSQueryInput struct {
	ProjectDir  string   `json:"project_dir,omitempty" jsonschema:"Project directory. Omit to query a globally installed artifact, naming it in context as id@version."`
	Context     string   `json:"context,omitempty" jsonschema:"Named imported context to query"`
	Table       string   `json:"table" jsonschema:"Table to query: entities for indexed symbols, files for indexed files; ast_fts_schema lists their columns (required)"`
	Filter      string   `json:"filter,omitempty" jsonschema:"Lance SQL predicate over this table's columns, for example \"path = 'internal/task/service.go'\", \"etype = 'Function' AND is_dep = false\" or \"name LIKE 'Open%'\". This is a WHERE clause only: there is no SELECT, JOIN, GROUP BY, aggregate or ORDER BY. Omit to match every row."`
	Columns     []string `json:"columns,omitempty" jsonschema:"Columns to return, for example [name, etype, path, line]. Omit for every compact column. The body and source columns are never returned and cannot be filtered on: they hold synthesised BM25 documents, not code — use ast_search in fts mode to match against them and ast_source to read real code. The embedding vector is never returned as numbers either."`
	TopK        int      `json:"top_k,omitempty" jsonschema:"Total row cap across pages (0 = no cap)"`
	PageSize    int      `json:"page_size,omitempty" jsonschema:"Rows per page (default: 20, max: 100); top_k remains the total-result cap"`
	Cursor      string   `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact query"`
	AiOptimized *bool    `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
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
	Format     string `json:"format" jsonschema:"Export format: obsidian or package (required)"`
	Output     string `json:"output" jsonschema:"Output file or directory path (required)"`
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
	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "index"),
		Description: "Build or repair the project's AST code graph when it is absent or explicit reindexing is needed. The daemon normally indexes edits; adapter stop hooks dispatch final sync. Do not duplicate that work after each edit or session.",
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
			lockedCtx, lifecycleLock, lockErr := storelifecycle.Acquire(ctx, ladybugCfg.StoreDir)
			if lockErr != nil {
				return errResult(lockErr)
			}
			defer lifecycleLock.Release()
			ctx = lockedCtx
			if err := os.RemoveAll(ladybugCfg.StoreDir); err != nil {
				return errResult(err)
			}
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

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "query"),
		Description: "Execute a Cypher query against the AST code graph database. Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDBWithContext(ctx, projectDir, input.Context)
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

	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("ast", "schema"),
		Description: "Return the AST GRAPH schema: node labels, properties, and relationship types, for writing Cypher with " +
			brand.MCPToolName("ast", "query") + ". " +
			"The graph is one of two stores: for the columns of the full-text tables, use " +
			brand.MCPToolName("ast", "fts", "schema") + " instead. " +
			"Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSchemaInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDBWithContext(ctx, projectDir, input.Context)
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

	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("ast", "fts", "schema"),
		Description: "Show the AST full-text tables: every column with its type, and the row count. " +
			"Read this before writing an " + brand.MCPToolName("ast", "fts", "query") + " filter. " +
			"These are LanceDB tables, a different store from the Cypher graph that " +
			brand.MCPToolName("ast", "schema") + " describes.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astFTSSchemaInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		index, err := openASTSearchIndex(ctx, projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = index.Close() }()

		var only []string
		if table := strings.TrimSpace(input.Table); table != "" {
			only = []string{table}
		}
		value, err := index.DescribeStore(ctx, only)
		if err != nil {
			return errResult(err)
		}
		return lanceSchemaResult(value, input.AiOptimized)
	}))

	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("ast", "fts", "query"),
		Description: "Answer a structured question about the indexed code: filter rows by predicate and return only the columns asked for. " +
			"Use it for questions ranking cannot answer — every entity in a file, how many of a kind exist, what is project code and what is a dependency. " +
			brand.MCPToolName("ast", "search") + " ranks by relevance; " + brand.MCPToolName("ast", "source") + " reads code. " +
			"This is the LanceDB side; " + brand.MCPToolName("ast", "query") + " is Cypher over the graph. " +
			"The filter is a WHERE clause, not SQL: no SELECT, JOIN, GROUP BY or ORDER BY, and results carry no ordering.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astFTSQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		table := strings.TrimSpace(input.Table)
		window, err := openPage(input.PageSize, input.Cursor, input.TopK, page.DefaultPageSize, struct {
			Tool, ProjectDir, Context, Table, Filter string
			Columns                                  []string
			TopK                                     int
		}{"ast_fts_query", projectDir, input.Context, table, input.Filter, input.Columns, input.TopK})
		if err != nil {
			return errResult(err)
		}
		index, err := openASTSearchIndex(ctx, projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = index.Close() }()

		result, err := index.QueryStore(ctx, lancequery.Request{
			Table:   table,
			Filter:  input.Filter,
			Columns: input.Columns,
			Limit:   window.FetchLimit - window.Offset,
			Offset:  window.Offset,
		})
		if err != nil {
			return errResult(err)
		}
		return lanceQueryResult(page.FinishFetched(window, result.Rows), input.AiOptimized)
	}))

	addTool(server, &mcp.Tool{
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
			lockedCtx, lifecycleLock, lockErr := storelifecycle.Acquire(ctx, ictx.StoreDir)
			if lockErr != nil {
				return errResult(lockErr)
			}
			defer lifecycleLock.Release()
			ctx = lockedCtx
			if err := os.RemoveAll(ictx.StoreDir); err != nil {
				return errResult(err)
			}
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
			memsvc := memory.NewMemoryServiceForContext(input.Context, ms).WithContext(ctx)
			_ = memsvc.IndexMemories(ctx)
			_ = memsvc.Close()
		}

		if aiOpt(input.AiOptimized) {
			return toonResult(result)
		}
		return jsonResult(result)
	}))

	addTool(server, &mcp.Tool{
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

		ladybugCfg := astConfigForProject(projectDir, "")
		lockedCtx, lifecycleLock, lockErr := storelifecycle.Acquire(ctx, ladybugCfg.StoreDir)
		if lockErr != nil {
			return errResult(lockErr)
		}
		defer lifecycleLock.Release()
		ctx = lockedCtx

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

	addTool(server, &mcp.Tool{
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

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "source"),
		Description: "Retrieve source code from the indexed code graph with support for head/tail, line ranges, entity extraction, and pattern search with context. This is the only way to read the source of an imported context or another project: the graph and its file text live in the global store, not in any project directory. Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSourceInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDBWithContext(ctx, projectDir, input.Context)
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

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "export"),
		Description: "Export the AST database to an Obsidian vault or an importable .ast package.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astExportInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		if input.Output == "" {
			return errResult(fmt.Errorf("output is required"))
		}

		outputPath := input.Output
		if input.Format == "package" {
			outputPath = artifactpackage.EnsureExtension(outputPath, ".ast")
		}
		absDir, err := filepath.Abs(outputPath)
		if err != nil {
			return errResult(err)
		}

		switch input.Format {
		case "obsidian":
			db, err := openASTDBWithContext(ctx, projectDir, "")
			if err != nil {
				return errResult(err)
			}
			defer func() { _ = db.Close() }()
			exporter := ast.NewObsidianExporter(db, projectDir)
			if err := exporter.Export(ctx, absDir); err != nil {
				return errResult(err)
			}
		case "package":
			if err := ast.ExportPackage(astConfigForProject(projectDir, "").StoreDir, absDir); err != nil {
				return errResult(err)
			}
		default:
			return errResult(fmt.Errorf("unsupported format %q (use obsidian or package)", input.Format))
		}

		return textResult(fmt.Sprintf("Exported successfully to %s", absDir))
	}))

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "embed"),
		Description: "Run embedding cycle to precompute or update semantic embeddings.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astEmbedInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var count int
		var deferred bool
		err = withProjectDir(projectDir, func() error {
			var ladybugCfg ast.LadybugConfig
			var repoRoot string
			if input.Context != "" {
				ladybugCfg = ast.LadybugConfigForContextIn(projectDir, input.Context)
				repoRoot = ast.ListImportedContextsIn(projectDir)[input.Context].SourcePath
			} else {
				ladybugCfg = ast.LadybugConfigFor(projectDir)
				repoRoot = projectDir
			}
			var rerr error
			count, deferred, rerr = ast.RunEmbeddingCycleOnce(ctx, ladybugCfg.StoreDir, repoRoot, ai.NewEmbeddingClientFromConfig, nil)
			return rerr
		})
		if err != nil {
			return errResult(err)
		}
		if deferred {
			return textResult("Embedding deferred while the AST store is updating.")
		}

		return textResult(fmt.Sprintf("%d entities embedded successfully.", count))
	}))

	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "search"),
		Description: "Hybrid search combining BM25 full-text and semantic vector search with Reciprocal Rank Fusion (RRF). Supports three modes: hybrid (default, best results), fts (keyword only), semantic (vector only). Without project_dir, pass the globally installed artifact's qualified identifier (id@version) as context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveArtifactScope(input.ProjectDir, input.Context)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDBWithContext(ctx, projectDir, input.Context)
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

		qs := ast.NewQueryServiceWithContext(ctx, db)
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

// openASTSearchIndex resolves the LanceDB full-text index of a project or an imported context.
//
// It mirrors openASTDBWithContext, which resolves the Cypher graph, and the two land in
// different places: the graph is graph.icebug, the index is search.lance, both under the same
// store directory. Passing the store directory rather than the index path is deliberate —
// OpenSearchIndex appends search.lance itself, and opening the parent as a store would make
// LanceDB report the index as if it were a table called `search`.
func openASTSearchIndex(ctx context.Context, projectDir, contextName string) (*ast.SearchIndex, error) {
	storeDir := store.ASTProjectDir(projectDir)
	if strings.TrimSpace(contextName) != "" {
		storeDir = store.ASTContextDirIn(projectDir, contextName)
	}
	if storeDir == "" {
		return nil, fmt.Errorf("no AST store for %s — it has not been indexed yet", projectDir)
	}
	return ast.OpenSearchIndex(ctx, storeDir)
}
