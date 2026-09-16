package memory

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
)

func RuleContent(contexts []string) string {
	_ = contexts
	return strings.Join([]string{
		"# Graphit Memory",
		"",
		"`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host.",
		"",
		"Preserve preferences, corrections, standing guidance, project facts and confirmed non-obvious knowledge that affect future work. Use Graphit, not native/model memory. Task holds investigation/progress; Knowledge holds maintained docs.",
		"",
		"## Recall whenever a question needs context",
		"",
		"Hooks load mandatory project/user memories. Reuse them; call `" + brand.MCPToolName("memory_mandatory") + "` only if the hook reports fallback, mandatory context is absent, or project/scope changes. In fallback, call once per missing scope (`scope: project` and `scope: user` for a project session); the default fetch covers only project scope. Use only user scope on an artifact-only server, without inventing `project_dir`. After compaction recover only missing context.",
		"",
		"At any stage—questions, exploration, implementation, debugging or review—when retained context cannot explain the system, a decision or a surprising result, use `" + brand.MCPToolName("memory_search") + "` with a focused query, `exclude_mandatory: true`, `top_k: 5` and `ai_optimized: true`. Do not wait for a new session, plan or blocker. Known ID: read directly. Search returns titles/IDs; read selected entries with `" + brand.MCPToolName("memory_source") + "`. Use `preview: true` to disambiguate; expand only for remaining gaps. Reuse sufficient context; a new question can need a new search in the same scope. Read a superseded entry's `current` version before relying on it.",
		"",
		"Use project scope and its known absolute `project_dir` by default; user scope is for cross-project preferences. Otherwise read the Hub skill and resolve the project with `" + brand.MCPToolName("cluster_projects") + "` before Hub discovery; never guess paths or read stores as files. Installed AST/Knowledge contexts do not provide project memories. If expected context is missing, refine once; use `" + brand.MCPToolName("memory_list") + "` for an intentional inventory or suspected empty store, not routine recall or post-write verification.",
		"",
		"## Capture at the first durable finding",
		"",
		"For the first unfamiliar mid-work recall, correction, duplicate merge or durable discovery, read [recall and capture cases](references/durable-context.md); reuse thereafter. If the local resource is unavailable, call `" + brand.MCPToolName("module_skill") + "` with `module: memory`, `reference: references/durable-context.md` and the known `project_dir` when available.",
		"",
		"Capture user facts about lifecycle/environment, policies, conventions or standing instructions even without “remember this”. Preserve its rationale and conditions when they affect future planning, compatibility, migration, review or risk. Also capture confirmed structural, reusable or costly discoveries: implicit contracts, sources of truth/generation flows, coupling, recurring failure modes, trade-offs and learned procedures. Capture now, not at repetition or task end.",
		"",
		"Before writing, reuse an existing matching entry already recalled; otherwise make one focused subject search. If the same fact is present and current, do not write it again. Use `" + brand.MCPToolName("memory_update") + "` for an existing subject so its ID/history survives; use `" + brand.MCPToolName("memory_insert") + "` when genuinely new. Include the concrete conclusion, scope/conditions, rationale and a compact source reference. Classify lifecycle/environment state as `fact`, standing choices as `decision`/`convention`, and corrections as `correction`. Task-local instructions, transient progress, speculation, obvious code facts and one-off results without future value stay out of Memory.",
		"",
		"Mark standing context needed in every session `mandatory`; use `important` for consequential but conditional knowledge. Use `" + brand.MCPToolName("memory_mark_mandatory") + "`/`" + brand.MCPToolName("memory_unmark_mandatory") + "` or `" + brand.MCPToolName("memory_promote") + "`/`" + brand.MCPToolName("memory_demote") + "` only when recall needs change. Never demote, shorten away conditions, or delete critical constraints merely to reduce tokens. `" + brand.MCPToolName("memory_important") + "` is for intentional review of promoted entries, not a second mandatory-memory fetch.",
		"",
		"## Reconcile without losing knowledge",
		"",
		"On contradiction, preserve provenance and conditions, then update the current entry using confirmed newer evidence; do not silently replace a user constraint with an inference. For duplicates, merge every distinct fact and constraint into the survivor, read that entry once to verify preservation, then `" + brand.MCPToolName("memory_delete") + "` the redundant entry. Preserve unique knowledge; leave unrelated memories alone.",
		"",
		"A successful write is durable and refreshes its own search indexes: trust the acknowledgement; do not list/search/sync after each write. Read back only if the response is ambiguous or verification is needed before removing a duplicate. Use `" + brand.MCPToolName("memory_index") + "` to repair indexes, `" + brand.MCPToolName("memory_schema") + "` for graph diagnostics, and `" + brand.MCPToolName("memory_sync") + "` to refresh an imported authoritative context. `" + brand.MCPToolName("memory_remove") + "` is destructive context maintenance, never a recall repair.",
		"",
		"Keep investigation/next action in Task and the reusable conclusion in Memory. If Memory MCP is unavailable, state that persistence was unavailable and retain the conclusion in the available task handoff; do not claim it was saved or substitute the Graphit CLI.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return agent.ModuleMandateTrigger(
		"Memory", memorySkillName,
		"answering a question or resolving a knowledge gap at any stage of work, or capturing durable guidance, corrections or discoveries",
		"Reuse sufficient context from hooks/prior reads; new doubts about the system, rationale or learned behavior trigger recall during work, not only session start/resume. For missing context: `"+brand.MCPToolName("memory_search")+"` (`exclude_mandatory: true`) → selected `"+brand.MCPToolName("memory_source")+"`. At the first durable finding, preserve scope/rationale with `"+brand.MCPToolName("memory_update")+"` for an existing subject or `"+brand.MCPToolName("memory_insert")+"` for a new one; skip unchanged duplicates. Keep task-local progress in Task. Never discard unique or critical constraints to save tokens.",
		nil, nil,
	)
}
