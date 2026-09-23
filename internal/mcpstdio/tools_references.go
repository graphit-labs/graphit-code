package mcpstdio

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/memory"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	"github.com/graphit-labs/graphit-code/internal/references"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

type referencesQueryInput struct {
	TargetScope   string  `json:"target_scope,omitempty" jsonschema:"Exact target scope for backlinks"`
	TargetScopeID string  `json:"target_scope_id,omitempty" jsonschema:"Logical target project/user identity"`
	TargetContext *string `json:"target_context,omitempty" jsonschema:"Exact target Knowledge context"`

	ProjectDir  string  `json:"project_dir" jsonschema:"Authorized project directory"`
	SourceType  string  `json:"source_type,omitempty" jsonschema:"Filter source type: task, session, memory or knowledge"`
	SourceID    string  `json:"source_id,omitempty" jsonschema:"Exact source record ID"`
	TargetType  string  `json:"target_type,omitempty" jsonschema:"Filter target type: task, session, memory or knowledge"`
	TargetID    string  `json:"target_id,omitempty" jsonschema:"Exact target record ID; use for incoming references"`
	Relation    string  `json:"relation,omitempty" jsonschema:"Exact relation name"`
	Scope       string  `json:"scope,omitempty" jsonschema:"Source scope: project or user"`
	ScopeID     string  `json:"scope_id,omitempty" jsonschema:"Exact logical source scope identity"`
	Context     *string `json:"context,omitempty" jsonschema:"Source knowledge context name"`
	IncludeUser bool    `json:"include_user,omitempty" jsonschema:"Include current authenticated user's memory relations; false by default"`
	PageSize    int     `json:"page_size,omitempty" jsonschema:"Rows per page, default 20, maximum 100"`
	TopK        int     `json:"top_k,omitempty" jsonschema:"Total result cap, default 100"`
	Cursor      string  `json:"cursor,omitempty" jsonschema:"Opaque cursor from the same filtered query"`
	AiOptimized *bool   `json:"ai_optimized,omitempty" jsonschema:"Compact output by default"`
}

type referencesReconcileInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Authorized project directory"`
	IncludeUser bool   `json:"include_user,omitempty" jsonschema:"Also reconcile current caller personal memory; false by default"`
	AiOptimized *bool  `json:"ai_optimized,omitempty"`
}

func registerReferenceTools(server *mcp.Server) {
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("references", "reconcile"), Description: "Repair persisted relations from explicit metadata and structural Task links in the project's writable stores. Does not infer links from prose or edit record contents. Imported Knowledge is not modified. Reports every module failure; repeat safely after repair."}, safeTool(func(ctx context.Context, _ *mcp.CallToolRequest, in referencesReconcileInput) (*mcp.CallToolResult, any, error) {
		dir, err := resolveProjectDir(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		failures := references.Reconcile(ctx, dir, in.IncludeUser)
		return taskResult(struct {
			Complete bool     `json:"complete"`
			Errors   []string `json:"errors"`
		}{len(failures) == 0, failures}, in.AiOptimized)
	}))

	addTool(server, &mcp.Tool{Name: brand.MCPToolName("references", "query"), Description: "Analyze persisted typed relationships across authorized Task, Session, Memory and Knowledge stores. Filter source for outgoing links or target for backlinks. Never infers from prose. Missing projections are explicitly reported. Targets may remain unresolved outside current access; a relation grants no target access."}, safeTool(func(ctx context.Context, _ *mcp.CallToolRequest, in referencesQueryInput) (*mcp.CallToolResult, any, error) {
		dir, err := resolveProjectDir(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		filter := references.Filter{SourceType: in.SourceType, SourceID: in.SourceID, TargetType: in.TargetType, TargetID: in.TargetID, Relation: in.Relation, Scope: in.Scope, ScopeID: in.ScopeID, Context: in.Context, TargetScope: in.TargetScope, TargetScopeID: in.TargetScopeID, TargetContext: in.TargetContext}
		userID := ""
		if in.IncludeUser {
			userID, err = memory.UserScopeIDForContext(ctx)
			if err != nil {
				return errResult(err)
			}
		}
		window, err := referenceQueryWindow(in, dir, userID, filter)
		if err != nil {
			return errResult(err)
		}
		snapshot := references.Read(ctx, dir, in.IncludeUser)
		rows := references.Select(snapshot.Edges, filter)
		result := page.Finish(window, rows)
		return taskResult(struct {
			Relations page.Page[relations.Edge] `json:"relations"`
			Complete  bool                      `json:"complete"`
			Warnings  []string                  `json:"warnings"`
		}{result, snapshot.Complete, snapshot.Warnings}, in.AiOptimized)
	}))
}

func referenceQueryWindow(in referencesQueryInput, dir, userID string, filter references.Filter) (page.Window, error) {
	if in.TopK == 0 {
		in.TopK = 100
	}
	return openPage(in.PageSize, in.Cursor, in.TopK, 20, struct {
		Tool, Project, UserID string
		Filter                references.Filter
		User                  bool
		TopK                  int
	}{"references_query", dir, userID, filter, in.IncludeUser, in.TopK})
}
