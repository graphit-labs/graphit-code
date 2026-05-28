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
)

type astIndexInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory to index (required)"`
	Path       string `json:"path,omitempty" jsonschema:"Target path to index (defaults to project_dir)"`
	Workers    int    `json:"workers,omitempty" jsonschema:"Number of parallel worker threads"`
	Reset      bool   `json:"reset,omitempty" jsonschema:"Reset database before indexing"`
	Reindex    bool   `json:"reindex,omitempty" jsonschema:"Force reindexing of unchanged files"`
	Cluster    string `json:"cluster,omitempty" jsonschema:"Optional cluster label for grouping"`
	NoSource   bool   `json:"no_source,omitempty" jsonschema:"Do not index file source contents"`
}

type astQueryInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query       string `json:"query" jsonschema:"Cypher query to execute against the AST graph database"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context to query instead of the default project"`
	AiOptimized bool   `json:"ai_optimized,omitempty" jsonschema:"Optimize the Cypher query execution for AI context"`
}

type astFTSInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query      string `json:"query" jsonschema:"Keywords to search for using BM25 full-text search"`
	TopK       int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search"`
}

type astSemanticInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query      string `json:"query" jsonschema:"Natural language query for semantic vector search"`
	TopK       int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search"`
}

type astAIQueryInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query      string `json:"query" jsonschema:"Natural language question about the codebase to convert to Cypher"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to query"`
}

type astSchemaInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type astInstallInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Path       string `json:"path" jsonschema:"Absolute path to the source project to import (required)"`
	Context    string `json:"context" jsonschema:"Name of the context to assign to the imported project (required)"`
	Reset      bool   `json:"reset,omitempty" jsonschema:"Reset the context database before importing"`
	Workers    int    `json:"workers,omitempty" jsonschema:"Number of parallel worker threads"`
}

type astRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Name of the imported context to remove. If empty, clears the main project graph."`
}

type astListInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
}

type astSourceInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
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
	LineNumbers bool   `json:"line_numbers,omitempty" jsonschema:"Include line numbers in the output"`
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

func registerASTTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "index"),
		Description: "Index files in the project to build the AST code graph database.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
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

		if input.Reset {
			ladybugCfg := ast.DefaultLadybugConfig()
			_ = os.RemoveAll(filepath.Dir(ladybugCfg.DBPath))
		}

		db, err := openASTDBReadWrite(projectDir, "")
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		if input.Reindex && !input.Reset {
			writer := ast.NewGraphWriter(db, absPath, true)
			_ = writer.DeleteRepository(ctx, absPath)
		}

		workers := input.Workers
		if workers <= 0 {
			workers = 4
		}

		indexSource := config.ResolveIndexSource(nil, projectCfg)
		if input.NoSource {
			indexSource = false
		}

		ladybugCfg := ast.DefaultLadybugConfig()
		pipeOpts := ast.PipelineOptions{
			Workers:      workers,
			IndexSource:  indexSource,
			CacheDir:     filepath.Dir(ladybugCfg.DBPath),
			Cluster:      input.Cluster,
			ForceRebuild: input.Reindex,
		}

		result, err := ast.RunPipeline(ctx, db, absPath, pipeOpts)
		if err != nil {
			return errResult(err)
		}

		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "query"),
		Description: "Execute a Cypher query against the AST code graph database.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		result, err := db.Query(ctx, input.Query, nil)
		if err != nil {
			return errResult(err)
		}

		return jsonResult(result.Records)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "search_fts"),
		Description: "Perform a BM25 full-text search across all code entities and files in the AST graph.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astFTSInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		topK := input.TopK
		if topK <= 0 {
			topK = 25
		}

		qs := ast.NewQueryService(db)
		results, err := qs.FullTextSearch(ctx, input.Query, topK)
		if err != nil {
			return errResult(err)
		}

		return jsonResult(results)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "search_semantic"),
		Description: "Perform a semantic vector similarity search over the AST graph using natural language.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSemanticInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		topK := input.TopK
		if topK <= 0 {
			topK = 10
		}

		embClient, err := ai.NewEmbeddingClientFromConfig()
		if err != nil {
			return errResult(err)
		}

		qs := ast.NewQueryService(db)
		qs.SetEmbeddingClient(embClient)

		results, err := qs.SemanticSearch(ctx, input.Query, topK, "")
		if err != nil {
			return errResult(err)
		}

		return jsonResult(results)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "query_ai"),
		Description: "Convert a natural language question about the codebase into a Cypher query using AI, execute it, and return results.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astAIQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return errResult(err)
		}

		resp, err := ast.GenerateAICypher(ctx, db, aiClient, ast.AICypherRequest{
			UserQuery:  input.Query,
			MaxResults: 25,
			Backend:    db.BackendType(),
		})
		if err != nil {
			return errResult(err)
		}

		return jsonResult(resp)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "schema"),
		Description: "Return the AST graph database schema: node labels, properties, and relationship types.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSchemaInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
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
		Description: "Import another local repository's code graph as a named context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astInstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		absSource, err := filepath.Abs(input.Path)
		if err != nil {
			return errResult(err)
		}

		ictx, err := ast.AddImportedContext(input.Context, absSource)
		if err != nil {
			return errResult(err)
		}

		if input.Reset {
			_ = os.RemoveAll(filepath.Dir(ictx.DBPath))
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

		pipeOpts := ast.PipelineOptions{
			Workers:     workers,
			IndexSource: true,
			CacheDir:    filepath.Dir(ictx.DBPath),
		}

		result, err := ast.RunPipeline(ctx, db, absSource, pipeOpts)
		if err != nil {
			return errResult(err)
		}

		// Sync memory context
		ms, msErr := memory.NewMemoryGitStore()
		if msErr == nil {
			memsvc := memory.NewMemoryServiceForContext(input.Context, ms)
			_ = memsvc.SyncToLocal()
			_ = memsvc.Close()
		}

		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "remove"),
		Description: "Remove an imported context or clear the main project code graph.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if input.Context != "" {
			if err := ast.RemoveImportedContext(input.Context); err != nil {
				return errResult(err)
			}
			memDir := filepath.Join(projectDir, brand.DotDir(), "memory", input.Context)
			if info, statErr := os.Lstat(memDir); statErr == nil {
				if info.Mode()&os.ModeSymlink != 0 {
					_ = os.Remove(memDir)
				} else {
					_ = os.RemoveAll(memDir)
				}
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
		Description: "List all imported AST contexts and their repository paths.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astListInput) (*mcp.CallToolResult, any, error) {
		_, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		contexts := ast.ListImportedContexts()
		return jsonResult(contexts)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "source"),
		Description: "Retrieve source code from the indexed code graph with support for head/tail, line ranges, entity extraction, and pattern search with context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input astSourceInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		svc := ast.NewSourceService(db)
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
			if err := ast.ExportBundle(ctx, db, projectDir, absDir); err != nil {
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
				ladybugCfg = ast.LadybugConfigForContext(input.Context)
			} else {
				ladybugCfg = ast.DefaultLadybugConfig()
			}
			cacheDir := filepath.Dir(ladybugCfg.DBPath)

			parseCache, cacheErr := ast.NewShardCache(cacheDir)
			if cacheErr != nil {
				return cacheErr
			}
			embCfg.ParseCache = parseCache

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
}
