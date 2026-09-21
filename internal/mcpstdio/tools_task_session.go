package mcpstdio

import (
	"context"
	"errors"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/toon"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

type taskSessionCreateInput struct {
	References     *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed relationships. Send the complete list when referencing records; omit to preserve existing links, send [] to clear. Each target requires type and id; qualify cross-scope targets."`
	ProjectDir     string           `json:"project_dir" jsonschema:"Project directory (required)"`
	Title          string           `json:"title" jsonschema:"Concise outcome of the complete user request (required)"`
	Description    string           `json:"description" jsonschema:"Detailed current request, scope, requirements, constraints, sources and success criteria sufficient for another agent without the conversation (required)"`
	Strategy       string           `json:"strategy" jsonschema:"Current approach, investigation and task decomposition strategy, uncertainties and validation plan (required)"`
	IdempotencyKey string           `json:"idempotency_key,omitempty" jsonschema:"Stable key for this logical request; reuse on uncertain retries"`
	AgentID        string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; distinct from the logical session ID"`
	AiOptimized    *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionGetInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Logical Task session ID (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionListInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Status      string `json:"status,omitempty" jsonschema:"open, in_progress, completed, or cancelled"`
	Owner       string `json:"owner,omitempty" jsonschema:"Exact coordinator identity filter"`
	Active      bool   `json:"active,omitempty" jsonschema:"Only nonterminal sessions (open or in_progress)"`
	PageSize    int    `json:"page_size,omitempty" jsonschema:"Results per page (default 20, maximum 100)"`
	Cursor      string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor from this exact listing"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionSearchInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query       string `json:"query" jsonschema:"Keywords in the request, strategy, checkpoints or prior revisions (required)"`
	TopK        int    `json:"top_k,omitempty" jsonschema:"Total ranked result cap (default 20)"`
	PageSize    int    `json:"page_size,omitempty" jsonschema:"Results per page (default 20, maximum 100)"`
	Cursor      string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor from this exact search"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionClaimInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Logical Task session ID (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host identity when omitted"`
	Lease       string `json:"lease,omitempty" jsonschema:"Positive coordinator lease such as 2h; default 1h"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionReviseInput struct {
	References       *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed relationships. Send the complete list when referencing records; omit to preserve existing links, send [] to clear. Each target requires type and id; qualify cross-scope targets."`
	ProjectDir       string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID               string           `json:"id" jsonschema:"Claimed logical Task session ID (required)"`
	ClaimToken       string           `json:"claim_token" jsonschema:"Private session coordinator token returned by session_claim (required)"`
	AgentID          string           `json:"agent_id,omitempty" jsonschema:"Stable coordinator identity"`
	ExpectedRevision int64            `json:"expected_revision" jsonschema:"Current session revision for compare-and-swap (required)"`
	Reason           string           `json:"reason" jsonschema:"What changed in user request or approach and its impact on the existing tasks (required)"`
	Title            *string          `json:"title,omitempty" jsonschema:"Replacement request title"`
	Description      *string          `json:"description,omitempty" jsonschema:"Complete replacement request specification, preserving remaining scope and constraints"`
	Strategy         *string          `json:"strategy,omitempty" jsonschema:"Replacement strategy, rationale and validation plan"`
	Lease            string           `json:"lease,omitempty" jsonschema:"Renewed lease; never shortens a longer active lease"`
	AiOptimized      *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionCheckpointInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed relationships. Send the complete list when referencing records; omit to preserve existing links, send [] to clear. Each target requires type and id; qualify cross-scope targets."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed logical Task session ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Private session coordinator token (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable coordinator identity"`
	Summary     string           `json:"summary" jsonschema:"Descriptive completed progress with evidence and affected task IDs (required)"`
	Problems    string           `json:"problems,omitempty" jsonschema:"Problems, blockers, uncertainty and attempted solutions"`
	Decisions   string           `json:"decisions,omitempty" jsonschema:"Decisions with rationale, alternatives and impact"`
	Strategy    string           `json:"strategy,omitempty" jsonschema:"Current execution approach; material specification changes also require session_revise"`
	NextStep    string           `json:"next_step" jsonschema:"Exact continuation action, target, prerequisites and done condition (required)"`
	Lease       string           `json:"lease,omitempty" jsonschema:"Renewed lease; never shortens a longer active lease"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionHeartbeatInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed logical Task session ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Private session coordinator token (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable coordinator identity"`
	Lease       string `json:"lease,omitempty" jsonschema:"Renewed lease; never shortens a longer active lease"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionReleaseInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed logical Task session ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Private session coordinator token (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable coordinator identity"`
	Summary     string           `json:"summary" jsonschema:"Self-contained handoff of results, remaining work, blockers and relevant tasks (required)"`
	NextStep    string           `json:"next_step" jsonschema:"Exact continuation action with target and done condition for the next coordinator (required)"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionCompleteInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed logical Task session ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Private session coordinator token (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable coordinator identity"`
	Summary     string           `json:"summary" jsonschema:"Final result mapped to requested outcomes and evidence; explain cancelled scope and residual limitations (required)"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionCancelInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Logical Task session ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Private token from the active session claim (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity"`
	Reason      string           `json:"reason" jsonschema:"Why this request is cancelled, remaining consequences and replacement if any (required)"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionForceTakeoverInput struct {
	ProjectDir       string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID               string `json:"id" jsonschema:"In-progress logical Task session ID (required)"`
	ConfirmID        string `json:"confirm_id" jsonschema:"Exact session ID confirmation (required)"`
	ExpectedRevision int64  `json:"expected_revision" jsonschema:"Current session revision (required)"`
	Reason           string `json:"reason" jsonschema:"Evidence that the coordinator is unrecoverable and takeover necessary (required)"`
	Lease            string `json:"lease" jsonschema:"Positive replacement coordinator lease such as 1h (required)"`
	AgentID          string `json:"agent_id,omitempty" jsonschema:"Different new coordinator identity; host identity when omitted"`
	AiOptimized      *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSessionSearcher interface {
	SessionSearch(context.Context, string, int) ([]graphtask.SessionSearchResult, error)
}

func paginateTaskSessionSearch(ctx context.Context, searcher taskSessionSearcher, in taskSessionSearchInput) (page.Page[graphtask.SessionSearchResult], error) {
	topK := in.TopK
	if topK == 0 {
		topK = defaultTaskSearchLimit
	}
	query := strings.TrimSpace(in.Query)
	window, err := openPage(in.PageSize, in.Cursor, topK, defaultTaskSearchLimit, struct {
		Tool, ProjectDir, Query string
		TopK                    int
	}{"task_session_search", in.ProjectDir, query, topK})
	if err != nil {
		return page.Page[graphtask.SessionSearchResult]{}, err
	}
	rows, err := searcher.SessionSearch(ctx, query, window.FetchLimit)
	if err != nil {
		return page.Page[graphtask.SessionSearchResult]{}, err
	}
	return page.Finish(window, rows), nil
}

func taskSessionPageResult[T any](value page.Page[T], optimized *bool) (*mcp.CallToolResult, any, error) {
	if aiOpt(optimized) {
		return textResult(paginationTOON(toon.FormatAny(value.Results), value.NextCursor))
	}
	return jsonResult(value)
}

func registerTaskSessionTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "create"), Description: "Create an idempotent durable request session with its complete description and strategy. Does not claim coordination or create tasks."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionCreateInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionCreate(ctx, graphtask.SessionCreateInput{Title: in.Title, Description: in.Description, Strategy: in.Strategy, IdempotencyKey: in.IdempotencyKey, Actor: taskActor(req, in.AgentID)})
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "get"), Description: "Read a session's authoritative request, strategy, ordered checkpoints and revisions, and associated task summaries without private tokens."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionGetInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionGet(ctx, in.ID)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "list"), Description: "Discover durable request sessions; active=true selects unfinished requests, including released work another coordinator can resume."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionListInput) (*mcp.CallToolResult, any, error) {
		svc, dir, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		window, err := openPage(in.PageSize, in.Cursor, 0, 20, struct {
			Tool, ProjectDir, Status, Owner string
			Active                          bool
		}{"task_session_list", dir, in.Status, in.Owner, in.Active})
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionList(ctx, graphtask.SessionListOptions{Status: in.Status, Owner: in.Owner, Active: in.Active})
		if err != nil {
			return errResult(err)
		}
		return taskSessionPageResult(page.Finish(window, v), in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "search"), Description: "Search session requests, strategies and durable history with LanceDB ranking; read selected sessions and their tasks to recover prior reasoning."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionSearchInput) (*mcp.CallToolResult, any, error) {
		svc, dir, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		in.ProjectDir = dir
		v, err := paginateTaskSessionSearch(ctx, svc, in)
		if err != nil {
			return errResult(err)
		}
		return taskSessionPageResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "claim"), Description: "Claim exclusive session coordination with a fenced lease, independent of task worker claims."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionClaimInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionClaim(ctx, in.ID, taskActor(req, in.AgentID), lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "revise"), Description: "Revise the current request or strategy when user direction or discoveries change it; preserve immutable prior specifications and require the current revision."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionReviseInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionRevise(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), graphtask.SessionReviseInput{ExpectedRevision: in.ExpectedRevision, Reason: in.Reason, Title: in.Title, Description: in.Description, Strategy: in.Strategy}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "checkpoint"), Description: "Append a descriptive session checkpoint: actual progress, problems, decisions, strategy and exact continuation. Does not replace a changed request specification."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionCheckpointInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionCheckpoint(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), graphtask.SessionCheckpointInput{Summary: in.Summary, Problems: in.Problems, Decisions: in.Decisions, Strategy: in.Strategy, NextStep: in.NextStep}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "heartbeat"), Description: "Renew session coordination without inventing progress; descriptive checkpoints remain explicit."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionHeartbeatInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionHeartbeat(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "release"), Description: "Hand off session coordination with durable results and an exact next step. The request stays open and tasks retain their independent lifecycle."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionReleaseInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionRelease(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Summary, in.NextStep)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "complete"), Description: "Explicitly close a fulfilled request with a final evidence-based summary. Refuses any associated task that is not completed or cancelled."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionCompleteInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionComplete(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Summary)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "cancel"), Description: "Cancel an obsolete request with an audited reason only after its tasks are terminal; never silently cancels associated work."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionCancelInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionCancel(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Reason)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "session", "force", "takeover"), Description: "Recover a live session from an unrecoverable coordinator using exact ID confirmation, current revision, reason, replacement lease and a different identity."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSessionForceTakeoverInput) (*mcp.CallToolResult, any, error) {
		if strings.TrimSpace(in.Lease) == "" {
			return errResult(errors.New("session takeover requires an explicit positive replacement lease"))
		}
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		v, err := svc.SessionForceTakeover(ctx, in.ID, taskActor(req, in.AgentID), graphtask.ForceTakeoverInput{ExpectedRevision: in.ExpectedRevision, ConfirmID: in.ConfirmID, Reason: in.Reason}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(v, in.AiOptimized)
	}))
}
