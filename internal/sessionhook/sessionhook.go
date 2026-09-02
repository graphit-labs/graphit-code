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

// CoreInvariant is intentionally small because adapters may reinject it after
// compaction, for subagents, or at another model boundary. Procedures belong in
// the just-in-time skills; this text only preserves routing and precedence.
func CoreInvariant() string {
	return "Graphit invariant: prefer Graphit MCP — `graphit_ast_*` for code discovery/structure, `graphit_knowledge_*`/`graphit_wiki_*` for project knowledge, `graphit_hub_*` before model knowledge or web for external systems, and `graphit_memory_*` for durable decisions/corrections. Read only the matching skill when its domain becomes relevant. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP."
}

// SubagentProtocol is self-contained because subagents may start with neither
// the parent's conversation nor its project instructions.
func SubagentProtocol(mandatory ...string) string {
	return SubagentProtocolMarker + "\n" + Protocol(mandatory...)
}

// Protocol is the context injected before the first model response. When
// mandatory is supplied, even as an empty string, the hook has already read the
// authoritative memory table; otherwise the agent receives the MCP fallback.
func Protocol(mandatory ...string) string {
	mandatoryTool := brand.MCPToolName("memory", "mandatory")
	search := brand.MCPToolName("memory", "search")
	wikiSource := brand.MCPToolName("wiki", "source")

	lines := []string{CoreInvariant(), "Graphit session bootstrap:"}
	if len(mandatory) == 0 {
		lines = append(lines, "1. Call `"+mandatoryTool+"` once and consume every result before acting.")
	} else if strings.TrimSpace(mandatory[0]) == "" {
		lines = append(lines, "1. The hook read the authoritative memory table; it contains no mandatory memories.")
	} else {
		lines = append(lines, "1. The hook read the authoritative memory table. Treat the following as standing context; do not call `"+mandatoryTool+"` again:\n"+strings.TrimSpace(mandatory[0]))
	}
	lines = append(lines,
		"2. For the current request, call `"+search+"` with `exclude_mandatory: true` and a focused query.",
		"3. Search returns titles. Read only the relevant result(s) with `"+wikiSource+"` and `wiki: \"memory\"` before acting.",
	)
	return strings.Join(lines, "\n")
}

// Render returns a native stdout payload for the output format selected by an
// adapter. Adapter names and lifecycle paths remain owned by the adapters.
func Render(format string, input []byte) ([]byte, error) {
	return RenderWithMandatory(format, input)
}

// RenderWithMandatory renders a native hook payload. Supplying mandatory marks
// phase one as completed by the hook and injects its content directly.
func RenderWithMandatory(format string, input []byte, mandatory ...string) ([]byte, error) {
	switch strings.ToLower(format) {
	case FormatSessionStart:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "SessionStart",
				"additionalContext": Protocol(mandatory...),
			},
		})
	case FormatAdditionalContext:
		return json.Marshal(map[string]any{"additional_context": Protocol(mandatory...)})
	case FormatPlainContext:
		return []byte(Protocol(mandatory...)), nil
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
				"additionalContext": SubagentProtocol(mandatory...),
			},
		})
	case FormatCursorSubagentTask:
		return renderCursorSubagentTask(input, mandatory...)
	case FormatToolContext:
		return json.Marshal(map[string]any{"additional_context": CoreInvariant()})
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
		context := CoreInvariant()
		if *event.InvocationNum == 0 {
			context = Protocol(mandatory...)
		}
		return json.Marshal(map[string]any{
			"injectSteps": []any{map[string]any{"ephemeralMessage": context}},
		})
	default:
		return nil, fmt.Errorf("unsupported session hook format %q", format)
	}
}

func renderCursorSubagentTask(input []byte, mandatory ...string) ([]byte, error) {
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
		toolInput[field] = SubagentProtocol(mandatory...) + "\n\nTask:\n" + value
		return json.Marshal(map[string]any{"permission": "allow", "updated_input": toolInput})
	}
	return json.Marshal(map[string]any{"permission": "allow"})
}
