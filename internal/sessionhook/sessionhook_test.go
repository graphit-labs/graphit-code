package sessionhook

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestProtocolPreservesTheOrderedTwoPhaseRecall(t *testing.T) {
	t.Parallel()

	protocol := Protocol()
	mandatory := strings.Index(protocol, "graphit_memory_mandatory")
	contextual := strings.Index(protocol, "graphit_memory_search")
	if mandatory < 0 || contextual < 0 || mandatory >= contextual {
		t.Fatalf("protocol does not order mandatory recall before contextual search:\n%s", protocol)
	}
	if !strings.Contains(protocol, "exclude_mandatory: true") {
		t.Fatalf("protocol does not exclude mandatory memories from contextual search:\n%s", protocol)
	}
	if !strings.Contains(protocol, "graphit_wiki_source") || !strings.Contains(protocol, `wiki: "memory"`) {
		t.Fatalf("protocol does not require reading selected memory pages:\n%s", protocol)
	}
}

func TestCoreInvariantFallsBackWhenGraphitToolsAreUnavailable(t *testing.T) {
	t.Parallel()
	invariant := CoreInvariant()
	if !strings.Contains(invariant, "tool is unavailable") || !strings.Contains(invariant, "default native tools") {
		t.Fatalf("invariant does not preserve native fallback when Graphit is unavailable: %s", invariant)
	}
	if !strings.Contains(invariant, "Resuming") || !strings.Contains(invariant, "reapplies this priority before the next action") {
		t.Fatalf("invariant does not restore Graphit-first routing on resume: %s", invariant)
	}
}

func TestUnitCompletionReminderUsesTheSmallestReportableBoundary(t *testing.T) {
	t.Parallel()

	reminder := UnitCompletionReminder()
	for _, want := range []string{"smallest independently reportable unit", "update the active task manager and task log now", "Do not defer"} {
		if !strings.Contains(reminder, want) {
			t.Fatalf("unit reminder missing %q: %s", want, reminder)
		}
	}

	tests := []struct {
		format string
		want   string
	}{
		{FormatPostToolUse, `"hookEventName":"PostToolUse"`},
		{FormatAfterTool, `"hookEventName":"AfterTool"`},
		{FormatCursorUnit, `"additional_context"`},
		{FormatPlainUnit, "Graphit task checkpoint"},
		{FormatPostInvocation, `"ephemeralMessage"`},
	}
	for _, tc := range tests {
		payload, err := Render(tc.format, nil)
		if err != nil {
			t.Fatalf("rendering %s: %v", tc.format, err)
		}
		if !strings.Contains(string(payload), tc.want) || !strings.Contains(string(payload), "smallest independently reportable unit") {
			t.Fatalf("%s did not carry the native task checkpoint: %s", tc.format, payload)
		}
	}
}

func TestFinalSyncFormatsAllowImmediateCompletion(t *testing.T) {
	t.Parallel()

	for _, format := range []string{FormatStop, FormatCursorStop, FormatAfterAgent, FormatSessionEnd} {
		payload, err := Render(format, nil)
		if err != nil {
			t.Fatalf("rendering %s: %v", format, err)
		}
		if !json.Valid(payload) || string(payload) != `{}` {
			t.Fatalf("%s did not allow immediate completion: %s", format, payload)
		}
	}
	payload, err := Render(FormatAntigravityStop, nil)
	if err != nil || !json.Valid(payload) || !strings.Contains(string(payload), `"decision":"stop"`) {
		t.Fatalf("Antigravity did not allow immediate completion: %s, %v", payload, err)
	}

	if payload, err := Render(FormatNoOutput, nil); err != nil || len(payload) != 0 {
		t.Fatalf("silent final sync output = %q, %v", payload, err)
	}
}

func TestRenderNativeFormats(t *testing.T) {
	t.Parallel()

	for _, format := range []string{FormatSessionStart, FormatAdditionalContext} {
		format := format
		t.Run(format, func(t *testing.T) {
			t.Parallel()
			payload, err := Render(format, nil)
			if err != nil {
				t.Fatal(err)
			}
			if !json.Valid(payload) || !strings.Contains(string(payload), "graphit_memory_mandatory") {
				t.Fatalf("invalid %s payload: %s", format, payload)
			}
		})
	}
}

func TestDynamicInstructionsReachBootstrapAndCompactionBoundaries(t *testing.T) {
	t.Parallel()
	context := Context{MandatoryLoaded: true, Instructions: "DYNAMIC MANDATE\nDYNAMIC HUB RULE"}
	tests := []struct {
		format string
		input  []byte
	}{
		{FormatSessionStart, nil},
		{FormatAdditionalContext, nil},
		{FormatPlainContext, nil},
		{FormatSubagentStart, nil},
		{FormatToolContext, nil},
		{FormatFirstInvocation, []byte(`{"invocationNum":0}`)},
		{FormatCursorSubagentTask, []byte(`{"tool_input":{"prompt":"work"}}`)},
	}
	for _, tc := range tests {
		tc := tc
		t.Run(tc.format, func(t *testing.T) {
			t.Parallel()
			payload, err := RenderWithContext(tc.format, tc.input, context)
			if err != nil {
				t.Fatal(err)
			}
			for _, want := range []string{"DYNAMIC MANDATE", "DYNAMIC HUB RULE"} {
				if count := strings.Count(string(payload), want); count != 1 {
					t.Fatalf("%s contains %q %d times, want exactly once: %s", tc.format, want, count, payload)
				}
			}
		})
	}
}

func TestRepeatedAgentBoundariesStayCompact(t *testing.T) {
	t.Parallel()
	context := Context{
		Mandatory:       "MANDATORY MEMORY",
		MandatoryLoaded: true,
		Instructions:    "DYNAMIC MANDATE\nDYNAMIC HUB RULE",
	}
	tests := []struct {
		format string
		input  []byte
		want   string
	}{
		{FormatUserPrompt, nil, `"hookEventName":"UserPromptSubmit"`},
		{FormatBeforeAgent, nil, `"hookEventName":"BeforeAgent"`},
		{FormatFirstInvocation, []byte(`{"invocationNum":1}`), `"ephemeralMessage"`},
	}
	for _, tc := range tests {
		payload, err := RenderWithContext(tc.format, tc.input, context)
		if err != nil {
			t.Fatalf("rendering %s: %v", tc.format, err)
		}
		if !strings.Contains(string(payload), tc.want) || !strings.Contains(string(payload), "Graphit invariant") {
			t.Fatalf("%s did not carry the compact invariant: %s", tc.format, payload)
		}
		for _, forbidden := range []string{"MANDATORY MEMORY", "DYNAMIC MANDATE", "DYNAMIC HUB RULE", "Graphit session bootstrap", "graphit_memory_search"} {
			if strings.Contains(string(payload), forbidden) {
				t.Fatalf("%s reinjected startup context %q: %s", tc.format, forbidden, payload)
			}
		}
	}
}

func TestRenderAntigravityBootstrapsFirstAndReassertsInvariantLater(t *testing.T) {
	t.Parallel()

	first, err := Render(FormatFirstInvocation, []byte(`{"invocationNum":0}`))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(first), "ephemeralMessage") || !strings.Contains(string(first), "graphit_memory_mandatory") {
		t.Fatalf("first invocation did not inject the protocol: %s", first)
	}

	later, err := Render(FormatFirstInvocation, []byte(`{"invocationNum":1}`))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(later), "Graphit invariant") || strings.Contains(string(later), "graphit_memory_mandatory") {
		t.Fatalf("later invocation must inject only the compact invariant: %s", later)
	}
}

func TestProtocolAcceptsMandatoryMemoryLoadedByHook(t *testing.T) {
	t.Parallel()
	protocol := Protocol("### project memory: Policy\nUse the new design")
	if !strings.Contains(protocol, "Use the new design") || !strings.Contains(protocol, "do not call `graphit_memory_mandatory` again") {
		t.Fatalf("mandatory hook context was not acknowledged: %s", protocol)
	}
}

func TestRenderRejectsInvalidInputAndUnknownAdapters(t *testing.T) {
	t.Parallel()

	if _, err := Render(FormatFirstInvocation, []byte(`{}`)); err == nil {
		t.Fatal("expected missing invocationNum to fail")
	}
	if _, err := Render("unknown", nil); err == nil {
		t.Fatal("expected unknown format to fail")
	}
}

func TestCursorSubagentTaskInjectsProtocolWithoutBlockingFallback(t *testing.T) {
	t.Parallel()
	taskPayload, err := RenderWithMandatory(
		FormatCursorSubagentTask,
		[]byte(`{"tool_name":"Task","tool_input":{"prompt":"Explore authentication"}}`),
		"### project memory: Policy\nUse Graphit",
	)
	if err != nil {
		t.Fatal(err)
	}
	var taskOutput struct {
		Permission   string         `json:"permission"`
		UpdatedInput map[string]any `json:"updated_input"`
	}
	if err := json.Unmarshal(taskPayload, &taskOutput); err != nil {
		t.Fatal(err)
	}
	prompt, _ := taskOutput.UpdatedInput["prompt"].(string)
	if taskOutput.Permission != "allow" || !strings.Contains(prompt, SubagentProtocolMarker) || !strings.Contains(prompt, "Use Graphit") {
		t.Fatalf("Cursor Task did not receive the self-contained protocol: %s", taskPayload)
	}

	fallbackPayload, err := Render(FormatCursorSubagentTask, []byte(`{"tool_name":"Task","tool_input":{}}`))
	if err != nil || !strings.Contains(string(fallbackPayload), `"permission":"allow"`) {
		t.Fatalf("Cursor must allow the native subagent path when protocol injection is impossible: %s, %v", fallbackPayload, err)
	}
}
