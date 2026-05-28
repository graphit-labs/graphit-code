package mcpstdio

import (
	"context"
	"fmt"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/wikisvc"
)

type wikiSearchInput struct {
	Query      string   `json:"query" jsonschema:"Natural language question to search across multiple wikis"`
	Wikis      []string `json:"wikis,omitempty" jsonschema:"Wiki sources to search (project, memory, or project IDs from ecosystem)"`
	HubRefs    []string `json:"hub_refs,omitempty" jsonschema:"Hub knowledge artifact references to include (format: artifact-id@version)"`
	SessionID  string   `json:"session_id,omitempty" jsonschema:"Session ID to continue an existing conversation"`
	TopK       int      `json:"top_k,omitempty" jsonschema:"BM25 results per wiki source (0 = no limit)"`
	ProjectDir string   `json:"project_dir" jsonschema:"Project directory (required)"`
}

type wikiChatInput struct {
	SessionID string `json:"session_id" jsonschema:"Chat session ID to continue"`
	Message   string `json:"message" jsonschema:"User message to send"`
}

type wikiSessionsInput struct {
	Action     string `json:"action" jsonschema:"Action: list or delete"`
	SessionID  string `json:"session_id,omitempty" jsonschema:"Session ID for delete action"`
	ProjectDir string `json:"project_dir" jsonschema:"Project directory for listing (required)"`
}

func registerWikiTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "search"),
		Description: "Search across multiple wiki sources using AI-powered retrieval.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var result *wikisvc.WikiSearchResult
		err = withProjectDir(projectDir, func() error {
			wikiSvc := wikisvc.NewWikiService(projectDir)
			var werr error
			result, werr = wikiSvc.SearchMultiWiki(ctx, wikisvc.WikiSearchOpts{
				Query:   input.Query,
				Wikis:   input.Wikis,
				HubRefs: input.HubRefs,
				TopK:    input.TopK,
			})
			return werr
		})
		if err != nil {
			return errResult(err)
		}

		var b strings.Builder
		b.WriteString(result.Answer)
		_, _ = fmt.Fprintf(&b, "\n\n---\nSession ID: %s (use graphit_wiki_chat to continue this conversation)", result.SessionID)

		return textResult(b.String())
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "chat"),
		Description: "Continue a wiki chat session started by graphit_wiki_search.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiChatInput) (*mcp.CallToolResult, any, error) {
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
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "sessions"),
		Description: "List or delete wiki chat sessions.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiSessionsInput) (*mcp.CallToolResult, any, error) {
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

			var sessions []*chat.ChatSession
			err = withProjectDir(projectDir, func() error {
				wikiSvc := wikisvc.NewWikiService(projectDir)
				var serr error
				sessions, serr = wikiSvc.ListSessions()
				return serr
			})
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
	}))
}
