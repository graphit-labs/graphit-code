package memory

import (
	"strings"
	"testing"
)

// This file used to pin the skill's description of memory_gc, which had a history
// of being documented backwards: the text once presented the bare call as a dry run
// while the tool's DryRun bool defaulted to false, so the bare call deleted. That was
// fixed twice — first by correcting the text to match the dangerous default, then by
// inverting the default itself.
//
// The tool is now gone entirely, and with it the class of bug. Garbage collection
// keyed on age answers the wrong question about a memory store: age says a memory has
// not been revised, not that it is wrong, and the memories that sit unread for months
// are exactly the conventions and corrections that later stop a repeated mistake.
// What replaces it is consolidation, which reasons about *content* and carries it into
// a surviving memory before removing anything.
//
// So what is worth pinning now is the shape of the replacement, in the skill.
func TestMemoryRuleContentTeachesSanitiseOnSight(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	if !strings.Contains(content, "Sanitise On Sight") {
		t.Error("skill does not tell the agent to resolve duplicates and contradictions when it finds them")
	}
	// The ordering rule is the whole safety property: write the survivor first, then
	// delete. Reversed, an interruption between the two loses the knowledge.
	if !strings.Contains(content, "Carry the content forward first") {
		t.Error("skill does not state the ordering rule that makes deletion safe")
	}
	if !strings.Contains(content, "Never delete a memory whose knowledge exists nowhere else") {
		t.Error("skill does not forbid deleting knowledge that exists nowhere else")
	}
}

// The four situations the agent has to act on, each with a tool. A protocol that
// names the problem without naming the fix gets read and not followed.
func TestMemoryRuleContentCoversEachSanitisationCase(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	for _, want := range []string{
		"Two memories say the same thing",
		"Two memories contradict",
		"A memory is deprecated",
		"A memory is right but incomplete or vague",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not cover the case: %q", want)
		}
	}
}

// Two statements in the same skill disagreed about what memory_search reads: one
// said raw .md files, another said a pre-compiled index. It searches the compiled
// wiki through FTS5 — which is also why a memory written seconds ago may not
// surface yet, a symptom the agent needs the right model to explain.
func TestMemoryRuleContentDescribesSearchAccurately(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	if strings.Contains(content, "searches raw `.md` memory files") {
		t.Error("skill still claims memory_search scans raw .md files")
	}
	if !strings.Contains(content, "FTS5") {
		t.Error("skill does not say memory_search runs over the compiled wiki via FTS5")
	}
}
