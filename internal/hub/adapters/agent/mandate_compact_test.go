package agent

import (
	"strings"
	"testing"
)

func TestMandatePreambleIsCompactAndLifecycleSafe(t *testing.T) {
	t.Parallel()
	content := mandatePreamble()
	for _, want := range []string{"current action", "immediately before first use", "Reuse loaded instructions for the same project/overrides", "Graphit MCP before native", "agent and subagent", "default native tools", "ai_optimized: true", "interruptions", "compaction", "independently reportable work unit", "active Task", "After the final task update", "full sync asynchronously", "do not duplicate it, wait for it, or sync after every edit"} {
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
	for _, want := range []string{"read `graphit-ast` if its instructions are not in the current context", "locating code", "impact analysis", "graphit_ast_search", "skill routes the remaining tools"} {
		if !strings.Contains(content, want) {
			t.Fatalf("trigger missing %q:\n%s", want, content)
		}
	}
	if len(content) > 700 {
		t.Fatalf("module trigger is too large: %d bytes", len(content))
	}
}

func TestMandateContextIsHookReadyAndDeterministicallyOrdered(t *testing.T) {
	t.Parallel()
	content := MandateContext(map[string]string{
		"doc_rule": "DOC",
		"mem_rule": "MEM",
		"ast_rule": "AST",
	})
	if !strings.HasPrefix(content, "<GRAPHIT_SYSTEM_MANDATE>") || !strings.HasSuffix(content, "</GRAPHIT_SYSTEM_MANDATE>") {
		t.Fatalf("mandate wrapper missing: %s", content)
	}
	if strings.Index(content, "<mem_rule>") >= strings.Index(content, "<ast_rule>") ||
		strings.Index(content, "<ast_rule>") >= strings.Index(content, "<doc_rule>") {
		t.Fatalf("mandates are not in canonical order: %s", content)
	}
}
