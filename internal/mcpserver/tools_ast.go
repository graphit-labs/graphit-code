package mcpserver

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

type astQueryInput struct {
	Query      string `json:"query" jsonschema:"Cypher query to execute against the AST graph database"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to query instead of the default project"`
}

type astSearchInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Query      string `json:"query" jsonschema:"Search query (keywords, natural language, or code identifiers)"`
	TopK       int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (default: 15)"`
	Mode       string `json:"mode,omitempty" jsonschema:"Search mode: hybrid (default, combines BM25 + semantic via RRF), fts (BM25 only), semantic (vector only)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search"`
}

type astAIQueryInput struct {
	Query      string `json:"query" jsonschema:"Natural language question about the codebase to convert to Cypher"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to query"`
}

type astSchemaInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type astSourceInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
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

func textResult(text string) (*mcp.CallToolResult, any, error) {
	return &mcp.CallToolResult{
		Content: []mcp.Content{&mcp.TextContent{Text: text}},
	}, nil, nil
}

func errResult(err error) (*mcp.CallToolResult, any, error) {
	return nil, nil, err
}

func registerASTTools(server *mcp.Server) {

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "query"),
		Description: "Execute a Cypher query against the AST code graph database. Returns matching records as JSON.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input astQueryInput) (*mcp.CallToolResult, any, error) {
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
			return errResult(fmt.Errorf("cypher query failed: %w", err))
		}

		if len(result.Records) == 0 {
			return textResult("No results.")
		}

		data, err := json.MarshalIndent(result.Records, "", "  ")
		if err != nil {
			return errResult(fmt.Errorf("marshal results: %w", err))
		}
		return textResult(string(data))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "search"),
		Description: "Hybrid search combining BM25 full-text and semantic vector search with Reciprocal Rank Fusion (RRF). Supports three modes: hybrid (default, best results), fts (keyword only), semantic (vector only).",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input astSearchInput) (*mcp.CallToolResult, any, error) {
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
			topK = 15
		}

		qs := ast.NewQueryService(db)
		defer qs.Close()

		switch input.Mode {
		case "fts":
			results, err := qs.FullTextSearch(ctx, input.Query, topK)
			if err != nil {
				return errResult(fmt.Errorf("full-text search failed: %w", err))
			}
			data, err := json.MarshalIndent(results, "", "  ")
			if err != nil {
				return errResult(fmt.Errorf("marshal results: %w", err))
			}
			return textResult(string(data))

		case "semantic":
			embClient, err := ai.NewEmbeddingClientFromConfig()
			if err != nil {
				return errResult(fmt.Errorf("embedding client init failed: %w", err))
			}
			qs.SetEmbeddingClient(embClient)
			results, err := qs.SemanticSearch(ctx, input.Query, topK, "")
			if err != nil {
				return errResult(fmt.Errorf("semantic search failed: %w", err))
			}
			data, err := json.MarshalIndent(results, "", "  ")
			if err != nil {
				return errResult(fmt.Errorf("marshal results: %w", err))
			}
			return textResult(string(data))

		default: // hybrid
			embClient, embErr := ai.NewEmbeddingClientFromConfig()
			if embErr == nil {
				qs.SetEmbeddingClient(embClient)
			}
			results, err := qs.HybridSearch(ctx, input.Query, topK)
			if err != nil {
				return errResult(fmt.Errorf("hybrid search failed: %w", err))
			}
			data, err := json.MarshalIndent(results, "", "  ")
			if err != nil {
				return errResult(fmt.Errorf("marshal results: %w", err))
			}
			return textResult(string(data))
		}
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "query_ai"),
		Description: "Convert a natural language question about the codebase into a Cypher query using AI, execute it, and return both the generated query and results.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input astAIQueryInput) (*mcp.CallToolResult, any, error) {
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
			return errResult(fmt.Errorf("AI client init failed: %w", err))
		}

		resp, err := ast.GenerateAICypher(ctx, db, aiClient, ast.AICypherRequest{
			UserQuery:  input.Query,
			MaxResults: 25,
			Backend:    db.BackendType(),
		})
		if err != nil {
			return errResult(fmt.Errorf("AI cypher generation failed: %w", err))
		}

		data, err := json.MarshalIndent(resp, "", "  ")
		if err != nil {
			return errResult(fmt.Errorf("marshal results: %w", err))
		}
		return textResult(string(data))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "schema"),
		Description: "Return the AST graph database schema: node labels, properties, and relationship types.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input astSchemaInput) (*mcp.CallToolResult, any, error) {
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
			return errResult(fmt.Errorf("schema retrieval failed: %w", err))
		}
		return textResult(schemaText)
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("ast", "source"),
		Description: "Retrieve source code from the indexed code graph with support for head/tail, line ranges, entity extraction, and pattern search with context.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input astSourceInput) (*mcp.CallToolResult, any, error) {
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
	})
}
