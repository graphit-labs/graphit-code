package sessionhook

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const (
	FormatSessionStart       = "session-start"
	FormatAdditionalContext  = "additional-context"
	FormatFirstInvocation    = "first-invocation"
	FormatPlainContext       = "plain-context"
	FormatBeforeAgent        = "before-agent"
	FormatToolContext        = "tool-context"
	FormatSubagentStart      = "subagent-start"
	FormatCursorSubagentTask = "cursor-subagent-task"
	FormatUserPrompt         = "user-prompt"
	FormatPostToolUse        = "post-tool-use"
	FormatAfterTool          = "after-tool"
	FormatCursorUnit         = "cursor-unit"
	FormatPlainUnit          = "plain-unit"
	FormatPostInvocation     = "post-invocation"
	FormatStop               = "stop"
	FormatCursorStop         = "cursor-stop"
	FormatAfterAgent         = "after-agent"
	FormatAntigravityStop    = "antigravity-stop"
	FormatSessionEnd         = "session-end"
	FormatNoOutput           = "no-output"
)

const SubagentProtocolMarker = "GRAPHIT_SUBAGENT_PROTOCOL_V1"

// Context is assembled at hook execution time from the active project. Skills
// are deliberately absent: hosts discover and load them only when a mandate
// trigger matches.
type Context struct {
	Mandatory       string
	MandatoryLoaded bool
	MemoryDisabled  bool
	TaskDisabled    bool
	Instructions    string
}

// CoreInvariant is intentionally small because adapters may reinject it after
// compaction, for subagents, or at another model boundary. Procedures belong in
// the just-in-time skills; this text only preserves routing and precedence.
func CoreInvariant() string {
	return "Graphit invariant: reapply module routing before every new/resumed action. Read only the matching skill before first use; reuse it while present, reload after compaction if lost. Use Graphit MCP before native equivalents, with `ai_optimized: true` when supported. If a required tool is unavailable, use default native tools, never the Graphit CLI. Resume durable task state; throughout work, new questions trigger needed enabled Memory/Task recall. Reuse sufficient context. `project_dir` is call-local; persist project identity and relative paths, never a machine-specific checkout root."
}

func cursorLifecycleCompensation() string {
	return "Cursor-specific hook compensation: `beforeSubmitPrompt` cannot inject context; Cloud may omit `sessionStart`. Reapply Graphit routing before new/resumed work. Post-tool reminders govern subsequent actions; they cannot enforce the first action."
}

func antigravityLifecycleCompensation() string {
	return "Antigravity-specific hook compensation: `PostToolUse` cannot inject context; apply checkpoints at completed work units without waiting for `PostInvocation`. No subagent-start hook: include the complete Graphit protocol and assigned task ids when delegating, preserving enabled Memory and Task bootstrap."
}

// UnitCompletionReminder is injected after the smallest objective work boundary
// the host exposes. The hook cannot decide whether a semantic unit is complete,
// so it asks the agent to make that judgment immediately instead of at turn end.
func UnitCompletionReminder() string {
	return "Graphit task checkpoint: on a completed work unit of a claimed task, call `" + brand.MCPToolName("task", "progress") + "` now with evidence and next step. Reads and task bookkeeping alone are not completed units. Keep task state in Graphit. " + DocumentationConsistencyReminder()
}

// DocumentationConsistencyReminder is delivered before the agent can decide
// that a task is complete. Final stop hooks are too late for semantic review,
// so adapters use this through their last context-capable checkpoint boundary.
func DocumentationConsistencyReminder() string {
	return "Before completion, verify acceptance checks and affected code/documentation consistency in both directions; record inspected targets and evidence in Task. Resolve divergence before closing."
}

// SubagentProtocol is self-contained because subagents may start with neither
// the parent's conversation nor its project instructions.
func SubagentProtocol(mandatory ...string) string {
	return SubagentProtocolMarker + "\n" + Protocol(mandatory...)
}

func subagentProtocolWithContext(context Context) string {
	return SubagentProtocolMarker + "\n" + protocolWithContext(context)
}

// Protocol is the context injected before the first model response. When
// mandatory is supplied, even as an empty string, the hook has already read the
// authoritative memory table; otherwise the agent receives the MCP fallback.
func Protocol(mandatory ...string) string {
	context := Context{}
	if len(mandatory) > 0 {
		context.Mandatory = mandatory[0]
		context.MandatoryLoaded = true
	}
	return protocolWithContext(context)
}

func protocolWithContext(context Context) string {
	mandatoryTool := brand.MCPToolName("memory", "mandatory")
	search := brand.MCPToolName("memory", "search")
	memorySource := brand.MCPToolName("memory", "source")
	taskSearch := brand.MCPToolName("task", "search")
	taskGet := brand.MCPToolName("task", "get")

	lines := []string{routingContext(context.Instructions)}
	if strings.TrimSpace(context.Instructions) == "" {
		lines = append(lines, "If module routing was not injected, use `"+brand.MCPToolName("mandates")+"` once when available. Read matching installed skills first; fetch a missing skill with `"+brand.MCPToolName("module", "skill")+"`. Project instructions remain authoritative.")
	}
	if context.MemoryDisabled && context.TaskDisabled {
		return strings.Join(lines, "\n")
	}
	lines = append(lines, "Graphit session bootstrap:")
	step := 1
	appendStep := func(text string) {
		lines = append(lines, fmt.Sprintf("%d. %s", step, text))
		step++
	}
	if !context.MemoryDisabled {
		if !context.MandatoryLoaded {
			appendStep("If available, read `" + brand.SkillDirName("memory") + "`; call `" + mandatoryTool + "` with `ai_optimized: true` once per missing available scope (`project` for a resolved project, and `user`). Consume all standing context before acting.")
		} else if strings.TrimSpace(context.Mandatory) == "" {
			appendStep("The hook read the authoritative memory table; it contains no mandatory memories.")
		} else {
			appendStep("The hook read the authoritative memory table. Treat the following as standing context; do not call `" + mandatoryTool + "` again:\n" + strings.TrimSpace(context.Mandatory))
		}
		appendStep("Whenever a question about the system, rationale or learned behavior needs context, including during work, read `" + brand.SkillDirName("memory") + "`; query `" + search + "` with `exclude_mandatory: true`, `top_k: 5`, `ai_optimized: true`, and the decision topic. Read selected ids with `" + memorySource + "`. Reuse sufficient context; new questions can require recall in the same session and scope.")
	}
	if !context.TaskDisabled {
		appendStep("Before project work, read `" + brand.SkillDirName("task") + "`; search `" + taskSearch + "` with `top_k: 5`, `ai_optimized: true`, focused on this request, or get an assigned id directly with `" + taskGet + "`. Read the chosen task, parent specification and relevant dependencies/precedents. During work, new doubts also trigger focused search of prior investigations, decisions and evidence; do not wait for restart or a new plan. Follow `next_cursor` only while a relevant gap remains. Reuse recalled context.")
		if strings.TrimSpace(context.Instructions) == "" {
			appendStep("For multi-step work, persist specification, acceptance criteria, plan and dependency-ordered tasks before execution. Resume from recorded progress/evidence; revise affected tasks when scope changes. A single umbrella task is not an executable project plan.")
		}
	}
	return strings.Join(lines, "\n")
}

func routingContext(instructions string) string {
	instructions = strings.TrimSpace(instructions)
	if instructions == "" {
		return CoreInvariant()
	}
	return instructions
}

// Render returns a native stdout payload for the output format selected by an
// adapter. Adapter names and lifecycle paths remain owned by the adapters.
func Render(format string, input []byte) ([]byte, error) {
	return RenderWithMandatory(format, input)
}

// RenderWithMandatory renders a native hook payload. Supplying mandatory marks
// phase one as completed by the hook and injects its content directly.
func RenderWithMandatory(format string, input []byte, mandatory ...string) ([]byte, error) {
	context := Context{}
	if len(mandatory) > 0 {
		context.Mandatory = mandatory[0]
		context.MandatoryLoaded = true
	}
	return RenderWithContext(format, input, context)
}

// RenderWithContext renders a native hook payload with current project
// instructions. Full bootstrap formats include memory and current project
// mandates. Repeated model boundaries receive only compact invariant/reminder
// text so long-lived sessions do not accumulate the startup context.
func RenderWithContext(format string, input []byte, context Context) ([]byte, error) {
	switch strings.ToLower(format) {
	case FormatSessionStart:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "SessionStart",
				"additionalContext": protocolWithContext(context),
			},
		})
	case FormatAdditionalContext:
		return json.Marshal(map[string]any{"additional_context": protocolWithContext(context) + "\n" + cursorLifecycleCompensation()})
	case FormatPlainContext:
		return []byte(protocolWithContext(context)), nil
	case FormatBeforeAgent:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "BeforeAgent",
				"additionalContext": CoreInvariant(),
			},
		})
	case FormatSubagentStart:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "SubagentStart",
				"additionalContext": subagentProtocolWithContext(context),
			},
		})
	case FormatCursorSubagentTask:
		return renderCursorSubagentTask(input, context)
	case FormatUserPrompt:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "UserPromptSubmit",
				"additionalContext": CoreInvariant(),
			},
		})
	case FormatToolContext:
		return json.Marshal(map[string]any{"additional_context": routingContext(context.Instructions)})
	case FormatPostToolUse:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "PostToolUse",
				"additionalContext": UnitCompletionReminder(),
			},
		})
	case FormatAfterTool:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "AfterTool",
				"additionalContext": UnitCompletionReminder(),
			},
		})
	case FormatCursorUnit:
		return json.Marshal(map[string]any{"additional_context": UnitCompletionReminder() + " Reapply Graphit routing before the next action."})
	case FormatPlainUnit:
		return []byte(UnitCompletionReminder()), nil
	case FormatPostInvocation:
		return json.Marshal(map[string]any{
			"injectSteps": []any{map[string]any{"ephemeralMessage": UnitCompletionReminder()}},
		})
	case FormatStop, FormatCursorStop, FormatAfterAgent, FormatSessionEnd:
		return []byte(`{}`), nil
	case FormatAntigravityStop:
		return json.Marshal(map[string]any{"decision": "stop"})
	case FormatNoOutput:
		return nil, nil
	case FormatFirstInvocation:
		var event struct {
			InvocationNum *int `json:"invocationNum"`
		}
		if err := json.Unmarshal(input, &event); err != nil {
			return nil, fmt.Errorf("decoding first-invocation hook input: %w", err)
		}
		if event.InvocationNum == nil {
			return nil, fmt.Errorf("decoding first-invocation hook input: invocationNum is missing")
		}
		var injected string
		if *event.InvocationNum == 0 {
			injected = protocolWithContext(context) + "\n" + antigravityLifecycleCompensation()
		} else {
			injected = CoreInvariant() + "\n" + antigravityLifecycleCompensation()
		}
		return json.Marshal(map[string]any{
			"injectSteps": []any{map[string]any{"ephemeralMessage": injected}},
		})
	default:
		return nil, fmt.Errorf("unsupported session hook format %q", format)
	}
}

func renderCursorSubagentTask(input []byte, context Context) ([]byte, error) {
	var event map[string]any
	if err := json.Unmarshal(input, &event); err != nil {
		return nil, fmt.Errorf("decoding Cursor Task hook input: %w", err)
	}
	toolInput, ok := event["tool_input"].(map[string]any)
	if !ok {
		return nil, fmt.Errorf("decoding Cursor Task hook input: tool_input is missing")
	}
	for _, field := range []string{"prompt", "task", "description"} {
		value, ok := toolInput[field].(string)
		if !ok || strings.TrimSpace(value) == "" {
			continue
		}
		protocol := subagentProtocolWithContext(context) + "\n\nTask:\n"
		if strings.HasPrefix(value, protocol) {
			return json.Marshal(map[string]any{"permission": "allow"})
		}
		toolInput[field] = protocol + value
		return json.Marshal(map[string]any{"permission": "allow", "updated_input": toolInput})
	}
	return json.Marshal(map[string]any{"permission": "allow"})
}
