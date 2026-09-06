package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
)

type memoryInsertInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	Title       string `json:"title" jsonschema:"Memory title (required)"`
	Content     string `json:"content" jsonschema:"Detailed memory content (required)"`
	Type        string `json:"type,omitempty" jsonschema:"Memory type: convention or correction or decision or tension or fact or skill"`
	Scope       string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	LinkProject bool   `json:"link_project,omitempty" jsonschema:"Link user memory to project identity"`
	Important   bool   `json:"important,omitempty" jsonschema:"Mark as important"`
	Mandatory   bool   `json:"mandatory,omitempty" jsonschema:"Mark as mandatory; mandatory memories are loaded unconditionally at session start"`
	Tags        string `json:"tags,omitempty" jsonschema:"Comma-separated tags"`
}

type memoryUpdateInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	ID         string `json:"id" jsonschema:"Memory ID to update (required)"`
	Content    string `json:"content,omitempty" jsonschema:"New content"`
	Title      string `json:"title,omitempty" jsonschema:"New title"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryDeleteInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	ID         string `json:"id" jsonschema:"Memory ID to delete (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryListInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	Scope       string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type memorySearchInput struct {
	ProjectDir       string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	Query            string `json:"query" jsonschema:"Keywords to search for in the memory wiki using BM25"`
	Scope            string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	TopK             int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	PageSize         int    `json:"page_size,omitempty" jsonschema:"Results per page (default: 20, max: 100); top_k remains the total-result cap"`
	Cursor           string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact search"`
	Preview          *bool  `json:"preview,omitempty" jsonschema:"Set to true to include a short text excerpt per hit. Default false: a search answers with titles, and the memory is read with wiki_source (wiki: memory) when the agent decides it needs it"`
	AiOptimized      *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
	ExcludeMandatory bool   `json:"exclude_mandatory,omitempty" jsonschema:"Exclude mandatory memories already loaded by the initial mandatory-memory call"`
}

type memoryImportantInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	Scope       string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type memoryMandatoryInput = memoryImportantInput

type memoryMandatoryChangeInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	ID         string `json:"id" jsonschema:"Memory ID (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryPromoteInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	ID         string `json:"id" jsonschema:"Memory ID to promote (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryDemoteInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	ID         string `json:"id" jsonschema:"Memory ID to demote (required)"`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
}

type memoryIndexInput struct {
	ProjectDir string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	Scope      string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
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
		Name:        brand.MCPToolName("memory", "insert"),
		Description: "Add a new memory to the project or user memory store.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryInsertInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
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
			svc, err := newMemorySvc(ctx, userScope, projectDir, true)
			if err != nil {
				return err
			}
			defer func() { _ = svc.Close() }()

			assocProject := ""
			if userScope && input.LinkProject {
				_, pID, _ := newMemorySvcDetails(ctx, false, projectDir)
				assocProject = pID
			}

			slug, err = svc.AddMemory(input.Title, input.Content, memory.MemoryOpts{
				ProjectID: assocProject,
				Important: input.Important,
				Mandatory: input.Mandatory,
				Type:      memory.MemoryType(input.Type),
				Tags:      tagList,
			})
			return err
		})
		if err != nil {
			return errResult(err)
		}
		msg := fmt.Sprintf("Memory %q saved", slug)
		if notice := memoryScopeNotice(userScope, projectDir); notice != "" {
			msg += "\n" + notice
		}
		return textResult(msg)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("memory", "update"),
		Description: "Update the title or content of an existing memory.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryUpdateInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(ctx, userScope, projectDir, true)
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
		Name:        brand.MCPToolName("memory", "delete"),
		Description: "Delete a memory entry by ID.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryDeleteInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(ctx, userScope, projectDir, true)
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
		Name:        brand.MCPToolName("memory", "list"),
		Description: "List all memories in the project or user store.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		var memories []memory.MemoryEntry
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(ctx, userScope, projectDir)
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
		if aiOpt(input.AiOptimized) {
			return toonResult(memories)
		}
		return jsonResult(memories)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("memory", "search"),
		Description: "Search the memory wiki using BM25 keyword ranking. " +
			"Answers with memory titles and scores, not memory text: pick the memory from the titles, then read it with " +
			brand.MCPToolName("wiki", "source") + " and wiki: \"memory\". Pass preview=true only when the titles are not enough to choose.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memorySearchInput) (*mcp.CallToolResult, any, error) {
		if input.TopK < 0 {
			return errResult(fmt.Errorf("top_k cannot be negative"))
		}
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := input.Scope == "user"
		notice := memoryScopeNotice(userScope, projectDir)
		scope := "project"
		if userScope {
			scope = "user"
		}
		window, err := openPage(input.PageSize, input.Cursor, input.TopK, 20, struct {
			Tool, ProjectDir, Scope, Query string
			TopK                           int
			ExcludeMandatory               bool
		}{"memory_search", projectDir, scope, input.Query, input.TopK, input.ExcludeMandatory})
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("memory", projectDir, scope)
		if wikiDir == "" {
			if notice != "" {
				return textResult(notice + "\nyour user memory has not been built yet, so there is nothing to recall")
			}
			return errResult(fmt.Errorf("memory wiki not found for %s scope", scope))
		}

		var results []memory.ChainResult
		err = withProjectDir(projectDir, func() error {
			results = memory.SearchChains(ctx, wikiDir, input.Query, window.FetchLimit, input.ExcludeMandatory)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		paged := page.Finish(window, results)
		if aiOpt(input.AiOptimized) {
			out := paginationTOON(memory.FormatChainResultsTOON(paged.Results, wantPreview(input.Preview)), paged.NextCursor)
			if notice != "" {
				return textResult(notice + "\n" + out)
			}
			return textResult(out)
		}
		if notice != "" {
			return noticeResult(notice, paged, false)
		}
		return jsonResult(paged)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("memory", "mandatory"),
		Description: "Return every mandatory memory with full content, without search. " +
			"This is phase one of session-start recall and must run before contextual memory_search.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryMandatoryInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		scope := "project"
		if input.Scope == "user" {
			scope = "user"
		}
		var entries []memory.MandatoryEntry
		err = withProjectDir(projectDir, func() error {
			var listErr error
			entries, listErr = memory.ListMandatoryMemories(scope)
			return listErr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(entries)
		}
		return jsonResult(entries)
	}))

	for _, spec := range []struct {
		name        string
		description string
		enabled     bool
	}{
		{"mark_mandatory", "Mark a memory as mandatory for unconditional session-start recall.", true},
		{"unmark_mandatory", "Remove mandatory status when unconditional recall is no longer required.", false},
	} {
		spec := spec
		mcp.AddTool(server, &mcp.Tool{
			Name: brand.MCPToolName("memory", spec.name), Description: spec.description,
		}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryMandatoryChangeInput) (*mcp.CallToolResult, any, error) {
			projectDir, err := resolveProjectDirOptional(input.ProjectDir)
			if err != nil {
				return errResult(err)
			}
			userScope := scopeFromString(input.Scope)
			err = withProjectDir(projectDir, func() error {
				svc, svcErr := newMemorySvc(ctx, userScope, projectDir, true)
				if svcErr != nil {
					return svcErr
				}
				defer func() { _ = svc.Close() }()
				if spec.enabled {
					return svc.MarkMandatory(input.ID)
				}
				return svc.UnmarkMandatory(input.ID)
			})
			if err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Memory %q mandatory=%t", input.ID, spec.enabled))
		}))
	}

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("memory", "important"),
		Description: "List all memories marked as important.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryImportantInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
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
		if aiOpt(input.AiOptimized) {
			return toonResult(entries)
		}
		return jsonResult(entries)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("memory", "promote"),
		Description: "Promote a memory to important status.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryPromoteInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(ctx, userScope, projectDir, true)
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
		Name:        brand.MCPToolName("memory", "demote"),
		Description: "Demote a memory from important status.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryDemoteInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(ctx, userScope, projectDir, true)
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
		Name:        brand.MCPToolName("memory", "index"),
		Description: "Regenerate the semantic wiki index of the memory store.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		userScope := scopeFromString(input.Scope)
		err = withProjectDir(projectDir, func() error {
			svc, err := newMemorySvc(ctx, userScope, projectDir, true)
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
		Name:        brand.MCPToolName("memory", "schema"),
		Description: "Show the authoritative memory table schema.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memorySchemaInput) (*mcp.CallToolResult, any, error) {
		return textResult("Memory Table Schema\nPrimary key: key\nCore columns: id, revision_id, superseded, title, body, type, tags_json, important, mandatory\nLifecycle columns: created_at, updated_at, revision, previous, next, updated_by\nScope columns: scope, scope_id, project_id\nVector column: embedding")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("memory", "remove"),
		Description: "Remove a memory context sync connection.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memoryRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		cleanCtx, err := sanitizeContextName(input.Context)
		if err != nil {
			return errResult(err)
		}
		_ = projectDir
		if err := os.RemoveAll(memory.TableDirFor(cleanCtx, cleanCtx)); err != nil {
			return errResult(err)
		}
		if err := os.RemoveAll(memory.MemoryWikiGlobalDir(cleanCtx, cleanCtx)); err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Removed memory context %q", cleanCtx))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("memory", "sync"),
		Description: "Sync memories from an external context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memorySyncInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		err = withProjectDir(projectDir, func() error {
			ms, err := memory.NewMemoryStore()
			if err != nil {
				return err
			}
			cleanCtx, err := sanitizeContextName(input.Context)
			if err != nil {
				return err
			}
			scopeID := cleanCtx
			if config.HubS3Config().Configured() {
				registry, err := hub.NewRegistryManager(ctx)
				if err != nil {
					return err
				}
				project, err := registry.ResolveProject(ctx, cleanCtx)
				if err != nil {
					return fmt.Errorf("memory context %q is not an authorized Hub project: %w", cleanCtx, err)
				}
				scopeID = project.ID
			}
			svc := memory.NewMemoryServiceForContext(scopeID, ms).WithContext(ctx)
			defer func() { _ = svc.Close() }()

			return svc.SyncWiki()
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory context %q synced successfully.", input.Context))
	}))
}

func newMemorySvcDetails(ctx context.Context, userScope bool, projectDir string) (*memory.MemoryService, string, error) {
	scope, scopeID, _, err := memoryScopeFor(ctx, userScope, projectDir)
	if err != nil {
		return nil, "", err
	}

	ms, _ := memory.NewMemoryStore()
	svc := memory.NewMemoryService(scope, scopeID, ms).WithContext(ctx)
	_ = svc.EnsureInitialised()
	return svc, scopeID, nil
}
