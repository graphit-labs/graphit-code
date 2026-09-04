package task

import (
	"strings"
	"testing"
)

func TestTaskSkillContentContract(t *testing.T) {
	t.Parallel()
	content := RuleContent()
	for _, want := range []string{
		"delivery-support or finalization work are subtasks",
		"self-contained: objective and value",
		"must** do or **must not** allow",
		"Given-When-Then",
		"method or command, target and conditions",
		"Markdown is supported for descriptions, check text and evidence",
		"IDs, titles, types, statuses, priorities, actors, and timestamps remain compact plain text",
		"explicit user/operator intent",
		"rotates the token",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("Task skill missing %q", want)
		}
	}
	if len(content) > 7000 {
		t.Fatalf("Task skill exceeded its token budget: %d bytes", len(content))
	}
	mandate := MandateTrigger()
	for _, want := range []string{"starting, resuming, planning", "graphit_task_search", "graphit_task_create", "graphit_task_claim", "graphit_task_force_takeover", "graphit_task_progress", "graphit_task_complete"} {
		if !strings.Contains(mandate, want) {
			t.Fatalf("Task mandate missing %q", want)
		}
	}
	if len(mandate) > 1800 {
		t.Fatalf("Task mandate exceeded its resident token budget: %d bytes", len(mandate))
	}
}
