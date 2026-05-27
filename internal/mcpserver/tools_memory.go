package mcpserver

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type memoryQueryInput struct {
	Query      string `json:"query" jsonschema:"Natural language question to search the memory wiki"`
	Scope      string `json:"scope,omitempty" jsonschema:"Memory scope: project (default) or user"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context to search"`
}

type memoryListInput struct {
	Scope      string `json:"scope,omitempty" jsonschema:"Memory scope: project (default) or user"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
}

type memoryAddInput struct {
	Title      string `json:"title" jsonschema:"Short descriptive title for the memory"`
	Content    string `json:"content" jsonschema:"Detailed content/body of the memory"`
	Type       string `json:"type,omitempty" jsonschema:"Memory type: convention or correction or decision or tension or fact or skill"`
	Scope      string `json:"scope,omitempty" jsonschema:"Memory scope: project (default) or user"`
	Important  bool   `json:"important,omitempty" jsonschema:"Mark as important (included in IDE rules)"`
	Tags       string `json:"tags,omitempty" jsonschema:"Comma-separated tags"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
}

type memoryRemoveInput struct {
	ID         string `json:"id" jsonschema:"Memory ID (slug) to remove"`
	Scope      string `json:"scope,omitempty" jsonschema:"Memory scope: project (default) or user"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
}

type memorySearchInput struct {
	Query      string `json:"query" jsonschema:"Text to search for in memory files"`
	Scope      string `json:"scope,omitempty" jsonschema:"Memory scope: project (default) or user"`
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory (defaults to server working directory)"`
}

func resolveMemoryWikiDir(scope, projectDir, contextName string) string {
	if contextName != "" {
		return resolveWikiDir("memory", projectDir, contextName)
	}
	origWd, _ := os.Getwd()
	_ = os.Chdir(projectDir)
	defer func() { _ = os.Chdir(origWd) }()

	if scope == "user" {
		return memory.WikiDir("user")
	}
	return memory.WikiDir("project")
}

func registerMemoryTools(server *mcp.Server) {

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_query",
		Description: "Search the memory wiki using AI-powered retrieval. Returns a synthesized answer based on stored memories (conventions, decisions, corrections, etc.).",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input memoryQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return nil, nil, err
		}

		wikiDir := resolveMemoryWikiDir(input.Scope, projectDir, input.Context)
		if wikiDir == "" {
			scopeLabel := input.Scope
			if scopeLabel == "" {
				scopeLabel = "project"
			}
			return nil, nil, fmt.Errorf("memory not initialized for %s scope — run 'graphit init' first", scopeLabel)
		}

		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return nil, nil, fmt.Errorf("AI not configured: %w", err)
		}

		result, err := wiki.SearchWiki(ctx, aiClient, input.Query, wiki.SearchConfig{
			WikiDir:   wikiDir,
			ModuleTag: "memory",
		})
		if err != nil {
			return nil, nil, err
		}

		return textResult(result.Answer)
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_list",
		Description: "List all stored memories. Returns memory entries with ID, title, creation date, importance, and type.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input memoryListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return nil, nil, err
		}

		userScope := input.Scope == "user"
		svc, err := newMemorySvc(userScope, projectDir)
		if err != nil {
			return nil, nil, err
		}
		defer func() { _ = svc.Close() }()

		memories, err := svc.ListMemories()
		if err != nil {
			return nil, nil, err
		}

		if len(memories) == 0 {
			return textResult("No memories found.")
		}

		out, _ := json.MarshalIndent(memories, "", "  ")
		return textResult(string(out))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_add",
		Description: "Add a new memory entry (convention, correction, decision, tension, fact, or skill). Memories persist across sessions and are available to all agents.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input memoryAddInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return nil, nil, err
		}

		if input.Type != "" && !memory.ValidMemoryType(input.Type) {
			return nil, nil, fmt.Errorf("invalid memory type %q — valid types: convention, correction, decision, tension, fact, skill", input.Type)
		}

		userScope := input.Scope == "user"
		svc, err := newMemorySvc(userScope, projectDir)
		if err != nil {
			return nil, nil, err
		}
		defer func() { _ = svc.Close() }()

		var tagList []string
		if input.Tags != "" {
			for _, t := range strings.Split(input.Tags, ",") {
				t = strings.TrimSpace(t)
				if t != "" {
					tagList = append(tagList, t)
				}
			}
		}

		slug, err := svc.AddMemory(input.Title, input.Content, memory.MemoryOpts{
			Important: input.Important,
			Type:      memory.MemoryType(input.Type),
			Tags:      tagList,
		})
		if err != nil {
			return nil, nil, err
		}

		scope := "project"
		if userScope {
			scope = "user"
		}
		return textResult(fmt.Sprintf("Memory %q saved [%s]", slug, scope))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_remove",
		Description: "Remove a memory entry by its ID (slug).",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input memoryRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return nil, nil, err
		}

		userScope := input.Scope == "user"
		svc, err := newMemorySvc(userScope, projectDir)
		if err != nil {
			return nil, nil, err
		}
		defer func() { _ = svc.Close() }()

		if err := svc.RemoveMemory(input.ID); err != nil {
			return nil, nil, err
		}

		return textResult(fmt.Sprintf("Memory %q removed", input.ID))
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_search",
		Description: "Search for text in memory files. Performs case-insensitive text matching across all memory entries.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, input memorySearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return nil, nil, err
		}

		scope := "project"
		if input.Scope == "user" {
			scope = "user"
		}

		origWd, _ := os.Getwd()
		_ = os.Chdir(projectDir)
		defer func() { _ = os.Chdir(origWd) }()

		dir := memory.RawDir(scope)
		if dir == "" {
			return textResult(fmt.Sprintf("No memories found in %s scope.", scope))
		}

		entries, err := os.ReadDir(dir)
		if err != nil {
			if os.IsNotExist(err) {
				return textResult(fmt.Sprintf("No memories found in %s scope.", scope))
			}
			return nil, nil, err
		}

		termLower := strings.ToLower(input.Query)
		var results []string
		for _, e := range entries {
			if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
				continue
			}
			absPath := filepath.Join(dir, e.Name())
			data, readErr := os.ReadFile(absPath)
			if readErr != nil {
				continue
			}
			if strings.Contains(strings.ToLower(string(data)), termLower) {
				title, _ := memory.ParseMemoryMetaPublic(absPath)
				id := strings.TrimSuffix(e.Name(), ".md")
				results = append(results, fmt.Sprintf("[%s] %s", id, title))
			}
		}

		if len(results) == 0 {
			return textResult(fmt.Sprintf("No memories matching %q in %s scope.", input.Query, scope))
		}

		return textResult(fmt.Sprintf("Found %d match(es):\n%s", len(results), strings.Join(results, "\n")))
	})
}
