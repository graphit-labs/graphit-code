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
	for _, want := range []string{"exclude_mandatory: true", "ai_optimized: true", "graphit_memory_source", "graphit_task_get", "Follow `next_cursor` only while a relevant gap remains", "get an assigned id directly", "once per missing available scope", "`project` for a resolved project, and `user`", "top_k: 5", "parent specification", "dependency-ordered tasks before execution"} {
		if !strings.Contains(protocol, want) {
			t.Fatalf("protocol does not require contextual recall detail %q:\n%s", want, protocol)
		}
	}
	if strings.Count(protocol, "ai_optimized: true") != 4 {
		t.Fatalf("protocol must optimize routing, fallback and both contextual searches:\n%s", protocol)
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

func TestSessionLifecycleGuidanceSurvivesBootstrapAndResume(t *testing.T) {
	// Only a coordinator opens, revises and closes a session. Delegated formats
	// are asserted separately below: handing them this guidance is what made a
	// worker open a second session for work it had already been given.
	for _, format := range []string{FormatSessionStart, FormatPlainContext, FormatAdditionalContext, FormatSessionPrompt} {
		payload, err := RenderWithContext(format, nil, Context{MandatoryLoaded: true})
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range []string{"graphit_task_session_list", "graphit_task_session_search", "graphit_task_session_get", "graphit_task_session_create", "graphit_task_session_revise", "graphit_task_session_checkpoint", "graphit_task_session_complete", "detailed user request", "claim only their task", "Stop hooks never close sessions", "native host session IDs are not Graphit session IDs"} {
			if !strings.Contains(string(payload), want) {
				t.Fatalf("%s session lifecycle missing %q", format, want)
			}
		}
	}
	for _, format := range []string{FormatUserPrompt, FormatBeforeAgent, FormatFirstInvocation, FormatToolContext} {
		payload, err := Render(format, []byte(`{"invocationNum":1}`))
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range []string{"resume durable session/task state", "revise changed intent", "close delivered sessions explicitly"} {
			if !strings.Contains(string(payload), want) {
				t.Fatalf("%s loses session continuation %q", format, want)
			}
		}
	}
}

// A delegated performer already holds the session and task ids. Telling it to
// open or claim a session is what produced duplicate sessions.
func TestDelegatedProtocolNeverInstructsSessionOwnership(t *testing.T) {
	t.Parallel()

	payloads := map[string]string{}
	for _, format := range []string{FormatSubagentStart, FormatCursorSubagentTask} {
		out, err := RenderWithContext(format, []byte(`{"tool_input":{"prompt":"Do the assigned work"}}`), Context{MandatoryLoaded: true})
		if err != nil {
			t.Fatal(err)
		}
		payloads[format] = string(out)
	}
	for _, role := range append(Roles(), Role{}) {
		name := role.Name
		if name == "" {
			name = "generic"
		}
		payloads["role:"+name] = RoleProtocol(role, Context{MandatoryLoaded: true})
	}

	for label, payload := range payloads {
		for _, forbidden := range []string{"graphit_task_session_create", "graphit_task_session_claim", "graphit_task_session_complete", "graphit_task_session_checkpoint", "Stay alive", "Never end yourself", "waits forever", "must reuse the same delegate", "should reuse the same delegate"} {
			if strings.Contains(payload, forbidden) {
				t.Fatalf("%s must not instruct session ownership, found %q", label, forbidden)
			}
		}
		for _, want := range []string{"never create, claim or close a coordination session", "Finishing a turn does not complete Graphit work", "The coordinator decides when this work ends", "then finish your turn", "Do not run a waiting loop", "may later send you another instruction", "follow-up/resume mechanism when supported", "Reuse is optional", "no keep-alive, reuse or explicit dismissal is required", "same optional follow-up capability", "records owned by another agent"} {
			if !strings.Contains(payload, want) {
				t.Fatalf("%s missing delegated contract %q", label, want)
			}
		}
	}
}

// The role also remains usable by a coordinator on a host without subagents.
func TestRoleProtocolReadsForEitherPerformer(t *testing.T) {
	t.Parallel()

	for _, role := range Roles() {
		protocol := RoleProtocol(role, Context{MandatoryLoaded: true})
		for _, forbidden := range []string{"the subagent", "The subagent"} {
			if strings.Contains(protocol, forbidden) {
				t.Fatalf("role %s uses %q, which presumes an origin", role.Name, forbidden)
			}
		}
		if !strings.Contains(protocol, "Exception: where the host cannot run agents separately") {
			t.Fatalf("role %s must carry the no-subagent exception, or the main agent performing it is told to wait for itself", role.Name)
		}
		if !strings.Contains(protocol, "You are performing the "+role.Name+" role") {
			t.Fatalf("role %s does not address its performer", role.Name)
		}
	}
}

func TestRoleProtocolCarriesOnlyItsOwnModulesAndPosture(t *testing.T) {
	t.Parallel()

	scout := RoleProtocol(RoleScout, Context{MandatoryLoaded: true})
	scribe := RoleProtocol(RoleScribe, Context{MandatoryLoaded: true})

	if !strings.Contains(scout, "This role never writes") {
		t.Fatalf("scout must be declared read-only: %s", scout)
	}
	if !strings.Contains(scout, "`file:line`") {
		t.Fatalf("scout must demand verifiable identifiers: %s", scout)
	}
	if strings.Contains(scribe, "This role never writes") {
		t.Fatal("scribe writes and must not be declared read-only")
	}
	if !strings.Contains(scribe, "writes only to task and memory") {
		t.Fatalf("scribe must bound its writes: %s", scribe)
	}

	// The tracker does not use Memory, so its protocol must not route memory
	// recall; the scout does.
	tracker := RoleProtocol(RoleTracker, Context{MandatoryLoaded: true})
	if strings.Contains(tracker, "graphit_memory_search") {
		t.Fatalf("tracker does not use Memory and must not route it: %s", tracker)
	}
	if !strings.Contains(scout, "graphit_memory_search") {
		t.Fatalf("scout uses Memory and must route it: %s", scout)
	}
}

func TestCursorDelegationPrefixesThePerformingRole(t *testing.T) {
	t.Parallel()

	input := []byte(`{"tool_input":{"subagent_type":"graphit-scout","prompt":"Where is the reminder registered?"}}`)
	first, err := RenderWithContext(FormatCursorSubagentTask, input, Context{MandatoryLoaded: true})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(first), SubagentProtocolMarker+" role=scout") {
		t.Fatalf("Cursor delegation lost the role marker: %s", first)
	}

	prefixed := RoleProtocol(RoleScout, Context{MandatoryLoaded: true}) + "\n\nTask:\nWhere is the reminder registered?"
	repeat, err := json.Marshal(map[string]any{"tool_input": map[string]any{"subagent_type": "graphit-scout", "prompt": prefixed}})
	if err != nil {
		t.Fatal(err)
	}
	again, err := RenderWithContext(FormatCursorSubagentTask, repeat, Context{MandatoryLoaded: true})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(again), "updated_input") {
		t.Fatalf("the same role must not be prefixed twice: %s", again)
	}

	// A prompt already carrying one role's protocol is a different delegation
	// when the target role changes, and still needs that role's own protocol.
	switched, err := json.Marshal(map[string]any{"tool_input": map[string]any{"subagent_type": "graphit-scribe", "prompt": prefixed}})
	if err != nil {
		t.Fatal(err)
	}
	other, err := RenderWithContext(FormatCursorSubagentTask, switched, Context{MandatoryLoaded: true})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(other), SubagentProtocolMarker+" role=scribe") {
		t.Fatalf("a different role must receive its own protocol: %s", other)
	}
}

func TestTaskDisabledOmitsSessionCheckpointTools(t *testing.T) {
	for _, format := range []string{FormatSessionStart, FormatPlainContext, FormatSubagentStart, FormatPostToolUse, FormatAfterTool, FormatPlainUnit, FormatPostInvocation, FormatCursorUnit} {
		payload, err := RenderWithContext(format, nil, Context{TaskDisabled: true})
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(payload), "graphit_task_") {
			t.Fatalf("%s instructs disabled Task operations: %s", format, payload)
		}
	}
}

func TestCoreInvariantFallsBackWhenGraphitToolsAreUnavailable(t *testing.T) {
	t.Parallel()
	invariant := CoreInvariant()
	if !strings.Contains(invariant, "tool is unavailable") || !strings.Contains(invariant, "default native tools") {
		t.Fatalf("invariant does not preserve native fallback when Graphit is unavailable: %s", invariant)
	}
	if !strings.Contains(invariant, "new/resumed action") || !strings.Contains(invariant, "reload after compaction if lost") {
		t.Fatalf("invariant does not restore Graphit-first routing on resume: %s", invariant)
	}
	for _, want := range []string{"Delegation", "applicable instruction that explicitly assigns bounded work to a delegated role", "authorizes and requires only that role/work", "recall→scout", "impact→tracker", "transcription→scribe", "otherwise no subagents", "host cannot run it", "coordinator performs only that role", "Acceptance, checkpoints, Task/session claims, revisions, completion and lifecycle stay with the coordinator"} {
		if !strings.Contains(invariant, want) {
			t.Fatalf("invariant does not preserve the bounded delegation rule %q after compaction: %s", want, invariant)
		}
	}
	for _, want := range []string{"Persistence boundary", "only non-sensitive, project-inherent", "personal/sensitive data", "secrets/credentials", "security-risk material", "transient feelings/speculation", "private user memory", "otherwise do not persist"} {
		if !strings.Contains(invariant, want) {
			t.Fatalf("invariant does not preserve persistence boundary %q after compaction: %s", want, invariant)
		}
	}
}

func TestUnitCompletionReminderUsesTheSmallestReportableBoundary(t *testing.T) {
	t.Parallel()

	reminder := UnitCompletionReminder()
	for _, want := range []string{"after meaningful work", "graphit_task_progress", "Reads/bookkeeping alone need none", "graphit_task_session_checkpoint", "coordinator", "problems, decisions, strategy, next step", "Revise changed intent", "close delivered sessions explicitly", "acceptance checks", "code/documentation consistency in both directions", "targets/evidence", "Resolve divergence before closing"} {
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
		for _, want := range []string{tc.want, "after meaningful work", "code/documentation consistency in both directions", "Resolve divergence before closing"} {
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
		"Follow `next_cursor` only while a relevant gap remains",
		"graphit_task_get",
		"chosen task, parent specification and relevant dependencies/precedents",
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
	for _, want := range []string{"graphit_task_search", "Follow `next_cursor` only while a relevant gap remains", "graphit_task_get", "chosen task, parent specification and relevant dependencies/precedents"} {
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
	if !strings.Contains(string(cursorStart), "Cursor-specific hook compensation") || strings.Contains(string(cursorStart), "Antigravity-specific") {
		t.Fatalf("Cursor bootstrap compensation is missing or leaked: %s", cursorStart)
	}
	if strings.Contains(string(cursorUnit), "specific hook compensation") || !strings.Contains(string(cursorUnit), "Reapply Graphit routing before the next action") || len(cursorUnit) > 550 {
		t.Fatalf("Cursor checkpoint should reassert routing without repeating startup compensation (%d bytes): %s", len(cursorUnit), cursorUnit)
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
		// Antigravity has no subagent-start hook, so the compensation has to say
		// where the protocol does come from: the installed role document, which
		// the agent can also read itself when it cannot delegate at all.
		if !strings.Contains(string(payload), "Antigravity-specific hook compensation") || !strings.Contains(string(payload), "role document installed under the agents directory") || !strings.Contains(string(payload), "read that document yourself when you cannot delegate") || strings.Contains(string(payload), "Cursor-specific") {
			t.Fatalf("Antigravity compensation is missing or leaked: %s", payload)
		}
		// The recurring payload carries the complete delegation boundary after
		// compaction, including coordinator-only lifecycle and host fallback.
		if strings.Contains(invocation, `:1`) && len(payload) > 2000 {
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

func TestRenderSessionPromptBootstrapsOnceThenStaysCompact(t *testing.T) {
	t.Parallel()

	first, err := RenderWithContext(FormatSessionPrompt, nil, Context{})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(first), "Graphit session bootstrap:") {
		t.Fatalf("the first prompt of a session must carry the bootstrap: %s", first)
	}

	later, err := RenderWithContext(FormatSessionPrompt, nil, Context{BootstrapDelivered: true})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(later), "Graphit invariant") {
		t.Fatalf("later prompts still need the routing invariant: %s", later)
	}
	if strings.Contains(string(later), "Graphit session bootstrap:") {
		t.Fatalf("the bootstrap must not repeat on every turn: %s", later)
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

func TestRepeatedCheckpointsKeepACompactPayload(t *testing.T) {
	t.Parallel()
	context := Context{Mandatory: strings.Repeat("long memory ", 1000), MandatoryLoaded: true, Instructions: strings.Repeat("module routing ", 1000)}
	for _, format := range []string{FormatPostToolUse, FormatAfterTool, FormatCursorUnit, FormatPlainUnit, FormatPostInvocation} {
		payload, err := RenderWithContext(format, nil, context)
		if err != nil {
			t.Fatal(err)
		}
		if len(payload) > 550 {
			t.Fatalf("%s recurring checkpoint costs %d bytes, budget 550", format, len(payload))
		}
		if strings.Contains(string(payload), "long memory") || strings.Contains(string(payload), "module routing") {
			t.Fatalf("%s repeats bootstrap state at every tool boundary", format)
		}
	}
}

func TestCursorSubagentProtocolIsNotPrependedTwice(t *testing.T) {
	t.Parallel()
	original := map[string]any{"tool_input": map[string]any{"prompt": "Inspect parser", "model": "configured-model"}}
	input, _ := json.Marshal(original)
	first, err := RenderWithMandatory(FormatCursorSubagentTask, input, "mandatory context")
	if err != nil {
		t.Fatal(err)
	}
	var output struct {
		UpdatedInput map[string]any `json:"updated_input"`
	}
	if err := json.Unmarshal(first, &output); err != nil {
		t.Fatal(err)
	}
	if output.UpdatedInput["model"] != "configured-model" {
		t.Fatal("injection changed unrelated subagent parameters")
	}
	input, _ = json.Marshal(map[string]any{"tool_input": output.UpdatedInput})
	second, err := RenderWithMandatory(FormatCursorSubagentTask, input, "mandatory context")
	if err != nil {
		t.Fatal(err)
	}
	if string(second) != `{"permission":"allow"}` {
		t.Fatalf("replayed Task call should preserve the already-injected prompt: %s", second)
	}
	fresh, err := RenderWithMandatory(FormatCursorSubagentTask, input, "changed mandatory context")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(fresh), "changed mandatory context") {
		t.Fatalf("deduplication hid a changed mandatory constraint: %s", fresh)
	}
}
