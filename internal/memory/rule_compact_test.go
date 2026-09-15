package memory

import (
	"strings"
	"testing"
)

func TestMemorySkillCompactContract(t *testing.T) {
	t.Parallel()
	for _, want := range []string{"user-provided project facts", "standing guidance", "agent-discovered structural", "non-obvious system knowledge"} {
		if !strings.Contains(memorySkillDescription, want) {
			t.Fatalf("Memory skill description missing acquisition contract %q", want)
		}
	}
	content := RuleContent(nil)
	for _, want := range []string{
		"adapter hook loads mandatory", "exclude_mandatory: true", "Results are titles", "superseded", "Prefer `graphit_memory_update`",
		"memory-capture event", "did not already know", "Do not require the user to ask for memory or repeat", "Presume it is durable",
		"Preserve the user's rationale", "on that first occurrence", "explicitly limited to the current task", "questions or speculation",
		"lifecycle or environment state as a `fact`", "standing project guidance as a `decision` or `convention`",
		"agent discovery as a memory-capture event", "analysis, implementation, debugging, or review", "Create or update Memory automatically",
		"do not wait for a user request", "implicit invariants or contracts", "sources of truth and generation flows",
		"non-obvious dependencies or coupling", "recurring root causes or failure modes", "learned procedures",
		"another agent would plan or act better", "complete investigation remains in Graphit Task",
		"Memory stores its durable reusable conclusion", "one-off results without future value", "unconfirmed hypotheses",
		"graphit_memory_mandatory", "graphit_memory_search", "graphit_memory_insert", "graphit_memory_update", "graphit_memory_list",
		"graphit_memory_important", "graphit_memory_promote", "graphit_memory_demote", "graphit_memory_mark_mandatory",
		"graphit_memory_unmark_mandatory", "graphit_memory_delete", "graphit_memory_index", "graphit_memory_schema",
		"graphit_memory_sync", "graphit_memory_remove", "graphit_memory_source",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("compact Memory skill missing %q", want)
		}
	}
	if len(content) > 5500 {
		t.Fatalf("Memory skill exceeded its token budget: %d bytes", len(content))
	}
	mandate := MandateTrigger()
	for _, want := range []string{
		"user-provided facts", "lifecycle or environment state", "project information the agent did not know",
		"standing instruction", "future planning", "compatibility", "migration", "unless explicitly task-local",
		"agent-discovered structural", "reusable non-obvious invariant", "source of truth", "structural dependency",
		"failure mode", "learned procedure",
	} {
		if !strings.Contains(mandate, want) {
			t.Fatalf("Memory mandate missing user-context acquisition contract %q", want)
		}
	}
	if len(mandate) > 1400 {
		t.Fatalf("Memory mandate exceeded its resident token budget: %d bytes", len(mandate))
	}
}
