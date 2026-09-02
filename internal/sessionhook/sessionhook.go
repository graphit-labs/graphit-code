package sessionhook

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const (
	FormatSessionStart      = "session-start"
	FormatAdditionalContext = "additional-context"
	FormatFirstInvocation   = "first-invocation"
)

// Protocol is the context injected by every supported IDE adapter before the
// first model response. Keep the two recall phases explicit and ordered: the
// contextual search must not duplicate memories already loaded unconditionally.
func Protocol() string {
	mandatory := brand.MCPToolName("memory", "mandatory")
	search := brand.MCPToolName("memory", "search")
	wikiSource := brand.MCPToolName("wiki", "source")

	return strings.Join([]string{
		"Graphit deterministic session initialization: complete this protocol before responding to the first user request or taking any other action.",
		"1. Call `" + mandatory + "` with no query and consume every mandatory memory it returns.",
		"2. Derive a contextual query from the current user request, then call `" + search + "` with that query and `exclude_mandatory: true`.",
		"3. Select the relevant contextual results and read their full content with `" + wikiSource + "` using `wiki: \"memory\"` before acting.",
		"If the memory store does not exist yet, proceed after the tool reports that condition. Execute this initialization exactly once for this session.",
	}, "\n")
}

// Render returns a native stdout payload for the output format selected by an
// adapter. Adapter names and lifecycle paths remain owned by the adapters.
func Render(format string, input []byte) ([]byte, error) {
	switch strings.ToLower(format) {
	case FormatSessionStart:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "SessionStart",
				"additionalContext": Protocol(),
			},
		})
	case FormatAdditionalContext:
		return json.Marshal(map[string]any{"additional_context": Protocol()})
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
		if *event.InvocationNum != 0 {
			return []byte("{}"), nil
		}
		return json.Marshal(map[string]any{
			"injectSteps": []any{map[string]any{"ephemeralMessage": Protocol()}},
		})
	default:
		return nil, fmt.Errorf("unsupported session hook format %q", format)
	}
}
