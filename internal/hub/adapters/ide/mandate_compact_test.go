package ide

import (
	"strings"
	"testing"
)

func TestMandatePreambleIsCompactAndLifecycleSafe(t *testing.T) {
	t.Parallel()
	content := mandatePreamble()
	for _, want := range []string{"current action", "once in the session", "prefer Graphit MCP", "every agent and subagent", "default native tools", "interruptions", "compaction", "graphit_sync", "do not sync after every edit"} {
		if !strings.Contains(content, want) {
			t.Fatalf("preamble missing %q:\n%s", want, content)
		}
	}
	if len(content) > 1600 {
		t.Fatalf("resident preamble is too large: %d bytes", len(content))
	}
}

func TestModuleMandateTriggerRoutesWithoutDuplicatingSkill(t *testing.T) {
	t.Parallel()
	content := ModuleMandateTrigger("AST", "graphit-ast", "code discovery", "", []string{"locating code", "impact analysis"}, []string{"ast_search", "ast_query"})
	for _, want := range []string{"read `graphit-ast` once", "locating code", "impact analysis", "graphit_ast_search", "skill routes the remaining tools"} {
		if !strings.Contains(content, want) {
			t.Fatalf("trigger missing %q:\n%s", want, content)
		}
	}
	if len(content) > 700 {
		t.Fatalf("module trigger is too large: %d bytes", len(content))
	}
}
