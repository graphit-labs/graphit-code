package task

import (
	"strings"
	"testing"
)

// This checks the context cost of the actual generated output. Semantic quality
// is evaluated with cold-start planning scenarios, not exact prose snapshots.
func TestTaskInstructionBudgets(t *testing.T) {
	t.Parallel()
	for name, instruction := range map[string]struct {
		text string
		max  int
	}{
		"description": {skillDescription, 220},
		// Hook activation and push authorization require room in the compact mandate.
		"mandate": {MandateTrigger(), 2350},
		// Explicit typed references are a new required write contract. Retain the
		// existing lifecycle guidance instead of trading it for this requirement.
		"skill": {RuleContent(), 14300},
	} {
		t.Run(name, func(t *testing.T) {
			if strings.TrimSpace(instruction.text) == "" {
				t.Fatal("generated instruction is empty")
			}
			if len(instruction.text) > instruction.max {
				t.Fatalf("%s context budget exceeded: %d bytes (maximum %d)", name, len(instruction.text), instruction.max)
			}
			t.Logf("%s: %d bytes", name, len(instruction.text))
		})
	}
}

func TestTaskInstructionsRequireValidationCommands(t *testing.T) {
	for _, phrase := range []string{"Install missing commands", "user-requested validation", "project check necessary", "one Windows/macOS/Linux invocation", "project-relative path", "system PATH", "never machine paths", "fast necessary checks in pre-commit", "long checks in pre-push", "verify lockfile hooks and Git activation", "Manual runs alone are no gate"} {
		if !strings.Contains(MandateTrigger(), phrase) {
			t.Fatalf("mandate lacks %q", phrase)
		}
	}
	for _, phrase := range []string{"quality, conformance and tests", "hooks.<event>", "error the user attributes to missing validation", "automatically", "absence of a tool is never a pass", "works unchanged on Windows, macOS and Linux", "cmd /C", "sh -c", "versioned project runner", "untested platforms", "relative to the project root", "system `PATH`", "machine-dependent absolute paths", "verify their `PATH` resolution", "need not invoke `sh`", "ask which defect each check prevents", "fast necessary checks in pre-commit", "reserve long checks for pre-push", "actual Git activation", "prove failures block", "An ad hoc pass is no gate"} {
		if !strings.Contains(RuleContent(), phrase) {
			t.Fatalf("skill lacks %q", phrase)
		}
	}
	for _, phrase := range []string{"never initiate or retry `git push` without an explicit engineer request", "`pre-push` is only a gate"} {
		if !strings.Contains(strings.ToLower(MandateTrigger()), phrase) {
			t.Fatalf("mandate lacks push authorization boundary %q", phrase)
		}
	}
	for _, phrase := range []string{"Never initiate or retry `git push` automatically", "explicit engineer request for that specific push", "hook only validates a push the engineer requested"} {
		if !strings.Contains(RuleContent(), phrase) {
			t.Fatalf("skill lacks push authorization boundary %q", phrase)
		}
	}
}
