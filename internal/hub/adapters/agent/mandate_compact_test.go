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
	// A delegation rule that only says "delegate" breaks on a host with no
	// subagent, and one that omits what stays behind invites a delegate to close
	// work it does not own. Both halves are required, not stylistic.
	//
	// The coordinator half of the lifecycle is required for the same reason. A
	// delegate stays alive until dismissed, so a coordinator that is not told to
	// dismiss it leaves it waiting forever, and one that is not told to reuse it
	// opens a second delegate for a follow-up the first is still holding context
	// for.
	//
	// Naming what a later question looks like is load-bearing too. "Send
	// follow-ups" reads as the planned next step, and the doubts that actually
	// arise mid-work do not feel like follow-ups: a state the coordinator is
	// unsure of, an expectation that did not hold, a why. Left unnamed, those go
	// to the coordinator's own investigation, which is the delegation the rule
	// exists to cause.
	for _, want := range []string{"Delegate recall", "where your host cannot run them", "perform it yourself", "reports without closing anything", "stays open", "Send later questions to the one that owns them", "an expectation that did not hold", "rather than investigating yourself", "dismiss it explicitly", "never delegated"} {
		if !strings.Contains(content, want) {
			t.Fatalf("preamble missing delegation guidance %q:\n%s", want, content)
		}
	}
	// The cap rose from 1600 with the delegation rule. It costs ~370 bytes once
	// per session and moves recall and impact review out of this window
	// entirely, which is worth far more than it spends.
	//
	// RAISED FROM 2000 TO 2200, in two steps in one session, for the coordinator
	// half of the delegate lifecycle. A delegate now stays alive until dismissed,
	// which makes three coordinator behaviours load-bearing rather than tidy:
	// reuse the open delegate, route later doubts to it, and dismiss it
	// explicitly. Omit any one and the rule produces the leak it exists to
	// prevent — a delegate waiting forever, a second one opened while the first
	// holds the context, or a coordinator quietly investigating for itself.
	//
	// The line was compressed at every step rather than only at the end: "a
	// delegate you open stays open" lost its redundant half, "instead of opening
	// another" went once "the one that owns them" made it redundant, "Judging
	// acceptance" became "Acceptance", "record transcription" became
	// "transcription", "why something behaves as it does" became "a why", and
	// the exception's "document in its agents directory" became
	// "agents-directory document". The last 40 bytes were not shaved, because
	// what remained was the enumeration of doubts, and that enumeration is the
	// working part: unnamed, those doubts do not read as follow-ups at all.
	//
	// Two raises in one session is a smell, and this is the ceiling. The next
	// addition to delegation belongs in the role documents, which are read on
	// demand per delegate, not in this preamble, which every agent pays for once
	// per session whether it delegates or not.
	if len(content) > 2200 {
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
