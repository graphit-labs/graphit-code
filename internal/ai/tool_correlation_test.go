package ai

import (
	"strings"
	"testing"
)

// Fixtures follow each provider's public wire format, not synthesized display names.
func TestNativeToolCallsKeepIdentityAndPayload(t *testing.T) {
	cases := []struct {
		name  string
		parse func([]byte) ([]Event, string, string)
		lines []string
		id    string
	}{
		{"claude", parseClaudeStreamLine, []string{
			`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"c1","name":"Read","input":{"path":"file"}}]}}`,
			`{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"c1","content":"result"}]}}`,
		}, "c1"},
		{"codex", parseCodexStreamLine, []string{
			`{"type":"item.started","item":{"id":"i1","type":"command_execution","command":"read file"}}`,
			`{"type":"item.completed","item":{"id":"i1","type":"command_execution","aggregated_output":"result"}}`,
		}, "i1"},
		{"gemini", parseGeminiStreamLine, []string{
			`{"type":"tool_use","tool_id":"g1","tool_name":"read","parameters":{"path":"file"}}`,
			`{"type":"tool_result","tool_id":"g1","status":"success","output":"result"}`,
		}, "g1"},
		{"opencode", parseOpenCodeStreamLine, []string{
			`{"type":"tool_use","part":{"type":"tool","callID":"o1","tool":"read","state":{"status":"completed","input":{"path":"file"},"output":"result"}}}`,
		}, "o1"},
		{"qwen", parseQwenStreamLine, []string{
			`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"q1","name":"Read","input":{"path":"file"}}]}}`,
			`{"type":"user","message":{"content":[{"type":"text","text":"not assistant output"},{"type":"tool_result","tool_use_id":"q1","content":"result"}]}}`,
		}, "q1"},
		{"kimi", parseKimiStreamLine, []string{
			`{"role":"assistant","content":"","tool_calls":[{"type":"function","id":"k1","function":{"name":"Read","arguments":"{\"path\":\"file\"}"}}]}`,
			`{"role":"tool","tool_call_id":"k1","content":"result"}`,
		}, "k1"},
		{"agy", parseAgyStreamLine, []string{
			`{"event":"step_update","step_update":{"step_index":0,"state":"ACTIVE","step_type":"tool","tool_name":"read","tool_info":{"parameters":{"path":"file"}}}}`,
			`{"event":"step_update","step_update":{"step_index":0,"state":"DONE","step_type":"tool","tool_name":"read","tool_info":{"parameters":{"path":"file"},"output":"result"}}}`,
		}, "agy-step:0"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			var calls, results int
			for _, line := range tc.lines {
				events, text, _ := tc.parse([]byte(line))
				if text != "" {
					t.Fatalf("tool transcript leaked into answer: %q", text)
				}
				for _, event := range events {
					if event.ToolCallID != tc.id {
						t.Fatalf("identity lost: %+v", event)
					}
					switch event.Kind {
					case EventToolUse:
						calls++
						if event.Tool == "" || !strings.Contains(event.Detail, "file") {
							t.Fatalf("input lost: %+v", event)
						}
					case EventToolResult:
						results++
						if event.Detail != "result" {
							t.Fatalf("result lost: %+v", event)
						}
					default:
						t.Fatalf("unexpected event: %+v", event)
					}
				}
			}
			if calls == 0 || results != 1 {
				t.Fatalf("calls=%d results=%d", calls, results)
			}
		})
	}
}

func TestToolIdentityIsOptionalAndNeverTakenFromSession(t *testing.T) {
	for name, parse := range map[string]func([]byte) ([]Event, string, string){"claude": parseClaudeStreamLine, "qwen": parseQwenStreamLine} {
		events, _, _ := parse([]byte(`{"type":"assistant","session_id":"private","parent_tool_use_id":"parent","message":{"content":[{"type":"tool_use","name":"read","input":{}}]}}`))
		if len(events) != 1 || events[0].ToolCallID != "" {
			t.Fatalf("%s invented identity: %+v", name, events)
		}
	}
	events, _, _ := parseAgyStreamLine([]byte(`{"event":"step_update","conversation_id":"private","step_update":{"state":"ACTIVE","step_type":"tool","tool_name":"read"}}`))
	if len(events) != 1 || events[0].ToolCallID != "" {
		t.Fatalf("absent step became zero: %+v", events)
	}
}

func TestCompletedToolSnapshotPreservesFailureAlongsideOutput(t *testing.T) {
	events, _, _ := parseOpenCodeStreamLine([]byte(`{"type":"tool_use","part":{"callID":"failed","tool":"read","state":{"status":"error","input":{},"output":"partial","error":"denied"}}}`))
	if len(events) != 2 || events[1].Kind != EventToolResult || events[1].Detail != "partial\nError: denied" {
		t.Fatalf("failure lost: %+v", events)
	}
}
