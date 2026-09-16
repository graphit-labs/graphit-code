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
		"Task owns project specifications, plans, backlog, checks and lifecycle, including analysis without file changes. Native plans/documents may supplement Task records. Use available MCP tools; never edit tables or substitute the CLI.",
		"",
		"## Retrieve work and prior knowledge",
		"",
		"Read a known task with " + t("task", "get") + "; otherwise run one focused " + t("task", "search") + " for the request before creating work. Use " + t("task", "list") + " with `ready: true` for available work or `parent_id` for direct children. Read selected records/results; resume matching work. Throughout questions, exploration, coding, debugging and review, search prior investigations/decisions/results when a new doubt exceeds retained context. Do not wait for session start/resume or a blocker; reuse sufficient evidence.",
		"Pass `ai_optimized: true` where supported. Search discovers; `get`/filtered `list` establish state. Start with small `top_k`/`page_size`; follow `next_cursor` while context is missing or completeness required. Cursors retain the query and total `top_k` cap; raising that cap requires a new search. Avoid whole-backlog reads/exports for one question.",
		"",
		"## Define work before implementation",
		"",
		"Start a claimed investigation/planning unit with the known request, scope and expected evidence. Investigate current behavior with AST, documented intent with Knowledge, and relevant decisions/history before deciding the complete delivery map. Resolve material questions first; keep unknowns as explicit refinement prerequisites, not invented defaults. The initial investigation task is not permission to execute a whole project as one task.",
		"Before creating or materially revising a multi-outcome specification/plan, read [references/planning.md](references/planning.md). Before saving the first such backlog, read [references/worked-feature.md](references/worked-feature.md): it fills specification, plan, every leaf, contracts, fixtures, coverage and claim sequence. Reuse these references while retained; do not load them for unrelated reads.",
		"For a whole system or multiple capabilities, also read [references/worked-system.md](references/worked-system.md) before saving its backlog: it fills umbrella -> deliveries -> subtasks, discovery, shared contracts and system acceptance.",
		"Persist specification -> technical plan -> task/subtask graph -> readiness review before implementation or delegating implementation. Record journeys/priorities, measurable requirements, data/error/boundary contracts, decisions and unknowns. Determine the actual delivery units, prerequisites and compatible parallel work from that evidence; give EACH unit a complete description, acceptance_criteria and tests before its implementation begins. A large prompt needs a delivery hierarchy, not one generic task. A small single-outcome fix can keep all stages in one packet.",
		"Read back saved records. Map every requirement to returned task and check IDs, including integration and documentation evidence. Check omissions, contradictory contracts, stale context, unjustified tasks, unavailable validation and hierarchy/dependency deadlocks. Each leaf must let another agent act and verify using named records, without guessing intent. Planning checks verify the saved plan, never product behavior. Complete planning separately; leave future deliveries open and unclaimed. Planning authorization does not authorize implementation.",
		"Before implementing/resuming a leaf, reviewing completion or handing off, read [references/execution.md](references/execution.md). For a small fix, its filled one-task example is sufficient; do not load the full feature example just to repair one outcome. On discovery that changes scope, update affected descriptions/contracts, coverage, checks and ordering before affected implementation; preserve completed evidence and pending reconciliation explicitly.",
		"If local skill files are inaccessible, use " + t("module", "skill") + " with `module: task`, the exact `reference: references/...md` above, and known `project_dir`. Omit `reference` to retrieve the entrypoint and reference names; do not fetch every body.",
		"Languages, frameworks, paths, symbols and commands in examples are illustrative. Adapt to the actual project and the user's language; preserve evidence and contract rigor, not a stack, business policy or fixed task count.",
		"",
		"## Use the framework at the decision boundary",
		"",
		"Read needed skills before first use; reuse retained instructions: Knowledge for requirements/architecture; AST for definitions, callers, tests and source before deciding impact; Hub for unfamiliar ecosystem projects/artifacts; Memory for durable constraints/decisions. Known absolute path → `project_dir`; otherwise local cluster before Hub catalog. For Hub-only projects, the Hub skill routes announcing/installing needed Knowledge/AST then module queries; the question authorizes preparation. Only without a local checkout, omit `project_dir` and use resolved installed `id@version`. Public technology (e.g. React) needs no Hub lookup; use known knowledge/official docs, verifying current details. History explains prior work; verify current behavior with AST/Knowledge. Persist plan-changing evidence.",
		"",
		"## Create and revise safely",
		"",
		"Use " + t("task", "create") + " with `description`, `acceptance_criteria`, `tests` and a stable `idempotency_key` per logical outcome. Specs/plans live in descriptions; do not invent spec/plan tools or fields. Create referenced tasks first and use returned IDs. Add/remove prerequisites with " + t("task", "dependency", "add") + "/" + t("task", "dependency", "remove") + " while the target is open. Validation, documentation, review and finalization work belongs under its delivery. Use a check for a condition; a subtask for separately owned work.",
		"Claim with " + t("task", "claim") + " before material work. Only ready tasks can be claimed, one per agent. Keep `claim_token` private; pass it with the same identity on owner mutations. Never change identity or bypass dependencies after rejection. Renew through progress or " + t("task", "heartbeat") + " before the lease expires during long work.",
		"Update the executable description through " + t("task", "revise") + " before changed implementation or handoff; progress alone is not a specification revision. Revision/comments require a live claim. Use the latest returned/current `expected_revision` and a reason. Direct revise appends checks with `add_acceptance_criteria`/`add_tests`; supplied `depends_on` replaces the whole list. Changing title/description/type resets active checks to pending. Supersede obsolete checks with " + t("task", "check", "supersede") + " and a reason/replacement; never mark obsolete or failed work passed.",
		"To revise another ready item, release the current claim, claim/revise/release that item, then resume. For a dependency-blocked item, save the complete correction package and affected IDs in a claimed refinement task; if needed, add refinement as a prerequisite without a cycle. Report reconciliation as pending. Once ready, its executor must read prerequisite results and reconcile description/checks before coding. Never delete real dependencies to gain a claim.",
		"Use " + t("task", "batch") + " for independent mutations (1–100 items). It executes in order, reports every result and does not roll back successful items. Inspect each result; retry only failed/unapplied work. Separate operations that require an earlier returned ID, token or revision. Batch `revise` appends checks with `acceptance_criteria`/`tests`, unlike direct revise's `add_*` fields.",
		"",
		"## Checkpoint, validate and hand off",
		"",
		"At each meaningful outcome verify affected code and documentation in both directions: code/config/behavior changes update their user/technical docs, and doc changes are checked against authoritative behavior. Load the Knowledge skill before maintaining those surfaces; record exact targets/evidence or a grounded no-impact result. Resolve divergence before closing the unit, not only at final integration. Then call " + t("task", "progress") + " with new facts/artifacts, evidence and exact `next_step` (action, target, prerequisite, done condition). Reference saved context; do not repeat it or log every read. Use " + t("task", "comment", "add") + " for consequential decisions, problems or lessons needing durable rationale. For analysis preserve the question, method/sources, evidence, conclusions, rationale, uncertainty and follow-on implications. 'Analysis done' is not its result. Promote reusable conclusions to Memory without replacing the Task record.",
		"Record each active check through " + t("task", "check") + " with `checks[].id` as `check_id`, actual pass/fail and concrete command/observation/artifact evidence. Compare delivered behavior against requirement coverage, integration and nonfunctional constraints. If a required check cannot run, record the limitation and leave it unresolved; planned or assumed success is not evidence.",
		"Use " + t("task", "flag") + " for unresolved conditions that gate completion; record resolution before " + t("task", "unflag") + ". Call " + t("task", "complete") + " only after every active check passes, dependencies/subtasks are complete and no flag remains. Summarize outcomes, evidence and limitations. When blocked/stopping/handing off, use " + t("task", "release") + " with current state and exact continuation; hooks/leases do not replace a handoff.",
		"Cancel obsolete work with " + t("task", "cancel") + " and its replacement/reason. Use " + t("task", "remove") + " only for justified hard deletion, with exact `confirm_id` and reason; referenced tasks cannot be removed. Use " + t("task", "export") + " only for a complete machine-readable archive: exact `id` includes descendants, omission includes the project. Report relevant task IDs, outcomes and next ready/blocked work at handoff, not the full audit trail.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return agent.ModuleMandateTrigger(
		"Task", skillName,
		"starting/resuming project work, planning, delegating/completing it, or answering questions and investigating system/history gaps at any stage",
		"Use a known task ID or focused search; read selected records/results. Search prior analysis, decisions and evidence whenever new questions arise during work; reuse sufficient context. Create/claim executable work. Investigate current code/docs and resolve material questions before defining the delivery map. Before implementation, save specification -> plan -> dependency-ordered tasks with acceptance criteria and validations; split multiple deliverables into executable subtasks. Each leaf must be usable by an agent without this conversation. Verify requirement coverage and readiness before coding. Keep official state/results in Task, checkpoint meaningful outcomes, and complete only on check evidence. Read other skills when their evidence is needed; avoid repeating already-sufficient searches.",
		nil,
		[]string{"task_get", "task_search", "task_create", "task_claim"},
	)
}
