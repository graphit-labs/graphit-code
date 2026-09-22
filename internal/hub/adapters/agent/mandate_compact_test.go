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
	for _, want := range []string{"Persistence boundary for every module", "only non-sensitive content inherent to the project", "personal or sensitive user/organization data", "secrets, credentials", "feelings, frustration", "unconfirmed speculation", "private user memory", "otherwise do not persist it", "technical hypotheses explicitly labelled", "never promote them to facts"} {
		if !strings.Contains(content, want) {
			t.Fatalf("preamble missing persistence boundary %q:\n%s", want, content)
		}
	}
	// An applicable named delegation is the narrow exception to the default: it
	// authorizes and requires only that role, without transferring coordination.
	for _, want := range []string{"Subagent default: create none unless", "applicable mandate, AGENTS.md, loaded skill or higher-priority instruction", "explicitly assigns bounded work to a delegated role", "authorizes and requires only that role and work", "This mandate assigns recall→", "impact review→", "decided transcription→", "already in that role works directly, not recursively", "host cannot run the role", "perform only that role yourself", "report evidence and may finish", "no waiting loop", "may later send that delegate another instruction", "follow-up/resume when supported", "Reuse is optional", "no keep-alive, reuse or explicit dismissal is required", "ending a turn does not complete Graphit work", "Task/session claims, revisions, completion and lifecycle stay with the coordinator"} {
		if !strings.Contains(content, want) {
			t.Fatalf("preamble missing delegation guidance %q:\n%s", want, content)
		}
	}
	for _, forbidden := range []string{"stays open", "waits forever", "Stay alive", "Never end yourself", "must reuse the same delegate", "should reuse the same delegate"} {
		if strings.Contains(content, forbidden) {
			t.Fatalf("preamble requires a live waiting turn: %q", forbidden)
		}
	}
	// The resident budget includes the explicit delegation authorization boundary;
	// keep that safety rule without letting the preamble grow unbounded.
	if len(content) > 3200 {
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
