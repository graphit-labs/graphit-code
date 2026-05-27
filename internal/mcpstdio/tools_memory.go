package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type memoryInsertInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Title       string `json:"title" jsonschema:"Memory title (required)"`
	Content     string `json:"content" jsonschema:"Detailed memory content (required)"`
	Type        string `json:"type,omitempty" jsonschema:"Memory type: convention or correction or decision or tension or fact or skill"`
	Scope       string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	LinkProject bool   `json:"link_project,omitempty" jsonschema:"Link user memory to project identity"`
	Important   bool   `json:"important,omitempty" jsonschema:"Mark as important"`
	Tags        string `json:"tags,omitempty" jsonschema:"Comma-separated tags"`
}

type memoryUpdateInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID         string `json:"id" jsonschema:"Memory ID to update (required)"`
	Content    string `json:"content,omitempty" jsonschema:"New content"`
	Title      string `json:"title,omitempty" jsonschema:"New title"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryDeleteInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID         string `json:"id" jsonschema:"Memory ID to delete (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryListInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memorySearchInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query      string `json:"query" jsonschema:"Text query to search (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryQueryInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query      string `json:"query" jsonschema:"Natural language query (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type memoryImportantInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryPromoteInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID         string `json:"id" jsonschema:"Memory ID to promote (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryDemoteInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID         string `json:"id" jsonschema:"Memory ID to demote (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryConsolidateInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	Apply      bool   `json:"apply,omitempty" jsonschema:"Apply proposed changes"`
}

type memoryGCInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	DryRun     bool   `json:"dry_run,omitempty" jsonschema:"Only scan, do not delete"`
	StaleDays  int    `json:"stale_days,omitempty" jsonschema:"Days of inactivity before memory is stale"`
}

type memoryIndexInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryExportInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
}

type memorySchemaInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
}

type memoryRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context" jsonschema:"Named context to remove (required)"`
}

type memorySyncInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context" jsonschema:"Named context to sync (required)"`
}

func registerMemoryTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_insert",
		Description: "Add a new memory to the project or user memory store.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryInsertInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		if input.Type != "" && !memory.ValidMemoryType(input.Type) {
			return errResult(fmt.Errorf("invalid memory type %q", input.Type))
		}

		var tagList []string
		if input.Tags != "" {
			for _, t := range strings.Split(input.Tags, ",") {
				t = strings.TrimSpace(t)
				if t != "" {
					tagList = append(tagList, t)
				}
			}
		}

		var slug string
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(userScope, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			assocProject := ""
			if userScope && input.LinkProject {
				// We can get Project ID from memory service helper
				_, pID, _ := newMemorySvcDetails(false, projectDir)
				assocProject = pID
			}

			slug, err = svc.AddMemory(input.Title, input.Content, memory.MemoryOpts{
				ProjectID: assocProject,
				Important: input.Important,
				Type:      memory.MemoryType(input.Type),
				Tags:      tagList,
			})
			return err
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory %q saved", slug))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_update",
		Description: "Update the title or content of an existing memory.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryUpdateInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(userScope, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			return svc.UpdateMemory(input.ID, input.Title, input.Content)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory %q updated", input.ID))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_delete",
		Description: "Delete a memory entry by ID.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryDeleteInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(userScope, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			return svc.RemoveMemory(input.ID)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory %q deleted", input.ID))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_list",
		Description: "List all memories in the project or user store.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		var memories []memory.MemoryEntry
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(userScope, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			memories, err = svc.ListMemories()
			return err
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(memories)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_search",
		Description: "Search for text matching in raw memory files.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memorySearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		scope := "project"
		if input.Scope == "user" {
			scope = "user"
		}

		type match struct {
			ID        string `json:"id"`
			Title     string `json:"title"`
			Important bool   `json:"important"`
		}
		var matches []match

		err = withProjectDir(projectDir, func() error {
			dir := memory.RawDir(scope)
			if dir == "" {
				return nil
			}
			entries, err := os.ReadDir(dir)
			if err != nil {
				if os.IsNotExist(err) {
					return nil
				}
				return err
			}

			termLower := strings.ToLower(input.Query)
			for _, e := range entries {
				if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
					continue
				}
				absPath := filepath.Join(dir, e.Name())
				data, readErr := os.ReadFile(absPath)
				if readErr != nil {
					continue
				}
				content := strings.ToLower(string(data))
				if strings.Contains(content, termLower) {
					title, _ := memory.ParseMemoryMetaPublic(absPath)
					name := e.Name()
					var id string
					if memory.IsImportantMemory(name) {
						id = strings.TrimSuffix(name, memory.ImportantMemorySuffix+".md")
					} else {
						id = strings.TrimSuffix(name, ".md")
					}
					matches = append(matches, match{
						ID:        id,
						Title:     title,
						Important: memory.IsImportantMemory(name),
					})
				}
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(matches)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_query",
		Description: "Search memories with AI Consultation and return a synthesized response.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryQueryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("memory", projectDir, input.Context)
		if wikiDir == "" {
			scopeLabel := input.Scope
			if scopeLabel == "" {
				scopeLabel = "project"
			}
			wikiDir = memory.WikiDir(scopeLabel)
		}

		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return errResult(err)
		}

		var result *wiki.SearchResult
		err = withProjectDir(projectDir, func() error {
			var qerr error
			result, qerr = wiki.SearchWiki(ctx, aiClient, input.Query, wiki.SearchConfig{
				WikiDir:   wikiDir,
				ModuleTag: "memory",
			})
			return qerr
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(result.Answer)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_important",
		Description: "List all memories marked as important.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryImportantInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		scope := "project"
		if input.Scope == "user" {
			scope = "user"
		}

		var entries []memory.ImportantEntry
		err = withProjectDir(projectDir, func() error {
			var lerr error
			entries, lerr = memory.ListImportantMemories(scope)
			return lerr
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(entries)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_promote",
		Description: "Promote a memory to important status.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryPromoteInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(userScope, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			return svc.PromoteMemory(input.ID)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory %q promoted", input.ID))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_demote",
		Description: "Demote a memory from important status.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryDemoteInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(userScope, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			return svc.DemoteMemory(input.ID)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory %q demoted", input.ID))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_consolidate",
		Description: "Analyze memory wiki for staleness, duplicates, contradictions, and suggestions.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryConsolidateInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		scope := "project"
		if input.Scope == "user" {
			scope = "user"
		}

		aiClient, _ := ai.NewClientFromConfig()

		var report *memory.ConsolidationReport
		err = withProjectDir(projectDir, func() error {
			var cerr error
			report, cerr = memory.RunConsolidation(ctx, scope, aiClient)
			if cerr != nil {
				return cerr
			}

			if input.Apply {
				userScope := scopeFromString(input.Scope)
				svc, err := newMemorySvc(userScope, projectDir)
				if err != nil {
					return err
				}
				defer func() { _ = svc.Close() }()

				for _, a := range report.Suggestions {
					if len(a.MemoryIDs) == 0 {
						continue
					}
					id := a.MemoryIDs[0]
					switch a.Type {
					case "promote":
						_ = svc.PromoteMemory(id)
					case "demote":
						_ = svc.DemoteMemory(id)
					case "delete":
						_ = svc.RemoveMemory(id)
					}
				}
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(report)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_gc",
		Description: "Garbage collect inactive or stale memories.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryGCInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		scope := "project"
		if input.Scope == "user" {
			scope = "user"
		}

		staleDays := input.StaleDays
		if staleDays <= 0 {
			staleDays = 30
		}

		var report *memory.GCReport
		err = withProjectDir(projectDir, func() error {
			var gerr error
			report, gerr = memory.RunGC(scope, staleDays)
			if gerr != nil {
				return gerr
			}

			if !input.DryRun && len(report.Candidates) > 0 {
				userScope := scopeFromString(input.Scope)
				svc, err := newMemorySvc(userScope, projectDir)
				if err != nil {
					return err
				}
				defer func() { _ = svc.Close() }()

				for _, c := range report.Candidates {
					_ = svc.RemoveMemory(c.ID)
				}
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(report)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_index",
		Description: "Regenerate the semantic wiki index of the memory store.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(userScope, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			return svc.IndexMemories(ctx)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult("Memory indexing completed.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_export",
		Description: "Index and sync project memories back to the local git repository.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryExportInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(false, projectDir)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			_ = svc.IndexMemories(ctx)
			return svc.SyncToLocal()
		})
		if err != nil {
			return errResult(err)
		}
		return textResult("Project memories exported to git repository.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_schema",
		Description: "Show the memory graph database schema details.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memorySchemaInput) (*mcp.CallToolResult, any, error) {
		return textResult("Memory Graph Schema\nNode labels: Document, Section\nEdge labels: REFERENCES, CONTAINS\nProperties:\n - Document: id, title, scope, scope_id, created_at, tags\n - Section: name, summary, section_level")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_remove",
		Description: "Remove a memory context sync connection.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		linkDir := filepath.Join(projectDir, brand.DotDir(), "memory", input.Context)
		if err := os.RemoveAll(linkDir); err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Removed memory context %q", input.Context))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        "graphit_memory_sync",
		Description: "Sync memories from an external context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memorySyncInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		err = withProjectDir(projectDir, func() error {
			ms, err := memory.NewMemoryGitStore()
			if err != nil {
				return err
			}
			svc := memory.NewMemoryServiceForContext(input.Context, ms)
			defer func() { _ = svc.Close() }()

			return svc.SyncToLocal()
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory context %q synced successfully.", input.Context))
	}))
}

func newMemorySvcDetails(userScope bool, projectDir string) (*memory.MemoryService, string, error) {
	var scope memory.MemoryScope
	var scopeID string

	if userScope {
		scope = memory.MemoryScopeUser
		hash, err := memory.UserHashFromGit()
		if err != nil {
			return nil, "", fmt.Errorf("cannot determine user identity: %w", err)
		}
		scopeID = hash
	} else {
		scope = memory.MemoryScopeProject
		lockPath := filepath.Join(projectDir, brand.LockFileName())
		lf, err := hub.LoadLockfile(lockPath)
		if err != nil || lf == nil {
			return nil, "", fmt.Errorf("project not initialised")
		}
		scopeID = lf.Project.ID
	}

	ms, _ := memory.NewMemoryGitStore()
	svc := memory.NewMemoryService(scope, scopeID, ms)
	_ = svc.EnsureInitialised()
	return svc, scopeID, nil
}
