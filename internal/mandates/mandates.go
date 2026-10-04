// Package mandates renders the per-module lifecycle mandates injected at agent session
// boundaries. The texts depend on nothing but the brand, so the hook binary renders them
// without loading any store; each module's MandateTrigger delegates here.
package mandates

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/relationtext"
)

// Task routes the agent to the Task module skill and tools.
func Task() string {
	return agent.ModuleMandateTrigger(
		"Task", brand.SkillDirName("task"),
		"starting/resuming project work, planning, delegating/completing it, or answering questions and investigating system/history gaps at any stage",
		relationtext.AgentMandate+" "+"Read the open sessions and DECIDE by scope, not recency: the same evolving demand continues in its session across interruptions, changed scope revises that session and its affected Tasks, and only a different demand gets a new one. With no match, create a detailed demand/strategy session and claim coordination. Read session.md before its lifecycle actions. Bind every agent-created Task to session_id; workers share it without taking coordination. Investigate before saving specification -> plan -> dependency-ordered Tasks with checks; batch planned packets with known IDs and inspect each result. Install missing commands needed for delivery quality, conformance and tests; persist one Windows/macOS/Linux invocation: project binaries by project-relative path, others via system PATH; never machine paths. A user-requested validation or user-reported missing-validation error makes its project check necessary. Before commit/delivery, put fast necessary checks in pre-commit and long checks in pre-push; verify lockfile hooks and Git activation. `pre-push` is only a gate: never initiate or retry `git push` without an explicit engineer request for that push. Manual runs alone are no gate; inactive hooks stay unresolved. Keep leaves usable without this conversation. Checkpoint progress, problems, decisions, strategy and next action at meaningful outcomes. Recall sessions/Tasks whenever new doubts arise. Handoff preserves state and releases claims; a turn ending is not completion. Close the session explicitly only after linked Tasks are terminal and scope/evidence is reconciled; cancelled work is not delivered.",
		nil,
		[]string{"task_session_get", "task_session_list", "task_session_create", "task_session_claim", "task_get", "task_query", "task_create"},
	)
}

// TaskWorker is the Task routing a delegated performer needs; see
// task.WorkerMandateTrigger for why it differs from the coordinator's.
func TaskWorker() string {
	return agent.ModuleMandateTrigger(
		"Task", brand.SkillDirName("task"),
		"reading the work you were assigned, recording its progress, or recalling prior decisions and evidence",
		relationtext.AgentMandate+" "+"Read the session and Task ids the coordinator gave you; search only while a relevant gap remains. Claim at most your own Task, and never create, claim or close a coordination session. Record progress and findings against the Task you were given, with evidence and the exact next action. Recall prior Tasks whenever a new doubt exceeds what you were handed.",
		nil,
		[]string{"task_get", "task_search", "task_progress", "task_comment_add"},
	)
}

// Memory routes the agent to the Memory module skill and tools.
func Memory() string {
	return agent.ModuleMandateTrigger(
		"Memory", brand.SkillDirName("memory"),
		"answering a question or resolving a knowledge gap at any stage of work, or capturing durable guidance, corrections or discoveries",
		relationtext.AgentMandate+" "+"Reuse sufficient context from hooks/prior reads; new doubts about the system, rationale or learned behavior trigger recall during work, not only session start/resume. For missing context: `"+brand.MCPToolName("memory_search")+"` (`exclude_mandatory: true`) → selected `"+brand.MCPToolName("memory_source")+"`. At the first durable finding, preserve scope/rationale with `"+brand.MCPToolName("memory_update")+"` for an existing subject or `"+brand.MCPToolName("memory_insert")+"` for a new one; skip unchanged duplicates. Keep task-local progress in Task. Never discard unique or critical constraints to save tokens. To count or list by a field rather than by relevance — every mandatory record, everything of one type — use `"+brand.MCPToolName("memory_query")+"` with a predicate; an `id` repeats across its revisions, so add `superseded = false` to reach live records only.",
		nil, nil,
	)
}

// AST routes the agent to the AST module skill and tools.
func AST() string {
	return agent.ModuleMandateTrigger(
		"AST Code Exploration", brand.SkillDirName("ast"),
		"locating/reading code or assessing structure/impact, including before native grep, glob, file reads or symbol navigation",
		"Use AST first: known file → `"+brand.MCPToolName("ast_source")+"` (pattern, head/tail, range, entity); unknown location → `"+brand.MCPToolName("ast_search")+"`; symbols/callers/metrics/impact → `"+brand.MCPToolName("ast_schema")+"` once per target, then `"+brand.MCPToolName("ast_query")+"`. Cluster neighbors are also managed: pass their returned `dir` as `project_dir`, never switch to native grep/walk for being outside cwd. Read the needed target skill, source and dependents/tests; record evidence in the coordinating Task. `"+brand.MCPToolName("ast_schema")+"`/`"+brand.MCPToolName("ast_query")+"` are the Cypher GRAPH; for a structured question over the indexed rows — every entity in a path, what is a dependency — use `"+brand.MCPToolName("ast_fts_schema")+"` then `"+brand.MCPToolName("ast_fts_query")+"`.",
		nil, nil,
	)
}

// Hub routes the agent to the Hub module skill and tools.
func Hub() string {
	return agent.ModuleMandateTrigger(
		"Hub Discovery", brand.SkillDirName("hub"),
		"resolving an ecosystem project/system, Hub artifact, or Graphit configuration",
		"Named system → `"+brand.MCPToolName("cluster_projects")+"` first. A match is Graphit-managed: use its `dir` as `project_dir` for enabled module reads, including AST/Knowledge/Memory/Task; no native discovery just because it is another checkout. Read the needed target skill. No scoped match → `"+brand.MCPToolName("hub_projects")+"`. Artifacts → `"+brand.MCPToolName("hub_search")+"`/`"+brand.MCPToolName("hub_show")+"`: report, announce/install an obvious requested fit; ask only for material ambiguity. Refine misses; distinguish failure from empty results. Published artifacts imply no checkout; `id@version` never replaces a local path. Public technologies need no Hub lookup; honor explicit Hub requests. Runnable installation does not authorize execution. Configuration → `"+brand.MCPToolName("config_get")+"`.",
		nil, nil,
	)
}

// Knowledge routes the agent to the Knowledge module skill and tools.
func Knowledge() string {
	return agent.ModuleMandateTrigger(
		"Knowledge & Documentation", brand.SkillDirName("knowledge"),
		"retrieving/writing documentation or starting/completing a code or documentation work unit",
		relationtext.AgentMandate+" "+"Known page → `"+brand.MCPToolName("wiki_source")+"`; unknown → `"+brand.MCPToolName("knowledge_search")+"` then source. Titles are discovery only; reuse evidence. For every code unit inspect/update affected user and technical docs; for every doc unit verify implementation with AST. Resolve drift and record inspected targets/evidence or justified no-impact in Task before completion. Organize docs by domain and reader goals; the skill routes design and worked examples before authoring. Task owns executable plans/results; query history only for a gap. Known local path first, otherwise cluster before Hub; public technologies need no Hub lookup. For a structured question over the index — which pages are stale, what links to a slug via `xrefs` — use `"+brand.MCPToolName("knowledge_query")+"`.",
		nil, nil,
	)
}
