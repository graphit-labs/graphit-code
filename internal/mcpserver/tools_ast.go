package mcpserver

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
)

type astQueryInput struct {
	Query      string `json:"query" jsonschema:"Cypher query to execute against the AST graph database"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to query instead of the default project"`
}

type astFTSInput struct {
	Query      string `json:"query" jsonschema:"Keywords to search for using BM25 full-text search"`
	TopK       int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search"`
}

type astSemanticInput struct {
	Query      string `json:"query" jsonschema:"Natural language query for semantic vector search"`
	TopK       int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
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
		Name:        "graphit_ast_query",
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
		defer db.Close()

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
		Name:        "graphit_ast_search_fts",
		Description: "Perform a BM25 full-text search across all code entities and files in the AST graph. Returns results ranked by relevance.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input astFTSInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer db.Close()

		topK := input.TopK

		qs := ast.NewQueryService(db)
		results, err := qs.FullTextSearch(ctx, input.Query, topK)
		if err != nil {
			return errResult(fmt.Errorf("full-text search failed: %w", err))
		}

		data, err := json.MarshalIndent(results, "", "  ")
		if err != nil {
			return errResult(fmt.Errorf("marshal results: %w", err))
		}
		return textResult(string(data))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_ast_search_semantic",
		Description: "Perform a semantic vector similarity search over the AST graph using natural language. Requires an embedding model.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input astSemanticInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openASTDB(projectDir, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer db.Close()

		topK := input.TopK

		embClient, err := ai.NewEmbeddingClientFromConfig()
		if err != nil {
			return errResult(fmt.Errorf("embedding client init failed: %w", err))
		}

		qs := ast.NewQueryService(db)
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
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_ast_query_ai",
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
		defer db.Close()

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
		Name:        "graphit_ast_schema",
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
		defer db.Close()

		schemaText, err := ast.SchemaText(ctx, db)
		if err != nil {
			return errResult(fmt.Errorf("schema retrieval failed: %w", err))
		}
		return textResult(schemaText)
	})
}
