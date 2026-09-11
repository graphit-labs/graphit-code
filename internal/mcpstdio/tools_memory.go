package mcpstdio

import (
	"context"
	"fmt"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	"github.com/graphit-labs/graphit-code/internal/textslice"
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
	Query            string `json:"query" jsonschema:"Keywords to search for directly in the authoritative memory table using BM25"`
	Scope            string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	TopK             int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	PageSize         int    `json:"page_size,omitempty" jsonschema:"Results per page (default: 20, max: 100); top_k remains the total-result cap"`
	Cursor           string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact search"`
	Preview          *bool  `json:"preview,omitempty" jsonschema:"Set to true to include a short text excerpt per hit. Default false: a search answers with titles, and the memory is read with memory_source when selected"`
	AiOptimized      *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
	ExcludeMandatory bool   `json:"exclude_mandatory,omitempty" jsonschema:"Exclude mandatory memories already loaded by the initial mandatory-memory call"`
}

type memoryImportantInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	Scope       string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type memorySourceInput struct {
	ProjectDir  string `json:"project_dir,omitempty" jsonschema:"Project directory. Omit for the global scope, which serves your user memory."`
	ID          string `json:"id" jsonschema:"Memory ID or id/revision-id returned by memory_search"`
	Scope       string `json:"scope,omitempty" jsonschema:"Scope: project (default) or user"`
	Head        int    `json:"head,omitempty"`
	Tail        int    `json:"tail,omitempty"`
	StartLine   int    `json:"start_line,omitempty"`
	EndLine     int    `json:"end_line,omitempty"`
	Pattern     string `json:"pattern,omitempty"`
	IsRegex     bool   `json:"regex,omitempty"`
	Before      int    `json:"before,omitempty"`
	After       int    `json:"after,omitempty"`
	LineNumbers bool   `json:"line_numbers,omitempty"`
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
		Description: "List all memories in the project or user store, ordered mandatory, important, normal and newest first within each group.",
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
		Description: "Search the authoritative memory table, ordered mandatory, important, normal and newest first within each group. Match score is metadata, not the primary order. " +
			"Answers with memory IDs, titles and scores, not memory text: pick a result and read it with " +
			brand.MCPToolName("memory", "source") + ". Pass preview=true only when titles cannot disambiguate.",
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

		var results []memory.ChainResult
		err = withProjectDir(projectDir, func() error {
			svc, svcErr := newMemorySvc(ctx, userScope, projectDir)
			if svcErr != nil {
				return svcErr
			}
			defer func() { _ = svc.Close() }()
			results, svcErr = svc.SearchMemories(ctx, input.Query, window.FetchLimit, memory.SearchOptions{
				ExcludeMandatory: input.ExcludeMandatory,
			})
			return svcErr
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
		Name:        brand.MCPToolName("memory", "source"),
		Description: "Read one current memory or archived revision directly from the authoritative table, with optional line and pattern slicing.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input memorySourceInput) (*mcp.CallToolResult, any, error) {
		if strings.TrimSpace(input.ID) == "" {
			return errResult(fmt.Errorf("id is required"))
		}
		projectDir, err := resolveProjectDirOptional(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		userScope := scopeFromString(input.Scope)
		var content string
		var found bool
		err = withProjectDir(projectDir, func() error {
			svc, svcErr := newMemorySvc(ctx, userScope, projectDir)
			if svcErr != nil {
				return svcErr
			}
			defer func() { _ = svc.Close() }()
			content, found, svcErr = svc.ReadMemory(ctx, input.ID)
			return svcErr
		})
		if err != nil {
			return errResult(err)
		}
		if !found {
			return errResult(fmt.Errorf("memory %q not found", input.ID))
		}
		result, err := textslice.Apply(content, textslice.Request{
			Head: input.Head, Tail: input.Tail, StartLine: input.StartLine, EndLine: input.EndLine,
			Pattern: input.Pattern, IsRegex: input.IsRegex, Before: input.Before, After: input.After,
			LineNumbers: input.LineNumbers,
		})
		if err != nil {
			return errResult(err)
		}
		if result.Source == "" && len(result.Matches) == 0 {
			return textResult(fmt.Sprintf("No matches found for pattern %q in memory %s", input.Pattern, input.ID))
		}
		return textResult(result.Source)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("memory", "mandatory"),
		Description: "Return every mandatory memory with full content, without search. " +
			"Results are newest first. This is phase one of session-start recall and must run before contextual memory_search.",
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
		Description: "List all memories marked as important, with mandatory entries first and newest first within each group.",
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
		Description: "Ensure and refresh the search indexes on the authoritative memory table.",
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
		ms, err := memory.NewMemoryStore()
		if err != nil {
			return errResult(err)
		}
		if err := ms.PruneScope(cleanCtx, cleanCtx); err != nil {
			return errResult(err)
		}
		_ = projectDir
		return textResult(fmt.Sprintf("Memory context %q disconnected; its local table was removed if present and any remote authoritative table was left unchanged", cleanCtx))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("memory", "sync"),
		Description: "Ensure direct search indexes for an external authoritative memory context.",
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
			if cfg := config.HubMetadataS3Config(ctx); cfg.Configured() && cfg.ResolutionError == nil {
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

			return svc.IndexMemories(ctx)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Memory context %q indexes refreshed successfully.", input.Context))
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
