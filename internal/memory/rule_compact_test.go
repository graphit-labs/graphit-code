package memory

import (
	"strings"
	"testing"
)

func TestMemorySkillCompactContract(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)
	for _, want := range []string{
		"adapter hook loads mandatory", "exclude_mandatory: true", "Results are titles", "superseded", "Prefer `graphit_memory_update`",
		"graphit_memory_mandatory", "graphit_memory_search", "graphit_memory_insert", "graphit_memory_update", "graphit_memory_list",
		"graphit_memory_important", "graphit_memory_promote", "graphit_memory_demote", "graphit_memory_mark_mandatory",
		"graphit_memory_unmark_mandatory", "graphit_memory_delete", "graphit_memory_index", "graphit_memory_schema",
		"graphit_memory_sync", "graphit_memory_remove", "graphit_wiki_source",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("compact Memory skill missing %q", want)
		}
	}
	if len(content) > 5500 {
		t.Fatalf("Memory skill exceeded its token budget: %d bytes", len(content))
	}
}
