package task

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

func RuleContent() string {
	t := func(parts ...string) string { return "`" + brand.MCPToolName(parts...) + "`" }
	return strings.Join([]string{
		"# Graphit Task",
		"",
		"Graphit Task is the only task-control source of truth. Always use it instead of the host agent's native task/TODO/planning mechanism, even when that mechanism is available. The shared LanceDB tables own task state; do not create Markdown task logs or TODO/backlog files.",
		"",
		"## Enter work",
		"",
		"Before material work, search prior tasks with " + t("task", "search") + " and follow `next_cursor` until the relevant history is covered; use " + t("task", "list") + " with `ready: true` to find unblocked work and " + t("task", "get") + " to read dependencies, progress, next step, and audit history. Resume an existing task instead of duplicating it.",
		"",
		"Create missing work with " + t("task", "create") + ", a stable `idempotency_key`, robust self-contained description (objective, context, scope, constraints, intended result), explicit acceptance criteria, and concrete tests/validations. Use `parent_id` for subtasks and dependency IDs for ordering; an unclaimed `open` task is the backlog. Add/remove dependencies with " + t("task", "dependency", "add") + "/" + t("task", "dependency", "remove") + "; never encode relations only in prose.",
		"For two or more independent mutations, prefer " + t("task", "batch") + "; it runs in input order and reports every item. Inspect all results and never use batching to bypass claims, checks, flags, dependencies, or removal confirmation.",
		"",
		"Claim with " + t("task", "claim") + " before changing project state. A claim returns a fencing token; keep it private and pass it to progress, heartbeat, release, and complete. A rejected claim means another agent owns the task—choose other ready work. Never bypass a claim or edit task tables directly.",
		"",
		"## Advance and hand off",
		"",
		"After each independently reportable unit, call " + t("task", "progress") + " with what landed and the exact next step. Add relevant typed comments with " + t("task", "comment", "add") + " for decisions, problems, lessons, and system knowledge. Record every acceptance/test result through " + t("task", "check") + " with concrete evidence; a parent cannot complete before all subtasks, and no task can complete with an unchecked/failed check.",
		"",
		"When a risk or unresolved condition must gate completion, call " + t("task", "flag") + " with its reason; flagged work may be released and claimed by another agent, but cannot complete until " + t("task", "unflag") + " records resolution. Call " + t("task", "complete") + " only after every check passes, all subtasks complete, and no flag remains. If stopping, blocked by external input, or handing off, call " + t("task", "release") + " with current evidence and next step; supported stop hooks release host-identifiable claims, leases recover crashes, and hooks reconcile invalid completion state.",
		"",
		"On a direction change, clean obsolete work immediately: use " + t("task", "cancel") + " when its history remains useful, or " + t("task", "remove") + " only when deletion is certainly correct. Removal requires the exact task ID and a reason and refuses referenced tasks. Never leave superseded work open or flagged as garbage.",
		"",
		"Every mutation is revision-checked and audited. Dependencies gate readiness; expired or stopped claims become open for takeover. Search is discovery; `get` and filtered `list` read authoritative state.",
		"",
		"Tool index: `graphit_task_search`, `graphit_task_list`, `graphit_task_get`, `graphit_task_batch`, `graphit_task_create`, `graphit_task_claim`, `graphit_task_progress`, `graphit_task_heartbeat`, `graphit_task_comment_add`, `graphit_task_check`, `graphit_task_flag`, `graphit_task_unflag`, `graphit_task_release`, `graphit_task_complete`, `graphit_task_cancel`, `graphit_task_remove`, `graphit_task_dependency_add`, `graphit_task_dependency_remove`.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return ide.ModuleMandateTrigger(
		"Task", skillName,
		"starting, resuming, planning, delegating, checkpointing, blocking, handing off, cancelling, removing, or completing project work",
		"Before material project changes, use Graphit Task—not the host's native task/TODO mechanism—so ownership, validation, and resumability are established in LanceDB.",
		[]string{
			"finding current or prior work, including backlog",
			"creating or relating work and dependencies",
			"creating or completing subtasks, acceptance criteria, or test checks",
			"recording task decisions, problems, lessons, or learned system knowledge",
			"claiming, progressing, releasing, taking over, or completing work",
			"cancelling or certainly removing obsolete work so no task garbage remains",
			"an agent starts, stops, resumes, delegates, or finishes a work unit",
		},
		[]string{"task_search", "task_list", "task_get", "task_batch", "task_create", "task_claim", "task_progress", "task_comment_add", "task_check", "task_flag", "task_release", "task_complete", "task_cancel", "task_remove"},
	)
}
