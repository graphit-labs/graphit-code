package memory

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

func RuleContent(contexts []string) string {
	_ = contexts
	return strings.Join([]string{
		"# Graphit Memory",
		"",
		"Graphit memory is the durable source for preferences, corrections, decisions, constraints, and non-obvious project knowledge. IDE-native/model memory is not a substitute.",
		"",
		"The adapter hook loads mandatory project and user memories at session start and reinjects the Graphit invariant at the strongest lifecycle points the host exposes. Do not repeat `" + brand.MCPToolName("memory", "mandatory") + "` unless the hook explicitly reports fallback or the user changes project/scope.",
		"",
		"## Recall",
		"",
		"For a relevant request, call `" + brand.MCPToolName("memory", "search") + "` with a focused query and `exclude_mandatory: true`. Results are titles; choose one or two and read them with `" + brand.MCPToolName("wiki", "source") + "` and `wiki: \"memory\"`. Use `preview: true` only to disambiguate titles. A superseded hit is historical: read its `current` memory before treating anything as present truth. Use `" + brand.MCPToolName("memory", "list") + "` to distinguish an empty store from a missed search.",
		"",
		"Use project scope by default. Use user scope only for preferences that genuinely apply across projects. For another project, pass that project's resolved `project_dir`; do not infer it or read the global store as files.",
		"",
		"## Write and maintain",
		"",
		"Write with `" + brand.MCPToolName("memory", "insert") + "` when information should survive the session: a correction, durable preference, design choice with rationale, constraint, or non-obvious behavior. Skip transient task state already captured by the task log and facts obvious from code.",
		"",
		"Prefer `" + brand.MCPToolName("memory", "update") + "` when the subject already exists. On contradiction, update the current memory so its id/history survives. On duplication, first merge every distinct fact into the survivor, verify it, then call `" + brand.MCPToolName("memory", "delete") + "` on the redundant entry. Never delete unique knowledge. Perform this sanitation when discovered, not as a vague future task.",
		"",
		"Mark only standing context required in every session with `" + brand.MCPToolName("memory", "mark", "mandatory") + "`; unmark it with `" + brand.MCPToolName("memory", "unmark", "mandatory") + "` when that stops being true. Use `promote`/`demote` for important but conditional memories. `important` lists promoted entries; `schema` describes the memory graph.",
		"",
		"Writes are durable immediately but search indexing is asynchronous. Confirm a fresh write with `list`; call `" + brand.MCPToolName("memory", "index") + "` only if indexing failed or immediate search visibility is required. `" + brand.MCPToolName("memory", "sync") + "`/`remove` manage imported memory contexts.",
		"",
		"Tool index: `graphit_memory_mandatory`, `graphit_memory_search`, `graphit_memory_insert`, `graphit_memory_update`, `graphit_memory_list`, `graphit_memory_important`, `graphit_memory_promote`, `graphit_memory_demote`, `graphit_memory_mark_mandatory`, `graphit_memory_unmark_mandatory`, `graphit_memory_delete`, `graphit_memory_index`, `graphit_memory_schema`, `graphit_memory_sync`, `graphit_memory_remove`, `graphit_wiki_source`.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return ide.ModuleMandateTrigger(
		"Memory",
		memorySkillName,
		"durable preferences, corrections, decisions, constraints, or learned system behavior",
		"",
		[]string{
			"planning a material change, getting stuck, or relying on an earlier decision/preference",
			"the user corrects, teaches, or states a durable preference or constraint",
			"a non-obvious project fact or trade-off should survive this session",
			"reading, writing, classifying, reconciling, or deleting memory, including another project's memory",
		},
		[]string{"memory_search", "memory_insert", "memory_update", "memory_list", "wiki_source"},
	)
}
