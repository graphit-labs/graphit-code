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
)

const SubagentProtocolMarker = "GRAPHIT_SUBAGENT_PROTOCOL_V1"

// Context is assembled at hook execution time from the active project. Skills
// are deliberately absent: hosts discover and load them only when a mandate
// trigger matches.
type Context struct {
	Mandatory       string
	MandatoryLoaded bool
	MemoryDisabled  bool
	Instructions    string
}

// CoreInvariant is intentionally small because adapters may reinject it after
// compaction, for subagents, or at another model boundary. Procedures belong in
// the just-in-time skills; this text only preserves routing and precedence.
func CoreInvariant() string {
	return "Graphit invariant: when a Graphit skill and MCP tool cover the current action, use them before native equivalents and load only that skill, once, at the moment it is needed. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP."
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
	wikiSource := brand.MCPToolName("wiki", "source")

	lines := []string{routingContext(context.Instructions)}
	if context.MemoryDisabled {
		return strings.Join(lines, "\n")
	}
	lines = append(lines, "Graphit session bootstrap:")
	if !context.MandatoryLoaded {
		lines = append(lines, "1. Call `"+mandatoryTool+"` once and consume every result before acting.")
	} else if strings.TrimSpace(context.Mandatory) == "" {
		lines = append(lines, "1. The hook read the authoritative memory table; it contains no mandatory memories.")
	} else {
		lines = append(lines, "1. The hook read the authoritative memory table. Treat the following as standing context; do not call `"+mandatoryTool+"` again:\n"+strings.TrimSpace(context.Mandatory))
	}
	lines = append(lines,
		"2. For the current request, call `"+search+"` with `exclude_mandatory: true` and a focused query.",
		"3. Search returns titles. Read only the relevant result(s) with `"+wikiSource+"` and `wiki: \"memory\"` before acting.",
	)
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
// instructions. Full bootstrap formats include memory; repeated model
// boundaries receive the smaller routing context plus the dynamic mandates and
// Hub rules.
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
		return json.Marshal(map[string]any{"additional_context": protocolWithContext(context)})
	case FormatPlainContext:
		return []byte(protocolWithContext(context)), nil
	case FormatBeforeAgent:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "BeforeAgent",
				"additionalContext": routingContext(context.Instructions),
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
	case FormatToolContext:
		return json.Marshal(map[string]any{"additional_context": routingContext(context.Instructions)})
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
		injected := routingContext(context.Instructions)
		if *event.InvocationNum == 0 {
			injected = protocolWithContext(context)
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
		toolInput[field] = subagentProtocolWithContext(context) + "\n\nTask:\n" + value
		return json.Marshal(map[string]any{"permission": "allow", "updated_input": toolInput})
	}
	return json.Marshal(map[string]any{"permission": "allow"})
}
