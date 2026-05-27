package mcpserver

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/wiki"
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
		Name:        "graphit_knowledge_query",
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
		Name:        "graphit_knowledge_search",
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
		Name:        "graphit_wiki_search",
		Description: "Search across multiple wiki sources using AI-powered retrieval. Supports project wiki, memory wiki, ecosystem project wikis, and hub knowledge artifacts. Returns a synthesized answer and creates a chat session for follow-up questions.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input wikiSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var sources []wiki.WikiSource

		for _, w := range input.Wikis {
			src, err := resolveWikiSource(w, projectDir)
			if err != nil {
				continue
			}
			sources = append(sources, src)
		}

		for _, ref := range input.HubRefs {
			src, err := resolveHubKnowledgeSource(ctx, ref)
			if err != nil {
				return errResult(fmt.Errorf("hub knowledge %q: %w", ref, err))
			}
			sources = append(sources, src)
		}

		if len(sources) == 0 {
			return errResult(fmt.Errorf("no valid wiki sources found — specify wikis or hub_refs"))
		}

		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return errResult(fmt.Errorf("AI not configured: %w", err))
		}

		topK := input.TopK

		result, err := wiki.SearchMultiWiki(ctx, aiClient, input.Query, wiki.MultiWikiSearchConfig{
			Sources:           sources,
			UseBM25:           true,
			BM25TopNPerSource: topK,
		})
		if err != nil {
			return errResult(err)
		}

		chatSources := make([]chat.WikiSource, len(sources))
		for i, s := range sources {
			chatSources[i] = chat.WikiSource{ID: s.ID, Label: s.Label, Dir: s.Dir}
		}
		session := chat.NewSession(projectDir, chatSources, input.Query)

		_ = session.Append(chat.ChatMessage{
			Role:    "user",
			Content: input.Query,
		})
		_ = session.Append(chat.ChatMessage{
			Role:    "assistant",
			Content: result.Answer,
		})

		var b strings.Builder
		b.WriteString(result.Answer)
		_, _ = fmt.Fprintf(&b, "\n\n---\nSession ID: %s (use graphit_wiki_chat to continue this conversation)", session.ID)

		return textResult(b.String())
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_wiki_chat",
		Description: "Continue a wiki chat session started by graphit_wiki_search. Send follow-up questions in the same conversation context.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input wikiChatInput) (*mcp.CallToolResult, any, error) {
		if input.SessionID == "" {
			return errResult(fmt.Errorf("session_id is required"))
		}
		if input.Message == "" {
			return errResult(fmt.Errorf("message is required"))
		}

		session, err := chat.LoadSession(input.SessionID)
		if err != nil {
			return errResult(fmt.Errorf("session not found: %w", err))
		}

		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return errResult(fmt.Errorf("AI not configured: %w", err))
		}

		engine := chat.NewChatEngine(aiClient, session)
		response, err := engine.Send(ctx, input.Message)
		if err != nil {
			return errResult(fmt.Errorf("chat error: %w", err))
		}

		return textResult(response)
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_wiki_sessions",
		Description: "List or delete wiki chat sessions. Use action 'list' to see all sessions for a project, or 'delete' to remove a specific session.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input wikiSessionsInput) (*mcp.CallToolResult, any, error) {
		switch input.Action {
		case "delete":
			if input.SessionID == "" {
				return errResult(fmt.Errorf("session_id is required for delete action"))
			}
			if err := chat.DeleteSession(input.SessionID); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Session %s deleted.", input.SessionID))

		case "list":
			projectDir, err := resolveProjectDir(input.ProjectDir)
			if err != nil {
				return errResult(err)
			}

			sessions, err := chat.ListSessions(projectDir)
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

func resolveWikiSource(name, projectDir string) (wiki.WikiSource, error) {
	switch name {
	case "project":
		dir := filepath.Join(projectDir, brand.DotDir(), "knowledge", "project")
		if _, err := os.Stat(dir); err != nil {
			wikiSub := filepath.Join(dir, "wiki")
			if _, err := os.Stat(wikiSub); err == nil {
				dir = wikiSub
			}
		}
		if _, err := os.Stat(dir); err != nil {
			return wiki.WikiSource{}, fmt.Errorf("project knowledge wiki not found at %s", dir)
		}
		return wiki.WikiSource{
			ID:    "project",
			Label: filepath.Base(projectDir),
			Dir:   dir,
		}, nil

	case "memory":
		dir := filepath.Join(projectDir, brand.DotDir(), "memory", "project")
		if _, err := os.Stat(dir); err != nil {
			wikiSub := filepath.Join(dir, "wiki")
			if _, err := os.Stat(wikiSub); err == nil {
				dir = wikiSub
			}
		}
		if _, err := os.Stat(dir); err != nil {
			return wiki.WikiSource{}, fmt.Errorf("project memory wiki not found at %s", dir)
		}
		return wiki.WikiSource{
			ID:    "memory",
			Label: "Memory (project)",
			Dir:   dir,
		}, nil

	default:
		return resolveEcosystemWikiSource(name)
	}
}

func resolveEcosystemWikiSource(projectID string) (wiki.WikiSource, error) {
	lockMgr, err := hub.NewGlobalLockManager()
	if err != nil {
		return wiki.WikiSource{}, fmt.Errorf("cannot access global lock: %w", err)
	}

	projects, err := lockMgr.ListActiveProjects()
	if err != nil {
		return wiki.WikiSource{}, fmt.Errorf("cannot list ecosystem projects: %w", err)
	}

	for _, p := range projects {
		if p.ID == projectID {
			dir := filepath.Join(p.Dir, brand.DotDir(), "knowledge", "project")
			if _, err := os.Stat(dir); err != nil {
				wikiSub := filepath.Join(dir, "wiki")
				if _, err := os.Stat(wikiSub); err == nil {
					dir = wikiSub
				}
			}
			if _, err := os.Stat(dir); err != nil {
				return wiki.WikiSource{}, fmt.Errorf("wiki not found for project %s at %s", projectID, dir)
			}
			return wiki.WikiSource{
				ID:    projectID,
				Label: filepath.Base(p.Dir),
				Dir:   dir,
			}, nil
		}
	}

	return wiki.WikiSource{}, fmt.Errorf("project %q not found in ecosystem — check global.lock.json", projectID)
}

func resolveHubKnowledgeSource(ctx context.Context, ref string) (wiki.WikiSource, error) {
	reg, err := hub.NewRegistryManager(ctx)
	if err != nil {
		return wiki.WikiSource{}, fmt.Errorf("hub registry not available: %w", err)
	}

	hubSvc := hub.NewHubService(reg)

	wikiDir, err := hubSvc.EnsureKnowledgeAvailable(ctx, ref)
	if err != nil {
		return wiki.WikiSource{}, err
	}

	artifactID := ref
	if parts := strings.SplitN(ref, "@", 2); len(parts) == 2 {
		artifactID = parts[0]
	}

	return wiki.WikiSource{
		ID:    "hub/" + artifactID,
		Label: artifactID,
		Dir:   wikiDir,
	}, nil
}
