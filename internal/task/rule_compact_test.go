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
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("Task skill missing %q", want)
		}
	}
}
