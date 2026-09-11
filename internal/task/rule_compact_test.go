package task

import (
	"strings"
	"testing"
)

func TestTaskSkillContentContract(t *testing.T) {
	t.Parallel()
	for _, want := range []string{"analysis-only tasks", "exhaustive specifications", "durable results", "prior-task search"} {
		if !strings.Contains(skillDescription, want) {
			t.Fatalf("Task skill description missing %q", want)
		}
	}
	content := RuleContent()
	for _, want := range []string{
		"analysis-only work are project work",
		"full analytical result is durably recorded in Task",
		"Whenever you search project or imported Knowledge",
		"one focused `graphit_task_search` for the same question",
		"Pass `ai_optimized: true`, follow `next_cursor`",
		"read every relevant hit with `graphit_task_get`",
		"specification, analytical result, progress, comments, check evidence, next step, and audit history alongside the Knowledge pages",
		"supplements and never replaces authoritative `graphit_wiki_source` reads",
		"if Task is disabled or unavailable, continue the Knowledge workflow",
		"delivery-support or finalization work are subtasks",
		"progress entries do not excuse leaving the executable specification incomplete",
		"exhaustive, implementation-ready report",
		"every relevant detail discovered by the model",
		"without repeating repository or code investigation",
		"verified facts, reasoned inferences, and unresolved unknowns",
		"must** do or **must not** allow",
		"Given-When-Then",
		"method or command, target and conditions",
		"store the complete analytical report in the task before completion",
		"Memory supplements but never replaces the complete Task record",
		"Markdown is supported for descriptions, check text and evidence",
		"Never use terse status phrases",
		"each checkpoint must accumulate the reusable analytical record",
		"IDs, titles, types, statuses, priorities, actors, and timestamps remain compact plain text",
		"expired or stopped claims become open for takeover",
		"fencing token; keep it private",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("Task skill missing %q", want)
		}
	}
	if len(content) > 12000 {
		t.Fatalf("Task skill exceeded its token budget: %d bytes", len(content))
	}
	mandate := MandateTrigger()
	for _, want := range []string{"starting, resuming, planning, analyzing", "analysis-only work", "Whenever Knowledge is searched", "also search related prior/current Graphit tasks", "follow `next_cursor` until relevant history is covered", "read every relevant result with `graphit_task_get`", "complete Task record alongside Knowledge evidence", "complete, detailed analytical result", "exhaustive execution-ready reports", "without repeating code investigation", "graphit_task_search", "graphit_task_create", "graphit_task_claim", "graphit_task_progress", "graphit_task_complete"} {
		if !strings.Contains(mandate, want) {
			t.Fatalf("Task mandate missing %q", want)
		}
	}
	for name, genericGuidance := range map[string]string{"skill": content, "mandate": mandate} {
		for _, hookOwned := range []string{"bidirectional code-documentation", "documentation-only", "only documentation changed", "adapter-specific mandate", "unresolved divergence"} {
			if strings.Contains(genericGuidance, hookOwned) {
				t.Fatalf("generic Task %s duplicates hook-owned documentation consistency guidance %q", name, hookOwned)
			}
		}
	}
	if len(mandate) > 3600 {
		t.Fatalf("Task mandate exceeded its resident token budget: %d bytes", len(mandate))
	}
	if strings.Contains(content, "graphit_task_force_takeover") || strings.Contains(mandate, "graphit_task_force_takeover") {
		t.Fatal("Task skill advertises unavailable graphit_task_force_takeover")
	}
}
