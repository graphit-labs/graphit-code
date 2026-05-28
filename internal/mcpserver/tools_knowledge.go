package mcpserver

import (
	"context"
	"fmt"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"github.com/graphit-labs/graphit-code/internal/wikisvc"
)

type knowledgeQueryInput struct {
	Query      string `json:"query" jsonschema:"Natural language question to search the project knowledge wiki"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search instead of the default project"`
}

type knowledgeSearchInput struct {
	Query      string `json:"query" jsonschema:"Keywords to search for in the knowledge wiki using BM25"`
	TopK       int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search"`
}

type wikiSearchInput struct {
	Query      string   `json:"query" jsonschema:"Natural language question to search across multiple wikis"`
	Wikis      []string `json:"wikis,omitempty" jsonschema:"Wiki sources to search (project, memory, or project IDs from ecosystem)"`
	HubRefs    []string `json:"hub_refs,omitempty" jsonschema:"Hub knowledge artifact references to include (format: artifact-id@version)"`
	SessionID  string   `json:"session_id,omitempty" jsonschema:"Session ID to continue an existing conversation"`
	TopK       int      `json:"top_k,omitempty" jsonschema:"BM25 results per wiki source (0 = no limit)"`
	ProjectDir string   `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
}

type wikiChatInput struct {
	SessionID string `json:"session_id" jsonschema:"Chat session ID to continue"`
	Message   string `json:"message" jsonschema:"User message to send"`
}

type wikiSessionsInput struct {
	Action     string `json:"action" jsonschema:"Action: list or delete"`
	SessionID  string `json:"session_id,omitempty" jsonschema:"Session ID for delete action"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory for listing"`
}

func registerKnowledgeTools(server *mcp.Server) {

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "query"),
		Description: "Search the project knowledge wiki using AI-powered retrieval. Returns a synthesized answer based on project documentation, architecture, and decisions.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		if wikiDir == "" {
			return errResult(fmt.Errorf("knowledge wiki not found — run 'graphit knowledge index' first"))
		}

		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return errResult(fmt.Errorf("AI not configured: %w", err))
		}

		result, err := wiki.SearchWiki(ctx, aiClient, input.Query, wiki.SearchConfig{
			WikiDir:   wikiDir,
			ModuleTag: "knowledge",
			UseBM25:   true,
		})
		if err != nil {
			return errResult(err)
		}

		return textResult(result.Answer)
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "search"),
		Description: "Search the project knowledge wiki using BM25 keyword ranking. Returns matching pages with relevance scores and snippets. Does not require AI — fast lexical search.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		if wikiDir == "" {
			return errResult(fmt.Errorf("knowledge wiki not found — run 'graphit knowledge index' first"))
		}

		topK := input.TopK

		results := wiki.BM25Search(wikiDir, input.Query, topK)
		if len(results) == 0 {
			return textResult(fmt.Sprintf("No results found for %q in the knowledge wiki.", input.Query))
		}

		var b strings.Builder
		_, _ = fmt.Fprintf(&b, "Found %d results for %q:\n\n", len(results), input.Query)
		for i, r := range results {
			_, _ = fmt.Fprintf(&b, "%d. %s", i+1, strings.TrimSuffix(r.Path, ".md"))
			if r.Title != "" {
				_, _ = fmt.Fprintf(&b, " — %s", r.Title)
			}
			_, _ = fmt.Fprintf(&b, " (score: %.3f)\n", r.Score)
			if r.Snippet != "" {
				_, _ = fmt.Fprintf(&b, "   %s\n", r.Snippet)
			}
		}

		return textResult(b.String())
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "search"),
		Description: "Search across multiple wiki sources using AI-powered retrieval. Supports project wiki, memory wiki, ecosystem project wikis, and hub knowledge artifacts. Returns a synthesized answer and creates a chat session for follow-up questions.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input wikiSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiSvc := wikisvc.NewWikiService(projectDir)
		result, err := wikiSvc.SearchMultiWiki(ctx, wikisvc.WikiSearchOpts{
			Query:   input.Query,
			Wikis:   input.Wikis,
			HubRefs: input.HubRefs,
			TopK:    input.TopK,
		})
		if err != nil {
			return errResult(err)
		}

		var b strings.Builder
		b.WriteString(result.Answer)
		_, _ = fmt.Fprintf(&b, "\n\n---\nSession ID: %s (use graphit_wiki_chat to continue this conversation)", result.SessionID)

		return textResult(b.String())
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "chat"),
		Description: "Continue a wiki chat session started by graphit_wiki_search. Send follow-up questions in the same conversation context.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input wikiChatInput) (*mcp.CallToolResult, any, error) {
		if input.SessionID == "" {
			return errResult(fmt.Errorf("session_id is required"))
		}
		if input.Message == "" {
			return errResult(fmt.Errorf("message is required"))
		}

		wikiSvc := wikisvc.NewWikiService("")
		response, err := wikiSvc.ContinueChat(ctx, input.SessionID, input.Message)
		if err != nil {
			return errResult(err)
		}

		return textResult(response)
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "sessions"),
		Description: "List or delete wiki chat sessions. Use action 'list' to see all sessions for a project, or 'delete' to remove a specific session.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input wikiSessionsInput) (*mcp.CallToolResult, any, error) {
		switch input.Action {
		case "delete":
			if input.SessionID == "" {
				return errResult(fmt.Errorf("session_id is required for delete action"))
			}
			wikiSvc := wikisvc.NewWikiService("")
			if err := wikiSvc.DeleteSession(input.SessionID); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Session %s deleted.", input.SessionID))

		case "list":
			projectDir, err := resolveProjectDir(input.ProjectDir)
			if err != nil {
				return errResult(err)
			}

			wikiSvc := wikisvc.NewWikiService(projectDir)
			sessions, err := wikiSvc.ListSessions()
			if err != nil {
				return textResult("No sessions found.")
			}
			if len(sessions) == 0 {
				return textResult("No sessions found.")
			}

			var b strings.Builder
			_, _ = fmt.Fprintf(&b, "Found %d session(s):\n\n", len(sessions))
			for i, s := range sessions {
				_, _ = fmt.Fprintf(&b, "%d. [%s] %s\n", i+1, s.ID, s.Title)
				_, _ = fmt.Fprintf(&b, "   Created: %s | Updated: %s | Messages: %d\n",
					s.CreatedAt.Format("2006-01-02 15:04"),
					s.UpdatedAt.Format("2006-01-02 15:04"),
					s.MessageCount)
				if len(s.WikiSources) > 0 {
					srcNames := make([]string, len(s.WikiSources))
					for j, ws := range s.WikiSources {
						srcNames[j] = ws.Label
					}
					_, _ = fmt.Fprintf(&b, "   Sources: %s\n", strings.Join(srcNames, ", "))
				}
			}
			return textResult(b.String())

		default:
			return errResult(fmt.Errorf("unknown action %q — use 'list' or 'delete'", input.Action))
		}
	})
}


