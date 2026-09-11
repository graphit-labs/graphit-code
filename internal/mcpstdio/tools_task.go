package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/toon"
)

type taskCreateInput struct {
	ProjectDir         string   `json:"project_dir" jsonschema:"Project directory (required)"`
	Title              string   `json:"title" jsonschema:"Concise action-oriented plain-text title naming one outcome (required)"`
	Description        string   `json:"description" jsonschema:"Self-contained Markdown specification: objective/value, context, in/out of scope, requirements or observable behavior, constraints/assumptions, interfaces/dependencies, risks/edge cases, and intended result (required; proportional detail)"`
	AcceptanceCriteria []string `json:"acceptance_criteria" jsonschema:"One singular imperative Markdown statement per item: what the system must do or must not allow, with condition and observable expected result; at least one required"`
	Tests              []string `json:"tests" jsonschema:"Behavior checks in Given-When-Then; other validations name method/command, target/conditions, and expected evidence/result; at least one Markdown item required"`
	Type               string   `json:"type,omitempty" jsonschema:"Task type such as task, bug, feature, epic, or chore"`
	Priority           *int     `json:"priority,omitempty" jsonschema:"Priority 0 (critical) through 4 (lowest); default 2"`
	ParentID           string   `json:"parent_id,omitempty" jsonschema:"Parent delivery task ID for a subtask; use for cleanup, validation, review, documentation, commit preparation, release checks, and similar finalization work"`
	DependsOn          []string `json:"depends_on,omitempty" jsonschema:"Task IDs that must complete first"`
	IdempotencyKey     string   `json:"idempotency_key,omitempty" jsonschema:"Stable caller key; defaults to the canonical title"`
	AgentID            string   `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	AiOptimized        *bool    `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskGetInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Task ID (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskExportInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID         string `json:"id,omitempty" jsonschema:"Exact task ID; omit to export every task in the project"`
}

type taskListInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Status      string `json:"status,omitempty" jsonschema:"open, blocked, flagged, in_progress, completed, or cancelled"`
	Owner       string `json:"owner,omitempty" jsonschema:"Filter by exact agent owner"`
	ParentID    string `json:"parent_id,omitempty" jsonschema:"Return only direct subtasks of this task ID"`
	Ready       bool   `json:"ready,omitempty" jsonschema:"Return only open tasks with all dependencies completed"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSearchInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query       string `json:"query" jsonschema:"Keywords for LanceDB full-text search"`
	TopK        int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (default: 20)"`
	PageSize    int    `json:"page_size,omitempty" jsonschema:"Results per page (default: 20, max: 100); top_k remains the total-result cap"`
	Cursor      string `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact search"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskClaimInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Ready task ID (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Lease       string `json:"lease,omitempty" jsonschema:"Lease duration such as 2h; default 1h"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskForceTakeoverInput struct {
	ProjectDir       string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID               string `json:"id" jsonschema:"In-progress task ID (required)"`
	ConfirmID        string `json:"confirm_id" jsonschema:"Exact task ID confirmation (required)"`
	ExpectedRevision int64  `json:"expected_revision" jsonschema:"Current task revision used as a compare-and-swap fence (required)"`
	Reason           string `json:"reason" jsonschema:"Markdown explanation proving the current owner is unrecoverable and takeover is necessary (required)"`
	Lease            string `json:"lease" jsonschema:"Positive replacement lease duration such as 1h (required)"`
	AgentID          string `json:"agent_id,omitempty" jsonschema:"Different new owner identity; host session identity is used when omitted"`
	AiOptimized      *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskProgressInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Summary     string `json:"summary" jsonschema:"Markdown checkpoint of completed facts, changed constraints, and concrete evidence (required)"`
	NextStep    string `json:"next_step,omitempty" jsonschema:"Markdown exact next action with target and completion condition for this or a takeover agent"`
	Lease       string `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskHeartbeatInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Lease       string `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskReleaseInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Summary     string `json:"summary,omitempty" jsonschema:"Markdown summary of completed work, current state, and blocking evidence"`
	NextStep    string `json:"next_step,omitempty" jsonschema:"Markdown exact continuation action with target and completion condition for the next agent"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCompleteInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Summary     string `json:"summary,omitempty" jsonschema:"Markdown final result mapped to acceptance evidence, with any residual limitations"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCancelInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Task ID to cancel (required)"`
	ClaimToken  string `json:"claim_token,omitempty" jsonschema:"Required fencing token when the task is in progress"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Reason      string `json:"reason" jsonschema:"Markdown explanation of why the task is no longer needed and what replaces it, if anything (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskRemoveInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Task ID to remove (required)"`
	ConfirmID   string `json:"confirm_id" jsonschema:"Exact task ID confirmation (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Reason      string `json:"reason" jsonschema:"Markdown explanation proving hard deletion is correct and no unique work history is needed (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskFlagInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Reason      string `json:"reason" jsonschema:"Markdown unresolved condition, completion impact, and objective clearing condition (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskUnflagInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCheckInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	CheckID     string `json:"check_id" jsonschema:"Acceptance or test check ID (required)"`
	Passed      bool   `json:"passed" jsonschema:"Whether this check passed"`
	Evidence    string `json:"evidence" jsonschema:"Markdown evidence naming the command, observation, or artifact, relevant conditions, and actual result (required)"`
	Lease       string `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskReviseInput struct {
	ProjectDir            string    `json:"project_dir" jsonschema:"Project directory (required)"`
	ID                    string    `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken            string    `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID               string    `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	ExpectedRevision      int64     `json:"expected_revision" jsonschema:"Current task revision used as a compare-and-swap fence (required)"`
	Reason                string    `json:"reason" jsonschema:"Markdown rationale for the specification change and its scope or verification impact (required)"`
	Title                 *string   `json:"title,omitempty" jsonschema:"Replacement concise action-oriented plain-text title naming one outcome"`
	Description           *string   `json:"description,omitempty" jsonschema:"Replacement self-contained Markdown specification with proportional objective, context, scope, requirements, constraints, interfaces, risks, and intended result"`
	Type                  *string   `json:"type,omitempty" jsonschema:"Replacement task type"`
	Priority              *int      `json:"priority,omitempty" jsonschema:"Replacement priority 0 through 4"`
	ParentID              *string   `json:"parent_id,omitempty" jsonschema:"Replacement parent task ID; empty clears the parent"`
	DependsOn             *[]string `json:"depends_on,omitempty" jsonschema:"Complete replacement dependency list; empty clears dependencies"`
	AddAcceptanceCriteria []string  `json:"add_acceptance_criteria,omitempty" jsonschema:"New singular imperative Markdown statements of what the system must do or must not allow"`
	AddTests              []string  `json:"add_tests,omitempty" jsonschema:"New Given-When-Then behavior checks or explicit method-target-expected-result validations to append"`
	Lease                 string    `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized           *bool     `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCheckSupersedeInput struct {
	ProjectDir       string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID               string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken       string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID          string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	ExpectedRevision int64  `json:"expected_revision" jsonschema:"Current task revision used as a compare-and-swap fence (required)"`
	CheckID          string `json:"check_id" jsonschema:"Active acceptance or test check ID (required)"`
	Reason           string `json:"reason" jsonschema:"Markdown rationale explaining why this check no longer represents the task specification (required)"`
	ReplacementText  string `json:"replacement_text,omitempty" jsonschema:"Optional replacement check using the same quality form as acceptance or test checks"`
	ReplacementKind  string `json:"replacement_kind,omitempty" jsonschema:"Optional replacement kind: acceptance or test; defaults to the superseded kind"`
	Lease            string `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized      *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCommentInput struct {
	ProjectDir     string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID             string `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken     string `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID        string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Kind           string `json:"kind" jsonschema:"note, decision, problem, lesson, or knowledge (required)"`
	Body           string `json:"body" jsonschema:"Durable self-contained Markdown comment with relevant context, rationale, impact, and references (required)"`
	IdempotencyKey string `json:"idempotency_key,omitempty" jsonschema:"Stable caller key; defaults to canonical kind and body"`
	Lease          string `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized    *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskDependencyInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string `json:"id" jsonschema:"Task that will depend on another task"`
	DependsOn   string `json:"depends_on" jsonschema:"Blocking task ID"`
	AgentID     string `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskBatchInput struct {
	ProjectDir  string                     `json:"project_dir" jsonschema:"Project directory (required)"`
	Operations  []graphtask.BatchOperation `json:"operations" jsonschema:"Ordered task mutations; 1 to 100 items"`
	AgentID     string                     `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Lease       string                     `json:"lease,omitempty" jsonschema:"Default lease for claim-renewing items; default 1h"`
	AiOptimized *bool                      `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

func taskActor(req *mcp.CallToolRequest, explicit string) string {
	if explicit != "" {
		return explicit
	}
	if req != nil && req.Extra != nil {
		if session := strings.TrimSpace(req.Extra.Header.Get(mcpproxy.AgentSessionHeader)); session != "" {
			return graphtask.AgentIDForSession(session)
		}
	}
	session := ""
	if req != nil && req.Session != nil {
		session = req.Session.ID()
	}
	if session == "" {
		session = fmt.Sprintf("mcp-%d", processID())
	}
	return graphtask.AgentIDForSession(session)
}

var processID = os.Getpid

func parseTaskLease(value string) (time.Duration, error) {
	if value == "" {
		return graphtask.DefaultLease, nil
	}
	d, err := time.ParseDuration(value)
	if err != nil || d <= 0 {
		return 0, fmt.Errorf("invalid positive lease duration %q", value)
	}
	return d, nil
}

func taskService(projectDir string) (*graphtask.Service, string, error) {
	dir, err := resolveProjectDir(projectDir)
	if err != nil {
		return nil, "", err
	}
	svc, err := graphtask.Open(dir)
	return svc, dir, err
}

func taskResult(value any, optimized *bool) (*mcp.CallToolResult, any, error) {
	if aiOpt(optimized) {
		return toonResult(value)
	}
	return jsonResult(value)
}

const defaultTaskSearchLimit = 20

type taskSearcher interface {
	Search(context.Context, string, int) ([]graphtask.SearchResult, error)
}

func paginateTaskSearch(ctx context.Context, searcher taskSearcher, in taskSearchInput) (page.Page[graphtask.SearchResult], error) {
	topK := in.TopK
	if topK == 0 {
		topK = defaultTaskSearchLimit
	}
	query := strings.TrimSpace(in.Query)
	window, err := openPage(in.PageSize, in.Cursor, topK, defaultTaskSearchLimit, struct {
		Tool, ProjectDir, Query string
		TopK                    int
	}{"task_search", in.ProjectDir, query, topK})
	if err != nil {
		return page.Page[graphtask.SearchResult]{}, err
	}
	results, err := searcher.Search(ctx, query, window.FetchLimit)
	if err != nil {
		return page.Page[graphtask.SearchResult]{}, err
	}
	return page.Finish(window, results), nil
}

func taskSearchResult(value page.Page[graphtask.SearchResult], optimized *bool) (*mcp.CallToolResult, any, error) {
	if aiOpt(optimized) {
		return textResult(paginationTOON(toon.FormatAny(value.Results), value.NextCursor))
	}
	return jsonResult(value)
}

func registerTaskTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "batch"), Description: "Run 1-100 task mutations in input order and return an explicit success or error for every item. Existing fencing and lifecycle checks apply to each item."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskBatchInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Batch(ctx, graphtask.BatchInput{Operations: in.Operations, Lease: in.Lease, Actor: taskActor(req, in.AgentID)})
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "create"), Description: "Create an idempotent open task in the shared LanceDB task store. Open and unclaimed is the backlog state."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCreateInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		priority := 2
		if in.Priority != nil {
			priority = *in.Priority
		}
		created, err := svc.Create(ctx, graphtask.CreateInput{Title: in.Title, Description: in.Description, AcceptanceCriteria: in.AcceptanceCriteria, Tests: in.Tests, Type: in.Type, Priority: priority, ParentID: in.ParentID, DependsOn: in.DependsOn, IdempotencyKey: in.IdempotencyKey, Actor: taskActor(req, in.AgentID)})
		if err != nil {
			return errResult(err)
		}
		return taskResult(created, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "get"), Description: "Read one authoritative task snapshot and its ordered audit history."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskGetInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Get(ctx, in.ID)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "export"), Description: "Export stable complete JSON for every project task or one exact task and its subtasks, including all public Task entities and audit history."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskExportInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Export(ctx, in.ID)
		if err != nil {
			return errResult(err)
		}
		return jsonResult(value)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "list"), Description: "List authoritative tasks. ready=true is the dependency-aware work queue; open and unclaimed tasks are the backlog."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskListInput) (*mcp.CallToolResult, any, error) {
		if in.Status != "" && in.Status != "blocked" && in.Status != "flagged" && !graphtask.ValidStatus(in.Status) {
			return errResult(fmt.Errorf("invalid task status %q", in.Status))
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.List(ctx, graphtask.ListOptions{Status: in.Status, Owner: in.Owner, ParentID: in.ParentID, Ready: in.Ready})
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "search"), Description: "Search prior and current tasks with LanceDB full-text ranking and opaque cursor pagination; use task_get for authoritative details."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSearchInput) (*mcp.CallToolResult, any, error) {
		svc, projectDir, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		in.ProjectDir = projectDir
		value, err := paginateTaskSearch(ctx, svc, in)
		if err != nil {
			return errResult(err)
		}
		return taskSearchResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "claim"), Description: "Atomically claim one ready task. Returns the fencing token required by every owner mutation."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskClaimInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Claim(ctx, in.ID, taskActor(req, in.AgentID), lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "force", "takeover"), Description: "Explicitly recover an unexpired in-progress claim from an unrecoverable owner using exact-ID confirmation, revision fencing, a reason, and token rotation."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskForceTakeoverInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.ForceTakeover(ctx, in.ID, taskActor(req, in.AgentID), graphtask.ForceTakeoverInput{ExpectedRevision: in.ExpectedRevision, ConfirmID: in.ConfirmID, Reason: in.Reason}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "progress"), Description: "Record a durable checkpoint and exact next step, fenced by the active claim."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskProgressInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Progress(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Summary, in.NextStep, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "heartbeat"), Description: "Renew the active task lease without changing its progress summary."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskHeartbeatInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Heartbeat(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "release"), Description: "Checkpoint and release a claim so another agent can continue immediately."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskReleaseInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Release(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Summary, in.NextStep)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "complete"), Description: "Complete a claimed task after acceptance checks pass, releasing its dependents."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCompleteInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Complete(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Summary)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "cancel"), Description: "Cancel a task with an audited reason. In-progress cancellation requires the current claim token."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCancelInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Cancel(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Reason)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "remove"), Description: "Hard-remove an unreferenced task only when exact-ID confirmation and a reason establish that deletion is correct."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskRemoveInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Remove(ctx, in.ID, in.ConfirmID, taskActor(req, in.AgentID), in.Reason)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "flag"), Description: "Flag a claimed task with a required reason. Work may continue or transfer, but completion is fenced until unflagged."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskFlagInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Flag(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Reason)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "unflag"), Description: "Remove a claimed task's flag after its reason has been resolved, allowing completion."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskUnflagInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Unflag(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID))
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "check"), Description: "Record pass/fail and concrete evidence for one acceptance or test check. Completion requires every check to pass."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCheckInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.VerifyCheck(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.CheckID, in.Passed, in.Evidence, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "revise"), Description: "Revise a claimed task specification with claim and expected-revision fencing, a required reason, and immutable before/after history."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskReviseInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Revise(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), graphtask.ReviseInput{ExpectedRevision: in.ExpectedRevision, Reason: in.Reason, Title: in.Title, Description: in.Description, Type: in.Type, Priority: in.Priority, ParentID: in.ParentID, DependsOn: in.DependsOn, AddAcceptanceCriteria: in.AddAcceptanceCriteria, AddTests: in.AddTests}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "check", "supersede"), Description: "Supersede an obsolete acceptance or test check without deleting history, optionally adding a replacement check."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCheckSupersedeInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.SupersedeCheck(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), graphtask.SupersedeCheckInput{ExpectedRevision: in.ExpectedRevision, CheckID: in.CheckID, Reason: in.Reason, ReplacementText: in.ReplacementText, ReplacementKind: in.ReplacementKind}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "comment", "add"), Description: "Append an idempotent, typed task comment for decisions, problems, lessons, knowledge, or other relevant work context."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCommentInput) (*mcp.CallToolResult, any, error) {
		lease, err := parseTaskLease(in.Lease)
		if err != nil {
			return errResult(err)
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.AddComment(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Kind, in.Body, in.IdempotencyKey, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "dependency", "add"), Description: "Add an explicit blocking dependency to an open task; cycles are rejected."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskDependencyInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.AddDependency(ctx, in.ID, in.DependsOn, taskActor(req, in.AgentID))
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	mcp.AddTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "dependency", "remove"), Description: "Remove an explicit blocking dependency from an open task."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskDependencyInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.RemoveDependency(ctx, in.ID, in.DependsOn, taskActor(req, in.AgentID))
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
}
