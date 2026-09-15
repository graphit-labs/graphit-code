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
		"Graphit memory is the durable source for preferences, corrections, decisions, constraints, and non-obvious project knowledge. Agent-native/model memory is not a substitute.",
		"",
		"The adapter hook loads mandatory project and user memories at session start and reinjects the Graphit invariant at the strongest lifecycle points the host exposes. Do not repeat `" + brand.MCPToolName("memory", "mandatory") + "` unless the hook explicitly reports fallback or the user changes project/scope.",
		"",
		"## Recall",
		"",
		"For a relevant request, call `" + brand.MCPToolName("memory", "search") + "` with a focused query and `exclude_mandatory: true`. Results are titles and ids; choose one or two and read them with `" + brand.MCPToolName("memory", "source") + "`. Use `preview: true` only to disambiguate titles. A superseded hit is historical: read its `current` memory before treating anything as present truth. Use `" + brand.MCPToolName("memory", "list") + "` to distinguish an empty store from a missed search.",
		"",
		"Use project scope by default. Use user scope only for preferences that genuinely apply across projects. For another project, pass that project's resolved `project_dir`; do not infer it or read the global store as files.",
		"",
		"## Write and maintain",
		"",
		"Treat a user statement as a memory-capture event when it provides project information the agent did not already know or standing guidance for how the agent should act. Do not require the user to ask for memory or repeat the statement. Presume it is durable when it describes the project, its lifecycle or environment, policies, or conventions, or when knowing it would change planning, implementation, review, compatibility, migration, safety, or risk decisions in later work. Preserve the user's rationale when it connects a project fact to an operating policy.",
		"",
		"Also treat an agent discovery as a memory-capture event when analysis, implementation, debugging, or review establishes relevant non-obvious knowledge that is structural, reusable across tasks, or costly to rediscover. Create or update Memory automatically; do not wait for a user request. Capture implicit invariants or contracts, sources of truth and generation flows, non-obvious dependencies or coupling, recurring root causes or failure modes, confirmed trade-offs, surprising tool/infrastructure/lifecycle behavior, and learned procedures. Ask whether another agent would plan or act better, or avoid material investigation, by knowing it.",
		"",
		"Write with `" + brand.MCPToolName("memory", "insert") + "` on that first occurrence. Use project scope for project facts and instructions; use user scope only for genuinely cross-project preferences. Classify lifecycle or environment state as a `fact` and standing project guidance as a `decision` or `convention`. Mark standing context needed in every session as mandatory and important but conditional context as important. If the statement is explicitly limited to the current task, keep it in Graphit Task instead. Also skip transient task state, questions or speculation, and facts obvious from authoritative code or documentation.",
		"",
		"The complete investigation remains in Graphit Task; Memory stores its durable reusable conclusion and never substitutes for Task. Do not capture trivial or obvious observations, transient progress, one-off results without future value, or unconfirmed hypotheses.",
		"",
		"Prefer `" + brand.MCPToolName("memory", "update") + "` when the subject already exists. On contradiction, update the current memory so its id/history survives. On duplication, first merge every distinct fact into the survivor, verify it, then call `" + brand.MCPToolName("memory", "delete") + "` on the redundant entry. Never delete unique knowledge. Perform this sanitation when discovered, not as a vague future task.",
		"",
		"Mark only standing context required in every session with `" + brand.MCPToolName("memory", "mark", "mandatory") + "`; unmark it with `" + brand.MCPToolName("memory", "unmark", "mandatory") + "` when that stops being true. Use `promote`/`demote` for important but conditional memories. `important` lists promoted entries; `schema` describes the memory graph.",
		"",
		"Writes are durable immediately and refresh the authoritative table's own search indexes. Confirm a fresh write with `list`; call `" + brand.MCPToolName("memory", "index") + "` only to repair or explicitly refresh those indexes. `" + brand.MCPToolName("memory", "sync") + "` validates and refreshes an imported authoritative context; no local memory projection is created.",
		"",
		"Tool index: `graphit_memory_mandatory`, `graphit_memory_search`, `graphit_memory_source`, `graphit_memory_insert`, `graphit_memory_update`, `graphit_memory_list`, `graphit_memory_important`, `graphit_memory_promote`, `graphit_memory_demote`, `graphit_memory_mark_mandatory`, `graphit_memory_unmark_mandatory`, `graphit_memory_delete`, `graphit_memory_index`, `graphit_memory_schema`, `graphit_memory_sync`, `graphit_memory_remove`.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return agent.ModuleMandateTrigger(
		"Memory",
		memorySkillName,
		"durable project or user context, including user-provided facts or guidance and agent-discovered structural or non-obvious system knowledge",
		"",
		[]string{
			"planning a material change, getting stuck, or relying on an earlier decision/preference",
			"the user provides project information the agent did not know or could not infer from authoritative sources",
			"the user states or corrects a project lifecycle or environment state, policy, convention, or standing instruction",
			"user guidance would change future planning, implementation, review, compatibility, migration, safety, or risk decisions unless explicitly task-local",
			"analysis, implementation, debugging, or review reveals a reusable non-obvious invariant, source of truth, structural dependency, failure mode, trade-off, or learned procedure",
			"reading, writing, classifying, reconciling, or deleting memory, including another project's memory",
		},
		[]string{"memory_search", "memory_source", "memory_insert", "memory_update", "memory_list"},
	)
}
