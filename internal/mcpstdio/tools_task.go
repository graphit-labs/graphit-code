package mcpstdio

import (
	"context"
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"os"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lancequery"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/toon"
)

type taskCreateInput struct {
	References         *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed relationships. Send the complete list when referencing records; omit to preserve existing links, send [] to clear. Each target requires type and id; qualify cross-scope targets."`
	ProjectDir         string           `json:"project_dir" jsonschema:"Project directory (required)"`
	Title              string           `json:"title" jsonschema:"Concise action-oriented plain-text title naming one outcome (required)"`
	Description        string           `json:"description" jsonschema:"Self-contained executable specification: goal, scope, requirement IDs/behavior, known sources, constraints, contracts, approach, dependencies, risks/unknowns and validation. Preserve execution-relevant detail without repeating history. Split multi-outcome work into related tasks before implementation (required)"`
	AcceptanceCriteria []string         `json:"acceptance_criteria" jsonschema:"One singular imperative Markdown statement per item: what the system must do or must not allow, with condition and observable expected result; at least one required"`
	Tests              []string         `json:"tests" jsonschema:"Behavior checks in Given-When-Then; other validations name method/command, target/conditions, and expected evidence/result; at least one Markdown item required"`
	Type               string           `json:"type,omitempty" jsonschema:"Task type such as task, bug, feature, epic, or chore"`
	Priority           *int             `json:"priority,omitempty" jsonschema:"Priority 0 (critical) through 4 (lowest); default 2"`
	SessionID          string           `json:"session_id,omitempty" jsonschema:"Logical Task session ID; inferred from parent or current coordinator when omitted; an agent task must belong to a session"`
	ParentID           string           `json:"parent_id,omitempty" jsonschema:"Parent delivery task ID for a subtask; use for cleanup, validation, review, documentation, commit preparation, release checks, and similar finalization work"`
	DependsOn          []string         `json:"depends_on,omitempty" jsonschema:"Task IDs that must complete first"`
	IdempotencyKey     string           `json:"idempotency_key,omitempty" jsonschema:"Stable caller key; defaults to the canonical title"`
	AgentID            string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	AiOptimized        *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
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
	SessionID   string `json:"session_id,omitempty" jsonschema:"Only tasks associated with this logical Task session"`
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Status      string `json:"status,omitempty" jsonschema:"open, blocked, flagged, in_progress, completed, or cancelled"`
	Owner       string `json:"owner,omitempty" jsonschema:"Filter by exact agent owner"`
	ParentID    string `json:"parent_id,omitempty" jsonschema:"Return only direct subtasks of this task ID"`
	Ready       bool   `json:"ready,omitempty" jsonschema:"Return only open tasks with all dependencies completed"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskSearchInput struct {
	SessionID   string `json:"session_id,omitempty" jsonschema:"Rank only tasks associated with this logical Task session"`
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
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed relationships. Send the complete list when referencing records; omit to preserve existing links, send [] to clear. Each target requires type and id; qualify cross-scope targets."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Summary     string           `json:"summary" jsonschema:"Markdown checkpoint of completed facts, changed constraints, and concrete evidence (required)"`
	NextStep    string           `json:"next_step,omitempty" jsonschema:"Markdown exact next action with target and completion condition for this or a takeover agent"`
	Lease       string           `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
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
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Summary     string           `json:"summary,omitempty" jsonschema:"Markdown summary of completed work, current state, and blocking evidence"`
	NextStep    string           `json:"next_step,omitempty" jsonschema:"Markdown exact continuation action with target and completion condition for the next agent"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCompleteInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Summary     string           `json:"summary,omitempty" jsonschema:"Markdown final result mapped to acceptance evidence, with any residual limitations"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCancelInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Task ID to cancel (required)"`
	ClaimToken  string           `json:"claim_token,omitempty" jsonschema:"Required fencing token when the task is in progress"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Reason      string           `json:"reason" jsonschema:"Markdown explanation of why the task is no longer needed and what replaces it, if anything (required)"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
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
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Reason      string           `json:"reason" jsonschema:"Markdown unresolved condition, completion impact, and objective clearing condition (required)"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskUnflagInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCheckInput struct {
	References  *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir  string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID          string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken  string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID     string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	CheckID     string           `json:"check_id" jsonschema:"Acceptance or test check ID (required)"`
	Passed      bool             `json:"passed" jsonschema:"Whether this check passed"`
	Evidence    string           `json:"evidence" jsonschema:"Markdown evidence naming the command, observation, or artifact, relevant conditions, and actual result (required)"`
	Lease       string           `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskReviseInput struct {
	References            *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed relationships. Send the complete list when referencing records; omit to preserve existing links, send [] to clear. Each target requires type and id; qualify cross-scope targets."`
	ProjectDir            string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID                    string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken            string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID               string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	ExpectedRevision      int64            `json:"expected_revision" jsonschema:"Current task revision used as a compare-and-swap fence (required)"`
	Reason                string           `json:"reason" jsonschema:"Markdown rationale for the specification change and its scope or verification impact (required)"`
	Title                 *string          `json:"title,omitempty" jsonschema:"Replacement concise action-oriented plain-text title naming one outcome"`
	Description           *string          `json:"description,omitempty" jsonschema:"Replacement executable specification; preserve requirement coverage, known sources, contracts, approach, constraints, risks/unknowns and validation so another agent can continue without the conversation"`
	Type                  *string          `json:"type,omitempty" jsonschema:"Replacement task type"`
	Priority              *int             `json:"priority,omitempty" jsonschema:"Replacement priority 0 through 4"`
	ParentID              *string          `json:"parent_id,omitempty" jsonschema:"Replacement parent task ID; empty clears the parent"`
	DependsOn             *[]string        `json:"depends_on,omitempty" jsonschema:"Complete replacement dependency list; empty clears dependencies"`
	AddAcceptanceCriteria []string         `json:"add_acceptance_criteria,omitempty" jsonschema:"New singular imperative Markdown statements of what the system must do or must not allow"`
	AddTests              []string         `json:"add_tests,omitempty" jsonschema:"New Given-When-Then behavior checks or explicit method-target-expected-result validations to append"`
	Lease                 string           `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized           *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCheckSupersedeInput struct {
	References       *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed references: full list replaces, omitted preserves, [] clears. Resolve target type and id before writing."`
	ProjectDir       string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID               string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken       string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID          string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	ExpectedRevision int64            `json:"expected_revision" jsonschema:"Current task revision used as a compare-and-swap fence (required)"`
	CheckID          string           `json:"check_id" jsonschema:"Active acceptance or test check ID (required)"`
	Reason           string           `json:"reason" jsonschema:"Markdown rationale explaining why this check no longer represents the task specification (required)"`
	ReplacementText  string           `json:"replacement_text,omitempty" jsonschema:"Optional replacement check using the same quality form as acceptance or test checks"`
	ReplacementKind  string           `json:"replacement_kind,omitempty" jsonschema:"Optional replacement kind: acceptance or test; defaults to the superseded kind"`
	Lease            string           `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized      *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskCommentInput struct {
	References     *[]relations.Ref `json:"references,omitempty" jsonschema:"Explicit typed relationships. Send the complete list when referencing records; omit to preserve existing links, send [] to clear. Each target requires type and id; qualify cross-scope targets."`
	ProjectDir     string           `json:"project_dir" jsonschema:"Project directory (required)"`
	ID             string           `json:"id" jsonschema:"Claimed task ID (required)"`
	ClaimToken     string           `json:"claim_token" jsonschema:"Fencing token returned by claim (required)"`
	AgentID        string           `json:"agent_id,omitempty" jsonschema:"Stable current-agent identity; host session identity is used when omitted"`
	Kind           string           `json:"kind" jsonschema:"note, decision, problem, lesson, or knowledge (required)"`
	Body           string           `json:"body" jsonschema:"Durable self-contained Markdown comment with relevant context, rationale, impact, and references (required)"`
	IdempotencyKey string           `json:"idempotency_key,omitempty" jsonschema:"Stable caller key; defaults to canonical kind and body"`
	Lease          string           `json:"lease,omitempty" jsonschema:"Renewed lease duration such as 2h; never shortens a longer active lease"`
	AiOptimized    *bool            `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
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

type taskSchemaInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Table       string `json:"table,omitempty" jsonschema:"Describe only this table; omit for every table"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

type taskQueryInput struct {
	ProjectDir  string   `json:"project_dir" jsonschema:"Project directory (required)"`
	Table       string   `json:"table" jsonschema:"Table to query, such as tasks, task_checks, task_comments or task_sessions; task_schema lists them (required)"`
	Filter      string   `json:"filter,omitempty" jsonschema:"Lance SQL predicate over this table's columns, for example \"id IN ('tsk-a','tsk-b')\" or \"status = 'completed' AND session_id = 'ses-x'\". This is a WHERE clause only: there is no SELECT, JOIN, GROUP BY, aggregate or ORDER BY. Omit to match every row."`
	Columns     []string `json:"columns,omitempty" jsonschema:"Columns to return, for example [id, status]. Omit for every compact column; long columns such as description and search_text, and the claim token, are excluded unless named, and the claim token is never returned at all."`
	TopK        int      `json:"top_k,omitempty" jsonschema:"Total row cap across pages (0 = no cap)"`
	PageSize    int      `json:"page_size,omitempty" jsonschema:"Rows per page (default: 20, max: 100); top_k remains the total-result cap"`
	Cursor      string   `json:"cursor,omitempty" jsonschema:"Opaque next_cursor returned by the preceding page of this exact query"`
	AiOptimized *bool    `json:"ai_optimized,omitempty" jsonschema:"Set false for verbose JSON; default compact TOON"`
}

func registerTaskInspectionTools(server *mcp.Server) {
	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("task", "schema"),
		Description: "Show the Task LanceDB tables: every column with its type, and the row count. " +
			"Read this before writing a task_query filter.",
	}, safeTool(func(ctx context.Context, _ *mcp.CallToolRequest, in taskSchemaInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		var only []string
		if table := strings.TrimSpace(in.Table); table != "" {
			only = []string{table}
		}
		value, err := svc.DescribeStore(ctx, only)
		if err != nil {
			return errResult(err)
		}
		return lanceSchemaResult(value, in.AiOptimized)
	}))

	addTool(server, &mcp.Tool{
		Name: brand.MCPToolName("task", "query"),
		Description: "Answer a structured question about known task records: filter rows by predicate and return only the columns asked for. " +
			"Use this instead of reading whole records when the question is \"which of these, and what state\" — the status of a set of ids is one call. " +
			"task_search ranks by relevance when the target is unknown; task_get returns one authoritative record in full. " +
			"The filter is a WHERE clause, not SQL: no SELECT, JOIN, GROUP BY or ORDER BY, and results carry no ordering.",
	}, safeTool(func(ctx context.Context, _ *mcp.CallToolRequest, in taskQueryInput) (*mcp.CallToolResult, any, error) {
		svc, projectDir, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := paginateTaskQuery(ctx, svc, projectDir, in)
		if err != nil {
			return errResult(err)
		}
		return lanceQueryResult(value, in.AiOptimized)
	}))
}

// paginateTaskQuery binds every input that changes the result set, so a cursor minted for one
// question cannot be replayed against another. Leaving the filter or the projection out of the
// binding would let a cursor walk a different query's rows, which is worse than no paging.
func paginateTaskQuery(
	ctx context.Context, svc taskStoreQuerier, projectDir string, in taskQueryInput,
) (page.Page[map[string]any], error) {
	table := strings.TrimSpace(in.Table)
	window, err := openPage(in.PageSize, in.Cursor, in.TopK, defaultTaskSearchLimit, struct {
		Tool, ProjectDir, Table, Filter string
		Columns                         []string
		TopK                            int
	}{"task_query", projectDir, table, in.Filter, in.Columns, in.TopK})
	if err != nil {
		return page.Page[map[string]any]{}, err
	}
	result, err := svc.QueryStore(ctx, lancequery.Request{
		Table:   table,
		Filter:  in.Filter,
		Columns: in.Columns,
		// FetchLimit is a PREFIX LENGTH measured from row zero, for a ranked source that
		// cannot skip. LanceDB can skip, so the rows to take are the tail of that prefix.
		// Passing FetchLimit straight through would re-read the earlier pages on every page.
		Limit:  window.FetchLimit - window.Offset,
		Offset: window.Offset,
	})
	if err != nil {
		return page.Page[map[string]any]{}, err
	}
	return page.FinishFetched(window, result.Rows), nil
}

const defaultTaskSearchLimit = 20

type taskSearcher interface {
	SearchInSession(context.Context, string, int, string) ([]graphtask.SearchResult, error)
}

type taskStoreQuerier interface {
	QueryStore(context.Context, lancequery.Request) (lancequery.Result, error)
}

func paginateTaskSearch(ctx context.Context, searcher taskSearcher, in taskSearchInput) (page.Page[graphtask.SearchResult], error) {
	topK := in.TopK
	if topK == 0 {
		topK = defaultTaskSearchLimit
	}
	query := strings.TrimSpace(in.Query)
	window, err := openPage(in.PageSize, in.Cursor, topK, defaultTaskSearchLimit, struct {
		Tool, ProjectDir, Query, SessionID string
		TopK                               int
	}{"task_search", in.ProjectDir, query, in.SessionID, topK})
	if err != nil {
		return page.Page[graphtask.SearchResult]{}, err
	}
	results, err := searcher.SearchInSession(ctx, query, window.FetchLimit, in.SessionID)
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
	registerTaskSessionTools(server)
	registerTaskInspectionTools(server)
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "batch"), Description: "Run 1-100 task mutations in input order and return an explicit success or error for every item. Existing fencing and lifecycle checks apply to each item."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskBatchInput) (*mcp.CallToolResult, any, error) {
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.Batch(ctx, graphtask.BatchInput{RequireSession: true, Operations: in.Operations, Lease: in.Lease, Actor: taskActor(req, in.AgentID)})
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "create"), Description: "Create an idempotent open task in the shared LanceDB task store. Open and unclaimed is the backlog state."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCreateInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		priority := 2
		if in.Priority != nil {
			priority = *in.Priority
		}
		created, err := svc.Create(ctx, graphtask.CreateInput{SessionID: in.SessionID, RequireSession: true, Title: in.Title, Description: in.Description, AcceptanceCriteria: in.AcceptanceCriteria, Tests: in.Tests, Type: in.Type, Priority: priority, ParentID: in.ParentID, DependsOn: in.DependsOn, IdempotencyKey: in.IdempotencyKey, Actor: taskActor(req, in.AgentID)})
		if err != nil {
			return errResult(err)
		}
		return taskResult(created, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "get"), Description: "Read one authoritative task snapshot and its ordered audit history."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskGetInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "export"), Description: "Export stable complete JSON for every project task or one exact task and its subtasks, including all public Task entities and audit history."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskExportInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "list"), Description: "List authoritative tasks. ready=true is the dependency-aware work queue; open and unclaimed tasks are the backlog."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskListInput) (*mcp.CallToolResult, any, error) {
		if in.Status != "" && in.Status != "blocked" && in.Status != "flagged" && !graphtask.ValidStatus(in.Status) {
			return errResult(fmt.Errorf("invalid task status %q", in.Status))
		}
		svc, _, err := taskService(in.ProjectDir)
		if err != nil {
			return errResult(err)
		}
		value, err := svc.List(ctx, graphtask.ListOptions{SessionID: in.SessionID, Status: in.Status, Owner: in.Owner, ParentID: in.ParentID, Ready: in.Ready})
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "search"), Description: "Search prior and current tasks with LanceDB full-text ranking and opaque cursor pagination; use task_get for authoritative details."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskSearchInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "claim"), Description: "Atomically claim one ready task. Returns the fencing token required by every owner mutation."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskClaimInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "force", "takeover"), Description: "Explicitly recover an unexpired in-progress claim from an unrecoverable owner using exact-ID confirmation, revision fencing, a reason, and token rotation."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskForceTakeoverInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "progress"), Description: "Record a durable checkpoint and exact next step, fenced by the active claim."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskProgressInput) (*mcp.CallToolResult, any, error) {
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
		value, err := svc.Progress(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Summary, in.NextStep, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "heartbeat"), Description: "Renew the active task lease without changing its progress summary."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskHeartbeatInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "release"), Description: "Checkpoint and release a claim so another agent can continue immediately."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskReleaseInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "complete"), Description: "Complete a claimed task after acceptance checks pass, releasing its dependents."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCompleteInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "cancel"), Description: "Cancel a task with an audited reason. In-progress cancellation requires the current claim token."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCancelInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "remove"), Description: "Hard-remove an unreferenced task only when exact-ID confirmation and a reason establish that deletion is correct."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskRemoveInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "flag"), Description: "Flag a claimed task with a required reason. Work may continue or transfer, but completion is fenced until unflagged."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskFlagInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "unflag"), Description: "Remove a claimed task's flag after its reason has been resolved, allowing completion."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskUnflagInput) (*mcp.CallToolResult, any, error) {
		if err := relations.Validate(in.References); err != nil {
			return errResult(err)
		}
		ctx = relations.WithInputs(ctx, in.References)
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "check"), Description: "Record pass/fail and concrete evidence for one acceptance or test check. Completion requires every check to pass."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCheckInput) (*mcp.CallToolResult, any, error) {
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
		value, err := svc.VerifyCheck(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.CheckID, in.Passed, in.Evidence, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "revise"), Description: "Revise a claimed task specification with claim and expected-revision fencing, a required reason, and immutable before/after history."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskReviseInput) (*mcp.CallToolResult, any, error) {
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
		value, err := svc.Revise(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), graphtask.ReviseInput{ExpectedRevision: in.ExpectedRevision, Reason: in.Reason, Title: in.Title, Description: in.Description, Type: in.Type, Priority: in.Priority, ParentID: in.ParentID, DependsOn: in.DependsOn, AddAcceptanceCriteria: in.AddAcceptanceCriteria, AddTests: in.AddTests}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "check", "supersede"), Description: "Supersede an obsolete acceptance or test check without deleting history, optionally adding a replacement check."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCheckSupersedeInput) (*mcp.CallToolResult, any, error) {
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
		value, err := svc.SupersedeCheck(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), graphtask.SupersedeCheckInput{ExpectedRevision: in.ExpectedRevision, CheckID: in.CheckID, Reason: in.Reason, ReplacementText: in.ReplacementText, ReplacementKind: in.ReplacementKind}, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "comment", "add"), Description: "Append an idempotent, typed task comment for decisions, problems, lessons, knowledge, or other relevant work context."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskCommentInput) (*mcp.CallToolResult, any, error) {
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
		value, err := svc.AddComment(ctx, in.ID, in.ClaimToken, taskActor(req, in.AgentID), in.Kind, in.Body, in.IdempotencyKey, lease)
		if err != nil {
			return errResult(err)
		}
		return taskResult(value, in.AiOptimized)
	}))
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "dependency", "add"), Description: "Add an explicit blocking dependency to an open task; cycles are rejected."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskDependencyInput) (*mcp.CallToolResult, any, error) {
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
	addTool(server, &mcp.Tool{Name: brand.MCPToolName("task", "dependency", "remove"), Description: "Remove an explicit blocking dependency from an open task."}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, in taskDependencyInput) (*mcp.CallToolResult, any, error) {
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
