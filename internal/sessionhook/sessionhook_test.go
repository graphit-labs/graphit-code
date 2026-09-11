package sessionhook

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestProtocolPreservesOrderedMemoryAndTaskRecall(t *testing.T) {
	t.Parallel()

	protocol := Protocol()
	mandatory := strings.Index(protocol, "graphit_memory_mandatory")
	memoryContextual := strings.Index(protocol, "graphit_memory_search")
	taskContextual := strings.Index(protocol, "graphit_task_search")
	if mandatory < 0 || memoryContextual < 0 || taskContextual < 0 || mandatory >= memoryContextual || memoryContextual >= taskContextual {
		t.Fatalf("protocol does not order mandatory recall before contextual search:\n%s", protocol)
	}
	for _, want := range []string{"exclude_mandatory: true", "ai_optimized: true", "graphit_memory_source", "graphit_task_get", "follow `next_cursor`", "analytical results", "audit history"} {
		if !strings.Contains(protocol, want) {
			t.Fatalf("protocol does not require contextual recall detail %q:\n%s", want, protocol)
		}
	}
	if strings.Count(protocol, "ai_optimized: true") != 2 {
		t.Fatalf("protocol must explicitly optimize both contextual searches:\n%s", protocol)
	}
}

func TestProtocolKeepsEnabledRecallModuleIndependent(t *testing.T) {
	t.Parallel()

	memoryDisabled := protocolWithContext(Context{MemoryDisabled: true})
	if strings.Contains(memoryDisabled, "graphit_memory_") || !strings.Contains(memoryDisabled, "graphit_task_search") || !strings.Contains(memoryDisabled, "graphit_task_get") {
		t.Fatalf("disabling Memory must preserve Task recall only:\n%s", memoryDisabled)
	}
	taskDisabled := protocolWithContext(Context{TaskDisabled: true})
	if strings.Contains(taskDisabled, "graphit_task_") || !strings.Contains(taskDisabled, "graphit_memory_search") || !strings.Contains(taskDisabled, "graphit_memory_source") {
		t.Fatalf("disabling Task must preserve Memory recall only:\n%s", taskDisabled)
	}
	bothDisabled := protocolWithContext(Context{MemoryDisabled: true, TaskDisabled: true})
	if strings.Contains(bothDisabled, "Graphit session bootstrap") || !strings.Contains(bothDisabled, "Graphit invariant") {
		t.Fatalf("disabling both recall modules must leave only routing context:\n%s", bothDisabled)
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
	for _, want := range []string{"smallest independently reportable unit", "graphit_task_progress", "Do not write Markdown task state", "defer", "bidirectional code-documentation consistency", "code, configuration, or behavior changed", "only documentation changed", "authoritative code or behavior", "concrete targets inspected", "unresolved divergence blocks completion"} {
		if !strings.Contains(reminder, want) {
			t.Fatalf("unit reminder missing %q: %s", want, reminder)
		}
	}

	tests := []struct {
		agent  string
		format string
		want   string
	}{
		{"Claude", FormatPostToolUse, `"hookEventName":"PostToolUse"`},
		{"Codex", FormatPostToolUse, `"hookEventName":"PostToolUse"`},
		{"Gemini", FormatAfterTool, `"hookEventName":"AfterTool"`},
		{"Cursor", FormatCursorUnit, `"additional_context"`},
		{"Kiro", FormatPlainUnit, "Graphit task checkpoint"},
		{"Antigravity", FormatPostInvocation, `"ephemeralMessage"`},
	}
	for _, tc := range tests {
		payload, err := Render(tc.format, nil)
		if err != nil {
			t.Fatalf("rendering %s checkpoint for %s: %v", tc.format, tc.agent, err)
		}
		for _, want := range []string{tc.want, "smallest independently reportable unit", "bidirectional code-documentation consistency", "unresolved divergence blocks completion"} {
			if !strings.Contains(string(payload), want) {
				t.Fatalf("%s did not carry %q through %s: %s", tc.agent, want, tc.format, payload)
			}
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

func TestProtocolKeepsMemoryAndTaskRecallIndependent(t *testing.T) {
	t.Parallel()

	both := protocolWithContext(Context{MandatoryLoaded: true})
	for _, want := range []string{
		"graphit_memory_search",
		"exclude_mandatory: true",
		"graphit_memory_source",
		"graphit_task_search",
		"follow `next_cursor`",
		"graphit_task_get",
		"full specification, analytical results, progress, comments, evidence, and audit history",
	} {
		if !strings.Contains(both, want) {
			t.Fatalf("complete bootstrap missing %q: %s", want, both)
		}
	}
	if count := strings.Count(both, "graphit_task_search"); count != 1 {
		t.Fatalf("complete bootstrap requests Task search %d times, want once: %s", count, both)
	}

	memoryOnly := protocolWithContext(Context{MandatoryLoaded: true, TaskDisabled: true})
	if !strings.Contains(memoryOnly, "graphit_memory_search") || !strings.Contains(memoryOnly, "graphit_memory_source") || strings.Contains(memoryOnly, "graphit_task_") {
		t.Fatalf("Memory-only bootstrap leaked or lost module guidance: %s", memoryOnly)
	}

	taskOnly := protocolWithContext(Context{MandatoryLoaded: true, MemoryDisabled: true})
	for _, want := range []string{"graphit_task_search", "follow `next_cursor`", "graphit_task_get", "full specification, analytical results, progress, comments, evidence, and audit history"} {
		if !strings.Contains(taskOnly, want) {
			t.Fatalf("Task-only bootstrap missing %q: %s", want, taskOnly)
		}
	}
	if strings.Contains(taskOnly, "graphit_memory_") {
		t.Fatalf("Task-only bootstrap leaked Memory guidance: %s", taskOnly)
	}

	disabled := protocolWithContext(Context{MemoryDisabled: true, TaskDisabled: true})
	if strings.Contains(disabled, "Graphit session bootstrap") || strings.Contains(disabled, "graphit_memory_") || strings.Contains(disabled, "graphit_task_") {
		t.Fatalf("disabled bootstrap leaked recall guidance: %s", disabled)
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
		for _, forbidden := range []string{"MANDATORY MEMORY", "DYNAMIC MANDATE", "DYNAMIC HUB RULE", "Graphit session bootstrap", "graphit_memory_search", "Whenever Knowledge is searched"} {
			if strings.Contains(string(payload), forbidden) {
				t.Fatalf("%s reinjected startup context %q: %s", tc.format, forbidden, payload)
			}
		}
	}
}

func TestLifecycleGapCompensationIsAdapterSpecific(t *testing.T) {
	t.Parallel()

	for _, format := range []string{FormatSessionStart, FormatPlainContext, FormatUserPrompt, FormatBeforeAgent, FormatPostToolUse, FormatAfterTool, FormatPlainUnit} {
		payload, err := Render(format, nil)
		if err != nil {
			t.Fatalf("rendering unaffected format %s: %v", format, err)
		}
		if strings.Contains(string(payload), "specific hook compensation") {
			t.Fatalf("unaffected format %s received adapter-specific compensation: %s", format, payload)
		}
	}

	cursorStart, err := Render(FormatAdditionalContext, nil)
	if err != nil {
		t.Fatal(err)
	}
	cursorUnit, err := Render(FormatCursorUnit, nil)
	if err != nil {
		t.Fatal(err)
	}
	for name, payload := range map[string][]byte{"sessionStart": cursorStart, "postToolUse": cursorUnit} {
		if !strings.Contains(string(payload), "Cursor-specific hook compensation") || strings.Contains(string(payload), "Antigravity-specific") || len(payload) > 1800 {
			t.Fatalf("Cursor %s compensation is missing, leaked, or too large (%d bytes): %s", name, len(payload), payload)
		}
	}
	cursorChild, err := RenderWithContext(FormatCursorSubagentTask, []byte(`{"tool_input":{"prompt":"work"}}`), Context{MandatoryLoaded: true})
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []string{"graphit_memory_search", "graphit_memory_source", "graphit_task_search", "graphit_task_get"} {
		if !strings.Contains(string(cursorChild), required) {
			t.Fatalf("Cursor subagent fallback did not inject the complete enabled-module bootstrap requirement %q: %s", required, cursorChild)
		}
	}

	for _, invocation := range []string{`{"invocationNum":0}`, `{"invocationNum":1}`} {
		payload, err := Render(FormatFirstInvocation, []byte(invocation))
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(payload), "Antigravity-specific hook compensation") || !strings.Contains(string(payload), "complete Graphit protocol") || !strings.Contains(string(payload), "enabled Memory and Task bootstrap") || strings.Contains(string(payload), "Cursor-specific") {
			t.Fatalf("Antigravity compensation is missing or leaked: %s", payload)
		}
		if strings.Contains(invocation, `:1`) && len(payload) > 1200 {
			t.Fatalf("repeated Antigravity compensation is too large: %d bytes", len(payload))
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
