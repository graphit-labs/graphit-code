package task

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

func RuleContent() string {
	t := func(parts ...string) string { return "`" + brand.MCPToolName(parts...) + "`" }
	return strings.Join([]string{
		"# Graphit Task",
		"",
		"`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host.",
		"",
		"Task owns durable sessions, specifications, plans, backlog, checks and lifecycle, including analysis. Use MCP; never edit tables or substitute the CLI. Native plans may supplement these records.",
		"",
		"## Start or resume the session",
		"",
		"Before creating/resuming a session, changing its scope, handing off or closing it, read [references/session.md](references/session.md). A session preserves one evolving demand across agents/turns; it is not a host chat ID or a Task. Known session → " + t("task", "session", "get") + "; otherwise " + t("task", "session", "list") + " for open/in_progress sessions or focused " + t("task", "session", "search") + ". Read matching intent, checkpoints and selected associated Tasks before resuming or creating work. Reuse sufficient evidence; retrieve sessions and Tasks again whenever new questions arise.",
		"Finding an open session does not settle it: DECIDE by scope. The same evolving demand continues in its session across interruption, compaction and agent replacement; changed scope revises it; only a different demand gets its own.",
		"With no matching session, " + t("task", "session", "create") + " saves a detailed description of the user's demand, scope/constraints, unknowns and strategy; then " + t("task", "session", "claim") + " takes coordination. Every agent-created Task must have this `session_id`, including planning and batch items. Delegate the ID and Task IDs; workers claim their Tasks without taking the coordinator's session claim. Session and Task claims are independent; keep both tokens private. Manual CLI Tasks may omit a session; agents must not bypass association.",
		"When the request or approach changes, " + t("task", "session", "revise") + " updates current description/strategy with latest `expected_revision` and reason before affected execution; reconcile the Task graph too. At meaningful outcomes " + t("task", "session", "checkpoint") + " records progress/evidence, problems, decisions/rationale, strategy and exact next action, referencing Task detail. Hooks remind/recover; they cannot invent this content. Before stopping, preserve a handoff and release claims. Ending a turn/compaction is not completion. Only " + t("task", "session", "complete") + " closes delivered work after linked Tasks are terminal and final scope/evidence is reconciled; cancelled Tasks are not delivered requirements.",
		"",
		"## Retrieve work and prior knowledge",
		"",
		"Read a known Task with " + t("task", "get") + "; otherwise focused " + t("task", "search") + " before creating work. Use " + t("task", "list") + " with `session_id` for associated work, `ready: true` for available work or `parent_id` for children. Read selected records/results. Throughout exploration, coding and review, recall prior analysis/decisions when a new doubt exceeds retained context, not only at startup or a blocker.",
		"For a question about records you can already name — the status of a set of ids, which checks are still pending — use " + t("task", "query") + ": one call filters by predicate and returns only the columns asked for, instead of fetching records to read one field. " + t("task", "schema") + " lists tables and columns before a first filter.",
		"Pass `ai_optimized: true`. Search discovers; get/filtered list establish state. Use small `top_k`/`page_size`; follow `next_cursor` only for missing context/completeness. Cursors preserve the query/total cap; raising it needs a new search. Avoid whole-backlog exports for one question.",
		"",
		"## Define work before implementation",
		"",
		"Create/claim an investigation/planning Task in the session. Inspect current AST behavior, Knowledge contracts and relevant history before deciding the delivery map. Resolve material questions; keep unknowns as refinement prerequisites, not invented defaults. Investigation is not permission to execute an entire project as one Task.",
		"Before defining/revising multi-outcome work, read [references/planning.md](references/planning.md). Before saving its backlog/batch, read [references/worked-feature.md](references/worked-feature.md) for filled contracts, fixtures, leaves, coverage and claim sequence. Reuse retained references.",
		"For a whole system, also read [references/worked-system.md](references/worked-system.md) before saving its backlog: umbrella -> deliveries -> subtasks, shared contracts and system acceptance.",
		"Persist specification -> technical plan -> task/subtask graph -> readiness review before implementation or delegating implementation. Record journeys/priorities, measurable requirements, data/error/boundary contracts, decisions and unknowns. Determine the actual delivery units, prerequisites and compatible parallel work from that evidence; give EACH unit a complete description, acceptance_criteria and tests before its implementation begins. A large prompt needs a delivery hierarchy, not one generic task. A small single-outcome fix can keep all stages in one packet.",
		"Read back records; map every requirement to returned Task/check IDs, integration and documentation proof. Check gaps, contradictions, stale context, unjustified work, unavailable validation and dependency deadlocks. Each leaf must support another agent without this conversation. Planning checks prove the plan, not product behavior. Complete planning separately; leave deliveries open/unclaimed. Planning authorization does not authorize implementation.",
		"Before implementing/resuming a leaf, reviewing completion or handing off, read [references/execution.md](references/execution.md). For a small fix, its filled one-task example is sufficient; do not load the full feature example just to repair one outcome. On discovery that changes scope, update affected descriptions/contracts, coverage, checks and ordering before affected implementation; preserve completed evidence and pending reconciliation explicitly.",
		"Without local files, " + t("module", "skill") + " with `module: task`, `project_dir` and exact `reference: references/...md` loads one guide, including session.md. Omit `reference` for entrypoint/names; never preload every body. Adapt illustrative languages, paths and commands to the project; preserve rigor, not its stack or task count.",
		"",
		"## Use the framework at the decision boundary",
		"",
		"Read target skills: Knowledge for contracts, AST for code/impact, Memory for constraints, Hub for discovery. Known/cluster-returned `dir` → `project_dir` on all neighbor module reads; keep execution records in the delivery's project. Neighbors remain Graphit-managed, never native grep/walk. Otherwise cluster before Hub; for Hub-only Knowledge/AST follow its install/context route. Public technologies use official docs. Compare history to current AST/Knowledge and save plan-changing evidence.",
		"",
		"## Create and revise safely",
		"",
		"Use " + t("task", "create") + " with `description`, `acceptance_criteria`, `tests` and a stable `idempotency_key` per logical outcome. Specs/plans live in descriptions; do not invent spec/plan tools or fields. Create referenced tasks first and use returned IDs. Add/remove prerequisites with " + t("task", "dependency", "add") + "/" + t("task", "dependency", "remove") + " while the target is open. Validation, documentation, review and finalization work belongs under its delivery. Use a check for a condition; a subtask for separately owned work.",
		"Claim with " + t("task", "claim") + " before material work: only ready Tasks, one per agent. Keep `claim_token` private and use the same identity. Never bypass dependencies/ownership. Renew through progress or " + t("task", "heartbeat") + " before expiry; renew the separate coordination lease when held.",
		"Use " + t("task", "revise") + " before changed implementation/handoff; progress cannot revise the specification. Revision/comments need a live claim. Use current `expected_revision` and reason; changed title/description/type resets active checks. Supersede obsolete checks through " + t("task", "check", "supersede") + ", never mark them passed. [Execution](references/execution.md) covers field semantics and ready/blocked revision recovery; preserve real dependencies and reconcile blocked correction packages before coding.",
		"Prefer " + t("task", "batch") + " for fully planned `create` items and other mutations with known inputs (1–100). Keep each full spec/checks. Inspect every ordered result; successes persist without rollback. `key` only correlates; split calls requiring returned IDs/tokens/revisions. Retry failed/uncertain creates with original idempotency keys; a repeated create never revises saved content. Batch `revise` appends `acceptance_criteria`/`tests`; direct revise uses `add_*`.",
		"",
		"## Checkpoint, validate and hand off",
		"",
		"At meaningful outcomes compare code and docs in both directions. Load Knowledge before maintaining user/technical surfaces; record targets/evidence or grounded no-impact and resolve drift before closing the unit. Call " + t("task", "progress") + " with new facts/artifacts, evidence and exact `next_step` (action, target, prerequisite, done condition); the coordinator checkpoints session-wide effects. Reference context instead of logging every read. Use " + t("task", "comment", "add") + " for consequential decisions/problems/lessons. Analysis retains question, method/sources, evidence, conclusions, rationale, uncertainty and implications; 'analysis done' is not a result. Promote reusable conclusions to Memory.",
		"Record each active check through " + t("task", "check") + " with `checks[].id` as `check_id`, actual pass/fail and concrete command/observation/artifact evidence. Compare delivered behavior against requirement coverage, integration and nonfunctional constraints. If a required check cannot run, record the limitation and leave it unresolved; planned or assumed success is not evidence.",
		"Use " + t("task", "flag") + " for unresolved conditions that gate completion; record resolution before " + t("task", "unflag") + ". Call " + t("task", "complete") + " only after every active check passes, dependencies/subtasks are complete and no flag remains. Summarize outcomes, evidence and limitations. When blocked/stopping/handing off, use " + t("task", "release") + " with current state and exact continuation; hooks/leases do not replace a handoff.",
		"Cancel obsolete work with " + t("task", "cancel") + " and reason/replacement. " + t("task", "remove") + " needs justified deletion, exact `confirm_id` and reason; references block it. " + t("task", "export") + " archives `id` with descendants, or the project if omitted. Handoff reports relevant IDs, outcomes and next work.",
	}, "\n") + "\n"
}

// WorkerMandateTrigger is the Task routing a delegated performer needs. It is a
// separate text because the coordinator's rule is written around owning a
// session — creating one, claiming it, closing it — and handing that to a
// performer is what made workers open a second session for work they had
// already been given.
func WorkerMandateTrigger() string {
	return agent.ModuleMandateTrigger(
		"Task", skillName,
		"reading the work you were assigned, recording its progress, or recalling prior decisions and evidence",
		"Read the session and Task ids the coordinator gave you; search only while a relevant gap remains. Claim at most your own Task, and never create, claim or close a coordination session. Record progress and findings against the Task you were given, with evidence and the exact next action. Recall prior Tasks whenever a new doubt exceeds what you were handed.",
		nil,
		[]string{"task_get", "task_search", "task_progress", "task_comment_add"},
	)
}

func MandateTrigger() string {
	return agent.ModuleMandateTrigger(
		"Task", skillName,
		"starting/resuming project work, planning, delegating/completing it, or answering questions and investigating system/history gaps at any stage",
		"Read the open sessions and DECIDE by scope, not recency: the same evolving demand continues in its session across interruptions, changed scope revises that session and its affected Tasks, and only a different demand gets a new one. With no match, create a detailed demand/strategy session and claim coordination. Read session.md before its lifecycle actions. Bind every agent-created Task to session_id; workers share it without taking coordination. Investigate before saving specification -> plan -> dependency-ordered Tasks with checks; batch planned packets with known IDs and inspect each result. Keep leaves usable without this conversation. Checkpoint progress, problems, decisions, strategy and next action at meaningful outcomes. Recall sessions/Tasks whenever new doubts arise. Handoff preserves state and releases claims; a turn ending is not completion. Close the session explicitly only after linked Tasks are terminal and scope/evidence is reconciled; cancelled work is not delivered.",
		nil,
		[]string{"task_session_get", "task_session_list", "task_session_create", "task_session_claim", "task_get", "task_query", "task_create"},
	)
}
