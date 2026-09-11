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
		"Graphit Task is the only task-control source of truth. Always use it instead of the host agent's native task/TODO/planning mechanism, even when that mechanism is available. The shared LanceDB tables own task state; do not create Markdown task logs or TODO/backlog files.",
		"Investigation, diagnosis, comparison, evaluation, architecture review, impact analysis, research, and any other analysis-only work are project work even when they produce no code or file change. Create, claim, checkpoint, and complete a Graphit task for them exactly as for implementation work. An analysis-only task is not complete until its full analytical result is durably recorded in Task for later analysis and system learning.",
		"",
		"## Enter work",
		"",
		"Before material work, search prior tasks with " + t("task", "search") + " and follow `next_cursor` until the relevant history is covered; use " + t("task", "list") + " with `ready: true` to find unblocked work and " + t("task", "get") + " to read dependencies, progress, next step, and audit history. Resume an existing task instead of duplicating it.",
		"Whenever you search project or imported Knowledge with `graphit_knowledge_search` or `graphit_wiki_search`, also run one focused " + t("task", "search") + " for the same question when Graphit Task is enabled and its MCP tools are available. Pass `ai_optimized: true`, follow `next_cursor` until the relevant history is covered, and read every relevant hit with " + t("task", "get") + ". Use its specification, analytical result, progress, comments, check evidence, next step, and audit history alongside the Knowledge pages. This paired Task-history pass supplements and never replaces authoritative `graphit_wiki_source` reads; if Task is disabled or unavailable, continue the Knowledge workflow.",
		"",
		"Create missing work with " + t("task", "create") + ", a stable `idempotency_key`, a complete specification, explicit acceptance criteria, and concrete tests/validations. Use `parent_id` for subtasks and dependency IDs for ordering; an unclaimed `open` task is the backlog. Add/remove dependencies with " + t("task", "dependency", "add") + "/" + t("task", "dependency", "remove") + "; never encode relations only in prose.",
		"Cleanup, validation, review, documentation, commit preparation, release checks, and similar delivery-support or finalization work are subtasks of the relevant delivery task, not unrelated top-level tasks. Use a check for a pass/fail condition; use a subtask when the validation or finalization itself is a work unit that must be owned, resumed, or audited.",
		"When discovery changes a claimed task's scope or reveals implementation-relevant detail missing from its description, use " + t("task", "revise") + " with the current task revision and a reason. Keep the description current with every relevant detail the model has discovered before delegation, handoff, or implementation begins; progress entries do not excuse leaving the executable specification incomplete. Supersede an obsolete check through " + t("task", "check", "supersede") + ", optionally creating its replacement; never falsify evidence or delete history to force completion.",
		"For two or more independent mutations, prefer " + t("task", "batch") + "; it runs in input order and reports every item. Inspect all results and never use batching to bypass claims, checks, flags, dependencies, or removal confirmation.",
		"",
		"## Specify work",
		"",
		"Keep titles as concise, action-oriented plain text that identifies one outcome. Treat every task description as an exhaustive, implementation-ready report rather than a proportional or skeletal brief. Include every relevant detail discovered by the model: objective and value; originating context and current state; in-scope and out-of-scope boundaries; requirements or externally observable behavior; exact code paths, symbols, callers, dependents, tests, data/control flow, and observed behavior; interfaces and dependencies; constraints and assumptions; alternatives considered, trade-offs, decisions, and rationale; material risks, edge cases, and failure modes; uncertainties and open questions; validation evidence and expected checks; and the intended result. Clearly distinguish verified facts, reasoned inferences, and unresolved unknowns. A different agent must be able to execute the task without repeating repository or code investigation merely to recover known context. Never omit a discovered execution-relevant fact because the task appears small or the detail could theoretically be rediscovered. Requirements must be necessary, singular, feasible, consistent, unambiguous, and implementation-independent unless an implementation choice is itself a constraint.",
		"",
		"Write each acceptance criterion as a singular imperative statement of what the system **must** do or **must not** allow, including the applicable condition and a measurable or observable expected result. State required behavior, not the implementation procedure. Write each behavioral test check in Given-When-Then form: known preconditions, one action or event, and observable outcomes. For non-behavioral validation, state the method or command, target and conditions, and expected evidence or result instead of forcing Gherkin. Include meaningful failure paths without duplicating equivalent scenarios.",
		"",
		"For analytical work, store the complete analytical report in the task before completion. Preserve the question and scope; method and sources consulted; code locations and relationships inspected; evidence and observations; intermediate reasoning that affects the conclusion; alternatives and counterarguments; trade-offs; constraints and assumptions; risks and uncertainty; confirmed conclusions; rejected hypotheses and why; unanswered questions; and concrete implications or recommendations for subsequent analysis or implementation. Use progress, typed comments, check evidence, and the completion summary together when needed, without loss or vague compression, so later agents can reuse the result without rerunning the analysis. If an analytical conclusion also qualifies as durable cross-task Memory, write it there too; Memory supplements but never replaces the complete Task record.",
		"",
		"Markdown is supported for descriptions, check text and evidence, progress and next steps, comments, reasons, and completion/release summaries. Every such field that documents task knowledge must be as detailed as its purpose requires and must preserve all relevant discoveries, reasoning, evidence, and implications known at that point. Use clear structure for long reports. Evidence names the exact command, source, observation, or artifact, relevant conditions, and actual result; progress records completed facts plus discoveries and their impact on scope or approach; next steps identify the exact action, target, prerequisites, and completion condition; comments preserve decisions, problems, lessons, system knowledge, alternatives, and rationale; reasons identify cause, evidence, impact, and the condition or replacement that resolves the change; release and completion summaries consolidate the full current state, outcomes, residual risks, and follow-on implications. Never use terse status phrases such as “analysis done,” “implemented,” or “tests pass” where they discard useful task knowledge. IDs, titles, types, statuses, priorities, actors, and timestamps remain compact plain text.",
		"",
		"Claim with " + t("task", "claim") + " before changing project state. A claim returns a fencing token; keep it private and pass it to progress, heartbeat, release, and complete. A rejected claim means another agent owns the task—choose other ready work. Never bypass a claim or edit task tables directly.",
		"",
		"## Advance and hand off",
		"",
		"After each independently reportable unit, call " + t("task", "progress") + " with the detailed result, every new discovery that can affect later work, and the exact next step. For analysis-only tasks, each checkpoint must accumulate the reusable analytical record rather than merely report that analysis occurred. Add relevant typed comments with " + t("task", "comment", "add") + " for decisions, problems, lessons, and system knowledge. Record every active acceptance/test result through " + t("task", "check") + " with concrete evidence; a parent cannot complete before all subtasks, and no task can complete with an unchecked/failed active check.",
		"",
		"When a risk or unresolved condition must gate completion, call " + t("task", "flag") + " with its reason; flagged work may be released and claimed by another agent, but cannot complete until " + t("task", "unflag") + " records resolution. Call " + t("task", "complete") + " only after every active check passes, all subtasks complete, and no flag remains. If stopping, blocked by external input, or handing off, call " + t("task", "release") + " with current evidence and next step; supported stop hooks release host-identifiable claims, leases recover crashes, and hooks reconcile invalid completion state.",
		"",
		"On a direction change, clean obsolete work immediately: use " + t("task", "cancel") + " when its history remains useful, or " + t("task", "remove") + " only when deletion is certainly correct. Removal requires the exact task ID and a reason and refuses referenced tasks. Never leave superseded work open or flagged as garbage.",
		"",
		"Every mutation is revision-checked and audited. Dependencies gate readiness; expired or stopped claims become open for takeover. Search is discovery; `get` and filtered `list` read authoritative state.",
		"",
		"Use " + t("task", "export") + " only when a machine-readable complete archive is required. Pass an exact task ID for that task and its subtasks, or omit it for every project task; the JSON includes all public Task entities in deterministic order and never exposes fencing tokens.",
		"",
		"Tool index: `graphit_task_search`, `graphit_task_list`, `graphit_task_get`, `graphit_task_export`, `graphit_task_batch`, `graphit_task_create`, `graphit_task_claim`, `graphit_task_revise`, `graphit_task_progress`, `graphit_task_heartbeat`, `graphit_task_comment_add`, `graphit_task_check`, `graphit_task_check_supersede`, `graphit_task_flag`, `graphit_task_unflag`, `graphit_task_release`, `graphit_task_complete`, `graphit_task_cancel`, `graphit_task_remove`, `graphit_task_dependency_add`, `graphit_task_dependency_remove`.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return agent.ModuleMandateTrigger(
		"Task", skillName,
		"starting, resuming, planning, analyzing, revising, delegating, checkpointing, blocking, handing off, cancelling, removing, or completing project work",
		"Before material project work, including work that only analyzes and changes no files, use Graphit Task—not the host's native task/TODO mechanism—so ownership, validation, knowledge capture, and resumability are established in LanceDB. Whenever Knowledge is searched, also search related prior/current Graphit tasks with `graphit_task_search`, follow `next_cursor` until relevant history is covered, and read every relevant result with `graphit_task_get`; use the complete Task record alongside Knowledge evidence. Task descriptions must be exhaustive execution-ready reports containing every relevant detail already discovered, so another agent can act without repeating code investigation. Record the complete, detailed analytical result in the task for reuse by later analyses and system learning; apply this same no-loss standard to progress, comments, evidence, reasons, handoffs, and completion summaries.",
		[]string{
			"finding current or prior work, including backlog",
			"performing analysis-only work or preserving its complete result, even when no project files change",
			"creating or relating work and dependencies",
			"creating or completing subtasks, acceptance criteria, or test checks",
			"revising task scope or superseding obsolete checks",
			"recording task decisions, problems, lessons, or learned system knowledge",
			"claiming, progressing, releasing, taking over, or completing work",
			"cancelling or certainly removing obsolete work so no task garbage remains",
			"an agent starts, stops, resumes, delegates, or finishes a work unit",
		},
		[]string{"task_search", "task_list", "task_get", "task_create", "task_claim", "task_revise", "task_progress", "task_check", "task_release", "task_complete"},
	)
}
